//! The name index: every chat's, person's and folder's names in memory, with each chat's frecency,
//! loaded at start and kept current from the feed's chat and person events. Names are matched
//! fuzzily by nucleo's matcher, fzf's scoring: word starts and runs score above scattered letters,
//! so "ar" finds Ana Ribeiro first. Case and accents are ignored on both sides, and a match too
//! scattered to be initials or parts of words isn't one.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;
use jaw_proto::find::{Field, Kind};
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use super::Cancel;
use crate::store::frecency;

/// A name to match, as it's shown and as the matcher reads it: one char a position, so the
/// matcher's positions are chars of `text`.
struct Name {
    field: Field,
    text: String,
    hay: Utf32String,
}

impl Name {
    fn new(field: Field, text: &str) -> Name {
        let hay = if text.is_ascii() {
            Utf32String::Ascii(text.into())
        } else {
            Utf32String::Unicode(text.chars().collect())
        };
        Name { field, text: text.to_string(), hay }
    }

    fn bytes(&self) -> usize {
        let hay = match &self.hay {
            Utf32String::Ascii(text) => text.len(),
            Utf32String::Unicode(chars) => chars.len() * 4,
        };
        std::mem::size_of::<Name>() + self.text.capacity() + hay
    }
}

struct ChatEntry {
    id: String,
    name: Option<String>,
    person: Option<i64>,
    folders: Vec<i64>,
    frecency: f64,
    opened_at: Option<i64>,
    last_at: i64,
    /// Its own name, or for a one-to-one chat its person's names and numbers.
    names: Vec<Name>,
}

struct PersonEntry {
    name: Option<String>,
    push_name: Option<String>,
    verified_name: Option<String>,
    phones: Vec<String>,
    folders: Vec<i64>,
    names: Vec<Name>,
}

impl PersonEntry {
    fn names(&self) -> Vec<Name> {
        let mut names = Vec::new();
        for (field, text) in
            [(Field::Name, &self.name), (Field::PushName, &self.push_name), (Field::VerifiedName, &self.verified_name)]
        {
            if let Some(text) = text.as_deref().filter(|text| !text.trim().is_empty()) {
                names.push(Name::new(field, text));
            }
        }
        names.extend(self.phones.iter().map(|phone| Name::new(Field::Phone, phone)));
        names
    }
}

struct FolderEntry {
    id: String,
    parent: Option<String>,
    name: Name,
}

/// A folder as jaw.db has it: its n, its built-in name, whether it's smart, its parent and name.
type FolderRecord = (i64, Option<String>, bool, Option<i64>, String);

const FOLDERS: &str = "SELECT n, builtin, rule IS NOT NULL, parent, name FROM folders";

