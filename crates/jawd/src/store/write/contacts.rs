//! The backfill that reads the contact cards jaw.db holds again from their raw protobufs, now that
//! a message of several cards shows them as one card's does, `contact`'s name and vCard, and a card
//! its message gives no name takes the one its vCard gives. Each message of kind contact takes the
//! text, spans, summary and detail its raw protobuf converts to now, at a new version with its
//! event, where that changes any of them; the rest of its row (its quote, its timer, its status)
//! stays as it is, as an amendment may have moved it on since. What a card with a time to go said
//! before goes as an erasure takes it (`expiry::unsaid`). One whose raw protobuf doesn't read,
//! or no longer converts to a card, is left as it is, and one whose conversion fails is kept in
//! quarantine with its raw protobuf while the step goes on, so no card holds up the rest.

use std::panic::{AssertUnwindSafe, catch_unwind};

use anyhow::{Result, anyhow};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{CHAT, MESSAGE, Stamps, expiry, record_quarantine, transient, unpack};
use crate::convert::{self, Me};
use crate::store::Change;

/// The backfill that reads the contact cards again.
pub const CONTACTS: &str = "contacts";

/// The next messages, each with whether it's a card still there.
pub(super) const NEXT_CARDS: &str =
    "SELECT n, kind = 'contact' AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const CARD: &str = "SELECT m.chat, c.id, m.id, m.raw FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";
const ME: &str = "SELECT value FROM sync_state WHERE key = 'me'";
/// A card as it converts now, where that changes what it shows or what finds it; its summary is
/// kept apart from its text only where the two differ, as every message's is.
const SHOWN: &str = "UPDATE messages SET text = ?2, spans = ?3, summary = iif(?4 IS ?2, NULL, ?4), detail = ?5, v = ?6
    WHERE n = ?1 AND kind = 'contact' AND NOT deleted
        AND (text IS NOT ?2 OR spans IS NOT ?3 OR summary IS NOT iif(?4 IS ?2, NULL, ?4) OR detail IS NOT ?5)";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// Reads card `n` again; one whose conversion fails, or panics, is kept in quarantine.
pub(super) fn backfilled(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let mark = stamps.mark();
    db.execute_batch("SAVEPOINT contacts_card")?;
    match read_again(db, stamps, n) {
        Ok(changed) => {
            db.execute_batch("RELEASE contacts_card")?;
            Ok(changed)
        }
        Err(e) if transient(&e) => Err(e),
        Err(e) => {
            db.execute_batch("ROLLBACK TO contacts_card; RELEASE contacts_card")?;
            stamps.undo(mark);
            let (chat, id, raw): (String, String, Option<Vec<u8>>) =
                db.prepare_cached(CARD)?.query_row([n], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?)))?;
            eprintln!("jawd: kept card {id} in {chat} in quarantine, as reading it again failed: {e:#}");
            record_quarantine(db, CONTACTS, &format!("{chat} {id}"), &format!("{e:#}"), raw.as_deref())?;
            Ok(false)
        }
    }
}

fn read_again(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    type Card = (i64, String, String, Option<Vec<u8>>);
    let card: Option<Card> =
        db.prepare_cached(CARD)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
    let Some((chat, _, id, Some(raw))) = card else { return Ok(false) };
    let Some(info) = unpack(&raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(false);
    };
    let me: Option<String> = db.prepare_cached(ME)?.query_row([], |r| r.get(0)).optional()?;
    let me = Me::new(me.unwrap_or_default().split_whitespace().map(str::to_string));
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
    let card = changes.into_iter().find_map(|change| match change {
        Change::Message(message) if message.id == id && message.kind == "contact" => Some(message),
        _ => None,
    });
    let Some(card) = card else { return Ok(false) };
    let seq = stamps.seq(MESSAGE, Some(n));
    let shown = || -> Result<usize> {
        Ok(db.prepare_cached(SHOWN)?.execute(params![n, card.text, card.spans, card.summary, card.detail, seq])?)
    };
    // What one with a time to go said before goes as an erasure takes it.
    let changed = if expiry::erasable(db, n)? {
        let mut changed = 0;
        expiry::unsaid(db, stamps, &[n], |_| {
            changed = shown()?;
            Ok(changed > 0)
        })?;
        changed
    } else {
        shown()?
    };
    #[cfg(test)]
    anyhow::ensure!(id != tests::FAILS, "a conversion that fails once it has written, as a test asks");
    #[cfg(test)]
    if id == tests::FILLS && !tests::FILLED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let full = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL);
        return Err(rusqlite::Error::SqliteFailure(full, Some("the disk filled, as a test asks".into())).into());
    }
    if changed == 0 {
        return Ok(false);
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
