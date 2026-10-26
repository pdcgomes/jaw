//! The socket interfaces talk to jawd on. As haunt's daemon does, it refuses to start while another
//! jawd answers, clears a stale socket, and is readable only by its owner; it also talks only to
//! the user who owns jaw's folder.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::{Context, Result, bail};
use jaw_proto::find::{ChatOpened, Find};
use jaw_proto::sync::HistoryMore;
use jaw_proto::{Error, Focus, Hello, LinkStart, Outcome, PROTOCOL, Push, Reply, Request, code};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

use crate::attention::Viewer;
use crate::engine::{EngineError, pb};
use crate::feed;
use crate::find::{Cancel, Cancelled};
use crate::state::State;
use crate::sync::Refused;

/// Lines waiting for an interface to read them. Window updates wait in their windows instead, so a
/// slow interface never holds anything else up, and a reply that finds no room waits on its own,
/// one at most.
const WAITING_LINES: usize = 64;
/// Requests a connection's lane holds, waiting or being answered, before it reads no more.
pub const LANE_MOST: usize = 64;

/// What a connection has said, its windows, and what it's looking at.
struct Session {
    greeted: bool,
    windows: Option<feed::Client>,
    viewer: Viewer,
}

/// Every connection's lane and waiting reply, summed for `jawd status`.
#[derive(Default)]
pub struct Lanes {
    requests: AtomicI64,
    full: AtomicI64,
    waiting: AtomicI64,
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

    /// The connections whose reply waits for room in the queue to their interface.
    pub fn waiting(&self) -> u64 {
        Lanes::count(&self.waiting)
    }

    fn count(count: &AtomicI64) -> u64 {
        u64::try_from(count.load(Ordering::Relaxed)).unwrap_or(0)
    }

    fn add(&self, before: [i64; 3], after: [i64; 3]) {
        for ((count, before), after) in [&self.requests, &self.full, &self.waiting].into_iter().zip(before).zip(after) {
            count.fetch_add(after - before, Ordering::Relaxed);
        }
    }
}

/// What a connection owes its interface: the requests it has sent down its lane and not had back,
/// and the reply waiting for room in the queue, one at most. It's counted in `Lanes` for as long as
/// the connection lasts, each change as it's made.
struct Owed<'a> {
    lanes: &'a Lanes,
    in_lane: usize,
    waiting: Option<Vec<u8>>,
}

impl<'a> Owed<'a> {
    fn new(lanes: &'a Lanes) -> Owed<'a> {
        Owed { lanes, in_lane: 0, waiting: None }
    }

    fn full(&self) -> bool {
        self.in_lane >= LANE_MOST
    }

    /// A request sent down the lane.
    fn took(&mut self) {
        self.change(|owed| owed.in_lane += 1);
    }

    /// A request's reply back from the lane.
    fn answered(&mut self) {
        self.change(|owed| owed.in_lane -= 1);
    }

    /// Keeps `line` until there's room for it.
    fn wait(&mut self, line: Vec<u8>) {
        self.change(|owed| owed.waiting = Some(line));
    }

    /// The reply that waited, now there's room for it.
    fn sent(&mut self) -> Vec<u8> {
        self.change(|owed| owed.waiting.take().expect("a reply waits"))
    }

    /// Its part in `Lanes`.
    fn counts(&self) -> [i64; 3] {
        [self.in_lane as i64, i64::from(self.full()), i64::from(self.waiting.is_some())]
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
        self.lanes.add(self.counts(), [0; 3]);
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

/// One connection. Its windows, hello and focus are answered on its own task, a find on a task of
/// its own, and every other request down the connection's lane, in the order sent, so a watch
/// never waits on jaw.db's writer, the engine or a search. Requests go ahead of the windows'
/// flushes. A reply that finds the queue to the interface full waits, and meanwhile no request is
/// read, no other reply or push is sent, and the windows send nothing, so nothing goes ahead of
/// the watch a reply may be the result of; but they still take their changes, so an interface
/// that leaves its queue full is told to watch again, a reply waiting or not.
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
        greeted: false,
        windows: state.feed().map(|feed| feed.client(out.clone())),
        viewer: state.attention().viewer(),
    };
    // Pushes start once the connection has said hello.
    let mut pushes: Option<broadcast::Receiver<Push>> = None;
    // Finds answer when they're done, so the next line, perhaps a newer find, is read meanwhile.
    let (found, mut finished) = mpsc::unbounded_channel();
    let mut finding = Finding::default();
    let (answered, mut answers) = mpsc::unbounded_channel();
    let lane = lane(state.clone(), answered);
    let mut owed = Owed::new(&state.lanes);
    loop {
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
            Some(reply) = answers.recv(), if open => {
                owed.answered();
                send(&out, &reply, &mut owed, &mut session)?;
            }
            Some(reply) = finished.recv(), if open => send(&out, &reply, &mut owed, &mut session)?,
            line = lines.next_line(), if open && !owed.full() => {
                let Some(line) = line? else { return Ok(()) };
                if line.trim().is_empty() {
                    continue;
                }
                match take(state, &line, &mut session).await {
                    Taken::Answer(reply) => send(&out, &reply, &mut owed, &mut session)?,
                    Taken::Find(request) => finding.start(state, request, found.clone()),
                    Taken::Lane(request) => {
                        owed.took();
                        lane.send(request).context("the connection's lane stopped")?;
                    }
                }
            }
            ready = windows(&mut session.windows) => {
                if let Some(windows) = session.windows.as_mut() {
                    windows.handle(ready).await;
                }
            }
            push = next(&mut pushes), if open => match push {
                // A connection too slow for its pushes misses them; the next says how things are.
                Ok(push) => drop(out.try_send(encode(&push))),
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => pushes = None,
            },
        }
    }
}

