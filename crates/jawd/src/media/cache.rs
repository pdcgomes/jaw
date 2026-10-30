//! The cache: its folders, made and synced at start with a stopped jawd's partial files removed; the
//! claims that keep a file from being removed while a job may name it; the sweep at start of files
//! no row names; and the cleaner, which removes the files no copy names any more and keeps the cache
//! to its size, dropping what was used longest ago first, but never a starred message's file or one
//! waiting to be sent. Every removal follows a synced commit that has stopped naming the file.

use std::collections::HashMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;
use tokio::sync::Notify;

use super::outgoing::{OUTGOING, UPLOADS};
use super::{FILES, Inner, PREVIEWS};
use crate::store::media::{Dropped, Step, from_hex};
use crate::store::{Change, Store};
use crate::work::Priority;

/// Files jobs are writing or naming (each a shared claim) and files being removed (an exclusive
/// one), by hash: a file is removed only while no job may name it, and no job names one while it's
/// being removed.
#[derive(Clone, Default)]
pub(super) struct Claims(Arc<ClaimsInner>);

#[derive(Default)]
struct ClaimsInner {
    /// How many jobs share each file, or -1 while it's being removed.
    held: Mutex<HashMap<Vec<u8>, isize>>,
    freed: Notify,
}

/// A job's claim on a file, until it's dropped.
pub(super) struct Shared {
    claims: Claims,
    sha256: Vec<u8>,
}

/// The cleaner's claim on a file, until it's dropped.
pub(super) struct Exclusive {
    claims: Claims,
    sha256: Vec<u8>,
}

impl Claims {
    /// A job's claim on `sha256`, waiting while the file is being removed.
    pub async fn share(&self, sha256: &[u8]) -> Shared {
        loop {
            let freed = self.0.freed.notified();
            {
                let mut held = self.0.held.lock().expect("claims");
                let count = held.entry(sha256.to_vec()).or_insert(0);
                if *count >= 0 {
                    *count += 1;
                    return Shared { claims: self.clone(), sha256: sha256.to_vec() };
                }
            }
            freed.await;
        }
    }

    /// The cleaner's claim on `sha256`, or None while a job holds it.
    pub fn exclusive(&self, sha256: &[u8]) -> Option<Exclusive> {
        let mut held = self.0.held.lock().expect("claims");
        if held.get(sha256).is_some_and(|&count| count != 0) {
            return None;
        }
        held.insert(sha256.to_vec(), -1);
        Some(Exclusive { claims: self.clone(), sha256: sha256.to_vec() })
    }

    fn release(&self, sha256: &[u8], exclusive: bool) {
        let mut held = self.0.held.lock().expect("claims");
        let free = match held.get_mut(sha256) {
            Some(count) if exclusive => {
                *count = 0;
                true
            }
            Some(count) => {
                *count -= 1;
                *count == 0
            }
            None => true,
        };
        if free {
            held.remove(sha256);
        }
        drop(held);
        self.0.freed.notify_waiters();
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        self.claims.release(&self.sha256, false);
    }
}

impl Drop for Exclusive {
    fn drop(&mut self) {
        self.claims.release(&self.sha256, true);
    }
}

/// Makes the cache's folders, syncing each one made and the folder it was made in, so no job makes
/// a folder; then removes the partial files a stopped jawd left in them, before any job runs, since
/// a running job's partial file looks the same. Says how many it removed.
pub(super) fn prepare(home: &Path) -> anyhow::Result<usize> {
    prepare_syncing(home, &|folder| std::fs::File::open(folder)?.sync_all())
}

/// `prepare`, syncing each folder with `sync`.
pub(super) fn prepare_syncing(home: &Path, sync: &dyn Fn(&Path) -> std::io::Result<()>) -> anyhow::Result<usize> {
    // Link previews' thumbnails make media/ as they make media/links, at any moment and without
    // syncing it in jaw's folder, so one made meanwhile is synced as if it were made here, and one
    // found made is synced in jaw's folder once the cache's folders are made in it.
    let found = home.join("media").is_dir();
    let mut made = false;
    for folder in ["media", FILES, PREVIEWS, OUTGOING, UPLOADS] {
        let path = home.join(folder);
        if path.is_dir() {
            continue;
        }
        match std::fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(e) => return Err(e.into()),
        }
        made = true;
        sync(&path)?;
        sync(path.parent().unwrap_or(home))?;
    }
    if found && made {
        sync(home)?;
    }
    let mut removed = 0;
    for folder in [FILES, PREVIEWS, OUTGOING, UPLOADS] {
        removed += jaw_media::remove_partial(&home.join(folder))?;
    }
    Ok(removed)
}

