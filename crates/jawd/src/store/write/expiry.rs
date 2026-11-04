//! Disappearing messages, erased once their time comes, as the design's "Erased when asked" says:
//! each becomes a tombstone (its chat, ID and time), as deleting it for you leaves one, so no copy
//! brings it back, and its row goes with its event, as that deletion's does, telling no other
//! device. Its reactions, receipts, votes, media rows and links go with it by their keys and
//! triggers, and the chat shows the message before it with its counts made again. Erasing zeroes
//! every page it frees, those a row's raw protobuf spilled onto among them, which the FAST
//! `secure_delete` the writer otherwise runs with leaves on disk as they were, and has search's
//! indexes take its words out of the segments that hold them (FTS5's `secure-delete`) rather than
//! leave them there until a merge; both go back to how they were as it ends.

use std::time::{Duration, Instant};

use anyhow::Result;
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{CHAT, MESSAGE, PENDING_FOR, Stamps, delete_row, now_ms, unpack};
use crate::convert;

/// How many due messages the timer hands the writer at once, which erases them in the background
/// lane's short transactions: each erasure's words taken out of the indexes' segments as it goes
/// cost about a millisecond at a million messages, so a transaction takes one or a few.
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
/// The contentless indexes, links_fts and library_fts: a deletion from one only marks its entry
/// deleted, leaving its words in the segment that holds them until a merge rewrites it, so an
/// erasure leaving words in one has the writer merge it whole in its idle moments
/// (`Writer::merge`). Each with where sync_state keeps when an erasure last left words in it, until
/// it has been merged whole since, by its place in what a transaction notes (`Stamps::left_in`).
pub const CONTENTLESS: [(&str, &str); 2] = [("links_fts", LINKS_ERASED), ("library_fts", LIBRARY_ERASED)];
/// links_fts's place in CONTENTLESS: it holds a message's links' words.
pub const LINKS: usize = 0;
/// library_fts's: it holds a photo's, a video's, a voice message's or a file's words, and a
/// message's links' words.
pub const LIBRARY: usize = 1;
pub const LINKS_ERASED: &str = "erased.links";
const NOTE_LINKS_ERASED: &str = "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('erased.links', ?1)";
pub const LIBRARY_ERASED: &str = "erased.library";
const NOTE_LIBRARY_ERASED: &str = "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('erased.library', ?1)";
/// Message ?1's words in the contentless indexes, once it's out of fts_backlog, from which they're
/// indexed: whether it has links, whose words both hold, and whether it's an item with words, whose
/// words library_fts holds.
const IN_CONTENTLESS: &str =
    "SELECT EXISTS (SELECT 1 FROM links WHERE message = ?1), shelf IS NOT NULL AND text IS NOT NULL
    FROM messages WHERE n = ?1 AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = ?1)";
/// Where sync_state keeps when a message was last erased, or a caption taken out of its row, until
/// the writer has emptied the WAL since in its idle moments (`Writer::empty_wal`): a running jawd
/// keeps the WAL's file, and in it the frames written before, which hold what went, until they're
/// written over.
pub const WAL_ERASED: &str = "erased.wal";
const NOTE_WAL_ERASED: &str = "INSERT OR REPLACE INTO sync_state (key, value) VALUES ('erased.wal', ?1)";
const SECURE_DELETE_ON: &str = "PRAGMA secure_delete = 1";
const SECURE_DELETE_FAST: &str = "PRAGMA secure_delete = FAST";
const SECURE_DELETE_NOW: &str = "PRAGMA secure_delete";
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

/// Whether message ?1 has a time to go or is marked to open once; for one of yours still waiting
/// for the server, what it goes as; and for one of yours with neither that nothing sends, its raw
/// protobuf, which says what it went as.
const ERASABLE: &str = "SELECT m.expires_at IS NOT NULL OR m.view_once, o.raw,
        iif(m.mine AND m.expires_at IS NULL AND NOT m.view_once AND o.n IS NULL, m.raw, NULL)
    FROM messages m LEFT JOIN outbox o ON o.message = m.n WHERE m.n = ?1";
/// What ERASABLE reads of a message.
type Marks = (bool, Option<Vec<u8>>, Option<Vec<u8>>);

/// Whether message `n` has a time to go or is marked to open once, so whatever changes what it
/// says, or takes it away, takes its words as an erasure takes them (`unsaid`), lest they outlive
/// it: an edit, a revoke, a deletion for you, a copy, a fold, a backfill's reading it again. One of
/// yours that goes with a timer has a time to go once the server has it, so it counts as one from
/// when it's written: by the outbox row that sends it while it waits, and by its own raw protobuf
/// once nothing sends it and it has no time, as when it failed and your deletion of it for everyone
/// waits for a late receipt, which leaves nothing to send it again.
pub(super) fn erasable(db: &Connection, n: i64) -> Result<bool> {
    let row: Option<Marks> =
        db.prepare_cached(ERASABLE)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    Ok(match row {
        Some((true, _, _)) => true,
        Some((false, Some(goes_as), _)) => convert::timer(&goes_as).is_some(),
        Some((false, None, Some(went))) => went_with(&went).is_some(),
        _ => false,
    })
}

