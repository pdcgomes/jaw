//! The socket interfaces talk to jawd on. As haunt's daemon does, it refuses to start while another
//! jawd answers, clears a stale socket, and is readable only by its owner; it also talks only to
//! the user who owns jaw's folder.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use jaw_proto::find::{ChatOpened, Find};
use jaw_proto::settings::{Settings as Terminal, SettingsKept};
use jaw_proto::sync::HistoryMore;
use jaw_proto::themes::Themes;
use jaw_proto::{Error, Focus, Hello, LinkStart, Outcome, PROTOCOL, Push, Reply, Request, code};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, watch};

use crate::attention::Viewer;
use crate::engine::{EngineError, pb};
use crate::feed;
use crate::find::{Cancel, Cancelled};
use crate::layout::Pushed;
use crate::state::State;
use crate::sync::Refused;

/// Lines waiting for an interface to read them. Window updates wait in their windows instead, so a
/// slow interface never holds anything else up, and a reply, a `layout` push or a `settings` push
/// that finds no room waits on its own, one at most.
const WAITING_LINES: usize = 64;
/// Requests a connection's lane holds, waiting or being answered, before it reads no more.
pub const LANE_MOST: usize = 64;
/// Requests a connection can have answered on tasks of their own at once; past it, another is
/// refused.
pub const APART_MOST: usize = 64;

/// The last connection's number.
static CONNECTIONS: AtomicU64 = AtomicU64::new(0);

/// What a connection has said, its windows, and what it's looking at.
struct Session {
    /// Its number, unique while jawd runs.
    id: u64,
    greeted: bool,
    /// The interface's name, as hello gave it.
    client: Arc<str>,
    windows: Option<feed::Client>,
    viewer: Viewer,
    /// Its interface's layout, as its other connections keep it.
    layout: Option<watch::Receiver<Option<Pushed>>>,
    /// Its domain's settings as this connection was last told them: made at its hello, whose
    /// answer carries what this receiver has seen, so no change between that answer and its first
    /// push is missed; none for a client jawd keeps no settings for.
    settings: Option<watch::Receiver<Terminal>>,
}

/// Every connection's lane, waiting reply and requests answered on tasks of their own, summed for
/// `jawd status`.
#[derive(Default)]
pub struct Lanes {
    requests: AtomicI64,
    full: AtomicI64,
    waiting: AtomicI64,
    apart: AtomicI64,
}

impl Lanes {
    /// The requests in every connection's lane, read and not yet answered.
    pub fn requests(&self) -> u64 {
        Lanes::count(&self.requests)
    }

    /// The connections whose lane holds `LANE_MOST`, which jawd reads no more from meanwhile.
    pub fn full(&self) -> u64 {
        Lanes::count(&self.full)
    }

    /// The connections whose reply, `layout` push or `settings` push waits for room in the queue to
    /// their interface.
    pub fn waiting(&self) -> u64 {
        Lanes::count(&self.waiting)
    }

    /// The requests every connection has being answered on tasks of their own, finds among them.
    pub fn apart(&self) -> u64 {
        Lanes::count(&self.apart)
    }

    fn count(count: &AtomicI64) -> u64 {
        u64::try_from(count.load(Ordering::Relaxed)).unwrap_or(0)
    }

    fn add(&self, before: [i64; 4], after: [i64; 4]) {
        let counts = [&self.requests, &self.full, &self.waiting, &self.apart];
        for ((count, before), after) in counts.into_iter().zip(before).zip(after) {
            count.fetch_add(after - before, Ordering::Relaxed);
        }
    }
}

/// What a connection owes its interface: the requests it has sent down its lane and not had back,
/// those being answered on tasks of their own, and the reply, `layout` push or `settings` push
/// waiting for room in the queue, one at most. It's counted in `Lanes` for as long as the
/// connection lasts, each change as it's made.
struct Owed<'a> {
    lanes: &'a Lanes,
    in_lane: usize,
    apart: usize,
    waiting: Option<Vec<u8>>,
}

impl<'a> Owed<'a> {
    fn new(lanes: &'a Lanes) -> Owed<'a> {
        Owed { lanes, in_lane: 0, apart: 0, waiting: None }
    }

    fn full(&self) -> bool {
        self.in_lane >= LANE_MOST
    }

    /// Whether every request read has had its reply sent, and nothing waits to be.
    fn settled(&self) -> bool {
        self.in_lane == 0 && self.apart == 0 && self.waiting.is_none()
    }

    /// A request sent down the lane.
    fn took(&mut self) {
        self.change(|owed| owed.in_lane += 1);
    }

    /// A request's reply back from the lane.
    fn answered(&mut self) {
        self.change(|owed| owed.in_lane -= 1);
    }

    /// A request given a task of its own.
    fn parted(&mut self) {
        self.change(|owed| owed.apart += 1);
    }

    /// A reply back from a task of its own.
    fn returned(&mut self) {
        self.change(|owed| owed.apart -= 1);
    }

    /// Keeps `line` until there's room for it.
    fn wait(&mut self, line: Vec<u8>) {
        self.change(|owed| owed.waiting = Some(line));
    }

    /// The line that waited, now there's room for it.
    fn sent(&mut self) -> Vec<u8> {
        self.change(|owed| owed.waiting.take().expect("a line waits"))
    }

    /// Its part in `Lanes`.
    fn counts(&self) -> [i64; 4] {
        [self.in_lane as i64, i64::from(self.full()), i64::from(self.waiting.is_some()), self.apart as i64]
    }

    fn change<T>(&mut self, change: impl FnOnce(&mut Self) -> T) -> T {
        let before = self.counts();
        let changed = change(self);
        self.lanes.add(before, self.counts());
        changed
    }
}

impl Drop for Owed<'_> {
    fn drop(&mut self) {
        self.lanes.add(self.counts(), [0; 4]);
    }
}

pub fn listen(path: &Path) -> Result<UnixListener> {
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        bail!("another jawd is already running on {}", path.display());
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).with_context(|| format!("can't listen on {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Serves every connection until jawd stops.
pub async fn serve(state: Arc<State>, listener: UnixListener) -> Result<()> {
    loop {
        let (stream, _) = tokio::select! {
            accepted = listener.accept() => accepted?,
            _ = state.stopping() => return Ok(()),
        };
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = one(&state, stream).await {
                eprintln!("jawd: a connection ended with an error: {e:#}");
            }
        });
    }
}

