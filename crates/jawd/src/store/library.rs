//! The library's reads. A message that's a photo, a video, a voice message or a file is an item of
//! that kind, its `shelf` (migration 25). The items of a kind are read newest first by their t (the
//! message's time, then its place in that millisecond) on an index that holds only items: across
//! chats, in one chat, or from one sender, by keyset, so the thousandth page costs what the first
//! does. Their words come from library_fts, which holds only items'. How many there are of each
//! kind, in a chat or in all of them, is read from `shelves`, which the triggers counting messages
//! keep; with any other filter, they're counted up to `COUNTED`.
//!
//! jaw.db is never analyzed, so the planner can't tell a sender's or a chat's items from all of a
//! kind's: each read names the index it's for, and fails to prepare if that index can't serve it.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Map, Value};

use super::files::Files;
use super::read::{From, chat_name, shown, with_media};
use super::write::{LAST_MS, SAME_MS_BITS};

/// The most items of a kind counted when a read has a filter other than its chat.
pub const COUNTED: u64 = 1_000;

/// A kind of item jaw keeps: the values of `messages.shelf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Shelf {
    Photo,
    Video,
    Voice,
    File,
}

impl Shelf {
    pub const ALL: [Shelf; 4] = [Shelf::Photo, Shelf::Video, Shelf::Voice, Shelf::File];

    pub fn as_str(self) -> &'static str {
        match self {
            Shelf::Photo => "photo",
            Shelf::Video => "video",
            Shelf::Voice => "voice",
            Shelf::File => "file",
        }
    }

    fn of(shelf: &str) -> Option<Shelf> {
        Shelf::ALL.into_iter().find(|s| s.as_str() == shelf)
    }
}

/// Whose items a read is for: anyone's, yours (which name no sender), or those any of a person's
/// JIDs sent, by their n in person_jids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sender {
    Anyone,
    Me,
    Jids(Vec<i64>),
}

impl Sender {
    /// What `sender IS ?` and `mine = ?` take, once for each JID.
    fn keys(&self) -> Vec<(Option<i64>, bool)> {
        match self {
            Sender::Anyone => Vec::new(),
            Sender::Me => vec![(None, true)],
            Sender::Jids(jids) => jids.iter().map(|&jid| (Some(jid), false)).collect(),
        }
    }
}

/// What a read is narrowed to, but its kind: a chat by its n, whose items, and the stretch of t
/// they're in, both ends included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub chat: Option<i64>,
    pub sender: Sender,
    pub since: i64,
    pub until: i64,
}

impl Scope {
    /// The stretch of t messages sent from `since` to `until` (milliseconds since the epoch, both
    /// included, either open) are in.
    pub fn times(since: Option<i64>, until: Option<i64>) -> (i64, i64) {
        let since = since.map_or(0, |at| at.clamp(0, LAST_MS) << SAME_MS_BITS);
        let until = until.map_or(i64::MAX, |at| (at.clamp(0, LAST_MS) << SAME_MS_BITS) | ((1 << SAME_MS_BITS) - 1));
        (since, until)
    }

    /// Whether anything but a chat narrows it, so its counts aren't the ones `shelves` keeps.
    fn narrowed(&self) -> bool {
        self.sender != Sender::Anyone || self.since > 0 || self.until < i64::MAX
    }
}

/// An item's sort on the wire: its message's time, then its place in that millisecond, both
/// zero-padded, so items read newest first from the largest down and no two share one.
pub fn sort(t: i64) -> String {
    format!("{:013}~{:07}", t >> SAME_MS_BITS, t & ((1 << SAME_MS_BITS) - 1))
}

/// Which way from a place a read goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toward {
    Older,
    Newer,
}

const ACROSS_OLDER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved
    WHERE shelf = ?1 AND t < ?2 AND t >= ?3 ORDER BY t DESC LIMIT ?4";
const ACROSS_NEWER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved
    WHERE shelf = ?1 AND t > ?2 AND t <= ?3 ORDER BY t LIMIT ?4";
