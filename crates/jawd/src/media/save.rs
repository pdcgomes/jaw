//! `media.save`: a message's file copied out of the cache to where it's asked, downloaded first if
//! it isn't yet; written whole under a name of its own and synced before it's answered, and never
//! over a file already there, which gives it a name with a number instead.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jaw_proto::media::{MediaSave, Saved};
use jaw_proto::{Error, code};
use rusqlite::OptionalExtension;
use serde_json::Value;

use super::{Inner, durably, internal, named, now_ms};
use crate::store::media::Step;
use crate::store::{Change, Lane};
use crate::work::Priority;

/// How long a save waits for its file to download.
const WAIT: Duration = Duration::from_secs(600);

/// Numbers the partial files saves write, so two never share one.
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) async fn save(inner: &Arc<Inner>, save: MediaSave) -> Result<Value, Error> {
    let to = target(&save.to)?;
    let named = named(inner, &save.message, save.chat.as_deref(), save.variant.unwrap_or_default()).await?;
    let local = match named.local.clone() {
        Some(local) => local,
        None => {
            let want = Step::Want { message: named.message, variant: named.variant, at: now_ms(), asked: true };
            durably(inner, vec![Change::Media(want)], "keep that the file was asked for").await?;
            inner.schedule.wake();
            downloaded(inner, named.message, named.variant).await?
        }
    };
    let source = inner.home.join(&local);
    let ext = Path::new(&local).extension().and_then(|e| e.to_str()).unwrap_or("bin").to_string();
    let name = named
        .name
        .as_deref()
        .map(|name| name.replace(['/', '\\', '\0'], "_"))
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .unwrap_or_else(|| format!("{}.{ext}", named.id));
    let saved = tokio::task::spawn_blocking(move || copy_whole(&source, &to, &name))
        .await
        .map_err(|e| internal("save the file", e))?
        .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't save the file: {e}")))?;
    if let Some(sha256) = named.sha256 {
        let used = Change::Media(Step::Used { sha256, at: now_ms() });
        let _ = inner.store.apply(Lane::Live, vec![used]).await;
    }
    Ok(serde_json::to_value(Saved { path: saved.display().to_string() }).expect("a saved path serializes"))
}

/// Where `to` says to save: a whole path, or one from your home folder with `~/`.
fn target(to: &str) -> Result<PathBuf, Error> {
    let path = match to.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest)),
        None => Some(PathBuf::from(to)).filter(|p| p.is_absolute()),
    };
    path.ok_or_else(|| {
        let mut error = Error::new(code::INVALID_PARAMS, "media.save's `to` is a whole path, or one starting ~/");
        error.data = Some(serde_json::json!({ "param": "to" }));
        error
    })
}

/// Waits for the copy's file to be in the cache, and says where; or why it can't be.
async fn downloaded(inner: &Inner, message: i64, variant: &'static str) -> Result<String, Error> {
    let mut committed = inner.store.committed();
    let waited = tokio::time::timeout(WAIT, async {
        loop {
            let now = inner
                .store
                .reads()
                .query(Priority::Interactive, move |db| {
                    Ok(db
                        .prepare_cached(
                            "SELECT f.path, md.state, md.error, md.wanted IS NOT NULL FROM media md
                            LEFT JOIN media_files f ON f.sha256 = md.sha256 WHERE md.message = ?1 AND md.variant = ?2",
                        )?
                        .query_row(rusqlite::params![message, variant], |r| {
                            Ok((
                                r.get::<_, Option<String>>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, Option<String>>(2)?,
                                r.get::<_, bool>(3)?,
                            ))
                        })
                        .optional()?)
                })
                .await
                .map_err(|e| internal("read jaw.db", e))?;
            match now {
                None => return Err(Error::new(code::NOT_FOUND, "the message went while its file was downloading")),
                Some((Some(path), ..)) => return Ok(path),
                Some((None, state, error, false)) if state != "waiting_for_phone" => {
                    let why = error.unwrap_or_else(|| "its download was cancelled".into());
                    return Err(Error::new(code::NOT_ALLOWED, format!("jawd couldn't download the file: {why}")));
                }
                Some(_) => {}
            }
            if committed.changed().await.is_err() {
                return Err(Error::new(code::INTERNAL, "jawd's store stopped"));
            }
        }
    })
    .await;
    waited.unwrap_or_else(|_| {
        Err(Error::new(code::NOT_ALLOWED, "the file didn't download in time; it goes on downloading, so save it again"))
    })
}

/// Copies `source` to `to`, or into it with `name` if it's a folder, under a name nothing has yet:
/// written to a partial file beside it, synced, linked in, and its folder synced.
fn copy_whole(source: &Path, to: &Path, name: &str) -> std::io::Result<PathBuf> {
    let (folder, file) = if to.is_dir() {
        (to.to_path_buf(), name.to_string())
    } else {
        let folder = to.parent().filter(|p| p.is_dir()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} isn't a folder there is", to.display()))
        })?;
        (folder.to_path_buf(), to.file_name().and_then(|n| n.to_str()).unwrap_or(name).to_string())
    };
    let part = folder.join(format!(".{file}.{}-{}.part", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let copied = (|| {
        std::fs::copy(source, &part)?;
        std::fs::File::open(&part)?.sync_all()
    })();
    if let Err(e) = copied {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    let (stem, ext) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (file.clone(), String::new()),
    };
    let mut placed = None;
    for n in 0..1_000 {
        let candidate = folder.join(if n == 0 { file.clone() } else { format!("{stem} ({n}){ext}") });
        match std::fs::hard_link(&part, &candidate) {
            Ok(()) => {
                placed = Some(candidate);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        }
    }
    let _ = std::fs::remove_file(&part);
    let placed = placed.ok_or_else(|| std::io::Error::other("a thousand files there have its name already"))?;
    std::fs::File::open(&folder)?.sync_all()?;
    Ok(placed)
}
