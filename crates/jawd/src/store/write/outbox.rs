//! The outbox and read positions: what sending writes and reads. A message you send is written as
//! pending with its outbox row in one transaction, so it shows at once and is sent however jawd
//! stops; the server's answer or any receipt removes the row; a refusal leaves it failed until it's
//! retried by hand, or, when the message may already have gone, unconfirmed until a receipt says
//! so or a day passes. Reading moves the chat's read position forward, unread is counted again from
//! it, and the read receipts owed to WhatsApp are the messages between how far it has been told and
//! that position.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{CHAT, MESSAGE, Message, Stamps, advanced, chat_jid, now_ms, upsert_message};

/// The most unread messages counted, so a count costs the same however many there are.
pub const UNREAD_COUNTED: i64 = 1_000;
/// The most messages one read receipt names; more go in more receipts, oldest first.
pub const RECEIPTS_AT_ONCE: usize = 1_000;
/// The most messages the outbox holds, waiting, unconfirmed or failed; past it a new one is
/// refused, and none already in it is dropped.
pub const OUTBOX_MOST: u64 = 1_000;
/// How long an interface's key for a message it sent is kept.
pub const KEY_KEPT_MS: i64 = 24 * 3600 * 1000;
/// What one step of regrouping may look at, in messages times the group's members: looking at a
/// message may walk all its recipients, at about 1.5 µs each, so a step stays under 2 ms whatever
/// the group's size.
const REGROUP_WORK: i64 = 1_024;
/// The most messages one step looks at, for small groups, where each look's own cost counts.
const REGROUP_AT_ONCE: i64 = 16;
/// How long a step goes on before it stops after the message it's on, on a machine slower than that.
const REGROUP_FOR: Duration = Duration::from_millis(2);
/// How many of a settled message's recipients one step lets go of.
const LET_GO_AT_ONCE: i64 = 128;
/// How many keys past their day one enqueue removes.
const KEYS_PRUNED: i64 = 64;

/// A step for the writer.
#[derive(Clone, Debug)]
pub enum Step {
    /// A message of yours to send: its row, pending, and its outbox row, with `raw`, the
    /// WAWebProtobufsE2E.Message to send, unless the interface's `key` already named one or the
    /// outbox is full. Its chat's draft is cleared, since it's what was sent. What it came to is
    /// put in `written`.
    Enqueue { message: Box<Message>, kind: &'static str, raw: Vec<u8>, key: Option<String>, written: Written },
    /// The server has outbox row `outbox`'s message.
    Sent { outbox: i64 },
    /// An attempt failed with `error`; it's tried again at `next_at`, or, without one, it has
    /// failed until it's retried by hand.
    Failed { outbox: i64, error: String, next_at: Option<i64> },
    /// WhatsApp refused a message that may already have gone; it waits for a receipt until `until`.
    Unconfirmed { outbox: i64, error: String, until: i64 },
    /// No receipt came for a message waiting for one, so it has failed until it's retried by hand.
    NoReceipt { outbox: i64, error: String },
    /// A failed message is queued again by hand, and pending again.
    Retry { outbox: i64 },
    /// You've read `chat` up to message `id`.
    Read { chat: String, id: String },
    /// WhatsApp has been told chat `chat` is read up to the message at (`at`, `n`).
    Told { chat: i64, at: i64, n: i64 },
    /// The next few of your messages that `member`, who left group `chat`, was sent are looked at
    /// again, since they no longer wait for them.
    Regroup { chat: i64, member: i64 },
    /// The next few of the recipients of `message`, which has got as far as it can, are let go of.
    LetGo { message: i64 },
}

/// Where an enqueue says what it came to, for whoever asked for it. The last attempt at its
/// transaction is the one that stands.
#[derive(Clone, Debug, Default)]
pub struct Written(Arc<Mutex<Option<Enqueued>>>);

impl Written {
    pub fn take(&self) -> Option<Enqueued> {
        self.0.lock().expect("an enqueue's result").take()
    }

