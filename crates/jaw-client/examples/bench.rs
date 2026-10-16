//! Measures the client: what applying a frame to a full 100-message window costs, then the client
//! live on `jawd bench feed`, which serves a made-up account while it writes messages at a steady
//! rate, watching the chat list and the open chat as an interface would: how long each line waits
//! from being read to its model being updated, each frame's apply, and the client's CPU and memory.
//!
//! `cargo run --release -p jaw-client --example bench -- --rate 2000 --seconds 10`

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jaw_client::{Chats, Client, Config, Event, Merged, Messages, Place, Update};
use jaw_proto::windows::{ChatRow, MessageRow};
use serde_json::json;

/// `jawd bench feed`'s open chat.
const OPEN: &str = "351900000000@s.whatsapp.net";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: u32| -> Result<u32> {
        match args.iter().position(|a| a == name) {
            Some(i) => args.get(i + 1).context("a number after it")?.parse().context(name.to_string()),
            None => Ok(default),
        }
    };
    let (rate, seconds) = (arg("--rate", 2_000)?, arg("--seconds", 10)?);
    apply_cost();
    live(rate, seconds)
}

fn at(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn us(d: Duration) -> String {
    format!("{:.1} µs", d.as_secs_f64() * 1e6)
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1e3)
}

fn spread(times: &mut [Duration], show: fn(Duration) -> String) -> String {
    times.sort();
    format!(
        "p50 {}, p99 {}, max {}",
        show(at(times, 0.5)),
        show(at(times, 0.99)),
        show(times.last().copied().unwrap_or_default())
    )
}

/// A message row as jawd sends one, with a sentence of text.
fn message(n: u64) -> MessageRow {
    serde_json::from_value(json!({
        "id": format!("3EB0{n:012X}"), "v": n, "sort": format!("{:013}~{n:012}", 1_792_145_060_000u64 + n),
        "chat": OPEN, "at": 1_792_145_060_000u64 + n, "from": {"id": "person:12", "name": "Ana", "colour": 4},
        "mine": false, "kind": "text",
        "spans": [{"t": "text", "s": "did you get the "}, {"t": "bold", "s": "photos"}, {"t": "text", "s": " from Sunday?"}],
        "reply": null, "forwarded": 0, "reactions": [{"emoji": "❤", "n": 2, "mine": true}], "status": null,
        "edited": false, "deleted": false, "expires_at": null, "view_once": false, "starred": false
    }))
    .expect("a message row")
}

/// Frames as a flood brings them to a full window of 100: at 500 messages a second into the open
/// chat and 60 frames a second, 8 come in and 8 leave each frame. Then a frame that replaces the
/// whole window, as moving far back does, and the merge on the reading thread that makes a frame
/// of three pushes.
fn apply_cost() {
    let full = || {
        let mut model: Messages<MessageRow> = Messages::new(OPEN, Place::newest(100));
        model.apply(Update { upsert: (0..100).map(message).collect(), ..Update::default() });
        model
    };
    let mut model = full();
    let mut times = Vec::new();
    let mut next = 100u64;
    for _ in 0..20_000 {
        let update = Update {
            seq: next,
            remove: (next - 100..next - 92).map(|n| message(n).id).collect(),
            upsert: (next..next + 8).map(message).collect(),
            newer: Some(0),
            ..Update::default()
        };
        let started = Instant::now();
        model.apply(update);
        times.push(started.elapsed());
        next += 8;
    }
    assert_eq!(model.len(), 100);
    println!("a frame applied to a full 100-message window:");
    println!("  8 in, 8 out      {}", spread(&mut times, us));

    let mut times = Vec::new();
    for round in 0..2_000u64 {
        let mut model = full();
        let base = 1_000 + round * 100;
        let update = Update {
            remove: (0..100).map(|n| message(n).id).collect(),
            upsert: (base..base + 100).map(message).collect(),
            ..Update::default()
        };
        let started = Instant::now();
        model.apply(update);
        times.push(started.elapsed());
    }
    println!("  all 100 replaced {}", spread(&mut times, us));

    let mut times = Vec::new();
    let mut ids: std::collections::HashSet<String> = (0..100).map(|n| message(n).id).collect();
    let mut next = 100u64;
    for _ in 0..5_000 {
        let pushes: Vec<Update<MessageRow>> = (0..3)
            .map(|k| Update {
                seq: next + k,
                remove: (next + k * 3 - 100..next + k * 3 - 97).map(|n| message(n).id).collect(),
                upsert: (next + k * 3..next + k * 3 + 3).map(message).collect(),
                ..Update::default()
            })
            .collect();
        let started = Instant::now();
        let mut merged = Merged::default();
        for push in pushes {
            merged.add(push, &mut ids);
        }
        let update = merged.into_update();
        times.push(started.elapsed());
        assert_eq!(update.upsert.len(), 9);
        next += 9;
    }
    println!("  3 pushes merged  {} (on the reading thread)\n", spread(&mut times, us));
}

