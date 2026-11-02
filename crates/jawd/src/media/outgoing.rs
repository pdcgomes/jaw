//! Media out: each copy of a file of yours on its way to WhatsApp, made and uploaded in lanes of
//! their own, off the realtime one. `send.media` keeps a copy of each file you picked in jaw's
//! folder (`keep`), cloned where the disk can, before any row names it, and writes its messages
//! with their copies to make (`outgoing`). Then each file is probed and planned, and its copies made
//! as jaw-media's lanes say: a photo's SD and HD copies from one decode on the image pool, a video's
//! or a GIF's one at a time on the transcode pool, SD first so it uploads while HD is made, and what
//! goes as it is on the uploads pool. A copy made is put in the cache by its content (`place`) and
//! uploaded by the engine, streamed from its file (`upload`); each step fills in what its message
//! says of its file, and once it's uploaded, in a synced commit, its outbox row sends it. `job`
//! pushes say how far each has got, and `media.cancel` stops it at any step, leaving it failed until
//! `send.retry`.
//!
//! The files follow media in's rules: every name is built one way, absolute and from jaw's folder;
//! jaw's copy of your file and the copies made from it are synced before a row names them; your
//! copy goes once no copy to make needs it, after a synced commit has dropped it (`outgoing_drop`);
//! a stopped jawd's partial files are removed, and files no row names swept, before anything starts;
//! free space is checked before a copy is made.

use std::collections::HashMap;
use std::future::Future;
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jaw_media::{Cancel, Destination, Error, Job, Kind, Lane, Plan, Prepared, Probe, Progress};
use jaw_proto::media::{JobKind, Variant};
use jaw_proto::{Push, code};
use prost::Message as _;
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, oneshot, watch};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Inner, now_ms, retryable};
use crate::convert;
use crate::engine::{EngineError, pb};
use crate::send::compose::{self, Made, Up};
use crate::store::media::{Keys, MediaCopy};
use crate::store::outbox::Step as OutboxStep;
use crate::store::outgoing::{self, Copy, DroppedPaths, KEPT_MOST, Prepared as Placed, Shown, Step, Uploaded};
use crate::store::{Change, Lane as WriteLane, Packed, unpack};
use crate::work::{Pool, Priority};

pub mod bench;
#[cfg(test)]
mod tests;

/// jaw's own copies of the files you picked, and the copies being made from them under names of
/// their own, before they go into the cache.
pub(super) const OUTGOING: &str = "media/outgoing";
/// The engine's encrypted copies on their way up, each a partial file of its own.
pub(super) const UPLOADS: &str = "media/uploads";

/// The largest file jaw sends, as WhatsApp allows.
pub const FILE_MOST: u64 = 2_000_000_000;
/// The most copies taken to do at once; the lanes bound the work itself.
const AT_ONCE: usize = 64;
/// The most often a job's progress is pushed.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// What's left free beside a copy being made, as a multiple of its size, as the media-files audit
/// asks (J6), with `Config::send_keep_free` more.
const ROOM_TIMES: u64 = 2;

/// The copies being made and uploaded, and those waiting to be tried again.
#[derive(Default)]
pub(super) struct Sending {
    /// The copies being made or uploaded now, by outbox row, with what cancels each.
    running: Mutex<HashMap<i64, Running>>,
    /// Uploads that couldn't be done for a reason that may pass: when each may be tried again, and
    /// the pause after that.
    later: Mutex<HashMap<i64, (Instant, Duration)>>,
    wake: Notify,
    /// Each upload's progress, by the engine's id for it: who it's pushed as, and when it last was.
    uploads: Mutex<HashMap<String, (Pushed, Instant)>>,
    /// Moves on each time a copy's state changes, for the outbox, whose rows wait for their copies.
    changed: Changed,
    /// Holds copies before they're made, while set, for the tests that look at what comes before.
    #[cfg(test)]
    pub hold: std::sync::atomic::AtomicBool,
    /// Holds a copy made or uploaded just before jaw.db is told, while set, counting those held,
    /// for the tests of what may come in between.
    #[cfg(test)]
    pub hold_keeping: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub keeping_held: std::sync::atomic::AtomicUsize,
    /// Holds an HD copy being made as it says how far it has got, until its job is cancelled, while
    /// set, counting those held, for the tests of what stops one.
    #[cfg(test)]
    pub hold_hd: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pub hd_held: std::sync::atomic::AtomicUsize,
}

impl Sending {
    #[cfg(test)]
    fn hd_held_while_made(&self, cancel: &Cancel) {
        use std::sync::atomic::Ordering::SeqCst;
        if self.hold_hd.load(SeqCst) && !cancel.is_cancelled() {
            self.hd_held.fetch_add(1, SeqCst);
            while self.hold_hd.load(SeqCst) && !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(10));
            }
            self.hd_held.fetch_sub(1, SeqCst);
        }
    }

    #[cfg(test)]
    async fn held_before_keeping(&self) {
        use std::sync::atomic::Ordering::SeqCst;
        if self.hold_keeping.load(SeqCst) {
            self.keeping_held.fetch_add(1, SeqCst);
            while self.hold_keeping.load(SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            self.keeping_held.fetch_sub(1, SeqCst);
        }
    }
}

/// What a job's pushes name.
#[derive(Clone, Debug)]
struct Pushed {
    id: String,
    message: String,
    variant: Variant,
}

struct Changed(watch::Sender<u64>);

impl Default for Changed {
    fn default() -> Changed {
        Changed(watch::channel(0).0)
    }
}

#[derive(Clone)]
struct Running {
    cancel: Cancel,
    /// The engine's id for its upload, while it's uploading.
    upload: Option<String>,
    /// For a job making a file's copies, the outbox rows of all of them, which its cancel stops
    /// together; None for an upload.
    making: Option<Arc<[i64]>>,
}

