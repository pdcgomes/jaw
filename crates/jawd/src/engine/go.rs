//! `GoEngine`: jaw-engine, the Go program on whatsmeow, run as jawd's child on its standard input
//! and output. Its standard error goes into jawd's, and a start that fails says the last line of
//! it, which says why: another jaw-engine holding session.lock, say. Each one started is kept until
//! it has exited: ended in order, its standard input is closed, which it exits on, and should it
//! not exit it's sent SIGTERM and then SIGKILL, each after a pause, and whoever ends it waits until
//! it has gone.

use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, watch};

use super::{Ack, BoxFuture, Commands, End, Engine, Incoming, Instance, Outgoing, PROTOCOL, Pending, frame, pb};

/// The Go runtime's soft memory limit and garbage collection target for jaw-engine, unless jawd's
/// own environment sets them: the engine has 80 MB steady and 200 MB during a full history sync,
/// and the limit leaves room beside Go's heap for SQLite's memory, which Go doesn't count.
const GO_MEMORY: [(&str, &str); 2] = [("GOMEMLIMIT", "160MiB"), ("GOGC", "50")];

/// How long jaw-engine has to say hello once started.
const HELLO: Duration = Duration::from_secs(10);
/// How long jaw-engine has to exit once its standard input is closed, before it's sent SIGTERM: it
/// takes a few milliseconds, a hundred or so on a loaded Mac.
const QUITTING: Duration = Duration::from_secs(1);
/// How long it has to exit after SIGTERM, before SIGKILL.
const TERMINATING: Duration = Duration::from_millis(500);
/// How long a start that failed waits, once jaw-engine has exited, for the end of what it said: a
/// moment, unless another process holds its standard error open.
const LAST_WORDS: Duration = Duration::from_millis(500);

/// How long jaw-engine, ended in order, has to exit once its standard input is closed, and then
/// once it's sent SIGTERM: QUITTING and TERMINATING, unless a test on a loaded Mac needs longer.
#[derive(Clone, Copy, Debug)]
pub struct Pauses {
    pub quitting: Duration,
    pub terminating: Duration,
}

impl Default for Pauses {
    fn default() -> Pauses {
        Pauses { quitting: QUITTING, terminating: TERMINATING }
    }
}

pub struct GoEngine {
    program: PathBuf,
    home: PathBuf,
    log: String,
    pauses: Pauses,
    running: Arc<Running>,
}

impl GoEngine {
    pub fn new(program: PathBuf, home: PathBuf) -> GoEngine {
        GoEngine { program, home, log: "info".into(), pauses: Pauses::default(), running: Arc::default() }
    }

    /// The same, its engines ended in order with `pauses`.
    pub fn with_pauses(self, pauses: Pauses) -> GoEngine {
        GoEngine { pauses, ..self }
    }

    /// Ends every jaw-engine still running as `how` says, and starts none after; says how each
    /// ended, once each has, and logs it, once however many ends waited for it.
    async fn end_all(&self, how: End) -> Vec<String> {
        let running = self.running.close(how);
        let mut ended = Vec::with_capacity(running.len());
        for process in running {
            let logged = process.logged.clone();
            let why = process.exited().await;
            if !logged.swap(true, Ordering::Relaxed) {
                eprintln!("jawd: {why}");
            }
            ended.push(why);
        }
        ended
    }

    /// jaw-engine's command line and environment, its standard streams jawd's to read and write.
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .arg("--home")
            .arg(&self.home)
            .arg("--log")
            .arg(&self.log)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Ctrl-C in jawd's terminal goes to its whole process group; jawd stops the engine.
            .process_group(0)
            .kill_on_drop(true);
        for (name, value) in GO_MEMORY {
            if std::env::var_os(name).is_none() {
                command.env(name, value);
            }
        }
        command
    }

    /// Where jaw-engine is: `JAW_ENGINE` if set, beside jawd, as in Jaw.app's Contents/Helpers/, in
    /// target/engine/ when jawd runs from a checkout's target/, or on PATH.
    pub fn find() -> Option<PathBuf> {
        if let Some(program) = std::env::var_os("JAW_ENGINE") {
            return Some(program.into());
        }
        if let Some(folder) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
            for candidate in [folder.join("jaw-engine"), folder.join("../engine/jaw-engine")] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|folder| folder.join("jaw-engine"))
            .find(|candidate| candidate.is_file())
    }
}

