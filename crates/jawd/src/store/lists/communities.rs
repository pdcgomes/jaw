//! A chat list's rows as interfaces show them: each chat the list holds, but a community is one
//! row wherever its groups would show, bringing the groups of it the list holds when the window
//! has it open, and otherwise only summing them up. A community sits in the list where the newest
//! of what it brings there does, itself or one of those groups, so reading the list by keyset
//! finds it in its place: of what it brings, the one read where it sits gives it, and the rest are
//! skipped. A group whose community jaw doesn't have is a row of its own.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use super::super::read::{self, CHAT_SELECT, ChatCursor, ChatRow, Last, OWN_JOINS, chat_row};
use super::{List, chats, chats_above, community_groups, community_members, held};

/// A row of a chat list: a chat, or a community with what it sums up of the groups of it the list
/// holds, and those groups when the window has it open.
#[derive(Debug)]
pub struct Entry {
    pub row: ChatRow,
    /// An open community's groups that the list holds, the latest written in first; none for a
    /// chat or a closed community.
    pub groups: Vec<ChatRow>,
    /// Where it sits in the list: a chat's own place, or the newest of what a community brings.
    pub place: ChatCursor,
    /// For a community, the unread messages and mentions of its groups the list holds.
    pub unread: i64,
    pub mentions: i64,
    /// For a community, the newest message of its groups the list holds.
    pub last: Option<Last>,
}

impl Entry {
    fn chat(row: ChatRow) -> Entry {
        Entry { place: row.cursor(), row, groups: Vec::new(), unread: 0, mentions: 0, last: None }
    }

    pub fn is_community(&self) -> bool {
        self.row.kind == "community"
    }
}

/// The fewest chats read at a time while a list's rows are gathered.
const READ_LEAST: usize = 16;

fn key(cursor: &ChatCursor) -> (&str, i64) {
    (cursor.sort.as_str(), cursor.n)
}

/// What a read of a list has found of a community.
enum Found {
    /// jaw has no community by its JID, or the list holds none of it.
    Absent,
    /// What the list holds of it, until it's given where it sits.
    Waiting(Box<Entry>),
    /// Given, where it sits.
    Given(ChatCursor),
}

impl Found {
    fn place(&self) -> Option<&ChatCursor> {
        match self {
            Found::Absent => None,
            Found::Waiting(entry) => Some(&entry.place),
            Found::Given(place) => Some(place),
        }
    }
}

/// What one read of a list has found of communities, by JID, so each is gathered once and given
/// once, and which the window has open.
#[derive(Default)]
pub struct Gathering {
    found: HashMap<String, Found>,
    /// The communities whose groups the window brings; every community's when None.
    open: Option<HashSet<String>>,
}

impl Gathering {
    /// A read bringing the groups of the communities `open` names, or of every one.
    pub fn new(open: Option<&[String]>) -> Gathering {
        Gathering { found: HashMap::new(), open: open.map(|open| open.iter().cloned().collect()) }
    }

    /// What the list holds of community `id`, gathered the first time it's asked for.
    fn find(&mut self, db: &Connection, list: &List, id: &str, now: i64) -> Result<&mut Found> {
        if !self.found.contains_key(id) {
            let open = self.open.as_ref().is_none_or(|open| open.contains(id));
            let found = match gather(db, list, id, open, now)? {
                Some(entry) => Found::Waiting(Box::new(entry)),
                None => Found::Absent,
            };
            self.found.insert(id.to_string(), found);
        }
        Ok(self.found.get_mut(id).expect("just found"))
    }

    /// Community `id` as what the list holds of it, once, where it sits; None if the list holds
    /// none of it or it was given already.
    fn take(&mut self, db: &Connection, list: &List, id: &str, now: i64) -> Result<Option<Entry>> {
        let found = self.find(db, list, id, now)?;
        let Found::Waiting(entry) = found else { return Ok(None) };
        let place = entry.place.clone();
        let Found::Waiting(entry) = std::mem::replace(found, Found::Given(place)) else { unreachable!() };
        Ok(Some(*entry))
    }

    /// `row`, read from the list, as the list gives it: as itself, or as its community's row where
    /// that sits, or not at all where its community sits elsewhere or was given already.
    fn row(&mut self, db: &Connection, list: &List, row: ChatRow, now: i64) -> Result<Option<Entry>> {
        let Some(id) = community(&row).map(str::to_string) else { return Ok(Some(Entry::chat(row))) };
        match self.find(db, list, &id, now)? {
            Found::Absent => Ok(Some(Entry::chat(row))),
            Found::Waiting(entry) if entry.place == row.cursor() => self.take(db, list, &id, now),
            Found::Waiting(_) | Found::Given(_) => Ok(None),
        }
    }
}

/// The community chat `row` is, or the one it names as a group's, whether or not jaw has it.
fn community(row: &ChatRow) -> Option<&str> {
    match row.kind.as_str() {
        "community" => Some(row.id.as_str()),
        _ => row.community.as_deref(),
    }
}

/// Community `id`'s row, if jaw has it as a community, with its own newest message as its last.
pub fn community_row(db: &Connection, id: &str) -> Result<Option<ChatRow>> {
    let sql = format!("{CHAT_SELECT} FROM chats c {OWN_JOINS} WHERE c.id = ?1 AND c.kind = 'community'");
    Ok(db.prepare_cached(&sql)?.query_row([id], chat_row).optional()?)
}

