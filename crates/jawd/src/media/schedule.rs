//! The scheduler: it takes what's wanted from jaw.db and does each copy: its thumbnail drawn, its
//! file downloaded if it downloads by itself or was asked for, its previews made, and the copy
//! settled. What an interface asked for is taken first, oldest first, however much else is wanted,
//! beside any job taken for the same copy before it was asked for, and what it writes goes in the
//! writer's live lane. The rest is taken UNASKED_AT_ONCE at most: what arrived live first, oldest
//! first, then what windows show and what's wanted again, newest first; it writes in the background
//! lane, so it waits behind live messages and keeps to the WAL's bound. It looks again when jaw.db
//! changes (at most so often while it keeps changing), when a job ends, when WhatsApp can be
//! reached again, and when a copy waiting to be tried again may be; so idle, it sleeps. A copy that
//! couldn't be done for a reason that may pass stays wanted, and is tried again after a pause that
//! doubles each time. While the disk is short of room it starts nothing, a copy asked for included,
//! which stays asked for and goes once there's room.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use jaw_proto::{ConnectionState, Push};
use tokio::sync::{Notify, broadcast, mpsc};

use super::fetch::{self, Asking, Fetched};
use super::previews::{self, Drawn};
use super::{Inner, lane, now_ms};
use crate::disk;
use crate::engine::pb;
use crate::store::files::Seen;
use crate::store::media::{Keys, Step, from_hex};
use crate::store::{Change, Lane};
use crate::work::Priority;

/// The most copies an interface asked for done at once; the lanes bound the work itself.
const AT_ONCE: usize = 256;
/// The most copies nobody asked for done at once, so their downloads, and what they write, stay
/// few however many are wanted, as after a walk through a library of history.
pub(super) const UNASKED_AT_ONCE: usize = 16;
/// The least time between two looks at what's wanted while jaw.db keeps changing.
const LOOK_EVERY: Duration = Duration::from_millis(25);
/// How long a copy a window showed isn't looked at again for it.
const SEEN_AGAIN: Duration = Duration::from_secs(600);
/// The most often a download's progress is written.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// The progress of downloads nobody waits on that may wait for the background lane; past that a
/// download's progress is passed over, as its next says more.
pub(super) const PROGRESS_BEHIND: usize = 64;

/// The files whose next job panics, as a test has them.
#[cfg(test)]
pub(super) static PANICKING: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// A copy jawd was given to do, as jaw.db has it.
#[derive(Clone, Debug)]
pub(super) struct Wanted {
    pub message: i64,
    pub variant: &'static str,
    pub keys: Keys,
    pub mime: Option<String>,
    pub bytes: Option<u64>,
    pub name: Option<String>,
    pub sha256: Vec<u8>,
    pub enc_sha256: Option<Vec<u8>>,
    pub media_key: Option<Vec<u8>>,
    pub direct_path: Option<String>,
    pub has_thumbnail: bool,
    pub state: String,
    pub asked: bool,
    /// Wanted since its message arrived live.
    pub arrived: bool,
    pub wanted: i64,
    /// Its message's kind: `image`, `sticker`, `video` and the rest.
    pub kind: String,
    pub view_once: bool,
    /// Its message arrived live.
    pub live: bool,
    /// Its file in the cache, its previews and its thumbnail, if they're there.
    pub local: Option<String>,
    pub previews: bool,
    pub thumbnail: bool,
    /// An interface cancelled its download, so it downloads only when asked for.
    pub cancelled: bool,
}

impl Wanted {
    /// Interactive for a copy an interface asked for, which someone waits on; background otherwise.
    pub fn priority(&self) -> Priority {
        if self.asked { Priority::Interactive } else { Priority::Background }
    }
}

