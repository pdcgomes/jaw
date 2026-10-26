//! What a window is and the query that fills it. A window is a stretch of a list around an
//! anchor: a folder's chat list from its top or around a chat, a chat's messages from its newest or
//! around a message, a folder's roster from its top or around someone; or the folder tree, whole.
//! Each query reads the window whole in one read transaction, with the feed's seq it's as of, so
//! what an interface is sent always matches a fresh query.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use jaw_proto::windows as proto;
use rusqlite::Connection;
use serde_json::{Map, Value};

use super::hashed;
use crate::store::lists::{self, List, PersonCursor};
use crate::store::read::{self, Around, ChatCursor, MessageCursor};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Spec {
    Chats(ChatSpec),
    Messages(MessageSpec),
    Folders,
    Roster(RosterSpec),
}

/// A stretch of a folder's chat list: from the top, or `before` chats above a chat and `after`
/// below it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ChatSpec {
    pub list: List,
    /// None for the top.
    pub anchor: Option<ChatAnchor>,
    pub before: usize,
    pub after: usize,
}

/// The chat a window follows as its place changes, and the place it was last seen at, which the
/// window keeps if the chat goes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ChatAnchor {
    pub id: String,
    pub cursor: ChatCursor,
}

/// A stretch of a chat's messages: its newest `before`, or `before` messages before one and
/// `after` after it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct MessageSpec {
    pub chat: String,
    /// None to follow the newest.
    pub anchor: Option<MessageAnchor>,
    pub before: usize,
    pub after: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct MessageAnchor {
    pub id: String,
    pub cursor: MessageCursor,
}

/// A stretch of a folder's roster, as a chat list's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RosterSpec {
    pub list: List,
    /// Whether people who aren't around are in it.
    pub offline: bool,
    pub anchor: Option<PersonAnchor>,
    pub before: usize,
    pub after: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct PersonAnchor {
    pub n: i64,
    pub cursor: PersonCursor,
}

/// The furthest `newer` counts, so it costs the same however far back a window is.
const MOST_NEWER: u64 = 1_000;

/// A window as one read transaction found it. Rows come in the list's order, each with its n.
pub(super) struct Snapshot {
    pub seq: i64,
    /// The oldest seq the feed keeps.
    pub oldest: i64,
    pub total: u64,
    pub older: bool,
    pub newer: u64,
    pub rows: Rows,
    /// A messages window's chat, by n, if it exists.
    pub chat: Option<i64>,
    /// The people who sent a messages window's rows.
    pub senders: HashSet<i64>,
    /// The JIDs a messages window's rows mention or quote someone by, `hashed`, whether or not
    /// jaw knows whose they are yet.
    pub named: HashSet<u64>,
    /// Whose presence the window shows: a roster's people, a one-to-one chat's person.
    pub people: Vec<i64>,
}