const IN_OLDER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND t < ?3 AND t >= ?4 ORDER BY t DESC LIMIT ?5";
const IN_NEWER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND t > ?3 AND t <= ?4 ORDER BY t LIMIT ?5";
// One sender's items in a chat are the chat's, with who sent each read from the same index.
const IN_FROM_OLDER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND sender IS ?3 AND mine = ?4 AND t < ?5 AND t >= ?6 ORDER BY t DESC LIMIT ?7";
const IN_FROM_NEWER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND sender IS ?3 AND mine = ?4 AND t > ?5 AND t <= ?6 ORDER BY t LIMIT ?7";
const FROM_OLDER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_by
    WHERE sender IS ?1 AND shelf = ?2 AND mine = ?3 AND t < ?4 AND t >= ?5 ORDER BY t DESC LIMIT ?6";
const FROM_NEWER: &str = "SELECT n, t FROM messages INDEXED BY messages_shelved_by
    WHERE sender IS ?1 AND shelf = ?2 AND mine = ?3 AND t > ?4 AND t <= ?5 ORDER BY t LIMIT ?6";

/// The bounds of a read toward `toward` from just past `from`: past this t, not including it, to
/// that one, including it.
fn bounds(scope: &Scope, toward: Toward, from: i64) -> (i64, i64) {
    match toward {
        Toward::Older => (from.min(scope.until.saturating_add(1)), scope.since),
        Toward::Newer => (from.max(scope.since.saturating_sub(1)), scope.until),
    }
}

/// Up to `limit` items of `shelf` in `scope` from just past `from` (a t) toward older or newer
/// ones, nearest first, as their n and t.
pub fn keys(
    db: &Connection,
    shelf: Shelf,
    scope: &Scope,
    toward: Toward,
    from: i64,
    limit: usize,
) -> Result<Vec<(i64, i64)>> {
    let (past, to) = bounds(scope, toward, from);
    let (shelf, most) = (shelf.as_str(), limit as i64);
    let pair = |r: &Row| Ok((r.get(0)?, r.get(1)?));
    let older = toward == Toward::Older;
    let mut found: Vec<(i64, i64)> = match (scope.chat, &scope.sender) {
        (None, Sender::Anyone) => {
            let mut select = if older { db.prepare_cached(ACROSS_OLDER)? } else { db.prepare_cached(ACROSS_NEWER)? };
            select.query_map(params![shelf, past, to, most], pair)?.collect::<rusqlite::Result<_>>()?
        }
        (Some(chat), Sender::Anyone) => {
            let mut select = if older { db.prepare_cached(IN_OLDER)? } else { db.prepare_cached(IN_NEWER)? };
            select.query_map(params![chat, shelf, past, to, most], pair)?.collect::<rusqlite::Result<_>>()?
        }
        (chat, sender) => {
            let mut found = Vec::new();
            for (jid, mine) in sender.keys() {
                let rows: Vec<(i64, i64)> = match chat {
                    Some(chat) => {
                        let mut select =
                            if older { db.prepare_cached(IN_FROM_OLDER)? } else { db.prepare_cached(IN_FROM_NEWER)? };
                        select
                            .query_map(params![chat, shelf, jid, mine, past, to, most], pair)?
                            .collect::<rusqlite::Result<_>>()?
                    }
                    None => {
                        let mut select =
                            if older { db.prepare_cached(FROM_OLDER)? } else { db.prepare_cached(FROM_NEWER)? };
                        select
                            .query_map(params![jid, shelf, mine, past, to, most], pair)?
                            .collect::<rusqlite::Result<_>>()?
                    }
                };
                found.extend(rows);
            }
            found
        }
    };
    // A person's JIDs are read one at a time, each in order, and put in order together here.
    found.sort_unstable_by_key(|&(_, t)| if older { -t } else { t });
    found.truncate(limit);
    Ok(found)
}

const ACROSS_COUNT: &str = "SELECT count(*) FROM (SELECT 1 FROM messages INDEXED BY messages_shelved
    WHERE shelf = ?1 AND t <= ?2 AND t >= ?3 LIMIT ?4)";
const IN_COUNT: &str = "SELECT count(*) FROM (SELECT 1 FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND t <= ?3 AND t >= ?4 LIMIT ?5)";
const IN_FROM_COUNT: &str = "SELECT count(*) FROM (SELECT 1 FROM messages INDEXED BY messages_shelved_in
    WHERE chat = ?1 AND shelf = ?2 AND sender IS ?3 AND mine = ?4 AND t <= ?5 AND t >= ?6 LIMIT ?7)";
const FROM_COUNT: &str = "SELECT count(*) FROM (SELECT 1 FROM messages INDEXED BY messages_shelved_by
    WHERE sender IS ?1 AND shelf = ?2 AND mine = ?3 AND t <= ?4 AND t >= ?5 LIMIT ?6)";