impl Engine for GoEngine {
    fn kind(&self) -> &'static str {
        "go"
    }

    fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, Result<Instance>> {
        Box::pin(async move {
            let Started { stdin, stdout, stderr, process, mut told } = self
                .running
                .start(&mut self.command(), self.pauses)
                .with_context(|| format!("can't start {}", self.program.display()))?;
            let speaking = tokio::spawn(speak(stderr));
            let mut reader = BufReader::with_capacity(64 << 10, stdout);
            let mut writer = BufWriter::with_capacity(64 << 10, stdin);
            let greeted = tokio::select! {
                hello = tokio::time::timeout(HELLO, handshake(&mut reader, &mut writer)) => hello
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("jaw-engine didn't say hello within {HELLO:?}"))),
                _ = told.wait_for(Option::is_some) => Err(anyhow::anyhow!("jawd ended jaw-engine before it said hello")),
            };
            let hello = match greeted {
                Ok(hello) => hello,
                Err(e) => {
                    // Gone before the supervisor can start another on the same session.db.
                    drop(writer);
                    process.tell(End::InOrder);
                    let ended = process.exited().await;
                    // One that can't start says why as it goes, as one refused session.lock does.
                    match tokio::time::timeout(LAST_WORDS, speaking).await.ok().and_then(Result::ok).flatten() {
                        Some(said) => bail!("{e:#}; it said last {said:?}; {ended}"),
                        None => bail!("{e:#}; {ended}"),
                    }
                }
            };
            let (tx, rx) = mpsc::unbounded_channel();
            let commands = Commands { tx };
            let pending = Pending::default();
            tokio::spawn(write(writer, rx, pending.clone(), told));
            let reading = tokio::spawn(read(reader, events, commands.clone(), pending));
            let exited: BoxFuture<'static, String> = Box::pin(async move {
                let why = process.exited().await;
                let _ = reading.await;
                why
            });
            Ok(Instance { hello, commands, exited })
        })
    }

    fn end(&self, how: End) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.end_all(how).await;
        })
    }
}

/// The jaw-engines a `GoEngine` started that haven't exited yet, and whether it may start another.
#[derive(Default)]
struct Running(Mutex<Processes>);

#[derive(Default)]
struct Processes {
    /// jawd has ended its engine, so none is started again.
    ended: bool,
    next: u64,
    live: HashMap<u64, Process>,
}

/// One jaw-engine started: what tells it how to end, why it exited, once it has, and whether an
/// end has logged that.
#[derive(Clone)]
struct Process {
    pid: u32,
    told: Arc<watch::Sender<Option<End>>>,
    exited: watch::Receiver<Option<String>>,
    logged: Arc<AtomicBool>,
}

/// A jaw-engine just started: its standard streams, and what it's told of how to end.
struct Started {
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    process: Process,
    told: watch::Receiver<Option<End>>,
}

impl Running {
    /// Starts `command` unless jawd has ended its engine, and keeps it, on a task of its own, until
    /// it has exited, ending it in order with `pauses`. It starts under the lock `close` takes, so
    /// a jaw-engine is either refused or among those `close` returns.
    fn start(self: &Arc<Running>, command: &mut Command, pauses: Pauses) -> Result<Started> {
        let mut processes = self.0.lock().expect("jaw-engine's processes");
        if processes.ended {
            bail!("jawd has ended its engine, so it starts no other");
        }
        let mut child = command.spawn()?;
        let pid = child.id().expect("a child just started");
        let stdin = child.stdin.take().expect("a piped stdin");
        let stdout = child.stdout.take().expect("a piped stdout");
        let stderr = child.stderr.take().expect("a piped stderr");
        let (told, telling) = watch::channel(None);
        let (said, exited) = watch::channel(None);
        let process = Process { pid, told: Arc::new(told), exited, logged: Arc::default() };
        let key = processes.next;
        processes.next += 1;
        processes.live.insert(key, process.clone());
        tokio::spawn(keep(child, telling.clone(), said, Arc::downgrade(self), key, pauses));
        Ok(Started { stdin, stdout, stderr, process, told: telling })
    }

    /// Starts no more, tells each still running to end as `how` says, and says which they are.
    /// Ending them at once, it also sends each SIGKILL itself, so they're killed even should their
    /// keepers' tasks not run again before jawd exits.
    fn close(&self, how: End) -> Vec<Process> {
        let mut processes = self.0.lock().expect("jaw-engine's processes");
        processes.ended = true;
        for process in processes.live.values() {
            process.tell(how);
            if how == End::AtOnce {
                // SAFETY: kill only sends a signal, to the group of a jaw-engine its keeper hasn't
                // forgotten yet. The keeper forgets it under this lock straight after waiting for
                // it, and the system gives a freed ID to another process only once it has gone
                // round all the others, so the ID is still the engine's.
                unsafe { libc::kill(-(process.pid as i32), libc::SIGKILL) };
            }
        }
        processes.live.values().cloned().collect()
    }

    fn forget(&self, key: u64) {
        self.0.lock().expect("jaw-engine's processes").live.remove(&key);
    }
}

impl Process {
    /// Has it end as `how` says, unless it's been told to end sooner.
    fn tell(&self, how: End) {
        self.told.send_if_modified(|told| {
            let sooner = told.is_none_or(|told| told < how);
            if sooner {
                *told = Some(how);
            }
            sooner
        });
    }

    /// Why it exited, once it has.
    async fn exited(mut self) -> String {
        match self.exited.wait_for(Option::is_some).await {
            Ok(why) => why.clone().unwrap_or_default(),
            Err(_) => "jawd's runtime ended before jaw-engine did".into(),
        }
    }
}

