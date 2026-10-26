//! `jawd bench media-out`: a photo's send, as an interface sees it. A 12-megapixel photo of yours is
//! sent with `send.media` on the real socket, one at a time, and timed to the answer, to the push
//! whose row shows its copy made and in the cache (ready to upload: its probe, its SD copy, and its
//! HD one with `--hd`, made from one decode, hashed, synced and committed), and to the push whose
//! row says it was sent (uploaded by the demo engine as jaw-engine streams one, then sent). All the
//! while, texts arrive through the demo engine, each timed to its ack; a quiet spell comes before
//! each photo, so what sending does to a message's arrival shows beside what it was without, under
//! the same load.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jaw_proto::FromJawd;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::super::{Config, Media, now_ms};
use crate::engine::demo::{self, DemoEngine, Picture, Replies, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::feed::{Feed, Timing};
use crate::send::Sending;
use crate::state::State;
use crate::store::Store;

pub struct Plan {
    pub count: u32,
    pub width: u32,
    pub height: u32,
    pub hd: bool,
    /// Milliseconds between two texts arriving.
    pub every: u64,
}

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
/// How long texts arrive with no photo going, before each photo and after the last.
const QUIET: Duration = Duration::from_millis(1_500);

/// When a text arrived, and how long its ack took.
type Ack = (Instant, Duration);

/// When the interface saw each step of one photo's send.
#[derive(Default)]
struct Seen {
    ready: Option<Instant>,
    sent: Option<Instant>,
}

pub async fn run(scratch: &Path, plan: Plan) -> Result<()> {
    let picked = scratch.join("picked");
    std::fs::create_dir_all(&picked)?;
    let made = Instant::now();
    let paths: Vec<std::path::PathBuf> = (0..plan.count)
        .map(|n| {
            let path = picked.join(format!("photo-{n}.jpg"));
            std::fs::write(&path, Picture::numbered(plan.width, plan.height, n + 1).data.as_slice()).map(|()| path)
        })
        .collect::<std::io::Result<_>>()?;
    let size = paths.first().map_or(0, |p| std::fs::metadata(p).map_or(0, |m| m.len()));
    eprintln!(
        "made {} photos of {}x{} ({:.1} MB each) in {:.1} s",
        plan.count,
        plan.width,
        plan.height,
        size as f64 / 1e6,
        made.elapsed().as_secs_f64()
    );

    let store = Store::open(scratch)?;
    crate::sync::running(&store).await?;
    let engine = DemoEngine::with(vec![demo::connected()]);
    let world: Arc<World> = engine.world();
    world.replying(Replies { delivered: Duration::from_millis(50), read: Duration::from_secs(3_600) });
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
    let (pushes, _) = broadcast::channel(4096);
    let media =
        Media::start(store.clone(), Some(supervisor.clone()), scratch, &live, &pushes, Config::default()).await?;
    tokio::spawn(crate::sync::run(
        incoming,
        store.clone(),
        live.clone(),
        pushes.clone(),
        Some(supervisor.clone()),
        scratch.into(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    let attention = Arc::new(crate::attention::Attention::default());
    let sending = Sending::start(
        store.clone(),
        Some(supervisor.clone()),
        live.clone(),
        &pushes,
        attention.focus(),
        crate::send::Timing::default(),
        crate::send::Makers { previews: crate::send::link::Previews::off(), media: Some(media.clone()) },
    )
    .await?;
    let feed = Feed::start(store.clone(), Arc::new(|| false), Timing::default());
    let socket = jaw_proto::local::socket(scratch);
    let listener = crate::sock::listen(&socket)?;
    let mut lanes = media.lanes();
    lanes.push(sending.lane());
    let state = State::new(scratch, Ok(supervisor), lanes, live, Some(store.clone()), pushes)?
        .with_feed(feed)
        .with_sending(sending)
        .with_media(media);
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    let hello = demo::text(ANA, ANA, "3EB0BENCHOUTHELLO", now_ms(), "send me the photos");
    world.deliver(hello).await.context("the demo engine took no message")?;
    // Rui writes all along, each text timed to its ack.
    let stop = Arc::new(AtomicBool::new(false));
    let texts = {
        let (world, stop) = (world.clone(), stop.clone());
        tokio::spawn(async move {
            let mut acks: Vec<Ack> = Vec::new();
            let mut n = 0u32;
            while !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(plan.every)).await;
                n += 1;
                let text = demo::text(RUI, RUI, &format!("3EB0BENCHOUTTEXT{n:06}"), now_ms(), "and the lake?");
                let at = Instant::now();
                if let Some(took) = world.deliver(text).await {
                    acks.push((at, took));
                }
            }
            acks
        })
    };

    let (timed_tx, timed) = std::sync::mpsc::channel::<(String, Seen)>();
    let (watching_tx, watching) = std::sync::mpsc::channel();
    let reader = {
        let socket = socket.clone();
        std::thread::spawn(move || watch(&socket, watching_tx, timed_tx))
    };
    watching.recv().context("the bench's interface couldn't watch")??;
    let mut asking = Asking::open(&socket)?;
    let mut times: Vec<(Duration, Duration, Duration)> = Vec::new();
    // When each photo was asked for and seen sent, to tell the texts' acks apart.
    let mut sends: Vec<(Instant, Instant)> = Vec::new();
    for (n, path) in paths.iter().enumerate() {
        tokio::time::sleep(QUIET).await;
        let quality = if plan.hd { "hd" } else { "sd" };
        let params =
            json!({ "chat": ANA, "files": [{ "path": path }], "quality": quality, "key": format!("bench-{n}") });
        let asked = Instant::now();
        let answer = tokio::task::block_in_place(|| asking.call("send.media", params))?;
        let answered = asked.elapsed();
        let id = answer["messages"][0].as_str().context("send.media named no message")?.to_string();
        let (seen_id, seen) = timed.recv_timeout(Duration::from_secs(120)).context("a photo never went")?;
        if seen_id != id {
            bail!("the interface saw {seen_id} go before {id}");
        }
        let (Some(ready), Some(sent)) = (seen.ready, seen.sent) else { bail!("{id} went without its copy shown made") };
        times.push((answered, ready.saturating_duration_since(asked), sent.saturating_duration_since(asked)));
        sends.push((asked, sent));
    }
    tokio::time::sleep(QUIET).await;
    stop.store(true, Ordering::Relaxed);
    let acks = texts.await?;
    drop(reader);
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let row = |what: &str, mut took: Vec<Duration>, what_of: &str| {
        took.sort();
        let at =
            |q: usize| took.get((took.len() * q / 100).min(took.len().saturating_sub(1))).copied().unwrap_or_default();
        println!(
            "{what:<44} p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms ({} {what_of})",
            ms(at(50)),
            ms(at(99)),
            ms(took.last().copied().unwrap_or_default()),
            took.len()
        );
    };
    println!(
        "a {}x{} photo ({:.1} MB) of yours, {}, from send.media to the interface seeing",
        plan.width,
        plan.height,
        size as f64 / 1e6,
        if plan.hd { "in SD and HD" } else { "in SD" }
    );
    row("send.media's answer", times.iter().map(|t| t.0).collect(), "photos");
    row("its copy made and in the cache (ready)", times.iter().map(|t| t.1).collect(), "photos");
    row("it sent, once uploaded", times.iter().map(|t| t.2).collect(), "photos");
    let sending = |at: &Instant| sends.iter().any(|(from, to)| at >= from && at <= to);
    let (during, quiet): (Vec<Ack>, Vec<Ack>) = acks.iter().partition(|(at, _)| sending(at));
    row("a text's ack, between photos", quiet.into_iter().map(|(_, took)| took).collect(), "texts");
    row("a text's ack, while a photo went", during.into_iter().map(|(_, took)| took).collect(), "texts");
    Ok(())
}

/// A second connection, for asking jawd and reading its answer.
struct Asking {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
    next: u64,
}

impl Asking {
    fn open(socket: &Path) -> Result<Asking> {
        let stream = UnixStream::connect(socket)?;
        let reader = BufReader::new(stream.try_clone()?);
        let mut asking = Asking { stream, reader, next: 1 };
        asking.call("hello", json!({ "protocol": 1, "client": "bench", "version": "0" }))?;
        Ok(asking)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let request = json!({ "id": id, "method": method, "params": params });
        self.stream.write_all(format!("{request}\n").as_bytes())?;
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                bail!("jawd went before answering {method}");
            }
            if let Ok(FromJawd::Reply(reply)) = serde_json::from_str::<FromJawd>(&line)
                && reply.id == id
            {
                return match reply.outcome {
                    jaw_proto::Outcome::Result(result) => Ok(result),
                    jaw_proto::Outcome::Error(e) => bail!("{method}: {}", e.message),
                };
            }
        }
    }
}