/// One connection. Its windows, hello and focus are answered on its own task; a find, and any
/// request that can take long, on a task of its own; and every other request down the
/// connection's lane, in the order sent, so a watch never waits on jaw.db's writer, the engine or
/// a search, nor a send on a long request. Requests go ahead of the windows' flushes. A reply, a
/// `layout` push or a `settings` push that finds the queue to the interface full waits, and
/// meanwhile no request is read, no other reply or push is sent, and the windows send nothing, so
/// nothing goes ahead of the watch a reply may be the result of; but they still take their
/// changes, so an interface that leaves its queue full is told to watch again, a reply waiting or
/// not. Once the interface has sent all it will, every request it sent is still answered before
/// the connection ends.
async fn one(state: &Arc<State>, stream: UnixStream) -> Result<()> {
    let peer = match stream.peer_cred() {
        Ok(credentials) => credentials.uid(),
        // A second jawd checking whether this one answers connects and leaves at once.
        Err(e) if e.kind() == std::io::ErrorKind::NotConnected => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if peer != state.owner {
        bail!("refused a connection from user {peer}, who doesn't own jaw's folder");
    }
    let (read, write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let out = writer(write);
    let mut session = Session {
        id: CONNECTIONS.fetch_add(1, Ordering::Relaxed) + 1,
        greeted: false,
        client: Arc::from(""),
        windows: state.feed().map(|feed| feed.client(out.clone())),
        viewer: state.attention().viewer(),
        layout: None,
        settings: None,
    };
    // Pushes start once the connection has said hello.
    let mut pushes: Option<broadcast::Receiver<Push>> = None;
    // Finds and the requests on tasks of their own answer when they're done, so the next line,
    // perhaps a newer find, is read meanwhile.
    let (found, mut finished) = mpsc::unbounded_channel();
    let mut finding = Finding::default();
    let (answered, mut answers) = mpsc::unbounded_channel();
    let lane = lane(state.clone(), answered);
    let mut owed = Owed::new(&state.lanes);
    let mut reading = true;
    loop {
        if !reading && owed.settled() {
            return Ok(());
        }
        if session.greeted && pushes.is_none() && owed.waiting.is_none() {
            pushes = Some(state.pushes.subscribe());
        }
        let open = owed.waiting.is_none();
        tokio::select! {
            biased;
            permit = out.reserve(), if !open => {
                permit.context("the interface stopped reading")?.send(owed.sent());
                if let Some(windows) = session.windows.as_mut() {
                    windows.hold(false);
                }
            }
            _ = state.stopping() => return Ok(()),
            reply = answers.recv(), if open => {
                // A lane whose task has died answers nothing more, so its interface is to connect
                // and ask again.
                let reply = reply.context("the connection's lane stopped")?;
                owed.answered();
                send(&out, &reply, &mut owed, &mut session)?;
            }
            Some(reply) = finished.recv(), if open => {
                owed.returned();
                send(&out, &reply, &mut owed, &mut session)?;
            }
            line = lines.next_line(), if reading && open && !owed.full() => {
                // An interface that has sent all it will, its writing side closed, still gets the
                // replies it's owed.
                let Some(line) = line? else {
                    reading = false;
                    continue;
                };
                if line.trim().is_empty() {
                    continue;
                }
                match take(state, &line, &mut session).await {
                    Taken::Answer(reply) => send(&out, &reply, &mut owed, &mut session)?,
                    Taken::Find(request) | Taken::Apart(request) if owed.apart >= APART_MOST => {
                        let mut error = Error::new(
                            code::LIMIT,
                            format!("this interface already has {APART_MOST} finds and saves being answered"),
                        );
                        error.data = Some(serde_json::json!({ "limit": "apart", "most": APART_MOST }));
                        send(&out, &failed(request.id, error), &mut owed, &mut session)?;
                    }
                    Taken::Find(request) => {
                        owed.parted();
                        finding.start(state, request, found.clone());
                    }
                    Taken::Apart(request) => {
                        owed.parted();
                        apart(state.clone(), request, found.clone());
                    }
                    Taken::Lane(request) => {
                        owed.took();
                        let laned = Laned { request, from: session.id, client: session.client.clone() };
                        lane.send(laned).context("the connection's lane stopped")?;
                    }
                }
            }
            ready = windows(&mut session.windows) => {
                if let Some(windows) = session.windows.as_mut() {
                    windows.handle(ready).await;
                }
            }
            push = next(&mut pushes), if open => match push {
                Ok(push) => {
                    // The lane's replies handed over before this push was sent go ahead of it, so
                    // a request's answer comes before the pushes it sends once it's answered.
                    while owed.waiting.is_none()
                        && let Ok(reply) = answers.try_recv()
                    {
                        owed.answered();
                        send(&out, &reply, &mut owed, &mut session)?;
                    }
                    // A connection too slow for its pushes misses them; the next says how things are.
                    if owed.waiting.is_none() {
                        drop(out.try_send(encode(&push)));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => pushes = None,
            },
            // A layout another of the interface's connections kept waits for room as a reply
            // does, never dropped. The channel holds only the latest, so leaving it unread while a
            // line waits loses nothing, and the connection ends on the latest.
            kept = layout_kept(&mut session.layout), if open => match kept {
                Some(Some(pushed)) if pushed.from != session.id => {
                    let params = serde_json::to_value(&pushed.saved).expect("jaw-proto's types serialize");
                    send(&out, &Push { method: "layout".into(), params }, &mut owed, &mut session)?;
                }
                Some(_) => {}
                None => session.layout = None,
            },
            // The domain's settings, once they change, wait for room as a reply does, never
            // dropped. The channel holds only the latest, so leaving it unread while a line waits
            // loses nothing, and the connection ends on the latest.
            settings = changed(&mut session.settings, session.greeted), if open => {
                // The lane's replies handed over before these settings were read go ahead of
                // their push: a change's answer is handed over before a later change can be
                // published, so it never comes after that change's push.
                while owed.waiting.is_none()
                    && let Ok(reply) = answers.try_recv()
                {
                    owed.answered();
                    send(&out, &reply, &mut owed, &mut session)?;
                }
                if owed.waiting.is_none() {
                    let push = Push {
                        method: "settings".into(),
                        params: serde_json::to_value(settings).expect("jawd's own types serialize"),
                    };
                    send(&out, &push, &mut owed, &mut session)?;
                } else if let Some(settings) = session.settings.as_mut() {
                    // A reply waits for room, so the push comes after it, as the latest then.
                    settings.mark_changed();
                }
            }
        }
    }
}

/// Puts `line`, a reply, a `layout` push or a `settings` push, in the queue to the interface, or,
/// with no room there, keeps it waiting and holds the windows.
fn send(
    out: &mpsc::Sender<Vec<u8>>,
    line: &impl serde::Serialize,
    owed: &mut Owed<'_>,
    session: &mut Session,
) -> Result<()> {
    match out.try_send(encode(line)) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(line)) => {
            owed.wait(line);
            if let Some(windows) = session.windows.as_mut() {
                windows.hold(true);
            }
            Ok(())
        }
        Err(mpsc::error::TrySendError::Closed(_)) => bail!("the interface stopped reading"),
    }
}

/// A request sent down a connection's lane, with the connection's number and the name its hello
/// gave as they were when the request was read, so a hello answered meanwhile changes neither.
struct Laned {
    request: Request,
    from: u64,
    client: Arc<str>,
}

/// A connection's lane: answers the requests sent down it one at a time, in order, each reply to
/// `answered`, until the connection or jawd stops; the request being answered then is answered
/// all the same. A push a request sends once it's answered, as a logout's `connection` push, goes
/// to every connection after the reply is handed over, so the one that asked has it first.
fn lane(state: Arc<State>, answered: mpsc::UnboundedSender<Reply>) -> mpsc::UnboundedSender<Laned> {
    let (lane, mut requests) = mpsc::unbounded_channel::<Laned>();
    tokio::spawn(async move {
        loop {
            let laned = tokio::select! {
                laned = requests.recv() => laned,
                _ = state.stopping() => return,
            };
            let Some(Laned { request, from, client }) = laned else { return };
            if answered.is_closed() {
                return;
            }
            let (reply, then) = match request.method.as_str() {
                "link.logout" => logout(&state, request.id).await,
                "layout.get" | "layout.set" => (layouts(&state, request, &client, from).await, None),
                // Its answer is handed over while the settings are held, so `settings_set` hands
                // it over itself.
                "settings.set" => {
                    settings_set(&state, request, &client, &answered).await;
                    continue;
                }
                _ => (answer(&state, request).await, None),
            };
            let gone = answered.send(reply).is_err();
            if let Some(push) = then {
                let _ = state.pushes.send(push);
            }
            if gone {
                return;
            }
        }
    });
    lane
}

/// Answers `request` on a task of its own, its reply to `found` once it's done.
fn apart(state: Arc<State>, request: Request, found: mpsc::UnboundedSender<Reply>) {
    let id = request.id;
    on_own_task(id, async move { answer_apart(&state, request).await }, found);
}

/// Runs `answering` on a task of its own and sends its reply to `found`, or an error if it panics:
/// the connection counts every reply it's owed, and waits for them before it ends.
fn on_own_task(id: u64, answering: impl Future<Output = Reply> + Send + 'static, found: mpsc::UnboundedSender<Reply>) {
    tokio::spawn(async move {
        let reply = tokio::spawn(answering)
            .await
            .unwrap_or_else(|e| failed(id, Error::new(code::INTERNAL, format!("jawd failed while answering: {e}"))));
        let _ = found.send(reply);
    });
}

/// The next layout kept for the connection's interface; none once nothing will be.
async fn layout_kept(layout: &mut Option<watch::Receiver<Option<Pushed>>>) -> Option<Option<Pushed>> {
    match layout {
        Some(layout) => match layout.changed().await {
            Ok(()) => Some(layout.borrow_and_update().clone()),
            Err(_) => None,
        },
        None => std::future::pending().await,
    }
}

/// A connection's find in progress: the next find cancels it, and so does the connection ending.
#[derive(Default)]
struct Finding(Option<Arc<Cancel>>);

impl Finding {
    /// Cancels the last find, whose reply says so, and starts this one, which replies on `found`.
    fn start(&mut self, state: &State, request: Request, found: mpsc::UnboundedSender<Reply>) {
        if let Some(last) = self.0.take() {
            last.cancel();
        }
        let id = request.id;
        let params = if request.params.is_null() { serde_json::json!({}) } else { request.params };
        let find = match serde_json::from_value::<Find>(params) {
            Ok(find) => find,
            Err(e) => {
                let _ = found.send(failed(id, Error::new(code::INVALID_PARAMS, format!("find's params: {e}"))));
                return;
            }
        };
        let Some(finder) = state.finder.get().cloned() else {
            let _ = found.send(failed(id, Error::new(code::INTERNAL, "jawd has no store open to find things in")));
            return;
        };
        let cancel = Arc::new(Cancel::default());
        self.0 = Some(cancel.clone());
        let finding = async move {
            match finder.find(find, cancel).await {
                Ok(result) => succeeded(id, &result),
                Err(e) if e.is::<Cancelled>() => failed(id, Error::new(code::CANCELLED, e.to_string())),
                Err(e) => failed(id, Error::new(code::INTERNAL, format!("jawd couldn't search: {e:#}"))),
            }
        };
        on_own_task(id, finding, found);
    }
}

impl Drop for Finding {
    fn drop(&mut self) {
        if let Some(last) = self.0.take() {
            last.cancel();
        }
    }
}

async fn next(pushes: &mut Option<broadcast::Receiver<Push>>) -> Result<Push, broadcast::error::RecvError> {
    match pushes {
        Some(pushes) => pushes.recv().await,
        None => std::future::pending().await,
    }
}

async fn windows(windows: &mut Option<feed::Client>) -> feed::Ready {
    match windows {
        Some(windows) => windows.ready().await,
        None => std::future::pending().await,
    }
}

/// The settings once they've changed since this connection was last told them: only after its
/// hello, and never once jawd has stopped publishing them. A push waits for room as a reply
/// does, so none is dropped, and changes made meanwhile end in the latest.
async fn changed(settings: &mut Option<watch::Receiver<Terminal>>, greeted: bool) -> Terminal {
    match settings {
        Some(settings) if greeted => match settings.changed().await {
            Ok(()) => settings.borrow_and_update().clone(),
            Err(_) => std::future::pending().await,
        },
        _ => std::future::pending().await,
    }
}

/// Writes the lines sent to it to the interface, in order, until the connection ends.
fn writer(mut write: tokio::net::unix::OwnedWriteHalf) -> mpsc::Sender<Vec<u8>> {
    let (out, mut lines) = mpsc::channel::<Vec<u8>>(WAITING_LINES);
    tokio::spawn(async move {
        while let Some(line) = lines.recv().await {
            if write.write_all(&line).await.is_err() {
                return;
            }
        }
    });
    out
}

fn encode(line: &impl serde::Serialize) -> Vec<u8> {
    let mut out = serde_json::to_vec(line).expect("jawd's own types serialize");
    out.push(b'\n');
    out
}

/// What a connection does with a request.
enum Taken {
    /// Answered on the connection's own task.
    Answer(Reply),
    /// A find, answered on a task of its own and cancelled by the next.
    Find(Request),
    /// A request that can take long, answered on a task of its own, in no order with the others.
    Apart(Request),
    /// Answered down the connection's lane, after the requests sent there before it.
    Lane(Request),
}

/// Requests that can take long and that nothing needs in order, each answered on a task of its
/// own: `media.save` waits for its file to be downloaded, for as long as the phone is away. In
/// tests, `test.wait` stands in for one, and `test.panic.apart` for one whose task dies.
fn apart_method(method: &str) -> bool {
    method == "media.save" || (cfg!(test) && (method == "test.wait" || method == "test.panic.apart"))
}

/// Takes one line from an interface: its windows, hello and focus are answered here, a find and
/// the requests that can take long each on a task of its own, and the rest down the lane. A
/// request is answered here only if it's quick and needs nothing but memory, as `themes` is; one
/// that writes jaw.db, asks the engine or must follow those sent before it takes the lane, as
/// `settings.set`, `media.fetch` and `media.cancel` do.
async fn take(state: &State, line: &str, session: &mut Session) -> Taken {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return Taken::Answer(failed(0, Error::new(code::PARSE, "that line isn't JSON"))),
    };
    let request: Request = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(_) => {
            let error =
                Error::new(code::INVALID_REQUEST, "that's JSON, but not a request: it needs an id and a method");
            return Taken::Answer(failed(0, error));
        }
    };
    let id = request.id;
    if !session.greeted && request.method != "hello" {
        let error = Error::new(code::VERSION, "say hello first, with the protocol version you speak");
        return Taken::Answer(failed(id, error));
    }
    let reply = match request.method.as_str() {
        "hello" => match serde_json::from_value::<Hello>(request.params) {
            Ok(hello) if hello.protocol == PROTOCOL => {
                session.greeted = true;
                session.layout = state.layouts().and_then(|layouts| layouts.subscribe(&hello.client));
                session.settings = state.settings().and_then(|s| s.subscribe(&hello.client));
                session.client = hello.client.into();
                session.viewer.focus(hello.focus);
                if let Some(windows) = session.windows.as_mut() {
                    windows.hello(hello.since);
                }
                let mut welcome = state.welcome();
                welcome.settings = session.settings.as_mut().map(|s| s.borrow_and_update().clone());
                succeeded(id, &welcome)
            }
            Ok(hello) => {
                let older = if hello.protocol > PROTOCOL { "jawd is older" } else { "this interface is older" };
                let mut error = Error::new(
                    code::VERSION,
                    format!(
                        "jawd speaks protocol {PROTOCOL} and this interface speaks {}: {older}, so run make install again",
                        hello.protocol
                    ),
                );
                error.data = Some(serde_json::json!({ "jawd": PROTOCOL, "client": hello.protocol }));
                failed(id, error)
            }
            Err(e) => failed(id, Error::new(code::INVALID_PARAMS, format!("hello's params: {e}"))),
        },
        "focus" => match serde_json::from_value::<Focus>(request.params) {
            Ok(Focus { focused }) => {
                session.viewer.focus(focused);
                succeeded(id, &serde_json::json!({}))
            }
            Err(e) => failed(id, Error::new(code::INVALID_PARAMS, format!("focus's params: {e}"))),
        },
        // What the interface looks at changes at once; the open counted for frecency is kept
        // down the lane.
        "chat.opened" => match serde_json::from_value::<ChatOpened>(request.params.clone()) {
            Ok(opened) => {
                session.viewer.opened(opened.chat);
                return Taken::Lane(request);
            }
            Err(e) => failed(id, Error::new(code::INVALID_PARAMS, format!("chat.opened's params: {e}"))),
        },
        "find" => return Taken::Find(request),
        method if apart_method(method) => return Taken::Apart(request),
        "chats.watch" | "messages.watch" | "folders.watch" | "roster.watch" | "watch.move" | "unwatch" => match session
            .windows
            .as_mut()
        {
            Some(windows) => {
                let chat = match request.method.as_str() {
                    "messages.watch" => request.params.get("chat").and_then(Value::as_str).map(str::to_string),
                    _ => None,
                };
                let unwatching = match request.method.as_str() {
                    "unwatch" => request.params.get("watch").and_then(Value::as_str).map(str::to_string),
                    _ => None,
                };
                match windows.call(&request.method, request.params).await {
                    Ok(result) => {
                        if let (Some(chat), Some(watch)) = (chat, result.get("watch").and_then(Value::as_str)) {
                            session.viewer.watched(watch.into(), chat);
                        }
                        if let Some(watch) = unwatching {
                            session.viewer.unwatched(&watch);
                        }
                        succeeded(id, &result)
                    }
                    Err(error) => failed(id, error),
                }
            }
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so there's nothing to watch")),
        },
        "themes" => succeeded(id, &Themes { themes: jaw_proto::themes::all().to_vec() }),
        _ => return Taken::Lane(request),
    };
    Taken::Answer(reply)
}

