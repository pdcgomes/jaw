//! `jawd bench feed`: an interface on the real socket watching the chat list and a chat's newest
//! messages while made-up messages are written at a steady rate, as the converter would write
//! them, with a few deleted for me; then messages one at a time after quiet spells, and again
//! while the unread counts and names of chats no window shows keep changing. It reports the
//! batches a second each window got during the burst, how long each message took from its commit
//! to the first interface reading its push, in the burst and alone, how often windows were read,
//! and the CPU jawd spent, with the writes alone first so the windows' share shows. Several
//! interfaces can watch at once.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jaw_proto::FromJawd;
use jaw_proto::windows::{ChatsPush, MessagesPush};
use serde_json::json;
use tokio::sync::broadcast;

use super::{Feed, Timing};
use crate::state::State;
use crate::store::{Change, Lane, Message, Person, Read, Store};

pub struct Plan {
    pub rate: u32,
    pub seconds: u32,
    pub window: u32,
    pub chats: u32,
    /// Messages written one at a time after the burst, each after a quiet spell, and as many again
    /// while other chats change.
    pub lone: u32,
    /// Interfaces watching the same windows; the first is the one timed.
    pub interfaces: u32,
    /// Chats that never reach the top of the list, whose unread counts and names keep changing
    /// while lone messages are timed a second time; none leaves that out.
    pub far: u32,
}

/// How long the feed is quiet before each lone message.
const LONE_EVERY: Duration = Duration::from_millis(200);
/// Changes a second to the far chats.
const FAR_RATE: u64 = 2_000;

fn far(n: usize) -> String {
    format!("35195{n:07}@s.whatsapp.net")
}

/// The chat the interface has open, and how many messages it has before the bench writes.
const OPEN: &str = "351900000000@s.whatsapp.net";
const OPEN_HISTORY: usize = 20_000;
/// Messages each other chat has before the bench writes.
const HISTORY: usize = 100;
/// The chat-list window's rows.
const CHAT_ROWS: u32 = 60;
/// One message in this many goes to the open chat.
const OPEN_SHARE: u64 = 4;
/// One message in this many is deleted for me, one the open chat had a moment before.
const DELETE_EVERY: u64 = 40;

fn chat(n: u64) -> String {
    if n == 0 { OPEN.into() } else { format!("3519{n:08}@s.whatsapp.net") }
}

fn message(chat: String, id: String, at: i64) -> Change {
    Change::Message(Message {
        sender: Some(chat.clone()),
        chat,
        id,
        at,
        kind: "text",
        text: Some("did you get the photos from Sunday?".into()),
        summary: "did you get the photos from Sunday?".into(),
        ..Message::default()
    })
}

/// The process's CPU time so far, user and system.
fn process_cpu() -> Duration {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: getrusage fills the struct it's given.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let time = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
    time(usage.ru_utime) + time(usage.ru_stime)
}

