//! `jawd bench media`: a photo's arrival to its 256 pixel preview, as an interface sees it. A
//! 12-megapixel photo is handed over by the demo engine as a message, and timed until the push on
//! the real socket whose row carries `previews.256`: the inbox's synced commit and its ack, the
//! conversion, the instant preview, the download into the cache (the demo's server writes it and
//! sends it to the drive as jaw-engine does), the previews made with jaw-media, their commit and
//! the push. One photo at a time, each a file of its own, so none is found already made.
//!
//! With `--flood`, texts' acks while photos or voice messages pour in instead: a text from Rui every
//! 20 ms, timed to its ack and to the push showing it to an interface, while a flood from Ana is
//! handed over back to back, downloaded and drawn; and jawd's memory meanwhile.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use jaw_proto::FromJawd;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::{Config, Media};
use crate::demo::usage::Sampler;
use crate::engine::demo::{self, DemoEngine, Picture, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::feed::{Feed, Timing};
use crate::state::State;
use crate::store::Store;

pub struct Plan {
    pub count: u32,
    pub width: u32,
    pub height: u32,
    /// Times texts' acks during a flood of this many photos instead, or voice messages with `voice`,
    /// or this many texts alone.
    pub flood: u32,
    pub voice: bool,
    pub alone: u32,
}

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
/// How often a text comes from Rui during a flood.
const TEXTS_EVERY: Duration = Duration::from_millis(20);
/// How long a flood may take, at most.
const FLOOD_FOR: Duration = Duration::from_secs(900);

/// jawd's store, sync, media, windows and socket on `scratch`, with the demo engine.
struct Served {
    store: Store,
    world: Arc<World>,
    media: Media,
    socket: std::path::PathBuf,
}

async fn serve(scratch: &Path) -> Result<Served> {
    let store = Store::open(scratch)?;
    crate::sync::running(&store).await?;
    let engine = DemoEngine::with(vec![demo::connected()]);
    let world = engine.world();
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let live: crate::sync::Shared = Arc::default();
    let (pushes, _) = broadcast::channel(256);
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
    ));
    let feed = Feed::start(store.clone(), Arc::new(|| false), Timing::default());
    let socket = jaw_proto::local::socket(scratch);
    let listener = crate::sock::listen(&socket)?;
    let state = State::new(scratch, Ok(supervisor), media.lanes(), live, Some(store.clone()), pushes)?
        .with_feed(feed)
        .with_media(media.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    Ok(Served { store, world, media, socket })
}

/// What the interface saw of one photo: when its thumbnail, its file and its 256 pixel preview came.
#[derive(Default)]
struct Seen {
    thumbnail: Option<Instant>,
    local: Option<Instant>,
    small: Option<Instant>,
}

