//! The rows of the interface's two windows, kept by ID: an update upserts rows, the newest version
//! winning, and removes them, so applying one twice or two out of order ends the same.

use crate::data::{Chat, Message};
use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;

/// The chat list on All.
#[derive(Debug, Default)]
pub struct Chats {
    rows: HashMap<String, Chat>,
    /// By `sort`, newest first.
    order: Vec<String>,
    /// The order as it was when moving through the list began, kept while it goes on.
    held: Option<Vec<String>>,
    pub total: usize,
}

impl Chats {
    pub fn apply(&mut self, reset: bool, upsert: Vec<Chat>, remove: &[String], total: usize) {
        if reset {
            self.rows.clear();
        }
        for id in remove {
            self.rows.remove(id);
        }
        for chat in upsert {
            match self.rows.get(&chat.id) {
                Some(old) if old.v > chat.v => {}
                _ => {
                    self.rows.insert(chat.id.clone(), chat);
                }
            }
        }
        self.total = total.max(self.rows.len());
        self.sort();
    }

    /// Sorts the order in place, which is nearly sorted already when one chat has moved; it's
    /// rebuilt only when chats came or went.
    fn sort(&mut self) {
        let rows = &self.rows;
        if self.order.len() != rows.len() || self.order.iter().any(|id| !rows.contains_key(id)) {
            self.order = rows.keys().cloned().collect();
        }
        self.order.sort_by(|a, b| rows[b].sort.cmp(&rows[a].sort).then_with(|| a.cmp(b)));
    }

    pub fn get(&self, id: &str) -> Option<&Chat> {
        self.rows.get(id)
    }

    /// The chats as the list shows them: in order, or as they were while the order is held, with
    /// any that came since first.
    pub fn order(&self) -> Vec<&str> {
        match &self.held {
            Some(held) => {
                let mut out: Vec<&str> =
                    self.order.iter().filter(|id| !held.contains(id)).map(String::as_str).collect();
                out.extend(held.iter().filter(|id| self.rows.contains_key(*id)).map(String::as_str));
                out
            }
            None => self.order.iter().map(String::as_str).collect(),
        }
    }

    /// Where a chat is in the list as shown.
    pub fn position(&self, id: &str) -> Option<usize> {
        match &self.held {
            Some(_) => self.order().iter().position(|c| *c == id),
            None => self.order.iter().position(|c| c == id),
        }
    }

    pub fn hold(&mut self) {
        if self.held.is_none() {
            self.held = Some(self.order.clone());
        }
    }

    pub fn release(&mut self) -> bool {
        self.held.take().is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn unread(&self) -> u32 {
        self.rows.values().map(|c| c.unread).sum()
    }
}

/// The open chat's messages, oldest first.
#[derive(Debug)]
pub struct Messages {
    pub chat: String,
    rows: HashMap<String, Message>,
    /// `sort` to ID.
    order: BTreeMap<String, String>,
    /// Whether there's more before the window.
    pub older: bool,
    /// How many have come past the window's end while it doesn't follow the newest.
    pub newer: u32,
    /// Whether the window has had its first rows.
    pub loaded: bool,
    /// Whether the window was asked to follow the newest.
    pub following: bool,
}

impl Messages {
    pub fn new(chat: &str) -> Messages {
        Messages {
            chat: chat.to_string(),
            rows: HashMap::new(),
            order: BTreeMap::new(),
            older: false,
            newer: 0,
            loaded: false,
            following: true,
        }
    }

    /// Applies an update; returns the IDs that left, so what was laid out for them can go too.
    pub fn apply(
        &mut self,
        reset: bool,
        upsert: Vec<Message>,
        remove: &[String],
        older: Option<bool>,
        newer: Option<u32>,
    ) -> Vec<String> {
        let mut gone = vec![];
        if reset {
            let ids: Vec<String> =
                self.rows.keys().filter(|id| !upsert.iter().any(|m| &m.id == *id)).cloned().collect();
            for id in ids {
                self.take(&id);
                gone.push(id);
            }
            self.loaded = true;
        }
        for id in remove {
            if self.take(id) {
                gone.push(id.clone());
            }
        }
        for m in upsert {
            if self.rows.get(&m.id).is_some_and(|old| old.v > m.v) {
                continue;
            }
            if let Some(old) = self.rows.get(&m.id)
                && old.sort != m.sort
            {
                self.order.remove(&old.sort);
            }
            self.order.insert(m.sort.clone(), m.id.clone());
            self.rows.insert(m.id.clone(), m);
        }
        if let Some(older) = older {
            self.older = older;
        }
        if let Some(newer) = newer {
            self.newer = newer;
        }
        gone
    }