fn folders(db: &Connection) -> Result<Vec<FolderRecord>> {
    Ok(db
        .prepare_cached(FOLDERS)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

#[derive(Default)]
pub struct Index {
    chats: HashMap<i64, ChatEntry>,
    by_id: HashMap<String, i64>,
    people: HashMap<i64, PersonEntry>,
    /// Each person's one-to-one chat, which stands for them in what's found.
    chat_of: HashMap<i64, i64>,
    folders: Vec<FolderEntry>,
    folder_names: HashMap<i64, String>,
}

/// A chat's or person's row as jaw.db has it now, or `None` once it's gone.
pub struct Changes {
    chats: Vec<(i64, Option<ChatRecord>)>,
    people: Vec<(i64, Option<PersonRecord>)>,
    /// The whole tree, when a folder changed.
    folders: Option<Vec<FolderRecord>>,
}

struct ChatRecord {
    id: String,
    name: Option<String>,
    person: Option<i64>,
    frecency: f64,
    opened_at: Option<i64>,
    last_at: i64,
    folders: Vec<i64>,
}

struct PersonRecord {
    name: Option<String>,
    push_name: Option<String>,
    verified_name: Option<String>,
    phones: Vec<String>,
    folders: Vec<i64>,
}

const CHAT: &str = "SELECT n, id, name, person, frecency, opened_at, last_at FROM chats";
const PERSON: &str = "SELECT n, name, push_name, verified_name FROM people";

fn chat_record(r: &rusqlite::Row) -> rusqlite::Result<(i64, ChatRecord)> {
    Ok((
        r.get(0)?,
        ChatRecord {
            id: r.get(1)?,
            name: r.get(2)?,
            person: r.get(3)?,
            frecency: r.get(4)?,
            opened_at: r.get(5)?,
            last_at: r.get(6)?,
            folders: Vec::new(),
        },
    ))
}

fn person_record(r: &rusqlite::Row) -> rusqlite::Result<(i64, PersonRecord)> {
    Ok((
        r.get(0)?,
        PersonRecord {
            name: r.get(1)?,
            push_name: r.get(2)?,
            verified_name: r.get(3)?,
            phones: Vec::new(),
            folders: Vec::new(),
        },
    ))
}

/// A phone number's JID as the number to show.
fn phone(jid: &str) -> Option<String> {
    jid.strip_suffix("@s.whatsapp.net").map(|user| format!("+{user}"))
}

/// Every chat, person and folder in jaw.db, read in one snapshot, and the feed's seq it was read at.
pub fn load(db: &Connection) -> Result<(Index, i64)> {
    let tx = db.unchecked_transaction()?;
    let seq: i64 = tx.query_row("SELECT coalesce(max(seq), 0) FROM events", [], |r| r.get(0))?;
    let mut people: HashMap<i64, PersonRecord> =
        tx.prepare(PERSON)?.query_map([], person_record)?.collect::<rusqlite::Result<_>>()?;
    let mut chats: HashMap<i64, ChatRecord> =
        tx.prepare(CHAT)?.query_map([], chat_record)?.collect::<rusqlite::Result<_>>()?;
    let mut jids = tx.prepare("SELECT person, jid FROM person_jids WHERE jid LIKE '%@s.whatsapp.net'")?;
    for row in jids.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (person, jid) = row?;
        if let (Some(person), Some(phone)) = (people.get_mut(&person), phone(&jid)) {
            person.phones.push(phone);
        }
    }
    let mut items = tx.prepare("SELECT folder, chat, person FROM folder_items")?;
    for row in items.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get(2)?)))? {
        match row? {
            (folder, Some(chat), _) => chats.get_mut(&chat).into_iter().for_each(|chat| chat.folders.push(folder)),
            (folder, _, Some(person)) => {
                people.get_mut(&person).into_iter().for_each(|person| person.folders.push(folder))
            }
            _ => {}
        }
    }
    // Someone's one-to-one chat is filed where they are.
    for chat in chats.values_mut() {
        if let Some(person) = chat.person.and_then(|n| people.get(&n)) {
            chat.folders.extend(person.folders.iter().copied());
        }
    }
    let folders = folders(&tx)?;
    let mut index = Index::default();
    index.apply(Changes {
        people: people.into_iter().map(|(n, person)| (n, Some(person))).collect(),
        chats: chats.into_iter().map(|(n, chat)| (n, Some(chat))).collect(),
        folders: Some(folders),
    });
    Ok((index, seq))
}

