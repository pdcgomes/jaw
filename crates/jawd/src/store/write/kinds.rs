//! The backfill that converts again the messages jawd couldn't show when they came, now that it
//! knows more kinds: a row of kind unknown, or one saying it couldn't be shown with no text,
//! whatever its kind (a system line jawd had no sentence for, or a copy that kept an unreadable
//! edit's words), takes what its raw protobuf converts to now, and one that turns out to change
//! another message, as an encrypted edit does, leaves the chat to wait in `pending` with its raw
//! protobuf, as it would arriving now. A row's raw protobuf is left as it is.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use anyhow::{Result, anyhow};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{
    CHAT, MESSAGE, Packed, Stamps, amend_message, media, polls, record_quarantine, remove_row, transient, unpack,
};
use crate::convert::{self, Me, kinds::UNKNOWN};
use crate::store::{Amend, Amendment, Change, Message};

/// The backfill that converts again the messages jawd couldn't show when they came.
pub const KINDS: &str = "kinds";
/// How long a step converts for: a conversion costs far more than a look at a message, so a step
/// meeting many in a row ends once its conversions have had this long, after one at least, and the
/// next goes on from the first it didn't take.
pub(super) const CONVERTING: Duration = Duration::from_millis(1);

/// The next messages, each with whether jawd couldn't show it when it came: one of kind unknown, or
/// one saying so with no text, as `kinds::UNKNOWN` says it, whatever its kind, as a copy of a
/// message keeps an earlier edit's words, those of an edit jaw couldn't read among them; that isn't
/// deleted. A message whose own words sum up to that sentence has its text. It reads no raw
/// protobuf, which a step would otherwise read whole for every message it looks at.
pub(super) const NEXT_UNSHOWN: &str = "SELECT n,
        NOT deleted AND (kind = 'unknown' OR text IS NULL AND summary = 'jaw can''t show this yet')
    FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const UNSHOWN: &str = "SELECT chat, id, raw, view_once FROM messages WHERE n = ?1";
const NAMED: &str = "SELECT c.id, m.id, m.raw FROM messages m JOIN chats c ON c.n = m.chat WHERE m.n = ?1";
const ME: &str = "SELECT value FROM sync_state WHERE key = 'me'";
/// A message as it converts now. Text a later edit gave it stays, an edit's time being past any
/// a message that isn't one has, unless jaw couldn't show that either, which leaves no text and the
/// sentence saying so, as no words of an edit do, though they may sum up to it: an edit after the
/// one a row stands in for left no raw protobuf of its own. A row whose raw protobuf is an edit
/// standing in for its message (?15) keeps what the message itself brought, as its copy may have
/// since: its quote, forwarding, expiry and mention of you, which an edit doesn't carry. One to open
/// once (?10, as `show` works it out) says only what it is (?16), whatever an edit said.
const SHOWN: &str = "UPDATE messages SET kind = ?2,
        text = iif(?10, NULL,
            iif(edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13), text, ?3)),
        spans = iif(?10, NULL,
            iif(edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13), spans, ?4)),
        summary = iif(?10, ?16,
            iif(edited_at > coalesce(?12, -1) AND (text IS NOT NULL OR summary IS NOT ?13), summary,
                iif(?5 IS ?3, NULL, ?5))),
        reply = iif(?15, reply, ?6), detail = ?7, forwarded = iif(?15, forwarded, ?8),
        expires_at = iif(?15 OR ?9 IS NULL OR ?9 >= coalesce(expires_at, ?9 + 1), expires_at, ?9),
        view_once = ?10, mentions_me = iif(?15, mentions_me, ?11), v = ?14
    WHERE n = ?1 AND NOT deleted";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";
/// The raw protobufs of what's kept for `target` from the same moment, among them a copy's.
const KEPT_ALIKE: &str = "SELECT raw FROM pending WHERE chat = ?1 AND target = ?2 AND at = ?3 AND kind = ?4";

