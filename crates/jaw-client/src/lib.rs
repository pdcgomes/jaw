//! The client side of jaw's protocol, for every Rust interface: connecting to jawd, saying hello,
//! and asking it things. The connection's own threads and the window models come with M1.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use jaw_proto::{AccountState, ConnectionState, FromJawd, Hello, Outcome, PROTOCOL, Request, Status, Welcome};
use serde_json::Value;

/// A connection that asks and waits: for commands such as `jaw status`, not for drawing.
pub struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
}

impl Connection {
    /// Connects to jawd's socket and says hello.
    pub fn open(socket: &Path, client: &str, version: &str) -> Result<(Connection, Welcome)> {
        let stream = UnixStream::connect(socket).map_err(|e| match e.kind() {
            ErrorKind::NotFound | ErrorKind::ConnectionRefused => {
                anyhow!("jawd isn't running: nothing answers on {}", socket.display())
            }
            _ => anyhow!("can't reach jawd on {}: {e}", socket.display()),
        })?;
        let mut connection = Connection { reader: BufReader::new(stream.try_clone()?), writer: stream, next: 1 };
        let hello =
            Hello { protocol: PROTOCOL, client: client.into(), version: version.into(), focus: false, since: None };
        let welcome = connection.call("hello", serde_json::to_value(hello)?)?;
        Ok((connection, serde_json::from_value(welcome).context("jawd's answer to hello")?))
    }

    /// Sends a request and waits for its reply. Pushes that arrive meanwhile are set aside.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let mut line = serde_json::to_vec(&Request { id, method: method.into(), params })?;
        line.push(b'\n');
        self.writer.write_all(&line)?;
        let mut text = String::new();
        loop {
            text.clear();
            if self.reader.read_line(&mut text)? == 0 {
                bail!("jawd closed the connection");
            }
            match serde_json::from_str::<FromJawd>(&text).context("a line from jawd that isn't jaw's protocol")? {
                FromJawd::Reply(reply) if reply.id == id => {
                    return match reply.outcome {
                        Outcome::Result(result) => Ok(result),
                        Outcome::Error(error) => Err(anyhow!(error.message)),
                    };
                }
                _ => continue,
            }
        }
    }
}

/// `status` as `jawd status` and `jaw status` print it.
pub fn describe(status: &Status) -> String {
    let account = match status.account.state {
        AccountState::Unlinked => "not linked yet".to_string(),
        AccountState::Linking => "linking".to_string(),
        AccountState::Linked => match &status.account.me {
            Some(me) => format!("linked as {} ({})", me.name, me.phone),
            None => "linked".to_string(),
        },
    };
    let whatsapp = match status.connection.state {
        ConnectionState::Connecting => "connecting".to_string(),
        ConnectionState::Online => "online".to_string(),
        ConnectionState::CatchingUp => match (status.connection.done, status.connection.total) {
            (Some(done), Some(total)) => format!("catching up, {done} of {total}"),
            _ => "catching up".to_string(),
        },
        ConnectionState::PhoneOffline => "online, but the phone is offline".to_string(),
        ConnectionState::Offline => "offline".to_string(),
        ConnectionState::LoggedOut => "logged out from the phone".to_string(),
        ConnectionState::UpdateNeeded => "WhatsApp wants a newer version of jaw".to_string(),
        ConnectionState::Banned => "temporarily banned".to_string(),
        ConnectionState::EngineRestarting => "reconnecting while the engine restarts".to_string(),
    };
    let mut engine = match &status.engine.version {
        Some(version) => format!("{}, {}: {version}", status.engine.kind, status.engine.state),
        None => format!("{}, {}", status.engine.kind, status.engine.state),
    };
    match status.engine.answer_us {
        Some(us) if us >= 1_000 => engine.push_str(&format!(", answering in {:.1} ms", us as f64 / 1_000.0)),
        Some(us) => engine.push_str(&format!(", answering in {us} µs")),
        None => {}
    }
    if status.engine.restarts > 0 {
        engine.push_str(&format!(", started again {} times", status.engine.restarts));
    }
    if let Some(why) = &status.engine.last_exit {
        engine.push_str(&format!("; last stopped: {why}"));
    }
    let mut lines = vec![
        format!("jawd      {} (protocol {})", status.jawd, status.protocol),
        format!("folder    {}", status.root),
        format!("account   {account}"),
        format!("whatsapp  {whatsapp}"),
        format!("engine    {engine}"),
    ];
    for missing in &status.missing {
        lines.push(format!("missing   {}, for {}", missing.what, missing.needed_for));
    }
    lines.join("\n")
}
