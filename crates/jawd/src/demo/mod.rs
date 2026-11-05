//! `jawd demo`: jawd on a made-up account (`account`), on a folder of its own, so jaw and any later
//! interface can be tried without WhatsApp, and the load harness can measure jawd as an interface
//! sees it. The demo engine plays the account's phone: the first time, it offers the whole history,
//! a million messages by default, which jawd stores and indexes as it would a real phone's. Then,
//! as asked, a few messages arrive and chats are opened, paged and searched (`--steady`); messages
//! pour in (`--flood`); a backlog waiting after time offline arrives as the account connects
//! (`--catch-up`); or nothing happens at all (`--idle`). Each phase measures what applies with an
//! interface of its own on the socket (`probe`) and the kernel's count of what the process costs
//! (`usage`), says it, and writes it to `--report` as JSON for `mise run perf`.

mod account;
mod probe;
pub(crate) mod usage;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use self::account::{Account, Shape};
use self::probe::{Asker, Expected, PAGE, Watcher};
use self::usage::Sampler;
use crate::engine::demo::{self, DemoEngine, World};
use crate::engine::failing::{Failing, Failure};
use crate::engine::pb::chat_presence::State as Said;
use crate::engine::{Engine, pb};
use crate::state::State;
use crate::store::Store;
use crate::store::bench::{Rng, Timings, ms};
use crate::work::Report;

pub struct Plan {
    pub folder: PathBuf,
    pub shape: Asked,
    /// Whether the history comes as on a first run, with a backlog and the app state at once.
    pub first_run: bool,
    pub steady: bool,
    pub paging: Option<Paging>,
    pub flood: Option<u32>,
    /// Chats whose people type, record and stop, each a few times a second.
    pub typing: u32,
    /// How many times a second the people jawd follows come or go; once every 5 s when 0.
    pub presence: u32,
    pub seconds: Option<f64>,
    pub catch_up: Option<u32>,
    pub in_flight: u32,
    pub after: f64,
    pub idle: Option<f64>,
    pub report: Option<PathBuf>,
    pub exit: bool,
    /// How the demo engine fails, from the first SIGUSR1 until the next.
    pub fail: Option<Failure>,
    /// How its jawd runs; a demo sends no notifications unless asked.
    pub options: crate::Options,
}

/// The library paged while `--steady`'s lone messages come: the time between pages, and how many
/// of each page's items are named as on screen.
#[derive(Clone, Copy, Debug)]
pub struct Paging {
    pub apart: Duration,
    pub shown: usize,
}

/// The folder a demo uses unless told otherwise, kept between runs so its history comes once.
pub fn folder() -> PathBuf {
    std::env::temp_dir().join("jaw-demo")
}

/// The account's shape as it was asked for; what isn't asked for is the folder's account's, or for
/// a new one, the default: 3,000 chats with 5,000 people and a million messages, in blobs of 20,000.
#[derive(Clone, Copy, Debug, Default)]
pub struct Asked {
    pub chats: Option<usize>,
    pub people: Option<usize>,
    pub messages: Option<usize>,
    pub chunk: Option<usize>,
    pub recent: Option<usize>,
}

impl Asked {
    fn shape(self, kept: Option<Shape>) -> Shape {
        let base = kept.unwrap_or(Shape {
            chats: 3_000,
            people: 5_000,
            messages: 1_000_000,
            chunk: 20_000,
            recent: account::RECENT,
        });
        Shape {
            chats: self.chats.unwrap_or(base.chats),
            people: self.people.unwrap_or(base.people),
            messages: self.messages.unwrap_or(base.messages),
            chunk: self.chunk.unwrap_or(base.chunk),
            recent: self.recent.unwrap_or(base.recent),
        }
    }
}

/// What the demo keeps beside its jaw.db: the account's shape, whether its history is all in, and
/// when the last run stopped, in milliseconds since the epoch, after which a catch-up's backlog
/// was sent.
#[derive(serde::Serialize, serde::Deserialize)]
struct Marker {
    shape: Shape,
    history: bool,
    #[serde(default)]
    ended: Option<i64>,
}

const MARKER: &str = "jaw-demo.json";

/// The newest chat, which a quarter of a flood goes to and the demo's interface has open.
const OPEN: usize = 0;
/// The chats near the top of the list typing is said in, and how often each changes.
const TYPING_AMONG: usize = 300;
const TYPING_EVERY: Duration = Duration::from_millis(150);
/// A live message this often while history goes in.
const LIVE_EVERY: Duration = Duration::from_millis(100);
/// A chat opened this often while something else goes on.
const OPEN_EVERY: Duration = Duration::from_millis(300);
/// The quiet before a lone message, and how many are timed.
const LONE_EVERY: Duration = Duration::from_millis(200);
const LONE: u32 = if cfg!(test) { 3 } else { 50 };
/// How long the watcher waits, as what it watched ends, for the messages still on their way to it.
const STRAGGLERS: Duration = Duration::from_secs(10);
/// Chats opened with nothing else going on, and times each page and find is asked for.
const OPENS: usize = if cfg!(test) { 12 } else { 300 };
const RUNS: usize = if cfg!(test) { 3 } else { 30 };
/// What's found: words in many messages, two together, a word in very few, and a name.
const FINDS: &[&str] = &["nao", "obrigado", "cafe amanha", "jantar", "zefiro", "lisbon"];
/// The longest a catch-up's backlog was waiting: since the last run stopped, or this long ago.
const OFFLINE_MS: i64 = 2 * 24 * 3600 * 1000;
/// The messages waiting on a first run unless asked, about as many as the owner's had.
const FIRST_RUN_BACKLOG: u32 = 2_000;
/// How long jawd goes without a commit before it's counted idle: past the blocking pool's threads'
/// 10 s, the WAL emptied after 10 s, and the avatars fetched after connecting.
const SETTLE: Duration = Duration::from_secs(15);
/// The commits remembered for timing a message from its commit to the interface.
const COMMITS_KEPT: usize = 500_000;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Refuses the folder of a real account: jaw's own, or one with an engine's session in it, or one
/// with a jaw.db the demo didn't make.
fn refuse(folder: &Path) -> Result<Option<Marker>> {
    let real = jaw_proto::local::platform();
    if folder == real || folder.canonicalize().ok().is_some_and(|folder| real.canonicalize().ok() == Some(folder)) {
        bail!("{} is jaw's own folder, which holds your account; jawd demo needs one of its own", folder.display());
    }
    if folder.join("session.db").exists() {
        bail!("{} holds an engine's session, a real account's; jawd demo needs a folder of its own", folder.display());
    }
    let marker = match std::fs::read(folder.join(MARKER)) {
        Ok(bytes) => Some(serde_json::from_slice::<Marker>(&bytes).context("the demo's note beside its jaw.db")?),
        Err(_) => None,
    };
    if marker.is_none() && folder.join("jaw.db").exists() {
        bail!("{} holds a jaw.db jawd demo didn't make; it needs a folder of its own", folder.display());
    }
    Ok(marker)
}

