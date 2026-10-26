//! `jaw demo`: a made-up account, for trying jaw without one, for snapshots and for the tests. It
//! plays jawd's part as the protocol describes it: it answers watches with windows of rows,
//! pushes upserts as messages arrive and their ticks move, and follows the newest only while the
//! window does. Live, a thread of its own has people type and write; a flood sends messages as
//! fast as asked.

mod actions;
mod find;
mod folders;
mod library;
mod panes;
pub mod pictures;
mod quick_look;
mod voice;

use crate::data::Place;
use crate::data::{
    Album, AlbumItem, Chat, ChatKind, FoundKind, Jawd, Kind, Last, Media, Message, Person, Quote, Reaction, Sender,
    Span, SpanKind, Tick, Typing, Update,
};
use crate::fmt::{DAY, MINUTE};
use crate::run::Event;
use jaw_client::{Moved, Update as Window};
use jaw_proto::windows::Around;
use jaw_proto::{Connection, ConnectionState, LinkStep};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc::Sender as Channel;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// The account's own person.
pub const ME: &str = "person:0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum State {
    /// Linked and online.
    Online,
    /// Not linked yet: the QR code, or a code typed on the phone.
    Linking,
    /// jaw starting jawd.
    Starting,
    CatchingUp,
    PhoneOffline,
    Offline,
    Reconnecting,
    LoggedOut,
    UpdateNeeded,
    Banned,
    /// jawd out of reach.
    Unreachable,
    /// Linked, with no chats yet.
    New,
}

#[derive(Clone, Copy, Debug)]
pub enum Clock {
    Fixed(i64),
    Wall,
}

impl Clock {
    fn now(self) -> i64 {
        match self {
            Clock::Fixed(at) => at,
            Clock::Wall => chrono::Utc::now().timestamp_millis(),
        }
    }
}

struct Thread {
    chat: Chat,
    messages: Vec<Message>,
}

/// The open chat's window, as jawd keeps it for an interface.
struct Open {
    chat: String,
    following: bool,
    before: usize,
    /// The first message in the window, and one past the last.
    lo: usize,
    hi: usize,
}

impl Open {
    /// What the window shows of a message just come into its chat, the chat's `messages` with it:
    /// the message and the oldest let go past its size while it follows the newest, or how many
    /// come after its last while it doesn't.
    fn took(&mut self, messages: &[Message], m: Message) -> Window<Message> {
        let len = messages.len();
        if !self.following {
            return Window { newer: Some((len - self.hi) as u64), ..Window::default() };
        }
        self.hi = len;
        let mut remove = vec![];
        if self.hi - self.lo > self.before {
            remove.push(messages[self.lo].id.clone());
            self.lo += 1;
        }
        Window { upsert: vec![m], remove, older: Some(self.lo > 0), ..Window::default() }
    }
}

enum Later {
    Tick {
        chat: String,
        id: String,
        tick: Tick,
    },
    Beat(usize),
    Qr,
    Linked,
    CatchUp,
    Retry,
    Started,
    /// The next of who comes and goes in the roster.
    Presence(usize),
}

pub struct Account {
    tx: Channel<Event>,
    clock: Clock,
    state: State,
    threads: Vec<Thread>,
    index: HashMap<String, usize>,
    open: Option<Open>,
    seq: u64,
    connection: Connection,
    later: Vec<(i64, Later)>,
    live: bool,
    gone: bool,
    qr: u32,
    /// People with no chat yet.
    contacts: Vec<Person>,
    /// How often each chat was opened by hand, which `find` ranks by.
    opens: HashMap<String, u32>,
    /// The messages sent, by the key each was composed under, as jawd keeps them.
    keyed: HashMap<String, String>,
    /// What's playing or paused, and what's being recorded.
    player: Option<voice::Player>,
    recorder: Option<voice::Recorder>,
    folders: folders::Folders,
    /// The library window watched, and its counts of each kind.
    library: Option<(crate::data::LibraryAsk, std::collections::HashMap<crate::data::LibraryKind, u32>)>,
    /// The quick look's windows: each chat, and how many of its newest messages it holds.
    quick_look: Vec<(String, usize)>,
    /// The windows of the panes that don't have the keys.
    parked: Vec<Open>,
}

struct Shared {
    account: Mutex<Account>,
    wake: Condvar,
    /// The thread `find` is answered on, once it has one, and the newest query it was asked.
    finder: Mutex<Option<Channel<find::Ask>>>,
    latest: AtomicU64,
    /// What the interface asked while the account was busy, for whoever takes it next.
    deferred: Mutex<Vec<Deferred>>,
    /// Whether voice keeps time, and whether its clock thread is running.
    clocked: AtomicBool,
    ticking: AtomicBool,
}

type Deferred = Box<dyn FnOnce(&mut Account) + Send>;

/// The made-up account as the interface's jawd.
#[derive(Clone)]
pub struct Demo {
    shared: Arc<Shared>,
}

impl Demo {
    /// The account in `state`, its first updates already on `tx`. Live, its people keep writing.
    pub fn start(tx: Channel<Event>, clock: Clock, state: State, live: bool) -> Demo {
        let now = clock.now();
        let threads = if state == State::New { vec![] } else { made_up(now) };
        let index = threads.iter().enumerate().map(|(i, t)| (t.chat.id.clone(), i)).collect();
        let seq = threads.iter().map(|t| t.messages.len() as u64).sum();
        let connection = match state {
            State::CatchingUp => {
                Connection { done: Some(1_204), total: Some(5_000), ..Connection::new(ConnectionState::CatchingUp) }
            }
            State::PhoneOffline => Connection::new(ConnectionState::PhoneOffline),
            State::Offline => Connection::new(ConnectionState::Offline),
            State::Reconnecting => Connection::new(ConnectionState::EngineRestarting),
            State::LoggedOut => Connection::new(ConnectionState::LoggedOut),
            State::UpdateNeeded => Connection::new(ConnectionState::UpdateNeeded),
            State::Banned => Connection { until: Some(now + 2 * DAY), ..Connection::new(ConnectionState::Banned) },
            State::Linking => Connection::new(ConnectionState::Connecting),
            _ => Connection::new(ConnectionState::Online),
        };
        let account = Account {
            tx,
            clock,
            state,
            threads,
            index,
            open: None,
            seq,
            connection,
            later: vec![],
            live,
            gone: false,
            qr: 0,
            contacts: contacts(),
            opens: HashMap::new(),
            keyed: HashMap::new(),
            player: None,
            recorder: None,
            folders: folders::Folders::made_up(now),
            library: None,
            quick_look: vec![],
            parked: vec![],
        };
        let demo = Demo {
            shared: Arc::new(Shared {
                account: Mutex::new(account),
                wake: Condvar::new(),
                finder: Mutex::new(None),
                latest: AtomicU64::new(0),
                deferred: Mutex::new(Vec::new()),
                clocked: AtomicBool::new(live),
                ticking: AtomicBool::new(false),
            }),
        };
        {
            let mut a = demo.lock();
            a.file_chats();
            match state {
                State::Starting => {
                    a.send(Update::Starting { failed: None });
                    a.after(2_500, Later::Started);
                }
                State::Unreachable => a.unreachable(),
                _ => a.hello(),
            }
            match state {
                State::CatchingUp => a.after(400, Later::CatchUp),
                State::Online => {
                    a.after(3_000, Later::Beat(0));
                    a.after(4_000, Later::Presence(0));
                }
                _ => {}
            }
        }
        if live {
            let shared = demo.shared.clone();
            std::thread::Builder::new()
                .name("jaw-demo".into())
                .spawn(move || live_thread(&shared))
                .expect("the demo's thread");
        }
        demo
    }