const SHELVES: &str = "SELECT shelf, items FROM shelves WHERE chat = ?1";

/// How many items of each kind `scope` holds: as `shelves` keeps them for a chat or all of them,
/// and counted up to COUNTED with any other filter, so it costs the same however many there are.
pub fn counts(db: &Connection, scope: &Scope) -> Result<HashMap<Shelf, u64>> {
    let mut counts: HashMap<Shelf, u64> = Shelf::ALL.into_iter().map(|shelf| (shelf, 0)).collect();
    if !scope.narrowed() {
        let rows: Vec<(String, i64)> = db
            .prepare_cached(SHELVES)?
            .query_map([scope.chat.unwrap_or(0)], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (shelf, items) in rows {
            if let Some(shelf) = Shelf::of(&shelf) {
                counts.insert(shelf, items.max(0) as u64);
            }
        }
        return Ok(counts);
    }
    let (until, since, most) = (scope.until, scope.since, COUNTED as i64);
    let count = |r: &Row| r.get::<_, i64>(0);
    for shelf in Shelf::ALL {
        let name = shelf.as_str();
        let mut n: i64 = 0;
        match (scope.chat, &scope.sender) {
            (None, Sender::Anyone) => {
                n = db.prepare_cached(ACROSS_COUNT)?.query_row(params![name, until, since, most], count)?;
            }
            (Some(chat), Sender::Anyone) => {
                n = db.prepare_cached(IN_COUNT)?.query_row(params![chat, name, until, since, most], count)?;
            }
            (chat, sender) => {
                for (jid, mine) in sender.keys() {
                    n += match chat {
                        Some(chat) => db
                            .prepare_cached(IN_FROM_COUNT)?
                            .query_row(params![chat, name, jid, mine, until, since, most], count)?,
                        None => db
                            .prepare_cached(FROM_COUNT)?
                            .query_row(params![jid, name, mine, until, since, most], count)?,
                    };
                }
            }
        }
        counts.insert(shelf, (n.max(0) as u64).min(COUNTED));
    }
    Ok(counts)
}

// library_fts holds only items, each with its kind, chat and sender as tags, so a search asks for
// what a window holds and reads only it, by rowid, which is its message's t.
const WORDS_OLDER: &str = "SELECT m.n, m.t FROM library_fts CROSS JOIN messages m ON m.t = library_fts.rowid
    WHERE library_fts MATCH ?1 AND library_fts.rowid < ?2 AND library_fts.rowid >= ?3
    ORDER BY library_fts.rowid DESC LIMIT ?4";
const WORDS_NEWER: &str = "SELECT m.n, m.t FROM library_fts CROSS JOIN messages m ON m.t = library_fts.rowid
    WHERE library_fts MATCH ?1 AND library_fts.rowid > ?2 AND library_fts.rowid <= ?3
    ORDER BY library_fts.rowid LIMIT ?4";
const WORDS_COUNT: &str = "SELECT count(*) FROM (SELECT 1 FROM library_fts
    WHERE library_fts MATCH ?1 AND rowid <= ?2 AND rowid >= ?3 LIMIT ?4)";

/// The FTS5 query for the items of `shelf` in `scope` with every word of `words` (an FTS5 query of
/// them), or None when nobody `scope` asks for could have sent any.
fn tagged(words: &str, shelf: Shelf, scope: &Scope) -> Option<String> {
    let mut query = format!("text : ({words}) AND tags : (s{})", shelf.as_str());
    if let Some(chat) = scope.chat {
        query.push_str(&format!(" AND tags : (c{chat})"));
    }
    match &scope.sender {
        Sender::Anyone => {}
        Sender::Me => query.push_str(" AND tags : (fme)"),
        Sender::Jids(jids) if jids.is_empty() => return None,
        Sender::Jids(jids) => {
            let jids: Vec<String> = jids.iter().map(|jid| format!("f{jid}")).collect();
            query.push_str(&format!(" AND tags : ({})", jids.join(" OR ")));
        }
    }
    Some(query)
}

