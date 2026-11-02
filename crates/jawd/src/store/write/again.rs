//! Messages read again from their raw protobufs, by a backfill, once jaw reads their kind more
//! fully: the locations jaw.db held before a location was found by its address as well as its name,
//! the events it held before their place and call link were read and their answers counted, and the
//! voice messages it held before their length and waveform were in their rows.
//! Each is read as the converter reads one now, and its row takes a new version only if that
//! changes what it shows or what finds it, so a step taken twice, or after the converter has
//! written the row again itself, changes nothing. An event's answers and edits that an earlier
//! jawd kept sealed are opened with its secret, as they would be arriving now.

use anyhow::Result;
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::links::{EVENTS, PLACES, VOICES};
use super::{CHAT, MESSAGE, Stamps, apply_pending, kinds, unpack};
use crate::store::Change;

/// A message as its row keeps it, unless it's been deleted or edited since, which its raw protobuf
/// no longer says.
const KEPT: &str = "SELECT chat, kind, raw, text, spans, summary, detail FROM messages
    WHERE n = ?1 AND NOT deleted AND edited_at IS NULL";
const READ_AGAIN: &str = "UPDATE messages SET text = ?2, spans = ?3, summary = ?4, detail = ?5, v = ?6 WHERE n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";
const NAMED: &str = "SELECT m.chat, c.id, m.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";
const KEPT_FOR: &str = "SELECT count(*) FROM pending WHERE chat = ?1 AND target = ?2";

/// What a message shows and what finds it: its text, spans, summary as a row keeps it (none when
/// it's the text), and detail.
type Shown = (Option<String>, Option<String>, Option<String>, Option<String>);

/// Reads location `n` again, as `read_again` does; one whose reading fails, or panics, is kept in
/// quarantine while the step goes on, as the kinds backfill keeps a conversion that fails.
pub(super) fn places(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    kinds::guarded(db, stamps, PLACES, n, read_again)
}

/// Reads voice message `n` again, as `read_again` does, for its length and waveform; guarded as
/// `places` is.
pub(super) fn voices(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    kinds::guarded(db, stamps, VOICES, n, read_again)
}

/// Reads event `n` again, as `read_again` does, and opens what an earlier jawd kept sealed for it,
/// its answers and its edits; guarded as `places` is.
pub(super) fn events(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    kinds::guarded(db, stamps, EVENTS, n, event_again)
}

fn event_again(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let changed = read_again(db, stamps, n)?;
    let (chat, chat_jid, id): (i64, String, String) =
        db.prepare_cached(NAMED)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let kept = |db: &Connection| -> Result<i64> {
        Ok(db.prepare_cached(KEPT_FOR)?.query_row(params![chat_jid, id], |r| r.get(0))?)
    };
    let before = kept(db)?;
    apply_pending(db, stamps, chat, n, &chat_jid, &id, "event")?;
    Ok(changed || kept(db)? < before)
}

/// Reads message `n` again from its raw protobuf, as the converter reads one now, and writes what
/// that gives if it's of the kind the row has and isn't what the row shows. Says whether it changed
/// it.
fn read_again(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let Some((chat, kind, raw, text, spans, summary, detail)) = db
        .prepare_cached(KEPT)?
        .query_row([n], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .optional()?
    else {
        return Ok(false);
    };
    let Some(info) = raw.and_then(|raw| unpack(&raw).ok()).and_then(|raw| WebMessageInfo::decode(raw.as_slice()).ok())
    else {
        return Ok(false);
    };
    let read = kinds::converted(&info, &kinds::account(db)?)?.into_iter().find_map(|change| match change {
        Change::Message(message) if message.kind == kind => Some(message),
        _ => None,
    });
    let Some(read) = read else { return Ok(false) };
    let summary_kept = (read.text.as_ref() != Some(&read.summary)).then_some(read.summary);
    let now: Shown = (read.text, read.spans, summary_kept, read.detail);
    if now == (text, spans, summary, detail) {
        return Ok(false);
    }
    let seq = stamps.seq(MESSAGE, Some(n));
    db.prepare_cached(READ_AGAIN)?.execute(params![n, now.0, now.1, now.2, now.3, seq])?;
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