    /// Changes a chat's row as the phone would, and says so to the chat list: an archived chat
    /// leaves All, and comes back when it's unarchived.
    fn change(&self, chat: &str, how: impl FnOnce(&mut Chat) + Send + 'static) {
        let chat = chat.to_string();
        self.defer(move |a| {
            let Some(&t) = a.index.get(&chat) else { return };
            let c = &mut a.threads[t].chat;
            how(c);
            c.v += 1;
            let (row, archived) = (c.clone(), c.archived);
            let total = a.total();
            let (upsert, remove) = if archived { (vec![], vec![row.id]) } else { (vec![row], vec![]) };
            a.send(Update::Chats(Window { upsert, remove, total: Some(total), ..Window::default() }));
        });
    }

    /// Does `job` on the account now if it's free, or as soon as the thread that holds it next
    /// takes it, waking the live thread for it, so the interface never waits on the account.
    fn defer(&self, job: impl FnOnce(&mut Account) + Send + 'static) {
        match self.shared.account.try_lock() {
            Ok(mut a) => {
                self.run_deferred(&mut a);
                job(&mut a);
            }
            Err(_) => self.shared.deferred.lock().unwrap_or_else(|e| e.into_inner()).push(Box::new(job)),
        }
        self.wake();
    }

    fn run_deferred(&self, a: &mut Account) {
        let jobs = std::mem::take(&mut *self.shared.deferred.lock().unwrap_or_else(|e| e.into_inner()));
        for job in jobs {
            job(a);
        }
    }

