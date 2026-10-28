//! Folders and favourites, as interfaces file chats and people, and presence and about texts, as
//! WhatsApp tells them. A step checks what it's asked against the tree as the writer's transaction
//! finds it, so two interfaces can't both make what only one may: a refusal is put in the step's
//! `done` with the protocol's error, and nothing is written. Folders' counts are migration 19's
//! triggers': a step changes only the rows they're derived from, and records an event for each
//! row an interface shows that it changed: the folder, and the chats and people whose `folders`
//! or favourite slot it changed.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use jaw_proto::code;
use rusqlite::{Connection, OptionalExtension, params};

use super::{PERSON, Stamps, chat_jid, touch_chat, touch_roster, user_jid};

/// The feed's event type for a folder row, for a favourite slot, keyed by the slot, and for
/// someone's presence, keyed by the person, which only rosters show.
pub(in crate::store) const FOLDER: &str = "folder";
pub(in crate::store) const FAVOURITE: &str = "favourite";
pub(in crate::store) const PRESENCE: &str = "presence";

/// Where a step says what it came to, for whoever asked. The last attempt at its transaction is
/// the one that stands.
#[derive(Clone, Debug, Default)]
pub struct Done(Arc<Mutex<Option<Outcome>>>);

impl Done {
    pub fn take(&self) -> Option<Outcome> {
        self.0.lock().expect("a folder step's outcome").take()
    }

    fn put(&self, outcome: Outcome) {
        *self.0.lock().expect("a folder step's outcome") = Some(outcome);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Done; for a folder made, its n.
    Done(Option<i64>),
    /// Not done, with the protocol's error code and a sentence an interface can show.
    Refused(i32, String),
}

/// Something filed: a chat by its JID, or a person by n. Filing someone's one-to-one chat files them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Chat(String),
    Person(i64),
}

/// What a favourite slot goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Folder(i64),
    Chat(String),
}

/// What WhatsApp says of someone's presence. Typing and recording aren't among it: who's typing is
/// never written to jaw.db.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Online,
    /// Away, and when they were last seen, unless they hide it.
    Away(Option<i64>),
}

#[derive(Clone, Debug)]
pub enum Step {
    /// A folder of yours under `parent` (yours, or the top), or with `rule` a smart one at the top,
    /// placed last among its siblings.
    Create {
        name: String,
        parent: Option<i64>,
        rule: Option<String>,
        done: Done,
    },
    Rename {
        folder: i64,
        name: String,
        done: Done,
    },
    /// A folder of yours into `parent` (yours, or the top) at `position` among its new siblings,
    /// last without one.
    Move {
        folder: i64,
        parent: Option<i64>,
        position: Option<u32>,
        done: Done,
    },
    /// A folder of yours or a smart one; yours moves its subfolders up to its parent, where it was,
    /// and unfiles what's in it, which stays wherever else it's filed.
    Delete {
        folder: i64,
        done: Done,
    },
    File {
        folder: i64,
        item: Item,
        done: Done,
    },
    Unfile {
        folder: i64,
        item: Item,
        done: Done,
    },
    /// A smart folder's rule, as JSON already checked.
    Rule {
        folder: i64,
        rule: String,
        done: Done,
    },
    /// Slot 1 to 9 to a folder or a chat, or emptied; a target already in another slot moves.
    Favourite {
        slot: u8,
        target: Option<Target>,
        done: Done,
    },
    /// Someone's presence, by any of their JIDs.
    Presence {
        jid: String,
        state: Presence,
    },
    /// Who jawd follows the presence of now: these people and nobody else.
    Following {
        people: Vec<i64>,
    },
    /// Nobody's presence can be known now, as when WhatsApp can't be reached or nobody's looking.
    Unknown,
    /// About texts asked of WhatsApp at `at`: each JID asked, and what it answered for those it did.
    About {
        asked: Vec<String>,
        about: Vec<(String, String)>,
        at: i64,
    },
    /// What folders keep their counts through, built again from what it's derived from, as the
    /// background check asks for when it finds it drifted.
    Rebuild,
}