impl Sending {
    /// Has the runner look at what's to do now.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// What moves on each time a copy's state changes.
    pub fn changes(&self) -> watch::Receiver<u64> {
        self.changed.0.subscribe()
    }

    pub(super) fn changed(&self) {
        self.changed.0.send_modify(|n| *n += 1);
    }

    fn running(&self, outbox: i64) -> bool {
        self.running.lock().expect("sending").contains_key(&outbox)
    }

    fn waiting(&self, outbox: i64) -> bool {
        self.later.lock().expect("sending").get(&outbox).is_some_and(|(at, _)| *at > Instant::now())
    }

    fn next_try(&self) -> Option<Instant> {
        self.later.lock().expect("sending").values().map(|(at, _)| *at).filter(|at| *at > Instant::now()).min()
    }

    fn try_later(&self, outbox: i64, first: Duration, most: Duration) {
        let mut later = self.later.lock().expect("sending");
        let pause = later.get(&outbox).map_or(first, |(_, pause)| *pause);
        later.insert(outbox, (Instant::now() + pause, (pause * 2).min(most)));
    }

    /// Every upload waiting to be tried again is tried now, as when WhatsApp can be reached again.
    pub fn retry_now(&self) {
        self.later.lock().expect("sending").clear();
        self.wake();
    }
}

/// A file of yours that `send.media` keeps a copy of, and what it is.
#[derive(Clone, Debug)]
pub struct KeptFile {
    /// jaw's copy, relative to jaw's folder, and its size.
    pub path: String,
    pub bytes: u64,
    pub probe: Probe,
}

/// Why a file can't be sent, as `send.media` says.
#[derive(Debug)]
pub enum Refused {
    /// It isn't a file jawd can read, as the sentence says.
    Unreadable(String),
    /// It's bigger than WhatsApp takes.
    TooLarge(u64),
    /// The disk is full.
    NoSpace(String),
    /// The files jaw keeps for copies still to make, these bytes of them, would pass KEPT_MOST with
    /// these.
    Making(u64),
}

/// Keeps a copy of each of `files` in jaw's folder, under the name given (the message's ID and an
/// extension), cloned where the disk can, synced before anything names it, and says what each is.
/// Before any is kept, their sizes are added to the `waiting` bytes kept already, against
/// KEPT_MOST, and those it copies rather than clones against the free space. The copies already
/// kept go again if one fails, since no row names them.
pub(super) async fn keep(
    inner: &Arc<Inner>,
    files: Vec<(String, PathBuf)>,
    waiting: u64,
) -> Result<Vec<KeptFile>, (usize, Refused)> {
    let (home, pipeline) = (inner.home.clone(), inner.pipeline.clone());
    let (free, floor, copying) = (inner.free_space(), inner.config.send_keep_free, inner.config.keeps_copied);
    tokio::task::spawn_blocking(move || {
        let mut sizes = Vec::with_capacity(files.len());
        for (i, (_, source)) in files.iter().enumerate() {
            let checked = std::fs::metadata(source)
                .map_err(|e| Refused::Unreadable(format!("jawd can't read {}: {e}", source.display())))
                .and_then(|m| {
                    if !m.is_file() {
                        Err(Refused::Unreadable(format!("{} isn't a file", source.display())))
                    } else if m.len() > FILE_MOST {
                        Err(Refused::TooLarge(m.len()))
                    } else {
                        Ok((m.len(), m.dev()))
                    }
                });
            sizes.push(checked.map_err(|why| (i, why))?);
        }
        if waiting.saturating_add(sizes.iter().map(|(bytes, _)| bytes).sum()) > KEPT_MOST {
            return Err((0, Refused::Making(waiting)));
        }
        // One on jaw's own volume is cloned there, as APFS does, and takes no room; one on another
        // is copied.
        let here = std::fs::metadata(&home).map(|m| m.dev()).ok();
        let mut copied = 0u64;
        for (i, (bytes, dev)) in sizes.iter().enumerate() {
            if !copying.unwrap_or(here != Some(*dev)) {
                continue;
            }
            copied += bytes;
            if let Some(free) = free.filter(|&free| free < copied.saturating_add(floor)) {
                return Err((
                    i,
                    Refused::NoSpace(format!(
                        "the disk has {} MB free, and keeping a copy of {} needs about {} MB",
                        free / 1_000_000,
                        files[i].1.display(),
                        copied.saturating_add(floor) / 1_000_000
                    )),
                ));
            }
        }
        let mut kept: Vec<KeptFile> = Vec::with_capacity(files.len());
        let undo = |kept: &[KeptFile]| {
            for file in kept {
                let _ = std::fs::remove_file(home.join(&file.path));
            }
        };
        for (i, (name, source)) in files.iter().enumerate() {
            let refused = |why: Refused| (i, why);
            let relative = format!("{OUTGOING}/{name}");
            let to = home.join(&relative);
            let bytes = match jaw_media::keep(source, &to) {
                Ok(bytes) => bytes,
                Err(e) => {
                    undo(&kept);
                    let full = matches!(&e, Error::Io { error, .. } if error.raw_os_error() == Some(libc::ENOSPC));
                    let why = format!("jawd couldn't keep a copy of {}: {e}", source.display());
                    return Err(refused(if full { Refused::NoSpace(why) } else { Refused::Unreadable(why) }));
                }
            };
            match pipeline.probe(&to) {
                Ok(probe) => kept.push(KeptFile { path: relative, bytes, probe }),
                Err(e) => {
                    let _ = std::fs::remove_file(&to);
                    undo(&kept);
                    return Err(refused(Refused::Unreadable(e.to_string())));
                }
            }
        }
        Ok(kept)
    })
    .await
    .unwrap_or_else(|e| Err((0, Refused::Unreadable(format!("keeping the files stopped: {e}")))))
}

