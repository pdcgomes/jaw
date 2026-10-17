//! The outbox and read receipts: what sending writes and reads. A message you send is written as
//! pending with its outbox row in one transaction, so it shows at once and is sent however jawd
//! stops; the server's answer or any receipt removes the row; a refusal leaves it failed until it's
//! retried by hand, or, when the message may already have gone, unconfirmed until a receipt says
//! so or a day passes. Deleting it for you leaves its row, which still sends it, as the phone does.
//! Reading moves the chat's one read position, which sync keeps (`set_read`, `settle`), and the
//! read receipts owed to WhatsApp are the messages between how far it has been told and there.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{CHAT, MESSAGE, Message, Stamps, advanced, chat_jid, now_ms, upsert_message};

/// The most messages one read receipt names; more go in more receipts, oldest first.
pub const RECEIPTS_AT_ONCE: usize = 1_000;
/// The most messages the outbox holds, waiting, unconfirmed or failed; past it a new one is
/// refused, and none already in it is dropped.
pub const OUTBOX_MOST: u64 = 1_000;
/// How long an interface's key for a message it sent is kept.
pub const KEY_KEPT_MS: i64 = 24 * 3600 * 1000;
/// What one step of regrouping may look at, in messages times the larger of the group's members
/// and the message's recipients: looking at a message may walk all who it was sent to, and every
/// member now, at about 1.5 µs each, so a step stays under 2 ms whatever the group's size or how
/// many have left it. A message that alone is more is looked at alone.
const REGROUP_WORK: i64 = 1_024;
/// The most messages one step looks at, for small groups, where each look's own cost counts.
const REGROUP_AT_ONCE: usize = 16;
/// The most of a member's messages one step goes through, those already at their end included,
/// which cost a lookup each and aren't looked at.
const REGROUP_SCANNED: i64 = 64;
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
    /// WhatsApp has been told chat `chat` is read from `from`, where it was told before, up to
    /// `upto`. It's kept only if "told" is still at `from`, since a merge or WhatsApp itself may have
    /// moved it meanwhile; what's left owed is then sent again, which is harmless.
    Told { chat: i64, from: (i64, i64), upto: (i64, i64) },
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
    /// The message, under its chat's JID as jaw keeps it and its chat's n, and its outbox row, if
    /// it's still to send: the one written now, or the one an earlier request with the same key
    /// wrote, which has no row or chat here if it was deleted for you since.
    Queued { message: Option<i64>, id: String, chat: String, chat_n: Option<i64>, outbox: Option<i64> },
    /// The outbox holds as many as it may, so nothing was written.
    Full { waiting: u64, failed: u64 },
}

const KEYED: &str = "SELECT k.message, k.id, coalesce(c.id, k.jid), c.n, o.n FROM send_keys k
    LEFT JOIN messages m ON m.n = k.message LEFT JOIN chats c ON c.n = m.chat LEFT JOIN outbox o ON o.message = m.n
    WHERE k.key = ?1";
const KEEP_KEY: &str = "INSERT INTO send_keys (key, message, jid, id, at) VALUES (?1, ?2, ?3, ?4, ?5)";
const PRUNE_KEYS: &str =
    "DELETE FROM send_keys WHERE key IN (SELECT key FROM send_keys WHERE at < ?1 ORDER BY at LIMIT ?2)";
const MESSAGE_BY_ID: &str =
    "SELECT m.n, m.chat FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1 AND m.id = ?2";
const ENQUEUE: &str = "INSERT INTO outbox (message, jid, id, kind, raw, next_at, created_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6) ON CONFLICT (message) DO NOTHING";
const OUTBOX_OF: &str = "SELECT n FROM outbox WHERE message = ?1";
const CLEAR_DRAFT: &str = "UPDATE chats SET draft = NULL, v = ?2 WHERE n = ?1 AND draft IS NOT NULL";
const OUTBOXED_MESSAGE: &str =
    "SELECT m.n, m.chat, m.status FROM outbox o LEFT JOIN messages m ON m.n = o.message WHERE o.n = ?1";
const DROP_ROW: &str = "DELETE FROM outbox WHERE n = ?1";
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
const TOLD: &str = "UPDATE chats SET
        told_at = iif((?4, ?5) < (read_at, coalesce(read_n, 9223372036854775807)), ?4, NULL),
        told_n = iif((?4, ?5) < (read_at, coalesce(read_n, 9223372036854775807)), ?5, NULL)
    WHERE n = ?1 AND (told_at, told_n) = (?2, ?3) AND (?4, ?5) > (?2, ?3)";
/// Where an outbox row goes, ?r being the row: the chat its message is in, or the one that chat is
/// being merged into, as chat_jid resolves it; or, once its message is deleted, where it was sent.
macro_rules! sent_where {
    () => {
        "LEFT JOIN messages m ON m.n = o.message LEFT JOIN chats c ON c.n = m.chat
        LEFT JOIN chat_merges cm ON cm.gone = c.n LEFT JOIN chats k ON k.n = cm.keep"
    };
}
const QUEUED: &str = concat!(
    "SELECT o.n, coalesce(k.id, c.id, o.jid) FROM outbox o ",
    sent_where!(),
    " WHERE o.state = 'queued' ORDER BY o.n"
);
const UNCONFIRMED_ROWS: &str = "SELECT n, next_at FROM outbox WHERE state = 'unconfirmed'";
const WAITING: &str = "SELECT count(*), coalesce(sum(state = 'failed'), 0) FROM outbox";
const OUTBOXED: &str = concat!(
    "SELECT o.n, coalesce(k.id, c.id, o.jid), o.id, o.raw, o.state, o.attempts, m.status FROM outbox o ",
    sent_where!()
);
const OWING: &str = "SELECT n FROM chats WHERE told_at IS NOT NULL";
const OWED_CHAT: &str = "SELECT c.id, c.kind, c.read_at, coalesce(c.read_n, 9223372036854775807), c.told_at, c.told_n,
        (SELECT keep FROM chat_merges WHERE gone = c.n), EXISTS (SELECT 1 FROM chat_merges WHERE keep = c.n)
    FROM chats c WHERE c.n = ?1 AND c.told_at IS NOT NULL";
