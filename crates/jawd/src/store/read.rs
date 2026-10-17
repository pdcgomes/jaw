//! The read pool: read-only connections to jaw.db, each on a thread of its own, one per core up
//! to 4, taking interactive queries ahead of background ones. A tokio task hands a query over and
//! awaits its answer, so it never blocks on SQLite, and in WAL mode a reader never waits for the
//! writer. The queries are keyset queries: a page costs the same at any depth.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, select_biased, unbounded};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde_json::Value;
use serde_json::value::RawValue;
use tokio::sync::oneshot;

use crate::work::{Priority, Report, Samples};

/// The most readers, whatever the cores.
const MOST_READERS: usize = 4;

struct Job {
    queued: Instant,
    interactive: bool,
    run: Box<dyn FnOnce(&Connection) + Send>,
}

#[derive(Default)]
struct Stats {
    running: u32,
    done: u64,
    wait_interactive: Samples,
    wait_background: Samples,
    run: Samples,
}

/// Hands queries to the readers. Readers end once every `Reads` is dropped.
#[derive(Clone)]
pub struct Reads {
    interactive: Sender<Job>,
    background: Sender<Job>,
    workers: usize,
    stats: Arc<Mutex<Stats>>,
}

impl Reads {
    /// Opens the readers on jaw.db at `path`, whose schema must already be up to date.
    pub(super) fn open(path: &Path) -> Result<Reads> {
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MOST_READERS);
        let (interactive, interactive_rx) = unbounded();
        let (background, background_rx) = unbounded();
        let stats = Arc::new(Mutex::new(Stats::default()));
        for n in 0..workers {
            let db = super::connect(path, super::Role::Reader)
                .with_context(|| format!("can't open {} to read", path.display()))?;
            let (interactive, background, stats) = (interactive_rx.clone(), background_rx.clone(), stats.clone());
            std::thread::Builder::new()
                .name(format!("jawd-reader-{n}"))
                .spawn(move || read(db, interactive, background, stats))?;
        }
        Ok(Reads { interactive, background, workers, stats })
    }

    /// Runs `query` on a reader, interactive queries ahead of background ones, and returns its answer.
    pub async fn query<T, F>(&self, priority: Priority, query: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    {
        let (answer, answered) = oneshot::channel();
        let job = Job {
            queued: Instant::now(),
            interactive: priority == Priority::Interactive,
            run: Box::new(move |db| {
                let _ = answer.send(query(db));
            }),
        };
        let lane = if job.interactive { &self.interactive } else { &self.background };
        lane.send(job).map_err(|_| anyhow::anyhow!("jawd's readers have stopped"))?;
        answered.await.map_err(|_| anyhow::anyhow!("a reader stopped before it answered"))?
    }
}

fn read(db: Connection, interactive: Receiver<Job>, background: Receiver<Job>, stats: Arc<Mutex<Stats>>) {
    loop {
        let job = select_biased! {
            recv(interactive) -> job => job,
            recv(background) -> job => job,
        };
        let Ok(job) = job else { return };
        let started = Instant::now();
        {
            let mut stats = stats.lock().expect("the readers' stats");
            stats.running += 1;
            let waits = if job.interactive { &mut stats.wait_interactive } else { &mut stats.wait_background };
            waits.add(started - job.queued);
        }
        (job.run)(&db);
        let mut stats = stats.lock().expect("the readers' stats");
        stats.running -= 1;
        stats.done += 1;
        stats.run.add(started.elapsed());
    }
}

impl Report for Reads {
    fn report(&self) -> jaw_proto::Pool {
        let stats = self.stats.lock().expect("the readers' stats");
        jaw_proto::Pool {
            name: "reads".into(),
            workers: self.workers as u32,
            running: stats.running,
            queued_interactive: self.interactive.len() as u32,
            queued_background: self.background.len() as u32,
            done: stats.done,
            wait_interactive: stats.wait_interactive.percentiles(),
            wait_background: stats.wait_background.percentiles(),
            run: stats.run.percentiles(),
        }
    }
}

