//! The backfill that converts again the messages jawd couldn't show when they came, now that it
//! knows more kinds: a row of kind unknown, or a system line jawd had no sentence for, takes what
//! its raw protobuf converts to now, and one that turns out to change another message, as an
//! encrypted edit does, leaves the chat to wait in `pending` with its raw protobuf, as it would
//! arriving now. A row's raw protobuf is left as it is.

use anyhow::Result;
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::{CHAT, MESSAGE, Packed, Stamps, amend_message, remove_row, unpack};
use crate::convert::{self, Me, kinds::UNKNOWN};
use crate::store::{Amend, Amendment, Change, Message};

/// The backfill that converts again the messages jawd couldn't show when they came.
pub const KINDS: &str = "kinds";

/// The next messages, each with whether jawd couldn't show it when it came: one of kind unknown,
/// or a system line saying so, as `kinds::UNKNOWN` does, that isn't deleted and has its raw
/// protobuf.
pub(super) const NEXT_UNSHOWN: &str = "SELECT n, NOT deleted AND raw IS NOT NULL
        AND (kind = 'unknown' OR kind = 'system' AND summary = 'jaw can''t show this yet')
    FROM messages WHERE n > ?1 AND n <= ?2 ORDER BY n LIMIT ?3";
const UNSHOWN: &str = "SELECT chat, id, raw FROM messages WHERE n = ?1";
const ME: &str = "SELECT value FROM sync_state WHERE key = 'me'";
/// A message as it converts now. Text a later edit gave it stays, unless jaw couldn't show that
/// either: an edit that came after the one a row stands in for left no raw protobuf of its own.
const SHOWN: &str = "UPDATE messages SET kind = ?2,
        text = iif(edited_at > ?12 AND summary IS NOT ?13, text, ?3),
        spans = iif(edited_at > ?12 AND summary IS NOT ?13, spans, ?4),
        summary = iif(edited_at > ?12 AND summary IS NOT ?13, summary, iif(?5 IS ?3, NULL, ?5)),
        reply = ?6, detail = ?7, forwarded = ?8, expires_at = ?9, view_once = ?10, mentions_me = ?11, v = ?14
    WHERE n = ?1 AND NOT deleted";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";
/// The raw protobufs of what's kept for `target` from the same moment, among them a copy's.
const KEPT_ALIKE: &str = "SELECT raw FROM pending WHERE chat = ?1 AND target = ?2 AND at = ?3 AND kind = ?4";

/// Converts message `n` again, if jawd couldn't show it when it came: it takes what its raw
/// protobuf converts to now, or, if that's an amendment to another message, leaves the chat to wait
/// in `pending`. One that still can't be shown, or whose raw protobuf doesn't read, is left as it is.
pub(super) fn backfilled(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<bool> {
    let row: Option<(i64, String, Vec<u8>)> =
        db.prepare_cached(UNSHOWN)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((chat, id, raw)) = row else { return Ok(false) };
    let Some(info) = unpack(&raw).ok().and_then(|plain| WebMessageInfo::decode(plain.as_slice()).ok()) else {
        return Ok(false);
    };
    let changes = convert::message(&info, &account(db)?, false);
    let shown = changes.iter().find_map(|change| match change {
        Change::Message(message) if message.id == id => Some(message),
        Change::Edited { shown, .. } if shown.id == id => Some(shown.as_ref()),
        _ => None,
    });
    match shown {
        Some(message) if unshown(message) => Ok(false),
        Some(message) => show(db, stamps, chat, n, message),
        None => wait(db, stamps, (chat, n, &id), &raw, changes),
    }
}

/// Whether a converted message is one jawd still can't show.
fn unshown(message: &Message) -> bool {
    message.kind == "unknown" || message.kind == "system" && message.summary == UNKNOWN
}

fn account(db: &Connection) -> Result<Me> {
    let me: Option<String> = db.prepare_cached(ME)?.query_row([], |r| r.get(0)).optional()?;
    Ok(Me::new(me.unwrap_or_default().split_whitespace().map(str::to_string)))
}

/// Message `n` in `chat` shows as `message`, with a new version, and so does its chat if it's the
/// chat's newest; the chat's unread messages and mentions are counted again, as what the message is
/// may count where it didn't.
fn show(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64, message: &Message) -> Result<bool> {
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
        message.view_once,
        message.mentions_me,
        message.edited_at,
        UNKNOWN,
        seq,
    ])?;
    if changed == 0 {
        return Ok(false);
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
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
fn own_id(raw: &[u8]) -> Option<String> {
    let plain = unpack(raw).ok()?;
    WebMessageInfo::decode(plain.as_slice()).ok()?.key.id.filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests;
