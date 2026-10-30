//! Media in: each message's file, downloaded through the engine straight into the cache, where it's
//! named by its content's SHA-256; the thumbnail its message brought, drawn at once as its instant
//! preview, and its 256 and 768 pixel previews once the file is there, made with jaw-media; the
//! phone asked to send a file again that WhatsApp's server no longer has; and the cache kept to its
//! size.
//!
//! jaw.db is the queue. A copy arriving live is written `wanted`, with its message; one from history
//! becomes wanted when a window shows it (`store::files`), and any copy when an interface asks for it
//! (`media.fetch`). The scheduler (`schedule`) takes what's wanted and does it on two lanes, each a
//! pool with what an interface shows or asks for ahead of the rest: downloads, six at a time, and
//! image work, half the cores, so a flood of photos never holds up a message or a keypress, which
//! never wait on either. Photos, stickers, GIFs and voice messages download by themselves; videos and
//! documents only when asked, or below `Config::auto_bytes`. What an interface asks, and what's done
//! for the copies it asked for, is written in the writer's live lane; everything else media writes,
//! what windows show and what downloads by itself among it, goes in the background lane, which waits
//! behind live messages and keeps to the WAL's bound, but for the synced commits after which a file
//! may be removed, which the background lane doesn't make.
//!
//! The files follow the media-files audit's rules (J1 to J9): every name is built one way, absolute
//! and from jaw's folder, and named by content, so a file already there is the one wanted; a row
//! names a downloaded file only once the engine has sent the file to the drive ordered before it,
//! so the row can't reach the disk first and both are durable with the next synced commit (`fetch`);
//! a file goes only after a synced commit has stopped naming it, while no job may be writing it
//! (`cache::Claims`); the folders are made and synced, and a stopped jawd's partial files removed,
//! before anything starts; free space is checked before a download. Previews and thumbnails are
//! derived and put in place without waiting for the disk; they're made durable later, and those a
//! power cut may have broken are checked at the next start, rows showing them only once they're
//! passed (`settle`, `store::files`).

mod cache;
pub(crate) mod drive;
mod fetch;
pub mod outgoing;
mod previews;
mod save;
mod schedule;
mod settle;

#[cfg(test)]
mod tests;

pub mod bench;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jaw_proto::media::{MediaCancel, MediaFetch, MediaSave, MediaStatus, Variant};
use jaw_proto::{Error, Push, code};
use rusqlite::{Connection, OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Notify, broadcast, mpsc};

use crate::engine::supervisor::Supervisor;
use crate::engine::{EngineError, pb};
use crate::store::media::{Step, to_hex};
use crate::store::{Change, Store};
use crate::sync::Shared;
use crate::work::{Pool, Priority, Report};

/// What jawd may change about media; the defaults suit a laptop.
#[derive(Clone, Debug)]
pub struct Config {
    /// The cache's size, previews included; past it, what was used longest ago goes first.
    pub cache_bytes: u64,
    /// Videos and documents download by themselves up to this size; photos, stickers, GIFs and voice
    /// messages always do.
    pub auto_bytes: u64,
    /// How long the phone has to answer when asked to send a file again.
    pub phone_wait: Duration,
    /// How many times a minute the phone may be asked.
    pub retries_per_minute: u32,
    /// Downloads at once.
    pub downloads: usize,
    /// Background downloads wait while the disk has less than this free.
    pub keep_free: u64,
    /// A copy of a file to send is made only while the disk has twice its size and this much free.
    pub send_keep_free: u64,
    /// Whether keeping a file you picked copies it, as a test says; otherwise one on the volume
    /// jaw's folder is on is cloned there, and one on another copied.
    pub keeps_copied: Option<bool>,
    /// How long a download that couldn't be done waits before it's tried again, at first and at most.
    pub retry_first: Duration,
    pub retry_most: Duration,
    /// The free space to take instead of asking the file system, as a test sets it.
    pub free: Option<Arc<std::sync::atomic::AtomicU64>>,
    /// The cache's size to keep to instead of `cache_bytes`, as a test changes it while jawd runs.
    pub limit: Option<Arc<std::sync::atomic::AtomicU64>>,
    /// How often the previews and thumbnails made since the last time are made durable.
    pub settle_every: Duration,
    /// How often every file's previews and thumbnail are looked at for what broke them during a run.
    pub look_every: Duration,
    /// How long a start waits before it checks the previews a power cut may have broken, as a test
    /// holds it.
    pub check_after: Duration,
    /// What sends files to the drive, as a test stands in for it; the system's calls otherwise.
    pub drive: Option<drive::Drive>,
    /// Whether a barrier orders what jaw writes, as a test says; otherwise the start looks at the
    /// volumes jaw's folder is on.
    pub barriers: Option<bool>,
    /// How many of the start's check's looks fail before they read anything, as a test has them.
    pub looks_failing: Option<Arc<std::sync::atomic::AtomicU32>>,
}