/// The disappearing timer, in seconds, that `packed`, a message's raw protobuf as its row keeps it,
/// went with; none for one that went without, or doesn't read.
pub(super) fn went_with(packed: &[u8]) -> Option<u32> {
    let info = unpack(packed).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok())?;
    convert::timer_of(info.message.as_ref()?)
}

/// Erases message `n` of chat `chat`, as a disappearing message goes, and what waits for it.
pub(super) fn erase(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64) -> Result<bool> {
    let erased = unsaid(db, stamps, &[n], |stamps| {
        forget_waiting(db, n)?;
        delete_row(db, stamps, chat, n)
    })?;
    if erased {
        stamps.erased_one();
    }
    Ok(erased)
}

/// Runs `change`, which takes what messages `ns` say, or some of it, out of their rows and search's
/// indexes, as an erasure takes it (`erasing`), saying whether it changed anything. If it did, what
/// it leaves to do is noted with it, so a kill leaves that to do too: the WAL, whose frames hold
/// what went, to empty, and each contentless index that held words of theirs to merge whole.
pub(super) fn unsaid(
    db: &Connection,
    stamps: &mut Stamps,
    ns: &[i64],
    change: impl FnOnce(&mut Stamps) -> Result<bool>,
) -> Result<bool> {
    let mut held = [false; CONTENTLESS.len()];
    for &n in ns {
        let words: Option<(bool, bool)> =
            db.prepare_cached(IN_CONTENTLESS)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        if let Some((linked, item)) = words {
            held[LINKS] |= linked;
            held[LIBRARY] |= linked || item;
        }
    }
    let began = Instant::now();
    let changed = erasing(db, || change(stamps))?;
    stamps.securely_deleted(began.elapsed());
    if changed {
        wal_to_empty(db, stamps)?;
        let now = now_ms();
        for (index, note) in [(LINKS, NOTE_LINKS_ERASED), (LIBRARY, NOTE_LIBRARY_ERASED)] {
            if held[index] {
                db.prepare_cached(note)?.execute([now.to_string()])?;
                stamps.left_in(index, now);
            }
        }
    }
    Ok(changed)
}

/// Notes, with what went, that the WAL is to be emptied, so a kill before the writer does it
/// leaves it to do.
fn wal_to_empty(db: &Connection, stamps: &mut Stamps) -> Result<()> {
    stamps.wal_erased(wal_noted(db)?);
    Ok(())
}

/// Notes in sync_state that the WAL is to be emptied, saying when it noted it.
pub(crate) fn wal_noted(db: &Connection) -> Result<i64> {
    let now = now_ms();
    db.prepare_cached(NOTE_WAL_ERASED)?.execute([now.to_string()])?;
    Ok(now)
}

