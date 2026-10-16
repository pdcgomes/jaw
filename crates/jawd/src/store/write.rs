//! What the writer does with chats, people and messages. Each change writes its `events` row in
//! the same transaction and takes that event's seq as the row's `v`, so a row's version says when
//! it last changed: interfaces converge however updates arrive, and a client that saw seq S needs
//! only the rows whose `v` is past S.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

/// A change for the writer to make.
pub enum Change {
    Chat(Chat),
    Person(Person),
    Message(Message),
}

/// A chat as WhatsApp describes it. Fields left `None` keep what the chat has.
#[derive(Clone, Debug, Default)]
pub struct Chat {
    /// Its JID.
    pub id: String,
    /// `direct`, `group`, `community`, `announcement`, `broadcast` or `self`. A new chat without
    /// one takes it from its JID.
    pub kind: Option<&'static str>,
    pub name: Option<String>,
    /// When it was pinned, in milliseconds, or 0 to unpin it.
    pub pinned: Option<i64>,
    pub archived: Option<bool>,
    /// Until when it's muted, in milliseconds, or 0 to unmute it.
    pub muted_until: Option<i64>,
    pub marked_unread: Option<bool>,
    pub unread: Option<u32>,
    pub mentions: Option<u32>,
    /// The JID of the community a group belongs to, or empty for none.
    pub community: Option<String>,
    /// The disappearing-message timer in seconds, or 0 for off.
    pub ephemeral: Option<u32>,
    /// What's typed in its box, or empty to clear it.
    pub draft: Option<String>,
}

/// Someone, by every JID they're known by. Fields left `None` keep what the person has; JIDs that
/// belonged to different people join them into one.
#[derive(Clone, Debug, Default)]
pub struct Person {
    /// Their phone number's JID, their LID, or both.
    pub jids: Vec<String>,
    /// As in the phone's contacts.
    pub name: Option<String>,
    /// What they call themselves.
    pub push_name: Option<String>,
    /// A business's verified name.
    pub verified_name: Option<String>,
}

/// A message, converted and ready to show. The whole row is replaced each time.
#[derive(Clone, Debug, Default)]
pub struct Message {
    /// Its chat's JID; a chat jaw hasn't seen is made.
    pub chat: String,
    /// Its WhatsApp ID, unique in its chat.
    pub id: String,
    /// When it was sent, in milliseconds since the epoch.
    pub at: i64,
    /// Who sent it, by JID; `None` for your own and for system lines.
    pub sender: Option<String>,
    pub mine: bool,
    /// As the protocol names kinds: `text`, `image`, `voice`, `poll`, `system`, `unknown` and so on.
    pub kind: &'static str,
    /// On your own: `pending`, `sent`, `delivered`, `read`, `played` or `failed`.
    pub status: Option<&'static str>,
    /// What search finds it by: its text, a caption, a poll's question or a document's name.
    pub text: Option<String>,
    /// The text parsed into the protocol's spans, as JSON; `None` when it's one plain span.
    pub spans: Option<String>,
    /// One line for the chat list and for quotes.
    pub summary: String,
    /// The quoted message, as the protocol's `reply` in JSON.
    pub reply: Option<String>,
    /// What its kind needs to be drawn (media, a poll, a location, a system line), as JSON.
    pub detail: Option<String>,
    pub forwarded: u32,
    pub edited_at: Option<i64>,
    pub deleted: bool,
    pub expires_at: Option<i64>,
    pub starred: bool,
    pub view_once: bool,
    /// WhatsApp's WebMessageInfo, compressed.
    pub raw: Option<Packed>,
}

/// A raw protobuf compressed with zstd, one frame per row. Frames name the dictionary they were
/// made with, so rows packed with a dictionary later can sit beside these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packed(Vec<u8>);

const ZSTD_LEVEL: i32 = 3;

impl Packed {
    /// Compresses `raw`; do it before handing a message to the writer, which shouldn't spend its
    /// time on it.
    pub fn new(raw: &[u8]) -> Packed {
        thread_local! {
            static COMPRESSOR: RefCell<Option<zstd::bulk::Compressor<'static>>> = const { RefCell::new(None) };
        }
        COMPRESSOR.with(|cell| {
            let mut cell = cell.borrow_mut();
            if cell.is_none() {
                *cell = zstd::bulk::Compressor::new(ZSTD_LEVEL).ok();
            }
            match cell.as_mut().and_then(|compressor| compressor.compress(raw).ok()) {
                Some(packed) => Packed(packed),
                None => Packed(zstd::bulk::compress(raw, ZSTD_LEVEL).unwrap_or_default()),
            }
        })
    }
}