/// The chats and people whose events came after `since`, up to `upto`, as they are now.
pub fn changes(db: &Connection, since: i64, upto: i64) -> Result<Changes> {
    let tx = db.unchecked_transaction()?;
    let mut chats = HashSet::new();
    let mut people = HashSet::new();
    let mut events = tx.prepare_cached(
        "SELECT type, key FROM events WHERE seq > ?1 AND seq <= ?2 AND type IN ('chat', 'person', 'folder') ORDER BY seq",
    )?;
    let mut tree = false;
    for row in events.query_map(params![since, upto], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        match row? {
            (kind, key) if kind == "chat" => chats.insert(key),
            (kind, key) if kind == "person" => people.insert(key),
            _ => {
                tree = true;
                false
            }
        };
    }
    let mut changes =
        Changes { chats: Vec::new(), people: Vec::new(), folders: tree.then(|| folders(&tx)).transpose()? };
    for n in chats {
        let chat = tx.prepare_cached(&format!("{CHAT} WHERE n = ?1"))?.query_row([n], chat_record).optional()?;
        let chat = match chat {
            Some((_, mut chat)) => {
                chat.folders = tx
                    .prepare_cached(
                        "SELECT folder FROM folder_items WHERE chat = ?1
                        UNION ALL SELECT folder FROM folder_items WHERE person = ?2",
                    )?
                    .query_map(params![n, chat.person], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                Some(chat)
            }
            None => None,
        };
        changes.chats.push((n, chat));
    }
    for n in people {
        let person = tx.prepare_cached(&format!("{PERSON} WHERE n = ?1"))?.query_row([n], person_record).optional()?;
        let person = match person {
            Some((_, mut person)) => {
                let jids: Vec<String> = tx
                    .prepare_cached("SELECT jid FROM person_jids WHERE person = ?1 ORDER BY n")?
                    .query_map([n], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                person.phones = jids.iter().filter_map(|jid| phone(jid)).collect();
                person.folders = tx
                    .prepare_cached("SELECT folder FROM folder_items WHERE person = ?1")?
                    .query_map([n], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                Some(person)
            }
            None => None,
        };
        changes.people.push((n, person));
    }
    Ok(changes)
}

/// What's typed, ready to match: each word fuzzily, and, when it's a number, a run of its digits.
pub struct Query {
    words: Pattern,
    digits: Option<Atom>,
    /// The least score a name needs to count as a match.
    least: u32,
}

/// What each matched char scores before bonuses and gaps, in nucleo's scoring.
const SCORE_MATCH: u32 = 16;

impl Query {
    pub fn new(typed: &str) -> Query {
        let folded: String = typed.chars().map(nucleo_matcher::chars::normalize).collect();
        let words = Pattern::new(&folded, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);
        let letters = folded.chars().filter(|c| !c.is_whitespace()).count() as u32;
        let numeric = typed.chars().all(|c| c.is_ascii_digit() || " +-().".contains(c));
        let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
        let digits = (numeric && digits.len() >= 3)
            .then(|| Atom::new(&digits, CaseMatching::Ignore, Normalization::Never, AtomKind::Substring, false));
        Query { words, digits, least: letters * SCORE_MATCH }
    }

    /// How well `name` matches, if it does.
    fn score(&self, name: &Name, matcher: &mut Matcher) -> Option<u32> {
        match name.field {
            Field::Phone => self.digits.as_ref()?.score(name.hay.slice(..), matcher).map(u32::from),
            _ => self.words.score(name.hay.slice(..), matcher).filter(|&score| score >= self.least),
        }
    }

    /// Where `name` matched, as byte ranges of its text.
    fn marks(&self, name: &Name, matcher: &mut Matcher) -> Vec<(usize, usize)> {
        let mut at = Vec::new();
        match name.field {
            Field::Phone => {
                if let Some(digits) = &self.digits {
                    digits.indices(name.hay.slice(..), matcher, &mut at);
                }
            }
            _ => {
                self.words.indices(name.hay.slice(..), matcher, &mut at);
            }
        }
        at.sort_unstable();
        at.dedup();
        let bytes: Vec<(usize, usize)> =
            name.text.char_indices().map(|(start, c)| (start, start + c.len_utf8())).collect();
        let mut marks: Vec<(usize, usize)> = Vec::new();
        for (start, end) in at.into_iter().filter_map(|n| bytes.get(n as usize).copied()) {
            match marks.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => marks.push((start, end)),
            }
        }
        marks
    }
}

thread_local! {
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new(Config::DEFAULT));
}

/// What a name found, ready to be made a result.
pub struct NameHit {
    pub kind: Kind,
    pub score: u32,
    pub text: String,
    pub marks: Vec<(usize, usize)>,
    pub field: Option<Field>,
    pub folders: Vec<String>,
    /// For a chat, its n, whose row comes from jaw.db.
    pub chat: Option<i64>,
    /// For a person or a folder, its row.
    pub row: Option<Value>,
}

#[derive(Clone, Copy)]
enum Key {
    Chat(i64),
    Person(i64),
    Folder(usize),
}

struct Candidate {
    key: Key,
    name: usize,
    score: u32,
    frecency: f64,
    last_at: i64,
    length: usize,
}

/// How much frecency can raise a name's score: by half, approached as opens pile up.
const FRECENCY_WEIGHT: f64 = 0.5;
/// The opens at which frecency gives half its weight.
const FRECENCY_HALF: f64 = 2.0;

/// A name's score raised by its chat's frecency, so a chat opened often ranks above one that
/// matched as well, but never above a match that's much better.
fn blend(score: u32, frecency: f64) -> u32 {
    (score as f64 * (1.0 + FRECENCY_WEIGHT * frecency / (frecency + FRECENCY_HALF))).round() as u32
}

/// How often matching checks whether its find was cancelled, in chats or people.
const CHECK_EVERY: usize = 512;

/// The best of `names` for `query`, and its score; the earlier name wins a tie.
fn best(query: &Query, names: &[Name], matcher: &mut Matcher) -> Option<(usize, u32)> {
    names
        .iter()
        .enumerate()
        .filter_map(|(at, name)| Some((at, query.score(name, matcher)?)))
        .max_by_key(|&(at, score)| (score, std::cmp::Reverse(at)))
}

impl Index {
    fn set_folders(&mut self, mut folders: Vec<FolderRecord>) {
        // The built-in folders first, as the tree has them.
        folders.sort_by_key(|(n, builtin, ..)| (builtin.is_none(), *n));
        self.folders.clear();
        self.folder_names.clear();
        for (n, builtin, smart, parent, name) in folders {
            self.folders.push(FolderEntry {
                id: crate::store::lists::folder_id(n, builtin.as_deref(), smart),
                parent: parent.map(|parent| format!("folder:{parent}")),
                name: Name::new(Field::Name, &name),
            });
            if builtin.is_none() && !smart {
                self.folder_names.insert(n, name);
            }
        }
    }

    /// Takes in rows as they are now: people first, since a one-to-one chat is matched by its
    /// person's names.
    pub fn apply(&mut self, changes: Changes) {
        if let Some(folders) = changes.folders {
            self.set_folders(folders);
        }
        let mut touched: HashSet<i64> = HashSet::new();
        for (n, person) in changes.people {
            match person {
                Some(person) => {
                    let mut entry = PersonEntry {
                        name: person.name,
                        push_name: person.push_name,
                        verified_name: person.verified_name,
                        phones: person.phones,
                        folders: person.folders,
                        names: Vec::new(),
                    };
                    entry.names = entry.names();
                    self.people.insert(n, entry);
                }
                None => {
                    self.people.remove(&n);
                }
            }
            touched.extend(self.chat_of.get(&n));
        }
        for (n, chat) in changes.chats {
            if let Some(old) = self.chats.remove(&n) {
                self.by_id.remove(&old.id);
                if let Some(person) = old.person
                    && self.chat_of.get(&person) == Some(&n)
                {
                    self.chat_of.remove(&person);
                }
            }
            if let Some(chat) = chat {
                if let Some(person) = chat.person {
                    self.chat_of.insert(person, n);
                }
                self.by_id.insert(chat.id.clone(), n);
                self.chats.insert(
                    n,
                    ChatEntry {
                        id: chat.id,
                        name: chat.name,
                        person: chat.person,
                        folders: chat.folders,
                        frecency: chat.frecency,
                        opened_at: chat.opened_at,
                        last_at: chat.last_at,
                        names: Vec::new(),
                    },
                );
                touched.insert(n);
            }
        }
        for n in touched {
            let Some(chat) = self.chats.get(&n) else { continue };
            let mut names: Vec<Name> = chat
                .name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .map(|name| Name::new(Field::Name, name))
                .into_iter()
                .collect();
            if let Some(person) = chat.person.and_then(|person| self.people.get(&person)) {
                names.extend(person.names());
            }
            if let Some(chat) = self.chats.get_mut(&n) {
                chat.names = names;
            }
        }
    }

    /// The name a chat is shown by.
    fn chat_name(&self, chat: &ChatEntry) -> String {
        match chat.names.first() {
            Some(name) => name.text.clone(),
            None => chat.id.split_once('@').map_or(chat.id.as_str(), |(user, _)| user).to_string(),
        }
    }

    /// The name of the chat with this JID, if jaw knows it.
    pub fn chat_name_of(&self, id: &str) -> Option<String> {
        self.by_id.get(id).and_then(|n| self.chats.get(n)).map(|chat| self.chat_name(chat))
    }

    fn folder_names(&self, folders: &[i64]) -> Vec<String> {
        folders.iter().filter_map(|n| self.folder_names.get(n).cloned()).collect()
    }

    /// The `limit` chats you open most, decayed to `now`, then the newest.
    pub fn recent(&self, limit: usize, now: i64) -> Vec<NameHit> {
        let mut chats: Vec<(i64, f64, i64)> = self
            .chats
            .iter()
            .map(|(&n, chat)| (n, frecency(chat.frecency, chat.opened_at, now), chat.last_at))
            .collect();
        chats.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(b.2.cmp(&a.2)).then(a.0.cmp(&b.0)));
        chats
            .into_iter()
            .take(limit)
            .map(|(n, frecency, _)| {
                let chat = &self.chats[&n];
                NameHit {
                    kind: Kind::Chat,
                    score: (frecency * 100.0).round() as u32,
                    text: self.chat_name(chat),
                    marks: Vec::new(),
                    field: None,
                    folders: self.folder_names(&chat.folders),
                    chat: Some(n),
                    row: None,
                }
            })
            .collect()
    }

    /// The best `limit` names of each of `kinds` matching `query`, best first, and how many of
    /// each kind matched; `None` once `cancel` says the find is no longer wanted.
    pub fn matches(
        &self,
        query: &Query,
        kinds: &[Kind],
        limit: usize,
        now: i64,
        cancel: &Cancel,
    ) -> Option<(Vec<NameHit>, BTreeMap<Kind, u32>)> {
        MATCHER.with(|matcher| {
            let matcher = &mut matcher.borrow_mut();
            let mut found: BTreeMap<Kind, Vec<Candidate>> = BTreeMap::new();
            if kinds.contains(&Kind::Chat) {
                for (seen, (&n, chat)) in self.chats.iter().enumerate() {
                    if seen % CHECK_EVERY == 0 && cancel.is_cancelled() {
                        return None;
                    }
                    if let Some((name, score)) = best(query, &chat.names, matcher) {
                        let frecency = frecency(chat.frecency, chat.opened_at, now);
                        found.entry(Kind::Chat).or_default().push(Candidate {
                            key: Key::Chat(n),
                            name,
                            score: blend(score, frecency),
                            frecency,
                            last_at: chat.last_at,
                            length: chat.names[name].text.len(),
                        });
                    }
                }
            }
            if kinds.contains(&Kind::Person) {
                for (seen, (&n, person)) in self.people.iter().enumerate() {
                    if seen % CHECK_EVERY == 0 && cancel.is_cancelled() {
                        return None;
                    }
                    if self.chat_of.contains_key(&n) {
                        continue;
                    }
                    if let Some((name, score)) = best(query, &person.names, matcher) {
                        found.entry(Kind::Person).or_default().push(Candidate {
                            key: Key::Person(n),
                            name,
                            score,
                            frecency: 0.0,
                            last_at: 0,
                            length: person.names[name].text.len(),
                        });
                    }
                }
            }
            if kinds.contains(&Kind::Folder) {
                for (at, folder) in self.folders.iter().enumerate() {
                    if let Some((_, score)) = best(query, std::slice::from_ref(&folder.name), matcher) {
                        found.entry(Kind::Folder).or_default().push(Candidate {
                            key: Key::Folder(at),
                            name: 0,
                            score,
                            frecency: 0.0,
                            last_at: 0,
                            length: folder.name.text.len(),
                        });
                    }
                }
            }
            let counts = found.iter().map(|(&kind, candidates)| (kind, candidates.len() as u32)).collect();
            let mut hits = Vec::new();
            for (kind, mut candidates) in found {
                candidates.sort_unstable_by(|a, b| {
                    b.score
                        .cmp(&a.score)
                        .then(b.frecency.total_cmp(&a.frecency))
                        .then(b.last_at.cmp(&a.last_at))
                        .then(a.length.cmp(&b.length))
                });
                for candidate in candidates.into_iter().take(limit) {
                    hits.push(self.hit(kind, &candidate, query, matcher));
                }
            }
            hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
            Some((hits, counts))
        })
    }

    fn hit(&self, kind: Kind, candidate: &Candidate, query: &Query, matcher: &mut Matcher) -> NameHit {
        let (name, folders, chat, row) = match candidate.key {
            Key::Chat(n) => {
                let chat = &self.chats[&n];
                (&chat.names[candidate.name], self.folder_names(&chat.folders), Some(n), None)
            }
            Key::Person(n) => {
                let person = &self.people[&n];
                let shown = &person.names[0].text;
                let row = json!({"id": format!("person:{n}"), "name": shown, "phone": person.phones.first()});
                (&person.names[candidate.name], self.folder_names(&person.folders), None, Some(row))
            }
            Key::Folder(at) => {
                let folder = &self.folders[at];
                let row = json!({"id": folder.id, "name": folder.name.text, "parent": folder.parent});
                (&folder.name, Vec::new(), None, Some(row))
            }
        };
        NameHit {
            kind,
            score: candidate.score,
            text: name.text.clone(),
            marks: query.marks(name, matcher),
            field: (kind != Kind::Folder).then_some(name.field),
            folders,
            chat,
            row,
        }
    }

    /// Roughly what the index holds on the heap, in bytes.
    pub fn bytes(&self) -> usize {
        let names = |names: &[Name]| names.iter().map(Name::bytes).sum::<usize>();
        let chats: usize = self
            .chats
            .values()
            .map(|chat| {
                std::mem::size_of::<(i64, ChatEntry)>()
                    + chat.id.capacity() * 2
                    + chat.name.as_ref().map_or(0, String::capacity)
                    + chat.folders.capacity() * 8
                    + names(&chat.names)
            })
            .sum();
        let people: usize = self
            .people
            .values()
            .map(|person| {
                std::mem::size_of::<(i64, PersonEntry)>()
                    + [&person.name, &person.push_name, &person.verified_name]
                        .iter()
                        .map(|text| text.as_ref().map_or(0, String::capacity))
                        .sum::<usize>()
                    + person.phones.iter().map(String::capacity).sum::<usize>()
                    + person.folders.capacity() * 8
                    + names(&person.names)
            })
            .sum();
        let folders: usize = self.folders.iter().map(|folder| folder.id.capacity() + folder.name.bytes()).sum();
        chats + people + folders + self.by_id.capacity() * 32 + self.chat_of.capacity() * 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_000_000_000;
    const WEEK: i64 = 7 * 24 * 3600 * 1000;
    const NAMES: [Kind; 3] = [Kind::Chat, Kind::Person, Kind::Folder];

    fn person(n: i64, name: &str) -> (i64, Option<PersonRecord>) {
        let record = PersonRecord {
            name: Some(name.into()),
            push_name: None,
            verified_name: None,
            phones: Vec::new(),
            folders: Vec::new(),
        };
        (n, Some(record))
    }

    fn group(n: i64, name: &str) -> ChatRecord {
        ChatRecord {
            id: format!("120363{n:012}@g.us"),
            name: Some(name.into()),
            person: None,
            frecency: 0.0,
            opened_at: None,
            last_at: 0,
            folders: Vec::new(),
        }
    }

    fn people(names: &[&str]) -> Index {
        let mut index = Index::default();
        let people = names.iter().enumerate().map(|(n, name)| person(n as i64 + 1, name)).collect();
        index.apply(Changes { people, chats: Vec::new(), folders: None });
        index
    }

    fn groups(chats: Vec<ChatRecord>) -> Index {
        let mut index = Index::default();
        index.apply(Changes {
            people: Vec::new(),
            chats: chats.into_iter().enumerate().map(|(n, chat)| (n as i64 + 1, Some(chat))).collect(),
            folders: None,
        });
        index
    }

    fn hits(index: &Index, typed: &str) -> Vec<NameHit> {
        index.matches(&Query::new(typed), &NAMES, 50, NOW, &Cancel::default()).expect("not cancelled").0
    }

    fn found(index: &Index, typed: &str) -> Vec<String> {
        hits(index, typed).into_iter().map(|hit| hit.text).collect()
    }

    #[test]
    fn initials_rank_above_the_same_letters_inside_words() {
        let index = people(&["Maria Santos", "Carla Moreira", "Ana Ribeiro", "Bárbara Lopes", "Pedro Gomes"]);
        let found = found(&index, "ar");
        assert_eq!(found[0], "Ana Ribeiro");
        assert_eq!(found.len(), 4, "Pedro Gomes has no a before an r: {found:?}");
        let marks = &hits(&index, "ar")[0].marks;
        assert_eq!(marks, &[(0, 1), (4, 5)], "the A and the R are marked");
    }

    #[test]
    fn a_word_start_ranks_above_the_same_letters_inside_a_word() {
        assert_eq!(found(&people(&["Caribe Bar", "Ana Ribeiro"]), "rib"), ["Ana Ribeiro", "Caribe Bar"]);
        assert_eq!(found(&people(&["Joana Silva", "Mariana", "Ana Ribeiro"]), "ana")[0], "Ana Ribeiro");
        assert_eq!(found(&people(&["Alcides", "Lisbon Climbing"]), "lc"), ["Lisbon Climbing", "Alcides"]);
    }

    #[test]
    fn accents_and_case_are_ignored_on_either_side() {
        let index = people(&["Inês Gomes", "Conceição Reis", "Ines Costa", "ANTÓNIO", "Rui"]);
        let mut ines = found(&index, "ines");
        ines.sort();
        assert_eq!(ines, ["Ines Costa", "Inês Gomes"]);
        let mut typed_with_accent = found(&index, "Inês");
        typed_with_accent.sort();
        assert_eq!(typed_with_accent, ["Ines Costa", "Inês Gomes"]);
        assert_eq!(found(&index, "conceicao"), ["Conceição Reis"]);
        assert_eq!(found(&index, "antonio"), ["ANTÓNIO"]);
        let hit = hits(&index, "ines").into_iter().find(|hit| hit.text == "Inês Gomes").unwrap();
        assert_eq!(hit.marks, [(0, 5)], "the whole of Inês, ê being two bytes");
    }

    #[test]
    fn letters_scattered_inside_words_are_not_a_match() {
        assert_eq!(found(&people(&["Pedro Gomes", "Leonor"]), "eo"), ["Leonor"]);
        assert!(found(&people(&["Pedro Gomes"]), "zx").is_empty());
    }

    #[test]
    fn english_names_and_several_words() {
        let index = people(&["Thomas Webb", "Tomás Brito", "Tom Baker", "Emma Hughes"]);
        assert_eq!(found(&index, "tom b")[..2], ["Tom Baker", "Tomás Brito"]);
        assert_eq!(found(&index, "emma h"), ["Emma Hughes"]);
        assert_eq!(found(&index, "hug"), ["Emma Hughes"]);
    }

    #[test]
    fn a_number_finds_people_by_a_run_of_its_digits() {
        let mut index = Index::default();
        let (_, ana) = person(1, "Ana");
        let (_, rui) = person(2, "Rui");
        let ana = PersonRecord { phones: vec!["+351912345678".into()], ..ana.unwrap() };
        let rui = PersonRecord { phones: vec!["+351961111111".into()], ..rui.unwrap() };
        index.apply(Changes { people: vec![(1, Some(ana)), (2, Some(rui))], chats: Vec::new(), folders: None });
        let hits = hits(&index, "912 34");
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].text.as_str(), hits[0].field), ("+351912345678", Some(Field::Phone)));
        assert_eq!(hits[0].marks, [(4, 9)]);
        assert!(found(&index, "91").is_empty(), "two digits aren't enough to find a number");
        assert_eq!(found(&index, "+351 96"), ["+351961111111"]);
    }

    #[test]
    fn a_person_with_a_chat_is_found_as_the_chat_by_any_of_their_names() {
        let mut index = Index::default();
        let (_, ana) = person(1, "Ana Ribeiro");
        let rui = PersonRecord { name: None, push_name: Some("Rui C".into()), ..person(3, "").1.unwrap() };
        let direct = |n: i64, person: i64| ChatRecord {
            id: format!("35191000000{n}@s.whatsapp.net"),
            name: None,
            person: Some(person),
            ..group(n, "")
        };
        index.apply(Changes {
            people: vec![(1, ana), person(2, "Artur Reis"), (3, Some(rui))],
            chats: vec![(10, Some(direct(1, 1))), (11, Some(direct(3, 3)))],
            folders: None,
        });
        let hits = hits(&index, "ar");
        let kinds: Vec<(Kind, &str)> = hits.iter().map(|hit| (hit.kind, hit.text.as_str())).collect();
        assert!(kinds.contains(&(Kind::Chat, "Ana Ribeiro")) && kinds.contains(&(Kind::Person, "Artur Reis")));
        assert_eq!(kinds.len(), 2, "Ana is found once, as her chat: {kinds:?}");
        let rui = self::hits(&index, "rui c");
        assert_eq!((rui[0].kind, rui[0].field, rui[0].chat), (Kind::Chat, Some(Field::PushName), Some(11)));
        assert_eq!(index.chat_name_of("351910000003@s.whatsapp.net").as_deref(), Some("Rui C"));
    }

    #[test]
    fn frecency_lifts_a_chat_above_as_good_a_match_but_not_above_a_much_better_one() {
        let index = groups(vec![
            group(1, "Rui Costa"),
            ChatRecord { frecency: 3.0, opened_at: Some(NOW), ..group(2, "Rui Santos") },
            ChatRecord { frecency: 50.0, opened_at: Some(NOW), ..group(3, "Marina") },
            group(4, "Ana Ribeiro"),
        ]);
        assert_eq!(found(&index, "rui"), ["Rui Santos", "Rui Costa"]);
        assert_eq!(found(&index, "ar")[..2], ["Ana Ribeiro", "Marina"], "initials still beat letters inside a word");
        let long_ago = NOW - 8 * WEEK;
        let index = groups(vec![
            ChatRecord { frecency: 1.0, opened_at: Some(NOW), ..group(1, "Rui Costa") },
            ChatRecord { frecency: 3.0, opened_at: Some(long_ago), ..group(2, "Rui Santos") },
        ]);
        assert_eq!(found(&index, "rui"), ["Rui Costa", "Rui Santos"], "opens two months ago count for little");
    }

    #[test]
    fn nothing_typed_gives_the_chats_opened_most_then_the_newest() {
        let week_ago = NOW - WEEK;
        let index = groups(vec![
            ChatRecord { last_at: 5, ..group(1, "Quiet") },
            ChatRecord { frecency: 1.0, opened_at: Some(NOW), ..group(2, "Once today") },
            ChatRecord { last_at: 9, ..group(3, "Busy") },
            ChatRecord { frecency: 5.0, opened_at: Some(week_ago), ..group(4, "Often last week") },
        ]);
        let recent: Vec<(String, u32)> = index.recent(10, NOW).into_iter().map(|hit| (hit.text, hit.score)).collect();
        let names: Vec<&str> = recent.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["Often last week", "Once today", "Busy", "Quiet"]);
        assert_eq!(recent[0].1, 250, "five opens a week ago count as two and a half");
    }

    #[test]
    fn folders_are_found_by_name_and_chats_say_which_they_are_in() {
        let mut index = groups(vec![ChatRecord { folders: vec![1], ..group(1, "Primos e primas") }]);
        index.set_folders(vec![
            (1, None, false, None, "Família".into()),
            (2, None, false, Some(1), "Primos".into()),
            (3, Some("archived".into()), false, None, "Archived".into()),
        ]);
        index.apply(Changes {
            people: Vec::new(),
            chats: vec![(1, Some(ChatRecord { folders: vec![2], ..group(1, "Primos e primas") }))],
            folders: None,
        });
        let hits = hits(&index, "fam");
        assert_eq!((hits[0].kind, hits[0].row.as_ref().unwrap()["id"].as_str()), (Kind::Folder, Some("folder:1")));
        let archived = self::hits(&index, "arch");
        assert_eq!(archived[0].row.as_ref().unwrap()["id"], "archived");
        let primos = self::hits(&index, "primos");
        let chat = primos.iter().find(|hit| hit.kind == Kind::Chat).unwrap();
        assert_eq!(chat.folders, ["Primos"]);
        let folder = primos.iter().find(|hit| hit.kind == Kind::Folder).unwrap();
        assert_eq!(folder.row.as_ref().unwrap()["parent"], "folder:1");
    }

    #[test]
    fn each_kind_is_counted_and_cut_to_its_limit() {
        let names: Vec<String> = (0..30).map(|n| format!("Ana {n}")).collect();
        let index = people(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let (hits, counts) = index.matches(&Query::new("ana"), &NAMES, 5, NOW, &Cancel::default()).unwrap();
        assert_eq!((hits.len(), counts[&Kind::Person]), (5, 30));
        let (none, _) = index.matches(&Query::new("ana"), &[Kind::Chat], 5, NOW, &Cancel::default()).unwrap();
        assert!(none.is_empty(), "only the kinds asked for");
    }

    #[test]
    fn a_cancelled_find_stops_matching() {
        let index = people(&["Ana"]);
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(index.matches(&Query::new("ana"), &NAMES, 5, NOW, &cancel).is_none());
    }
}
