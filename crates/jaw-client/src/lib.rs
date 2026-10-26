//! The client side of jaw's protocol, for every Rust interface: `Client`, a connection with threads
//! of its own that never makes its caller wait and reconnects by itself; the window models it
//! feeds, `Chats` and `Messages`; `Connection`, which asks and waits, for commands such as
//! `jaw status`; and `usage`, what a process costs, for the benches.

mod client;
pub mod usage;
mod window;

pub use client::{Client, Config, Event, Failure, Frame, Stats, WindowId};
pub use window::{Chats, Merged, Messages, Moved, Place, Row, Update};

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

/// The process listening on `socket`, jawd's, as the socket says, to tell which jawd to stop; none
/// if nothing listens or the system doesn't say.
pub fn jawd_pid(socket: &Path) -> Option<u32> {
    peer_pid(&UnixStream::connect(socket).ok()?)
}

#[cfg(target_os = "macos")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut pid: libc::pid_t = 0;
    let mut size = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `size` bytes into `pid`, which is that size, for a
    // descriptor the stream holds open.
    let asked = unsafe {
        libc::getsockopt(stream.as_raw_fd(), libc::SOL_LOCAL, libc::LOCAL_PEERPID, (&raw mut pid).cast(), &mut size)
    };
    (asked == 0 && pid > 0).then_some(pid as u32)
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let mut peer = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt writes at most `size` bytes into `peer`, which is that size, for a
    // descriptor the stream holds open.
    let asked = unsafe {
        libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut peer).cast(), &mut size)
    };
    (asked == 0 && peer.pid > 0).then_some(peer.pid as u32)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_pid(_: &UnixStream) -> Option<u32> {
    None
}

/// How the account came to be logged out, in a few words: jawd's reason, which may be WhatsApp's,
/// or the phone's doing when it gave none.
pub fn logged_out(reason: Option<&str>) -> String {
    match reason {
        None => "logged out from the phone".to_string(),
        Some(reason) if reason.to_lowercase().starts_with("logged out") => reason.to_string(),
        Some(reason) => format!("logged out: {reason}"),
    }
}

/// `status` as `jawd status` and `jaw status` print it.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

