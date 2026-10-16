//! What a window is and the query that fills it. A window is a stretch of a list around an
//! anchor: the chat list from its top or around a chat, a chat's messages from its newest or
//! around a message. Each query reads the window whole in one read transaction, with the feed's
//! seq it's as of, so what an interface is sent always matches a fresh query.

use std::collections::HashSet;

use anyhow::Result;
use jaw_proto::windows as proto;
use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::store::read::{self, Around, ChatCursor, MessageCursor};

#[derive(Clone, Debug)]
pub(super) enum Spec {
    Chats(ChatSpec),
    Messages(MessageSpec),
}

/// A stretch of the chat list: from the top, or `before` chats above a chat and `after` below it.
#[derive(Clone, Debug)]
pub(super) struct ChatSpec {
    pub archived: bool,
    /// None for the top.
    pub anchor: Option<ChatAnchor>,
    pub before: usize,
    pub after: usize,
}

/// The chat a window follows as its place changes, and the place it was last seen at, which the
/// window keeps if the chat goes.
#[derive(Clone, Debug)]
pub(super) struct ChatAnchor {
    pub id: String,
    pub cursor: ChatCursor,
}

/// A stretch of a chat's messages: its newest `before`, or `before` messages before one and
/// `after` after it.
#[derive(Clone, Debug)]
pub(super) struct MessageSpec {
    pub chat: String,
    /// None to follow the newest.
    pub anchor: Option<MessageAnchor>,
    pub before: usize,
    pub after: usize,
}

#[derive(Clone, Debug)]
pub(super) struct MessageAnchor {
    pub id: String,
    pub cursor: MessageCursor,
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
}

pub(super) enum Rows {
    Chats(Vec<(i64, proto::ChatRow)>),
    Messages(Vec<(i64, proto::MessageRow)>),
}

/// Reads the window `spec` describes, or, for a chat list given `only`, just those chats as they
/// are now, wherever they've moved; and gives the spec back with its anchor where it was found.
pub(super) fn query(db: &Connection, mut spec: Spec, only: Option<Vec<i64>>) -> Result<(Snapshot, Spec)> {
    let db = db.unchecked_transaction()?;
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
    };
    match &mut spec {
        Spec::Chats(spec) => {
            snapshot.total = read::chat_count(&db, spec.archived)?;
            let rows = match (&mut spec.anchor, only) {
                (_, Some(only)) => {
                    let mut rows = read::chats_by_n(&db, &only)?;
                    rows.retain(|row| row.archived == spec.archived);
                    rows
                }
                (None, None) => read::chats(&db, spec.archived, None, spec.after)?,
                (Some(anchor), None) => {
                    if let Some((_, cursor, _)) = read::chat_place(&db, &anchor.id)? {
                        anchor.cursor = cursor;
                    }
                    let mut rows = read::chats_above(&db, spec.archived, &anchor.cursor, spec.before)?;
                    let mut from = read::chats_from(&db, spec.archived, &anchor.cursor, spec.after + 1)?;
                    if from.first().is_none_or(|row| row.cursor() != anchor.cursor) {
                        from.truncate(spec.after);
                    }
                    rows.append(&mut from);
                    rows
                }
            };
            snapshot.rows = Rows::Chats(rows.into_iter().map(|row| (row.n, chat_row(row))).collect());
        }
        Spec::Messages(spec) => {
            snapshot.chat = read::chat_n(&db, &spec.chat)?;
            snapshot.total = read::message_count(&db, &spec.chat)?;
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
            snapshot.rows = Rows::Messages(rows.into_iter().map(|row| (row.n, message_row(row))).collect());
        }
    }
    Ok((snapshot, spec))
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
        children_unread: None,
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
        reactions: Vec::new(),
        status: row.status,
        edited: row.edited,
        deleted: row.deleted,
        expires_at: row.expires_at,
        view_once: row.view_once,
        starred: row.starred,
        detail,
    }
}