impl Default for Config {
    fn default() -> Config {
        let mb = |n: u64| n * 1_000_000;
        let from_env = |name: &str, default: u64| {
            std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).map(mb).unwrap_or(default)
        };
        Config {
            cache_bytes: from_env("JAW_MEDIA_CACHE_MB", mb(5_000)),
            auto_bytes: from_env("JAW_MEDIA_AUTO_MB", mb(16)),
            phone_wait: Duration::from_secs(120),
            retries_per_minute: 20,
            downloads: 6,
            keep_free: mb(512),
            send_keep_free: mb(64),
            keeps_copied: None,
            retry_first: Duration::from_secs(2),
            retry_most: Duration::from_secs(300),
            free: None,
            limit: None,
            settle_every: Duration::from_secs(5),
            look_every: Duration::from_secs(7 * 24 * 3600),
            check_after: Duration::ZERO,
            drive: None,
            barriers: None,
            looks_failing: None,
        }
    }
}

/// Where the cache keeps files and what's made of them, under jaw's folder.
const FILES: &str = "media/files";
const PREVIEWS: &str = "media/previews";

/// Media in, running.
#[derive(Clone)]
pub struct Media {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    engine: Option<Supervisor>,
    home: PathBuf,
    pipeline: jaw_media::Media,
    config: Config,
    downloads: Arc<Pool<fetch::Download>>,
    images: Arc<Pool<previews::Drawn>>,
    /// Media out's lanes: videos, GIFs and audio encoded by ffmpeg, one or two at a time, and what
    /// goes as it is, and every upload.
    transcodes: Arc<Pool<()>>,
    uploads: Arc<Pool<()>>,
    sending: outgoing::Sending,
    /// What every interface is told: how far a job has got, and why a file couldn't be sent.
    pushes: broadcast::Sender<Push>,
    claims: cache::Claims,
    schedule: schedule::Schedule,
    /// Wakes the cleaner, which drops unnamed files and keeps the cache to its size.
    clean: Notify,
    /// Wakes whatever waits for a copy waiting for the phone to be over.
    asked_phone: Notify,
    stats: Mutex<Stats>,
    /// Why background downloads are paused, while they are.
    paused: Mutex<Option<String>>,
    /// Whether the start's check of previews a power cut may have broken is done, so newer ones
    /// may be settled.
    checked: std::sync::atomic::AtomicBool,
    /// Whether a barrier can't order everything jaw writes, as off the Mac's own drive, so downloads
    /// and settles flush the drive in full instead.
    full_flush: bool,
    /// What sends files to the drive.
    drive: drive::Drive,
    /// The view-once messages being opened, by n, whose files are downloaded and drawn as no other
    /// view-once message's are (`open_file`).
    opening: Mutex<std::collections::HashSet<i64>>,
    /// The cleaner's passes that found nothing more to do, as a test waits for one.
    #[cfg(test)]
    tidied: std::sync::atomic::AtomicU64,
}