/// Takes what waits for message `n`, which can never apply once it's erased, as a message that
/// arrives after its time takes it: what's kept in pending for it, such as an edit or an event's
/// answer sealed with its secret, which this jawd keeps for a later one, and what was set aside in
/// quarantine past what waits, a receipt of it opened elsewhere, named by the message, and votes,
/// answers and edits, named from it on.
fn forget_waiting(db: &Connection, n: i64) -> Result<()> {
    let named: Option<(String, String)> =
        db.prepare_cached(NAMED)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let Some((chat, id)) = named else { return Ok(()) };
    db.prepare_cached(PENDING_FOR)?.execute(params![chat, id])?;
    db.prepare_cached(OPENED_ASIDE)?.execute([format!("{chat} {id}")])?;
    db.prepare_cached(SET_ASIDE_FOR)?.execute(params![format!("{chat} {id} "), format!("{chat} {id}!")])?;
    Ok(())
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
/// Message ?1's ID, if it's open here.
pub(super) const OPEN_HERE: &str = "SELECT m.id FROM opened o JOIN messages m ON m.n = o.message WHERE o.message = ?1";
const STILL_OPEN: &str = "SELECT message FROM opened ORDER BY message LIMIT ?1";
/// Whether message ?1 is someone else's view-once message.
const ONCE: &str = "SELECT chat FROM messages WHERE n = ?1 AND view_once AND NOT mine";
const OPENED_WAITING_NOW: &str = "SELECT count(*) FROM pending WHERE kind = 'opened'";
const SET_OPENED_ASIDE: &str = "INSERT OR IGNORE INTO quarantine (at, source, ref, error)
    SELECT ?1, 'opened', chat || ' ' || target,
        'more receipts of view-once messages opened on your other devices waited than jaw keeps, so this one waits here'
    FROM pending WHERE kind = 'opened' ORDER BY n LIMIT ?2";
const DROP_OPENED_ASIDE: &str =
    "DELETE FROM pending WHERE n IN (SELECT n FROM pending WHERE kind = 'opened' ORDER BY n LIMIT ?1)";
const OPENED_ASIDE: &str = "DELETE FROM quarantine WHERE source = 'opened' AND ref = ?1";
/// Message ?1's chat's JID and its ID, as what waits for it names them.
const NAMED: &str = "SELECT c.id, m.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";
/// The votes, answers and edits set aside past what waits for a message, named from ?1 to ?2.
const SET_ASIDE_FOR: &str =
    "DELETE FROM quarantine WHERE source IN ('vote', 'answer', 'edit') AND ref >= ?1 AND ref < ?2";

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

/// The backfill that keeps the view-once messages an earlier jawd kept as this one keeps them. It
/// marked whatever came in a view-once wrapper, so one of a kind that can't be opened once shows
/// as it is, and it kept a photo's, a video's or a voice message's caption as its text, so one to
/// open once says only what it is, its caption kept in its raw protobuf alone, for its opening.
pub const VIEW_ONCE: &str = "once";
/// How long a step makes messages as this jawd keeps them for, past the first: taking a caption's
/// words out of search's segments costs far more than a look at a message, about a millisecond at
/// a million messages, so the step, and the transaction it's in, counts the next caption at what
/// the costliest so far took, and stops before one that would take it past its time.
pub(super) const UNMARKING: Duration = Duration::from_millis(1);
/// The next messages, each with whether it's marked view-once and not deleted. It reads no raw
/// protobuf.
pub(super) const NEXT_MARKED: &str =
    "SELECT n, view_once AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const MARKED: &str = "SELECT chat, kind, text IS NOT NULL OR spans IS NOT NULL, summary FROM messages
    WHERE n = ?1 AND view_once AND NOT deleted";
const UNMARKED: &str = "UPDATE messages SET view_once = 0, v = ?2 WHERE n = ?1 AND view_once";
/// Message ?1's caption gone from its row, which says ?2, unless its raw protobuf, which opening
/// it reads its caption from, is gone too.
const UNCAPTIONED: &str = "UPDATE messages SET text = NULL, spans = NULL, summary = ?2, v = ?3
    WHERE n = ?1 AND view_once AND raw IS NOT NULL AND (text IS NOT NULL OR spans IS NOT NULL OR summary IS NOT ?2)";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// Keeps message `n` as this jawd keeps a view-once message, if an earlier one marked it, saying
/// whether it changed: one of a kind that can't be opened once is marked no more, and one that can
/// loses its caption from its row, its words taken out of search's indexes as an erasure takes them.
pub(super) fn marked(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let row: Option<(i64, String, bool, Option<String>)> =
        db.prepare_cached(MARKED)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
    let Some((chat, kind, captioned, summary)) = row else { return Ok(false) };
    let said = convert::kinds::view_once(&kind);
    let once = convert::once_kind(&kind);
    if once && !captioned && summary.as_deref() == Some(said) {
        return Ok(false);
    }
    let seq = stamps.seq(MESSAGE, Some(n));
    let changed = if once {
        unsaid(db, stamps, &[n], |_| Ok(db.prepare_cached(UNCAPTIONED)?.execute(params![n, said, seq])? > 0))?
    } else {
        db.prepare_cached(UNMARKED)?.execute(params![n, seq])? > 0
    };
    if !changed {
        return Ok(false);
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

/// Runs `erase` with every page it frees zeroed and search's indexes taking out what it deletes,
/// putting both back after it, whether it worked or not. Turning FTS5's `secure-delete` off writes
/// out what the indexes hold of the transaction first, with it still on.
pub(super) fn erasing<T>(db: &Connection, erase: impl FnOnce() -> Result<T>) -> Result<T> {
    // Inside an erasure already, whose secure deletes are on until it ends, as when two copies
    // folded together take a caption out.
    if db.prepare_cached(SECURE_DELETE_NOW)?.query_row([], |r| r.get::<_, i64>(0))? == 1 {
        return erase();
    }
    zeroing(db, || {
        db.prepare_cached(MESSAGES_SECURE)?.execute([1])?;
        db.prepare_cached(LINKS_SECURE)?.execute([1])?;
        let erased = erase()?;
        db.prepare_cached(MESSAGES_SECURE)?.execute([0])?;
        db.prepare_cached(LINKS_SECURE)?.execute([0])?;
        Ok(erased)
    })
}

/// Runs `zero` with every page it frees zeroed, as an erasure and a merge taking out what one left
/// in a contentless index free them, putting FAST back after it, whether it worked or not.
pub(crate) fn zeroing<T>(db: &Connection, zero: impl FnOnce() -> Result<T>) -> Result<T> {
    secure_delete(db, SECURE_DELETE_ON, 1)?;
    let zeroed = zero();
    let fast = secure_delete(db, SECURE_DELETE_FAST, FAST);
    let zeroed = zeroed?;
    fast?;
    Ok(zeroed)
}

/// Sets `secure_delete` with `pragma`, checking it reads back as `expected`.
fn secure_delete(db: &Connection, pragma: &str, expected: i64) -> Result<()> {
    let read: i64 = db.prepare_cached(pragma)?.query_row([], |r| r.get(0))?;
    anyhow::ensure!(read == expected, "secure_delete is {read} after setting it to {expected}");
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
