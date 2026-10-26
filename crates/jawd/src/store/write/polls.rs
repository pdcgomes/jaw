//! What the writer does with polls: each poll's options as its message is written, each voter's
//! latest vote, which one older than it never replaces, and the votes that wait in `pending` for a
//! poll jaw hasn't seen yet, VOTES_WAITING at most. A poll's tallies, who voted and whether you did
//! aren't kept anywhere: windows count them from these rows as they read the poll (`read`).

use anyhow::Result;
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{CHAT, MESSAGE, Stamps, now_ms, unpack};
use crate::convert::{self, kinds};

/// The most votes kept waiting in `pending` for polls jaw hasn't seen; past it the oldest are set
/// aside in quarantine, whole, and put back when their poll comes, so none is lost and `pending`
/// stays bounded.
pub const VOTES_WAITING: i64 = 10_000;

const OPTION: &str = "INSERT INTO poll_options (message, position, name, hash) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (message, position) DO NOTHING";
const HAS_OPTIONS: &str = "SELECT EXISTS (SELECT 1 FROM poll_options WHERE message = ?1)";
const LATEST: &str = "SELECT at, id FROM poll_voters WHERE message = ?1 AND voter = ?2";
const VOTER: &str = "INSERT INTO poll_voters (message, voter, at, id) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (message, voter) DO UPDATE SET at = excluded.at, id = excluded.id";
const UNVOTED: &str = "DELETE FROM poll_votes WHERE message = ?1 AND voter = ?2";
const VOTED: &str = "INSERT INTO poll_votes (message, voter, position, at)
    SELECT message, ?2, position, ?3 FROM poll_options WHERE message = ?1 AND hash = ?4
    ON CONFLICT (message, voter, position) DO NOTHING";
const WAITING: &str = "SELECT count(*) FROM pending WHERE kind = 'vote'";
// A vote set aside is named by its poll and then its voter and its own ID, so one arriving again is
// the same row, and kept whole, its pending row as JSONB, to wait there for its poll.
const SET_ASIDE: &str = "INSERT INTO quarantine (at, source, ref, error, raw)
    SELECT ?1, 'vote', chat || ' ' || target || ' ' || coalesce(sender, '') || ' ' || coalesce(effect ->> '$.id', n),
        'more votes waited for polls jaw has not seen than it keeps in pending, so this one waits here for its poll',
        jsonb_object('chat', chat, 'target', target, 'at', at, 'sender', sender, 'mine', mine, 'effect', effect,
            'raw', hex(raw))
    FROM pending WHERE n IN (SELECT n FROM pending WHERE kind = 'vote' ORDER BY n LIMIT ?2)
    ON CONFLICT (source, ref) DO NOTHING";
const DROP_SET_ASIDE: &str =
    "DELETE FROM pending WHERE n IN (SELECT n FROM pending WHERE kind = 'vote' ORDER BY n LIMIT ?1)";
const BACK: &str = "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
    SELECT raw ->> '$.chat', raw ->> '$.target', 'vote', raw ->> '$.at', unhex(raw ->> '$.raw'), raw ->> '$.sender',
        raw ->> '$.mine', raw ->> '$.effect'
    FROM quarantine WHERE source = 'vote' AND ref >= ?1 AND ref < ?2";
const BACK_DONE: &str = "DELETE FROM quarantine WHERE source = 'vote' AND ref >= ?1 AND ref < ?2";
const POLL_ROW: &str = "SELECT chat, raw, detail FROM messages WHERE n = ?1";
const POLL_MAX: &str = "UPDATE messages SET detail = json_set(detail, '$.poll.max', ?2), v = ?3
    WHERE n = ?1 AND detail ->> '$.poll.max' IS NOT ?2";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";

/// SHA-256 of a poll option's name, as a vote names it.
pub fn option_hash(name: &str) -> Vec<u8> {
    Sha256::digest(name.as_bytes()).to_vec()
}

/// Bytes from hex, as a vote's options come.
fn unhex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok()).collect()
}