pub(super) fn provenance(step: &Step) -> (&'static str, String, Option<&[u8]>) {
    let what = match step {
        Step::Create { name, .. } => format!("create {name}"),
        Step::Rename { folder, .. } => format!("rename {folder}"),
        Step::Move { folder, .. } => format!("move {folder}"),
        Step::Delete { folder, .. } => format!("delete {folder}"),
        Step::File { folder, item, .. } => format!("file {item:?} in {folder}"),
        Step::Unfile { folder, item, .. } => format!("unfile {item:?} from {folder}"),
        Step::Rule { folder, .. } => format!("rule {folder}"),
        Step::Favourite { slot, .. } => format!("favourite {slot}"),
        Step::Presence { jid, .. } => return ("presence", jid.clone(), None),
        Step::Following { .. } => return ("presence", "following".into(), None),
        Step::Unknown => return ("presence", "unknown".into(), None),
        Step::About { at, .. } => return ("about", at.to_string(), None),
        Step::Rebuild => "rebuild".into(),
    };
    ("folder", what, None)
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, step: &Step) -> Result<bool> {
    match step {
        Step::Create { name, parent, rule, done } => create(db, stamps, name, *parent, rule.as_deref(), done),
        Step::Rename { folder, name, done } => rename(db, stamps, *folder, name, done),
        Step::Move { folder, parent, position, done } => move_folder(db, stamps, *folder, *parent, *position, done),
        Step::Delete { folder, done } => delete(db, stamps, *folder, done),
        Step::File { folder, item, done } => file(db, stamps, *folder, item, true, done),
        Step::Unfile { folder, item, done } => file(db, stamps, *folder, item, false, done),
        Step::Rule { folder, rule, done } => set_rule(db, stamps, *folder, rule, done),
        Step::Favourite { slot, target, done } => favourite(db, stamps, *slot, target.as_ref(), done),
        Step::Presence { jid, state } => presence(db, stamps, jid, *state),
        Step::Following { people } => following(db, stamps, people),
        Step::Unknown => unknown(db, stamps),
        Step::About { asked, about, at } => about_texts(db, stamps, asked, about, *at),
        Step::Rebuild => rebuild(db, stamps),
    }
}

/// Builds folder_paths, folder_chats, folder_people, folder_counts and people.direct_chats again
/// from the tree, what's filed and the chats, through the triggers that keep them: emptied, then
/// each folder's places in the tree written again, which brings what's filed under them.
fn rebuild(db: &Connection, stamps: &mut Stamps) -> Result<bool> {
    for sql in [
        "DELETE FROM folder_chats",
        "DELETE FROM folder_people",
        "DELETE FROM folder_paths",
        "DELETE FROM folder_counts WHERE folder IN (SELECT n FROM folders WHERE rule IS NOT NULL)",
        "INSERT OR IGNORE INTO folder_counts (folder) SELECT n FROM folders WHERE rule IS NULL",
        "UPDATE folder_counts SET unread = 0, mentions = 0, chats = 0, lone = 0",
        "UPDATE people SET direct_chats = (SELECT count(*) FROM chats c WHERE c.person = people.n)
            WHERE direct_chats IS NOT (SELECT count(*) FROM chats c WHERE c.person = people.n)",
        "WITH RECURSIVE up (folder, ancestor) AS (
            SELECT n, n FROM folders WHERE builtin IS NULL AND rule IS NULL
            UNION SELECT up.folder, f.parent FROM up JOIN folders f ON f.n = up.ancestor WHERE f.parent IS NOT NULL)
        INSERT INTO folder_paths (folder, ancestor) SELECT folder, ancestor FROM up",
        "UPDATE folder_counts SET (chats, unread, mentions) = (SELECT count(c.n), coalesce(sum(c.unread), 0),
            coalesce(sum(c.mentions), 0) FROM folders f, chats c WHERE f.n = folder_counts.folder AND CASE f.builtin
                WHEN 'all' THEN c.archived = 0
                WHEN 'unread' THEN c.archived = 0 AND (c.unread > 0 OR c.marked_unread <> 0)
                WHEN 'groups' THEN c.archived = 0 AND c.kind IN ('group', 'announcement')
                WHEN 'communities' THEN c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL)
                ELSE c.archived <> 0 END)
        WHERE folder IN (SELECT n FROM folders WHERE builtin IS NOT NULL)",
    ] {
        db.prepare_cached(sql)?.execute([])?;
    }
    let folders: Vec<i64> = db
        .prepare_cached("SELECT n AS rebuilt FROM folders")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for n in folders {
        touch_folder(db, stamps, n)?;
    }
    Ok(true)
}

