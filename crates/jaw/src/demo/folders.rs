//! The made-up account's folders and roster, as jawd's would be: a tree of your folders holding
//! chats and people, the built-in and smart folders beside it, each with its chats' unread total
//! counted once, a chat list for any of them, and people whose presence changes as time goes by.

use super::{Account, Later};
use crate::data::{Chat, ChatKind, FolderRow, Presence, PresenceState, RosterRow, Update};
use crate::fmt::{DAY, MINUTE};
use jaw_client::Update as Window;
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// The built-in folders, always first.
const BUILT_IN: [(&str, &str); 5] = [
    ("all", "All"),
    ("unread", "Unread"),
    ("groups", "Groups"),
    ("communities", "Communities"),
    ("archived", "Archived"),
];

/// The account's people: their ID, name, number, one-to-one chat and about text.
const PEOPLE: &[(&str, &str, &str, Option<&str>, &str)] = &[
    ("person:1", "Ana Ribeiro", "+351 912 345 678", Some("ana"), ""),
    ("person:2", "Mãe", "+351 913 210 002", Some("mae"), ""),
    ("person:3", "Rui Costa", "+351 913 410 003", Some("rui"), "a escalar, não ligar"),
    ("person:4", "Inês Gomes", "+351 913 002 210", None, "Lisboa ⇄ Porto"),
    ("person:5", "Sofia Martins", "+351 913 550 005", Some("sofia"), "Disponível"),
    ("person:6", "Tiago Fonseca", "+351 913 660 006", Some("tiago"), ""),
    ("person:7", "Avó Lurdes", "+351 213 770 007", Some("avo"), ""),
    ("person:10", "Mariana Lopes", "+351 913 100 010", Some("mariana"), "at the studio"),
    ("person:20", "Marta", "+351 913 200 020", None, "belay certified"),
    ("person:21", "João", "+351 913 210 021", None, ""),
    ("person:22", "Leonor", "+351 913 220 022", None, "Lisbon for now"),
    ("person:23", "Duarte", "+351 913 230 023", None, ""),
    ("person:24", "Bea", "+351 913 240 024", None, ""),
];

/// Who's around at the start: online, typing, last seen minutes ago, or hidden.
fn presence_at_start(person: &str, now: i64) -> Presence {
    let seen = |minutes: i64| Presence { state: PresenceState::Seen, at: Some(now - minutes * MINUTE), known: true };
    let state = |state| Presence { state, at: None, known: true };
    match person {
        "person:1" => state(PresenceState::Typing),
        "person:3" | "person:20" | "person:21" => state(PresenceState::Online),
        "person:2" => seen(5),
        "person:4" => seen(120),
        "person:7" => seen(26 * 60),
        "person:6" => seen(3 * 24 * 60),
        "person:10" => seen(40),
        "person:22" => seen(50),
        "person:23" => seen(30 * 60),
        _ => state(PresenceState::Hidden),
    }
}

/// What changes as the made-up account runs, one every few seconds: who comes and goes.
const COMINGS: &[(&str, PresenceState)] = &[
    ("person:4", PresenceState::Online),
    ("person:3", PresenceState::Seen),
    ("person:2", PresenceState::Typing),
    ("person:2", PresenceState::Online),
    ("person:22", PresenceState::Online),
    ("person:4", PresenceState::Seen),
    ("person:2", PresenceState::Seen),
    ("person:3", PresenceState::Online),
    ("person:22", PresenceState::Seen),
];

/// The made-up folder tree: your folders, the smart ones, and what's in each.
pub struct Folders {
    pub rows: Vec<FolderRow>,
    pub items: HashMap<String, Vec<String>>,
    pub presence: HashMap<String, Presence>,
    pub next: u32,
    /// The chat list's folder, the roster's, and whether the tree is watched.
    pub list: String,
    pub roster: Option<String>,
    pub watched: bool,
    /// The folder rows last sent, to send only those that changed.
    sent: HashMap<String, FolderRow>,
}