    fn lock(&self) -> MutexGuard<'_, Account> {
        let mut a = self.shared.account.lock().unwrap_or_else(|e| e.into_inner());
        self.run_deferred(&mut a);
        a
    }

    fn wake(&self) {
        self.shared.wake.notify_all();
    }

    /// Sends `rate` messages a second for `seconds`, half into the open chat and half across the
    /// others, from a thread of its own.
    pub fn flood(&self, rate: u32, seconds: f64) -> std::thread::JoinHandle<u64> {
        let demo = self.clone();
        std::thread::Builder::new()
            .name("jaw-flood".into())
            .spawn(move || {
                let start = std::time::Instant::now();
                let mut sent = 0u64;
                let mut rng = 0x5eed_u64;
                while start.elapsed().as_secs_f64() < seconds {
                    let due = (start.elapsed().as_secs_f64() * rate as f64) as u64;
                    if sent < due {
                        let mut a = demo.lock();
                        if a.gone {
                            break;
                        }
                        while sent < due {
                            a.flooded(sent, &mut rng);
                            sent += 1;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                sent
            })
            .expect("the flood's thread")
    }
}

fn live_thread(shared: &Shared) {
    let mut a = shared.account.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let now = a.now();
        let due: Vec<Later> = {
            let (due, rest): (Vec<_>, Vec<_>) =
                std::mem::take(&mut a.later).into_iter().partition(|(at, _)| *at <= now);
            a.later = rest;
            due.into_iter().map(|(_, l)| l).collect()
        };
        for l in due {
            a.run(l);
        }
        if a.gone {
            return;
        }
        let next = a.later.iter().map(|(at, _)| *at).min();
        a = match next {
            Some(at) => {
                let wait = Duration::from_millis((at - a.now()).max(1) as u64);
                shared.wake.wait_timeout(a, wait).unwrap_or_else(|e| e.into_inner()).0
            }
            None => shared.wake.wait(a).unwrap_or_else(|e| e.into_inner()),
        };
    }
}

/// Something one of the made-up account's people says while it's live.
struct Beat {
    /// After the beat before, in milliseconds.
    after: i64,
    chat: &'static str,
    who: (&'static str, &'static str, u8),
    /// Whether they're seen typing first.
    types: bool,
    text: &'static str,
}

const BEATS: &[Beat] = &[
    Beat {
        after: 3_000, chat: "ana", who: ("person:1", "Ana", 0), types: true, text: "já agora, levas tu o *vinho*?"
    },
    Beat {
        after: 5_000,
        chat: "familia",
        who: ("person:3", "Rui", 1),
        types: false,
        text: "chego às 20h, guardem-me um lugar",
    },
    Beat {
        after: 6_000,
        chat: "climbing",
        who: ("person:20", "Marta", 2),
        types: false,
        text: "alguém para sábado às 9?",
    },
    Beat {
        after: 4_000,
        chat: "escola",
        who: ("person:30", "Prof. Helena", 4),
        types: false,
        text: "Lembrete: visita de estudo _amanhã_ às 9h",
    },
    Beat {
        after: 7_000,
        chat: "ana",
        who: ("person:1", "Ana", 0),
        types: true,
        text: "ou compro eu no caminho, diz-me",
    },
    Beat { after: 6_000, chat: "familia", who: ("person:4", "Inês", 2), types: false, text: "@Pedro trazes a avó?" },
    Beat {
        after: 9_000,
        chat: "climbing",
        who: ("person:21", "João", 3),
        types: true,
        text: "eu vou! levo a corda de 70",
    },
];

impl Account {
    fn now(&self) -> i64 {
        self.clock.now()
    }

    fn send(&mut self, u: Update) {
        if self.tx.send(Event::Update(u)).is_err() {
            self.gone = true;
        }
    }

    fn after(&mut self, ms: i64, l: Later) {
        if self.live {
            let at = self.now() + ms;
            self.later.push((at, l));
        }
    }

    fn hello(&mut self) {
        let linked = self.state != State::Linking;
        let root = pictures::folder().display().to_string();
        self.send(Update::Hello { root, linked, me: vec![ME.into()], connection: self.connection.clone() });
    }

    fn unreachable(&mut self) {
        let why = "jawd isn't running: nothing answers on jaw.sock".to_string();
        self.send(Update::Unreachable { why, retry_in: 4_000 });
        self.after(4_000, Later::Retry);
    }

    fn run(&mut self, l: Later) {
        match l {
            Later::Tick { chat, id, tick } => self.tick(&chat, &id, tick),
            Later::Beat(i) => self.beat(i),
            Later::Qr => {
                if matches!(self.state, State::Linking) {
                    self.qr += 1;
                    self.send_qr();
                }
            }
            Later::Linked => {
                self.state = State::Online;
                self.connection = Connection::new(ConnectionState::Online);
                self.send(Update::Link(LinkStep::Done { phone: "+351 912 000 000".into() }));
                self.send(Update::Connection(self.connection.clone()));
            }
            Later::CatchUp => {
                let (done, total) = (self.connection.done.unwrap_or(0) + 173, self.connection.total.unwrap_or(0));
                if done >= total {
                    self.connection = Connection::new(ConnectionState::Online);
                } else {
                    self.connection.done = Some(done);
                    self.after(400, Later::CatchUp);
                }
                self.send(Update::Connection(self.connection.clone()));
            }
            Later::Retry => self.unreachable(),
            Later::Started => {
                self.state = State::Online;
                self.hello();
                self.after(3_000, Later::Beat(0));
            }
            Later::Presence(i) => self.presence_step(i),
        }
    }

    fn beat(&mut self, i: usize) {
        let beat = &BEATS[i % BEATS.len()];
        let next = BEATS[(i + 1) % BEATS.len()].after;
        let sender = Sender { id: beat.who.0.into(), name: beat.who.1.into(), colour: beat.who.2 };
        let Some(&t) = self.index.get(beat.chat) else { return };
        if beat.types && self.threads[t].chat.typing.is_empty() {
            self.set_typing(t, Some(sender.name.clone()));
            self.later.push((self.now() + 2_500, Later::Beat(i)));
            return;
        }
        self.set_typing(t, None);
        let at = self.now();
        let m = self.message(at, Some(sender), markup(beat.text), None);
        self.arrive(t, m);
        self.after(next, Later::Beat(i + 1));
    }

    fn set_typing(&mut self, t: usize, who: Option<String>) {
        let chat = &mut self.threads[t].chat;
        chat.typing = who.into_iter().collect();
        chat.v += 1;
        self.push_chat(t);
    }

    fn message(&mut self, at: i64, from: Option<Sender>, spans: Vec<Span>, status: Option<Tick>) -> Message {
        self.seq += 1;
        Message {
            id: format!("3EB0JAW{:09}", self.seq),
            v: 1,
            chat: String::new(),
            sort: format!("{at:013}~{:09}", self.seq),
            at,
            mine: from.is_none(),
            from,
            kind: Kind::Text,
            spans,
            reply: None,
            forwarded: 0,
            reactions: vec![],
            status,
            edited: false,
            deleted: false,
            summary: None,
            system: None,
            media: None,
            album: None,
            voice: None,
        }
    }

    /// How many chats the chat list's folder holds.
    fn total(&self) -> u64 {
        let list = &self.folders.list;
        self.threads.iter().filter(|t| self.in_folder(list, &t.chat, None)).count() as u64
    }

    /// A chat's row changed: into the chat list if its folder holds it, out if not, and the
    /// folders' counts after it.
    fn push_chat(&mut self, t: usize) {
        let row = self.threads[t].chat.clone();
        let list = self.folders.list.clone();
        let total = Some(self.total());
        let update = if self.in_folder(&list, &row, None) {
            Window { upsert: vec![row], total, ..Window::default() }
        } else {
            Window { remove: vec![row.id], total, ..Window::default() }
        };
        self.send(Update::Chats(update));
        self.folders_changed();
    }

    /// A message comes into chat `t`: its row moves up, and the open window takes it if it
    /// follows the newest, or counts it as newer if it doesn't.
    fn arrive(&mut self, t: usize, mut m: Message) {
        let thread = &mut self.threads[t];
        m.chat = thread.chat.id.clone();
        let mention = m.spans.iter().any(|s| s.t == SpanKind::Mention && s.person.as_deref() == Some(ME));
        thread.chat.last = Some(Last {
            from: m.from.as_ref().filter(|_| thread.chat.kind == ChatKind::Group).map(|f| f.name.clone()),
            summary: match (&m.summary, m.kind) {
                (Some(s), k) if k != Kind::Text => s.clone(),
                _ => crate::fmt::one_line(&m.spans.iter().map(|s| s.s.as_str()).collect::<String>()),
            },
            status: m.status,
            at: m.at,
        });
        thread.chat.sort = format!("{}~{:013}", u8::from(thread.chat.pinned), m.at);
        if !m.mine {
            thread.chat.unread += 1;
            thread.chat.mentions += u32::from(mention);
        } else {
            thread.chat.unread = 0;
            thread.chat.mentions = 0;
            thread.chat.draft = None;
        }
        thread.chat.v += 1;
        thread.messages.push(m.clone());
        let name = thread.chat.name.clone();
        self.library_arrived(&name, &m);
        let chat = self.threads[t].chat.id.clone();
        self.push_chat(t);
        self.quick_look_arrived(t);
        let messages = &self.threads[t].messages;
        let updates: Vec<Window<Message>> = (self.open.iter_mut().chain(self.parked.iter_mut()))
            .filter(|o| o.chat == chat)
            .map(|o| o.took(messages, m.clone()))
            .collect();
        for update in updates {
            self.send(Update::Messages { chat: chat.clone(), update });
        }
    }

    fn tick(&mut self, chat: &str, id: &str, tick: Tick) {
        let Some(&t) = self.index.get(chat) else { return };
        let thread = &mut self.threads[t];
        let Some(i) = thread.messages.iter().position(|m| m.id == id) else { return };
        let m = &mut thread.messages[i];
        m.status = Some(tick);
        m.v += 1;
        let m = m.clone();
        if i + 1 == thread.messages.len()
            && let Some(last) = thread.chat.last.as_mut()
        {
            last.status = Some(tick);
            thread.chat.v += 1;
            self.push_chat(t);
        }
        if self.shows(chat, i) {
            let chat = chat.to_string();
            self.send(Update::Messages { chat, update: Window { upsert: vec![m], ..Window::default() } });
        }
    }

    /// A flood's `n`th message: every other one into the open chat.
    fn flooded(&mut self, n: u64, rng: &mut u64) {
        if self.threads.is_empty() {
            return;
        }
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let open = self.open.as_ref().and_then(|o| self.index.get(&o.chat).copied());
        let t = match open {
            Some(t) if n.is_multiple_of(2) => t,
            _ => (*rng >> 33) as usize % self.threads.len(),
        };
        let who = (*rng >> 20) as usize % NAMES.len();
        let sender = Sender { id: format!("person:{}", 100 + who), name: NAMES[who].into(), colour: who as u8 };
        let words: Vec<&str> =
            (0..3 + (*rng >> 40) as usize % 12).map(|k| WORDS[(*rng as usize >> (k % 16)) % WORDS.len()]).collect();
        let at = self.now();
        let mut m = self.message(
            at,
            Some(sender),
            vec![Span { t: SpanKind::Text, s: words.join(" "), person: None, url: None }],
            None,
        );
        if Some(t) == open && n.is_multiple_of(8) {
            m.kind = Kind::Image;
            m.summary = Some("▣ photo".into());
            m.media = Some(Box::new(pictures::media(HISTORY_PHOTOS[(n / 8) as usize % HISTORY_PHOTOS.len()])));
        }
        self.arrive(t, m);
    }

    fn send_qr(&mut self) {
        let code = format!("2@jaw-demo-{:04},made-up,not-a-real-code,{}", self.qr, "x".repeat(8));
        let expires = self.now() + 20_000;
        self.send(Update::Link(LinkStep::Qr { code, expires }));
        self.after(20_000, Later::Qr);
    }
}

/// What the interface asks of the made-up account, done on it.
impl Account {
    fn on_watch_chats(&mut self, folder: &str, place: Place) {
        let rows = self.list_rows(folder);
        let total = rows.len() as u64;
        let moved = Some(Moved { place, newer: 0 });
        self.send(Update::Chats(Window { upsert: rows, total: Some(total), moved, ..Window::default() }));
    }

    fn on_watch_messages(&mut self, chat: &str, place: Place) {
        let Some(&t) = self.index.get(chat) else { return };
        let messages = &self.threads[t].messages;
        let len = messages.len();
        let (before, after) = (place.before as usize, place.after as usize);
        let newest = (len.saturating_sub(before), len, true);
        let (lo, hi, following) = match &place.around {
            Around::Row(id) => match messages.iter().position(|m| &m.id == id) {
                Some(i) => (i.saturating_sub(before), (i + after + 1).min(len), false),
                None => newest,
            },
            _ => newest,
        };
        let remove = match self.open.as_ref().filter(|o| o.chat == chat) {
            Some(o) => (o.lo..o.hi).filter(|i| !(lo..hi).contains(i)).map(|i| messages[i].id.clone()).collect(),
            None => vec![],
        };
        let rows = messages[lo..hi].to_vec();
        let newer = (len - hi) as u64;
        self.open = Some(Open { chat: chat.into(), following, before, lo, hi });
        let update = Window {
            upsert: rows,
            remove,
            total: Some(len as u64),
            older: Some(lo > 0),
            newer: Some(newer),
            moved: Some(Moved { place, newer }),
            seq: 0,
        };
        self.send(Update::Messages { chat: chat.into(), update });
    }

    fn on_draft(&mut self, chat: &str, text: &str) {
        let Some(&t) = self.index.get(chat) else { return };
        let c = &mut self.threads[t].chat;
        c.draft = (!text.is_empty()).then(|| text.to_string());
        c.v += 1;
        self.push_chat(t);
    }

    fn on_read(&mut self, chat: &str, _up_to: &str) {
        let Some(&t) = self.index.get(chat) else { return };
        let c = &mut self.threads[t].chat;
        if c.unread > 0 || c.mentions > 0 {
            c.unread = 0;
            c.mentions = 0;
            c.v += 1;
            self.push_chat(t);
        }
    }

    fn on_opened(&mut self, chat: &str) {
        *self.opens.entry(chat.to_string()).or_default() += 1;
    }

    fn on_start_chat(&mut self, person: &str) {
        let Some(i) = self.contacts.iter().position(|p| p.id == person) else { return };
        let p = self.contacts.remove(i);
        let digits: String = p.phone.unwrap_or_default().chars().filter(char::is_ascii_digit).collect();
        let now = self.now();
        let mut w = Writing::new(&format!("{digits}@s.whatsapp.net"), &p.name, ChatKind::Direct, now);
        w.thread.chat.sort = format!("0~{now:013}");
        let thread = w.done(0, 0);
        let chat = thread.chat.clone();
        let at = self.threads.len();
        self.index.insert(chat.id.clone(), at);
        self.threads.push(thread);
        let total = self.total();
        self.send(Update::Chats(Window { upsert: vec![chat.clone()], total: Some(total), ..Window::default() }));
        self.send(Update::ChatStarted { chat: Box::new(chat) });
    }

    /// A chat by number: a contact's if it's theirs, the one jaw has if any, or a new one named
    /// for the number; numbers too short to be one aren't on WhatsApp.
    fn on_start_by_phone(&mut self, phone: &str) {
        let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
        if digits.len() < 8 {
            self.send(Update::Notice(format!("{phone} isn't on WhatsApp")));
            return;
        }
        let same = |p: &Option<String>| {
            p.as_deref().is_some_and(|p| p.chars().filter(char::is_ascii_digit).collect::<String>() == digits)
        };
        if let Some(person) = self.contacts.iter().find(|p| same(&p.phone)).map(|p| p.id.clone()) {
            return self.on_start_chat(&person);
        }
        let id = format!("{digits}@s.whatsapp.net");
        if let Some(&t) = self.index.get(&id) {
            let chat = self.threads[t].chat.clone();
            self.send(Update::ChatStarted { chat: Box::new(chat) });
            return;
        }
        let now = self.now();
        let mut w = Writing::new(&id, &format!("+{digits}"), ChatKind::Direct, now);
        w.thread.chat.sort = format!("0~{now:013}");
        let thread = w.done(0, 0);
        let chat = thread.chat.clone();
        self.index.insert(chat.id.clone(), self.threads.len());
        self.threads.push(thread);
        let total = self.total();
        self.send(Update::Chats(Window { upsert: vec![chat.clone()], total: Some(total), ..Window::default() }));
        self.send(Update::ChatStarted { chat: Box::new(chat) });
    }

    /// A failed message of yours, pending again, then on its way.
    fn on_retry(&mut self, message: &str) {
        let Some((t, i)) = self.locate(message) else { return };
        if !self.threads[t].messages[i].mine || self.threads[t].messages[i].status != Some(Tick::Failed) {
            return;
        }
        let chat = self.threads[t].chat.id.clone();
        self.tick(&chat, message, Tick::Pending);
        self.after(500, Later::Tick { chat: chat.clone(), id: message.to_string(), tick: Tick::Sent });
        self.after(1_500, Later::Tick { chat, id: message.to_string(), tick: Tick::Delivered });
    }

    fn on_link_start(&mut self, phone: Option<String>) {
        match phone {
            None => self.send_qr(),
            Some(_) => {
                self.send(Update::Link(LinkStep::Code { code: "K4PM-7XQ2".into() }));
                self.after(8_000, Later::Linked);
            }
        }
    }
}

impl Jawd for Demo {
    /// The made-up account's list fits one window, so it sends every chat wherever it's asked.
    fn watch_chats(&mut self, folder: &str, place: Place) {
        let folder = folder.to_string();
        self.defer(move |a| a.on_watch_chats(&folder, place));
    }

    fn watch_folders(&mut self) {
        self.defer(|a| a.on_watch_folders());
    }

    fn watch_library(&mut self, ask: &crate::data::LibraryAsk) {
        let ask = ask.clone();
        self.defer(move |a| a.on_watch_library(ask));
    }

    fn unwatch_library(&mut self) {
        self.defer(|a| a.on_unwatch_library());
    }

    fn watch_roster(&mut self, folder: &str) {
        let folder = folder.to_string();
        self.defer(move |a| a.on_watch_roster(&folder));
    }

    fn folder_create(&mut self, name: &str, parent: Option<&str>) {
        let (name, parent) = (name.to_string(), parent.map(String::from));
        self.defer(move |a| a.on_folder_create(&name, parent.as_deref()));
    }

    fn folder_rename(&mut self, folder: &str, name: &str) {
        let (folder, name) = (folder.to_string(), name.to_string());
        self.defer(move |a| a.on_folder_rename(&folder, &name));
    }

    fn folder_move(&mut self, folder: &str, parent: Option<&str>) {
        let (folder, parent) = (folder.to_string(), parent.map(String::from));
        self.defer(move |a| a.on_folder_move(&folder, parent.as_deref()));
    }

    fn folder_delete(&mut self, folder: &str) {
        let folder = folder.to_string();
        self.defer(move |a| a.on_folder_delete(&folder));
    }

    fn folder_add(&mut self, folder: &str, item: &str) {
        let (folder, item) = (folder.to_string(), item.to_string());
        self.defer(move |a| a.on_folder_add(&folder, &item));
    }

    fn folder_remove(&mut self, folder: &str, item: &str) {
        let (folder, item) = (folder.to_string(), item.to_string());
        self.defer(move |a| a.on_folder_remove(&folder, &item));
    }

    fn watch_messages(&mut self, chat: &str, place: Place) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_watch_messages(&chat, place));
    }

    fn watch_quick_look(&mut self, chats: &[String], newest: u32) {
        let chats = chats.to_vec();
        self.defer(move |a| a.on_watch_quick_look(&chats, newest));
    }

    fn unwatch_quick_look(&mut self) {
        self.defer(|a| a.on_unwatch_quick_look());
    }

    fn park_messages(&mut self) {
        self.defer(|a| a.on_park());
    }

    fn unpark_messages(&mut self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_unpark(&chat));
    }

    fn unwatch_parked(&mut self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_unwatch_parked(&chat));
    }

    fn send_text(&mut self, chat: &str, text: &str, key: &str, reply: Option<&str>, mentions: &[String]) {
        let (chat, text, key, reply, mentions) =
            (chat.to_string(), text.to_string(), key.to_string(), reply.map(String::from), mentions.to_vec());
        self.defer(move |a| a.on_send_text(&chat, &text, &key, reply.as_deref(), &mentions));
    }

    // The made-up account's message IDs are its own and never shared, so it finds a message without
    // its chat.
    fn react(&mut self, _chat: &str, message: &str, emoji: &str) {
        let (message, emoji) = (message.to_string(), emoji.to_string());
        self.defer(move |a| a.on_react(&message, &emoji));
    }

    fn edit(&mut self, _chat: &str, message: &str, text: &str) {
        let (message, text) = (message.to_string(), text.to_string());
        self.defer(move |a| a.on_edit(&message, &text));
    }

    fn delete(&mut self, _chat: &str, message: &str, everyone: bool) {
        let message = message.to_string();
        self.defer(move |a| a.on_delete(&message, everyone));
    }

    fn forward(&mut self, _chat: &str, messages: &[String], to: &[String]) {
        let (messages, to) = (messages.to_vec(), to.to_vec());
        self.defer(move |a| a.on_forward(&messages, &to));
    }

    fn message_info(&mut self, _chat: &str, message: &str) {
        let message = message.to_string();
        self.defer(move |a| a.on_message_info(&message));
    }

    fn chat_info(&mut self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_chat_info(&chat));
    }

    fn voice_play(&mut self, message: Option<&str>, at: Option<f64>, speed: f64) {
        let message = message.map(String::from);
        self.defer(move |a| a.on_voice_play(message, at, speed));
        self.clock_voice();
    }

    fn voice_pause(&mut self) {
        self.defer(|a| a.on_voice_pause());
    }

    fn voice_seek(&mut self, at: f64) {
        self.defer(move |a| a.on_voice_seek(at));
    }

    fn voice_speed(&mut self, speed: f64) {
        self.defer(move |a| a.on_voice_speed(speed));
    }

    fn voice_record(&mut self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_voice_record(&chat));
        self.clock_voice();
    }

    fn voice_stop(&mut self) {
        self.defer(|a| a.on_voice_stop());
    }

    fn voice_send(&mut self) {
        self.defer(|a| a.on_voice_send());
    }

    fn voice_cancel(&mut self) {
        self.defer(|a| a.on_voice_cancel());
    }

    fn draft(&mut self, chat: &str, text: &str) {
        let (chat, text) = (chat.to_string(), text.to_string());
        self.defer(move |a| a.on_draft(&chat, &text));
    }

    fn read(&mut self, chat: &str, up_to: &str) {
        let (chat, up_to) = (chat.to_string(), up_to.to_string());
        self.defer(move |a| a.on_read(&chat, &up_to));
    }

    fn opened(&mut self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| a.on_opened(&chat));
    }

    fn typing(&mut self, _chat: &str, _state: Typing) {}

    fn focus(&mut self, _focused: bool) {}

    fn find(&mut self, id: u64, query: &str, kind: Option<FoundKind>, chat: Option<&str>, limit: usize) {
        self.ask(find::Ask { id, query: query.to_string(), kind, chat: chat.map(String::from), limit });
    }

    fn start_chat_by_phone(&mut self, phone: &str) {
        let phone = phone.to_string();
        self.defer(move |a| a.on_start_by_phone(&phone));
    }

    fn retry(&mut self, message: &str) {
        let message = message.to_string();
        self.defer(move |a| a.on_retry(&message));
    }

    fn start_chat(&mut self, person: &str) {
        let person = person.to_string();
        self.defer(move |a| a.on_start_chat(&person));
    }

    fn mute(&mut self, chat: &str, until: Option<i64>) {
        self.change(chat, move |c| c.muted_until = until);
    }

    fn archive(&mut self, chat: &str, archived: bool) {
        self.change(chat, move |c| c.archived = archived);
    }

    fn pin(&mut self, chat: &str, pinned: bool) {
        self.change(chat, move |c| {
            c.pinned = pinned;
            let at = c.sort.split_once('~').map_or("0", |(_, at)| at).to_string();
            c.sort = format!("{}~{at}", u8::from(pinned));
        });
    }

    fn mark_unread(&mut self, chat: &str, unread: bool) {
        self.change(chat, move |c| c.marked_unread = unread);
    }

    fn link_start(&mut self, phone: Option<String>) {
        self.defer(move |a| a.on_link_start(phone));
    }
}