/// Keeps poll message `n`'s options, `names` in their order; the same again changes nothing.
pub(super) fn options(db: &Connection, n: i64, names: &[String]) -> Result<()> {
    for (position, name) in names.iter().enumerate() {
        db.prepare_cached(OPTION)?.execute(params![n, position as i64, name, option_hash(name)])?;
    }
    Ok(())
}

/// Applies `voter`'s vote in poll `n`, made at `at` as message `id`, choosing `options` (hex SHA-256
/// of their names): it replaces their last one only if it's newer, by time and then ID, so a vote
/// arriving again or late changes nothing. An option the poll doesn't have is no choice. Says how
/// many rows of the poll changed, 0 or 1.
pub(super) fn vote(
    db: &Connection,
    stamps: &mut Stamps,
    n: i64,
    voter: i64,
    options: &[String],
    id: &str,
    at: i64,
) -> Result<usize> {
    // A poll converted before votes were kept is given its options first, as its backfill would.
    if !db.prepare_cached(HAS_OPTIONS)?.query_row([n], |r| r.get::<_, bool>(0))? {
        backfilled(db, stamps, n)?;
    }
    let latest: Option<(i64, String)> =
        db.prepare_cached(LATEST)?.query_row([n, voter], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if latest.as_ref().is_some_and(|(was, was_id)| (*was, was_id.as_str()) >= (at, id)) {
        return Ok(0);
    }
    db.prepare_cached(VOTER)?.execute(params![n, voter, at, id])?;
    db.prepare_cached(UNVOTED)?.execute([n, voter])?;
    for hash in options.iter().filter_map(|hex| unhex(hex)) {
        db.prepare_cached(VOTED)?.execute(params![n, voter, at, hash])?;
    }
    Ok(1)
}

/// Gives poll message `n`, converted before votes were kept, its options and how many a vote may
/// choose, as its raw protobuf says; one that has them already changes nothing. A poll whose raw
/// protobuf doesn't read is given the options its row shows, and keeps the choices it says, so
/// neither its backfill nor a vote in it is held up by it. Its row takes a new version if what it
/// shows changed.
pub(super) fn backfilled(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let (chat, raw, detail): (i64, Option<Vec<u8>>, Option<String>) =
        db.prepare_cached(POLL_ROW)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let info = raw.and_then(|raw| unpack(&raw).ok()).and_then(|raw| WebMessageInfo::decode(raw.as_slice()).ok());
    let Some(poll) = info.as_ref().and_then(|info| info.message.as_ref()).and_then(convert::poll_of) else {
        let shown: serde_json::Value = detail.and_then(|detail| serde_json::from_str(&detail).ok()).unwrap_or_default();
        let names: Vec<String> = shown["poll"]["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| option["name"].as_str().map(str::to_string))
            .collect();
        options(db, n, &names)?;
        return Ok(false);
    };
    let names: Vec<String> = poll.options.iter().map(|o| o.option_name.clone().unwrap_or_default()).collect();
    options(db, n, &names)?;
    let seq = stamps.seq(MESSAGE, Some(n));
    if db.prepare_cached(POLL_MAX)?.execute(params![n, kinds::poll_max(poll), seq])? == 0 {
        return Ok(false);
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

/// Puts the votes set aside for poll `id` in `chat`, which has just come, back in `pending`, to be
/// applied with the rest waiting for it.
pub(super) fn set_aside_back(db: &Connection, chat: &str, id: &str) -> Result<()> {
    let (from, to) = (format!("{chat} {id} "), format!("{chat} {id}!"));
    if db.prepare_cached(BACK)?.execute([&from, &to])? > 0 {
        db.prepare_cached(BACK_DONE)?.execute([&from, &to])?;
    }
    Ok(())
}

/// Keeps the votes waiting for polls within VOTES_WAITING, setting the oldest past it aside.
pub(super) fn bounded(db: &Connection) -> Result<()> {
    let waiting: i64 = db.prepare_cached(WAITING)?.query_row([], |r| r.get(0))?;
    if waiting > VOTES_WAITING {
        let over = waiting - VOTES_WAITING;
        db.prepare_cached(SET_ASIDE)?.execute(params![now_ms(), over])?;
        db.prepare_cached(DROP_SET_ASIDE)?.execute([over])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