pub async fn run(scratch: &Path, plan: Plan) -> Result<()> {
    if plan.flood > 0 || plan.alone > 0 {
        return flood(scratch, plan).await;
    }
    let made = Instant::now();
    let pictures: Vec<Picture> = (0..plan.count).map(|n| Picture::numbered(plan.width, plan.height, n + 1)).collect();
    let size = pictures.first().map_or(0, |p| p.data.len());
    eprintln!(
        "made {} photos of {}x{} ({:.1} MB each) in {:.1} s",
        plan.count,
        plan.width,
        plan.height,
        size as f64 / 1e6,
        made.elapsed().as_secs_f64()
    );

    let Served { world, socket, .. } = serve(scratch).await?;
    // The chat exists before it's watched, as an open chat does.
    let hello = demo::text(ANA, ANA, "3EB0BENCHHELLO", super::now_ms(), "photos coming");
    world.deliver(hello).await.context("the demo engine took no message")?;

    let (timed_tx, timed) = std::sync::mpsc::channel::<(String, Seen)>();
    let (watching_tx, watching) = std::sync::mpsc::channel();
    let ids: Vec<String> = (0..plan.count).map(|n| format!("3EB0BENCHPHOTO{n:04}")).collect();
    let watched = ids.clone();
    let reader = std::thread::spawn(move || watch(&socket, watched, watching_tx, timed_tx));
    watching.recv().context("the bench's interface couldn't watch")??;

    let mut handed = Vec::new();
    let mut seen: Vec<Seen> = Vec::new();
    for (picture, id) in pictures.iter().zip(&ids) {
        let event = world.photo(picture, ANA, ANA, id, super::now_ms(), "");
        let at = Instant::now();
        world.deliver(event).await.context("the demo engine took no photo")?;
        handed.push(at);
        let (got, times) = timed.recv_timeout(Duration::from_secs(60)).context("a photo's preview never came")?;
        if &got != id {
            bail!("the interface saw {got} before {id}");
        }
        seen.push(times);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    drop(reader);
    let row = |what: &str, pick: fn(&Seen) -> Option<Instant>| {
        let mut took: Vec<Duration> =
            handed.iter().zip(&seen).filter_map(|(at, s)| pick(s).map(|t| t.saturating_duration_since(*at))).collect();
        took.sort();
        let at =
            |q: usize| took.get((took.len() * q / 100).min(took.len().saturating_sub(1))).copied().unwrap_or_default();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "{what:<28} p50 {:.0} ms, max {:.0} ms, min {:.0} ms ({} photos)",
            ms(at(50)),
            ms(took.last().copied().unwrap_or_default()),
            ms(took.first().copied().unwrap_or_default()),
            took.len()
        );
    };
    println!(
        "a {}x{} photo ({:.1} MB) from the engine's hand-over to the interface seeing",
        plan.width,
        plan.height,
        size as f64 / 1e6
    );
    row("its thumbnail", |s| s.thumbnail);
    row("its file, downloaded", |s| s.local);
    row("its 256 px preview", |s| s.small);
    Ok(())
}

/// An interface on the socket watching Ana's chat, saying when each photo's thumbnail, file and 256
/// pixel preview first show in its pushes, once its preview has.
fn watch(
    socket: &Path,
    ids: Vec<String>,
    watching: std::sync::mpsc::Sender<Result<()>>,
    timed: std::sync::mpsc::Sender<(String, Seen)>,
) {
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
    let mut done: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let now = Instant::now();
        let Ok(push) = serde_json::from_str::<Value>(&line) else { continue };
        for row in push["params"]["upsert"].as_array().into_iter().flatten() {
            let Some(id) = row["id"].as_str().filter(|id| ids.iter().any(|i| i == id) && !done.contains(*id)) else {
                continue;
            };
            let media = &row["media"];
            let times = seen.entry(id.to_string()).or_default();
            if media["thumbnail"].is_string() {
                times.thumbnail.get_or_insert(now);
            }
            if media["state"] == "local" {
                times.local.get_or_insert(now);
            }
            if media["previews"]["256"].is_string() && times.small.is_none() {
                times.small = Some(now);
                done.insert(id.to_string());
                let times = seen.remove(id).unwrap_or_default();
                if timed.send((id.to_string(), times)).is_err() {
                    return;
                }
            }
        }
    }
}

/// Each duration's p50, p99 and the most, in milliseconds, and how many there were.
fn spread(mut took: Vec<Duration>) -> String {
    if took.is_empty() {
        return "none".into();
    }
    took.sort();
    let at = |q: f64| took[((took.len() as f64 * q) as usize).min(took.len() - 1)];
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    format!(
        "p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms ({})",
        ms(at(0.50)),
        ms(at(0.99)),
        ms(took[took.len() - 1]),
        took.len()
    )
}

/// The load average over the last minute, five and fifteen.
fn load() -> String {
    let mut loads = [0f64; 3];
    // SAFETY: getloadavg fills at most the three it's given.
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    if n < 3 { "unknown".into() } else { format!("{:.2} {:.2} {:.2}", loads[0], loads[1], loads[2]) }
}

