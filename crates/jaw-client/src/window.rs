//! Window models: a window's rows as jawd sent them, kept by id and ordered by `sort`, applying
//! watch results, moves and pushes alike. They know nothing of the connection, so any interface
//! keeps them the same way and a test drives them with made-up updates.
//!
//! An upserted row replaces the one held when its `v` is at least as new, since the same `v` can
//! come again with another `sort` while a chat list is held during a catch-up. A window holds only
//! the rows jawd sent for it, since jawd removes each row that leaves.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Bound;

use jaw_proto::windows::{Around, ChatRow, MessageRow};

/// A window's row: an id, a version that only grows, and where it sorts, bytewise.
pub trait Row: Clone {
    fn id(&self) -> &str;
    fn v(&self) -> u64;
    fn sort(&self) -> &str;
}

impl Row for ChatRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
    fn sort(&self) -> &str {
        &self.sort
    }
}

impl Row for MessageRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
    fn sort(&self) -> &str {
        &self.sort
    }
}

/// Where a window is anchored, and how many rows it keeps on each side of the anchor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub around: Around,
    pub before: u32,
    pub after: u32,
    /// For a chat list, the communities whose groups it brings; every community's when None.
    pub open: Option<Vec<String>>,
}

impl Place {
    /// The chat list from its first row.
    pub fn top(after: u32) -> Place {
        Place { around: Around::Top, before: 0, after, open: None }
    }

    /// A chat's newest messages; the window follows new ones.
    pub fn newest(before: u32) -> Place {
        Place { around: Around::Newest, before, after: 0, open: None }
    }

    /// A chat's first unread message, found once when the window is watched, or its newest when
    /// nothing is unread.
    pub fn unread(before: u32, after: u32) -> Place {
        Place { around: Around::Unread, before, after, open: None }
    }

    /// Around a row: a chat in the list, or a message in a chat.
    pub fn around(id: impl Into<String>, before: u32, after: u32) -> Place {
        Place { around: Around::Row(id.into()), before, after, open: None }
    }

    /// A chat list bringing the groups of these communities only, each other community coming as
    /// its one row.
    pub fn open(self, communities: impl IntoIterator<Item = String>) -> Place {
        let mut open: Vec<String> = communities.into_iter().collect();
        open.sort();
        open.dedup();
        Place { open: Some(open), ..self }
    }
}

/// A watch or move that landed: the window's new place, and how many messages came after its last
/// row then, from which what arrives since is counted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub place: Place,
    pub newer: u64,
}

/// What changes a window: a watch's or move's result, a push, or several of them merged.
#[derive(Clone, Debug, PartialEq)]
pub struct Update<R> {
    /// The feed's seq it's as of.
    pub seq: u64,
    /// Rows that left the window, applied before `upsert`.
    pub remove: Vec<String>,
    pub upsert: Vec<R>,
    pub total: Option<u64>,
    pub older: Option<bool>,
    pub newer: Option<u64>,
    pub moved: Option<Moved>,
}

impl<R> Default for Update<R> {
    fn default() -> Self {
        Update { seq: 0, remove: Vec::new(), upsert: Vec::new(), total: None, older: None, newer: None, moved: None }
    }
}

/// Updates merged as they arrive, so however many come between two frames, a window applies one,
/// and applying it ends where applying each in turn would. It keeps no more than the window does:
/// a row that came and went since the last frame leaves nothing behind.
#[derive(Debug)]
pub struct Merged<R> {
    seq: u64,
    remove: HashSet<String>,
    upsert: HashMap<String, R>,
    /// Rows upserted here that the model didn't hold when the last frame took the merge.
    fresh: HashSet<String>,
    total: Option<u64>,
    older: Option<bool>,
    newer: Option<u64>,
    moved: Option<Moved>,
}

impl<R> Default for Merged<R> {
    fn default() -> Self {
        Merged {
            seq: 0,
            remove: HashSet::new(),
            upsert: HashMap::new(),
            fresh: HashSet::new(),
            total: None,
            older: None,
            newer: None,
            moved: None,
        }
    }
}

impl<R: Row> Merged<R> {
    /// Takes `update` in after what's merged. `ids` are the rows the model will hold once the merge
    /// is applied, which this keeps current.
    pub fn add(&mut self, update: Update<R>, ids: &mut HashSet<String>) {
        for id in update.remove {
            if !ids.remove(&id) {
                continue;
            }
            self.upsert.remove(&id);
            if !self.fresh.remove(&id) {
                self.remove.insert(id);
            }
        }
        for row in update.upsert {
            if !ids.contains(row.id()) && !self.remove.contains(row.id()) {
                self.fresh.insert(row.id().to_string());
            }
            ids.insert(row.id().to_string());
            match self.upsert.get(row.id()) {
                Some(held) if held.v() > row.v() => {}
                _ => {
                    self.upsert.insert(row.id().to_string(), row);
                }
            }
        }
        self.seq = self.seq.max(update.seq);
        self.total = update.total.or(self.total);
        self.older = update.older.or(self.older);
        self.newer = update.newer.or(self.newer);
        self.moved = update.moved.or(self.moved.take());
    }

