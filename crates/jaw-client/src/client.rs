//! The connection an interface keeps to jawd, which never makes its caller wait. One thread
//! writes what the caller asks and another reads what jawd sends; replies and pushes wait in an
//! inbox the caller drains, each window's updates merged as they come. jawd's pushes wake the
//! caller at most once a frame, so a flood costs it one apply and one draw a frame; the answer to
//! anything it asked (hello, a window's watch or move, a request) wakes it at once, so what it
//! waits for comes as soon as jawd sends it.
//!
//! When jawd goes away, or isn't there yet, the client says so, tries again from 100 ms doubling to
//! 2 s, says hello again and watches every window again with the rows its model holds, so the
//! models end as a fresh watch would leave them. A `resync` is handled the same way. A request
//! in flight when the connection drops is answered with a failure that says so.
//!
//! Who's typing is kept as the protocol says: a watch's or move's answer replaces it for every
//! chat of its window, a `typing` push for its chat, and it's forgotten as a chat leaves every
//! window and when the connection drops. A frame carries all of it when it changed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Select, Sender, TrySendError};
use jaw_proto::find::{Find, Found};
use jaw_proto::windows::{
    ChatRow, ChatsPush, ChatsWatch, ChatsWatched, MessageRow, MessagesPush, MessagesWatch, MessagesWatched, TypingPush,
    Typist, Typists, Watch, WatchMove,
};
use jaw_proto::{Connection, Hello, LinkStep, PROTOCOL, Push, Request, Welcome, code};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::value::RawValue;

use crate::window::{Merged, Moved, Place, Row, Update};

/// The first wait before connecting again, and the longest.
const FIRST_RETRY: Duration = Duration::from_millis(100);
const LAST_RETRY: Duration = Duration::from_secs(2);
/// How long a write may wait for jawd to read before the connection counts as gone.
const WRITE_STALL: Duration = Duration::from_secs(10);
/// The longest line jawd sends, as the protocol has it.
const LONGEST_LINE: u64 = 16 << 20;
/// How many read times a frame keeps, for measuring.
const READS_KEPT: usize = 1_024;
/// jawd flushes a connection's windows one after another, each after its own read, so a window
/// update that comes after a quiet spell waits this long for the others before waking the caller.
const SETTLE: Duration = Duration::from_millis(2);

/// How to reach jawd and how to pace the caller.
#[derive(Clone, Debug)]
pub struct Config {
    pub socket: PathBuf,
    /// The interface's name and version, for hello.
    pub client: String,
    pub version: String,
    /// Whether the interface is in front when it connects.
    pub focus: bool,
    /// The shortest time between two wakes for jawd's pushes: an interface's frame.
    pub frame: Duration,
    /// How many of the caller's requests can wait to be written before `request` refuses more.
    pub queue: usize,
    /// How many replies and pushes can wait for the caller before the client stops reading from
    /// jawd, which then keeps window updates in their windows as it does for any slow interface.
    pub events: usize,
}

impl Config {
    pub fn new(socket: impl Into<PathBuf>, client: &str, version: &str) -> Config {
        Config {
            socket: socket.into(),
            client: client.into(),
            version: version.into(),
            focus: false,
            frame: Duration::from_micros(16_667),
            queue: 256,
            events: 4_096,
        }
    }
}

/// A window the caller watches. It outlives the connection: jawd's own id for it changes each
/// time it's watched again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(u64);

/// Why a request has no answer from jawd, in words an interface can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// jawd answered with an error.
    Jawd(jaw_proto::Error),
    /// The connection dropped after it was sent, so jawd may or may not have done it.
    Lost,
    /// It wasn't sent: jawd isn't connected.
    Unreachable,
    /// It wasn't sent: this many requests are already waiting for jawd to read them.
    Full(usize),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Jawd(error) => f.write_str(&error.message),
            Failure::Lost => {
                f.write_str("the connection to jawd dropped before it answered, so it may not have been done")
            }
            Failure::Unreachable => {
                f.write_str("jawd isn't connected; jaw is trying to reach it, so try again in a moment")
            }
            Failure::Full(n) => write!(f, "jawd has stopped reading what jaw sends: {n} requests are waiting for it"),
        }
    }
}

impl std::error::Error for Failure {}

/// What the caller learns from jawd, in the order it came.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// jawd answered hello, first or after connecting again.
    Connected(Welcome),
    /// jawd couldn't be reached, or the connection dropped; the client tries again in `retry_in`.
    Disconnected {
        why: String,
        retry_in: Duration,
    },
    /// A request's answer.
    Reply {
        id: u64,
        outcome: Result<Value, Failure>,
    },
    /// The answer to the latest `find`; earlier ones are dropped.
    Found {
        id: u64,
        query: String,
        outcome: Result<Found, Failure>,
    },
    /// A window couldn't be watched or moved as asked, and stays where it was.
    Window {
        window: WindowId,
        failure: Failure,
    },
    Connection(Connection),
    Link(LinkStep),
    /// Any other push.
    Push(Push),
}