/// The files `keep` kept, removed when nothing came to name them.
pub(super) fn unkeep(inner: &Inner, kept: &[KeptFile]) {
    for file in kept {
        if file.path.starts_with(&format!("{OUTGOING}/")) && !file.path.contains("..") {
            let _ = std::fs::remove_file(inner.home.join(&file.path));
        }
    }
}

pub(super) fn start(inner: &Arc<Inner>) {
    tokio::spawn(run(inner.clone()));
    tokio::spawn(drop_unneeded(inner.clone()));
    tokio::spawn(stop_taken(inner.clone()));
}

/// Stops the jobs of copies whose rows a commit took, as a deletion takes an HD copy's, until jawd
/// stops, so none goes on making or uploading a copy nothing will send.
async fn stop_taken(inner: Arc<Inner>) {
    let mut taken = inner.store.copies_taken();
    while taken.changed().await.is_ok() {
        if let Err(e) = stop_gone(&inner).await {
            eprintln!("jawd: can't read which copies of your files are still on their way: {e:#}");
        }
    }
}

/// Stops the job of each copy whose row has gone, as `cancel` stops one: an upload at once, and a
/// job making a file's copies once none of them is still to make, since it passes over those that
/// have gone (`make_all`). Only a gone row's job is stopped: no job takes a row that has gone, so
/// the job running for it is the one read here, and one whose copy is made or uploaded finishes.
async fn stop_gone(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let running: Vec<(i64, Running)> =
        inner.sending.running.lock().expect("sending").iter().map(|(o, r)| (*o, r.clone())).collect();
    if running.is_empty() {
        return Ok(());
    }
    let outboxes: Vec<i64> = running.iter().map(|(o, _)| *o).collect();
    let states: HashMap<i64, String> = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| outgoing::states(db, &outboxes))
        .await?
        .into_iter()
        .collect();
    let to_make = |outbox: &i64| states.get(outbox).is_some_and(|state| state == "preparing");
    for (outbox, running) in running {
        let making = running.making.as_ref().is_some_and(|copies| copies.iter().any(to_make));
        if !states.contains_key(&outbox) && !making {
            stop(inner, outbox).await;
        }
    }
    Ok(())
}

/// Removes, once at start and before any copy is made, the files in jaw's folder of yours that no
/// copy to make needs: what a crash left between keeping a file and the commit naming it, and
/// copies made under their own names whose commit a crash came before. Says how many went.
pub(super) async fn sweep(inner: &Arc<Inner>) -> anyhow::Result<u64> {
    let dir = inner.home.join(OUTGOING);
    let names = tokio::task::spawn_blocking(move || -> Vec<String> {
        std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(|e| Some(e.ok()?.file_name().to_str()?.to_string())).collect())
            .unwrap_or_default()
    })
    .await?;
    let mut removed = 0;
    for name in names {
        let relative = format!("{OUTGOING}/{name}");
        let asked = relative.clone();
        let needed = inner.store.reads().query(Priority::Background, move |db| outgoing::needed(db, &asked)).await?;
        if !needed && std::fs::remove_file(inner.home.join(&relative)).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// How long the dropper waits between looks while jaw.db is being written.
const DROP_EVERY: Duration = Duration::from_secs(2);

/// Removes the copies of your files that no copy to make needs any more, each once a synced commit
/// has dropped it, until jawd stops.
async fn drop_unneeded(inner: Arc<Inner>) {
    let mut committed = inner.store.committed();
    let mut changes = inner.sending.changes();
    loop {
        match drop_some(&inner).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => eprintln!("jawd: can't remove the copies of your files nothing needs, this time: {e:#}"),
        }
        let looked = Instant::now();
        tokio::select! {
            changed = changes.changed() => if changed.is_err() { return },
            changed = committed.changed() => if changed.is_err() { return },
        }
        tokio::time::sleep_until((looked + DROP_EVERY).into()).await;
    }
}

/// Drops a few of the copies nothing needs, in a synced commit, then removes their files; says
/// whether there may be more.
async fn drop_some(inner: &Arc<Inner>) -> anyhow::Result<bool> {
    let paths = inner.store.reads().query(Priority::Background, |db| outgoing::to_drop(db, 64)).await?;
    if paths.is_empty() {
        return Ok(false);
    }
    let dropped = DroppedPaths::default();
    let step = Step::Dropped { paths: paths.clone(), dropped: dropped.clone() };
    inner.store.apply_durable(vec![Change::Outgoing(step)]).await?;
    let home = inner.home.clone();
    let gone = dropped.take();
    tokio::task::spawn_blocking(move || {
        for path in gone {
            if path.starts_with(&format!("{OUTGOING}/")) && !path.split('/').any(|p| p == ".." || p.is_empty()) {
                let _ = std::fs::remove_file(home.join(path));
            }
        }
    })
    .await?;
    Ok(paths.len() == 64)
}