#[derive(Default)]
struct Stats {
    partial_removed: u64,
    orphans_removed: u64,
    evicted: u64,
    remade: u64,
}

impl Media {
    /// Starts media: the cache's folders are made and synced, and a stopped jawd's partial files
    /// removed, before the lanes start; then what a stopped jawd left half done is taken up again,
    /// files no row names are swept, and what's wanted is done. Call it once jaw.lock is held and a
    /// synced commit has made jaw.db's state durable (`sync::running`), since the sweep removes
    /// what that state doesn't name.
    pub async fn start(
        store: Store,
        engine: Option<Supervisor>,
        home: &Path,
        live: &Shared,
        pushes: &broadcast::Sender<Push>,
        config: Config,
    ) -> anyhow::Result<Media> {
        let pipeline = jaw_media::Media::new(jaw_media::Config {
            previews: jaw_media::Durability::Derived,
            ..jaw_media::Config::default()
        });
        Media::start_with(store, engine, home, live, pushes, config, pipeline).await
    }

    pub(crate) async fn start_with(
        store: Store,
        engine: Option<Supervisor>,
        home: &Path,
        live: &Shared,
        pushes: &broadcast::Sender<Push>,
        config: Config,
        pipeline: jaw_media::Media,
    ) -> anyhow::Result<Media> {
        let partial = tokio::task::spawn_blocking({
            let home = home.to_path_buf();
            move || cache::prepare(&home)
        })
        .await??;
        if partial > 0 {
            eprintln!("jawd: removed {partial} partial media files a stopped jawd left");
        }
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        let full_flush =
            !config.barriers.unwrap_or_else(|| drive::barriers_hold(home, &[&home.join(FILES), &home.join(PREVIEWS)]));
        if full_flush {
            eprintln!(
                "jawd: jaw's folder isn't all on the Mac's own drive, so media files are flushed in full rather than fenced"
            );
        }
        let drive = config.drive.clone().unwrap_or_else(|| drive::Drive::system(full_flush));
        let inner = Arc::new(Inner {
            store,
            engine,
            home: home.to_path_buf(),
            pipeline,
            downloads: Pool::new("downloads", config.downloads, 1_000),
            images: Pool::new("images", (cores / 2).max(1), 1_000),
            transcodes: Pool::new("transcodes", if cores >= 8 { 2 } else { 1 }, 1_000),
            uploads: Pool::new("uploads", 3, 1_000),
            sending: outgoing::Sending::default(),
            pushes: pushes.clone(),
            config,
            claims: cache::Claims::default(),
            schedule: schedule::Schedule::default(),
            clean: Notify::new(),
            asked_phone: Notify::new(),
            stats: Mutex::new(Stats { partial_removed: partial as u64, ..Stats::default() }),
            paused: Mutex::new(None),
            checked: std::sync::atomic::AtomicBool::new(false),
            full_flush,
            drive,
            opening: Mutex::default(),
            #[cfg(test)]
            tidied: std::sync::atomic::AtomicU64::new(0),
        });
        // Previews and thumbnails a power cut may have broken are shown only once they're checked,
        // so they're known before anything reads a row.
        let (settled, made) = inner.store.pair("SELECT settled, made FROM media_made WHERE id = 1")?;
        let suspects = settle::suspects(&inner, settled, made).await?;
        inner.store.files().suspect(suspects.iter().cloned());
        let (progress, progressing) = mpsc::channel(256);
        live.lock().expect("live state").media_progress = Some(progress);
        let (seen, seeing) = mpsc::channel(1_024);
        inner.store.files().watch(seen);
        let restarted = Change::Media(Step::Restarted { at: now_ms() });
        // Live, as nothing is served until media has started, whatever background work waits.
        inner.store.apply(crate::store::Lane::Live, vec![restarted]).await?;
        let swept = outgoing::sweep(&inner).await?;
        if swept > 0 {
            eprintln!("jawd: removed {swept} copies of files to send that no message names");
        }
        inner.stats.lock().expect("media stats").orphans_removed += swept;
        schedule::start(&inner, progressing, seeing, pushes.subscribe());
        cache::start(&inner);
        fetch::start(&inner);
        settle::start(&inner, suspects);
        outgoing::start(&inner);
        Ok(Media { inner })
    }