/// A chat, as the chat list shows it.
#[derive(Debug, Serialize)]
pub struct ChatRow {
    pub n: i64,
    pub id: String,
    pub v: i64,
    /// Pinned first, then by newest message, compared bytewise and read from the top down.
    pub sort: String,
    pub kind: String,
    pub name: String,
    pub person: Option<i64>,
    pub avatar: Option<String>,
    pub last: Option<Last>,
    pub unread: i64,
    pub mentions: i64,
    pub pinned: bool,
    pub muted_until: Option<i64>,
    pub archived: bool,
    pub marked_unread: bool,
    pub draft: Option<String>,
    pub community: Option<String>,
}

/// A chat's newest message, for its line in the chat list.
#[derive(Debug, Serialize)]
pub struct Last {
    pub from: Option<String>,
    pub mine: bool,
    pub summary: String,
    pub kind: String,
    pub status: Option<String>,
    pub at: i64,
}

/// Where a page of the chat list starts: after this chat.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChatCursor {
    sort: String,
    n: i64,
}

impl ChatRow {
    pub fn cursor(&self) -> ChatCursor {
        ChatCursor { sort: self.sort.clone(), n: self.n }
    }
}

/// A message, ready to draw.
#[derive(Debug, Serialize)]
pub struct MessageRow {
    pub n: i64,
    pub chat: String,
    pub id: String,
    pub v: i64,
    pub at: i64,
    pub from: Option<From>,
    pub mine: bool,
    pub kind: String,
    pub status: Option<String>,
    pub spans: Box<RawValue>,
    pub summary: String,
    pub reply: Option<Box<RawValue>>,
    pub detail: Option<Box<RawValue>>,
    pub forwarded: i64,
    pub edited: bool,
    pub deleted: bool,
    pub expires_at: Option<i64>,
    pub starred: bool,
    pub view_once: bool,
    /// Each emoji, how many reacted with it, and whether you did, in the order they first came.
    pub reactions: Box<RawValue>,
}

/// Who sent a message: the person and the name to show.
#[derive(Debug, Serialize)]
pub struct From {
    pub id: i64,
    pub name: String,
}

/// A message's place in its chat, for paging from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MessageCursor {
    at: i64,
    n: i64,
}

impl MessageRow {
    pub fn cursor(&self) -> MessageCursor {
        MessageCursor { at: self.at, n: self.n }
    }
}

/// Which page of a chat's messages.
#[derive(Clone, Copy, Debug)]
pub enum Around {
    /// The newest.
    Newest,
    /// The ones just before this message, not including it.
    Before(MessageCursor),
    /// The ones just after this message, not including it.
    After(MessageCursor),
}

/// The name to show for someone with these names, or their number when jaw knows no name.
pub(crate) fn shown(
    name: Option<String>,
    push_name: Option<String>,
    verified: Option<String>,
    jid: Option<&str>,
) -> String {
    name.or(push_name).or(verified).unwrap_or_else(|| match jid.and_then(|jid| jid.split_once('@')) {
        Some((user, "s.whatsapp.net")) => format!("+{user}"),
        Some((user, _)) => user.to_string(),
        None => String::new(),
    })
}

const CHAT_COLUMNS: &str = "
    SELECT c.n, c.id, c.v, c.sort, c.kind, c.name, c.person, c.avatar, c.unread, c.mentions, c.pinned, c.muted_until,
        c.archived, c.marked_unread, c.draft, c.community, p.name, p.push_name, p.verified_name,
        m.n, m.at, m.mine, m.kind, m.status, coalesce(m.summary, m.text, ''), mp.name, mp.push_name,
        mp.verified_name, mj.jid,
        iif(c.name IS NULL AND c.kind <> 'direct', (SELECT group_concat(shown, ', ') FROM (
            SELECT coalesce(gp.name, gp.push_name, gp.verified_name) AS shown FROM group_members g
                JOIN person_jids gj ON gj.n = g.member JOIN people gp ON gp.n = gj.person
            WHERE g.chat = c.n AND shown IS NOT NULL LIMIT 4)), NULL),
        m.edited_at,
        (SELECT json_group_array(json_object('at', created_at, 'effect', json(effect)))
            FROM (SELECT created_at, effect FROM outbox
                WHERE target = c.last_message AND effect IS NOT NULL AND state <> 'failed' ORDER BY n))
    FROM chats c
    LEFT JOIN people p ON p.n = c.person
    LEFT JOIN messages m ON m.n = c.last_message
    LEFT JOIN person_jids mj ON mj.n = m.sender
    LEFT JOIN people mp ON mp.n = mj.person";