/// Puts `reply` in the queue to the interface, or, with no room there, keeps it waiting and holds
/// the windows.
fn send(out: &mpsc::Sender<Vec<u8>>, reply: &Reply, owed: &mut Owed<'_>, session: &mut Session) -> Result<()> {
    match out.try_send(encode(reply)) {
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

/// A connection's lane: answers the requests sent down it one at a time, in order, each reply to
/// `answered`, until the connection or jawd stops; the request being answered then is answered
/// all the same.
fn lane(state: Arc<State>, answered: mpsc::UnboundedSender<Reply>) -> mpsc::UnboundedSender<Request> {
    let (lane, mut requests) = mpsc::unbounded_channel::<Request>();
    tokio::spawn(async move {
        loop {
            let request = tokio::select! {
                request = requests.recv() => request,
                _ = state.stopping() => return,
            };
            let Some(request) = request else { return };
            if answered.is_closed() {
                return;
            }
            if answered.send(answer(&state, request).await).is_err() {
                return;
            }
        }
    });
    lane
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
        tokio::spawn(async move {
            let reply = match finder.find(find, cancel).await {
                Ok(result) => succeeded(id, &result),
                Err(e) if e.is::<Cancelled>() => failed(id, Error::new(code::CANCELLED, e.to_string())),
                Err(e) => failed(id, Error::new(code::INTERNAL, format!("jawd couldn't search: {e:#}"))),
            };
            let _ = found.send(reply);
        });
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
    /// A find, answered on a task of its own.
    Find(Request),
    /// Answered down the connection's lane, after the requests sent there before it.
    Lane(Request),
}

/// Takes one line from an interface: its windows, hello and focus are answered here, a find on a
/// task of its own, and the rest down the lane.
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
                session.viewer.focus(hello.focus);
                if let Some(windows) = session.windows.as_mut() {
                    windows.hello(hello.since);
                }
                succeeded(id, &state.welcome())
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
        "chats.watch" | "messages.watch" | "watch.move" | "unwatch" => match session.windows.as_mut() {
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
        _ => return Taken::Lane(request),
    };
    Taken::Answer(reply)
}

/// The answer to a request down the lane: whatever waits on jaw.db's writer or the engine.
async fn answer(state: &State, request: Request) -> Reply {
    let id = request.id;
    match request.method.as_str() {
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
        "link.logout" => logout(state, id).await,
        "history.more" => history_more(state, id, request.params).await,
        method if crate::send::Sending::METHODS.contains(&method) => match state.sending() {
            Some(sending) => match sending.call(&request.method, request.params).await {
                Ok(result) => succeeded(id, &result),
                Err(error) => failed(id, error),
            },
            None => failed(id, Error::new(code::INTERNAL, "this jawd has no jaw.db open, so it can't send")),
        },
        method => failed(id, Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
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
    match crate::sync::history_more(store, engine, more.chat).await {
        Ok(()) => succeeded(id, &serde_json::json!({})),
        Err(Refused::NotFound(why)) => failed(id, Error::new(code::NOT_FOUND, why)),
        Err(Refused::Engine(EngineError::Failed { message, retry: false })) => {
            failed(id, Error::new(code::NOT_ALLOWED, message))
        }
        Err(Refused::Engine(e)) => failed(id, Error::new(code::ENGINE_UNAVAILABLE, e.to_string())),
    }
}

/// Logs the account out, and once the engine has, ends it as WhatsApp logging it out would, before
/// answering: whatsmeow says nothing after a logout it was asked for.
async fn logout(state: &State, id: u64) -> Reply {
    let reply = engine(state, id, pb::command::Kind::Logout(pb::Logout {})).await;
    if let (Outcome::Result(_), Some(store)) = (&reply.outcome, state.store())
        && let Err(e) = crate::sync::logged_out(store, state.live(), &state.pushes, "logged out from jaw").await
    {
        let why = format!("the account is logged out, but jaw.db couldn't forget it: {e:#}");
        return failed(id, Error::new(code::INTERNAL, why));
    }
    reply
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

    use jaw_proto::{AccountState, ConnectionState, Status};
    use serde_json::json;

    use super::*;
    use crate::engine::supervisor::{Backoff, Supervisor};
    use crate::find::Finder;
    use crate::store::{Change, Lane, Message, Store};
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
    async fn hello_comes_first_and_must_match() {
        let (_home, socket) = serving().await;
        tokio::task::spawn_blocking(move || {
            let replies = exchange(
                &socket,
                &[
                    r#"{"id":1,"method":"status"}"#,
                    r#"{"id":2,"method":"hello","params":{"protocol":2,"client":"test","version":"9"}}"#,
                    "not json",
                    r#"{"method":"hello"}"#,
                ],
            );
            assert_eq!(replies[0]["error"]["code"], code::VERSION);
            assert_eq!(replies[1]["error"]["code"], code::VERSION);
            assert_eq!(replies[1]["error"]["data"]["client"], 2);
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
            stream.write_all(b"{\"id\":1,\"method\":\"hello\",\"params\":{\"protocol\":1,\"client\":\"test\",\"version\":\"0\"}}\n").unwrap();
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
                r#"{"id":1,"method":"hello","params":{"protocol":1,"client":"test","version":"0","focus":true}}"#
                    .to_string(),
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