    /// Makes the previews and thumbnails made since the last time durable, as at a clean stop.
    #[cfg(test)]
    pub async fn settle(&self) -> anyhow::Result<()> {
        settle::settle(&self.inner).await
    }

    /// `settle`, given up after `bound`, as a stop has only so long: what's left is checked at the
    /// next start.
    pub async fn settle_within(&self, bound: Duration) -> anyhow::Result<()> {
        match tokio::time::timeout(bound, settle::settle(&self.inner)).await {
            Ok(settled) => settled,
            Err(_) => anyhow::bail!("not done within {} ms, so the next start checks what's left", bound.as_millis()),
        }
    }

    /// The download, image, transcode and upload lanes, for `status`.
    pub fn lanes(&self) -> Vec<Arc<dyn Report>> {
        let inner = &self.inner;
        vec![inner.downloads.clone(), inner.images.clone(), inner.transcodes.clone(), inner.uploads.clone()]
    }

    /// Keeps a copy of each of `files`, a name for it and the file you picked, in jaw's folder, and
    /// says what each is, as `send.media` does before anything names them; or which can't be, and
    /// why, as when with the `waiting` bytes kept already they'd pass the bound.
    pub async fn keep(
        &self,
        files: Vec<(String, PathBuf)>,
        waiting: u64,
    ) -> Result<Vec<outgoing::KeptFile>, (usize, outgoing::Refused)> {
        outgoing::keep(&self.inner, files, waiting).await
    }

    /// Removes the copies `keep` kept, when nothing came to name them.
    pub fn unkeep(&self, kept: &[outgoing::KeptFile]) {
        outgoing::unkeep(&self.inner, kept);
    }

    /// Has media look at the copies to make and upload now, as when some are written.
    pub fn send_now(&self) {
        self.inner.sending.wake();
    }

    /// Holds the copies of files to send before they're made, while `held`.
    #[cfg(test)]
    pub fn hold_making(&self, held: bool) {
        self.inner.sending.hold.store(held, std::sync::atomic::Ordering::SeqCst);
        self.inner.sending.wake();
    }

    /// Holds copies of files to send just before jaw.db is told they're made or uploaded, while
    /// `held`, and says how many are held.
    #[cfg(test)]
    pub fn hold_keeping(&self, held: bool) -> usize {
        use std::sync::atomic::Ordering::SeqCst;
        self.inner.sending.hold_keeping.store(held, SeqCst);
        self.inner.sending.keeping_held.load(SeqCst)
    }

    /// What moves on each time a copy of a file to send changes state, for whatever waits for one.
    pub fn copies_changed(&self) -> tokio::sync::watch::Receiver<u64> {
        self.inner.sending.changes()
    }