/// Up to `limit` items of `shelf` in `scope` with every word of `words` (an FTS5 query of them),
/// from just past `from` (a t) toward older or newer ones, nearest first, as their n and t.
pub fn words(
    db: &Connection,
    words: &str,
    shelf: Shelf,
    scope: &Scope,
    toward: Toward,
    from: i64,
    limit: usize,
) -> Result<Vec<(i64, i64)>> {
    let Some(query) = tagged(words, shelf, scope) else { return Ok(Vec::new()) };
    let (past, to) = bounds(scope, toward, from);
    let mut select =
        if toward == Toward::Older { db.prepare_cached(WORDS_OLDER)? } else { db.prepare_cached(WORDS_NEWER)? };
    let found = select
        .query_map(params![query, past, to, limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(found)
}

/// How many items of each kind with every word of `words` `scope` holds, each counted up to
/// COUNTED by their rowids alone.
pub fn word_counts(db: &Connection, words: &str, scope: &Scope) -> Result<HashMap<Shelf, u64>> {
    let mut counts = HashMap::new();
    let mut select = db.prepare_cached(WORDS_COUNT)?;
    for shelf in Shelf::ALL {
        let n: i64 = match tagged(words, shelf, scope) {
            Some(query) => select.query_row(params![query, scope.until, scope.since, COUNTED as i64], |r| r.get(0))?,
            None => 0,
        };
        counts.insert(shelf, n.max(0) as u64);
    }
    Ok(counts)
}

const NAMED_IN: &str = "SELECT n, t, shelf FROM messages WHERE chat = (SELECT n FROM chats WHERE id = ?1) AND id = ?2";
const NAMED: &str =
    "SELECT n, t, shelf FROM messages INDEXED BY messages_shelved_named WHERE id = ?1 AND shelf IS NOT NULL";

/// The items with message ID `id`, in `chat` or in any chat, as their n, t and kind.
pub fn named(db: &Connection, id: &str, chat: Option<&str>) -> Result<Vec<(i64, i64, Shelf)>> {
    let row = |r: &Row| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?));
    let found: Vec<(i64, i64, Option<String>)> = match chat {
        Some(chat) => {
            db.prepare_cached(NAMED_IN)?.query_map(params![chat, id], row)?.collect::<rusqlite::Result<_>>()?
        }
        None => db.prepare_cached(NAMED)?.query_map([id], row)?.collect::<rusqlite::Result<_>>()?,
    };
    Ok(found.into_iter().filter_map(|(n, t, shelf)| Some((n, t, Shelf::of(shelf.as_deref()?)?))).collect())
}

const PERSON: &str = "SELECT EXISTS (SELECT 1 FROM people WHERE n = ?1)";
const PERSON_JIDS: &str =
    "SELECT j.n, instr(' ' || coalesce((SELECT value FROM sync_state WHERE key = 'me'), '') || ' ',
        ' ' || j.jid || ' ') > 0
    FROM person_jids j WHERE j.person = ?1";