/// Takes what's to do and does it, until jawd stops.
async fn run(inner: Arc<Inner>) {
    loop {
        let woken = inner.sending.wake.notified();
        if let Err(e) = take(&inner).await {
            eprintln!("jawd: can't read which copies of your files are to make or upload: {e:#}");
        }
        let next = inner.sending.next_try();
        tokio::select! {
            () = woken => {}
            () = async {
                match next {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
    }
}

/// Starts each copy to make or upload that isn't being done or waiting to be tried again: a file's
/// copies to make together, so a photo is decoded once for both and a video's SD copy is made, and
/// uploads, while its HD one is.
async fn take(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let copies = inner.store.reads().query(Priority::Interactive, |db| outgoing::to_do(db, AT_ONCE)).await?;
    let busy = |copy: &Copy| inner.sending.running(copy.outbox) || inner.sending.waiting(copy.outbox);
    let mut files: Vec<Vec<Copy>> = Vec::new();
    for copy in copies {
        if copy.state == "uploading" {
            if busy(&copy) {
                continue;
            }
            let mark = Running { cancel: Cancel::new(), upload: None, making: None };
            inner.sending.running.lock().expect("sending").insert(copy.outbox, mark);
            let doing = inner.clone();
            tokio::spawn(async move {
                let outbox = copy.outbox;
                let priority = if copy.variant == "hd" { Priority::Background } else { Priority::Interactive };
                let uploading = doing.clone();
                let done = on(doing.uploads.as_ref(), format!("upload:{outbox}"), priority, (), async move {
                    upload(&uploading, copy).await;
                })
                .await;
                if done.is_none() {
                    eprintln!("jawd: the upload for outbox row {outbox} was taken off its lane");
                }
                doing.sending.running.lock().expect("sending").remove(&outbox);
                doing.sending.wake();
            });
            continue;
        }
        match files.iter_mut().find(|f| f[0].source == copy.source && f[0].message == copy.message) {
            Some(file) => file.push(copy),
            None => files.push(vec![copy]),
        }
    }
    for mut file in files {
        if file.iter().any(busy) {
            continue;
        }
        file.sort_by_key(|c| c.variant == "hd");
        let cancel = Cancel::new();
        let making: Arc<[i64]> = file.iter().map(|c| c.outbox).collect();
        {
            let mut running = inner.sending.running.lock().expect("sending");
            for copy in &file {
                let mark = Running { cancel: cancel.clone(), upload: None, making: Some(making.clone()) };
                running.insert(copy.outbox, mark);
            }
        }
        let doing = inner.clone();
        tokio::spawn(async move {
            let outboxes: Vec<i64> = file.iter().map(|c| c.outbox).collect();
            make(&doing, file, cancel).await;
            let mut running = doing.sending.running.lock().expect("sending");
            for outbox in outboxes {
                running.remove(&outbox);
            }
            drop(running);
            doing.sending.wake();
        });
    }
    Ok(())
}

/// What became of a step for a copy.
enum Failed {
    /// Trying again later may work.
    Later(String),
    /// Trying again won't, until you retry it.
    Never(String),
    Cancelled,
}

impl From<Error> for Failed {
    fn from(e: Error) -> Failed {
        match e {
            Error::Cancelled => Failed::Cancelled,
            Error::Busy { .. } => Failed::Later(e.to_string()),
            Error::Io { ref error, .. } if error.raw_os_error() == Some(libc::ENOSPC) => Failed::Never(e.to_string()),
            e => Failed::Never(e.to_string()),
        }
    }
}

/// What jaw-media takes a file as, from what it goes as.
fn media_kind(kind: &str, probed: Kind) -> Kind {
    match kind {
        "image" => Kind::Photo,
        "video" => Kind::Video,
        "gif" => Kind::Gif,
        "audio" => Kind::Audio,
        "voice" => Kind::Voice,
        "document" => Kind::Document,
        _ => probed,
    }
}

/// What a recording jawd made is, read with jaw-voice, which checks it's Ogg Opus and gives its
/// length: a voice note goes as it is, and needs neither ffprobe nor ffmpeg.
fn voice_probe(path: &Path) -> jaw_media::Result<Probe> {
    let unreadable = |why: String| Error::Unreadable { path: path.to_path_buf(), why };
    let file = jaw_voice::VoiceFile::open(path).map_err(|e| unreadable(e.to_string()))?;
    let size = std::fs::metadata(path).map_err(|e| unreadable(e.to_string()))?.len();
    let mut probe = Probe::new(Kind::Voice, jaw_media::Format::Ogg, compose::VOICE_MIME, size);
    probe.audio_codec = Some("opus".into());
    probe.channels = Some(1);
    probe.duration = Some(file.duration());
    Ok(probe)
}

/// The plan for one of a file's copies.
fn plan_of(plans: &jaw_media::Plans, variant: &str) -> Option<Plan> {
    if variant == "hd" { plans.hd.clone() } else { Some(plans.sd.clone()) }
}

/// Where a copy is made under a name of its own, beside jaw's copy of your file: by the ID its
/// outbox row sends it with, so no two copies share one.
fn destination(inner: &Inner, copy: &Copy, plan: &Plan) -> Destination {
    let extension = if plan.extension.is_empty() { "bin" } else { plan.extension };
    Destination::new(inner.home.join(OUTGOING).join(format!("{}.made.{extension}", copy.id)))
}

/// Free space before a copy is made: twice its estimated size, and a little more.
fn room_for(inner: &Inner, probe: &Probe, plan: &Plan) -> Result<(), Failed> {
    if plan.action == jaw_media::Action::Untouched {
        return Ok(());
    }
    let estimate = match plan.action {
        jaw_media::Action::Video { max_bit_rate, audio_bit_rate, .. } => {
            let seconds = probe.duration.map_or(60.0, |d| d.as_secs_f64());
            ((max_bit_rate + audio_bit_rate.unwrap_or(0)) as f64 * seconds / 8.0) as u64
        }
        jaw_media::Action::Audio { bit_rate } | jaw_media::Action::Voice { bit_rate } => {
            (bit_rate as f64 * probe.duration.map_or(600.0, |d| d.as_secs_f64()) / 8.0) as u64
        }
        _ => probe.size,
    }
    .min(probe.size.saturating_mul(4).max(1));
    let needed = estimate.saturating_mul(ROOM_TIMES).saturating_add(inner.config.send_keep_free);
    match super::fetch::free_space(&inner.home) {
        Some(free) if free < needed => Err(Failed::Never(format!(
            "the disk has {} MB free, and making this copy needs about {} MB beside it",
            free / 1_000_000,
            needed / 1_000_000
        ))),
        _ => Ok(()),
    }
}

/// Makes a file's copies: probes jaw's copy of the file you picked, plans it, and makes each copy
/// on its lane's pool, putting each in the cache as it's made.
async fn make(inner: &Arc<Inner>, copies: Vec<Copy>, cancel: Cancel) {
    let Some(source) = copies[0].source.clone() else { return };
    #[cfg(test)]
    while inner.sending.hold.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let outcome = make_all(inner, &copies, &source, &cancel).await;
    match outcome {
        Ok(()) => {}
        Err((_, Failed::Later(why))) => {
            eprintln!("jawd: the copies of {source} are made again later: {why}");
            for copy in &copies {
                inner.sending.try_later(copy.outbox, inner.config.retry_first, inner.config.retry_most);
            }
        }
        Err((copy, Failed::Never(why))) => {
            let failed: Vec<i64> = copies.iter().map(|c| c.outbox).filter(|o| *o >= copy).collect();
            unsent(inner, &copies, &failed, &why).await;
        }
        Err((_, Failed::Cancelled)) => {}
    }
}

/// Each copy made in turn, or the first that couldn't be, by outbox row, and why. A deletion may
/// have taken some of them since they were read, as it takes an HD copy's rows, and may take one
/// while those before it are made: none taken is made.
async fn make_all(inner: &Arc<Inner>, copies: &[Copy], source: &str, cancel: &Cancel) -> Result<(), (i64, Failed)> {
    let wanted = to_make(inner, copies).await;
    let copies: Vec<&Copy> = copies.iter().filter(|c| wanted.contains(&c.outbox)).collect();
    let Some(first) = copies.first().map(|c| c.outbox) else { return Ok(()) };
    let path = inner.home.join(source);
    let pipeline = inner.pipeline.clone();
    let voice = copies[0].kind == "voice";
    let probed = blocking(move || if voice { voice_probe(&path) } else { pipeline.probe(&path) })
        .await
        .map_err(|e| (first, e.into()))?;
    let mut probe = probed;
    probe.kind = media_kind(&copies[0].kind, probe.kind);
    let plans = jaw_media::plan(&probe);
    let mut planned = Vec::new();
    for copy in copies {
        let Some(plan) = plan_of(&plans, copy.variant) else {
            return Err((copy.outbox, Failed::Never("the file has no more to give in HD than in SD".into())));
        };
        room_for(inner, &probe, &plan).map_err(|e| (copy.outbox, e))?;
        planned.push((copy.clone(), plan));
    }
    if probe.kind == Kind::Photo {
        let made = prepare(inner, source, &probe, &planned, cancel, Lane::Image).await.map_err(|e| (first, e))?;
        for ((copy, plan), prepared) in planned.iter().zip(made) {
            place(inner, copy, plan, prepared).await.map_err(|e| (copy.outbox, e))?;
        }
        return Ok(());
    }
    for (n, (copy, plan)) in planned.iter().enumerate() {
        if n > 0 && to_make(inner, std::slice::from_ref(copy)).await.is_empty() {
            continue;
        }
        let one = [(copy.clone(), plan.clone())];
        let mut made = prepare(inner, source, &probe, &one, cancel, plan.lane()).await.map_err(|e| (copy.outbox, e))?;
        let prepared = made.pop().ok_or_else(|| (copy.outbox, Failed::Never("jaw-media made nothing".into())))?;
        place(inner, copy, plan, prepared).await.map_err(|e| (copy.outbox, e))?;
    }
    Ok(())
}

/// The outbox rows of those of `copies` still to make, as jaw.db has them now; all of them if it
/// can't be read, since a copy made whose row has gone costs only the work, its file going once
/// it's in the cache.
async fn to_make(inner: &Arc<Inner>, copies: &[Copy]) -> Vec<i64> {
    let outboxes: Vec<i64> = copies.iter().map(|c| c.outbox).collect();
    let asked = outboxes.clone();
    match inner.store.reads().query(Priority::Interactive, move |db| outgoing::states(db, &asked)).await {
        Ok(states) => states.into_iter().filter(|(_, state)| state == "preparing").map(|(outbox, _)| outbox).collect(),
        Err(_) => outboxes,
    }
}

/// Makes `copies` of the file at `source` on `lane`'s pool, pushing how far each has got.
async fn prepare(
    inner: &Arc<Inner>,
    source: &str,
    probe: &Probe,
    copies: &[(Copy, Plan)],
    cancel: &Cancel,
    lane: Lane,
) -> Result<Vec<Prepared>, Failed> {
    let pipeline = inner.pipeline.clone();
    let path = inner.home.join(source);
    let probe = probe.clone();
    let made: Vec<(Plan, Destination)> = copies.iter().map(|(c, p)| (p.clone(), destination(inner, c, p))).collect();
    // A copy made under this name by an earlier try whose commit didn't come is this copy's own, and
    // no row names it.
    for (_, to) in &made {
        let _ = std::fs::remove_file(&to.file);
    }
    let pushed: Vec<Option<Pushed>> = copies.iter().map(|(c, _)| pushed(c, "make")).collect();
    let pushes = inner.pushes.clone();
    let job_cancel = cancel.clone();
    #[cfg(test)]
    let (holding, held, hd): (Arc<Inner>, Cancel, Vec<bool>) =
        (inner.clone(), cancel.clone(), copies.iter().map(|(c, _)| c.variant == "hd").collect());
    let work = blocking(move || {
        let mut last: Vec<Option<Instant>> = vec![None; pushed.len()];
        let mut job = Job::new(job_cancel).on_progress(|p: Progress| {
            #[cfg(test)]
            if hd.get(p.copy).copied().unwrap_or(false) {
                holding.sending.hd_held_while_made(&held);
            }
            let Some(Some(who)) = pushed.get(p.copy) else { return };
            let now = Instant::now();
            if p.done < 1.0 && last[p.copy].is_some_and(|at| now - at < PROGRESS_EVERY) {
                return;
            }
            last[p.copy] = Some(now);
            push(&pushes, who, JobKind::Transcode, f64::from(p.done.clamp(0.0, 1.0)));
        });
        let copies: Vec<(&Plan, &Destination)> = made.iter().map(|(p, d)| (p, d)).collect();
        pipeline.prepare(&path, &probe, &copies, &mut job)
    });
    let priority =
        if copies.iter().all(|(c, _)| c.variant == "hd") { Priority::Background } else { Priority::Interactive };
    let key = format!("make:{}", copies[0].0.outbox);
    let done = match lane {
        Lane::Image => on(inner.images.as_ref(), key, priority, super::previews::Drawn::Nothing, work).await,
        Lane::Transcode => on(inner.transcodes.as_ref(), key, priority, (), work).await,
        Lane::Copy => on(inner.uploads.as_ref(), key, priority, (), work).await,
    };
    match done {
        Some(made) => made.map_err(Failed::from),
        None => Err(Failed::Cancelled),
    }
}

/// Runs `work` as a job on `pool`, and says what it came to; None if the pool dropped it before it
/// ran, as cancelling it does.
async fn on<P, T>(
    pool: &Pool<P>,
    key: String,
    priority: Priority,
    done: P,
    work: impl Future<Output = T> + Send + 'static,
) -> Option<T>
where
    P: Clone + Send + Sync + 'static,
    T: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let handle = pool
        .submit(key, priority, async move {
            let _ = tx.send(work.await);
            done
        })
        .await;
    drop(handle);
    rx.await.ok()
}

/// The hash a copy's file is named by in the cache.
fn hash(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_vec())
}

/// Puts a copy made into the cache, named by its content, and has its message and outbox row say
/// what it is; the name it was made under then goes, since the cache holds it now.
async fn place(inner: &Arc<Inner>, copy: &Copy, plan: &Plan, prepared: Prepared) -> Result<(), Failed> {
    let made = prepared.path.clone();
    let hashing = made.clone();
    let sha256 = tokio::task::spawn_blocking(move || hash(&hashing))
        .await
        .map_err(|e| Failed::Later(format!("hashing stopped: {e}")))?
        .map_err(|e| Failed::Never(format!("jawd couldn't read the copy it made: {e}")))?;
    let relative = super::fetch::cache_name(&sha256, Some(&prepared.mime), None);
    let claim = inner.claims.share(&sha256).await;
    let (from, to) = (made.clone(), inner.home.join(&relative));
    match blocking(move || jaw_media::link(&from, &to)).await {
        Ok(()) | Err(Error::Exists { .. }) => {}
        Err(e) => return Err(e.into()),
    }
    let mut raw = e2e::Message::decode(copy.raw.as_slice())
        .map_err(|e| Failed::Never(format!("its message doesn't decode: {e}")))?;
    let seconds = prepared.duration.map(|d| d.as_secs_f64().ceil() as u32).filter(|&s| s > 0);
    let facts = Made {
        mime: prepared.mime.clone(),
        sha256: sha256.clone(),
        length: prepared.size,
        dimensions: prepared.dimensions.or(plan.dimensions),
        seconds,
        thumbnail: prepared.thumbnail.clone(),
    };
    compose::made(&mut raw, &facts);
    let media = convert::copy_of(&raw).unwrap_or_else(|| MediaCopy { keys: copy.keys(), ..MediaCopy::default() });
    let shown = shown(copy, &raw);
    let step = Placed {
        outbox: copy.outbox,
        sha256: sha256.clone(),
        path: relative,
        bytes: prepared.size,
        copy: MediaCopy { bytes: Some(prepared.size), ..media },
        raw: raw.encode_to_vec(),
        shown,
        at: now_ms(),
    };
    #[cfg(test)]
    inner.sending.held_before_keeping().await;
    let applied = inner.store.apply(WriteLane::Live, vec![Change::Outgoing(Step::Prepared(Box::new(step)))]).await;
    drop(claim);
    match applied {
        Err(e) => return Err(Failed::Later(format!("the copy is made, and jaw.db couldn't say so yet: {e:#}"))),
        Ok(applied) if applied.quarantined > 0 => {
            return Err(Failed::Never("jaw.db couldn't keep the copy made; what failed is kept in quarantine".into()));
        }
        Ok(_) => {}
    }
    if !prepared.untouched {
        let _ = std::fs::remove_file(&made);
    }
    if let Some(who) = pushed(copy, "make") {
        push(&inner.pushes, &who, JobKind::Transcode, 1.0);
    }
    inner.sending.changed();
    inner.sending.wake();
    inner.schedule.wake();
    inner.clean.notify_one();
    Ok(())
}

/// What a copy's message's row shows once its message says `raw`: for an SD copy whose message is
/// still there, the row's raw WebMessageInfo with `raw` in it, and its detail.
fn shown(copy: &Copy, raw: &e2e::Message) -> Option<Shown> {
    let packed = copy.message_raw.as_deref()?;
    let mut info = WebMessageInfo::decode(unpack(packed).ok()?.as_slice()).ok()?;
    info.message = Some(raw.clone());
    Some(Shown { raw: Packed::new(&info.encode_to_vec()), detail: convert::detail_of(raw) })
}

/// Uploads a copy in the cache through the engine, streamed from its file, and has its message and
/// outbox row say where it is, in a synced commit, so it goes.
async fn upload(inner: &Arc<Inner>, copy: Copy) {
    let outbox = copy.outbox;
    match upload_one(inner, &copy).await {
        Ok(()) => {}
        Err(Failed::Later(why)) => {
            eprintln!("jawd: uploading a copy for {} is tried again later: {why}", copy.id);
            inner.sending.try_later(outbox, inner.config.retry_first, inner.config.retry_most);
        }
        Err(Failed::Never(why)) => unsent(inner, std::slice::from_ref(&copy), &[outbox], &why).await,
        Err(Failed::Cancelled) => {}
    }
}

async fn upload_one(inner: &Arc<Inner>, copy: &Copy) -> Result<(), Failed> {
    let (Some(sha256), Some(path)) = (copy.sha256.clone(), copy.path.clone()) else {
        return Err(Failed::Never("its copy isn't in the cache any more; send it again".into()));
    };
    let Some(engine) = &inner.engine else { return Err(Failed::Later("jawd runs no engine".into())) };
    let _claim = inner.claims.share(&sha256).await;
    let id = format!("up:{}", copy.outbox);
    if let Some(running) = inner.sending.running.lock().expect("sending").get_mut(&copy.outbox) {
        running.upload = Some(id.clone());
    }
    if let Some(who) = pushed(copy, "upload") {
        inner.sending.uploads.lock().expect("sending").insert(id.clone(), (who, Instant::now() - PROGRESS_EVERY));
    }
    let kind = match copy.keys() {
        Keys::Image => pb::media_download::Kind::Image,
        Keys::Video => pb::media_download::Kind::Video,
        Keys::Audio => pb::media_download::Kind::Audio,
        Keys::Document => pb::media_download::Kind::Document,
    };
    let command = pb::MediaUpload {
        id: id.clone(),
        kind: kind as i32,
        path: inner.home.join(&path).display().to_string(),
        temp: inner.home.join(UPLOADS).join(&copy.id).display().to_string(),
        file_sha256: sha256.clone(),
    };
    let answered = engine.call(pb::command::Kind::MediaUpload(command)).await;
    let pushed = inner.sending.uploads.lock().expect("sending").remove(&id);
    use pb::media_uploaded::State;
    let up = match answered {
        Ok(pb::reply::Kind::MediaUploaded(up)) => match up.state() {
            State::Uploaded => up,
            State::Cancelled => return Err(Failed::Cancelled),
            State::Changed => {
                return Err(Failed::Never(format!("the copy in jaw's cache isn't the file it was: {}", up.message)));
            }
            State::NoSpace => return Err(Failed::Never(up.message)),
            State::Unspecified => return Err(Failed::Later("the engine said nothing of the upload".into())),
        },
        Ok(other) => return Err(Failed::Never(format!("the engine answered an upload with {other:?}"))),
        Err(e) if retryable(&e) => return Err(Failed::Later(e.to_string())),
        Err(EngineError::Failed { message, .. }) => return Err(Failed::Never(message)),
        Err(e) => return Err(Failed::Later(e.to_string())),
    };
    let mut raw = e2e::Message::decode(copy.raw.as_slice())
        .map_err(|e| Failed::Never(format!("its message doesn't decode: {e}")))?;
    let facts = Up {
        url: up.url.clone(),
        direct_path: up.direct_path.clone(),
        media_key: up.media_key.clone(),
        enc_sha256: up.file_enc_sha256.clone(),
        at: now_ms() / 1000,
    };
    compose::uploaded(&mut raw, &facts);
    let step = Uploaded {
        outbox: copy.outbox,
        sha256,
        media_key: up.media_key,
        enc_sha256: up.file_enc_sha256,
        direct_path: up.direct_path,
        raw: raw.encode_to_vec(),
        shown: shown(copy, &raw),
    };
    #[cfg(test)]
    inner.sending.held_before_keeping().await;
    // Kept in a synced commit before the message goes: once it has gone, a power cut mustn't lose
    // where its file is, or the file would be uploaded and the message sent again.
    let applied = inner
        .store
        .apply_durable(vec![Change::Outgoing(Step::Uploaded(Box::new(step)))])
        .await
        .map_err(|e| Failed::Later(format!("the copy is uploaded, and jaw.db couldn't say so yet: {e:#}")))?;
    if applied.quarantined > 0 {
        return Err(Failed::Never(
            "jaw.db couldn't keep where the copy was uploaded; what failed is kept in quarantine".into(),
        ));
    }
    if let Some((who, _)) = pushed {
        push(&inner.pushes, &who, JobKind::Upload, 1.0);
    }
    inner.sending.later.lock().expect("sending").remove(&copy.outbox);
    inner.sending.changed();
    Ok(())
}

/// The copies `failed` couldn't be made or uploaded, as `why` says: each fails its outbox row, in a
/// synced commit, and every interface is told.
async fn unsent(inner: &Arc<Inner>, copies: &[Copy], failed: &[i64], why: &str) {
    let changes: Vec<Change> =
        failed.iter().map(|&outbox| Change::Outbox(OutboxStep::Unsent { outbox, error: why.to_string() })).collect();
    let kept = match inner.store.apply_durable(changes).await {
        Ok(applied) if applied.quarantined == 0 => Ok(()),
        Ok(_) => Err("what failed is kept in quarantine".to_string()),
        Err(e) => Err(format!("{e:#}")),
    };
    if let Err(why) = kept {
        eprintln!("jawd: can't keep that a copy couldn't be sent, so it's tried again later: {why}");
        for &outbox in failed {
            inner.sending.try_later(outbox, inner.config.retry_first, inner.config.retry_most);
        }
        return;
    }
    let what = match copies.iter().find(|c| failed.contains(&c.outbox)).map(|c| c.kind.as_str()) {
        Some("image") => "a photo",
        Some("video") => "a video",
        Some("gif") => "a GIF",
        Some("audio") => "an audio file",
        Some("voice") => "a voice message",
        _ => "a file",
    };
    let notice = jaw_proto::sync::Notice {
        text: format!("jaw couldn't send {what}: {why}"),
        tone: jaw_proto::sync::Tone::Warning,
    };
    let params = serde_json::to_value(notice).expect("jawd's own types serialize");
    let _ = inner.pushes.send(Push { method: "notice".into(), params });
    inner.sending.changed();
}

/// Stops the copies of message `n` still to make or upload, each failed in a synced commit before
/// it's answered: an ffmpeg making one is killed, an upload stopped, and a job waiting taken off its
/// pool. False when the message has none, so media in looks at it instead.
pub(super) async fn cancel(inner: &Arc<Inner>, n: i64) -> Result<bool, jaw_proto::Error> {
    let copies = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| outgoing::of_message(db, n))
        .await
        .map_err(|e| super::internal("read jaw.db", e))?;
    if copies.is_empty() {
        return Ok(false);
    }
    let stopping: Vec<i64> = copies
        .iter()
        .filter(|(_, state)| matches!(state.as_str(), "preparing" | "uploading"))
        .map(|(o, _)| *o)
        .collect();
    let changes: Vec<Change> = stopping
        .iter()
        .map(|&outbox| Change::Outbox(OutboxStep::Unsent { outbox, error: "you cancelled it".into() }))
        .collect();
    if !changes.is_empty() {
        super::durably(inner, changes, "keep that sending the file was cancelled").await?;
    }
    for &outbox in &stopping {
        stop(inner, outbox).await;
    }
    inner.sending.changed();
    Ok(true)
}