    pub fn into_update(self) -> Update<R> {
        Update {
            seq: self.seq,
            remove: self.remove.into_iter().collect(),
            upsert: self.upsert.into_values().collect(),
            total: self.total,
            older: self.older,
            newer: self.newer,
            moved: self.moved,
        }
    }
}

/// A window's rows by id, and their order by `sort`, then id, so two rows passing through the same
/// `sort` while a batch applies can't lose each other.
#[derive(Debug)]
struct Rows<R> {
    by_id: HashMap<String, R>,
    order: BTreeSet<(String, String)>,
}

impl<R> Default for Rows<R> {
    fn default() -> Self {
        Rows { by_id: HashMap::new(), order: BTreeSet::new() }
    }
}

impl<R: Row> Rows<R> {
    /// Applies an update's rows; returns the ids that left.
    fn apply(&mut self, remove: Vec<String>, upsert: Vec<R>) -> Vec<String> {
        let mut gone = Vec::new();
        for id in remove {
            if let Some(row) = self.by_id.remove(&id) {
                self.order.remove(&(row.sort().to_string(), id.clone()));
                gone.push(id);
            }
        }
        for row in upsert {
            match self.by_id.get(row.id()) {
                Some(held) if held.v() > row.v() => continue,
                Some(held) if held.sort() == row.sort() => {}
                Some(held) => {
                    self.order.remove(&(held.sort().to_string(), row.id().to_string()));
                    self.order.insert((row.sort().to_string(), row.id().to_string()));
                }
                None => {
                    self.order.insert((row.sort().to_string(), row.id().to_string()));
                }
            }
            self.by_id.insert(row.id().to_string(), row);
        }
        gone
    }

    fn row(&self, key: &(String, String)) -> &R {
        &self.by_id[&key.1]
    }

    /// From the smallest `sort` up.
    fn iter(&self) -> impl DoubleEndedIterator<Item = &R> + '_ {
        self.order.iter().map(|key| self.row(key))
    }

    fn below(&self, sort: &str) -> impl DoubleEndedIterator<Item = &(String, String)> + '_ {
        self.order.range::<(String, String), _>((Bound::Unbounded, Bound::Excluded((sort.to_string(), String::new()))))
    }

    fn from(&self, sort: &str) -> impl DoubleEndedIterator<Item = &(String, String)> + '_ {
        self.order.range::<(String, String), _>((Bound::Included((sort.to_string(), String::new())), Bound::Unbounded))
    }

    fn above<'a>(&'a self, sort: &'a str) -> impl Iterator<Item = &'a (String, String)> + 'a {
        self.from(sort).skip_while(move |key| key.0 == sort)
    }
}

/// A chat list: its chats from the largest `sort` down. While the interface moves through it, it
/// can hold its order, so the chat under the cursor stays put as others move.
#[derive(Debug)]
pub struct Chats<R> {
    pub folder: String,
    pub place: Place,
    rows: Rows<R>,
    held: Option<Held>,
    /// How many chats the whole list has.
    pub total: u64,
    /// The feed's seq the window is as of.
    pub seq: u64,
    /// Whether the window has had its first rows.
    pub loaded: bool,
}

/// The list's order when the hold began.
#[derive(Debug)]
struct Held {
    order: Vec<String>,
    ids: HashSet<String>,
}

impl<R: Row> Chats<R> {
    pub fn new(folder: &str, place: Place) -> Chats<R> {
        Chats { folder: folder.to_string(), place, rows: Rows::default(), held: None, total: 0, seq: 0, loaded: false }
    }

    /// Applies an update; returns the ids that left, so what was kept for them can go too.
    pub fn apply(&mut self, update: Update<R>) -> Vec<String> {
        let gone = self.rows.apply(update.remove, update.upsert);
        if let Some(total) = update.total {
            self.total = total;
        }
        if let Some(moved) = update.moved {
            self.place = moved.place;
        }
        self.seq = self.seq.max(update.seq);
        self.loaded = true;
        gone
    }

    pub fn get(&self, id: &str) -> Option<&R> {
        self.rows.by_id.get(id)
    }

    pub fn len(&self) -> usize {
        self.rows.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.by_id.is_empty()
    }