/// An interface on the socket watching Ana's chat, saying when each photo of yours first shows
/// its copy made, and when it was sent.
fn watch(socket: &Path, watching: std::sync::mpsc::Sender<Result<()>>, timed: std::sync::mpsc::Sender<(String, Seen)>) {
    let opened = (|| -> Result<(UnixStream, BufReader<UnixStream>)> {
        let mut stream = UnixStream::connect(socket)?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        for request in [
            json!({ "id": 1, "method": "hello", "params": { "protocol": 1, "client": "bench", "version": "0" } }),
            json!({ "id": 2, "method": "messages.watch", "params": { "chat": ANA, "around": "newest", "before": 20 } }),
        ] {
            stream.write_all(format!("{request}\n").as_bytes())?;
            loop {
                line.clear();
                reader.read_line(&mut line)?;
                if let Ok(FromJawd::Reply(reply)) = serde_json::from_str::<FromJawd>(&line)
                    && reply.id == request["id"]
                {
                    break;
                }
            }
        }
        Ok((stream, reader))
    })();
    let (_stream, mut reader) = match opened {
        Ok(opened) => opened,
        Err(e) => {
            let _ = watching.send(Err(e));
            return;
        }
    };
    let _ = watching.send(Ok(()));
    let mut seen: std::collections::HashMap<String, Seen> = std::collections::HashMap::new();
    let mut gone: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let now = Instant::now();
        let Ok(push) = serde_json::from_str::<Value>(&line) else { continue };
        for row in push["params"]["upsert"].as_array().into_iter().flatten() {
            let (Some(id), true) = (row["id"].as_str(), row["mine"] == json!(true) && row["kind"] == "image") else {
                continue;
            };
            if gone.contains(id) {
                continue;
            }
            let times = seen.entry(id.to_string()).or_default();
            if row["media"]["state"] == "local" {
                times.ready.get_or_insert(now);
            }
            if matches!(row["status"].as_str(), Some("sent" | "delivered" | "read")) {
                times.sent = Some(now);
                let times = seen.remove(id).unwrap_or_default();
                gone.insert(id.to_string());
                if timed.send((id.to_string(), times)).is_err() {
                    return;
                }
            }
        }
    }
}