    fn take(&mut self, id: &str) -> bool {
        match self.rows.remove(id) {
            Some(m) => {
                self.order.remove(&m.sort);
                true
            }
            None => false,
        }
    }

    pub fn get(&self, id: &str) -> Option<&Message> {
        self.rows.get(id)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn newest(&self) -> Option<&Message> {
        self.order.values().next_back().and_then(|id| self.rows.get(id))
    }

    pub fn oldest(&self) -> Option<&Message> {
        self.order.values().next().and_then(|id| self.rows.get(id))
    }

    /// The message just before `sort`.
    pub fn before(&self, sort: &str) -> Option<&Message> {
        self.order
            .range::<str, _>((Bound::Unbounded, Bound::Excluded(sort)))
            .next_back()
            .and_then(|(_, id)| self.rows.get(id))
    }

    pub fn after(&self, sort: &str) -> Option<&Message> {
        self.order
            .range::<str, _>((Bound::Excluded(sort), Bound::Unbounded))
            .next()
            .and_then(|(_, id)| self.rows.get(id))
    }

    /// The message at `sort`, or the nearest after it, or before it, for an anchor whose message
    /// has gone.
    pub fn nearest(&self, sort: &str) -> Option<&Message> {
        self.order
            .range::<str, _>((Bound::Included(sort), Bound::Unbounded))
            .next()
            .or_else(|| self.order.iter().next_back())
            .and_then(|(_, id)| self.rows.get(id))
    }

    /// How many come after `sort`.
    pub fn count_after(&self, sort: &str) -> usize {
        self.order.range::<str, _>((Bound::Excluded(sort), Bound::Unbounded)).count()
    }

    /// How many come before `sort`, counting no further than `most`.
    pub fn count_before(&self, sort: &str, most: usize) -> usize {
        self.order.range::<str, _>((Bound::Unbounded, Bound::Excluded(sort))).take(most).count()
    }

    /// The `n`th newest that isn't yours, counting from one: where unread messages begin.
    pub fn nth_newest_theirs(&self, n: usize) -> Option<&Message> {
        self.order.values().rev().filter_map(|id| self.rows.get(id)).filter(|m| !m.mine).nth(n.saturating_sub(1))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::data::{ChatKind, Kind};

    fn chat(id: &str, v: u64, sort: &str) -> Chat {
        Chat {
            id: id.into(),
            v,
            sort: sort.into(),
            kind: ChatKind::Direct,
            name: id.into(),
            last: None,
            unread: 0,
            mentions: 0,
            pinned: false,
            muted_until: None,
            archived: false,
            marked_unread: false,
            draft: None,
            typing: vec![],
        }
    }

    pub fn message(id: &str, v: u64, sort: &str) -> Message {
        Message {
            id: id.into(),
            v,
            chat: "c".into(),
            sort: sort.into(),
            at: 0,
            from: None,
            mine: false,
            kind: Kind::Text,
            spans: vec![],
            reply: None,
            reactions: vec![],
            status: None,
            edited: false,
            deleted: false,
            summary: None,
            system: None,
        }
    }

    #[test]
    fn chats_converge_in_any_order_and_hold_their_order_while_moving() {
        let mut a = Chats::default();
        a.apply(true, vec![chat("ana", 1, "0~2"), chat("rui", 1, "0~1")], &[], 2);
        assert_eq!(a.order(), ["ana", "rui"]);
        let newer = chat("rui", 2, "0~3");
        a.apply(false, vec![newer.clone(), chat("rui", 1, "0~1")], &[], 2);
        assert_eq!(a.order(), ["rui", "ana"], "an older version doesn't undo a newer one");
        a.hold();
        a.apply(false, vec![chat("ana", 2, "0~4"), chat("sofia", 1, "0~5")], &[], 3);
        assert_eq!(a.order(), ["sofia", "rui", "ana"], "held, with what came since first");
        assert!(a.release());
        assert_eq!(a.order(), ["sofia", "ana", "rui"]);
    }

    #[test]
    fn messages_keep_their_order_and_say_what_left() {
        let mut w = Messages::new("ana");
        w.apply(true, vec![message("b", 1, "2"), message("a", 1, "1")], &[], Some(true), None);
        assert_eq!(w.oldest().unwrap().id, "a");
        assert_eq!(w.newest().unwrap().id, "b");
        assert_eq!(w.after("1").unwrap().id, "b");
        assert!(w.before("1").is_none());
        let gone = w.apply(true, vec![message("c", 1, "3"), message("b", 2, "2")], &[], None, Some(2));
        assert_eq!(gone, ["a"]);
        assert_eq!((w.count_after(""), w.newer, w.older), (2, 2, true));
        assert_eq!(w.nearest("1").unwrap().id, "b", "an anchor that went falls to the next");
    }
}
