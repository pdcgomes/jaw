//! `GoEngine`: jaw-engine, the Go program on whatsmeow, run as jawd's child on its standard input
//! and output. Its standard error goes into jawd's.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

use super::{Ack, BoxFuture, Commands, Engine, Incoming, Instance, Outgoing, PROTOCOL, Pending, frame, pb};

/// The Go runtime's soft memory limit and garbage collection target for jaw-engine, unless jawd's
/// own environment sets them: the engine has 80 MB steady and 200 MB during a full history sync,
/// and the limit leaves room beside Go's heap for SQLite's memory, which Go doesn't count.
const GO_MEMORY: [(&str, &str); 2] = [("GOMEMLIMIT", "160MiB"), ("GOGC", "50")];

pub struct GoEngine {
    program: PathBuf,
    home: PathBuf,
    log: String,
}

impl GoEngine {
    pub fn new(program: PathBuf, home: PathBuf) -> GoEngine {
        GoEngine { program, home, log: "info".into() }
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

    /// Where jaw-engine is: `JAW_ENGINE` if set, beside jawd as `make install` puts it, in
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
            let mut child =
                self.command().spawn().with_context(|| format!("can't start {}", self.program.display()))?;
            let stdin = child.stdin.take().expect("a piped stdin");
            let stdout = child.stdout.take().expect("a piped stdout");
            let stderr = child.stderr.take().expect("a piped stderr");
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("engine: {line}");
                }
            });
            let mut reader = BufReader::with_capacity(64 << 10, stdout);
            let mut writer = BufWriter::with_capacity(64 << 10, stdin);
            let hello = tokio::time::timeout(Duration::from_secs(10), handshake(&mut reader, &mut writer))
                .await
                .context("jaw-engine didn't say hello within 10 seconds")??;
            let (tx, rx) = mpsc::unbounded_channel();
            let commands = Commands { tx };
            let pending = Pending::default();
            tokio::spawn(write(writer, rx, pending.clone()));
            let reading = tokio::spawn(read(reader, events, commands.clone(), pending));
            let exited: BoxFuture<'static, String> = Box::pin(async move {
                let status = child.wait().await;
                let _ = reading.await;
                match status {
                    Ok(status) => format!("jaw-engine exited ({status})"),
                    Err(e) => format!("can't wait for jaw-engine: {e}"),
                }
            });
            Ok(Instance { hello, commands, exited })
        })
    }
}

/// Reads the engine's hello, checks it, and answers with jawd's.
async fn handshake(reader: &mut BufReader<ChildStdout>, writer: &mut BufWriter<ChildStdin>) -> Result<pb::Hello> {
    let first = frame::read(reader).await?.context("jaw-engine closed the link before saying hello")?;
    let Some(pb::frame::Body::Hello(hello)) = first.body else {
        bail!("jaw-engine's first frame wasn't a hello");
    };
    if hello.protocol != PROTOCOL {
        bail!(
            "jaw-engine speaks link protocol {} and jawd {PROTOCOL}: rebuild them together with make build",
            hello.protocol
        );
    }
    if hello.whatsmeow != wa_proto::whatsmeow() {
        bail!(
            "jaw-engine was built with whatsmeow {} but jawd reads WhatsApp's protobufs from {}: run make sync-protos and make build",
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
/// has gone, it closes the engine's standard input, which tells the engine to exit.
async fn write(mut writer: BufWriter<ChildStdin>, mut outgoing: mpsc::UnboundedReceiver<Outgoing>, pending: Pending) {
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

    use super::*;

    /// The engine `make engine` builds. Without it the test says so and passes, except in CI,
    /// where `make test` builds it first.
    fn built() -> Option<PathBuf> {
        let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/engine/jaw-engine");
        if program.is_file() {
            return Some(program);
        }
        assert!(std::env::var_os("CI").is_none(), "make test builds jaw-engine before the Rust tests");
        eprintln!("skipped: {} isn't built; make engine builds it", program.display());
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
}