    /// The chats as the list shows them: from the largest `sort` down, or, while held, as they
    /// were when the hold began, with any that came since first.
    pub fn rows(&self) -> impl Iterator<Item = &R> + '_ {
        let held = self.held.as_ref();
        let came = self.rows.order.iter().rev().filter(move |key| held.is_none_or(|held| !held.ids.contains(&key.1)));
        let kept = held.into_iter().flat_map(|held| held.order.iter()).filter_map(|id| self.rows.by_id.get(id));
        came.map(|key| self.rows.row(key)).chain(kept)
    }

    /// Where a chat is in the list as shown.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.rows().position(|row| row.id() == id)
    }

    /// Keeps the list's order as it is until `release`; chats that come meanwhile go first.
    pub fn hold(&mut self) {
        if self.held.is_none() {
            let order: Vec<String> = self.rows.order.iter().rev().map(|key| key.1.clone()).collect();
            self.held = Some(Held { ids: order.iter().cloned().collect(), order });
        }
    }

    /// Lets the list take its order again; says whether it was held.
    pub fn release(&mut self) -> bool {
        self.held.take().is_some()
    }

    pub fn held(&self) -> bool {
        self.held.is_some()
    }
}

/// A chat's messages, from the smallest `sort` up.
#[derive(Debug)]
pub struct Messages<R> {
    pub chat: String,
    pub place: Place,
    rows: Rows<R>,
    /// How many messages the chat has.
    pub total: u64,
    /// Whether there are messages before the window.
    pub older: bool,
    /// How many messages come after the window's last row, counted up to 1,000.
    pub newer: u64,
    /// `newer` when the window was last watched or moved.
    newer_then: u64,
    pub seq: u64,
    pub loaded: bool,
}

impl<R: Row> Messages<R> {
    pub fn new(chat: &str, place: Place) -> Messages<R> {
        Messages {
            chat: chat.to_string(),
            place,
            rows: Rows::default(),
            total: 0,
            older: false,
            newer: 0,
            newer_then: 0,
            seq: 0,
            loaded: false,
        }
    }

    /// Applies an update; returns the ids that left, so what was laid out for them can go too.
    pub fn apply(&mut self, update: Update<R>) -> Vec<String> {
        let gone = self.rows.apply(update.remove, update.upsert);
        if let Some(total) = update.total {
            self.total = total;
        }
        if let Some(older) = update.older {
            self.older = older;
        }
        if let Some(newer) = update.newer {
            self.newer = newer;
        }
        if let Some(moved) = update.moved {
            self.place = moved.place;
            self.newer_then = moved.newer;
        }
        self.seq = self.seq.max(update.seq);
        self.loaded = true;
        gone
    }

    /// Whether the window follows the chat's newest messages.
    pub fn following(&self) -> bool {
        self.place.around == Around::Newest
    }

    /// Whether the window's last row is the chat's newest message, so an interface at the bottom
    /// can follow the newest from here.
    pub fn holds_newest(&self) -> bool {
        self.following() || self.newer == 0
    }

    /// How many messages have come after the window since it was last watched or moved: the
    /// "2 new" an interface shows below it.
    pub fn arrived(&self) -> u64 {
        self.newer.saturating_sub(self.newer_then)
    }

    pub fn get(&self, id: &str) -> Option<&R> {
        self.rows.by_id.get(id)
    }

    pub fn len(&self) -> usize {
        self.rows.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.by_id.is_empty()
    }

    /// Oldest first.
    pub fn rows(&self) -> impl DoubleEndedIterator<Item = &R> + '_ {
        self.rows.iter()
    }

    pub fn oldest(&self) -> Option<&R> {
        self.rows.iter().next()
    }

    pub fn newest(&self) -> Option<&R> {
        self.rows.iter().next_back()
    }

    /// The message just before `sort`.
    pub fn before(&self, sort: &str) -> Option<&R> {
        self.rows.below(sort).next_back().map(|key| self.rows.row(key))
    }

    /// The message just after `sort`.
    pub fn after(&self, sort: &str) -> Option<&R> {
        self.rows.above(sort).next().map(|key| self.rows.row(key))
    }

    /// The message at `sort`, or the nearest after it, or the newest, for an anchor whose message
    /// has gone.
    pub fn nearest(&self, sort: &str) -> Option<&R> {
        self.rows.from(sort).next().or_else(|| self.rows.order.iter().next_back()).map(|key| self.rows.row(key))
    }

    /// How many come after `sort`.
    pub fn count_after(&self, sort: &str) -> usize {
        self.rows.above(sort).count()
    }

    /// How many come before `sort`, counting no further than `most`.
    pub fn count_before(&self, sort: &str, most: usize) -> usize {
        self.rows.below(sort).rev().take(most).count()
    }
}

#[cfg(test)]
mod tests;
