//! Previews, on the image lane: the thumbnail a message brought, drawn as its instant preview; the
//! 256 and 768 pixel previews once its file is in the cache (a video's poster and an animated
//! sticker's first frame as jaw-media makes them); each named by the file's hash, so copies share
//! them. They're derived and put in place without waiting for the disk, the row naming them only
//! once jaw-media has returned them whole; `settle` sends them to the drive later, and one a power
//! cut broke before that is found at the start, reads as missing until then (`store::files`), and
//! is mended here: forgotten in a synced commit, removed, and made again.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jaw_media::{Cancel, Durability, Error, Kind, Previews, previews_lane};

use super::schedule::Wanted;
use super::{Inner, PREVIEWS, now_ms};
use crate::convert;
use crate::store::media::{Made, Step, to_hex};
use crate::store::{Change, Lane, unpack};
use crate::work::Priority;

/// What became of drawing something.
#[derive(Clone, Debug)]
pub(super) enum Drawn {
    Done,
    /// Made and in place, for jaw.db to name with this step.
    Made(Step),
    /// Nothing to draw: no thumbnail, or a file with no picture, which shows its type's icon.
    Nothing,
    Failed(String),
}

/// How many times a preview another call is writing is waited for.
const BUSY_TRIES: u32 = 20;
const BUSY_PAUSE: Duration = Duration::from_millis(50);

fn stem(inner: &Inner, sha256: &[u8]) -> PathBuf {
    inner.home.join(PREVIEWS).join(to_hex(sha256))
}

fn relative(inner: &Inner, path: &Path) -> String {
    path.strip_prefix(&inner.home).unwrap_or(path).display().to_string()
}

/// Draws the thumbnail `copy`'s message brought as its instant preview, on the image lane.
pub(super) async fn thumbnail(inner: &Arc<Inner>, copy: &Wanted, priority: Priority) -> Drawn {
    let job = draw_thumbnail(inner.clone(), copy.clone());
    let handle = inner.images.submit(format!("thumbnail:{}", to_hex(&copy.sha256)), priority, job).await;
    handle.result().await.unwrap_or(Drawn::Nothing)
}

async fn draw_thumbnail(inner: Arc<Inner>, copy: Wanted) -> Drawn {
    let _claim = inner.claims.share(&copy.sha256).await;
    let message = copy.message;
    let raw = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            let raw: Option<Vec<u8>> =
                db.prepare_cached("SELECT raw FROM messages WHERE n = ?1")?.query_row([message], |r| r.get(0))?;
            raw.map(|raw| unpack(&raw)).transpose()
        })
        .await;
    let Some(thumbnail) = raw.ok().flatten().and_then(|raw| convert::thumbnail_in(&raw)) else { return Drawn::Nothing };
    let stem = stem(&inner, &copy.sha256);
    let drawn = blocking(move || {
        let mut tries = 0;
        loop {
            match jaw_media::instant_preview(&thumbnail, &stem, Durability::Derived) {
                Err(Error::Busy { .. }) if tries < BUSY_TRIES => {
                    tries += 1;
                    std::thread::sleep(BUSY_PAUSE);
                }
                Err(Error::Exists { path }) if jaw_media::whole_preview(&path) => {
                    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
                    return Ok((path, size));
                }
                // Left by a crash before the commit naming it, so no row names it, and this call holds
                // the file's claim: it's removed, and drawn again.
                Err(Error::Exists { path }) if tries == 0 => {
                    tries += 1;
                    let _ = std::fs::remove_file(path);
                }
                other => return other.map(|p| (p.path, p.size)),
            }
        }
    })
    .await;
    match drawn {
        Ok((path, size)) => {
            let made = Step::Thumbnail {
                sha256: copy.sha256.clone(),
                path: relative(&inner, &path),
                bytes: size,
                at: now_ms(),
            };
            match inner.store.apply(Lane::Live, vec![Change::Media(made)]).await {
                Ok(_) => {
                    inner.clean.notify_one();
                    Drawn::Done
                }
                Err(e) => Drawn::Failed(format!("{e:#}")),
            }
        }
        Err(e) => Drawn::Failed(e.to_string()),
    }
}

/// Makes the 256 and 768 pixel previews of `copy`'s file, at `path` in the cache, on the image lane,
/// and names them; the file stays claimed until they're named.
pub(super) async fn previews(inner: &Arc<Inner>, copy: &Wanted, path: &str, priority: Priority) -> Drawn {
    let _claim = inner.claims.share(&copy.sha256).await;
    let job = draw_previews(inner.clone(), copy.clone(), path.to_string());
    let handle = inner.images.submit(format!("previews:{}", to_hex(&copy.sha256)), priority, job).await;
    let made = match handle.result().await.unwrap_or(Drawn::Nothing) {
        Drawn::Made(made) => made,
        drawn => return drawn,
    };
    match inner.store.apply(Lane::Live, vec![Change::Media(made)]).await {
        Ok(_) => {
            inner.clean.notify_one();
            Drawn::Done
        }
        Err(e) => Drawn::Failed(format!("{e:#}")),
    }
}