/// Whose items person `n` sent: theirs, by their JIDs, or yours if they're you; None if jaw has no
/// such person.
pub fn person(db: &Connection, n: i64) -> Result<Option<Sender>> {
    if !db.prepare_cached(PERSON)?.query_row([n], |r| r.get::<_, bool>(0))? {
        return Ok(None);
    }
    let jids: Vec<(i64, bool)> = db
        .prepare_cached(PERSON_JIDS)?
        .query_map([n], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if jids.iter().any(|&(_, me)| me) {
        return Ok(Some(Sender::Me));
    }
    Ok(Some(Sender::Jids(jids.into_iter().map(|(jid, _)| jid).collect())))
}

/// An item as its row shows it.
#[derive(Debug)]
pub struct Item {
    pub n: i64,
    pub id: String,
    pub chat_n: i64,
    pub chat: String,
    pub v: i64,
    pub at: i64,
    pub t: i64,
    pub mine: bool,
    pub shelf: Shelf,
    /// Its message's caption, without a file's name, which documents' text ends with.
    pub caption: String,
    /// Its message's line: an audio file's has its length.
    pub summary: String,
    /// What message rows have as `media`: what WhatsApp said of the file, and the copy's state.
    pub media: Map<String, Value>,
    pub from: Option<From>,
}

const ITEM: &str = "SELECT m.n, m.id, m.chat, c.id, m.v, m.at, m.t, m.mine, m.kind, m.shelf, m.text,
        coalesce(m.summary, m.text, ''), m.detail, m.view_once, p.n, p.name, p.push_name, p.verified_name, j.jid,
        (SELECT json_object('state', iif(f.path IS NOT NULL, 'local', md.state), 'progress', md.progress,
                'error', md.error, 'path', f.path, 'small', f.small, 'large', f.large, 'thumbnail', f.thumbnail,
                'has_thumbnail', md.thumbnail, 'sha256', hex(md.sha256),
                'hd', (SELECT json_object('path', hf.path) FROM media h LEFT JOIN media_files hf ON hf.sha256 = h.sha256
                    WHERE h.message = m.n AND h.variant = 'hd'))
            FROM media md LEFT JOIN media_files f ON f.sha256 = md.sha256 WHERE md.message = m.n AND md.variant = 'sd')
    FROM messages m JOIN chats c ON c.n = m.chat LEFT JOIN person_jids j ON j.n = m.sender
    LEFT JOIN people p ON p.n = j.person
    WHERE m.n = ?1";

/// The items with these n that are still items, in the order given; showing them tells media of
/// the copies they lack, as a messages window's rows do.
pub fn items(db: &Connection, ns: &[i64]) -> Result<Vec<Item>> {
    let files = Files::of(db);
    let mut select = db.prepare_cached(ITEM)?;
    let mut items = Vec::with_capacity(ns.len());
    for &n in ns {
        let item = select.query_row([n], |r| item(r, files.as_deref())).optional()?;
        items.extend(item.flatten());
    }
    Ok(items)
}

fn item(r: &Row, files: Option<&Files>) -> rusqlite::Result<Option<Item>> {
    let Some(shelf) = r.get::<_, Option<String>>(9)?.as_deref().and_then(Shelf::of) else { return Ok(None) };
    let (n, kind): (i64, String) = (r.get(0)?, r.get(8)?);
    let mut detail: Option<String> = r.get(12)?;
    with_media(&mut detail, n, &kind, r.get(13)?, r.get(19)?, files);
    let media = match detail.as_deref().map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(mut detail))) => match detail.remove("media") {
            Some(Value::Object(media)) => media,
            _ => Map::new(),
        },
        _ => Map::new(),
    };
    let text: String = r.get::<_, Option<String>>(10)?.unwrap_or_default();
    let caption = match media.get("name").and_then(Value::as_str).filter(|_| kind == "document") {
        Some(name) if text == name => String::new(),
        Some(name) => text.strip_suffix(name).and_then(|t| t.strip_suffix('\n')).unwrap_or(&text).to_string(),
        None => text,
    };
    let from = match r.get::<_, Option<i64>>(14)? {
        Some(id) => {
            let jid: Option<String> = r.get(18)?;
            Some(From { id, name: shown(r.get(15)?, r.get(16)?, r.get(17)?, jid.as_deref()) })
        }
        None => None,
    };
    Ok(Some(Item {
        n,
        id: r.get(1)?,
        chat_n: r.get(2)?,
        chat: r.get(3)?,
        v: r.get(4)?,
        at: r.get(5)?,
        t: r.get(6)?,
        mine: r.get(7)?,
        shelf,
        caption,
        summary: r.get(11)?,
        media,
        from,
    }))
}

const CHAT_NAME: &str = "SELECT c.id, c.name, p.name, p.push_name, p.verified_name,
        iif(c.name IS NULL AND c.kind <> 'direct', (SELECT group_concat(shown, ', ') FROM (
            SELECT coalesce(gp.name, gp.push_name, gp.verified_name) AS shown FROM group_members g
                JOIN person_jids gj ON gj.n = g.member JOIN people gp ON gp.n = gj.person
            WHERE g.chat = c.n AND shown IS NOT NULL LIMIT 4)), NULL)
    FROM chats c LEFT JOIN people p ON p.n = c.person WHERE c.n = ?1";

/// Each of these chats' names, as the chat list shows them, by n.
pub fn chat_names(db: &Connection, chats: impl IntoIterator<Item = i64>) -> Result<HashMap<i64, String>> {
    let mut select = db.prepare_cached(CHAT_NAME)?;
    let mut names = HashMap::new();
    for n in chats {
        if names.contains_key(&n) {
            continue;
        }
        let name = select
            .query_row([n], |r| {
                let id: String = r.get(0)?;
                Ok(chat_name(r.get(1)?, r.get(5)?, [r.get(2)?, r.get(3)?, r.get(4)?], &id))
            })
            .optional()?;
        if let Some(name) = name {
            names.insert(n, name);
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests;