impl Folders {
    pub fn made_up(now: i64) -> Folders {
        let mine = |id: &str, parent: Option<&str>, name: &str, position: i64| FolderRow {
            id: id.into(),
            parent: parent.map(String::from),
            name: name.into(),
            position,
            ..FolderRow::default()
        };
        let smart = |id: &str, name: &str, position: i64, rule| FolderRow {
            id: id.into(),
            name: name.into(),
            position,
            smart: Some(rule),
            ..FolderRow::default()
        };
        let mut rows: Vec<FolderRow> =
            BUILT_IN.iter().enumerate().map(|(i, (id, name))| mine(id, None, name, i as i64)).collect();
        rows.extend([
            FolderRow { favourite: Some(1), ..mine("folder:1", None, "Family", 0) },
            mine("folder:2", Some("folder:1"), "Parents", 0),
            mine("folder:3", Some("folder:1"), "Cousins", 1),
            mine("folder:4", None, "Work", 1),
            FolderRow { favourite: Some(3), ..mine("folder:5", None, "Climbing", 2) },
            mine("folder:6", None, "Ana's 40th", 3),
            smart("smart:1", "Waiting on me", 0, json!({ "waiting": 86_400 })),
            smart("smart:2", "Mentions me", 1, json!({ "mentions": true })),
            smart("smart:3", "Muted, unread", 2, json!({ "muted": true, "unread": true })),
        ]);
        let items = [
            ("folder:1", vec!["familia", "avo", "person:4"]),
            ("folder:2", vec!["mae"]),
            ("folder:3", vec!["person:4", "sofia"]),
            ("folder:4", vec!["mariana", "tiago"]),
            ("folder:5", vec!["climbing", "person:20", "person:21", "person:22", "person:23", "person:24"]),
            ("folder:6", vec!["jantar", "pedro-ana", "ana"]),
        ]
        .into_iter()
        .map(|(f, items)| (f.to_string(), items.into_iter().map(String::from).collect()))
        .collect();
        let presence = PEOPLE.iter().map(|p| (p.0.to_string(), presence_at_start(p.0, now))).collect();
        Folders {
            rows,
            items,
            presence,
            next: 7,
            list: "all".into(),
            roster: None,
            watched: false,
            sent: HashMap::new(),
        }
    }

    /// A folder and every folder under it.
    fn subtree(&self, folder: &str) -> Vec<String> {
        let mut out = vec![folder.to_string()];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i].clone();
            out.extend(self.rows.iter().filter(|r| r.parent.as_deref() == Some(&parent)).map(|r| r.id.clone()));
            i += 1;
        }
        out
    }

    /// The items in a folder and under it, each once.
    fn held(&self, folder: &str) -> HashSet<String> {
        self.subtree(folder).iter().flat_map(|f| self.items.get(f).cloned().unwrap_or_default()).collect()
    }
}

/// A person's one-to-one chat, and the person whose chat it is.
fn chat_of(person: &str) -> Option<&'static str> {
    PEOPLE.iter().find(|p| p.0 == person).and_then(|p| p.3)
}

fn person_of(chat: &str) -> Option<&'static str> {
    PEOPLE.iter().find(|p| p.3 == Some(chat)).map(|p| p.0)
}

impl Account {
    /// Whether `chat` is in `folder`'s chat list.
    pub(super) fn in_folder(&self, folder: &str, chat: &Chat, held: Option<&HashSet<String>>) -> bool {
        let now = self.now();
        let unread = chat.unread > 0 || chat.marked_unread;
        match folder {
            "all" => !chat.archived,
            "unread" => unread && !chat.archived,
            "groups" => matches!(chat.kind, ChatKind::Group | ChatKind::Announcement) && !chat.archived,
            "communities" => chat.kind == ChatKind::Community || chat.community.is_some(),
            "archived" => chat.archived,
            "smart:1" => {
                let theirs = chat.last.as_ref().is_some_and(|l| l.status.is_none() && now - l.at > DAY);
                theirs && chat.unread > 0 && !chat.archived
            }
            "smart:2" => chat.mentions > 0,
            "smart:3" => chat.muted(now) && unread,
            _ => {
                let own;
                let held = match held {
                    Some(h) => h,
                    None => {
                        own = self.folders.held(folder);
                        &own
                    }
                };
                held.contains(&chat.id) || person_of(&chat.id).is_some_and(|p| held.contains(p))
            }
        }
    }