/// An interface on the socket watching Rui's chat, saying when each text of the flood first shows.
fn watch_texts(socket: &Path, seen: std::sync::mpsc::Sender<(String, Instant)>) -> Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    for request in [
        json!({ "id": 1, "method": "hello", "params": { "protocol": jaw_proto::PROTOCOL, "client": "bench", "version": "0" } }),
        json!({ "id": 2, "method": "messages.watch", "params": { "chat": RUI, "around": "newest", "before": 20 } }),
    ] {
        stream.write_all(format!("{request}\n").as_bytes())?;
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                bail!("jawd hung up");
            }
            if let Ok(FromJawd::Reply(reply)) = serde_json::from_str::<FromJawd>(&line)
                && reply.id == request["id"]
            {
                break;
            }
        }
    }
    std::thread::spawn(move || {
        let _stream = stream;
        let mut shown = std::collections::HashSet::new();
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let now = Instant::now();
            if let Ok(push) = serde_json::from_str::<Value>(&line) {
                for row in push["params"]["upsert"].as_array().into_iter().flatten() {
                    if let Some(id) = row["id"].as_str().filter(|id| id.starts_with("3EB0BENCHTEXT"))
                        && shown.insert(id.to_string())
                        && seen.send((id.to_string(), now)).is_err()
                    {
                        return;
                    }
                }
            }
            line.clear();
        }
    });
    Ok(())
}

/// A voice message's Ogg file of about `bytes`, each `n` another.
fn voice_note(bytes: usize, n: u32) -> Arc<Vec<u8>> {
    let mut seed = 0x9E37_79B9_u32 ^ n.wrapping_mul(0x85EB_CA6B);
    let mut data = b"OggS".to_vec();
    data.extend((4..bytes).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as u8
    }));
    Arc::new(data)
}

