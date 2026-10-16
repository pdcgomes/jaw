//! The socket interfaces talk to jawd on. As haunt's daemon does, it refuses to start while another
//! jawd answers, clears a stale socket, and is readable only by its owner; it also talks only to
//! the user who owns jaw's folder.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use jaw_proto::{Error, Hello, LinkStart, Outcome, PROTOCOL, Push, Reply, Request, code};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

use crate::engine::{EngineError, pb};
use crate::state::State;

pub fn listen(path: &Path) -> Result<UnixListener> {
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        bail!("another jawd is already running on {}", path.display());
    }
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).with_context(|| format!("can't listen on {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub async fn serve(state: Arc<State>, listener: UnixListener) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
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
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let mut greeted = false;
    // Pushes start once the connection has said hello.
    let mut pushes: Option<broadcast::Receiver<Push>> = None;
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { return Ok(()) };
                if line.trim().is_empty() {
                    continue;
                }
                let reply = answer(state, &line, &mut greeted).await;
                send(&mut write, &reply).await?;
                if greeted && pushes.is_none() {
                    pushes = Some(state.pushes.subscribe());
                }
            }
            push = next(&mut pushes) => match push {
                Ok(push) => send(&mut write, &push).await?,
                // A connection too slow for its pushes misses the oldest; the next says how things are.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => pushes = None,
            },
        }
    }
}

async fn next(pushes: &mut Option<broadcast::Receiver<Push>>) -> Result<Push, broadcast::error::RecvError> {
    match pushes {
        Some(pushes) => pushes.recv().await,
        None => std::future::pending().await,
    }
}

async fn send(write: &mut tokio::net::unix::OwnedWriteHalf, line: &impl serde::Serialize) -> Result<()> {
    let mut out = serde_json::to_vec(line)?;
    out.push(b'\n');
    write.write_all(&out).await?;
    Ok(())
}

/// The reply to one line from an interface.
async fn answer(state: &State, line: &str, greeted: &mut bool) -> Reply {
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
    if !*greeted && request.method != "hello" {
        return failed(id, Error::new(code::VERSION, "say hello first, with the protocol version you speak"));
    }
    match request.method.as_str() {
        "hello" => match serde_json::from_value::<Hello>(request.params) {
            Ok(hello) if hello.protocol == PROTOCOL => {
                *greeted = true;
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
        "link.cancel" => engine(state, id, pb::command::Kind::LinkCancel(pb::LinkCancel {})).await,
        "link.logout" => engine(state, id, pb::command::Kind::Logout(pb::Logout {})).await,
        method => failed(id, Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
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

    use jaw_proto::{AccountState, ConnectionState, Status};

    use super::*;

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