pub(super) enum Rows {
    Chats(Vec<(i64, proto::ChatRow)>),
    Messages(Vec<(i64, proto::MessageRow)>),
    Folders(Vec<(i64, proto::FolderRow)>, Vec<proto::Favourite>),
    People(Vec<(i64, proto::RosterRow)>),
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Reads the window `spec` describes, or, for a chat list given `only`, just those chats as they
/// are now, wherever they've moved; and gives the spec back with its anchor where it was found.
pub(super) fn query(db: &Connection, mut spec: Spec, only: Option<Vec<i64>>) -> Result<(Snapshot, Spec)> {
    let db = db.unchecked_transaction()?;
    let now = now_ms();
    let (oldest, seq) = read::feed_bounds(&db)?;
    let mut snapshot = Snapshot {
        seq,
        oldest,
        total: 0,
        older: false,
        newer: 0,
        rows: Rows::Chats(Vec::new()),
        chat: None,
        senders: HashSet::new(),
        named: HashSet::new(),
        people: Vec::new(),
    };
    match &mut spec {
        Spec::Chats(spec) => {
            snapshot.total = lists::chat_count(&db, &spec.list, now)?;
            let rows = match (&mut spec.anchor, only) {
                (_, Some(only)) => {
                    let rows = read::chats_by_n(&db, &only)?;
                    let held = lists::held(&db, &spec.list, &rows, now)?;
                    rows.into_iter().filter(|row| held.contains(&row.n)).collect()
                }
                (None, None) => lists::chats(&db, &spec.list, None, spec.after, now)?,
                (Some(anchor), None) => {
                    if let Some((_, cursor, _)) = read::chat_place(&db, &anchor.id)? {
                        anchor.cursor = cursor;
                    }
                    let mut rows = lists::chats_above(&db, &spec.list, &anchor.cursor, spec.before, now)?;
                    let mut from = lists::chats_from(&db, &spec.list, &anchor.cursor, spec.after + 1, now)?;
                    if from.first().is_none_or(|row| row.cursor() != anchor.cursor) {
                        from.truncate(spec.after);
                    }
                    rows.append(&mut from);
                    rows
                }
            };
            let rows = with_groups(&db, &spec.list, rows, now)?;
            snapshot.rows = Rows::Chats(gathered(rows.into_iter().map(|row| (row.n, chat_row(row))).collect()));
        }
        Spec::Messages(spec) => {
            snapshot.chat = read::chat_n(&db, &spec.chat)?;
            snapshot.total = read::message_count(&db, &spec.chat)?;
            if let Some(person) = read::chat_person(&db, &spec.chat)? {
                snapshot.people.push(person);
            }
            let rows = match &mut spec.anchor {
                None => {
                    let rows = read::messages(&db, &spec.chat, Around::Newest, spec.before)?;
                    snapshot.older = match rows.first() {
                        Some(first) => read::messages_before(&db, &spec.chat, first.cursor())?,
                        None => snapshot.total > 0,
                    };
                    rows
                }
                Some(anchor) => {
                    if let Some(cursor) = read::message_place(&db, &spec.chat, &anchor.id)? {
                        anchor.cursor = cursor;
                    }
                    let mut rows = read::messages(&db, &spec.chat, Around::Before(anchor.cursor), spec.before)?;
                    let mut from = read::messages_from(&db, &spec.chat, anchor.cursor, spec.after + 1)?;
                    if from.first().is_none_or(|row| row.cursor() != anchor.cursor) {
                        from.truncate(spec.after);
                    }
                    rows.append(&mut from);
                    let first = rows.first().map_or(anchor.cursor, |row| row.cursor());
                    let last = rows.last().map_or(anchor.cursor, |row| row.cursor());
                    snapshot.older = read::messages_before(&db, &spec.chat, first)?;
                    snapshot.newer = read::messages_after(&db, &spec.chat, last, MOST_NEWER)?;
                    rows
                }
            };
            snapshot.senders = rows.iter().filter_map(|row| row.from.as_ref().map(|from| from.id)).collect();
            snapshot.named = rows.iter().filter_map(|row| row.quoted.as_deref().map(hashed)).collect();
            let rows: Vec<(i64, proto::MessageRow)> = rows.into_iter().map(|row| (row.n, message_row(row))).collect();
            snapshot.named.extend(rows.iter().flat_map(|(_, row)| mentioned(&row.spans)).map(hashed));
            snapshot.rows = Rows::Messages(rows);
        }
        Spec::Folders => {
            let rows = lists::folders(&db, now)?;
            snapshot.total = rows.len() as u64;
            let favourites = lists::favourites(&db)?
                .into_iter()
                .map(|favourite| proto::Favourite {
                    slot: favourite.slot.clamp(1, 9) as u8,
                    folder: favourite.folder,
                    name: favourite.chat.as_ref().map(|chat| chat.name.clone()),
                    chat: favourite.chat.map(|chat| chat.id),
                })
                .collect();
            snapshot.rows = Rows::Folders(rows.into_iter().map(|row| (row.n, folder_row(row))).collect(), favourites);
        }
        Spec::Roster(spec) => {
            snapshot.total = lists::roster_count(&db, &spec.list, spec.offline, now)?;
            let rows = match &mut spec.anchor {
                None => lists::roster(&db, &spec.list, None, spec.after, spec.offline, now)?,
                Some(anchor) => {
                    if let Some(cursor) = lists::person_place(&db, anchor.n)? {
                        anchor.cursor = cursor;
                    }
                    let mut rows =
                        lists::roster_above(&db, &spec.list, &anchor.cursor, spec.before, spec.offline, now)?;
                    let mut from =
                        lists::roster_from(&db, &spec.list, &anchor.cursor, spec.after + 1, spec.offline, now)?;
                    if from.first().is_none_or(|row| row.cursor() != anchor.cursor) {
                        from.truncate(spec.after);
                    }
                    rows.append(&mut from);
                    rows
                }
            };
            snapshot.people = rows.iter().map(|row| row.n).collect();
            snapshot.rows = Rows::People(rows.into_iter().map(|row| (row.n, roster_row(row))).collect());
        }
    }
    Ok((snapshot, spec))
}

/// The JIDs a row's spans mention someone by.
fn mentioned(spans: &Value) -> impl Iterator<Item = &str> {
    spans
        .as_array()
        .into_iter()
        .flatten()
        .filter(|span| span["t"] == "mention")
        .filter_map(|span| span["person"].as_str())
}

/// `rows` with the groups `list` holds of each community among them, which come with it.
fn with_groups(db: &Connection, list: &List, mut rows: Vec<read::ChatRow>, now: i64) -> Result<Vec<read::ChatRow>> {
    let communities: Vec<String> =
        rows.iter().filter(|row| row.kind == "community").map(|row| row.id.clone()).collect();
    if communities.is_empty() {
        return Ok(rows);
    }
    let mut present: HashSet<String> = rows.iter().map(|row| row.id.clone()).collect();
    for community in communities {
        for group in lists::community_groups(db, list, &community, now)? {
            if present.insert(group.id.clone()) {
                rows.push(group);
            }
        }
    }
    Ok(rows)
}

/// A community's groups sorted right after it, its announcement group first and the rest as they
/// sort themselves, when it's in the window; a community's own sort ends past any of theirs.
fn gathered(mut rows: Vec<(i64, proto::ChatRow)>) -> Vec<(i64, proto::ChatRow)> {
    let communities: HashMap<String, String> = rows
        .iter()
        .filter(|(_, row)| row.kind == "community")
        .map(|(_, row)| (row.id.clone(), row.sort.clone()))
        .collect();
    for (n, row) in &mut rows {
        if row.kind == "community" {
            row.sort.push_str("~~");
        } else if let Some(sort) = row.community.as_ref().and_then(|community| communities.get(community)) {
            row.sort =
                if row.kind == "announcement" { format!("{sort}~2~{n:012}") } else { format!("{sort}~1~{}", row.sort) };
        }
    }
    rows
}

/// A chat's sort on the wire: the store's, then its n, so no two chats share one.
pub(crate) fn chat_row(row: read::ChatRow) -> proto::ChatRow {
    proto::ChatRow {
        sort: format!("{}~{:012}", row.sort, row.n),
        id: row.id,
        v: row.v.max(0) as u64,
        kind: row.kind,
        name: row.name,
        avatar: row.avatar,
        last: row.last.map(|last| proto::Last {
            from: last.from,
            mine: last.mine,
            summary: last.summary,
            kind: last.kind,
            status: last.status,
            at: last.at,
        }),
        unread: row.unread.max(0) as u64,
        mentions: row.mentions.max(0) as u64,
        pinned: row.pinned,
        muted_until: row.muted_until,
        archived: row.archived,
        marked_unread: row.marked_unread,
        draft: row.draft,
        typing: Vec::new(),
        community: row.community,
        children_unread: row.children_unread.map(|n| n.max(0) as u64),
        folders: row.folders.into_iter().map(|n| format!("folder:{n}")).collect(),
        favourite: row.favourite.map(|slot| slot.clamp(1, 9) as u8),
    }
}

pub(crate) fn folder_row(row: lists::FolderRow) -> proto::FolderRow {
    proto::FolderRow {
        id: row.id,
        v: row.v.max(0) as u64,
        parent: row.parent,
        name: row.name,
        position: row.position.clamp(0, i64::from(u32::MAX)) as u32,
        smart: row.smart,
        unread: row.unread.max(0) as u64,
        mentions: row.mentions.max(0) as u64,
        count: row.count.max(0) as u64,
        favourite: row.favourite.map(|slot| slot.clamp(1, 9) as u8),
    }
}

/// Someone's sort on the wire: the store's, then a byte lower than any a name has, then their n, so
/// bytewise it orders as the store's (sort, n) does.
fn roster_row(row: lists::PersonRow) -> proto::RosterRow {
    let state = match (row.presence.as_deref(), row.last_seen) {
        (Some(state), _) => state.to_string(),
        (None, Some(at)) if at > 0 => "seen".into(),
        _ => "hidden".into(),
    };
    let at = (state == "seen").then_some(row.last_seen).flatten();
    proto::RosterRow {
        id: format!("person:{}", row.n),
        v: row.v.max(0) as u64,
        sort: format!("{}\u{1}{:012}", row.sort, row.n),
        name: row.name,
        phone: row.phone,
        about: row.about,
        avatar: row.avatar,
        presence: proto::Presence { state, at, known: row.followed },
        chat: row.chat,
        folders: row.folders.into_iter().map(|n| format!("folder:{n}")).collect(),
    }
}

/// How many sender colours there are for an interface to choose from.
const COLOURS: i64 = 8;

/// A message's sort on the wire: its time, then its n, both zero-padded so they compare bytewise.
pub(crate) fn message_row(row: read::MessageRow) -> proto::MessageRow {
    let json = |raw: &serde_json::value::RawValue| serde_json::from_str::<Value>(raw.get()).unwrap_or(Value::Null);
    let detail = match row.detail.as_deref().map(json) {
        Some(Value::Object(detail)) => detail,
        _ => Map::new(),
    };
    proto::MessageRow {
        sort: format!("{:013}~{:012}", row.at.max(0), row.n),
        id: row.id,
        v: row.v.max(0) as u64,
        chat: row.chat,
        at: row.at,
        from: row.from.map(|from| proto::From {
            id: format!("person:{}", from.id),
            name: from.name,
            colour: from.id.rem_euclid(COLOURS) as u8,
        }),
        mine: row.mine,
        kind: row.kind,
        spans: json(&row.spans),
        reply: row.reply.as_deref().map(json),
        forwarded: row.forwarded.clamp(0, i64::from(u32::MAX)) as u32,
        reactions: serde_json::from_str(row.reactions.get()).unwrap_or_default(),
        status: row.status,
        edited: row.edited,
        deleted: row.deleted,
        expires_at: row.expires_at,
        view_once: row.view_once,
        starred: row.starred,
        detail,
    }
}