    /// The cache and what's waiting, for `status`.
    pub fn status(&self) -> MediaStatus {
        let inner = &self.inner;
        let count = |what: &str| inner.store.count(what).unwrap_or(0);
        let (cache_bytes, cache_files) = cache::size(&inner.store).unwrap_or((0, 0));
        let sending = inner.store.outgoing_counts().unwrap_or_default();
        let stats = inner.stats.lock().expect("media stats");
        MediaStatus {
            cache_bytes,
            cache_files,
            cache_limit: inner.cache_limit(),
            auto_bytes: inner.config.auto_bytes,
            wanted: count("media WHERE wanted IS NOT NULL"),
            downloading: inner.schedule.running() as u64,
            waiting_for_phone: count("media WHERE state = 'waiting_for_phone'"),
            unnamed: count("media_drop"),
            partial_removed: stats.partial_removed,
            orphans_removed: stats.orphans_removed,
            evicted: stats.evicted,
            remade: stats.remade,
            paused: inner.paused.lock().expect("paused").clone(),
            unsettled: inner
                .store
                .pair("SELECT made - settled, 0 FROM media_made WHERE id = 1")
                .map_or(0, |(n, _)| n.max(0) as u64),
            checking: !inner.checked.load(std::sync::atomic::Ordering::SeqCst),
            suspect: inner.store.files().suspects() as u64,
            full_flush: inner.full_flush,
            unasked: inner.schedule.counts().1 as u64,
            unasked_most: schedule::UNASKED_AT_ONCE as u64,
            naming: inner.schedule.naming.load(std::sync::atomic::Ordering::Relaxed),
            progress_waiting: inner.schedule.progress_behind.load(std::sync::atomic::Ordering::Relaxed),
            progress_most: schedule::PROGRESS_BEHIND as u64,
            progress_passed: inner.schedule.progress_passed.load(std::sync::atomic::Ordering::Relaxed),
            preparing: sending.preparing,
            uploading: sending.uploading,
            unsent: sending.unsent,
            kept_files: sending.kept_files,
            kept_bytes: sending.kept_bytes,
            kept_unneeded: sending.unneeded,
        }
    }

    /// Answers `media.fetch`, `media.save` and `media.cancel`.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let inner = &self.inner;
        match method {
            "media.fetch" => fetch(inner, params_of::<MediaFetch>(method, params)?).await,
            "media.save" => save::save(inner, params_of::<MediaSave>(method, params)?).await,
            "media.cancel" => cancel(inner, params_of::<MediaCancel>(method, params)?).await,
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        }
    }

    /// The methods `call` answers.
    pub const METHODS: &[&str] = &["media.fetch", "media.save", "media.cancel"];

    /// Voice message `id`'s file, as `voice.play` plays it, found in `chat` or in any chat as
    /// `media.fetch` finds one and refused as it refuses one, and error 4 for a message that isn't a
    /// voice message.
    pub async fn voice_file(&self, id: &str, chat: Option<&str>) -> Result<VoiceCopy, Error> {
        let inner = &self.inner;
        let (looked, within) = (id.to_string(), chat.map(str::to_string));
        let found = inner
            .store
            .reads()
            .query(Priority::Interactive, move |db| {
                let Some(n) = crate::store::outgoing::message_n(db, &looked, within.as_deref())? else {
                    return Ok(None);
                };
                let kind = |r: &rusqlite::Row| Ok((r.get::<_, String>(0)?, r.get::<_, Option<f64>>(1)?));
                Ok(db.prepare_cached(KIND)?.query_row([n], kind).optional()?)
            })
            .await
            .map_err(|e| internal("read jaw.db", e))?;
        if let Some((kind, _)) = found.as_ref().filter(|(kind, _)| kind != "voice") {
            return Err(Error::new(code::NOT_ALLOWED, format!("message {id} is a {kind}, not a voice message")));
        }
        let named = named(inner, id, chat, Variant::Sd).await?;
        Ok(VoiceCopy {
            message: named.message,
            sha256: named.sha256,
            path: named.local.map(|p| inner.home.join(p)),
            seconds: found.and_then(|(_, seconds)| seconds),
        })
    }

    /// Wants `copy`'s file now, ahead of what downloads by itself, synced, as `media.fetch` wants
    /// one.
    pub async fn fetch_now(&self, copy: &VoiceCopy) -> Result<(), Error> {
        let inner = &self.inner;
        let want = Change::Media(Step::Want { message: copy.message, variant: "sd", at: now_ms(), asked: true });
        durably(inner, vec![want], "keep that the file was asked for").await?;
        if let Some(sha256) = &copy.sha256 {
            inner.downloads.promote(&fetch::key(sha256));
        }
        inner.schedule.wake();
        Ok(())
    }

    /// Waits for message `message`'s file to be in the cache, and says where; or why it can't be.
    pub async fn downloaded(&self, message: i64) -> Result<PathBuf, Error> {
        let path = save::downloaded(&self.inner, message, "sd").await?;
        Ok(self.inner.home.join(path))
    }

    /// The file named `sha256` is used now, so the cache keeps it longest.
    pub async fn used(&self, sha256: Vec<u8>) {
        let used = Change::Media(Step::Used { sha256, at: now_ms() });
        let _ = self.inner.store.apply(crate::store::Lane::Live, vec![used]).await;
    }

    /// The free space on the disk jaw's folder is on, or what a test says it is.
    pub fn free_space(&self) -> Option<u64> {
        self.inner.free_space()
    }

    /// The file of view-once message `n`, someone else's, as it's opened: wanted now, ahead of what
    /// downloads by itself, and waited for, with its previews for a photo (`pictured`). Error 4 if
    /// it can't be downloaded, as when WhatsApp no longer has it.
    pub async fn open_file(&self, n: i64, pictured: bool) -> Result<Opening, Error> {
        let inner = &self.inner;
        inner.opening.lock().expect("view-once files").insert(n);
        let opened = async {
            let want = Change::Media(Step::Want { message: n, variant: "sd", at: now_ms(), asked: true });
            durably(inner, vec![want], "keep that the file was asked for").await?;
            inner.schedule.wake();
            save::opened(inner, n, pictured).await
        }
        .await;
        inner.opening.lock().expect("view-once files").remove(&n);
        opened
    }
}