/// A folder as the tree has it.
struct Folder {
    builtin: bool,
    smart: bool,
    parent: Option<i64>,
    name: String,
}

impl Folder {
    fn yours(&self) -> bool {
        !self.builtin && !self.smart
    }
}

fn folder(db: &Connection, n: i64) -> Result<Option<Folder>> {
    Ok(db
        .prepare_cached("SELECT builtin IS NOT NULL, rule IS NOT NULL, parent, name FROM folders WHERE n = ?1")?
        .query_row([n], |r| Ok(Folder { builtin: r.get(0)?, smart: r.get(1)?, parent: r.get(2)?, name: r.get(3)? }))
        .optional()?)
}

fn refuse(done: &Done, code: i32, message: impl Into<String>) -> Result<bool> {
    done.put(Outcome::Refused(code, message.into()));
    Ok(false)
}

fn no_folder(done: &Done, n: i64) -> Result<bool> {
    refuse(done, code::NOT_FOUND, format!("jawd has no folder {n}"))
}

/// Whether a folder of yours under `parent` other than `but` is called `name`, whatever its case.
fn named(db: &Connection, parent: Option<i64>, name: &str, but: Option<i64>) -> Result<bool> {
    Ok(db
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM folders WHERE coalesce(parent, 0) = coalesce(?1, 0) AND lower(name) = lower(?2)
                AND builtin IS NULL AND rule IS NULL AND n IS NOT ?3)",
        )?
        .query_row(params![parent, name, but], |r| r.get(0))?)
}