/// Stops the job making or uploading outbox row `outbox`'s copy: an ffmpeg making it is killed, an
/// upload stopped, and a job waiting taken off its pool.
async fn stop(inner: &Arc<Inner>, outbox: i64) {
    let running = inner.sending.running.lock().expect("sending").get(&outbox).cloned();
    if let Some(running) = running {
        running.cancel.cancel();
        if let (Some(id), Some(engine)) = (running.upload, &inner.engine) {
            let _ = engine.call(pb::command::Kind::MediaCancel(pb::MediaCancel { id })).await;
        }
    }
    let key = format!("make:{outbox}");
    let _ = inner.images.cancel(&key) || inner.transcodes.cancel(&key) || inner.uploads.cancel(&key);
    inner.uploads.cancel(&format!("upload:{outbox}"));
    inner.sending.later.lock().expect("sending").remove(&outbox);
}

/// Pushes an upload's progress, as the engine says it, at most every PROGRESS_EVERY.
pub(super) fn upload_progress(inner: &Inner, said: &pb::MediaProgress) {
    let mut uploads = inner.sending.uploads.lock().expect("sending");
    let Some((who, at)) = uploads.get_mut(&said.id) else { return };
    let done = if said.size > 0 { (said.bytes as f64 / said.size as f64).min(1.0) } else { 0.0 };
    if done < 1.0 && at.elapsed() < PROGRESS_EVERY {
        return;
    }
    *at = Instant::now();
    push(&inner.pushes, who, JobKind::Upload, done);
}