/// A view-once message's file in place, as it's opened, with its previews where it has them, each
/// relative to jaw's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opening {
    pub path: String,
    pub small: Option<String>,
    pub large: Option<String>,
}

/// A voice message's file: its message, its copy's hash, its whole path while it's in the cache, and
/// how long its message says it is.
#[derive(Clone, Debug)]
pub struct VoiceCopy {
    pub message: i64,
    pub sha256: Option<Vec<u8>>,
    pub path: Option<PathBuf>,
    pub seconds: Option<f64>,
}

const KIND: &str = "SELECT m.kind, md.seconds FROM messages m
    LEFT JOIN media md ON md.message = m.n AND md.variant = 'sd' WHERE m.n = ?1";

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn internal(what: &str, e: impl std::fmt::Display) -> Error {
    Error::new(code::INTERNAL, format!("jawd couldn't {what}: {e:#}"))
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// The writer's lane for what's written at `priority`: live for what an interface asked for, which
/// someone waits on; the background lane for the rest.
fn lane(priority: Priority) -> crate::store::Lane {
    match priority {
        Priority::Interactive => crate::store::Lane::Live,
        Priority::Background => crate::store::Lane::Background,
    }
}

fn variant_name(variant: Variant) -> &'static str {
    match variant {
        Variant::Sd => "sd",
        Variant::Hd => "hd",
    }
}

/// A copy of a message's file an interface named, as jaw.db has it.
#[derive(Clone, Debug)]
struct Named {
    message: i64,
    variant: &'static str,
    sha256: Option<Vec<u8>>,
    local: Option<String>,
    name: Option<String>,
    id: String,
}