/// A copy, by its message's n and its variant.
type CopyKey = (i64, &'static str);

/// A job: the copy it does, at the priority it was taken at.
type JobKey = (CopyKey, Priority);

/// The copies being done, and those waiting to be tried again.
#[derive(Default)]
pub(super) struct Schedule {
    /// The jobs running: one at most for each copy at each priority, so one for a copy an interface
    /// asks for starts beside the one a window's want began, and writes live.
    doing: Mutex<HashSet<JobKey>>,
    /// When each copy that couldn't be done may be tried again, and the pause after that.
    later: Mutex<HashMap<CopyKey, (Instant, Duration)>>,
    wake: Notify,
    pub asking: Asking,
    /// The files being downloaded for copies an interface asked for, by their downloads key, with
    /// how many such copies wait on each, so their progress is written in the live lane.
    asked_files: Mutex<HashMap<String, usize>>,
    /// Files downloaded and waiting to be named, one for each job at most.
    pub naming: AtomicU64,
    /// Progress of downloads nobody waits on that waits for the background lane, PROGRESS_BEHIND
    /// at most, and how much was passed over past that since jawd started.
    pub progress_behind: AtomicU64,
    pub progress_passed: AtomicU64,
}

impl Schedule {
    /// Has the scheduler look at what's wanted now.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Copies being done now, a job each, or two for a copy asked for while a window's job did it.
    pub fn running(&self) -> usize {
        self.doing.lock().expect("schedule").len()
    }

    /// Jobs running now for copies an interface asked for, and for copies nobody did.
    pub fn counts(&self) -> (usize, usize) {
        let doing = self.doing.lock().expect("schedule");
        let unasked = doing.iter().filter(|(_, p)| *p == Priority::Background).count();
        (doing.len() - unasked, unasked)
    }

    /// The writer's lane for the progress of the file `key`'s download: live while a copy an
    /// interface asked for waits on it, as one that joined a download a window's copy began does.
    fn progress_lane(&self, key: &str) -> Lane {
        if self.asked_files.lock().expect("schedule").contains_key(key) { Lane::Live } else { Lane::Background }
    }

    /// Forgets that these copies were to be tried again.
    pub fn cancel(&self, copies: impl Iterator<Item = CopyKey>) {
        let mut later = self.later.lock().expect("schedule");
        for copy in copies {
            later.remove(&copy);
        }
    }

    /// The copy waits before it's tried again, a pause that doubles each time.
    fn try_later(&self, copy: CopyKey, first: Duration, most: Duration) {
        let mut later = self.later.lock().expect("schedule");
        let pause = later.get(&copy).map_or(first, |(_, pause)| *pause);
        later.insert(copy, (Instant::now() + pause, (pause * 2).min(most)));
    }

    fn waiting(&self, copy: &CopyKey) -> bool {
        self.later.lock().expect("schedule").get(copy).is_some_and(|(at, _)| *at > Instant::now())
    }

    /// When the next copy waiting may be tried again.
    fn next_try(&self) -> Option<Instant> {
        self.later.lock().expect("schedule").values().map(|(at, _)| *at).filter(|at| *at > Instant::now()).min()
    }
}

pub(super) fn start(
    inner: &Arc<Inner>,
    progressing: mpsc::Receiver<pb::MediaProgress>,
    seeing: mpsc::Receiver<Seen>,
    pushes: broadcast::Receiver<Push>,
) {
    tokio::spawn(run(inner.clone()));
    tokio::spawn(progress(inner.clone(), progressing));
    tokio::spawn(seen(inner.clone(), seeing));
    tokio::spawn(online(inner.clone(), pushes));
}

/// The copies an interface asked for, oldest first.
const ASKED: &str = "SELECT md.message, md.variant, md.keys, md.mime, md.bytes, md.name, md.sha256, md.enc_sha256,
        md.media_key, md.direct_path, md.thumbnail, md.state, md.asked, md.wanted, m.kind, m.view_once,
        m.envelope IS NOT NULL, f.path, f.small IS NOT NULL AND f.large IS NOT NULL, f.thumbnail IS NOT NULL,
        md.cancelled, md.arrived
    FROM media md JOIN messages m ON m.n = md.message LEFT JOIN media_files f ON f.sha256 = md.sha256
    WHERE md.asked = 1 AND md.wanted IS NOT NULL ORDER BY md.wanted LIMIT ?1";
/// The copies wanted since their messages arrived live, which nobody asked for, oldest first.
const ARRIVED: &str = "SELECT md.message, md.variant, md.keys, md.mime, md.bytes, md.name, md.sha256, md.enc_sha256,
        md.media_key, md.direct_path, md.thumbnail, md.state, md.asked, md.wanted, m.kind, m.view_once,
        m.envelope IS NOT NULL, f.path, f.small IS NOT NULL AND f.large IS NOT NULL, f.thumbnail IS NOT NULL,
        md.cancelled, md.arrived
    FROM media md JOIN messages m ON m.n = md.message LEFT JOIN media_files f ON f.sha256 = md.sha256
    WHERE md.arrived = 1 AND md.wanted IS NOT NULL AND md.asked = 0 ORDER BY md.wanted LIMIT ?1";
/// The rest of the copies nobody asked for, what windows show among them, newest first.
const SHOWN: &str = "SELECT md.message, md.variant, md.keys, md.mime, md.bytes, md.name, md.sha256, md.enc_sha256,
        md.media_key, md.direct_path, md.thumbnail, md.state, md.asked, md.wanted, m.kind, m.view_once,
        m.envelope IS NOT NULL, f.path, f.small IS NOT NULL AND f.large IS NOT NULL, f.thumbnail IS NOT NULL,
        md.cancelled, md.arrived
    FROM media md JOIN messages m ON m.n = md.message LEFT JOIN media_files f ON f.sha256 = md.sha256
    WHERE md.wanted IS NOT NULL AND md.asked = 0 AND md.arrived = 0 ORDER BY md.wanted DESC LIMIT ?1";

/// The copies one look reads, in the order a look takes them: what an interface asked for, what
/// arrived live, then the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pass {
    Asked,
    Arrived,
    Shown,
}