/// The raw protobuf a `Packed` holds.
pub fn unpack(packed: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(packed).context("a message's raw protobuf doesn't decompress")
}

/// The seqs handed out in the writer's transaction. A row changed twice in one transaction keeps
/// the seq of its first change and writes one event, since no reader sees between the two.
pub(super) struct Stamps {
    next: i64,
    began: i64,
    touched: HashMap<(&'static str, i64), i64>,
}

impl Stamps {
    pub(super) fn load(db: &Connection) -> Result<Stamps> {
        let last: i64 = db.query_row("SELECT coalesce(max(seq), 0) FROM events", [], |row| row.get(0))?;
        Ok(Stamps { next: last + 1, began: last + 1, touched: HashMap::new() })
    }

    pub(super) fn begin(&mut self) {
        self.began = self.next;
        self.touched.clear();
    }

    pub(super) fn rolled_back(&mut self) {
        self.next = self.began;
        self.touched.clear();
    }

    /// The seq a change to an existing row would take.
    fn seq(&self, kind: &'static str, key: Option<i64>) -> i64 {
        key.and_then(|key| self.touched.get(&(kind, key)).copied()).unwrap_or(self.next)
    }

    /// Writes the event for a row that changed with `seq`, unless it already has one.
    fn record(&mut self, db: &Connection, kind: &'static str, key: i64, chat: Option<i64>, seq: i64) -> Result<()> {
        if self.touched.contains_key(&(kind, key)) {
            return Ok(());
        }
        debug_assert_eq!(seq, self.next);
        db.prepare_cached("INSERT INTO events (seq, at, type, key, chat) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![seq, now_ms(), kind, key, chat])?;
        self.touched.insert((kind, key), seq);
        self.next += 1;
        Ok(())
    }

    /// The last seq written, committed or not.
    pub(super) fn last(&self) -> i64 {
        self.next - 1
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Makes one change inside the writer's open transaction, saying whether anything changed.
pub(super) fn apply(db: &Connection, stamps: &mut Stamps, change: &Change) -> Result<bool> {
    match change {
        Change::Chat(chat) => upsert_chat(db, stamps, chat),
        Change::Person(person) => upsert_person(db, stamps, person),
        Change::Message(message) => upsert_message(db, stamps, message),
    }
}

/// A JID without its device: messages from someone's phone and their laptop are from them.
fn user_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server @ ("s.whatsapp.net" | "lid"))) => {
            let user = user.split([':', '.']).next().unwrap_or(user);
            format!("{user}@{server}")
        }
        _ => jid.to_string(),
    }
}

fn is_person(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net") || jid.ends_with("@lid")
}

fn kind_of(jid: &str) -> &'static str {
    if jid.ends_with("@g.us") {
        "group"
    } else if jid.ends_with("@broadcast") {
        "broadcast"
    } else {
        "direct"
    }
}

/// The chat's n, making the chat if jaw hasn't seen it.
fn ensure_chat(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    if let Some(n) =
        db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([jid], |r| r.get(0)).optional()?
    {
        return Ok(n);
    }
    upsert_chat(db, stamps, &Chat { id: jid.to_string(), ..Chat::default() })?;
    Ok(db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([jid], |r| r.get(0))?)
}

/// The n of `jid` in person_jids, making a person for it if jaw hasn't seen it.
fn ensure_jid(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    let jid = user_jid(jid);
    let find = "SELECT n FROM person_jids WHERE jid = ?1";
    if let Some(n) = db.prepare_cached(find)?.query_row([&jid], |r| r.get(0)).optional()? {
        return Ok(n);
    }
    let seq = stamps.seq(PERSON, None);
    let person: i64 =
        db.prepare_cached("INSERT INTO people (v) VALUES (?1) RETURNING n")?.query_row([seq], |r| r.get(0))?;
    stamps.record(db, PERSON, person, None, seq)?;
    Ok(db
        .prepare_cached("INSERT INTO person_jids (jid, person) VALUES (?1, ?2) RETURNING n")?
        .query_row(params![jid, person], |r| r.get(0))?)
}

const CHAT: &str = "chat";
const PERSON: &str = "person";
const MESSAGE: &str = "message";