/// The cache's bytes on disk and its files, as jaw.db's triggers keep them.
pub(super) fn size(store: &Store) -> anyhow::Result<(u64, u64)> {
    let (bytes, files) = store.pair("SELECT bytes, files FROM media_cache WHERE id = 1")?;
    Ok((bytes.max(0) as u64, files.max(0) as u64))
}

pub(super) fn start(inner: &Arc<Inner>) {
    tokio::spawn(sweep(inner.clone()));
    tokio::spawn(clean(inner.clone()));
}

impl Inner {
    /// The cache's size: `cache_bytes`, or what a test has it be now.
    pub(super) fn cache_limit(&self) -> u64 {
        match &self.config.limit {
            Some(limit) => limit.load(std::sync::atomic::Ordering::Relaxed),
            None => self.config.cache_bytes,
        }
    }
}

/// The hash a file in the cache is named by: the hex its name starts with.
fn hash_of(name: &str) -> Option<Vec<u8>> {
    from_hex(name.get(..64)?).filter(|_| name.as_bytes().get(64) == Some(&b'.'))
}

/// What a row names of file `sha256`: its file, previews and thumbnail, relative to jaw's folder.
async fn named(store: &Store, sha256: Vec<u8>) -> anyhow::Result<Option<Vec<String>>> {
    store
        .reads()
        .query(Priority::Background, move |db| {
            let paths: Option<[Option<String>; 4]> = db
                .prepare_cached("SELECT path, small, large, thumbnail FROM media_files WHERE sha256 = ?1")?
                .query_row([sha256], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))
                .optional()?;
            Ok(paths.map(|paths| paths.into_iter().flatten().collect()))
        })
        .await
}

