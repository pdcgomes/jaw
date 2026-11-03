//! Media out: each copy of a file of yours on its way to WhatsApp, a row in `outgoing` beside the
//! outbox row that sends it. `send.media` writes them with their messages (`outbox::Step::Attach`);
//! then each copy is made, from jaw's own copy of the file you picked, and put in the cache by its
//! content (`Prepared`), and uploaded (`Uploaded`), and each step fills in what its message and its
//! outbox row say of the file, so its outbox row sends the message as it is once it's ready. A step
//! is done only from the state before it, so one done again, or late, changes nothing twice. A copy
//! that can't be made or uploaded fails its outbox row (`outbox::Step::Unsent`). The copy of your
//! file goes once no copy needs it: its row's trigger lists it in `outgoing_drop`, and jawd removes
//! it after the synced commit that drops it from there (`Dropped`).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::media::{Keys, MediaCopy};
use super::{MESSAGE, Packed, Stamps};

/// The most bytes of files you picked that jaw keeps waiting for their copies to be made; past it,
/// `send.media` is refused until some have been.
pub const KEPT_MOST: u64 = 8_000_000_000;

/// A step for the writer.
#[derive(Clone, Debug)]
pub enum Step {
    Prepared(Box<Prepared>),
    Uploaded(Box<Uploaded>),
    /// Files of yours that nothing needs any more leave `outgoing_drop`; what's dropped is put in
    /// `dropped`, for jawd to remove once this commit is synced.
    Dropped {
        paths: Vec<String>,
        dropped: DroppedPaths,
    },
}

/// A copy made and in the cache, at `path` (relative to jaw's folder), `bytes` long, which
/// `outbox`'s row sends; `copy` is it as its message's media row keeps it, `raw` what the outbox row
/// sends now, and `shown` its message's raw protobuf and detail now, for a copy a row shows.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub outbox: i64,
    pub sha256: Vec<u8>,
    pub path: String,
    pub bytes: u64,
    pub copy: MediaCopy,
    pub raw: Vec<u8>,
    pub shown: Option<Shown>,
    pub at: i64,
}

/// A copy on WhatsApp's media server, with what its message names it by.
#[derive(Clone, Debug)]
pub struct Uploaded {
    pub outbox: i64,
    pub sha256: Vec<u8>,
    pub media_key: Vec<u8>,
    pub enc_sha256: Vec<u8>,
    pub direct_path: String,
    pub raw: Vec<u8>,
    pub shown: Option<Shown>,
}

/// A message's row as a step leaves it: its raw WebMessageInfo, compressed, and its detail.
#[derive(Clone, Debug)]
pub struct Shown {
    pub raw: Packed,
    pub detail: Option<String>,
}

/// Where a Dropped says what it dropped. The last attempt at its transaction is the one that stands.
#[derive(Clone, Debug, Default)]
pub struct DroppedPaths(Arc<Mutex<Vec<String>>>);

impl DroppedPaths {
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().expect("a drop's result"))
    }
}

/// A copy to make or upload, as `to_do` reads it.
#[derive(Clone, Debug)]
pub struct Copy {
    pub outbox: i64,
    /// Its message's n, an HD copy's being its SD message's; None once it's deleted for you.
    pub message: Option<i64>,
    pub variant: &'static str,
    /// What it goes as, or `voice` for a voice note, which goes as audio.
    pub kind: String,
    /// jaw's own copy of your file, while the copy is still to be made.
    pub source: Option<String>,
    /// `preparing` or `uploading`.
    pub state: String,
    pub sha256: Option<Vec<u8>>,
    /// The ID its outbox row sends it with, and what that row sends now.
    pub id: String,
    pub raw: Vec<u8>,
    /// Its message's ID, while it's there, and, for an SD copy, its raw protobuf.
    pub message_id: Option<String>,
    pub message_raw: Option<Vec<u8>>,
    /// Its file in the cache, once it's made.
    pub path: Option<String>,
}