/// Waits for jaw-engine to exit, ending it as `told` comes to say, or once nobody is left to say;
/// then it's no longer running, and `said` says why it exited.
async fn keep(
    mut child: Child,
    mut told: watch::Receiver<Option<End>>,
    said: watch::Sender<Option<String>>,
    running: Weak<Running>,
    key: u64,
    pauses: Pauses,
) {
    let why = ended(&mut child, &mut told, pauses).await;
    if let Some(running) = running.upgrade() {
        running.forget(key);
    }
    said.send_replace(Some(why));
}

/// How jaw-engine ended: by itself, or as it was told. In order, whoever writes to its standard
/// input closes it once it's told, and it's sent SIGTERM if it hasn't exited after the first of
/// `pauses`, and SIGKILL if it hasn't after the second; at once, SIGKILL.
async fn ended(child: &mut Child, told: &mut watch::Receiver<Option<End>>, pauses: Pauses) -> String {
    // What it was told comes first, so an engine killed as it's told to end isn't said to have
    // exited by itself.
    let how = tokio::select! {
        biased;
        how = told.wait_for(Option::is_some) => how.ok().and_then(|how| *how).unwrap_or(End::InOrder),
        status = child.wait() => return exited("jaw-engine exited", status),
    };
    if how == End::InOrder {
        if let Some(status) = within(child, told, pauses.quitting).await {
            return exited("jaw-engine exited once its standard input closed", status);
        }
        if !at_once(told) {
            signal(child, libc::SIGTERM);
            if let Some(status) = within(child, told, pauses.terminating).await {
                return after(Sent::Term, status);
            }
            if !at_once(told) {
                signal(child, libc::SIGKILL);
                return after(Sent::Kill, child.wait().await);
            }
        }
    }
    signal(child, libc::SIGKILL);
    after(Sent::AtOnce, child.wait().await)
}

/// The signal jawd last sent jaw-engine to end it.
#[derive(Clone, Copy)]
enum Sent {
    /// SIGTERM, ended in order, once its standard input had closed and the first pause gone by.
    Term,
    /// SIGKILL, ended in order, once SIGTERM's pause had gone by too.
    Kill,
    /// SIGKILL, at once.
    AtOnce,
}

/// How jaw-engine ended once jawd had sent it `sent`, as its status says: SIGTERM's or SIGKILL's
/// doing, should it show either, and otherwise by itself, just as `sent` came.
fn after(sent: Sent, status: std::io::Result<ExitStatus>) -> String {
    let terminated = "jaw-engine didn't exit once its standard input closed, so SIGTERM ended it";
    let killed = match sent {
        Sent::Term | Sent::Kill => "jaw-engine didn't exit on SIGTERM either, so SIGKILL ended it",
        Sent::AtOnce => "jaw-engine was killed at once, with SIGKILL",
    };
    let how = match (status.as_ref().map(ExitStatusExt::signal), sent) {
        (Ok(Some(libc::SIGTERM)), _) | (Err(_), Sent::Term) => terminated,
        (Ok(Some(libc::SIGKILL)), _) | (Err(_), Sent::Kill | Sent::AtOnce) => killed,
        (Ok(_), Sent::Term) => "jaw-engine exited by itself just as SIGTERM was sent",
        (Ok(_), Sent::Kill) => "jaw-engine exited by itself just as SIGKILL was sent",
        (Ok(_), Sent::AtOnce) => "jaw-engine exited by itself just as it was to be killed at once",
    };
    exited(how, status)
}

/// Its exit, should it come within `pause`, and before it's told to end at once.
async fn within(
    child: &mut Child,
    told: &mut watch::Receiver<Option<End>>,
    pause: Duration,
) -> Option<std::io::Result<ExitStatus>> {
    tokio::select! {
        biased;
        _ = told.wait_for(|told| *told == Some(End::AtOnce)) => None,
        status = child.wait() => Some(status),
        () = tokio::time::sleep(pause) => None,
    }
}

fn at_once(told: &watch::Receiver<Option<End>>) -> bool {
    *told.borrow() == Some(End::AtOnce)
}

/// Sends `signal` to jaw-engine's process group, its own since it started, and so to anything it
/// started too.
fn signal(child: &Child, signal: i32) {
    if let Some(pid) = child.id() {
        // SAFETY: kill only sends a signal, to the group of a child not yet waited for, which keeps
        // its ID from being taken by another process.
        unsafe { libc::kill(-(pid as i32), signal) };
    }
}

fn exited(how: &str, status: std::io::Result<ExitStatus>) -> String {
    match status {
        Ok(status) => format!("{how} ({status})"),
        Err(e) => format!("{how}, but jawd can't wait for it: {e}"),
    }
}

/// Copies what jaw-engine says on its standard error into jawd's, line by line, until it closes,
/// and says its last line.
async fn speak(stderr: ChildStderr) -> Option<String> {
    let mut lines = BufReader::new(stderr).lines();
    let mut last = None;
    while let Ok(Some(line)) = lines.next_line().await {
        eprintln!("engine: {line}");
        if !line.trim().is_empty() {
            last = Some(line);
        }
    }
    last
}

