//! jawd, jaw's daemon: it keeps every chat and message, does the work interfaces would otherwise
//! repeat, and serves them on a Unix socket. docs/plans/2026-10-16-jaw-design.md is its design.

mod attention;
mod convert;
mod engine;
mod feed;
mod find;
mod notify;
mod send;
mod sock;
mod state;
mod store;
mod sync;
mod work;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tokio::signal::unix::{Signal, SignalKind, signal};
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
    /// Sends no notifications; JAW_NOTIFY=off does the same
    #[arg(long)]
    no_notify: bool,
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
        /// Ack each message only once it's committed to a jaw.db's inbox, as a real one is
        #[arg(long)]
        commit: bool,
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
    /// jaw.db: a made-up account built through the writer, then its pages, chat list and search timed
    Store {
        #[arg(long, default_value_t = 1_000_000)]
        messages: u32,
        #[arg(long, default_value_t = 2_000)]
        chats: u32,
        /// How many times each query is timed
        #[arg(long, default_value_t = 300)]
        runs: u32,
    },
    /// The palette's find: a made-up account with named people and chats, then keystrokes timed
    Find {
        #[arg(long, default_value_t = 1_000_000)]
        messages: u32,
        #[arg(long, default_value_t = 2_000)]
        chats: u32,
        #[arg(long, default_value_t = 5_000)]
        people: u32,
        /// How many times each keystroke is timed
        #[arg(long, default_value_t = 50)]
        runs: u32,
    },
    /// Windows: a client on the socket watching the chat list and a chat while messages are written
    Feed {
        /// Messages written a second
        #[arg(long, default_value_t = 2_000)]
        rate: u32,
        #[arg(long, default_value_t = 5)]
        seconds: u32,
        /// The messages window's rows
        #[arg(long, default_value_t = 100)]
        window: u32,
        #[arg(long, default_value_t = 300)]
        chats: u32,
        /// Messages written one at a time after the burst, 200 ms apart, and as many again while
        /// far chats change
        #[arg(long, default_value_t = 50)]
        lone: u32,
        /// Interfaces watching the same windows
        #[arg(long, default_value_t = 1)]
        interfaces: u32,
        /// Chats far down the list whose counts and names change while lone messages are timed again
        #[arg(long, default_value_t = 0)]
        far: u32,
    },
    /// Sending: send.text answered with its synced commit, and sends there and back through the demo engine
    Send {
        #[arg(long, default_value_t = 1_000)]
        count: u32,
        /// Sends timed from asking to sent and to read
        #[arg(long, default_value_t = 200)]
        trips: u32,
        #[arg(long, default_value_t = 50)]
        chats: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = jaw_proto::local::home();
    match cli.command {
        Some(Command::Status) => status(&home),
        Some(Command::Bench(Bench::Link { count, size, commit })) => bench_link(cli.engine, count, size, commit),
        Some(Command::Bench(Bench::Lanes { background, interactive })) => bench_lanes(background, interactive),
        Some(Command::Bench(Bench::Store { messages, chats, runs })) => bench_store(messages, chats, runs),
        Some(Command::Bench(Bench::Find { messages, chats, people, runs })) => {
            bench_find(find::bench::Plan { messages, chats, people, runs })
        }
        Some(Command::Bench(Bench::Feed { rate, seconds, window, chats, lone, interfaces, far })) => {
            bench_feed(feed::bench::Plan { rate, seconds, window, chats, lone, interfaces, far })
        }
        Some(Command::Bench(Bench::Send { count, trips, chats })) => {
            bench_send(send::bench::Plan { count, trips, chats })
        }
        None => run(&home, cli.engine, cli.no_notify),
    }
}