/// Everything that came since the last drain: events in order, and one merged update for each
/// window that changed.
#[derive(Debug)]
pub struct Frame<C, M> {
    pub events: Vec<Event>,
    pub chats: Vec<(WindowId, Update<C>)>,
    pub messages: Vec<(WindowId, Update<M>)>,
    /// Who's typing in the chats the windows show, all of it as it is now, when it changed.
    pub typing: Option<Typists>,
    /// When the lines this frame carries were read, up to a thousand or so, for measuring how long
    /// they waited.
    pub read: Vec<Instant>,
}

impl<C, M> Frame<C, M> {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.chats.is_empty() && self.messages.is_empty() && self.typing.is_none()
    }
}

/// Counts since the client started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Lines read from jawd.
    pub lines: u64,
    /// Lines that weren't jaw's protocol, and were skipped.
    pub skipped: u64,
    /// Times hello was answered.
    pub connects: u64,
    /// Windows jawd told to watch again.
    pub resyncs: u64,
    /// Drains that took something.
    pub frames: u64,
}

/// The client: requests and windows go in, frames come out. Chat and message rows are read as
/// `C` and `M`, jaw-proto's own unless an interface has rows of its own.
pub struct Client<C: Row = ChatRow, M: Row = MessageRow> {
    shared: Arc<Shared<C, M>>,
    session: Option<JoinHandle<()>>,
    waker: Option<JoinHandle<()>>,
}

struct Shared<C, M> {
    state: Mutex<State<C, M>>,
    /// Signalled whenever the state changes in a way someone waits for: room in the inbox, a wake
    /// to schedule or given, a connection ending, the client closing.
    changed: Condvar,
    config: Config,
    wake: Box<dyn Fn() + Send + Sync>,
    /// The client's own lines, at most one in flight per window, and word to the writer.
    control: Sender<Control>,
    /// The caller's requests, bounded.
    requests: Sender<Line>,
}

enum Control {
    Line(Vec<u8>),
    /// The reader found the connection of this generation gone.
    Lost {
        generation: u64,
        why: String,
    },
    Shutdown,
}

struct Line {
    id: u64,
    bytes: Vec<u8>,
}

struct State<C, M> {
    closed: bool,
    /// Counts connections, so a reader of one that has gone changes nothing.
    generation: u64,
    /// Whether the current connection takes requests: hello has gone and it hasn't dropped.
    live: bool,
    /// Whether the current connection's hello was answered.
    greeted: bool,
    stream: Option<UnixStream>,
    focus: bool,
    next_id: u64,
    asked: HashMap<u64, Asked>,
    next_window: u64,
    windows: BTreeMap<WindowId, Window>,
    /// jawd's watch ids, to their windows.
    watches: HashMap<String, WindowId>,
    /// The latest find.
    find: Option<u64>,

    events: VecDeque<Event>,
    chats: HashMap<WindowId, Merged<C>>,
    messages: HashMap<WindowId, Merged<M>>,
    /// Who's typing, by chat, for the chats the windows show, and whether it changed since the
    /// last drain.
    typing: Typists,
    typing_changed: bool,
    read: Vec<Instant>,
    /// Whether the caller has been woken, or will be, for what's in the inbox.
    signalled: bool,
    /// Whether a wake was given that `wait` hasn't taken.
    woken: bool,
    wake_at: Option<Instant>,
    drained: Instant,
    stats: Stats,
}

/// What a request in flight was, for its reply.
enum Asked {
    Hello,
    Request,
    Find(String),
    Window(Landing),
    Unwatch,
}

/// A watch or a move in flight.
struct Landing {
    id: u64,
    window: WindowId,
    place: Place,
    /// For a watch, whether the window is being watched again; a move has none.
    again: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Chats,
    Messages,
}

struct Window {
    kind: Kind,
    /// The folder or the chat.
    of: String,
    /// Where the caller wants it.
    place: Place,
    /// Where jawd last put it.
    shown: Option<Place>,
    /// jawd's id for it on this connection.
    watch: Option<String>,
    /// The watch or move in flight, by request id.
    asking: Option<u64>,
    /// The ids its model will hold once what's merged is applied.
    ids: HashSet<String>,
    /// The newest seq its updates carried.
    seq: Option<u64>,
    /// Unwatched while a request for it was in flight.
    closing: bool,
}

impl Window {
    /// Whether it shows `chat`: a chat list among its rows, a chat's messages as theirs.
    fn shows(&self, chat: &str) -> bool {
        match self.kind {
            Kind::Chats => self.ids.contains(chat),
            Kind::Messages => self.of == chat,
        }
    }
}

/// A line from jawd, its params and result left unread until it's known what they are.
#[derive(Deserialize)]
struct Envelope<'a> {
    id: Option<u64>,
    #[serde(borrow)]
    method: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow)]
    params: Option<&'a RawValue>,
    #[serde(borrow)]
    result: Option<&'a RawValue>,
    error: Option<jaw_proto::Error>,
}

fn encode(id: u64, method: &str, params: Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&Request { id, method: method.into(), params }).expect("requests serialize");
    line.push(b'\n');
    line
}

fn value<T: serde::Serialize>(params: &T) -> Value {
    serde_json::to_value(params).expect("params serialize")
}