/// Finds copy `variant` of message `id`, in `chat` or in any chat, as an interface names it: error
/// 3 for a message or a copy jawd doesn't have, 4 for one deleted or view-once, whose file jaw
/// doesn't keep, and -32602 for an ID in more than one chat. A message jaw.db held from before it
/// kept media, which the copies backfill hasn't reached, takes its copy first, so it's fetched,
/// saved or cancelled as if the backfill were done.
async fn named(inner: &Inner, id: &str, chat: Option<&str>, variant: Variant) -> Result<Named, Error> {
    let (id, chat, variant) = (id.to_string(), chat.map(str::to_string), variant_name(variant));
    let look = || {
        let (id, chat) = (id.clone(), chat.clone());
        inner.store.reads().query(Priority::Interactive, move |db| find(db, &id, chat.as_deref(), variant))
    };
    let message = match look().await.map_err(|e| internal("read jaw.db", e))? {
        Looked::Named(named) => return Ok(named),
        Looked::Refused(error) => return Err(error),
        Looked::Copyless(message, _) => message,
    };
    let copy = Change::Media(Step::Copy { message });
    inner
        .store
        .apply(crate::store::Lane::Live, vec![copy])
        .await
        .map_err(|e| internal("read the message's file", e))?;
    match look().await.map_err(|e| internal("read jaw.db", e))? {
        Looked::Named(named) => Ok(named),
        Looked::Refused(error) | Looked::Copyless(_, error) => Err(error),
    }
}

/// What jaw.db has of a copy an interface named.
enum Looked {
    Named(Named),
    Refused(Error),
    /// A message with no copy yet, that the copies backfill has still to look at, and the error
    /// for one with no file.
    Copyless(i64, Error),
}

/// A message an interface named and its copy: its n, whether it's deleted or view-once, the copy's
/// hash, its file if it's in the cache, its name, and its variant if it has one.
type Found = (i64, bool, bool, Option<Vec<u8>>, Option<String>, Option<String>, Option<String>);

const NAMED_IN_CHAT: &str = "SELECT m.n, m.deleted, m.view_once, md.sha256, f.path, md.name, md.variant
    FROM chats c JOIN messages m ON m.chat = c.n AND m.id = ?2
    LEFT JOIN media md ON md.message = m.n AND md.variant = ?3 LEFT JOIN media_files f ON f.sha256 = md.sha256
    WHERE c.id = ?1";
const NAMED_ANYWHERE: &str = "SELECT m.n, m.deleted, m.view_once, md.sha256, f.path, md.name, md.variant
    FROM chats c CROSS JOIN messages m ON m.chat = c.n AND m.id = ?1
    LEFT JOIN media md ON md.message = m.n AND md.variant = ?2 LEFT JOIN media_files f ON f.sha256 = md.sha256 LIMIT 2";

/// Whether the copies backfill has still to look at message ?2.
const COPIES_AHEAD: &str = "SELECT 1 FROM backfills WHERE name = ?1 AND after < ?2 AND upto >= ?2";

fn find(db: &Connection, id: &str, chat: Option<&str>, variant: &'static str) -> anyhow::Result<Looked> {
    let row = |r: &rusqlite::Row| -> rusqlite::Result<Found> {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))
    };
    let found: Vec<Found> = match chat {
        Some(chat) => db
            .prepare_cached(NAMED_IN_CHAT)?
            .query_row(params![chat, id, variant], row)
            .optional()?
            .into_iter()
            .collect(),
        None => {
            db.prepare_cached(NAMED_ANYWHERE)?.query_map(params![id, variant], row)?.collect::<rusqlite::Result<_>>()?
        }
    };
    let (message, deleted, view_once, sha256, local, name, copy) = match found.as_slice() {
        [] => return Ok(Looked::Refused(Error::new(code::NOT_FOUND, format!("jawd has no message {id}")))),
        [one] => one.clone(),
        _ => {
            let mut error =
                Error::new(code::INVALID_PARAMS, format!("message {id} is in more than one chat; say which"));
            error.data = Some(serde_json::json!({ "param": "chat" }));
            return Ok(Looked::Refused(error));
        }
    };
    if deleted {
        return Ok(Looked::Refused(Error::new(
            code::NOT_ALLOWED,
            "that message was deleted, so jaw doesn't keep its file",
        )));
    }
    if view_once {
        return Ok(Looked::Refused(Error::new(
            code::NOT_ALLOWED,
            "jaw doesn't keep the file of a message you can view once",
        )));
    }
    if copy.is_none() {
        let what = if variant == "hd" { "HD copy" } else { "file" };
        let error = Error::new(code::NOT_FOUND, format!("message {id} has no {what}"));
        if variant == "sd" && db.prepare_cached(COPIES_AHEAD)?.exists(params![crate::store::copies::COPIES, message])? {
            return Ok(Looked::Copyless(message, error));
        }
        return Ok(Looked::Refused(error));
    }
    Ok(Looked::Named(Named { message, variant, sha256, local, name, id: id.to_string() }))
}