impl Pass {
    const ALL: [Pass; 3] = [Pass::Asked, Pass::Arrived, Pass::Shown];

    fn priority(self) -> Priority {
        if self == Pass::Asked { Priority::Interactive } else { Priority::Background }
    }

    /// How many of its priority's jobs may run at once.
    fn most(self) -> usize {
        if self == Pass::Asked { AT_ONCE } else { UNASKED_AT_ONCE }
    }
}

/// A wanted copy as a look reads it, or one with no hash, which can't be done.
pub(super) type Row = Result<Wanted, (i64, &'static str, i64)>;

/// The copies `pass` takes, `most` at most, in the order it takes them.
pub(super) fn read(db: &Connection, pass: Pass, most: usize) -> rusqlite::Result<Vec<Row>> {
    let mut statement = match pass {
        Pass::Asked => db.prepare_cached(ASKED)?,
        Pass::Arrived => db.prepare_cached(ARRIVED)?,
        Pass::Shown => db.prepare_cached(SHOWN)?,
    };
    statement
        .query_map([most as i64], |r| {
            let variant = if r.get::<_, String>(1)? == "hd" { "hd" } else { "sd" };
            let sha256: Option<Vec<u8>> = r.get(6)?;
            let (message, wanted): (i64, i64) = (r.get(0)?, r.get(13)?);
            let Some(sha256) = sha256 else { return Ok(Err((message, variant, wanted))) };
            Ok(Ok(Wanted {
                message,
                variant,
                keys: Keys::from_str(&r.get::<_, String>(2)?),
                mime: r.get(3)?,
                bytes: r.get::<_, Option<i64>>(4)?.map(|b| b.max(0) as u64),
                name: r.get(5)?,
                sha256,
                enc_sha256: r.get(7)?,
                media_key: r.get(8)?,
                direct_path: r.get(9)?,
                has_thumbnail: r.get(10)?,
                state: r.get(11)?,
                asked: r.get(12)?,
                arrived: r.get(21)?,
                wanted,
                kind: r.get(14)?,
                view_once: r.get(15)?,
                live: r.get(16)?,
                local: r.get(17)?,
                previews: r.get::<_, Option<bool>>(18)?.unwrap_or(false),
                thumbnail: r.get::<_, Option<bool>>(19)?.unwrap_or(false),
                cancelled: r.get(20)?,
            }))
        })?
        .collect()
}

async fn wanted(inner: &Inner, pass: Pass, most: usize) -> anyhow::Result<Vec<Row>> {
    inner.store.reads().query(Priority::Interactive, move |db| Ok(read(db, pass, most)?)).await
}

/// Takes what's wanted and does it, until jawd stops; while the disk is short of room it takes
/// nothing, and looks at the disk again every `disk::POLL`.
async fn run(inner: Arc<Inner>) {
    let mut committed = inner.store.committed();
    loop {
        let woken = inner.schedule.wake.notified();
        let looked = Instant::now();
        let room = inner.store.disk().room();
        if room && let Err(e) = take(&inner).await {
            eprintln!("jawd: can't read which media is wanted: {e:#}");
        }
        let next = if room { inner.schedule.next_try() } else { Some(Instant::now() + disk::POLL) };
        tokio::select! {
            () = woken => {}
            changed = committed.changed() => {
                if changed.is_err() {
                    return;
                }
                tokio::time::sleep_until((looked + LOOK_EVERY).into()).await;
            }
            () = async {
                match next {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
    }
}

/// A job's place among those running, given back however the job ends, a panic included.
struct Slot {
    inner: Arc<Inner>,
    job: JobKey,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.inner.schedule.doing.lock().unwrap_or_else(PoisonError::into_inner).remove(&self.job);
        self.inner.schedule.wake();
    }
}

/// Starts a job for each copy wanted that isn't being done at its priority or waiting to be tried
/// again: every copy an interface asked for, beside a job a window's want began for it, then copies
/// nobody asked for while fewer than UNASKED_AT_ONCE are, what arrived live first.
async fn take(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let waiting = inner.schedule.later.lock().expect("schedule").len();
    for pass in Pass::ALL {
        let (priority, most) = (pass.priority(), pass.most());
        let room = |schedule: &Schedule| {
            let (asked, unasked) = schedule.counts();
            if priority == Priority::Interactive { asked < most } else { unasked < most }
        };
        if !room(&inner.schedule) {
            continue;
        }
        for row in wanted(inner, pass, most + waiting).await? {
            let copy = match row {
                Ok(copy) => copy,
                Err((message, variant, wanted)) => {
                    let error = Some("its message doesn't say which file it is".to_string());
                    let settled = Step::Settled { message, variant, wanted, state: Some("remote"), error };
                    inner.store.apply(lane(priority), vec![Change::Media(settled)]).await?;
                    continue;
                }
            };
            let key = (copy.message, copy.variant);
            if inner.schedule.waiting(&key) {
                continue;
            }
            {
                let mut doing = inner.schedule.doing.lock().expect("schedule");
                let busy = doing.contains(&(key, Priority::Interactive))
                    || (priority == Priority::Background && doing.contains(&(key, Priority::Background)));
                if busy {
                    continue;
                }
                doing.insert((key, priority));
            }
            let slot = Slot { inner: inner.clone(), job: (key, priority) };
            tokio::spawn(async move {
                job(&slot.inner, copy).await;
                drop(slot);
            });
            if !room(&inner.schedule) {
                break;
            }
        }
    }
    Ok(())
}

/// Whether a copy downloads without being asked for: photos, stickers, GIFs and voice messages
/// always; videos, video messages, audio files and documents up to `auto_bytes`; never an HD copy,
/// nor one whose download an interface cancelled.
pub(super) fn downloads_by_itself(inner: &Inner, copy: &Wanted) -> bool {
    if copy.variant == "hd" || copy.view_once || copy.cancelled {
        return false;
    }
    match copy.kind.as_str() {
        "image" | "sticker" | "gif" | "voice" => true,
        _ => copy.bytes.is_some_and(|bytes| bytes <= inner.config.auto_bytes),
    }
}

/// Does one copy: its thumbnail while its file downloads, if it's to be, then its previews, then
/// settles it.
async fn job(inner: &Arc<Inner>, copy: Wanted) {
    #[cfg(test)]
    {
        let mut panicking = PANICKING.lock().expect("panicking jobs");
        if let Some(at) = panicking.iter().position(|sha256| *sha256 == copy.sha256) {
            panicking.remove(at);
            drop(panicking);
            panic!("a test has this file's next job panic");
        }
    }
    let priority = copy.priority();
    let key = (copy.message, copy.variant);
    // A view-once message's file, and its thumbnail, are never kept, but for one being opened.
    if copy.view_once && !inner.opening.lock().expect("view-once files").contains(&copy.message) {
        let _ = inner.store.apply(lane(priority), vec![settled(&copy, leave(&copy.state), None)]).await;
        return;
    }
    let thumbnail = async {
        if copy.variant == "sd"
            && copy.has_thumbnail
            && !copy.thumbnail
            && let Drawn::Failed(why) = previews::thumbnail(inner, &copy, priority).await
        {
            eprintln!("jawd: no instant preview for message {} this time: {why}", copy.message);
        }
    };
    let (_, path) = tokio::join!(thumbnail, file(inner, &copy, priority));
    let Some(path) = path else { return };
    inner.schedule.later.lock().expect("schedule").remove(&key);
    if let Some(path) = &path
        && copy.variant == "sd"
        && !copy.previews
        && let Drawn::Failed(why) = previews::previews(inner, &copy, path, priority).await
    {
        eprintln!("jawd: no previews for message {} this time: {why}", copy.message);
    }
    let state = if path.is_some() { Some("remote") } else { leave(&copy.state) };
    let _ = inner.store.apply(lane(priority), vec![settled(&copy, state, None)]).await;
}

/// The copy's file: the one in place, or the one downloaded now if it's to be; None if the job
/// ends here, having said why or left the copy to be tried again.
async fn file(inner: &Arc<Inner>, copy: &Wanted, priority: Priority) -> Option<Option<String>> {
    // A copy left downloading is one whose download, shared with another message's copy, was
    // cancelled for that one, and is downloaded again.
    let fresh = matches!(copy.state.as_str(), "remote" | "downloading");
    let fetching = copy.local.is_none() && (copy.asked || (fresh && downloads_by_itself(inner, copy)));
    match (&copy.local, fetching) {
        (Some(path), _) => return Some(Some(path.clone())),
        (None, false) => return Some(None),
        (None, true) => {}
    }
    let downloading = Step::Downloading { message: copy.message, variant: copy.variant };
    let _ = inner.store.apply(lane(priority), vec![Change::Media(downloading)]).await;
    let settle = |state: &'static str, why: String| async move {
        let _ = inner.store.apply(lane(priority), vec![settled(copy, Some(state), Some(why))]).await;
    };
    let fetched = {
        let _asked = (priority == Priority::Interactive).then(|| AskedFile::new(&inner.schedule, &copy.sha256));
        fetch::file(inner, copy, priority).await
    };
    match fetched {
        Fetched::Placed { path } => return Some(Some(path)),
        Fetched::Gone(why) => {
            if fetch::gone(inner, copy, why.clone()).await {
                later(inner, copy, why).await;
            }
        }
        Fetched::BadHash(why) => settle("expired", why).await,
        Fetched::NoSpace(why) if copy.asked => settle("remote", why).await,
        Fetched::NoSpace(why) | Fetched::Later(why) => later(inner, copy, why).await,
        Fetched::Never(why) => settle("remote", why).await,
        Fetched::Cancelled => {}
    }
    None
}

/// The change that leaves `copy` in `state`, or as it is with none, done with for now, unless it was
/// wanted again since.
fn settled(copy: &Wanted, state: Option<&'static str>, error: Option<String>) -> Change {
    Change::Media(Step::Settled { message: copy.message, variant: copy.variant, wanted: copy.wanted, state, error })
}

/// The state to leave a copy jawd did nothing for in: none, so it stays as it is, which its phone's
/// answer or its silence may have changed meanwhile; but a copy left downloading by a download
/// another copy shared and cancelled isn't downloading.
fn leave(state: &str) -> Option<&'static str> {
    (state == "downloading").then_some("remote")
}

/// The copy couldn't be done now; it stays wanted, and waits before it's tried again.
async fn later(inner: &Arc<Inner>, copy: &Wanted, why: String) {
    let step = Step::Later { message: copy.message, variant: copy.variant, error: why };
    let _ = inner.store.apply(lane(copy.priority()), vec![Change::Media(step)]).await;
    inner.schedule.try_later((copy.message, copy.variant), inner.config.retry_first, inner.config.retry_most);
}

/// A file being downloaded for a copy an interface asked for, for as long as it lives.
struct AskedFile<'a> {
    schedule: &'a Schedule,
    key: String,
}