/// Converts message `n` again, if jawd couldn't show it when it came: it takes what its raw
/// protobuf converts to now, or, if that's an amendment to another message, leaves the chat to wait
/// in `pending`. One that still can't be shown, or whose raw protobuf doesn't read, is left as it is,
/// and one whose conversion fails, or panics, is kept in quarantine with its raw protobuf while the
/// step goes on, so no message holds up the rest.
pub(super) fn backfilled(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    guarded(db, stamps, KINDS, n, converted_again)
}

/// Takes message `n` as `take` does, in a savepoint of its own: one whose taking fails, or panics,
/// is kept in quarantine under `source` with its raw protobuf, what it wrote undone, while the step
/// goes on, so no message holds up the rest; one failing for a reason that passes fails the step.
pub(super) fn guarded(
    db: &Connection,
    stamps: &mut Stamps,
    source: &str,
    n: i64,
    take: fn(&Connection, &mut Stamps, i64) -> Result<bool>,
) -> Result<bool> {
    let mark = stamps.mark();
    db.execute_batch("SAVEPOINT backfill_message")?;
    match take(db, stamps, n) {
        Ok(changed) => {
            db.execute_batch("RELEASE backfill_message")?;
            Ok(changed)
        }
        Err(e) if transient(&e) => Err(e),
        Err(e) => {
            db.execute_batch("ROLLBACK TO backfill_message; RELEASE backfill_message")?;
            stamps.undo(mark);
            let (chat, id, raw): (String, String, Option<Vec<u8>>) =
                db.prepare_cached(NAMED)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            eprintln!("jawd: kept message {id} in {chat} in quarantine, as converting it again failed: {e:#}");
            record_quarantine(db, source, &format!("{chat} {id}"), &format!("{e:#}"), raw.as_deref())?;
            Ok(false)
        }
    }
}

/// What `info` converts to now, for the account `me`; one whose conversion panics is an error, as
/// the writer catches nothing, and every commit and ack waits on it.
pub(super) fn converted(info: &WebMessageInfo, me: &Me) -> Result<Vec<Change>> {
    catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        assert_ne!(info.key.id.as_deref(), Some(tests::PANICS), "a conversion that panics, as a test asks");
        convert::message(info, me, false)
    }))
    .map_err(|panic| {
        let why = panic.downcast_ref::<&str>().copied().or(panic.downcast_ref::<String>().map(String::as_str));
        anyhow!("converting it panicked: {}", why.unwrap_or("for no reason it gave"))
    })
}

fn converted_again(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let row: Option<(i64, String, Option<Vec<u8>>, bool)> =
        db.prepare_cached(UNSHOWN)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
    let Some((chat, id, Some(raw), once)) = row else { return Ok(false) };
    let Some(info) = unpack(&raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(false);
    };
    let changes = converted(&info, &account(db)?)?;
    // What it shows, and whether that's an edit standing in for its message.
    let shown = changes.iter().find_map(|change| match change {
        Change::Message(message) if message.id == id => Some((message, false)),
        Change::Edited { shown, .. } if shown.id == id => Some((shown.as_ref(), true)),
        _ => None,
    });
    let changed = match shown {
        Some((message, _)) if unshown(message) => false,
        Some((message, standing_in)) => show(db, stamps, (chat, n, once), message, standing_in)?,
        None => wait(db, stamps, (chat, n, &id), &raw, changes)?,
    };
    #[cfg(test)]
    anyhow::ensure!(id != tests::FAILS, "a conversion that fails once it has written, as a test asks");
    #[cfg(test)]
    if id == tests::FILLS && !tests::FILLED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let full = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL);
        return Err(rusqlite::Error::SqliteFailure(full, Some("the disk filled, as a test asks".into())).into());
    }
    Ok(changed)
}

/// Whether a converted message is one jawd still can't show, as it says with no text.
fn unshown(message: &Message) -> bool {
    message.kind == "unknown" || message.text.is_none() && message.summary == UNKNOWN
}

pub(super) fn account(db: &Connection) -> Result<Me> {
    let me: Option<String> = db.prepare_cached(ME)?.query_row([], |r| r.get(0)).optional()?;
    Ok(Me::new(me.unwrap_or_default().split_whitespace().map(str::to_string)))
}