fn upsert_chat(db: &Connection, stamps: &mut Stamps, chat: &Chat) -> Result<bool> {
    let existing: Option<(i64, String)> = db
        .prepare_cached("SELECT n, kind FROM chats WHERE id = ?1")?
        .query_row([&chat.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let was = existing.as_ref().map_or(kind_of(&chat.id), |(_, kind)| kind.as_str());
    let person = match chat.kind.unwrap_or(was) {
        "direct" if is_person(&chat.id) && (existing.is_none() || was != "direct") => {
            Some(person_of(db, stamps, &chat.id)?)
        }
        _ => None,
    };
    let seq = stamps.seq(CHAT, existing.as_ref().map(|(n, _)| *n));
    let changed: Option<i64> = db
        .prepare_cached(
            "INSERT INTO chats (id, kind, person, name, pinned, archived, muted_until, marked_unread, unread, mentions,
                community, ephemeral, draft, v)
            VALUES (?1, coalesce(?2, ?15), ?3, ?4, coalesce(?5, 0), coalesce(?6, 0), coalesce(?7, 0), coalesce(?8, 0), coalesce(?9, 0),
                coalesce(?10, 0), nullif(?11, ''), coalesce(?12, 0), nullif(?13, ''), ?14)
            ON CONFLICT (id) DO UPDATE SET
                kind = coalesce(?2, kind), person = coalesce(?3, person), name = coalesce(?4, name),
                pinned = coalesce(?5, pinned), archived = coalesce(?6, archived),
                muted_until = coalesce(?7, muted_until), marked_unread = coalesce(?8, marked_unread),
                unread = coalesce(?9, unread), mentions = coalesce(?10, mentions),
                community = iif(?11 IS NULL, community, nullif(?11, '')), ephemeral = coalesce(?12, ephemeral),
                draft = iif(?13 IS NULL, draft, nullif(?13, '')), v = ?14
            WHERE coalesce(?2, kind) IS NOT kind OR coalesce(?3, person) IS NOT person
                OR coalesce(?4, name) IS NOT name OR coalesce(?5, pinned) IS NOT pinned
                OR coalesce(?6, archived) IS NOT archived OR coalesce(?7, muted_until) IS NOT muted_until
                OR coalesce(?8, marked_unread) IS NOT marked_unread OR coalesce(?9, unread) IS NOT unread
                OR coalesce(?10, mentions) IS NOT mentions
                OR iif(?11 IS NULL, community, nullif(?11, '')) IS NOT community
                OR coalesce(?12, ephemeral) IS NOT ephemeral OR iif(?13 IS NULL, draft, nullif(?13, '')) IS NOT draft
            RETURNING n",
        )?
        .query_row(
            params![
                chat.id,
                chat.kind,
                person,
                chat.name,
                chat.pinned,
                chat.archived,
                chat.muted_until,
                chat.marked_unread,
                chat.unread,
                chat.mentions,
                chat.community,
                chat.ephemeral,
                chat.draft,
                seq,
                kind_of(&chat.id)
            ],
            |r| r.get(0),
        )
        .optional()?;
    match changed {
        Some(n) => {
            stamps.record(db, CHAT, n, Some(n), seq)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// The person a JID belongs to, making one if needed.
fn person_of(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    let n = ensure_jid(db, stamps, jid)?;
    Ok(db.prepare_cached("SELECT person FROM person_jids WHERE n = ?1")?.query_row([n], |r| r.get(0))?)
}

fn upsert_person(db: &Connection, stamps: &mut Stamps, person: &Person) -> Result<bool> {
    let mut jids: Vec<String> = person.jids.iter().map(|jid| user_jid(jid)).collect();
    jids.sort();
    jids.dedup();
    if jids.is_empty() {
        bail!("a person needs at least one JID");
    }
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for jid in &jids {
        let found: Option<i64> = db
            .prepare_cached("SELECT person FROM person_jids WHERE jid = ?1")?
            .query_row([jid], |r| r.get(0))
            .optional()?;
        match found {
            Some(n) => known.push(n),
            None => unknown.push(jid),
        }
    }
    known.sort_unstable();
    known.dedup();
    let mut joined = !unknown.is_empty() || known.len() > 1;
    let keep = match known.first() {
        Some(&n) => n,
        None => {
            let seq = stamps.seq(PERSON, None);
            let n: i64 =
                db.prepare_cached("INSERT INTO people (v) VALUES (?1) RETURNING n")?.query_row([seq], |r| r.get(0))?;
            stamps.record(db, PERSON, n, None, seq)?;
            joined = true;
            n
        }
    };
    for &gone in known.iter().skip(1) {
        join(db, stamps, keep, gone)?;
    }
    for jid in unknown {
        db.prepare_cached("INSERT INTO person_jids (jid, person) VALUES (?1, ?2)")?.execute(params![jid, keep])?;
    }
    let seq = stamps.seq(PERSON, Some(keep));
    let changed: Option<i64> = db
        .prepare_cached(
            "UPDATE people SET name = coalesce(?2, name), push_name = coalesce(?3, push_name),
                verified_name = coalesce(?4, verified_name), v = ?5
            WHERE n = ?1 AND (?6 OR coalesce(?2, name) IS NOT name OR coalesce(?3, push_name) IS NOT push_name
                OR coalesce(?4, verified_name) IS NOT verified_name)
            RETURNING n",
        )?
        .query_row(params![keep, person.name, person.push_name, person.verified_name, seq, joined], |r| r.get(0))
        .optional()?;
    if changed.is_some() {
        stamps.record(db, PERSON, keep, None, seq)?;
    }
    Ok(changed.is_some())
}

/// Joins `gone` into `keep`: its JIDs, its chats and its folder places move, the fields `keep`
/// lacks are filled from it, and it's removed. Rows that name a JID aren't touched, which keeps a
/// join to a few rows however many messages they sent.
fn join(db: &Connection, stamps: &mut Stamps, keep: i64, gone: i64) -> Result<()> {
    db.prepare_cached(
        "UPDATE people AS k SET name = coalesce(k.name, g.name), push_name = coalesce(k.push_name, g.push_name),
            verified_name = coalesce(k.verified_name, g.verified_name), about = coalesce(k.about, g.about),
            about_at = coalesce(k.about_at, g.about_at), avatar = coalesce(k.avatar, g.avatar),
            avatar_id = coalesce(k.avatar_id, g.avatar_id), last_seen = max(coalesce(k.last_seen, 0), coalesce(g.last_seen, 0))
        FROM (SELECT * FROM people WHERE n = ?2) AS g WHERE k.n = ?1",
    )?
    .execute(params![keep, gone])?;
    db.prepare_cached("UPDATE person_jids SET person = ?1 WHERE person = ?2")?.execute(params![keep, gone])?;
    let chats: Vec<i64> = db
        .prepare_cached("UPDATE chats SET person = ?1 WHERE person = ?2 RETURNING n")?
        .query_map(params![keep, gone], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for chat in chats {
        let seq = stamps.seq(CHAT, Some(chat));
        db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute(params![chat, seq])?;
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    db.prepare_cached("UPDATE OR IGNORE folder_items SET person = ?1 WHERE person = ?2")?
        .execute(params![keep, gone])?;
    db.prepare_cached("DELETE FROM folder_items WHERE person = ?1")?.execute([gone])?;
    db.prepare_cached("DELETE FROM people WHERE n = ?1")?.execute([gone])?;
    let seq = stamps.seq(PERSON, Some(gone));
    stamps.record(db, PERSON, gone, None, seq)
}

/// How many bits of a message's t count messages within one millisecond.
const SAME_MS_BITS: u32 = 20;
/// The latest time a message's t can hold, in the year 2248.
const LAST_MS: i64 = (1 << (63 - SAME_MS_BITS)) - 1;

/// A free t for a message sent at `at`: its millisecond, then the next count free in it.
fn allocate(db: &Connection, at: i64) -> Result<i64> {
    let first = at.clamp(0, LAST_MS) << SAME_MS_BITS;
    let last = first | ((1 << SAME_MS_BITS) - 1);
    let taken: Option<i64> = db
        .prepare_cached("SELECT t FROM messages WHERE t BETWEEN ?1 AND ?2 ORDER BY t DESC LIMIT 1")?
        .query_row(params![first, last], |r| r.get(0))
        .optional()?;
    match taken {
        None => Ok(first),
        Some(n) if n < last => Ok(n + 1),
        Some(_) => bail!("more than a million messages are stamped with the same millisecond, {at}"),
    }
}

fn upsert_message(db: &Connection, stamps: &mut Stamps, message: &Message) -> Result<bool> {
    let chat = ensure_chat(db, stamps, &message.chat)?;
    let sender = message.sender.as_deref().map(|jid| ensure_jid(db, stamps, jid)).transpose()?;
    let existing: Option<(i64, i64)> = db
        .prepare_cached("SELECT n, t FROM messages WHERE chat = ?1 AND id = ?2")?
        .query_row(params![chat, message.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let seq = stamps.seq(MESSAGE, existing.map(|(n, _)| n));
    let raw = message.raw.as_ref().map(|packed| packed.0.as_slice());
    let summary = (message.text.as_ref() != Some(&message.summary)).then_some(&message.summary);
    let (n, t) = match existing {
        Some((n, t)) => (Some(n), t),
        None => (None, allocate(db, message.at)?),
    };
    let fields = params![
        message.at,
        sender,
        message.mine,
        message.kind,
        message.status,
        message.text,
        message.spans,
        summary,
        message.reply,
        message.detail,
        message.forwarded,
        message.edited_at,
        message.deleted,
        message.expires_at,
        message.starred,
        message.view_once,
        raw,
        seq,
        n,
        chat,
        message.id,
        t,
    ];
    let changed: Option<i64> = if existing.is_some() {
        db.prepare_cached(
            "UPDATE messages SET at = ?1, sender = ?2, mine = ?3, kind = ?4, status = ?5, text = ?6, spans = ?7,
                summary = ?8, reply = ?9, detail = ?10, forwarded = ?11, edited_at = ?12, deleted = ?13,
                expires_at = ?14, starred = ?15, view_once = ?16, raw = ?17, v = ?18
            WHERE n = ?19 AND (at IS NOT ?1 OR sender IS NOT ?2 OR mine IS NOT ?3 OR kind IS NOT ?4
                OR status IS NOT ?5 OR text IS NOT ?6 OR spans IS NOT ?7 OR summary IS NOT ?8 OR reply IS NOT ?9
                OR detail IS NOT ?10 OR forwarded IS NOT ?11 OR edited_at IS NOT ?12 OR deleted IS NOT ?13
                OR expires_at IS NOT ?14 OR starred IS NOT ?15 OR view_once IS NOT ?16 OR raw IS NOT ?17)
            RETURNING n",
        )?
        .query_row(&fields[..19], |r| r.get(0))
        .optional()?
    } else {
        Some(
            db.prepare_cached(
                "INSERT INTO messages (at, sender, mine, kind, status, text, spans, summary, reply, detail, forwarded,
                    edited_at, deleted, expires_at, starred, view_once, raw, v, n, chat, id, t)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                    ?22)
                RETURNING n",
            )?
            .query_row(fields, |r| r.get(0))?,
        )
    };
    let Some(n) = changed else { return Ok(false) };
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    // The chat's newest message sets its sort; older history never moves it.
    let seq = stamps.seq(CHAT, Some(chat));
    let moved = db
        .prepare_cached(
            "UPDATE chats SET last_at = ?2, last_message = ?3, v = ?4
            WHERE n = ?1 AND (last_message IS NULL OR last_message = ?3
                OR ?5 > (SELECT t FROM messages WHERE n = last_message))",
        )?
        .execute(params![chat, message.at.max(0), n, seq, t])?;
    if moved > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

/// How long the feed keeps events.
pub(super) const EVENTS_KEPT: Duration = Duration::from_secs(7 * 24 * 3600);

/// Removes up to `limit` events older than the feed keeps, always leaving the newest, whose seq
/// the next change follows. Says how many went.
pub(super) fn prune(db: &Connection, limit: usize) -> Result<usize> {
    let before = now_ms() - EVENTS_KEPT.as_millis() as i64;
    Ok(db
        .prepare_cached(
            "DELETE FROM events WHERE seq IN (
            SELECT seq FROM events WHERE at < ?1 AND seq < (SELECT max(seq) FROM events) ORDER BY seq LIMIT ?2)",
        )?
        .execute(params![before, limit as i64])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_is_dropped_from_a_persons_jid_but_not_from_others() {
        assert_eq!(user_jid("351910000001:12@s.whatsapp.net"), "351910000001@s.whatsapp.net");
        assert_eq!(user_jid("9988776655:3@lid"), "9988776655@lid");
        assert_eq!(user_jid("120363000000000001@g.us"), "120363000000000001@g.us");
    }

    #[test]
    fn a_packed_protobuf_unpacks_to_what_it_was() {
        let raw: Vec<u8> = (0..2_000u32).map(|n| (n % 7) as u8).collect();
        let packed = Packed::new(&raw);
        assert!(packed.0.len() < raw.len() / 4, "{} bytes packed from {}", packed.0.len(), raw.len());
        assert_eq!(unpack(&packed.0).unwrap(), raw);
    }
}
