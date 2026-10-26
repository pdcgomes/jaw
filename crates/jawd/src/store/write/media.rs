//! Media: each copy of a message's file, and the cache's files by the SHA-256 of their content. A
//! copy is written with its message (`upsert`), or with the message it's the HD copy of (an
//! amendment, `attach`); what jawd then does with it (downloading it, its previews and thumbnail,
//! asking the phone to upload it again) comes as steps. A file in the cache is named by its hash,
//! so every copy with that hash shows it, and a step that changes what a copy shows gives each
//! message showing it a new version. A file's row goes only in a synced commit, and jawd removes
//! the file after it (`Drop`), so a row never names a file that isn't there.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{MESSAGE, Stamps, chat_jid, located};

/// Which of WhatsApp's keys a file is encrypted with; a sticker's are a photo's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keys {
    Image,
    Video,
    Audio,
    #[default]
    Document,
}

impl Keys {
    pub fn as_str(self) -> &'static str {
        match self {
            Keys::Image => "image",
            Keys::Video => "video",
            Keys::Audio => "audio",
            Keys::Document => "document",
        }
    }

    pub fn from_str(keys: &str) -> Keys {
        match keys {
            "image" => Keys::Image,
            "video" => Keys::Video,
            "audio" => Keys::Audio,
            _ => Keys::Document,
        }
    }
}

/// A copy of a message's file as WhatsApp describes it: what it is, and the keys to download it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaCopy {
    pub keys: Keys,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "hex")]
    pub sha256: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "hex")]
    pub enc_sha256: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "hex")]
    pub media_key: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_path: Option<String>,
    /// Whether the message carries a thumbnail to draw until the file comes.
    #[serde(default)]
    pub thumbnail: bool,
}

/// Bytes as lowercase hex in JSON, as an amendment waiting in `pending` keeps them.
mod hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => s.serialize_str(&super::to_hex(bytes)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|text| super::from_hex(&text).ok_or_else(|| serde::de::Error::custom("bytes as hex")))
            .transpose()
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect()
}

/// Which of a file's previews, and its thumbnail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    /// The 256 and 768 pixel previews, made and forgotten together.
    Previews,
    Thumbnail,
}

/// What the phone answered when asked to upload a file again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Uploaded again, to this direct path.
    Uploaded(String),
    /// It no longer has the file.
    NotFound,
    /// It couldn't, or jawd couldn't read its answer; asking again may work.
    Failed(String),
}

