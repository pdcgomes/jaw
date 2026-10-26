//! Downloads: one at a time for each file, through the downloads lane, by the engine straight into
//! the cache under the file's hash, after free space is checked. A file WhatsApp's server no longer
//! has is asked of the phone again, a few a minute at most, when an interface asked for it or it
//! arrived live; the phone's answer comes through the inbox (`sync::drain`), and one that doesn't
//! come in time leaves the copy expired, retryable, and saying why.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::schedule::Wanted;
use super::{FILES, Inner, now_ms, retryable};
use crate::convert;
use crate::engine::pb::media_file::State;
use crate::engine::{EngineError, pb};
use crate::store::media::{Keys, Step, to_hex};
use crate::store::{Change, Lane, unpack};
use crate::work::Priority;

/// What became of a file's download.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Fetched {
    /// In the cache at `path`, relative to jaw's folder, `bytes` long, and every copy shows it.
    Placed {
        path: String,
        bytes: u64,
    },
    /// WhatsApp's server no longer has it.
    Gone(String),
    /// What came isn't the file its message names.
    BadHash(String),
    /// There's no room for it.
    NoSpace(String),
    Cancelled,
    /// Trying again later may work: the engine isn't connected, the network, the server.
    Later(String),
    /// Trying again won't.
    Never(String),
}

/// The downloads lane's key for a file: one download for each, however many copies want it.
pub(super) fn key(sha256: &[u8]) -> String {
    format!("file:{}", to_hex(sha256))
}

/// The file a copy is kept as in the cache, relative to jaw's folder: its hash and an extension for
/// what it is, so another program opening it knows it.
pub(super) fn file_name(copy: &Wanted) -> String {
    let by_mime = match copy.mime.as_deref().unwrap_or_default().split(';').next().unwrap_or_default().trim() {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/heic" => Some("heic"),
        "video/mp4" => Some("mp4"),
        "video/quicktime" => Some("mov"),
        "video/3gpp" => Some("3gp"),
        "audio/ogg" => Some("ogg"),
        "audio/mp4" | "audio/aac" => Some("m4a"),
        "audio/mpeg" => Some("mp3"),
        "application/pdf" => Some("pdf"),
        _ => None,
    };
    let by_name = copy.name.as_deref().and_then(|n| n.rsplit_once('.')).map(|(_, ext)| ext.to_ascii_lowercase());
    let ext = by_mime
        .map(str::to_string)
        .or(by_name.filter(|e| (1..=8).contains(&e.len()) && e.bytes().all(|b| b.is_ascii_alphanumeric())))
        .unwrap_or_else(|| "bin".into());
    format!("{FILES}/{}.{ext}", to_hex(&copy.sha256))
}

/// Downloads `copy`'s file through the downloads lane, or joins its download already going, and
/// says what became of it.
pub(super) async fn file(inner: &Arc<Inner>, copy: &Wanted, priority: Priority) -> Fetched {
    let job = download(inner.clone(), copy.clone(), priority);
    let handle = inner.downloads.submit(key(&copy.sha256), priority, job).await;
    handle.result().await.unwrap_or(Fetched::Cancelled)
}

async fn download(inner: Arc<Inner>, copy: Wanted, priority: Priority) -> Fetched {
    let _claim = inner.claims.share(&copy.sha256).await;
    let relative = file_name(&copy);
    let path = inner.home.join(&relative);
    let fetched = match std::fs::symlink_metadata(&path) {
        // Named by its content, a file already there is this one, whole and synced by the engine
        // that put it there, though a crash came before the commit naming it.
        Ok(there) => Fetched::Placed { path: relative.clone(), bytes: there.len() },
        Err(_) => from_engine(&inner, &copy, &path, &relative, priority).await,
    };
    if let Fetched::Placed { path, bytes } = &fetched {
        let placed = Step::Placed { sha256: copy.sha256.clone(), path: path.clone(), bytes: *bytes, at: now_ms() };
        if let Err(e) = inner.store.apply(Lane::Live, vec![Change::Media(placed)]).await {
            return Fetched::Later(format!("the file is here, and jaw.db couldn't say so yet: {e:#}"));
        }
        inner.clean.notify_one();
    }
    fetched
}

