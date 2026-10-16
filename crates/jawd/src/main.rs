//! jawd, jaw's daemon: it keeps every chat and message, does the work interfaces would otherwise
//! repeat, and serves them on a Unix socket. docs/plans/2026-10-16-jaw-design.md is its design.

mod engine;
mod sock;
mod state;
mod work;

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
    /// Measures a part of jawd on made-up work
    #[command(subcommand)]
    Bench(Bench),
}

#[derive(Subcommand)]
enum Bench {
    /// The engine link: made-up messages sent as real ones are, each waiting for jawd's ack
    Link {
        #[arg(long, default_value_t = 20_000)]
        count: u32,
        /// Each message's size in bytes
        #[arg(long, default_value_t = 600)]
        size: u32,
    },
    /// The work lanes: a backlog of background jobs, interactive ones arriving through it, and sends
    Lanes {
        /// Background jobs queued at the start
        #[arg(long, default_value_t = 5_000)]
        background: u32,
        /// Interactive jobs, one every few milliseconds while the backlog runs
        #[arg(long, default_value_t = 300)]
        interactive: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = jaw_proto::local::home();
    match cli.command {
        Some(Command::Status) => status(&home),
        Some(Command::Bench(Bench::Link { count, size })) => bench_link(cli.engine, count, size),
        Some(Command::Bench(Bench::Lanes { background, interactive })) => bench_lanes(background, interactive),
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
        let state = Arc::new(state::State::new(home, supervisor, Vec::new())?);
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
fn bench_link(kind: EngineKind, count: u32, size: u32) -> Result<()> {
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

/// A day's worth of work at once: a backlog of made-up downloads (1 to 5 ms each), interactive ones
/// arriving every 3 ms while it runs, transcodes, and sends across 50 chats; then the pools' table.
fn bench_lanes(background: u32, interactive: u32) -> Result<()> {
    use std::time::Duration;

    use crate::work::{Pool, Priority, Report, Serial};

    runtime()?.block_on(async {
        let downloads: Arc<Pool<u32>> = Pool::new("downloads", 6, 1_000);
        let transcodes: Arc<Pool<u32>> = Pool::new("transcodes", 2, 100);
        let sends = Serial::new("sends", 16);
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let started = std::time::Instant::now();
        let backlog = {
            let downloads = downloads.clone();
            let delays: Vec<u64> = (0..background).map(|_| 1 + next() % 5).collect();
            tokio::spawn(async move {
                let mut handles = Vec::new();
                for (n, ms) in delays.into_iter().enumerate() {
                    let job = async move {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                        n as u32
                    };
                    handles.push(downloads.submit(format!("prefetch {n}"), Priority::Background, job).await);
                }
                for handle in handles {
                    handle.result().await;
                }
            })
        };
        for n in 0..40u32 {
            let job = async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                n
            };
            drop(transcodes.submit(format!("video {n}"), Priority::Background, job).await);
        }
        for n in 0..2_000u32 {
            sends.push(format!("chat {}", n % 50), async { tokio::time::sleep(Duration::from_micros(500)).await });
        }
        let mut shown = Vec::new();
        let (mut promoted, mut cancelled, mut refused) = (0, 0, 0);
        for n in 0..interactive {
            tokio::time::sleep(Duration::from_millis(3)).await;
            let job = async move {
                tokio::time::sleep(Duration::from_millis(2)).await;
                n
            };
            shown.push(downloads.submit(format!("opened {n}"), Priority::Interactive, job).await);
            // Scrolling brings queued prefetches into view, and some are no longer wanted.
            let queued = format!("prefetch {}", next() % u64::from(background.max(1)));
            if n % 3 == 0 && downloads.promote(&queued) {
                promoted += 1;
            } else if n % 25 == 0 && downloads.cancel(&queued) {
                cancelled += 1;
            }
            // Thumbnails for a list that scrolled past are dropped when the queue has no room.
            let thumbnail = async move { n };
            if downloads.try_submit(format!("thumbnail {n}"), Priority::Background, thumbnail).is_err() {
                refused += 1;
            }
        }
        for handle in shown {
            handle.result().await;
        }
        backlog.await?;
        println!(
            "{background} background downloads, {interactive} interactive ones arriving through them ({promoted} prefetches promoted, {cancelled} cancelled, {refused} thumbnails refused while the queue was full), 40 transcodes and 2,000 sends in 50 chats, in {:.1} s:\n",
            started.elapsed().as_secs_f64()
        );
        let reports = vec![downloads.report(), transcodes.report(), sends.report()];
        println!("{}", jaw_client::pools(&reports));
        Ok(())
    })
}
