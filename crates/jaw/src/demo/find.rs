//! The made-up account's `find`, following jawd's rules: names first, matched fuzzily, people
//! found only when they have no chat yet, a chat ranked up by how often it's opened; then
//! messages, newest first, from the second letter typed. Asked again before it answers, it gives
//! up on the old query, which is answered as cancelled, as jawd answers it with error 7.

use super::{Account, Demo, Thread};
use crate::data::{Chat, Counts, Found, FoundKind, Hit, Kind, Mark, MarkKind, Message, Person, Update};
use crate::find;
use std::sync::atomic::Ordering;

/// One `find`, as the interface asked it.
pub struct Ask {
    pub id: u64,
    pub query: String,
    pub kind: Option<FoundKind>,
    pub chat: Option<String>,
    pub limit: usize,
}

/// The built-in folders, by their fixed IDs.
const FOLDERS: &[(&str, &str)] = &[
    ("all", "All"),
    ("unread", "Unread"),
    ("groups", "Groups"),
    ("communities", "Communities"),
    ("archived", "Archived"),
];
/// Messages looked at between taking the account's lock again and looking for a newer query.
const CHUNK: usize = 200;
/// Messages are counted up to this many.
const MOST_COUNTED: usize = 1_000;

/// A message that matched: when it was sent, where it is in the account, and where it matched.
struct Seen {
    at: i64,
    thread: usize,
    index: usize,
    ranges: Vec<(usize, usize)>,
}

fn plain(s: &str) -> Vec<Mark> {
    vec![Mark { t: MarkKind::Text, s: s.to_string() }]
}

/// What a message says, as its text is searched.
fn said(m: &Message) -> String {
    let text: String = m.spans.iter().map(|s| s.s.as_str()).collect();
    match &m.summary {
        Some(summary) if m.kind != Kind::Text => format!("{summary} {text}"),
        _ => text,
    }
}

impl Demo {
    pub(super) fn ask(&self, ask: Ask) {
        self.shared.latest.store(ask.id, Ordering::SeqCst);
        let finder = self.shared.finder.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match finder {
            Some(tx) => {
                let _ = tx.send(ask);
            }
            None => self.answer(ask),
        }
    }

    /// Answers `find` from a thread of its own, as jawd does, so a search never holds up drawing.
    pub fn answer_on_a_thread(&self) {
        let (tx, rx) = std::sync::mpsc::channel::<Ask>();
        let demo = self.clone();
        std::thread::Builder::new()
            .name("jaw-find".into())
            .spawn(move || {
                while let Ok(mut ask) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        demo.cancel(ask.id);
                        ask = newer;
                    }
                    demo.answer(ask);
                }
            })
            .expect("the demo's find thread");
        *self.shared.finder.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    }

    fn superseded(&self, id: u64) -> bool {
        self.shared.latest.load(Ordering::SeqCst) != id
    }

    fn cancel(&self, id: u64) {
        let why = "a newer find replaced this one".to_string();
        self.lock().send(Update::FindFailed { id, cancelled: true, why });
    }

    fn answer(&self, ask: Ask) {
        let (mut results, mut counts) = self.lock().names(&ask);
        let words = find::words(&ask.query);
        if ask.kind.is_none_or(|k| k == FoundKind::Message) && !words.is_empty() {
            let Some((found, n)) = self.messages(&ask, &words) else {
                self.cancel(ask.id);
                return;
            };
            if n > 0 {
                counts.insert(FoundKind::Message, n);
            }
            results.extend(found);
        }
        if self.superseded(ask.id) {
            self.cancel(ask.id);
            return;
        }
        self.lock().send(Update::Found { id: ask.id, results, counts });
    }

    /// The messages that match, newest first, looked through a chunk at a time; `None` if a newer
    /// query came meanwhile.
    fn messages(&self, ask: &Ask, words: &[Vec<char>]) -> Option<(Vec<Found>, u32)> {
        let chats: Vec<(usize, usize)> = {
            let a = self.lock();
            let here = |t: &Thread| ask.chat.as_ref().is_none_or(|c| *c == t.chat.id);
            a.threads
                .iter()
                .enumerate()
                .filter(|(_, t)| here(t) && !t.chat.archived)
                .map(|(i, t)| (i, t.messages.len()))
                .collect()
        };
        let mut hits: Vec<Seen> = vec![];
        for (t, len) in chats {
            let (mut end, mut here) = (len, 0);
            while end > 0 && here < MOST_COUNTED {
                if self.superseded(ask.id) {
                    return None;
                }
                let a = self.lock();
                let start = end.saturating_sub(CHUNK);
                for j in (start..end).rev() {
                    let m = &a.threads[t].messages[j];
                    if m.deleted || matches!(m.kind, Kind::System | Kind::Call) {
                        continue;
                    }
                    if let Some(ranges) = find::message(words, &find::fold(&said(m))) {
                        hits.push(Seen { at: m.at, thread: t, index: j, ranges });
                        here += 1;
                    }
                }
                end = start;
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.at));
        let a = self.lock();
        let found = hits
            .iter()
            .take(ask.limit)
            .map(|h| {
                let thread = &a.threads[h.thread];
                let m = &thread.messages[h.index];
                let (s, moved) = find::stretch(&said(m), &h.ranges, 160);
                Found {
                    kind: FoundKind::Message,
                    score: 0,
                    text: find::marked(&s, &moved),
                    chat_name: Some(thread.chat.name.clone()),
                    folders: vec![],
                    hit: Hit::Message(m.clone()),
                }
            })
            .collect();
        Some((found, hits.len().min(MOST_COUNTED) as u32))
    }
}