/// WhatsApp's own formatting into spans, as jawd's converter gives them: *bold*, _italic_,
/// ~strike~ and `mono`, links, and @Pedro as a mention of you.
pub fn markup(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = vec![];
    let mut push = |t: SpanKind, s: &str, person: Option<&str>| {
        if s.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.t == t && t == SpanKind::Text => last.s.push_str(s),
            _ => out.push(Span {
                t,
                s: s.into(),
                person: person.map(String::from),
                url: (t == SpanKind::Link).then(|| s.into()),
            }),
        }
    };
    let mut rest = text;
    while !rest.is_empty() {
        let next = rest.find(['*', '_', '~', '`', '@', 'h']).unwrap_or(rest.len());
        push(SpanKind::Text, &rest[..next], None);
        rest = &rest[next..];
        let Some(c) = rest.chars().next() else { break };
        if c == 'h' && (rest.starts_with("https://") || rest.starts_with("http://")) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            push(SpanKind::Link, &rest[..end], None);
            rest = &rest[end..];
            continue;
        }
        if c == '@' && rest.starts_with("@Pedro") {
            push(SpanKind::Mention, "@Pedro", Some(ME));
            rest = &rest["@Pedro".len()..];
            continue;
        }
        let kind = match c {
            '*' => SpanKind::Bold,
            '_' => SpanKind::Italic,
            '~' => SpanKind::Strike,
            '`' => SpanKind::Mono,
            _ => SpanKind::Text,
        };
        match rest[1..].find(c).filter(|_| kind != SpanKind::Text) {
            Some(end) if end > 0 => {
                push(kind, &rest[1..1 + end], None);
                rest = &rest[end + 2..];
            }
            _ => {
                push(SpanKind::Text, &rest[..c.len_utf8()], None);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// People on WhatsApp you have no chat with yet.
fn contacts() -> Vec<Person> {
    [("31", "Artur Reis"), ("32", "Alice Rocha"), ("33", "Tomás Duarte"), ("34", "Inês Duarte")]
        .iter()
        .map(|(n, name)| Person {
            id: format!("person:{n}"),
            name: name.to_string(),
            phone: Some(format!("+3519130000{n}")),
        })
        .collect()
}

const NAMES: &[&str] = &["Ana", "Rui", "Inês", "Sofia", "Tiago", "Marta", "João", "Leonor"];
const WORDS: &[&str] = &[
    "sim", "não", "amanhã", "hoje", "jantar", "vinho", "praia", "comboio", "às", "nove", "quem", "vem", "leva", "o",
    "a", "bolo", "fotos", "ok", "boa", "ideia", "chego", "tarde", "cedo", "logo", "depois", "falamos", "obrigado",
];

/// A chat being written: messages added oldest first.
struct Writing {
    thread: Thread,
    now: i64,
    seq: u64,
}

fn person(id: &str, name: &str, colour: u8) -> Sender {
    Sender { id: id.into(), name: name.into(), colour }
}

impl Writing {
    fn new(id: &str, name: &str, kind: ChatKind, now: i64) -> Writing {
        let chat = Chat {
            id: id.into(),
            v: 1,
            sort: format!("0~{:013}", now - 2 * DAY),
            kind,
            name: name.into(),
            last: None,
            unread: 0,
            mentions: 0,
            pinned: false,
            muted_until: None,
            archived: false,
            marked_unread: false,
            draft: None,
            typing: vec![],
            community: None,
            folders: vec![],
        };
        Writing { thread: Thread { chat, messages: vec![] }, now, seq: 0 }
    }

    /// A message `ago` minutes before now.
    fn add(&mut self, ago: f64, from: Option<&Sender>, kind: Kind, text: &str) -> &mut Message {
        self.seq += 1;
        let at = self.now - (ago * MINUTE as f64) as i64;
        let id = format!("{}-{:05}", self.thread.chat.id.split('@').next().unwrap_or("chat"), self.seq);
        let chat = self.thread.chat.id.clone();
        self.thread.messages.push(Message {
            id,
            v: 1,
            chat,
            sort: format!("{at:013}~{:09}", self.seq),
            at,
            from: from.cloned(),
            mine: from.is_none(),
            kind,
            spans: markup(text),
            reply: None,
            forwarded: 0,
            reactions: vec![],
            status: from.is_none().then_some(Tick::Read),
            edited: false,
            deleted: false,
            summary: None,
            system: None,
            media: None,
            album: None,
            voice: None,
        });
        self.thread.messages.last_mut().expect("just added")
    }

    fn theirs(&mut self, ago: f64, from: &Sender, text: &str) -> &mut Message {
        self.add(ago, Some(from), Kind::Text, text)
    }

    fn mine(&mut self, ago: f64, text: &str) -> &mut Message {
        self.add(ago, None, Kind::Text, text)
    }

    fn other(&mut self, ago: f64, from: Option<&Sender>, kind: Kind, summary: &str, caption: &str) -> &mut Message {
        let m = self.add(ago, from, kind, caption);
        m.summary = Some(summary.into());
        m
    }

    fn system(&mut self, ago: f64, kind: Kind, said: &str) {
        let m = self.add(ago, None, kind, "");
        m.mine = false;
        m.status = None;
        m.system = Some(said.into());
    }

    /// The chat's row from its newest message, with `unread` of theirs not yet seen.
    fn done(mut self, unread: u32, mentions: u32) -> Thread {
        let c = &mut self.thread.chat;
        if let Some(m) = self.thread.messages.last() {
            let summary = match m.kind {
                Kind::Text => crate::fmt::one_line(&m.spans.iter().map(|s| s.s.as_str()).collect::<String>()),
                Kind::System | Kind::Call => m.system.clone().unwrap_or_default(),
                _ => m.summary.clone().unwrap_or_default(),
            };
            let from = m.from.as_ref().filter(|_| c.kind == ChatKind::Group).map(|f| f.name.clone());
            c.last = Some(Last { from, summary, status: m.status.filter(|_| m.mine), at: m.at });
            c.sort = format!("{}~{:013}", u8::from(c.pinned), m.at);
        }
        c.unread = unread;
        c.mentions = mentions;
        self.thread
    }
}

/// What the climbing group says under its photos, the links it shares, and its files.
const CAPTIONS: &[&str] = &[
    "",
    "Sintra, the slab",
    "",
    "na praia depois da escalada",
    "Cascais at sunset",
    "",
    "a vista do topo",
    "praia do Guincho",
];
const LINKS: &[(&str, &str)] = &[
    ("o guia de Sintra", "https://thecrag.com/en/climbing/portugal/sintra"),
    ("previsão para sábado", "https://www.ipma.pt/pt/otempo/prev.localidade.hora/"),
    ("Peneda in May?", "https://8a.nu/crags/sportclimbing/portugal/peneda"),
    ("comboio para Sintra", "https://www.cp.pt/passageiros/pt/consultar-horarios"),
];
const FILES: &[&str] = &[
    "topo-sintra.pdf · 1.2 MB · 4 pages",
    "rota-sintra-cascais.gpx · 88 KB",
    "seguro-escalada-2026.pdf · 310 KB · 2 pages",
];

/// The photos the climbing group shares, and a flood brings into the open chat.
const HISTORY_PHOTOS: [&str; 5] = ["city", "sunset", "lake", "bars", "beach"];

/// The made-up account: a dozen chats around a Friday lunchtime in Lisbon.
fn made_up(now: i64) -> Vec<Thread> {
    let h = 60.0;
    let ana = person("person:1", "Ana", 0);
    let mae = person("person:2", "Mãe", 0);
    let rui = person("person:3", "Rui", 1);
    let ines = person("person:4", "Inês", 2);
    let sofia = person("person:5", "Sofia", 3);
    let mut threads = vec![];

    let mut w = Writing::new("ana", "Ana Ribeiro", ChatKind::Direct, now);
    w.thread.chat.pinned = true;
    w.thread.chat.typing = vec!["Ana".into()];
    w.theirs(14.0 * h + 52.0, &ana, "chegaste bem?");
    w.mine(14.0 * h + 50.0, "cheguei, boa noite");
    w.mine(4.0 * h + 50.0, "bom dia! dormiste bem?");
    w.theirs(4.0 * h + 47.0, &ana, "sim! a fazer café, e tu?");
    w.mine(4.0 * h + 46.0, "reunião às 10, falamos ao almoço");
    w.theirs(4.0, &ana, "did you get the photos from Sunday?");
    let photo = w.other(3.9, Some(&ana), Kind::Image, "▣ photo · 4032 × 3024", "");
    photo.reactions = vec![Reaction { emoji: "❤".into(), n: 2, mine: true }];
    photo.media = Some(Box::new(pictures::media("lake")));
    w.mine(3.0, "yes, the one at the lake");
    w.mine(2.9, "a Sofia tirou mais, vou pedir-lhe");
    voice::voiced(w.other(1.0, Some(&ana), Kind::Voice, "▶ voice message 0:14", ""), 14.0, 1, false);
    w.mine(0.9, "haha, ela vai adorar");
    w.theirs(0.0, &ana, "vens jantar *sexta*? a tua mãe pergunta");
    threads.push(w.done(1, 0));

    let mut w = Writing::new("mae", "Mãe", ChatKind::Direct, now);
    w.thread.chat.pinned = true;
    w.theirs(3.0 * h, &mae, "deixei-te sopa no frigorífico");
    w.mine(2.9 * h, "obrigado mãe ❤");
    w.theirs(40.0, &mae, "liga-me quando puderes");
    w.mine(22.0, "ok, ligo-te logo à noite");
    threads.push(w.done(0, 0));

    let helena = person("person:30", "Prof. Helena", 4);
    let mut w = Writing::new("escola", "Escola do Bairro", ChatKind::Group, now);
    for (i, said) in [
        "Bom dia! As inscrições para a natação abrem segunda.",
        "Os livros de fichas chegaram, podem levantar na secretaria.",
        "Lembrem-se das autorizações para a visita de estudo.",
        "A festa de Natal será a 18 de dezembro.",
        "3.º B: reunião na quinta às 18h",
    ]
    .iter()
    .enumerate()
    {
        w.theirs(107.0 + (4 - i) as f64 * 3.0 * h, &helena, said);
    }
    threads.push(w.done(9, 0));

    let mut w = Writing::new("familia", "Família Gomes", ChatKind::Group, now);
    let wed = 2.0 * 24.0 * h;
    w.theirs(wed + 5.0 * h, &mae, "Bom dia a todos! O *jantar de sexta* é às 20h, cá em casa.");
    w.theirs(wed + 5.0 * h - 1.0, &mae, "@Pedro trazes o _vinho_? e ~não te esqueças~ do pão");
    w.theirs(wed + 5.0 * h - 1.0, &mae, "a lista: `pão, vinho, queijo da serra`");
    w.theirs(wed + 5.0 * h - 8.0, &rui, "eu levo o vinho, o Pedro leva o pão").reply = Some(Quote {
        id: None,
        from: Some("Mãe".into()),
        colour: 0,
        summary: "@Pedro trazes o vinho? e não te esqueças do pão".into(),
    });
    w.theirs(wed + 4.0 * h + 31.0, &ines, "e para o verão? https://casadapraia.pt/ericeira");
    let album = w.other(wed + 2.0 * h, Some(&ines), Kind::Album, "▣ 2 photos", "a vista do quarto");
    let items = ["room", "beach"].iter().enumerate();
    let items =
        items.map(|(i, p)| AlbumItem { id: format!("{}-{i}", album.id), kind: Kind::Image, media: pictures::media(p) });
    album.album = Some(Album { items: items.collect() });
    w.other(wed + 2.0 * h - 6.0, Some(&sofia), Kind::Sticker, "◇ sticker", "").media =
        Some(Box::new(pictures::media("heart")));
    let video =
        w.other(wed + 1.0 * h + 42.0, Some(&rui), Kind::Video, "▶ video 0:42 · 18.4 MB", "o Tomás a nadar sozinho!");
    video.media = Some(Box::new(Media { seconds: Some(42.0), bytes: Some(18_400_000), ..pictures::media("pool") }));
    voice::voiced(w.other(wed + 1.0 * h + 22.0, Some(&mae), Kind::Voice, "▶ voice message 0:31", ""), 31.0, 2, true);
    voice::voiced(w.other(wed + 1.0 * h + 21.0, Some(&mae), Kind::Voice, "▶ voice message 0:09", ""), 9.0, 5, false);
    w.other(wed + 57.0, Some(&rui), Kind::Document, "▣ orcamento-obras-cozinha.pdf · 2.4 MB · 3 pages", "");
    w.other(wed + 32.0, Some(&mae), Kind::Location, "⌖ Miradouro da Senhora do Monte · Lisbon", "");
    w.other(wed + 21.0, Some(&ines), Kind::Contact, "◉ Dr.ª Marta Lopes · +351 213 456 789", "");
    w.mine(3.0 * h, "eu levo o pão e a sobremesa");
    w.theirs(2.0 * h + 20.0, &mae, "boa! 😘");
    w.theirs(2.0 * h + 15.0, &ines, "@Pedro vens buscar a avó?");
    w.other(2.0 * h + 10.0, Some(&ines), Kind::Image, "▣ photo", "").media =
        Some(Box::new(pictures::media("portrait")));
    threads.push(w.done(3, 1));

    let mut w = Writing::new("pedro-ana", "Pedro & Ana", ChatKind::Group, now);
    w.other(20.0 * h, Some(&ana), Kind::Document, "▣ bilhetes-comboio.pdf", "");
    threads.push(w.done(0, 0));

    let mut w = Writing::new("rui", "Rui Costa", ChatKind::Direct, now);
    w.theirs(21.0 * h, &rui, "sábado há futebol?");
    w.mine(20.9 * h, "há, às 10");
    w.thread.chat.draft = Some("levo o bolo?".into());
    threads.push(w.done(0, 0));

    let mut w = Writing::new("jantar", "Jantar de sexta", ChatKind::Group, now);
    let thu = 24.0 * h;
    let tiago = person("person:6", "Tiago", 4);
    w.system(thu - 4.0 * h - 40.0, Kind::System, "Rui added Sofia");
    w.other(thu - 4.0 * h - 20.0, Some(&rui), Kind::Poll, "☰ Onde jantamos sexta? · one choice · 6 voted", "");
    w.theirs(thu - 4.0 * h - 24.0, &ines, "Ramiro! há anos que não vou").reactions =
        vec![Reaction { emoji: "❤".into(), n: 3, mine: true }, Reaction { emoji: "☺".into(), n: 1, mine: false }];
    w.system(thu - 5.0 * h - 2.0, Kind::Call, "✆ Missed voice call from Rui");
    w.mine(thu - 5.0 * h - 5.0, "até já").status = Some(Tick::Pending);
    w.mine(thu - 5.0 * h - 5.0, "a sair de casa").status = Some(Tick::Sent);
    w.mine(thu - 5.0 * h - 6.0, "estou no metro").status = Some(Tick::Delivered);
    w.mine(thu - 5.0 * h - 31.0, "cheguei!");
    let mine = w.other(thu - 5.0 * h - 33.0, None, Kind::Voice, "▶ voice message 0:06", "");
    mine.status = Some(Tick::Played);
    voice::voiced(mine, 6.0, 3, true);
    w.mine(thu - 5.0 * h - 34.0, "esqueci-me do pão").status = Some(Tick::Failed);
    w.theirs(thu - 5.0 * h - 40.0, &sofia, "afinal chego às 21h").edited = true;
    w.theirs(thu - 5.0 * h - 41.0, &sofia, "").deleted = true;
    w.mine(thu - 5.0 * h - 42.0, "").deleted = true;
    w.add(thu - 5.0 * h - 45.0, Some(&tiago), Kind::Unknown, "");
    w.theirs(thu - 5.0 * h - 50.0, &rui, "alguém tem o número do restaurante?");
    w.theirs(thu - 5.0 * h - 52.0, &rui, "liguei agora, mesa para 6 às 20h30");
    let mut sorted = std::mem::take(&mut w.thread.messages);
    sorted.sort_by_key(|m| m.at);
    w.thread.messages = sorted;
    threads.push(w.done(0, 0));

    let mut w = Writing::new("sofia", "Sofia Martins", ChatKind::Direct, now);
    voice::voiced(w.other(4.0 * 24.0 * h, Some(&sofia), Kind::Voice, "▶ voice message 0:42", ""), 42.0, 4, false);
    threads.push(w.done(0, 0));

    let mut w = Writing::new("avo", "Avó Lurdes", ChatKind::Direct, now);
    w.thread.chat.muted_until = Some(i64::MAX);
    w.theirs(5.0 * 24.0 * h, &person("person:7", "Avó", 5), "Beijinhos a todos");
    threads.push(w.done(1, 0));

    let mut w = Writing::new("climbing", "Lisbon Climbing", ChatKind::Group, now);
    let climbers: Vec<Sender> = ["Marta", "João", "Leonor", "Duarte", "Bea"]
        .iter()
        .enumerate()
        .map(|(i, n)| person(&format!("person:{}", 20 + i), n, i as u8))
        .collect();
    let mut rng = 7u64;
    let count = 1_500;
    for i in 0..count {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ago = 3.0 * h + (count - i) as f64 * 55.0;
        let words: Vec<&str> =
            (0..2 + (rng >> 40) as usize % 14).map(|k| WORDS[((rng >> (k % 24)) as usize) % WORDS.len()]).collect();
        let text = words.join(" ");
        let who = &climbers[(rng >> 50) as usize % climbers.len()];
        if i % 20 == 10 {
            let p = HISTORY_PHOTOS[(i / 20) % HISTORY_PHOTOS.len()];
            let caption = CAPTIONS[(i / 20) % CAPTIONS.len()];
            let m = w.other(ago, Some(who), Kind::Image, "▣ photo", caption);
            m.media = Some(Box::new(pictures::media(p)));
        } else if i % 45 == 15 {
            let (said, url) = LINKS[(i / 45) % LINKS.len()];
            w.theirs(ago, who, &format!("{said} {url}"));
        } else if i % 90 == 50 {
            let file = FILES[(i / 90) % FILES.len()];
            w.other(ago, Some(who), Kind::Document, &format!("▣ {file}"), "");
        } else if (rng >> 13).is_multiple_of(5) {
            w.mine(ago, &text);
        } else {
            w.theirs(ago, &climbers[(rng >> 50) as usize % climbers.len()], &text);
        }
    }
    w.theirs(3.0 * h, &climbers[0], "Saturday at 9?");
    threads.push(w.done(14, 0));

    let mut w = Writing::new("mariana", "Mariana Lopes", ChatKind::Direct, now);
    w.thread.chat.sort = format!("0~{:013}", now - 26 * h as i64 * MINUTE);
    threads.push(w.done(0, 0));

    let mut w = Writing::new("tiago", "Tiago Fonseca", ChatKind::Direct, now);
    w.mine(9.0 * 24.0 * h + 5.0, "obrigado pela boleia!");
    w.theirs(9.0 * 24.0 * h, &tiago, "de nada, quando quiseres");
    threads.push(w.done(0, 0));

    let mut w = Writing::new("ginasio", "Ginásio do Bairro", ChatKind::Group, now);
    w.thread.chat.muted_until = Some(i64::MAX);
    w.theirs(6.0 * h, &person("person:40", "Recepção", 6), "Aulas de sábado canceladas esta semana");
    threads.push(w.done(14, 0));

    // A community, its groups under it, the announcement group first.
    let days = 24.0 * h;
    let junta = person("person:41", "Junta de Freguesia", 5);
    let groups: [(&str, &str, ChatKind, f64, &str, u32); 3] = [
        (
            "bairro-avisos",
            "Avisos do Bairro",
            ChatKind::Announcement,
            11.0 * days,
            "Recolha de monos na quinta-feira",
            1,
        ),
        ("bairro-festa", "Festa de Natal", ChatKind::Group, 12.0 * days, "quem traz as luzes?", 2),
        ("bairro-biblioteca", "Biblioteca", ChatKind::Group, 13.0 * days, "o clube de leitura muda para terça", 0),
    ];
    let mut unread = 0;
    for (id, name, kind, ago, said, n) in groups {
        let mut w = Writing::new(id, name, kind, now);
        w.thread.chat.community = Some("bairro".into());
        w.theirs(ago, &junta, said);
        unread += n;
        threads.push(w.done(n, 0));
    }
    let mut w = Writing::new("bairro", "Bairro de Alvalade", ChatKind::Community, now);
    w.theirs(11.0 * days, &junta, "Recolha de monos na quinta-feira");
    let mut community = w.done(0, 0);
    community.chat.unread = unread;
    community.messages.clear();
    threads.push(community);

    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_becomes_spans_as_jawd_gives_them() {
        let spans = markup("@Pedro trazes o _vinho_? e ~não~ do *pão* `já` https://x.pt ok");
        let kinds: Vec<(SpanKind, &str)> = spans.iter().map(|s| (s.t, s.s.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (SpanKind::Mention, "@Pedro"),
                (SpanKind::Text, " trazes o "),
                (SpanKind::Italic, "vinho"),
                (SpanKind::Text, "? e "),
                (SpanKind::Strike, "não"),
                (SpanKind::Text, " do "),
                (SpanKind::Bold, "pão"),
                (SpanKind::Text, " "),
                (SpanKind::Mono, "já"),
                (SpanKind::Text, " "),
                (SpanKind::Link, "https://x.pt"),
                (SpanKind::Text, " ok"),
            ]
        );
        assert_eq!(markup("a_b c")[0].s, "a_b c", "a lone marker is only text");
    }
}