/// Who a copy's job pushes are about: its message, as rows name it, an HD copy's being its SD
/// message; none once the message is deleted for you.
fn pushed(copy: &Copy, what: &str) -> Option<Pushed> {
    let message = copy.message_id.clone()?;
    let variant = if copy.variant == "hd" { Variant::Hd } else { Variant::Sd };
    Some(Pushed { id: format!("{what}:{}", copy.outbox), message, variant })
}

fn push(pushes: &tokio::sync::broadcast::Sender<Push>, who: &Pushed, kind: JobKind, progress: f64) {
    let job = jaw_proto::media::Job {
        id: who.id.clone(),
        kind,
        message: who.message.clone(),
        variant: Some(who.variant),
        progress: (progress * 1000.0).round() / 1000.0,
    };
    let params = serde_json::to_value(job).expect("jawd's own types serialize");
    let _ = pushes.send(Push { method: "job".into(), params });
}

/// Runs jaw-media's blocking work off the runtime's threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> jaw_media::Result<T> + Send + 'static,
) -> jaw_media::Result<T> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|e| {
        Err(Error::Io { path: PathBuf::new(), error: std::io::Error::other(format!("the work stopped: {e}")) })
    })
}

/// The error `send.media` answers for a file it can't take, naming it.
pub fn refusal(index: usize, path: &str, why: Refused) -> jaw_proto::Error {
    let (code, message, data) = match why {
        Refused::Unreadable(why) => {
            (code::INVALID_PARAMS, why, serde_json::json!({ "param": "files", "file": index, "path": path }))
        }
        Refused::TooLarge(bytes) => (
            code::LIMIT,
            format!(
                "{path} is {} MB, and WhatsApp takes files of {} MB at most",
                bytes / 1_000_000,
                FILE_MOST / 1_000_000
            ),
            serde_json::json!({ "limit": "file", "most": FILE_MOST, "file": index }),
        ),
        Refused::NoSpace(why) => (code::LIMIT, why, serde_json::json!({ "limit": "disk", "file": index })),
        Refused::Making(waiting) => return making(waiting),
    };
    let mut error = jaw_proto::Error::new(code, message);
    error.data = Some(data);
    error
}

/// The error `send.media` answers when the files jaw keeps for copies still to make, `waiting`
/// bytes of them, would pass KEPT_MOST with those it was asked to send.
pub fn making(waiting: u64) -> jaw_proto::Error {
    let mut error = jaw_proto::Error::new(
        code::LIMIT,
        format!("jaw is still making {} MB of files to send; it takes more once some have gone", waiting / 1_000_000),
    );
    error.data = Some(serde_json::json!({ "limit": "media", "waiting_bytes": waiting, "most": KEPT_MOST }));
    error
}