impl Account {
    fn opened(&self, chat: &str) -> u32 {
        self.opens.get(chat).copied().unwrap_or(0)
    }

    fn found_chat(&self, c: &Chat, score: i64, text: Vec<Mark>) -> Found {
        Found { kind: FoundKind::Chat, score, text, chat_name: None, folders: vec![], hit: Hit::Chat(c.clone()) }
    }

    /// Chats, people and folders: what's opened most when nothing is typed, else what matches.
    fn names(&self, ask: &Ask) -> (Vec<Found>, Counts) {
        let q = ask.query.trim();
        let want = |k: FoundKind| ask.kind.is_none_or(|x| x == k);
        let mut counts = Counts::new();
        let mut out: Vec<Found> = vec![];
        if q.is_empty() {
            if want(FoundKind::Chat) {
                let mut chats: Vec<&Chat> = self.threads.iter().map(|t| &t.chat).filter(|c| !c.archived).collect();
                chats.sort_by(|a, b| self.opened(&b.id).cmp(&self.opened(&a.id)).then_with(|| b.sort.cmp(&a.sort)));
                counts.insert(FoundKind::Chat, chats.len() as u32);
                for c in chats.into_iter().take(ask.limit) {
                    out.push(self.found_chat(c, 100 * self.opened(&c.id) as i64, plain(&c.name)));
                }
            }
            return (out, counts);
        }
        let mut kind = |k: FoundKind, mut found: Vec<Found>| {
            found.sort_by_key(|f| std::cmp::Reverse(f.score));
            if !found.is_empty() {
                counts.insert(k, found.len() as u32);
            }
            found.truncate(ask.limit);
            out.extend(found);
        };
        if want(FoundKind::Chat) {
            let found = self
                .threads
                .iter()
                .filter_map(|t| {
                    let (score, r) = find::name(q, &t.chat.name)?;
                    let score = score + (score / 2).min(10 * self.opened(&t.chat.id) as i64);
                    Some(self.found_chat(&t.chat, score, find::marked(&t.chat.name, &r)))
                })
                .collect();
            kind(FoundKind::Chat, found);
        }
        if want(FoundKind::Person) {
            let digits: String = q.chars().filter(char::is_ascii_digit).collect();
            let by_number = digits.len() >= 3 && q.chars().all(|c| c.is_ascii_digit() || " +-".contains(c));
            let found = self
                .contacts
                .iter()
                .filter_map(|p: &Person| {
                    let phone = p.phone.clone().unwrap_or_default();
                    let (score, text) = if by_number {
                        let at = phone.find(&digits)?;
                        let start = phone[..at].chars().count();
                        (50, find::marked(&phone, &[(start, start + digits.chars().count())]))
                    } else {
                        let (score, r) = find::name(q, &p.name)?;
                        (score, find::marked(&p.name, &r))
                    };
                    let hit = Hit::Person(p.clone());
                    Some(Found { kind: FoundKind::Person, score, text, chat_name: None, folders: vec![], hit })
                })
                .collect();
            kind(FoundKind::Person, found);
        }
        if want(FoundKind::Folder) {
            let found = FOLDERS
                .iter()
                .filter_map(|(id, name)| {
                    let (score, r) = find::name(q, name)?;
                    let hit = Hit::Folder(crate::data::Folder { id: id.to_string(), name: name.to_string() });
                    let text = find::marked(name, &r);
                    Some(Found { kind: FoundKind::Folder, score, text, chat_name: None, folders: vec![], hit })
                })
                .collect();
            kind(FoundKind::Folder, found);
        }
        (out, counts)
    }
}