fn write_marker(folder: &Path, marker: &Marker) -> Result<()> {
    std::fs::write(folder.join(MARKER), serde_json::to_vec_pretty(marker)?)?;
    Ok(())
}

pub fn run(plan: Plan) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&plan.folder)
        .with_context(|| format!("can't make {}", plan.folder.display()))?;
    let marker = refuse(&plan.folder)?;
    let shape = plan.shape.shape(marker.as_ref().map(|marker| marker.shape));
    if let Some(marker) = &marker
        && (marker.shape.chats, marker.shape.people, marker.shape.messages)
            != (shape.chats, shape.people, shape.messages)
    {
        bail!(
            "{} holds a made-up account of {} messages in {} chats with {} people; remove it to make another",
            plan.folder.display(),
            marker.shape.messages,
            marker.shape.chats,
            marker.shape.people
        );
    }
    let history = !marker.as_ref().is_some_and(|marker| marker.history);
    if plan.first_run && !history {
        bail!(
            "{} has had its first run, and its history is in; a first run needs a folder of its own",
            plan.folder.display()
        );
    }
    if history && plan.catch_up.is_some() && !plan.first_run {
        bail!("a catch-up comes after the history: run jawd demo on {} once first", plan.folder.display());
    }
    let ended = marker.and_then(|marker| marker.ended);
    write_marker(&plan.folder, &Marker { shape, history: !history, ended })?;
    crate::runtime()?.block_on(serve(plan, shape, history, ended))
}

/// The account's connection in `state`: connecting, before a catch-up, or connected.
fn connection(state: pb::connection::State) -> pb::Event {
    let mut event = demo::connected();
    if let Some(pb::event::Kind::Connection(connection)) = &mut event.kind {
        connection.state = state as i32;
    }
    event
}

async fn serve(plan: Plan, shape: Shape, history: bool, ended: Option<i64>) -> Result<()> {
    let account = Account::new(shape, now_ms());
    let account = Arc::new(if plan.first_run { account.with_app_state() } else { account });
    // A catch-up's or a first run's account connects as its phase begins.
    let connecting = plan.catch_up.is_some() || plan.first_run;
    let mut script = vec![if connecting { connection(pb::connection::State::Connecting) } else { demo::connected() }];
    if history {
        script.extend(account.notifications());
    }
    let engine = DemoEngine::for_account(script, account.clone());
    let world = engine.world();
    let engine: Arc<dyn Engine> = match plan.fail {
        Some(failure) => Arc::new(Failing::new(engine, failure)),
        None => Arc::new(engine),
    };
    let folder = plan.folder.clone();
    let socket = jaw_proto::local::socket(&folder);
    let mut signals = crate::Signals::new()?;
    let (_lock, listener) = crate::open(&folder, &socket)?;
    let started = crate::start_on(&folder, &socket, Ok(engine), &plan.options, &OnceLock::new()).await?;
    tokio::spawn(crate::sock::serve(started.state.clone(), listener));
    let store = started.state.store().context("the demo's jawd has no store")?.clone();
    println!(
        "jawd demo: {} messages in {} chats with {} people, on {}; JAW_HOME={} jaw shows them",
        account.shape.messages,
        account.shape.chats,
        account.shape.people,
        folder.display(),
        folder.display()
    );
    let demo = Demo {
        shape,
        history_in: AtomicBool::new(!history),
        offline_since: ended.unwrap_or_default().max(now_ms() - OFFLINE_MS),
        account,
        world,
        state: started.state.clone(),
        store,
        socket: socket.clone(),
        sampler: Sampler::start(folder.join("jaw.db-wal")),
        expected: Arc::default(),
        commits: Arc::default(),
        report: Mutex::new(serde_json::Map::new()),
        report_to: plan.report.clone(),
        run: now_ms(),
        sent: AtomicU64::new(0),
        // A first run's backlog comes through jaw-engine, which waits for each ack.
        in_flight: Arc::new(Semaphore::new(if plan.first_run { 1 } else { plan.in_flight.max(1) as usize })),
        waiting: Arc::default(),
        most_waiting: AtomicU64::new(0),
        rng: Mutex::new(Rng(0x5EED_DE30)),
    };
    let exit = plan.exit;
    // A signal while the phases run stops the demo; only phases that finish wait for one.
    let (outcome, signalled) = tokio::select! {
        done = demo.phases(&plan, history) => (done, false),
        signal = signals.next() => {
            eprintln!("jawd demo: {signal}, so it stops");
            (Ok(()), true)
        }
    };
    if outcome.is_ok() && !exit && !signalled {
        println!("jawd demo: serving until Ctrl-C");
        let signal = signals.next().await;
        eprintln!("jawd demo: {signal}, so it stops");
    }
    let began = Instant::now();
    match tokio::time::timeout(crate::STOPPING, crate::stop(started, &socket)).await {
        Ok(()) => eprintln!("jawd demo: stopped in {} ms", began.elapsed().as_millis()),
        Err(_) => eprintln!("jawd demo: didn't stop within {} s", crate::STOPPING.as_secs()),
    }
    let history = demo.history_in.load(Ordering::Relaxed);
    write_marker(&folder, &Marker { shape, history, ended: Some(now_ms()) })?;
    outcome
}

struct Demo {
    /// The account's shape, which the folder's note keeps.
    shape: Shape,
    /// Whether the account's history is all in.
    history_in: AtomicBool,
    /// When the account went offline, after which a catch-up's backlog was sent.
    offline_since: i64,
    account: Arc<Account>,
    world: Arc<World>,
    state: Arc<State>,
    store: Store,
    socket: PathBuf,
    sampler: Sampler,
    expected: Arc<Expected>,
    commits: Arc<Mutex<VecDeque<(i64, Instant)>>>,
    report: Mutex<serde_json::Map<String, Value>>,
    report_to: Option<PathBuf>,
    /// When this run started, in the IDs of the messages it sends, so no two runs' are the same.
    run: i64,
    sent: AtomicU64,
    in_flight: Arc<Semaphore>,
    /// Messages handed over and not yet acked, and the most at once in a first run's backlog.
    waiting: Arc<AtomicU64>,
    most_waiting: AtomicU64,
    rng: Mutex<Rng>,
}