impl Copy {
    /// Which of WhatsApp's keys it's encrypted with.
    pub fn keys(&self) -> Keys {
        match self.kind.as_str() {
            "image" => Keys::Image,
            "video" | "gif" => Keys::Video,
            "audio" | "voice" => Keys::Audio,
            _ => Keys::Document,
        }
    }
}

/// A voice message's copy goes as audio, and is planned as the voice note its outbox row sends.
const TO_DO: &str = "SELECT g.outbox, g.message, g.variant, iif(o.kind = 'voice', 'voice', g.kind), g.source, g.state,
        g.sha256, o.id, o.raw,
        m.id, iif(g.variant = 'sd', m.raw, NULL), f.path
    FROM outgoing g JOIN outbox o ON o.n = g.outbox LEFT JOIN messages m ON m.n = g.message
        LEFT JOIN media_files f ON f.sha256 = g.sha256
    WHERE g.state IN ('preparing', 'uploading') ORDER BY g.outbox LIMIT ?1";
const OF_MESSAGE: &str = "SELECT outbox, state FROM outgoing WHERE message = ?1";
const STATE: &str = "SELECT state FROM outgoing WHERE outbox = ?1";
const MESSAGE_IN: &str = "SELECT m.n FROM chats c JOIN messages m ON m.chat = c.n AND m.id = ?2 WHERE c.id = ?1";
const MESSAGE_ANYWHERE: &str = "SELECT m.n FROM chats c CROSS JOIN messages m ON m.chat = c.n AND m.id = ?1 LIMIT 2";
const NEEDED: &str = "SELECT EXISTS (SELECT 1 FROM outgoing WHERE source = ?1)";
const TO_DROP: &str = "SELECT path FROM outgoing_drop LIMIT ?1";
const KEPT: &str = "SELECT count(*), coalesce(sum(bytes), 0)
    FROM (SELECT source, max(bytes) AS bytes FROM outgoing WHERE source IS NOT NULL GROUP BY source)";
const COUNTS: &str = "SELECT count(*) FILTER (WHERE state = 'preparing'), count(*) FILTER (WHERE state = 'uploading'),
        count(*) FILTER (WHERE state = 'failed'), (SELECT count(*) FROM outgoing_drop)
    FROM outgoing";

const PREPARED: &str = "UPDATE outgoing SET state = 'uploading', sha256 = ?2, source = NULL, error = NULL
    WHERE outbox = ?1 AND state = 'preparing' RETURNING message, variant";
const FILED: &str = "INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (sha256) DO UPDATE SET path = coalesce(path, excluded.path),
        bytes = iif(path IS NULL, excluded.bytes, bytes), used_at = max(used_at, excluded.used_at)";
/// A copy of yours, wanted at once and as if asked for, so its thumbnail and previews are made ahead
/// of what nobody's looking at.
const COPIED: &str = "INSERT INTO media (message, variant, keys, mime, bytes, w, h, seconds, name, sha256, thumbnail,
        wanted, asked)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
    ON CONFLICT (message, variant) DO UPDATE SET keys = excluded.keys, mime = excluded.mime, bytes = excluded.bytes,
        w = excluded.w, h = excluded.h, seconds = excluded.seconds, name = coalesce(excluded.name, name),
        sha256 = coalesce(sha256, excluded.sha256), thumbnail = max(thumbnail, excluded.thumbnail),
        wanted = coalesce(wanted, excluded.wanted), asked = 1, error = NULL";
const UPLOADED: &str = "UPDATE outgoing SET state = 'ready', error = NULL
    WHERE outbox = ?1 AND state = 'uploading' AND sha256 = ?2 RETURNING message, variant";
const KEYED: &str = "UPDATE media SET media_key = coalesce(media_key, ?3), enc_sha256 = coalesce(enc_sha256, ?4),
        direct_path = coalesce(direct_path, ?5)
    WHERE message = ?1 AND variant = ?2";
const SENDS: &str = "UPDATE outbox SET raw = ?2 WHERE n = ?1";
const SHOWN: &str = "UPDATE messages SET raw = ?2, detail = ?3, v = ?4 WHERE n = ?1 RETURNING chat";
const DROPPED: &str = "DELETE FROM outgoing_drop WHERE path = ?1";

