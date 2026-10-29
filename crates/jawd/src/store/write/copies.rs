//! The backfill that gives the messages jaw.db held before media was kept the copy of the file each
//! brings: a photo, video, voice message, audio file, document or sticker stored by a jawd before
//! migration 20 takes the copy its raw protobuf converts to now, with nothing wanting it, as
//! history's copies are written, so it shows its state, a window showing it wants its thumbnail and
//! its file, and media.fetch fetches it; an HD copy kept as a message of its own is its SD
//! message's HD copy too. A message deleted, or one with its copy already, is left as it is, as is
//! one whose raw protobuf doesn't read; a message's raw protobuf is never rewritten. One an
//! interface names before the backfill reaches it takes its copy then (`media::Step::Copy`).

use std::panic::{AssertUnwindSafe, catch_unwind};

use anyhow::{Result, anyhow};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{Stamps, amend_message, kinds, media, record_quarantine, transient, unpack};
use crate::convert;
use crate::store::{Amendment, Change};

/// The backfill that gives the messages jaw.db held before media was kept their files' copies.
pub const COPIES: &str = "copies";
/// How long a step copies for past its first copy: none, so a step writes one copy at most. A copy
/// costs its transaction's commit more than its work (its message's page, its own row, its hash's
/// entry and its event), and the writer allows for a commit only once a transaction has taken 16
/// steps, so steps of a few copies each made transactions of 6 ms with their commits.
pub(super) const COPYING: std::time::Duration = std::time::Duration::ZERO;
/// How many messages a step looks at: fewer than other backfills, as a step looks again at those
/// past its copy, which a stretch of photos makes most of them.
pub(super) const STEP: i64 = 16;

/// The next messages, each with whether it brings a file it has no copy of: one whose detail says
/// what its file is, as the converter says it for every kind that brings one, that isn't deleted
/// and has no copy yet. It reads no raw protobuf, which a step would otherwise read whole for every
/// message it looks at.
pub(super) const NEXT_COPYLESS: &str = "SELECT n, NOT deleted AND instr(detail, '\"media\":') > 0
        AND NOT EXISTS (SELECT 1 FROM media WHERE message = messages.n AND variant = 'sd')
    FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const COPYLESS: &str = "SELECT chat, id, raw FROM messages WHERE n = ?1 AND NOT deleted";
const NAMED: &str = "SELECT c.id, m.id, m.raw FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";

/// Gives message `n` the copy of the file its raw protobuf brings, as it would be written arriving
/// now from history, saying whether it did. One whose conversion fails, or panics, is kept in
/// quarantine with its raw protobuf while the step goes on, so no message holds up the rest.
pub(super) fn backfilled(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let mark = stamps.mark();
    db.execute_batch("SAVEPOINT copies_message")?;
    match copied(db, stamps, n) {
        Ok(changed) => {
            db.execute_batch("RELEASE copies_message")?;
            Ok(changed)
        }
        Err(e) if transient(&e) => Err(e),
        Err(e) => {
            db.execute_batch("ROLLBACK TO copies_message; RELEASE copies_message")?;
            stamps.undo(mark);
            let (chat, id, raw): (String, String, Option<Vec<u8>>) =
                db.prepare_cached(NAMED)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            eprintln!("jawd: kept message {id} in {chat} in quarantine, as reading its file's copy failed: {e:#}");
            record_quarantine(db, COPIES, &format!("{chat} {id}"), &format!("{e:#}"), raw.as_deref())?;
            Ok(false)
        }
    }
}

fn copied(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let row: Option<(i64, String, Option<Vec<u8>>)> =
        db.prepare_cached(COPYLESS)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((chat, id, Some(raw))) = row else { return Ok(false) };
    let Some(info) = unpack(&raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(false);
    };
    let me = kinds::account(db)?;
    // The writer catches nothing, and every commit and ack waits on it.
    let changes = catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        assert_ne!(id, tests::PANICS, "a conversion that panics, as a test asks");
        convert::message(&info, &me, false)
    }))
    .map_err(|panic| {
        let why = panic.downcast_ref::<&str>().copied().or(panic.downcast_ref::<String>().map(String::as_str));
        anyhow!("converting it panicked: {}", why.unwrap_or("for no reason it gave"))
    })?;
    let mut changed = false;
    for change in changes {
        match change {
            Change::Message(message) if message.id == id => {
                if let Some(copy) = &message.media {
                    changed |= media::upsert(db, stamps, n, chat, "sd", copy, None)?;
                }
            }
            // An HD copy an earlier jawd kept as a message of its own: the row keeps showing that
            // file, and its SD message takes it as its HD copy, as it would arriving now, so the HD
            // copy arriving again adds nothing. Arriving now, one of yours would also delete such a
            // row for you; the backfill takes away nothing jaw shows.
            Change::Amend(amend) if matches!(amend.what, Amendment::Hd { .. }) => {
                if let Amendment::Hd { copy } = &amend.what {
                    changed |= media::upsert(db, stamps, n, chat, "sd", copy, None)?;
                }
                changed |= amend_message(db, stamps, &amend)?;
            }
            _ => {}
        }
    }
    #[cfg(test)]
    if id == tests::FILLS && !tests::FILLED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let full = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL);
        return Err(rusqlite::Error::SqliteFailure(full, Some("the disk filled, as a test asks".into())).into());
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use self::tests::{Before, before_media, hd_child};
