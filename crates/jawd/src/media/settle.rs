//! Previews and thumbnails made durable after the fact, checked at a start, and looked at weekly.
//! They're put in place without waiting for the disk, each write numbered in jaw.db (`media_made`).
//! Every so often, and at a clean stop, those written since the last time are sent to the drive,
//! fenced with one barrier, and only then noted as durable up to their number, so the note can't
//! reach the disk before them (`drive`). At a start, the files with writes past that number may have
//! been broken by a power cut: they're suspect, checked off the read path, which doesn't show their
//! previews and thumbnail until the check has passed them, and a broken one is mended, forgotten in
//! a synced commit, removed and made again. One a job holds the file of stays suspect until it can
//! be mended, and nothing settles until every one has been passed. Weekly, every file's previews and
//! thumbnail are looked at the same way, for what a disk error or a hand from outside broke.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::drive::Flush;
use super::{Inner, PREVIEWS, previews};
use crate::store::media::{Made, Step};
use crate::store::{Change, Lane};
use crate::work::Priority;

/// Files checked, looked at or settled at once.
const AT_ONCE: i64 = 256;
/// How long the check waits before trying again a file it couldn't mend, or a page it couldn't
/// read: AGAIN at first, doubling to AGAIN_MOST.
const AGAIN: Duration = Duration::from_millis(50);
const AGAIN_MOST: Duration = Duration::from_secs(1);
/// Where jaw.db keeps when every file's previews were last looked at, in ms since 1970.
const LOOKED: &str = "media_looked_at";

/// The files, by hash, with previews or thumbnails written past `settled`, up to `made`, which a
/// power cut may have broken, as a start finds them.
pub(super) async fn suspects(inner: &Inner, settled: i64, made: i64) -> anyhow::Result<Vec<Vec<u8>>> {
    let mut suspects = Vec::new();
    let mut after = settled;
    while after < made {
        let page = numbered(inner, after, made).await?;
        let Some(last) = page.last() else { break };
        after = last.made;
        suspects.extend(page.into_iter().map(|row| row.sha256));
    }
    Ok(suspects)
}

