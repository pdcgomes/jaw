//! Disappearing messages, erased once their time comes, as the design's "Erased when asked" says:
//! each becomes a tombstone (its chat, ID and time), as deleting it for you leaves one, so no copy
//! brings it back, and its row goes with its event, as that deletion's does, telling no other
//! device. Its reactions, receipts, votes, media rows and links go with it by their keys and
//! triggers, and the chat shows the message before it with its counts made again. Erasing zeroes
//! every page it frees, those a row's raw protobuf spilled onto among them, which the FAST
//! `secure_delete` the writer otherwise runs with leaves on disk as they were, and has search's
//! indexes take its words out of the segments that hold them (FTS5's `secure-delete`) rather than
//! leave them there until a merge; both go back to how they were as it ends.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{MESSAGE, Stamps, delete_row, now_ms};

/// How many due messages one synced commit erases at most: with each erasure's words taken out of
/// the indexes' segments as it goes, a batch keeps to a short transaction.
pub const BATCH: usize = 32;

/// The messages that go soonest, from ?1 on, with their times, ?2 at most, from the index's first
/// entries.
pub const NEXT: &str = "SELECT n, expires_at FROM messages WHERE expires_at >= ?1 ORDER BY expires_at LIMIT ?2";
/// The messages due by ?1, soonest first, at most ?2.
pub const DUE: &str = "SELECT n FROM messages WHERE expires_at <= ?1 ORDER BY expires_at LIMIT ?2";
/// Whether message ?1 is still due by ?2, as one whose erasure failed is.
pub const STILL_DUE: &str = "SELECT 1 FROM messages WHERE n = ?1 AND expires_at <= ?2";
/// When the next message goes, 0 for none, for `status`.
pub const NEXT_FOR_STATUS: &str = "SELECT coalesce(min(expires_at), 0), 0 FROM messages WHERE expires_at IS NOT NULL";
/// The messages with a time to go, counted for `status`.
pub const EXPIRING: &str = "messages WHERE expires_at IS NOT NULL";

/// Message `n`'s chat, if its time has come by `at`.
const DUE_IN: &str = "SELECT chat FROM messages WHERE n = ?1 AND expires_at <= ?2";
/// Where sync_state keeps when a message with links was last erased, until links_fts has been
/// merged whole since, which the writer does in its idle moments (`Writer::merge`).
pub const LINKS_ERASED: &str = "erased.links";
const NOTE_LINKS_ERASED: &str = "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('erased.links', ?1)";
const SECURE_DELETE_ON: &str = "PRAGMA secure_delete = 1";
const SECURE_DELETE_FAST: &str = "PRAGMA secure_delete = FAST";
const MESSAGES_SECURE: &str = "INSERT INTO messages_fts (messages_fts, rank) VALUES ('secure-delete', ?1)";
const LINKS_SECURE: &str = "INSERT INTO links_fts (links_fts, rank) VALUES ('secure-delete', ?1)";

/// What `PRAGMA secure_delete` reads as at FAST, which the writer runs with.
pub const FAST: i64 = 2;

/// Erases message `n` if its time has come by `at`, saying whether it did: one erased already, or
/// given a later time since, is left as it is, so the same erasure twice erases once.
pub(super) fn expire(db: &Connection, stamps: &mut Stamps, n: i64, at: i64) -> Result<bool> {
    let Some(chat): Option<i64> = db.prepare_cached(DUE_IN)?.query_row(params![n, at], |r| r.get(0)).optional()? else {
        return Ok(false);
    };
    erase(db, stamps, chat, n)
}

/// Erases message `n` of chat `chat`, as a disappearing message goes; one whose links' words
/// links_fts holds is noted, for the whole merge that takes them out.
pub(super) fn erase(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64) -> Result<bool> {
    let linked = db.prepare_cached(LINKED)?.exists([n])?;
    let erased = erasing(db, || delete_row(db, stamps, chat, n))?;
    if erased {
        stamps.erased_one();
    }
    if linked && erased {
        let now = now_ms();
        db.prepare_cached(NOTE_LINKS_ERASED)?.execute([now.to_string()])?;
        stamps.links_erased(now);
    }
    Ok(erased)
}

/// What becomes of a view-once message of someone else's opened here.
#[derive(Clone, Debug)]
pub enum Once {
    /// Message `n` opened: it goes at `until` if nothing closes it first, and WhatsApp is told it
    /// was played, through the outbox, with `receipt`, an engine MarkRead, to `jid`, naming its
    /// message `id`. Nothing changes for one opened before, which isn't opened again.
    Opened { n: i64, until: i64, jid: String, id: String, receipt: Vec<u8> },
    /// Message `n`, opened, is shown no more, so it's erased.
    Closed { n: i64 },
    /// A start: the connections that opened the messages still open are gone, so they're erased.
    Restarted,
}

/// The most view-once messages open at once.
pub const OPEN_MOST: i64 = 16;
/// The most receipts of messages opened on your other devices that wait for messages jaw hasn't
/// seen; past it the oldest are set aside in quarantine, where a message arriving finds its own.
pub const OPENED_WAITING: i64 = 1_000;

const OPEN: &str = "INSERT INTO opened (message, until) SELECT ?1, ?2 FROM messages
    WHERE n = ?1 AND view_once AND NOT mine ON CONFLICT (message) DO NOTHING";
const OPEN_UNTIL: &str = "UPDATE messages SET expires_at = iif(expires_at < ?2, expires_at, ?2), v = ?3 WHERE n = ?1
    RETURNING chat";
const PLAYED: &str = "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at, target)
    VALUES (?1, ?2, 'played', ?3, ?4, ?4, ?5)";