/// The process's CPU time so far, user and system, and its largest resident size in bytes.
fn usage() -> (Duration, u64) {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: getrusage fills the struct it's given.
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let time = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
    let rss = usage.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") { rss } else { rss * 1024 };
    (time(usage.ru_utime) + time(usage.ru_stime), bytes)
}

fn live(rate: u32, seconds: u32) -> Result<()> {
    let exe = std::env::current_exe()?;
    let profile = exe.parent().and_then(Path::parent).context("the example sits in target/<profile>/examples")?;
    let mut build = Command::new("cargo");
    build.args(["build", "--quiet", "-p", "jawd", "--bin", "jawd"]);
    if profile.file_name().is_some_and(|name| name == "release") {
        build.arg("--release");
    }
    if !build.status()?.success() {
        bail!("jawd didn't build");
    }
    let scratch = tempfile::tempdir()?;
    let mut bench = Command::new(profile.join("jawd"))
        .args(["bench", "feed", "--rate", &rate.to_string(), "--seconds", &seconds.to_string()])
        .env("TMPDIR", scratch.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let socket = scratch.path().join(format!("jaw-bench-feed-{}", bench.id())).join("jaw.sock");
    while !socket.exists() {
        if bench.try_wait()?.is_some() {
            bail!("jawd bench feed ended before it served");
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let client: Client = Client::start(Config::new(&socket, "bench", "0"), || {});
    let list = client.watch_chats("all", Place::top(60));
    let open = client.watch_messages(OPEN, Place::newest(100));
    let mut chats: Chats<ChatRow> = Chats::new("all", Place::top(60));
    let mut messages: Messages<MessageRow> = Messages::new(OPEN, Place::newest(100));
    let (cpu, _) = usage();
    let started = Instant::now();
    let (mut waits, mut applies) = (Vec::new(), Vec::new());
    'frames: loop {
        if !client.wait(Duration::from_secs(1)) {
            continue;
        }
        let frame = client.drain();
        let applying = Instant::now();
        for (window, update) in frame.chats {
            if window == list {
                chats.apply(update);
            }
        }
        for (window, update) in frame.messages {
            if window == open {
                messages.apply(update);
            }
        }
        let applied = Instant::now();
        applies.push(applied - applying);
        waits.extend(frame.read.iter().map(|read| applied - *read));
        for event in frame.events {
            if let Event::Disconnected { .. } = event {
                break 'frames;
            }
        }
    }
    let took = started.elapsed();
    let (cpu_after, peak) = usage();
    let stats = client.stats();
    drop(client);
    let mut report = String::new();
    if let Some(mut out) = bench.stdout.take() {
        out.read_to_string(&mut report)?;
    }
    bench.wait()?;

    println!(
        "the client on jawd bench feed at {rate} messages a second for {} s, watching the top 60 chats and the open chat's newest 100:",
        seconds * 2
    );
    println!("  read to applied  {} ({} lines)", spread(&mut waits, ms), stats.lines);
    println!(
        "  frames           {}, {:.1} a second, each applied in {}",
        stats.frames,
        stats.frames as f64 / took.as_secs_f64(),
        spread(&mut applies, us)
    );
    println!(
        "  CPU              {:.1}% of a core over {:.1} s, the client's threads and the applying loop together",
        (cpu_after - cpu).as_secs_f64() / took.as_secs_f64() * 100.0,
        took.as_secs_f64()
    );
    println!("  memory           {:.1} MB at most, the whole process", peak as f64 / 1e6);
    println!("  windows          {} chats and {} messages at the end\n", chats.len(), messages.len());
    println!("jawd's own report:\n{report}");
    Ok(())
}