/// Times sending on a jaw.db in a folder of its own, removed afterwards.
fn bench_send(plan: send::bench::Plan) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-send-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(send::bench::run(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// Builds a jaw.db in a folder of its own, removed afterwards, and times the palette's find on it.
fn bench_find(plan: find::bench::Plan) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-find-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(find::bench::run(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// Serves a made-up jaw.db in a folder of its own, removed afterwards.
fn bench_feed(plan: feed::bench::Plan) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-feed-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(feed::bench::run(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// Builds and times a jaw.db in a folder of its own, removed afterwards.
fn bench_store(messages: u32, chats: u32, runs: u32) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-store-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(store::bench::run(&scratch, store::bench::Plan { messages, chats, runs }));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
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

fn run(home: &Path, kind: EngineKind, no_notify: bool) -> Result<()> {
    std::fs::create_dir_all(home).with_context(|| format!("can't make {}", home.display()))?;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))?;
    let runtime = runtime()?;
    let outcome = runtime.block_on(async {
        let mut signals = Signals::new()?;
        let socket = jaw_proto::local::socket(home);
        let (lock, listener) = open(home, &socket)?;
        let started = tokio::select! {
            started = start_on(home, &socket, lock, kind, no_notify) => started?,
            signal = signals.next() => {
                eprintln!("jawd: {signal} while starting, so it stops");
                let _ = std::fs::remove_file(&socket);
                return Ok(());
            }
        };
        let served = tokio::select! {
            served = sock::serve(started.state.clone(), listener) => served,
            signal = signals.next() => {
                eprintln!("jawd: {signal}, so it stops");
                Ok(())
            }
        };
        let began = Instant::now();
        tokio::select! {
            stopped = tokio::time::timeout(STOPPING, stop(started, &socket)) => match stopped {
                Ok(()) => eprintln!("jawd: stopped in {} ms", began.elapsed().as_millis()),
                Err(_) => eprintln!("jawd: didn't stop within {} s, so it stops now", STOPPING.as_secs()),
            },
            signal = signals.next() => {
                eprintln!("jawd: {signal} again, so it stops at once");
                std::process::exit(1);
            }
        }
        served
    });
    // Whatever's still running was told to stop; what doesn't by now is abandoned.
    runtime.shutdown_timeout(Duration::from_millis(100));
    outcome
}

/// How long jawd has to stop once asked; a second signal stops it at once.
const STOPPING: Duration = Duration::from_secs(3);

/// jawd running: what it serves, and what stopping it waits for.
struct Started {
    state: Arc<state::State>,
    notifications: Option<tokio::task::JoinHandle<()>>,
    /// jaw.lock, held while jawd runs.
    _lock: std::fs::File,
}

/// Takes jaw.lock in `home`, or says another jawd has it, so two never open jaw.db together: two
/// would race the backup an upgrade takes, and each other's writes.
fn lock(home: &Path) -> Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let path = home.join("jaw.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("can't open {}", path.display()))?;
    // SAFETY: the descriptor is the open file's, which outlives the call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let e = std::io::Error::last_os_error();
        if e.kind() == std::io::ErrorKind::WouldBlock {
            anyhow::bail!("another jawd is running on {}", home.display());
        }
        return Err(e).with_context(|| format!("can't lock {}", path.display()));
    }
    Ok(file)
}

/// Takes jaw.lock, then listens on the socket, so a jawd refused because another holds the lock
/// never touches the socket, even while the other is stopping and has removed its own.
fn open(home: &Path, socket: &Path) -> Result<(std::fs::File, tokio::net::UnixListener)> {
    let lock = lock(home)?;
    Ok((lock, sock::listen(socket)?))
}

/// Starts jawd listening on `socket`, removing the socket if it can't start, so a jawd that fails
/// leaves nothing an interface would try to reach.
async fn start_on(
    home: &Path,
    socket: &Path,
    lock: std::fs::File,
    kind: EngineKind,
    no_notify: bool,
) -> Result<Started> {
    let started = start(home, lock, kind, no_notify).await;
    if started.is_err() {
        let _ = std::fs::remove_file(socket);
    }
    started
}

async fn start(home: &Path, lock: std::fs::File, kind: EngineKind, no_notify: bool) -> Result<Started> {
    let store = store::Store::open(home)?;
    std::fs::create_dir_all(home.join("spool"))?;
    sync::running(&store).await?;
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = engine(kind, home).map(|engine| Supervisor::spawn(engine, events, Backoff::default()));
    let live: sync::Shared = Arc::default();
    let (pushes, _) = tokio::sync::broadcast::channel(256);
    tokio::spawn(trouble(store.trouble(), pushes.clone()));
    let engine = supervisor.as_ref().ok().cloned();
    tokio::spawn(sync::run(incoming, store.clone(), live.clone(), pushes.clone(), engine.clone(), home.to_path_buf()));
    let sending = send::Sending::start(store.clone(), engine, live.clone(), &pushes, send::Timing::default()).await?;
    let pools: Vec<Arc<dyn work::Report>> =
        vec![Arc::new(store.clone()), Arc::new(store.reads().clone()), sending.lane()];
    let finder = find::Finder::start(store.clone()).await?;
    let feed = feed::Feed::start(store.clone(), feed::catching_up(live.clone()), feed::Timing::default());
    let notifier = notify::chosen(no_notify);
    let says = match &notifier {
        Ok(notifier) => notifier.name().to_string(),
        Err(why) => why.clone(),
    };
    eprintln!("jawd: notifications {says}");
    let state = state::State::new(home, supervisor, pools, live.clone(), Some(store.clone()), pushes.clone())?
        .with_feed(feed)
        .with_sending(sending)
        .with_notifications(says);
    let state = Arc::new(state);
    let _ = state.finder.set(finder);
    let notifications = notifier.ok().map(|notifier| {
        notify::start(
            store,
            state.attention().clone(),
            feed::catching_up(live),
            pushes.subscribe(),
            Arc::from(notifier),
            notify::Timing::default(),
        )
    });
    Ok(Started { state, notifications, _lock: lock })
}

/// Stops jawd in the order that keeps its data whole: first it stops taking work, so no interface
/// is answered for something that won't be done; then whatsmeow disconnects, the engine is ended
/// and started no more, whatever its supervisor was waiting for, and anything it handed over that
/// jawd hadn't acked comes again from WhatsApp; only then does jaw.db note a clean stop, and the
/// store closes last, once jawd's tasks are dropped.
async fn stop(Started { state, notifications, .. }: Started, socket: &Path) {
    state.stop();
    let _ = std::fs::remove_file(socket);
    if let Some(notifications) = notifications {
        notifications.abort();
    }
    match (state.store(), state.engine()) {
        (Some(store), engine) => sync::stop(store, engine.ok()).await,
        (None, Ok(engine)) => engine.stop().await,
        (None, Err(_)) => {}
    }
    if let Some(store) = state.store() {
        match tokio::time::timeout(EMPTYING, store.truncate()).await {
            Ok(Ok(())) => {
                if let Err(e) = store.remove_old_backups() {
                    eprintln!("jawd: can't remove the old backups taken before upgrading jaw.db: {e:#}");
                }
            }
            Ok(Err(e)) => eprintln!("jawd: can't empty the WAL at the stop: {e:#}"),
            Err(_) => eprintln!("jawd: the WAL wasn't emptied in time at the stop"),
        }
    }
}

/// How long a clean stop waits for the WAL to be copied into jaw.db and emptied, within STOPPING.
const EMPTYING: Duration = Duration::from_secs(2);

/// Tells interfaces when jaw.db can't be written, and when it can again.
async fn trouble(
    mut trouble: tokio::sync::watch::Receiver<Option<String>>,
    pushes: tokio::sync::broadcast::Sender<jaw_proto::Push>,
) {
    use jaw_proto::sync::{Notice, Tone};
    while trouble.changed().await.is_ok() {
        let notice = match trouble.borrow_and_update().clone() {
            Some(why) => Notice { text: why, tone: Tone::Error },
            None => Notice { text: "jaw.db can be written again".into(), tone: Tone::Info },
        };
        let params = serde_json::to_value(notice).expect("a notice serializes");
        let _ = pushes.send(jaw_proto::Push { method: "notice".into(), params });
    }
}

/// Ctrl-C and SIGTERM, which launchd and systemd send to stop jawd. Each can come more than once.
struct Signals {
    interrupt: Signal,
    terminate: Signal,
}

impl Signals {
    fn new() -> Result<Signals> {
        Ok(Signals {
            interrupt: signal(SignalKind::interrupt()).context("can't listen for Ctrl-C")?,
            terminate: signal(SignalKind::terminate()).context("can't listen for SIGTERM")?,
        })
    }

    /// Waits for the next, and says which it was.
    async fn next(&mut self) -> &'static str {
        tokio::select! {
            _ = self.interrupt.recv() => "Ctrl-C",
            _ = self.terminate.recv() => "SIGTERM",
        }
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
fn bench_link(kind: EngineKind, count: u32, size: u32, commit: bool) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(async {
        let engine = engine(kind, &scratch).map_err(|(_, why)| anyhow::anyhow!(why))?;
        let store = if commit { Some(store::Store::open(&scratch)?) } else { None };
        let (events, mut incoming) = mpsc::channel(1024);
        let instance = engine.start(events).await?;
        tokio::spawn(async move {
            while let Some(Incoming { event, ack }) = incoming.recv().await {
                match (&store, event.kind) {
                    (Some(store), Some(pb::event::Kind::Bench(bench))) => store.inbox("bench", 0, bench.payload, ack),
                    _ => {
                        if let Some(ack) = ack {
                            ack.send();
                        }
                    }
                }
            }
        });
        let reply = instance.commands.call(pb::command::Kind::BenchRun(pb::BenchRun { count, size })).await?;
        let pb::reply::Kind::BenchResult(result) = reply else { anyhow::bail!("the engine answered the bench with {reply:?}") };
        let seconds = result.elapsed_us as f64 / 1e6;
        println!(
            "{} round trips in {seconds:.2} s, {} a second: p50 {} µs, p99 {} µs, max {} µs ({} engine, {size}-byte messages, {})",
            result.count,
            (result.count as f64 / seconds.max(1e-9)).round(),
            result.p50_us,
            result.p99_us,
            result.max_us,
            engine.kind(),
            if commit { "each committed to jaw.db's inbox before its ack" } else { "acked as they arrived" },
        );
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// A day's worth of work at once: a backlog of made-up downloads (1 to 5 ms each), interactive ones
/// arriving every 3 ms while it runs, transcodes, and sends across 50 chats; then the pools' table.
fn bench_lanes(background: u32, interactive: u32) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::store::{Change, Lane, Message};

    #[tokio::test(flavor = "multi_thread")]
    async fn jawd_stops_well_within_its_bound_while_messages_pour_into_a_watched_chat() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let started = start(home.path(), lock, EngineKind::Demo, true).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let store = started.state.store().unwrap().clone();
        let chat = "120363012345678901@g.us";
        let pouring = Arc::new(AtomicBool::new(true));
        let writer = {
            let (store, pouring) = (store.clone(), pouring.clone());
            tokio::spawn(async move {
                let mut n = 0u64;
                let mut tick = tokio::time::interval(Duration::from_millis(1));
                while pouring.load(Ordering::Relaxed) {
                    tick.tick().await;
                    let changes = (0..2)
                        .map(|_| {
                            n += 1;
                            let text = format!("message {n}");
                            Change::Message(Message {
                                chat: chat.into(),
                                id: format!("m{n}"),
                                at: 1_792_000_000_000 + n as i64,
                                kind: "text",
                                text: Some(text.clone()),
                                summary: text,
                                live: true,
                                ..Message::default()
                            })
                        })
                        .collect();
                    if store.apply(Lane::Live, changes).await.is_err() {
                        return n;
                    }
                }
                n
            })
        };
        let path = socket.clone();
        let reading = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            let watch = serde_json::json!({ "chat": chat, "around": "newest", "before": 100 });
            connection.call("messages.watch", watch).unwrap();
            let mut pushes = 0;
            while connection.push().is_ok() {
                pushes += 1;
            }
            pushes
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let began = Instant::now();
        stop(started, &socket).await;
        let took = began.elapsed();
        pouring.store(false, Ordering::Relaxed);
        let (written, pushes) = (writer.await.unwrap(), reading.await.unwrap());
        eprintln!("stopped in {} ms after {written} messages and {pushes} pushes", took.as_millis());
        assert!(pushes > 0, "the window was busy");
        assert!(took < STOPPING / 3, "stopping took {took:?}");
        assert!(!socket.exists(), "the socket is removed");
    }

    #[tokio::test]
    async fn a_jawd_refused_while_another_stops_leaves_no_socket_behind() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        // The other is stopping: its socket is gone, and it still holds jaw.lock.
        let other = lock(home.path()).unwrap();
        let refused = open(home.path(), &socket).unwrap_err().to_string();
        assert!(refused.contains("another jawd is running"), "{refused}");
        assert!(!socket.exists(), "the refused jawd didn't bind the socket");
        drop(other);
        let (_lock, _listener) = open(home.path(), &socket).unwrap();
        assert!(socket.exists());
    }

    #[tokio::test]
    async fn a_jawd_that_fails_to_start_leaves_no_socket_behind() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("jaw.db"), b"not a database, so jaw.db can't be opened").unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, _listener) = open(home.path(), &socket).unwrap();
        assert!(socket.exists());
        let failed = start_on(home.path(), &socket, lock, EngineKind::Demo, true).await;
        assert!(failed.is_err(), "jaw.db isn't one");
        assert!(!socket.exists(), "the socket went with the start that failed");
    }

    #[test]
    fn a_second_jawd_on_the_same_folder_is_refused_until_the_first_has_gone() {
        let home = tempfile::tempdir().unwrap();
        let first = lock(home.path()).unwrap();
        let refused = lock(home.path()).unwrap_err().to_string();
        assert!(refused.contains("another jawd is running"), "{refused}");
        drop(first);
        lock(home.path()).unwrap();
    }
}
