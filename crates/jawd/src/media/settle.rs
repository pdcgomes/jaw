//! Previews and thumbnails made durable after the fact, and checked at a start. They're put in place
//! without waiting for the disk, each write numbered in jaw.db (`media_made`). Every so often, and
//! at a clean stop, those written since the last time are sent to the drive and a synced commit
//! notes up to which number they're durable. At a start, those past that number may have been
//! broken by a power cut: they're checked off the read path, which doesn't show them until the
//! check has passed them, and a broken one is mended, forgotten in a synced commit, removed and
//! made again, before anything is settled past it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::{Inner, PREVIEWS, previews};
use crate::store::media::{Made, Step};
use crate::store::{Change, Lane};
use crate::work::Priority;

/// Files checked or settled at once.
const AT_ONCE: i64 = 256;
/// How long the note that previews are durable may wait to share a synced commit.
const NOTE_WITHIN: Duration = Duration::from_secs(1);
/// How often, and how many times, mending a file a job holds is tried again.
const MEND_PAUSE: Duration = Duration::from_millis(50);
const MEND_TRIES: u32 = 600;

/// Checks what a start found unsettled, numbered past `settled` and up to `made`, then settles
/// what's made from then on, every `settle_every`.
pub(super) fn start(inner: &Arc<Inner>, settled: i64, made: i64) {
    let inner = inner.clone();
    tokio::spawn(async move {
        check(&inner, settled, made).await;
        loop {
            tokio::time::sleep(inner.config.settle_every).await;
            if let Err(e) = settle(&inner, NOTE_WITHIN).await {
                eprintln!("jawd: can't make the previews made lately durable this time: {e:#}");
            }
        }
    });
}

/// A file's previews and thumbnail as jaw.db names them, with its number and a copy of it.
struct Derived {
    sha256: Vec<u8>,
    made: i64,
    message: Option<i64>,
    small: Option<String>,
    large: Option<String>,
    thumbnail: Option<String>,
}

const UNSETTLED: &str = "SELECT f.sha256, f.made, f.small, f.large, f.thumbnail,
        (SELECT md.message FROM media md WHERE md.sha256 = f.sha256 AND md.variant = 'sd' LIMIT 1)
    FROM media_files f WHERE f.made > ?1 AND f.made <= ?2 ORDER BY f.made LIMIT ?3";

/// The files whose previews or thumbnail were last written as a number past `after` and up to
/// `upto`, the lowest first, AT_ONCE at most.
async fn unsettled(inner: &Inner, after: i64, upto: i64) -> anyhow::Result<Vec<Derived>> {
    inner
        .store
        .reads()
        .query(Priority::Background, move |db| {
            Ok(db
                .prepare_cached(UNSETTLED)?
                .query_map(rusqlite::params![after, upto, AT_ONCE], |r| {
                    Ok(Derived {
                        sha256: r.get(0)?,
                        made: r.get(1)?,
                        small: r.get(2)?,
                        large: r.get(3)?,
                        thumbnail: r.get(4)?,
                        message: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
}

/// Checks the previews and thumbnails numbered past `settled` and up to `made`, as a start finds
/// them: rows show each once the check has passed it, whole ones at once, broken ones once mended.
async fn check(inner: &Arc<Inner>, settled: i64, made: i64) {
    let files = inner.store.files().clone();
    let mut after = settled;
    while after < made {
        let rows = match unsettled(inner, after, made).await {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("jawd: can't read which previews to check, so it's tried again: {e:#}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let Some(last) = rows.last() else { break };
        let through = last.made;
        let home = inner.home.clone();
        let looked = tokio::task::spawn_blocking(move || rows.into_iter().map(|row| broken(&home, row)).collect())
            .await
            .unwrap_or_else(|_| Vec::new());
        let mut whole = Vec::new();
        for (row, broken) in looked {
            if broken.is_empty() {
                whole.push(row.sha256);
                continue;
            }
            // A job holding the file is downloading it or drawing its thumbnail for a moment.
            let mut tries = 0;
            while !previews::mend(inner, &row.sha256, row.message, &broken).await && tries < MEND_TRIES {
                tries += 1;
                tokio::time::sleep(MEND_PAUSE).await;
            }
        }
        files.checked(through);
        if !whole.is_empty() {
            let shown = Change::Media(Step::Checked { sha256s: whole });
            let _ = inner.store.apply(Lane::Live, vec![shown]).await;
        }
        after = through;
    }
    files.checked(made);
    inner.checked.store(true, Ordering::SeqCst);
}

/// What of `row`'s previews and thumbnail isn't whole on disk.
fn broken(home: &Path, row: Derived) -> (Derived, Vec<(Made, String)>) {
    let whole = |path: &String| jaw_media::whole_preview(&home.join(path));
    let mut broken = Vec::new();
    if let (Some(small), Some(large)) = (&row.small, &row.large)
        && !(whole(small) && whole(large))
    {
        broken.push((Made::Previews, small.clone()));
    }
    if let Some(thumbnail) = row.thumbnail.as_ref().filter(|path| !whole(path)) {
        broken.push((Made::Thumbnail, thumbnail.clone()));
    }
    (row, broken)
}

/// Sends the previews and thumbnails made since the last time to the drive, then notes in a synced
/// commit, which may wait `within` to share one, up to which number they're durable. Nothing
/// settles before the start's check is done.
pub(super) async fn settle(inner: &Arc<Inner>, within: Duration) -> anyhow::Result<()> {
    if !inner.checked.load(Ordering::SeqCst) {
        return Ok(());
    }
    loop {
        let (settled, made) = inner.store.pair("SELECT settled, made FROM media_made WHERE id = 1")?;
        let rows = unsettled(inner, settled, made).await?;
        let Some(last) = rows.last() else { return Ok(()) };
        let through = last.made;
        let paths: Vec<PathBuf> = rows
            .iter()
            .flat_map(|row| [&row.small, &row.large, &row.thumbnail])
            .flatten()
            .map(|path| inner.home.join(path))
            .collect();
        let folder = inner.home.join(PREVIEWS);
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            // One forgotten and removed meanwhile is no longer named, so it needs nothing.
            for path in paths {
                if let Ok(file) = std::fs::File::open(path) {
                    push(&file)?;
                }
            }
            push(&std::fs::File::open(folder)?)
        })
        .await??;
        inner.store.apply_durable_soon(vec![Change::Media(Step::Flushed { through })], within).await?;
        if (rows.len() as i64) < AT_ONCE {
            return Ok(());
        }
    }
}

/// Sends what's written to `file` to the drive. On macOS fsync stops at the drive's cache, so the
/// synced commit after it, whose F_FULLFSYNC empties the cache, makes it durable; elsewhere fsync
/// is durable itself.
fn push(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor is the open file's, which outlives the call.
        if unsafe { libc::fsync(file.as_raw_fd()) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
    }
    #[cfg(not(target_os = "macos"))]
    file.sync_data()
}
