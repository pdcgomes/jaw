//! The client side of jaw's protocol, for every Rust interface: connecting to jawd, saying hello,
//! and asking it things. The connection's own threads and the window models come with M1.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use jaw_proto::{AccountState, ConnectionState, FromJawd, Hello, Outcome, PROTOCOL, Push, Request, Status, Welcome};
use serde_json::Value;

/// A connection that asks and waits: for commands such as `jaw status`, not for drawing.
pub struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
    /// Pushes that arrived while a call waited for its reply.
    pushes: VecDeque<Push>,
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
        let mut connection = Connection {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            next: 1,
            pushes: VecDeque::new(),
        };
        let hello =
            Hello { protocol: PROTOCOL, client: client.into(), version: version.into(), focus: false, since: None };
        let welcome = connection.call("hello", serde_json::to_value(hello)?)?;
        Ok((connection, serde_json::from_value(welcome).context("jawd's answer to hello")?))
    }

    /// Sends a request and waits for its reply. Pushes that arrive meanwhile are kept for `push`.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let mut line = serde_json::to_vec(&Request { id, method: method.into(), params })?;
        line.push(b'\n');
        self.writer.write_all(&line)?;
        loop {
            match self.line()? {
                FromJawd::Reply(reply) if reply.id == id => {
                    return match reply.outcome {
                        Outcome::Result(result) => Ok(result),
                        Outcome::Error(error) => Err(anyhow!(error.message)),
                    };
                }
                FromJawd::Push(push) => self.pushes.push_back(push),
                FromJawd::Reply(_) => continue,
            }
        }
    }

    /// The next push from jawd, waiting for one if none has come.
    pub fn push(&mut self) -> Result<Push> {
        if let Some(push) = self.pushes.pop_front() {
            return Ok(push);
        }
        loop {
            if let FromJawd::Push(push) = self.line()? {
                return Ok(push);
            }
        }
    }

    fn line(&mut self) -> Result<FromJawd> {
        let mut text = String::new();
        if self.reader.read_line(&mut text)? == 0 {
            bail!("jawd closed the connection");
        }
        serde_json::from_str(&text).context("a line from jawd that isn't jaw's protocol")
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
    if let Some(catch_up) = &status.catch_up {
        let rate = |seconds: f64| catch_up.stored as f64 / seconds.max(1e-6);
        lines.push(match catch_up.seconds {
            Some(seconds) => format!(
                "catch-up  {} messages in {seconds:.1} s, {:.0} a second ({} receipts and {} things in all)",
                catch_up.stored,
                rate(seconds),
                catch_up.receipts,
                catch_up.total
            ),
            None => format!("catch-up  {} of {} messages so far", catch_up.stored, catch_up.messages),
        });
    }
    if let Some(history) = &status.history {
        lines.push(format!(
            "history   {} messages in {} chats from {} blobs ({:.1} MB), the phone says {}%; fetched in {:.1} s, stored in {:.1} s",
            history.messages,
            history.conversations,
            history.blobs,
            history.bytes as f64 / 1e6,
            history.progress,
            history.download_seconds,
            history.store_seconds
        ));
    }
    if let Some(stored) = status.stored {
        lines
            .push(format!("stored    {} kept as they arrived, {} messages from history", stored.inbox, stored.history));
    }
    for missing in &status.missing {
        lines.push(format!("missing   {}, for {}", missing.what, missing.needed_for));
    }
    if !status.pools.is_empty() {
        lines.push(String::new());
        lines.push(pools(&status.pools));
    }
    lines.join("\n")
}

/// The work pools as a table: what each is running and holding, and its waits and runs.
pub fn pools(pools: &[jaw_proto::Pool]) -> String {
    let times = |p: Option<jaw_proto::Percentiles>| match p {
        Some(p) => format!("{} / {}", duration(p.p50_us), duration(p.p99_us)),
        None => "–".to_string(),
    };
    let mut rows = vec![[
        "pool".to_string(),
        "running".to_string(),
        "queued".to_string(),
        "done".to_string(),
        "waits, interactive p50 / p99".to_string(),
        "waits, background".to_string(),
        "runs".to_string(),
    ]];
    for pool in pools {
        rows.push([
            pool.name.clone(),
            format!("{} of {}", pool.running, pool.workers),
            format!("{} + {}", pool.queued_interactive, pool.queued_background),
            pool.done.to_string(),
            times(pool.wait_interactive),
            times(pool.wait_background),
            times(pool.run),
        ]);
    }
    let widths: Vec<usize> = (0..7).map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0)).collect();
    rows.iter()
        .map(|row| {
            row.iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn duration(us: u64) -> String {
    match us {
        0..1_000 => format!("{us} µs"),
        1_000..1_000_000 => format!("{:.1} ms", us as f64 / 1e3),
        _ => format!("{:.1} s", us as f64 / 1e6),
    }
}