/// The answer to a request down the lane: whatever waits on jaw.db's writer or the engine.
async fn answer(state: &State, request: Request) -> Reply {
    let id = request.id;
    match request.method.as_str() {
        #[cfg(test)]
        "test.panic" => panic!("the test made the lane's task panic"),
        "status" => {
            let mut status = state.status();
            status.engine.answer_us = state.ping().await;
            succeeded(id, &status)
        }
        "link.start" => {
            let params = if request.params.is_null() { serde_json::json!({}) } else { request.params };
            match serde_json::from_value::<LinkStart>(params) {
                Ok(start) => {
                    let phone = start.phone.unwrap_or_default();
                    engine(state, id, pb::command::Kind::LinkStart(pb::LinkStart { phone })).await
                }
                Err(e) => failed(id, Error::new(code::INVALID_PARAMS, format!("link.start's params: {e}"))),
            }
        }
        "chat.opened" => match (serde_json::from_value::<ChatOpened>(request.params), state.finder.get()) {
            (Err(e), _) => failed(id, Error::new(code::INVALID_PARAMS, format!("chat.opened's params: {e}"))),
            (Ok(_), None) => failed(id, Error::new(code::INTERNAL, "jawd has no store open to keep that in")),
            (Ok(opened), Some(finder)) => match finder.opened(opened.chat).await {
                Ok(()) => succeeded(id, &serde_json::json!({})),
                Err(e) => failed(
                    id,
                    Error::new(code::INTERNAL, format!("jawd couldn't keep that the chat was opened: {e:#}")),
                ),
            },
        },
        "link.cancel" => engine(state, id, pb::command::Kind::LinkCancel(pb::LinkCancel {})).await,
        "history.more" => history_more(state, id, request.params).await,
        method if crate::organise::METHODS.contains(&method) => match state.store() {
            Some(store) => match crate::organise::call(store, method, request.params).await {
                Ok(result) => succeeded(id, &result),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it has no folders")),
        },
        method if crate::send::Sending::METHODS.contains(&method) => match state.sending() {
            Some(sending) => match sending.call(&request.method, request.params).await {
                Ok(result) => succeeded(id, &result),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it can't send")),
        },
        method if crate::media::Media::METHODS.contains(&method) => match state.media() {
            Some(media) => match media.call(&request.method, request.params).await {
                Ok(result) => succeeded(id, &result),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it keeps no media")),
        },
        method => failed(id, Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
    }
}

/// The answer to a request on a task of its own: whatever can take long.
async fn answer_apart(state: &State, request: Request) -> Reply {
    let id = request.id;
    match request.method.as_str() {
        #[cfg(test)]
        "test.panic.apart" => panic!("the test made a long request's task panic"),
        "media.save" => match state.media() {
            Some(media) => match media.call(&request.method, request.params).await {
                Ok(result) => succeeded(id, &result),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it keeps no media")),
        },
        // A stand-in for a long request, which waits on jaw.db's writer.
        #[cfg(test)]
        "test.wait" => {
            let waited = crate::store::Change::Remember { key: "test.wait".into(), value: id.to_string() };
            match state.store().map(|store| store.apply_durable(vec![waited])) {
                Some(applying) => match applying.await {
                    Ok(_) => succeeded(id, &serde_json::json!({})),
                    Err(e) => failed(id, Error::new(code::INTERNAL, format!("{e:#}"))),
                },
                None => failed(id, Error::new(code::INTERNAL, "no store to wait on")),
            }
        }
        method => failed(id, Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
    }
}

/// `layout.get` and `layout.set`, down the lane of the connection numbered `from`, as the hello it
/// had said when it sent them named it, `interface`: they wait on jaw.db, so they're answered in
/// the order sent, as the lane's other requests are.
async fn layouts(state: &State, request: Request, interface: &str, from: u64) -> Reply {
    let id = request.id;
    let Some(layouts) = state.layouts() else {
        return failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it keeps no layouts"));
    };
    let answer = match request.method.as_str() {
        "layout.get" => layouts.get(interface, request.params).await,
        _ => layouts.set(interface, from, request.params).await,
    };
    match answer {
        Ok(saved) => succeeded(id, &saved),
        Err(error) => failed(id, error),
    }
}

/// `settings.set`, down the lane, in the domain the hello the connection had said when it sent it
/// named, `client`: it waits on jaw.db, so it's answered in the order sent, as the lane's other
/// requests are. Its answer goes to `answered` while the domain's settings are still held, so no
/// later change can be pushed to the connection before it.
async fn settings_set(state: &State, request: Request, client: &str, answered: &mpsc::UnboundedSender<Reply>) {
    let id = request.id;
    let hand = |reply| drop(answered.send(reply));
    match state.settings() {
        Some(settings) => {
            let answer = |kept| {
                hand(match kept {
                    Ok(settings) => succeeded(id, &SettingsKept { settings }),
                    Err(error) => failed(id, error),
                })
            };
            settings.set(client, request.params, answer).await;
        }
        None => hand(failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it can't keep settings"))),
    }
}

/// Asks the phone for older messages in a chat; they arrive as upserts once it answers.
async fn history_more(state: &State, id: u64, params: Value) -> Reply {
    let more = match serde_json::from_value::<HistoryMore>(params) {
        Ok(more) => more,
        Err(e) => return failed(id, Error::new(code::INVALID_PARAMS, format!("history.more's params: {e}"))),
    };
    let (Some(store), Ok(engine)) = (state.store(), state.engine()) else {
        return failed(id, Error::new(code::ENGINE_UNAVAILABLE, "there's no engine or store to ask the phone with"));
    };
    match crate::sync::history_more(store, engine, state.live(), more.chat).await {
        Ok(()) => succeeded(id, &serde_json::json!({})),
        Err(Refused::NotFound(why)) => failed(id, Error::new(code::NOT_FOUND, why)),
        Err(Refused::Engine(EngineError::Failed { message, retry: false })) => {
            failed(id, Error::new(code::NOT_ALLOWED, message))
        }
        Err(Refused::Engine(e)) => failed(id, Error::new(code::ENGINE_UNAVAILABLE, e.to_string())),
    }
}

/// Logs the account out, and once the engine has, ends it as WhatsApp logging it out would, before
/// answering: whatsmeow says nothing after a logout it was asked for. With the answer comes the
/// `connection` push saying so, for every interface once the answer is on its way.
async fn logout(state: &State, id: u64) -> (Reply, Option<Push>) {
    let reply = engine(state, id, pb::command::Kind::Logout(pb::Logout {})).await;
    let (Outcome::Result(_), Some(store)) = (&reply.outcome, state.store()) else { return (reply, None) };
    match crate::sync::logged_out(store, state.live(), "logged out from jaw").await {
        Ok(push) => (reply, Some(push)),
        Err(e) => {
            let why = format!("the account is logged out, but jaw.db couldn't forget it: {e:#}");
            (failed(id, Error::new(code::INTERNAL, why)), None)
        }
    }
}

/// Carries out a command on the engine, with an empty result once it's done.
async fn engine(state: &State, id: u64, command: pb::command::Kind) -> Reply {
    let engine = match state.engine() {
        Ok(engine) => engine,
        Err(why) => return failed(id, Error::new(code::ENGINE_UNAVAILABLE, why)),
    };
    match engine.call(command).await {
        Ok(_) => succeeded(id, &serde_json::json!({})),
        Err(EngineError::Failed { message, retry: false }) => failed(id, Error::new(code::NOT_ALLOWED, message)),
        Err(e) => failed(id, Error::new(code::ENGINE_UNAVAILABLE, e.to_string())),
    }
}

fn succeeded(id: u64, result: &impl serde::Serialize) -> Reply {
    Reply { id, outcome: Outcome::Result(serde_json::to_value(result).expect("jawd's own types serialize")) }
}

fn failed(id: u64, error: Error) -> Reply {
    Reply { id, outcome: Outcome::Error(error) }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::time::Duration;

    use jaw_proto::settings::{Messages, TERMINAL};
    use jaw_proto::{AccountState, ConnectionState, Status};
    use serde_json::json;

    use super::*;
    use crate::engine::supervisor::{Backoff, Supervisor};
    use crate::find::Finder;
    use crate::store::{Change, Chat, Lane, Message, Store};
    use crate::work::Priority;

    /// jawd serving on a socket in a folder of its own, for as long as the test runs.
    async fn serving() -> (tempfile::TempDir, std::path::PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let state = Arc::new(
            State::new(
                home.path(),
                Err(("go", "no engine in this test".into())),
                Vec::new(),
                Arc::default(),
                None,
                broadcast::channel(16).0,
            )
            .unwrap(),
        );
        tokio::spawn(serve(state, listener));
        (home, socket)
    }

    /// The raw reply to each line, for what jaw-client wouldn't send.
    fn exchange(socket: &Path, lines: &[&str]) -> Vec<Value> {
        let mut stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        lines
            .iter()
            .map(|line| {
                stream.write_all(format!("{line}\n").as_bytes()).unwrap();
                let mut reply = String::new();
                reader.read_line(&mut reply).unwrap();
                serde_json::from_str(&reply).unwrap()
            })
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hello_then_status() {
        let (home, socket) = serving().await;
        let root = home.path().display().to_string();
        tokio::task::spawn_blocking(move || {
            let (mut connection, welcome) = jaw_client::Connection::open(&socket, "test", "0.0.0").unwrap();
            assert_eq!(welcome.protocol, PROTOCOL);
            assert_eq!(welcome.root, root);
            assert_eq!(welcome.account.state, AccountState::Unlinked);
            let status: Status = serde_json::from_value(connection.call("status", Value::Null).unwrap()).unwrap();
            assert_eq!(status.connection.state, ConnectionState::Offline);
            assert!(jaw_client::describe(&status).contains("not linked yet"));
            let unknown = connection.call("chats.dance", Value::Null).unwrap_err();
            assert_eq!(unknown.to_string(), "jawd doesn't know chats.dance");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hello_says_linked_and_connecting_before_the_engine_has_said_anything() {
        let home = tempfile::tempdir().unwrap();
        let store = crate::store::Store::open(home.path()).unwrap();
        let pedro = jaw_proto::Me {
            id: "351910000001:12@s.whatsapp.net".into(),
            name: "Pedro".into(),
            phone: "+351910000001".into(),
        };
        let account =
            crate::store::Change::Remember { key: "account".into(), value: serde_json::to_string(&pedro).unwrap() };
        store.apply(crate::store::Lane::Live, vec![account]).await.unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let live = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            live,
            Some(store),
            broadcast::channel(16).0,
        );
        tokio::spawn(serve(Arc::new(state.unwrap()), listener));
        let welcome =
            tokio::task::spawn_blocking(move || jaw_client::Connection::open(&socket, "test", "0.0.0").unwrap().1)
                .await
                .unwrap();
        assert_eq!(welcome.account.state, AccountState::Linked);
        assert_eq!(welcome.account.me, Some(pedro));
        assert_eq!(welcome.connection.state, ConnectionState::Connecting);
    }

    /// An engine that does whatever it's asked, at once.
    struct Obliging;

    impl crate::engine::Engine for Obliging {
        fn kind(&self) -> &'static str {
            "obliging"
        }

        fn start(
            &self,
            _events: tokio::sync::mpsc::Sender<crate::engine::Incoming>,
        ) -> crate::engine::BoxFuture<'_, anyhow::Result<crate::engine::Instance>> {
            Box::pin(async move {
                let (commands, mut outgoing) = crate::engine::Commands::for_tests();
                tokio::spawn(async move {
                    while let Some(out) = outgoing.recv().await {
                        if let crate::engine::Outgoing::Command { reply, .. } = out {
                            let _ = reply.send(Ok(pb::reply::Kind::Done(pb::Done {})));
                        }
                    }
                });
                let hello = pb::Hello {
                    protocol: crate::engine::PROTOCOL,
                    program: "obliging".into(),
                    whatsmeow: String::new(),
                };
                Ok(crate::engine::Instance { hello, commands, exited: Box::pin(std::future::pending()) })
            })
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_logout_jawd_asks_for_is_answered_only_once_the_account_is_forgotten_with_a_synced_commit() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let pedro = jaw_proto::Me {
            id: "351910000001:12@s.whatsapp.net".into(),
            name: "Pedro".into(),
            phone: "+351910000001".into(),
        };
        let account = Change::Remember { key: "account".into(), value: serde_json::to_string(&pedro).unwrap() };
        store.apply(Lane::Live, vec![account]).await.unwrap();
        let (events, _incoming) = tokio::sync::mpsc::channel(16);
        let supervisor = Supervisor::spawn(Arc::new(Obliging), events, Backoff::default());
        while supervisor.status().state.name() != "running" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let live = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let state =
            State::new(home.path(), Ok(supervisor), Vec::new(), live, Some(store.clone()), broadcast::channel(16).0);
        tokio::spawn(serve(Arc::new(state.unwrap()), listener));
        let hello = |socket: std::path::PathBuf| {
            tokio::task::spawn_blocking(move || jaw_client::Connection::open(&socket, "test", "0.0.0").unwrap())
        };
        let (mut connection, welcome) = hello(socket.clone()).await.unwrap();
        assert_eq!(welcome.account.state, AccountState::Linked);
        // jaw.db's write lock, held from outside, so the account can't be forgotten until it's let go.
        let holder = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let synced = store.storage().synced_commits;
        // On a thread of its own, so an answer or a push that never comes fails the test rather than
        // hanging it.
        let (answered, mut answer) = tokio::sync::oneshot::channel();
        let (told, tell) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = answered.send(connection.call("link.logout", Value::Null).map_err(|e| e.to_string()));
            let _ = told.send(connection.push().unwrap());
        });
        let early = tokio::time::timeout(Duration::from_millis(400), &mut answer).await;
        assert!(early.is_err(), "answered while jaw.db couldn't forget the account: {early:?}");
        holder.execute_batch("COMMIT").unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(5), answer).await.expect("answered once it could");
        assert_eq!(answer.unwrap(), Ok(json!({})));
        assert_eq!(store.storage().synced_commits - synced, 1, "the account forgotten with a synced commit");
        let told = tokio::time::timeout(Duration::from_secs(5), tell).await.expect("interfaces were told").unwrap();
        assert_eq!(told.method, "connection");
        let told: jaw_proto::Connection = serde_json::from_value(told.params).unwrap();
        assert_eq!((told.state, told.reason.as_deref()), (ConnectionState::LoggedOut, Some("logged out from jaw")));
        assert_eq!(store.remembered("account").unwrap().as_deref(), Some(""), "jaw.db forgot the account");
        assert_eq!(crate::sync::starting(&store, true).account.state, AccountState::Unlinked, "so the next start says");
        let (_, welcome) = hello(socket).await.unwrap();
        assert_eq!(welcome.account, jaw_proto::Account { state: AccountState::Unlinked, me: None }, "and a hello now");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_connection_push_after_a_logout_comes_after_its_answer_on_the_raw_socket() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let pedro = jaw_proto::Me {
            id: "351910000001:12@s.whatsapp.net".into(),
            name: "Pedro".into(),
            phone: "+351910000001".into(),
        };
        let account = Change::Remember { key: "account".into(), value: serde_json::to_string(&pedro).unwrap() };
        store.apply(Lane::Live, vec![account]).await.unwrap();
        let (events, _incoming) = tokio::sync::mpsc::channel(16);
        let supervisor = Supervisor::spawn(Arc::new(Obliging), events, Backoff::default());
        while supervisor.status().state.name() != "running" {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let live = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let state =
            State::new(home.path(), Ok(supervisor), Vec::new(), live, Some(store.clone()), broadcast::channel(16).0);
        tokio::spawn(serve(Arc::new(state.unwrap()), listener));
        // jaw-client keeps the pushes that come while a call waits, so only the raw socket shows
        // their order.
        let early = tokio::task::spawn_blocking(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut read = move || {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                serde_json::from_str::<Value>(&line).unwrap()
            };
            stream.write_all(format!("{{\"id\":1,\"method\":\"hello\",\"params\":{{\"protocol\":{PROTOCOL},\"client\":\"test\",\"version\":\"0\"}}}}\n").as_bytes()).unwrap();
            read();
            let mut early = 0;
            for id in 10..210u64 {
                stream.write_all(format!("{{\"id\":{id},\"method\":\"link.logout\"}}\n").as_bytes()).unwrap();
                let (mut answered, mut pushed, mut n) = (None, None, 0);
                while answered.is_none() || pushed.is_none() {
                    let value = read();
                    if value["id"] == id {
                        assert!(value.get("result").is_some(), "{value}");
                        answered = Some(n);
                    } else if value["method"] == "connection" {
                        pushed = Some(n);
                    }
                    n += 1;
                }
                if pushed < answered {
                    early += 1;
                }
            }
            early
        })
        .await
        .unwrap();
        assert_eq!(early, 0, "of 200 logouts, the connection push came before the answer {early} times");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hello_comes_first_and_must_match() {
        let (_home, socket) = serving().await;
        tokio::task::spawn_blocking(move || {
            let newer = format!(
                r#"{{"id":2,"method":"hello","params":{{"protocol":{},"client":"test","version":"9"}}}}"#,
                PROTOCOL + 1
            );
            let replies =
                exchange(&socket, &[r#"{"id":1,"method":"status"}"#, &newer, "not json", r#"{"method":"hello"}"#]);
            assert_eq!(replies[0]["error"]["code"], code::VERSION);
            assert_eq!(replies[1]["error"]["code"], code::VERSION);
            assert_eq!(replies[1]["error"]["data"]["client"], PROTOCOL + 1);
            assert!(replies[1]["error"]["message"].as_str().unwrap().contains("jawd is older"));
            assert_eq!(replies[2]["error"]["code"], code::PARSE);
            assert_eq!(replies[3]["error"]["code"], code::INVALID_REQUEST);
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_newer_find_cancels_the_last_and_each_is_answered() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let store = Store::open(home.path()).unwrap();
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store.clone()),
            broadcast::channel(16).0,
        )
        .unwrap();
        let _ = state.finder.set(Finder::start(store.clone()).await.unwrap());
        tokio::spawn(serve(Arc::new(state), listener));
        let chat = "120363000000000001@g.us";
        let message = Message {
            chat: chat.into(),
            id: "m1".into(),
            at: 1_000,
            kind: "text",
            text: Some("jantar amanhã".into()),
            summary: "jantar amanhã".into(),
            ..Message::default()
        };
        store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
        store.indexed().await.unwrap();
        // Every reader waits at a gate, so the first find is still waiting when the second comes.
        let (gate, waiting) = crossbeam_channel::bounded::<()>(0);
        for _ in 0..4 {
            let waiting = waiting.clone();
            let reads = store.reads().clone();
            tokio::spawn(async move {
                reads
                    .query(Priority::Interactive, move |_| {
                        let _ = waiting.recv();
                        Ok(())
                    })
                    .await
            });
        }
        let replies = tokio::task::spawn_blocking(move || {
            let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut reply = || {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                serde_json::from_str::<Value>(&line).unwrap()
            };
            stream.write_all(format!("{{\"id\":1,\"method\":\"hello\",\"params\":{{\"protocol\":{PROTOCOL},\"client\":\"test\",\"version\":\"0\"}}}}\n").as_bytes()).unwrap();
            reply();
            stream.write_all(b"{\"id\":2,\"method\":\"find\",\"params\":{\"query\":\"jant\"}}\n").unwrap();
            stream.write_all(b"{\"id\":3,\"method\":\"find\",\"params\":{\"query\":\"jantar\"}}\n").unwrap();
            std::thread::sleep(Duration::from_millis(200));
            drop(gate);
            let mut replies = vec![reply(), reply()];
            for line in [
                r#"{"id":4,"method":"find","params":{"query":7}}"#,
                &format!(r#"{{"id":5,"method":"chat.opened","params":{{"chat":"{chat}"}}}}"#),
            ] {
                stream.write_all(format!("{line}\n").as_bytes()).unwrap();
                replies.push(reply());
            }
            replies.sort_by_key(|reply| reply["id"].as_u64());
            replies
        })
        .await
        .unwrap();
        assert_eq!(replies[0]["error"]["code"], code::CANCELLED, "{}", replies[0]);
        assert_eq!(replies[0]["error"]["message"], "a newer find replaced this one");
        let results = replies[1]["result"]["results"].as_array().unwrap();
        assert_eq!(
            (results.len(), &results[0]["kind"], &results[0]["row"]["id"]),
            (1, &json!("message"), &json!("m1"))
        );
        assert_eq!(replies[2]["error"]["code"], code::INVALID_PARAMS);
        assert_eq!(replies[3]["result"], json!({}));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_folder_methods_answer_with_the_folder_made_and_refuse_as_the_protocol_says() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let store = Store::open(home.path()).unwrap();
        let message = Message {
            chat: "351910000002@s.whatsapp.net".into(),
            id: "m1".into(),
            at: 1_000,
            sender: Some("351910000002@s.whatsapp.net".into()),
            kind: "text",
            summary: "olá".into(),
            ..Message::default()
        };
        store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store),
            broadcast::channel(16).0,
        )
        .unwrap();
        tokio::spawn(serve(Arc::new(state), listener));
        let replies = tokio::task::spawn_blocking(move || {
            let hello =
                format!(r#"{{"id":1,"method":"hello","params":{{"protocol":{PROTOCOL},"client":"test","version":"0"}}}}"#);
            exchange(
                &socket,
                &[
                    &hello,
                    r#"{"id":2,"method":"folder.create","params":{"name":"  Family "}}"#,
                    r#"{"id":3,"method":"folder.create","params":{"name":"   "}}"#,
                    r#"{"id":4,"method":"folder.rename","params":{"folder":"all","name":"Everything"}}"#,
                    r#"{"id":5,"method":"folder.add","params":{"folder":"folder:999","item":"351910000002@s.whatsapp.net"}}"#,
                    r#"{"id":6,"method":"folder.smart","params":{"name":"Odd","rule":{"sunny":true}}}"#,
                    r#"{"id":7,"method":"favourite.set","params":{"slot":10,"target":"all"}}"#,
                    r#"{"id":8,"method":"favourite.set","params":{"slot":2,"target":"unread"}}"#,
                    r#"{"id":9,"method":"folder.add","params":{"folder":"unread","item":"351910000002@s.whatsapp.net"}}"#,
                    r#"{"id":10,"method":"folder.create","params":{"name":"family"}}"#,
                    r#"{"id":11,"method":"folder.add","params":{"folder":"folder:9","item":"person:12345"}}"#,
                ],
            )
        })
        .await
        .unwrap();
        let folder = &replies[1]["result"]["folder"];
        assert_eq!(
            (folder["name"].as_str(), &folder["parent"], folder["count"].as_u64()),
            (Some("Family"), &json!(null), Some(0))
        );
        assert!(folder["id"].as_str().unwrap().starts_with("folder:"), "{folder}");
        assert_eq!(replies[2]["error"]["code"], code::INVALID_PARAMS, "a name of spaces");
        assert_eq!(replies[2]["error"]["data"]["param"], "name");
        assert_eq!(replies[3]["error"]["code"], code::NOT_ALLOWED, "renaming a built-in folder");
        assert_eq!(replies[4]["error"]["code"], code::NOT_FOUND, "a folder jawd doesn't have");
        assert_eq!(replies[5]["error"]["code"], code::INVALID_PARAMS, "a rule's field jawd doesn't know");
        assert_eq!(replies[6]["error"]["code"], code::INVALID_PARAMS, "a slot past nine");
        assert_eq!(replies[7]["result"], json!({}), "a built-in folder can be a favourite");
        assert_eq!(replies[8]["error"]["code"], code::NOT_ALLOWED, "filing into a built-in folder");
        assert_eq!(replies[9]["error"]["code"], code::NOT_ALLOWED, "a name a sibling has, whatever its case");
        assert_eq!(replies[10]["error"]["code"], code::NOT_FOUND, "a person jawd doesn't have");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn focus_and_the_chats_a_connection_reads_last_as_long_as_it_does() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let store = Store::open(home.path()).unwrap();
        let feed = feed::Feed::start(store.clone(), Arc::new(|| false), feed::Timing::default());
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store),
            broadcast::channel(16).0,
        )
        .unwrap()
        .with_feed(feed);
        let state = Arc::new(state);
        tokio::spawn(serve(state.clone(), listener));
        let ana = "351912345678@s.whatsapp.net";
        let looking = {
            let state = state.clone();
            move || (state.attention().connected(), state.focused(), state.attention().reading(ana))
        };
        let seen = tokio::task::spawn_blocking(move || {
            let stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut seen = Vec::new();
            let mut lines = vec![
                format!(r#"{{"id":1,"method":"hello","params":{{"protocol":{PROTOCOL},"client":"test","version":"0","focus":true}}}}"#),
                format!(
                    r#"{{"id":2,"method":"messages.watch","params":{{"chat":"{ana}","around":"newest","before":10}}}}"#
                ),
                r#"{"id":3,"method":"focus","params":{"focused":false}}"#.to_string(),
                r#"{"id":4,"method":"focus","params":{"focused":true}}"#.to_string(),
            ];
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            let mut watch = String::new();
            for line in lines.drain(..) {
                writer.write_all(format!("{line}\n").as_bytes()).unwrap();
                let mut reply = String::new();
                reader.read_line(&mut reply).unwrap();
                let reply: Value = serde_json::from_str(&reply).unwrap();
                assert!(reply.get("error").is_none(), "{reply}");
                if let Some(w) = reply["result"]["watch"].as_str() {
                    watch = w.to_string();
                }
                seen.push(looking());
            }
            let unwatch = format!(r#"{{"id":5,"method":"unwatch","params":{{"watch":"{watch}"}}}}"#);
            writer.write_all(format!("{unwatch}\n").as_bytes()).unwrap();
            let mut reply = String::new();
            reader.read_line(&mut reply).unwrap();
            seen.push(looking());
            seen
        })
        .await
        .unwrap();
        assert_eq!(seen, vec![(1, true, false), (1, true, true), (1, false, false), (1, true, true), (1, true, false)]);
        for _ in 0..100 {
            if state.attention().connected() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(state.attention().connected(), 0, "the connection that ended is forgotten");
    }

    /// jawd serving jaw.db with its settings, on a socket in a folder of its own, once jaw.db keeps
    /// `kept`, as another jawd may have left it.
    async fn serving_settings(
        kept: Vec<Change>,
    ) -> (tempfile::TempDir, std::path::PathBuf, Store, Arc<crate::settings::Settings>) {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        if !kept.is_empty() {
            assert_eq!(store.apply_durable(kept).await.unwrap().quarantined, 0);
        }
        let settings = Arc::new(crate::settings::Settings::load(store.clone()).unwrap());
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store.clone()),
            broadcast::channel(16).0,
        )
        .unwrap()
        .with_settings(settings.clone());
        tokio::spawn(serve(Arc::new(state), listener));
        (home, socket, store, settings)
    }

    /// The answer to a hello from `client` as jawd wrote it, since `Terminal` reads an empty or
    /// unknown value as the default.
    fn raw_hello(socket: &Path, client: &str) -> Value {
        let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        let mut heard = BufReader::new(stream.try_clone().unwrap());
        let hello =
            json!({"id": 1, "method": "hello", "params": {"protocol": PROTOCOL, "client": client, "version": "0"}});
        (&stream).write_all(format!("{hello}\n").as_bytes()).unwrap();
        let mut line = String::new();
        heard.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hello_carries_a_domains_settings_and_each_change_is_pushed_to_its_connections_alone() {
        let (_home, socket, _store, _) = serving_settings(Vec::new()).await;
        let checked = tokio::task::spawn_blocking(move || {
            let fresh = raw_hello(&socket, TERMINAL);
            assert_eq!(fresh["result"]["settings"], json!({"theme": "tokyo-night", "messages": "bubbles"}));
            let (mut a, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            let (mut b, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            // A client of no domain, read line by line, so a push to it would show.
            let other = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut heard = BufReader::new(other.try_clone().unwrap());
            let mut line = String::new();
            let mut say = |request: &str, line: &mut String| {
                (&other).write_all(format!("{request}\n").as_bytes()).unwrap();
                line.clear();
                heard.read_line(line).map(|_| serde_json::from_str::<Value>(line).unwrap())
            };
            let hello = format!(
                r#"{{"id":1,"method":"hello","params":{{"protocol":{PROTOCOL},"client":"test","version":"0"}}}}"#
            );
            let welcome = say(&hello, &mut line).unwrap();
            assert!(welcome["result"].get("settings").is_none(), "a client jawd keeps no settings for gets none");
            let kept = a.call("settings.set", json!({"theme": "gruvbox", "messages": "irc"})).unwrap();
            assert_eq!(kept, json!({"settings": {"theme": "gruvbox", "messages": "irc"}}));
            let gruvbox = Terminal { theme: "gruvbox".into(), messages: Messages::Irc };
            for connection in [&mut a, &mut b] {
                let push = connection.push().unwrap();
                assert_eq!(push.method, "settings");
                assert_eq!(serde_json::from_value::<Terminal>(push.params).unwrap(), gruvbox);
            }
            let asked = r#"{"id":2,"method":"settings.set","params":{"theme":"nord"}}"#;
            let answer = say(asked, &mut line).unwrap();
            assert_eq!(answer["id"], 2, "the next line it hears is its answer, not a push: {answer}");
            assert_eq!(answer["error"]["code"], code::NOT_FOUND);
            assert_eq!(answer["error"]["data"], json!({"client": "test"}));
            let (_, later) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            assert_eq!(later.settings, Some(gruvbox), "a later hello carries them");
            let refused = a.call("settings.set", json!({"theme": "solarized"})).unwrap_err();
            assert!(refused.to_string().contains("theme takes one of"), "{refused}");
        });
        tokio::time::timeout(Duration::from_secs(20), checked).await.expect("no push or answer went missing").unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_set_answers_only_once_its_commit_is_synced() {
        let (home, socket, store, _) = serving_settings(Vec::new()).await;
        let path = socket.clone();
        let (mut connection, _) =
            tokio::task::spawn_blocking(move || jaw_client::Connection::open(&path, TERMINAL, "0").unwrap())
                .await
                .unwrap();
        // jaw.db's write lock, held from outside, so the setting can't be kept until it's let go.
        let holder = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let synced = store.storage().synced_commits;
        let (answered, mut answer) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = answered.send(connection.call("settings.set", json!({"theme": "nord"})).map_err(|e| e.to_string()));
        });
        let early = tokio::time::timeout(Duration::from_millis(400), &mut answer).await;
        assert!(early.is_err(), "answered while jaw.db couldn't keep it: {early:?}");
        holder.execute_batch("COMMIT").unwrap();
        let answer =
            tokio::time::timeout(Duration::from_secs(5), answer).await.expect("answered once it could").unwrap();
        assert_eq!(answer.unwrap()["settings"]["theme"], "nord");
        assert_eq!(store.storage().synced_commits - synced, 1, "kept with one synced commit");
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"nord\""));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_connections_changing_settings_at_once_hear_each_answer_before_the_later_change_and_end_on_it() {
        let (_home, socket, store, settings) = serving_settings(Vec::new()).await;
        // The first change waits once it's published, before it's answered, until it's let go.
        let (open, gate) = tokio::sync::oneshot::channel();
        settings.gate_next(gate);
        tokio::task::spawn_blocking(move || {
            let mut first = Lines::open(&socket, TERMINAL);
            let mut second = Lines::open(&socket, TERMINAL);
            let nord = first.send("settings.set", json!({"theme": "nord"}));
            let pushed = first.within(Duration::from_secs(10)).expect("nord is published");
            assert_eq!(pushed["params"]["theme"], "nord", "its own push can come before its answer: {pushed}");
            // Asked while the first change holds the settings, unanswered, so it can be kept and
            // pushed only once the first is answered.
            let gruvbox = second.send("settings.set", json!({"theme": "gruvbox"}));
            std::thread::sleep(Duration::from_millis(300));
            open.send(()).unwrap();
            // Each connection's lines until its answer and gruvbox's push have come, and for 300 ms
            // after.
            let heard = |lines: &mut Lines, asked: u64| {
                let mut heard: Vec<Value> = Vec::new();
                while !heard.iter().any(|line| line["id"] == asked)
                    || !heard.iter().any(|line| line["params"]["theme"] == "gruvbox")
                {
                    heard.push(lines.within(Duration::from_secs(10)).expect("its answer and gruvbox's push"));
                }
                while let Some(line) = lines.within(Duration::from_millis(300)) {
                    heard.push(line);
                }
                heard
            };
            let (first, second) = (heard(&mut first, nord), heard(&mut second, gruvbox));
            let answered = first.iter().position(|line| line["id"] == nord).unwrap();
            let later = first.iter().position(|line| line["params"]["theme"] == "gruvbox").unwrap();
            assert!(answered < later, "the later change was pushed before the first was answered: {first:?}");
            assert_eq!(first[answered]["result"]["settings"]["theme"], "nord", "answered as it left them");
            let answer = second.iter().find(|line| line["id"] == gruvbox).unwrap();
            assert_eq!(answer["result"]["settings"]["theme"], "gruvbox");
            for heard in [&first, &second] {
                let last = heard.iter().rev().find(|line| line["method"] == "settings").unwrap();
                assert_eq!(last["params"], json!({"theme": "gruvbox", "messages": "bubbles"}), "{heard:?}");
            }
        })
        .await
        .unwrap();
        assert_eq!(store.setting(TERMINAL, "theme").unwrap().as_deref(), Some("\"gruvbox\""), "jaw.db agrees");
        assert_eq!(settings.current().theme, "gruvbox");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_full_when_its_change_is_answered_hears_the_answer_before_the_later_changes_push() {
        // Each round the asking connection's change is held once it's published, while answers it
        // leaves unread fill the queue to it until one waits, so its loop takes nothing more; then
        // the change is answered and another connection's is published, and once it reads, its
        // answer and the later push are ready at once. The bias towards answers sends the answer
        // first, and so does the drain before a push; without both, the select takes either, so in
        // twelve rounds it all but surely sends the push first in one.
        const ROUNDS: usize = 12;
        let (_home, socket, store, settings) = serving_settings(Vec::new()).await;
        let gates = settings.clone();
        let checked = tokio::task::spawn_blocking(move || {
            let themes = jaw_proto::themes::all();
            let theme = |settings: &Value| settings["theme"].as_str().unwrap_or_default().to_string();
            let mut asker = Lines::open(&socket, TERMINAL);
            let mut other = Lines::open(&socket, TERMINAL);
            let mut last = "";
            for round in 0..ROUNDS {
                let first = themes[2 * round % themes.len()].id.as_str();
                let later = themes[(2 * round + 1) % themes.len()].id.as_str();
                let (open, gate) = tokio::sync::oneshot::channel();
                gates.gate_next(gate);
                let asked = asker.send("settings.set", json!({ "theme": first }));
                loop {
                    let line = asker.within(Duration::from_secs(10)).expect("its change is published");
                    assert_ne!(line["id"], asked, "round {round}: answered while it was held: {line}");
                    if line["method"] == "settings" {
                        assert_eq!(line["params"]["theme"], first, "round {round}: {line}");
                        break;
                    }
                }
                let mut unread = 0;
                while other.waiting() == 0 {
                    assert!(unread < 400, "round {round}: no answer waited for room");
                    for _ in 0..8 {
                        asker.send("themes", Value::Null);
                    }
                    unread += 8;
                }
                let kept = other.send("settings.set", json!({ "theme": later }));
                open.send(()).unwrap();
                loop {
                    let line = other.within(Duration::from_secs(10)).expect("the later change is kept");
                    if line["id"] == kept {
                        assert_eq!(line["result"]["settings"]["theme"], later, "round {round}: {line}");
                        break;
                    }
                }
                // What it hears once it reads, the answers to themes left out.
                let mut heard: Vec<String> = Vec::new();
                while !heard.iter().any(|line| line.starts_with("answered"))
                    || !heard.contains(&format!("pushed {later}"))
                {
                    let line = asker.within(Duration::from_secs(10)).expect("its answer and the later change's push");
                    match (line["id"].as_u64(), line["method"].as_str()) {
                        (Some(id), _) if id == asked => {
                            heard.push(format!("answered {}", theme(&line["result"]["settings"])))
                        }
                        (None, Some("settings")) => heard.push(format!("pushed {}", theme(&line["params"]))),
                        _ => {}
                    }
                }
                assert_eq!(
                    heard,
                    [format!("answered {first}"), format!("pushed {later}")],
                    "round {round}: the answer, as the change left them, then the later change's push"
                );
                last = later;
            }
            last
        });
        let last =
            tokio::time::timeout(Duration::from_secs(120), checked).await.expect("nothing went missing").unwrap();
        assert_eq!(store.setting(TERMINAL, "theme").unwrap(), Some(format!("\"{last}\"")), "jaw.db agrees");
        assert_eq!(settings.current().theme, last);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_that_reads_nothing_while_settings_change_ends_on_the_latest() {
        // The themes asked for and their answers left unread: fewer than its queue holds, so it's
        // left in its loop rather than waiting on an answer, and more than a socket takes, so the
        // rest wait in the queue; then more changes than the queue holds, whose pushes fill it.
        let (asks, changes) = (WAITING_LINES * 5 / 8, 2 * WAITING_LINES);
        let answer = encode(&succeeded(0, &Themes { themes: jaw_proto::themes::all().to_vec() })).len();
        assert!(asks * answer > 256 << 10, "{asks} answers of {answer} bytes might all fit in the socket");
        let (_home, socket, _store, _) = serving_settings(Vec::new()).await;
        let seen = tokio::task::spawn_blocking(move || {
            let stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            let mut writer = stream.try_clone().unwrap();
            writer
                .write_all(
                    format!("{{\"id\":1,\"method\":\"hello\",\"params\":{{\"protocol\":{PROTOCOL},\"client\":\"jaw\",\"version\":\"0\"}}}}\n")
                        .as_bytes(),
                )
                .unwrap();
            for id in 2..asks + 2 {
                writer.write_all(format!("{{\"id\":{id},\"method\":\"themes\"}}\n").as_bytes()).unwrap();
            }
            let (mut fast, _) = jaw_client::Connection::open(&socket, TERMINAL, "0").unwrap();
            for i in 0..changes {
                let theme = ["gruvbox", "nord"][i % 2];
                fast.call("settings.set", json!({"theme": theme})).unwrap();
            }
            fast.call("settings.set", json!({"theme": "kanagawa"})).unwrap();
            stream.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
            let mut reader = BufReader::new(stream);
            let (mut last, mut line) = (None, String::new());
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                let value: Value = serde_json::from_str(&line).unwrap();
                if value["method"] == "settings" {
                    last = value["params"]["theme"].as_str().map(str::to_string);
                }
                line.clear();
            }
            last
        });
        let seen = tokio::time::timeout(Duration::from_secs(60), seen).await.unwrap().unwrap();
        assert_eq!(seen.as_deref(), Some("kanagawa"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn status_says_every_domains_settings_and_each_value_this_jawd_doesnt_take() {
        // A theme a newer jawd took, kept in jaw.db before this one starts.
        let newer = Change::Settings {
            domain: TERMINAL.into(),
            values: vec![("theme".into(), "\"rose-pine-moon\"".into())],
            at: 1,
        };
        let (_home, socket, _store, _) = serving_settings(vec![newer]).await;
        let status = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&socket, "test", "0").unwrap();
            connection.call("status", Value::Null).unwrap()
        })
        .await
        .unwrap();
        assert_eq!(status["settings"], json!({"jaw": {"messages": "bubbles", "theme": "tokyo-night"}}));
        assert_eq!(status["settings_ignored"], json!([{"domain": "jaw", "key": "theme", "value": "rose-pine-moon"}]));
        let said = jaw_client::describe(&serde_json::from_value::<Status>(status).unwrap());
        assert!(
            said.contains(
                "settings  jaw: messages bubbles, theme tokyo-night; theme \"rose-pine-moon\" isn't one this jawd takes, so its default is used"
            ),
            "{said}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn themes_lists_every_theme_in_order_with_its_colours() {
        let (_home, socket) = serving().await;
        let themes = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&socket, "test", "0").unwrap();
            connection.call("themes", Value::Null).unwrap()
        })
        .await
        .unwrap();
        let themes: jaw_proto::themes::Themes = serde_json::from_value(themes).unwrap();
        assert_eq!(themes.themes.as_slice(), jaw_proto::themes::all());
        assert_eq!(themes.themes[20].id, "tokyo-night");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stopping_ends_every_connection_and_takes_no_new_ones() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let state = Arc::new(
            State::new(
                home.path(),
                Err(("go", "no engine in this test".into())),
                Vec::new(),
                Arc::default(),
                None,
                broadcast::channel(16).0,
            )
            .unwrap(),
        );
        let served = tokio::spawn(serve(state.clone(), listener));
        let path = socket.clone();
        let (connection, _) =
            tokio::task::spawn_blocking(move || jaw_client::Connection::open(&path, "test", "0").unwrap())
                .await
                .unwrap();
        state.stop();
        tokio::time::timeout(Duration::from_secs(1), served).await.unwrap().unwrap().unwrap();
        tokio::task::spawn_blocking(move || {
            let mut connection = connection;
            assert!(connection.call("status", Value::Null).is_err(), "the connection was ended");
            assert!(std::os::unix::net::UnixStream::connect(&socket).is_err(), "nothing listens any more");
        })
        .await
        .unwrap();
    }

    const ANA: &str = "351912345678@s.whatsapp.net";
    const CLIMBING: &str = "120363012345678901@g.us";

    /// jawd serving layouts, with Ana's chat and the climbing group, on a socket of its own.
    async fn serving_layouts() -> (tempfile::TempDir, std::path::PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let listener = listen(&socket).unwrap();
        let store = Store::open(home.path()).unwrap();
        let chats = [ANA, CLIMBING].map(|id| Change::Chat(Chat { id: id.into(), ..Chat::default() }));
        store.apply(Lane::Live, chats.to_vec()).await.unwrap();
        let state = State::new(
            home.path(),
            Err(("go", "no engine in this test".into())),
            Vec::new(),
            Arc::default(),
            Some(store.clone()),
            broadcast::channel(16).0,
        )
        .unwrap()
        .with_layouts(crate::layout::Layouts::new(store));
        tokio::spawn(serve(Arc::new(state), listener));
        (home, socket)
    }

    /// A connection read a line at a time, keeping the pushes that come before each answer.
    struct Lines {
        reader: BufReader<std::os::unix::net::UnixStream>,
        writer: std::os::unix::net::UnixStream,
        next: u64,
        pushes: Vec<Value>,
    }

    impl Lines {
        fn open(socket: &Path, client: &str) -> Lines {
            let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
            let reader = BufReader::new(stream.try_clone().unwrap());
            let mut lines = Lines { reader, writer: stream, next: 1, pushes: Vec::new() };
            let hello = lines.call("hello", json!({ "protocol": PROTOCOL, "client": client, "version": "0" }));
            assert!(hello.get("result").is_some(), "{hello}");
            lines
        }

        fn call(&mut self, method: &str, params: Value) -> Value {
            let id = self.send(method, params);
            loop {
                let line = self.within(Duration::from_secs(10)).expect("an answer");
                if line["id"] == id {
                    return line;
                }
                self.pushes.push(line);
            }
        }

        /// Sends a request without waiting for its answer, and says its id.
        fn send(&mut self, method: &str, params: Value) -> u64 {
            let id = self.next;
            self.next += 1;
            writeln!(self.writer, "{}", json!({ "id": id, "method": method, "params": params })).unwrap();
            id
        }

        /// The connections whose reply, `layout` push or `settings` push waits for room, as `status`
        /// says.
        fn waiting(&mut self) -> u64 {
            let status = self.call("status", Value::Null);
            status["result"]["interfaces"]["replies_waiting"].as_u64().unwrap_or_else(|| panic!("{status}"))
        }

        /// What comes until the answers to `asked` and a push of version `latest` have, and for
        /// 300 ms after: the answers' ids and versions, and the pushes' versions, in the order they
        /// came.
        fn until(&mut self, asked: &[u64], latest: u64) -> (Vec<(u64, u64)>, Vec<u64>) {
            let (mut answers, mut pushes) = (Vec::<(u64, u64)>::new(), Vec::new());
            let mut wait = Duration::from_secs(10);
            while let Some(line) = self.within(wait) {
                match line["id"].as_u64() {
                    Some(id) => answers.push((id, line["result"]["v"].as_u64().unwrap_or_else(|| panic!("{line}")))),
                    None => pushes.push(line["params"]["v"].as_u64().unwrap_or_else(|| panic!("{line}"))),
                }
                let answered = asked.iter().all(|id| answers.iter().any(|(answered, _)| answered == id));
                if answered && pushes.contains(&latest) {
                    wait = Duration::from_millis(300);
                }
            }
            (answers, pushes)
        }

        /// The next line, if one comes within `wait`.
        fn within(&mut self, wait: Duration) -> Option<Value> {
            self.reader.get_ref().set_read_timeout(Some(wait)).unwrap();
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(n) if n > 0 => Some(serde_json::from_str(&line).unwrap()),
                _ => None,
            }
        }
    }

    fn halves(v: u64) -> Value {
        let first = if v.is_multiple_of(2) { 400 } else { 600 };
        json!({ "format": 1, "focused": 1, "tree": { "split": {
            "how": "across", "first": first, "a": { "chat": ANA }, "b": { "chat": CLIMBING } } } })
    }

    /// Ana's chat beside a chat jawd doesn't have, whose JID takes 8 KiB: every answer and push of
    /// it names that JID in `dropped`, so a few dozen fill the queue to a connection and its socket.
    fn wide() -> Value {
        let gone = format!("{}@s.whatsapp.net", "9".repeat(8_192));
        json!({ "format": 1, "focused": 0, "tree": { "split": {
            "how": "across", "first": 500, "a": { "chat": ANA }, "b": { "chat": gone } } } })
    }

    /// A line read within `wait`: the line, the end of the connection, or nothing in time.
    enum Heard {
        Line(Value),
        Closed,
        Nothing,
    }

    fn heard(lines: &mut Lines, wait: Duration) -> Heard {
        // macOS refuses a timeout once both sides are shut; the one set before stays.
        let _ = lines.reader.get_ref().set_read_timeout(Some(wait));
        let mut line = String::new();
        match lines.reader.read_line(&mut line) {
            Ok(0) => Heard::Closed,
            Ok(_) => Heard::Line(serde_json::from_str(&line).unwrap()),
            Err(_) => Heard::Nothing,
        }
    }

    /// Everything the connection hears until jawd closes it, or nothing comes for `wait`: the
    /// answers' ids, the pushes' versions, and whether it was closed.
    fn to_the_end(lines: &mut Lines, wait: Duration) -> (Vec<u64>, Vec<u64>, bool) {
        let (mut answers, mut pushes) = (Vec::new(), Vec::new());
        loop {
            match heard(lines, wait) {
                Heard::Line(line) => match line["id"].as_u64() {
                    Some(id) => answers.push(id),
                    None => pushes.push(line["params"]["v"].as_u64().unwrap_or_else(|| panic!("{line}"))),
                },
                Heard::Closed => return (answers, pushes, true),
                Heard::Nothing => return (answers, pushes, false),
            }
        }
    }

    /// Keeps wide layouts from `setter` until a push to another of its interface's connections
    /// waits for room, then `more` after it; says the last version kept.
    fn fill_then(setter: &mut Lines, more: u64) -> u64 {
        let mut v = 0;
        while setter.waiting() == 0 {
            v += 1;
            assert!(v <= 400, "no push waited for room");
            let answer = setter.call("layout.set", json!({ "v": v, "layout": wide() }));
            assert_eq!(answer["result"]["kept"], true, "{answer}");
        }
        for _ in 0..more {
            v += 1;
            let answer = setter.call("layout.set", json!({ "v": v, "layout": wide() }));
            assert_eq!(answer["result"]["kept"], true, "{answer}");
        }
        v
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_kept_is_pushed_to_its_interfaces_other_connections_and_another_interface_can_read_it() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut setter = Lines::open(&socket, "jaw");
            let mut other = Lines::open(&socket, "jaw");
            let mut mac = Lines::open(&socket, "jaw-mac");
            let answer = setter.call("layout.set", json!({ "v": 5, "layout": halves(5) }));
            assert_eq!(answer["result"]["kept"], true, "{answer}");
            let pushed = other.within(Duration::from_secs(10)).expect("the terminal's other connection is told");
            assert_eq!(pushed["method"], "layout", "{pushed}");
            assert_eq!(pushed["params"], json!({ "interface": "jaw", "v": 5, "layout": halves(5) }));
            std::thread::sleep(Duration::from_millis(200));
            setter.call("status", Value::Null);
            mac.call("status", Value::Null);
            assert_eq!(setter.pushes, Vec::<Value>::new(), "the connection that set it isn't told");
            assert_eq!(mac.pushes, Vec::<Value>::new(), "nor is another interface");
            let read = mac.call("layout.get", json!({ "interface": "jaw" }));
            assert_eq!(
                read["result"],
                json!({ "interface": "jaw", "v": 5, "layout": halves(5) }),
                "but it can open it"
            );
            let own = mac.call("layout.get", Value::Null);
            assert_eq!(own["result"], json!({ "interface": "jaw-mac", "v": 0 }));
            let stale = other.call("layout.set", json!({ "v": 4, "layout": halves(4) }));
            assert_eq!(stale["result"]["kept"], false, "{stale}");
            assert_eq!(stale["result"]["layout"], halves(5));
            assert_eq!(setter.within(Duration::from_millis(300)), None, "and a stale one is pushed to nobody");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_slow_to_read_ends_on_the_latest_layout() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut slow = Lines::open(&socket, "jaw");
            let mut setter = Lines::open(&socket, "jaw");
            // Far more pushes than the connection's lines waiting and its socket hold.
            const SETS: u64 = 150;
            for v in 1..=SETS {
                let answer = setter.call("layout.set", json!({ "v": v, "layout": halves(v) }));
                assert_eq!(answer["result"]["kept"], true, "{answer}");
            }
            let mut last = None;
            while let Some(push) = slow.within(Duration::from_millis(500)) {
                last = Some(push["params"]["v"].as_u64().unwrap());
            }
            assert_eq!(last, Some(SETS), "a push may be skipped, but never the last");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn under_the_lane_a_slow_connection_has_every_answer_once_in_order_and_ends_on_the_latest_layout() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut slow = Lines::open(&socket, "jaw");
            let mut setter = Lines::open(&socket, "jaw");
            let keep = |setter: &mut Lines, v: u64| {
                let answer = setter.call("layout.set", json!({ "v": v, "layout": wide() }));
                assert_eq!(answer["result"]["kept"], true, "{answer}");
            };
            // The slow connection reads nothing while the terminal's other connection keeps layouts,
            // until the queue to it is full and a push waits for room, as a reply would.
            let mut v = 0;
            while setter.waiting() == 0 {
                v += 1;
                assert!(v <= 400, "no push waited for room, so pushes were dropped or held the connection up");
                keep(&mut setter, v);
            }
            // Meanwhile its requests aren't read, and the layouts kept after the push that waits stay
            // in its channel, the latest last.
            let asked: Vec<u64> = (0..20).map(|_| slow.send("layout.get", Value::Null)).collect();
            for _ in 0..5 {
                v += 1;
                keep(&mut setter, v);
            }
            // Once it reads, the push that waited comes, then the requests are read and answered down
            // the lane, in order, each with the latest layout, and the latest is pushed.
            let (answers, pushes) = slow.until(&asked, v);
            assert_eq!(answers.iter().map(|(id, _)| *id).collect::<Vec<_>>(), asked, "each answered once, in order");
            assert!(answers.iter().all(|&(_, got)| got == v), "each read after the last layout kept: {answers:?}");
            assert!(pushes.is_sorted_by(|a, b| a < b), "a push older than one before it: {pushes:?}");
            assert_eq!(pushes.last(), Some(&v), "it ends on the latest");

            // Now an answer waits: it asks for its layout again and again, reading nothing, until the
            // answers fill the queue to it.
            let mut asked = Vec::new();
            while setter.waiting() == 0 {
                assert!(asked.len() < 400, "no answer waited for room");
                asked.push(slow.send("layout.get", Value::Null));
            }
            // The layouts kept meanwhile are neither pushed ahead of the answer that waits nor in its
            // place: once it reads, every answer comes, once and in order, then only the latest.
            let before = v;
            for _ in 0..5 {
                v += 1;
                keep(&mut setter, v);
            }
            let (answers, pushes) = slow.until(&asked, v);
            assert_eq!(answers.iter().map(|(id, _)| *id).collect::<Vec<_>>(), asked, "every answer once, in order");
            let read = answers.iter().map(|&(_, got)| got);
            assert!(read.clone().all(|got| (before..=v).contains(&got)), "{answers:?}");
            assert!(read.is_sorted(), "an answer older than one before it: {answers:?}");
            assert_eq!(pushes, [v], "only the latest of the layouts kept while the answer waited");
            assert_eq!(setter.waiting(), 0, "nothing waits once it has read");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn layout_requests_take_their_turn_in_the_lane_as_the_hello_before_them_named_the_connection() {
        let (home, socket) = serving_layouts().await;
        let db = home.path().join("jaw.db");
        tokio::task::spawn_blocking(move || {
            let mut jaw = Lines::open(&socket, "jaw");
            // jaw.db's write lock, held from outside, so the set waits on the writer, and the get
            // sent after it waits for it.
            let holder = rusqlite::Connection::open(&db).unwrap();
            holder.execute_batch("BEGIN IMMEDIATE").unwrap();
            let set = jaw.send("layout.set", json!({ "v": 5, "layout": halves(5) }));
            let get = jaw.send("layout.get", Value::Null);
            // Meanwhile what's answered at once is: focus, and a hello giving the connection another
            // name.
            let focus = jaw.call("focus", json!({ "focused": true }));
            assert!(focus.get("result").is_some(), "{focus}");
            let hello = jaw.call("hello", json!({ "protocol": PROTOCOL, "client": "jaw-mac", "version": "0" }));
            assert!(hello.get("result").is_some(), "{hello}");
            assert_eq!(jaw.pushes, Vec::<Value>::new(), "the set or the get was answered while the writer was held");
            holder.execute_batch("COMMIT").unwrap();
            // Let go, the set is kept and the get reads it, in the order sent, each as `jaw`, the
            // name the connection had when it sent them.
            let answers: Vec<Value> = (0..2).map(|_| jaw.within(Duration::from_secs(10)).expect("an answer")).collect();
            assert_eq!((&answers[0]["id"], &answers[0]["result"]["kept"]), (&json!(set), &json!(true)), "{answers:?}");
            assert_eq!(answers[1]["id"], get, "{answers:?}");
            for answer in &answers {
                assert_eq!((&answer["result"]["interface"], &answer["result"]["v"]), (&json!("jaw"), &json!(5)));
            }
            // Named anew, it has nothing kept of its own.
            let own = jaw.call("layout.get", Value::Null);
            assert_eq!(own["result"], json!({ "interface": "jaw-mac", "v": 0 }));
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_half_closed_connection_owed_nothing_with_a_push_waiting_is_closed_with_nothing_left_waiting() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut slow = Lines::open(&socket, "jaw");
            let mut setter = Lines::open(&socket, "jaw");
            fill_then(&mut setter, 5);
            slow.reader.get_ref().set_read_timeout(Some(Duration::from_secs(20))).unwrap();
            slow.writer.shutdown(std::net::Shutdown::Write).unwrap();
            let (answers, pushes, closed) = to_the_end(&mut slow, Duration::from_secs(20));
            assert!(answers.is_empty());
            assert!(pushes.is_sorted_by(|a, b| a < b), "{pushes:?}");
            assert!(closed, "jawd closed it");
            assert_eq!(setter.waiting(), 0);
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_named_anew_is_pushed_its_new_interfaces_layouts_and_not_its_old_ones() {
        let (_home, socket) = serving_layouts().await;
        tokio::task::spawn_blocking(move || {
            let mut renamed = Lines::open(&socket, "jaw");
            let hello = renamed.call("hello", json!({ "protocol": PROTOCOL, "client": "jaw-mac", "version": "0" }));
            assert!(hello.get("result").is_some(), "{hello}");
            let mut jaw = Lines::open(&socket, "jaw");
            let mut mac = Lines::open(&socket, "jaw-mac");
            assert_eq!(jaw.call("layout.set", json!({ "v": 5, "layout": halves(5) }))["result"]["kept"], true);
            assert_eq!(mac.call("layout.set", json!({ "v": 7, "layout": halves(7) }))["result"]["kept"], true);
            let mut pushes = Vec::new();
            while let Some(line) = renamed.within(Duration::from_secs(1)) {
                pushes.push((line["params"]["interface"].clone(), line["params"]["v"].clone()));
            }
            assert_eq!(pushes, [(json!("jaw-mac"), json!(7))], "the new name's layout, and not the old one's");
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_second_jawd_refuses_to_start() {
        let (_home, socket) = serving().await;
        let refused = listen(&socket).unwrap_err();
        assert!(refused.to_string().contains("another jawd is already running"));
    }

    #[test]
    fn a_stale_socket_is_cleared() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        std::fs::write(&socket, b"").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            listen(&socket).unwrap();
        });
        let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