const OWED_MESSAGES: &str = "SELECT m.id, j.jid, m.at, m.n FROM messages m LEFT JOIN person_jids j ON j.n = m.sender
    WHERE m.chat = ?1 AND m.mine = 0 AND m.kind NOT IN ('system', 'call') AND (m.at, m.n) > (?2, ?3)
        AND (m.at, m.n) <= (?4, ?5)
    ORDER BY m.at, m.n LIMIT ?6";
const STATUS_OF: &str = "SELECT status FROM messages WHERE n = ?1";
const FOLD_OUTBOX: &str = "UPDATE OR IGNORE outbox SET message = ?1 WHERE message = ?2";
const FOLD_KEYS: &str = "UPDATE send_keys SET message = ?1 WHERE message = ?2";

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
const RECIPIENT_COUNT: &str = "SELECT count(*) FROM recipients WHERE message = ?1";
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
        Step::Told { chat, upto, .. } => ("told", format!("{chat} {} {}", upto.0, upto.1), None),
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
        Step::Told { chat, from, upto } => {
            db.prepare_cached(TOLD)?.execute(params![chat, from.0, from.1, upto.0, upto.1])?;
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
        type Keyed = (Option<i64>, String, String, Option<i64>, Option<i64>);
        let keyed: Option<Keyed> = db
            .prepare_cached(KEYED)?
            .query_row([key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .optional()?;
        if let Some((message, id, chat, chat_n, outbox)) = keyed {
            written.put(Enqueued::Queued { message, id, chat, chat_n, outbox });
            return Ok(false);
        }
    }
    let (waiting, failed) = waiting(db)?;
    if waiting >= OUTBOX_MOST {
        written.put(Enqueued::Full { waiting, failed });
        return Ok(false);
    }
    let chat = chat_jid(db, &message.chat)?;
    let message = &Message { chat: chat.clone(), here: true, ..message.clone() };
    let changed = upsert_message(db, stamps, message, false)?;
    let (n, chat_n): (i64, i64) = db
        .prepare_cached(MESSAGE_BY_ID)?
        .query_row(params![message.chat, message.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let now = now_ms();
    db.prepare_cached(ENQUEUE)?.execute(params![n, chat, message.id, kind, raw, now])?;
    let outbox: i64 = db.prepare_cached(OUTBOX_OF)?.query_row([n], |r| r.get(0))?;
    db.prepare_cached(RECIPIENTS)?.execute(params![n, chat_n, me(db)?])?;
    db.prepare_cached(PRUNE_KEYS)?.execute(params![now - KEY_KEPT_MS, KEYS_PRUNED])?;
    if let Some(key) = key {
        db.prepare_cached(KEEP_KEY)?.execute(params![key, n, chat, message.id, now])?;
    }
    written.put(Enqueued::Queued {
        message: Some(n),
        id: message.id.clone(),
        chat,
        chat_n: Some(chat_n),
        outbox: Some(outbox),
    });
    let seq = stamps.seq(CHAT, Some(chat_n));
    if db.prepare_cached(CLEAR_DRAFT)?.execute([chat_n, seq])? > 0 {
        stamps.record(db, CHAT, chat_n, Some(chat_n), seq)?;
        return Ok(true);
    }
    Ok(changed)
}

/// An outbox row's message: its n, its chat's n and its status.
type Its = (i64, i64, Option<String>);

/// Outbox row `outbox`'s message, `Some(None)` for a row whose message was deleted for you, which
/// is sent all the same.
fn outboxed(db: &Connection, outbox: i64) -> Result<Option<Option<Its>>> {
    let row: Option<(Option<i64>, Option<i64>, Option<String>)> = db
        .prepare_cached(OUTBOXED_MESSAGE)?
        .query_row([outbox], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    Ok(row.map(|(n, chat, status)| n.zip(chat).map(|(n, chat)| (n, chat, status))))
}

/// Removes outbox row `outbox`, whose message was deleted for you: it has gone, or it can't, and
/// with nothing left to show it, nothing can retry it.
fn dropped(db: &Connection, outbox: i64) -> Result<bool> {
    db.prepare_cached(DROP_ROW)?.execute([outbox])?;
    Ok(false)
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
    let (n, chat, status) = match outboxed(db, outbox)? {
        None => return Ok(false),
        Some(None) => return dropped(db, outbox),
        Some(Some(row)) => row,
    };
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
    if at_end(kind, Some(status)) {
        db.prepare_cached(SETTLED)?.execute([n])?;
    }
    Ok(())
}

/// Whether a message of `kind` at `status` has got as far as it can, so no receipt or member
/// leaving can move it.
pub(super) fn at_end(kind: &str, status: Option<&str>) -> bool {
    let played = matches!(kind, "voice" | "audio" | "video" | "video_note");
    status == Some("played") || (status == Some("read") && !played)
}

/// Some of the messages whose recipients are still to be let go of.
pub fn settled_messages(db: &Connection, most: usize) -> Result<Vec<i64>> {
    Ok(db.prepare_cached(SETTLED_ROWS)?.query_map([most as i64], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// An attempt at a queued message failed; one that's still queued after it is still pending.
fn failed(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str, next_at: Option<i64>) -> Result<bool> {
    let Some(row) = outboxed(db, outbox)? else { return Ok(false) };
    if db.prepare_cached(ATTEMPTED)?.execute(params![outbox, error, next_at])? == 0 || next_at.is_some() {
        return Ok(false);
    }
    match row {
        Some((n, chat, status)) => shown_failed(db, stamps, chat, n, status.as_deref()),
        None => dropped(db, outbox),
    }
}

/// A message waiting for a receipt had none in time; one that had is no longer in the outbox, or
/// no longer waiting.
fn no_receipt(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str) -> Result<bool> {
    let Some(row) = outboxed(db, outbox)? else { return Ok(false) };
    if db.prepare_cached(NO_RECEIPT)?.execute(params![outbox, error])? == 0 {
        return Ok(false);
    }
    match row {
        Some((n, chat, status)) => shown_failed(db, stamps, chat, n, status.as_deref()),
        None => dropped(db, outbox),
    }
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
    let Some(Some((n, chat, _))) = outboxed(db, outbox)? else { return Ok(false) };
    let queued = db.prepare_cached(REQUEUE)?.execute(params![outbox, now_ms()])?;
    let seq = stamps.seq(MESSAGE, Some(n));
    let pending = db.prepare_cached(PENDING_AGAIN)?.execute([n, seq])?;
    if pending > 0 {
        touched(db, stamps, chat, n, seq)?;
    }
    Ok(queued + pending > 0)
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

/// A message of yours as regrouping looks at it: its status and kind.
type Mine = (Option<String>, String);

/// What one step of regrouping goes through, in order: each message, with its status and kind if
/// it's to be looked at, or `None` if it's at its end or no longer yours there.
#[derive(Debug, Default)]
struct Plan {
    messages: Vec<(i64, Option<Mine>)>,
    /// The work the messages to look at add up to, as REGROUP_WORK counts it.
    work: i64,
    /// Whether there's nothing after them.
    all: bool,
}

/// The next of your messages in `chat` that `member` was sent, from after `after`: as many to look
/// at as REGROUP_WORK allows, and those at their end on the way, which aren't counted.
fn plan(db: &Connection, chat: i64, member: i64, after: i64) -> Result<Plan> {
    let members: i64 = db.prepare_cached(GROUP_SIZE)?.query_row([chat], |r| r.get(0))?;
    let scanned: Vec<i64> = db
        .prepare_cached(SENT_TO_MEMBER)?
        .query_map(params![chat, member, after, REGROUP_SCANNED], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut plan = Plan { all: (scanned.len() as i64) < REGROUP_SCANNED, ..Plan::default() };
    let mut looking = 0;
    for &n in &scanned {
        let mine: Option<Mine> =
            db.prepare_cached(MINE_IN)?.query_row([n, chat], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let Some((kept, kind)) = mine.filter(|(kept, kind)| !at_end(kind, kept.as_deref())) else {
            plan.messages.push((n, None));
            continue;
        };
        let recipients: i64 = db.prepare_cached(RECIPIENT_COUNT)?.query_row([n], |r| r.get(0))?;
        let work = members.max(recipients).max(1);
        if looking > 0 && (looking == REGROUP_AT_ONCE || plan.work + work > REGROUP_WORK) {
            plan.all = false;
            break;
        }
        plan.work += work;
        looking += 1;
        plan.messages.push((n, Some((kept, kind))));
    }
    Ok(plan)
}

/// Looks again at the next few of your messages in `chat` that `member` was sent, as `plan` picks
/// them, and stops sooner once REGROUP_FOR has passed.
fn regroup(db: &Connection, stamps: &mut Stamps, chat: i64, member: i64) -> Result<bool> {
    let began = Instant::now();
    let Some(after): Option<i64> =
        db.prepare_cached(REGROUP_FROM)?.query_row([chat, member], |r| r.get(0)).optional()?
    else {
        return Ok(false);
    };
    let plan = plan(db, chat, member, after)?;
    let mut changed = false;
    let mut last = None;
    let mut looked = false;
    let mut stopped = false;
    for (n, look) in &plan.messages {
        let Some((kept, kind)) = look else {
            last = Some(*n);
            continue;
        };
        if looked && began.elapsed() >= REGROUP_FOR {
            stopped = true;
            break;
        }
        looked = true;
        last = Some(*n);
        let Some(Some(reached)) = group_reached(db, *n, chat, kept.as_deref())? else { continue };
        if advanced(kept.as_deref(), Some(reached)) != kept.as_deref() {
            let seq = stamps.seq(MESSAGE, Some(*n));
            db.prepare_cached(REGROUPED)?.execute(params![n, reached, seq])?;
            touched(db, stamps, chat, *n, seq)?;
            settled(db, *n, kind, reached)?;
            changed = true;
        }
    }
    match last {
        Some(last) if stopped || !plan.all => {
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
    Ok(db.prepare_cached(&format!("{OUTBOXED} WHERE o.id = ?1"))?.query_row([id], outboxed_row).optional()?)
}

/// The message an interface's key names, if it was sent in the last day, with its chat's JID.
pub fn keyed(db: &Connection, key: &str) -> Result<Option<(String, String)>> {
    Ok(db.prepare_cached(KEYED)?.query_row([key], |r| Ok((r.get(2)?, r.get(1)?))).optional()?)
}

/// Read receipts owed in one chat: its JID, how far WhatsApp was told and how far this lot goes,
/// and the messages from others between, oldest first, each with who sent it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owed {
    pub chat: i64,
    pub jid: String,
    pub group: bool,
    pub from: (i64, i64),
    pub upto: (i64, i64),
    pub messages: Vec<(String, Option<String>)>,
}

/// What a chat owes WhatsApp now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owing {
    Nothing,
    /// The chat is merging, as `into`, or into `into`: what's owed is worked out once its messages
    /// are all in one chat, since one not moved yet would be passed over.
    Merging {
        into: i64,
    },
    Receipts(Owed),
}

/// The chats whose read receipts WhatsApp hasn't all had.
pub fn owing(db: &Connection) -> Result<Vec<i64>> {
    Ok(db.prepare_cached(OWING)?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// The next lot of what chat `chat` owes WhatsApp, if anything: up to RECEIPTS_AT_ONCE of the
/// oldest messages owed, going as far as the last of them, or to the read position if that's all.
pub fn owed(db: &Connection, chat: i64) -> Result<Owing> {
    type Found = (String, String, i64, i64, i64, i64, Option<i64>, bool);
    let found: Option<Found> = db
        .prepare_cached(OWED_CHAT)?
        .query_row([chat], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })
        .optional()?;
    let Some((jid, kind, at, n, told_at, told_n, into, merged_into)) = found else { return Ok(Owing::Nothing) };
    if let Some(into) = into {
        return Ok(Owing::Merging { into });
    }
    if merged_into {
        return Ok(Owing::Merging { into: chat });
    }
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
    Ok(Owing::Receipts(Owed { chat, jid, group: kind != "direct", from: (told_at, told_n), upto, messages }))
}

/// Two copies of a message, one in each of two chats being merged, become `kept`, before `gone` is
/// deleted: a send waiting for either goes on as the kept one's, its own if it has one, unless the
/// kept one has already reached the server; and an interface's key for either names the kept one.
/// Nothing of `gone`'s is left in the outbox, where its deletion would leave it a second send.
pub(super) fn folded(db: &Connection, kept: i64, gone: i64) -> Result<()> {
    let status: Option<String> = db.prepare_cached(STATUS_OF)?.query_row([kept], |r| r.get(0))?;
    if matches!(status.as_deref(), Some("sent" | "delivered" | "read" | "played")) {
        received(db, kept)?;
    } else {
        db.prepare_cached(FOLD_OUTBOX)?.execute([kept, gone])?;
    }
    db.prepare_cached(DEQUEUE)?.execute([gone])?;
    db.prepare_cached(FOLD_KEYS)?.execute([kept, gone])?;
    Ok(())
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
        let by_id = format!("{OUTBOXED} WHERE o.id = ?1");
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
                "SEARCH o USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
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
            ("told", TOLD, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "queued",
                QUEUED,
                "SCAN o\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH cm USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
            ),
            ("waiting", WAITING, "SCAN outbox"),
            (
                "outboxed by id",
                &by_id,
                "SCAN o\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH cm USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
            ),
            (
                "outboxed by n",
                &by_n,
                "SEARCH o USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH cm USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH k USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN",
            ),
            ("owing", OWING, "SCAN chats"),
            (
                "owed by a chat",
                OWED_CHAT,
                "SEARCH c USING INTEGER PRIMARY KEY (rowid=?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH chat_merges USING INTEGER PRIMARY KEY (rowid=?)\nCORRELATED SCALAR SUBQUERY 2\nSEARCH chat_merges USING COVERING INDEX chat_merges_by_keep (keep=?)",
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
                "SEARCH recipients USING COVERING INDEX recipients_by_member (member=? AND chat=? AND message>?)",
            ),
            (
                "keyed",
                KEYED,
                "SEARCH k USING PRIMARY KEY (key=?)\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH c USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN\nSEARCH o USING COVERING INDEX sqlite_autoindex_outbox_1 (message=?) LEFT-JOIN",
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
            ("a message's recipients", RECIPIENT_COUNT, "SEARCH recipients USING PRIMARY KEY (message=?)"),
            ("regroup on", REGROUP_ON, "SEARCH regroup USING PRIMARY KEY (chat=? AND member=?)"),
            ("regroup done", REGROUP_DONE, "SEARCH regroup USING PRIMARY KEY (chat=? AND member=?)"),
            ("regroups", REGROUPS, "SCAN regroup USING COVERING INDEX regroup_by_member"),
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

    /// A database as migrations 1 to 6 left it, with `rows` written into it.
    fn at_6(rows: &str) -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        for migration in &schema::MIGRATIONS[..6] {
            tx.execute_batch(migration).unwrap();
        }
        tx.execute_batch(rows).unwrap();
        tx.pragma_update(None, "user_version", 6).unwrap();
        tx.commit().unwrap();
        db
    }

    #[test]
    fn migration_7_keeps_every_outbox_row_beside_its_message_or_to_send_without_one() {
        let mut db = at_6(
            "INSERT INTO chats (n, id, kind, v) VALUES (1, 'a@s.whatsapp.net', 'direct', 1);
            INSERT INTO messages (n, chat, id, at, t, kind, v) VALUES (7, 1, 'm1', 5, 5, 'text', 1);
            INSERT INTO outbox (id, chat, kind, raw, next_at, created_at) VALUES ('m1', 'a@s.whatsapp.net', 'text', x'01', 9, 9);
            INSERT INTO outbox (id, chat, kind, raw, next_at, created_at) VALUES ('gone', 'a@s.whatsapp.net', 'text', x'02', 9, 10);",
        );
        schema::migrate(&mut db).unwrap();
        type Row = (Option<i64>, String, String, Vec<u8>, String);
        let kept: Vec<Row> = db
            .prepare("SELECT message, jid, id, raw, state FROM outbox ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            kept,
            [
                (Some(7), "a@s.whatsapp.net".into(), "m1".into(), vec![1], "queued".into()),
                (None, "a@s.whatsapp.net".into(), "gone".into(), vec![2], "queued".into()),
            ]
        );
    }

    /// Makes `changes` in one transaction as the writer does, settle and all.
    fn written(db: &Connection, changes: &[crate::store::Change]) {
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        for change in changes {
            super::super::apply(db, &mut stamps, change, false).unwrap();
        }
        super::super::settle(db, &mut stamps).unwrap();
    }

    fn owed_ids(db: &Connection, chat: i64) -> Vec<String> {
        match owed(db, chat).unwrap() {
            Owing::Receipts(owed) => owed.messages.into_iter().map(|(id, _)| id).collect(),
            Owing::Nothing => Vec::new(),
            Owing::Merging { .. } => panic!("chat {chat} isn't merging"),
        }
    }

    #[test]
    fn upgraded_read_positions_owe_what_whatsapp_hasnt_heard_once_read_here() {
        use crate::store::{Change, Message, Read};
        // Read up to 5 as WhatsApp said; up to 5 as a guess; never; and the phone's counts, 2, at
        // the newest, as migration 6 started every chat with messages.
        let mut chats = String::new();
        for (n, read) in [(1, "5, 1, NULL, 1"), (2, "5, 0, NULL, 1"), (3, "NULL, 0, NULL, 3"), (4, "7, 1, 2, 2")] {
            chats.push_str(&format!(
                "INSERT INTO chats (n, id, kind, v, read_at, read_known, phone_unread, unread)
                VALUES ({n}, '35191000000{n}@s.whatsapp.net', 'direct', 1, {read});"
            ));
            for (m, at) in [(1, 3), (2, 5), (3, 7)] {
                chats.push_str(&format!(
                    "INSERT INTO messages (chat, id, at, t, kind, v) VALUES ({n}, 'm{m}', {at}, {n}{at}0, 'text', 1);"
                ));
            }
        }
        let mut db = at_6(&chats);
        schema::migrate(&mut db).unwrap();
        let columns: i64 = db
            .query_row(
                "SELECT count(*) FROM chats WHERE read_n IS NULL AND told_at IS NULL AND told_n IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(columns, 4, "every position is still a time, and WhatsApp knows as much as jaw");
        assert!((1..=4).all(|chat| owed_ids(&db, chat).is_empty()), "nothing's owed until it's read here");
        // A history message at exactly the time a chat was read up to is read too: (5, i64::MAX).
        let late = Message {
            chat: "351910000001@s.whatsapp.net".into(),
            id: "m2b".into(),
            at: 5,
            kind: "text",
            ..Message::default()
        };
        written(&db, &[Change::Message(late)]);
        let unread =
            |chat: i64| -> i64 { db.query_row("SELECT unread FROM chats WHERE n = ?1", [chat], |r| r.get(0)).unwrap() };
        assert_eq!(unread(1), 1, "only m3 is after where it was read");
        assert_eq!(unread(4), 2, "the phone's counts stand in");
        let here = |chat: i64| {
            Change::Read(Read {
                chat: format!("35191000000{chat}@s.whatsapp.net"),
                ids: vec!["m3".into()],
                here: true,
                ..Read::default()
            })
        };
        written(&db, &(1..=4).map(here).collect::<Vec<_>>());
        assert_eq!(owed_ids(&db, 1), ["m3"], "WhatsApp knew up to 5");
        assert_eq!(owed_ids(&db, 2), ["m3"], "and guessed up to 5");
        assert_eq!(owed_ids(&db, 3), ["m1", "m2", "m3"], "nothing was read before");
        assert_eq!(owed_ids(&db, 4), ["m2", "m3"], "the phone still showed its newest 2 unread");
        assert!((1..=4).all(|chat| unread(chat) == 0));
    }

    #[test]
    fn whatsapps_position_replaces_a_guess_even_when_its_earlier_and_told_goes_with_it() {
        use crate::store::{Change, Read};
        let mut db = at_6(
            "INSERT INTO chats (n, id, kind, v, read_at, read_known, phone_unread, unread)
                VALUES (1, 'a@s.whatsapp.net', 'direct', 1, 7, 1, 2, 2);
            INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'm1', 3, 30, 'text', 1);
            INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'm2', 5, 50, 'text', 1);
            INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'm3', 7, 70, 'text', 1);",
        );
        schema::migrate(&mut db).unwrap();
        // The phone's app state says it's read up to 3, before migration 6's guess at the newest.
        written(&db, &[Change::Read(Read { chat: "a@s.whatsapp.net".into(), at: 3, ..Read::default() })]);
        let position: (i64, Option<i64>, Option<i64>, i64) = db
            .query_row("SELECT read_at, read_n, phone_unread, unread FROM chats", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(position, (3, None, None, 2), "WhatsApp's position replaces the guess, and counts from it");
        assert!(owed_ids(&db, 1).is_empty(), "WhatsApp knows where it said");
        let here = Read { chat: "a@s.whatsapp.net".into(), ids: vec!["m2".into()], here: true, ..Read::default() };
        written(&db, &[Change::Read(here)]);
        assert_eq!(owed_ids(&db, 1), ["m2"]);
        // Now known, an earlier position from WhatsApp only tells it more, and moves nothing back.
        written(&db, &[Change::Read(Read { chat: "a@s.whatsapp.net".into(), at: 1, ..Read::default() })]);
        assert_eq!(owed_ids(&db, 1), ["m2"]);
        written(
            &db,
            &[Change::Read(Read { chat: "a@s.whatsapp.net".into(), ids: vec!["m2".into()], ..Read::default() })],
        );
        assert!(owed_ids(&db, 1).is_empty(), "the phone read it too");
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

    /// How long each step took: regrouping, then letting go.
    type Steps = (Vec<Duration>, Vec<Duration>);

    /// A group of 1,024 members and you, MESSAGES of yours read by the members `read` picks, then
    /// those `leave` picks leaving in one change, and every step of regrouping and letting go
    /// applied alone. Each step is checked against what it may do, which is the same however fast
    /// the machine is.
    async fn in_a_thousand(read: fn(usize) -> bool, leave: fn(usize) -> bool) -> Steps {
        use crate::store::{Change, Lane, Members, Store};
        use crate::work::Priority::Interactive;
        const HUGE: &str = "120363000000000004@g.us";
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, group(HUGE, (1..=1_024).map(member))).await.unwrap();
        let sends = (0..MESSAGES).map(|n| Change::Outbox(in_group(HUGE, format!("h{n}")))).collect();
        store.apply(Lane::Live, sends).await.unwrap();
        // Written straight in: 32,000 receipts through the writer would take a debug build
        // minutes, and nothing here depends on how they came.
        let readers: Vec<String> = (1..=1_024).filter(|&m| read(m)).map(member).collect();
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        db.execute(
            "INSERT INTO receipts (message, recipient, read_at) SELECT r.message, r.member, 2000 FROM recipients r
            JOIN person_jids j ON j.n = r.member WHERE j.jid IN (SELECT value FROM json_each(?1))",
            [serde_json::to_string(&readers).unwrap()],
        )
        .unwrap();
        drop(db);
        let remove = (1..=1_024).filter(|&m| leave(m)).map(member).collect();
        let left = Members { chat: HUGE.into(), all: false, set: Vec::new(), remove };
        store.apply(Lane::Live, vec![Change::Members(left)]).await.unwrap();
        let (mut regrouping, mut letting_go) = (Vec::new(), Vec::new());
        loop {
            let next = store
                .reads()
                .query(Interactive, |db| {
                    let Some(&(chat, member)) = regroups(db, 1)?.first() else { return Ok(None) };
                    let after: i64 = db.query_row(REGROUP_FROM, [chat, member], |r| r.get(0))?;
                    Ok(Some((chat, member, plan(db, chat, member, after)?)))
                })
                .await
                .unwrap();
            let Some((chat, member, plan)) = next else { break };
            let looks = plan.messages.iter().filter(|(_, look)| look.is_some()).count();
            assert!(looks <= REGROUP_AT_ONCE && (looks <= 1 || plan.work <= REGROUP_WORK), "{plan:?}");
            let step = Change::Outbox(Step::Regroup { chat, member });
            regrouping.push(store.apply(Lane::Background, vec![step]).await.unwrap().took);
        }
        assert!(statuses(&store, HUGE).await.iter().all(|s| s == "read"), "{:?}", statuses(&store, HUGE).await);
        loop {
            let settled = store.reads().query(Interactive, |db| settled_messages(db, 1)).await.unwrap();
            let Some(&message) = settled.first() else { break };
            let before = store.count("recipients").unwrap();
            let step = Change::Outbox(Step::LetGo { message });
            letting_go.push(store.apply(Lane::Background, vec![step]).await.unwrap().took);
            let gone = before - store.count("recipients").unwrap();
            assert!(gone <= LET_GO_AT_ONCE as u64, "a step let go of {gone}");
        }
        assert_eq!(letting_go.len(), MESSAGES * 9, "1,024 recipients a message, 128 a step, then none left");
        assert_eq!(store.count("recipients").unwrap(), 0, "all let go of once read by all");
        // A commit that meets the checkpointer's pass waits for its flushes of the drive, which
        // it starts each time about 1,024 frames wait, every ten or so steps here; other
        // processes' I/O can stall one too. So the wall clock is allowed one step in thirty past
        // 3 ms, but none past what two flushes take; what a step may do is checked above, exactly.
        if !cfg!(debug_assertions) {
            for (what, took) in [("regrouping", &regrouping), ("letting go", &letting_go)] {
                let over = took.iter().filter(|took| **took >= Duration::from_millis(3)).count();
                assert!(over <= 1 + took.len() / 30, "{over} steps of {what} took 3 ms or more: {took:?}");
                let slowest = took.iter().max().unwrap();
                assert!(*slowest < Duration::from_millis(15), "a step of {what} took {slowest:?}");
            }
        }
        (regrouping, letting_go)
    }

    const MESSAGES: usize = 32;

    #[tokio::test(flavor = "multi_thread")]
    async fn in_a_group_of_a_thousand_a_step_of_regrouping_looks_at_one_message_so_it_stays_short() {
        let (regrouping, _) = in_a_thousand(|m| m != 1, |m| m == 1).await;
        assert_eq!(regrouping.len(), MESSAGES, "a message a step");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn when_most_of_a_thousand_leave_steps_are_sized_by_who_was_sent_to_and_skip_what_is_read() {
        let (regrouping, _) = in_a_thousand(|m| m > 1_000, |m| m <= 1_000).await;
        assert_eq!(
            regrouping.len(),
            MESSAGES + 999,
            "the first to be looked at, a message a step since each was sent to 1,024; the rest, \
            once each is read, a step each"
        );
    }

    const ANA: &str = "351910000002@s.whatsapp.net";
    const ANA_LID: &str = "100000000002@lid";

    fn from_ana(chat: &str, id: &str, at: i64) -> Message {
        Message {
            chat: chat.into(),
            id: id.into(),
            at,
            sender: Some(chat.into()),
            kind: "text",
            live: true,
            ..Message::default()
        }
    }

    fn sending(chat: &str, id: &str, at: i64, key: Option<&str>) -> crate::store::Change {
        let message = Message { chat: chat.into(), ..mine(id, at) };
        crate::store::Change::Outbox(Step::Enqueue {
            message: Box::new(message),
            kind: "text",
            raw: vec![1],
            key: key.map(str::to_string),
            written: Written::default(),
        })
    }

    async fn on(store: &crate::store::Store, query: impl FnOnce(&Connection) -> Result<i64> + Send + 'static) -> i64 {
        store.reads().query(crate::work::Priority::Interactive, query).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn merging_a_persons_two_chats_keeps_the_later_position_the_earlier_told_and_every_send() {
        use crate::store::{Change, Lane, Person, Read, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let apply = |changes: Vec<Change>| store.apply(Lane::Live, changes);
        // Under her number, read up to p2 on the phone; under her LID, l1 read here and not yet
        // told, and three messages of yours: s1 still to send, "dup" and "went", each also in the
        // number's chat as copies of their own, still pending and already delivered.
        apply(vec![
            Change::Message(from_ana(ANA, "p1", 1_000)),
            Change::Message(from_ana(ANA, "p2", 3_000)),
            Change::Read(Read { chat: ANA.into(), ids: vec!["p2".into()], ..Read::default() }),
            Change::Message(from_ana(ANA_LID, "l1", 2_000)),
            Change::Read(Read { chat: ANA_LID.into(), ids: vec!["l1".into()], here: true, ..Read::default() }),
            sending(ANA_LID, "s1", 4_000, Some("compose-1")),
            sending(ANA_LID, "dup", 4_100, None),
            sending(ANA_LID, "went", 4_200, None),
            Change::Message(Message { chat: ANA.into(), live: false, ..mine("dup", 4_100) }),
            Change::Message(Message {
                chat: ANA.into(),
                live: false,
                status: Some("delivered"),
                ..mine("went", 4_200)
            }),
        ])
        .await
        .unwrap();
        let lid =
            on(&store, |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [ANA_LID], |r| r.get(0))?)).await;
        let number = on(&store, |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [ANA], |r| r.get(0))?)).await;
        let owed_lid =
            store.reads().query(crate::work::Priority::Interactive, move |db| Ok(owed_ids(db, lid))).await.unwrap();
        assert_eq!(owed_lid, ["l1"]);
        apply(vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })])
            .await
            .unwrap();
        let during = store.reads().query(crate::work::Priority::Interactive, move |db| owed(db, number)).await.unwrap();
        assert_eq!(during, Owing::Merging { into: number }, "nothing is worked out while messages are still moving");
        loop {
            let merging =
                on(&store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0))?)).await;
            if merging == 0 {
                break;
            }
            store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        }
        let owed_now =
            store.reads().query(crate::work::Priority::Interactive, move |db| Ok(owed_ids(db, number))).await.unwrap();
        assert_eq!(
            owed_now,
            ["l1", "p2"],
            "from before the LID's oldest WhatsApp wasn't told of, to the later position"
        );
        let unread =
            on(&store, move |db| Ok(db.query_row("SELECT unread FROM chats WHERE n = ?1", [number], |r| r.get(0))?))
                .await;
        assert_eq!(unread, 0);
        let sends: Vec<(String, String)> = store
            .reads()
            .query(crate::work::Priority::Interactive, |db| {
                Ok(db
                    .prepare("SELECT o.id, c.id FROM outbox o JOIN messages m ON m.n = o.message JOIN chats c ON c.n = m.chat ORDER BY o.id")?
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await
            .unwrap();
        assert_eq!(
            sends,
            [("dup".into(), ANA.into()), ("s1".into(), ANA.into())],
            "s1 followed its message, dup's send went to the copy kept, and went's had gone"
        );
        let queued = store.reads().query(crate::work::Priority::Interactive, queued).await.unwrap();
        assert!(queued.iter().all(|(_, chat)| chat == ANA), "{queued:?}");
        let key = store.reads().query(crate::work::Priority::Interactive, |db| keyed(db, "compose-1")).await.unwrap();
        assert_eq!(key, Some((ANA.to_string(), "s1".to_string())));
        assert_eq!(store.count("outbox WHERE message IS NULL").unwrap(), 0, "no copy's send left over");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_here_during_a_merge_reaches_the_chat_merging_and_tells_whatsapp_only_once_receipts_go() {
        use crate::store::{Change, Lane, Person, Read, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let apply = |changes: Vec<Change>| store.apply(Lane::Live, changes);
        apply(vec![
            Change::Message(from_ana(ANA_LID, "g1", 2_000)),
            Change::Message(from_ana(ANA, "k1", 1_000)),
            Change::Message(from_ana(ANA, "k2", 3_000)),
            Change::Read(Read { chat: ANA.into(), ids: vec!["k1".into()], ..Read::default() }),
        ])
        .await
        .unwrap();
        let n_of = |jid: &'static str| {
            on(&store, move |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [jid], |r| r.get(0))?))
        };
        let (number, lid) = (n_of(ANA).await, n_of(ANA_LID).await);
        apply(vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })])
            .await
            .unwrap();
        // The merge has begun and no step has run: g1 is still in the LID chat.
        apply(vec![Change::Read(Read { chat: ANA.into(), ids: vec!["k2".into()], here: true, ..Read::default() })])
            .await
            .unwrap();
        let unread = |chat: i64| {
            on(&store, move |db| Ok(db.query_row("SELECT unread FROM chats WHERE n = ?1", [chat], |r| r.get(0))?))
        };
        assert_eq!(
            (unread(number).await, unread(lid).await),
            (0, 0),
            "reading the kept chat reads the one merging into it"
        );
        let told = |chat: i64| {
            store.reads().query(crate::work::Priority::Interactive, move |db| {
                Ok(db.query_row("SELECT told_at, told_n FROM chats WHERE n = ?1", [chat], |r| {
                    Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?))
                })?)
            })
        };
        assert_eq!(told(lid).await.unwrap(), (None, None), "nothing was told of the LID chat");
        let during = store.reads().query(crate::work::Priority::Interactive, move |db| owed(db, number)).await.unwrap();
        assert_eq!(during, Owing::Merging { into: number }, "and nothing is, until g1 is in the chat kept");
        loop {
            let merging =
                on(&store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0))?)).await;
            if merging == 0 {
                break;
            }
            store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        }
        let Owing::Receipts(lot) =
            store.reads().query(crate::work::Priority::Interactive, move |db| owed(db, number)).await.unwrap()
        else {
            panic!("receipts are owed once the merge is done");
        };
        let ids: Vec<&str> = lot.messages.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["g1", "k2"]);
        assert!(told(number).await.unwrap().0.is_some(), "WhatsApp is told of them only once they've gone");
        store
            .apply(Lane::Live, vec![Change::Outbox(Step::Told { chat: number, from: lot.from, upto: lot.upto })])
            .await
            .unwrap();
        assert_eq!(told(number).await.unwrap(), (None, None));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_deleted_for_you_before_it_goes_still_goes_as_on_the_phone() {
        use crate::store::{Change, Lane, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, vec![sending("a@s.whatsapp.net", "m1", 1_000, Some("compose-1"))]).await.unwrap();
        let delete = Change::DeleteMessage { chat: "a@s.whatsapp.net".into(), id: "m1".into() };
        store.apply(Lane::Live, vec![delete]).await.unwrap();
        let written = Written::default();
        let again = Step::Enqueue {
            message: Box::new(Message { chat: "a@s.whatsapp.net".into(), ..mine("m1-again", 1_100) }),
            kind: "text",
            raw: vec![1],
            key: Some("compose-1".into()),
            written: written.clone(),
        };
        store.apply(Lane::Live, vec![Change::Outbox(again)]).await.unwrap();
        assert_eq!(
            written.take(),
            Some(Enqueued::Queued {
                message: None,
                id: "m1".into(),
                chat: "a@s.whatsapp.net".into(),
                chat_n: None,
                outbox: None
            }),
            "asking again with its key answers with the message deleted, which is still on its way"
        );
        assert_eq!((store.count("outbox").unwrap(), store.count("messages").unwrap()), (1, 0), "and sends nothing new");
        let row =
            store.reads().query(crate::work::Priority::Interactive, |db| outboxed_by_n(db, 1)).await.unwrap().unwrap();
        assert_eq!((row.chat.as_str(), row.id.as_str(), row.state.as_str()), ("a@s.whatsapp.net", "m1", "queued"));
        assert_eq!(store.count("messages").unwrap(), 0, "it's gone from the chat");
        store.apply(Lane::Live, vec![Change::Outbox(Step::Sent { outbox: 1 })]).await.unwrap();
        assert_eq!(store.count("outbox").unwrap(), 0, "and from the outbox once the server has it");
        store.apply(Lane::Live, vec![sending("a@s.whatsapp.net", "m2", 2_000, None)]).await.unwrap();
        let delete = Change::DeleteMessage { chat: "a@s.whatsapp.net".into(), id: "m2".into() };
        store.apply(Lane::Live, vec![delete]).await.unwrap();
        store
            .apply(Lane::Live, vec![Change::Outbox(Step::Failed { outbox: 2, error: "no".into(), next_at: None })])
            .await
            .unwrap();
        assert_eq!(store.count("outbox").unwrap(), 0, "refused, with nothing to show it or retry it");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_message_isnt_pending_again_but_by_a_retry() {
        use crate::store::{Change, Lane, Store};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, vec![sending("a@s.whatsapp.net", "m1", 1_000, None)]).await.unwrap();
        store
            .apply(Lane::Live, vec![Change::Outbox(Step::Failed { outbox: 1, error: "no".into(), next_at: None })])
            .await
            .unwrap();
        let copy = Message { chat: "a@s.whatsapp.net".into(), live: false, ..mine("m1", 1_000) };
        store.apply(Lane::Live, vec![Change::Message(copy)]).await.unwrap();
        let status = || {
            store.reads().query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT status FROM messages", [], |r| r.get::<_, String>(0))?)
            })
        };
        assert_eq!(status().await.unwrap(), "failed", "a copy saying pending doesn't take it back");
        store.apply(Lane::Live, vec![Change::Outbox(Step::Retry { outbox: 1 })]).await.unwrap();
        assert_eq!(status().await.unwrap(), "pending");
    }

    #[test]
    fn how_far_whatsapp_was_told_never_passes_the_read_position_and_moves_only_from_where_it_was() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        db.execute(
            "INSERT INTO chats (n, id, kind, v, read_at, read_n) VALUES (1, 'a@s.whatsapp.net', 'direct', 1, 5, 3)",
            [],
        )
        .unwrap();
        let told = |at: i64, n: i64| db.execute("UPDATE chats SET told_at = ?1, told_n = ?2", [at, n]);
        assert!(told(6, 1).is_err(), "past the position");
        assert!(db.execute("UPDATE chats SET told_at = 5", []).is_err(), "half a place");
        told(5, 1).unwrap();
        let at = || -> (Option<i64>, Option<i64>) {
            db.query_row("SELECT told_at, told_n FROM chats", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        db.execute(TOLD, params![1, 4, 9, 5, 2]).unwrap();
        assert_eq!(at(), (Some(5), Some(1)), "not from where it was told");
        db.execute(TOLD, params![1, 5, 1, 5, 2]).unwrap();
        assert_eq!(at(), (Some(5), Some(2)));
        db.execute(TOLD, params![1, 5, 2, 5, 3]).unwrap();
        assert_eq!(at(), (None, None), "once it's at the position, WhatsApp knows as much as jaw");
    }
}