    fn put(&self, enqueued: Enqueued) {
        *self.0.lock().expect("an enqueue's result") = Some(enqueued);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enqueued {
    /// The message, under its chat's JID as jaw keeps it, and its outbox row, if it's still to send:
    /// the one written now, or the one an earlier request with the same key wrote.
    Queued { message: i64, id: String, chat: String, outbox: Option<i64> },
    /// The outbox holds as many as it may, so nothing was written.
    Full { waiting: u64, failed: u64 },
}

const KEYED: &str = "SELECT m.n, m.id, c.id, o.n FROM send_keys k JOIN messages m ON m.n = k.message
    JOIN chats c ON c.n = m.chat LEFT JOIN outbox o ON o.message = m.n WHERE k.key = ?1";
const KEEP_KEY: &str = "INSERT INTO send_keys (key, message, at) VALUES (?1, ?2, ?3)";
const PRUNE_KEYS: &str =
    "DELETE FROM send_keys WHERE key IN (SELECT key FROM send_keys WHERE at < ?1 ORDER BY at LIMIT ?2)";
const MESSAGE_BY_ID: &str =
    "SELECT m.n, m.chat FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1 AND m.id = ?2";
const ENQUEUE: &str = "INSERT INTO outbox (message, kind, raw, next_at, created_at) VALUES (?1, ?2, ?3, ?4, ?4)
    ON CONFLICT (message) DO NOTHING";
const OUTBOX_OF: &str = "SELECT n FROM outbox WHERE message = ?1";
const CLEAR_DRAFT: &str = "UPDATE chats SET draft = NULL, v = ?2 WHERE n = ?1 AND draft IS NOT NULL";
const OUTBOXED_MESSAGE: &str =
    "SELECT m.n, m.chat, m.status FROM outbox o JOIN messages m ON m.n = o.message WHERE o.n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";
const DEQUEUE: &str = "DELETE FROM outbox WHERE message = ?1";
const SENT: &str = "UPDATE messages SET status = ?2, v = ?3 WHERE n = ?1 AND status IS NOT ?2";
const EXPIRES: &str = "UPDATE messages SET expires_at = ?2 + c.ephemeral * 1000 FROM chats c
    WHERE messages.n = ?1 AND c.n = messages.chat AND c.ephemeral > 0 AND messages.expires_at IS NULL";
const ATTEMPTED: &str = "UPDATE outbox SET attempts = attempts + 1, error = ?2, next_at = coalesce(?3, next_at),
        state = iif(?3 IS NULL, 'failed', 'queued')
    WHERE n = ?1 AND state = 'queued'";
const UNCONFIRMED: &str = "UPDATE outbox SET attempts = attempts + 1, error = ?2, next_at = ?3, state = 'unconfirmed'
    WHERE n = ?1 AND state = 'queued'";
const NO_RECEIPT: &str = "UPDATE outbox SET error = ?2, state = 'failed' WHERE n = ?1 AND state = 'unconfirmed'";
const FAILED: &str = "UPDATE messages SET status = 'failed', v = ?2 WHERE n = ?1";
const REQUEUE: &str =
    "UPDATE outbox SET state = 'queued', attempts = 0, error = NULL, next_at = ?2 WHERE n = ?1 AND state = 'failed'";
const PENDING_AGAIN: &str = "UPDATE messages SET status = 'pending', v = ?2 WHERE n = ?1 AND status = 'failed'";
const READ_TARGET: &str =
    "SELECT c.n, m.at, m.n, c.unread FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1 AND m.id = ?2";
const READ_POSITION: &str = "SELECT at, n FROM reads WHERE chat = ?1";
const MOVE_READ: &str = "UPDATE reads SET at = ?2, n = ?3 WHERE chat = ?1";
const FIRST_READ: &str = "INSERT INTO reads (chat, at, n, told_at, told_n) VALUES (?1, ?2, ?3, ?4, ?5)";
const UNREAD_AFTER: &str =
    "SELECT count(*) FROM (SELECT 1 FROM messages WHERE chat = ?1 AND mine = 0 AND (at, n) > (?2, ?3) LIMIT ?4)";
const SET_UNREAD: &str = "UPDATE chats SET unread = ?2, mentions = iif(?2 = 0, 0, mentions), v = ?3
    WHERE n = ?1 AND (unread IS NOT ?2 OR (?2 = 0 AND mentions <> 0))";
const NEWEST_FROM_OTHERS: &str = "SELECT at, n FROM messages WHERE chat = ?1 AND mine = 0 AND (at, n) <= (?2, ?3)
    ORDER BY at DESC, n DESC LIMIT ?4";
const TOLD: &str = "UPDATE reads SET told_at = ?2, told_n = ?3
    WHERE chat = ?1 AND (?2, ?3) > (told_at, told_n) AND (?2, ?3) <= (at, n)";
const QUEUED: &str = "SELECT o.n, c.id FROM outbox o JOIN messages m ON m.n = o.message JOIN chats c ON c.n = m.chat
    WHERE o.state = 'queued' ORDER BY o.n";
const UNCONFIRMED_ROWS: &str = "SELECT n, next_at FROM outbox WHERE state = 'unconfirmed'";
const WAITING: &str = "SELECT count(*), coalesce(sum(state = 'failed'), 0) FROM outbox";
const OUTBOXED: &str = "SELECT o.n, c.id, m.id, o.raw, o.state, o.attempts, m.status
    FROM outbox o CROSS JOIN messages m ON m.n = o.message JOIN chats c ON c.n = m.chat";
const OWING: &str = "SELECT chat FROM reads WHERE (told_at, told_n) < (at, n)";
const OWED_CHAT: &str = "SELECT c.id, c.kind, r.at, r.n, r.told_at, r.told_n FROM reads r JOIN chats c ON c.n = r.chat
    WHERE r.chat = ?1 AND (r.told_at, r.told_n) < (r.at, r.n)";
const OWED_MESSAGES: &str = "SELECT m.id, j.jid, m.at, m.n FROM messages m LEFT JOIN person_jids j ON j.n = m.sender
    WHERE m.chat = ?1 AND m.mine = 0 AND (m.at, m.n) > (?2, ?3) AND (m.at, m.n) <= (?4, ?5)
    ORDER BY m.at, m.n LIMIT ?6";

const ME: &str = "SELECT value FROM sync_state WHERE key = 'me'";
const RECIPIENTS: &str = "INSERT OR IGNORE INTO recipients (message, member, chat)
    SELECT ?1, g.member, ?2 FROM group_members g JOIN person_jids j ON j.n = g.member
    WHERE g.chat = ?2 AND j.person NOT IN (SELECT p.person FROM json_each(?3) e JOIN person_jids p ON p.jid = e.value)";
const SETTLED: &str =
    "INSERT OR IGNORE INTO settled (message) SELECT ?1 WHERE EXISTS (SELECT 1 FROM recipients WHERE message = ?1)";
const LET_GO: &str = "DELETE FROM recipients WHERE message = ?1 AND member IN
    (SELECT member FROM recipients WHERE message = ?1 ORDER BY member LIMIT ?2)";
const LET_GO_DONE: &str = "DELETE FROM settled WHERE message = ?1";
const SETTLED_ROWS: &str = "SELECT message FROM settled LIMIT ?1";
/// A message's recipients: those it was sent to who are still members of group ?2, you apart, in
/// case it was sent before jaw knew your JIDs, ?3.
macro_rules! sent_to {
    () => {
        "FROM recipients s JOIN group_members g ON g.chat = ?2 AND g.member = s.member
        JOIN person_jids mj ON mj.n = s.member WHERE s.message = ?1
        AND mj.person NOT IN (SELECT p.person FROM json_each(?3) e JOIN person_jids p ON p.jid = e.value)"
    };
}
/// Everyone in group ?2 now but you, whose JIDs are ?3, for a message jaw didn't send.
macro_rules! members_now {
    () => {
        "FROM group_members g JOIN person_jids mj ON mj.n = g.member
        WHERE g.chat = ?2 AND mj.person NOT IN (SELECT p.person FROM json_each(?3) e JOIN person_jids p ON p.jid = e.value)"
    };
}
/// Whether any of the recipients `$from` names hasn't got as far as `$reached` with message ?1,
/// under any of their JIDs, so a receipt from someone's LID counts for their number.
macro_rules! laggard {
    ($from:expr, $reached:literal) => {
        concat!(
            "SELECT EXISTS (SELECT 1 ",
            $from,
            " AND NOT EXISTS (SELECT 1 FROM person_jids xj CROSS JOIN receipts x ON x.message = ?1 AND x.recipient = xj.n
                WHERE xj.person = mj.person AND ",
            $reached,
            "))"
        )
    };
}
const ANY_SENT_TO: &str = concat!("SELECT EXISTS (SELECT 1 ", sent_to!(), ")");
const ANY_MEMBER: &str = concat!("SELECT EXISTS (SELECT 1 ", members_now!(), ")");
/// The statements that find someone not yet as far as each status, for those a message was sent to
/// and for everyone in the group now.
const LAGGARDS: [(&str, [&str; 2]); 3] = [
    ("played", [laggard!(sent_to!(), "x.played_at IS NOT NULL"), laggard!(members_now!(), "x.played_at IS NOT NULL")]),
    (
        "read",
        [
            laggard!(sent_to!(), "coalesce(x.read_at, x.played_at) IS NOT NULL"),
            laggard!(members_now!(), "coalesce(x.read_at, x.played_at) IS NOT NULL"),
        ],
    ),
    (
        "delivered",
        [
            laggard!(sent_to!(), "coalesce(x.delivered_at, x.read_at, x.played_at) IS NOT NULL"),
            laggard!(members_now!(), "coalesce(x.delivered_at, x.read_at, x.played_at) IS NOT NULL"),
        ],
    ),
];
const LEFT: &str =
    "INSERT INTO regroup (chat, member) VALUES (?1, ?2) ON CONFLICT (chat, member) DO UPDATE SET after = 0";
const REGROUP_FROM: &str = "SELECT after FROM regroup WHERE chat = ?1 AND member = ?2";
const GROUP_SIZE: &str = "SELECT count(*) FROM group_members WHERE chat = ?1";
const SENT_TO_MEMBER: &str =
    "SELECT message FROM recipients WHERE chat = ?1 AND member = ?2 AND message > ?3 ORDER BY message LIMIT ?4";
const REGROUP_ON: &str = "UPDATE regroup SET after = ?3 WHERE chat = ?1 AND member = ?2";
const REGROUP_DONE: &str = "DELETE FROM regroup WHERE chat = ?1 AND member = ?2";
const REGROUPS: &str = "SELECT chat, member FROM regroup LIMIT ?1";
const MINE_IN: &str = "SELECT status, kind FROM messages WHERE n = ?1 AND chat = ?2 AND mine";
const REGROUPED: &str = "UPDATE messages SET status = ?2, v = ?3 WHERE n = ?1";

/// Where a step that failed came from, for the quarantine.
pub(super) fn provenance(step: &Step) -> (&'static str, String, Option<&[u8]>) {
    match step {
        Step::Enqueue { message, .. } => {
            ("send", format!("{} {}", message.chat, message.id), message.raw.as_ref().map(|p| p.0.as_slice()))
        }
        Step::Sent { outbox }
        | Step::Failed { outbox, .. }
        | Step::Unconfirmed { outbox, .. }
        | Step::NoReceipt { outbox, .. }
        | Step::Retry { outbox } => ("outbox", outbox.to_string(), None),
        Step::Read { chat, id } => ("read", format!("{chat} {id}"), None),
        Step::Told { chat, n, .. } => ("told", format!("{chat} {n}"), None),
        Step::Regroup { chat, member } => ("regroup", format!("{chat} {member}"), None),
        Step::LetGo { message } => ("let go", message.to_string(), None),
    }
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, step: &Step) -> Result<bool> {
    match step {
        Step::Enqueue { message, kind, raw, key, written } => enqueue(db, stamps, message, kind, raw, key, written),
        Step::Sent { outbox } => sent(db, stamps, *outbox),
        Step::Failed { outbox, error, next_at } => failed(db, stamps, *outbox, error, *next_at),
        Step::Unconfirmed { outbox, error, until } => {
            db.prepare_cached(UNCONFIRMED)?.execute(params![outbox, error, until])?;
            Ok(false)
        }
        Step::NoReceipt { outbox, error } => no_receipt(db, stamps, *outbox, error),
        Step::Retry { outbox } => retry(db, stamps, *outbox),
        Step::Read { chat, id } => read(db, stamps, chat, id),
        Step::Told { chat, at, n } => {
            db.prepare_cached(TOLD)?.execute(params![chat, at, n])?;
            Ok(false)
        }
        Step::Regroup { chat, member } => regroup(db, stamps, *chat, *member),
        Step::LetGo { message } => {
            if db.prepare_cached(LET_GO)?.execute(params![message, LET_GO_AT_ONCE])? < LET_GO_AT_ONCE as usize {
                db.prepare_cached(LET_GO_DONE)?.execute([message])?;
            }
            Ok(false)
        }
    }
}

fn enqueue(
    db: &Connection,
    stamps: &mut Stamps,
    message: &Message,
    kind: &str,
    raw: &[u8],
    key: &Option<String>,
    written: &Written,
) -> Result<bool> {
    if let Some(key) = key {
        let keyed: Option<(i64, String, String, Option<i64>)> = db
            .prepare_cached(KEYED)?
            .query_row([key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .optional()?;
        if let Some((message, id, chat, outbox)) = keyed {
            written.put(Enqueued::Queued { message, id, chat, outbox });
            return Ok(false);
        }
    }
    let (waiting, failed) = waiting(db)?;
    if waiting >= OUTBOX_MOST {
        written.put(Enqueued::Full { waiting, failed });
        return Ok(false);
    }
    let chat = chat_jid(db, &message.chat)?;
    let resolved;
    let message = if chat == message.chat {
        message
    } else {
        resolved = Message { chat: chat.clone(), ..message.clone() };
        &resolved
    };
    let changed = upsert_message(db, stamps, message, false)?;
    let (n, chat_n): (i64, i64) = db
        .prepare_cached(MESSAGE_BY_ID)?
        .query_row(params![message.chat, message.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let now = now_ms();
    db.prepare_cached(ENQUEUE)?.execute(params![n, kind, raw, now])?;
    let outbox: i64 = db.prepare_cached(OUTBOX_OF)?.query_row([n], |r| r.get(0))?;
    db.prepare_cached(RECIPIENTS)?.execute(params![n, chat_n, me(db)?])?;
    db.prepare_cached(PRUNE_KEYS)?.execute(params![now - KEY_KEPT_MS, KEYS_PRUNED])?;
    if let Some(key) = key {
        db.prepare_cached(KEEP_KEY)?.execute(params![key, n, now])?;
    }
    written.put(Enqueued::Queued { message: n, id: message.id.clone(), chat, outbox: Some(outbox) });
    let seq = stamps.seq(CHAT, Some(chat_n));
    if db.prepare_cached(CLEAR_DRAFT)?.execute([chat_n, seq])? > 0 {
        stamps.record(db, CHAT, chat_n, Some(chat_n), seq)?;
        return Ok(true);
    }
    Ok(changed)
}

/// An outbox row's message: its n, its chat's n and its status.
fn outboxed(db: &Connection, outbox: i64) -> Result<Option<(i64, i64, Option<String>)>> {
    Ok(db
        .prepare_cached(OUTBOXED_MESSAGE)?
        .query_row([outbox], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?)
}

/// Writes the events for message `n` in chat `chat` having changed with `seq`, and for its chat
/// when it's the chat's newest, whose line in the chat list shows its status.
fn touched(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64, seq: i64) -> Result<()> {
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CHAT_SHOWS)?.execute([chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(())
}

/// The message's time stays when you sent it, as the phone shows it, rather than the server's,
/// which would move it among the messages around it and away from where search keeps it.
fn sent(db: &Connection, stamps: &mut Stamps, outbox: i64) -> Result<bool> {
    let Some((n, chat, status)) = outboxed(db, outbox)? else { return Ok(false) };
    received(db, n)?;
    let status = advanced(status.as_deref(), Some("sent"));
    let seq = stamps.seq(MESSAGE, Some(n));
    if db.prepare_cached(SENT)?.execute(params![n, status, seq])? == 0 {
        return Ok(false);
    }
    touched(db, stamps, chat, n, seq)?;
    Ok(true)
}

/// The server has message `n` of yours, as its answer or any receipt says, whichever comes first:
/// nothing of it is left to send, and in a chat with disappearing messages its timer starts, since
/// it can wait to go for longer than the timer.
pub(super) fn received(db: &Connection, n: i64) -> Result<()> {
    if db.prepare_cached(DEQUEUE)?.execute([n])? > 0 {
        db.prepare_cached(EXPIRES)?.execute(params![n, now_ms()])?;
    }
    Ok(())
}

/// Once a message of yours has got as far as it can (read, or for what's played, played), who it
/// was sent to no longer matters, and they're let go of in the background. Until they all are, the
/// ones left can't move its status, which is as far as it goes.
pub(super) fn settled(db: &Connection, n: i64, kind: &str, status: &str) -> Result<()> {
    let played = matches!(kind, "voice" | "audio" | "video" | "video_note");
    if status == "played" || (status == "read" && !played) {
        db.prepare_cached(SETTLED)?.execute([n])?;
    }
    Ok(())
}

/// Some of the messages whose recipients are still to be let go of.
pub fn settled_messages(db: &Connection, most: usize) -> Result<Vec<i64>> {
    Ok(db.prepare_cached(SETTLED_ROWS)?.query_map([most as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// An attempt at a queued message failed; one that's still queued after it is still pending.
fn failed(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str, next_at: Option<i64>) -> Result<bool> {
    let Some((n, chat, status)) = outboxed(db, outbox)? else { return Ok(false) };
    if db.prepare_cached(ATTEMPTED)?.execute(params![outbox, error, next_at])? == 0 || next_at.is_some() {
        return Ok(false);
    }
    shown_failed(db, stamps, chat, n, status.as_deref())
}

/// A message waiting for a receipt had none in time; one that had is no longer in the outbox, or
/// no longer waiting.
fn no_receipt(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str) -> Result<bool> {
    let Some((n, chat, status)) = outboxed(db, outbox)? else { return Ok(false) };
    if db.prepare_cached(NO_RECEIPT)?.execute(params![outbox, error])? == 0 {
        return Ok(false);
    }
    shown_failed(db, stamps, chat, n, status.as_deref())
}

/// Message `n`, at `status`, shows failed, unless it has got further.
fn shown_failed(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64, status: Option<&str>) -> Result<bool> {
    if advanced(status, Some("failed")) == status {
        return Ok(false);
    }
    let seq = stamps.seq(MESSAGE, Some(n));
    db.prepare_cached(FAILED)?.execute([n, seq])?;
    touched(db, stamps, chat, n, seq)?;
    Ok(true)
}

/// A manual retry is the one way a status goes back: from failed, which the server never had, to
/// pending.
fn retry(db: &Connection, stamps: &mut Stamps, outbox: i64) -> Result<bool> {
    let Some((n, chat, _)) = outboxed(db, outbox)? else { return Ok(false) };
    let queued = db.prepare_cached(REQUEUE)?.execute(params![outbox, now_ms()])?;
    let seq = stamps.seq(MESSAGE, Some(n));
    let pending = db.prepare_cached(PENDING_AGAIN)?.execute([n, seq])?;
    if pending > 0 {
        touched(db, stamps, chat, n, seq)?;
    }
    Ok(queued + pending > 0)
}

/// Moves `chat`'s read position forward to message `id` and counts its unread messages again from
/// there. A chat read here for the first time owes WhatsApp receipts for as many of its newest
/// messages from others as it had unread. Marked unread is the phone's to clear, in app state.
fn read(db: &Connection, stamps: &mut Stamps, chat: &str, id: &str) -> Result<bool> {
    let chat = chat_jid(db, chat)?;
    let found: Option<(i64, i64, i64, i64)> = db
        .prepare_cached(READ_TARGET)?
        .query_row(params![chat, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    let Some((chat, at, n, unread)) = found else { return Ok(false) };
    let was: Option<(i64, i64)> =
        db.prepare_cached(READ_POSITION)?.query_row([chat], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    match was {
        Some(was) if was >= (at, n) => return Ok(false),
        Some(_) => {
            db.prepare_cached(MOVE_READ)?.execute(params![chat, at, n])?;
        }
        None => {
            let told = told_before(db, chat, (at, n), unread)?;
            db.prepare_cached(FIRST_READ)?.execute(params![chat, at, n, told.0, told.1])?;
        }
    }
    let unread: i64 = db.prepare_cached(UNREAD_AFTER)?.query_row(params![chat, at, n, UNREAD_COUNTED], |r| r.get(0))?;
    let seq = stamps.seq(CHAT, Some(chat));
    let changed = db.prepare_cached(SET_UNREAD)?.execute(params![chat, unread, seq])?;
    if changed > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(changed > 0)
}

/// The place before the newest `unread` messages from others up to `upto`, which WhatsApp is taken
/// to have been told of; the start of the chat if it has no more than that.
fn told_before(db: &Connection, chat: i64, upto: (i64, i64), unread: i64) -> Result<(i64, i64)> {
    if unread <= 0 {
        return Ok(upto);
    }
    let unread = unread.min(UNREAD_COUNTED);
    let walked: Vec<(i64, i64)> = db
        .prepare_cached(NEWEST_FROM_OTHERS)?
        .query_map(params![chat, upto.0, upto.1, unread + 1], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(walked.get(unread as usize).copied().unwrap_or((i64::MIN, 0)))
}

/// The account's own JIDs as sync keeps them, as a JSON array.
fn me(db: &Connection) -> Result<String> {
    let me: Option<String> = db.prepare_cached(ME)?.query_row([], |r| r.get(0)).optional()?;
    Ok(serde_json::to_string(&me.as_deref().unwrap_or_default().split_whitespace().collect::<Vec<_>>())?)
}

/// How far message `n` of yours in group `chat`, at `kept` now, has got with all its recipients:
/// those it was sent to who are still members, or, for a message jaw didn't send, everyone in the
/// group now but you. `None` when jaw doesn't know who they are, and then a receipt from anyone
/// stands for all. Each status is looked for only if it's further than `kept`, and the look stops at
/// the first recipient not that far, so a receipt in a big group costs little until it's the last.
pub(super) fn group_reached(
    db: &Connection,
    n: i64,
    chat: i64,
    kept: Option<&str>,
) -> Result<Option<Option<&'static str>>> {
    let me = me(db)?;
    let exists =
        |sql: &str| -> Result<bool> { Ok(db.prepare_cached(sql)?.query_row(params![n, chat, me], |r| r.get(0))?) };
    let source = if exists(ANY_SENT_TO)? {
        0
    } else if exists(ANY_MEMBER)? {
        1
    } else {
        return Ok(None);
    };
    for (status, laggards) in LAGGARDS {
        if advanced(kept, Some(status)) == kept {
            break;
        }
        if !exists(laggards[source])? {
            return Ok(Some(Some(status)));
        }
    }
    Ok(Some(None))
}

/// Members who left group `chat` no longer hold up your messages they were sent there; they're
/// looked at again in the background, so a big change stays a short transaction.
pub(super) fn members_left(db: &Connection, chat: i64, members: &[i64]) -> Result<()> {
    for &member in members {
        db.prepare_cached(LEFT)?.execute([chat, member])?;
    }
    Ok(())
}

/// Looks again at the next few of your messages in `chat` that `member` was sent, as many as
/// REGROUP_WORK allows in a group that size, and stops sooner once REGROUP_FOR has passed.
fn regroup(db: &Connection, stamps: &mut Stamps, chat: i64, member: i64) -> Result<bool> {
    let began = Instant::now();
    let Some(after): Option<i64> =
        db.prepare_cached(REGROUP_FROM)?.query_row([chat, member], |r| r.get(0)).optional()?
    else {
        return Ok(false);
    };
    let members: i64 = db.prepare_cached(GROUP_SIZE)?.query_row([chat], |r| r.get(0))?;
    let most = (REGROUP_WORK / members.max(1)).clamp(1, REGROUP_AT_ONCE);
    let messages: Vec<i64> = db
        .prepare_cached(SENT_TO_MEMBER)?
        .query_map(params![chat, member, after, most], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut changed = false;
    let mut last = None;
    for &n in &messages {
        if last.is_some() && began.elapsed() >= REGROUP_FOR {
            break;
        }
        last = Some(n);
        let Some((kept, kind)): Option<(Option<String>, String)> =
            db.prepare_cached(MINE_IN)?.query_row([n, chat], |r| Ok((r.get(0)?, r.get(1)?))).optional()?
        else {
            continue;
        };
        let Some(Some(reached)) = group_reached(db, n, chat, kept.as_deref())? else { continue };
        if advanced(kept.as_deref(), Some(reached)) != kept.as_deref() {
            let seq = stamps.seq(MESSAGE, Some(n));
            db.prepare_cached(REGROUPED)?.execute(params![n, reached, seq])?;
            touched(db, stamps, chat, n, seq)?;
            settled(db, n, &kind, reached)?;
            changed = true;
        }
    }
    match last {
        Some(last) if messages.last() != Some(&last) || messages.len() as i64 == most => {
            db.prepare_cached(REGROUP_ON)?.execute([chat, member, last])?;
        }
        _ => {
            db.prepare_cached(REGROUP_DONE)?.execute([chat, member])?;
        }
    }
    Ok(changed)
}

/// Some of the members whose leaving is still to be looked at, as `(chat, member)`.
pub fn regroups(db: &Connection, most: usize) -> Result<Vec<(i64, i64)>> {
    Ok(db
        .prepare_cached(REGROUPS)?
        .query_map([most as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// An outbox row as the sender takes it.
#[derive(Clone, Debug)]
pub struct Outboxed {
    pub outbox: i64,
    pub chat: String,
    pub id: String,
    pub raw: Vec<u8>,
    /// `queued`, `unconfirmed` or `failed`.
    pub state: String,
    /// The attempts that failed, each of which reached the engine.
    pub attempts: i64,
    pub status: Option<String>,
}

/// The rows waiting to be sent, oldest first, as `(outbox, chat)`.
pub fn queued(db: &Connection) -> Result<Vec<(i64, String)>> {
    Ok(db.prepare_cached(QUEUED)?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
}

/// The rows waiting for a receipt, as `(outbox, until)`.
pub fn unconfirmed(db: &Connection) -> Result<Vec<(i64, i64)>> {
    Ok(db
        .prepare_cached(UNCONFIRMED_ROWS)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// How many messages the outbox holds, and how many of them failed and wait for `send.retry`.
pub fn waiting(db: &Connection) -> Result<(u64, u64)> {
    let (all, failed): (i64, i64) = db.prepare_cached(WAITING)?.query_row([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok((all.max(0) as u64, failed.max(0) as u64))
}

fn outboxed_row(r: &rusqlite::Row) -> rusqlite::Result<Outboxed> {
    Ok(Outboxed {
        outbox: r.get(0)?,
        chat: r.get(1)?,
        id: r.get(2)?,
        raw: r.get(3)?,
        state: r.get(4)?,
        attempts: r.get(5)?,
        status: r.get(6)?,
    })
}

/// Outbox row `outbox`, if it's still there.
pub fn outboxed_by_n(db: &Connection, outbox: i64) -> Result<Option<Outboxed>> {
    Ok(db.prepare_cached(&format!("{OUTBOXED} WHERE o.n = ?1"))?.query_row([outbox], outboxed_row).optional()?)
}

/// The outbox row of your message `id`, if it has one. The outbox is read first, since message IDs
/// are unique only within a chat.
pub fn outboxed_by_id(db: &Connection, id: &str) -> Result<Option<Outboxed>> {
    Ok(db.prepare_cached(&format!("{OUTBOXED} WHERE m.id = ?1"))?.query_row([id], outboxed_row).optional()?)
}

/// The message an interface's key names, if it was sent in the last day, with its chat's JID.
pub fn keyed(db: &Connection, key: &str) -> Result<Option<(String, String)>> {
    Ok(db.prepare_cached(KEYED)?.query_row([key], |r| Ok((r.get(2)?, r.get(1)?))).optional()?)
}

/// Read receipts owed in one chat: its JID, how far this lot goes, and the messages from others
/// between how far WhatsApp has been told and there, oldest first, each with who sent it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owed {
    pub chat: i64,
    pub jid: String,
    pub group: bool,
    pub upto: (i64, i64),
    pub messages: Vec<(String, Option<String>)>,
}

/// The chats whose read receipts WhatsApp hasn't all had.
pub fn owing(db: &Connection) -> Result<Vec<i64>> {
    Ok(db.prepare_cached(OWING)?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// The next lot of what chat `chat` owes WhatsApp, if anything: up to RECEIPTS_AT_ONCE of the
/// oldest messages owed, going as far as the last of them, or to the read position if that's all.
pub fn owed(db: &Connection, chat: i64) -> Result<Option<Owed>> {
    let found: Option<(String, String, i64, i64, i64, i64)> = db
        .prepare_cached(OWED_CHAT)?
        .query_row([chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .optional()?;
    let Some((jid, kind, at, n, told_at, told_n)) = found else { return Ok(None) };
    let rows: Vec<(String, Option<String>, i64, i64)> = db
        .prepare_cached(OWED_MESSAGES)?
        .query_map(params![chat, told_at, told_n, at, n, RECEIPTS_AT_ONCE as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let upto = match rows.last() {
        Some(&(_, _, at, n)) if rows.len() == RECEIPTS_AT_ONCE => (at, n),
        _ => (at, n),
    };
    let messages = rows.into_iter().map(|(id, sender, _, _)| (id, sender)).collect();
    Ok(Some(Owed { chat, jid, group: kind != "direct", upto, messages }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema;

    /// Every statement sending runs, as SQLite plans it on a fresh database with no statistics, as
    /// jawd's always is: each reads an index, never scans a big table and never sorts in a
    /// temporary b-tree. The outbox, reads and regroup are small (a row per message waiting, per
    /// chat read and per member still to look at again) and may be read whole.
    #[test]
    fn every_statement_sending_runs_reads_an_index_and_sorts_nothing() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let by_id = format!("{OUTBOXED} WHERE m.id = ?1");
        let by_n = format!("{OUTBOXED} WHERE o.n = ?1");
        let statements = [
            (
                "message by id",
                MESSAGE_BY_ID,
                "SEARCH c USING COVERING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING COVERING INDEX messages_by_id (chat=? AND id=?)",
            ),
            ("enqueue", ENQUEUE, ""),
            ("clear the draft", CLEAR_DRAFT, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "an outbox row's message",
                OUTBOXED_MESSAGE,
                "SEARCH o USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("the chat shows it", CHAT_SHOWS, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
            ("dequeue", DEQUEUE, "SEARCH outbox USING INDEX sqlite_autoindex_outbox_1 (message=?)"),
            ("sent", SENT, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "expires",
                EXPIRES,
                "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("attempted", ATTEMPTED, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
            ("failed", FAILED, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            ("queued again", REQUEUE, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
            ("pending again", PENDING_AGAIN, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "what's read",
                READ_TARGET,
                "SEARCH c USING INDEX sqlite_autoindex_chats_1 (id=?)\nSEARCH m USING INDEX messages_by_id (chat=? AND id=?)",
            ),
            ("the read position", READ_POSITION, "SEARCH reads USING INTEGER PRIMARY KEY (rowid=?)"),
            ("read further", MOVE_READ, "SEARCH reads USING INTEGER PRIMARY KEY (rowid=?)"),
            ("read first", FIRST_READ, ""),
            (
                "unread after it",
                UNREAD_AFTER,
                "CO-ROUTINE (subquery-1)\nSEARCH messages USING INDEX messages_by_time (chat=? AND at>?)\nSCAN (subquery-1)",
            ),
            ("unread", SET_UNREAD, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "the newest from others",
                NEWEST_FROM_OTHERS,
                "SEARCH messages USING INDEX messages_by_time (chat=? AND at<?)",
            ),
            ("told", TOLD, "SEARCH reads USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "queued",
                QUEUED,
                "SCAN o\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("waiting", WAITING, "SCAN outbox"),
            (
                "outboxed by id",
                &by_id,
                "SCAN o\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            (
                "outboxed by n",
                &by_n,
                "SEARCH o USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("owing", OWING, "SCAN reads"),
            (
                "owed by a chat",
                OWED_CHAT,
                "SEARCH r USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            (
                "owed messages",
                OWED_MESSAGES,
                "SEARCH m USING INDEX messages_by_time (chat=? AND at>? AND at<?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
            ),
            ("me", ME, "SEARCH sync_state USING PRIMARY KEY (key=?)"),
            (
                "recipients",
                RECIPIENTS,
                "SEARCH g USING PRIMARY KEY (chat=?)\nSEARCH j USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER",
            ),
            (
                "any sent to",
                ANY_SENT_TO,
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 2\nSEARCH s USING PRIMARY KEY (message=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER\nSEARCH g USING COVERING INDEX group_members_by_member (member=? AND chat=?)",
            ),
            (
                "any member",
                ANY_MEMBER,
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 2\nSEARCH g USING PRIMARY KEY (chat=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER",
            ),
            (
                "not delivered to one sent to",
                LAGGARDS[2].1[0],
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 3\nSEARCH s USING PRIMARY KEY (message=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER\nCORRELATED SCALAR SUBQUERY 2\nSEARCH xj USING COVERING INDEX person_jids_by_person (person=?)\nSEARCH x USING PRIMARY KEY (message=? AND recipient=?)\nSEARCH g USING COVERING INDEX group_members_by_member (member=? AND chat=?)",
            ),
            (
                "not delivered to a member",
                LAGGARDS[2].1[1],
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 3\nSEARCH g USING PRIMARY KEY (chat=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER\nCORRELATED SCALAR SUBQUERY 2\nSEARCH xj USING COVERING INDEX person_jids_by_person (person=?)\nSEARCH x USING PRIMARY KEY (message=? AND recipient=?)",
            ),
            (
                "not read by one sent to",
                LAGGARDS[1].1[0],
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 3\nSEARCH s USING PRIMARY KEY (message=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER\nCORRELATED SCALAR SUBQUERY 2\nSEARCH xj USING COVERING INDEX person_jids_by_person (person=?)\nSEARCH x USING PRIMARY KEY (message=? AND recipient=?)\nSEARCH g USING COVERING INDEX group_members_by_member (member=? AND chat=?)",
            ),
            (
                "not played by a member",
                LAGGARDS[0].1[1],
                "SCAN CONSTANT ROW\nSCALAR SUBQUERY 3\nSEARCH g USING PRIMARY KEY (chat=?)\nSEARCH mj USING INTEGER PRIMARY KEY (rowid=?)\nLIST SUBQUERY 1\nSCAN e VIRTUAL TABLE INDEX 1:\nSEARCH p USING INDEX sqlite_autoindex_person_jids_1 (jid=?)\nCREATE BLOOM FILTER\nCORRELATED SCALAR SUBQUERY 2\nSEARCH xj USING COVERING INDEX person_jids_by_person (person=?)\nSEARCH x USING PRIMARY KEY (message=? AND recipient=?)",
            ),
            (
                "sent to a member",
                SENT_TO_MEMBER,
                "SEARCH recipients USING COVERING INDEX recipients_by_member (chat=? AND member=? AND message>?)",
            ),
            (
                "keyed",
                KEYED,
                "SEARCH k USING PRIMARY KEY (key=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH o USING COVERING INDEX sqlite_autoindex_outbox_1 (message=?) LEFT-JOIN",
            ),
            ("keep a key", KEEP_KEY, ""),
            (
                "prune keys",
                PRUNE_KEYS,
                "SEARCH send_keys USING PRIMARY KEY (key=?)\nLIST SUBQUERY 1\nSEARCH send_keys USING COVERING INDEX send_keys_by_at (at<?)",
            ),
            (
                "outbox of a message",
                OUTBOX_OF,
                "SEARCH outbox USING COVERING INDEX sqlite_autoindex_outbox_1 (message=?)",
            ),
            ("unconfirmed", UNCONFIRMED, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
            ("no receipt", NO_RECEIPT, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
            ("unconfirmed rows", UNCONFIRMED_ROWS, "SCAN outbox"),
            ("settled", SETTLED, "SEARCH recipients EXISTS USING PRIMARY KEY (message=?)"),
            (
                "let go",
                LET_GO,
                "SEARCH recipients USING PRIMARY KEY (message=? AND member=?)\nLIST SUBQUERY 1\nSEARCH recipients USING PRIMARY KEY (message=?)",
            ),
            ("let go of all", LET_GO_DONE, "SEARCH settled USING INTEGER PRIMARY KEY (rowid=?)"),
            ("settled rows", SETTLED_ROWS, "SCAN settled"),
            ("left", LEFT, ""),
            ("regroup from", REGROUP_FROM, "SEARCH regroup USING PRIMARY KEY (chat=? AND member=?)"),
            ("a group's size", GROUP_SIZE, "SEARCH group_members USING PRIMARY KEY (chat=?)"),
            ("regroup on", REGROUP_ON, "SEARCH regroup USING PRIMARY KEY (chat=? AND member=?)"),
            ("regroup done", REGROUP_DONE, "SEARCH regroup USING PRIMARY KEY (chat=? AND member=?)"),
            ("regroups", REGROUPS, "SCAN regroup"),
            ("mine in a chat", MINE_IN, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            ("regrouped", REGROUPED, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
        ];
        let mut wrong = Vec::new();
        for (what, sql, expected) in statements {
            let mut explain = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let count = explain.parameter_count();
            let plan: Vec<String> = explain
                .query_map(rusqlite::params_from_iter(std::iter::repeat_n(0, count)), |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            let plan = plan.join("\n");
            if plan != expected {
                wrong.push(format!("{what}:\n{plan}\n(expected)\n{expected}"));
            }
        }
        assert!(wrong.is_empty(), "plans changed:\n\n{}", wrong.join("\n\n"));
    }

    #[test]
    fn migration_6_keeps_an_outbox_row_beside_its_message_and_refuses_one_with_none() {
        let at_5 = |orphan: bool| {
            let mut db = Connection::open_in_memory().unwrap();
            let tx = db.transaction().unwrap();
            for migration in &schema::MIGRATIONS[..5] {
                tx.execute_batch(migration).unwrap();
            }
            tx.execute("INSERT INTO chats (n, id, kind, v) VALUES (1, 'a@s.whatsapp.net', 'direct', 1)", []).unwrap();
            tx.execute("INSERT INTO messages (n, chat, id, at, t, kind, v) VALUES (7, 1, 'm1', 5, 5, 'text', 1)", [])
                .unwrap();
            tx.execute(
                "INSERT INTO outbox (id, chat, kind, raw, next_at, created_at) VALUES ('m1', 'a@s.whatsapp.net', 'text', x'01', 9, 9)",
                [],
            )
            .unwrap();
            if orphan {
                tx.execute(
                    "INSERT INTO outbox (id, chat, kind, raw, next_at, created_at) VALUES ('gone', 'a@s.whatsapp.net', 'text', x'02', 9, 9)",
                    [],
                )
                .unwrap();
            }
            tx.pragma_update(None, "user_version", 5).unwrap();
            tx.commit().unwrap();
            db
        };
        let mut db = at_5(false);
        schema::migrate(&mut db).unwrap();
        let kept: (i64, Vec<u8>, String) = db
            .query_row("SELECT message, raw, state FROM outbox", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(kept, (7, vec![1], "queued".into()));
        let mut db = at_5(true);
        assert!(schema::migrate(&mut db).is_err(), "an outbox row naming no message isn't dropped");
        let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        let rows: i64 = db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0)).unwrap();
        assert_eq!((version, rows), (5, 2), "and the database is as it was");
    }

    fn mine(id: &str, at: i64) -> Message {
        Message {
            chat: "a@s.whatsapp.net".into(),
            id: id.into(),
            at,
            mine: true,
            kind: "text",
            status: Some("pending"),
            text: Some("hi".into()),
            summary: "hi".into(),
            ..Message::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_status_only_moves_forward_and_only_a_retry_takes_failed_back_to_pending() {
        use crate::store::{Amend, Amendment, Change, Lane, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let apply = |step: Step| store.apply(Lane::Live, vec![Change::Outbox(step)]);
        let enqueue = |id: &str| Step::Enqueue {
            message: Box::new(mine(id, 1_000)),
            kind: "text",
            raw: vec![1],
            key: None,
            written: Written::default(),
        };
        apply(enqueue("m1")).await.unwrap();
        let again = apply(enqueue("m1")).await.unwrap();
        assert_eq!(again.changed, 0, "enqueuing the same message again changes nothing");
        let state = |id: &'static str| {
            store.reads().query(crate::work::Priority::Interactive, move |db| {
                Ok(db.query_row(
                    "SELECT m.status, m.at, (SELECT o.n || ' ' || o.state || ' ' || o.attempts FROM outbox o WHERE o.message = m.n)
                    FROM messages m WHERE m.id = ?1",
                    [id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?)),
                )?)
            })
        };
        assert_eq!(state("m1").await.unwrap(), ("pending".into(), 1_000, Some("1 queued 0".into())));
        apply(Step::Failed { outbox: 1, error: "not connected".into(), next_at: Some(5) }).await.unwrap();
        assert_eq!(state("m1").await.unwrap(), ("pending".into(), 1_000, Some("1 queued 1".into())));
        let receipt = Amend {
            chat: "a@s.whatsapp.net".into(),
            target: "m1".into(),
            sender: Some("a@s.whatsapp.net".into()),
            mine: false,
            at: 2_000,
            what: Amendment::Status { status: "delivered".into() },
            raw: None,
        };
        store.apply(Lane::Live, vec![Change::Amend(receipt.clone())]).await.unwrap();
        assert_eq!(
            state("m1").await.unwrap(),
            ("delivered".into(), 1_000, None),
            "a receipt takes the outbox row with it"
        );
        apply(Step::Sent { outbox: 1 }).await.unwrap();
        assert_eq!(
            state("m1").await.unwrap(),
            ("delivered".into(), 1_000, None),
            "sent after delivered changes nothing"
        );
        let late = apply(Step::Failed { outbox: 1, error: "late".into(), next_at: None }).await.unwrap();
        assert_eq!(late.changed, 0);
        apply(enqueue("m2")).await.unwrap();
        let (_, _, queued) = state("m2").await.unwrap();
        let outbox: i64 = queued.unwrap().split(' ').next().unwrap().parse().unwrap();
        apply(Step::Failed { outbox, error: "gone".into(), next_at: None }).await.unwrap();
        assert_eq!(state("m2").await.unwrap(), ("failed".into(), 1_000, Some(format!("{outbox} failed 1"))));
        apply(Step::Retry { outbox }).await.unwrap();
        assert_eq!(state("m2").await.unwrap(), ("pending".into(), 1_000, Some(format!("{outbox} queued 0"))));
        apply(Step::Sent { outbox }).await.unwrap();
        let retried = apply(Step::Retry { outbox }).await.unwrap();
        assert_eq!((state("m2").await.unwrap().0.as_str(), retried.changed), ("sent", 0), "nothing takes sent back");
        apply(enqueue("m3")).await.unwrap();
        let (_, _, queued) = state("m3").await.unwrap();
        let outbox: i64 = queued.unwrap().split(' ').next().unwrap().parse().unwrap();
        apply(Step::Failed { outbox, error: "refused".into(), next_at: None }).await.unwrap();
        store.apply(Lane::Live, vec![Change::Amend(Amend { target: "m3".into(), ..receipt })]).await.unwrap();
        assert_eq!(
            state("m3").await.unwrap(),
            ("delivered".into(), 1_000, None),
            "a delivered receipt leaves no failed outbox row behind"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_a_message_still_waiting_for_a_receipt_fails_for_want_of_one() {
        use crate::store::{Change, Lane, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let apply = |step: Step| store.apply(Lane::Live, vec![Change::Outbox(step)]);
        let step = Step::Enqueue {
            message: Box::new(mine("m1", 1_000)),
            kind: "text",
            raw: vec![1],
            key: None,
            written: Written::default(),
        };
        apply(step).await.unwrap();
        let state = || {
            store.reads().query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row(
                    "SELECT m.status, o.state, o.attempts FROM outbox o JOIN messages m ON m.n = o.message",
                    [],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
                )?)
            })
        };
        let no_receipt = || Step::NoReceipt { outbox: 1, error: "no receipt".into() };
        assert_eq!(apply(no_receipt()).await.unwrap().changed, 0);
        assert_eq!(state().await.unwrap(), ("pending".into(), "queued".into(), 0), "a queued message is left alone");
        apply(Step::Unconfirmed { outbox: 1, error: "refused".into(), until: 9 }).await.unwrap();
        apply(Step::Failed { outbox: 1, error: "late".into(), next_at: None }).await.unwrap();
        assert_eq!(
            state().await.unwrap(),
            ("pending".into(), "unconfirmed".into(), 1),
            "an attempt's failure doesn't touch what waits for a receipt"
        );
        assert_eq!(apply(no_receipt()).await.unwrap().changed, 1);
        assert_eq!(state().await.unwrap(), ("failed".into(), "failed".into(), 1));
        assert_eq!(apply(no_receipt()).await.unwrap().changed, 0, "and only once");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_disappearing_message_starts_its_timer_whether_a_receipt_or_the_servers_answer_comes_first() {
        use crate::store::{Amend, Amendment, Change, Chat, Lane, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = Chat { id: "a@s.whatsapp.net".into(), ephemeral: Some(86_400), ..Chat::default() };
        store.apply(Lane::Live, vec![Change::Chat(chat)]).await.unwrap();
        for id in ["m1", "m2"] {
            let step = Step::Enqueue {
                message: Box::new(mine(id, 1_000)),
                kind: "text",
                raw: vec![1],
                key: None,
                written: Written::default(),
            };
            store.apply(Lane::Live, vec![Change::Outbox(step)]).await.unwrap();
        }
        let delivered = |id: &str| {
            Change::Amend(Amend {
                chat: "a@s.whatsapp.net".into(),
                target: id.into(),
                sender: Some("a@s.whatsapp.net".into()),
                mine: false,
                at: 2_000,
                what: Amendment::Status { status: "delivered".into() },
                raw: None,
            })
        };
        let expires = || {
            store.reads().query(crate::work::Priority::Interactive, |db| {
                Ok(db
                    .prepare("SELECT expires_at FROM messages ORDER BY id")?
                    .query_map([], |r| r.get::<_, Option<i64>>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
        };
        assert_eq!(expires().await.unwrap(), [None, None], "neither has gone yet");
        let began = now_ms();
        store.apply(Lane::Live, vec![delivered("m1")]).await.unwrap();
        store.apply(Lane::Live, vec![Change::Outbox(Step::Sent { outbox: 2 })]).await.unwrap();
        let first = expires().await.unwrap();
        let day = 86_400 * 1000;
        assert!(first.iter().all(|at| at.is_some_and(|at| at >= began + day)), "{first:?}");
        store.apply(Lane::Live, vec![Change::Outbox(Step::Sent { outbox: 1 })]).await.unwrap();
        store.apply(Lane::Live, vec![delivered("m2")]).await.unwrap();
        assert_eq!(expires().await.unwrap(), first, "what settles it second leaves the timer as it started");
        assert_eq!(store.count("outbox").unwrap(), 0);
    }

    /// Group `chat` with `members` and you, known as ME_JID.
    fn group(chat: &str, members: impl IntoIterator<Item = String>) -> Vec<crate::store::Change> {
        use crate::store::{Change, Members};
        let mut set: Vec<(String, &'static str)> = members.into_iter().map(|jid| (jid, "member")).collect();
        set.push((ME_JID.into(), "member"));
        vec![
            Change::Remember { key: "me".into(), value: ME_JID.into() },
            Change::Members(Members { chat: chat.into(), all: true, set, remove: Vec::new() }),
        ]
    }

    const ME_JID: &str = "351910000001@s.whatsapp.net";

    fn member(n: usize) -> String {
        format!("35192{n:07}@s.whatsapp.net")
    }

    fn in_group(chat: &str, id: String) -> Step {
        let message = Message { chat: chat.into(), ..mine(&id, 1_000) };
        Step::Enqueue { message: Box::new(message), kind: "text", raw: vec![1], key: None, written: Written::default() }
    }

    fn read_by(chat: &str, id: &str, who: &str) -> crate::store::Change {
        use crate::store::{Amend, Amendment, Change};
        Change::Amend(Amend {
            chat: chat.into(),
            target: id.into(),
            sender: Some(who.into()),
            mine: false,
            at: 2_000,
            what: Amendment::Status { status: "read".into() },
            raw: None,
        })
    }

    async fn statuses(store: &crate::store::Store, chat: &'static str) -> Vec<String> {
        store
            .reads()
            .query(crate::work::Priority::Interactive, move |db| {
                Ok(db
                    .prepare(
                        "SELECT m.status FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = ?1 ORDER BY m.n",
                    )?
                    .query_map([chat], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await
            .unwrap()
    }

    /// Steps of regrouping until nothing is left to look at, and of letting go of the recipients of
    /// what's settled, as the background lane takes them. Says how many steps regrouping took.
    async fn regroup_all(store: &crate::store::Store) -> usize {
        use crate::store::{Change, Lane};
        let mut steps = 0;
        loop {
            let (left, settled) = store
                .reads()
                .query(crate::work::Priority::Interactive, |db| Ok((regroups(db, 64)?, settled_messages(db, 64)?)))
                .await
                .unwrap();
            if left.is_empty() && settled.is_empty() {
                return steps;
            }
            let regroup = left.iter().map(|&(chat, member)| Change::Outbox(Step::Regroup { chat, member }));
            let let_go = settled.iter().map(|&message| Change::Outbox(Step::LetGo { message }));
            store.apply(Lane::Background, regroup.chain(let_go).collect()).await.unwrap();
            steps += left.len();
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn when_the_last_member_who_hadnt_read_leaves_every_message_of_yours_there_is_read() {
        use crate::store::{Change, Lane, Members, Store};
        const CLIMBING: &str = "120363000000000001@g.us";
        const OTHER: &str = "120363000000000002@g.us";
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, group(CLIMBING, (1..=5).map(member))).await.unwrap();
        store.apply(Lane::Live, group(OTHER, [member(1)])).await.unwrap();
        let mut changes = Vec::new();
        for n in 0..100 {
            changes.push(Change::Outbox(in_group(CLIMBING, format!("c{n}"))));
        }
        for n in 0..150 {
            changes.push(Change::Outbox(in_group(OTHER, format!("o{n}"))));
        }
        store.apply(Lane::Live, changes).await.unwrap();
        let receipts: Vec<Change> =
            (0..100).flat_map(|n| (2..=5).map(move |m| read_by(CLIMBING, &format!("c{n}"), &member(m)))).collect();
        store.apply(Lane::Live, receipts).await.unwrap();
        assert!(statuses(&store, CLIMBING).await.iter().all(|s| s == "sent"), "member 1 hasn't read any");
        let left = Members { chat: CLIMBING.into(), all: false, set: Vec::new(), remove: vec![member(1)] };
        store.apply(Lane::Live, vec![Change::Members(left)]).await.unwrap();
        assert!(statuses(&store, CLIMBING).await.iter().all(|s| s == "sent"), "the change only notes who left");
        regroup_all(&store).await;
        assert!(statuses(&store, CLIMBING).await.iter().all(|s| s == "read"), "{:?}", statuses(&store, CLIMBING).await);
        assert!(statuses(&store, OTHER).await.iter().all(|s| s == "pending"), "the other group's are left alone");
        let rows = store.count("recipients").unwrap();
        assert_eq!(rows, 150, "a message read by all lets go of who it was sent to");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn hundreds_leaving_at_once_is_a_short_change_and_regrouping_goes_in_short_steps() {
        use crate::store::{Change, Lane, Members, Store};
        const BIG: &str = "120363000000000003@g.us";
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, group(BIG, (1..=400).map(member))).await.unwrap();
        let sends = (0..100).map(|n| Change::Outbox(in_group(BIG, format!("b{n}")))).collect();
        store.apply(Lane::Live, sends).await.unwrap();
        let receipts: Vec<Change> =
            (0..100).flat_map(|n| (200..=400).map(move |m| read_by(BIG, &format!("b{n}"), &member(m)))).collect();
        store.apply(Lane::Background, receipts).await.unwrap();
        let left = Members { chat: BIG.into(), all: false, set: Vec::new(), remove: (1..200).map(member).collect() };
        let began = std::time::Instant::now();
        let applied = store.apply(Lane::Live, vec![Change::Members(left)]).await.unwrap();
        let took = began.elapsed();
        assert!(took < std::time::Duration::from_millis(500), "199 leaving took {took:?} in one transaction");
        assert_eq!(applied.transactions, 1);
        let steps = regroup_all(&store).await;
        assert!(steps >= 199, "each member who left is a step of its own at least: {steps}");
        assert!(statuses(&store, BIG).await.iter().all(|s| s == "read"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn in_a_group_of_a_thousand_a_step_of_regrouping_looks_at_one_message_so_it_stays_short() {
        use crate::store::{Change, Lane, Members, Store};
        const HUGE: &str = "120363000000000004@g.us";
        const MESSAGES: usize = 32;
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, group(HUGE, (1..=1_024).map(member))).await.unwrap();
        let sends = (0..MESSAGES).map(|n| Change::Outbox(in_group(HUGE, format!("h{n}")))).collect();
        store.apply(Lane::Live, sends).await.unwrap();
        // Read by everyone but member 1, written straight in: 32,000 receipts through the writer
        // would take a debug build minutes, and nothing here depends on how they came.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        db.execute(
            "INSERT INTO receipts (message, recipient, read_at) SELECT r.message, r.member, 2000 FROM recipients r
            JOIN person_jids j ON j.n = r.member WHERE j.jid <> ?1",
            [member(1)],
        )
        .unwrap();
        drop(db);
        let left = Members { chat: HUGE.into(), all: false, set: Vec::new(), remove: vec![member(1)] };
        store.apply(Lane::Live, vec![Change::Members(left)]).await.unwrap();
        let (mut regrouping, mut letting_go) = (Vec::new(), Vec::new());
        loop {
            let left = store.reads().query(crate::work::Priority::Interactive, |db| regroups(db, 1)).await.unwrap();
            let Some(&(chat, member)) = left.first() else { break };
            let step = Change::Outbox(Step::Regroup { chat, member });
            regrouping.push(store.apply(Lane::Background, vec![step]).await.unwrap().took);
        }
        assert!(statuses(&store, HUGE).await.iter().all(|s| s == "read"), "{:?}", statuses(&store, HUGE).await);
        assert_eq!(regrouping.len(), MESSAGES + 1, "a message a step, and one to find there's none left");
        loop {
            let settled =
                store.reads().query(crate::work::Priority::Interactive, |db| settled_messages(db, 1)).await.unwrap();
            let Some(&message) = settled.first() else { break };
            let step = Change::Outbox(Step::LetGo { message });
            letting_go.push(store.apply(Lane::Background, vec![step]).await.unwrap().took);
        }
        assert_eq!(letting_go.len(), MESSAGES * 9, "1,024 recipients a message, 128 a step, then none left");
        assert_eq!(store.count("recipients").unwrap(), 0, "all let go of once read by all");
        // A WAL checkpoint can fall in any one commit, until durability's checkpointer takes them.
        if !cfg!(debug_assertions) {
            for (what, took) in [("regrouping", &regrouping), ("letting go", &letting_go)] {
                let over = took.iter().filter(|took| **took >= Duration::from_millis(3)).count();
                assert!(over <= 1 + took.len() / 100, "{over} steps of {what} took 3 ms or more: {took:?}");
            }
        }
    }

    #[test]
    fn how_far_whatsapp_was_told_never_passes_the_read_position() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        db.execute("INSERT INTO chats (n, id, kind, v) VALUES (1, 'a@s.whatsapp.net', 'direct', 1)", []).unwrap();
        let refused = db.execute("INSERT INTO reads (chat, at, n, told_at, told_n) VALUES (1, 5, 1, 6, 1)", []);
        assert!(refused.is_err());
        db.execute("INSERT INTO reads (chat, at, n, told_at, told_n) VALUES (1, 5, 3, 5, 1)", []).unwrap();
        db.execute(TOLD, params![1, 5, 4]).unwrap();
        db.execute(TOLD, params![1, 5, 0]).unwrap();
        let told: (i64, i64) =
            db.query_row("SELECT told_at, told_n FROM reads", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(told, (5, 1), "neither past the position nor back");
    }
}