/// Your folders under `parent`, or the smart ones, in their order, `but` left out. Siblings are a
/// handful, put in order here.
fn siblings(db: &Connection, parent: Option<i64>, smart: bool, but: i64) -> Result<Vec<i64>> {
    let mut rows: Vec<(i64, i64)> = if smart {
        db.prepare_cached("SELECT position, n FROM folders WHERE rule IS NOT NULL AND n <> ?1")?
            .query_map([but], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    } else {
        db.prepare_cached(
            "SELECT position, n FROM folders WHERE parent IS ?1 AND builtin IS NULL AND rule IS NULL AND n <> ?2",
        )?
        .query_map(params![parent, but], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?
    };
    rows.sort_unstable();
    Ok(rows.into_iter().map(|(_, n)| n).collect())
}

/// Gives each of `order` its place, under `parent`, writing those whose place or parent changed.
fn place(db: &Connection, stamps: &mut Stamps, order: &[i64], parent: Option<i64>) -> Result<()> {
    for (position, &n) in order.iter().enumerate() {
        let seq = stamps.seq(FOLDER, Some(n));
        let moved = db
            .prepare_cached(
                "UPDATE folders SET parent = ?2, position = ?3, v = ?4
                WHERE n = ?1 AND (parent IS NOT ?2 OR position IS NOT ?3)",
            )?
            .execute(params![n, parent, position as i64, seq])?;
        if moved > 0 {
            stamps.record(db, FOLDER, n, None, seq)?;
        }
    }
    Ok(())
}

/// Bumps folder `n`'s version, as what it holds changed.
fn touch_folder(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<()> {
    let seq = stamps.seq(FOLDER, Some(n));
    db.prepare_cached("UPDATE folders SET v = ?2 WHERE n = ?1")?.execute(params![n, seq])?;
    stamps.record(db, FOLDER, n, None, seq)
}

/// What filing or unfiling `item` changes the rows of: the chat filed, or the person and their
/// one-to-one chats, whose `folders` say where they're filed.
fn touch_item(db: &Connection, stamps: &mut Stamps, chat: Option<i64>, person: Option<i64>) -> Result<()> {
    if let Some(chat) = chat {
        touch_chat(db, stamps, chat)?;
    }
    if let Some(person) = person {
        touch_roster(db, stamps, person)?;
        let chats: Vec<i64> = db
            .prepare_cached("SELECT n FROM chats WHERE person = ?1")?
            .query_map([person], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for chat in chats {
            touch_chat(db, stamps, chat)?;
        }
    }
    Ok(())
}

fn create(
    db: &Connection,
    stamps: &mut Stamps,
    name: &str,
    parent: Option<i64>,
    rule: Option<&str>,
    done: &Done,
) -> Result<bool> {
    let name = name.trim();
    if let Some(parent) = parent {
        match folder(db, parent)? {
            None => return no_folder(done, parent),
            Some(f) if !f.yours() => return refuse(done, code::NOT_ALLOWED, "folders go only in folders of yours"),
            Some(_) if rule.is_some() => return refuse(done, code::NOT_ALLOWED, "smart folders sit at the top"),
            Some(_) => {}
        }
    }
    if rule.is_none() && named(db, parent, name, None)? {
        return refuse(done, code::NOT_ALLOWED, format!("a folder there is called {name} already"));
    }
    let position = siblings(db, parent, rule.is_some(), 0)?.len() as i64;
    let seq = stamps.seq(FOLDER, None);
    let n: i64 = db
        .prepare_cached(
            "INSERT INTO folders (parent, name, position, rule, v) VALUES (?1, ?2, ?3, ?4, ?5) RETURNING n",
        )?
        .query_row(params![parent, name, position, rule, seq], |r| r.get(0))?;
    stamps.record(db, FOLDER, n, None, seq)?;
    done.put(Outcome::Done(Some(n)));
    Ok(true)
}

fn rename(db: &Connection, stamps: &mut Stamps, n: i64, name: &str, done: &Done) -> Result<bool> {
    let name = name.trim();
    let Some(f) = folder(db, n)? else { return no_folder(done, n) };
    if f.builtin {
        return refuse(done, code::NOT_ALLOWED, "the built-in folders keep their names");
    }
    if f.name == name {
        done.put(Outcome::Done(None));
        return Ok(false);
    }
    if f.yours() && named(db, f.parent, name, Some(n))? {
        return refuse(done, code::NOT_ALLOWED, format!("a folder beside it is called {name} already"));
    }
    let seq = stamps.seq(FOLDER, Some(n));
    db.prepare_cached("UPDATE folders SET name = ?2, v = ?3 WHERE n = ?1")?.execute(params![n, name, seq])?;
    stamps.record(db, FOLDER, n, None, seq)?;
    done.put(Outcome::Done(None));
    Ok(true)
}

fn move_folder(
    db: &Connection,
    stamps: &mut Stamps,
    n: i64,
    parent: Option<i64>,
    position: Option<u32>,
    done: &Done,
) -> Result<bool> {
    let Some(f) = folder(db, n)? else { return no_folder(done, n) };
    if !f.yours() {
        return refuse(done, code::NOT_ALLOWED, "only folders of yours move");
    }
    if let Some(parent) = parent {
        match folder(db, parent)? {
            None => return no_folder(done, parent),
            Some(p) if !p.yours() => return refuse(done, code::NOT_ALLOWED, "folders go only in folders of yours"),
            Some(_) => {}
        }
        let under: bool = db
            .prepare_cached("SELECT EXISTS (SELECT 1 FROM folder_paths WHERE folder = ?1 AND ancestor = ?2)")?
            .query_row([parent, n], |r| r.get(0))?;
        if under {
            return refuse(done, code::NOT_ALLOWED, "a folder can't go into itself or a folder under it");
        }
    }
    if parent != f.parent && named(db, parent, &f.name, Some(n))? {
        return refuse(done, code::NOT_ALLOWED, format!("a folder there is called {} already", f.name));
    }
    let mut order = siblings(db, parent, false, n)?;
    let at = position.map_or(order.len(), |p| (p as usize).min(order.len()));
    order.insert(at, n);
    let changed = db.total_changes();
    if parent != f.parent {
        let left = siblings(db, f.parent, false, n)?;
        place(db, stamps, &left, f.parent)?;
    }
    place(db, stamps, &order, parent)?;
    done.put(Outcome::Done(None));
    Ok(db.total_changes() > changed)
}

fn delete(db: &Connection, stamps: &mut Stamps, n: i64, done: &Done) -> Result<bool> {
    let Some(f) = folder(db, n)? else { return no_folder(done, n) };
    if f.builtin {
        return refuse(done, code::NOT_ALLOWED, "the built-in folders stay");
    }
    let mut children: Vec<(i64, i64, String)> = db
        .prepare_cached("SELECT position, n, name FROM folders WHERE parent = ?1")?
        .query_map([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    children.sort_unstable();
    let children: Vec<(i64, String)> = children.into_iter().map(|(_, n, name)| (n, name)).collect();
    for (child, name) in &children {
        let beside: bool = db
            .prepare_cached(
                "SELECT EXISTS (SELECT 1 FROM folders WHERE coalesce(parent, 0) = coalesce(?1, 0) AND lower(name) = lower(?2)
                    AND builtin IS NULL AND rule IS NULL AND n NOT IN (?3, ?4))",
            )?
            .query_row(params![f.parent, name, child, n], |r| r.get(0))?;
        if beside {
            return refuse(
                done,
                code::NOT_ALLOWED,
                format!("{name}, in it, would sit beside a folder of the same name: rename one first"),
            );
        }
    }
    let items: Vec<(Option<i64>, Option<i64>)> = db
        .prepare_cached("SELECT chat, person FROM folder_items WHERE folder = ?1")?
        .query_map([n], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    db.prepare_cached("DELETE FROM folder_items WHERE folder = ?1")?.execute([n])?;
    for (chat, person) in items {
        touch_item(db, stamps, chat, person)?;
    }
    let slots: Vec<i64> = db
        .prepare_cached("SELECT slot FROM favourites WHERE folder = ?1")?
        .query_map([n], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for slot in slots {
        let seq = stamps.seq(FAVOURITE, Some(slot));
        stamps.record(db, FAVOURITE, slot, None, seq)?;
    }
    let (order, parent) = if f.yours() {
        // Its subfolders take its place, in their order. It steps out of the way of a subfolder of
        // its own name first; nothing sees it so, since it goes in the same transaction.
        db.prepare_cached("UPDATE folders SET name = ?2 WHERE n = ?1")?
            .execute(params![n, format!("\u{1}going {n}")])?;
        let at = siblings_before(db, f.parent, n)?;
        let mut order = siblings(db, f.parent, false, n)?;
        let at = at.min(order.len());
        for (i, (child, _)) in children.iter().enumerate() {
            order.insert(at + i, *child);
        }
        (order, f.parent)
    } else {
        (siblings(db, None, true, n)?, None)
    };
    if f.yours() {
        place(db, stamps, &order, parent)?;
    }
    db.prepare_cached("DELETE FROM folders WHERE n = ?1")?.execute([n])?;
    if !f.yours() {
        place(db, stamps, &order, parent)?;
    }
    let seq = stamps.seq(FOLDER, Some(n));
    stamps.record(db, FOLDER, n, None, seq)?;
    done.put(Outcome::Done(None));
    Ok(true)
}

/// How many of your folders under `parent` come before folder `n`.
fn siblings_before(db: &Connection, parent: Option<i64>, n: i64) -> Result<usize> {
    let position: i64 = db.prepare_cached("SELECT position FROM folders WHERE n = ?1")?.query_row([n], |r| r.get(0))?;
    let before: i64 = db
        .prepare_cached(
            "SELECT count(*) FROM folders WHERE parent IS ?1 AND builtin IS NULL AND rule IS NULL AND n <> ?2
                AND (position < ?3 OR position = ?3 AND n < ?2)",
        )?
        .query_row(params![parent, n, position], |r| r.get(0))?;
    Ok(before.max(0) as usize)
}

/// The chat or person an item files, by n: someone's one-to-one chat files them.
fn item_of(db: &Connection, item: &Item) -> Result<Option<(Option<i64>, Option<i64>)>> {
    match item {
        Item::Chat(jid) => {
            let found: Option<(i64, Option<i64>)> = db
                .prepare_cached("SELECT n, person FROM chats WHERE id = ?1")?
                .query_row([chat_jid(db, jid)?], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            Ok(found.map(|(n, person)| match person {
                Some(person) => (None, Some(person)),
                None => (Some(n), None),
            }))
        }
        Item::Person(n) => {
            let found = db.prepare_cached("SELECT 1 FROM people WHERE n = ?1")?.exists([n])?;
            Ok(found.then_some((None, Some(*n))))
        }
    }
}

fn file(db: &Connection, stamps: &mut Stamps, n: i64, item: &Item, filing: bool, done: &Done) -> Result<bool> {
    let Some(f) = folder(db, n)? else { return no_folder(done, n) };
    if !f.yours() {
        let why = if f.smart {
            "a smart folder holds what its rule finds"
        } else {
            "the built-in folders hold what they find"
        };
        return refuse(done, code::NOT_ALLOWED, format!("{why}, so nothing is filed in it"));
    }
    let Some((chat, person)) = item_of(db, item)? else {
        let what = match item {
            Item::Chat(jid) => format!("chat {jid}"),
            Item::Person(n) => format!("person:{n}"),
        };
        return refuse(done, code::NOT_FOUND, format!("jawd has no {what}"));
    };
    let changed = if filing {
        db.prepare_cached("INSERT OR IGNORE INTO folder_items (folder, chat, person) VALUES (?1, ?2, ?3)")?
            .execute(params![n, chat, person])?
    } else {
        db.prepare_cached("DELETE FROM folder_items WHERE folder = ?1 AND chat IS ?2 AND person IS ?3")?
            .execute(params![n, chat, person])?
    };
    done.put(Outcome::Done(None));
    if changed == 0 {
        return Ok(false);
    }
    touch_item(db, stamps, chat, person)?;
    touch_folder(db, stamps, n)?;
    Ok(true)
}

fn set_rule(db: &Connection, stamps: &mut Stamps, n: i64, rule: &str, done: &Done) -> Result<bool> {
    let Some(f) = folder(db, n)? else { return no_folder(done, n) };
    if !f.smart {
        return refuse(done, code::NOT_ALLOWED, "only a smart folder has a rule; make a smart folder for this one");
    }
    let seq = stamps.seq(FOLDER, Some(n));
    let changed = db
        .prepare_cached("UPDATE folders SET rule = ?2, v = ?3 WHERE n = ?1 AND rule IS NOT ?2")?
        .execute(params![n, rule, seq])?;
    if changed > 0 {
        stamps.record(db, FOLDER, n, None, seq)?;
    }
    done.put(Outcome::Done(None));
    Ok(changed > 0)
}

fn favourite(db: &Connection, stamps: &mut Stamps, slot: u8, target: Option<&Target>, done: &Done) -> Result<bool> {
    let (folder_n, chat_n) = match target {
        None => (None, None),
        Some(Target::Folder(n)) => match folder(db, *n)? {
            Some(_) => (Some(*n), None),
            None => return no_folder(done, *n),
        },
        Some(Target::Chat(jid)) => {
            let chat: Option<i64> = db
                .prepare_cached("SELECT n FROM chats WHERE id = ?1")?
                .query_row([chat_jid(db, jid)?], |r| r.get(0))
                .optional()?;
            match chat {
                Some(n) => (None, Some(n)),
                None => return refuse(done, code::NOT_FOUND, format!("jawd has no chat {jid}")),
            }
        }
    };
    let slot = i64::from(slot);
    let was: Vec<(i64, Option<i64>, Option<i64>)> = db
        .prepare_cached("SELECT slot, folder, chat FROM favourites WHERE slot = ?1 OR folder = ?2 OR chat = ?3")?
        .query_map(params![slot, folder_n, chat_n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if was.len() == 1 && was[0] == (slot, folder_n, chat_n) || was.is_empty() && target.is_none() {
        done.put(Outcome::Done(None));
        return Ok(false);
    }
    db.prepare_cached("DELETE FROM favourites WHERE slot = ?1 OR folder = ?2 OR chat = ?3")?
        .execute(params![slot, folder_n, chat_n])?;
    if target.is_some() {
        db.prepare_cached("INSERT INTO favourites (slot, folder, chat) VALUES (?1, ?2, ?3)")?
            .execute(params![slot, folder_n, chat_n])?;
    }
    let mut slots: HashSet<i64> = was.iter().map(|w| w.0).collect();
    slots.insert(slot);
    let folders: HashSet<i64> = was.iter().filter_map(|w| w.1).chain(folder_n).collect();
    let chats: HashSet<i64> = was.iter().filter_map(|w| w.2).chain(chat_n).collect();
    for slot in slots {
        stamps.record(db, FAVOURITE, slot, None, stamps.seq(FAVOURITE, Some(slot)))?;
    }
    for n in folders {
        touch_folder(db, stamps, n)?;
    }
    for n in chats {
        touch_chat(db, stamps, n)?;
    }
    done.put(Outcome::Done(None));
    Ok(true)
}

fn presence(db: &Connection, stamps: &mut Stamps, jid: &str, state: Presence) -> Result<bool> {
    let person: Option<(i64, Option<i64>)> = db
        .prepare_cached("SELECT p.n, p.last_seen FROM person_jids j JOIN people p ON p.n = j.person WHERE j.jid = ?1")?
        .query_row([user_jid(jid)], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((n, seen)) = person else { return Ok(false) };
    let (now, seen) = match state {
        Presence::Online => (Some("online"), seen),
        Presence::Away(at) => (None, at.filter(|&at| at > 0)),
    };
    let seq = stamps.seq(PRESENCE, Some(n));
    let changed = db
        .prepare_cached(
            "UPDATE people SET presence = ?2, last_seen = ?3, roster_v = max(roster_v, ?4)
            WHERE n = ?1 AND (presence IS NOT ?2 OR last_seen IS NOT ?3)",
        )?
        .execute(params![n, now, seen, seq])?;
    if changed > 0 {
        stamps.record(db, PRESENCE, n, None, seq)?;
    }
    Ok(changed > 0)
}

/// Those followed now, or around: everyone whose presence may show.
fn present(db: &Connection) -> Result<Vec<(i64, bool)>> {
    Ok(db
        .prepare_cached("SELECT n, followed FROM people WHERE presence IS NOT NULL OR followed <> 0")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

fn following(db: &Connection, stamps: &mut Stamps, people: &[i64]) -> Result<bool> {
    let wanted: HashSet<i64> = people.iter().copied().collect();
    let mut changed = false;
    for (n, followed) in present(db)? {
        if followed && !wanted.contains(&n) {
            let seq = stamps.seq(PRESENCE, Some(n));
            db.prepare_cached("UPDATE people SET followed = 0, roster_v = max(roster_v, ?2) WHERE n = ?1")?
                .execute(params![n, seq])?;
            stamps.record(db, PRESENCE, n, None, seq)?;
            changed = true;
        }
    }
    for &n in &wanted {
        let seq = stamps.seq(PRESENCE, Some(n));
        let now = db
            .prepare_cached(
                "UPDATE people SET followed = 1, roster_v = max(roster_v, ?2) WHERE n = ?1 AND followed = 0",
            )?
            .execute(params![n, seq])?;
        if now > 0 {
            stamps.record(db, PRESENCE, n, None, seq)?;
            changed = true;
        }
    }
    Ok(changed)
}

fn unknown(db: &Connection, stamps: &mut Stamps) -> Result<bool> {
    let present = present(db)?;
    for &(n, _) in &present {
        let seq = stamps.seq(PRESENCE, Some(n));
        db.prepare_cached(
            "UPDATE people SET presence = NULL, followed = 0, roster_v = max(roster_v, ?2) WHERE n = ?1",
        )?
        .execute(params![n, seq])?;
        stamps.record(db, PRESENCE, n, None, seq)?;
    }
    Ok(!present.is_empty())
}

fn about_texts(
    db: &Connection,
    stamps: &mut Stamps,
    asked: &[String],
    about: &[(String, String)],
    at: i64,
) -> Result<bool> {
    let person = |jid: &str| -> Result<Option<i64>> {
        Ok(db
            .prepare_cached("SELECT person FROM person_jids WHERE jid = ?1")?
            .query_row([user_jid(jid)], |r| r.get(0))
            .optional()?)
    };
    for jid in asked {
        if let Some(n) = person(jid)? {
            db.prepare_cached("UPDATE people SET about_at = ?2 WHERE n = ?1 AND (about_at IS NULL OR about_at <= ?2)")?
                .execute(params![n, at])?;
        }
    }
    let mut changed = false;
    for (jid, text) in about {
        let Some(n) = person(jid)? else { continue };
        let seq = stamps.seq(PERSON, Some(n));
        let now = db
            .prepare_cached(
                "UPDATE people SET about = ?2, roster_v = max(roster_v, ?3)
                WHERE n = ?1 AND about IS NOT ?2 AND about_at = ?4",
            )?
            .execute(params![n, text, seq, at])?;
        if now > 0 {
            stamps.record(db, PERSON, n, None, seq)?;
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
