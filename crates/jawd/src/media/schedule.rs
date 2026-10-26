//! The scheduler: it takes what's wanted from jaw.db, oldest first, and does each copy: its
//! thumbnail drawn, its file downloaded if it downloads by itself or was asked for, its previews
//! made, and the copy settled. It looks again when jaw.db changes (at most so often while it keeps
//! changing), when a job ends, when WhatsApp can be reached again, and when a copy waiting to be
//! tried again may be; so idle, it sleeps. A copy that couldn't be done for a reason that may pass
//! stays wanted, and is tried again after a pause that doubles each time.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jaw_proto::{ConnectionState, Push};
use tokio::sync::{Notify, broadcast, mpsc};

use super::fetch::{self, Asking, Fetched, Naming};
use super::previews::{self, Drawn};
use super::{Inner, now_ms};
use crate::engine::pb;
use crate::store::files::Seen;
use crate::store::media::{Keys, Step, from_hex};
use crate::store::{Change, Lane};
use crate::work::Priority;

/// The most copies done at once; the lanes bound the work itself.
const AT_ONCE: usize = 256;
/// The least time between two looks at what's wanted while jaw.db keeps changing.
const LOOK_EVERY: Duration = Duration::from_millis(25);
/// How long a copy a window showed isn't looked at again for it.
const SEEN_AGAIN: Duration = Duration::from_secs(600);
/// The most often a download's progress is written.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

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