async fn from_engine(inner: &Inner, copy: &Wanted, path: &Path, relative: &str, priority: Priority) -> Fetched {
    let Some(engine) = &inner.engine else { return Fetched::Later("jawd runs no engine".into()) };
    let (Some(media_key), Some(direct_path)) = (&copy.media_key, &copy.direct_path) else {
        return Fetched::Never("its message doesn't say where its file is, or how to read it".into());
    };
    let size = copy.bytes.unwrap_or(0);
    let keep = if priority == Priority::Interactive { 64_000_000 } else { inner.config.keep_free };
    match free_space(&inner.home) {
        Some(free) if free < size.saturating_add(keep) => {
            let why = format!(
                "the disk has {} MB free, and jaw keeps {} MB free beside a file of {} MB",
                free / 1_000_000,
                keep / 1_000_000,
                size / 1_000_000
            );
            if priority == Priority::Background {
                *inner.paused.lock().expect("paused") = Some(why.clone());
            }
            return Fetched::NoSpace(why);
        }
        _ => *inner.paused.lock().expect("paused") = None,
    }
    let kind = match copy.keys {
        Keys::Image => pb::media_download::Kind::Image,
        Keys::Video => pb::media_download::Kind::Video,
        Keys::Audio => pb::media_download::Kind::Audio,
        Keys::Document => pb::media_download::Kind::Document,
    };
    let command = pb::MediaDownload {
        id: to_hex(&copy.sha256),
        kind: kind as i32,
        direct_path: direct_path.clone(),
        media_key: media_key.clone(),
        file_sha256: copy.sha256.clone(),
        file_enc_sha256: copy.enc_sha256.clone().unwrap_or_default(),
        size,
        path: path.display().to_string(),
    };
    match engine.call(pb::command::Kind::MediaDownload(command)).await {
        Ok(pb::reply::Kind::MediaFile(file)) => match file.state() {
            State::Placed | State::Exists => Fetched::Placed { path: relative.to_string(), bytes: file.bytes },
            State::Gone => Fetched::Gone(file.message),
            State::BadHash => Fetched::BadHash(file.message),
            State::NoSpace => Fetched::NoSpace(file.message),
            State::Cancelled => Fetched::Cancelled,
            State::Unspecified => Fetched::Later("the engine said nothing of the file".into()),
        },
        Ok(other) => Fetched::Never(format!("the engine answered a download with {other:?}")),
        Err(e) if retryable(&e) => Fetched::Later(e.to_string()),
        Err(e) => Fetched::Never(e.to_string()),
    }
}

