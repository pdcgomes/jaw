//! jawd, jaw's daemon: it keeps every chat and message, does the work interfaces would otherwise
//! repeat, and serves them on a Unix socket. docs/plans/2026-10-16-jaw-design.md is its design.

mod attention;
mod convert;
mod demo;
mod engine;
mod feed;
mod find;
#[cfg(target_os = "macos")]
mod home;
mod layout;
mod media;
mod notify;
mod organise;
mod presence;
mod send;
mod settings;
mod sock;
mod state;
mod store;
mod sync;
mod voice;
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
use crate::engine::failing::{Failing, Failure};
use crate::engine::go::GoEngine;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::engine::{Engine, Incoming, pb};

#[derive(Parser)]
#[command(
    name = "jawd",
    version = jaw_version::VERSION,
    about = "jaw's daemon: it keeps your WhatsApp and does the work for jaw's interfaces"
)]
struct Cli {
    /// The engine jawd reaches WhatsApp through
    #[arg(long, value_enum, default_value_t = EngineKind::Go, global = true)]
    engine: EngineKind,
    /// Has the demo engine fail as a real one can, from the first SIGUSR1 jawd gets until the next,
    /// to try and test what interfaces show
    #[arg(long, value_enum, global = true, hide = true)]
    fail: Option<Failure>,
    /// Sends no notifications; JAW_NOTIFY=off does the same
    #[arg(long)]
    no_notify: bool,
    /// Behaves as if a program jawd uses weren't installed, to try and test what interfaces show
    #[arg(long, value_name = "PROGRAM", value_parser = ["ffmpeg", "ffprobe"], global = true, hide = true)]
    without: Vec<String>,
    /// Seconds the phone has to answer before jawd says it's offline, 20 unless asked
    #[arg(long, value_name = "SECONDS", global = true, hide = true)]
    phone_patience: Option<f64>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineKind {
    /// jaw-engine, on whatsmeow
    Go,
    /// A made-up account, for trying jaw out and for tests
    Demo,
    /// The made-up account's engine with no device, as after a logout while jawd was off, for tests
    #[value(hide = true)]
    DemoUnlinked,
}

#[derive(Subcommand)]
enum Command {
    /// What jawd is connected to, how its engine is, and what's missing
    Status,
    /// jawd on a made-up account in a folder of its own, to try jaw without WhatsApp and to measure
    /// jawd: thousands of chats, a million messages of history, and messages pouring in when asked
    Demo {
        /// The folder the account is kept in, which JAW_HOME points jaw at; a temporary one by default
        #[arg(long)]
        folder: Option<std::path::PathBuf>,
        /// The account's chats, 3,000 for a new one; the folder's account's otherwise
        #[arg(long)]
        chats: Option<u32>,
        /// The people in its contacts, 5,000 for a new one
        #[arg(long)]
        people: Option<u32>,
        /// The messages in the history the phone offers the first time, a million for a new one
        #[arg(long)]
        messages: Option<u32>,
        /// Messages in each blob of history, 20,000 for a new one
        #[arg(long)]
        blob: Option<u32>,
        /// Each chat's newest messages in the history's first pass, the blobs of many small
        /// conversations a phone sends as INITIAL_BOOTSTRAP, 20 for a new account
        #[arg(long)]
        recent: Option<u32>,
        /// The history comes as on a first run: as the account connects, a backlog waiting after
        /// time offline arrives a message at a time, as jaw-engine acks each (--catch-up says how
        /// many, 2,000 otherwise), and the phone's app state is fetched in full; for a new folder
        #[arg(long)]
        first_run: bool,
        /// Times lone messages, opening chats, pages at any depth and finds, with nothing else going on
        #[arg(long)]
        steady: bool,
        /// With --steady, pages the library's photos 120 at a time while the lone messages come, this
        /// many milliseconds apart (0 for back to back), as jaw pages it
        #[arg(long)]
        paging: Option<u64>,
        /// Of each page --paging reads, the items named as on screen: a 140-by-40 jaw shows 49
        #[arg(long, default_value_t = 49)]
        shown: usize,
        /// Messages a second to pour in, a quarter of them into the newest chat
        #[arg(long)]
        flood: Option<u32>,
        /// Chats near the top of the list whose people type, record and stop, each a few times a
        /// second, from once the history is in until the demo ends; busy groups first
        #[arg(long, default_value_t = 0)]
        typing: u32,
        /// How many times a second the people jawd follows come or go, as the made-up WhatsApp
        /// tells it; once every 5 s without
        #[arg(long, default_value_t = 0)]
        presence: u32,
        /// How long the flood lasts, timed; it goes on untimed until Ctrl-C without
        #[arg(long)]
        seconds: Option<f64>,
        /// Messages waiting after time offline, arriving as the account connects
        #[arg(long)]
        catch_up: Option<u32>,
        /// Messages the engine hands over before their acks come; whatsmeow waits for each, as 1 does
        #[arg(long, default_value_t = 256)]
        in_flight: u32,
        /// Seconds to wait before a catch-up, the steady timings or a flood, for interfaces to attach
        #[arg(long, default_value_t = 0.0)]
        after: f64,
        /// Seconds of nothing at the end, counting how often jawd wakes
        #[arg(long)]
        idle: Option<f64>,
        /// Writes what each phase measured to this file, as JSON
        #[arg(long)]
        report: Option<std::path::PathBuf>,
        /// Stops once what was asked for is done, rather than serving until Ctrl-C
        #[arg(long)]
        exit: bool,
        /// Sends notifications, which a demo doesn't otherwise
        #[arg(long)]
        notify: bool,
    },
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
    /// jaw.db's writer: live writes timed while the inbox takes a steady stream of rows, as in a catch-up
    Inbox {
        /// Inbox rows a second, each rate on a jaw.db of its own
        #[arg(long, value_delimiter = ',', default_values_t = [0, 100, 300, 500])]
        rates: Vec<u32>,
        #[arg(long, default_value_t = 4)]
        seconds: u32,
        /// Milliseconds between two durable live writes, with a conversion halfway between
        #[arg(long, default_value_t = 50)]
        every: u32,
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
    /// Media in: a photo's arrival through the demo engine to its 256-pixel preview on the socket
    Media {
        #[arg(long, default_value_t = 10)]
        count: u32,
        /// The photo's size in pixels, a 12-megapixel phone photo's by default
        #[arg(long, default_value_t = 4032)]
        width: u32,
        #[arg(long, default_value_t = 3024)]
        height: u32,
        /// Times texts' acks during a flood of this many photos instead
        #[arg(long, default_value_t = 0)]
        flood: u32,
        /// A flood of voice messages rather than photos
        #[arg(long)]
        voice: bool,
        /// Times this many texts' acks with no flood, to compare with
        #[arg(long, default_value_t = 0)]
        texts_alone: u32,
        /// Flushes each downloaded file in full, as off the Mac's own drive, rather than fencing it
        /// with a barrier
        #[arg(long)]
        full_flush: bool,
    },
    /// Media out: a photo of yours sent with send.media on the socket, to its copy made and to sent, while texts arrive
    MediaOut {
        #[arg(long, default_value_t = 10)]
        count: u32,
        /// The photo's size in pixels, a 12-megapixel phone photo's by default
        #[arg(long, default_value_t = 4032)]
        width: u32,
        #[arg(long, default_value_t = 3024)]
        height: u32,
        /// Sends each in HD too, as WhatsApp's dual upload does
        #[arg(long)]
        hd: bool,
        /// Milliseconds between two texts arriving all along
        #[arg(long, default_value_t = 50)]
        every: u64,
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

fn main() {
    unblock_signals();
    // On one line, its causes after it, so jaw, having started a jawd that stopped, says the whole
    // of why from the log's last line.
    if let Err(e) = jawd(Cli::parse()) {
        eprintln!("jawd: {e:#}");
        std::process::exit(1);
    }
}

fn jawd(cli: Cli) -> Result<()> {
    let home = jaw_proto::local::home();
    let options = Options {
        no_notify: cli.no_notify,
        without: cli.without.clone(),
        phone_patience: cli.phone_patience.map(Duration::from_secs_f64),
    };
    let demo = matches!(cli.command, Some(Command::Demo { .. }));
    if cli.fail.is_some() && !demo && !matches!(cli.engine, EngineKind::Demo | EngineKind::DemoUnlinked) {
        anyhow::bail!("--fail is for the demo engine: add --engine demo");
    }
    match cli.command {
        Some(Command::Status) => status(&home),
        Some(Command::Demo {
            folder,
            chats,
            people,
            messages,
            blob,
            recent,
            first_run,
            steady,
            paging,
            shown,
            flood,
            typing,
            presence,
            seconds,
            catch_up,
            in_flight,
            after,
            idle,
            report,
            exit,
            notify,
        }) => demo::run(demo::Plan {
            folder: folder.unwrap_or_else(demo::folder),
            shape: demo::Asked {
                chats: chats.map(|n| n as usize),
                people: people.map(|n| n as usize),
                messages: messages.map(|n| n as usize),
                chunk: blob.map(|n| n as usize),
                recent: recent.map(|n| n as usize),
            },
            first_run,
            steady,
            paging: paging.map(|ms| demo::Paging { apart: std::time::Duration::from_millis(ms), shown }),
            flood,
            typing,
            presence,
            seconds,
            catch_up,
            in_flight,
            after,
            idle,
            report,
            exit,
            fail: cli.fail,
            options: Options { no_notify: !notify, ..options },
        }),
        Some(Command::Bench(Bench::Link { count, size, commit })) => bench_link(cli.engine, count, size, commit),
        Some(Command::Bench(Bench::Lanes { background, interactive })) => bench_lanes(background, interactive),
        Some(Command::Bench(Bench::Store { messages, chats, runs })) => bench_store(messages, chats, runs),
        Some(Command::Bench(Bench::Inbox { rates, seconds, every })) => {
            bench_inbox(store::bench::Stream { rates, seconds, every_ms: every })
        }
        Some(Command::Bench(Bench::Find { messages, chats, people, runs })) => {
            bench_find(find::bench::Plan { messages, chats, people, runs })
        }
        Some(Command::Bench(Bench::Feed { rate, seconds, window, chats, lone, interfaces, far })) => {
            bench_feed(feed::bench::Plan { rate, seconds, window, chats, lone, interfaces, far })
        }
        Some(Command::Bench(Bench::Send { count, trips, chats })) => {
            bench_send(send::bench::Plan { count, trips, chats })
        }
        Some(Command::Bench(Bench::MediaOut { count, width, height, hd, every })) => {
            bench_media_out(media::outgoing::bench::Plan { count, width, height, hd, every })
        }
        Some(Command::Bench(Bench::Media { count, width, height, flood, voice, texts_alone, full_flush })) => {
            bench_media(media::bench::Plan { count, width, height, flood, voice, alone: texts_alone, full_flush })
        }
        None => run(&home, cli.engine, cli.fail, &options),
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

/// Times a photo's arrival to its preview on a jaw.db in a folder of its own, removed afterwards.
fn bench_media(plan: media::bench::Plan) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-media-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(media::bench::run(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// Builds a jaw.db in a folder of its own, removed afterwards, and times the palette's find on it.
fn bench_media_out(plan: media::outgoing::bench::Plan) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-media-out-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(media::outgoing::bench::run(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

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

/// Streams inbox rows into jaw.dbs in a folder of its own, removed afterwards.
fn bench_inbox(plan: store::bench::Stream) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("jaw-bench-inbox-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let outcome = runtime()?.block_on(store::bench::stream(&scratch, plan));
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build()?)
}

/// The engine to run, or its kind and why it can't run; a demo engine fails as `fail` says.
fn engine(kind: EngineKind, home: &Path, fail: Option<Failure>) -> Chosen {
    let demo = |demo: DemoEngine| -> Arc<dyn Engine> {
        match fail {
            Some(failure) => Arc::new(Failing::new(demo, failure)),
            None => Arc::new(demo),
        }
    };
    match kind {
        EngineKind::Demo => Ok(demo(DemoEngine::new())),
        EngineKind::DemoUnlinked => Ok(demo(DemoEngine::unlinked())),
        EngineKind::Go => match engine_program() {
            Some(program) => Ok(Arc::new(GoEngine::new(program, home.to_path_buf()))),
            None => Err((
                "go",
                "jaw-engine isn't beside jawd, in libexec/jaw/ beside its folder or on PATH; make build builds it"
                    .into(),
            )),
        },
    }
}

/// Where jaw-engine is: `JAW_ENGINE` if set; beside jawd, as in Jaw.app's Contents/Helpers/, in
/// libexec/jaw/ beside jawd's folder as `make install` and Homebrew put it, or in target/engine/
/// when jawd runs from a checkout's target/, each looked for from where jawd really is; then as
/// `GoEngine::find` looks, which ends on PATH.
fn engine_program() -> Option<std::path::PathBuf> {
    if std::env::var_os("JAW_ENGINE").is_some() {
        return GoEngine::find();
    }
    jaw_proto::local::beside(&["jaw-engine", "../libexec/jaw/jaw-engine", "../engine/jaw-engine"])
        .or_else(GoEngine::find)
}

fn run(home: &Path, kind: EngineKind, fail: Option<Failure>, options: &Options) -> Result<()> {
    std::fs::create_dir_all(home).with_context(|| format!("can't make {}", home.display()))?;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))?;
    #[cfg(target_os = "macos")]
    home::keep_out(home);
    let runtime = runtime()?;
    let outcome = runtime.block_on(async {
        let mut signals = Signals::new()?;
        let socket = jaw_proto::local::socket(home);
        let (lock, listener) = open(home, &socket)?;
        let started = tokio::select! {
            started = start_on(home, &socket, lock, engine(kind, home, fail), options) => started?,
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

/// How jawd runs, as its command line says.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Sends no notifications.
    pub no_notify: bool,
    /// The programs it behaves as if it didn't have.
    pub without: Vec<String>,
    /// How long the phone has to answer before jawd says it's offline, if not `sync::PHONE_PATIENCE`.
    pub phone_patience: Option<Duration>,
}

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

/// The engine jawd runs, or its kind and why it can't run.
type Chosen = Result<Arc<dyn Engine>, (&'static str, String)>;

/// Starts jawd listening on `socket`, removing the socket if it can't start, so a jawd that fails
/// leaves nothing an interface would try to reach.
async fn start_on(
    home: &Path,
    socket: &Path,
    lock: std::fs::File,
    engine: Chosen,
    options: &Options,
) -> Result<Started> {
    let started = start(home, lock, engine, options).await;
    if started.is_err() {
        let _ = std::fs::remove_file(socket);
    }
    started
}

async fn start(home: &Path, lock: std::fs::File, engine: Chosen, options: &Options) -> Result<Started> {
    let store = store::Store::open(home)?;
    std::fs::create_dir_all(home.join("spool"))?;
    sync::running(&store).await?;
    let mut starting = sync::starting(&store, engine.is_ok());
    if let Some(patience) = options.phone_patience {
        starting.phone_patience = patience;
    }
    let live: sync::Shared = Arc::new(std::sync::Mutex::new(starting));
    let feed = feed::Feed::start(store.clone(), feed::catching_up(live.clone()), feed::Timing::default());
    let typing = feed.typing().clone();
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = engine.map(|engine| Supervisor::spawn(engine, events, Backoff::default()));
    let (pushes, _) = tokio::sync::broadcast::channel(256);
    tokio::spawn(trouble(store.trouble(), pushes.clone()));
    let engine = supervisor.as_ref().ok().cloned();
    let following = presence::Following::default();
    tokio::spawn(sync::run(
        incoming,
        store.clone(),
        live.clone(),
        pushes.clone(),
        engine.clone(),
        home.to_path_buf(),
        following.clone(),
        typing.clone(),
    ));
    let attention = Arc::new(attention::Attention::default());
    let media =
        media::Media::start(store.clone(), engine.clone(), home, &live, &pushes, media::Config::default()).await?;
    let sending = send::Sending::start(
        store.clone(),
        engine.clone(),
        live.clone(),
        &pushes,
        attention.focus(),
        send::Timing::default(),
        send::Makers { previews: send::link::Previews::default(), media: Some(media.clone()) },
    )
    .await?;
    let voice = voice::Voice::start(
        home,
        store.clone(),
        media.clone(),
        sending.clone(),
        pushes.clone(),
        jaw_voice::SystemSpeaker,
        jaw_voice::SystemMicrophone,
        voice::Config::default(),
    )?;
    let me = live.lock().expect("live state").me();
    typing.start(store.clone(), me, sending.hearing());
    let mut pools: Vec<Arc<dyn work::Report>> =
        vec![Arc::new(store.clone()), Arc::new(store.reads().clone()), sending.lane()];
    pools.extend(media.lanes());
    let finder = find::Finder::start(store.clone()).await?;
    let connection = live.clone();
    let reach = Arc::new(move || {
        let live = connection.lock().expect("live state");
        (presence::reachable(live.connection.state), live.ended())
    });
    let connections = live.lock().expect("live state").connections();
    let looking = presence::Looking { focused: attention.focus(), pushes: pushes.subscribe(), reach, connections };
    following.start(store.clone(), engine.clone(), feed.clone(), looking, Default::default());
    let checks = Arc::new(organise::Checks::default());
    tokio::spawn(organise::check(store.clone(), checks.clone()));
    let notifier = notify::chosen(options.no_notify);
    let says = match &notifier {
        Ok(notifier) => notifier.name().to_string(),
        Err(why) => why.clone(),
    };
    eprintln!("jawd: notifications {says}");
    let settings = Arc::new(settings::Settings::load(store.clone())?);
    let state = state::State::new(home, supervisor, pools, live.clone(), Some(store.clone()), pushes.clone())?
        .with_feed(feed)
        .with_attention(attention)
        .with_sending(sending)
        .with_layouts(layout::Layouts::new(store.clone()))
        .with_settings(settings)
        .with_following(following, checks)
        .with_media(media)
        .with_voice(voice)
        .with_notifications(says)
        .without(&options.without);
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
/// is answered for something that won't be done, and media settles its latest previews; then
/// whatsmeow disconnects, the engine is ended
/// and started no more, whatever its supervisor was waiting for, and anything it handed over that
/// jawd hadn't acked comes again from WhatsApp; only then does jaw.db note a clean stop, and the
/// store closes last, once jawd's tasks are dropped.
async fn stop(Started { state, notifications, .. }: Started, socket: &Path) {
    state.stop();
    let _ = std::fs::remove_file(socket);
    if let Some(voice) = state.voice() {
        voice.stop().await;
    }
    if let Some(notifications) = notifications {
        notifications.abort();
    }
    // Previews and thumbnails made since media last settled them, so the next start needn't check
    // them; those it can't settle within SETTLING are checked then instead.
    if let Some(media) = state.media()
        && let Err(e) = media.settle_within(SETTLING).await
    {
        eprintln!("jawd: can't make the previews made lately durable at the stop: {e:#}");
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
/// How long a clean stop gives media to settle its latest previews, within STOPPING, before the
/// engine's orderly stop and the WAL's emptying.
const SETTLING: Duration = Duration::from_millis(500);

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

/// Lets through Ctrl-C and SIGTERM, which a jawd started with them blocked would never hear: jaw's
/// terminal blocks SIGTERM for a thread of its own to wait on, and the jawd it starts inherits the
/// mask, so `kill` couldn't stop it. Done before any thread starts, so every thread inherits it.
fn unblock_signals() {
    // SAFETY: the set is initialised by sigemptyset before it's used, and pthread_sigmask changes
    // only this thread's mask.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
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
    let (mut connection, _) = jaw_client::Connection::open(&socket, "jawd status", jaw_version::VERSION)?;
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
        let engine = engine(kind, &scratch, None).map_err(|(_, why)| anyhow::anyhow!(why))?;
        let store = if commit { Some(store::Store::open(&scratch)?) } else { None };
        let (events, mut incoming) = mpsc::channel(1024);
        let instance = engine.start(events).await?;
        let keeping = store.clone();
        tokio::spawn(async move {
            while let Some(Incoming { event, ack }) = incoming.recv().await {
                match (&keeping, event.kind) {
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
        if let Some(store) = &store {
            let runs = work::Report::report(store).run.map_or((0, 0), |run| (run.p50_us, run.p99_us));
            let flush = store::bench::flushes(&scratch, FLUSHES)?;
            let bare = store::bench::unflushed(&scratch, size as usize, FLUSHES)?;
            let at = |times: &[Duration], q: usize| times[(times.len() * q / 100).min(times.len() - 1)].as_micros() as u64;
            let outside = result.p50_us.saturating_sub(runs.0);
            println!(
                "  the inbox's transactions p50 {} µs, p99 {} µs, their flush included; one like them without its flush p50 {} µs; the drive's flush alone, {FLUSHES} times 4 KiB written and synced as a commit syncs it: p50 {} µs, p99 {} µs",
                runs.0,
                runs.1,
                at(&bare, 50),
                at(&flush, 50),
                at(&flush, 99),
            );
            println!(
                "  so jawd's own work in a round trip is about {} µs at p50: {outside} µs outside the inbox's transaction, and {} µs of the transaction's own",
                outside + at(&bare, 50),
                at(&bare, 50),
            );
        }
        Ok(())
    });
    let _ = std::fs::remove_dir_all(&scratch);
    outcome
}

/// How many times the drive's flush, and a transaction without it, are timed alone.
const FLUSHES: usize = 500;

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
    use crate::store::{Amend, Amendment, Change, Lane, Message};

    #[tokio::test(flavor = "multi_thread")]
    async fn the_demo_engines_account_shows_a_link_preview_with_its_thumbnail_through_the_socket() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let path = socket.clone();
        let link = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            let watch = serde_json::json!({ "chat": "351912345678@s.whatsapp.net", "around": "newest", "before": 20 });
            let watched = connection.call("messages.watch", watch).unwrap();
            let previewed = |rows: &serde_json::Value| {
                rows.as_array()?
                    .iter()
                    .find(|row| row["id"] == "3EB0DEMO00000004" && row["link"]["preview"].is_string())
                    .cloned()
            };
            if let Some(row) = previewed(&watched["rows"]) {
                return row["link"].clone();
            }
            loop {
                let push = connection.push().unwrap();
                if let Some(row) = previewed(&push.params["upsert"]) {
                    return row["link"].clone();
                }
            }
        });
        let link = tokio::time::timeout(Duration::from_secs(20), link).await.expect("the preview showed").unwrap();
        assert_eq!(link["title"], "Previsão para Sintra · IPMA");
        assert_eq!(link["url"], "https://www.ipma.pt/pt/otempo/prev.localidade.hora/");
        let thumbnail = home.path().join(link["preview"].as_str().unwrap());
        assert!(image::open(&thumbnail).is_ok(), "{} is the thumbnail", thumbnail.display());
        stop(started, &socket).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_demo_engines_account_shows_a_poll_with_its_votes_and_takes_yours_through_the_socket() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let path = socket.clone();
        let polls = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            let watch = serde_json::json!({ "chat": "120363012345678901@g.us", "around": "newest", "before": 20 });
            let watched = connection.call("messages.watch", watch).unwrap();
            // The poll as it shows once `voters` people voted, the votes of each option and whether
            // you chose it.
            let poll = |rows: &serde_json::Value, voters: u64| {
                let row = rows.as_array()?.iter().find(|row| row["id"] == "3EB0DEMO00000005")?;
                (row["poll"]["voters"] == voters).then(|| row["poll"].clone())
            };
            let mut rows = watched["rows"].clone();
            let before = loop {
                if let Some(poll) = poll(&rows, 2) {
                    break poll;
                }
                rows = connection.push().unwrap().params["upsert"].clone();
            };
            let vote = serde_json::json!({ "message": "3EB0DEMO00000005", "options": ["Pizza"] });
            assert_eq!(connection.call("poll.vote", vote).unwrap(), serde_json::json!({}));
            let after = loop {
                rows = connection.push().unwrap().params["upsert"].clone();
                if let Some(poll) = poll(&rows, 3) {
                    break poll;
                }
            };
            (before, after, connection)
        });
        let (before, after, mut connection) =
            tokio::time::timeout(Duration::from_secs(20), polls).await.expect("the poll showed").unwrap();
        let options = |poll: &serde_json::Value| -> Vec<(String, u64, bool)> {
            poll["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| (o["name"].as_str().unwrap().into(), o["votes"].as_u64().unwrap(), o["mine"] == true))
                .collect()
        };
        assert_eq!(before["question"], "Onde jantamos sexta?");
        assert_eq!(
            options(&before),
            [("Ramen".into(), 1, false), ("Tasca do Chico".into(), 1, false), ("Pizza".into(), 0, false)]
        );
        assert_eq!(
            options(&after),
            [("Ramen".into(), 1, false), ("Tasca do Chico".into(), 1, false), ("Pizza".into(), 1, true)],
            "yours, shown at once and kept once WhatsApp has it"
        );
        // A vote for a poll jawd hasn't seen waits for it, as status says.
        let waiting = Amend {
            chat: "120363012345678901@g.us".into(),
            target: "3EB0NOTYET".into(),
            sender: Some("351914444444@s.whatsapp.net".into()),
            mine: false,
            at: 1_792_000_000_000,
            what: Amendment::Vote { options: Vec::new(), id: "3EB0VOTE".into() },
            raw: None,
        };
        started.state.store().unwrap().apply(Lane::Live, vec![Change::Amend(waiting)]).await.unwrap();
        let stored = tokio::task::spawn_blocking(move || connection.call("status", serde_json::Value::Null).unwrap())
            .await
            .unwrap()["stored"]
            .clone();
        assert_eq!((&stored["votes_waiting"], &stored["votes_set_aside"]), (&1.into(), &0.into()));
        assert_eq!(stored["polls_backfill"], 0);
        assert_eq!((&stored["kinds_backfill"], &stored["encrypted_kept"]), (&0.into(), &0.into()));
        assert_eq!(stored["copies_backfill"], 0);
        assert_eq!(stored["contacts_backfill"], 0);
        assert_eq!(stored["voices_backfill"], 0);
        stop(started, &socket).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_demo_engines_account_shows_a_place_and_takes_yours_through_the_socket() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let path = socket.clone();
        let places = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            let ana = "351912345678@s.whatsapp.net";
            let watched =
                connection.call("messages.watch", serde_json::json!({ "chat": ana, "around": "newest", "before": 20 }));
            let found = |rows: &serde_json::Value, pick: &dyn Fn(&serde_json::Value) -> bool| {
                rows.as_array().and_then(|rows| rows.iter().find(|row| pick(row)).cloned())
            };
            let mut rows = watched.unwrap()["rows"].clone();
            let theirs = loop {
                if let Some(row) = found(&rows, &|row| row["id"] == "3EB0DEMO0000000C") {
                    break row;
                }
                rows = connection.push().unwrap().params["upsert"].clone();
            };
            let pin =
                serde_json::json!({ "chat": ana, "lat": 38.7139, "lon": -9.1334, "name": "Castelo", "key": "k1" });
            let sent = connection.call("send.location", pin).unwrap();
            let id = sent["message"].as_str().unwrap().to_string();
            let yours = loop {
                rows = connection.push().unwrap().params["upsert"].clone();
                if let Some(row) = found(&rows, &|row| row["id"] == id.as_str() && row["status"] != "pending") {
                    break row;
                }
            };
            (theirs, yours, connection)
        });
        let (theirs, yours, mut connection) =
            tokio::time::timeout(Duration::from_secs(20), places).await.expect("the places showed").unwrap();
        assert_eq!((&theirs["kind"], &theirs["spans"]), (&"location".into(), &serde_json::json!([])));
        assert_eq!(
            theirs["location"],
            serde_json::json!({
                "lat": 38.71625, "lon": -9.13197, "name": "Miradouro da Graça",
                "address": "Calçada da Graça, 1100-265 Lisboa", "url": null,
            }),
            "Ana's pin, with its place"
        );
        assert_eq!((&yours["kind"], yours["mine"].as_bool()), (&"location".into(), Some(true)));
        assert_eq!(yours["location"]["name"], "Castelo", "yours, gone through the engine");
        let stored = tokio::task::spawn_blocking(move || connection.call("status", serde_json::Value::Null).unwrap())
            .await
            .unwrap()["stored"]
            .clone();
        assert_eq!(stored["places_backfill"], 0, "a new jaw.db has no places to read again");
        stop(started, &socket).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_demo_engines_account_shows_an_event_and_its_answers_and_takes_yours_through_the_socket() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let path = socket.clone();
        let events = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            let climbing = "120363012345678901@g.us";
            let watched = connection
                .call("messages.watch", serde_json::json!({ "chat": climbing, "around": "newest", "before": 20 }));
            let found = |rows: &serde_json::Value, pick: &dyn Fn(&serde_json::Value) -> bool| {
                rows.as_array().and_then(|rows| rows.iter().find(|row| pick(row)).cloned())
            };
            // Rui's event, once both answers to it have come.
            let mut rows = watched.unwrap()["rows"].clone();
            let theirs = loop {
                let answered = |row: &serde_json::Value| {
                    row["id"] == "3EB0DEMO00000009" && row["event"]["going"] == 1 && row["event"]["maybe"] == 1
                };
                if let Some(row) = found(&rows, &answered) {
                    break row;
                }
                rows = connection.push().unwrap().params["upsert"].clone();
            };
            let planned = serde_json::json!({
                "chat": climbing, "name": "Bouldering", "start": 1_892_000_000_000_i64, "place": "Lisboa Climbing Spot",
                "key": "e1",
            });
            let sent = connection.call("send.event", planned).unwrap();
            let id = sent["message"].as_str().unwrap().to_string();
            let yours = loop {
                rows = connection.push().unwrap().params["upsert"].clone();
                if let Some(row) = found(&rows, &|row| row["id"] == id.as_str() && row["event"]["going"] == 1) {
                    break row;
                }
            };
            (theirs, yours, connection)
        });
        let (theirs, yours, mut connection) =
            tokio::time::timeout(Duration::from_secs(30), events).await.expect("the events showed").unwrap();
        let event = &theirs["event"];
        assert_eq!((&theirs["kind"], &theirs["spans"]), (&"event".into(), &serde_json::json!([])));
        assert_eq!((&event["name"], &event["place"]["name"]), (&"Sintra".into(), &"Cabo da Roca".into()));
        assert_eq!(
            (&event["going"], &event["maybe"], &event["mine"]),
            (&1.into(), &1.into(), &"maybe".into()),
            "{event}"
        );
        assert_eq!((&yours["kind"], yours["mine"].as_bool()), (&"event".into(), Some(true)));
        assert_eq!(yours["event"]["place"]["name"], "Lisboa Climbing Spot", "yours, gone through the engine");
        let stored = tokio::task::spawn_blocking(move || connection.call("status", serde_json::Value::Null).unwrap())
            .await
            .unwrap()["stored"]
            .clone();
        assert_eq!((&stored["events_backfill"], &stored["answers_waiting"]), (&0.into(), &0.into()));
        assert_eq!((&stored["answers_set_aside"], &stored["edits_set_aside"]), (&0.into(), &0.into()));
        assert_eq!(stored["events_opening"], 0, "nothing is left to take for the demo's event");
        assert_eq!(stored["events_unopened"], 0, "and nothing kept for it stays sealed");
        stop(started, &socket).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_demo_engines_account_shows_its_contact_cards_and_takes_yours_through_the_socket() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
        tokio::spawn(sock::serve(started.state.clone(), listener));
        let path = socket.clone();
        let cards = tokio::task::spawn_blocking(move || {
            let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
            // The row `id` in `chat` once the demo has played it, as its window shows it.
            let mut card = |chat: &str, id: &str| {
                let watch = serde_json::json!({ "chat": chat, "around": "newest", "before": 20 });
                let mut rows = connection.call("messages.watch", watch).unwrap()["rows"].clone();
                loop {
                    if let Some(row) = rows.as_array().and_then(|rows| rows.iter().find(|row| row["id"] == id)) {
                        return row.clone();
                    }
                    rows = connection.push().unwrap().params["upsert"].clone();
                }
            };
            let ana = card("351912345678@s.whatsapp.net", "3EB0DEMO0000000F");
            let climbers = card("120363012345678901@g.us", "3EB0DEMO0000000E");
            let marta = jaw_proto::vcard::write("Marta Lopes", &["+351213456789"]);
            let send = serde_json::json!({
                "chat": "351912345678@s.whatsapp.net",
                "cards": [{ "name": "Marta Lopes", "vcard": marta }],
                "key": "jaw-demo-card",
            });
            let sent = connection.call("send.contact", send).unwrap();
            let id = sent["message"].clone();
            loop {
                let push = connection.push().unwrap();
                let upserts = push.params["upsert"].as_array().cloned().unwrap_or_default();
                if upserts.iter().any(|row| row["id"] == id && row["status"] == "read") {
                    break;
                }
            }
            (ana, climbers, sent, marta)
        });
        let (ana, climbers, sent, marta) = tokio::time::timeout(Duration::from_secs(20), cards)
            .await
            .expect("the cards showed and yours went")
            .unwrap();
        assert_eq!(ana["kind"], "contact");
        assert_eq!(ana["contact"]["name"], "Dr.ª Marta Lopes");
        let read = jaw_proto::vcard::read(ana["contact"]["vcard"].as_str().unwrap());
        let numbers: Vec<&str> = read[0].phones.iter().map(|phone| phone.dialled()).collect();
        assert_eq!(numbers, ["+351213456789", "21 000 00 00"], "her card's numbers, the first on WhatsApp");
        assert_eq!(climbers["contact"]["name"], "2 contacts");
        let read = jaw_proto::vcard::read(climbers["contact"]["vcard"].as_str().unwrap());
        let names: Vec<_> = read.iter().map(|card| card.name.clone().unwrap()).collect();
        assert_eq!(names, ["Inês Gomes", "Tiago Martins"], "both cards in one message");
        assert_eq!((&sent["row"]["kind"], &sent["row"]["status"]), (&"contact".into(), &"pending".into()));
        assert_eq!(sent["row"]["contact"], serde_json::json!({ "name": "Marta Lopes", "vcard": marta }));
        stop(started, &socket).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn jawd_stops_well_within_its_bound_while_messages_pour_into_a_watched_chat() {
        let home = tempfile::tempdir().unwrap();
        let socket = jaw_proto::local::socket(home.path());
        let (lock, listener) = open(home.path(), &socket).unwrap();
        let options = Options { no_notify: true, ..Options::default() };
        let started = start(home.path(), lock, engine(EngineKind::Demo, home.path(), None), &options).await.unwrap();
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
        let options = Options { no_notify: true, ..Options::default() };
        let failed = start_on(home.path(), &socket, lock, engine(EngineKind::Demo, home.path(), None), &options).await;
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