pub fn describe(status: &Status) -> String {
    let account = match status.account.state {
        AccountState::Unlinked => "not linked yet".to_string(),
        AccountState::Linking => "linking".to_string(),
        AccountState::Linked => match &status.account.me {
            Some(me) if me.name.is_empty() => format!("linked as {}", me.phone),
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
        ConnectionState::LoggedOut => logged_out(status.connection.reason.as_deref()),
        ConnectionState::UpdateNeeded => match status.connection.reason.as_deref() {
            Some(why) => format!("update needed: {why}"),
            None => "WhatsApp wants a newer version of jaw".to_string(),
        },
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
        lines.push(format!(
            "stored    {} messages in {} chats, {} people",
            stored.messages, stored.chats, stored.people
        ));
        let waiting: Vec<String> = [
            (stored.inbox, "events to convert"),
            (stored.history, "messages of history to convert"),
            (stored.unindexed, "messages to index for search"),
            (stored.quarantined, "changes that failed, kept in quarantine"),
            (stored.merging, "chats being merged into another"),
            (stored.merges_stuck, "merges that kept failing, left until jawd starts again"),
            (stored.outbox, "messages of yours to send"),
            (stored.unconfirmed, "of them refused when they may have gone, waiting for a receipt"),
            (stored.regroup, "members who left groups, whose ticks there are to look at again"),
            (stored.settled, "group messages read by all, whose recipients are to let go of"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !waiting.is_empty() {
            lines.push(format!("waiting   {}", waiting.join(", ")));
        }
    }
    if let Some(interfaces) = &status.interfaces {
        let front = if interfaces.focused { ", one in front" } else { "" };
        let connected = match interfaces.connected {
            1 => "1 interface".to_string(),
            n => format!("{n} interfaces"),
        };
        lines.push(format!("open      {connected}{front}; notifications {}", interfaces.notifications));
        if interfaces.lane_most > 0 {
            let requests = match interfaces.in_lanes {
                1 => "1 request".to_string(),
                n => format!("{n} requests"),
            };
            lines.push(format!(
                "lanes     {requests}, this one among them, at most {} a connection; {} lanes full, {} replies waiting for their interface to read; {} finds and saves on tasks of their own, at most {} a connection",
                interfaces.lane_most,
                interfaces.lanes_full,
                interfaces.replies_waiting,
                interfaces.apart,
                interfaces.apart_most
            ));
        }
    }
    if let Some(windows) = &status.windows {
        lines.push(format!(
            "windows   {} watched, {} of them different, at most {} a connection; their reads kept {:.1} of {:.1} MB, {} let go; {} reads, and {} times a window took another's",
            windows.watched,
            windows.distinct,
            windows.most,
            windows.kept_bytes as f64 / 1e6,
            windows.kept_most as f64 / 1e6,
            windows.let_go,
            windows.reads,
            windows.taken + windows.joined
        ));
    }
    for (domain, settings) in &status.settings {
        let said: Vec<String> = settings
            .iter()
            .map(|(key, value)| format!("{key} {}", value.as_str().map_or_else(|| value.to_string(), str::to_string)))
            .collect();
        let mut line = format!("settings  {domain}: {}", said.join(", "));
        for ignored in status.settings_ignored.iter().filter(|i| &i.domain == domain) {
            line.push_str(&format!(
                "; {} {} isn't one this jawd takes, so its default is used",
                ignored.key, ignored.value
            ));
        }
        lines.push(line);
    }
    let times = |n: u64| if n == 1 { "1 time".to_string() } else { format!("{n} times") };
    if let Some(presence) = &status.presence {
        lines.push(format!(
            "presence  {} people followed, at most {} a connection and {} new a minute; {} shown in windows, {} of them past the limits; {} heard, {} dropped; about texts asked for {}",
            presence.followed,
            presence.most,
            presence.per_minute,
            presence.wanted,
            presence.waiting,
            presence.heard,
            presence.dropped,
            times(presence.about_asked)
        ));
    }
    if let Some(folders) = &status.folders {
        let mut line = format!(
            "folders   counts checked {}, found drifted and built again {}",
            times(folders.checks),
            times(folders.drifted)
        );
        if let Some(last) = &folders.last_drift {
            line.push_str(&format!(", last: {last}"));
        }
        lines.push(line);
    }
    if let Some(storage) = &status.storage {
        if let Some(trouble) = &storage.trouble {
            lines.push(format!("trouble   {trouble}"));
        }
        let mut wal = format!(
            "wal       {:.1} MB, {} frames, {} not yet in jaw.db, at most {} frames; {} synced commits",
            storage.wal_bytes as f64 / 1e6,
            storage.wal_frames,
            storage.frames_waiting,
            storage.wal_peak_frames,
            storage.synced_commits
        );
        if storage.background_waits > 0 {
            wal.push_str(&format!(
                "; background waited for it {} times, {:.1} s in all",
                storage.background_waits,
                storage.background_waited_ms as f64 / 1e3
            ));
        }
        if storage.shared_writes > 0 || storage.shared_failed > 0 {
            wal.push_str(&format!(
                "; {} live writes in the inbox's commits, {} of those commits made again without them",
                storage.shared_writes, storage.shared_failed
            ));
        }
        wal.push_str(if storage.full_flush { "; checkpoints flushed in full" } else { "; checkpoints fenced" });
        lines.push(wal);
        for backup in &storage.backups {
            let days = (backup.kept_until_ms - now_ms()).max(0) / 86_400_000;
            lines.push(format!(
                "backup    {}, {:.1} MB, taken before an upgrade; kept {days} more days",
                backup.file,
                backup.bytes as f64 / 1e6
            ));
        }
    }
    if let Some(layouts) = &status.layouts {
        let mut kept = format!(
            "layouts   {} kept of {}, the biggest {} of {} bytes",
            layouts.kept, layouts.most, layouts.largest_bytes, layouts.bytes_most
        );
        if layouts.refused > 0 {
            kept.push_str(&format!("; {} refused for being past a limit", layouts.refused));
        }
        lines.push(kept);
    }
    if let Some(media) = &status.media {
        let mut cache = format!(
            "media     {} files, {:.1} MB of {:.0} MB; videos and documents download by themselves up to {:.0} MB",
            media.cache_files,
            media.cache_bytes as f64 / 1e6,
            media.cache_limit as f64 / 1e6,
            media.auto_bytes as f64 / 1e6
        );
        let counts: Vec<String> = [
            (media.wanted, "waiting"),
            (media.downloading, "being done"),
            (media.waiting_for_phone, "waiting for the phone"),
            (media.unnamed, "no message names, to remove"),
            (media.partial_removed, "partial files removed at start"),
            (media.orphans_removed, "files no message named removed at start"),
            (media.evicted, "dropped to keep to its size"),
            (media.remade, "previews found broken and made again"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !counts.is_empty() {
            cache.push_str(&format!("; {}", counts.join(", ")));
        }
        lines.push(cache);
        if let Some(paused) = &media.paused {
            lines.push(format!("paused    background downloads: {paused}"));
        }
        let mut previews = vec![format!("{} preview writes to settle", media.unsettled)];
        if media.checking {
            previews.push(format!("checking {} files a power cut may have broken", media.suspect));
        }
        if media.full_flush {
            previews.push("files flushed in full, since jaw's folder isn't all on the Mac's own drive".into());
        }
        lines.push(format!("previews  {}", previews.join("; ")));
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

#[cfg(test)]
mod tests {
    #[test]
    fn status_says_how_many_layouts_are_kept_of_the_most_their_biggest_and_how_many_were_refused() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/fixtures/status.result.json")).unwrap();
        let mut status: super::Status = serde_json::from_value(fixture["result"].clone()).unwrap();
        let said = super::describe(&status);
        assert!(said.lines().any(|l| l == "layouts   2 kept of 16, the biggest 312 of 16384 bytes"), "{said}");
        status.layouts.as_mut().unwrap().refused = 3;
        let said = super::describe(&status);
        let refused = "layouts   2 kept of 16, the biggest 312 of 16384 bytes; 3 refused for being past a limit";
        assert!(said.lines().any(|l| l == refused), "{said}");
    }

    #[test]
    fn jawds_process_is_the_one_listening_on_its_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("jaw.sock");
        let _listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert_eq!(super::jawd_pid(&socket), Some(std::process::id()));
        assert_eq!(super::jawd_pid(&dir.path().join("nothing.sock")), None);
    }

    #[test]
    fn a_logout_is_said_with_jawds_reason_and_as_the_phones_without_one() {
        assert_eq!(super::logged_out(None), "logged out from the phone");
        assert_eq!(super::logged_out(Some("logged out from jaw")), "logged out from jaw");
        assert_eq!(super::logged_out(Some("logged out from another device")), "logged out from another device");
        let gone = super::logged_out(Some("this device isn't linked any more"));
        assert_eq!(gone, "logged out: this device isn't linked any more");
    }

    #[test]
    fn an_update_needed_is_said_with_the_engines_reason_and_as_whatsapps_without_one() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/fixtures/status.result.json")).unwrap();
        let mut status: super::Status = serde_json::from_value(fixture["result"].clone()).unwrap();
        status.connection = jaw_proto::Connection::new(super::ConnectionState::UpdateNeeded);
        let said = super::describe(&status);
        assert!(said.lines().any(|l| l == "whatsapp  WhatsApp wants a newer version of jaw"), "{said}");
        status.connection.reason = Some("session.db is from a newer jaw-engine".into());
        let said = super::describe(&status);
        assert!(said.lines().any(|l| l == "whatsapp  update needed: session.db is from a newer jaw-engine"), "{said}");
    }

    #[test]
    fn status_says_the_settings_and_any_this_jawd_doesnt_take() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/fixtures/status.result.json")).unwrap();
        let status: jaw_proto::Status = serde_json::from_value(fixture["result"].clone()).unwrap();
        let said = super::describe(&status);
        assert!(
            said.contains(
                "settings  jaw: messages irc, theme tokyo-night; theme \"rose-pine-moon\" isn't one this jawd takes, so its default is used"
            ),
            "{said}"
        );
    }

    #[test]
    fn status_says_whose_presence_is_followed_within_its_limits_and_how_the_folders_counts_checked() {
        let fixture = include_str!("../../../protocol/fixtures/status.result.json");
        let reply: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let status: jaw_proto::Status = serde_json::from_value(reply["result"].clone()).unwrap();
        let said = super::describe(&status);
        let line = |start: &str| said.lines().find(|line| line.starts_with(start)).unwrap_or_default().to_string();
        assert_eq!(
            line("presence  "),
            "presence  12 people followed, at most 200 a connection and 20 new a minute; 40 shown in windows, 28 of them past the limits; 310 heard, 0 dropped; about texts asked for 3 times"
        );
        assert_eq!(
            line("folders   "),
            "folders   counts checked 2 times, found drifted and built again 1 time, last: 1 of your folders' counts aren't what they hold"
        );
    }
}