/// Reads `raw` as `T`, or says what jawd sent that isn't the protocol.
fn read<'a, T: Deserialize<'a>>(raw: Option<&'a RawValue>, what: &str) -> Result<T, Failure> {
    serde_json::from_str(raw.map_or("null", RawValue::get)).map_err(|e| {
        Failure::Jawd(jaw_proto::Error::new(
            code::INTERNAL,
            format!("what jawd sent for {what} isn't jaw's protocol: {e}"),
        ))
    })
}

fn chats_push<R>(push: ChatsPush<R>) -> (String, Update<R>) {
    let update =
        Update { seq: push.seq, remove: push.remove, upsert: push.upsert, total: push.total, ..Update::default() };
    (push.watch, update)
}

fn messages_push<R>(push: MessagesPush<R>) -> (String, Update<R>) {
    let update = Update {
        seq: push.seq,
        remove: push.remove,
        upsert: push.upsert,
        total: push.total,
        older: push.older,
        newer: push.newer,
        moved: None,
    };
    (push.watch, update)
}

/// A watch's or move's answer: jawd's watch id, the update, `newer` then, and who's typing in the
/// window's chats.
struct Landed<R> {
    watch: String,
    update: Update<R>,
    newer: u64,
    typing: Typists,
}

fn chats_watched<R>(watched: ChatsWatched<R>) -> Landed<R> {
    let update = Update {
        seq: watched.seq,
        remove: watched.remove,
        upsert: watched.rows,
        total: Some(watched.total),
        ..Update::default()
    };
    Landed { watch: watched.watch, update, newer: 0, typing: watched.typing }
}

fn messages_watched<R>(watched: MessagesWatched<R>) -> Landed<R> {
    let update = Update {
        seq: watched.seq,
        remove: watched.remove,
        upsert: watched.rows,
        total: Some(watched.total),
        older: Some(watched.older),
        newer: Some(watched.newer),
        moved: None,
    };
    Landed { watch: watched.watch, update, newer: watched.newer, typing: watched.typing }
}

impl<C, M> State<C, M> {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Whether a reader of connection `generation` may still change anything.
    fn current(&self, generation: u64) -> bool {
        self.live && self.generation == generation
    }

    /// Notes something new in the inbox, waking the caller a frame after the last drain and no
    /// sooner than `settle` from now; says whether to wake it now.
    fn signal(&mut self, frame: Duration, settle: Duration) -> bool {
        if self.signalled && self.wake_at.is_none() {
            return false;
        }
        self.signalled = true;
        let now = Instant::now();
        let at = (self.drained + frame).max(now + settle);
        let at = self.wake_at.map_or(at, |scheduled| scheduled.min(at));
        if at <= now {
            self.wake_at = None;
            self.woken = true;
            true
        } else {
            self.wake_at = Some(at);
            false
        }
    }

    fn note_read(&mut self, at: Instant) {
        self.stats.lines += 1;
        if self.read.len() < READS_KEPT {
            self.read.push(at);
        }
    }

    /// Whether a window it keeps shows `chat`.
    fn shows(&self, chat: &str) -> bool {
        self.windows.values().any(|w| !w.closing && w.shows(chat))
    }

    fn set_typing(&mut self, chat: String, who: Vec<Typist>) {
        if who.is_empty() {
            self.typing_changed |= self.typing.remove(&chat).is_some();
        } else if self.typing.get(&chat) != Some(&who) {
            self.typing.insert(chat, who);
            self.typing_changed = true;
        }
    }

    /// Forgets the typing of the chats no window shows.
    fn unshown(&mut self) {
        let gone: Vec<String> = self.typing.keys().filter(|chat| !self.shows(chat)).cloned().collect();
        for chat in gone {
            self.typing.remove(&chat);
            self.typing_changed = true;
        }
    }

    fn forget_typing(&mut self) {
        self.typing_changed |= !self.typing.is_empty();
        self.typing.clear();
    }
}

/// The default anchor of a window's list, keeping as many rows, for a window whose anchor has gone.
fn fallback(kind: Kind, place: &Place) -> Place {
    let rows = place.before + place.after;
    match kind {
        Kind::Chats => Place::top(rows),
        Kind::Messages => Place::newest(rows),
    }
}