/// The library as `--steady` measures it: each window's answers, how many items it said matched,
/// the pages of a walk down its photos, how deep the walk got, and the load average around it.
struct Library {
    windows: Vec<(&'static str, String, u64, Vec<Duration>)>,
    walk: Vec<Duration>,
    deepest: usize,
    loads: (f64, f64),
}

/// The pages a walk down the library's photos reads at most.
const LIBRARY_PAGES: usize = if cfg!(test) { 3 } else { 500 };
/// A page of the library, as jaw asks for one.
const LIBRARY_PAGE: u32 = 120;

/// The library opened as jaw opens it: its photos, every kind's newest, the biggest chat's, one
/// person's and yours, found by a word captions say often and by one they say once, and its counts
/// alone; then a walk 120 at a time from the newest photo down, as jaw pages it.
fn library(asker: &mut Asker, account: &Account) -> Result<Library> {
    let before = load_average();
    let page = |more: Value| {
        let mut params = json!({"around": "top", "before": 0, "after": LIBRARY_PAGE});
        params.as_object_mut().expect("an object").extend(more.as_object().expect("an object").clone());
        params
    };
    let (_, newest) = asker.library(page(json!({"kind": "photo"})))?;
    let sender = newest["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find_map(|row| row["from"]["id"].as_str().map(str::to_string)))
        .unwrap_or_else(|| "me".into());
    let asks = [
        ("photos", "its newest 120 photos".to_string(), page(json!({"kind": "photo"}))),
        ("all", "every kind's newest 14".to_string(), page(json!({"kind": "all", "after": 14}))),
        (
            "chat",
            "the biggest chat's newest 120 photos".to_string(),
            page(json!({"kind": "photo", "chat": account.chat(0)})),
        ),
        ("person", format!("{sender}'s newest 120 photos"), page(json!({"kind": "photo", "person": sender}))),
        ("mine", "your newest 120 files".to_string(), page(json!({"kind": "file", "person": "me"}))),
        ("jantar", "photos whose captions say jantar".to_string(), page(json!({"kind": "photo", "text": "jantar"}))),
        ("zefiro", "photos whose captions say zéfiro".to_string(), page(json!({"kind": "photo", "text": "zéfiro"}))),
        ("counts", "its counts alone".to_string(), page(json!({"kind": "photo", "after": 0}))),
        (
            "person_counts",
            format!("{sender}'s counts alone"),
            page(json!({"kind": "photo", "person": sender, "after": 0})),
        ),
    ];
    let mut windows = Vec::new();
    for (key, what, params) in asks {
        let mut took = Vec::with_capacity(RUNS);
        let mut count = 0;
        for _ in 0..RUNS {
            let (t, window) = asker.library(params.clone())?;
            took.push(t);
            count = window["total"].as_u64().unwrap_or_default();
        }
        windows.push((key, what, count, took));
    }
    let mut walk = Vec::new();
    let mut around = "top".to_string();
    let mut deepest = 0;
    while walk.len() < LIBRARY_PAGES {
        let (t, window) =
            asker.library(json!({"kind": "photo", "around": around, "before": 0, "after": LIBRARY_PAGE}))?;
        walk.push(t);
        let Some(last) = window["rows"].as_array().and_then(|rows| rows.last()) else { break };
        deepest += window["rows"].as_array().map_or(0, Vec::len) - usize::from(around != "top");
        around = last["id"].as_str().unwrap_or_default().to_string();
        if window["older"] != true {
            break;
        }
    }
    Ok(Library { windows, walk, deepest, loads: (before, load_average()) })
}

/// Walks down the library's photos 120 at a time as jaw pages them, `paging.apart` between pages,
/// naming the first `paging.shown` of each as on screen, until `halt`; says what each page took and
/// how deep it got, going round again from the top at the end.
fn walk(socket: &Path, paging: Paging, halt: &AtomicBool) -> Result<(Vec<Duration>, usize)> {
    let mut asker = Asker::open(socket)?;
    let (mut took, mut deepest, mut around) = (Vec::new(), 0, "top".to_string());
    while !halt.load(Ordering::Relaxed) {
        let (t, window) =
            asker.library(json!({"kind": "photo", "around": around, "before": 0, "after": LIBRARY_PAGE}))?;
        took.push(t);
        let rows = window["rows"].as_array().cloned().unwrap_or_default();
        if paging.shown > 0 {
            let on_screen: Vec<&str> = rows.iter().take(paging.shown).filter_map(|row| row["id"].as_str()).collect();
            asker.shown(&on_screen)?;
        }
        match rows.last() {
            Some(last) if window["older"] == true => {
                deepest += rows.len() - usize::from(around != "top");
                around = last["id"].as_str().unwrap_or_default().to_string();
            }
            _ => around = "top".to_string(),
        }
        std::thread::sleep(paging.apart);
    }
    Ok((took, deepest))
}

/// The machine's load average over the last minute.
fn load_average() -> f64 {
    let mut loads = [0f64; 3];
    // SAFETY: getloadavg fills at most as many values as it's given room for.
    unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    loads[0]
}

/// Timings as `mise run perf` reads them: how many, and the 50th and 99th percentiles and the most,
/// in milliseconds.
fn summary(timings: &[Duration]) -> Value {
    let mut sorted = timings.to_vec();
    sorted.sort_unstable();
    let at = |q: usize| sorted.get((sorted.len() * q / 100).min(sorted.len().saturating_sub(1))).copied();
    let millis = |d: Option<Duration>| d.map(ms);
    json!({"n": sorted.len(), "p50": millis(at(50)), "p99": millis(at(99)), "max": millis(sorted.last().copied())})
}

fn line(what: &str, timings: &[Duration]) -> String {
    Timings(timings.to_vec()).line(what)
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1e6
}

/// Bytes in MiB, as the WAL's bound is set.
fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1 << 20) as f64
}

impl Demo {
    async fn phases(&self, plan: &Plan, history: bool) -> Result<()> {
        tokio::spawn(log_commits(self.store.clone(), self.commits.clone()));
        match (history, plan.first_run) {
            (true, true) => self.first_run(plan.catch_up.unwrap_or(FIRST_RUN_BACKLOG)).await?,
            (true, false) => self.history().await?,
            (false, _) => {}
        }
        let typing = self.keep_typing(plan.typing);
        if plan.presence > 0 {
            println!("jawd demo: someone jawd follows comes or goes {} times a second", plan.presence);
            self.world.come_and_go_every(Duration::from_secs_f64(1.0 / f64::from(plan.presence)));
        }
        if plan.after > 0.0 {
            tokio::time::sleep(Duration::from_secs_f64(plan.after)).await;
        }
        if let Some(backlog) = plan.catch_up.filter(|_| !plan.first_run) {
            self.catch_up(backlog, plan.in_flight).await?;
        }
        if plan.steady {
            self.steady(plan.paging).await?;
        }
        if let Some(rate) = plan.flood {
            match plan.seconds {
                Some(seconds) => self.flood(rate, seconds, plan.in_flight).await?,
                None => self.pour(rate).await,
            }
        }
        if let Some(seconds) = plan.idle {
            self.idle(seconds).await?;
        }
        if let Some(typing) = typing {
            typing.abort();
        }
        Ok(())
    }

