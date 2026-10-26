//! `jawd bench media`: a photo's arrival to its 256 pixel preview, as an interface sees it. A
//! 12-megapixel photo is handed over by the demo engine as a message, and timed until the push on
//! the real socket whose row carries `previews.256`: the inbox's synced commit and its ack, the
//! conversion, the instant preview, the download into the cache (the demo's server writes and
//! syncs it as jaw-engine does), the previews made with jaw-media, their commit and the push. One
//! photo at a time, each a file of its own, so none is found already made.

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
use crate::engine::demo::{self, DemoEngine, Picture};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::feed::{Feed, Timing};
use crate::state::State;
use crate::store::Store;

pub struct Plan {
    pub count: u32,
    pub width: u32,
    pub height: u32,
}

const ANA: &str = "351912345678@s.whatsapp.net";

/// What the interface saw of one photo: when its thumbnail, its file and its 256 pixel preview came.
#[derive(Default)]
struct Seen {
    thumbnail: Option<Instant>,
    local: Option<Instant>,
    small: Option<Instant>,
}

pub async fn run(scratch: &Path, plan: Plan) -> Result<()> {
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
        .with_media(media);
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
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