/// Removes, once at start, the files in the cache's folders that no row names: what a crash left
/// between a file's placing and the commit naming it, or a removal a power cut undid. jaw.db's state
/// is durable by now, and each file is looked at while it's claimed, so none a job is naming goes.
async fn sweep(inner: Arc<Inner>) {
    let mut removed = 0u64;
    for folder in [FILES, PREVIEWS] {
        let dir = inner.home.join(folder);
        let names = tokio::task::spawn_blocking(move || -> Vec<String> {
            std::fs::read_dir(&dir)
                .map(|entries| entries.filter_map(|e| Some(e.ok()?.file_name().to_str()?.to_string())).collect())
                .unwrap_or_default()
        })
        .await
        .unwrap_or_default();
        for name in names {
            let relative = format!("{folder}/{name}");
            let Some(sha256) = hash_of(&name) else { continue };
            let Some(_claim) = inner.claims.exclusive(&sha256) else { continue };
            match named(&inner.store, sha256).await {
                Ok(Some(paths)) if paths.contains(&relative) => continue,
                Ok(_) => {}
                Err(e) => {
                    eprintln!("jawd: can't read which media files are named, so the sweep stops: {e:#}");
                    return;
                }
            }
            if std::fs::remove_file(inner.home.join(&relative)).is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        eprintln!("jawd: removed {removed} media files no message names");
    }
    inner.stats.lock().expect("media stats").orphans_removed += removed;
}

/// How often, at most, the cleaner looks for files no copy names while jaw.db is being written.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// The files one drop takes.
const DROP_AT_ONCE: i64 = 64;

/// Removes the files no copy names any more, and keeps the cache to its size, until jawd stops.
async fn clean(inner: Arc<Inner>) {
    let mut committed = inner.store.committed();
    loop {
        let woken = inner.clean.notified();
        match tidy(&inner).await {
            Ok(true) => continue,
            Ok(false) => {
                #[cfg(test)]
                inner.tidied.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            Err(e) => eprintln!("jawd: can't tidy the media cache this time: {e:#}"),
        }
        let looked = Instant::now();
        tokio::select! {
            () = woken => {}
            changed = committed.changed() => {
                if changed.is_err() {
                    return;
                }
                // A message deleted may have left a file no copy names; looked for at most so often.
                tokio::time::sleep_until((looked + LOOK_EVERY).into()).await;
            }
        }
    }
}

/// One step of tidying: files no copy names, a page at a time past those a job holds, which go once
/// it's done, then, past the cache's size, what was used longest ago. Says whether there may be
/// more to do.
async fn tidy(inner: &Arc<Inner>) -> anyhow::Result<bool> {
    let (mut after, mut dropped) = (Vec::new(), 0);
    loop {
        let from = after.clone();
        let unnamed: Vec<Vec<u8>> = inner
            .store
            .reads()
            .query(Priority::Background, move |db| {
                Ok(db
                    .prepare_cached("SELECT sha256 FROM media_drop WHERE sha256 > ?1 ORDER BY sha256 LIMIT ?2")?
                    .query_map(rusqlite::params![from, DROP_AT_ONCE], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await?;
        let Some(last) = unnamed.last() else { break };
        after = last.clone();
        let whole_page = unnamed.len() as i64 == DROP_AT_ONCE;
        dropped += drop_files(inner, unnamed, false).await?.0;
        if !whole_page {
            break;
        }
    }
    if dropped > 0 {
        return Ok(true);
    }
    let bytes: i64 = inner
        .store
        .reads()
        .query(Priority::Background, |db| {
            Ok(db.prepare_cached("SELECT bytes FROM media_cache WHERE id = 1")?.query_row([], |r| r.get(0))?)
        })
        .await?;
    let bytes = bytes.max(0) as u64;
    let limit = inner.cache_limit();
    if bytes <= limit {
        return Ok(false);
    }
    // Down to nine tenths of the limit, so the cache doesn't drop a file for every one it places;
    // only what a drop removes counts, so files it keeps (a starred message's, one waiting to be
    // sent, one a job holds) are gone past rather than taken as room made.
    let mut over = bytes - limit * 9 / 10;
    let mut after: (i64, Vec<u8>) = (i64::MIN, Vec::new());
    while over > 0 {
        let from = after.clone();
        let oldest: Vec<(i64, Vec<u8>, i64)> = inner
            .store
            .reads()
            .query(Priority::Background, move |db| {
                Ok(db
                    .prepare_cached(
                        "SELECT used_at, sha256, stored FROM media_files WHERE (used_at, sha256) > (?1, ?2)
                        ORDER BY used_at, sha256 LIMIT ?3",
                    )?
                    .query_map(rusqlite::params![from.0, from.1, DROP_AT_ONCE], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await?;
        if oldest.is_empty() {
            break;
        }
        // The next look goes on from the last file taken, so none after it is passed over.
        let mut take = Vec::new();
        let mut covered = 0;
        for (used_at, sha256, stored) in oldest {
            if covered >= over {
                break;
            }
            covered += stored.max(0) as u64;
            after = (used_at, sha256.clone());
            take.push(sha256);
        }
        let (evicted, freed) = drop_files(inner, take, true).await?;
        over = over.saturating_sub(freed);
        inner.stats.lock().expect("media stats").evicted += evicted as u64;
    }
    Ok(false)
}

/// Drops `sha256s` from the cache in one synced commit, those claimed by a job left for later, then
/// removes the files of those it dropped; with `evict`, files copies name go too, but for a starred
/// message's or one waiting to be sent. Says how many went, and the bytes they took.
pub(super) async fn drop_files(inner: &Arc<Inner>, sha256s: Vec<Vec<u8>>, evict: bool) -> anyhow::Result<(usize, u64)> {
    let claims: Vec<_> = sha256s.iter().filter_map(|sha256| inner.claims.exclusive(sha256)).collect();
    if claims.is_empty() {
        return Ok((0, 0));
    }
    let held: Vec<Vec<u8>> = claims.iter().map(|claim| claim.sha256.clone()).collect();
    let dropped = Dropped::default();
    let step = Step::Drop { sha256s: held, evict, dropped: dropped.clone() };
    inner.store.apply_durable(vec![Change::Media(step)]).await?;
    let files = dropped.take();
    let home = inner.home.clone();
    let paths: Vec<PathBuf> = files.iter().flat_map(|f| f.paths.iter()).filter_map(|p| inside(&home, p)).collect();
    tokio::task::spawn_blocking(move || {
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
    })
    .await?;
    drop(claims);
    Ok((files.len(), files.iter().map(|file| file.bytes).sum()))
}

/// The file a row names, if it's in the cache's folders, as every path media writes is.
pub(super) fn inside(home: &Path, relative: &str) -> Option<PathBuf> {
    let safe = (relative.starts_with(&format!("{FILES}/")) || relative.starts_with(&format!("{PREVIEWS}/")))
        && !relative.split('/').any(|part| part == ".." || part.is_empty());
    safe.then(|| home.join(relative))
}