/// The free space on the disk jaw's folder is on, if it can be read.
fn free_space(folder: &Path) -> Option<u64> {
    let name = CString::new(folder.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `name` is a NUL-terminated path and `stats` is a statvfs to fill in.
    (unsafe { libc::statvfs(name.as_ptr(), &mut stats) } == 0).then(|| stats.f_bavail as u64 * stats.f_frsize as u64)
}

/// The phone may be asked this many times a minute, so a window full of history can't keep it busy.
#[derive(Default)]
pub(super) struct Asking(Mutex<Vec<Instant>>);

impl Asking {
    /// Whether the phone may be asked now, counting it if so.
    fn may(&self, per_minute: u32) -> bool {
        let mut asked = self.0.lock().expect("asking");
        asked.retain(|at| at.elapsed() < Duration::from_secs(60));
        if asked.len() >= per_minute as usize {
            return false;
        }
        asked.push(Instant::now());
        true
    }
}

/// What to do with a copy WhatsApp's server no longer has: ask the phone, if an interface asked for
/// it or it arrived live, a few a minute at most; otherwise it's expired, until it's asked for.
/// Says whether the copy is still to be tried again later.
pub(super) async fn gone(inner: &Arc<Inner>, copy: &Wanted, why: String) -> bool {
    let settle = |state: &'static str, error: String| {
        Change::Media(Step::Settled {
            message: copy.message,
            variant: copy.variant,
            wanted: copy.wanted,
            state,
            error: Some(error),
        })
    };
    if !(copy.asked || copy.live) || copy.variant == "hd" {
        let error = format!("{why}; ask for it to have your phone send it again");
        let _ = inner.store.apply(Lane::Live, vec![settle("expired", error)]).await;
        return false;
    }
    if !inner.schedule.asking.may(inner.config.retries_per_minute) {
        return true;
    }
    let Some(engine) = &inner.engine else { return true };
    let message = copy.message;
    let key = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            let (envelope, raw): (Option<Vec<u8>>, Option<Vec<u8>>) = db
                .prepare_cached("SELECT envelope, raw FROM messages WHERE n = ?1")?
                .query_row([message], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let envelope = envelope.map(|e| unpack(&e)).transpose()?;
            let raw = raw.map(|r| unpack(&r)).transpose()?;
            Ok(convert::key_in(envelope.as_deref(), raw.as_deref()))
        })
        .await;
    let (Ok(Some(key)), Some(media_key)) = (key, copy.media_key.clone()) else {
        let error = format!("{why}, and jaw can't tell the phone which message it is");
        let _ = inner.store.apply(Lane::Live, vec![settle("expired", error)]).await;
        return false;
    };
    let ask = pb::MediaRetry { chat: key.chat, sender: key.sender, from_me: key.from_me, id: key.id, media_key };
    match engine.call(pb::command::Kind::MediaRetry(ask)).await {
        Ok(_) => {
            let at = now_ms();
            let asked = Step::AskedPhone { message: copy.message, variant: copy.variant, wanted: copy.wanted, at };
            let _ = inner.store.apply(Lane::Live, vec![Change::Media(asked)]).await;
            inner.asked_phone.notify_one();
            false
        }
        Err(e) if retryable(&e) => true,
        Err(EngineError::Failed { message, .. }) => {
            let error = format!("{why}, and jaw couldn't ask your phone for it: {message}");
            let _ = inner.store.apply(Lane::Live, vec![settle("expired", error)]).await;
            false
        }
        Err(_) => true,
    }
}

pub(super) fn start(inner: &Arc<Inner>) {
    tokio::spawn(phone_silent(inner.clone()));
}

/// Leaves each copy whose phone didn't answer in time expired, saying why, so none waits for good:
/// asked for again, it's tried again. Sleeps until the next one's time is up.
async fn phone_silent(inner: Arc<Inner>) {
    let wait = inner.config.phone_wait.as_millis() as i64;
    loop {
        let asked = inner.asked_phone.notified();
        let oldest = inner
            .store
            .reads()
            .query(Priority::Background, |db| {
                Ok(db
                    .prepare_cached(
                        "SELECT message, variant, retried_at FROM media WHERE state = 'waiting_for_phone'
                        ORDER BY retried_at LIMIT 64",
                    )?
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await;
        let oldest = match oldest {
            Ok(oldest) => oldest,
            Err(e) => {
                eprintln!("jawd: can't read which files wait for the phone: {e:#}");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        let now = now_ms();
        let silent: Vec<Change> = oldest
            .iter()
            .filter(|(_, _, at)| at + wait <= now)
            .map(|(message, variant, at)| {
                let variant = if variant == "hd" { "hd" } else { "sd" };
                Change::Media(Step::PhoneSilent { message: *message, variant, before: *at })
            })
            .collect();
        if !silent.is_empty() {
            if let Err(e) = inner.store.apply(Lane::Live, silent).await {
                eprintln!("jawd: can't say the phone didn't answer yet: {e:#}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            continue;
        }
        match oldest.first() {
            Some((_, _, at)) => {
                let until = Duration::from_millis((at + wait - now).max(0) as u64);
                tokio::select! {
                    () = tokio::time::sleep(until) => {}
                    () = asked => {}
                }
            }
            None => asked.await,
        }
    }
}