    /// Every folder row with its counts as they are now.
    fn folder_rows(&self) -> Vec<FolderRow> {
        let mut rows = self.folders.rows.clone();
        for row in &mut rows {
            let held = self.folders.held(&row.id);
            let chats: Vec<&Chat> =
                self.threads.iter().map(|t| &t.chat).filter(|c| self.in_folder(&row.id, c, Some(&held))).collect();
            row.unread = chats.iter().filter(|c| c.kind != ChatKind::Community).map(|c| c.unread).sum();
            row.mentions = chats.iter().map(|c| c.mentions).sum();
            let people = held.iter().filter(|i| i.starts_with("person:") && chat_of(i).is_none()).count();
            row.count = (chats.len() + people) as u32;
        }
        rows
    }

    /// Sends the folder rows whose counts or names changed since they were last sent.
    pub(super) fn folders_changed(&mut self) {
        if !self.folders.watched {
            return;
        }
        let rows = self.folder_rows();
        let changed: Vec<FolderRow> = rows.into_iter().filter(|r| self.folders.sent.get(&r.id) != Some(r)).collect();
        if changed.is_empty() {
            return;
        }
        for r in &changed {
            self.folders.sent.insert(r.id.clone(), r.clone());
        }
        self.send(Update::Folders { upsert: changed, remove: vec![], whole: false });
    }

    pub(super) fn on_watch_folders(&mut self) {
        self.folders.watched = true;
        let rows = self.folder_rows();
        self.folders.sent = rows.iter().map(|r| (r.id.clone(), r.clone())).collect();
        self.send(Update::Folders { upsert: rows, remove: vec![], whole: true });
    }

    /// The people in a folder, sorted as jawd sorts them: around first, then by when last seen,
    /// then those who hide it.
    fn roster_rows(&self, folder: &str) -> Vec<RosterRow> {
        let held = self.folders.held(folder);
        let built_in = BUILT_IN.iter().any(|(id, _)| *id == folder) || folder.starts_with("smart:");
        PEOPLE
            .iter()
            .filter(|p| {
                if !built_in {
                    return held.contains(p.0) || p.3.is_some_and(|c| held.contains(c));
                }
                match p.3.and_then(|c| self.index.get(c)) {
                    Some(&t) => self.in_folder(folder, &self.threads[t].chat, None),
                    None => folder == "all",
                }
            })
            .map(|p| self.roster_row(p.0))
            .collect()
    }

    fn roster_row(&self, person: &str) -> RosterRow {
        let p = PEOPLE.iter().find(|p| p.0 == person).copied().unwrap_or(("", "", "", None, ""));
        let presence = self.folders.presence.get(person).copied().unwrap_or_default();
        let name = p.1.to_lowercase();
        let sort = match (presence.around(), presence.state, presence.at) {
            (true, _, _) => format!("0~{name}"),
            (false, PresenceState::Seen, Some(at)) => format!("1~{:019}~{name}", i64::MAX - at),
            _ => format!("2~{name}"),
        };
        let folders = self
            .folders
            .items
            .iter()
            .filter(|(_, items)| items.iter().any(|i| i == person || Some(i.as_str()) == p.3))
            .map(|(f, _)| f.clone())
            .collect();
        RosterRow {
            id: person.to_string(),
            sort,
            name: p.1.to_string(),
            phone: Some(p.2.to_string()),
            about: Some(p.4.to_string()).filter(|a| !a.is_empty()),
            presence,
            chat: p.3.map(String::from),
            folders,
        }
    }

