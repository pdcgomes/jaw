//! What the writer does for links beyond what the triggers on messages do: the backfill that links
//! the messages jaw.db held before links were kept, a short step at a time from where the last one
//! left off, and the thumbnails of link previews, whose rows go in once their files are whole.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::contacts::{self, CONTACTS};
use super::copies::{self, COPIES};
use super::expiry::{self, VIEW_ONCE};
use super::kinds::{self, KINDS};
use super::{CHAT, MESSAGE, Stamps, chat_jid, located, now_ms};

/// The backfill that links the messages jaw.db held when links began to be kept.
pub const LINKS: &str = "links";
/// The backfill that gives the polls jaw.db held before votes were kept their options.
pub const POLLS: &str = "polls";
/// The backfill that reads the locations jaw.db held again, so each is found by its address too.
pub const PLACES: &str = "places";
/// The backfill that reads the events jaw.db held again, for their place and call link, and opens
/// the answers and edits an earlier jawd kept sealed for them.
pub const EVENTS: &str = "events";
/// The backfill that reads the voice messages jaw.db held again, for their length and waveform.
pub const VOICES: &str = "voices";
/// The backfills this jawd knows, which a migration may leave rows for.
pub const BACKFILLS: &[&str] = &[LINKS, POLLS, KINDS, COPIES, PLACES, EVENTS, CONTACTS, VOICES, VIEW_ONCE];
/// How many messages one step of a backfill looks at: most are none of its business, and a look at
/// one that isn't is a read of its row.
const STEP: i64 = 64;

const BACKFILL: &str = "SELECT after, upto FROM backfills WHERE name = ?1";
/// The next messages, each with whether it may have links, as the triggers on messages ask.
const NEXT_LINKED: &str = "SELECT n, instr(spans, '\"t\":\"link\"') > 0 OR instr(detail, '\"link\":') > 0
    FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
/// The next messages, each with whether it's a poll still there.
const NEXT_POLLS: &str =
    "SELECT n, kind = 'poll' AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
/// The next messages, each with whether it's a location still there.
const NEXT_PLACES: &str =
    "SELECT n, kind = 'location' AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
/// The next messages, each with whether it's an event still there.
const NEXT_EVENTS: &str =
    "SELECT n, kind = 'event' AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
/// The next messages, each with whether it's a voice message still there.
const NEXT_VOICES: &str =
    "SELECT n, kind = 'voice' AND NOT deleted FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const LINKED: &str = "SELECT 1 FROM links WHERE message = ?1";
const CHAT_OF: &str = "SELECT chat FROM messages WHERE n = ?1";
const LINK: &str = "INSERT INTO links (message, place, chat, at, sender, mine, url, host, title, description)
    SELECT message, place, chat, at, sender, mine, url, host, title, description FROM message_links
    WHERE message = ?1";
/// A message's links' words, as one document, unless it waits in fts_backlog, whose steps index them
/// with its text.
pub(super) const INDEX_LINKS: &str = "INSERT INTO links_fts (rowid, text) SELECT t, text FROM message_link_text
    WHERE message = ?1 AND text IS NOT NULL AND NOT EXISTS (SELECT 1 FROM fts_backlog WHERE n = ?1)";
const BACKFILLED: &str = "UPDATE backfills SET after = ?2 WHERE name = ?1";
const BACKFILL_DONE: &str = "DELETE FROM backfills WHERE name = ?1";
const BACKFILL_KEPT: &str = "DELETE FROM quarantine WHERE source = 'backfill' AND ref = ?1";
const THUMBNAIL: &str =
    "INSERT INTO thumbnails (sha256, bytes, at) VALUES (?1, ?2, ?3) ON CONFLICT (sha256) DO NOTHING";
const SHOWN_AGAIN: &str = "UPDATE messages SET v = ?2 WHERE n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// What one step of a backfill does to each message it's for, saying whether it changed anything.
type Take = fn(&Connection, &mut Stamps, i64) -> Result<bool>;