/// `media.fetch`: the copy is wanted now, ahead of what downloads by itself, synced before it's
/// answered, so however jawd stops it's fetched; its progress and its file come as upserts to its
/// message.
async fn fetch(inner: &Arc<Inner>, fetch: MediaFetch) -> Result<Value, Error> {
    let named = named(inner, &fetch.message, fetch.chat.as_deref(), fetch.variant.unwrap_or_default()).await?;
    let want = Change::Media(Step::Want { message: named.message, variant: named.variant, at: now_ms(), asked: true });
    durably(inner, vec![want], "keep that the file was asked for").await?;
    if let Some(sha256) = &named.sha256 {
        inner.downloads.promote(&fetch::key(sha256));
    }
    inner.schedule.wake();
    Ok(Value::Object(Default::default()))
}

/// `media.cancel`: nothing more is done for the message's copies, synced before it's answered; a
/// download running is stopped and leaves nothing.
async fn cancel(inner: &Arc<Inner>, cancel: MediaCancel) -> Result<Value, Error> {
    let (id, chat) = (cancel.message.clone(), cancel.chat.clone());
    let n = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| crate::store::outgoing::message_n(db, &id, chat.as_deref()))
        .await
        .map_err(|e| internal("read jaw.db", e))?;
    if let Some(n) = n
        && outgoing::cancel(inner, n).await?
    {
        return Ok(Value::Object(Default::default()));
    }
    let mut changes = Vec::new();
    let mut running = Vec::new();
    for variant in [Variant::Sd, Variant::Hd] {
        match named(inner, &cancel.message, cancel.chat.as_deref(), variant).await {
            Ok(named) => {
                changes.push(Change::Media(Step::Cancel { message: named.message, variant: named.variant }));
                running.extend(named.sha256);
            }
            Err(e) if e.code == code::NOT_FOUND && variant == Variant::Hd => {}
            Err(e) => return Err(e),
        }
    }
    inner.schedule.cancel(changes.iter().filter_map(|c| match c {
        Change::Media(Step::Cancel { message, variant }) => Some((*message, *variant)),
        _ => None,
    }));
    durably(inner, changes, "keep that the download was cancelled").await?;
    for sha256 in running {
        let key = fetch::key(&sha256);
        if !inner.downloads.cancel(&key)
            && let Some(engine) = &inner.engine
        {
            let _ = engine.call(pb::command::Kind::MediaCancel(pb::MediaCancel { id: to_hex(&sha256) })).await;
        }
    }
    Ok(Value::Object(Default::default()))
}

/// Makes `changes` in one transaction synced before it returns, as what an interface is told is
/// done must be.
async fn durably(inner: &Inner, changes: Vec<Change>, what: &str) -> Result<(), Error> {
    let applied = inner.store.apply_durable(changes).await.map_err(|e| internal(what, e))?;
    if applied.quarantined > 0 {
        return Err(Error::new(code::INTERNAL, format!("jawd couldn't {what}; what failed is kept in quarantine")));
    }
    Ok(())
}

/// Whether what the engine said means trying again later might work.
fn retryable(e: &EngineError) -> bool {
    match e {
        EngineError::Gone | EngineError::Unavailable(_) => true,
        EngineError::Failed { retry, .. } => *retry,
    }
}