/// The copies to make or upload, oldest first, `most` at most.
pub fn to_do(db: &Connection, most: usize) -> Result<Vec<Copy>> {
    Ok(db
        .prepare_cached(TO_DO)?
        .query_map([most as i64], |r| {
            Ok(Copy {
                outbox: r.get(0)?,
                message: r.get(1)?,
                variant: if r.get::<_, String>(2)? == "hd" { "hd" } else { "sd" },
                kind: r.get(3)?,
                source: r.get(4)?,
                state: r.get(5)?,
                sha256: r.get(6)?,
                id: r.get(7)?,
                raw: r.get(8)?,
                message_id: r.get(9)?,
                message_raw: r.get(10)?,
                path: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// Message `n`'s copies on their way, by outbox row, with their state.
pub fn of_message(db: &Connection, n: i64) -> Result<Vec<(i64, String)>> {
    Ok(db
        .prepare_cached(OF_MESSAGE)?
        .query_map([n], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// The state of the copy each of outbox rows `outboxes` sends, for those still there.
pub fn states(db: &Connection, outboxes: &[i64]) -> Result<Vec<(i64, String)>> {
    let mut state = db.prepare_cached(STATE)?;
    let mut states = Vec::new();
    for &outbox in outboxes {
        if let Some(s) = state.query_row([outbox], |r| r.get(0)).optional()? {
            states.push((outbox, s));
        }
    }
    Ok(states)
}

/// Message `id`'s n, in `chat` or in any chat; None for one jawd doesn't have, or one in more chats
/// than one, which only media in looks for.
pub fn message_n(db: &Connection, id: &str, chat: Option<&str>) -> Result<Option<i64>> {
    let found: Vec<i64> = match chat {
        Some(chat) => db
            .prepare_cached(MESSAGE_IN)?
            .query_map(params![chat, id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
        None => db.prepare_cached(MESSAGE_ANYWHERE)?.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?,
    };
    Ok(match found.as_slice() {
        [n] => Some(*n),
        _ => None,
    })
}

/// Whether a copy still to be made needs jaw's copy of your file at `path`.
pub fn needed(db: &Connection, path: &str) -> Result<bool> {
    Ok(db.prepare_cached(NEEDED)?.query_row([path], |r| r.get(0))?)
}

/// Some of the copies of your files that nothing needs any more.
pub fn to_drop(db: &Connection, most: usize) -> Result<Vec<String>> {
    Ok(db.prepare_cached(TO_DROP)?.query_map([most as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// The files you picked that jaw keeps until their copies are made, and their bytes.
pub fn kept(db: &Connection) -> Result<(u64, u64)> {
    let (files, bytes): (i64, i64) = db.prepare_cached(KEPT)?.query_row([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok((files.max(0) as u64, bytes.max(0) as u64))
}

/// What `jawd status` says of media out: the copies on their way by state, and the files of yours
/// kept for them, their bytes, and those nothing needs any more.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    pub preparing: u64,
    pub uploading: u64,
    pub unsent: u64,
    pub kept_files: u64,
    pub kept_bytes: u64,
    pub unneeded: u64,
}

pub fn counts(db: &Connection) -> Result<Counts> {
    let [preparing, uploading, unsent, unneeded] = db
        .prepare_cached(COUNTS)?
        .query_row([], |r| Ok([r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))?
        .map(|n| n.max(0) as u64);
    let (kept_files, kept_bytes) = kept(db)?;
    Ok(Counts { preparing, uploading, unsent, kept_files, kept_bytes, unneeded })
}

pub(super) fn provenance(step: &Step) -> (&'static str, String, Option<&[u8]>) {
    match step {
        Step::Prepared(p) => ("outgoing", format!("prepared {}", p.outbox), Some(p.raw.as_slice())),
        Step::Uploaded(u) => ("outgoing", format!("uploaded {}", u.outbox), Some(u.raw.as_slice())),
        Step::Dropped { paths, .. } => ("outgoing", format!("dropped {} files", paths.len()), None),
    }
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, step: &Step) -> Result<bool> {
    match step {
        Step::Prepared(p) => prepared(db, stamps, p),
        Step::Uploaded(u) => uploaded(db, stamps, u),
        Step::Dropped { paths, dropped } => {
            let mut gone = Vec::new();
            for path in paths {
                if db.prepare_cached(DROPPED)?.execute([path])? > 0 {
                    gone.push(path.clone());
                }
            }
            *dropped.0.lock().expect("a drop's result") = gone;
            Ok(false)
        }
    }
}

/// A copy made: it's in the cache, its message's media row shows it, wanted so its previews are
/// made, and its message and outbox row say what it is. Once a copy is made, jaw's copy of your file
/// isn't needed for it any more. One cancelled or taken while it was made, as a deletion takes an
/// HD copy's rows, is in the cache all the same: its file's row lists it to go, as nothing names it.
/// One whose message was deleted for everyone while it was made goes as its outbox row sends it,
/// and its message's row, which the deletion took what it said from, shows nothing of it.
fn prepared(db: &Connection, stamps: &mut Stamps, p: &Prepared) -> Result<bool> {
    let made: Option<(Option<i64>, String)> = db
        .prepare_cached(PREPARED)?
        .query_row(params![p.outbox, p.sha256], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    db.prepare_cached(FILED)?.execute(params![p.sha256, p.path, p.bytes as i64, p.at])?;
    let Some((message, variant)) = made else { return Ok(false) };
    db.prepare_cached(SENDS)?.execute(params![p.outbox, p.raw])?;
    let Some(n) = message else { return Ok(true) };
    if db.prepare_cached(super::outbox::REVOKED)?.exists([n])? {
        return Ok(true);
    }
    let copy = &p.copy;
    let wanted = (variant == "sd").then_some(p.at);
    db.prepare_cached(COPIED)?.execute(params![
        n,
        variant,
        copy.keys.as_str(),
        copy.mime,
        p.bytes as i64,
        copy.w,
        copy.h,
        copy.seconds,
        copy.name,
        p.sha256,
        copy.thumbnail,
        wanted,
    ])?;
    show(db, stamps, n, p.shown.as_ref())?;
    Ok(true)
}

/// A copy uploaded: its message's media row has what WhatsApp names it by, and its message and
/// outbox row say where it is, so it goes. One whose message was deleted for everyone while it was
/// uploaded goes as its outbox row sends it, as one made then does.
fn uploaded(db: &Connection, stamps: &mut Stamps, u: &Uploaded) -> Result<bool> {
    let done: Option<(Option<i64>, String)> = db
        .prepare_cached(UPLOADED)?
        .query_row(params![u.outbox, u.sha256], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((message, variant)) = done else { return Ok(false) };
    db.prepare_cached(SENDS)?.execute(params![u.outbox, u.raw])?;
    let Some(n) = message else { return Ok(true) };
    if db.prepare_cached(super::outbox::REVOKED)?.exists([n])? {
        return Ok(true);
    }
    db.prepare_cached(KEYED)?.execute(params![n, variant, u.media_key, u.enc_sha256, u.direct_path])?;
    show(db, stamps, n, u.shown.as_ref())?;
    Ok(true)
}

/// Message `n`'s row as `shown` says, or just with a new version, as its media changed.
fn show(db: &Connection, stamps: &mut Stamps, n: i64, shown: Option<&Shown>) -> Result<()> {
    let seq = stamps.seq(MESSAGE, Some(n));
    let chat: i64 = match shown {
        Some(shown) => db.prepare_cached(SHOWN)?.query_row(params![n, shown.raw.0, shown.detail, seq], |r| r.get(0))?,
        None => db
            .prepare_cached("UPDATE messages SET v = ?2 WHERE n = ?1 RETURNING chat")?
            .query_row(params![n, seq], |r| r.get(0))?,
    };
    super::outbox::touched(db, stamps, chat, n, seq)
}