    /// `chats` chats near the top of the list, busy groups first, whose people type, record and
    /// stop for as long as the demo runs, each chat a few times a second, sent over the engine's
    /// link as jaw-engine sends WhatsApp's chat presence, in order with the messages a flood sends.
    fn keep_typing(&self, chats: u32) -> Option<tokio::task::JoinHandle<()>> {
        if chats == 0 {
            return None;
        }
        let top = self.account.chats().min(TYPING_AMONG);
        let mut typed: Vec<usize> = (0..top).filter(|&c| self.account.group(c)).collect();
        typed.extend((0..top).filter(|&c| !self.account.group(c)));
        typed.truncate(chats as usize);
        println!(
            "jawd demo: someone types, records or stops in {} chats every {} ms",
            typed.len(),
            TYPING_EVERY.as_millis()
        );
        let (account, world) = (self.account.clone(), self.world.clone());
        Some(tokio::spawn(async move {
            let mut rng = Rng(0x7E5E_7E5E);
            let mut tick = tokio::time::interval(TYPING_EVERY / typed.len().max(1) as u32);
            for &c in typed.iter().cycle() {
                tick.tick().await;
                let state = match rng.below(4) {
                    0 | 1 => Said::Composing,
                    2 => Said::Recording,
                    _ => Said::Paused,
                };
                world.typing(account.chat(c), &account.typist(c, rng.next()), state).await;
            }
        }))
    }

    fn record(&self, phase: &str, numbers: Value) -> Result<()> {
        let mut report = self.report.lock().expect("the demo's report");
        report.insert(phase.into(), numbers);
        if let Some(path) = &self.report_to {
            std::fs::write(path, serde_json::to_vec_pretty(&*report)?)?;
        }
        Ok(())
    }

    /// A message ID no other run's can have: this run's start, to the millisecond, and a count.
    fn next_id(&self) -> String {
        format!("3EB1{:011X}{:08X}", self.run, self.sent.fetch_add(1, Ordering::Relaxed))
    }

    /// A chat for the next message of a flood: a quarter go to the open chat, most of the rest to the
    /// chats near the top, and one in ten anywhere, so chats from far down jump to the top.
    fn next_chat(&self) -> usize {
        let mut rng = self.rng.lock().expect("the demo's dice");
        let chats = self.account.chats() as u64;
        match rng.below(40) {
            0..10 => OPEN,
            10..14 => rng.below(chats) as usize,
            _ => rng.below(chats.min(300)) as usize,
        }
    }

    /// Hands a message in chat `c` to jawd, sent at `at`, telling the watcher to expect it if it's in
    /// the open chat; says how long jawd took to ack it.
    async fn deliver(&self, c: usize, at: i64) -> Option<Duration> {
        let id = self.next_id();
        let event = self.account.live(c, id.clone(), at);
        if c == OPEN {
            self.expected.expect(id, Instant::now());
        }
        self.world.deliver(event).await
    }

    /// From each message that reached the watcher, the time from its commit, as the store's last
    /// committed seq first reached its version.
    fn since_commit(&self, came: &[(Duration, i64, Instant)]) -> Vec<Duration> {
        let commits = self.commits.lock().expect("the commits");
        came.iter()
            .filter_map(|&(_, v, read)| {
                let i = commits.partition_point(|&(seq, _)| seq < v);
                commits.get(i).map(|&(_, at)| read.saturating_duration_since(at))
            })
            .collect()
    }

