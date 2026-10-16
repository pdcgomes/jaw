//! The socket interfaces talk to jawd on. As haunt's daemon does, it refuses to start while another
//! jawd answers, clears a stale socket, and is readable only by its owner; it also talks only to
//! the user who owns jaw's folder.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

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

/// Lines waiting for an interface to read them. A reply waits for room; window updates wait in
/// their windows instead, so a slow interface never holds anything else up.
const WAITING_LINES: usize = 64;

/// What a connection has said, its windows, and what it's looking at.
struct Session {
    greeted: bool,
    windows: Option<feed::Client>,
    viewer: Viewer,
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

async fn one(state: &State, stream: UnixStream) -> Result<()> {
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
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { return Ok(()) };
                if line.trim().is_empty() {
                    continue;
                }
                if session.greeted
                    && let Ok(request) = serde_json::from_str::<Request>(&line)
                    && request.method == "find"
                {
                    finding.start(state, request, found.clone());
                    continue;
                }
                let reply = answer(state, &line, &mut session).await;
                out.send(encode(&reply)).await.context("the interface stopped reading")?;
                if session.greeted && pushes.is_none() {
                    pushes = Some(state.pushes.subscribe());
                }
            }
            Some(reply) = finished.recv() => out.send(encode(&reply)).await.context("the interface stopped reading")?,
            push = next(&mut pushes) => match push {
                // A connection too slow for its pushes misses them; the next says how things are.
                Ok(push) => drop(out.try_send(encode(&push))),
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => pushes = None,
            },
            ready = windows(&mut session.windows) => {
                if let Some(windows) = session.windows.as_mut() {
                    windows.handle(ready).await;
                }
            }
            _ = state.stopping() => return Ok(()),
        }
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

/// The reply to one line from an interface.
async fn answer(state: &State, line: &str, session: &mut Session) -> Reply {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return failed(0, Error::new(code::PARSE, "that line isn't JSON")),
    };
    let request: Request = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(_) => {
            return failed(
                0,
                Error::new(code::INVALID_REQUEST, "that's JSON, but not a request: it needs an id and a method"),
            );
        }
    };
    let id = request.id;
    if !session.greeted && request.method != "hello" {
        return failed(id, Error::new(code::VERSION, "say hello first, with the protocol version you speak"));
    }
    match request.method.as_str() {
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
        "focus" => match serde_json::from_value::<Focus>(request.params) {
            Ok(Focus { focused }) => {
                session.viewer.focus(focused);
                succeeded(id, &serde_json::json!({}))
            }
            Err(e) => failed(id, Error::new(code::INVALID_PARAMS, format!("focus's params: {e}"))),
        },
        "chat.opened" => match (serde_json::from_value::<ChatOpened>(request.params), state.finder.get()) {
            (Err(e), _) => failed(id, Error::new(code::INVALID_PARAMS, format!("chat.opened's params: {e}"))),
            (Ok(opened), finder) => {
                session.viewer.opened(opened.chat.clone());
                match finder {
                    None => failed(id, Error::new(code::INTERNAL, "jawd has no store open to keep that in")),
                    Some(finder) => match finder.opened(opened.chat).await {
                        Ok(()) => succeeded(id, &serde_json::json!({})),
                        Err(e) => failed(
                            id,
                            Error::new(code::INTERNAL, format!("jawd couldn't keep that the chat was opened: {e:#}")),
                        ),
                    },
                }
            }
        },
        "link.cancel" => engine(state, id, pb::command::Kind::LinkCancel(pb::LinkCancel {})).await,
        "link.logout" => engine(state, id, pb::command::Kind::Logout(pb::Logout {})).await,
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
        "history.more" => history_more(state, id, request.params).await,
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