/// Checks what a start found suspect, then settles what's made from then on, every `settle_every`,
/// and looks at every file's previews every `look_every`.
pub(super) fn start(inner: &Arc<Inner>, suspects: Vec<Vec<u8>>) {
    let inner = inner.clone();
    tokio::spawn(async move {
        check(&inner, suspects).await;
        let mut look_at = next_look(&inner).await;
        loop {
            let settle_at = tokio::time::Instant::now() + inner.config.settle_every;
            tokio::time::sleep_until(settle_at.min(look_at)).await;
            if tokio::time::Instant::now() >= look_at {
                look(&inner).await;
                look_at = tokio::time::Instant::now() + inner.config.look_every;
            } else if let Err(e) = settle(&inner).await {
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

const NUMBERED: &str = "SELECT f.sha256, f.made, f.small, f.large, f.thumbnail,
        (SELECT md.message FROM media md WHERE md.sha256 = f.sha256 AND md.variant = 'sd' LIMIT 1)
    FROM media_files f WHERE f.made > ?1 AND f.made <= ?2 ORDER BY f.made LIMIT ?3";
const BY_HASH: &str = "SELECT f.sha256, f.made, f.small, f.large, f.thumbnail,
        (SELECT md.message FROM media md WHERE md.sha256 = f.sha256 AND md.variant = 'sd' LIMIT 1)
    FROM media_files f WHERE f.sha256 = ?1";
const EVERY: &str = "SELECT f.sha256, f.made, f.small, f.large, f.thumbnail,
        (SELECT md.message FROM media md WHERE md.sha256 = f.sha256 AND md.variant = 'sd' LIMIT 1)
    FROM media_files f WHERE f.sha256 > ?1 ORDER BY f.sha256 LIMIT ?2";

fn derived(r: &rusqlite::Row) -> rusqlite::Result<Derived> {
    Ok(Derived {
        sha256: r.get(0)?,
        made: r.get(1)?,
        small: r.get(2)?,
        large: r.get(3)?,
        thumbnail: r.get(4)?,
        message: r.get(5)?,
    })
}

/// The files whose previews or thumbnail were last written as a number past `after` and up to
/// `upto`, the lowest first, AT_ONCE at most.
async fn numbered(inner: &Inner, after: i64, upto: i64) -> anyhow::Result<Vec<Derived>> {
    inner
        .store
        .reads()
        .query(Priority::Background, move |db| {
            Ok(db
                .prepare_cached(NUMBERED)?
                .query_map(rusqlite::params![after, upto, AT_ONCE], derived)?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
}

/// Checks the files a start found suspect: each is passed once it's found whole, or once it's
/// mended, and the messages showing it get a new version, so windows show its previews. One a job
/// holds the file of is tried again until it can be mended, and so is a page whose files couldn't
/// be read; only then can anything settle.
async fn check(inner: &Arc<Inner>, suspects: Vec<Vec<u8>>) {
    tokio::time::sleep(inner.config.check_after).await;
    let (mut left, mut again, mut failing) = (suspects, AGAIN, None);
    while !left.is_empty() {
        let mut still = Vec::new();
        for page in left.chunks(AT_ONCE as usize) {
            match look_at(inner, page.to_vec()).await {
                Ok(unmended) => still.extend(unmended),
                Err(e) => {
                    let why = format!("{e:#}");
                    if failing.as_ref() != Some(&why) {
                        eprintln!(
                            "jawd: can't check the previews a power cut may have broken, so it's tried again: {why}"
                        );
                        failing = Some(why);
                    }
                    still.extend(page.iter().cloned());
                }
            }
        }
        left = still;
        if !left.is_empty() {
            tokio::time::sleep(again).await;
            again = (again * 2).min(AGAIN_MOST);
        }
    }
    inner.checked.store(true, Ordering::SeqCst);
}

/// Looks at the previews and thumbnails of the files `hashes` names, as jaw.db names them now:
/// whole ones are passed, broken ones mended and then passed, and the messages showing either get a
/// new version. Says which it couldn't mend, because a job holds their file.
async fn look_at(inner: &Arc<Inner>, hashes: Vec<Vec<u8>>) -> anyhow::Result<Vec<Vec<u8>>> {
    if let Some(failing) = &inner.config.looks_failing
        && one_less(failing)
    {
        anyhow::bail!("a test failed this look before it read anything");
    }
    let rows = inner
        .store
        .reads()
        .query(Priority::Background, move |db| {
            let mut rows = Vec::new();
            let mut by_hash = db.prepare_cached(BY_HASH)?;
            for sha256 in &hashes {
                match by_hash.query_row([sha256], derived) {
                    Ok(row) => rows.push(row),
                    // A file dropped since names nothing to check.
                    Err(rusqlite::Error::QueryReturnedNoRows) => rows.push(Derived {
                        sha256: sha256.clone(),
                        made: 0,
                        message: None,
                        small: None,
                        large: None,
                        thumbnail: None,
                    }),
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(rows)
        })
        .await?;
    let home = inner.home.clone();
    let looked =
        tokio::task::spawn_blocking(move || rows.into_iter().map(|row| broken(&home, row)).collect::<Vec<_>>())
            .await
            .map_err(|e| anyhow::anyhow!("the look at the files stopped: {e}"))?;
    let (mut passed, mut unmended) = (Vec::new(), Vec::new());
    for (row, broken) in looked {
        if broken.is_empty() || previews::mend(inner, &row.sha256, row.message, &broken).await {
            passed.push(row.sha256);
        } else {
            unmended.push(row.sha256);
        }
    }
    inner.store.files().passed(&passed);
    if !passed.is_empty() {
        let shown = Change::Media(Step::Checked { sha256s: passed });
        let _ = inner.store.apply(Lane::Background, vec![shown]).await;
    }
    Ok(unmended)
}

/// Takes one from `count` unless it's 0, saying whether it did.
fn one_less(count: &AtomicU32) -> bool {
    let mut n = count.load(Ordering::SeqCst);
    while n > 0 {
        match count.compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(now) => n = now,
        }
    }
    false
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

/// When every file's previews are next to be looked at: `look_every` after the last time jaw.db
/// says they were, or after the first start, whose time it then notes.
async fn next_look(inner: &Inner) -> tokio::time::Instant {
    let last: Option<i64> = inner.store.remembered(LOOKED).ok().flatten().and_then(|at| at.parse().ok());
    let last = match last {
        Some(last) => last,
        None => {
            let first = Change::Remember { key: LOOKED.into(), value: now_ms().to_string() };
            let _ = inner.store.apply(Lane::Background, vec![first]).await;
            now_ms()
        }
    };
    let since = Duration::from_millis((now_ms() - last).max(0) as u64);
    tokio::time::Instant::now() + inner.config.look_every.saturating_sub(since)
}

/// Looks at every file's previews and thumbnail, a page at a time in the background, mending what
/// a disk error or a hand from outside broke, then notes when it did.
async fn look(inner: &Arc<Inner>) {
    let mut after = Vec::new();
    loop {
        let from = after.clone();
        let page = inner
            .store
            .reads()
            .query(Priority::Background, move |db| {
                Ok(db
                    .prepare_cached(EVERY)?
                    .query_map(rusqlite::params![from, AT_ONCE], derived)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await;
        let page = match page {
            Ok(page) => page,
            Err(e) => {
                eprintln!("jawd: can't look at the previews this week: {e:#}");
                return;
            }
        };
        let Some(last) = page.last() else { break };
        after = last.sha256.clone();
        let hashes: Vec<Vec<u8>> = page.into_iter().map(|row| row.sha256).collect();
        // One a job holds the file of is being written now, and is looked at next week.
        if let Err(e) = look_at(inner, hashes).await {
            eprintln!("jawd: can't look at the previews this week: {e:#}");
            return;
        }
    }
    let noted = Change::Remember { key: LOOKED.into(), value: now_ms().to_string() };
    let _ = inner.store.apply(Lane::Background, vec![noted]).await;
}

/// Sends the previews and thumbnails made since the last time to the drive, fences them with one
/// barrier, then notes up to which number they're durable, so the note can't reach the disk before
/// them; durable once the next synced commit is. A file gone since names nothing to send, but one
/// that can't be opened for another reason stops it. Nothing settles before the start's check is
/// done.
pub(super) async fn settle(inner: &Arc<Inner>) -> anyhow::Result<()> {
    if !inner.checked.load(Ordering::SeqCst) {
        return Ok(());
    }
    loop {
        let (settled, made) = inner.store.pair("SELECT settled, made FROM media_made WHERE id = 1")?;
        let rows = numbered(inner, settled, made).await?;
        let Some(last) = rows.last() else { return Ok(()) };
        let through = last.made;
        let paths: Vec<PathBuf> = rows
            .iter()
            .flat_map(|row| [&row.small, &row.large, &row.thumbnail])
            .flatten()
            .map(|path| inner.home.join(path))
            .collect();
        let (drive, folder) = (inner.drive.clone(), inner.home.join(PREVIEWS));
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            for path in paths {
                match drive.send(Flush::Push, &path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
            drive.send(Flush::Push, &folder)?;
            drive.send(Flush::Barrier, &folder)
        })
        .await??;
        inner.store.apply(Lane::Background, vec![Change::Media(Step::Flushed { through })]).await?;
        if (rows.len() as i64) < AT_ONCE {
            return Ok(());
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}