/// Reads the engine's hello, checks it, and answers with jawd's.
async fn handshake(reader: &mut BufReader<ChildStdout>, writer: &mut BufWriter<ChildStdin>) -> Result<pb::Hello> {
    let first = frame::read(reader).await?.context("jaw-engine closed the link before saying hello")?;
    let Some(pb::frame::Body::Hello(hello)) = first.body else {
        bail!("jaw-engine's first frame wasn't a hello");
    };
    if hello.protocol != PROTOCOL {
        bail!(
            "jaw-engine speaks link protocol {} and jawd {PROTOCOL}: rebuild them together with mise run build",
            hello.protocol
        );
    }
    if hello.whatsmeow != wa_proto::whatsmeow() {
        bail!(
            "jaw-engine was built with whatsmeow {} but jawd reads WhatsApp's protobufs from {}: run mise run sync-protos, then mise run build",
            hello.whatsmeow,
            wa_proto::whatsmeow()
        );
    }
    let ours = pb::Hello {
        protocol: PROTOCOL,
        program: format!("jawd {}", env!("CARGO_PKG_VERSION")),
        whatsmeow: wa_proto::whatsmeow().into(),
    };
    frame::write(writer, &pb::Frame { id: 0, body: Some(pb::frame::Body::Hello(ours)) }).await?;
    writer.flush().await?;
    Ok(hello)
}

/// Writes jawd's commands and acks to the engine, a burst at a time. When every Commands and Ack
/// has gone, or the engine is told to end, it closes the engine's standard input, which tells the
/// engine to exit.
async fn write(
    writer: BufWriter<ChildStdin>,
    outgoing: mpsc::UnboundedReceiver<Outgoing>,
    pending: Pending,
    mut told: watch::Receiver<Option<End>>,
) {
    tokio::select! {
        () = pass(writer, outgoing, pending) => {}
        _ = told.wait_for(Option::is_some) => {}
    }
}

async fn pass(mut writer: BufWriter<ChildStdin>, mut outgoing: mpsc::UnboundedReceiver<Outgoing>, pending: Pending) {
    let mut next_id = 1u64;
    while let Some(first) = outgoing.recv().await {
        let mut item = Some(first);
        while let Some(out) = item.take() {
            let frame = match out {
                Outgoing::Ack(seq) => pb::Frame { id: 0, body: Some(pb::frame::Body::Ack(pb::Ack { seq })) },
                Outgoing::Command { kind, reply } => {
                    let id = next_id;
                    next_id += 1;
                    pending.add(id, reply);
                    pb::Frame { id, body: Some(pb::frame::Body::Command(pb::Command { kind: Some(kind) })) }
                }
            };
            if frame::write(&mut writer, &frame).await.is_err() {
                pending.fail_all();
                return;
            }
            item = outgoing.try_recv().ok();
        }
        if writer.flush().await.is_err() {
            pending.fail_all();
            return;
        }
    }
    let _ = writer.shutdown().await;
}