    pub(super) fn on_watch_roster(&mut self, folder: &str) {
        self.folders.roster = Some(folder.to_string());
        let upsert = self.roster_rows(folder);
        self.send(Update::Roster { folder: folder.to_string(), upsert, remove: vec![], whole: true });
    }

    /// The roster again, as it is, for whatever changed in it.
    fn roster_changed(&mut self) {
        if let Some(folder) = self.folders.roster.clone() {
            let upsert = self.roster_rows(&folder);
            self.send(Update::Roster { folder, upsert, remove: vec![], whole: true });
        }
    }

    /// The next of who comes and goes.
    pub(super) fn presence_step(&mut self, i: usize) {
        let (person, state) = COMINGS[i % COMINGS.len()];
        let at = (state == PresenceState::Seen).then(|| self.now());
        self.folders.presence.insert(person.to_string(), Presence { state, at, known: true });
        self.roster_changed();
        self.after(6_000, Later::Presence(i + 1));
    }

    /// The chat list's folder: its chats, as `chats.watch` sends them.
    pub(super) fn list_rows(&mut self, folder: &str) -> Vec<Chat> {
        self.folders.list = folder.to_string();
        let held = self.folders.held(folder);
        self.threads.iter().map(|t| t.chat.clone()).filter(|c| self.in_folder(folder, c, Some(&held))).collect()
    }

    /// The folders an item is filed in directly, for its row.
    pub(super) fn filed(&self, item: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .folders
            .items
            .iter()
            .filter(|(_, items)| items.iter().any(|i| i == item))
            .map(|(f, _)| f.clone())
            .collect();
        out.sort();
        out
    }

    /// Something about folders changed: the rows, the chats' `folders`, the counts, the roster.
    fn refiled(&mut self) {
        for t in 0..self.threads.len() {
            let id = self.threads[t].chat.id.clone();
            let mut folders = self.filed(&id);
            if let Some(p) = person_of(&id) {
                folders.extend(self.filed(p));
                folders.sort();
                folders.dedup();
            }
            if self.threads[t].chat.folders != folders {
                self.threads[t].chat.folders = folders;
                self.threads[t].chat.v += 1;
                self.push_chat(t);
            }
        }
        let list = self.folders.list.clone();
        let rows = self.list_rows(&list);
        let total = Some(rows.len() as u64);
        self.send(Update::Chats(Window { upsert: rows, total, ..Window::default() }));
        self.folders_changed();
        self.roster_changed();
    }

    /// Each chat's `folders`, as it starts, without saying so.
    pub(super) fn file_chats(&mut self) {
        for t in 0..self.threads.len() {
            let id = self.threads[t].chat.id.clone();
            let mut folders = self.filed(&id);
            if let Some(p) = person_of(&id) {
                folders.extend(self.filed(p));
                folders.sort();
                folders.dedup();
            }
            self.threads[t].chat.folders = folders;
        }
    }

    fn mine(&self, folder: &str) -> bool {
        self.folders.rows.iter().any(|r| r.id == folder && r.yours())
    }

    fn refuse(&mut self, why: &str) {
        self.send(Update::Notice(why.to_string()));
    }

    pub(super) fn on_folder_create(&mut self, name: &str, parent: Option<&str>) {
        let name = name.trim();
        if name.is_empty() {
            return self.refuse("a folder needs a name");
        }
        if parent.is_some_and(|p| !self.mine(p)) {
            return self.refuse("folders go only in your own folders");
        }
        let siblings = self.folders.rows.iter().filter(|r| r.yours() && r.parent.as_deref() == parent);
        if siblings.clone().any(|r| r.name == name) {
            return self.refuse("a folder there has that name already");
        }
        let position = siblings.count() as i64;
        let id = format!("folder:{}", self.folders.next);
        self.folders.next += 1;
        let row = FolderRow {
            id,
            parent: parent.map(String::from),
            name: name.to_string(),
            position,
            ..FolderRow::default()
        };
        self.folders.rows.push(row.clone());
        self.folders_changed();
        self.send(Update::FolderMade(Box::new(row)));
    }