const OPEN_IN: &str = "SELECT m.chat FROM opened o JOIN messages m ON m.n = o.message WHERE o.message = ?1";
const STILL_OPEN: &str = "SELECT message FROM opened ORDER BY message LIMIT ?1";
/// Whether message ?1 is someone else's view-once message.
const ONCE: &str = "SELECT chat FROM messages WHERE n = ?1 AND view_once AND NOT mine";
const LINKED: &str = "SELECT 1 FROM links WHERE message = ?1";
const OPENED_WAITING_NOW: &str = "SELECT count(*) FROM pending WHERE kind = 'opened'";
const SET_OPENED_ASIDE: &str = "INSERT OR IGNORE INTO quarantine (at, source, ref, error)
    SELECT ?1, 'opened', chat || ' ' || target,
        'more receipts of view-once messages opened on your other devices waited than jaw keeps, so this one waits here'
    FROM pending WHERE kind = 'opened' ORDER BY n LIMIT ?2";
const DROP_OPENED_ASIDE: &str =
    "DELETE FROM pending WHERE n IN (SELECT n FROM pending WHERE kind = 'opened' ORDER BY n LIMIT ?1)";
const OPENED_ASIDE: &str = "DELETE FROM quarantine WHERE source = 'opened' AND ref = ?1";

pub(super) fn once(db: &Connection, stamps: &mut Stamps, once: &Once) -> Result<bool> {
    match once {
        Once::Opened { n, until, jid, id, receipt } => {
            if db.prepare_cached(OPEN)?.execute(params![n, until])? == 0 {
                return Ok(false);
            }
            let seq = stamps.seq(MESSAGE, Some(*n));
            let chat: i64 = db.prepare_cached(OPEN_UNTIL)?.query_row(params![n, until, seq], |r| r.get(0))?;
            stamps.record(db, MESSAGE, *n, Some(chat), seq)?;
            stamps.expires(Some(*until));
            db.prepare_cached(PLAYED)?.execute(params![jid, id, receipt, now_ms(), n])?;
            Ok(true)
        }
        Once::Closed { n } => {
            let Some(chat): Option<i64> = db.prepare_cached(OPEN_IN)?.query_row([n], |r| r.get(0)).optional()? else {
                return Ok(false);
            };
            erase(db, stamps, chat, *n)
        }
        Once::Restarted => {
            let open: Vec<i64> = db
                .prepare_cached(STILL_OPEN)?
                .query_map([OPEN_MOST * 4], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut erased = false;
            for n in open {
                if let Some(chat) = db.prepare_cached(OPEN_IN)?.query_row([n], |r| r.get(0)).optional()? {
                    erased |= erase(db, stamps, chat, n)?;
                }
            }
            Ok(erased)
        }
    }
}

/// Erases message `n` of chat `chat` if it's someone else's view-once message, as one opened on
/// another of your devices is; any other message is left as it is.
pub(super) fn opened_elsewhere(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    match db.prepare_cached(ONCE)?.query_row([n], |r| r.get(0)).optional()? {
        Some(chat) => erase(db, stamps, chat, n),
        None => Ok(false),
    }
}

/// Keeps the receipts of messages opened elsewhere that wait for messages jaw hasn't seen within
/// OPENED_WAITING, setting the oldest past it aside.
pub(super) fn opened_bounded(db: &Connection) -> Result<()> {
    let waiting: i64 = db.prepare_cached(OPENED_WAITING_NOW)?.query_row([], |r| r.get(0))?;
    if waiting > OPENED_WAITING {
        let over = waiting - OPENED_WAITING;
        db.prepare_cached(SET_OPENED_ASIDE)?.execute(params![now_ms(), over])?;
        db.prepare_cached(DROP_OPENED_ASIDE)?.execute([over])?;
    }
    Ok(())
}

/// Whether a receipt set aside says message `id` in the chat whose JID is `chat`, arriving now,
/// was opened on another of your devices; the receipt goes either way.
pub(super) fn opened_aside(db: &Connection, chat: &str, id: &str) -> Result<bool> {
    Ok(db.prepare_cached(OPENED_ASIDE)?.execute([format!("{chat} {id}")])? > 0)
}

/// Runs `erase` with every page it frees zeroed and search's indexes taking out what it deletes,
/// putting both back after it, whether it worked or not. Turning FTS5's `secure-delete` off writes
/// out what the indexes hold of the transaction first, with it still on.
pub(super) fn erasing<T>(db: &Connection, erase: impl FnOnce() -> Result<T>) -> Result<T> {
    secure_delete(db, SECURE_DELETE_ON, 1)?;
    let erased = (|| -> Result<T> {
        db.prepare_cached(MESSAGES_SECURE)?.execute([1])?;
        db.prepare_cached(LINKS_SECURE)?.execute([1])?;
        let erased = erase()?;
        db.prepare_cached(MESSAGES_SECURE)?.execute([0])?;
        db.prepare_cached(LINKS_SECURE)?.execute([0])?;
        Ok(erased)
    })();
    let fast = secure_delete(db, SECURE_DELETE_FAST, FAST);
    let erased = erased?;
    fast?;
    Ok(erased)
}

/// Sets `secure_delete` with `pragma`, checking it reads back as `expected`.
fn secure_delete(db: &Connection, pragma: &str, expected: i64) -> Result<()> {
    let read: i64 = db.prepare_cached(pragma)?.query_row([], |r| r.get(0))?;
    anyhow::ensure!(read == expected, "secure_delete is {read} after setting it to {expected}");
    Ok(())
}

#[cfg(test)]
mod tests;
