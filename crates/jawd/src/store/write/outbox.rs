//! The outbox and read positions: what sending writes and reads. A message you send is written as
//! pending with its outbox row in one transaction, so it shows at once and is sent however jawd
//! stops; the server's answer moves it to sent and removes the row; a failure counts an attempt,
//! and the last one leaves it failed until it's retried by hand. Reading moves the chat's read
//! position forward, unread is counted again from it, and the read receipts owed to WhatsApp are
//! the messages between how far it has been told and that position.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::{CHAT, MESSAGE, Message, Stamps, advanced, chat_jid, now_ms, upsert_message};

/// The most unread messages counted, so a count costs the same however many there are.
pub const UNREAD_COUNTED: i64 = 1_000;
/// The most messages one flush of read receipts names, newest first.
pub const RECEIPTS_AT_ONCE: usize = 1_000;

/// A step for the writer.
#[derive(Clone, Debug)]
pub enum Step {
    /// A message of yours to send: its row, pending, and its outbox row, with `raw`, the
    /// WAWebProtobufsE2E.Message to send. Its chat's draft is cleared, since it's what was sent.
    Enqueue { message: Box<Message>, kind: &'static str, raw: Vec<u8> },
    /// The server has outbox row `outbox`'s message, at its time `at`.
    Sent { outbox: i64, at: Option<i64> },
    /// An attempt failed with `error`; it's tried again at `next_at`, or, without one, it has
    /// failed until it's retried by hand.
    Failed { outbox: i64, error: String, next_at: Option<i64> },
    /// A failed message is queued again by hand, and pending again.
    Retry { outbox: i64 },
    /// You've read `chat` up to message `id`.
    Read { chat: String, id: String },
    /// WhatsApp has been told chat `chat` is read up to the message at (`at`, `n`).
    Told { chat: i64, at: i64, n: i64 },
}

const MESSAGE_BY_ID: &str =
    "SELECT m.n, m.chat FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1 AND m.id = ?2";
const ENQUEUE: &str = "INSERT INTO outbox (message, kind, raw, next_at, created_at) VALUES (?1, ?2, ?3, ?4, ?4)
    ON CONFLICT (message) DO NOTHING";
const CLEAR_DRAFT: &str = "UPDATE chats SET draft = NULL, v = ?2 WHERE n = ?1 AND draft IS NOT NULL";
const OUTBOXED_MESSAGE: &str =
    "SELECT m.n, m.chat, m.status FROM outbox o JOIN messages m ON m.n = o.message WHERE o.n = ?1";
const CHAT_SHOWS: &str = "UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2";
const DEQUEUE: &str = "DELETE FROM outbox WHERE n = ?1";
const SENT: &str = "UPDATE messages SET status = ?2, at = coalesce(?3, at), v = ?4
    WHERE n = ?1 AND (status IS NOT ?2 OR at IS NOT coalesce(?3, at))";
const SENT_AT: &str = "UPDATE chats SET last_at = ?3 WHERE n = ?1 AND last_message = ?2";
const ATTEMPTED: &str = "UPDATE outbox SET attempts = attempts + 1, error = ?2, next_at = coalesce(?3, next_at),
        state = iif(?3 IS NULL, 'failed', 'queued')
    WHERE n = ?1";
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
const SET_UNREAD: &str = "UPDATE chats SET unread = ?2, mentions = iif(?2 = 0, 0, mentions), marked_unread = 0, v = ?3
    WHERE n = ?1 AND (unread IS NOT ?2 OR (?2 = 0 AND mentions <> 0) OR marked_unread <> 0)";
const NEWEST_FROM_OTHERS: &str = "SELECT at, n FROM messages WHERE chat = ?1 AND mine = 0 AND (at, n) <= (?2, ?3)
    ORDER BY at DESC, n DESC LIMIT ?4";
const TOLD: &str = "UPDATE reads SET told_at = ?2, told_n = ?3
    WHERE chat = ?1 AND (?2, ?3) > (told_at, told_n) AND (?2, ?3) <= (at, n)";
const QUEUED: &str = "SELECT o.n, c.id FROM outbox o JOIN messages m ON m.n = o.message JOIN chats c ON c.n = m.chat
    WHERE o.state = 'queued' ORDER BY o.n";
const WAITING: &str = "SELECT count(*) FROM outbox WHERE state = 'queued'";
const OUTBOXED: &str = "SELECT o.n, c.id, m.id, o.raw, o.state = 'queued', m.status
    FROM outbox o CROSS JOIN messages m ON m.n = o.message JOIN chats c ON c.n = m.chat";
const OWING: &str = "SELECT chat FROM reads WHERE (told_at, told_n) < (at, n)";
const OWED_CHAT: &str = "SELECT c.id, c.kind, r.at, r.n, r.told_at, r.told_n FROM reads r JOIN chats c ON c.n = r.chat
    WHERE r.chat = ?1 AND (r.told_at, r.told_n) < (r.at, r.n)";
const OWED_MESSAGES: &str = "SELECT m.id, j.jid FROM messages m LEFT JOIN person_jids j ON j.n = m.sender
    WHERE m.chat = ?1 AND m.mine = 0 AND (m.at, m.n) > (?2, ?3) AND (m.at, m.n) <= (?4, ?5)
    ORDER BY m.at DESC, m.n DESC LIMIT ?6";

/// Where a step that failed came from, for the quarantine.
pub(super) fn provenance(step: &Step) -> (&'static str, String, Option<&[u8]>) {
    match step {
        Step::Enqueue { message, .. } => {
            ("send", format!("{} {}", message.chat, message.id), message.raw.as_ref().map(|p| p.0.as_slice()))
        }
        Step::Sent { outbox, .. } | Step::Failed { outbox, .. } | Step::Retry { outbox } => {
            ("outbox", outbox.to_string(), None)
        }
        Step::Read { chat, id } => ("read", format!("{chat} {id}"), None),
        Step::Told { chat, n, .. } => ("told", format!("{chat} {n}"), None),
    }
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, step: &Step) -> Result<bool> {
    match step {
        Step::Enqueue { message, kind, raw } => enqueue(db, stamps, message, kind, raw),
        Step::Sent { outbox, at } => sent(db, stamps, *outbox, *at),
        Step::Failed { outbox, error, next_at } => failed(db, stamps, *outbox, error, *next_at),
        Step::Retry { outbox } => retry(db, stamps, *outbox),
        Step::Read { chat, id } => read(db, stamps, chat, id),
        Step::Told { chat, at, n } => {
            db.prepare_cached(TOLD)?.execute(params![chat, at, n])?;
            Ok(false)
        }
    }
}

fn enqueue(db: &Connection, stamps: &mut Stamps, message: &Message, kind: &str, raw: &[u8]) -> Result<bool> {
    let chat = chat_jid(db, &message.chat)?;
    let resolved;
    let message = if chat == message.chat {
        message
    } else {
        resolved = Message { chat: chat.clone(), ..message.clone() };
        &resolved
    };
    let changed = upsert_message(db, stamps, message, false)?;
    let (n, chat): (i64, i64) = db
        .prepare_cached(MESSAGE_BY_ID)?
        .query_row(params![message.chat, message.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    db.prepare_cached(ENQUEUE)?.execute(params![n, kind, raw, now_ms()])?;
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached(CLEAR_DRAFT)?.execute([chat, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
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

fn sent(db: &Connection, stamps: &mut Stamps, outbox: i64, at: Option<i64>) -> Result<bool> {
    let Some((n, chat, status)) = outboxed(db, outbox)? else { return Ok(false) };
    db.prepare_cached(DEQUEUE)?.execute([outbox])?;
    let status = advanced(status.as_deref(), Some("sent"));
    let seq = stamps.seq(MESSAGE, Some(n));
    if db.prepare_cached(SENT)?.execute(params![n, status, at, seq])? == 0 {
        return Ok(false);
    }
    if let Some(at) = at {
        db.prepare_cached(SENT_AT)?.execute(params![chat, n, at.max(0)])?;
    }
    touched(db, stamps, chat, n, seq)?;
    Ok(true)
}

fn failed(db: &Connection, stamps: &mut Stamps, outbox: i64, error: &str, next_at: Option<i64>) -> Result<bool> {
    let Some((n, chat, status)) = outboxed(db, outbox)? else { return Ok(false) };
    db.prepare_cached(ATTEMPTED)?.execute(params![outbox, error, next_at])?;
    if next_at.is_some() || advanced(status.as_deref(), Some("failed")) == status.as_deref() {
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
/// messages from others as it had unread.
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

/// An outbox row as the sender takes it.
#[derive(Clone, Debug)]
pub struct Outboxed {
    pub outbox: i64,
    pub chat: String,
    pub id: String,
    pub raw: Vec<u8>,
    pub queued: bool,
    pub status: Option<String>,
}

/// The rows waiting to be sent, oldest first, as `(outbox, chat)`.
pub fn queued(db: &Connection) -> Result<Vec<(i64, String)>> {
    Ok(db.prepare_cached(QUEUED)?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
}

/// How many messages wait to be sent, failed ones apart.
pub fn waiting(db: &Connection) -> Result<u64> {
    let n: i64 = db.prepare_cached(WAITING)?.query_row([], |r| r.get(0))?;
    Ok(n.max(0) as u64)
}

fn outboxed_row(r: &rusqlite::Row) -> rusqlite::Result<Outboxed> {
    Ok(Outboxed {
        outbox: r.get(0)?,
        chat: r.get(1)?,
        id: r.get(2)?,
        raw: r.get(3)?,
        queued: r.get(4)?,
        status: r.get(5)?,
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

/// Read receipts owed in one chat: its JID, how far it's read, and the messages from others
/// between how far WhatsApp has been told and there, newest first, each with who sent it.
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

/// What chat `chat` owes WhatsApp, if anything.
pub fn owed(db: &Connection, chat: i64) -> Result<Option<Owed>> {
    let found: Option<(String, String, i64, i64, i64, i64)> = db
        .prepare_cached(OWED_CHAT)?
        .query_row([chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .optional()?;
    let Some((jid, kind, at, n, told_at, told_n)) = found else { return Ok(None) };
    let messages = db
        .prepare_cached(OWED_MESSAGES)?
        .query_map(params![chat, told_at, told_n, at, n, RECEIPTS_AT_ONCE as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(Owed { chat, jid, group: kind != "direct", upto: (at, n), messages }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema;

    /// Every statement sending runs, as SQLite plans it on a fresh database with no statistics, as
    /// jawd's always is: each reads an index, never scans a big table and never sorts in a
    /// temporary b-tree. The outbox and reads are small, a row per message waiting and per chat
    /// read, and may be read whole.
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
            ("dequeue", DEQUEUE, "SEARCH outbox USING INTEGER PRIMARY KEY (rowid=?)"),
            ("sent", SENT, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            ("sent at", SENT_AT, "SEARCH chats USING INTEGER PRIMARY KEY (rowid=?)"),
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
        let enqueue = |id: &str| Step::Enqueue { message: Box::new(mine(id, 1_000)), kind: "text", raw: vec![1] };
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
        store.apply(Lane::Live, vec![Change::Amend(receipt)]).await.unwrap();
        apply(Step::Sent { outbox: 1, at: Some(1_500) }).await.unwrap();
        assert_eq!(
            state("m1").await.unwrap(),
            ("delivered".into(), 1_500, None),
            "sent comes after delivered and changes only its time"
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
        apply(Step::Sent { outbox, at: None }).await.unwrap();
        let retried = apply(Step::Retry { outbox }).await.unwrap();
        assert_eq!((state("m2").await.unwrap().0.as_str(), retried.changed), ("sent", 0), "nothing takes sent back");
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