fn chat_row(r: &Row) -> rusqlite::Result<ChatRow> {
    let id: String = r.get(1)?;
    // A group without a subject goes by its members' names, as the phone shows it.
    let name = match (r.get::<_, Option<String>>(5)?, r.get::<_, Option<String>>(29)?) {
        (Some(name), _) | (None, Some(name)) => name,
        (None, None) => shown(r.get(16)?, r.get(17)?, r.get(18)?, Some(&id)),
    };
    let last = match r.get::<_, Option<i64>>(19)? {
        Some(_) => {
            let from = match r.get::<_, Option<String>>(28)? {
                Some(jid) => Some(shown(r.get(25)?, r.get(26)?, r.get(27)?, Some(&jid))),
                None => None,
            };
            let mut summary: String = r.get(24)?;
            super::overlay::last(&mut summary, r.get(30)?, r.get::<_, Option<String>>(31)?.as_deref());
            Some(Last { from, at: r.get(20)?, mine: r.get(21)?, kind: r.get(22)?, status: r.get(23)?, summary })
        }
        None => None,
    };
    let muted_until: i64 = r.get(11)?;
    Ok(ChatRow {
        n: r.get(0)?,
        id,
        v: r.get(2)?,
        sort: r.get(3)?,
        kind: r.get(4)?,
        name,
        person: r.get(6)?,
        avatar: r.get(7)?,
        unread: r.get(8)?,
        mentions: r.get(9)?,
        pinned: r.get::<_, i64>(10)? > 0,
        muted_until: (muted_until != 0).then_some(muted_until),
        archived: r.get(12)?,
        marked_unread: r.get(13)?,
        draft: r.get(14)?,
        community: r.get(15)?,
        last,
    })
}

