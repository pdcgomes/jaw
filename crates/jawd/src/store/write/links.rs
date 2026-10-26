//! What the writer does for links beyond what the triggers on messages do: the backfill that links
//! the messages jaw.db held before links were kept, a short step at a time from where the last one
//! left off, and the thumbnails of link previews, whose rows go in once their files are whole.

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::{CHAT, MESSAGE, Stamps, chat_jid, located, now_ms};

/// The backfill that links the messages jaw.db held when links began to be kept.
pub const LINKS: &str = "links";
/// The backfills this jawd knows, which a migration may leave rows for.
pub const BACKFILLS: &[&str] = &[LINKS];
/// How many messages one step of the links backfill looks at: most have no link, and a look at one
/// that hasn't is a read of its row.
const LINKS_STEP: i64 = 64;

const BACKFILL: &str = "SELECT after, upto FROM backfills WHERE name = ?1";
/// The next messages, each with whether it may have links, as the triggers on messages ask.
const NEXT_MESSAGES: &str = "SELECT n, instr(spans, '\"t\":\"link\"') > 0 OR instr(detail, '\"link\":') > 0
    FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const LINKED: &str = "SELECT 1 FROM links WHERE message = ?1";
const LINK: &str = "INSERT INTO links (message, place, chat, at, sender, mine, url, host, title, description)
    SELECT message, place, chat, at, sender, mine, url, host, title, description FROM message_links
    WHERE message = ?1";
const INDEX_LINKS: &str =
    "INSERT INTO links_fts (rowid, text) SELECT t, text FROM message_link_text WHERE message = ?1 AND text IS NOT NULL";
const BACKFILLED: &str = "UPDATE backfills SET after = ?2 WHERE name = ?1";
const BACKFILL_DONE: &str = "DELETE FROM backfills WHERE name = ?1";
const BACKFILL_KEPT: &str = "DELETE FROM quarantine WHERE source = 'backfill' AND ref = ?1";
const THUMBNAIL: &str =
    "INSERT INTO thumbnails (sha256, bytes, at) VALUES (?1, ?2, ?3) ON CONFLICT (sha256) DO NOTHING";
const SHOWN_AGAIN: &str = "UPDATE messages SET v = ?2 WHERE n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// Takes one step of backfill `name`, if it's still to do: the next messages it hasn't looked at,
/// and once none are left, its row goes. A message the triggers have linked since is left as it
/// is, so a step taken twice, or after the message changed, links nothing twice.
pub(super) fn backfill(db: &Connection, name: &str) -> Result<bool> {
    let Some((after, upto)) = db
        .prepare_cached(BACKFILL)?
        .query_row([name], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .optional()?
    else {
        return Ok(false);
    };
    let step = match name {
        LINKS => LINKS_STEP,
        other => bail!("jawd doesn't know the backfill {other}"),
    };
    let next: Vec<(i64, Option<bool>)> = db
        .prepare_cached(NEXT_MESSAGES)?
        .query_map(params![after, upto, step], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut changed = false;
    for &(n, linkish) in &next {
        if linkish == Some(true)
            && !db.prepare_cached(LINKED)?.exists([n])?
            && db.prepare_cached(LINK)?.execute([n])? > 0
        {
            db.prepare_cached(INDEX_LINKS)?.execute([n])?;
            changed = true;
        }
    }
    match next.last() {
        Some(&(last, _)) if next.len() as i64 == step && last < upto => {
            db.prepare_cached(BACKFILLED)?.execute(params![name, last])?;
        }
        _ => {
            db.prepare_cached(BACKFILL_DONE)?.execute([name])?;
            db.prepare_cached(BACKFILL_KEPT)?.execute([name])?;
        }
    }
    Ok(changed)
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