/// Message `n` in `chat` shows as `message`, with a new version, and so does its chat if it's the
/// chat's newest; its poll's options and its file's copy are written as a message arriving writes
/// them; the chat's unread messages and mentions are counted again, as what the message is may count
/// where it didn't. `once`: the row is marked view-once now. `standing_in`: `message` is an edit
/// standing in for it.
fn show(
    db: &Connection,
    stamps: &mut Stamps,
    (chat, n, once): (i64, i64, bool),
    message: &Message,
    standing_in: bool,
) -> Result<bool> {
    // A copy never makes a view-once message an ordinary one, and an edit brings no mark of its
    // own, but only what can be opened once is one.
    let view_once = (once || message.view_once && !standing_in) && convert::once_kind(message.kind);
    let seq = stamps.seq(MESSAGE, Some(n));
    let changed = db.prepare_cached(SHOWN)?.execute(params![
        n,
        message.kind,
        message.text,
        message.spans,
        message.summary,
        message.reply,
        message.detail,
        message.forwarded,
        message.expires_at,
        view_once,
        message.mentions_me,
        message.edited_at,
        UNKNOWN,
        seq,
        standing_in,
        convert::kinds::view_once(message.kind),
    ])?;
    if changed == 0 {
        return Ok(false);
    }
    if !message.poll.is_empty() {
        polls::options(db, n, &message.poll)?;
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    stamps.expires(message.expires_at);
    // Its file, as a copy that nothing wants until it's shown or asked for, as history's are.
    if let Some(copy) = &message.media {
        media::upsert(db, stamps, n, chat, "sd", copy, None)?;
    }
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute(params![chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    stamps.unsettle(chat);
    Ok(true)
}

/// A row that turns out to change another message: each amendment it keeps for a later jawd waits
/// in `pending` with the row's raw protobuf, once however many copies of it come, and the row goes,
/// one of yours with a tombstone, as its conversion says, so receipts for it don't wait for it. A
/// row that converts to nothing kept is left as it is, so nothing it said is lost.
fn wait(db: &Connection, stamps: &mut Stamps, row: (i64, i64, &str), raw: &[u8], changes: Vec<Change>) -> Result<bool> {
    let (chat, n, id) = row;
    let mut kept = Vec::new();
    let mut own = None;
    for change in changes {
        match change {
            Change::Amend(amend) if matches!(amend.what, Amendment::Keep { .. }) => kept.push(amend),
            Change::Amend(amend) if amend.what == Amendment::DeleteForMe && amend.target == id => own = Some(amend),
            _ => {}
        }
    }
    if kept.is_empty() {
        return Ok(false);
    }
    for mut amend in kept {
        amend.raw = Some(Packed(raw.to_vec()));
        amend_message(db, stamps, &amend)?;
    }
    if let Some(own) = own {
        amend_message(db, stamps, &own)?;
    }
    remove_row(db, stamps, chat, n)?;
    Ok(true)
}

/// Whether what `amend` keeps for a later jawd, with its raw protobuf `raw`, waits already in
/// `chat`: a copy of the same message, as history or a retry brings one again, by its own ID in the
/// raw protobuf, so two different ones from the same second both wait.
pub(super) fn kept_already(db: &Connection, chat: &str, amend: &Amend, raw: &[u8]) -> Result<bool> {
    let Some(id) = own_id(raw) else { return Ok(false) };
    let mut alike = db.prepare_cached(KEPT_ALIKE)?;
    let raws = alike.query_map(params![chat, amend.target, amend.at, amend.what.kind()], |r| r.get::<_, Vec<u8>>(0))?;
    for kept in raws {
        if own_id(&kept?).as_deref() == Some(id.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The ID of the message a raw protobuf is, if it reads.
pub(super) fn own_id(raw: &[u8]) -> Option<String> {
    let plain = unpack(raw).ok()?;
    WebMessageInfo::decode(plain.as_slice()).ok()?.key.id.filter(|id| !id.is_empty())
}

#[cfg(test)]
pub(super) mod tests;