/// A page of the chat list, pinned first and then by newest message: the first `limit` chats, or
/// the next `limit` after `after`. Archived chats are a list of their own.
pub fn chats(db: &Connection, archived: bool, after: Option<&ChatCursor>, limit: usize) -> Result<Vec<ChatRow>> {
    let (sort, n) = after.map_or(("2", i64::MAX), |after| (after.sort.as_str(), after.n));
    let sql = format!(
        "{CHAT_COLUMNS} WHERE c.archived = ?1 AND (c.sort, c.n) < (?2, ?3) ORDER BY c.sort DESC, c.n DESC LIMIT ?4"
    );
    let rows = db
        .prepare_cached(&sql)?
        .query_map(params![archived, sort, n, limit as i64], chat_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// A chat by its n, as the chat list shows it.
pub fn chat(db: &Connection, n: i64) -> Result<Option<ChatRow>> {
    let sql = format!("{CHAT_COLUMNS} WHERE c.n = ?1");
    Ok(db.prepare_cached(&sql)?.query_row([n], chat_row).optional()?)
}

/// What message_row reads, from `messages m` joined with MESSAGE_JOINS. A quote names who wrote
/// it as a mention does: by their name once jaw knows it, "You" for yours, and by their JID until
/// then.
const MESSAGE_COLUMNS: &str = "
    SELECT m.n, c.id, m.id, m.v, m.at, m.mine, m.kind, m.status, m.text, m.spans, coalesce(m.summary, m.text, ''),
        iif(m.reply ->> 'from' IS NULL, m.reply, json_set(m.reply, '$.from', coalesce(
            iif(instr(' ' || coalesce((SELECT value FROM sync_state WHERE key = 'me'), '') || ' ',
                ' ' || (m.reply ->> 'from') || ' ') > 0, 'You', NULL),
            (SELECT coalesce(rp.name, rp.push_name, rp.verified_name) FROM person_jids rj JOIN people rp ON rp.n = rj.person
                WHERE rj.jid = m.reply ->> 'from'),
            m.reply ->> 'from'))),
        m.detail,
        m.forwarded, m.edited_at, m.deleted, m.expires_at, m.starred, m.view_once,
        p.n, p.name, p.push_name, p.verified_name, j.jid,
        (SELECT json_group_array(json_object('emoji', emoji, 'n', n, 'mine', json(iif(mine, 'true', 'false'))))
            FROM (SELECT emoji, count(*) AS n, max(mine) AS mine, min(at) AS first FROM reactions
                WHERE message = m.n GROUP BY emoji ORDER BY first)),
        iif(instr(m.spans, '\"t\":\"mention\"') > 0, (SELECT json_group_object(e.value ->> 'person',
                coalesce(mp.name, mp.push_name, mp.verified_name))
            FROM json_each(m.spans) e JOIN person_jids mj ON mj.jid = e.value ->> 'person' JOIN people mp ON mp.n = mj.person
            WHERE e.value ->> 't' = 'mention' AND coalesce(mp.name, mp.push_name, mp.verified_name) IS NOT NULL), NULL),
        (SELECT json_group_array(json_object('at', created_at, 'effect', json(effect),
                'mine_at', (SELECT max(r.at) FROM reactions r WHERE r.message = m.n AND r.mine)))
            FROM (SELECT created_at, effect FROM outbox WHERE target = m.n AND effect IS NOT NULL AND state <> 'failed'
                ORDER BY n))";

/// How many columns MESSAGE_COLUMNS has, for queries that select more after them.
const MESSAGE_COLUMN_COUNT: usize = 27;

const MESSAGE_JOINS: &str = "
    JOIN chats c ON c.n = m.chat
    LEFT JOIN person_jids j ON j.n = m.sender
    LEFT JOIN people p ON p.n = j.person";

#[derive(Serialize)]
struct Span<'a> {
    t: &'static str,
    s: &'a str,
}

fn json(text: Option<String>) -> rusqlite::Result<Option<Box<RawValue>>> {
    text.map(|text| {
        RawValue::from_string(text)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
    })
    .transpose()
}

/// `spans` with each mention of someone in `names` (their JID to the name to show) said as `@` and
/// that name, as the phones show it; WhatsApp writes `@` and the user part of their JID.
fn mentions_named(spans: String, names: Option<String>) -> String {
    let Some(names) = names.and_then(|names| serde_json::from_str::<serde_json::Map<String, Value>>(&names).ok())
    else {
        return spans;
    };
    let Ok(mut parsed) = serde_json::from_str::<Vec<serde_json::Map<String, Value>>>(&spans) else { return spans };
    for span in &mut parsed {
        let named = match (span.get("t").and_then(Value::as_str), span.get("person").and_then(Value::as_str)) {
            (Some("mention"), Some(person)) => names.get(person).and_then(Value::as_str).map(|name| format!("@{name}")),
            _ => None,
        };
        if let Some(named) = named {
            span.insert("s".into(), Value::String(named));
        }
    }
    serde_json::to_string(&parsed).unwrap_or(spans)
}

fn message_row(r: &Row) -> rusqlite::Result<MessageRow> {
    let text: Option<String> = r.get(8)?;
    let mut spans = match r.get::<_, Option<String>>(9)? {
        Some(spans) => mentions_named(spans, r.get(25)?),
        None => match text.as_deref() {
            Some(text) if !text.is_empty() => serde_json::to_string(&[Span { t: "text", s: text }])
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
            _ => "[]".into(),
        },
    };
    let (mut summary, mut reply, mut detail): (String, Option<String>, Option<String>) =
        (r.get(10)?, r.get(11)?, r.get(12)?);
    let (mut edited_at, mut deleted): (Option<i64>, bool) = (r.get(14)?, r.get(15)?);
    let mut reactions: String = r.get::<_, Option<String>>(24)?.unwrap_or_else(|| "[]".into());
    let row = super::overlay::Shown {
        spans: &mut spans,
        summary: &mut summary,
        reply: &mut reply,
        detail: &mut detail,
        edited_at: &mut edited_at,
        deleted: &mut deleted,
        reactions: &mut reactions,
    };
    super::overlay::message(row, r.get::<_, Option<String>>(26)?.as_deref());
    let from = match r.get::<_, Option<i64>>(19)? {
        Some(id) => {
            let jid: Option<String> = r.get(23)?;
            Some(From { id, name: shown(r.get(20)?, r.get(21)?, r.get(22)?, jid.as_deref()) })
        }
        None => None,
    };
    Ok(MessageRow {
        n: r.get(0)?,
        chat: r.get(1)?,
        id: r.get(2)?,
        v: r.get(3)?,
        at: r.get(4)?,
        mine: r.get(5)?,
        kind: r.get(6)?,
        status: r.get(7)?,
        spans: json(Some(spans))?.expect("spans are always there"),
        summary,
        reply: json(reply)?,
        detail: json(detail)?,
        forwarded: r.get(13)?,
        edited: edited_at.is_some(),
        deleted,
        expires_at: r.get(16)?,
        starred: r.get(17)?,
        view_once: r.get(18)?,
        reactions: json(Some(reactions))?.expect("reactions are always there"),
        from,
    })
}

/// A page of `limit` messages in `chat`, oldest first, by keyset on (chat, time, rowid) and never
/// by offset, so the 50,000th page costs what the first does. An unknown chat has none.
pub fn messages(db: &Connection, chat: &str, around: Around, limit: usize) -> Result<Vec<MessageRow>> {
    let from =
        format!("{MESSAGE_COLUMNS} FROM messages m {MESSAGE_JOINS} WHERE m.chat = (SELECT n FROM chats WHERE id = ?1)");
    let limit = limit as i64;
    let mut rows: Vec<MessageRow> = match around {
        Around::Newest => {
            let sql = format!("{from} ORDER BY m.at DESC, m.n DESC LIMIT ?2");
            db.prepare_cached(&sql)?.query_map(params![chat, limit], message_row)?.collect::<rusqlite::Result<_>>()?
        }
        Around::Before(cursor) => {
            let sql = format!("{from} AND (m.at, m.n) < (?2, ?3) ORDER BY m.at DESC, m.n DESC LIMIT ?4");
            db.prepare_cached(&sql)?
                .query_map(params![chat, cursor.at, cursor.n, limit], message_row)?
                .collect::<rusqlite::Result<_>>()?
        }
        Around::After(cursor) => {
            let sql = format!("{from} AND (m.at, m.n) > (?2, ?3) ORDER BY m.at, m.n LIMIT ?4");
            return Ok(db
                .prepare_cached(&sql)?
                .query_map(params![chat, cursor.at, cursor.n, limit], message_row)?
                .collect::<rusqlite::Result<_>>()?);
        }
    };
    rows.reverse();
    Ok(rows)
}

/// A message by its chat and ID, with its raw protobuf decompressed.
pub fn message(db: &Connection, chat: &str, id: &str) -> Result<Option<(MessageRow, Option<Vec<u8>>)>> {
    let sql = format!(
        "{MESSAGE_COLUMNS} FROM messages m {MESSAGE_JOINS} WHERE m.chat = (SELECT n FROM chats WHERE id = ?1) AND m.id = ?2"
    );
    let Some(row) = db.prepare_cached(&sql)?.query_row(params![chat, id], message_row).optional()? else {
        return Ok(None);
    };
    let packed: Option<Vec<u8>> =
        db.prepare_cached("SELECT raw FROM messages WHERE n = ?1")?.query_row([row.n], |r| r.get(0))?;
    let raw = packed.map(|packed| super::write::unpack(&packed)).transpose()?;
    Ok(Some((row, raw)))
}

/// A message search found, with where the query matched in its text, as byte ranges.
#[derive(Debug, Serialize)]
pub struct Hit {
    pub message: MessageRow,
    pub text: String,
    pub marks: Vec<(usize, usize)>,
    /// Its place in search order, newest first.
    #[serde(skip)]
    t: i64,
}

/// A page of search hits, and where the next page starts if there may be one.
#[derive(Debug, Serialize)]
pub struct Found {
    pub hits: Vec<Hit>,
    pub cursor: Option<i64>,
}

/// The FTS5 query for what someone typed: every word, each matching the start of a word, so
/// typing finds as it goes. Quoting keeps FTS5's own syntax out of it.
fn fts_query(typed: &str) -> String {
    typed.split_whitespace().map(|word| format!("\"{}\"*", word.replace('"', "\"\""))).collect::<Vec<_>>().join(" ")
}

const MARK_START: char = '\u{2}';
const MARK_END: char = '\u{3}';

/// The text FTS5's highlight() marked, without its marks, and where they were.
fn unmark(marked: &str) -> (String, Vec<(usize, usize)>) {
    let mut text = String::with_capacity(marked.len());
    let mut marks = Vec::new();
    let mut start = 0;
    for c in marked.chars() {
        match c {
            MARK_START => start = text.len(),
            MARK_END => marks.push((start, text.len())),
            c => text.push(c),
        }
    }
    (text, marks)
}

/// Messages matching `typed`, newest first, accent-blind ("nao" finds "não"), in `chat` or in
/// every chat; `before` is the cursor a previous page returned.
pub fn search(db: &Connection, typed: &str, chat: Option<&str>, before: Option<i64>, limit: usize) -> Result<Found> {
    let query = fts_query(typed);
    if query.is_empty() {
        return Ok(Found { hits: Vec::new(), cursor: None });
    }
    // CROSS JOIN keeps FTS5 the outer loop, so it reads its rowids newest first and stops at the limit.
    let sql = format!(
        "{MESSAGE_COLUMNS}, messages_fts.rowid, highlight(messages_fts, 0, char(2), char(3))
        FROM messages_fts CROSS JOIN messages m ON m.t = messages_fts.rowid {MESSAGE_JOINS}
        WHERE messages_fts MATCH ?1 AND messages_fts.rowid < ?2 AND (?3 IS NULL OR c.id = ?3)
        ORDER BY messages_fts.rowid DESC LIMIT ?4"
    );
    let hits: Vec<Hit> = db
        .prepare_cached(&sql)?
        .query_map(params![query, before.unwrap_or(i64::MAX), chat, limit as i64], |r| {
            let marked: String = r.get(MESSAGE_COLUMN_COUNT + 1)?;
            let (text, marks) = unmark(&marked);
            Ok(Hit { message: message_row(r)?, text, marks, t: r.get(MESSAGE_COLUMN_COUNT)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let cursor = (hits.len() == limit).then(|| hits.last().map(|hit| hit.t)).flatten();
    Ok(Found { hits, cursor })
}

/// The newest `most` messages matching `query`, an FTS5 query, by their t, in `chat` or in every
/// chat. Only FTS5's rowids are read, so a long list costs little more than a short one; a prefix
/// query's cost is in FTS5 gathering every word with the prefix, which it does once.
pub fn matching(db: &Connection, query: &str, chat: Option<&str>, most: usize) -> Result<Vec<i64>> {
    let ts = match chat {
        None => db
            .prepare_cached("SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1 ORDER BY rowid DESC LIMIT ?2")?
            .query_map(params![query, most as i64], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
        Some(chat) => db
            .prepare_cached(
                "SELECT messages_fts.rowid FROM messages_fts CROSS JOIN messages m ON m.t = messages_fts.rowid
                WHERE messages_fts MATCH ?1 AND m.chat = (SELECT n FROM chats WHERE id = ?2)
                ORDER BY messages_fts.rowid DESC LIMIT ?3",
            )?
            .query_map(params![query, chat, most as i64], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
    };
    Ok(ts)
}

/// Messages by their t, in the order given, each with the text search reads; one that's gone is
/// left out.
pub fn messages_by_t(db: &Connection, ts: &[i64]) -> Result<Vec<(MessageRow, String)>> {
    let sql = format!("{MESSAGE_COLUMNS} FROM messages m {MESSAGE_JOINS} WHERE m.t = ?1");
    let mut select = db.prepare_cached(&sql)?;
    let mut rows = Vec::with_capacity(ts.len());
    for &t in ts {
        let row = select
            .query_row([t], |r| Ok((message_row(r)?, r.get::<_, Option<String>>(8)?.unwrap_or_default())))
            .optional()?;
        rows.extend(row);
    }
    Ok(rows)
}

/// A change in the feed: the row of `kind` with n `key` changed, or was removed, at `seq`.
#[derive(Debug, Serialize)]
pub struct Event {
    pub seq: i64,
    pub at: i64,
    pub kind: String,
    pub key: i64,
    pub chat: Option<i64>,
}

/// Up to `limit` events after `since`, oldest first.
pub fn events(db: &Connection, since: i64, limit: usize) -> Result<Vec<Event>> {
    let events = db
        .prepare_cached("SELECT seq, at, type, key, chat FROM events WHERE seq > ?1 ORDER BY seq LIMIT ?2")?
        .query_map(params![since, limit as i64], |r| {
            Ok(Event { seq: r.get(0)?, at: r.get(1)?, kind: r.get(2)?, key: r.get(3)?, chat: r.get(4)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(events)
}

/// The oldest seq the feed keeps and its last, or zeros before anything changed. Each is its own
/// subquery, since SQLite reads one end of the key only when a query asks for one of them.
pub fn feed_bounds(db: &Connection) -> Result<(i64, i64)> {
    Ok(db
        .prepare_cached(
            "SELECT coalesce((SELECT min(seq) FROM events), 0), coalesce((SELECT max(seq) FROM events), 0)",
        )?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))?)
}

/// A chat's n and its place in its list, archived or not, by its JID.
pub fn chat_place(db: &Connection, id: &str) -> Result<Option<(i64, ChatCursor, bool)>> {
    Ok(db
        .prepare_cached("SELECT n, sort, archived FROM chats WHERE id = ?1")?
        .query_row([id], |r| Ok((r.get(0)?, ChatCursor { sort: r.get(1)?, n: r.get(0)? }, r.get(2)?)))
        .optional()?)
}

/// Up to `limit` chats just above `cursor` in the list, in the list's order, so the nearest is
/// last.
pub fn chats_above(db: &Connection, archived: bool, cursor: &ChatCursor, limit: usize) -> Result<Vec<ChatRow>> {
    let sql =
        format!("{CHAT_COLUMNS} WHERE c.archived = ?1 AND (c.sort, c.n) > (?2, ?3) ORDER BY c.sort, c.n LIMIT ?4");
    let mut rows: Vec<ChatRow> = db
        .prepare_cached(&sql)?
        .query_map(params![archived, cursor.sort, cursor.n, limit as i64], chat_row)?
        .collect::<rusqlite::Result<_>>()?;
    rows.reverse();
    Ok(rows)
}

/// Up to `limit` chats from `cursor` down the list, the chat at it first if it's in this list.
pub fn chats_from(db: &Connection, archived: bool, cursor: &ChatCursor, limit: usize) -> Result<Vec<ChatRow>> {
    chats(db, archived, Some(&ChatCursor { sort: cursor.sort.clone(), n: cursor.n.saturating_add(1) }), limit)
}

/// The chats with these n that are still there, in no particular order.
pub fn chats_by_n(db: &Connection, ns: &[i64]) -> Result<Vec<ChatRow>> {
    let sql = format!("{CHAT_COLUMNS} WHERE c.n IN (SELECT value FROM json_each(?1))");
    let ns = serde_json::to_string(ns)?;
    let rows = db.prepare_cached(&sql)?.query_map([ns], chat_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// How many chats the list has.
pub fn chat_count(db: &Connection, archived: bool) -> Result<u64> {
    let count: i64 =
        db.prepare_cached("SELECT count(*) FROM chats WHERE archived = ?1")?.query_row([archived], |r| r.get(0))?;
    Ok(count.max(0) as u64)
}

/// A chat's n by its JID.
pub fn chat_n(db: &Connection, id: &str) -> Result<Option<i64>> {
    Ok(db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([id], |r| r.get(0)).optional()?)
}

/// A message's place in its chat.
pub fn message_place(db: &Connection, chat: &str, id: &str) -> Result<Option<MessageCursor>> {
    Ok(db
        .prepare_cached("SELECT at, n FROM messages WHERE chat = (SELECT n FROM chats WHERE id = ?1) AND id = ?2")?
        .query_row(params![chat, id], |r| Ok(MessageCursor { at: r.get(0)?, n: r.get(1)? }))
        .optional()?)
}

/// The chat's first unread message, with its ID: the first from others after where the chat was
/// read, or, while the phone's own count stands in for where, that many of theirs before it; none
/// when nothing's unread.
pub fn first_unread(db: &Connection, chat: &str) -> Result<Option<(String, MessageCursor)>> {
    type Found = (i64, i64, Option<i64>, Option<i64>, i64);
    let found: Option<Found> = db
        .prepare_cached("SELECT n, unread, read_at, read_n, coalesce(phone_unread, 0) FROM chats WHERE id = ?1")?
        .query_row([chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .optional()?;
    let Some((n, _, read_at, read_n, phone)) = found.filter(|&(_, unread, ..)| unread > 0) else { return Ok(None) };
    let mut after = read_at.map_or((i64::MIN, i64::MIN), |at| (at, read_n.unwrap_or(i64::MAX)));
    if phone > 0 {
        let newest: Vec<(i64, i64)> = db
            .prepare_cached(
                "SELECT at, n FROM messages WHERE chat = ?1 AND (at, n) <= (?2, ?3) AND mine = 0
                    AND kind NOT IN ('system', 'call')
                ORDER BY at DESC, n DESC LIMIT ?4",
            )?
            .query_map(params![n, after.0, after.1, phone], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if let Some(&(at, n)) = newest.last() {
            after = (at, n - 1);
        }
    }
    Ok(db
        .prepare_cached(
            "SELECT id, at, n FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND mine = 0
                AND kind NOT IN ('system', 'call')
            ORDER BY at, n LIMIT 1",
        )?
        .query_row(params![n, after.0, after.1], |r| Ok((r.get(0)?, MessageCursor { at: r.get(1)?, n: r.get(2)? })))
        .optional()?)
}

/// Up to `limit` messages in `chat` from `cursor` on, the one at it first if it's still there,
/// oldest first.
pub fn messages_from(db: &Connection, chat: &str, cursor: MessageCursor, limit: usize) -> Result<Vec<MessageRow>> {
    messages(db, chat, Around::After(MessageCursor { at: cursor.at, n: cursor.n.saturating_sub(1) }), limit)
}

/// How many messages `chat` has, as the writer keeps count.
pub fn message_count(db: &Connection, chat: &str) -> Result<u64> {
    let count: Option<i64> = db
        .prepare_cached("SELECT message_count FROM chats WHERE id = ?1")?
        .query_row([chat], |r| r.get(0))
        .optional()?;
    Ok(count.unwrap_or(0).max(0) as u64)
}

/// How many messages in `chat` come after `cursor`, counting no further than `most`, so it costs
/// the same however deep the cursor is.
pub fn messages_after(db: &Connection, chat: &str, cursor: MessageCursor, most: u64) -> Result<u64> {
    let count: i64 = db
        .prepare_cached(
            "SELECT count(*) FROM (SELECT 1 FROM messages
            WHERE chat = (SELECT n FROM chats WHERE id = ?1) AND (at, n) > (?2, ?3) LIMIT ?4)",
        )?
        .query_row(params![chat, cursor.at, cursor.n, most as i64], |r| r.get(0))?;
    Ok(count.max(0) as u64)
}

/// Whether `chat` has a message before `cursor`.
pub fn messages_before(db: &Connection, chat: &str, cursor: MessageCursor) -> Result<bool> {
    Ok(db
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = (SELECT n FROM chats WHERE id = ?1) AND (at, n) < (?2, ?3))",
        )?
        .query_row(params![chat, cursor.at, cursor.n], |r| r.get(0))?)
}