impl<'a> AskedFile<'a> {
    fn new(schedule: &'a Schedule, sha256: &[u8]) -> AskedFile<'a> {
        let key = fetch::key(sha256);
        *schedule.asked_files.lock().expect("schedule").entry(key.clone()).or_default() += 1;
        AskedFile { schedule, key }
    }
}

impl Drop for AskedFile<'_> {
    fn drop(&mut self) {
        let mut asked = self.schedule.asked_files.lock().expect("schedule");
        if let Some(waiting) = asked.get_mut(&self.key) {
            *waiting -= 1;
            if *waiting == 0 {
                asked.remove(&self.key);
            }
        }
    }
}

/// Writes how far each download has got, at most every PROGRESS_EVERY for each: at once for a file
/// a copy an interface asked for waits on, and otherwise in the background, behind which the next
/// never waits.
async fn progress(inner: Arc<Inner>, mut progressing: mpsc::Receiver<pb::MediaProgress>) {
    let (behind, mut writing) = mpsc::channel::<Change>(PROGRESS_BEHIND);
    let background = inner.clone();
    tokio::spawn(async move {
        while let Some(step) = writing.recv().await {
            let _ = background.store.apply(Lane::Background, vec![step]).await;
            background.schedule.progress_behind.fetch_sub(1, Ordering::Relaxed);
        }
    });
    let mut written: HashMap<String, (Instant, f64)> = HashMap::new();
    while let Some(said) = progressing.recv().await {
        if said.id.starts_with("up:") {
            super::outgoing::upload_progress(&inner, &said);
            continue;
        }
        let (Some(sha256), true) = (from_hex(&said.id), said.size > 0) else { continue };
        let fraction = (said.bytes as f64 / said.size as f64).min(1.0);
        if fraction >= 1.0 {
            written.remove(&said.id);
            continue;
        }
        if written.get(&said.id).is_some_and(|(at, was)| at.elapsed() < PROGRESS_EVERY || (fraction - was).abs() < 0.01)
        {
            continue;
        }
        written.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
        written.insert(said.id.clone(), (Instant::now(), fraction));
        let lane = inner.schedule.progress_lane(&fetch::key(&sha256));
        let step = Change::Media(Step::Progress { sha256, progress: fraction });
        match lane {
            Lane::Live => {
                let _ = inner.store.apply(Lane::Live, vec![step]).await;
            }
            Lane::Background => {
                // Counted before it's sent, so the writer's count down can't come first.
                inner.schedule.progress_behind.fetch_add(1, Ordering::Relaxed);
                if behind.try_send(step).is_err() {
                    inner.schedule.progress_behind.fetch_sub(1, Ordering::Relaxed);
                    inner.schedule.progress_passed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Takes what windows show and lack: the copy wanted, so its thumbnail is drawn and its file
/// downloaded if it downloads by itself, ahead of what nobody's looking at, in the background lane
/// as the rest of what nobody asked for. A copy is looked at for what it lacks at most once each
/// SEEN_AGAIN, however often windows show it.
async fn seen(inner: Arc<Inner>, mut seeing: mpsc::Receiver<Seen>) {
    let mut recently: HashMap<i64, Instant> = HashMap::new();
    while let Some(first) = seeing.recv().await {
        let mut wants = Vec::new();
        let mut next = Some(first);
        while let Some(seen) = next.take() {
            next = seeing.try_recv().ok();
            if recently.get(&seen.message).is_some_and(|at| at.elapsed() < SEEN_AGAIN) {
                continue;
            }
            recently.insert(seen.message, Instant::now());
            inner.downloads.promote(&fetch::key(&seen.sha256));
            wants.push(Change::Media(Step::Want { message: seen.message, variant: "sd", at: now_ms(), asked: false }));
        }
        if recently.len() > 10_000 {
            recently.retain(|_, at| at.elapsed() < SEEN_AGAIN);
        }
        if !wants.is_empty() {
            let _ = inner.store.apply(Lane::Background, wants).await;
        }
    }
}

/// Once WhatsApp can be reached again, what was waiting for it is tried at once.
async fn online(inner: Arc<Inner>, mut pushes: broadcast::Receiver<Push>) {
    loop {
        match pushes.recv().await {
            Ok(push) if push.method == "connection" => {
                let online = serde_json::from_value::<jaw_proto::Connection>(push.params)
                    .is_ok_and(|c| c.state == ConnectionState::Online);
                if online {
                    inner.schedule.later.lock().expect("schedule").clear();
                    inner.schedule.wake();
                    inner.sending.retry_now();
                }
            }
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}