/// The calling thread's CPU time so far.
fn thread_cpu() -> Duration {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime fills the timespec it's given.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) };
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

pub async fn run(scratch: &Path, plan: Plan) -> Result<()> {
    let store = Store::open(scratch)?;
    let mut history = Vec::new();
    let start = 1_700_000_000_000i64;
    for n in 0..OPEN_HISTORY {
        history.push(message(OPEN.into(), format!("h0-{n}"), start + n as i64 * 1000));
    }
    for c in 1..u64::from(plan.chats) {
        for n in 0..HISTORY {
            history.push(message(chat(c), format!("h{c}-{n}"), start + (n as i64 * 97 + c as i64) * 1000));
        }
    }
    for n in 0..plan.far as usize {
        history.push(message(far(n), format!("far{n}"), start - 1_000_000_000 + n as i64));
    }
    let seeded = history.len();
    store.apply(Lane::Background, history).await?;

    let feed = Feed::start(store.clone(), Arc::new(|| false), Timing::default());
    let socket = jaw_proto::local::socket(scratch);
    let listener = crate::sock::listen(&socket)?;
    let state = State::new(
        scratch,
        Err(("demo", "no engine in this bench".into())),
        Vec::new(),
        Arc::default(),
        Some(store.clone()),
        broadcast::channel(16).0,
    )?
    .with_feed(feed.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));

    let seconds = Duration::from_secs(u64::from(plan.seconds));
    let commits = Arc::new(Mutex::new(HashMap::new()));
    let alone = write(&store, &plan, seconds, "a", commits.clone()).await?;

    let stop = Arc::new(AtomicBool::new(false));
    let mut clients = Vec::new();
    for _ in 0..plan.interfaces.max(1) {
        let (watching, watched) = std::sync::mpsc::channel();
        let (stop, window, socket) = (stop.clone(), plan.window, socket.clone());
        clients.push(std::thread::spawn(move || watch(socket, window, watching, stop)));
        watched.recv().context("the bench's interface couldn't watch")??;
    }
    let reads = feed.reads();
    let started = Instant::now();
    let together = write(&store, &plan, seconds, "b", commits.clone()).await?;
    let wrote = started.elapsed();
    let burst_reads = feed.reads() - reads;
    let burst_ended = Instant::now() + Duration::from_millis(200);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let quiet = lone(&store, &feed, plan.lone, "l", &commits).await?;
    let busy = if plan.far > 0 {
        let writing = Arc::new(AtomicBool::new(true));
        let changing = tokio::spawn(far_changes(store.clone(), plan.far as usize, writing.clone()));
        let busy = lone(&store, &feed, plan.lone, "m", &commits).await?;
        writing.store(false, Ordering::Relaxed);
        Some((busy, changing.await??))
    } else {
        None
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    stop.store(true, Ordering::Relaxed);
    let mut seens = Vec::new();
    for client in clients {
        seens.push(client.join().map_err(|_| anyhow::anyhow!("the bench's interface panicked"))??);
    }
    let watching_cpu: Duration = seens.iter().map(|seen| seen.cpu).sum();
    let mut seen = seens.swap_remove(0);
    seen.chats.retain(|&at| at < burst_ended);
    seen.messages.retain(|&at| at < burst_ended);

    let commits = commits.lock().expect("commit times");
    let delays_of = |run: &str| {
        let mut delays: Vec<Duration> = seen
            .rows
            .iter()
            .filter(|(id, _)| id.starts_with(run))
            .filter_map(|(id, read)| commits.get(id).map(|committed| read.saturating_duration_since(*committed)))
            .collect();
        delays.sort();
        delays
    };
    let (delays, alone_delays, busy_delays) = (delays_of("b"), delays_of("l"), delays_of("m"));
    let percentile = |delays: &[Duration], p: f64| {
        let at = delays.get(((delays.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or_default();
        at.as_secs_f64() * 1e3
    };
    let per_second = |n: usize| n as f64 / wrote.as_secs_f64();
    let busiest = |times: &[Instant]| {
        let mut most = 0;
        for (i, &at) in times.iter().enumerate() {
            most = most.max(times[i..].iter().take_while(|&&t| t < at + Duration::from_secs(1)).count());
        }
        most
    };
    let share = |cpu: Duration, over: Duration| cpu.as_secs_f64() / over.as_secs_f64() * 100.0;
    let interfaces = match plan.interfaces.max(1) {
        1 => "an interface".to_string(),
        n => format!("{n} interfaces, each"),
    };
    println!(
        "{seeded} messages in {} chats first, the open chat with {OPEN_HISTORY}; then {} messages a second for {} s, one in {OPEN_SHARE} to the open chat, with {interfaces} watching the top {CHAT_ROWS} chats and the open chat's newest {}:\n",
        plan.chats + plan.far,
        plan.rate,
        plan.seconds,
        plan.window
    );
    println!(
        "chat list        {} batches, {:.1} a second, at most {} in any second; {:.0} window reads a second in all",
        seen.chats.len(),
        per_second(seen.chats.len()),
        busiest(&seen.chats),
        burst_reads as f64 / wrote.as_secs_f64()
    );
    println!(
        "messages window  {} batches, {:.1} a second, at most {} in any second; {} new messages in them",
        seen.messages.len(),
        per_second(seen.messages.len()),
        busiest(&seen.messages),
        delays.len()
    );
    println!(
        "commit to push   p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms (committed to read by the interface)",
        percentile(&delays, 0.5),
        percentile(&delays, 0.99),
        percentile(&delays, 1.0)
    );
    println!(
        "a lone message   p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms from commit to the open chat's window, for {} messages {} ms apart after the burst; {:.0} window reads a second",
        percentile(&alone_delays, 0.5),
        percentile(&alone_delays, 0.99),
        percentile(&alone_delays, 1.0),
        alone_delays.len(),
        LONE_EVERY.as_millis(),
        quiet.reads_per_second
    );
    if let Some((busy, far_changed)) = busy {
        println!(
            "  while busy     p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms, for {} more while {far_changed} unread counts and names changed in {} chats no window shows, {FAR_RATE} a second; {:.0} window reads a second",
            percentile(&busy_delays, 0.5),
            percentile(&busy_delays, 0.99),
            percentile(&busy_delays, 1.0),
            busy_delays.len(),
            plan.far,
            busy.reads_per_second
        );
    }
    println!(
        "jawd's CPU       {:.1}% of a core writing alone, {:.1}% with {} watching (their own parsing left out, {:.1}%)",
        share(alone.cpu, alone.took),
        share(together.cpu.saturating_sub(watching_cpu), together.took),
        if plan.interfaces > 1 { "the interfaces" } else { "the interface" },
        share(watching_cpu, together.took)
    );
    if alone.behind > Duration::from_millis(100) || together.behind > Duration::from_millis(100) {
        println!(
            "the writer fell behind its rate by {:.0} ms alone and {:.0} ms with the interface watching",
            alone.behind.as_secs_f64() * 1e3,
            together.behind.as_secs_f64() * 1e3
        );
    }
    Ok(())
}

struct Lone {
    reads_per_second: f64,
}

/// Writes `count` messages to the open chat one at a time, `LONE_EVERY` apart, noting when each
/// was committed, and says how often windows were read meanwhile.
async fn lone(
    store: &Store,
    feed: &Feed,
    count: u32,
    run: &str,
    commits: &Mutex<HashMap<String, Instant>>,
) -> Result<Lone> {
    let (reads, started) = (feed.reads(), Instant::now());
    for n in 0..count {
        tokio::time::sleep(LONE_EVERY).await;
        let id = format!("{run}{n}");
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as i64;
        store.apply(Lane::Live, vec![message(OPEN.into(), id.clone(), at)]).await?;
        commits.lock().expect("commit times").insert(id, Instant::now());
    }
    Ok(Lone { reads_per_second: (feed.reads() - reads) as f64 / started.elapsed().as_secs_f64() })
}

/// Changes the unread counts and names of chats far down the list, `FAR_RATE` a second, each in
/// a transaction of its own, until told to stop; each touches every chat list without changing
/// what it shows. Says how many it made.
async fn far_changes(store: Store, chats: usize, writing: Arc<AtomicBool>) -> Result<u64> {
    let started = Instant::now();
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as i64;
    let mut n = 0u64;
    while writing.load(Ordering::Relaxed) {
        let to = far(n as usize % chats);
        let change = if n.is_multiple_of(2) {
            Change::Read(Read { chat: to, at, ids: Vec::new(), phone: Some(((n % 7) as u32, 0)), here: false })
        } else {
            Change::Person(Person { jids: vec![to], name: Some(format!("Someone {n}")), ..Person::default() })
        };
        store.apply(Lane::Live, vec![change]).await?;
        n += 1;
        tokio::time::sleep_until((started + Duration::from_micros(n * 1_000_000 / FAR_RATE)).into()).await;
    }
    Ok(n)
}

struct Wrote {
    took: Duration,
    cpu: Duration,
    /// How far the last write ended past when the rate would have finished it.
    behind: Duration,
}

/// Writes `plan.rate` messages a second for `seconds`, a millisecond's worth to a transaction,
/// noting when each was committed.
async fn write(
    store: &Store,
    plan: &Plan,
    seconds: Duration,
    run: &str,
    commits: Arc<Mutex<HashMap<String, Instant>>>,
) -> Result<Wrote> {
    let total = u64::from(plan.rate) * seconds.as_secs();
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let started = Instant::now();
    let cpu = process_cpu();
    let mut written = 0u64;
    let mut tick = tokio::time::interval(Duration::from_millis(1));
    // Each run's messages are newer than the last's, as new messages are.
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as i64;
    while written < total {
        tick.tick().await;
        let due = (u64::from(plan.rate) * started.elapsed().as_millis() as u64 / 1000).min(total);
        let mut batch = Vec::new();
        let mut ids = Vec::new();
        while written < due {
            if written >= DELETE_EVERY && written.is_multiple_of(DELETE_EVERY) {
                let id = format!("{run}{}", written - DELETE_EVERY);
                batch.push(Change::DeleteMessage { chat: OPEN.into(), id });
            }
            let to = if written.is_multiple_of(OPEN_SHARE) { 0 } else { 1 + next() % u64::from(plan.chats - 1).max(1) };
            let id = format!("{run}{written}");
            batch.push(message(chat(to), id.clone(), at + (written * 1000 / u64::from(plan.rate)) as i64));
            ids.push(id);
            written += 1;
        }
        if batch.is_empty() {
            continue;
        }
        store.apply(Lane::Live, batch).await?;
        let committed = Instant::now();
        let mut commits = commits.lock().expect("commit times");
        for id in ids {
            commits.insert(id, committed);
        }
    }
    let took = started.elapsed();
    Ok(Wrote { took, cpu: process_cpu() - cpu, behind: took.saturating_sub(seconds) })
}

/// What the interface saw: when each window's batches came, when each new message was read, and
/// the CPU it spent.
struct Seen {
    chats: Vec<Instant>,
    messages: Vec<Instant>,
    rows: Vec<(String, Instant)>,
    cpu: Duration,
}

/// The interface: watches the chat list and the open chat, then reads pushes until told to stop.
fn watch(
    socket: PathBuf,
    window: u32,
    watching: std::sync::mpsc::Sender<Result<()>>,
    stop: Arc<AtomicBool>,
) -> Result<Seen> {
    let mut stream = UnixStream::connect(&socket)?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let requests = [
        json!({"id": 1, "method": "hello", "params": {"protocol": jaw_proto::PROTOCOL, "client": "bench", "version": "0"}}),
        json!({"id": 2, "method": "chats.watch", "params": {"folder": "all", "around": "top", "after": CHAT_ROWS}}),
        json!({"id": 3, "method": "messages.watch", "params": {"chat": OPEN, "around": "newest", "before": window}}),
    ];
    let mut line = String::new();
    for request in requests {
        stream.write_all(format!("{request}\n").as_bytes())?;
        line.clear();
        while reader.read_line(&mut line).is_err() {}
        if line.contains("\"error\"") {
            let _ = watching.send(Err(anyhow::anyhow!("jawd refused the bench's request: {line}")));
            bail!("jawd refused the bench's request: {line}");
        }
    }
    let _ = watching.send(Ok(()));
    let cpu = thread_cpu();
    let mut seen = Seen { chats: Vec::new(), messages: Vec::new(), rows: Vec::new(), cpu: Duration::ZERO };
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => bail!("jawd closed the bench's connection"),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        let read = Instant::now();
        let FromJawd::Push(push) = serde_json::from_str(&line)? else { continue };
        match push.method.as_str() {
            "chats" => {
                let _: ChatsPush = serde_json::from_value(push.params)?;
                seen.chats.push(read);
            }
            "messages" => {
                let push: MessagesPush = serde_json::from_value(push.params)?;
                seen.messages.push(read);
                // Only the messages window's pushes are timed: a lone message shows there first.
                seen.rows.extend(push.upsert.into_iter().map(|row| (row.id, read)));
            }
            _ => {}
        }
    }
    seen.cpu = thread_cpu() - cpu;
    Ok(seen)
}