/// Takes one step of backfill `name`, if it's still to do: the next STEP messages it hasn't looked
/// at (fewer for some), each it's for taken as `name` takes one, and once none are left, its row goes. A message
/// the triggers or the writer have dealt with since is left as it is, so a step taken twice, or
/// after the message changed, does nothing twice.
pub(super) fn backfill(db: &Connection, stamps: &mut Stamps, name: &str) -> Result<bool> {
    let Some((after, upto)) = db
        .prepare_cached(BACKFILL)?
        .query_row([name], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .optional()?
    else {
        return Ok(false);
    };
    // How long a step may take messages for, past the first, when taking one costs much, and how
    // many it looks at.
    let (next, take, taking, step): (&str, Take, Option<Duration>, i64) = match name {
        LINKS => (NEXT_LINKED, link, None, STEP),
        POLLS => (NEXT_POLLS, super::polls::backfilled, None, STEP),
        KINDS => (kinds::NEXT_UNSHOWN, kinds::backfilled, Some(kinds::CONVERTING), STEP),
        COPIES => (copies::NEXT_COPYLESS, copies::backfilled, Some(copies::COPYING), copies::STEP),
        PLACES => (NEXT_PLACES, super::again::places, Some(kinds::CONVERTING), STEP),
        EVENTS => (NEXT_EVENTS, super::again::events, Some(kinds::CONVERTING), STEP),
        CONTACTS => (contacts::NEXT_CARDS, contacts::backfilled, Some(kinds::CONVERTING), STEP),
        VOICES => (NEXT_VOICES, super::again::voices, Some(kinds::CONVERTING), super::again::VOICES_STEP),
        VIEW_ONCE => (expiry::NEXT_MARKED, expiry::marked, Some(expiry::UNMARKING), STEP),
        other => bail!("jawd doesn't know the backfill {other}"),
    };
    let rows: Vec<(i64, Option<bool>)> = db
        .prepare_cached(next)?
        .query_map(params![after, upto, step], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let began = Instant::now();
    let (mut changed, mut took) = (false, false);
    let mut looked = rows.len();
    for (i, &(n, wanted)) in rows.iter().enumerate() {
        if wanted != Some(true) {
            continue;
        }
        if took && taking.is_some_and(|most| began.elapsed() >= most) {
            looked = i;
            break;
        }
        changed |= take(db, stamps, n)?;
        took = true;
    }
    // A step that stopped before the end of what it read goes on, next time, from where it stopped.
    match rows[..looked].last() {
        Some(&(last, _)) if (looked < rows.len() || rows.len() as i64 == step) && last < upto => {
            db.prepare_cached(BACKFILLED)?.execute(params![name, last])?;
        }
        _ => {
            db.prepare_cached(BACKFILL_DONE)?.execute([name])?;
            db.prepare_cached(BACKFILL_KEPT)?.execute([name])?;
        }
    }
    Ok(changed)
}

/// Links message `n`, unless the triggers have since. Its links are the library's now, so its event
/// is written, though its row is as it was.
fn link(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    if db.prepare_cached(LINKED)?.exists([n])? || db.prepare_cached(LINK)?.execute([n])? == 0 {
        return Ok(false);
    }
    db.prepare_cached(INDEX_LINKS)?.execute([n])?;
    let chat: i64 = db.prepare_cached(CHAT_OF)?.query_row([n], |r| r.get(0))?;
    let seq = stamps.seq(MESSAGE, Some(n));
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    Ok(true)
}

/// A link preview's thumbnail is whole in its file: its row goes in, and message `id` in `chat`,
/// which shows it, takes a new version, so windows show it there.
pub(super) fn thumbnail(
    db: &Connection,
    stamps: &mut Stamps,
    sha256: &str,
    bytes: u64,
    chat: &str,
    id: &str,
) -> Result<bool> {
    let bytes = i64::try_from(bytes)?;
    if db.prepare_cached(THUMBNAIL)?.execute(params![sha256, bytes, now_ms()])? == 0 {
        return Ok(false);
    }
    let Some((chat, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(true) };
    let seq = stamps.seq(MESSAGE, Some(n));
    db.prepare_cached(SHOWN_AGAIN)?.execute(params![n, seq])?;
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