/// Reads the engine's events and replies until it closes its end. It holds only a weak handle on
/// the outgoing channel, for acks, so it doesn't keep the engine's standard input open.
async fn read(
    mut reader: BufReader<ChildStdout>,
    events: mpsc::Sender<Incoming>,
    commands: Commands,
    pending: Pending,
) {
    let weak = commands.tx.downgrade();
    drop(commands);
    loop {
        match frame::read(&mut reader).await {
            Ok(Some(frame)) => match frame.body {
                Some(pb::frame::Body::Event(event)) => {
                    let ack = (event.seq > 0).then(|| weak.upgrade()).flatten().map(|tx| Ack { seq: event.seq, tx });
                    if events.send(Incoming { event, ack }).await.is_err() {
                        break;
                    }
                }
                Some(pb::frame::Body::Reply(reply)) => pending.answer(frame.id, reply),
                other => eprintln!("jawd: jaw-engine sent a frame jawd doesn't take: {other:?}"),
            },
            Ok(None) => break,
            Err(e) => {
                eprintln!("jawd: the engine link broke: {e:#}");
                break;
            }
        }
    }
    pending.fail_all();
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::time::Instant;

    use super::*;
    use crate::engine::supervisor::{Backoff, State, Supervisor};

    /// The engine `mise run engine` builds. Without it the test says so and passes, except in CI,
    /// where `mise run test` builds it first.
    fn built() -> Option<PathBuf> {
        let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/engine/jaw-engine");
        if program.is_file() {
            return Some(program);
        }
        assert!(std::env::var_os("CI").is_none(), "mise run test builds jaw-engine before the Rust tests");
        eprintln!("skipped: {} isn't built; mise run engine builds it", program.display());
        None
    }

    #[test]
    fn the_go_engine_runs_within_its_memory_unless_jawds_own_environment_says_otherwise() {
        let engine = GoEngine::new("jaw-engine".into(), "/nowhere".into());
        let command = engine.command();
        let given: Vec<(&OsStr, Option<&OsStr>)> = command.as_std().get_envs().collect();
        for (name, value) in GO_MEMORY {
            let set = given.iter().find(|(n, _)| *n == name).and_then(|(_, v)| *v);
            match std::env::var_os(name) {
                None => assert_eq!(set, Some(OsStr::new(value)), "{name}"),
                Some(_) => assert_eq!(set, None, "{name} is left as jawd's environment has it"),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_go_engine_says_hello_answers_and_benches() {
        let Some(program) = built() else { return };
        let home = tempfile::tempdir().unwrap();
        let engine = GoEngine::new(program, home.path().to_path_buf());
        let (tx, mut rx) = mpsc::channel(64);
        let instance = engine.start(tx).await.unwrap();
        assert!(instance.hello.program.starts_with("jaw-engine "), "{}", instance.hello.program);
        assert_eq!(instance.hello.whatsmeow, wa_proto::whatsmeow());
        let unlinked = rx.recv().await.unwrap();
        let Some(pb::event::Kind::Connection(connection)) = unlinked.event.kind else { panic!("{:?}", unlinked.event) };
        assert_eq!(connection.state(), pb::connection::State::Unlinked);
        assert!(home.path().join("session.db").is_file());

        let acking = tokio::spawn(async move {
            let mut acked = 0u32;
            while let Some(incoming) = rx.recv().await {
                if let Some(ack) = incoming.ack {
                    ack.send();
                    acked += 1;
                }
            }
            acked
        });
        let pong = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap();
        assert!(matches!(pong, pb::reply::Kind::Pong(_)));
        let result = instance
            .commands
            .call(pb::command::Kind::BenchRun(pb::BenchRun { count: 2_000, size: 600 }))
            .await
            .unwrap();
        let pb::reply::Kind::BenchResult(result) = result else { panic!("{result:?}") };
        assert_eq!(result.count, 2_000);
        assert!(result.p50_us <= result.p99_us && result.p99_us <= result.max_us);
        let refused = instance.commands.call(pb::command::Kind::Logout(pb::Logout {})).await;
        assert!(refused.is_err(), "logging out with no account: {refused:?}");

        drop(instance.commands);
        let why = instance.exited.await;
        assert!(why.contains("exited"), "{why}");
        assert_eq!(acking.await.unwrap(), 2_000);
    }

    /// The engine `mise run engine` builds, started on `home`, once its session is open: it says it's
    /// unlinked then, as it is on a folder of its own.
    async fn serving(program: &Path, home: &Path) -> (GoEngine, Instance, mpsc::Receiver<Incoming>) {
        let engine = GoEngine::new(program.to_path_buf(), home.to_path_buf()).with_pauses(fake::PATIENT);
        let (tx, mut rx) = mpsc::channel(64);
        let instance = engine.start(tx).await.unwrap();
        let unlinked = rx.recv().await.unwrap();
        assert!(matches!(unlinked.event.kind, Some(pb::event::Kind::Connection(_))), "{:?}", unlinked.event);
        (engine, instance, rx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_second_engine_on_its_folder_is_refused_as_a_start_that_failed_and_the_first_runs_on_untouched() {
        let Some(program) = built() else { return };
        let home = tempfile::tempdir().unwrap();
        let (first, instance, _events) = serving(&program, home.path()).await;
        let second = GoEngine::new(program, home.path().to_path_buf());
        let (tx, _rx) = mpsc::channel(8);
        let refused = second.start(tx).await.err().map(|e| format!("{e:#}"));
        let why = format!("another jaw-engine is running on {}", home.path().display());
        assert!(
            refused.as_ref().is_some_and(|e| e.contains("before saying hello")
                && e.contains(&why)
                && e.ends_with("(exit status: 1)")),
            "{refused:?}"
        );
        let pong = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await;
        assert!(matches!(pong, Ok(pb::reply::Kind::Pong(_))), "the first answers as before: {pong:?}");
        let ended = first.end_all(End::InOrder).await;
        assert_eq!(ended, ["jaw-engine exited once its standard input closed (exit status: 0)"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_refused_while_another_has_its_folder_starts_once_that_one_has_gone() {
        let Some(program) = built() else { return };
        let home = tempfile::tempdir().unwrap();
        let (first, _instance, _events) = serving(&program, home.path()).await;
        let (events, _incoming) = mpsc::channel(64);
        let pausing =
            Backoff { first: Duration::from_millis(100), most: Duration::from_millis(400), ..Backoff::default() };
        let engine = Arc::new(GoEngine::new(program, home.path().to_path_buf()));
        let supervisor = Supervisor::spawn(engine, events, pausing);
        let mut status = supervisor.watch();
        let refused = tokio::time::timeout(Duration::from_secs(20), status.wait_for(|s| s.restarts >= 1)).await;
        let refused = refused.expect("it was never refused").unwrap().clone();
        assert!(
            refused.runs == 0
                && refused.last_exit.as_ref().is_some_and(|why| why.contains("another jaw-engine is running on")),
            "{refused:?}"
        );
        first.end_all(End::InOrder).await;
        let running =
            tokio::time::timeout(Duration::from_secs(20), status.wait_for(|s| s.state == State::Running)).await;
        assert!(
            running.is_ok_and(|running| running.is_ok()),
            "it starts once the first has gone: {:?}",
            supervisor.status()
        );
        supervisor.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_lock_left_by_an_engine_that_died_is_free_again() {
        let Some(program) = built() else { return };
        let home = tempfile::tempdir().unwrap();
        let (first, instance, _events) = serving(&program, home.path()).await;
        let pid = first.running.0.lock().unwrap().live.values().next().unwrap().pid;
        // SAFETY: SIGKILL to the engine this test started, which its keeper hasn't seen exit.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        let why = instance.exited.await;
        assert!(why.contains("signal: 9 (SIGKILL)"), "{why}");
        assert!(home.path().join("session.lock").is_file(), "the file stays, and only its lock went");
        let (second, instance, _events) = serving(&program, home.path()).await;
        let pong = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await;
        assert!(matches!(pong, Ok(pb::reply::Kind::Pong(_))), "{pong:?}");
        second.end_all(End::InOrder).await;
    }

    /// `engine` started on a fake that never says hello, once it runs.
    async fn starting(engine: &Arc<GoEngine>, home: &Path) -> tokio::task::JoinHandle<Result<Instance>> {
        let (tx, rx) = mpsc::channel(8);
        let engine = engine.clone();
        let starting = tokio::spawn(async move {
            let _rx = rx;
            engine.start(tx).await
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while fake::running(home).is_none() {
            assert!(Instant::now() < deadline, "the fake engine never ran");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        starting
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ended_in_order_an_engine_has_its_standard_input_closed_and_is_waited_for_until_it_exits() {
        let home = tempfile::tempdir().unwrap();
        let said = home.path().join("said");
        // It reads what jawd writes until its standard input closes, then takes its time to exit.
        let then =
            format!("{}\ncat > /dev/null\nsleep 0.1\necho ended > '{}'", fake::hello(home.path()), said.display());
        let engine =
            GoEngine::new(fake::engine(home.path(), &then), home.path().to_path_buf()).with_pauses(fake::PATIENT);
        let (tx, _rx) = mpsc::channel(8);
        let instance = engine.start(tx).await.unwrap();
        assert_eq!(instance.hello.program, "a fake jaw-engine");
        let ended = engine.end_all(End::InOrder).await;
        assert_eq!(ended, ["jaw-engine exited once its standard input closed (exit status: 0)"]);
        assert!(said.is_file(), "it had exited by then");
        assert_eq!(fake::running(home.path()), None);
        assert_eq!(instance.exited.await, ended[0]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_that_doesnt_exit_once_its_standard_input_closes_is_sent_sigterm_then_sigkill_after_their_pauses()
    {
        for (then, how, after) in [
            (
                "exec sleep 600",
                "didn't exit once its standard input closed, so SIGTERM ended it (signal: 15 (SIGTERM))",
                QUITTING,
            ),
            (
                "trap '' TERM\nexec sleep 600",
                "didn't exit on SIGTERM either, so SIGKILL ended it (signal: 9 (SIGKILL))",
                QUITTING + TERMINATING,
            ),
        ] {
            let home = tempfile::tempdir().unwrap();
            let engine = Arc::new(GoEngine::new(fake::engine(home.path(), then), home.path().to_path_buf()));
            let starting = starting(&engine, home.path()).await;
            let took = Instant::now();
            let ending = engine.end_all(End::InOrder);
            let ended = tokio::time::timeout(QUITTING + TERMINATING + Duration::from_secs(10), ending)
                .await
                .expect("it never ended");
            let took = took.elapsed();
            assert_eq!(ended, [format!("jaw-engine {how}")]);
            assert!(took >= after, "{then}: ended after {took:?}");
            assert_eq!(fake::running(home.path()), None, "{then}");
            let given_up = starting.await.unwrap().err().map(|e| format!("{e:#}"));
            assert!(
                given_up.as_ref().is_some_and(|e| e.contains("before it said hello") && e.ends_with(how)),
                "{given_up:?}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_ended_takes_what_it_started_with_it() {
        let home = tempfile::tempdir().unwrap();
        let started = home.path().join("started.pid");
        // It starts a sleep of its own and becomes another, which only SIGTERM ends: a shell waiting
        // for its sleep can exit 0 as SIGTERM ends that, before SIGTERM ends the shell itself.
        let then = format!("sleep 600 &\necho $! > '{}'\nexec sleep 600", started.display());
        let engine = Arc::new(GoEngine::new(fake::engine(home.path(), &then), home.path().to_path_buf()));
        let _starting = starting(&engine, home.path()).await;
        while !std::fs::read_to_string(&started).is_ok_and(|pid| pid.ends_with('\n')) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let sleep: i32 = std::fs::read_to_string(&started).unwrap().trim().parse().unwrap();
        let ended = engine.end_all(End::InOrder).await;
        assert!(ended.len() == 1 && ended[0].contains("so SIGTERM ended it"), "{ended:?}");
        let deadline = Instant::now() + Duration::from_secs(2);
        // SAFETY: a signal of 0 is sent to nobody; kill only says whether the process exists.
        while unsafe { libc::kill(sleep, 0) } == 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // SAFETY: as above, and then SIGKILL, to the sleep this test's engine started, should it outlive the engine.
        let outlived = unsafe { libc::kill(sleep, 0) == 0 && libc::kill(sleep, libc::SIGKILL) == 0 };
        assert!(!outlived, "what the engine started outlived it");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ended_at_once_an_engine_is_killed_with_no_pause_even_while_it_is_ended_in_order() {
        let home = tempfile::tempdir().unwrap();
        let engine = Arc::new(GoEngine::new(
            fake::engine(home.path(), "trap '' TERM\nexec sleep 600"),
            home.path().to_path_buf(),
        ));
        let _starting = starting(&engine, home.path()).await;
        let in_order = tokio::spawn({
            let engine = engine.clone();
            async move { engine.end_all(End::InOrder).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ended = engine.end_all(End::AtOnce).await;
        assert!(ended.len() == 1 && ended[0].starts_with("jaw-engine was killed at once, with SIGKILL"), "{ended:?}");
        assert_eq!(in_order.await.unwrap(), ended, "and so it ended for the end in order too");
        assert_eq!(fake::running(home.path()), None);
    }

    /// Whether `pid` has exited, whether or not it has been waited for.
    fn gone(pid: i32) -> bool {
        // SAFETY: a signal of 0 is sent to nobody; kill only says whether the process exists.
        if unsafe { libc::kill(pid, 0) } != 0 {
            return true;
        }
        let ps = std::process::Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
        String::from_utf8_lossy(&ps.stdout).trim_start().starts_with('Z')
    }

    #[test]
    fn ended_at_once_an_engine_is_killed_with_what_it_started_though_its_keeper_never_runs_again() {
        let home = tempfile::tempdir().unwrap();
        let started = home.path().join("started.pid");
        // It ignores its standard input closing and SIGTERM, and waits for a sleep of its own.
        let then = format!("trap '' TERM\nsleep 600 &\necho $! > '{}'\nwait", started.display());
        let engine = Arc::new(GoEngine::new(fake::engine(home.path(), &then), home.path().to_path_buf()));
        // A runtime of this thread's own runs its tasks only while this thread waits on it.
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let (fake, sleep) = runtime.block_on(async {
            let _starting = starting(&engine, home.path()).await;
            while !std::fs::read_to_string(&started).is_ok_and(|pid| pid.ends_with('\n')) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let sleep: i32 = std::fs::read_to_string(&started).unwrap().trim().parse().unwrap();
            (fake::running(home.path()).unwrap(), sleep)
        });
        engine.running.close(End::AtOnce);
        // Its keeper never runs again, as a runtime about to go may never run it.
        drop(runtime);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !(gone(fake) && gone(sleep)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // SAFETY: SIGKILL to the sleep this test's engine started, should it outlive the engine.
        let outlived = !gone(sleep) && unsafe { libc::kill(sleep, libc::SIGKILL) } == 0;
        assert!(!outlived, "what the engine started outlived it");
        assert!(gone(fake), "the engine outlived its end");
    }

    #[tokio::test]
    async fn an_engine_that_exits_by_itself_just_as_it_is_to_be_killed_at_once_is_said_to_have() {
        let mut child = Command::new("sh").args(["-c", "exit 0"]).process_group(0).spawn().unwrap();
        let pid = child.id().unwrap() as i32;
        // Exited, and not yet waited for, as its keeper finds it when it's told first.
        while !gone(pid) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (_told, mut telling) = watch::channel(Some(End::AtOnce));
        let why = ended(&mut child, &mut telling, Pauses::default()).await;
        assert_eq!(why, "jaw-engine exited by itself just as it was to be killed at once (exit status: 0)");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_that_exits_by_itself_just_as_its_end_in_order_sends_a_signal_is_said_to_have() {
        let home = tempfile::tempdir().unwrap();
        let ready = home.path().join("ready");
        // It exits 0 as SIGTERM comes, as one exiting by itself just then would, once it's ready to.
        let then = format!("trap 'exit 0' TERM\necho > '{}'\nwhile :; do sleep 0.05; done", ready.display());
        let mut child = Command::new("sh").args(["-c", &then]).process_group(0).spawn().unwrap();
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (_told, mut telling) = watch::channel(Some(End::InOrder));
        let pauses = Pauses { quitting: Duration::from_millis(100), terminating: fake::PATIENT.quitting };
        let why = ended(&mut child, &mut telling, pauses).await;
        assert_eq!(why, "jaw-engine exited by itself just as SIGTERM was sent (exit status: 0)");
        // No process stays to see SIGKILL come, so only a status can say it exited just then.
        let status = |raw| Ok(ExitStatus::from_raw(raw));
        let ended = [0, libc::SIGKILL, libc::SIGTERM].map(|raw| after(Sent::Kill, status(raw)));
        assert_eq!(
            ended,
            [
                "jaw-engine exited by itself just as SIGKILL was sent (exit status: 0)",
                "jaw-engine didn't exit on SIGTERM either, so SIGKILL ended it (signal: 9 (SIGKILL))",
                "jaw-engine didn't exit once its standard input closed, so SIGTERM ended it (signal: 15 (SIGTERM))",
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_instance_ends_once_its_engine_has_exited_not_once_its_standard_output_closes() {
        let home = tempfile::tempdir().unwrap();
        let said = home.path().join("said");
        // Once it has said hello it closes its standard output, and exits only a moment later.
        let then = format!("{}\nexec >&-\nsleep 0.3\necho exited > '{}'", fake::hello(home.path()), said.display());
        let engine = GoEngine::new(fake::engine(home.path(), &then), home.path().to_path_buf());
        let (tx, _rx) = mpsc::channel(8);
        let instance = engine.start(tx).await.unwrap();
        let why = instance.exited.await;
        assert!(said.is_file(), "its end came before it had exited: {why}");
        assert_eq!(why, "jaw-engine exited (exit status: 0)");
        assert_eq!(fake::running(home.path()), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn once_ended_it_starts_no_other_engine() {
        let home = tempfile::tempdir().unwrap();
        let engine = GoEngine::new(fake::engine(home.path(), "exec sleep 600"), home.path().to_path_buf());
        assert_eq!(engine.end_all(End::InOrder).await, Vec::<String>::new(), "none was running");
        let (tx, _rx) = mpsc::channel(8);
        let refused = engine.start(tx).await.err().map(|e| format!("{e:#}"));
        assert!(refused.as_ref().is_some_and(|e| e.contains("so it starts no other")), "{refused:?}");
        assert!(!home.path().join("engine.pid").exists(), "nothing was started");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_start_whose_engine_wont_do_has_it_gone_before_it_says_so() {
        let home = tempfile::tempdir().unwrap();
        let ack = pb::Frame { id: 0, body: Some(pb::frame::Body::Ack(pb::Ack { seq: 1 })) };
        let then = format!("{}\nexec sleep 600", fake::frame(home.path(), "ack", &ack));
        let engine = GoEngine::new(fake::engine(home.path(), &then), home.path().to_path_buf());
        let (tx, _rx) = mpsc::channel(8);
        let refused = engine.start(tx).await.err().map(|e| format!("{e:#}"));
        assert!(
            refused
                .as_ref()
                .is_some_and(|e| e.contains("first frame wasn't a hello") && e.contains("so SIGTERM ended it")),
            "{refused:?}"
        );
        assert_eq!(fake::running(home.path()), None, "gone before the start said it failed");
    }
}

/// Fake jaw-engines for tests: shell scripts that write down their PID and then do as they're told.
#[cfg(test)]
pub(crate) mod fake {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use prost::Message as _;

    use super::{PROTOCOL, Pauses, TERMINATING, pb};

    /// Pauses that an engine which exits once its standard input closes always exits within, however
    /// loaded the Mac running the tests, so a test of an end in order never sees SIGTERM's instead.
    pub(crate) const PATIENT: Pauses = Pauses { quitting: Duration::from_secs(10), terminating: TERMINATING };

    /// A jaw-engine in `home` that writes its PID to engine.pid there, then runs `then`.
    pub(crate) fn engine(home: &Path, then: &str) -> PathBuf {
        let program = home.join("fake-engine");
        let pid = home.join("engine.pid");
        std::fs::write(&program, format!("#!/bin/sh\necho $$ > '{}'\n{then}\n", pid.display())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    /// A line of shell that sends `frame` to jawd, kept in `home` as `name`.
    pub(crate) fn frame(home: &Path, name: &str, frame: &pb::Frame) -> String {
        let data = frame.encode_to_vec();
        let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
        bytes.extend(data);
        let path = home.join(name);
        std::fs::write(&path, bytes).unwrap();
        format!("cat '{}'", path.display())
    }

    /// A line of shell that says a hello jawd takes.
    pub(crate) fn hello(home: &Path) -> String {
        let hello = pb::Hello {
            protocol: PROTOCOL,
            program: "a fake jaw-engine".into(),
            whatsmeow: wa_proto::whatsmeow().into(),
        };
        frame(home, "hello", &pb::Frame { id: 0, body: Some(pb::frame::Body::Hello(hello)) })
    }

    /// The PID the fake engine in `home` wrote down, while a process has it.
    pub(crate) fn running(home: &Path) -> Option<i32> {
        let pid: i32 = std::fs::read_to_string(home.join("engine.pid")).ok()?.trim().parse().ok()?;
        // SAFETY: a signal of 0 is sent to nobody; kill only says whether the process exists.
        (unsafe { libc::kill(pid, 0) } == 0).then_some(pid)
    }
}