impl<C: Row + DeserializeOwned + Send + 'static, M: Row + DeserializeOwned + Send + 'static> Client<C, M> {
    /// Starts connecting to jawd, in the background. `wake` is called, from one of the client's
    /// threads, when something new waits to be drained: at once for an answer to what the caller
    /// asked, and at most once a frame for pushes.
    pub fn start(config: Config, wake: impl Fn() + Send + Sync + 'static) -> Client<C, M> {
        let (control, control_rx) = crossbeam_channel::unbounded();
        let (requests, requests_rx) = crossbeam_channel::bounded(config.queue);
        let now = Instant::now();
        let state = State {
            closed: false,
            generation: 0,
            live: false,
            greeted: false,
            stream: None,
            focus: config.focus,
            next_id: 0,
            asked: HashMap::new(),
            next_window: 0,
            windows: BTreeMap::new(),
            watches: HashMap::new(),
            find: None,
            events: VecDeque::new(),
            chats: HashMap::new(),
            messages: HashMap::new(),
            typing: Typists::new(),
            typing_changed: false,
            read: Vec::new(),
            signalled: false,
            woken: false,
            wake_at: None,
            drained: now.checked_sub(config.frame).unwrap_or(now),
            stats: Stats::default(),
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(state),
            changed: Condvar::new(),
            config,
            wake: Box::new(wake),
            control,
            requests,
        });
        let session = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("jaw-client".into())
                .spawn(move || shared.session(control_rx, requests_rx))
                .expect("the client's thread")
        };
        let waker = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("jaw-client-wake".into())
                .spawn(move || shared.waker())
                .expect("the wake thread")
        };
        Client { shared, session: Some(session), waker: Some(waker) }
    }

    /// Watches a folder's chat list. Its updates come in frames under the id this returns.
    pub fn watch_chats(&self, folder: &str, place: Place) -> WindowId {
        self.shared.open(Kind::Chats, folder, place)
    }

    /// Watches a chat's messages.
    pub fn watch_messages(&self, chat: &str, place: Place) -> WindowId {
        self.shared.open(Kind::Messages, chat, place)
    }

    /// Moves a window, for scrolling. Moves asked while one is in flight wait for it, and only the
    /// latest is sent.
    pub fn move_window(&self, window: WindowId, place: Place) {
        let mut state = self.shared.lock();
        let Some(w) = state.windows.get_mut(&window) else { return };
        w.place = place;
        if w.asking.is_none() && state.live {
            self.shared.ask(&mut state, window, false);
        }
    }

    /// Stops watching a window; nothing more comes for it.
    pub fn unwatch(&self, window: WindowId) {
        let mut state = self.shared.lock();
        state.chats.remove(&window);
        state.messages.remove(&window);
        let Some(w) = state.windows.get_mut(&window) else { return };
        if w.asking.is_some() {
            w.closing = true;
            return;
        }
        let watch = w.watch.take();
        state.windows.remove(&window);
        state.unshown();
        if let Some(watch) = watch {
            state.watches.remove(&watch);
            self.shared.unwatch_line(&mut state, watch);
        }
    }

    /// Sends a request; its reply comes as `Event::Reply` with the id this returns.
    pub fn request(&self, method: &str, params: Value) -> Result<u64, Failure> {
        self.shared.request(method, params, Asked::Request)
    }

    /// Sends a find for the palette, at each keystroke: only the latest one's answer comes, as
    /// `Event::Found`.
    pub fn find(&self, find: Find) -> Result<u64, Failure> {
        let query = find.query.clone();
        self.shared.request("find", value(&find), Asked::Find(query))
    }

    /// Says whether the interface is in front, now and in every hello from here on.
    pub fn focus(&self, focused: bool) -> Result<u64, Failure> {
        self.shared.lock().focus = focused;
        self.request("focus", serde_json::json!({ "focused": focused }))
    }

    /// Takes everything that came since the last drain.
    pub fn drain(&self) -> Frame<C, M> {
        let (events, chats, messages, read, typing) = {
            let mut state = self.shared.lock();
            state.signalled = false;
            state.woken = false;
            state.wake_at = None;
            state.drained = Instant::now();
            let typing = std::mem::take(&mut state.typing_changed).then(|| state.typing.clone());
            let taken = (
                std::mem::take(&mut state.events),
                std::mem::take(&mut state.chats),
                std::mem::take(&mut state.messages),
                std::mem::take(&mut state.read),
                typing,
            );
            if !taken.0.is_empty() || !taken.1.is_empty() || !taken.2.is_empty() || taken.4.is_some() {
                state.stats.frames += 1;
            }
            self.shared.changed.notify_all();
            taken
        };
        Frame {
            events: events.into(),
            chats: chats.into_iter().map(|(window, merged)| (window, merged.into_update())).collect(),
            messages: messages.into_iter().map(|(window, merged)| (window, merged.into_update())).collect(),
            typing,
            read,
        }
    }

    /// Waits up to `timeout` for the next wake, for a caller with no loop of its own; says whether
    /// one came.
    pub fn wait(&self, timeout: Duration) -> bool {
        let until = Instant::now() + timeout;
        let mut state = self.shared.lock();
        loop {
            if state.woken {
                state.woken = false;
                return true;
            }
            let now = Instant::now();
            if state.closed || now >= until {
                return false;
            }
            state = self.shared.changed.wait_timeout(state, until - now).expect("the client's state").0;
        }
    }

    pub fn stats(&self) -> Stats {
        self.shared.lock().stats
    }
}

impl<C: Row, M: Row> Drop for Client<C, M> {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            if let Some(stream) = &state.stream {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        self.shared.changed.notify_all();
        let _ = self.shared.control.send(Control::Shutdown);
        for thread in [self.session.take(), self.waker.take()].into_iter().flatten() {
            let _ = thread.join();
        }
    }
}