/// A copy, by its message's n and its variant.
type CopyKey = (i64, &'static str);

/// The copies being done, and those waiting to be tried again.
#[derive(Default)]
pub(super) struct Schedule {
    doing: Mutex<HashSet<CopyKey>>,
    /// When each copy that couldn't be done may be tried again, and the pause after that.
    later: Mutex<HashMap<CopyKey, (Instant, Duration)>>,
    wake: Notify,
    pub asking: Asking,
}

impl Schedule {
    /// Has the scheduler look at what's wanted now.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Copies being done now.
    pub fn running(&self) -> usize {
        self.doing.lock().expect("schedule").len()
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

const WANTED: &str = "SELECT md.message, md.variant, md.keys, md.mime, md.bytes, md.name, md.sha256, md.enc_sha256,
        md.media_key, md.direct_path, md.thumbnail, md.state, md.asked, md.wanted, m.kind, m.view_once,
        m.envelope IS NOT NULL, f.path, f.small IS NOT NULL AND f.large IS NOT NULL, f.thumbnail IS NOT NULL,
        md.cancelled
    FROM media md JOIN messages m ON m.n = md.message LEFT JOIN media_files f ON f.sha256 = md.sha256
    WHERE md.wanted IS NOT NULL ORDER BY md.wanted LIMIT ?1";

/// The copies wanted, oldest first, and those of them with no hash, which can't be done.
async fn wanted(inner: &Inner, most: usize) -> anyhow::Result<Vec<Result<Wanted, (i64, &'static str, i64)>>> {
    inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            let rows = db
                .prepare_cached(WANTED)?
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
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
        .await
}

/// Takes what's wanted and does it, until jawd stops.
async fn run(inner: Arc<Inner>) {
    let mut committed = inner.store.committed();
    loop {
        let woken = inner.schedule.wake.notified();
        let looked = Instant::now();
        if let Err(e) = take(&inner).await {
            eprintln!("jawd: can't read which media is wanted: {e:#}");
        }
        let next = inner.schedule.next_try();
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

/// Starts a job for each copy wanted that isn't being done or waiting to be tried again.
async fn take(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let doing = inner.schedule.running();
    if doing >= AT_ONCE {
        return Ok(());
    }
    let waiting = inner.schedule.later.lock().expect("schedule").len();
    for row in wanted(inner, AT_ONCE + waiting).await? {
        let copy = match row {
            Ok(copy) => copy,
            Err((message, variant, wanted)) => {
                let error = Some("its message doesn't say which file it is".to_string());
                let settled = Step::Settled { message, variant, wanted, state: "remote", error };
                inner.store.apply(Lane::Live, vec![Change::Media(settled)]).await?;
                continue;
            }
        };
        let key = (copy.message, copy.variant);
        if inner.schedule.waiting(&key) || !inner.schedule.doing.lock().expect("schedule").insert(key) {
            continue;
        }
        let doing = inner.clone();
        tokio::spawn(async move {
            job(&doing, copy).await;
            doing.schedule.doing.lock().expect("schedule").remove(&key);
            doing.schedule.wake();
        });
        if inner.schedule.running() >= AT_ONCE {
            break;
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
    let priority = if copy.asked { Priority::Interactive } else { Priority::Background };
    let key = (copy.message, copy.variant);
    // A view-once message's file, and its thumbnail, are never kept.
    if copy.view_once {
        let _ = inner.store.apply(Lane::Live, vec![settled(&copy, leave(&copy.state), None)]).await;
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
    let (_, file) = tokio::join!(thumbnail, file(inner, &copy, priority));
    let Some(file) = file else { return };
    inner.schedule.later.lock().expect("schedule").remove(&key);
    if let Some((path, named)) = &file {
        // Its previews are drawn while jaw.db comes to name its file, and named once it has.
        let drawing = async {
            if copy.variant == "sd"
                && !copy.previews
                && let Drawn::Failed(why) = previews::previews(inner, &copy, path, named.clone(), priority).await
            {
                eprintln!("jawd: no previews for message {} this time: {why}", copy.message);
            }
        };
        let (named, ()) = tokio::join!(named.clone().wait(), drawing);
        if let Err(why) = named {
            later(inner, &copy, why).await;
            return;
        }
    }
    let state = if file.is_some() || copy.state == "downloading" { "remote" } else { leave(&copy.state) };
    let _ = inner.store.apply(Lane::Live, vec![settled(&copy, state, None)]).await;
}

/// The copy's file, and whether jaw.db names it yet: the one in place, or the one downloaded now if
/// it's to be; None if the job ends here, having said why or left the copy to be tried again.
async fn file(inner: &Arc<Inner>, copy: &Wanted, priority: Priority) -> Option<Option<(String, Naming)>> {
    // A copy left downloading is one whose download, shared with another message's copy, was
    // cancelled for that one, and is downloaded again.
    let fresh = matches!(copy.state.as_str(), "remote" | "downloading");
    let fetching = copy.local.is_none() && (copy.asked || (fresh && downloads_by_itself(inner, copy)));
    match (&copy.local, fetching) {
        (Some(path), _) => return Some(Some((path.clone(), Naming::named()))),
        (None, false) => return Some(None),
        (None, true) => {}
    }
    let downloading = Step::Downloading { message: copy.message, variant: copy.variant };
    let _ = inner.store.apply(Lane::Live, vec![Change::Media(downloading)]).await;
    let settle = |state: &'static str, why: String| async move {
        let _ = inner.store.apply(Lane::Live, vec![settled(copy, state, Some(why))]).await;
    };
    match fetch::file(inner, copy, priority).await {
        Fetched::Placed { path, named } => return Some(Some((path, named))),
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

/// The change that leaves `copy` in `state`, done with for now, unless it was wanted again since.
fn settled(copy: &Wanted, state: &'static str, error: Option<String>) -> Change {
    Change::Media(Step::Settled { message: copy.message, variant: copy.variant, wanted: copy.wanted, state, error })
}

/// The state a copy jawd had nothing more to do for keeps.
fn leave(state: &str) -> &'static str {
    match state {
        "expired" => "expired",
        "gone" => "gone",
        _ => "remote",
    }
}

/// The copy couldn't be done now; it stays wanted, and waits before it's tried again.
async fn later(inner: &Arc<Inner>, copy: &Wanted, why: String) {
    let step = Step::Later { message: copy.message, variant: copy.variant, error: why };
    let _ = inner.store.apply(Lane::Live, vec![Change::Media(step)]).await;
    inner.schedule.try_later((copy.message, copy.variant), inner.config.retry_first, inner.config.retry_most);
}

/// Writes how far each download has got, at most every PROGRESS_EVERY for each.
async fn progress(inner: Arc<Inner>, mut progressing: mpsc::Receiver<pb::MediaProgress>) {
    let mut written: HashMap<String, (Instant, f64)> = HashMap::new();
    while let Some(said) = progressing.recv().await {
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
        let step = Step::Progress { sha256, progress: fraction };
        let _ = inner.store.apply(Lane::Live, vec![Change::Media(step)]).await;
    }
}

/// Takes what windows show and lack: the copy wanted, so its thumbnail is drawn and its file
/// downloaded if it downloads by itself, ahead of what nobody's looking at. A copy is looked at for
/// what it lacks at most once each SEEN_AGAIN, however often windows show it.
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
            let _ = inner.store.apply(Lane::Live, wants).await;
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
                }
            }
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}