    pub(super) fn on_folder_rename(&mut self, folder: &str, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() || !self.mine(folder) {
            return self.refuse("only your own folders can be renamed");
        }
        if let Some(r) = self.folders.rows.iter_mut().find(|r| r.id == folder) {
            r.name = name;
        }
        self.folders_changed();
    }

    pub(super) fn on_folder_move(&mut self, folder: &str, parent: Option<&str>) {
        if !self.mine(folder)
            || parent.is_some_and(|p| !self.mine(p) || self.folders.subtree(folder).iter().any(|f| f == p))
        {
            return self.refuse("a folder can't go there");
        }
        let position = self.folders.rows.iter().filter(|r| r.yours() && r.parent.as_deref() == parent).count() as i64;
        if let Some(r) = self.folders.rows.iter_mut().find(|r| r.id == folder) {
            r.parent = parent.map(String::from);
            r.position = position;
        }
        self.refiled();
    }

    pub(super) fn on_folder_delete(&mut self, folder: &str) {
        if !self.mine(folder) && !folder.starts_with("smart:") {
            return self.refuse("the built-in folders stay");
        }
        let parent = self.folders.rows.iter().find(|r| r.id == folder).and_then(|r| r.parent.clone());
        for r in self.folders.rows.iter_mut().filter(|r| r.parent.as_deref() == Some(folder)) {
            r.parent = parent.clone();
        }
        self.folders.rows.retain(|r| r.id != folder);
        self.folders.items.remove(folder);
        self.folders.sent.remove(folder);
        if self.folders.watched {
            self.send(Update::Folders { upsert: vec![], remove: vec![folder.to_string()], whole: false });
        }
        self.refiled();
    }

    pub(super) fn on_folder_add(&mut self, folder: &str, item: &str) {
        if !self.mine(folder) {
            return self.refuse("chats go only in your own folders");
        }
        let items = self.folders.items.entry(folder.to_string()).or_default();
        if !items.iter().any(|i| i == item) {
            items.push(item.to_string());
        }
        self.refiled();
    }

    pub(super) fn on_folder_remove(&mut self, folder: &str, item: &str) {
        if let Some(items) = self.folders.items.get_mut(folder) {
            items.retain(|i| i != item);
        }
        self.refiled();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Clock, Demo, State};
    use crate::data::{PresenceState, Update};
    use crate::run::Event;

    #[test]
    fn who_comes_and_goes_reaches_the_roster_watched() {
        let (tx, rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx, Clock::Fixed(1_792_159_320_000), State::Online, false);
        demo.lock().on_watch_roster("folder:1");
        demo.lock().presence_step(0);
        let rosters: Vec<_> = rx
            .try_iter()
            .filter_map(|e| match e {
                Event::Update(Update::Roster { upsert, .. }) => Some(upsert),
                _ => None,
            })
            .collect();
        let before = rosters.first().unwrap().iter().find(|r| r.id == "person:4").unwrap().presence.state;
        let after = rosters.last().unwrap().iter().find(|r| r.id == "person:4").unwrap().presence.state;
        assert_eq!((before, after), (PresenceState::Seen, PresenceState::Online), "Inês comes online");
    }

    #[test]
    fn a_folders_count_holds_each_chat_once_and_its_subfolders_too() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx, Clock::Fixed(1_792_159_320_000), State::Online, false);
        let a = demo.lock();
        let rows = a.folder_rows();
        let family = rows.iter().find(|r| r.id == "folder:1").unwrap();
        let cousins = rows.iter().find(|r| r.id == "folder:3").unwrap();
        assert_eq!((family.count, cousins.count), (5, 2), "Inês is in Family and Cousins, counted once in Family");
        assert!(family.unread >= cousins.unread);
    }
}