impl<C: Row + DeserializeOwned + Send + 'static, M: Row + DeserializeOwned + Send + 'static> Shared<C, M> {
    fn lock(&self) -> MutexGuard<'_, State<C, M>> {
        self.state.lock().expect("the client's state")
    }

    /// Lets go of the state, waking anyone waiting on it, and the caller if `now`.
    fn notify(&self, state: MutexGuard<'_, State<C, M>>, now: bool) {
        drop(state);
        self.changed.notify_all();
        if now {
            (self.wake)();
        }
    }

    /// Puts an event in the inbox, first waiting for room while connection `generation` lasts. An
    /// answer to what the caller asked wakes it at once, and a push when it's due.
    fn push_event(&self, mut state: MutexGuard<'_, State<C, M>>, generation: u64, event: Event) {
        while state.events.len() >= self.config.events && !state.closed && state.current(generation) {
            state = self.changed.wait(state).expect("the client's state");
        }
        let frame = match &event {
            Event::Connected(_) | Event::Reply { .. } | Event::Found { .. } | Event::Window { .. } => Duration::ZERO,
            Event::Disconnected { .. } | Event::Connection(_) | Event::Link(_) | Event::Push(_) => self.config.frame,
        };
        state.events.push_back(event);
        let now = state.signal(frame, Duration::ZERO);
        self.notify(state, now);
    }

    fn open(&self, kind: Kind, of: &str, place: Place) -> WindowId {
        let mut state = self.lock();
        state.next_window += 1;
        let window = WindowId(state.next_window);
        let w = Window {
            kind,
            of: of.to_string(),
            place,
            shown: None,
            watch: None,
            asking: None,
            ids: HashSet::new(),
            seq: None,
            closing: false,
        };
        state.windows.insert(window, w);
        if state.live {
            self.ask(&mut state, window, false);
        }
        window
    }

    /// Sends what a window needs to be where the caller wants it.
    fn ask(&self, state: &mut State<C, M>, window: WindowId, again: bool) {
        if let Some(line) = self.asking(state, window, again) {
            let _ = self.control.send(Control::Line(line));
        }
    }

    /// The line a window needs to be where the caller wants it: a move if jawd has it, otherwise a
    /// watch, with the rows its model holds when it's watched again.
    fn asking(&self, state: &mut State<C, M>, window: WindowId, again: bool) -> Option<Vec<u8>> {
        let id = state.id();
        let w = state.windows.get_mut(&window)?;
        let place = w.place.clone();
        let Place { around, before, after } = place.clone();
        let line = match &w.watch {
            Some(watch) if !again => {
                let moving = WatchMove { watch: watch.clone(), around, before, after };
                encode(id, "watch.move", value(&moving))
            }
            _ => {
                let mut have: Vec<String> = w.ids.iter().cloned().collect();
                have.sort();
                match w.kind {
                    // A chat list held while catching up sends rows with an old sort and their v
                    // unchanged, so a chat list comes whole, its have saying which rows left.
                    Kind::Chats => {
                        let watch = ChatsWatch { folder: w.of.clone(), around, before, after, since: None, have };
                        encode(id, "chats.watch", value(&watch))
                    }
                    Kind::Messages => {
                        let since = if have.is_empty() { None } else { w.seq };
                        let watch = MessagesWatch { chat: w.of.clone(), around, before, after, since, have };
                        encode(id, "messages.watch", value(&watch))
                    }
                }
            }
        };
        let again = (w.watch.is_none() || again).then_some(again);
        w.asking = Some(id);
        state.asked.insert(id, Asked::Window(Landing { id, window, place, again }));
        Some(line)
    }

    fn unwatch_line(&self, state: &mut State<C, M>, watch: String) {
        if !state.live {
            return;
        }
        let id = state.id();
        state.asked.insert(id, Asked::Unwatch);
        let _ = self.control.send(Control::Line(encode(id, "unwatch", value(&Watch { watch }))));
    }

    fn request(&self, method: &str, params: Value, asked: Asked) -> Result<u64, Failure> {
        let mut state = self.lock();
        if !state.live {
            return Err(Failure::Unreachable);
        }
        let id = state.id();
        match self.requests.try_send(Line { id, bytes: encode(id, method, params) }) {
            Ok(()) => {
                if let Asked::Find(_) = asked {
                    state.find = Some(id);
                }
                state.asked.insert(id, asked);
                Ok(id)
            }
            Err(TrySendError::Full(_)) => Err(Failure::Full(self.config.queue)),
            Err(TrySendError::Disconnected(_)) => Err(Failure::Unreachable),
        }
    }

    /// Gives the caller its wakes, each a frame after the last drain.
    fn waker(&self) {
        let mut state = self.lock();
        loop {
            if state.closed {
                return;
            }
            match state.wake_at {
                None => state = self.changed.wait(state).expect("the client's state"),
                Some(at) => {
                    let now = Instant::now();
                    if at <= now {
                        state.wake_at = None;
                        state.woken = true;
                        self.notify(state, true);
                        state = self.lock();
                    } else {
                        state = self.changed.wait_timeout(state, at - now).expect("the client's state").0;
                    }
                }
            }
        }
    }

    /// Connects, serves the connection until it drops, and connects again, until the client closes.
    fn session(self: &Arc<Self>, control: Receiver<Control>, requests: Receiver<Line>) {
        let mut retry = FIRST_RETRY;
        loop {
            if self.lock().closed {
                return;
            }
            let why = match UnixStream::connect(&self.config.socket) {
                Ok(stream) => match self.serve(stream, &control, &requests) {
                    Some(why) => why,
                    None => return,
                },
                Err(e) => match e.kind() {
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused => {
                        format!("jawd isn't running: nothing answers on {}", self.config.socket.display())
                    }
                    _ => format!("can't reach jawd on {}: {e}", self.config.socket.display()),
                },
            };
            if self.lost(&control, &requests, why, &mut retry) {
                return;
            }
            let until = Instant::now() + retry;
            loop {
                match control.recv_deadline(until) {
                    Ok(Control::Shutdown) => return,
                    Ok(_) => continue,
                    Err(_) => break,
                }
            }
            retry = (retry * 2).min(LAST_RETRY);
        }
    }

    /// Serves one connection: hello and the windows first, then whatever is asked. Returns why it
    /// ended, or nothing when the client closed.
    fn serve(
        self: &Arc<Self>,
        stream: UnixStream,
        control: &Receiver<Control>,
        requests: &Receiver<Line>,
    ) -> Option<String> {
        let _ = stream.set_write_timeout(Some(WRITE_STALL));
        let (reading, kept) = match (stream.try_clone(), stream.try_clone()) {
            (Ok(reading), Ok(kept)) => (reading, kept),
            (Err(e), _) | (_, Err(e)) => return Some(format!("can't read from jawd: {e}")),
        };
        let (generation, lines) = {
            let mut state = self.lock();
            if state.closed {
                return None;
            }
            state.generation += 1;
            state.live = true;
            state.greeted = false;
            state.stream = Some(kept);
            let id = state.id();
            let hello = Hello {
                protocol: PROTOCOL,
                client: self.config.client.clone(),
                version: self.config.version.clone(),
                focus: state.focus,
                since: None,
            };
            state.asked.insert(id, Asked::Hello);
            let mut lines = vec![encode(id, "hello", value(&hello))];
            let windows: Vec<WindowId> = state.windows.keys().copied().collect();
            for window in windows {
                if state.windows[&window].closing {
                    state.windows.remove(&window);
                    continue;
                }
                let again = state.windows[&window].shown.is_some();
                lines.extend(self.asking(&mut state, window, again));
            }
            (state.generation, lines)
        };
        let reader = {
            let shared = self.clone();
            std::thread::Builder::new()
                .name("jaw-client-read".into())
                .spawn(move || shared.reader(reading, generation))
                .expect("the client's reading thread")
        };
        let ended = pump(&stream, &lines, control, requests, generation);
        let _ = stream.shutdown(Shutdown::Both);
        {
            let mut state = self.lock();
            state.live = false;
            state.stream = None;
        }
        self.changed.notify_all();
        let _ = reader.join();
        ended
    }

    /// After a connection dropped or couldn't be made: fails what was in flight, forgets jawd's
    /// watch ids, and says when it tries again, from 100 ms after a connection that was greeted.
    /// Says whether the client is closing.
    fn lost(&self, control: &Receiver<Control>, requests: &Receiver<Line>, why: String, retry: &mut Duration) -> bool {
        let mut state = self.lock();
        state.live = false;
        if std::mem::take(&mut state.greeted) {
            *retry = FIRST_RETRY;
        }
        let mut unsent = HashSet::new();
        while let Ok(line) = requests.try_recv() {
            unsent.insert(line.id);
        }
        let mut closing = state.closed;
        while let Ok(message) = control.try_recv() {
            closing |= matches!(message, Control::Shutdown);
        }
        let mut asked: Vec<(u64, Asked)> = state.asked.drain().collect();
        asked.sort_by_key(|(id, _)| *id);
        let mut events = Vec::new();
        for (id, asked) in asked {
            let failure = if unsent.contains(&id) { Failure::Unreachable } else { Failure::Lost };
            match asked {
                Asked::Request => events.push(Event::Reply { id, outcome: Err(failure) }),
                Asked::Find(query) if state.find == Some(id) => {
                    events.push(Event::Found { id, query, outcome: Err(failure) })
                }
                _ => {}
            }
        }
        state.find = None;
        state.watches.clear();
        state.forget_typing();
        state.windows.retain(|_, w| !w.closing);
        for w in state.windows.values_mut() {
            w.watch = None;
            w.asking = None;
        }
        events.push(Event::Disconnected { why, retry_in: *retry });
        state.events.extend(events);
        let now = state.signal(self.config.frame, Duration::ZERO);
        self.notify(state, now);
        closing
    }

    fn reader(&self, stream: UnixStream, generation: u64) {
        let mut reader = BufReader::with_capacity(1 << 16, stream);
        let mut line = Vec::new();
        let why = loop {
            line.clear();
            match (&mut reader).take(LONGEST_LINE).read_until(b'\n', &mut line) {
                Ok(0) => break "jawd closed the connection".to_string(),
                Ok(_) if line.last() != Some(&b'\n') && line.len() as u64 >= LONGEST_LINE => {
                    break "jawd sent a line longer than the protocol allows".to_string();
                }
                Ok(_) => {}
                Err(e) => break format!("the connection to jawd broke: {e}"),
            }
            if !self.take(&line, generation, Instant::now()) {
                return;
            }
        };
        let _ = self.control.send(Control::Lost { generation, why });
    }

    /// Takes one line from jawd. False once this connection is no longer the client's.
    fn take(&self, line: &[u8], generation: u64, at: Instant) -> bool {
        let Ok(envelope) = serde_json::from_slice::<Envelope>(line) else { return self.skip(generation) };
        match (envelope.id, envelope.method.as_deref()) {
            (Some(id), _) => self.reply(id, envelope.result, envelope.error, generation, at),
            (None, Some("chats")) => {
                let push = read::<ChatsPush<C>>(envelope.params, "a chats push").map(chats_push);
                self.window_push(push, generation, at, |state| &mut state.chats)
            }
            (None, Some("messages")) => {
                let push = read::<MessagesPush<M>>(envelope.params, "a messages push").map(messages_push);
                self.window_push(push, generation, at, |state| &mut state.messages)
            }
            (None, Some("typing")) => {
                let Ok(TypingPush { chat, who }) = read(envelope.params, "a typing push") else {
                    return self.skip(generation);
                };
                let mut state = self.lock();
                if !state.current(generation) {
                    return false;
                }
                state.note_read(at);
                if state.shows(&chat) {
                    state.set_typing(chat, who);
                }
                let now = state.typing_changed && state.signal(self.config.frame, SETTLE);
                self.notify(state, now);
                true
            }
            (None, Some("resync")) => {
                let Ok(Watch { watch }) = read(envelope.params, "a resync") else { return self.skip(generation) };
                let mut state = self.lock();
                if !state.current(generation) {
                    return false;
                }
                state.note_read(at);
                if let Some(window) = state.watches.remove(&watch) {
                    state.stats.resyncs += 1;
                    let closing = state.windows.get_mut(&window).map(|w| {
                        w.watch = None;
                        w.closing
                    });
                    match closing {
                        Some(true) => {
                            state.windows.remove(&window);
                            state.unshown();
                        }
                        Some(false) => self.ask(&mut state, window, true),
                        None => {}
                    }
                }
                true
            }
            (None, Some(method)) => {
                let raw = envelope.params.map_or("null", RawValue::get);
                let event = match method {
                    "connection" => serde_json::from_str(raw).ok().map(Event::Connection),
                    "link" => serde_json::from_str(raw).ok().map(Event::Link),
                    _ => None,
                };
                let event = event.unwrap_or_else(|| {
                    Event::Push(Push {
                        method: method.to_string(),
                        params: serde_json::from_str(raw).unwrap_or(Value::Null),
                    })
                });
                let mut state = self.lock();
                if !state.current(generation) {
                    return false;
                }
                state.note_read(at);
                self.push_event(state, generation, event);
                true
            }
            (None, None) => self.skip(generation),
        }
    }

    fn skip(&self, generation: u64) -> bool {
        let mut state = self.lock();
        state.stats.skipped += 1;
        state.current(generation)
    }

    fn window_push<R: Row>(
        &self,
        push: Result<(String, Update<R>), Failure>,
        generation: u64,
        at: Instant,
        merges: impl FnOnce(&mut State<C, M>) -> &mut HashMap<WindowId, Merged<R>>,
    ) -> bool {
        let Ok((watch, update)) = push else { return self.skip(generation) };
        let mut state = self.lock();
        if !state.current(generation) {
            return false;
        }
        state.note_read(at);
        if let Some(&window) = state.watches.get(&watch) {
            self.merge(state, window, update, false, None, merges);
        }
        true
    }

    /// Merges an update into a window's inbox and wakes the caller: at once for what it's waiting
    /// for, a watch's or move's answer where it wants the window, and otherwise when it's due. A
    /// watch's or move's answer says who's typing in every chat of the window (`answered`).
    fn merge<R: Row>(
        &self,
        mut state: MutexGuard<'_, State<C, M>>,
        window: WindowId,
        update: Update<R>,
        awaited: bool,
        answered: Option<Typists>,
        merges: impl FnOnce(&mut State<C, M>) -> &mut HashMap<WindowId, Merged<R>>,
    ) {
        let Some(w) = state.windows.get_mut(&window).filter(|w| !w.closing) else { return };
        w.seq = Some(w.seq.map_or(update.seq, |seq| seq.max(update.seq)));
        let mut ids = std::mem::take(&mut w.ids);
        merges(&mut state).entry(window).or_default().add(update, &mut ids);
        if let Some(w) = state.windows.get_mut(&window) {
            w.ids = ids;
        }
        if let Some(mut answered) = answered {
            // Each chat once, whether it's held, answered or both.
            let shown: BTreeSet<String> = match state.windows.get(&window) {
                Some(w) => state.typing.keys().chain(answered.keys()).filter(|chat| w.shows(chat)).cloned().collect(),
                None => BTreeSet::new(),
            };
            for chat in shown {
                let who = answered.remove(&chat).unwrap_or_default();
                state.set_typing(chat, who);
            }
        }
        state.unshown();
        let now = if awaited {
            state.signal(Duration::ZERO, Duration::ZERO)
        } else {
            state.signal(self.config.frame, SETTLE)
        };
        self.notify(state, now);
    }

    fn reply(
        &self,
        id: u64,
        result: Option<&RawValue>,
        error: Option<jaw_proto::Error>,
        generation: u64,
        at: Instant,
    ) -> bool {
        let asked = {
            let mut state = self.lock();
            if !state.current(generation) {
                return false;
            }
            state.note_read(at);
            state.asked.remove(&id)
        };
        let Some(asked) = asked else { return true };
        let outcome = match (error, result) {
            (Some(error), _) => Err(Failure::Jawd(error)),
            (None, Some(result)) => Ok(Some(result)),
            (None, None) => Err(Failure::Jawd(jaw_proto::Error::new(
                code::INTERNAL,
                "jawd answered with neither a result nor an error",
            ))),
        };
        match asked {
            Asked::Hello => {
                let welcome = outcome.and_then(|raw| read::<Welcome>(raw, "hello"));
                let mut state = self.lock();
                if !state.current(generation) {
                    return false;
                }
                match welcome {
                    Ok(welcome) => {
                        state.greeted = true;
                        state.stats.connects += 1;
                        self.push_event(state, generation, Event::Connected(welcome));
                        true
                    }
                    Err(failure) => {
                        state.live = false;
                        drop(state);
                        let _ = self.control.send(Control::Lost { generation, why: failure.to_string() });
                        false
                    }
                }
            }
            Asked::Request => {
                let outcome = outcome.and_then(|raw| read::<Value>(raw, "a request"));
                self.push_event(self.lock(), generation, Event::Reply { id, outcome });
                true
            }
            Asked::Find(query) => {
                let outcome = outcome.and_then(|raw| read::<Found>(raw, "find"));
                let mut state = self.lock();
                if state.find == Some(id) {
                    state.find = None;
                    self.push_event(state, generation, Event::Found { id, query, outcome });
                }
                true
            }
            Asked::Unwatch => true,
            Asked::Window(landing) => {
                let kind = self.lock().windows.get(&landing.window).map(|w| w.kind);
                match kind {
                    Some(Kind::Chats) => {
                        let landed =
                            outcome.and_then(|raw| read::<ChatsWatched<C>>(raw, "a chat list")).map(chats_watched);
                        self.landed(landing, landed, generation, |state| &mut state.chats)
                    }
                    Some(Kind::Messages) => {
                        let landed = outcome
                            .and_then(|raw| read::<MessagesWatched<M>>(raw, "a chat's messages"))
                            .map(messages_watched);
                        self.landed(landing, landed, generation, |state| &mut state.messages)
                    }
                    None => true,
                }
            }
        }
    }

    /// A watch's or move's answer.
    fn landed<R: Row>(
        &self,
        landing: Landing,
        landed: Result<Landed<R>, Failure>,
        generation: u64,
        merges: impl FnOnce(&mut State<C, M>) -> &mut HashMap<WindowId, Merged<R>>,
    ) -> bool {
        let Landing { id, window, place, again } = landing;
        let mut state = self.lock();
        if !state.current(generation) {
            return false;
        }
        // One superseded by watching again after a resync changes nothing.
        let Some(w) = state.windows.get_mut(&window).filter(|w| w.asking == Some(id)) else { return true };
        w.asking = None;
        match landed {
            Ok(Landed { watch, mut update, newer, typing }) => {
                if w.closing {
                    state.windows.remove(&window);
                    state.unshown();
                    self.unwatch_line(&mut state, watch);
                    return true;
                }
                if again != Some(true) || w.shown.as_ref() != Some(&place) {
                    update.moved = Some(Moved { place: place.clone(), newer });
                }
                let follow = w.place != place;
                w.shown = Some(place);
                w.watch = Some(watch.clone());
                state.watches.insert(watch, window);
                if follow {
                    self.ask(&mut state, window, false);
                }
                self.merge(state, window, update, !follow, Some(typing), merges);
            }
            Err(failure) => {
                if w.closing {
                    state.windows.remove(&window);
                    state.unshown();
                    return true;
                }
                let gone = matches!(&failure, Failure::Jawd(error) if error.code == code::NOT_FOUND);
                if again == Some(true) && gone {
                    w.place = fallback(w.kind, &place);
                    self.ask(&mut state, window, true);
                } else if w.place != place {
                    self.ask(&mut state, window, false);
                } else {
                    if let Some(shown) = w.shown.clone() {
                        w.place = shown;
                    }
                    self.push_event(state, generation, Event::Window { window, failure });
                }
            }
        }
        true
    }
}

/// Writes `first`, then the client's lines and the caller's requests as they come, until the
/// connection of `generation` drops, saying why, or the client closes.
fn pump(
    mut stream: &UnixStream,
    first: &[Vec<u8>],
    control: &Receiver<Control>,
    requests: &Receiver<Line>,
    generation: u64,
) -> Option<String> {
    let mut write = |line: &[u8]| stream.write_all(line).err().map(|e| format!("can't write to jawd: {e}"));
    for line in first {
        if let Some(why) = write(line) {
            return Some(why);
        }
    }
    let mut select = Select::new();
    let from_client = select.recv(control);
    select.recv(requests);
    loop {
        let ready = select.select();
        let why = if ready.index() == from_client {
            match ready.recv(control) {
                Ok(Control::Line(line)) => write(&line),
                Ok(Control::Lost { generation: lost, why }) if lost == generation => Some(why),
                Ok(Control::Lost { .. }) => None,
                Ok(Control::Shutdown) | Err(_) => return None,
            }
        } else {
            match ready.recv(requests) {
                Ok(line) => write(&line.bytes),
                Err(_) => return None,
            }
        };
        if why.is_some() {
            return why;
        }
    }
}

#[cfg(test)]
mod tests;