/// Texts from Rui every TEXTS_EVERY, timed to their acks and to an interface, while `plan.flood`
/// photos (or voice messages) from Ana are handed over back to back, downloaded and drawn.
async fn flood(scratch: &Path, plan: Plan) -> Result<()> {
    let made = Instant::now();
    let what = if plan.flood == 0 {
        "nothing (texts alone)"
    } else if plan.voice {
        "voice messages"
    } else {
        "photos"
    };
    let (pictures, voices): (Vec<Picture>, Vec<Arc<Vec<u8>>>) = if plan.voice {
        (Vec::new(), (0..plan.flood).map(|n| voice_note(40_000, n + 1)).collect())
    } else {
        ((0..plan.flood).map(|n| Picture::numbered(plan.width, plan.height, n + 1)).collect(), Vec::new())
    };
    let held: usize = pictures.iter().map(|picture| picture.data.len()).sum::<usize>()
        + voices.iter().map(|data| data.len()).sum::<usize>();
    eprintln!(
        "made {} {what} ({:.1} MB in all) in {:.1} s",
        plan.flood,
        held as f64 / 1e6,
        made.elapsed().as_secs_f64()
    );
    let load_before = load();
    let served = serve(scratch).await?;
    let (store, world) = (served.store.clone(), served.world.clone());
    let sampler = Sampler::start(scratch.join("jaw.db-wal"));
    sampler.sampling(true);
    world.deliver(demo::text(RUI, RUI, "3EB0BENCHRUI", super::now_ms(), "texts coming")).await.context("no ack")?;
    world.deliver(demo::text(ANA, ANA, "3EB0BENCHANA", super::now_ms(), "a flood coming")).await.context("no ack")?;
    let (seen_tx, seen) = std::sync::mpsc::channel();
    watch_texts(&served.socket, seen_tx)?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = sampler.most();
    let began = Instant::now();
    let delivering = {
        let world = world.clone();
        let voice = plan.voice;
        tokio::spawn(async move {
            let mut acks = Vec::new();
            for n in 0..pictures.len().max(voices.len()) {
                let id = format!("3EB0BENCHFLOOD{n:05}");
                let event = if voice {
                    world.voice(voices[n].clone(), ANA, ANA, &id, super::now_ms(), 12)
                } else {
                    world.photo(&pictures[n], ANA, ANA, &id, super::now_ms(), "")
                };
                if let Some(took) = world.deliver(event).await {
                    acks.push(took);
                }
            }
            (acks, Instant::now())
        })
    };
    // Each text's hand-over, its ack, and whether the flood was still being handed over.
    let mut texts: Vec<(String, Instant, Duration, bool)> = Vec::new();
    let mut done_at = None;
    let column = if plan.voice { "path" } else { "small" };
    let count_done = format!("SELECT (SELECT count(*) FROM media_files WHERE {column} IS NOT NULL), 0");
    for n in 0u32.. {
        tokio::time::sleep(TEXTS_EVERY).await;
        let id = format!("3EB0BENCHTEXT{n:06}");
        let handing = !delivering.is_finished();
        let at = Instant::now();
        let took =
            world.deliver(demo::text(RUI, RUI, &id, super::now_ms(), "hi")).await.context("a text had no ack")?;
        texts.push((id, at, took, handing));
        if plan.flood == 0 && texts.len() >= plan.alone as usize {
            done_at = Some(Instant::now());
            break;
        }
        if plan.flood > 0 && n % 25 == 24 && !handing {
            let (done, _) = store.pair(&count_done)?;
            if done >= i64::from(plan.flood) {
                done_at = Some(Instant::now());
                break;
            }
        }
        if began.elapsed() > FLOOD_FOR {
            eprintln!("the flood didn't end in {} s", FLOOD_FOR.as_secs());
            break;
        }
    }
    let (flood_acks, handed_at) = delivering.await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let most = sampler.most();
    let shown: std::collections::HashMap<String, Instant> = seen.try_iter().collect();
    let phase = |handing: Option<bool>| -> (Vec<Duration>, Vec<Duration>) {
        let picked: Vec<_> = texts.iter().filter(|text| handing.is_none_or(|h| text.3 == h)).collect();
        let acks = picked.iter().map(|text| text.2).collect();
        let pushed = picked
            .iter()
            .filter_map(|text| shown.get(&text.0).map(|at| at.saturating_duration_since(text.1)))
            .collect();
        (acks, pushed)
    };
    let mb = |bytes: u64| bytes as f64 / 1e6;
    println!("load before {load_before}, after {}", load());
    println!(
        "a flood of {} {what} ({:.2} MB each), handed over in {:.1} s, downloaded{} after {:.1} s",
        plan.flood,
        held as f64 / 1e6 / f64::from(plan.flood.max(1)),
        handed_at.saturating_duration_since(began).as_secs_f64(),
        if plan.voice { "" } else { " and drawn" },
        done_at.map_or(f64::NAN, |at| at.saturating_duration_since(began).as_secs_f64())
    );
    println!("the flood's acks:           {}", spread(flood_acks));
    let (acks, pushed) = phase(None);
    println!("texts' acks:                {}", spread(acks));
    println!(
        "texts to the interface:     {} ({} not seen)",
        spread(pushed),
        texts.len() - shown.len().min(texts.len())
    );
    let (acks, _) = phase(Some(true));
    println!("  while it was handed over: {}", spread(acks));
    let (acks, _) = phase(Some(false));
    println!("  while it was downloaded:  {}", spread(acks));
    println!(
        "memory: {:.0} MB at most ({:.0} MB resident), {:.0} MB before the flood; the bench's demo engine holds {:.0} MB of the flood's files",
        mb(most.footprint),
        mb(most.resident),
        mb(before.footprint),
        mb(held as u64)
    );
    println!("the WAL: {:.0} MiB on disk at most", most.wal as f64 / 1_048_576.0);
    println!("writer: {}", serde_json::to_string(&crate::work::Report::report(&store))?);
    for lane in served.media.lanes() {
        println!("lane: {}", serde_json::to_string(&lane.report())?);
    }
    println!("storage: {}", serde_json::to_string(&store.storage())?);
    Ok(())
}
