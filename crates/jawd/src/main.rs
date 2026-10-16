//! jawd, jaw's daemon: it keeps every chat and message, does the work interfaces would otherwise
//! repeat, and serves them on a Unix socket. docs/plans/2026-10-16-jaw-design.md is its design.

mod engine;
mod sock;
mod state;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tokio::sync::mpsc;

use crate::engine::demo::DemoEngine;
use crate::engine::go::GoEngine;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::engine::{Engine, Incoming, pb};

#[derive(Parser)]
#[command(
    name = "jawd",
    version,
    about = "jaw's daemon: it keeps your WhatsApp and does the work for jaw's interfaces"
)]
struct Cli {
    /// The engine jawd reaches WhatsApp through
    #[arg(long, value_enum, default_value_t = EngineKind::Go, global = true)]
    engine: EngineKind,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineKind {
    /// jaw-engine, on whatsmeow
    Go,
    /// A made-up account, for trying jaw out and for tests
    Demo,
}

#[derive(Subcommand)]
enum Command {
    /// What jawd is connected to, how its engine is, and what's missing
    Status,
    /// Measures the engine link: made-up messages sent as real ones are, each waiting for jawd's ack
    Bench {
        #[arg(long, default_value_t = 20_000)]
        count: u32,
        /// Each message's size in bytes
        #[arg(long, default_value_t = 600)]
        size: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = jaw_proto::local::home();
    match cli.command {
        Some(Command::Status) => status(&home),
        Some(Command::Bench { count, size }) => bench(cli.engine, count, size),
        None => run(&home, cli.engine),
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build()?)
}

/// The engine to run, or its kind and why it can't run.
fn engine(kind: EngineKind, home: &Path) -> Result<Arc<dyn Engine>, (&'static str, String)> {
    match kind {
        EngineKind::Demo => Ok(Arc::new(DemoEngine::new())),
        EngineKind::Go => match GoEngine::find() {
            Some(program) => Ok(Arc::new(GoEngine::new(program, home.to_path_buf()))),
            None => Err(("go", "jaw-engine isn't beside jawd or on PATH; make build builds it".into())),
        },
    }
}

fn run(home: &Path, kind: EngineKind) -> Result<()> {
    std::fs::create_dir_all(home).with_context(|| format!("can't make {}", home.display()))?;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))?;
    runtime()?.block_on(async {
        let socket = jaw_proto::local::socket(home);
        let listener = sock::listen(&socket)?;
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = engine(kind, home).map(|engine| Supervisor::spawn(engine, events, Backoff::default()));
        tokio::spawn(hold(incoming));
        let state = Arc::new(state::State::new(home, supervisor)?);
        let outcome = tokio::select! {
            served = sock::serve(state, listener) => served,
            _ = shutdown() => Ok(()),
        };
        let _ = std::fs::remove_file(&socket);
        outcome
    })
}

/// Until jawd has its store (M1), it keeps no messages. It acks only bench events, and leaves a
/// real message unacked, so the engine waits and WhatsApp keeps it to send again.
async fn hold(mut incoming: mpsc::Receiver<Incoming>) {
    let mut held = false;
    while let Some(Incoming { event, ack }) = incoming.recv().await {
        match (event.kind, ack) {
            (Some(pb::event::Kind::Bench(_)), Some(ack)) => ack.send(),
            (Some(pb::event::Kind::Message(_)), Some(_)) if !held => {
                held = true;
                eprintln!(
                    "jawd: a message arrived, but jawd can't store messages yet, so it's left for WhatsApp to send again"
                );
            }
            _ => {}
        }
    }
}

/// Waits for Ctrl-C or SIGTERM, which launchd and systemd send to stop jawd.
async fn shutdown() {
    let mut term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("a SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

fn status(home: &Path) -> Result<()> {
    let socket = jaw_proto::local::socket(home);
    let (mut connection, _) = jaw_client::Connection::open(&socket, "jawd status", env!("CARGO_PKG_VERSION"))?;
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", serde_json::Value::Null)?)?;
    println!("{}", jaw_client::describe(&status));
    Ok(())
}

/// Runs an engine on a folder of its own, so a bench never touches the real session, and acks each
/// made-up message as it arrives: the link alone, before any database.
fn bench(kind: EngineKind, count: u32, size: u32) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(async {
        let engine = engine(kind, &scratch).map_err(|(_, why)| anyhow::anyhow!(why))?;
        let (events, mut incoming) = mpsc::channel(1024);
        let instance = engine.start(events).await?;
        tokio::spawn(async move {
            while let Some(Incoming { ack, .. }) = incoming.recv().await {
                if let Some(ack) = ack {
                    ack.send();
                }
            }
        });
        let reply = instance.commands.call(pb::command::Kind::BenchRun(pb::BenchRun { count, size })).await?;
        let pb::reply::Kind::BenchResult(result) = reply else { anyhow::bail!("the engine answered the bench with {reply:?}") };
        let seconds = result.elapsed_us as f64 / 1e6;
        println!(
            "{} round trips in {seconds:.2} s, {} a second: p50 {} µs, p99 {} µs, max {} µs ({} engine, {size}-byte messages, acked as they arrived)",
            result.count,
            (result.count as f64 / seconds.max(1e-9)).round(),
            result.p50_us,
            result.p99_us,
            result.max_us,
            engine.kind(),
        );
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}