    /// Opens a chat every OPEN_EVERY on a thread of its own, from the chats at the top and from
    /// anywhere, until it's stopped.
    fn keep_opening(&self) -> Opening {
        let stop = Arc::new(AtomicBool::new(false));
        let (socket, account, stopping) = (self.socket.clone(), self.account.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut asker = Asker::open(&socket)?;
            let mut rng = Rng(0x0BE7);
            let mut took = Vec::new();
            while !stopping.load(Ordering::Relaxed) {
                std::thread::sleep(OPEN_EVERY);
                let chats = account.chats() as u64;
                let c = if rng.below(3) == 0 { rng.below(chats) } else { rng.below(chats.min(100)) } as usize;
                took.push(asker.open_chat(account.chat(c))?);
            }
            Ok(took)
        });
        Opening { stop, thread }
    }

    async fn history(&self) -> Result<()> {
        let blobs = self.account.blobs();
        println!(
            "jawd demo: the phone offers the history, {} messages in {blobs} blobs of up to {}",
            self.account.shape.messages, self.account.shape.chunk
        );
        self.sampler.most();
        let started = Instant::now();
        let watcher = self.watch().await?;
        let opening = self.keep_opening();
        let mut acks = Vec::new();
        let mut said = Instant::now();
        loop {
            let tick = Instant::now();
            if let Some(ack) = self.deliver(OPEN, now_ms()).await {
                acks.push(ack);
            }
            let done = self.state.live().lock().expect("live state").history.as_ref().map_or(0, |h| h.blobs);
            if done as usize >= blobs {
                break;
            }
            if said.elapsed() >= Duration::from_secs(10) {
                said = Instant::now();
                if self.store.count("quarantine")? > 0 {
                    bail!("some of the history went to quarantine; jawd's log says why");
                }
                println!("jawd demo: {done} of {blobs} blobs in after {:.0} s", started.elapsed().as_secs_f64());
            }
            tokio::time::sleep_until((tick + LIVE_EVERY).into()).await;
        }
        let stored = started.elapsed();
        self.converted().await?;
        let indexing = Instant::now();
        self.store.indexed().await?;
        let indexed = indexing.elapsed();
        let opens = opening.stop().await?;
        self.expected.settled(STRAGGLERS).await;
        stop(watcher).await;
        let (came, missing) = self.expected.take();
        let arrivals: Vec<Duration> = came.iter().map(|c| c.0).collect();
        let from_commit = self.since_commit(&came);
        let most = self.sampler.most();
        let after = usage::now().footprint;
        let storage = self.store.storage();
        let messages = self.store.count("messages")?;
        let rate = self.account.shape.messages as f64 / stored.as_secs_f64();
        println!(
            "history: {messages} messages stored in {:.1} s, {rate:.0} a second, then indexed for search in {:.1} s; the WAL held {} frames at most and reached {:.0} MiB on disk; jawd's memory at most {:.0} MB ({:.0} MB resident), and {:.0} MB once it was in",
            stored.as_secs_f64(),
            indexed.as_secs_f64(),
            storage.wal_peak_frames,
            mib(most.wal),
            mb(most.footprint),
            mb(most.resident),
            mb(after)
        );
        println!("{}", line(&format!("a live message every {} ms, acked", LIVE_EVERY.as_millis()), &acks));
        println!("{}", line("  from hand-over to the interface", &arrivals));
        println!("{}", line("  from its commit to the interface", &from_commit));
        println!("{}", line(&format!("a chat opened every {} ms", OPEN_EVERY.as_millis()), &opens));
        if missing > 0 {
            println!("  {missing} live messages never reached the interface");
        }
        self.record(
            "history",
            json!({
                "messages": messages,
                "chats": self.account.shape.chats,
                "blobs": blobs,
                "stored_seconds": stored.as_secs_f64(),
                "rate": rate,
                "indexed_seconds": indexed.as_secs_f64(),
                "wal_peak_frames": storage.wal_peak_frames,
                "wal_peak_mib": mib(most.wal),
                "peak_mb": mb(most.footprint),
                "peak_resident_mb": mb(most.resident),
                "lifetime_peak_mb": mb(usage::now().peak),
                "after_mb": mb(after),
                "ack": summary(&acks),
                "arrival": summary(&arrivals),
                "commit_to_interface": summary(&from_commit),
                "open": summary(&opens),
                "missing": missing,
            }),
        )?;
        self.history_in.store(true, Ordering::Relaxed);
        let folder = self.socket.parent().context("the socket's folder")?;
        write_marker(folder, &Marker { shape: self.shape, history: true, ended: None })
    }

    /// The account's first run on this jawd, as a phone linked again after time offline brings it:
    /// as it connects, what waited meanwhile arrives a message at a time, each waiting for its ack
    /// as jaw-engine waits for each, while the phone's app state is fetched in full, a mutation at a
    /// time, and the history comes, every chat's newest few first in blobs of many small
    /// conversations, then the rest.
    async fn first_run(&self, backlog: u32) -> Result<()> {
        let (blobs, bootstrap) = (self.account.blobs(), self.account.bootstrap_blobs());
        println!(
            "jawd demo: a first run: {backlog} messages waiting, one at a time, while the app state is fetched in full and the phone offers the history, {} messages in {blobs} blobs, the first {bootstrap} each chat's newest {}",
            self.account.shape.messages, self.account.shape.recent
        );
        self.sampler.most();
        self.most_waiting.store(0, Ordering::Relaxed);
        let started = Instant::now();
        let watcher = self.watch().await?;
        let opening = self.keep_opening();
        let history = {
            let state = self.state.clone();
            tokio::spawn(async move {
                let mut first = None;
                loop {
                    let got = state.live().lock().expect("live state").history.clone().unwrap_or_default();
                    if first.is_none() && got.blobs as usize >= bootstrap {
                        first = Some((started.elapsed(), got.clone()));
                    }
                    if got.blobs as usize >= blobs {
                        return (first, started.elapsed(), got);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
        };
        let (events, receipts) = self.backlog(backlog);
        let preview =
            pb::OfflinePreview { total: backlog + receipts, messages: backlog, receipts, ..Default::default() };
        let acks: Arc<Mutex<Vec<Duration>>> = Arc::default();
        self.world.deliver(connection(pb::connection::State::Connected)).await;
        self.world.deliver(pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) }).await;
        for (event, expected) in events {
            self.hand_over(|| (event, expected), &acks).await;
        }
        self.landed(1).await;
        let drained = started.elapsed();
        let done = pb::OfflineDone { count: backlog + receipts };
        self.world.deliver(pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflineDone(done)) }).await;
        let (first, stored, got) = loop {
            if history.is_finished() {
                break history.await?;
            }
            if self.store.count("quarantine")? > 0 {
                bail!("some of the first run went to quarantine; jawd's log says why");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let fetched = loop {
            if let Some(fetched) = self.world.fetched().pop() {
                break fetched;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        self.converted().await?;
        let converted = started.elapsed();
        let indexing = Instant::now();
        self.store.indexed().await?;
        let indexed = indexing.elapsed();
        let opens = opening.stop().await?;
        self.expected.settled(STRAGGLERS).await;
        stop(watcher).await;
        let (came, missing) = self.expected.take();
        let arrivals: Vec<Duration> = came.iter().map(|c| c.0).collect();
        let acks = std::mem::take(&mut *acks.lock().expect("the acks"));
        let most = self.sampler.most();
        let storage = self.store.storage();
        let waited = self.store.report().wait_background;
        let (bootstrapped, early) = first.unwrap_or_default();
        let rate = f64::from(backlog + receipts) / drained.as_secs_f64();
        println!(
            "first run: {backlog} messages and {receipts} receipts acked in {:.1} s, {rate:.0} a second one at a time; the app state's {} mutations fetched in {:.1} s; the history's {} messages stored in {:.1} s, the first {bootstrap} blobs' {} messages in {} chats in {:.1} s, and indexed {:.1} s later; all converted {:.1} s after the start",
            drained.as_secs_f64(),
            fetched.acks.len(),
            fetched.took.as_secs_f64(),
            got.messages,
            stored.as_secs_f64(),
            early.messages,
            early.conversations,
            bootstrapped.as_secs_f64(),
            indexed.as_secs_f64(),
            converted.as_secs_f64()
        );
        println!("{}", line("a message or receipt of the backlog, acked", &acks));
        println!("{}", line("  the open chat's, from hand-over to the interface", &arrivals));
        println!("{}", line("an app-state mutation, acked", &fetched.acks));
        println!("{}", line(&format!("a chat opened every {} ms", OPEN_EVERY.as_millis()), &opens));
        println!(
            "  background work waited for the WAL {} times, {:.1} s in all, {:.0} ms at p99; the WAL reached {:.0} MiB ({} frames); jawd's memory at most {:.0} MB ({:.0} MB resident){}",
            storage.background_waits,
            storage.background_waited_ms as f64 / 1000.0,
            waited.map_or(0.0, |w| w.p99_us as f64 / 1000.0),
            mib(most.wal),
            storage.wal_peak_frames,
            mb(most.footprint),
            mb(most.resident),
            if missing > 0 { format!("; {missing} messages never reached the interface") } else { String::new() }
        );
        self.record(
            "first_run",
            json!({
                "catch_up": {
                    "messages": backlog,
                    "receipts": receipts,
                    "acked_seconds": drained.as_secs_f64(),
                    "rate": rate,
                    "most_waiting": self.most_waiting.load(Ordering::Relaxed),
                    "ack": summary(&acks),
                    "arrival": summary(&arrivals),
                },
                "app_state": {
                    "mutations": fetched.acks.len(),
                    "seconds": fetched.took.as_secs_f64(),
                    "ack": summary(&fetched.acks),
                },
                "bootstrap": {
                    "blobs": bootstrap,
                    "conversations": early.conversations,
                    "messages": early.messages,
                    "seconds": bootstrapped.as_secs_f64(),
                    "store_seconds": early.store_seconds,
                },
                "history": {
                    "blobs": got.blobs,
                    "messages": got.messages,
                    "stored_seconds": stored.as_secs_f64(),
                    "store_seconds": got.store_seconds,
                    "indexed_seconds": indexed.as_secs_f64(),
                },
                "converted_seconds": converted.as_secs_f64(),
                "background_waits": storage.background_waits,
                "background_waited_ms": storage.background_waited_ms,
                "background_wait_p99_ms": waited.map_or(0.0, |w| w.p99_us as f64 / 1000.0),
                "open": summary(&opens),
                "wal_peak_mib": mib(most.wal),
                "wal_peak_frames": storage.wal_peak_frames,
                "peak_mb": mb(most.footprint),
                "peak_resident_mb": mb(most.resident),
                "missing": missing,
            }),
        )?;
        self.history_in.store(true, Ordering::Relaxed);
        let folder = self.socket.parent().context("the socket's folder")?;
        write_marker(folder, &Marker { shape: self.shape, history: true, ended: None })
    }

    /// Waits until the converter has taken every row the engine handed over.
    async fn converted(&self) -> Result<()> {
        while self.store.count("inbox")? > 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok(())
    }

    async fn watch(&self) -> Result<Watcher> {
        self.expected.take();
        let (socket, chat, expected) =
            (self.socket.clone(), self.account.chat(OPEN).to_string(), self.expected.clone());
        tokio::task::spawn_blocking(move || Watcher::start(&socket, &chat, expected)).await?
    }

    /// Lone messages after quiet spells, chats opened, pages at three depths and finds, with nothing
    /// else going on, or with the library paged while the lone messages come.
    async fn steady(&self, paging: Option<Paging>) -> Result<()> {
        let watcher = self.watch().await?;
        let halt = Arc::new(AtomicBool::new(false));
        let walking = paging.map(|paging| {
            let (socket, halt) = (self.socket.clone(), halt.clone());
            tokio::task::spawn_blocking(move || walk(&socket, paging, &halt))
        });
        let wal = {
            let (store, halt) = (self.store.clone(), halt.clone());
            tokio::spawn(async move {
                let mut most = 0;
                while !halt.load(Ordering::Relaxed) {
                    most = most.max(store.storage().wal_frames);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                most
            })
        };
        let load = load_average();
        let mut acks = Vec::new();
        for _ in 0..LONE {
            tokio::time::sleep(LONE_EVERY).await;
            acks.extend(self.deliver(OPEN, now_ms()).await);
        }
        self.converted().await?;
        tokio::time::sleep(LONE_EVERY).await;
        halt.store(true, Ordering::Relaxed);
        let walked = match walking {
            Some(walking) => Some(walking.await??),
            None => None,
        };
        let wal_most = wal.await?;
        self.expected.settled(STRAGGLERS).await;
        stop(watcher).await;
        let (came, missing) = self.expected.take();
        let arrivals: Vec<Duration> = came.iter().map(|c| c.0).collect();
        let from_commit = self.since_commit(&came);
        let paged = walked.map(|(took, deepest)| (paging.expect("paged as asked"), took, deepest));

        let (socket, account) = (self.socket.clone(), self.account.clone());
        let asked = tokio::task::spawn_blocking(move || -> Result<_> {
            let mut asker = Asker::open(&socket)?;
            let mut rng = Rng(0xC4A7);
            let chats = account.chats() as u64;
            let mut opens = Vec::new();
            for n in 0..OPENS {
                let c = match n % 3 {
                    0 => rng.below(chats.min(100)),
                    1 => rng.below(chats),
                    _ => rng.below(3),
                } as usize;
                opens.push(asker.open_chat(account.chat(c))?);
            }
            // The biggest chat's newest page, one from its middle and its oldest.
            let size = account.size(0);
            let mut pages = Vec::new();
            for (what, i) in [("newest", size - 1), ("middle", size / 2), ("oldest", PAGE as usize)] {
                let id = account.message_id(0, i);
                let took: Vec<Duration> =
                    (0..RUNS).map(|_| asker.page_before(account.chat(0), &id)).collect::<Result<_>>()?;
                pages.push((what, size - 1 - i, took));
            }
            let mut finds = Vec::new();
            for query in FINDS {
                let mut took = Vec::new();
                let mut matched = 0;
                for _ in 0..RUNS {
                    let (t, n) = asker.find(query)?;
                    took.push(t);
                    matched = n;
                }
                finds.push((*query, matched, took));
            }
            let shelves = library(&mut asker, &account)?;
            Ok((opens, pages, finds, shelves))
        });
        let (opens, pages, finds, shelves) = asked.await??;
        let usage = usage::now();
        println!("steady, with the account's {} messages:", self.account.shape.messages);
        if let Some((paging, took, deepest)) = &paged {
            println!(
                "{}",
                line(
                    &format!(
                        "while the lone messages come, the library's photos paged {} ms apart, {} of each page named on \
                        screen, to {deepest} in",
                        paging.apart.as_millis(),
                        paging.shown
                    ),
                    took
                )
            );
        }
        println!(
            "{}",
            line(&format!("a lone message, {} ms apart, acked (load {load:.0})", LONE_EVERY.as_millis()), &acks)
        );
        println!("{}", line("  from hand-over to the interface", &arrivals));
        println!("{}", line("  from its commit to the interface", &from_commit));
        println!("  the WAL held {wal_most} frames at most meanwhile");
        println!("{}", line(&format!("a chat opened, {OPENS} of them"), &opens));
        let mut numbers = json!({
            "lone_ack": summary(&acks),
            "lone_arrival": summary(&arrivals),
            "commit_to_interface": summary(&from_commit),
            "lone_wal_frames": wal_most,
            "lone_load": load,
            "open": summary(&opens),
            "missing": missing,
            "footprint_mb": mb(usage.footprint),
            "resident_mb": mb(usage.resident),
        });
        if let Some((paging, took, deepest)) = &paged {
            numbers["paging"] = json!({"apart_ms": paging.apart.as_millis() as u64, "shown": paging.shown, "deepest": deepest, "page": summary(took)});
        }
        for (what, depth, took) in &pages {
            println!("{}", line(&format!("the biggest chat's {what} page ({depth} messages in)"), took));
            numbers[format!("page_{what}")] = summary(took);
        }
        let mut all = Vec::new();
        for (query, matched, took) in &finds {
            println!("{}", line(&format!("find \"{query}\" in messages ({matched} matched)"), took));
            numbers[format!("find_{}", query.replace(' ', "_"))] = summary(took);
            all.extend(took);
        }
        numbers["find"] = summary(&all);
        let Library { windows, walk, deepest, loads } = shelves;
        println!("  the library, at a load of {:.0} to {:.0}:", loads.0, loads.1);
        for (key, what, count, took) in &windows {
            println!("{}", line(&format!("  {what} ({count} of them)"), took));
            numbers[format!("library_{key}")] = summary(took);
        }
        println!("{}", line(&format!("  its photos, 120 at a time from the newest to {deepest} in"), &walk));
        numbers["library_page"] = summary(&walk);
        numbers["library_page_deepest"] = json!(deepest);
        numbers["library_load"] = json!([loads.0, loads.1]);
        println!("  jawd's memory now {:.0} MB ({:.0} MB resident)", mb(usage.footprint), mb(usage.resident));
        self.record("steady", numbers)
    }

    /// Hands `event` over once a place in flight is free, timing its ack into `acks`, so at most
    /// `--in-flight` wait for theirs at once.
    async fn hand_over(&self, event: impl FnOnce() -> (pb::Event, Option<String>), acks: &Arc<Mutex<Vec<Duration>>>) {
        let Ok(place) = self.in_flight.clone().acquire_owned().await else { return };
        let (event, expected) = event();
        if let Some(id) = expected {
            self.expected.expect(id, Instant::now());
        }
        let waiting = self.waiting.fetch_add(1, Ordering::Relaxed) + 1;
        self.most_waiting.fetch_max(waiting, Ordering::Relaxed);
        let (world, acks, waiting) = (self.world.clone(), acks.clone(), self.waiting.clone());
        tokio::spawn(async move {
            if let Some(ack) = world.deliver(event).await {
                acks.lock().expect("the acks").push(ack);
            }
            waiting.fetch_sub(1, Ordering::Relaxed);
            drop(place);
        });
    }

    /// Waits until every message handed over has been acked.
    async fn landed(&self, in_flight: u32) {
        let _all = self.in_flight.acquire_many(in_flight.max(1)).await;
    }

    async fn flood(&self, rate: u32, seconds: f64, in_flight: u32) -> Result<()> {
        let watcher = self.watch().await?;
        let opening = self.keep_opening();
        self.sampler.most();
        let (pushes, lists) = self.expected.pushes();
        let presences = self.expected.presences();
        let total = (f64::from(rate) * seconds) as u64;
        let acks: Arc<Mutex<Vec<Duration>>> = Arc::default();
        let (began, cpu) = (Instant::now(), usage::now().cpu);
        let mut handed = 0u64;
        let mut tick = tokio::time::interval(Duration::from_millis(1));
        while handed < total {
            tick.tick().await;
            let due = ((f64::from(rate) * began.elapsed().as_secs_f64()) as u64).min(total);
            while handed < due {
                let c = self.next_chat();
                let id = self.next_id();
                let event = || {
                    let expected = (c == OPEN).then(|| id.clone());
                    (self.account.live(c, id, now_ms()), expected)
                };
                self.hand_over(event, &acks).await;
                handed += 1;
            }
        }
        self.landed(in_flight).await;
        let took = began.elapsed();
        let cpu = usage::now().cpu - cpu;
        let landed = Instant::now();
        self.converted().await?;
        let converted_after = landed.elapsed();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let opens = opening.stop().await?;
        let (pushed, listed) = self.expected.pushes();
        let presences = self.expected.presences() - presences;
        self.expected.settled(STRAGGLERS).await;
        let watcher_cpu = stop(watcher).await;
        let (came, missing) = self.expected.take();
        let arrivals: Vec<Duration> = came.iter().map(|c| c.0).collect();
        let from_commit = self.since_commit(&came);
        let acks = std::mem::take(&mut *acks.lock().expect("the acks"));
        let most = self.sampler.most();
        let wal_frames = self.store.storage().wal_peak_frames;
        let achieved = handed as f64 / took.as_secs_f64();
        let share = (cpu.saturating_sub(watcher_cpu)).as_secs_f64() / took.as_secs_f64() * 100.0;
        println!(
            "flood: {handed} messages in {:.1} s, {achieved:.0} a second of {rate} asked, at most {in_flight} waiting for acks at once, all converted {:.0} ms after the last ack; the WAL reached {:.0} MiB on disk ({wal_frames} frames at most since jawd started); jawd's memory at most {:.0} MB ({:.0} MB resident), its CPU {share:.0}% of a core with the demo engine's",
            took.as_secs_f64(),
            ms(converted_after),
            mib(most.wal),
            mb(most.footprint),
            mb(most.resident)
        );
        println!("{}", line("a message, acked", &acks));
        println!("{}", line("  the open chat's, from hand-over to the interface", &arrivals));
        println!("{}", line("  from its commit to the interface", &from_commit));
        println!("{}", line(&format!("a chat opened every {} ms", OPEN_EVERY.as_millis()), &opens));
        let seconds_taken = took.as_secs_f64();
        println!(
            "  the interface had {:.0} pushes a second, {:.1} of them for the chat list{}",
            (pushed - pushes) as f64 / seconds_taken,
            (listed - lists) as f64 / seconds_taken,
            if missing > 0 { format!("; {missing} messages never reached it") } else { String::new() }
        );
        self.record(
            "flood",
            json!({
                "rate": rate,
                "seconds": seconds_taken,
                "in_flight": in_flight,
                "messages": handed,
                "achieved": achieved,
                "converted_after_ms": ms(converted_after),
                "ack": summary(&acks),
                "arrival": summary(&arrivals),
                "commit_to_interface": summary(&from_commit),
                "open": summary(&opens),
                "cpu_percent": share,
                "wal_peak_mib": mib(most.wal),
                "wal_peak_frames": wal_frames,
                "peak_mb": mb(most.footprint),
                "peak_resident_mb": mb(most.resident),
                "lifetime_peak_mb": mb(usage::now().peak),
                "pushes_per_second": (pushed - pushes) as f64 / seconds_taken,
                "list_pushes_per_second": (listed - lists) as f64 / seconds_taken,
                "missing": missing,
                "typing": self.state.feed().map(|feed| feed.typing().stats()),
                "presence": self.state.status().presence,
                "presence_pushes_per_second": presences as f64 / seconds_taken,
            }),
        )
    }

    /// Messages a second until jawd demo stops, untimed: a demo of a busy account.
    async fn pour(&self, rate: u32) {
        println!("jawd demo: {rate} messages a second until Ctrl-C");
        let acks: Arc<Mutex<Vec<Duration>>> = Arc::default();
        let every = Duration::from_secs_f64(1.0 / f64::from(rate.max(1)));
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            let (c, id) = (self.next_chat(), self.next_id());
            self.hand_over(|| (self.account.live(c, id, now_ms()), None), &acks).await;
            acks.lock().expect("the acks").clear();
        }
    }

    /// What waited while the account was offline arrives as it connects: WhatsApp says how much is
    /// coming, then the messages, sent since the last run stopped (two days at most) to chats all
    /// over the list, so each is the newest in its chat, with the receipts for messages of yours
    /// among them, then that it's all come.
    /// `backlog` messages sent while the account was offline, to chats all over the list, so each
    /// is the newest in its chat, with the receipts for messages of yours among them, each with the
    /// ID the watcher expects if it's in the open chat; and how many receipts.
    fn backlog(&self, backlog: u32) -> (Vec<(pb::Event, Option<String>)>, u32) {
        let now = now_ms();
        let chats = self.account.chats() as u64;
        let mut events = Vec::with_capacity(backlog as usize);
        let mut receipts = 0u32;
        let mut rng = self.rng.lock().expect("the demo's dice");
        for n in 0..u64::from(backlog) {
            let c = match rng.below(8) {
                0 => OPEN,
                1..4 => rng.below(chats.min(50)) as usize,
                _ => rng.below(chats) as usize,
            };
            let offline = (now - self.offline_since).max(1);
            let at = self.offline_since + (n as i64 * offline / i64::from(backlog.max(1)));
            let id = self.next_id();
            events.push((self.account.live(c, id.clone(), at), (c == OPEN).then_some(id)));
            if n % 50 == 49
                && let Some(read) = self.account.read(rng.below(chats) as usize, at)
            {
                events.push((read, None));
                receipts += 1;
            }
        }
        (events, receipts)
    }

    async fn catch_up(&self, backlog: u32, in_flight: u32) -> Result<()> {
        let watcher = self.watch().await?;
        self.sampler.most();
        let (pushes, lists) = self.expected.pushes();
        let (events, receipts) = self.backlog(backlog);
        let preview =
            pb::OfflinePreview { total: backlog + receipts, messages: backlog, receipts, ..Default::default() };
        let acks: Arc<Mutex<Vec<Duration>>> = Arc::default();
        let began = Instant::now();
        self.world.deliver(connection(pb::connection::State::Connected)).await;
        self.world.deliver(pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) }).await;
        for (event, expected) in events {
            self.hand_over(|| (event, expected), &acks).await;
        }
        self.landed(in_flight).await;
        let drained = began.elapsed();
        let done = pb::OfflineDone { count: backlog + receipts };
        self.world.deliver(pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflineDone(done)) }).await;
        let converted = Instant::now();
        self.converted().await?;
        let all_in = drained + converted.elapsed();
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.expected.settled(STRAGGLERS).await;
        let (pushed, listed) = self.expected.pushes();
        stop(watcher).await;
        let (came, missing) = self.expected.take();
        let arrivals: Vec<Duration> = came.iter().map(|c| c.0).collect();
        let from_commit = self.since_commit(&came);
        let acks = std::mem::take(&mut *acks.lock().expect("the acks"));
        let most = self.sampler.most();
        let wal_frames = self.store.storage().wal_peak_frames;
        let rate = f64::from(backlog + receipts) / drained.as_secs_f64();
        println!(
            "catch-up: {backlog} messages and {receipts} receipts waiting, all acked in {:.1} s, {rate:.0} a second with at most {in_flight} waiting for acks at once, and all converted in {:.1} s; the WAL reached {:.0} MiB on disk ({wal_frames} frames at most since jawd started); jawd's memory at most {:.0} MB ({:.0} MB resident)",
            drained.as_secs_f64(),
            all_in.as_secs_f64(),
            mib(most.wal),
            mb(most.footprint),
            mb(most.resident)
        );
        println!("{}", line("a message or receipt, acked", &acks));
        println!("{}", line("  the open chat's, from hand-over to the interface", &arrivals));
        println!("{}", line("  from its commit to the interface", &from_commit));
        let seconds = all_in.as_secs_f64();
        println!(
            "  the interface had {:.0} pushes a second, {:.1} of them for the chat list{}",
            (pushed - pushes) as f64 / seconds,
            (listed - lists) as f64 / seconds,
            if missing > 0 { format!("; {missing} messages never reached it") } else { String::new() }
        );
        self.record(
            "catch_up",
            json!({
                "messages": backlog,
                "receipts": receipts,
                "in_flight": in_flight,
                "acked_seconds": drained.as_secs_f64(),
                "converted_seconds": all_in.as_secs_f64(),
                "rate": rate,
                "ack": summary(&acks),
                "arrival": summary(&arrivals),
                "commit_to_interface": summary(&from_commit),
                "wal_peak_mib": mib(most.wal),
                "wal_peak_frames": wal_frames,
                "peak_mb": mb(most.footprint),
                "peak_resident_mb": mb(most.resident),
                "lifetime_peak_mb": mb(usage::now().peak),
                "pushes_per_second": (pushed - pushes) as f64 / seconds,
                "list_pushes_per_second": (listed - lists) as f64 / seconds,
                "missing": missing,
            }),
        )
    }

    /// Nothing happens for `seconds`, with an interface watching as jaw would, while the kernel
    /// counts how often jawd's threads wake and the CPU they spend.
    async fn idle(&self, seconds: f64) -> Result<()> {
        let watcher = self.watch().await?;
        self.sampler.sampling(false);
        let settling = Instant::now();
        loop {
            let last = self.commits.lock().expect("the commits").back().map(|&(_, at)| at);
            let quiet = last.map_or(settling.elapsed(), |at| at.elapsed());
            if quiet >= SETTLE {
                break;
            }
            tokio::time::sleep(SETTLE - quiet).await;
        }
        let settled = settling.elapsed();
        println!("jawd demo: quiet after {:.0} s; counting for {seconds:.0} s", settled.as_secs_f64());
        let (pushes, _) = self.expected.pushes();
        let before = usage::now();
        tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
        let after = usage::now();
        let (pushed, _) = self.expected.pushes();
        stop(watcher).await;
        self.sampler.sampling(true);
        let per_minute = |n: u64| n as f64 * 60.0 / seconds;
        let (wakeups, switches) = (after.wakeups - before.wakeups, after.switches - before.switches);
        let cpu = after.cpu.saturating_sub(before.cpu);
        println!(
            "idle for {seconds:.0} s, an interface watching: jawd woke {wakeups} times ({:.1} a minute), switched {switches} times, spent {:.1} ms of CPU and sent {} pushes; its memory {:.0} MB ({:.0} MB resident)",
            per_minute(wakeups),
            ms(cpu),
            pushed - pushes,
            mb(after.footprint),
            mb(after.resident)
        );
        self.record(
            "idle",
            json!({
                "seconds": seconds,
                "settled_seconds": settled.as_secs_f64(),
                "wakeups": wakeups,
                "wakeups_per_minute": per_minute(wakeups),
                "switches": switches,
                "cpu_ms": ms(cpu),
                "pushes": pushed - pushes,
                "footprint_mb": mb(after.footprint),
                "resident_mb": mb(after.resident),
            }),
        )
    }
}

/// Ends the watcher off the runtime's threads, and says the CPU its thread spent.
async fn stop(watcher: Watcher) -> Duration {
    tokio::task::spawn_blocking(move || watcher.stop()).await.unwrap_or_default()
}

/// Chats opened on a thread of their own while something else goes on.
struct Opening {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<Result<Vec<Duration>>>,
}

impl Opening {
    /// Stops opening chats, and says how long each took.
    async fn stop(self) -> Result<Vec<Duration>> {
        self.stop.store(true, Ordering::Relaxed);
        let thread = self.thread;
        tokio::task::spawn_blocking(move || thread.join())
            .await?
            .map_err(|_| anyhow::anyhow!("the demo's thread opening chats panicked"))?
    }
}

/// Notes when each seq was first seen committed, so a message can be timed from its commit.
async fn log_commits(store: Store, log: Arc<Mutex<VecDeque<(i64, Instant)>>>) {
    let mut committed = store.committed();
    while committed.changed().await.is_ok() {
        let seq = *committed.borrow_and_update();
        let mut log = log.lock().expect("the commits");
        if log.len() == COMMITS_KEPT {
            log.pop_front();
        }
        log.push_back((seq, Instant::now()));
    }
}

#[cfg(test)]
mod tests;