/// What `list` holds of community `id`: its row, what it sums up of the groups of it the list holds,
/// those groups' rows when it's `open`, and its place, the newest of them and of itself if the
/// list holds it. None when jaw has no community `id` or the list holds none of it. A closed
/// community reads its groups' places, counts and last messages' times alone, and the row of the
/// group whose last message is newest for its line, as an open one takes that group's.
fn gather(db: &Connection, list: &List, id: &str, open: bool, now: i64) -> Result<Option<Entry>> {
    let Some(row) = community_row(db, id)? else { return Ok(None) };
    let own = held(db, list, std::slice::from_ref(&row), now)?.contains(&row.n);
    let (groups, places, unread, mentions, last) = if open {
        let groups = community_groups(db, list, id, now)?;
        let places: Vec<ChatCursor> = groups.iter().map(ChatRow::cursor).collect();
        let sum = |count: fn(&ChatRow) -> i64| groups.iter().map(count).sum::<i64>();
        let (unread, mentions) = (sum(|g| g.unread), sum(|g| g.mentions));
        let last = groups
            .iter()
            .filter_map(|g| g.last.as_ref().map(|last| (last.at, g.n, last)))
            .max_by_key(|&(at, n, _)| (at, n))
            .map(|(_, _, last)| last.clone());
        (groups, places, unread, mentions, last)
    } else {
        let members = community_members(db, list, id, now)?;
        let newest = members.iter().filter_map(|m| m.last.map(|at| (at, m.n))).max().map(|(_, n)| n);
        let last = match newest {
            Some(n) => read::chat(db, n)?.and_then(|row| row.last),
            None => None,
        };
        let (unread, mentions) = (members.iter().map(|m| m.unread).sum(), members.iter().map(|m| m.mentions).sum());
        (Vec::new(), members.into_iter().map(|m| m.cursor).collect(), unread, mentions, last)
    };
    let place = places.into_iter().chain(own.then(|| row.cursor())).max_by(|a, b| key(a).cmp(&key(b)));
    Ok(place.map(|place| Entry { row, groups, place, unread, mentions, last }))
}

/// Up to `limit` rows of `list` below `after`, or from the top without one, in its order.
pub fn below(
    db: &Connection,
    list: &List,
    after: Option<&ChatCursor>,
    limit: usize,
    now: i64,
    gathering: &mut Gathering,
) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut after = after.cloned();
    while out.len() < limit {
        let want = (limit - out.len()).max(READ_LEAST);
        let page = chats(db, list, after.as_ref(), want, now)?;
        let ended = page.len() < want;
        for row in page {
            after = Some(row.cursor());
            if let Some(entry) = gathering.row(db, list, row, now)? {
                out.push(entry);
                if out.len() == limit {
                    break;
                }
            }
        }
        if ended {
            break;
        }
    }
    Ok(out)
}

/// Up to `limit` rows of `list` from `cursor` down, the one there first if the list has it.
pub fn from(
    db: &Connection,
    list: &List,
    cursor: &ChatCursor,
    limit: usize,
    now: i64,
    gathering: &mut Gathering,
) -> Result<Vec<Entry>> {
    let after = ChatCursor { sort: cursor.sort.clone(), n: cursor.n.saturating_add(1) };
    below(db, list, Some(&after), limit, now, gathering)
}

/// Up to `limit` rows of `list` just above `cursor`, in its order, so the nearest is last.
pub fn above(
    db: &Connection,
    list: &List,
    cursor: &ChatCursor,
    limit: usize,
    now: i64,
    gathering: &mut Gathering,
) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut at = cursor.clone();
    while out.len() < limit {
        let want = (limit - out.len()).max(READ_LEAST);
        let page = chats_above(db, list, &at, want, now)?;
        let ended = page.len() < want;
        for row in page.into_iter().rev() {
            at = row.cursor();
            if let Some(entry) = gathering.row(db, list, row, now)? {
                out.push(entry);
                if out.len() == limit {
                    break;
                }
            }
        }
        if ended {
            break;
        }
    }
    out.reverse();
    Ok(out)
}

/// Where chat `id`'s row sits in `list`: its community's place for a community, or a group of one,
/// that the list holds any of; else its own, as `cursor` has it.
pub fn place(
    db: &Connection,
    list: &List,
    id: &str,
    cursor: ChatCursor,
    now: i64,
    gathering: &mut Gathering,
) -> Result<ChatCursor> {
    let found: Option<(String, Option<String>)> = db
        .prepare_cached("SELECT kind, community FROM chats WHERE id = ?1")?
        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let community = match found {
        Some((kind, _)) if kind == "community" => Some(id.to_string()),
        Some((_, community)) => community,
        None => None,
    };
    Ok(match community {
        Some(community) => gathering.find(db, list, &community, now)?.place().cloned().unwrap_or(cursor),
        None => cursor,
    })
}

/// The rows of `list` among chats `rows`, as a window held in its order reads them: each chat the
/// list holds, and each community among them the list holds any of, with the groups of it the
/// list holds. A group comes under its community only when that is among them, so a group whose
/// community came meanwhile waits for the window to take its new order.
pub fn among(
    db: &Connection,
    list: &List,
    rows: Vec<ChatRow>,
    now: i64,
    mut gathering: Gathering,
) -> Result<Vec<Entry>> {
    let held = held(db, list, &rows, now)?;
    let present: HashSet<String> =
        rows.iter().filter(|row| row.kind == "community").map(|row| row.id.clone()).collect();
    let mut out = Vec::new();
    for row in rows {
        if row.kind == "community" {
            out.extend(gathering.take(db, list, &row.id, now)?);
            continue;
        }
        if !held.contains(&row.n) {
            continue;
        }
        if let Some(community) = row.community.as_deref().filter(|community| present.contains(*community))
            && gathering.find(db, list, community, now)?.place().is_some()
        {
            continue;
        }
        out.push(Entry::chat(row));
    }
    Ok(out)
}