/// A step for the writer. A copy is named by its message's n and its variant (`sd` or `hd`), a
/// file by its hash.
#[derive(Clone, Debug)]
pub enum Step {
    /// A copy jawd was given to do is downloading, from nothing yet.
    Downloading { message: i64, variant: &'static str },
    /// How far the download of file `sha256` has got, from 0 to 1, for every copy downloading it.
    Progress { sha256: Vec<u8>, progress: f64 },
    /// A copy's download failed in a way that may pass, as `error` says; it stays wanted.
    Later { message: i64, variant: &'static str, error: String },
    /// The phone, asked for a copy at or before `before`, didn't answer in time.
    PhoneSilent { message: i64, variant: &'static str, before: i64 },
    /// A file is whole and synced in the cache at `path` (relative to jaw's folder), `bytes` long;
    /// every copy with its hash now shows it, and none of them is waiting for anything.
    Placed { sha256: Vec<u8>, path: String, bytes: u64, at: i64 },
    /// A file's previews, made and in place, `bytes` on disk for both.
    Previews { sha256: Vec<u8>, small: String, large: String, bytes: u64, at: i64 },
    /// A file's message's thumbnail, drawn as its instant preview, in place.
    Thumbnail { sha256: Vec<u8>, path: String, bytes: u64, at: i64 },
    /// Nothing more is to be done for a copy now: it's as `state` says (`remote`, `expired` or
    /// `gone`), with `error` saying why, if it's one jawd took when it was `wanted` then; a copy
    /// wanted again since keeps being wanted.
    Settled { message: i64, variant: &'static str, wanted: i64, state: &'static str, error: Option<String> },
    /// The phone was asked to upload a copy again at `at`; it waits for the answer.
    AskedPhone { message: i64, variant: &'static str, wanted: i64, at: i64 },
    /// The phone's answer, by the message's chat and ID as WhatsApp named them; an upload is wanted
    /// again at `at`.
    Answered { chat: String, id: String, answer: Answer, at: i64 },
    /// jawd is given a copy to do at `at`: an interface asked for it (`asked`), or showed it.
    Want { message: i64, variant: &'static str, at: i64, asked: bool },
    /// A copy's download is cancelled: nothing more is done for it.
    Cancel { message: i64, variant: &'static str },
    /// A file was placed, asked for, saved or shown at `at`.
    Used { sha256: Vec<u8>, at: i64 },
    /// A file's previews or thumbnail, found broken where `path` says, are forgotten, so they can be
    /// removed and made again.
    Forget { sha256: Vec<u8>, made: Made, path: String },
    /// Files leave the cache: those no copy names, or, with `evict`, also those copies name, unless a
    /// starred message or one waiting to be sent does. What's dropped is put in `dropped`, with its
    /// paths, for jawd to remove once this commit is synced.
    Drop { sha256s: Vec<Vec<u8>>, evict: bool, dropped: Dropped },
    /// jawd is starting: copies a stopped jawd left downloading aren't any more, and those it left
    /// waiting for the phone, whose answer the engine started now can't read, are wanted again.
    Restarted { at: i64 },
}

/// Where a Drop says what it dropped. The last attempt at its transaction is the one that stands.
#[derive(Clone, Debug, Default)]
pub struct Dropped(Arc<Mutex<Vec<DroppedFile>>>);

impl Dropped {
    pub fn take(&self) -> Vec<DroppedFile> {
        std::mem::take(&mut *self.0.lock().expect("a drop's result"))
    }
}

/// A file dropped from the cache, and the paths of everything made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedFile {
    pub sha256: Vec<u8>,
    pub paths: Vec<String>,
}

const UPSERT: &str = "INSERT INTO media (message, variant, keys, mime, bytes, w, h, seconds, name, sha256, enc_sha256,
        media_key, direct_path, thumbnail, wanted)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
    ON CONFLICT (message, variant) DO UPDATE SET keys = excluded.keys, mime = coalesce(excluded.mime, mime),
        bytes = coalesce(excluded.bytes, bytes), w = coalesce(excluded.w, w), h = coalesce(excluded.h, h),
        seconds = coalesce(excluded.seconds, seconds), name = coalesce(excluded.name, name),
        sha256 = coalesce(sha256, excluded.sha256), enc_sha256 = coalesce(enc_sha256, excluded.enc_sha256),
        media_key = coalesce(media_key, excluded.media_key), direct_path = coalesce(direct_path, excluded.direct_path),
        thumbnail = max(thumbnail, excluded.thumbnail), wanted = coalesce(wanted, excluded.wanted)
    WHERE keys IS NOT excluded.keys OR coalesce(excluded.mime, mime) IS NOT mime
        OR coalesce(excluded.bytes, bytes) IS NOT bytes OR coalesce(excluded.w, w) IS NOT w
        OR coalesce(excluded.h, h) IS NOT h OR coalesce(excluded.seconds, seconds) IS NOT seconds
        OR coalesce(excluded.name, name) IS NOT name OR coalesce(sha256, excluded.sha256) IS NOT sha256
        OR coalesce(enc_sha256, excluded.enc_sha256) IS NOT enc_sha256
        OR coalesce(media_key, excluded.media_key) IS NOT media_key
        OR coalesce(direct_path, excluded.direct_path) IS NOT direct_path
        OR max(thumbnail, excluded.thumbnail) IS NOT thumbnail OR coalesce(wanted, excluded.wanted) IS NOT wanted";
const SHOWN_BY: &str =
    "SELECT md.message, m.chat FROM media md JOIN messages m ON m.n = md.message WHERE md.sha256 = ?1";
const CHAT_OF: &str = "SELECT chat FROM messages WHERE n = ?1";
const NEW_VERSION: &str = "UPDATE messages SET v = ?2 WHERE n = ?1";
const DOWNLOADING: &str = "UPDATE media SET state = 'downloading', progress = 0, error = NULL
    WHERE message = ?1 AND variant = ?2 AND wanted IS NOT NULL AND state IS NOT 'downloading'";
const PROGRESS: &str = "UPDATE media SET progress = ?2
    WHERE sha256 = ?1 AND state = 'downloading' AND progress IS NOT ?2 RETURNING message";
const LATER: &str = "UPDATE media SET state = iif(state = 'downloading', 'remote', state), progress = NULL, error = ?3
    WHERE message = ?1 AND variant = ?2 AND wanted IS NOT NULL";
const PHONE_SILENT: &str = "UPDATE media SET state = 'expired', error = ?4
    WHERE message = ?1 AND variant = ?2 AND state = 'waiting_for_phone' AND retried_at <= ?3";
const PLACED: &str = "INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (sha256) DO UPDATE SET path = excluded.path, bytes = excluded.bytes,
        used_at = max(used_at, excluded.used_at)
    WHERE path IS NULL";
const READY: &str = "UPDATE media SET state = 'remote', progress = NULL, error = NULL
    WHERE sha256 = ?1 AND (state IS NOT 'remote' OR progress IS NOT NULL OR error IS NOT NULL)";
const PREVIEWS: &str = "UPDATE media_files SET small = ?2, large = ?3, previews_bytes = ?4,
        used_at = max(used_at, ?5)
    WHERE sha256 = ?1 AND small IS NULL AND large IS NULL";
const THUMBNAIL: &str = "INSERT INTO media_files (sha256, thumbnail, thumbnail_bytes, used_at) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (sha256) DO UPDATE SET thumbnail = excluded.thumbnail, thumbnail_bytes = excluded.thumbnail_bytes
    WHERE thumbnail IS NULL";
const SETTLED: &str = "UPDATE media SET wanted = NULL, asked = 0, state = ?4, progress = NULL,
        error = iif(?5 IS NULL AND state = ?4, error, ?5)
    WHERE message = ?1 AND variant = ?2 AND wanted IS ?3";
const ASKED_PHONE: &str = "UPDATE media SET wanted = NULL, asked = 0, state = 'waiting_for_phone', retried_at = ?4,
        progress = NULL, error = NULL
    WHERE message = ?1 AND variant = ?2 AND wanted IS ?3";
const UPLOADED: &str =
    "UPDATE media SET direct_path = ?2, state = 'remote', error = NULL, wanted = coalesce(wanted, ?3),
        asked = 1
    WHERE message = ?1 AND variant = 'sd'
        AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.sha256 = media.sha256 AND f.path IS NOT NULL)";
const NOT_UPLOADED: &str = "UPDATE media SET state = ?2, error = ?3
    WHERE message = ?1 AND variant = 'sd' AND state = 'waiting_for_phone'";
const WANT: &str = "UPDATE media SET wanted = iif(wanted IS NULL OR ?4 > asked, ?3, wanted), asked = max(asked, ?4),
        error = iif(?4, NULL, error), cancelled = iif(?4, 0, cancelled)
    WHERE message = ?1 AND variant = ?2 AND (wanted IS NULL OR ?4 > asked)";
const CANCEL: &str = "UPDATE media SET wanted = NULL, asked = 0, progress = NULL, cancelled = 1,
        state = iif(state = 'downloading', 'remote', state)
    WHERE message = ?1 AND variant = ?2 AND (wanted IS NOT NULL OR state = 'downloading' OR cancelled = 0)";
const USED: &str = "UPDATE media_files SET used_at = ?2 WHERE sha256 = ?1 AND used_at < ?2";
const FORGET_PREVIEWS: &str = "UPDATE media_files SET small = NULL, large = NULL, previews_bytes = NULL WHERE sha256 = ?1 AND ?2 IN (small, large)";
const FORGET_THUMBNAIL: &str =
    "UPDATE media_files SET thumbnail = NULL, thumbnail_bytes = NULL WHERE sha256 = ?1 AND thumbnail = ?2";
const NAMED: &str = "SELECT EXISTS (SELECT 1 FROM media WHERE sha256 = ?1)";
const KEPT: &str = "SELECT EXISTS (SELECT 1 FROM media md JOIN messages m ON m.n = md.message
    WHERE md.sha256 = ?1 AND (m.starred OR EXISTS (SELECT 1 FROM outbox o WHERE o.message = m.n)))";
const FILE_PATHS: &str = "SELECT path, small, large, thumbnail FROM media_files WHERE sha256 = ?1";
const DROP_FILE: &str = "DELETE FROM media_files WHERE sha256 = ?1";
const DROPPED: &str = "DELETE FROM media_drop WHERE sha256 = ?1";
const LEFT_DOWNLOADING: &str =
    "SELECT message, variant FROM media WHERE state = 'downloading' AND wanted IS NOT NULL LIMIT 1000";
const NOT_DOWNLOADING: &str =
    "UPDATE media SET state = 'remote', progress = NULL WHERE message = ?1 AND variant = ?2 AND state = 'downloading'";
const LEFT_WAITING: &str = "SELECT message, variant FROM media WHERE state = 'waiting_for_phone' LIMIT 1000";
const WANTED_AGAIN: &str = "UPDATE media SET state = 'remote', wanted = coalesce(wanted, ?3), asked = 1
    WHERE message = ?1 AND variant = ?2 AND state = 'waiting_for_phone'";
const REVOKED: &str = "DELETE FROM media WHERE message = ?1";
const FOLD: &str = "INSERT INTO media (message, variant, keys, mime, bytes, w, h, seconds, name, sha256, enc_sha256,
        media_key, direct_path, thumbnail, state, progress, error, wanted, asked, retried_at, cancelled)
    SELECT ?1, variant, keys, mime, bytes, w, h, seconds, name, sha256, enc_sha256, media_key, direct_path, thumbnail,
        iif(state = 'downloading', 'remote', state), NULL, error, wanted, asked, retried_at, cancelled
    FROM media WHERE message = ?2
    ON CONFLICT (message, variant) DO UPDATE SET mime = coalesce(mime, excluded.mime),
        bytes = coalesce(bytes, excluded.bytes), w = coalesce(w, excluded.w), h = coalesce(h, excluded.h),
        seconds = coalesce(seconds, excluded.seconds), name = coalesce(name, excluded.name),
        sha256 = coalesce(sha256, excluded.sha256), enc_sha256 = coalesce(enc_sha256, excluded.enc_sha256),
        media_key = coalesce(media_key, excluded.media_key), direct_path = coalesce(direct_path, excluded.direct_path),
        thumbnail = max(thumbnail, excluded.thumbnail),
        wanted = coalesce(min(wanted, excluded.wanted), wanted, excluded.wanted), asked = max(asked, excluded.asked),
        cancelled = max(cancelled, excluded.cancelled)";

/// Gives message `n` a new version, as something it shows changed.
fn shown(db: &Connection, stamps: &mut Stamps, n: i64, chat: i64) -> Result<()> {
    let seq = stamps.seq(MESSAGE, Some(n));
    db.prepare_cached(NEW_VERSION)?.execute(params![n, seq])?;
    stamps.record(db, MESSAGE, n, Some(chat), seq)
}

/// Gives message `n` a new version, finding its chat.
fn shown_n(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<()> {
    let chat: Option<i64> = db.prepare_cached(CHAT_OF)?.query_row([n], |r| r.get(0)).optional()?;
    match chat {
        Some(chat) => shown(db, stamps, n, chat),
        None => Ok(()),
    }
}

/// Gives every message with a copy of file `sha256` a new version.
fn shown_by(db: &Connection, stamps: &mut Stamps, sha256: &[u8]) -> Result<()> {
    let shown_by: Vec<(i64, i64)> = db
        .prepare_cached(SHOWN_BY)?
        .query_map([sha256], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (n, chat) in shown_by {
        shown(db, stamps, n, chat)?;
    }
    Ok(())
}

/// Writes copy `variant` of message `n`, in chat `chat`, by the merge rules: what describes it
/// fills in what was missing, its hash and keys once known stay, a direct path the phone gave
/// stays over a copy's, and it's `wanted` as the first copy that wanted it said. Says whether
/// anything changed, and gives the message a new version if it did.
pub(super) fn upsert(
    db: &Connection,
    stamps: &mut Stamps,
    n: i64,
    chat: i64,
    variant: &str,
    copy: &MediaCopy,
    wanted: Option<i64>,
) -> Result<bool> {
    let changed = db.prepare_cached(UPSERT)?.execute(params![
        n,
        variant,
        copy.keys.as_str(),
        copy.mime,
        copy.bytes.map(|b| b as i64),
        copy.w,
        copy.h,
        copy.seconds,
        copy.name,
        copy.sha256.as_deref().filter(|s| s.len() == 32),
        copy.enc_sha256.as_deref().filter(|s| s.len() == 32),
        copy.media_key.as_deref().filter(|s| s.len() == 32),
        copy.direct_path,
        copy.thumbnail,
        wanted,
    ])? > 0;
    if changed {
        shown(db, stamps, n, chat)?;
    }
    Ok(changed)
}

/// Message `n` was deleted for everyone: its copies go, and their files once no copy names them.
pub(super) fn revoked(db: &Connection, n: i64) -> Result<()> {
    db.prepare_cached(REVOKED)?.execute([n])?;
    Ok(())
}

/// Message `gone`, the same as `kept`, is folded into it: its copies fill in what `kept`'s lack,
/// and a download one of them was given goes on for `kept`; `gone`'s go with it.
pub(super) fn fold(db: &Connection, kept: i64, gone: i64) -> Result<()> {
    db.prepare_cached(FOLD)?.execute([kept, gone])?;
    Ok(())
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, step: &Step) -> Result<bool> {
    match step {
        Step::Downloading { message, variant } => one(db, stamps, *message, DOWNLOADING, params![message, variant]),
        Step::Progress { sha256, progress } => {
            let moved: Vec<i64> = db
                .prepare_cached(PROGRESS)?
                .query_map(params![sha256, progress.clamp(0.0, 1.0)], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for &n in &moved {
                shown_n(db, stamps, n)?;
            }
            Ok(!moved.is_empty())
        }
        Step::Later { message, variant, error } => one(db, stamps, *message, LATER, params![message, variant, error]),
        Step::PhoneSilent { message, variant, before } => {
            let why = "your phone didn't answer when jaw asked it to send this file again; it needs to be online \
                and have the file, and jaw asks again when you open it";
            one(db, stamps, *message, PHONE_SILENT, params![message, variant, before, why])
        }
        Step::Placed { sha256, path, bytes, at } => {
            let placed = db.prepare_cached(PLACED)?.execute(params![sha256, path, *bytes as i64, at])? > 0;
            let ready = db.prepare_cached(READY)?.execute([sha256])? > 0;
            if placed || ready {
                shown_by(db, stamps, sha256)?;
            }
            Ok(placed || ready)
        }
        Step::Previews { sha256, small, large, bytes, at } => {
            let made = db.prepare_cached(PREVIEWS)?.execute(params![sha256, small, large, *bytes as i64, at])? > 0;
            if made {
                shown_by(db, stamps, sha256)?;
            }
            Ok(made)
        }
        Step::Thumbnail { sha256, path, bytes, at } => {
            let made = db.prepare_cached(THUMBNAIL)?.execute(params![sha256, path, *bytes as i64, at])? > 0;
            if made {
                shown_by(db, stamps, sha256)?;
            }
            Ok(made)
        }
        Step::Settled { message, variant, wanted, state, error } => {
            one(db, stamps, *message, SETTLED, params![message, variant, wanted, state, error])
        }
        Step::AskedPhone { message, variant, wanted, at } => {
            one(db, stamps, *message, ASKED_PHONE, params![message, variant, wanted, at])
        }
        Step::Answered { chat, id, answer, at } => {
            let Some((_, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(false) };
            match answer {
                Answer::Uploaded(path) => one(db, stamps, n, UPLOADED, params![n, path, at]),
                Answer::NotFound => {
                    let why = "your phone no longer has this file, so it can't send it again";
                    one(db, stamps, n, NOT_UPLOADED, params![n, "gone", why])
                }
                Answer::Failed(why) => one(db, stamps, n, NOT_UPLOADED, params![n, "expired", why]),
            }
        }
        Step::Want { message, variant, at, asked } => {
            one(db, stamps, *message, WANT, params![message, variant, at, asked])
        }
        Step::Cancel { message, variant } => one(db, stamps, *message, CANCEL, params![message, variant]),
        Step::Used { sha256, at } => {
            db.prepare_cached(USED)?.execute(params![sha256, at])?;
            Ok(false)
        }
        Step::Forget { sha256, made, path } => {
            let sql = match made {
                Made::Previews => FORGET_PREVIEWS,
                Made::Thumbnail => FORGET_THUMBNAIL,
            };
            let forgot = db.prepare_cached(sql)?.execute(params![sha256, path])? > 0;
            if forgot {
                shown_by(db, stamps, sha256)?;
            }
            Ok(forgot)
        }
        Step::Drop { sha256s, evict, dropped } => {
            let mut files = Vec::new();
            for sha256 in sha256s {
                let named: bool = db.prepare_cached(NAMED)?.query_row([sha256], |r| r.get(0))?;
                db.prepare_cached(DROPPED)?.execute([sha256])?;
                if named && (!evict || db.prepare_cached(KEPT)?.query_row([sha256], |r| r.get::<_, bool>(0))?) {
                    continue;
                }
                let paths: Option<[Option<String>; 4]> = db
                    .prepare_cached(FILE_PATHS)?
                    .query_row([sha256], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))
                    .optional()?;
                let Some(paths) = paths else { continue };
                db.prepare_cached(DROP_FILE)?.execute([sha256])?;
                shown_by(db, stamps, sha256)?;
                files.push(DroppedFile { sha256: sha256.clone(), paths: paths.into_iter().flatten().collect() });
            }
            let any = !files.is_empty();
            *dropped.0.lock().expect("a drop's result") = files;
            Ok(any)
        }
        Step::Restarted { at } => {
            let mut changed = false;
            let left: Vec<(i64, String)> = db
                .prepare_cached(LEFT_DOWNLOADING)?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (n, variant) in left {
                changed |= one(db, stamps, n, NOT_DOWNLOADING, params![n, variant])?;
            }
            let waiting: Vec<(i64, String)> = db
                .prepare_cached(LEFT_WAITING)?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (n, variant) in waiting {
                changed |= one(db, stamps, n, WANTED_AGAIN, params![n, variant, at])?;
            }
            Ok(changed)
        }
    }
}

/// Runs `sql` on one copy of message `n`, giving the message a new version if it changed anything.
fn one(db: &Connection, stamps: &mut Stamps, n: i64, sql: &str, params: impl rusqlite::Params) -> Result<bool> {
    let changed = db.prepare_cached(sql)?.execute(params)? > 0;
    if changed {
        shown_n(db, stamps, n)?;
    }
    Ok(changed)
}

pub(super) fn provenance(step: &Step) -> (&'static str, String, Option<&[u8]>) {
    let what = match step {
        Step::Downloading { message, variant } => format!("downloading {message} {variant}"),
        Step::Progress { sha256, .. } => format!("progress {}", to_hex(sha256)),
        Step::Later { message, variant, .. } => format!("later {message} {variant}"),
        Step::PhoneSilent { message, variant, .. } => format!("phone silent {message} {variant}"),
        Step::Placed { sha256, .. } => format!("placed {}", to_hex(sha256)),
        Step::Previews { sha256, .. } => format!("previews {}", to_hex(sha256)),
        Step::Thumbnail { sha256, .. } => format!("thumbnail {}", to_hex(sha256)),
        Step::Settled { message, variant, .. } => format!("settled {message} {variant}"),
        Step::AskedPhone { message, variant, .. } => format!("asked the phone {message} {variant}"),
        Step::Answered { chat, id, .. } => format!("answered {chat} {id}"),
        Step::Want { message, variant, .. } => format!("want {message} {variant}"),
        Step::Cancel { message, variant } => format!("cancel {message} {variant}"),
        Step::Used { sha256, .. } => format!("used {}", to_hex(sha256)),
        Step::Forget { sha256, made, .. } => format!("forget {made:?} {}", to_hex(sha256)),
        Step::Drop { sha256s, .. } => format!("drop {} files", sha256s.len()),
        Step::Restarted { .. } => "restarted".into(),
    };
    ("media", what, None)
}