/// What jaw-media should take a file as, from its message's kind: a sticker or a voice message is
/// one whatever its file looks like.
fn kind_of(kind: &str, probed: Kind) -> Kind {
    match kind {
        "sticker" if probed == Kind::AnimatedSticker => Kind::AnimatedSticker,
        "sticker" => Kind::Sticker,
        "voice" => Kind::Voice,
        "audio" => Kind::Audio,
        "gif" => Kind::Gif,
        "video" | "video_note" => Kind::Video,
        _ => probed,
    }
}

async fn draw_previews(inner: Arc<Inner>, copy: Wanted, path: String) -> Drawn {
    let _claim = inner.claims.share(&copy.sha256).await;
    let source = inner.home.join(&path);
    let stem = stem(&inner, &copy.sha256);
    let pipeline = inner.pipeline.clone();
    let kind = copy.kind.clone();
    let drawn = blocking(move || {
        let mut probe = pipeline.probe(&source)?;
        probe.kind = kind_of(&kind, probe.kind);
        if previews_lane(&probe) == jaw_media::Lane::Copy {
            return Ok(None);
        }
        let mut tries = 0;
        loop {
            match pipeline.previews(&source, &probe, &stem, &Cancel::new()) {
                Ok(Previews::Pictures { small, large }) => {
                    return Ok(Some((small.path, large.path, small.size + large.size)));
                }
                Ok(Previews::Icon(_)) => return Ok(None),
                Err(Error::Busy { .. }) if tries < BUSY_TRIES => {
                    tries += 1;
                    std::thread::sleep(BUSY_PAUSE);
                }
                Err(Error::Exists { .. }) if tries < BUSY_TRIES => {
                    tries = BUSY_TRIES;
                    if let Some(whole) = whole_pair(&stem) {
                        return Ok(Some(whole));
                    }
                    // Left by a crash before the commit naming them, so no row names them, and this
                    // call holds the file's claim: they're removed, and made again.
                    for label in ["256", "768"] {
                        for alpha in [false, true] {
                            let _ = std::fs::remove_file(jaw_media::preview_name(&stem, label, alpha));
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
    })
    .await;
    match drawn {
        Ok(Some((small, large, bytes))) => Drawn::Made(Step::Previews {
            sha256: copy.sha256.clone(),
            small: relative(&inner, &small),
            large: relative(&inner, &large),
            bytes,
            at: now_ms(),
        }),
        Ok(None) => Drawn::Nothing,
        Err(Error::Missing { tool, needed_for }) => Drawn::Failed(format!("{tool} is missing, needed {needed_for}")),
        Err(e) => Drawn::Failed(e.to_string()),
    }
}

/// The previews already beside `stem`, if both are there and whole, and their bytes.
fn whole_pair(stem: &Path) -> Option<(PathBuf, PathBuf, u64)> {
    [false, true].into_iter().find_map(|alpha| {
        let (small, large) = (jaw_media::preview_name(stem, "256", alpha), jaw_media::preview_name(stem, "768", alpha));
        let whole = jaw_media::whole_preview(&small) && jaw_media::whole_preview(&large);
        let bytes = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
        whole.then(|| (small.clone(), large.clone(), bytes(&small) + bytes(&large)))
    })
}

/// Mends file `sha256`'s previews or thumbnail found broken: each is forgotten in a synced commit,
/// then removed, while no job may name it, and copy `message` is wanted again, so they're made
/// again. Says whether it could, which it can't while a job holds the file.
pub(super) async fn mend(inner: &Arc<Inner>, sha256: &[u8], message: Option<i64>, broken: &[(Made, String)]) -> bool {
    let Some(_claim) = inner.claims.exclusive(sha256) else { return false };
    let forget: Vec<Change> = broken
        .iter()
        .map(|(made, path)| Change::Media(Step::Forget { sha256: sha256.to_vec(), made: *made, path: path.clone() }))
        .collect();
    if inner.store.apply_durable(forget).await.is_err() {
        return false;
    }
    let mut removed = Vec::new();
    for (made, path) in broken {
        match made {
            Made::Thumbnail => removed.extend(super::cache::inside(&inner.home, path)),
            Made::Previews => {
                let stem = stem(inner, sha256);
                for label in ["256", "768"] {
                    for alpha in [false, true] {
                        removed.push(jaw_media::preview_name(&stem, label, alpha));
                    }
                }
            }
        }
    }
    let _ = blocking(move || {
        for path in removed {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    })
    .await;
    inner.stats.lock().expect("media stats").remade += broken.len() as u64;
    if let Some(message) = message {
        let again = Change::Media(Step::Want { message, variant: "sd", at: now_ms(), asked: false });
        let _ = inner.store.apply(Lane::Live, vec![again]).await;
        inner.schedule.wake();
    }
    true
}

/// Runs jaw-media's blocking work off the runtime's threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> jaw_media::Result<T> + Send + 'static,
) -> jaw_media::Result<T> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|e| {
        Err(Error::Io { path: PathBuf::new(), error: std::io::Error::other(format!("the work stopped: {e}")) })
    })
}
