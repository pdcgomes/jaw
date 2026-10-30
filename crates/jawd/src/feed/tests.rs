use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jaw_proto::{FromJawd, code};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, watch};

use super::client::MOST_WINDOWS;
use super::window::{chat_row, message_row};
use super::*;
use crate::engine::pb::chat_presence::State::{Composing, Paused, Recording};
use crate::state::State;
use crate::store::read::{self, Around};
use crate::store::{Amend, Amendment, Change, Chat, Lane, Message, Person, Read, Store};
use crate::work::Report;

mod communities;

pub(super) fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

pub(super) fn brisk() -> Timing {
    Timing { quiet: ms(1), burst: ms(10), hold: ms(300), behind: ms(300) }
}

pub(super) struct Bed {
    home: tempfile::TempDir,
    pub(super) store: Store,
    pub(super) feed: Feed,
    catching_up: Arc<AtomicBool>,
}

impl Bed {
    pub(super) fn new(timing: Timing) -> Bed {
        Bed::keeping(timing, CHANGES_KEPT)
    }

    /// With a feed keeping `kept` reads of the events for a connection that falls behind.
    fn keeping(timing: Timing, kept: usize) -> Bed {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let catching_up = Arc::new(AtomicBool::new(false));
        let holding = catching_up.clone();
        let feed = Feed::keeping(store.clone(), Arc::new(move || holding.load(Ordering::Relaxed)), timing, kept);
        Bed { home, store, feed, catching_up }
    }

    pub(super) async fn apply(&self, changes: Vec<Change>) -> i64 {
        self.store.apply(Lane::Live, changes).await.unwrap().seq
    }

    pub(super) fn watcher(&self, room: usize) -> Watcher {
        let (out, lines) = mpsc::channel(room);
        Watcher {
            client: self.feed.client(out),
            lines,
            windows: HashMap::new(),
            seen: self.feed.seq() as i64,
            resynced: Vec::new(),
            batches: HashMap::new(),
            typing: BTreeMap::new(),
            told: Vec::new(),
            presences: BTreeMap::new(),
            presence_told: Vec::new(),
            pushes: Vec::new(),
        }
    }
}

fn text(chat: &str, id: &str, at: i64, sender: Option<&str>, words: &str) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: sender.map(Into::into),
        mine: sender.is_none(),
        status: sender.is_none().then_some("sent"),
        kind: "text",
        text: Some(words.into()),
        summary: words.into(),
        ..Message::default()
    }
}

/// A message from `sender` mentioning `whom` by their JID, as WhatsApp sends one.
fn mentioning(chat: &str, id: &str, at: i64, sender: &str, whom: &str) -> Message {
    let number = whom.split('@').next().unwrap_or(whom);
    let spans = json!([{"t": "text", "s": "ask "}, {"t": "mention", "s": format!("@{number}"), "person": whom}]);
    Message { spans: Some(spans.to_string()), ..text(chat, id, at, Some(sender), &format!("ask @{number}")) }
}

/// A message from `sender` quoting `quoted`, which `whom` wrote.
fn quoting(chat: &str, id: &str, at: i64, sender: &str, quoted: &str, whom: &str) -> Message {
    let reply = json!({"id": quoted, "from": whom, "summary": "Saturday at 9?"});
    Message { reply: Some(reply.to_string()), ..text(chat, id, at, Some(sender), "works for me") }
}

fn named(jid: &str, name: &str) -> Person {
    Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() }
}

/// A window as an interface holds it, built only from results and pushes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Model {
    chats: bool,
    /// A messages window's chat.
    of: Option<String>,
    rows: HashMap<String, Value>,
    pub(super) total: u64,
    pub(super) older: bool,
    newer: u64,
    seq: u64,
    /// A library window's counts of each kind.
    pub(super) counts: Value,
}

impl Model {
    fn take(&mut self, update: &Value, rows: &str) {
        for row in update[rows].as_array().into_iter().flatten() {
            let id = row["id"].as_str().unwrap().to_string();
            let newer = self.rows.get(&id).is_none_or(|held| held["v"].as_u64() <= row["v"].as_u64());
            if newer {
                self.rows.insert(id, row.clone());
            }
        }
        for id in update["remove"].as_array().into_iter().flatten() {
            self.rows.remove(id.as_str().unwrap());
        }
        if let Some(total) = update["total"].as_u64() {
            self.total = total;
        }
        if let Some(older) = update["older"].as_bool() {
            self.older = older;
        }
        if let Some(newer) = update["newer"].as_u64() {
            self.newer = newer;
        }
        if let Some(seq) = update["seq"].as_u64() {
            self.seq = self.seq.max(seq);
        }
        if let Some(counts) = update.get("counts") {
            self.counts = counts.clone();
        }
    }

    /// Its rows in the list's order: chats and items from the largest sort down, messages from the
    /// smallest up.
    pub(super) fn ordered(&self) -> Vec<Value> {
        let mut rows: Vec<Value> = self.rows.values().cloned().collect();
        rows.sort_by(|a, b| a["sort"].as_str().cmp(&b["sort"].as_str()));
        if self.chats {
            rows.reverse();
        }
        rows
    }

    pub(super) fn ids(&self) -> Vec<String> {
        self.ordered().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
    }

    /// The chats it shows: a chat list's rows, a messages window's chat, or a roster's people's
    /// one-to-one chats.
    fn shown(&self) -> Vec<String> {
        match &self.of {
            Some(chat) => vec![chat.clone()],
            None => self
                .rows
                .iter()
                .filter_map(|(id, row)| match id.starts_with("person:") {
                    true => row["chat"].as_str().map(str::to_string),
                    false => Some(id.clone()),
                })
                .collect(),
        }
    }

    fn assert_matches(&self, fresh: &Model, what: &str) {
        assert_eq!(self.ids(), fresh.ids(), "{what}: its rows");
        assert_eq!(self.ordered(), fresh.ordered(), "{what}: what its rows say");
        assert_eq!(self.total, fresh.total, "{what}: total");
        if !self.chats {
            assert_eq!((self.older, self.newer), (fresh.older, fresh.newer), "{what}: older and newer");
        }
    }
}

/// A client driven by the test, with what its interface would hold.
pub(super) struct Watcher {
    client: Client,
    lines: mpsc::Receiver<Vec<u8>>,
    pub(super) windows: HashMap<String, Model>,
    seen: i64,
    resynced: Vec<String>,
    pub(super) batches: HashMap<String, usize>,
    /// Who it holds as typing, by chat, as the protocol keeps it, and the `typing` pushes it took,
    /// in order, each after the window pushes that came before it.
    typing: BTreeMap<String, Value>,
    told: Vec<(String, Value)>,
    /// The presence it holds for each one-to-one chat its conversations show, and the `presence`
    /// pushes it took, in order.
    presences: BTreeMap<String, Value>,
    presence_told: Vec<(String, Value)>,
    /// Every push it took, by method and the chat or watch it's for, in order.
    pushes: Vec<(String, Value)>,
}

impl Watcher {
    pub(super) async fn call(&mut self, method: &str, params: Value) -> Result<Value, jaw_proto::Error> {
        let of = params.get("chat").and_then(Value::as_str).map(str::to_string);
        let unwatching = (method == "unwatch").then(|| params["watch"].as_str().unwrap_or_default().to_string());
        let result = self.client.call(method, params).await?;
        if let Some(watch) = result["watch"].as_str() {
            let model = self.windows.entry(watch.to_string()).or_default();
            // Its kind is the method that watched it, which a move keeps: a chat's messages read
            // from the smallest sort up, and every other window from the largest down.
            if method != "watch.move" {
                model.chats = method != "messages.watch";
            }
            if method == "messages.watch" {
                model.of = of;
            }
            model.take(&result, "rows");
            // The answer says who's typing in every chat of its window, and a conversation's its
            // person's presence.
            for chat in model.shown() {
                match result["typing"].get(&chat) {
                    Some(who) => self.typing.insert(chat, who.clone()),
                    None => self.typing.remove(&chat),
                };
            }
            if let Some(chat) = model.of.clone() {
                match result.get("presence") {
                    Some(presence) => self.presences.insert(chat, presence.clone()),
                    None => self.presences.remove(&chat),
                };
            }
            self.forget_unshown();
        }
        if let Some(watch) = unwatching {
            self.windows.remove(&watch);
            self.forget_unshown();
        }
        Ok(result)
    }

    /// Forgets the typing of chats none of its windows shows, and the presence of those none of its
    /// conversations shows, as an interface does.
    fn forget_unshown(&mut self) {
        let shown: HashSet<String> = self.windows.values().flat_map(Model::shown).collect();
        self.typing.retain(|chat, _| shown.contains(chat));
        let open: HashSet<String> = self.windows.values().filter_map(|model| model.of.clone()).collect();
        self.presences.retain(|chat, _| open.contains(chat));
    }

    pub(super) async fn watch(&mut self, method: &str, params: Value) -> String {
        let result = self.call(method, params).await.unwrap();
        result["watch"].as_str().unwrap().to_string()
    }

    fn drain(&mut self) {
        while let Ok(line) = self.lines.try_recv() {
            self.take(&line);
        }
    }

    fn take(&mut self, line: &[u8]) {
        let FromJawd::Push(push) = serde_json::from_slice(line).unwrap() else { panic!("a reply among the pushes") };
        if push.method == "typing" {
            let chat = push.params["chat"].as_str().unwrap().to_string();
            let shown = self.windows.values().any(|model| model.shown().contains(&chat));
            assert!(shown, "typing pushed for {chat}, which no window shows: {}", push.params);
            let who = push.params["who"].clone();
            self.told.push((chat.clone(), who.clone()));
            self.pushes.push(("typing".into(), json!(chat)));
            match who.as_array().is_some_and(|who| who.is_empty()) {
                true => self.typing.remove(&chat),
                false => self.typing.insert(chat, who),
            };
            return;
        }
        if push.method == "presence" {
            let chat = push.params["chat"].as_str().unwrap().to_string();
            let open = self.windows.values().any(|model| model.of.as_deref() == Some(chat.as_str()));
            assert!(open, "presence pushed for {chat}, which no conversation shows: {}", push.params);
            let mut presence = push.params.clone();
            presence.as_object_mut().unwrap().remove("chat");
            self.presence_told.push((chat.clone(), presence.clone()));
            self.pushes.push(("presence".into(), json!(chat)));
            self.presences.insert(chat, presence);
            return;
        }
        let watch = push.params["watch"].as_str().unwrap().to_string();
        self.pushes.push((push.method.clone(), push.params.clone()));
        match push.method.as_str() {
            "resync" => {
                self.windows.remove(&watch);
                self.resynced.push(watch);
            }
            "chats" | "messages" | "roster" | "folders" | "library" => {
                *self.batches.entry(watch.clone()).or_default() += 1;
                let model = self.windows.get_mut(&watch).expect("a push for a window it watches");
                // A window never goes back to an older read than its interface holds.
                let seq = push.params["seq"].as_u64().unwrap();
                assert!(seq >= model.seq, "{watch} went back from seq {} to {seq}: {}", model.seq, push.params);
                model.take(&push.params, "upsert");
            }
            method => panic!("an unexpected {method} push"),
        }
        self.forget_unshown();
    }

    /// Runs the client for `long`, taking what it sends.
    pub(super) async fn run_for(&mut self, long: Duration) {
        let until = Instant::now() + long;
        while let Ok(ready) = tokio::time::timeout_at(until.into(), self.client.ready()).await {
            if let Ready::Changes(Ok(changes)) = &ready {
                self.seen = self.seen.max(changes.seq);
            }
            self.client.handle(ready).await;
            self.drain();
        }
        self.drain();
    }

    async fn turn(&mut self) {
        let ready = tokio::time::timeout(Duration::from_secs(10), self.client.ready()).await.expect("nothing happened");
        if let Ready::Changes(Ok(changes)) = &ready {
            self.seen = self.seen.max(changes.seq);
        }
        self.client.handle(ready).await;
    }

    /// Runs the client until it has taken every change up to `seq` and sent what they meant.
    pub(super) async fn settle(&mut self, seq: i64) {
        loop {
            self.drain();
            if self.seen >= seq && !self.client.pending() {
                break;
            }
            self.turn().await;
        }
        self.drain();
    }
}

/// What a window should hold, as a test asks for it.
#[derive(Clone, Debug)]
enum Want {
    Chats { archived: bool, anchor: Option<String>, before: usize, after: usize },
    Messages { chat: String, anchor: Option<(String, i64, i64)>, before: usize, after: usize },
}

impl Want {
    fn params(&self) -> (&'static str, Value) {
        match self {
            Want::Chats { archived, anchor, before, after } => (
                "chats.watch",
                json!({"folder": if *archived { "archived" } else { "all" }, "around": anchor.as_deref().unwrap_or("top"), "before": before, "after": after}),
            ),
            Want::Messages { chat, anchor, before, after } => (
                "messages.watch",
                json!({"chat": chat, "around": anchor.as_ref().map_or("newest", |(id, _, _)| id.as_str()), "before": before, "after": after}),
            ),
        }
    }
}

/// The window `want` describes, from a fresh read of whole lists, sliced around its anchor.
async fn fresh(store: &Store, want: &Want) -> Model {
    let want = want.clone();
    let query = move |db: &rusqlite::Connection| -> anyhow::Result<Model> {
        Ok(match want {
            Want::Chats { archived, anchor, before, after } => {
                let full = read::chats(db, archived, None, 100_000)?;
                let total = full.len() as u64;
                let rows = match anchor {
                    None => full.into_iter().take(after).collect::<Vec<_>>(),
                    Some(id) => {
                        let (n, _, _) = read::chat_place(db, &id)?.expect("an anchor chat stays");
                        let at = read::chats_by_n(db, &[n])?.remove(0);
                        let (above, below): (Vec<_>, Vec<_>) =
                            full.into_iter().partition(|row| (row.sort.as_str(), row.n) > (at.sort.as_str(), at.n));
                        let take = if below.first().is_some_and(|row| row.n == n) { after + 1 } else { after };
                        let skip = above.len().saturating_sub(before);
                        above.into_iter().skip(skip).chain(below.into_iter().take(take)).collect()
                    }
                };
                let rows = rows.into_iter().map(|row| (row.id.clone(), serde_json::to_value(chat_row(row)).unwrap()));
                Model { chats: true, rows: rows.collect(), total, ..Model::default() }
            }
            Want::Messages { chat, anchor, before, after } => {
                let full = read::messages(db, &chat, Around::Newest, 1_000_000)?;
                let total = full.len() as u64;
                let (rows, older, newer) = match anchor {
                    None => {
                        let skip = full.len().saturating_sub(before);
                        (full.into_iter().skip(skip).collect::<Vec<_>>(), skip > 0, 0)
                    }
                    Some((id, at, n)) => {
                        let (at, n) = full.iter().find(|row| row.id == id).map_or((at, n), |row| (row.at, row.n));
                        let (above, below): (Vec<_>, Vec<_>) =
                            full.into_iter().partition(|row| (row.at, row.n) < (at, n));
                        let take = if below.first().is_some_and(|row| row.n == n) { after + 1 } else { after };
                        let skip = above.len().saturating_sub(before);
                        let shown = below.len().min(take);
                        let newer = below.len() - shown;
                        let rows = above.into_iter().skip(skip).chain(below.into_iter().take(take)).collect();
                        (rows, skip > 0, newer as u64)
                    }
                };
                let rows =
                    rows.into_iter().map(|row| (row.id.clone(), serde_json::to_value(message_row(row)).unwrap()));
                Model { chats: false, of: Some(chat), rows: rows.collect(), total, older, newer, ..Model::default() }
            }
        })
    };
    store.reads().query(crate::work::Priority::Interactive, query).await.unwrap()
}

/// A xorshift generator, so a failing seed can be run again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, of: &'a [T]) -> &'a T {
        &of[self.below(of.len())]
    }
}

/// A made-up account the property test writes to at random.
struct World {
    rng: Rng,
    clock: i64,
    chats: Vec<String>,
    messages: Vec<Message>,
    pinned: HashMap<String, bool>,
    archived: HashMap<String, bool>,
    next: u64,
}

const DIRECT: usize = 8;
const GROUPS: usize = 4;
const OPEN: &str = "351910000000@s.whatsapp.net";
const GROUP: &str = "120363000000000000@g.us";

fn person(n: usize) -> String {
    format!("3519100000{n:02}@s.whatsapp.net")
}

impl World {
    fn new(seed: u64) -> World {
        let mut chats: Vec<String> = (0..DIRECT).map(person).collect();
        chats.extend((0..GROUPS).map(|n| format!("1203630000000000{n:02}@g.us")));
        World {
            rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            clock: 1_700_000_000_000,
            chats,
            messages: Vec::new(),
            pinned: HashMap::new(),
            archived: HashMap::new(),
            next: 0,
        }
    }

    fn message(&mut self, chat: String, at: i64) -> Message {
        self.next += 1;
        let id = format!("m{}", self.next);
        let sender = match self.rng.below(5) {
            0 => None,
            _ if chat.ends_with("@g.us") => Some(person(DIRECT + self.rng.below(4))),
            _ => Some(chat.clone()),
        };
        let mut message = text(&chat, &id, at, sender.as_deref(), &format!("words {}", self.rng.next() % 1000));
        // Now and then one mentions someone or quotes an earlier message, so windows name people
        // who wrote none of their rows, and see them renamed.
        match self.rng.below(8) {
            0 if chat.ends_with("@g.us") => {
                let whom = person(self.rng.below(DIRECT + 4));
                message.spans = Some(json!([{"t": "mention", "s": "@someone", "person": whom}]).to_string());
            }
            1 if !self.messages.is_empty() => {
                let quoted = self.rng.pick(&self.messages).clone();
                if let Some(from) = quoted.sender {
                    let reply = json!({"id": quoted.id, "from": from, "summary": quoted.summary});
                    message.reply = Some(reply.to_string());
                }
            }
            _ => {}
        }
        self.messages.push(message.clone());
        message
    }

    /// The open chat and the group half the time, so their windows see plenty.
    fn some_chat(&mut self) -> String {
        match self.rng.below(4) {
            0 => OPEN.into(),
            1 => GROUP.into(),
            _ => {
                if self.rng.below(12) == 0 {
                    self.chats.push(format!("35192{:07}@s.whatsapp.net", self.rng.next() % 10_000_000));
                }
                self.rng.pick(&self.chats).clone()
            }
        }
    }

    fn new_message(&mut self) -> Change {
        self.clock += 1 + self.rng.below(3_000) as i64;
        let chat = self.some_chat();
        Change::Message(self.message(chat, self.clock))
    }

    /// One random write.
    fn change(&mut self) -> Vec<Change> {
        let roll = self.rng.below(100);
        match roll {
            0..30 => vec![self.new_message()],
            30..40 => {
                let chat = self.some_chat();
                let at = 1_700_000_000_000 + (self.rng.next() % (self.clock - 1_700_000_000_000).max(1) as u64) as i64;
                vec![Change::Message(self.message(chat, at))]
            }
            40..55 if !self.messages.is_empty() => {
                let k = self.rng.below(self.messages.len());
                let message = &mut self.messages[k];
                if message.mine {
                    message.status = Some(["delivered", "read", "played"][(self.rng.next() % 3) as usize]);
                } else {
                    message.text = Some(format!("edited {}", self.rng.next() % 1000));
                    message.summary = message.text.clone().unwrap();
                    message.edited_at = Some(self.clock);
                }
                vec![Change::Message(message.clone())]
            }
            55..63 if !self.messages.is_empty() => {
                let gone = self.messages.swap_remove(self.rng.below(self.messages.len()));
                vec![Change::DeleteMessage { chat: gone.chat, id: gone.id }]
            }
            63..73 => {
                let chat = self.rng.pick(&self.chats).clone();
                let pinned = self.pinned.entry(chat.clone()).or_default();
                *pinned = !*pinned;
                let pinned = if *pinned { self.clock } else { 0 };
                vec![Change::Chat(Chat { id: chat, pinned: Some(pinned), ..Chat::default() })]
            }
            73..78 => {
                let chat = self.rng.pick(&self.chats).clone();
                let archived = self.archived.entry(chat.clone()).or_default();
                *archived = !*archived;
                vec![Change::Chat(Chat { id: chat, archived: Some(*archived), ..Chat::default() })]
            }
            78..85 => {
                let chat = self.rng.pick(&self.chats).clone();
                let unread = self.rng.below(5) as u32;
                vec![Change::Read(Read {
                    chat,
                    at: self.clock,
                    ids: Vec::new(),
                    phone: Some((unread, 0)),
                    here: false,
                })]
            }
            85..92 => {
                let who = person(self.rng.below(DIRECT + 4));
                let name = format!("Someone {}", self.rng.next() % 100);
                vec![Change::Person(Person { jids: vec![who], name: Some(name), ..Person::default() })]
            }
            _ => (0..20).map(|_| self.new_message()).collect(),
        }
    }
}

/// Watches the windows the property test checks, through `watcher`, and says what each should be.
async fn watch_all(watcher: &mut Watcher, wants: &[Want]) -> Vec<(String, Want)> {
    let mut watched = Vec::new();
    for want in wants {
        let (method, params) = want.params();
        watched.push((watcher.watch(method, params).await, want.clone()));
    }
    watched
}

async fn check(bed: &Bed, watcher: &Watcher, watched: &[(String, Want)], what: &str) {
    for (watch, want) in watched {
        let expected = fresh(&bed.store, want).await;
        watcher.windows[watch].assert_matches(&expected, &format!("{what}, {watch} {want:?}"));
    }
}

async fn anchor_on(store: &Store, chat: &str, back: usize) -> Option<(String, i64, i64)> {
    let chat = chat.to_string();
    let rows = store
        .reads()
        .query(crate::work::Priority::Interactive, move |db| read::messages(db, &chat, Around::Newest, 1_000))
        .await
        .unwrap();
    rows.len().checked_sub(back + 1).map(|k| (rows[k].id.clone(), rows[k].at, rows[k].n))
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_match_fresh_queries_after_random_writes() {
    for seed in 1..=12u64 {
        let bed = Bed::new(brisk());
        let mut world = World::new(seed);
        let mut seeding: Vec<Change> =
            world.chats.iter().map(|chat| Change::Chat(Chat { id: chat.clone(), ..Chat::default() })).collect();
        seeding.extend((0..60).flat_map(|_| world.new_message_or_two()));
        bed.apply(seeding).await;
        let read = Read { chat: GROUP.into(), at: world.clock, ids: Vec::new(), phone: Some((3, 0)), here: false };
        bed.apply(vec![Change::Read(read)]).await;
        let mut watcher = bed.watcher(64);
        let wants = [
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: Some(person(3)), before: 2, after: 2 },
            Want::Chats { archived: true, anchor: None, before: 0, after: 3 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed.store, OPEN, 8).await, before: 3, after: 3 },
        ];
        let mut watched = watch_all(&mut watcher, &wants).await;
        let (unread, _) = watcher
            .call("messages.watch", json!({"chat": GROUP, "around": "unread", "before": 2, "after": 4}))
            .await
            .map(|result| (result["watch"].as_str().unwrap().to_string(), result))
            .unwrap();
        // The unread window's anchor is the third newest message others sent in the group.
        let group_rows =
            fresh(&bed.store, &Want::Messages { chat: GROUP.into(), anchor: None, before: 1000, after: 0 }).await;
        let theirs: Vec<Value> =
            group_rows.ordered().into_iter().filter(|row| !row["mine"].as_bool().unwrap()).collect();
        let anchor = &theirs[theirs.len() - 3];
        let at = anchor["at"].as_i64().unwrap();
        let n: i64 = anchor["sort"].as_str().unwrap().rsplit('~').next().unwrap().parse().unwrap();
        let unread_want = Want::Messages {
            chat: GROUP.into(),
            anchor: Some((anchor["id"].as_str().unwrap().into(), at, n)),
            before: 2,
            after: 4,
        };
        watched.push((unread, unread_want));
        check(&bed, &watcher, &watched, &format!("seed {seed}, as watched")).await;

        for step in 0..200 {
            let changes = world.change();
            // A new name changes chat rows without moving their versions, so it's checked on its own.
            let renamed = matches!(changes[..], [Change::Person(_)]);
            let seq = bed.apply(changes).await;
            if world.rng.below(20) == 0 {
                // Scrolling: the anchored windows move to another message or chat.
                let (watch, want) = watched[4].clone();
                let moved = match want {
                    Want::Messages { chat, before, after, .. } => {
                        let anchor = anchor_on(&bed.store, &chat, world.rng.below(12)).await;
                        Want::Messages { chat, anchor, before, after }
                    }
                    other => other,
                };
                let around = match &moved {
                    Want::Messages { anchor: Some((id, _, _)), .. } => id.clone(),
                    _ => "newest".into(),
                };
                watcher
                    .call("watch.move", json!({"watch": watch, "around": around, "before": 3, "after": 3}))
                    .await
                    .unwrap();
                watched[4].1 = moved;
                let chat = world.rng.pick(&world.chats[..DIRECT]).clone();
                watcher
                    .call("watch.move", json!({"watch": watched[1].0, "around": chat, "before": 2, "after": 2}))
                    .await
                    .unwrap();
                watched[1].1 = Want::Chats { archived: false, anchor: Some(chat), before: 2, after: 2 };
            }
            if renamed || world.rng.below(4) == 0 {
                watcher.settle(seq).await;
                check(&bed, &watcher, &watched, &format!("seed {seed}, step {step}")).await;
            }
        }
        let seq = bed.apply(vec![world.new_message()]).await;
        watcher.settle(seq).await;
        check(&bed, &watcher, &watched, &format!("seed {seed}, at the end")).await;
    }
}

impl World {
    fn new_message_or_two(&mut self) -> Vec<Change> {
        let mut changes = vec![self.new_message()];
        if self.rng.below(2) == 0 {
            changes.push(Change::Message({
                let chat = self.some_chat();
                self.message(chat, self.clock)
            }));
        }
        changes
    }
}

/// Runs a client on its own task, sending each line it writes, with when, to the test.
fn running(mut client: Client) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let ready = client.ready().await;
            client.handle(ready).await;
        }
    })
}

/// The most of `times` in any second.
fn busiest(times: &[Instant]) -> usize {
    (0..times.len())
        .map(|i| times[i..].iter().take_while(|&&t| t < times[i] + Duration::from_secs(1)).count())
        .max()
        .unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_burst_of_2000_changes_becomes_at_most_20_batches_a_second_per_window() {
    let bed = Bed::new(Timing::default());
    let mut seeding = Vec::new();
    for n in 0..300 {
        seeding.push(Change::Message(text(
            &person(n % 30),
            &format!("s{n}"),
            1_000 + n as i64,
            Some(&person(n % 30)),
            "hi",
        )));
    }
    bed.apply(seeding).await;
    let mut watcher = bed.watcher(1024);
    let chats = watcher.watch("chats.watch", json!({"around": "top", "after": 20})).await;
    let messages = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 50})).await;
    let Watcher { client, mut lines, mut windows, .. } = watcher;
    let task = running(client);
    let collecting = tokio::spawn(async move {
        let mut got = Vec::new();
        while let Some(line) = lines.recv().await {
            got.push((Instant::now(), line));
        }
        got
    });
    let started = Instant::now();
    for n in 0..2_000i64 {
        let chat = if n % 2 == 0 { OPEN.to_string() } else { person(1 + (n as usize % 29)) };
        bed.apply(vec![Change::Message(text(&chat, &format!("b{n}"), 10_000 + n, Some(&chat), "burst"))]).await;
        // Two changes a millisecond, as 2,000 messages a second arrive.
        if n % 2 == 1 {
            tokio::time::sleep_until((started + ms(n as u64 / 2)).into()).await;
        }
    }
    tokio::time::sleep(ms(300)).await;
    task.abort();
    let _ = task.await;
    let got = collecting.await.unwrap();
    let mut times: HashMap<String, Vec<Instant>> = HashMap::new();
    for (at, line) in &got {
        let FromJawd::Push(push) = serde_json::from_slice(line).unwrap() else { panic!("a reply among the pushes") };
        let watch = push.params["watch"].as_str().unwrap().to_string();
        times.entry(watch.clone()).or_default().push(*at);
        windows.get_mut(&watch).unwrap().take(&push.params, "upsert");
    }
    let took = started.elapsed().as_secs_f64();
    for watch in [&chats, &messages] {
        let batches = &times[watch];
        // A window's batches go at least 50 ms apart, so 20 a second; they're timed here as this
        // test's task receives them, and its scheduling can bring two receipts closer, so one more.
        assert!(busiest(batches) <= 21, "{watch}: {} batches in one second", busiest(batches));
        assert!(batches.len() >= 10, "{watch}: only {} batches in {took:.1} s", batches.len());
    }
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 50, after: 0 };
    windows[&messages].assert_matches(&fresh(&bed.store, &want).await, "the messages window after the burst");
    let want = Want::Chats { archived: false, anchor: None, before: 0, after: 20 };
    windows[&chats].assert_matches(&fresh(&bed.store, &want).await, "the chat list after the burst");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_to_the_open_chat_reaches_its_conversation_before_the_chat_list() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "first", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(64);
    // Watched first, so it would be read first if the order of watching decided.
    let chats = watcher.watch("chats.watch", json!({"around": "top", "after": 20})).await;
    let messages = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 50})).await;
    let seq = bed.apply(vec![Change::Message(text(OPEN, "lone", 2_000, Some(OPEN), "on its own"))]).await;
    while watcher.seen < seq || watcher.client.pending() {
        let ready =
            tokio::time::timeout(Duration::from_secs(10), watcher.client.ready()).await.expect("nothing happened");
        if let Ready::Changes(Ok(changes)) = &ready {
            watcher.seen = watcher.seen.max(changes.seq);
        }
        watcher.client.handle(ready).await;
    }
    let mut order = Vec::new();
    while let Ok(line) = watcher.lines.try_recv() {
        let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else { panic!("a reply among the pushes") };
        order.push(push.params["watch"].as_str().unwrap().to_string());
    }
    assert_eq!(order, [messages, chats]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lone_change_after_quiet_goes_at_once_rather_than_waiting_to_coalesce() {
    // Coalescing waits of 100 ms, so a change that waited for one couldn't hide in a debug build's
    // slowness; the 5 ms budget itself is measured in a release build by jawd bench feed.
    let slow = Timing { quiet: ms(100), burst: ms(100), ..Timing::default() };
    let bed = Bed::new(slow);
    bed.apply(vec![Change::Message(text(OPEN, "first", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(64);
    watcher.watch("chats.watch", json!({"around": "top", "after": 20})).await;
    let messages = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 50})).await;
    let Watcher { client, mut lines, .. } = watcher;
    let task = running(client);
    let mut delays = Vec::new();
    for n in 0..10i64 {
        // Twice a burst, so each message comes after a quiet spell however long the last read took
        // on a busy machine: every read counts for a window's pace.
        tokio::time::sleep(ms(200)).await;
        let id = format!("lone{n}");
        bed.apply(vec![Change::Message(text(OPEN, &id, 2_000 + n, Some(OPEN), "on its own"))]).await;
        let committed = Instant::now();
        loop {
            let line = tokio::time::timeout(Duration::from_secs(1), lines.recv()).await.unwrap().unwrap();
            let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else {
                panic!("a reply among the pushes")
            };
            let carries = push.params["upsert"].as_array().into_iter().flatten().any(|row| row["id"] == id.as_str());
            if push.params["watch"] == messages.as_str() && carries {
                delays.push(committed.elapsed());
                break;
            }
        }
    }
    task.abort();
    assert!(delays.iter().all(|&delay| delay < ms(50)), "a lone message waited to coalesce: {delays:?}");
}

/// Writes 2,000 messages, four a millisecond and half of them to the open chat, each in a
/// transaction of its own, and says how long they took and the feed's seq at the end.
async fn burst(store: Store, run: &'static str, from: i64) -> (Duration, i64) {
    let started = Instant::now();
    let mut seq = 0;
    for n in 0..2_000i64 {
        let chat = if n % 2 == 0 { OPEN.to_string() } else { person(1 + (n as usize % 20)) };
        let message = text(&chat, &format!("{run}{n}"), from + n, Some(&chat), "x");
        seq = store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap().seq;
        if n % 4 == 3 {
            tokio::time::sleep(ms(1)).await;
        }
    }
    (started.elapsed(), seq)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_client_is_bounded_holds_nobody_up_and_is_told_to_watch_again() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "first", 1_000, Some(OPEN), "hi"))]).await;
    // The same writes with nobody watching, to compare with: a writer that waited on a client
    // whose queue stays full would never finish, and one slowed by it would show here.
    let (alone, _) = burst(bed.store.clone(), "a", 2_000).await;
    let room = 4;
    let mut slow = bed.watcher(room);
    let slow_chats = slow.watch("chats.watch", json!({"around": "top", "after": 10})).await;
    let slow_messages = slow.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 30})).await;
    let mut quick = bed.watcher(64);
    let quick_messages = quick.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 30})).await;

    let mut writing = tokio::spawn(burst(bed.store.clone(), "b", 10_000));
    // Both clients run; only the quick one's interface reads. The writer is watched too, since
    // two idle clients rightly never wake.
    let mut quick_during = 0;
    let (watched, seq) = loop {
        tokio::select! {
            written = &mut writing => break written.unwrap(),
            ready = slow.client.ready() => slow.client.handle(ready).await,
            ready = quick.client.ready() => {
                if let Ready::Changes(Ok(changes)) = &ready {
                    quick.seen = quick.seen.max(changes.seq);
                }
                quick.client.handle(ready).await;
            }
        }
        let before = quick.batches.values().sum::<usize>();
        quick.drain();
        if quick.batches.values().sum::<usize>() > before {
            quick_during += 1;
        }
        assert!(slow.lines.len() <= room);
    };
    assert!(quick_during >= 5, "the quick interface got {quick_during} batches while the slow one held its queue full");
    assert!(
        watched < alone * 3 + ms(250),
        "the writes took {watched:?} with a slow interface watching and {alone:?} with nobody"
    );
    quick.settle(seq).await;
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 30, after: 0 };
    quick.windows[&quick_messages].assert_matches(&fresh(&bed.store, &want).await, "the quick interface");

    // The slow interface left its queue full for longer than `behind`: its windows are dropped.
    let until = Instant::now() + brisk().behind * 2;
    while let Ok(ready) = tokio::time::timeout_at(until.into(), slow.client.ready()).await {
        slow.client.handle(ready).await;
    }
    assert_eq!(slow.lines.len(), room, "its queue stayed as full as it can be");
    assert!(slow.client.pending(), "its windows were dropped, and it's owed a resync for each");
    while slow.resynced.len() < 2 {
        slow.drain();
        slow.turn().await;
        slow.drain();
    }
    assert_eq!(slow.resynced, [slow_chats.clone(), slow_messages.clone()]);
    assert!(slow.call("unwatch", json!({"watch": slow_chats})).await.is_err(), "its windows are gone");
    let again = slow.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 30})).await;
    slow.windows[&again].assert_matches(&fresh(&bed.store, &want).await, "the slow interface, watching again");
}

#[tokio::test(flavor = "multi_thread")]
async fn while_catching_up_the_chat_list_updates_in_place_and_reorders_once_each_hold() {
    let bed = Bed::new(brisk());
    let mut seeding = Vec::new();
    for n in 0..8 {
        seeding.push(Change::Message(text(
            &person(n),
            &format!("s{n}"),
            1_000 * (n as i64 + 1),
            Some(&person(n)),
            "hi",
        )));
    }
    bed.apply(seeding).await;
    bed.catching_up.store(true, Ordering::Relaxed);
    let mut watcher = bed.watcher(64);
    let started = Instant::now();
    let chats = watcher.watch("chats.watch", json!({"around": "top", "after": 5})).await;
    let order = watcher.windows[&chats].ids();
    assert_eq!(order[0], person(7));

    bed.apply(vec![Change::Message(text(&person(4), "new", 50_000, Some(&person(4)), "moved up"))]).await;
    while watcher.windows[&chats].rows[&person(4)]["last"]["summary"] != "moved up" {
        watcher.turn().await;
        watcher.drain();
    }
    if started.elapsed() < brisk().hold {
        assert_eq!(watcher.windows[&chats].ids(), order, "the row changed in place, and the order held");
    }
    while watcher.windows[&chats].ids() == order {
        watcher.turn().await;
        watcher.drain();
    }
    assert!(started.elapsed() >= brisk().hold, "it reordered after {:?}", started.elapsed());
    let want = Want::Chats { archived: false, anchor: None, before: 0, after: 5 };
    watcher.windows[&chats].assert_matches(&fresh(&bed.store, &want).await, "the chat list once it reorders");

    bed.catching_up.store(false, Ordering::Relaxed);
    let caught_up = Instant::now();
    let seq = bed.apply(vec![Change::Message(text(&person(2), "later", 60_000, Some(&person(2)), "again"))]).await;
    watcher.settle(seq).await;
    assert!(caught_up.elapsed() < brisk().hold, "once caught up it reorders at once");
    watcher.windows[&chats].assert_matches(&fresh(&bed.store, &want).await, "the chat list once caught up");
}

#[tokio::test(flavor = "multi_thread")]
async fn watches_say_what_they_cant_do() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "m1", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(8);
    let code_of = |result: Result<Value, jaw_proto::Error>| result.unwrap_err().code;
    assert_eq!(
        code_of(watcher.call("chats.watch", json!({"folder": "work", "around": "top"})).await),
        code::INVALID_PARAMS
    );
    assert_eq!(code_of(watcher.call("chats.watch", json!({"around": "newest"})).await), code::INVALID_PARAMS);
    assert_eq!(code_of(watcher.call("chats.watch", json!({"around": "nobody@s.whatsapp.net"})).await), code::NOT_FOUND);
    assert_eq!(
        code_of(watcher.call("chats.watch", json!({"around": "top", "after": 5000})).await),
        code::INVALID_PARAMS
    );
    assert_eq!(code_of(watcher.call("messages.watch", json!({"chat": OPEN, "around": "nope"})).await), code::NOT_FOUND);
    assert_eq!(code_of(watcher.call("watch.move", json!({"watch": "w9", "around": "top"})).await), code::NOT_FOUND);
    assert_eq!(code_of(watcher.call("messages.watch", json!({"around": "newest"})).await), code::INVALID_PARAMS);

    // A chat with no messages yet is an empty window that fills when they come.
    let new = "351929999999@s.whatsapp.net";
    let empty = watcher.watch("messages.watch", json!({"chat": new, "around": "newest", "before": 10})).await;
    assert_eq!(watcher.windows[&empty].total, 0);
    let seq = bed.apply(vec![Change::Message(text(new, "hello", 2_000, Some(new), "hello"))]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&empty].ids(), ["hello"]);
}

/// jawd on a socket in `bed`'s folder, serving its feed.
fn serve(bed: &Bed) -> std::path::PathBuf {
    serve_pushing(bed).0
}

/// `serve`, with the pushes it sends every connection in the test's hands.
fn serve_pushing(bed: &Bed) -> (std::path::PathBuf, broadcast::Sender<jaw_proto::Push>) {
    let socket = jaw_proto::local::socket(bed.home.path());
    let listener = crate::sock::listen(&socket).unwrap();
    let (pushes, _) = broadcast::channel(1_024);
    let state = State::new(
        bed.home.path(),
        Err(("demo", "no engine in this test".into())),
        Vec::new(),
        Arc::default(),
        Some(bed.store.clone()),
        pushes.clone(),
    )
    .unwrap()
    .with_feed(bed.feed.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    (socket, pushes)
}

/// Pushes that aren't for windows, enough to fill jawd's queue to an interface that has stopped
/// reading, and the socket behind it.
async fn fill(pushes: &broadcast::Sender<jaw_proto::Push>) {
    let words = "x".repeat(4_000);
    for _ in 0..200 {
        let _ =
            pushes.send(jaw_proto::Push { method: "notice".into(), params: json!({"text": words, "tone": "info"}) });
        tokio::time::sleep(ms(2)).await;
    }
}

/// A raw connection: requests go out as lines, and the reply to each comes back, with any pushes
/// that came first kept.
struct Line {
    stream: std::os::unix::net::UnixStream,
    reader: BufReader<std::os::unix::net::UnixStream>,
    next: u64,
    pushes: Vec<Value>,
}

impl Line {
    fn open(socket: &std::path::Path, since: Option<u64>) -> (Line, Value) {
        let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = Line { stream, reader, next: 1, pushes: Vec::new() };
        let mut hello = json!({"protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0"});
        if let Some(since) = since {
            hello["since"] = since.into();
        }
        let welcome = line.call("hello", hello);
        (line, welcome)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.stream, "{}", json!({"id": id, "method": method, "params": params})).unwrap();
        loop {
            let value = self.read();
            if value["id"] == id {
                return value["result"].clone();
            }
            self.pushes.push(value);
        }
    }

    fn read(&mut self) -> Value {
        let mut text = String::new();
        self.reader.read_line(&mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn push(&mut self) -> Value {
        if self.pushes.is_empty() { self.read() } else { self.pushes.remove(0) }
    }

    /// Sends a request without waiting for its reply.
    fn ask(&mut self, id: u64, method: &str, params: Value) {
        writeln!(self.stream, "{}", json!({"id": id, "method": method, "params": params})).unwrap();
    }

    /// Every line that comes until none has for `quiet`.
    fn came(&mut self, quiet: Duration) -> Vec<Value> {
        self.stream.set_read_timeout(Some(quiet)).unwrap();
        let mut came = Vec::new();
        let mut text = String::new();
        while self.reader.read_line(&mut text).is_ok_and(|n| n > 0) {
            came.push(serde_json::from_str(&text).unwrap());
            text.clear();
        }
        self.stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        came
    }

    /// Every line that comes until jawd closes the connection, and whether it did, rather than
    /// sending nothing for the read timeout `open` set.
    fn until_closed(&mut self) -> (Vec<Value>, bool) {
        let mut came = Vec::new();
        let mut text = String::new();
        loop {
            match self.reader.read_line(&mut text) {
                Ok(0) => return (came, true),
                Ok(_) => came.push(serde_json::from_str(&text).unwrap()),
                Err(_) => return (came, false),
            }
            text.clear();
        }
    }
}

/// Holds every one of jaw.db's readers, and the reads asked for behind them, until it's dropped.
fn hold_readers(bed: &Bed) -> crossbeam_channel::Sender<()> {
    let (gate, waiting) = crossbeam_channel::bounded::<()>(0);
    for _ in 0..8 {
        let (waiting, reads) = (waiting.clone(), bed.store.reads().clone());
        tokio::spawn(async move {
            reads
                .query(crate::work::Priority::Interactive, move |_| {
                    let _ = waiting.recv();
                    Ok(())
                })
                .await
        });
    }
    gate
}

/// Floods `chat`, which a window of the interface shows while it reads nothing, until jawd's queue
/// to it is full: its windows then ask for no read, however many messages come. Returns the last
/// message's number.
async fn fill_window(bed: &Bed, chat: &str, sender: &str) -> i64 {
    let words = "words to fill the queue with, words to fill the queue with";
    let deadline = Instant::now() + Duration::from_secs(30);
    let (mut n, mut reads, mut since) = (0, bed.feed.reads(), Instant::now());
    loop {
        n += 1;
        bed.apply(vec![Change::Message(text(chat, &format!("p{n}"), 2_000 + n, Some(sender), words))]).await;
        tokio::time::sleep(ms(12)).await;
        let now = bed.feed.reads();
        if now != reads {
            (reads, since) = (now, Instant::now());
        } else if since.elapsed() > ms(500) {
            return n;
        }
        assert!(Instant::now() < deadline, "jawd's queue to the interface never filled");
    }
}

/// jawd on a socket in `bed`'s folder, serving its feed, and sending and finding as jawd does, its
/// engine never there.
async fn serve_all(bed: &Bed) -> std::path::PathBuf {
    serve_all_pushing(bed).await.0
}

/// `serve_all`, with the pushes it sends every connection in the test's hands.
async fn serve_all_pushing(bed: &Bed) -> (std::path::PathBuf, broadcast::Sender<jaw_proto::Push>) {
    let socket = jaw_proto::local::socket(bed.home.path());
    let listener = crate::sock::listen(&socket).unwrap();
    let (pushes, _) = broadcast::channel(1_024);
    let live: crate::sync::Shared = Arc::default();
    let attention = Arc::new(crate::attention::Attention::default());
    let timing = crate::send::Timing::default();
    let sending = crate::send::Sending::start(
        bed.store.clone(),
        None,
        live.clone(),
        &pushes,
        attention.focus(),
        timing,
        crate::send::Makers { previews: crate::send::link::Previews::off(), media: None },
    )
    .await;
    let state = State::new(
        bed.home.path(),
        Err(("demo", "no engine in this test".into())),
        Vec::new(),
        live,
        Some(bed.store.clone()),
        pushes.clone(),
    )
    .unwrap()
    .with_feed(bed.feed.clone())
    .with_attention(attention)
    .with_sending(sending.unwrap());
    let _ = state.finder.set(crate::find::Finder::start(bed.store.clone()).await.unwrap());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    (socket, pushes)
}

/// Holds jaw.db's write lock from outside until dropped, so jawd's writer waits meanwhile.
struct Locked(rusqlite::Connection);

impl Locked {
    fn on(bed: &Bed) -> Locked {
        let side = rusqlite::Connection::open(bed.home.path().join("jaw.db")).unwrap();
        side.busy_timeout(Duration::from_secs(5)).unwrap();
        side.execute_batch("BEGIN IMMEDIATE").unwrap();
        Locked(side)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = self.0.execute_batch("ROLLBACK");
    }
}

/// The open chat's draft, as jaw.db has it.
async fn draft(bed: &Bed) -> Option<String> {
    bed.store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            Ok(read::chats(db, false, None, 10)?.into_iter().find(|chat| chat.id == OPEN).and_then(|chat| chat.draft))
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn reconnecting_with_since_returns_only_what_changed() {
    let bed = Bed::new(brisk());
    let mut seeding = Vec::new();
    for n in 0..12 {
        seeding.push(Change::Message(text(&person(n), &format!("c{n}"), 1_000 + n as i64, Some(&person(n)), "hi")));
    }
    for n in 0..40 {
        let mine = n % 3 == 0;
        seeding.push(Change::Message(text(OPEN, &format!("o{n}"), 10_000 + n, (!mine).then_some(OPEN), "hello")));
    }
    let seeded = bed.apply(seeding).await;
    let socket = serve(&bed);
    let chats_want = Want::Chats { archived: false, anchor: None, before: 0, after: 5 };
    let messages_want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };

    let (first, chats, messages) = tokio::task::spawn_blocking({
        let socket = socket.clone();
        move || {
            let (mut line, welcome) = Line::open(&socket, None);
            assert_eq!(welcome["seq"].as_i64(), Some(seeded), "hello says the feed's seq");
            let mut chats = Model { chats: true, ..Model::default() };
            chats.take(&line.call("chats.watch", json!({"around": "top", "after": 5})), "rows");
            let mut messages = Model::default();
            messages
                .take(&line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10})), "rows");
            (line, chats, messages)
        }
    })
    .await
    .unwrap();
    chats.assert_matches(&fresh(&bed.store, &chats_want).await, "the first chat list");
    messages.assert_matches(&fresh(&bed.store, &messages_want).await, "the first messages window");

    // A message arrives while it's connected, and its push comes.
    bed.apply(vec![Change::Message(text(OPEN, "live", 20_000, Some(OPEN), "live one"))]).await;
    let (mut chats, mut messages) = tokio::task::spawn_blocking(move || {
        let mut first = first;
        let (mut chats, mut messages) = (chats, messages);
        let mut seen = (false, false);
        while seen != (true, true) {
            let push = first.push();
            match push["method"].as_str() {
                Some("chats") => {
                    chats.take(&push["params"], "upsert");
                    seen.0 = true;
                }
                Some("messages") => {
                    messages.take(&push["params"], "upsert");
                    seen.1 = messages.rows.contains_key("live");
                }
                _ => {}
            }
        }
        (chats, messages)
    })
    .await
    .unwrap();
    let since = chats.seq.min(messages.seq);

    // Then it's gone a while: a message is edited, one goes, two arrive, and a chat pinned meanwhile
    // comes into the top.
    let mut read = text(OPEN, "o39", 10_039, Some(OPEN), "hello");
    read.edited_at = Some(30_000);
    read.text = Some("hello, edited".into());
    read.summary = "hello, edited".into();
    bed.apply(vec![
        Change::Message(read),
        Change::DeleteMessage { chat: OPEN.into(), id: "o35".into() },
        Change::Message(text(OPEN, "new1", 40_000, Some(OPEN), "back")),
        Change::Message(text(OPEN, "new2", 40_001, None, "welcome back")),
        Change::Chat(Chat { id: person(1), pinned: Some(1), ..Chat::default() }),
    ])
    .await;

    let have = |model: &Model| model.ids();
    let (chats_have, messages_have) = (have(&chats), have(&messages));
    let (chats_result, messages_result, everything) = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, Some(since));
        let chats = line.call("chats.watch", json!({"around": "top", "after": 5, "have": chats_have}));
        let messages =
            line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10, "have": messages_have}));
        let (mut fresh_line, _) = Line::open(&socket, Some(u64::MAX / 2));
        let everything =
            fresh_line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10, "have": ["o39"]}));
        (chats, messages, everything)
    })
    .await
    .unwrap();
    let ids = |result: &Value| -> Vec<String> {
        result["rows"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
    };
    let removed = |result: &Value| -> Vec<String> {
        result["remove"].as_array().into_iter().flatten().map(|id| id.as_str().unwrap().to_string()).collect()
    };
    assert_eq!(ids(&messages_result), ["o39", "new1", "new2"], "only what changed or came in");
    assert_eq!(removed(&messages_result), ["o31", "o35"], "the one that went and the oldest, pushed out");
    assert_eq!(ids(&chats_result), [person(1), OPEN.into()], "the pinned chat, and the open chat's newest message");
    assert_eq!(removed(&chats_result), [person(8)]);
    chats.take(&chats_result, "rows");
    messages.take(&messages_result, "rows");
    chats.assert_matches(&fresh(&bed.store, &chats_want).await, "the chat list, reconnected");
    messages.assert_matches(&fresh(&bed.store, &messages_want).await, "the messages window, reconnected");
    assert_eq!(ids(&everything).len(), 10, "a since past the feed's last is from another jaw.db, so everything comes");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_that_makes_two_voters_one_reaches_a_window_of_their_poll_and_a_reconnect_from_before_it() {
    let bed = Bed::new(brisk());
    let (rui, ana, ana_lid) = ("351910000003@s.whatsapp.net", "351910000002@s.whatsapp.net", "100000000002@lid");
    let poll = Message {
        chat: GROUP.into(),
        id: "p1".into(),
        at: 1_000,
        sender: Some(rui.into()),
        kind: "poll",
        summary: "Poll: Jantar?".into(),
        detail: Some(json!({"poll": {"question": "Jantar?", "options": [], "max": 1, "voters": 0}}).to_string()),
        poll: vec!["Ramen".into(), "Pizza".into()],
        ..Message::default()
    };
    let vote = |voter: &str, at: i64, id: &str, chose: &str| {
        let hex = crate::store::polls::option_hash(chose).iter().map(|b| format!("{b:02x}")).collect();
        Change::Amend(Amend {
            chat: GROUP.into(),
            target: "p1".into(),
            sender: Some(voter.into()),
            mine: false,
            at,
            what: Amendment::Vote { options: vec![hex], id: id.into() },
            raw: None,
        })
    };
    // Rui's poll, which Ana votes in from her number and then from her LID, choosing again, while
    // jaw knows the two as two people; the group shows none of her messages.
    bed.apply(vec![Change::Message(poll), vote(ana, 2_000, "a1", "Ramen"), vote(ana_lid, 3_000, "a2", "Pizza")]).await;
    let socket = serve(&bed);
    let voters = |row: &Value| row["poll"]["voters"].as_u64();
    let (line, held) = tokio::task::spawn_blocking({
        let socket = socket.clone();
        move || {
            let (mut line, _) = Line::open(&socket, None);
            let mut held = Model::default();
            held.take(&line.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 10})), "rows");
            (line, held)
        }
    })
    .await
    .unwrap();
    assert_eq!(voters(&held.rows["p1"]), Some(2), "two people, as jaw knows them");
    let since = held.seq;

    bed.apply(vec![Change::Person(Person { jids: vec![ana.into(), ana_lid.into()], ..Person::default() })]).await;
    let pushed = tokio::task::spawn_blocking(move || {
        let mut line = line;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            for push in line.came(ms(100)) {
                let rows = push["params"]["upsert"].as_array().cloned().unwrap_or_default();
                if let Some(poll) = rows.into_iter().find(|row| row["id"] == "p1") {
                    return Some(poll);
                }
            }
        }
        None
    })
    .await
    .unwrap();
    let pushed = pushed.expect("the window is told of the poll's new tally");
    assert_eq!(voters(&pushed), Some(1), "one person now: {pushed}");
    assert!(pushed["v"].as_u64() > held.rows["p1"]["v"].as_u64(), "at a new version");

    let have = held.ids();
    let again = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, Some(since));
        line.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 10, "have": have}))
    })
    .await
    .unwrap();
    let rows = again["rows"].as_array().unwrap();
    let poll =
        rows.iter().find(|row| row["id"] == "p1").expect("a reconnect from before the join reads the poll again");
    assert_eq!((voters(poll), &poll["poll"]["options"][1]["votes"]), (Some(1), &json!(1)), "her Pizza alone: {poll}");
}

#[tokio::test(flavor = "multi_thread")]
async fn changes_a_window_doesnt_show_cost_it_at_most_one_read_each_burst() {
    let bed = Bed::new(Timing::default());
    let far = |n: usize| format!("35193{n:07}@s.whatsapp.net");
    let near = |n: usize| format!("35194{n:07}@s.whatsapp.net");
    let mut seeding = Vec::new();
    for n in 0..250 {
        seeding.push(Change::Message(text(&far(n), &format!("f{n}"), 1_000 + n as i64, Some(&far(n)), "hi")));
    }
    // Enough newer chats that the far ones never reach the top 20.
    for n in 0..30 {
        seeding.push(Change::Message(text(&near(n), &format!("n{n}"), 500_000 + n as i64, Some(&near(n)), "hi")));
        seeding.push(Change::Message(text(OPEN, &format!("o{n}"), 900_000 + n as i64, Some(OPEN), "hi")));
    }
    bed.apply(seeding).await;
    let mut watcher = bed.watcher(1024);
    let chats = watcher.watch("chats.watch", json!({"around": "top", "after": 20})).await;
    let messages = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 20})).await;
    let Watcher { client, mut lines, mut windows, .. } = watcher;
    let task = running(client);
    let collecting = tokio::spawn(async move {
        let mut got = Vec::new();
        while let Some(line) = lines.recv().await {
            got.push(line);
        }
        got
    });
    let reads = bed.feed.reads();
    let started = Instant::now();
    for n in 0..2_000usize {
        // Unread counts and names of chats far down the list: each touches the chat list, and none
        // changes what it shows.
        let change = if n % 2 == 0 {
            Change::Read(Read {
                chat: far(n % 250),
                at: 1,
                ids: Vec::new(),
                phone: Some(((n % 7) as u32, 0)),
                here: false,
            })
        } else {
            Change::Person(Person { jids: vec![far(n % 250)], name: Some(format!("Someone {n}")), ..Person::default() })
        };
        bed.apply(vec![change]).await;
        if n % 2 == 1 {
            tokio::time::sleep_until((started + ms(n as u64 / 2)).into()).await;
        }
    }
    tokio::time::sleep(ms(100)).await;
    let took = started.elapsed();
    let read = bed.feed.reads() - reads;
    // One window is touched, so at most one read each 50 ms, and one at once after the quiet.
    let most = (took.as_secs_f64() * 20.0).ceil() as u64 + 2;
    assert!(read <= most, "{read} window reads in {took:?}, where {most} would do");
    assert!(read >= 5, "the chat list was read only {read} times while its chats changed");
    task.abort();
    let _ = task.await;
    for line in collecting.await.unwrap() {
        let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else { panic!("a reply among the pushes") };
        let watch = push.params["watch"].as_str().unwrap().to_string();
        windows.get_mut(&watch).unwrap().take(&push.params, "upsert");
    }
    let want = Want::Chats { archived: false, anchor: None, before: 0, after: 20 };
    windows[&chats].assert_matches(&fresh(&bed.store, &want).await, "the chat list");
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 20, after: 0 };
    windows[&messages].assert_matches(&fresh(&bed.store, &want).await, "the open chat");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_whose_read_fails_is_read_again_at_its_pace_then_resynced() {
    let bed = Bed::new(brisk());
    let seeding = (0..20).map(|n| Change::Message(text(OPEN, &format!("o{n}"), 1_000 + n, Some(OPEN), "hi")));
    bed.apply(seeding.collect()).await;
    let mut watcher = bed.watcher(64);
    let params = json!({"chat": OPEN, "around": "newest", "before": 10});
    let watch = watcher.watch("messages.watch", params.clone()).await;
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    // A row the window shows that can't be read, written behind the store's back.
    let side = rusqlite::Connection::open(bed.home.path().join("jaw.db")).unwrap();
    side.busy_timeout(Duration::from_secs(5)).unwrap();
    // The schema's CHECK (json_valid(spans)) keeps the store from writing such a row.
    side.pragma_update(None, "ignore_check_constraints", true).unwrap();
    let spans = |spans: Option<&str>| {
        side.execute("UPDATE messages SET spans = ?1 WHERE id = 'o15'", [spans]).unwrap();
    };

    spans(Some("{"));
    bed.apply(vec![Change::Message(text(OPEN, "new1", 5_000, Some(OPEN), "while it can't be read"))]).await;
    let reads = bed.feed.reads();
    let trying = brisk().behind / 2;
    watcher.run_for(trying).await;
    let tried = bed.feed.reads() - reads;
    let most = (trying.as_millis() / brisk().burst.as_millis()) as u64 + 2;
    assert!((2..=most).contains(&tried), "{tried} reads in {trying:?}: at least twice, and no faster than its pace");
    assert!(!watcher.windows[&watch].rows.contains_key("new1"), "nothing could be sent");
    assert!(watcher.resynced.is_empty(), "it hasn't failed for long");

    // Mended in time, the window catches up with what it owed.
    spans(None);
    watcher.run_for(ms(100)).await;
    watcher.windows[&watch].assert_matches(&fresh(&bed.store, &want).await, "the window once its rows read again");
    assert!(watcher.resynced.is_empty());

    // Broken for longer than `behind`, the interface is told to watch again, and watching again says why.
    spans(Some("{"));
    let broke = Instant::now();
    bed.apply(vec![Change::Message(text(OPEN, "new2", 6_000, Some(OPEN), "still can't be read"))]).await;
    while watcher.resynced.is_empty() {
        watcher.turn().await;
        watcher.drain();
    }
    assert!(broke.elapsed() >= brisk().behind, "resynced after {:?}", broke.elapsed());
    assert_eq!(watcher.resynced, [watch]);
    let refused = watcher.call("messages.watch", params).await.unwrap_err();
    assert_eq!(refused.code, code::INTERNAL, "{}", refused.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_match_fresh_queries_while_writes_race_their_reads() {
    for seed in 1..=8u64 {
        let bed = Bed::new(brisk());
        let mut world = World::new(seed + 1000);
        let mut seeding: Vec<Change> =
            world.chats.iter().map(|chat| Change::Chat(Chat { id: chat.clone(), ..Chat::default() })).collect();
        seeding.extend((0..60).flat_map(|_| world.new_message_or_two()));
        bed.apply(seeding).await;
        let mut watcher = bed.watcher(64);
        let wants = [
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: Some(person(3)), before: 2, after: 2 },
            Want::Chats { archived: true, anchor: None, before: 0, after: 3 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed.store, OPEN, 8).await, before: 3, after: 3 },
            Want::Messages { chat: GROUP.into(), anchor: None, before: 4, after: 0 },
        ];
        let watched = watch_all(&mut watcher, &wants).await;
        let (paused, mut pauses) = mpsc::channel::<i64>(1);
        let (go, mut going) = mpsc::channel::<()>(1);
        let store = bed.store.clone();
        let writer = tokio::spawn(async move {
            for _ in 0..25 {
                let mut seq = 0;
                for _ in 0..1 + world.rng.below(30) {
                    seq = store.apply(Lane::Live, world.change()).await.unwrap().seq;
                    // Mostly back to back, sometimes after a quiet spell longer than a burst, so the
                    // next change goes at once while the client may still be reading for the last.
                    match world.rng.below(6) {
                        0 => tokio::time::sleep(ms(12 + world.rng.below(10) as u64)).await,
                        1 => tokio::time::sleep(ms(world.rng.below(3) as u64)).await,
                        _ => tokio::task::yield_now().await,
                    }
                }
                paused.send(seq).await.unwrap();
                going.recv().await.unwrap();
            }
        });
        loop {
            tokio::select! {
                ready = watcher.client.ready() => {
                    if let Ready::Changes(Ok(changes)) = &ready {
                        watcher.seen = watcher.seen.max(changes.seq);
                    }
                    watcher.client.handle(ready).await;
                    watcher.drain();
                }
                pause = pauses.recv() => match pause {
                    Some(seq) => {
                        watcher.settle(seq).await;
                        check(&bed, &watcher, &watched, &format!("seed {seed}, paused at {seq}")).await;
                        go.send(()).await.unwrap();
                    }
                    None => break,
                },
            }
        }
        writer.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn identical_windows_on_several_connections_share_each_read() {
    let bed = Bed::new(brisk());
    let mut seeding: Vec<Change> =
        (0..20).map(|n| Change::Message(text(OPEN, &format!("o{n}"), 1_000 + n, Some(OPEN), "hi"))).collect();
    seeding.extend((0..5).map(|n| {
        let who = person(n);
        Change::Message(text(&who, &format!("p{n}"), 500 + n as i64, Some(&who), "hello"))
    }));
    bed.apply(seeding).await;
    let chats = Want::Chats { archived: false, anchor: None, before: 0, after: 10 };
    let open = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let shorter = Want::Messages { chat: OPEN.into(), anchor: None, before: 5, after: 0 };
    // Three connections showing the same windows, and a fourth with the same chat list and a
    // shorter stretch of the open chat.
    let mut watchers = Vec::new();
    for wants in [[&chats, &open], [&chats, &open], [&chats, &open], [&chats, &shorter]] {
        let mut watcher = bed.watcher(64);
        let watched = watch_all(&mut watcher, &wants.map(Want::clone)).await;
        watchers.push((watcher, watched));
    }
    let new = |id: &str, at: i64| vec![Change::Message(text(OPEN, id, at, Some(OPEN), "to everyone"))];

    let reads = bed.feed.reads();
    let seq = bed.apply(new("new1", 5_000)).await;
    for (watcher, watched) in &mut watchers {
        watcher.settle(seq).await;
        check(&bed, watcher, watched, "after one message").await;
    }
    assert_eq!(bed.feed.reads() - reads, 3, "one read of the chat list and one of each stretch of the open chat");

    // A connection that has heard of a change the last read doesn't have reads again, and the
    // others take its read.
    let reads = bed.feed.reads();
    let seq = bed.apply(new("new2", 6_000)).await;
    watchers[1].0.settle(seq).await;
    let seq = bed.apply(new("new3", 7_000)).await;
    watchers[2].0.settle(seq).await;
    assert_eq!(bed.feed.reads() - reads, 4, "the second connection read for new2, the third again for new3");
    for (watcher, watched) in &mut watchers {
        watcher.settle(seq).await;
        check(&bed, watcher, watched, "after two more").await;
        assert!(watcher.windows.values().any(|model| model.rows.contains_key("new3")));
    }
    assert_eq!(bed.feed.reads() - reads, 5, "the rest took the third's reads, but for the shorter stretch");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_takes_another_ones_read_only_if_it_has_every_change_heard_of() {
    // Reads 400 ms apart, so one connection's window waits for its next read while another's reads.
    let timing = Timing { quiet: ms(1), burst: ms(400), ..brisk() };
    let bed = Bed::new(timing);
    let (known, new) = (person(DIRECT), person(DIRECT + 2));
    bed.apply(vec![Change::Message(text(GROUP, "g1", 1_000, Some(&known), "hi"))]).await;
    let want = Want::Messages { chat: GROUP.into(), anchor: None, before: 10, after: 0 };
    let (mut waiting, mut reading) = (bed.watcher(64), bed.watcher(64));
    let waiting_watched = watch_all(&mut waiting, std::slice::from_ref(&want)).await;
    let reading_watched = watch_all(&mut reading, &[want]).await;

    // One connection reads the group, and a burst later the other takes that read, which keeps it
    // from reading again for a burst.
    let seq = bed.apply(vec![Change::Message(text(GROUP, "g2", 2_000, Some(&known), "again"))]).await;
    reading.settle(seq).await;
    tokio::time::sleep(timing.burst + ms(50)).await;
    waiting.settle(seq).await;

    // Someone new writes, and the connection free to read reads their message; their name comes a
    // moment later, which doesn't touch the group as the waiting connection holds it, since they
    // hadn't written there. Its window has to come with the name all the same.
    let seq = bed.apply(vec![Change::Message(text(GROUP, "g3", 3_000, Some(&new), "I'm new here"))]).await;
    reading.settle(seq).await;
    let named = Person { jids: vec![new.clone()], name: Some("Rita Sousa".into()), ..Person::default() };
    let seq = bed.apply(vec![Change::Person(named)]).await;
    waiting.settle(seq).await;
    check(&bed, &waiting, &waiting_watched, "the connection that waited for its read").await;
    reading.settle(seq).await;
    check(&bed, &reading, &reading_watched, "the connection that read first").await;
}

/// The newest `before` messages in the group.
fn group(before: usize) -> Want {
    Want::Messages { chat: GROUP.into(), anchor: None, before, after: 0 }
}

// The tests of names make their connections before they write, so settling waits for what was
// written to come to them, and only the change a test makes next can mark a window.

#[tokio::test(flavor = "multi_thread")]
async fn a_window_is_told_the_new_name_of_someone_its_rows_only_mention() {
    let bed = Bed::new(brisk());
    let (writer, mentioned) = (person(DIRECT), person(DIRECT + 1));
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&mentioned, "Bea Lopes")),
            Change::Message(mentioning(GROUP, "g1", 1_000, &writer, &mentioned)),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@Bea Lopes");

    let seq = bed.apply(vec![Change::Person(named(&mentioned, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@Bea Martins");
    check(&bed, &watcher, &watched, "once someone only mentioned is renamed").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_is_told_the_new_name_of_someone_its_rows_only_quote() {
    let bed = Bed::new(brisk());
    let (writer, quoted) = (person(DIRECT), person(DIRECT + 1));
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&quoted, "Bea Lopes")),
            Change::Message(text(GROUP, "q1", 1_000, Some(&quoted), "Saturday at 9?")),
            Change::Message(quoting(GROUP, "g1", 2_000, &writer, "q1", &quoted)),
            Change::Message(text(GROUP, "g2", 3_000, Some(&writer), "or Sunday")),
        ])
        .await;
    // The window holds the two newest, so whoever is quoted wrote none of its rows.
    let watched = watch_all(&mut watcher, &[group(2)]).await;
    watcher.settle(seq).await;
    assert!(!watcher.windows[&watched[0].0].rows.contains_key("q1"));
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["reply"]["from"], "Bea Lopes");

    let seq = bed.apply(vec![Change::Person(named(&quoted, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["reply"]["from"], "Bea Martins");
    check(&bed, &watcher, &watched, "once someone only quoted is renamed").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_is_told_the_name_of_someone_its_rows_mention_once_jaw_learns_who_they_are() {
    let bed = Bed::new(brisk());
    let (writer, stranger) = (person(DIRECT), person(DIRECT + 1));
    let mut watcher = bed.watcher(64);
    let seq = bed.apply(vec![Change::Message(mentioning(GROUP, "g1", 1_000, &writer, &stranger))]).await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@351910000009");

    // No one had that JID, so the person jaw makes for it is one no window could have known of.
    let seq = bed.apply(vec![Change::Person(named(&stranger, "Cris Neves"))]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@Cris Neves");
    check(&bed, &watcher, &watched, "once someone mentioned is someone jaw knows").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_is_told_the_name_its_mention_takes_when_a_number_and_a_lid_turn_out_one_person() {
    let bed = Bed::new(brisk());
    let (writer, number, lid) = (person(DIRECT), person(DIRECT + 1), "84726193520417@lid".to_string());
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(Person { jids: vec![lid.clone()], ..Person::default() }),
            Change::Person(named(&number, "Dora Reis")),
            Change::Message(mentioning(GROUP, "g1", 1_000, &writer, &lid)),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@84726193520417");

    let joined = Person { jids: vec![number.clone(), lid.clone()], ..Person::default() };
    let seq = bed.apply(vec![Change::Person(joined)]).await;
    watcher.settle(seq).await;
    assert_eq!(watcher.windows[&watched[0].0].rows["g1"]["spans"][1]["s"], "@Dora Reis");
    check(&bed, &watcher, &watched, "once the LID mentioned is joined to a named number").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_holding_a_name_since_changed_is_not_taken_by_a_connection_that_heard_of_the_change() {
    // Reads 400 ms apart, so one connection's window waits for its next read while another's reads.
    let timing = Timing { quiet: ms(1), burst: ms(400), ..brisk() };
    let bed = Bed::new(timing);
    let (writer, mentioned) = (person(DIRECT), person(DIRECT + 1));
    let (mut waiting, mut reading) = (bed.watcher(64), bed.watcher(64));
    bed.apply(vec![
        Change::Person(named(&mentioned, "Bea Lopes")),
        Change::Message(text(GROUP, "g1", 1_000, Some(&writer), "hi")),
    ])
    .await;
    let waiting_watched = watch_all(&mut waiting, &[group(10)]).await;
    let reading_watched = watch_all(&mut reading, &[group(10)]).await;

    // One connection reads the group, and a burst later the other takes that read, which keeps it
    // from reading again for a burst.
    let seq = bed.apply(vec![Change::Message(text(GROUP, "g2", 2_000, Some(&writer), "again"))]).await;
    reading.settle(seq).await;
    tokio::time::sleep(timing.burst + ms(50)).await;
    waiting.settle(seq).await;

    // A message mentions her, and the connection free to read reads it with her old name. She's
    // renamed a moment later, which doesn't touch the group as the waiting connection holds it,
    // since none of its rows name her; the read with her old name is no use to it all the same.
    let seq = bed.apply(vec![Change::Message(mentioning(GROUP, "g3", 3_000, &writer, &mentioned))]).await;
    reading.settle(seq).await;
    assert_eq!(reading.windows[&reading_watched[0].0].rows["g3"]["spans"][1]["s"], "@Bea Lopes");
    let seq = bed.apply(vec![Change::Person(named(&mentioned, "Bea Martins"))]).await;
    waiting.settle(seq).await;
    assert_eq!(waiting.windows[&waiting_watched[0].0].rows["g3"]["spans"][1]["s"], "@Bea Martins");
    check(&bed, &waiting, &waiting_watched, "the connection that waited for its read").await;
    reading.settle(seq).await;
    check(&bed, &reading, &reading_watched, "the connection that read the old name").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_reads_again_only_the_windows_whose_rows_name_whoever_was_renamed() {
    let bed = Bed::new(brisk());
    let (writer, mentioned, other) = (person(DIRECT), person(DIRECT + 1), person(DIRECT + 2));
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&mentioned, "Bea Lopes")),
            Change::Person(named(&other, "Rui Lopes")),
            Change::Message(mentioning(GROUP, "g1", 1_000, &writer, &mentioned)),
            Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hello")),
        ])
        .await;
    let open = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let watched = watch_all(&mut watcher, &[group(10), open]).await;
    watcher.settle(seq).await;
    let before = bed.feed.reads();

    let seq = bed.apply(vec![Change::Person(named(&other, "Rui Martins"))]).await;
    watcher.settle(seq).await;
    assert_eq!(bed.feed.reads(), before, "no window's rows name him");
    let seq = bed.apply(vec![Change::Person(named(&mentioned, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    assert_eq!(bed.feed.reads(), before + 1, "only the group's rows name her");
    check(&bed, &watcher, &watched, "once she's renamed").await;
}

/// A new connection watching what `was` watched again, as jaw does after a reconnect or a resync:
/// messages windows from the seq and the ids their interface holds, chat lists whole.
async fn reconnected(bed: &Bed, was: &Watcher, watched: &[(String, Want)]) -> (Watcher, Vec<(String, Want)>) {
    let mut again = bed.watcher(64);
    let mut now = Vec::new();
    for (watch, want) in watched {
        let (method, mut params) = want.params();
        let mut model = match want {
            Want::Messages { .. } => {
                let held = was.windows[watch].clone();
                params["since"] = held.seq.into();
                params["have"] = json!(held.ids());
                held
            }
            Want::Chats { .. } => Model { chats: true, ..Model::default() },
        };
        let result = again.client.call(method, params).await.unwrap();
        model.take(&result, "rows");
        let watch = result["watch"].as_str().unwrap().to_string();
        again.windows.insert(watch.clone(), model);
        now.push((watch, want.clone()));
    }
    (again, now)
}

/// A window holding the row `row` makes from a writer and whoever is renamed loses its connection,
/// she's renamed, and the window is watched again from its seq.
async fn watched_again_after_a_rename(row: fn(&str, &str) -> Message) {
    let bed = Bed::new(brisk());
    let (writer, renamed) = (person(DIRECT), person(DIRECT + 1));
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&renamed, "Bea Lopes")),
            Change::Person(named(&writer, "Rui")),
            Change::Message(row(&writer, &renamed)),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    check(&bed, &watcher, &watched, "as watched").await;

    // The connection takes nothing more from here.
    let renaming = bed.apply(vec![Change::Person(named(&renamed, "Bea Martins"))]).await;
    let (again, watched) = reconnected(&bed, &watcher, &watched).await;
    check(&bed, &again, &watched, "watched again from its seq after the rename").await;
    assert_eq!(again.windows[&watched[0].0].rows["g1"]["v"], renaming, "the row's version is the rename's");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_from_its_seq_shows_the_new_name_of_someone_its_rows_only_mention() {
    watched_again_after_a_rename(|writer, renamed| mentioning(GROUP, "g1", 2_000, writer, renamed)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_from_its_seq_shows_the_new_name_of_someone_its_rows_only_quote() {
    // What it quotes isn't in jaw.db, so whoever wrote that sent none of the window's rows.
    watched_again_after_a_rename(|writer, renamed| quoting(GROUP, "g1", 2_000, writer, "q0", renamed)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_from_its_seq_shows_the_new_name_of_whoever_sent_its_rows() {
    watched_again_after_a_rename(|_, renamed| text(GROUP, "g1", 2_000, Some(renamed), "hi")).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rename_reaches_a_window_when_one_of_the_many_people_renamed_is_one_of_the_many_its_rows_name() {
    let bed = Bed::new(brisk());
    let writer = person(DIRECT);
    let mentioned: Vec<String> = (1..=6).map(|k| person(DIRECT + k)).collect();
    let strangers: Vec<String> = (0..42).map(|k| format!("3519200001{k:02}@s.whatsapp.net")).collect();
    let mut watcher = bed.watcher(64);
    let mut writes: Vec<Change> =
        mentioned.iter().chain(&strangers).map(|jid| Change::Person(named(jid, "Someone"))).collect();
    for (k, whom) in mentioned.iter().enumerate() {
        writes.push(Change::Message(mentioning(GROUP, &format!("g{k}"), 1_000 + k as i64, &writer, whom)));
    }
    let seq = bed.apply(writes).await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;

    // The window names six JIDs, and each commit renames one of the six with seven people no row
    // names, so both sides of the meeting have several JIDs and share one: looking up only some
    // of the smaller side's would miss it in most commits.
    for (k, whom) in mentioned.iter().enumerate() {
        let mut renames = vec![Change::Person(named(whom, &format!("Bea {k}")))];
        renames.extend(strangers[k * 7..k * 7 + 7].iter().map(|jid| Change::Person(named(jid, &format!("Rui {k}")))));
        let seq = bed.apply(renames).await;
        watcher.settle(seq).await;
        let row = &watcher.windows[&watched[0].0].rows[&format!("g{k}")];
        assert_eq!(row["spans"][1]["s"], format!("@Bea {k}"), "the {k}th mentioned");
        check(&bed, &watcher, &watched, &format!("once the {k}th mentioned is renamed among seven others")).await;
    }
}

/// How many people the account of names has, each with a number and a LID.
const NAMED_PEOPLE: usize = 6;

fn number_of(k: usize) -> String {
    format!("3519200000{k:02}@s.whatsapp.net")
}

fn lid_of(k: usize) -> String {
    format!("{}@lid", 84_726_193_520_000u64 + k as u64)
}

fn named_groups() -> Vec<String> {
    (0..3).map(|g| format!("1203630000000001{g:02}@g.us")).collect()
}

/// A made-up account whose people each have a number and a LID, send, are mentioned and are
/// quoted by either, are renamed by either, and now and then are found to be one person.
struct Names {
    rng: Rng,
    clock: i64,
    next: u64,
    sent: Vec<Message>,
}

impl Names {
    fn jid(&mut self) -> String {
        let k = self.rng.below(NAMED_PEOPLE);
        if self.rng.below(2) == 0 { number_of(k) } else { lid_of(k) }
    }

    fn message(&mut self) -> Change {
        self.clock += 1 + self.rng.below(2_000) as i64;
        self.next += 1;
        let chat = self.rng.pick(&named_groups()).clone();
        let sender = self.jid();
        let mut message = text(&chat, &format!("a{}", self.next), self.clock, Some(&sender), "words");
        match self.rng.below(3) {
            0 => {
                let whom = self.jid();
                let number = whom.split('@').next().unwrap().to_string();
                let spans =
                    json!([{"t": "text", "s": "hey "}, {"t": "mention", "s": format!("@{number}"), "person": whom}]);
                message.spans = Some(spans.to_string());
            }
            1 if !self.sent.is_empty() => {
                let quoted = self.rng.pick(&self.sent).clone();
                let reply = json!({"id": quoted.id, "from": quoted.sender, "summary": quoted.summary});
                message.reply = Some(reply.to_string());
            }
            _ => {}
        }
        self.sent.push(message.clone());
        Change::Message(message)
    }

    fn change(&mut self) -> Change {
        match self.rng.below(10) {
            0..=3 => self.message(),
            4..=5 => {
                let jid = self.jid();
                let n = self.rng.next() % 100;
                let person = match self.rng.below(3) {
                    0 => Person { jids: vec![jid], name: Some(format!("Name {n}")), ..Person::default() },
                    1 => Person { jids: vec![jid], push_name: Some(format!("push {n}")), ..Person::default() },
                    _ => Person { jids: vec![jid], verified_name: Some(format!("Biz {n}")), ..Person::default() },
                };
                Change::Person(person)
            }
            6 => {
                let k = self.rng.below(NAMED_PEOPLE);
                Change::Person(Person { jids: vec![number_of(k), lid_of(k)], ..Person::default() })
            }
            7 if !self.sent.is_empty() => {
                let k = self.rng.below(self.sent.len());
                let whom = self.jid();
                let mut edited = self.sent[k].clone();
                self.clock += 1;
                edited.edited_at = Some(self.clock);
                edited.text = Some(format!("edited {}", self.clock));
                edited.spans = (self.rng.below(2) == 0).then(|| {
                    let number = whom.split('@').next().unwrap().to_string();
                    json!([{"t": "mention", "s": format!("@{number}"), "person": whom}]).to_string()
                });
                self.sent[k] = edited.clone();
                Change::Message(edited)
            }
            8 if !self.sent.is_empty() => {
                let gone = self.sent.swap_remove(self.rng.below(self.sent.len()));
                Change::DeleteMessage { chat: gone.chat, id: gone.id }
            }
            _ => self.message(),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_match_fresh_queries_through_renames_joins_new_people_and_edits() {
    for seed in 1..=10u64 {
        let bed = Bed::new(brisk());
        let rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut names = Names { rng, clock: 1_700_000_000_000, next: 0, sent: Vec::new() };
        let seeding: Vec<Change> = (0..30).map(|_| names.message()).collect();
        bed.apply(seeding).await;
        let mut wants: Vec<Want> =
            named_groups().into_iter().map(|chat| Want::Messages { chat, anchor: None, before: 6, after: 0 }).collect();
        wants.push(Want::Chats { archived: false, anchor: None, before: 0, after: 5 });
        // Two connections on the same windows, the second settling only now and then, so it takes
        // the first's kept reads, and now and then reconnecting instead, from what it last held.
        let (mut first, mut second) = (bed.watcher(64), bed.watcher(64));
        let first_watched = watch_all(&mut first, &wants).await;
        let mut second_watched = watch_all(&mut second, &wants).await;
        let mut reconnects = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
        for step in 0..150 {
            let change = names.change();
            let seq = bed.apply(vec![change]).await;
            first.settle(seq).await;
            check(&bed, &first, &first_watched, &format!("seed {seed}, step {step}, the first")).await;
            if names.rng.below(4) == 0 {
                second.settle(seq).await;
                check(&bed, &second, &second_watched, &format!("seed {seed}, step {step}, the second")).await;
            } else if reconnects.below(8) == 0 {
                (second, second_watched) = reconnected(&bed, &second, &second_watched).await;
                let what = format!("seed {seed}, step {step}, the second reconnected");
                check(&bed, &second, &second_watched, &what).await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_watched_again_shows_the_new_name_of_the_older_of_two_people_a_row_mentions() {
    let bed = Bed::new(brisk());
    let (writer, ana, bea) = (person(DIRECT), person(DIRECT + 1), person(DIRECT + 2));
    // One row mentions Ana first and the other Bea first, so neither the first mention's version
    // nor the last's passes for the newest.
    let both = |first: &str, second: &str| {
        json!([
            {"t": "mention", "s": "@a", "person": first}, {"t": "text", "s": " and "},
            {"t": "mention", "s": "@b", "person": second}
        ])
        .to_string()
    };
    let ab = Message { spans: Some(both(&ana, &bea)), ..text(GROUP, "g1", 2_000, Some(&writer), "@a and @b") };
    let ba = Message { spans: Some(both(&bea, &ana)), ..text(GROUP, "g2", 2_001, Some(&writer), "@b and @a") };
    let mut watcher = bed.watcher(64);
    let seq = bed
        .apply(vec![
            Change::Person(named(&ana, "Ana Lopes")),
            Change::Person(named(&bea, "Bea Lopes")),
            Change::Person(named(&writer, "Rui")),
            Change::Message(ab),
            Change::Message(ba),
        ])
        .await;
    let watched = watch_all(&mut watcher, &[group(10)]).await;
    watcher.settle(seq).await;
    let seq = bed.apply(vec![Change::Person(named(&bea, "Bea Martins"))]).await;
    watcher.settle(seq).await;
    check(&bed, &watcher, &watched, "Bea renamed").await;

    // The connection goes, and Ana, the older of the two, is renamed.
    let renaming = bed.apply(vec![Change::Person(named(&ana, "Ana Martins"))]).await;
    let (again, watched) = reconnected(&bed, &watcher, &watched).await;
    check(&bed, &again, &watched, "watched again after Ana's rename").await;
    for id in ["g1", "g2"] {
        assert_eq!(again.windows[&watched[0].0].rows[id]["v"], renaming, "{id}'s version is Ana's rename's");
    }
}

/// How many people the account of versions has, each with a number and a LID.
const VERSIONED_PEOPLE: usize = 10;

fn versioned_number(k: usize) -> String {
    format!("3519400000{k:02}@s.whatsapp.net")
}

fn versioned_lid(k: usize) -> String {
    format!("{}@lid", 84_726_193_530_000u64 + k as u64)
}

fn versioned_groups() -> Vec<String> {
    (0..3).map(|g| format!("1203630000000003{g:02}@g.us")).collect()
}

/// A made-up account whose people each have a number and a LID; its rows mention up to three of
/// them and quote what was sent or someone jaw may not know yet, and, when `direct`, some go to
/// one-to-one chats, which merge when a number and a LID are found to be one person.
struct Versions {
    rng: Rng,
    clock: i64,
    next: u64,
    sent: Vec<Message>,
    direct: bool,
}

impl Versions {
    fn new(seed: u64, direct: bool) -> Versions {
        let rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        Versions { rng, clock: 1_700_000_000_000, next: 0, sent: Vec::new(), direct }
    }

    fn jid(&mut self) -> String {
        let k = self.rng.below(VERSIONED_PEOPLE);
        if self.rng.below(2) == 0 { versioned_number(k) } else { versioned_lid(k) }
    }

    fn spans(&mut self) -> Option<String> {
        let many = self.rng.below(4);
        if many == 0 {
            return None;
        }
        let mut spans = vec![json!({"t": "text", "s": "hey "})];
        for _ in 0..many {
            let whom = self.jid();
            let number = whom.split('@').next().unwrap().to_string();
            spans.push(json!({"t": "mention", "s": format!("@{number}"), "person": whom}));
            spans.push(json!({"t": "text", "s": " "}));
        }
        Some(Value::Array(spans).to_string())
    }

    fn message(&mut self) -> Change {
        self.clock += 1 + self.rng.below(2_000) as i64;
        self.next += 1;
        let sender = self.jid();
        let chat = if self.direct && self.rng.below(3) == 0 {
            sender.clone()
        } else {
            self.rng.pick(&versioned_groups()).clone()
        };
        let mut message = text(&chat, &format!("b{}", self.next), self.clock, Some(&sender), "words");
        message.spans = self.spans();
        if self.rng.below(3) == 0 {
            let (id, from) = match self.rng.below(2) {
                0 if !self.sent.is_empty() => {
                    let quoted = self.rng.pick(&self.sent).clone();
                    (quoted.id, quoted.sender)
                }
                _ => ("elsewhere".to_string(), Some(self.jid())),
            };
            message.reply = Some(json!({"id": id, "from": from, "summary": "quoted"}).to_string());
        }
        self.sent.push(message.clone());
        Change::Message(message)
    }

    fn change(&mut self) -> Change {
        match self.rng.below(12) {
            0..=3 => self.message(),
            4..=6 => {
                let jid = self.jid();
                let n = self.rng.next() % 100;
                Change::Person(match self.rng.below(3) {
                    0 => Person { jids: vec![jid], name: Some(format!("Name {n}")), ..Person::default() },
                    1 => Person { jids: vec![jid], push_name: Some(format!("push {n}")), ..Person::default() },
                    _ => Person { jids: vec![jid], verified_name: Some(format!("Biz {n}")), ..Person::default() },
                })
            }
            7 => {
                let k = self.rng.below(VERSIONED_PEOPLE);
                let name = (self.rng.below(2) == 0).then(|| format!("Joined {}", self.rng.next() % 100));
                Change::Person(Person { jids: vec![versioned_number(k), versioned_lid(k)], name, ..Person::default() })
            }
            8 if !self.sent.is_empty() => {
                let k = self.rng.below(self.sent.len());
                let mut edited = self.sent[k].clone();
                self.clock += 1;
                edited.edited_at = Some(self.clock);
                edited.text = Some(format!("edited {}", self.clock));
                edited.spans = self.spans();
                self.sent[k] = edited.clone();
                Change::Message(edited)
            }
            9 if !self.sent.is_empty() => {
                let gone = self.sent.swap_remove(self.rng.below(self.sent.len()));
                Change::DeleteMessage { chat: gone.chat, id: gone.id }
            }
            _ => self.message(),
        }
    }
}

/// Every message row of every chat, as a fresh query has it, by its n.
async fn every_message_row(store: &Store) -> HashMap<i64, Value> {
    store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            let mut all = HashMap::new();
            for chat in read::chats(db, false, None, 100_000)? {
                for row in read::messages(db, &chat.id, Around::Newest, 1_000_000)? {
                    all.insert(row.n, serde_json::to_value(message_row(row)).unwrap());
                }
            }
            Ok(all)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a probe's long run, about a minute in debug: cargo test -p jawd -- --ignored a_rows_version"]
async fn a_rows_version_never_goes_back_and_moves_whenever_what_it_shows_changes() {
    let without_v = |row: &Value| {
        let mut row = row.clone();
        row.as_object_mut().unwrap().remove("v");
        row
    };
    for seed in 1..=20u64 {
        let bed = Bed::new(brisk());
        let mut world = Versions::new(seed, true);
        let seeding: Vec<Change> = (0..20).map(|_| world.message()).collect();
        bed.apply(seeding).await;
        let mut was = every_message_row(&bed.store).await;
        for step in 0..200 {
            let many = 1 + world.rng.below(3);
            let changes: Vec<Change> = (0..many).map(|_| world.change()).collect();
            bed.apply(changes).await;
            if world.rng.below(3) == 0 {
                bed.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
            }
            let now = every_message_row(&bed.store).await;
            for (n, row) in &now {
                let Some(before) = was.get(n) else { continue };
                let (v, v0) = (row["v"].as_u64().unwrap(), before["v"].as_u64().unwrap());
                assert!(
                    v >= v0,
                    "seed {seed}, step {step}: message {n}'s v went back from {v0} to {v}:\nwas {before}\nnow {row}"
                );
                if without_v(row) != without_v(before) {
                    assert!(
                        v > v0,
                        "seed {seed}, step {step}: message {n} shows something new at v {v}:\nwas {before}\nnow {row}"
                    );
                }
            }
            was = now;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a probe's long run, about half a minute in debug: cargo test -p jawd -- --ignored windows_watched_again"]
async fn windows_watched_again_from_their_seq_match_fresh_queries_through_many_mentions_and_joins() {
    for seed in 1..=10u64 {
        let bed = Bed::new(brisk());
        let mut world = Versions::new(seed, false);
        let seeding: Vec<Change> = (0..30).map(|_| world.message()).collect();
        bed.apply(seeding).await;
        let wants: Vec<Want> = versioned_groups()
            .into_iter()
            .map(|chat| Want::Messages { chat, anchor: None, before: 8, after: 0 })
            .collect();
        let mut live = bed.watcher(64);
        let live_watched = watch_all(&mut live, &wants).await;
        // The other connection takes no pushes; every few commits it's gone and comes back from what
        // it held, as jaw does after a reconnect or a resync.
        let mut away = bed.watcher(64);
        let mut away_watched = watch_all(&mut away, &wants).await;
        let mut gap = 0;
        for step in 0..150 {
            let many = 1 + world.rng.below(3);
            let changes: Vec<Change> = (0..many).map(|_| world.change()).collect();
            let seq = bed.apply(changes).await;
            live.settle(seq).await;
            check(&bed, &live, &live_watched, &format!("seed {seed}, step {step}, live")).await;
            if gap == 0 {
                (away, away_watched) = reconnected(&bed, &away, &away_watched).await;
                check(&bed, &away, &away_watched, &format!("seed {seed}, step {step}, watched again")).await;
                gap = world.rng.below(4);
            } else {
                gap -= 1;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_list_owed_a_reorder_is_read_at_its_pace_while_its_reads_fail() {
    let bed = Bed::new(brisk());
    let seeding = (0..5).map(|n| {
        let who = person(n);
        Change::Message(text(&who, &format!("p{n}"), 1_000 + n as i64, Some(&who), "hello"))
    });
    bed.apply(seeding.collect()).await;
    let mut watcher = bed.watcher(64);
    let watched = watch_all(&mut watcher, &[Want::Chats { archived: false, anchor: None, before: 0, after: 10 }]).await;
    // While catching up, a chat list takes a new message where its chat was and owes a reorder
    // once `hold` has passed since it was watched.
    bed.catching_up.store(true, Ordering::Relaxed);
    bed.apply(vec![Change::Message(text(&person(0), "p0b", 9_000, Some(&person(0)), "again"))]).await;
    watcher.run_for(ms(50)).await;
    assert!(watcher.client.pending(), "it owes a reorder");

    bed.feed.fail_reads(true);
    let reads = bed.feed.reads();
    let trying = brisk().hold + brisk().behind / 2 - ms(50);
    watcher.run_for(trying).await;
    let tried = bed.feed.reads() - reads;
    let most = (trying.as_millis() / brisk().burst.as_millis()) as u64 + 2;
    assert!((2..=most).contains(&tried), "{tried} reads in {trying:?}: at least twice, and no faster than its pace");
    assert!(watcher.resynced.is_empty(), "it hasn't failed for long");

    bed.feed.fail_reads(false);
    watcher.run_for(ms(100)).await;
    assert!(!watcher.client.pending(), "it took its new order once it could be read");
    check(&bed, &watcher, &watched, "once it could be read").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_moved_while_its_reads_fail_counts_its_failures_from_the_move() {
    let bed = Bed::new(brisk());
    let seeding = (0..30).map(|n| Change::Message(text(OPEN, &format!("o{n}"), 1_000 + n, Some(OPEN), "hi")));
    bed.apply(seeding.collect()).await;
    let mut watcher = bed.watcher(64);
    let watch = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10})).await;

    bed.feed.fail_reads(true);
    bed.apply(vec![Change::Message(text(OPEN, "new1", 5_000, Some(OPEN), "while it can't be read"))]).await;
    watcher.run_for(brisk().behind * 2 / 3).await;
    assert!(watcher.resynced.is_empty(), "it hasn't failed for long");

    // Moved while it can be read, it holds a fresh window, and fails again only from the next read.
    bed.feed.fail_reads(false);
    watcher.call("watch.move", json!({"watch": watch, "around": "o5", "before": 3, "after": 3})).await.unwrap();
    bed.feed.fail_reads(true);
    let moved = Instant::now();
    bed.apply(vec![Change::Message(text(OPEN, "new2", 6_000, Some(OPEN), "still can't be read"))]).await;
    watcher.run_for(brisk().behind / 2).await;
    assert!(watcher.resynced.is_empty(), "resynced {:?} after it was moved", moved.elapsed());
    while watcher.resynced.is_empty() {
        watcher.turn().await;
        watcher.drain();
    }
    assert!(moved.elapsed() >= brisk().behind, "resynced after {:?}", moved.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_on_several_connections_match_fresh_queries_while_writes_race_their_reads() {
    for seed in 1..=6u64 {
        let bed = Bed::new(brisk());
        let mut world = World::new(seed + 2000);
        let mut seeding: Vec<Change> =
            world.chats.iter().map(|chat| Change::Chat(Chat { id: chat.clone(), ..Chat::default() })).collect();
        seeding.extend((0..60).flat_map(|_| world.new_message_or_two()));
        bed.apply(seeding).await;
        // Two connections watching the same windows, and a third sharing some of them.
        let same = vec![
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: Some(person(3)), before: 2, after: 2 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: GROUP.into(), anchor: None, before: 4, after: 0 },
        ];
        let other = vec![
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: None, before: 0, after: 8 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed.store, OPEN, 8).await, before: 3, after: 3 },
        ];
        type Ask = (i64, tokio::sync::oneshot::Sender<HashMap<String, Model>>);
        let mut connections = Vec::new();
        for wants in [same.clone(), same, other] {
            let mut watcher = bed.watcher(64);
            let watched = watch_all(&mut watcher, &wants).await;
            let (ask, mut asks) = mpsc::channel::<Ask>(1);
            let task = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        ready = watcher.client.ready() => {
                            if let Ready::Changes(Ok(changes)) = &ready {
                                watcher.seen = watcher.seen.max(changes.seq);
                            }
                            watcher.client.handle(ready).await;
                            watcher.drain();
                        }
                        asked = asks.recv() => match asked {
                            Some((seq, reply)) => {
                                watcher.settle(seq).await;
                                let _ = reply.send(watcher.windows.clone());
                            }
                            None => break,
                        },
                    }
                }
            });
            connections.push((ask, task, watched));
        }
        for pause in 0..25 {
            let mut seq = 0;
            for _ in 0..1 + world.rng.below(30) {
                seq = bed.apply(world.change()).await;
                match world.rng.below(6) {
                    0 => tokio::time::sleep(ms(12 + world.rng.below(10) as u64)).await,
                    1 => tokio::time::sleep(ms(world.rng.below(3) as u64)).await,
                    _ => tokio::task::yield_now().await,
                }
            }
            for (k, (ask, _, watched)) in connections.iter().enumerate() {
                let (reply, windows) = tokio::sync::oneshot::channel();
                ask.send((seq, reply)).await.unwrap();
                let windows = windows.await.unwrap();
                for (watch, want) in watched {
                    let expected = fresh(&bed.store, want).await;
                    let what = format!("seed {seed}, pause {pause}, connection {k}, {watch} {want:?}");
                    windows[watch].assert_matches(&expected, &what);
                }
            }
        }
        for (ask, task, _) in connections {
            drop(ask);
            task.await.unwrap();
        }
    }
}

/// What the probe asks of one of its connections.
enum Ask {
    Check(i64, tokio::sync::oneshot::Sender<Vec<String>>),
    Move(usize, Want),
    Rewatch(usize),
}

/// How `model` differs from `fresh`, if it does.
fn differs(model: &Model, fresh: &Model) -> Option<String> {
    if model.ids() != fresh.ids() {
        return Some(format!("rows {:?} where a fresh query has {:?}", model.ids(), fresh.ids()));
    }
    let (a, b) = (model.ordered(), fresh.ordered());
    if a != b {
        let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(0);
        return Some(format!("row {k} says {} where a fresh query says {}", a[k], b[k]));
    }
    if model.total != fresh.total {
        return Some(format!("total {} where a fresh query has {}", model.total, fresh.total));
    }
    if !model.chats && (model.older, model.newer) != (fresh.older, fresh.newer) {
        let (held, wanted) = ((model.older, model.newer), (fresh.older, fresh.newer));
        return Some(format!("older and newer {held:?} where a fresh query has {wanted:?}"));
    }
    None
}

/// One of the probe's connections: what it watches, and what it found wrong.
struct Probe {
    name: String,
    watcher: Watcher,
    watched: Vec<(String, Want)>,
    problems: Vec<String>,
    /// Windows resynced and not watched again yet.
    unmended: Vec<String>,
    rng: Rng,
    /// Sleeps a moment after a third of the changes it hears.
    slow: bool,
    /// Reads its queue only every 50 to 500 ms, so it fills and the windows are resynced.
    stalls: bool,
    store: Store,
    resyncs: u64,
    lags: u64,
}

impl Probe {
    fn drain(&mut self) {
        self.watcher.drain();
        let resynced = std::mem::take(&mut self.watcher.resynced);
        self.resyncs += resynced.len() as u64;
        self.unmended.extend(resynced);
    }

    /// Watches window `k` again, or says why it can't yet: its anchor may have gone, in which case
    /// it's watched around another message next time.
    async fn watch_again(&mut self, k: usize) -> bool {
        let (method, params) = self.watched[k].1.params();
        match self.watcher.call(method, params).await {
            Ok(result) => {
                self.watched[k].0 = result["watch"].as_str().unwrap().to_string();
                true
            }
            Err(e) => {
                if e.code == code::NOT_FOUND
                    && let Want::Messages { chat, before, after, .. } = self.watched[k].1.clone()
                {
                    let anchor = anchor_on(&self.store, &chat, self.rng.below(12)).await;
                    self.watched[k].1 = Want::Messages { chat, anchor, before, after };
                }
                false
            }
        }
    }

    async fn mend(&mut self) {
        for watch in std::mem::take(&mut self.unmended) {
            let Some(k) = self.watched.iter().position(|(w, _)| *w == watch) else { continue };
            if !self.watch_again(k).await {
                self.unmended.push(watch);
            }
        }
    }

    async fn turn(&mut self) {
        let ready =
            tokio::time::timeout(Duration::from_secs(10), self.watcher.client.ready()).await.expect("nothing happened");
        self.heard(&ready);
        self.watcher.client.handle(ready).await;
    }

    fn heard(&mut self, ready: &Ready) {
        match ready {
            Ready::Changes(Ok(changes)) => self.watcher.seen = self.watcher.seen.max(changes.seq),
            Ready::Changes(Err(broadcast::error::RecvError::Lagged(_))) => self.lags += 1,
            _ => {}
        }
    }

    async fn settle(&mut self, seq: i64) {
        let mut tries = 0;
        loop {
            self.drain();
            self.mend().await;
            if !self.unmended.is_empty() {
                tries += 1;
                assert!(tries < 1000, "{}: can't watch {:?} again", self.name, self.unmended);
                continue;
            }
            if self.watcher.seen >= seq && !self.watcher.client.pending() {
                break;
            }
            self.turn().await;
        }
        self.drain();
    }

    async fn check(&mut self, seq: i64) -> Vec<String> {
        self.settle(seq).await;
        let mut problems = std::mem::take(&mut self.problems);
        for (watch, want) in &self.watched {
            let expected = fresh(&self.store, want).await;
            if let Some(why) = differs(&self.watcher.windows[watch], &expected) {
                problems.push(format!("{}: {watch} {want:?} at seq {seq}: {why}", self.name));
            }
        }
        problems
    }

    async fn moved(&mut self, k: usize, want: Want) {
        self.drain();
        let watch = self.watched[k].0.clone();
        let (around, before, after) = match &want {
            Want::Chats { anchor, before, after, .. } => {
                (anchor.clone().unwrap_or_else(|| "top".into()), *before, *after)
            }
            Want::Messages { anchor, before, after, .. } => {
                (anchor.as_ref().map_or("newest".to_string(), |(id, _, _)| id.clone()), *before, *after)
            }
        };
        let held = self.watcher.windows.get(&watch).map_or(0, |model| model.seq);
        let params = json!({"watch": watch, "around": around, "before": before, "after": after});
        if let Ok(result) = self.watcher.call("watch.move", params).await {
            let seq = result["seq"].as_u64().unwrap();
            if seq < held {
                self.problems.push(format!("{}: {watch} moved back from seq {held} to {seq}", self.name));
            }
            self.watched[k].1 = want;
        }
    }

    async fn rewatched(&mut self, k: usize) {
        self.drain();
        let watch = self.watched[k].0.clone();
        let _ = self.watcher.call("unwatch", json!({"watch": watch})).await;
        self.watcher.windows.remove(&watch);
        self.unmended.retain(|w| *w != watch);
        if !self.watch_again(k).await {
            self.unmended.push(self.watched[k].0.clone());
        }
    }

    /// Runs the connection until it's told to stop, and says how often it was resynced and fell
    /// behind the feed.
    async fn run(mut self, mut asks: mpsc::Receiver<Ask>) -> (u64, u64) {
        let mut drain_at = Instant::now();
        loop {
            tokio::select! {
                ready = self.watcher.client.ready() => {
                    let heard = matches!(ready, Ready::Changes(_));
                    self.heard(&ready);
                    self.watcher.client.handle(ready).await;
                    if !self.stalls || Instant::now() >= drain_at {
                        self.drain();
                        self.mend().await;
                        if self.stalls {
                            drain_at = Instant::now() + ms(50 + self.rng.below(450) as u64);
                        }
                    }
                    if self.slow && heard && self.rng.below(3) == 0 {
                        tokio::time::sleep(ms(self.rng.below(15) as u64)).await;
                    }
                }
                asked = asks.recv() => match asked {
                    Some(Ask::Check(seq, reply)) => {
                        let problems = self.check(seq).await;
                        let _ = reply.send(problems);
                    }
                    Some(Ask::Move(k, want)) => self.moved(k, want).await,
                    Some(Ask::Rewatch(k)) => self.rewatched(k).await,
                    None => break,
                },
            }
        }
        (self.resyncs, self.lags)
    }
}

/// What the probe went through.
#[derive(Debug, Default)]
struct Probed {
    moves: u64,
    rewatches: u64,
    holds: u64,
    failing_spells: u64,
    resyncs: u64,
    lags: u64,
    reads: u64,
    taken: u64,
    joined: u64,
}

/// Five connections racing the writes, through moves, watches again, catch-ups, failing reads and
/// stalls. Two watch the same five windows; a third the same top of the chat list and open chat,
/// and windows of its own; a fourth the first two's, sleeping a moment after some of what it
/// hears; and a fifth the same, reading its queue of 4 only every 50 to 500 ms, so it's resynced.
/// The writes are the property test's, with group messages from 12 new senders and their names.
/// Between them a catch-up starts or ends, reads fail for a spell, a window moves, to another
/// anchor or onto the spec of another window, or is unwatched and watched again. At each pause
/// every window is compared with a fresh query, and throughout no window's seq goes back. An even
/// seed resyncs after 40 ms rather than 300, and the third and fourth of every four keep only 4
/// reads of the events, so connections fall behind them.
async fn probe(seeds: std::ops::RangeInclusive<u64>, pauses: usize) -> Probed {
    let mut probed = Probed::default();
    for seed in seeds {
        let behind = if seed % 2 == 0 { ms(40) } else { ms(300) };
        let kept = if (seed - 1) % 4 >= 2 { 4 } else { CHANGES_KEPT };
        let bed = Bed::keeping(Timing { behind, ..brisk() }, kept);
        let mut world = World::new(seed + 7000);
        let mut seeding: Vec<Change> =
            world.chats.iter().map(|chat| Change::Chat(Chat { id: chat.clone(), ..Chat::default() })).collect();
        seeding.extend((0..60).flat_map(|_| world.new_message_or_two()));
        bed.apply(seeding).await;
        let base = vec![
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: Some(person(3)), before: 2, after: 2 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: GROUP.into(), anchor: None, before: 4, after: 0 },
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed.store, OPEN, 8).await, before: 3, after: 3 },
        ];
        let other = vec![
            Want::Chats { archived: false, anchor: None, before: 0, after: 5 },
            Want::Chats { archived: false, anchor: None, before: 0, after: 8 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 },
            Want::Messages { chat: GROUP.into(), anchor: None, before: 10, after: 0 },
            Want::Messages { chat: GROUP.into(), anchor: anchor_on(&bed.store, GROUP, 5).await, before: 2, after: 2 },
        ];
        let kinds = [("c0", false, false, 64), ("c1", false, false, 64), ("c2", false, false, 64)];
        let kinds = kinds.into_iter().chain([("c3, slow", true, false, 64), ("c4, stalling", false, true, 4)]);
        let mut connections = Vec::new();
        for (n, (name, slow, stalls, room)) in kinds.enumerate() {
            let wants = if n == 2 { other.clone() } else { base.clone() };
            let mut watcher = bed.watcher(room);
            let watched = watch_all(&mut watcher, &wants).await;
            let probe = Probe {
                name: format!("seed {seed}, {name}"),
                watcher,
                watched,
                problems: Vec::new(),
                unmended: Vec::new(),
                rng: Rng((seed * 31 + n as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
                slow,
                stalls,
                store: bed.store.clone(),
                resyncs: 0,
                lags: 0,
            };
            let (ask, asks) = mpsc::channel::<Ask>(8);
            connections.push((ask, tokio::spawn(probe.run(asks))));
        }
        let (mut extra, mut failing) = (0u64, false);
        for pause in 0..pauses {
            let mut seq = 0;
            for _ in 0..1 + world.rng.below(30) {
                match world.rng.below(100) {
                    0..4 => {
                        let holding = !bed.catching_up.load(Ordering::Relaxed);
                        probed.holds += u64::from(holding);
                        bed.catching_up.store(holding, Ordering::Relaxed);
                    }
                    4..7 => {
                        probed.failing_spells += u64::from(!failing);
                        failing = true;
                        bed.feed.fail_reads(true);
                    }
                    7..13 => {
                        let c = world.rng.below(connections.len());
                        let k = if world.rng.below(2) == 0 { 1 } else { 4 };
                        let want = match (c == 2, k) {
                            (false, 1) if world.rng.below(4) == 0 => {
                                Want::Chats { archived: false, anchor: None, before: 0, after: 5 }
                            }
                            (false, 1) => Want::Chats {
                                archived: false,
                                anchor: Some(person(world.rng.below(DIRECT))),
                                before: 2,
                                after: 2,
                            },
                            (true, 1) => {
                                let after = if world.rng.below(2) == 0 { 5 } else { 8 };
                                Want::Chats { archived: false, anchor: None, before: 0, after }
                            }
                            (false, _) if world.rng.below(4) == 0 => {
                                Want::Messages { chat: OPEN.into(), anchor: None, before: 6, after: 0 }
                            }
                            (false, _) => Want::Messages {
                                chat: OPEN.into(),
                                anchor: anchor_on(&bed.store, OPEN, world.rng.below(12)).await,
                                before: 3,
                                after: 3,
                            },
                            (true, _) if world.rng.below(3) == 0 => {
                                Want::Messages { chat: GROUP.into(), anchor: None, before: 10, after: 0 }
                            }
                            (true, _) => Want::Messages {
                                chat: GROUP.into(),
                                anchor: anchor_on(&bed.store, GROUP, world.rng.below(8)).await,
                                before: 2,
                                after: 2,
                            },
                        };
                        probed.moves += 1;
                        connections[c].0.send(Ask::Move(k, want)).await.unwrap();
                    }
                    13..15 => {
                        let c = world.rng.below(connections.len());
                        probed.rewatches += 1;
                        connections[c].0.send(Ask::Rewatch(world.rng.below(5))).await.unwrap();
                    }
                    _ => {}
                }
                if failing && world.rng.below(3) == 0 {
                    failing = false;
                    bed.feed.fail_reads(false);
                }
                let changes = match world.rng.below(100) {
                    0..12 => {
                        extra += 1;
                        world.clock += 1 + world.rng.below(500) as i64;
                        let who = person(DIRECT + 4 + world.rng.below(12));
                        vec![Change::Message(text(GROUP, &format!("x{extra}"), world.clock, Some(&who), "hello"))]
                    }
                    12..20 => {
                        let who = person(DIRECT + 4 + world.rng.below(12));
                        let name = format!("New {}", world.rng.next() % 100);
                        vec![Change::Person(Person { jids: vec![who], name: Some(name), ..Person::default() })]
                    }
                    _ => world.change(),
                };
                seq = bed.apply(changes).await;
                match world.rng.below(6) {
                    0 => tokio::time::sleep(ms(12 + world.rng.below(10) as u64)).await,
                    1 => tokio::time::sleep(ms(world.rng.below(3) as u64)).await,
                    _ => tokio::task::yield_now().await,
                }
            }
            failing = false;
            bed.feed.fail_reads(false);
            bed.catching_up.store(false, Ordering::Relaxed);
            let mut problems = Vec::new();
            for (ask, _) in &connections {
                let (reply, answer) = tokio::sync::oneshot::channel();
                ask.send(Ask::Check(seq, reply)).await.unwrap();
                problems.extend(answer.await.unwrap());
            }
            assert!(problems.is_empty(), "seed {seed}, pause {pause}: {problems:#?}");
        }
        for (ask, task) in connections {
            drop(ask);
            let (resyncs, lags) = task.await.unwrap();
            probed.resyncs += resyncs;
            probed.lags += lags;
        }
        let windows = bed.feed.windows();
        probed.reads += windows.reads;
        probed.taken += windows.taken;
        probed.joined += windows.joined;
    }
    probed
}

#[tokio::test(flavor = "multi_thread")]
async fn many_connections_match_fresh_queries_through_moves_catch_ups_failing_reads_and_stalls() {
    let probed = probe(1..=4, 25).await;
    assert!(probed.taken > 0 && probed.joined > 0, "windows shared reads: {probed:?}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "the probe's long run, about two minutes in debug: cargo test -p jawd -- --ignored many_connections"]
async fn many_connections_match_fresh_queries_through_moves_catch_ups_failing_reads_and_stalls_for_40_seeds() {
    let probed = probe(1..=40, 25).await;
    eprintln!("the probe went through {probed:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_under_way_is_joined_only_if_it_was_asked_for_after_every_change_heard() {
    // Reads 300 ms apart, so the second connection waits out its pace while the first's read is
    // under way, and hears a change made after that read was asked for.
    let timing = Timing { quiet: ms(1), burst: ms(300), ..brisk() };
    let bed = Bed::new(timing);
    bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let (mut first, mut second) = (bed.watcher(64), bed.watcher(64));
    let first_watched = watch_all(&mut first, std::slice::from_ref(&want)).await;
    let second_watched = watch_all(&mut second, &[want]).await;

    // The first connection reads for o2, and the window keeps that read.
    let o2 = bed.apply(vec![Change::Message(text(OPEN, "o2", 2_000, Some(OPEN), "one"))]).await;
    first.settle(o2).await;
    tokio::time::sleep(timing.burst + ms(50)).await;

    // It reads again for o3, a read whose answer is held back once its snapshot is taken.
    bed.feed.slow_reads(1_000);
    let asked = bed.feed.reads();
    let o3 = bed.apply(vec![Change::Message(text(OPEN, "o3", 3_000, Some(OPEN), "two"))]).await;
    let reading = tokio::spawn(async move {
        first.settle(o3).await;
        first
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while bed.feed.reads() == asked {
        assert!(Instant::now() < deadline, "the first connection never asked for a read for o3");
        tokio::time::sleep(ms(1)).await;
    }
    tokio::time::sleep(ms(50)).await;
    bed.feed.slow_reads(0);

    // o4 comes while that read is under way. The second connection takes the kept read for o2,
    // waits out its pace, hearing o3 and o4, and then mustn't join the read under way, asked for
    // before o4.
    let o4 = bed.apply(vec![Change::Message(text(OPEN, "o4", 4_000, Some(OPEN), "three"))]).await;
    second.settle(o4).await;
    check(&bed, &second, &second_watched, "the connection that heard o4 while the other's read was under way").await;
    let mut first = reading.await.unwrap();
    first.settle(o4).await;
    check(&bed, &first, &first_watched, "the connection whose read was slow").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_never_takes_a_read_older_than_what_its_interface_holds() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    let wide = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let (mut reader, mut mover) = (bed.watcher(64), bed.watcher(64));
    watch_all(&mut reader, &[wide]).await;
    let watch = mover.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await;

    // One connection reads the wide window with o2, and the window keeps that read.
    let o2 = bed.apply(vec![Change::Message(text(OPEN, "o2", 2_000, Some(OPEN), "one"))]).await;
    reader.settle(o2).await;
    let o3 = bed.apply(vec![Change::Message(text(OPEN, "o3", 3_000, Some(OPEN), "two"))]).await;

    // The other moves onto the wide window before it has heard of o2 or o3: its own read has both.
    let moved = mover.call("watch.move", json!({"watch": watch, "around": "newest", "before": 10})).await.unwrap();
    let moved_seq = moved["seq"].as_u64().unwrap();
    assert!(mover.windows[&watch].rows.contains_key("o3"));

    // Then it hears o2, which marks the window with a change the kept read has, though that read
    // is older than what its interface holds.
    while mover.seen < o3 || mover.client.pending() {
        let ready =
            tokio::time::timeout(Duration::from_secs(10), mover.client.ready()).await.expect("nothing happened");
        if let Ready::Changes(Ok(changes)) = &ready {
            mover.seen = mover.seen.max(changes.seq);
        }
        mover.client.handle(ready).await;
        while let Ok(line) = mover.lines.try_recv() {
            let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else {
                panic!("a reply among the pushes")
            };
            let seq = push.params["seq"].as_u64().unwrap();
            assert!(seq >= moved_seq, "a push as of seq {seq} after a move as of {moved_seq}: {}", push.params);
            mover.take(&line);
        }
    }
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    mover.windows[&watch].assert_matches(&fresh(&bed.store, &want).await, "the moved window");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lagged_connection_takes_only_a_read_asked_for_after_the_lag() {
    // A feed that keeps only 4 reads of the events, so a connection left alone falls behind.
    let bed = Bed::keeping(brisk(), 4);
    bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let (mut lagging, mut reading) = (bed.watcher(64), bed.watcher(64));
    let lagging_watched = watch_all(&mut lagging, std::slice::from_ref(&want)).await;
    watch_all(&mut reading, &[want]).await;

    // One connection reads for o2, and the window keeps that read; then o3 comes, and changes to
    // another chat, while the other connection reads nothing, so it loses o3 among them.
    let o2 = bed.apply(vec![Change::Message(text(OPEN, "o2", 2_000, Some(OPEN), "one"))]).await;
    reading.settle(o2).await;
    bed.apply(vec![Change::Message(text(OPEN, "o3", 3_000, Some(OPEN), "two"))]).await;
    let mut last = 0;
    for n in 0..12 {
        let other = person(5);
        let elsewhere = text(&other, &format!("p{n}"), 4_000 + n, Some(&other), "elsewhere");
        last = bed.apply(vec![Change::Message(elsewhere)]).await;
        tokio::time::sleep(ms(3)).await;
    }

    // It hears it fell behind; the kept read has o2 and every change it had heard of, but not o3.
    lagging.settle(last).await;
    check(&bed, &lagging, &lagging_watched, "the connection that fell behind").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_whose_task_panics_leaves_nothing_under_way_for_its_window_to_wait_on() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    let want = Want::Messages { chat: OPEN.into(), anchor: None, before: 10, after: 0 };
    let mut watcher = bed.watcher(64);
    let watched = watch_all(&mut watcher, &[want]).await;

    // The read for o2 answers, and its task panics before anyone has the answer. The window is
    // read again at its pace, as after any read that fails, rather than waiting on a read that
    // will never answer until it's resynced.
    bed.feed.panic_next_read();
    bed.apply(vec![Change::Message(text(OPEN, "o2", 2_000, Some(OPEN), "one"))]).await;
    watcher.run_for(brisk().behind / 2).await;
    assert!(watcher.resynced.is_empty(), "it was resynced");
    check(&bed, &watcher, &watched, "the window whose read's task panicked").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_watches_at_most_its_windows_and_is_told_why_when_it_asks_for_more() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(64);
    let mut watches = Vec::new();
    for before in 1..=MOST_WINDOWS {
        watches
            .push(watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": before})).await);
    }
    for (method, params) in [
        ("chats.watch", json!({"around": "top", "after": 5})),
        ("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 50})),
    ] {
        let refused = watcher.call(method, params).await.unwrap_err();
        assert_eq!(refused.code, code::LIMIT, "{}", refused.message);
        assert_eq!(refused.data, Some(json!({"limit": "windows", "most": MOST_WINDOWS})));
    }

    // Moving a window isn't watching another, and once one is unwatched another can come.
    watcher.call("watch.move", json!({"watch": watches[0], "around": "newest", "before": 50})).await.unwrap();
    watcher.call("unwatch", json!({"watch": watches[0]})).await.unwrap();
    watcher.watch("chats.watch", json!({"around": "top", "after": 5})).await;
    let windows = bed.feed.windows();
    assert_eq!((windows.watched, windows.most), (MOST_WINDOWS as u64, MOST_WINDOWS as u64));

    // Another connection has room of its own, and a connection that goes takes its windows with it.
    let mut other = bed.watcher(64);
    other.watch("chats.watch", json!({"around": "top", "after": 5})).await;
    assert_eq!(bed.feed.windows().watched, MOST_WINDOWS as u64 + 1);
    drop(watcher);
    assert_eq!(bed.feed.windows().watched, 1);
}

/// The bytes of the rows `want` shows, as JSON.
async fn json_bytes(store: &Store, want: &Want) -> u64 {
    fresh(store, want).await.rows.values().map(|row| serde_json::to_vec(row).unwrap().len() as u64).sum()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reads_kept_for_windows_to_share_are_counted_in_bytes_and_held_under_the_most() {
    let bed = Bed::new(brisk());
    let mut seeding = Vec::new();
    for n in 0..40 {
        seeding.push(Change::Message(text(OPEN, &format!("o{n}"), 1_000 + n, Some(OPEN), "words in the open chat")));
        let group = text(GROUP, &format!("g{n}"), 1_000 + n, Some(&person(DIRECT)), "words in the group");
        seeding.push(Change::Message(group));
    }
    seeding.extend((0..12).map(|n| {
        let who = person(n);
        Change::Message(text(&who, &format!("p{n}"), 500 + n as i64, Some(&who), "hello"))
    }));
    bed.apply(seeding).await;
    let wants = vec![
        Want::Chats { archived: false, anchor: None, before: 0, after: 10 },
        Want::Messages { chat: OPEN.into(), anchor: None, before: 30, after: 0 },
        Want::Messages { chat: GROUP.into(), anchor: None, before: 30, after: 0 },
        Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed.store, OPEN, 20).await, before: 5, after: 5 },
    ];
    let mut connections = Vec::new();
    for _ in 0..2 {
        let mut watcher = bed.watcher(64);
        let watched = watch_all(&mut watcher, &wants).await;
        connections.push((watcher, watched));
    }
    let windows = bed.feed.windows();
    assert_eq!((windows.watched, windows.distinct), (8, 4));
    assert_eq!(windows.kept_bytes, 0, "a watch reads for its connection alone");

    // A change to every window: each is read once and kept, its rows as the JSON they're sent as.
    let touch = |n: i64| {
        vec![
            Change::Message(text(OPEN, &format!("new{n}"), 5_000 + n, Some(OPEN), "to the open chat")),
            Change::Message(text(GROUP, &format!("gnew{n}"), 5_000 + n, Some(&person(DIRECT)), "to the group")),
        ]
    };
    let (reads, seq) = (bed.feed.reads(), bed.apply(touch(1)).await);
    for (watcher, watched) in &mut connections {
        watcher.settle(seq).await;
        check(&bed, watcher, watched, "with every read kept").await;
    }
    let all = bed.feed.windows();
    let mut json = 0;
    for want in &wants {
        json += json_bytes(&bed.store, want).await;
    }
    assert_eq!(all.reads - reads, 4, "one read of each window, shared");
    assert!(
        all.kept_bytes >= json && all.kept_bytes <= json * 2 + 16_384,
        "{} kept for {json} of JSON",
        all.kept_bytes
    );
    assert_eq!(all.let_go, 0);

    // With room for about half of them the oldest are let go, and their windows read for themselves.
    bed.feed.keep_at_most(all.kept_bytes / 2);
    let seq = bed.apply(touch(2)).await;
    for (watcher, watched) in &mut connections {
        watcher.settle(seq).await;
        check(&bed, watcher, watched, "with some kept reads let go").await;
    }
    let half = bed.feed.windows();
    assert!(
        half.kept_bytes <= all.kept_bytes / 2,
        "{} kept where at most {} may be",
        half.kept_bytes,
        all.kept_bytes / 2
    );
    assert!(half.let_go > 0);

    // The reads kept go with the last windows that share them.
    drop(connections);
    let gone = Instant::now() + Duration::from_secs(5);
    loop {
        let windows = bed.feed.windows();
        if (windows.watched, windows.distinct, windows.kept_bytes) == (0, 0, 0) {
            break;
        }
        assert!(Instant::now() < gone, "still {windows:?} once every window had gone");
        tokio::time::sleep(ms(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_says_how_many_windows_are_watched_and_what_the_reads_they_share_keep() {
    let bed = Bed::new(brisk());
    let seq = bed.apply(vec![Change::Message(text(OPEN, "o1", 1_000, Some(OPEN), "hi"))]).await;
    // The feed has told every connection of o1 before the interfaces connect, so only o2 reads.
    bed.watcher(8).settle(seq).await;
    let socket = serve(&bed);
    let watch = json!({"chat": OPEN, "around": "newest", "before": 10});
    let lines = tokio::task::spawn_blocking(move || {
        let mut lines = [Line::open(&socket, None).0, Line::open(&socket, None).0];
        for line in &mut lines {
            line.call("messages.watch", watch.clone());
        }
        lines
    })
    .await
    .unwrap();
    bed.apply(vec![Change::Message(text(OPEN, "o2", 2_000, Some(OPEN), "to both"))]).await;
    let status = tokio::task::spawn_blocking(move || {
        let mut lines = lines;
        for line in &mut lines {
            while !line.push()["params"]["upsert"].as_array().into_iter().flatten().any(|row| row["id"] == "o2") {}
        }
        lines[0].call("status", Value::Null)
    })
    .await
    .unwrap();
    let windows = &status["windows"];
    assert_eq!((windows["watched"].as_u64(), windows["distinct"].as_u64()), (Some(2), Some(1)), "{status}");
    assert_eq!(windows["most"].as_u64(), Some(MOST_WINDOWS as u64));
    assert!(windows["kept_bytes"].as_u64() > Some(0) && windows["kept_most"].as_u64() > windows["kept_bytes"].as_u64());
    assert_eq!(windows["reads"].as_u64(), Some(1), "one read for both connections");
    assert_eq!(windows["taken"].as_u64().unwrap() + windows["joined"].as_u64().unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_oldest_kept_read_is_let_go_first() {
    let bed = Bed::new(brisk());
    let mut seeding: Vec<Change> =
        (0..40).map(|n| Change::Message(text(OPEN, &format!("o{n}"), 1_000 + n, Some(OPEN), "words"))).collect();
    seeding.push(Change::Message(text(GROUP, "g0", 1_000, Some(&person(DIRECT)), "hi")));
    bed.apply(seeding).await;
    let mut watcher = bed.watcher(64);
    let watched = watch_all(
        &mut watcher,
        &[
            Want::Messages { chat: OPEN.into(), anchor: None, before: 40, after: 0 },
            Want::Messages { chat: OPEN.into(), anchor: None, before: 1, after: 0 },
            Want::Messages { chat: GROUP.into(), anchor: None, before: 1, after: 0 },
        ],
    )
    .await;
    // The open chat's two windows are read and kept, the long one first.
    let seq = bed.apply(vec![Change::Message(text(OPEN, "o40", 2_000, Some(OPEN), "words"))]).await;
    watcher.settle(seq).await;
    assert_eq!(bed.feed.windows().let_go, 0);

    // With room for the two short windows' reads alone, keeping the group's lets go of the oldest,
    // the long window's, rather than the newest, which the windows reading next would take.
    bed.feed.keep_at_most(4_096);
    let seq = bed.apply(vec![Change::Message(text(GROUP, "g1", 2_000, Some(&person(DIRECT)), "hi"))]).await;
    watcher.settle(seq).await;
    let windows = bed.feed.windows();
    assert_eq!(windows.let_go, 1, "{windows:?}");
    assert!(windows.kept_bytes > 0 && windows.kept_bytes <= 4_096, "{windows:?}");
    check(&bed, &watcher, &watched, "with the oldest kept read let go").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_client_sends_nothing_and_is_told_to_watch_again_once_held_for_behind() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(64);
    let watch = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10})).await;

    // Held, as while a reply waits for room ahead of it, it sends nothing.
    watcher.client.hold(true);
    bed.apply(vec![Change::Message(text(OPEN, "o1", 2_000, Some(OPEN), "one"))]).await;
    watcher.run_for(brisk().behind / 3).await;
    assert!(!watcher.windows[&watch].rows.contains_key("o1"), "it sent while held");

    // Let go in time, it sends what it held back.
    watcher.client.hold(false);
    watcher.run_for(ms(100)).await;
    assert!(watcher.windows[&watch].rows.contains_key("o1"), "it sends once let go");

    // Held for longer than `behind` with a change waiting, its windows are dropped, as when its
    // queue stays full, and it's told to watch again once let go.
    watcher.client.hold(true);
    bed.apply(vec![Change::Message(text(OPEN, "o2", 3_000, Some(OPEN), "two"))]).await;
    watcher.run_for(brisk().behind * 2).await;
    assert_eq!(bed.feed.windows().watched, 0, "its window was dropped");
    assert!(watcher.resynced.is_empty(), "a resync waits for the hold too");
    watcher.client.hold(false);
    watcher.run_for(ms(100)).await;
    assert_eq!(watcher.resynced, [watch]);
}

/// The lines in `came` for watch `watch`: its result and the pushes for it, each with its seq.
fn about(came: &[Value], watch: &str) -> Vec<(usize, u64)> {
    let of = |value: &Value| value["result"]["watch"] == watch || value["params"]["watch"] == watch;
    let seq = |value: &Value| value["result"]["seq"].as_u64().or(value["params"]["seq"].as_u64()).unwrap_or(0);
    came.iter().enumerate().filter(|(_, value)| of(value)).map(|(k, value)| (k, seq(value))).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interface_owed_a_reply_it_has_no_room_for_is_still_told_to_watch_again() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(2), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_pushing(&bed);
    let other = socket.clone();
    let mut line = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 30}));
        line
    })
    .await
    .unwrap();

    // It stops reading while pushes that aren't for windows fill jawd's queue to it, so its window
    // has had nothing to send, and asks for jawd's status, whose reply waits.
    fill(&pushes).await;
    line.ask(99, "status", Value::Null);
    let waiting = tokio::task::spawn_blocking(move || lanes(&mut Line::open(&other, None).0, [1, 0, 1])).await.unwrap();
    assert_eq!(waiting, [1, 64, 0, 1], "its reply waits: in the lanes, the most, lanes full, replies waiting");
    // Messages then pour into the chat it watches, and its window, held behind the reply, can't
    // send them.
    let started = Instant::now();
    let mut n = 0;
    while started.elapsed() < Duration::from_secs(8) {
        n += 1;
        bed.apply(vec![Change::Message(text(OPEN, &format!("p{n}"), 2_000 + n, Some(OPEN), "more"))]).await;
        tokio::time::sleep(ms(12)).await;
    }
    // Held for `behind`, its window is dropped as when its queue stays full; once it reads again,
    // it gets the reply and is then told to watch again.
    assert_eq!(bed.feed.windows().watched, 0, "its window was still watched after four times `behind`");
    let came = tokio::task::spawn_blocking(move || line.came(ms(700))).await.unwrap();
    let reply = came.iter().position(|value| value["id"] == 99).expect("the reply");
    assert!(came[reply..].iter().any(|value| value["method"] == "resync"), "it was told to watch again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_result_waiting_for_room_comes_before_anything_its_window_sends() {
    // Never dropped for falling behind, so the window watched while the queue is full lives on.
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![
        Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi")),
        Change::Message(text(GROUP, "g0", 1_000, Some(&person(DIRECT)), "hi")),
    ])
    .await;
    let socket = serve(&bed);
    let other = socket.clone();
    let mut line = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 30}));
        line
    })
    .await
    .unwrap();

    // It stops reading while messages pour into the open chat, until jawd's queue to it is full;
    // then it watches the group, whose result finds no room, and messages come to the group too.
    fill_window(&bed, OPEN, OPEN).await;
    line.ask(50, "messages.watch", json!({"chat": GROUP, "around": "newest", "before": 10}));
    let waiting = tokio::task::spawn_blocking(move || lanes(&mut Line::open(&other, None).0, [1, 0, 1])).await.unwrap();
    assert_eq!(waiting, [1, 64, 0, 1], "its result waits: in the lanes, the most, lanes full, replies waiting");
    // A request after it waits unread until there's room for that result, so no reply is lost.
    line.ask(51, "focus", json!({"focused": false}));
    for n in 0..40 {
        let message = text(GROUP, &format!("q{n}"), 2_000 + n, Some(&person(DIRECT)), "to the group");
        bed.apply(vec![Change::Message(message)]).await;
        tokio::time::sleep(ms(12)).await;
    }

    // Once it reads again, the group's window comes first as the watch's result, and nothing of
    // it is as of an older seq than what came before.
    let came = tokio::task::spawn_blocking(move || line.came(ms(700))).await.unwrap();
    let result = came.iter().position(|value| value["id"] == 50).expect("the watch's result");
    assert!(came[result + 1..].iter().any(|value| value["id"] == 51), "the reply to the request after it");
    let watch = came[result]["result"]["watch"].as_str().unwrap();
    let lines = about(&came, watch);
    assert_eq!(lines.first().map(|(k, _)| *k), Some(result), "something of {watch} came before its result");
    assert!(lines.len() > 1, "the group's messages came after it");
    assert!(lines.windows(2).all(|pair| pair[0].1 <= pair[1].1), "{watch} went back in seq: {lines:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_is_answered_while_a_request_sent_before_it_waits_on_the_writer() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve_all(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // The open counted for frecency waits on jaw.db's writer, held by another connection.
    let locked = Locked::on(&bed);
    let (mut line, came) = tokio::task::spawn_blocking(move || {
        line.ask(10, "chat.opened", json!({"chat": OPEN}));
        line.ask(11, "messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10}));
        let came = line.came(ms(1_000));
        (line, came)
    })
    .await
    .unwrap();
    assert!(came.iter().any(|value| value["id"] == 11 && value["result"]["watch"].is_string()), "{came:?}");
    assert!(!came.iter().any(|value| value["id"] == 10), "the open was kept while the writer was held");

    // Once the writer can write, the open is kept and answered.
    drop(locked);
    let later = tokio::task::spawn_blocking(move || line.came(ms(2_000))).await.unwrap();
    assert!(later.iter().any(|value| value["id"] == 10 && value.get("result").is_some()), "{later:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_down_the_lane_take_effect_in_the_order_they_were_sent() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve_all(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // A message sent clears its chat's draft, and a draft set after it stays. Both wait on the
    // writer, held meanwhile, so neither is done before the other is asked for.
    let locked = Locked::on(&bed);
    line.ask(20, "send.text", json!({"chat": OPEN, "text": "on my way", "key": "jaw-lane-1"}));
    line.ask(21, "draft.set", json!({"chat": OPEN, "text": "and then"}));
    tokio::time::sleep(ms(300)).await;
    drop(locked);
    let came = tokio::task::spawn_blocking(move || line.came(ms(2_000))).await.unwrap();
    let replies: Vec<&Value> = came.iter().filter(|value| value.get("id").is_some()).collect();
    assert_eq!(replies.iter().map(|value| value["id"].as_u64()).collect::<Vec<_>>(), [Some(20), Some(21)]);
    assert!(replies.iter().all(|value| value.get("result").is_some()), "{replies:?}");
    assert_eq!(draft(&bed).await.as_deref(), Some("and then"), "the draft set after the message stays");
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_sent_during_a_windows_read_all_go_ahead_of_the_windows_still_due() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve(&bed);
    let mut line = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.call("chats.watch", json!({"around": "top", "after": 10}));
        line.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10}));
        line
    })
    .await
    .unwrap();

    // Each read answers 300 ms after its snapshot, and a message marks both windows: each time, 40
    // requests come while the conversation's read is under way, the chat list due after it. Each
    // waits for that one read at most, rather than the whole flush, and none for the chat list's.
    bed.feed.slow_reads(300);
    for round in 0..2u64 {
        let reads = bed.feed.reads();
        let message = text(OPEN, &format!("o{}", round + 1), 2_000 + round as i64, Some(OPEN), "to both windows");
        bed.apply(vec![Change::Message(message)]).await;
        let deadline = Instant::now() + Duration::from_secs(10);
        while bed.feed.reads() == reads {
            assert!(Instant::now() < deadline, "the conversation's read was never asked for");
            tokio::time::sleep(ms(5)).await;
        }
        let burst: String = (0..40u64)
            .map(|n| {
                format!("{}\n", json!({"id": 1_000 * (round + 1) + n, "method": "focus", "params": {"focused": true}}))
            })
            .collect();
        line.stream.write_all(burst.as_bytes()).unwrap();
        let came;
        (line, came) = tokio::task::spawn_blocking(move || {
            let came = line.came(ms(1_200));
            (line, came)
        })
        .await
        .unwrap();
        let chats = came.iter().position(|value| value["method"] == "chats").expect("the chat list's push");
        let before = came[..chats].iter().filter(|value| value["id"].as_u64().is_some()).count();
        assert_eq!(before, 40, "only {before} of the 40 requests went before the chat list's read: {came:?}");
    }
    bed.feed.slow_reads(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn with_64_requests_in_its_lane_a_connection_reads_no_more_and_drops_none() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve_all(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // 64 drafts wait on jaw.db's writer, held meanwhile, and a watch and a focus come after them:
    // with its lane full, jawd reads neither.
    let locked = Locked::on(&bed);
    let (mut line, early) = tokio::task::spawn_blocking(move || {
        for n in 0..64u64 {
            line.ask(100 + n, "draft.set", json!({"chat": OPEN, "text": format!("d{n}")}));
        }
        line.ask(200, "messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10}));
        line.ask(201, "focus", json!({"focused": false}));
        let came = line.came(ms(800));
        (line, came)
    })
    .await
    .unwrap();
    let read: Vec<&Value> = early.iter().filter(|value| value["id"] == 200 || value["id"] == 201).collect();
    assert!(read.is_empty(), "a line was read while 64 requests were in the lane: {read:?}");

    // Once the writer can write, every draft is answered, in order and each a success, and so are
    // the watch and the focus: none was dropped.
    drop(locked);
    let later = tokio::task::spawn_blocking(move || line.came(ms(3_000))).await.unwrap();
    let ids: Vec<u64> = later.iter().filter_map(|value| value["id"].as_u64()).collect();
    let drafts: Vec<u64> = ids.iter().copied().filter(|id| (100..164).contains(id)).collect();
    assert_eq!(drafts, (100..164).collect::<Vec<_>>(), "every request in the lane answered, in order");
    let succeeded = |id: u64| later.iter().any(|value| value["id"] == id && value.get("result").is_some());
    assert!((100..164).chain([200, 201]).all(succeeded), "each a success: {later:?}");
    assert_eq!(draft(&bed).await.as_deref(), Some("d63"));
}

/// What `status`, asked on `asking`, says of every connection's lane: the requests in the lanes,
/// the most one holds, the lanes full and the replies waiting. It asks until the three counts are
/// `want`, or for 10 s.
fn lanes(asking: &mut Line, want: [u64; 3]) -> [u64; 4] {
    let counts = |interfaces: &Value| {
        let count = |name: &str| interfaces[name].as_u64().unwrap_or_else(|| panic!("no {name}: {interfaces}"));
        [count("in_lanes"), count("lane_most"), count("lanes_full"), count("replies_waiting")]
    };
    counts(&interfaces_once(asking, Duration::from_secs(10), |interfaces| {
        let said = counts(interfaces);
        [said[0], said[2], said[3]] == want
    }))
}

/// What `status`, asked on `asking`, says of the interfaces once `done` holds of it, or once
/// `within` has passed.
fn interfaces_once(asking: &mut Line, within: Duration, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + within;
    loop {
        let interfaces = asking.call("status", Value::Null)["interfaces"].clone();
        if done(&interfaces) || Instant::now() > deadline {
            return interfaces;
        }
        std::thread::sleep(ms(20));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_says_how_many_requests_are_in_the_lanes_and_whose_lane_is_full_or_reply_waits() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_all_pushing(&bed).await;
    let (mut line, mut asking) =
        tokio::task::spawn_blocking(move || (Line::open(&socket, None).0, Line::open(&socket, None).0)).await.unwrap();

    // 64 drafts wait on jaw.db's writer, held meanwhile, so a lane is full. Status, asked on a
    // connection of its own, counts itself among the requests in the lanes.
    let locked = Locked::on(&bed);
    let (line, mut asking, full) = tokio::task::spawn_blocking(move || {
        for n in 0..64u64 {
            line.ask(100 + n, "draft.set", json!({"chat": OPEN, "text": format!("d{n}")}));
        }
        let full = lanes(&mut asking, [65, 1, 0]);
        (line, asking, full)
    })
    .await
    .unwrap();
    assert_eq!(full, [65, 64, 1, 0], "in the lanes, the most a lane holds, lanes full, replies waiting");

    // It stops reading while pushes fill jawd's queue to it. Let go, the writer answers its lane:
    // the first reply finds no room and waits, and the others wait behind it.
    fill(&pushes).await;
    drop(locked);
    let (mut asking, waiting) = tokio::task::spawn_blocking(move || {
        let waiting = lanes(&mut asking, [64, 0, 1]);
        (asking, waiting)
    })
    .await
    .unwrap();
    assert_eq!(waiting, [64, 64, 0, 1], "in the lanes, the most a lane holds, lanes full, replies waiting");

    // Once it has gone, nothing of it is counted.
    drop(line);
    let gone = tokio::task::spawn_blocking(move || lanes(&mut asking, [1, 0, 0])).await.unwrap();
    assert_eq!(gone, [1, 64, 0, 0], "in the lanes, the most a lane holds, lanes full, replies waiting");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_push_never_overtakes_a_waiting_watch_result_even_with_room_made_mid_flush() {
    // Never dropped for falling behind, so the window watched while the queue is full lives on.
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(GROUP, "g0", 1_000, Some(&person(DIRECT)), "hi"))]).await;
    let (socket, pushes) = serve_pushing(&bed);
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // It watches nothing yet and stops reading while pushes that aren't for windows fill jawd's
    // queue to it, so no window of its is blocked; then its watch's result finds no room and waits.
    fill(&pushes).await;
    line.ask(50, "messages.watch", json!({"chat": GROUP, "around": "newest", "before": 10}));
    tokio::time::sleep(ms(300)).await;

    // A message makes the window due. A flush that ran for it now would stop before it looked for
    // room while the interface reads, so there would be room by the time it looked.
    let (stopped, go) = bed.feed.pause_next_flush();
    bed.apply(vec![Change::Message(text(GROUP, "g1", 2_000, Some(&person(DIRECT)), "more"))]).await;
    let _ = tokio::time::timeout(ms(1_500), stopped.notified()).await;
    let (mut line, mut came) = tokio::task::spawn_blocking(move || {
        let came = line.came(ms(300));
        (line, came)
    })
    .await
    .unwrap();
    go.notify_one();
    came.extend(tokio::task::spawn_blocking(move || line.came(ms(1_000))).await.unwrap());

    // The result comes first all the same, and the window's push of g1 after it.
    let result = came.iter().position(|value| value["id"] == 50).expect("the watch's result");
    let watch = came[result]["result"]["watch"].as_str().unwrap();
    let lines = about(&came, watch);
    assert_eq!(lines.first().map(|(k, _)| *k), Some(result), "something of {watch} came before its result: {lines:?}");
    assert!(lines.len() > 1, "the window's push of g1 came after it: {lines:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_window_starves_while_the_others_keep_falling_due() {
    let bed = Bed::new(Timing { quiet: ms(1), burst: ms(20), hold: ms(300), behind: Duration::from_secs(60) });
    let chats: Vec<String> = (0..6).map(person).collect();
    let seeding = chats.iter().map(|chat| Change::Message(text(chat, &format!("{chat}-0"), 1_000, Some(chat), "hi")));
    bed.apply(seeding.collect()).await;
    let mut watcher = bed.watcher(100_000);
    let mut watches = vec![watcher.watch("chats.watch", json!({"around": "top", "after": 10})).await];
    for chat in &chats {
        watches.push(watcher.watch("messages.watch", json!({"chat": chat, "around": "newest", "before": 10})).await);
    }

    // Each read takes 6 ms and every chat changes every 3 ms for 2 s, so a window flushed is
    // marked again long before the seven are flushed; one window a flush, each still gets its turn.
    bed.feed.slow_reads(6);
    let (store, flooding) = (bed.store.clone(), chats.clone());
    let flood = tokio::spawn(async move {
        let started = Instant::now();
        let mut n = 0;
        while started.elapsed() < Duration::from_secs(2) {
            n += 1;
            let batch = flooding
                .iter()
                .map(|chat| Change::Message(text(chat, &format!("{chat}-{n}"), 2_000 + n, Some(chat), "more")))
                .collect();
            store.apply(Lane::Live, batch).await.unwrap();
            tokio::time::sleep(ms(3)).await;
        }
    });
    let started = Instant::now();
    let until = started + Duration::from_secs(2);
    let mut last: HashMap<String, Instant> = watches.iter().map(|watch| (watch.clone(), started)).collect();
    let mut longest: HashMap<String, Duration> = HashMap::new();
    while Instant::now() < until
        && let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await
    {
        watcher.client.handle(ready).await;
        let before = watcher.batches.clone();
        watcher.drain();
        let now = Instant::now();
        for watch in &watches {
            if watcher.batches.get(watch) != before.get(watch) {
                let most = longest.entry(watch.clone()).or_default();
                *most = (*most).max(now - last[watch]);
                last.insert(watch.clone(), now);
            }
        }
    }
    let end = Instant::now();
    for watch in &watches {
        let most = longest.entry(watch.clone()).or_default();
        *most = (*most).max(end - last[watch]);
    }
    flood.await.unwrap();
    bed.feed.slow_reads(0);
    let report: Vec<(&String, usize, u128)> = watches
        .iter()
        .map(|watch| (watch, watcher.batches.get(watch).copied().unwrap_or(0), longest[watch].as_millis()))
        .collect();
    for (watch, _, gap) in &report {
        assert!(*gap < 500, "{watch} went {gap} ms without a batch while its chats kept changing: {report:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn replies_coming_down_the_lane_while_one_waits_for_room_each_come_once_in_order() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_all_pushing(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // Twenty drafts wait on the writer, and the interface stops reading while pushes fill jawd's
    // queue to it.
    let locked = Locked::on(&bed);
    for n in 0..20u64 {
        line.ask(100 + n, "draft.set", json!({"chat": OPEN, "text": format!("d{n}")}));
    }
    tokio::time::sleep(ms(200)).await;
    fill(&pushes).await;

    // Let go, the writer answers them all: the first reply finds no room and waits, and the others
    // come back down the lane meanwhile. None takes its place, so each comes once, in order.
    drop(locked);
    tokio::time::sleep(ms(500)).await;
    let came = tokio::task::spawn_blocking(move || line.came(ms(1_500))).await.unwrap();
    let ids: Vec<u64> = came.iter().filter_map(|value| value["id"].as_u64()).collect();
    assert_eq!(ids, (100..120).collect::<Vec<_>>(), "every reply, once and in order");
    assert_eq!(draft(&bed).await.as_deref(), Some("d19"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interface_that_closes_its_writing_side_still_gets_every_reply_and_then_the_connection_ends() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve_all(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // A find waits on jaw.db's readers and ten drafts on its writer, both held meanwhile, when the
    // interface closes its writing side, as a script does once its input ends.
    let readers = hold_readers(&bed);
    let locked = Locked::on(&bed);
    line.ask(10, "find", json!({"query": "hi"}));
    for n in 0..10u64 {
        line.ask(20 + n, "draft.set", json!({"chat": OPEN, "text": format!("d{n}")}));
    }
    line.stream.shutdown(std::net::Shutdown::Write).unwrap();
    tokio::time::sleep(ms(300)).await;
    drop(readers);
    drop(locked);

    // Each is answered, the drafts in order, and only then does jawd end the connection.
    let (came, closed) = tokio::task::spawn_blocking(move || line.until_closed()).await.unwrap();
    let ids: Vec<u64> = came.iter().filter_map(|value| value["id"].as_u64()).collect();
    let drafts: Vec<u64> = ids.iter().copied().filter(|&id| id != 10).collect();
    assert_eq!(drafts, (20..30).collect::<Vec<_>>(), "the drafts' replies, in order: {ids:?}");
    assert_eq!(ids.iter().filter(|&&id| id == 10).count(), 1, "the find's reply: {ids:?}");
    assert!(came.iter().filter(|value| value.get("id").is_some()).all(|value| value.get("result").is_some()));
    assert!(closed, "jawd kept the connection open once everything was answered");
    assert_eq!(draft(&bed).await.as_deref(), Some("d9"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finds_reply_coming_while_another_reply_waits_for_room_neither_takes_its_place_nor_is_lost() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_all_pushing(&bed).await;
    let other = socket.clone();
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // A find waits on jaw.db's readers, held meanwhile. The interface stops reading while pushes
    // fill jawd's queue to it, and the reply to its status finds no room and waits.
    let readers = hold_readers(&bed);
    line.ask(10, "find", json!({"query": "hi"}));
    fill(&pushes).await;
    line.ask(11, "status", Value::Null);
    let waiting = tokio::task::spawn_blocking(move || lanes(&mut Line::open(&other, None).0, [1, 0, 1])).await.unwrap();
    assert_eq!(waiting, [1, 64, 0, 1], "its reply waits: in the lanes, the most, lanes full, replies waiting");

    // Let go, the readers do the find's read, and its reply comes while that one waits.
    let done = bed.store.reads().report().done;
    drop(readers);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let reads = bed.store.reads().report();
        if reads.done > done + 8 && reads.running == 0 && reads.queued_interactive == 0 {
            break;
        }
        assert!(Instant::now() < deadline, "the find never read");
        tokio::time::sleep(ms(10)).await;
    }
    tokio::time::sleep(ms(300)).await;

    // Once the interface reads, the reply that waited comes, then the find's, each once.
    let came = tokio::task::spawn_blocking(move || line.came(ms(700))).await.unwrap();
    let ids: Vec<u64> = came.iter().filter_map(|value| value["id"].as_u64()).collect();
    assert_eq!(ids, [11, 10], "the reply that waited, then the find's");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_and_a_message_find_finds_come_as_their_windows_send_them() {
    let bed = Bed::new(brisk());
    let ana = Person { jids: vec![OPEN.into()], name: Some("Ana Ribeiro".into()), ..Person::default() };
    let group = Chat { id: GROUP.into(), name: Some("Jantar de sexta".into()), ..Chat::default() };
    bed.apply(vec![
        Change::Person(ana),
        Change::Chat(group),
        Change::Message(text(GROUP, "j1", 1_000, Some(OPEN), "o jantar foi ótimo")),
    ])
    .await;
    bed.store.indexed().await.unwrap();
    let socket = serve_all(&bed).await;
    let (chats, messages, found) = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        let chats = line.call("chats.watch", json!({"around": "top", "after": 5}));
        let messages = line.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5}));
        (chats, messages, line.call("find", json!({"query": "jantar"})))
    })
    .await
    .unwrap();

    // Interfaces read a hit's row as they read their windows' rows: the chat list's sort, the
    // sender's person and colour, a message's detail flattened, and nothing of jaw.db's own.
    let hit = |kind: &str| found["results"].as_array().unwrap().iter().find(|hit| hit["kind"] == kind);
    let chat = chats["rows"].as_array().unwrap().iter().find(|row| row["id"] == GROUP);
    assert_eq!(hit("chat").map(|hit| &hit["row"]), chat, "the chat, as chats.watch sends it: {found}");
    let message = messages["rows"].as_array().unwrap().first();
    assert_eq!(hit("message").map(|hit| &hit["row"]), message, "the message, as messages.watch sends it: {found}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_long_request_holds_up_neither_the_lane_nor_watches_and_a_connection_has_64_at_most() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let socket = serve_all(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // 64 stand-ins for a long request, as `media.save` waiting for its download, wait on jaw.db's
    // writer, held meanwhile, and a 65th is refused. A status down the lane and a watch sent after
    // them are answered all the same.
    let locked = Locked::on(&bed);
    let (mut line, early) = tokio::task::spawn_blocking(move || {
        for n in 0..65u64 {
            line.ask(100 + n, "test.wait", Value::Null);
        }
        line.ask(200, "status", Value::Null);
        line.ask(201, "messages.watch", json!({"chat": OPEN, "around": "newest", "before": 10}));
        let came = line.came(ms(1_000));
        (line, came)
    })
    .await
    .unwrap();
    let reply = |id: u64| early.iter().find(|value| value["id"] == id).cloned();
    let (status, watch) = (reply(200), reply(201));
    assert!(status.is_some() && watch.is_some(), "the lane and the watch waited for the long requests: {early:?}");
    let refused = reply(164).expect("the 65th, refused");
    assert_eq!(refused["error"]["code"], code::LIMIT, "{refused}");
    assert_eq!(refused["error"]["data"], json!({"limit": "apart", "most": 64}));
    assert!((100..164).all(|id| reply(id).is_none()), "a long request was answered while it waited: {early:?}");
    let interfaces = &status.unwrap()["result"]["interfaces"];
    assert_eq!((interfaces["apart"].as_u64(), interfaces["apart_most"].as_u64()), (Some(64), Some(64)), "{interfaces}");

    // Let go, each is answered.
    drop(locked);
    let later = tokio::task::spawn_blocking(move || line.came(ms(2_000))).await.unwrap();
    let mut answered: Vec<u64> = later.iter().filter_map(|value| value["id"].as_u64()).collect();
    answered.sort_unstable();
    assert_eq!(answered, (100..164).collect::<Vec<_>>());
    assert!(later.iter().all(|value| value.get("result").is_some()), "{later:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_half_closed_connection_with_long_requests_and_replies_waiting_gets_every_reply_then_ends() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_all_pushing(&bed).await;
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // Five long requests and five drafts wait on jaw.db's writer, held meanwhile, and the
    // interface reads nothing while pushes fill jawd's queue to it, then closes its writing side.
    let locked = Locked::on(&bed);
    for n in 0..5u64 {
        line.ask(10 + n, "test.wait", Value::Null);
        line.ask(20 + n, "draft.set", json!({"chat": OPEN, "text": format!("d{n}")}));
    }
    tokio::time::sleep(ms(200)).await;
    fill(&pushes).await;
    line.stream.shutdown(std::net::Shutdown::Write).unwrap();
    tokio::time::sleep(ms(300)).await;

    // Let go, they're all answered while the first reply waits for room; once the interface
    // reads, every reply comes, the drafts' in order, and only then does the connection end.
    drop(locked);
    tokio::time::sleep(ms(500)).await;
    let (came, closed) = tokio::task::spawn_blocking(move || line.until_closed()).await.unwrap();
    let ids: Vec<u64> = came.iter().filter_map(|value| value["id"].as_u64()).collect();
    let drafts: Vec<u64> = ids.iter().copied().filter(|id| (20..25).contains(id)).collect();
    let mut long: Vec<u64> = ids.iter().copied().filter(|id| (10..15).contains(id)).collect();
    long.sort_unstable();
    assert_eq!(drafts, (20..25).collect::<Vec<_>>(), "the drafts' replies, in order: {ids:?}");
    assert_eq!(long, (10..15).collect::<Vec<_>>(), "the long requests' replies: {ids:?}");
    assert_eq!(ids.len(), 10, "each once: {ids:?}");
    assert!(closed, "jawd kept the connection open once everything was answered");
    assert_eq!(draft(&bed).await.as_deref(), Some("d4"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_half_closed_connections_last_reply_waiting_for_room_still_comes_before_it_ends() {
    let bed = Bed::new(Timing { behind: Duration::from_secs(60), ..brisk() });
    bed.apply(vec![Change::Message(text(OPEN, "o0", 1_000, Some(OPEN), "hi"))]).await;
    let (socket, pushes) = serve_all_pushing(&bed).await;
    let other = socket.clone();
    let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

    // A draft waits on jaw.db's writer, held meanwhile; the interface closes its writing side and
    // reads nothing while pushes fill jawd's queue to it.
    let locked = Locked::on(&bed);
    line.ask(20, "draft.set", json!({"chat": OPEN, "text": "d0"}));
    tokio::time::sleep(ms(200)).await;
    line.stream.shutdown(std::net::Shutdown::Write).unwrap();
    fill(&pushes).await;

    // Let go, the writer answers it, and its reply, the only one owed, waits for room, the
    // connection kept for it.
    drop(locked);
    let waiting = tokio::task::spawn_blocking(move || lanes(&mut Line::open(&other, None).0, [1, 0, 1])).await.unwrap();
    let counted = "in the lanes, the most, lanes full, replies waiting";
    assert_eq!(waiting, [1, 64, 0, 1], "no reply waits, so the connection didn't stay for it: {counted}");

    // Once the interface reads, the reply comes, and only then does the connection end.
    let (came, closed) = tokio::task::spawn_blocking(move || line.until_closed()).await.unwrap();
    let ids: Vec<u64> = came.iter().filter_map(|value| value["id"].as_u64()).collect();
    assert_eq!(ids, [20], "the reply that waited came before the connection ended");
    assert!(closed, "jawd kept the connection open once its reply had gone");
    assert_eq!(draft(&bed).await.as_deref(), Some("d0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_whose_lane_dies_ends_and_nothing_of_it_is_counted() {
    let bed = Bed::new(brisk());
    let socket = serve_all(&bed).await;
    let other = socket.clone();

    // A focused interface's lane task panics on its request; jawd ends the connection, so the
    // interface connects and asks again.
    let (came, closed) = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.call("focus", json!({"focused": true}));
        line.stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        line.ask(5, "test.panic", Value::Null);
        line.until_closed()
    })
    .await
    .unwrap();
    assert!(closed, "jawd kept the connection whose lane died: {came:?}");

    // Once it has ended, it's neither connected nor in front, and none of its requests is counted.
    let interfaces = tokio::task::spawn_blocking(move || {
        interfaces_once(&mut Line::open(&other, None).0, Duration::from_secs(3), |interfaces| {
            interfaces["connected"] == 1
        })
    })
    .await
    .unwrap();
    let counted = (&interfaces["connected"], &interfaces["focused"], &interfaces["in_lanes"]);
    assert_eq!(counted, (&json!(1), &json!(false), &json!(1)), "only the asking status: {interfaces}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_half_closed_connection_whose_long_request_dies_is_told_and_still_ends() {
    let bed = Bed::new(brisk());
    let socket = serve_all(&bed).await;

    // A long request's task panics, and the interface closes its writing side after a status.
    let (came, closed) = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        line.ask(5, "test.panic.apart", Value::Null);
        line.ask(6, "status", Value::Null);
        line.stream.shutdown(std::net::Shutdown::Write).unwrap();
        line.until_closed()
    })
    .await
    .unwrap();

    // The request that died is answered with an error, the status as ever, and then the
    // connection ends.
    let reply = |id: u64| came.iter().find(|value| value["id"] == id);
    assert_eq!(reply(5).map(|value| &value["error"]["code"]), Some(&json!(code::INTERNAL)), "{came:?}");
    assert!(reply(6).is_some_and(|value| value.get("result").is_some()), "{came:?}");
    assert!(closed, "jawd kept the half-closed connection open: {came:?}");
}

mod folders {
    use super::*;
    use crate::store::folders::{Done, Item, Outcome, Presence, Step, Target};
    use crate::store::lists::{self, List};

    const ANA: &str = "351910000002@s.whatsapp.net";
    const RUI: &str = "351910000003@s.whatsapp.net";
    const INES: &str = "351910000004@s.whatsapp.net";
    const CLIMBING: &str = "120363000000000001@g.us";
    const AT: i64 = 1_792_000_000_000;

    async fn step(bed: &Bed, make: impl FnOnce(Done) -> Step) -> (Outcome, i64) {
        let done = Done::default();
        let applied = bed.store.apply_durable(vec![Change::Folders(make(done.clone()))]).await.unwrap();
        (done.take().unwrap(), applied.seq)
    }

    async fn create(bed: &Bed, name: &str, parent: Option<i64>) -> i64 {
        match step(bed, |done| Step::Create { name: name.into(), parent, rule: None, done }).await.0 {
            Outcome::Done(Some(n)) => n,
            other => panic!("{other:?}"),
        }
    }

    /// A window as an interface holds it, and how it watched it.
    struct Held {
        method: &'static str,
        params: Value,
        model: Model,
    }

    impl Held {
        async fn watch(watcher: &mut Watcher, method: &'static str, params: Value) -> Held {
            let result = watcher.call(method, params.clone()).await.unwrap();
            watcher.call("unwatch", json!({"watch": result["watch"]})).await.unwrap();
            let mut model = Model { chats: true, ..Model::default() };
            model.take(&result, "rows");
            Held { method, params, model }
        }

        /// Watches again as an interface coming back does, from the seq it held and with the ids of
        /// the rows it holds, and takes what comes: it must then hold what a fresh watch gives, so
        /// every row it held whose content changed since took a new version.
        async fn come_back(&mut self, watcher: &mut Watcher, what: &str) {
            let mut params = self.params.clone();
            params["since"] = self.model.seq.into();
            params["have"] = json!(self.model.ids());
            let result = watcher.call(self.method, params).await.unwrap();
            watcher.call("unwatch", json!({"watch": result["watch"]})).await.unwrap();
            self.model.take(&result, "rows");
            let fresh = Held::watch(watcher, self.method, self.params.clone()).await;
            self.model.assert_matches(&fresh.model, what);
        }

        fn row(&self, id: &str) -> &Value {
            self.model.rows.get(id).unwrap_or_else(|| panic!("no row {id}: {:?}", self.model.ids()))
        }
    }

    async fn person_n(bed: &Bed, jid: &'static str) -> i64 {
        bed.store
            .reads()
            .query(crate::work::Priority::Interactive, move |db| {
                Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [jid], |r| r.get::<_, i64>(0))?)
            })
            .await
            .unwrap()
    }

    async fn done(bed: &Bed, make: impl FnOnce(Done) -> Step) {
        assert_eq!(step(bed, make).await.0, Outcome::Done(None));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_chat_list_watched_again_from_since_is_sent_every_row_whose_content_changed() {
        let bed = Bed::new(brisk());
        let (bairro, party) = ("120363000000000010@g.us", "120363000000000012@g.us");
        let ana_lid = "100000000002@lid";
        let person = |jid: &str| {
            Change::Person(Person { jids: vec![jid.into()], name: Some("Ana".into()), ..Person::default() })
        };
        bed.apply(vec![
            person(ANA),
            // Her LID, someone else to jaw until WhatsApp says they're one.
            person(ana_lid),
            Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
            Change::Message(text(RUI, "r1", AT + 1, Some(RUI), "boas")),
            Change::Message(Message { live: true, ..text(CLIMBING, "c1", AT + 2, Some(RUI), "sábado?") }),
            Change::Chat(Chat {
                id: bairro.into(),
                kind: Some("community"),
                name: Some("Bairro".into()),
                ..Chat::default()
            }),
            Change::Chat(Chat {
                id: party.into(),
                kind: Some("group"),
                community: Some(bairro.into()),
                ..Chat::default()
            }),
            Change::Message(Message { live: true, ..text(party, "p1", AT + 3, Some(RUI), "festa") }),
        ])
        .await;
        let (ana, lid) = (person_n(&bed, ANA).await, person_n(&bed, ana_lid).await);
        let family = create(&bed, "Family", None).await;
        let work = create(&bed, "Work", None).await;
        done(&bed, |done| Step::File { folder: work, item: Item::Person(lid), done }).await;
        let mut watcher = bed.watcher(64);
        let mut all =
            Held::watch(&mut watcher, "chats.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
        let in_family = json!({"folder": format!("folder:{family}"), "around": "top", "after": 20});
        let mut filed = Held::watch(&mut watcher, "chats.watch", in_family).await;
        let (family_id, work_id) = (format!("folder:{family}"), format!("folder:{work}"));

        bed.apply(vec![Change::Message(Message { live: true, ..text(party, "p2", AT + 10, Some(RUI), "às 9") })]).await;
        all.come_back(&mut watcher, "a message in a community's group").await;
        assert_eq!(all.row(bairro)["children_unread"], json!(2));

        bed.apply(vec![Change::Chat(Chat { id: party.into(), community: Some(String::new()), ..Chat::default() })])
            .await;
        all.come_back(&mut watcher, "a group leaving its community").await;
        assert_eq!(all.row(bairro)["children_unread"], json!(0), "Party's no longer Bairro's");

        bed.apply(vec![Change::Chat(Chat { id: CLIMBING.into(), community: Some(bairro.into()), ..Chat::default() })])
            .await;
        all.come_back(&mut watcher, "a group joining a community").await;
        assert_eq!(all.row(bairro)["children_unread"], json!(1));

        done(&bed, |done| Step::File { folder: family, item: Item::Chat(CLIMBING.into()), done }).await;
        all.come_back(&mut watcher, "a chat filed").await;
        filed.come_back(&mut watcher, "a chat filed, in its folder").await;
        assert_eq!(all.row(CLIMBING)["folders"], json!([family_id]));

        done(&bed, |done| Step::File { folder: family, item: Item::Person(ana), done }).await;
        all.come_back(&mut watcher, "someone filed").await;
        filed.come_back(&mut watcher, "someone filed, in their folder").await;
        assert_eq!(all.row(ANA)["folders"], json!([family_id]), "her chat says where she's filed");

        done(&bed, |done| Step::Unfile { folder: family, item: Item::Chat(CLIMBING.into()), done }).await;
        all.come_back(&mut watcher, "a chat unfiled").await;
        filed.come_back(&mut watcher, "a chat unfiled, from its folder").await;
        assert_eq!(all.row(CLIMBING)["folders"], Value::Null, "filed nowhere");

        let slot = |slot: u8, chat: Option<&str>| {
            let target = chat.map(|chat| Target::Chat(chat.into()));
            move |done| Step::Favourite { slot, target, done }
        };
        done(&bed, slot(2, Some(RUI))).await;
        all.come_back(&mut watcher, "a favourite").await;
        assert_eq!(all.row(RUI)["favourite"], json!(2));
        done(&bed, slot(3, Some(RUI))).await;
        all.come_back(&mut watcher, "a favourite moved to another slot").await;
        assert_eq!(all.row(RUI)["favourite"], json!(3));
        done(&bed, slot(3, Some(ANA))).await;
        all.come_back(&mut watcher, "a favourite's slot given to another chat").await;
        assert_eq!((all.row(RUI)["favourite"].clone(), all.row(ANA)["favourite"].clone()), (Value::Null, json!(3)));
        done(&bed, slot(3, None)).await;
        all.come_back(&mut watcher, "a favourite's slot emptied").await;
        assert_eq!(all.row(ANA)["favourite"], Value::Null);

        // Her LID has no chat, so only where she's filed changes for her chat.
        bed.apply(vec![Change::Person(Person { jids: vec![ANA.into(), ana_lid.into()], ..Person::default() })]).await;
        all.come_back(&mut watcher, "her number and LID joined").await;
        filed.come_back(&mut watcher, "her number and LID joined, in her folder").await;
        assert_eq!(all.row(ANA)["folders"], json!([family_id, work_id]), "filed where her LID was too");

        done(&bed, |done| Step::Delete { folder: work, done }).await;
        all.come_back(&mut watcher, "a folder deleted").await;
        filed.come_back(&mut watcher, "a folder deleted, beside this one").await;
        assert_eq!(all.row(ANA)["folders"], json!([family_id]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_roster_watched_again_from_since_is_sent_every_row_whose_content_changed() {
        let bed = Bed::new(brisk());
        let (ze, ze_lid) = ("351910000005@s.whatsapp.net", "100000000005@lid");
        let person = |jid: &str, name: &str| {
            Change::Person(Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() })
        };
        let avatar = |chat: &str| {
            let file = format!("media/avatars/{}.jpg", chat.replace('@', "_"));
            Change::Chat(Chat { id: chat.into(), avatar: Some(file), avatar_id: Some("1".into()), ..Chat::default() })
        };
        bed.apply(vec![
            person(ANA, "Ana"),
            person(RUI, "Rui"),
            person(INES, "Inês"),
            Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
            // Zé's chat under his LID comes first, then one under his number, each with its picture.
            person(ze_lid, "Zé"),
            Change::Message(text(ze_lid, "z1", AT + 1, Some(ze_lid), "viva")),
            avatar(ze_lid),
            person(ze, "Zé"),
            Change::Message(text(ze, "z2", AT + 2, Some(ze), "então")),
            avatar(ze),
        ])
        .await;
        let (ana, rui, ines) = (person_n(&bed, ANA).await, person_n(&bed, RUI).await, person_n(&bed, INES).await);
        let family = create(&bed, "Family", None).await;
        let mut watcher = bed.watcher(64);
        let mut all =
            Held::watch(&mut watcher, "roster.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
        let in_family = json!({"folder": format!("folder:{family}"), "around": "top", "after": 20});
        let mut filed = Held::watch(&mut watcher, "roster.watch", in_family).await;
        let id = |n: i64| format!("person:{n}");

        bed.apply(vec![avatar(ANA)]).await;
        all.come_back(&mut watcher, "the avatar of someone's chat").await;
        assert_eq!(all.row(&id(ana))["avatar"], json!("media/avatars/351910000002_s.whatsapp.net.jpg"));

        bed.apply(vec![Change::Message(text(RUI, "r1", AT + 3, Some(RUI), "boas"))]).await;
        all.come_back(&mut watcher, "someone's first chat").await;
        assert_eq!(all.row(&id(rui))["chat"], json!(RUI));

        done(&bed, |done| Step::File { folder: family, item: Item::Person(ines), done }).await;
        all.come_back(&mut watcher, "someone filed").await;
        filed.come_back(&mut watcher, "someone filed, in their folder").await;
        done(&bed, |done| Step::File { folder: family, item: Item::Chat(ANA.into()), done }).await;
        all.come_back(&mut watcher, "someone's chat filed").await;
        filed.come_back(&mut watcher, "someone's chat filed, in their folder").await;
        assert_eq!(all.row(&id(ana))["folders"], json!([format!("folder:{family}")]));

        let said = |jid: &str, state| Change::Folders(Step::Presence { jid: jid.into(), state });
        bed.apply(vec![said(RUI, Presence::Online), said(INES, Presence::Away(Some(AT + 5)))]).await;
        all.come_back(&mut watcher, "presence heard").await;
        filed.come_back(&mut watcher, "presence heard, in a folder").await;
        assert_eq!(all.row(&id(rui))["presence"]["state"], json!("online"));
        bed.apply(vec![Change::Folders(Step::Following { people: vec![rui] })]).await;
        all.come_back(&mut watcher, "someone followed").await;
        assert_eq!(all.row(&id(rui))["presence"]["known"], json!(true));
        bed.apply(vec![Change::Folders(Step::Following { people: Vec::new() })]).await;
        all.come_back(&mut watcher, "someone no longer followed").await;
        assert_eq!(all.row(&id(rui))["presence"]["known"], json!(false));
        bed.apply(vec![Change::Folders(Step::Unknown)]).await;
        all.come_back(&mut watcher, "nobody's presence known").await;
        assert_eq!(all.row(&id(rui))["presence"]["state"], json!("hidden"));

        let about = vec![(ANA.to_string(), "Lisboa".to_string())];
        bed.apply(vec![Change::Folders(Step::About { asked: vec![ANA.into()], about, at: AT + 6 })]).await;
        all.come_back(&mut watcher, "an about text").await;
        filed.come_back(&mut watcher, "an about text, in a folder").await;
        assert_eq!(all.row(&id(ana))["about"], json!("Lisboa"));

        // His chat under his number is kept, with its own picture, and the LID one merged into it.
        bed.apply(vec![Change::Person(Person { jids: vec![ze.into(), ze_lid.into()], ..Person::default() })]).await;
        all.come_back(&mut watcher, "his number and LID joined").await;
        while bed.store.count("chat_merges").unwrap() > 0 {
            bed.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        }
        all.come_back(&mut watcher, "his chats merged").await;
        let his = all.model.rows.values().find(|row| row["name"] == "Zé").unwrap();
        assert_eq!(
            (&his["chat"], &his["avatar"]),
            (&json!(ze), &json!("media/avatars/351910000005_s.whatsapp.net.jpg"))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_change_to_the_open_chat_reaches_its_conversation_before_the_tree_and_the_roster() {
        let bed = Bed::new(brisk());
        bed.apply(vec![Change::Message(text(ANA, "a0", AT, Some(ANA), "olá"))]).await;
        let mut watcher = bed.watcher(64);
        // Watched first, so they'd be read first if the order of watching decided.
        let tree = watcher.watch("folders.watch", json!({})).await;
        let roster = watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
        let messages = watcher.watch("messages.watch", json!({"chat": ANA, "around": "newest", "before": 50})).await;
        // Her message counts in the tree, and a contact comes into the roster with it.
        let said = Message { live: true, ..text(ANA, "a1", AT + 1, Some(ANA), "e agora?") };
        let contact = Person { jids: vec![INES.into()], name: Some("Inês".into()), ..Person::default() };
        let seq = bed.apply(vec![Change::Message(said), Change::Person(contact)]).await;
        while watcher.seen < seq || watcher.client.pending() {
            let ready =
                tokio::time::timeout(Duration::from_secs(10), watcher.client.ready()).await.expect("nothing happened");
            if let Ready::Changes(Ok(changes)) = &ready {
                watcher.seen = watcher.seen.max(changes.seq);
            }
            watcher.client.handle(ready).await;
        }
        let mut order = Vec::new();
        while let Ok(line) = watcher.lines.try_recv() {
            let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else {
                panic!("a reply among the pushes")
            };
            order.push(push.params["watch"].as_str().unwrap().to_string());
        }
        assert_eq!(order.first(), Some(&messages), "{order:?}");
        assert!(order.contains(&tree) && order.contains(&roster), "all three changed: {order:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_folder_methods_wait_their_turn_down_the_lane_while_the_tree_and_a_roster_are_answered_at_once() {
        // What comes on `line` until each of `ids` is answered, or `within` has passed.
        fn answered(line: &mut Line, ids: &[u64], within: Duration) -> Vec<Value> {
            let mut came: Vec<Value> = Vec::new();
            let deadline = Instant::now() + within;
            while !ids.iter().all(|id| came.iter().any(|value| value["id"] == *id)) && Instant::now() < deadline {
                came.extend(line.came(ms(100)));
            }
            came
        }
        let bed = Bed::new(brisk());
        bed.apply(vec![
            Change::Person(Person { jids: vec![RUI.into()], name: Some("Rui".into()), ..Person::default() }),
            Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
        ])
        .await;
        let rui = person_n(&bed, RUI).await;
        let family = format!("folder:{}", create(&bed, "Family", None).await);
        let cousins = format!("folder:{}", create(&bed, "Cousins", None).await);
        // Taken out of order, most would be refused: Rui's chat is started, then filed, given a slot
        // and unfiled; Family is renamed, so a new folder can take its name, and Cousins moved into
        // it, before it's deleted.
        let lane = [
            ("chat.start", json!({"person": format!("person:{rui}")})),
            ("folder.add", json!({"folder": family, "item": RUI})),
            ("favourite.set", json!({"slot": 2, "target": RUI})),
            ("folder.remove", json!({"folder": family, "item": RUI})),
            ("folder.rename", json!({"folder": family, "name": "Família"})),
            ("folder.create", json!({"name": "Family"})),
            ("folder.move", json!({"folder": cousins, "parent": family})),
            ("folder.delete", json!({"folder": family})),
            ("folder.smart", json!({"name": "Unread groups", "rule": {"kind": "group", "unread": true}})),
        ];
        let sent: Vec<u64> = (10..).take(lane.len()).collect();
        let socket = serve_all(&bed).await;
        let (mut line, _) = tokio::task::spawn_blocking(move || Line::open(&socket, None)).await.unwrap();

        // Nothing down the lane can be answered while the writer is held.
        let locked = Locked::on(&bed);
        let ids = sent.clone();
        let (mut line, came) = tokio::task::spawn_blocking(move || {
            for ((method, params), id) in lane.into_iter().zip(ids) {
                line.ask(id, method, params);
            }
            line.ask(20, "folders.watch", json!({}));
            line.ask(21, "roster.watch", json!({"folder": "all", "around": "top", "after": 20}));
            let came = answered(&mut line, &[20, 21], Duration::from_secs(10));
            (line, came)
        })
        .await
        .unwrap();
        for id in [20, 21] {
            assert!(
                came.iter().any(|value| value["id"] == id && value["result"]["watch"].is_string()),
                "{id} answered while the writer is held, as a watch: {came:?}"
            );
        }
        let tree = came.iter().find(|value| value["id"] == 20).unwrap();
        assert!(
            tree["result"]["rows"].as_array().unwrap().iter().any(|row| row["id"] == family && row["name"] == "Family"),
            "the tree as jaw.db had it, before the requests sent ahead of it: {tree}"
        );

        drop(locked);
        let ids = sent.clone();
        let came =
            tokio::task::spawn_blocking(move || answered(&mut line, &ids, Duration::from_secs(30))).await.unwrap();
        let replies: Vec<&Value> = came.iter().filter(|value| value.get("id").is_some()).collect();
        let order: Vec<u64> = replies.iter().map(|value| value["id"].as_u64().unwrap()).collect();
        assert_eq!(order, sent, "answered in the order sent: {came:?}");
        for reply in replies {
            assert!(reply.get("result").is_some(), "each in its turn, after what it needed: {reply}");
        }
    }

    /// A folder's chat list as a fresh read gives it, whole, as its window should hold it.
    async fn listed(bed: &Bed, list: List, after: usize) -> Model {
        let rows = bed
            .store
            .reads()
            .query(crate::work::Priority::Interactive, move |db| {
                let total = lists::chat_count(db, &list, AT)?;
                Ok((lists::chats(db, &list, None, after, AT)?, total))
            })
            .await
            .unwrap();
        let (rows, total) = rows;
        let rows = rows.into_iter().map(|row| (row.id.clone(), serde_json::to_value(chat_row(row)).unwrap()));
        Model { chats: true, rows: rows.collect(), total, ..Model::default() }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_folders_chat_list_is_what_a_fresh_read_gives_as_it_is_filed_written_to_and_moved() {
        let bed = Bed::new(brisk());
        let seq = bed
            .apply(vec![
                Change::Person(Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Person::default() }),
                Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
                Change::Message(text(RUI, "r1", AT + 1, Some(RUI), "boas")),
                Change::Message(text(CLIMBING, "c1", AT + 2, Some(RUI), "sábado?")),
            ])
            .await;
        let family = create(&bed, "Family", None).await;
        let cousins = create(&bed, "Cousins", Some(family)).await;
        let mut watcher = bed.watcher(64);
        let list = watcher
            .watch("chats.watch", json!({"folder": format!("folder:{family}"), "around": "top", "after": 10}))
            .await;
        let tree = watcher.watch("folders.watch", json!({})).await;
        watcher.settle(seq).await;
        let ana = bed
            .store
            .reads()
            .query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [ANA], |r| r.get::<_, i64>(0))?)
            })
            .await
            .unwrap();
        let steps: Vec<Box<dyn Fn(Done) -> Step>> = vec![
            Box::new(move |done| Step::File { folder: family, item: Item::Person(ana), done }),
            Box::new(move |done| Step::File { folder: cousins, item: Item::Chat(CLIMBING.into()), done }),
            Box::new(move |done| Step::File { folder: cousins, item: Item::Chat(RUI.into()), done }),
            Box::new(move |done| Step::Move { folder: cousins, parent: None, position: None, done }),
            Box::new(move |done| Step::Move { folder: cousins, parent: Some(family), position: Some(0), done }),
            Box::new(move |done| Step::Unfile { folder: cousins, item: Item::Chat(RUI.into()), done }),
        ];
        for (i, make) in steps.into_iter().enumerate() {
            let (outcome, seq) = step(&bed, make).await;
            assert_eq!(outcome, Outcome::Done(None), "step {i}");
            let seq = bed
                .apply(vec![Change::Message(text(CLIMBING, &format!("x{i}"), AT + 10 + i as i64, Some(RUI), "e"))])
                .await
                .max(seq);
            watcher.settle(seq).await;
            let fresh = listed(&bed, List::Folder(family), 10).await;
            watcher.windows[&list].assert_matches(&fresh, &format!("Family after step {i}"));
            let now = crate::feed::now_ms();
            let folders = bed
                .store
                .reads()
                .query(crate::work::Priority::Interactive, move |db| lists::folders(db, now))
                .await
                .unwrap();
            let shown = &watcher.windows[&tree];
            for row in folders {
                let held = &shown.rows[&row.id];
                assert_eq!(
                    (held["unread"].as_i64(), held["count"].as_i64()),
                    (Some(row.unread), Some(row.count)),
                    "{} after step {i}",
                    row.id
                );
            }
        }
        let row = &watcher.windows[&list].rows[ANA];
        assert_eq!(row["folders"], json!([format!("folder:{family}")]), "her chat says where she's filed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_smart_folders_window_follows_its_rule_as_its_chats_are_answered_and_written_to() {
        let bed = Bed::new(brisk());
        let now = crate::feed::now_ms();
        let day = 86_400_000;
        let seq = bed
            .apply(vec![
                Change::Message(text(ANA, "a1", now - 2 * day, Some(ANA), "olá")),
                Change::Message(text(RUI, "r1", now - 2 * day + 1, Some(RUI), "boas")),
                Change::Message(text(INES, "i1", now - 3_600_000, Some(INES), "já?")),
            ])
            .await;
        let waiting = bed
            .store
            .reads()
            .query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT n FROM folders WHERE name = 'Waiting on me'", [], |r| r.get::<_, i64>(0))?)
            })
            .await
            .unwrap();
        let mut watcher = bed.watcher(64);
        let list = watcher
            .watch("chats.watch", json!({"folder": format!("smart:{waiting}"), "around": "top", "after": 10}))
            .await;
        let tree = watcher.watch("folders.watch", json!({})).await;
        watcher.settle(seq).await;
        let smart = format!("smart:{waiting}");
        assert_eq!(watcher.windows[&list].ids(), [RUI, ANA], "those who wrote a day ago and wait");
        assert_eq!(watcher.windows[&tree].rows[&smart]["count"], json!(2));
        let seq = bed.apply(vec![Change::Message(text(ANA, "a2", now, None, "sim"))]).await;
        watcher.settle(seq).await;
        assert_eq!(watcher.windows[&list].ids(), [RUI], "Ana's answered");
        assert_eq!(watcher.windows[&tree].rows[&smart]["count"], json!(1));
        let seq = bed.apply(vec![Change::Message(text(RUI, "r2", now, Some(RUI), "e então?"))]).await;
        watcher.settle(seq).await;
        assert!(watcher.windows[&list].ids().is_empty(), "Rui's newest is new: he hasn't waited a day");
        assert_eq!(watcher.windows[&tree].rows[&smart]["count"], json!(0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_trees_window_sends_favourites_whole_when_they_change_and_a_deleted_folder_goes() {
        let bed = Bed::new(brisk());
        let seq = bed.apply(vec![Change::Message(text(ANA, "a1", AT, Some(ANA), "olá"))]).await;
        let family = create(&bed, "Family", None).await;
        let mut watcher = bed.watcher(64);
        let tree = watcher.watch("folders.watch", json!({})).await;
        let watched = watcher.call("folders.watch", json!({})).await.unwrap();
        assert_eq!(watched["favourites"], json!([]));
        watcher.settle(seq).await;
        let (_, seq) =
            step(&bed, |done| Step::Favourite { slot: 3, target: Some(Target::Chat(ANA.into())), done }).await;
        watcher.settle(seq).await;
        let (_, seq) = step(&bed, |done| Step::Favourite { slot: 1, target: Some(Target::Folder(family)), done }).await;
        watcher.settle(seq).await;
        assert_eq!(watcher.windows[&tree].rows[&format!("folder:{family}")]["favourite"], json!(1));
        let (_, seq) = step(&bed, |done| Step::Delete { folder: family, done }).await;
        watcher.settle(seq).await;
        assert!(!watcher.windows[&tree].rows.contains_key(&format!("folder:{family}")), "gone from the tree");
        assert!(watcher.windows[&tree].rows.contains_key("all") && watcher.windows[&tree].rows.contains_key("unread"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_trees_pushes_carry_the_favourite_slots_whole_when_they_change_and_only_then() {
        // The tree's pushes up to `seq`, as they come: a window's model keeps none of the slots.
        async fn pushed(watcher: &mut Watcher, tree: &str, seq: i64) -> Vec<Value> {
            let mut came = Vec::new();
            loop {
                while let Ok(line) = watcher.lines.try_recv() {
                    let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else {
                        panic!("a reply among the pushes")
                    };
                    if push.method == "folders" && push.params["watch"] == tree {
                        came.push(push.params);
                    }
                }
                if watcher.seen >= seq && !watcher.client.pending() {
                    return came;
                }
                watcher.turn().await;
            }
        }
        let bed = Bed::new(brisk());
        let seq = bed.apply(vec![Change::Message(text(ANA, "a1", AT, Some(ANA), "olá"))]).await;
        let mut watcher = bed.watcher(64);
        let tree = watcher.watch("folders.watch", json!({})).await;
        pushed(&mut watcher, &tree, seq).await;

        let (_, seq) =
            step(&bed, |done| Step::Favourite { slot: 3, target: Some(Target::Chat(ANA.into())), done }).await;
        let came = pushed(&mut watcher, &tree, seq).await;
        assert!(
            came.iter().any(|push| push["favourites"]
                .as_array()
                .is_some_and(|slots| slots.iter().any(|slot| slot["slot"] == 3 && slot["chat"] == ANA))),
            "the slots, Ana's chat on 3: {came:?}"
        );

        let said = Message { live: true, ..text(ANA, "a2", AT + 1, Some(ANA), "e agora?") };
        let seq = bed.apply(vec![Change::Message(said)]).await;
        let came = pushed(&mut watcher, &tree, seq).await;
        assert!(!came.is_empty(), "her message counts in the tree");
        assert!(came.iter().all(|push| push.get("favourites").is_none()), "the slots stay as they were: {came:?}");

        // jaw keeps the slots it was last sent, so an emptied one must come too.
        let (_, seq) = step(&bed, |done| Step::Favourite { slot: 3, target: None, done }).await;
        let came = pushed(&mut watcher, &tree, seq).await;
        assert!(came.iter().any(|push| push["favourites"] == json!([])), "the slots, none left: {came:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_roster_puts_whos_around_first_then_by_last_seen_and_follows_what_whatsapp_says() {
        let bed = Bed::new(brisk());
        let person = |jid: &str, name: &str| {
            Change::Person(Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() })
        };
        let seq = bed.apply(vec![person(ANA, "Ana"), person(RUI, "Rui"), person(INES, "Inês")]).await;
        let mut watcher = bed.watcher(64);
        let roster = watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 10})).await;
        watcher.settle(seq).await;
        let order = |watcher: &Watcher| {
            let mut rows: Vec<&Value> = watcher.windows[&roster].rows.values().collect();
            rows.sort_by(|a, b| a["sort"].as_str().cmp(&b["sort"].as_str()));
            rows.iter().map(|row| row["name"].as_str().unwrap().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(order(&watcher), ["Ana", "Inês", "Rui"], "by name while nobody's known");
        let said = |jid: &str, state| Change::Folders(Step::Presence { jid: jid.into(), state });
        let seq = bed
            .apply(vec![
                said(RUI, Presence::Online),
                said(INES, Presence::Away(Some(AT))),
                said(ANA, Presence::Away(Some(AT + 5))),
            ])
            .await;
        watcher.settle(seq).await;
        assert_eq!(order(&watcher), ["Rui", "Ana", "Inês"], "around, then the most recently seen");
        let rui = watcher.windows[&roster].rows.values().find(|row| row["name"] == "Rui").unwrap().clone();
        assert_eq!(rui["presence"], json!({"state": "online", "known": false}), "heard, but not followed now");
        let mut offline = bed.watcher(64);
        let around = offline
            .watch("roster.watch", json!({"folder": "all", "around": "top", "after": 10, "offline": false}))
            .await;
        assert_eq!(offline.windows[&around].rows.len(), 1, "only who's around");
    }

    /// Ana, with her chat, and each way her presence changes in turn, with what her roster row
    /// says then.
    async fn presence_changes(bed: &Bed) -> (String, Vec<(&'static str, Change, Value)>) {
        let ana = Change::Person(Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Person::default() });
        bed.apply(vec![ana, Change::Message(text(ANA, "a1", AT, Some(ANA), "olá"))]).await;
        let ana = person_n(bed, ANA).await;
        let said = |state| Change::Folders(Step::Presence { jid: ANA.into(), state });
        let following = |people| Change::Folders(Step::Following { people });
        let seen = |known| json!({"state": "seen", "at": AT + 5, "known": known});
        let changes = vec![
            ("online", said(Presence::Online), json!({"state": "online", "known": false})),
            ("gone away", said(Presence::Away(Some(AT + 5))), seen(false)),
            ("followed", following(vec![ana]), seen(true)),
            ("back online", said(Presence::Online), json!({"state": "online", "known": true})),
            ("no longer followed", following(Vec::new()), json!({"state": "online", "known": false})),
            ("nobody's presence known", Change::Folders(Step::Unknown), seen(false)),
        ];
        (format!("person:{ana}"), changes)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_roster_is_pushed_each_change_of_someones_presence() {
        let bed = Bed::new(brisk());
        let (ana, changes) = presence_changes(&bed).await;
        let mut watcher = bed.watcher(64);
        let roster = watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 10})).await;
        for (what, change, shows) in changes {
            let pushed = watcher.batches.get(&roster).copied().unwrap_or(0);
            let seq = bed.apply(vec![change]).await;
            watcher.settle(seq).await;
            assert!(watcher.batches.get(&roster).copied().unwrap_or(0) > pushed, "{what} reaches the roster");
            assert_eq!(watcher.windows[&roster].rows[&ana]["presence"], shows, "{what}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_roster_watched_again_from_since_is_sent_each_change_of_someones_presence() {
        let bed = Bed::new(brisk());
        let (ana, changes) = presence_changes(&bed).await;
        let mut watcher = bed.watcher(64);
        let mut all =
            Held::watch(&mut watcher, "roster.watch", json!({"folder": "all", "around": "top", "after": 10})).await;
        for (what, change, shows) in changes {
            bed.apply(vec![change]).await;
            all.come_back(&mut watcher, what).await;
            assert_eq!(all.row(&ana)["presence"], shows, "{what}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_communitys_groups_come_with_it_each_with_its_own_sort_however_far_down_that_is() {
        let bed = Bed::new(brisk());
        let community = "120363000000000010@g.us";
        let (news, party) = ("120363000000000011@g.us", "120363000000000012@g.us");
        let mut changes = vec![
            Change::Chat(Chat {
                id: community.into(),
                kind: Some("community"),
                name: Some("Bairro".into()),
                ..Chat::default()
            }),
            Change::Chat(Chat {
                id: news.into(),
                kind: Some("announcement"),
                community: Some(community.into()),
                ..Chat::default()
            }),
            Change::Chat(Chat {
                id: party.into(),
                kind: Some("group"),
                community: Some(community.into()),
                ..Chat::default()
            }),
            Change::Message(Message { live: true, ..text(news, "n1", AT, Some(RUI), "avisos") }),
        ];
        // Chats newer than the announcement group, so it would fall out of a window of three.
        for i in 0..4 {
            changes.push(Change::Message(text(
                &format!("35191000010{i}@s.whatsapp.net"),
                &format!("m{i}"),
                AT + 10 + i,
                Some(RUI),
                "x",
            )));
        }
        changes.push(Change::Message(Message { live: true, ..text(party, "p1", AT + 100, Some(RUI), "festa") }));
        let seq = bed.apply(changes).await;
        let mut watcher = bed.watcher(64);
        let list = watcher.watch("chats.watch", json!({"folder": "all", "around": "top", "after": 3})).await;
        watcher.settle(seq).await;
        let ids = watcher.windows[&list].ids();
        assert_eq!(ids.len(), 5, "the community with its two groups, and two chats: {ids:?}");
        assert_eq!(&ids[..2], [community, party], "the community just above its newest group: {ids:?}");
        assert_eq!(ids.last().map(String::as_str), Some(news), "its announcement group, by its own sort: {ids:?}");
        let rows = &watcher.windows[&list].rows;
        assert_eq!(rows[news]["community"], json!(community));
        assert_eq!(rows[community]["children_unread"], json!(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn whose_presence_the_windows_show_is_an_open_chats_person_first_then_a_rosters_people() {
        let bed = Bed::new(brisk());
        let person = |jid: &str, name: &str| {
            Change::Person(Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() })
        };
        let seq = bed
            .apply(vec![
                person(ANA, "Ana"),
                person(RUI, "Rui"),
                Change::Message(text(INES, "i1", AT, Some(INES), "olá")),
            ])
            .await;
        let mut watcher = bed.watcher(64);
        let roster = watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 10})).await;
        watcher.watch("messages.watch", json!({"chat": INES, "around": "newest", "before": 10})).await;
        watcher.settle(seq).await;
        let (open, listed) = bed.feed.showing();
        let ines = bed
            .store
            .reads()
            .query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [INES], |r| r.get::<_, i64>(0))?)
            })
            .await
            .unwrap();
        assert_eq!(open, [ines], "the open chat's person");
        assert_eq!(listed.len(), 2, "and the roster's others, each once: {listed:?}");
        watcher.call("unwatch", json!({"watch": roster})).await.unwrap();
        assert_eq!(bed.feed.showing(), (vec![ines], Vec::new()), "a window unwatched shows nobody");
    }

    /// A message row's v is the newest of its message's and those of the people it names, so only
    /// what changes how a row names someone may move their people.v: anything else that did would
    /// send every row naming them again to every window holding it.
    #[tokio::test(flavor = "multi_thread")]
    async fn no_message_row_is_sent_again_for_presence_an_about_text_filing_or_a_chat_only_for_a_new_name() {
        let bed = Bed::new(brisk());
        let mut watcher = bed.watcher(64);
        // Ana sends in her chat and in Climbing, where Rui mentions her and quotes her.
        bed.apply(vec![
            Change::Person(named(ANA, "Ana")),
            Change::Person(named(RUI, "Rui")),
            Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
            Change::Message(text(CLIMBING, "c1", AT + 1, Some(ANA), "sábado?")),
            Change::Message(mentioning(CLIMBING, "c2", AT + 2, RUI, ANA)),
            Change::Message(quoting(CLIMBING, "c3", AT + 3, RUI, "c1", ANA)),
        ])
        .await;
        let (ana, rui) = (person_n(&bed, ANA).await, person_n(&bed, RUI).await);
        let family = create(&bed, "Family", None).await;
        let messages = |chat: &str| Want::Messages { chat: chat.into(), anchor: None, before: 10, after: 0 };
        let watched = watch_all(&mut watcher, &[messages(ANA), messages(CLIMBING)]).await;
        let mut last = bed.apply(Vec::new()).await;
        watcher.settle(last).await;
        let versions = |watcher: &Watcher| -> HashMap<String, Value> {
            watched
                .iter()
                .flat_map(|(watch, _)| {
                    watcher.windows[watch].rows.iter().map(|(id, row)| (id.clone(), row["v"].clone()))
                })
                .collect()
        };
        let held = versions(&watcher);
        assert_eq!(held.len(), 4, "{held:?}");

        let said = |state| Change::Folders(Step::Presence { jid: ANA.into(), state });
        let following = |people| Change::Folders(Step::Following { people });
        let about = vec![(ANA.to_string(), "Lisboa".to_string())];
        let avatar = Some("media/avatars/ana.jpg".to_string());
        let changes = vec![
            ("her online", said(Presence::Online)),
            ("her followed", following(vec![ana])),
            ("her no longer followed", following(Vec::new())),
            ("her gone away", said(Presence::Away(Some(AT + 5)))),
            ("her followed again", following(vec![ana])),
            ("nobody's presence known", Change::Folders(Step::Unknown)),
            ("her about text", Change::Folders(Step::About { asked: vec![ANA.into()], about, at: AT + 6 })),
            (
                "her filed",
                Change::Folders(Step::File { folder: family, item: Item::Person(ana), done: Done::default() }),
            ),
            ("his one-to-one chat", Change::Chat(Chat { id: RUI.into(), ..Chat::default() })),
            (
                "her chat's avatar",
                Change::Chat(Chat { id: ANA.into(), avatar, avatar_id: Some("1".into()), ..Chat::default() }),
            ),
        ];
        for (what, change) in changes {
            let pushed = watcher.batches.clone();
            let seq = bed.apply(vec![change]).await;
            assert!(seq > last, "{what} wrote nothing");
            last = seq;
            watcher.settle(seq).await;
            for (watch, _) in &watched {
                assert_eq!(watcher.batches.get(watch), pushed.get(watch), "{what} sent {watch} message rows again");
            }
            assert_eq!(versions(&watcher), held, "{what}");
            check(&bed, &watcher, &watched, what).await;
        }
        for (who, n) in [("Ana", ana), ("Rui", rui)] {
            let (v, roster_v) = bed
                .store
                .reads()
                .query(crate::work::Priority::Interactive, move |db| {
                    Ok(db.query_row("SELECT v, roster_v FROM people WHERE n = ?1", [n], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
                    })?)
                })
                .await
                .unwrap();
            assert!(roster_v > v, "{who}'s roster row took the changes ({roster_v}) and their v didn't ({v})");
        }

        // Her new name is in every one of those rows, each sent again with the rename's seq.
        let renamed = bed.apply(vec![Change::Person(named(ANA, "Ana Ribeiro"))]).await;
        watcher.settle(renamed).await;
        assert!(versions(&watcher).values().all(|v| *v == renamed), "{:?}", versions(&watcher));
        let climbing = &watcher.windows[&watched[1].0];
        assert_eq!(
            (&climbing.rows["c2"]["spans"][1]["s"], &climbing.rows["c3"]["reply"]["from"]),
            (&json!("@Ana Ribeiro"), &json!("Ana Ribeiro"))
        );
        check(&bed, &watcher, &watched, "her renamed").await;
    }

    /// Presence, following and nobody's presence known read again only the rosters and the
    /// conversation of the person's own chat, which says their presence, whatever else is watched.
    #[tokio::test(flavor = "multi_thread")]
    async fn presence_reads_again_only_the_rosters_and_the_persons_own_conversation() {
        let bed = Bed::new(brisk());
        let mut watcher = bed.watcher(64);
        bed.apply(vec![
            Change::Person(named(ANA, "Ana")),
            Change::Person(named(RUI, "Rui")),
            Change::Message(text(ANA, "a1", AT, Some(ANA), "olá")),
            Change::Message(text(CLIMBING, "c1", AT + 1, Some(ANA), "sábado?")),
            Change::Message(mentioning(CLIMBING, "c2", AT + 2, RUI, ANA)),
        ])
        .await;
        let ana = person_n(&bed, ANA).await;
        let family = create(&bed, "Family", None).await;
        done(&bed, |done| Step::File { folder: family, item: Item::Person(ana), done }).await;
        watcher.watch("chats.watch", json!({"folder": "all", "around": "top", "after": 50})).await;
        watcher.watch("chats.watch", json!({"folder": format!("folder:{family}"), "around": "top", "after": 50})).await;
        watcher.watch("folders.watch", json!({})).await;
        watcher.watch("messages.watch", json!({"chat": ANA, "around": "newest", "before": 10})).await;
        watcher.watch("messages.watch", json!({"chat": CLIMBING, "around": "newest", "before": 10})).await;
        watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
        watcher
            .watch("roster.watch", json!({"folder": format!("folder:{family}"), "around": "top", "after": 20}))
            .await;
        let seq = bed.apply(Vec::new()).await;
        watcher.settle(seq).await;
        let said = |state| Change::Folders(Step::Presence { jid: ANA.into(), state });
        let following = |people| Change::Folders(Step::Following { people });
        for (what, change) in [
            ("online", said(Presence::Online)),
            ("followed", following(vec![ana])),
            ("gone away", said(Presence::Away(Some(AT + 5)))),
            ("no longer followed", following(Vec::new())),
            ("back online", said(Presence::Online)),
            ("nobody's presence known", Change::Folders(Step::Unknown)),
        ] {
            let before = bed.feed.reads();
            let seq = bed.apply(vec![change]).await;
            watcher.settle(seq).await;
            assert_eq!(bed.feed.reads() - before, 3, "{what}: only the two rosters and her chat are read again");
        }
    }
}

/// Long enough that nothing fades in a test that isn't about fading.
const NEVER: Duration = Duration::from_secs(300);
/// The most a change of typing takes to reach a watcher on a loaded machine.
const TYPED: Duration = Duration::from_secs(15);

/// A feed whose typing fades after `fade` and is told at most every 20 ms, WhatsApp heard while the
/// sender returned says so; and Ana's chat (`OPEN`), Lisbon Climbing (`GROUP`), where Rui writes and
/// Zé is a member, and five chats more, newest first.
async fn typing_bed(fade: Duration) -> (Bed, Typing, watch::Sender<bool>) {
    typing_bed_paced(fade, brisk()).await
}

/// `typing_bed`, its windows paced by `timing`.
async fn typing_bed_paced(fade: Duration, timing: Timing) -> (Bed, Typing, watch::Sender<bool>) {
    typing_bed_of(Typing::new(typing::Timing { fade, pace: ms(20) }), timing).await
}

/// `typing_bed`, who's typing told from `typing` and its windows paced by `timing`.
async fn typing_bed_of(typing: Typing, timing: Timing) -> (Bed, Typing, watch::Sender<bool>) {
    let mut bed = Bed::new(timing);
    bed.feed = bed.feed.with_typing(typing.clone());
    let hearing = watch::channel(true).0;
    let me = watch::channel(crate::convert::Me::new(["351919999999@s.whatsapp.net".to_string()])).0;
    typing.start(bed.store.clone(), me.subscribe(), hearing.subscribe());
    let (rui, ze) = (person(9), person(10));
    let mut changes = vec![
        Change::Person(named(OPEN, "Ana")),
        Change::Person(named(&rui, "Rui")),
        Change::Person(named(&ze, "Zé")),
        Change::Chat(Chat {
            id: GROUP.into(),
            kind: Some("group"),
            name: Some("Lisbon Climbing".into()),
            ..Chat::default()
        }),
    ];
    let chats = [OPEN.to_string(), GROUP.to_string(), person(1), person(2), person(3), person(4), person(5)];
    for (n, chat) in chats.iter().enumerate() {
        let sender = if chat == GROUP { rui.clone() } else { chat.clone() };
        changes.push(Change::Message(text(chat, &format!("t{n}"), 10_000 - 100 * n as i64, Some(&sender), "olá")));
    }
    bed.apply(changes).await;
    (bed, typing, hearing)
}

/// Who connections are told is typing in `chat`, by name.
fn typing_names(typing: &Typing, chat: &str) -> Vec<String> {
    typing.watching().now(chat).map(|who| who.iter().map(|t| t.name.clone()).collect()).unwrap_or_default()
}

/// Waits until connections are told `names` are typing in `chat`.
async fn typed(typing: &Typing, chat: &str, names: &[&str]) {
    let end = Instant::now() + TYPED;
    while typing_names(typing, chat) != names {
        assert!(Instant::now() < end, "{chat} was never told {names:?}, only {:?}", typing_names(typing, chat));
        tokio::time::sleep(ms(5)).await;
    }
}

/// What an answer or a model says is typing, by chat and name.
fn names_in(typing: &serde_json::Map<String, Value>) -> BTreeMap<String, Vec<String>> {
    let names = |who: &Value| who.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    typing.iter().map(|(chat, who)| (chat.clone(), names(who))).collect()
}

fn answered_typing(result: &Value) -> BTreeMap<String, Vec<String>> {
    result.get("typing").map_or_else(BTreeMap::new, |typing| names_in(typing.as_object().unwrap()))
}

fn want(typing: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    typing.iter().map(|(chat, who)| (chat.to_string(), who.iter().map(|n| n.to_string()).collect())).collect()
}

impl Watcher {
    /// Who its interface holds as typing, by chat and name.
    fn typing_names(&self) -> BTreeMap<String, Vec<String>> {
        names_in(&self.typing.clone().into_iter().collect())
    }

    /// Runs the client, its interface reading, until that holds `typing` as who's typing.
    async fn until_typing(&mut self, typing: &[(&str, &[&str])]) {
        let want = want(typing);
        let end = Instant::now() + TYPED;
        loop {
            self.drain();
            if self.typing_names() == want {
                return;
            }
            assert!(Instant::now() < end, "its interface was never told {want:?}, only {:?}", self.typing_names());
            if let Ok(ready) = tokio::time::timeout_at(end.into(), self.client.ready()).await {
                if let Ready::Changes(Ok(changes)) = &ready {
                    self.seen = self.seen.max(changes.seq);
                }
                self.client.handle(ready).await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_says_whos_typing_in_its_chats_and_a_typing_push_follows_each_change_there_and_nowhere_else() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    typing.heard(OPEN, OPEN, "", Composing);
    typed(&typing, OPEN, &["Ana"]).await;
    let mut watcher = bed.watcher(64);
    let list = watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    assert_eq!(watcher.windows[&list].ids(), [OPEN.to_string(), GROUP.into(), person(1)]);
    assert_eq!(watcher.typing_names(), want(&[(OPEN, &["Ana"])]), "the answer says who's typing now");
    let ana = &watcher.typing[OPEN][0];
    assert!(ana["id"].as_str().unwrap().starts_with("person:") && ana["state"] == "typing", "{ana}");
    let conversation = watcher.watch("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5})).await;

    // Rui records in the group, which both windows show; someone types in a chat neither shows,
    // whose typing is never pushed (`Watcher::take` checks); and Ana stops.
    typing.heard(GROUP, &person(9), "", Recording);
    typing.heard(&person(5), &person(5), "", Composing);
    typing.heard(OPEN, OPEN, "", Paused);
    watcher.until_typing(&[(GROUP, &["Rui"])]).await;
    assert_eq!(watcher.typing[GROUP][0]["state"], "recording");
    let sent: Vec<&str> = watcher.told.iter().map(|(chat, _)| chat.as_str()).collect();
    assert!(sent.iter().all(|chat| [OPEN, GROUP].contains(chat)), "{sent:?}");
    assert!(sent.contains(&OPEN) && sent.contains(&GROUP), "{sent:?}");
    assert!(typing.stats().sent >= 2, "{:?}", typing.stats());

    // With the conversation unwatched, the list still shows the group, so it's still told.
    watcher.call("unwatch", json!({"watch": conversation})).await.unwrap();
    typing.heard(GROUP, &person(10), "", Composing);
    watcher.until_typing(&[(GROUP, &["Rui", "Zé"])]).await;

    // Moved down the list, the window's answer says who's typing in the chats it shows now.
    let moved = json!({"watch": list, "around": person(4), "before": 1, "after": 1});
    let moved = watcher.call("watch.move", moved).await.unwrap();
    assert_eq!(watcher.windows[&list].ids(), [person(3), person(4), person(5)]);
    assert_eq!(answered_typing(&moved), want(&[(&person(5), &["+351910000005"])]), "{moved}");
    assert_eq!(watcher.typing_names(), want(&[(&person(5), &["+351910000005"])]));
}

#[tokio::test(flavor = "multi_thread")]
async fn watching_again_from_since_says_whos_typing_now_as_a_fresh_watch_does_whatever_changed_meanwhile() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    let socket = serve(&bed);
    typing.heard(OPEN, OPEN, "", Composing);
    typing.heard(GROUP, &person(9), "", Composing);
    typed(&typing, OPEN, &["Ana"]).await;
    typed(&typing, GROUP, &["Rui"]).await;
    let list = json!({"around": "top", "after": 3});
    let conversation = json!({"chat": GROUP, "around": "newest", "before": 5});

    // An interface watches its chat list and the group, then goes.
    let (first_list, first_conversation, since) = tokio::task::spawn_blocking({
        let (socket, list, conversation) = (socket.clone(), list.clone(), conversation.clone());
        move || {
            let (mut line, welcome) = Line::open(&socket, None);
            let first_list = line.call("chats.watch", list);
            let first_conversation = line.call("messages.watch", conversation);
            (first_list, first_conversation, welcome["seq"].as_u64().unwrap())
        }
    })
    .await
    .unwrap();
    assert_eq!(answered_typing(&first_list), want(&[(OPEN, &["Ana"]), (GROUP, &["Rui"])]));
    assert_eq!(answered_typing(&first_conversation), want(&[(GROUP, &["Rui"])]));

    // While it's gone, Rui's message in the group ends his typing there and Zé records, someone
    // starts typing in a chat whose row doesn't change, and Ana stops.
    typing.message(GROUP, &person(9), "");
    bed.apply(vec![Change::Message(text(GROUP, "later", 20_000, Some(&person(9)), "a caminho"))]).await;
    typing.heard(GROUP, &person(10), "", Recording);
    typing.heard(&person(1), &person(1), "", Composing);
    typing.heard(OPEN, OPEN, "", Paused);
    typed(&typing, GROUP, &["Zé"]).await;
    typed(&typing, &person(1), &["+351910000001"]).await;
    typed(&typing, OPEN, &[]).await;

    // Back, it watches again from `since`, saying what it holds; a fresh interface watches the same.
    let ids = |result: &Value| -> Vec<String> {
        result["rows"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
    };
    let (held_list, held_conversation) = (ids(&first_list), ids(&first_conversation));
    let (again_list, again_conversation, fresh_list, fresh_conversation) = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, Some(since));
        let mut list_again = list.clone();
        list_again["have"] = json!(held_list);
        let mut conversation_again = conversation.clone();
        conversation_again["have"] = json!(held_conversation);
        let again_list = line.call("chats.watch", list_again);
        let again_conversation = line.call("messages.watch", conversation_again);
        let (mut fresh, _) = Line::open(&socket, None);
        (again_list, again_conversation, fresh.call("chats.watch", list), fresh.call("messages.watch", conversation))
    })
    .await
    .unwrap();
    let now = want(&[(GROUP, &["Zé"]), (&person(1), &["+351910000001"])]);
    assert_eq!(answered_typing(&fresh_list), now, "a fresh watch says how things are now");
    assert_eq!(answered_typing(&again_list), answered_typing(&fresh_list), "and so does one from since");
    assert_eq!(answered_typing(&again_conversation), answered_typing(&fresh_conversation));
    assert_eq!(answered_typing(&again_conversation), want(&[(GROUP, &["Zé"])]));
    assert!(!ids(&again_list).contains(&person(1)), "its row is left out, unchanged since: {again_list}");
    assert_eq!(again_list["typing"][person(1)][0]["state"], "typing", "but who's typing there isn't");
    assert_eq!(again_list["typing"][GROUP][0]["state"], "recording");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_that_comes_into_a_window_is_told_whos_typing_there_just_after_the_push_that_brings_it() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    let mut watcher = bed.watcher(64);
    let list = watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    typing.heard(&person(5), &person(5), "", Composing);
    typing.heard(&person(1), &person(1), "", Composing);
    watcher.until_typing(&[(&person(1), &["+351910000001"])]).await;

    // Pinned, the fifth chat comes to the top, and the window lets go of the third.
    let seq = bed.apply(vec![Change::Chat(Chat { id: person(5), pinned: Some(1), ..Chat::default() })]).await;
    watcher.settle(seq).await;
    watcher.until_typing(&[(&person(5), &["+351910000005"])]).await;
    assert_eq!(watcher.windows[&list].ids(), [person(5), OPEN.into(), GROUP.into()]);
    let brought = watcher.pushes.iter().position(|(method, params)| {
        method == "chats" && params["upsert"].as_array().is_some_and(|rows| rows.iter().any(|r| r["id"] == person(5)))
    });
    let told = watcher.pushes.iter().position(|(method, chat)| method == "typing" && chat == &json!(person(5)));
    assert!(
        brought.is_some() && told > brought,
        "its typing came before the push that brought it: {:?}",
        watcher.pushes
    );
    // The chat that left is forgotten, by the interface as by jawd: nothing more comes for it while
    // it's out, as it stops and then records,
    typing.heard(&person(1), &person(1), "", Paused);
    typing.heard(OPEN, OPEN, "", Composing);
    watcher.until_typing(&[(&person(5), &["+351910000005"]), (OPEN, &["Ana"])]).await;
    typed(&typing, &person(1), &[]).await;
    typing.heard(&person(1), &person(1), "", Recording);
    typed(&typing, &person(1), &["+351910000001"]).await;
    watcher.run_for(ms(100)).await;
    assert_eq!(watcher.told.iter().filter(|(chat, _)| chat == &person(1)).count(), 1, "{:?}", watcher.told);
    // and once it's back, unpinned, it's told who's typing there as any chat brought in is.
    let seq = bed.apply(vec![Change::Chat(Chat { id: person(5), pinned: Some(0), ..Chat::default() })]).await;
    watcher.settle(seq).await;
    watcher.until_typing(&[(OPEN, &["Ana"]), (&person(1), &["+351910000001"])]).await;
    assert_eq!(watcher.windows[&list].ids(), [OPEN.to_string(), GROUP.into(), person(1)]);
    assert_eq!(watcher.typing[&person(1)][0]["state"], "recording", "{:?}", watcher.told);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_falls_behind_the_typing_told_looks_again_at_every_chat_its_windows_show() {
    // Each change is told at once, and a connection keeps the last two rounds it hasn't taken.
    let typing = Typing::keeping(typing::Timing { fade: NEVER, pace: ms(1) }, 2);
    let (bed, typing, _hearing) = typing_bed_of(typing, brisk()).await;
    let mut watcher = bed.watcher(64);
    watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    typing.heard(OPEN, OPEN, "", Composing);
    watcher.until_typing(&[(OPEN, &["Ana"])]).await;

    // While the connection takes nothing, Ana stops, and six rounds more are told, each in a chat
    // its window doesn't show, so the round that told hers is gone by the time it looks.
    typing.heard(OPEN, OPEN, "", Paused);
    typed(&typing, OPEN, &[]).await;
    let unshown = [person(2), person(3), person(4), person(5)];
    for (n, chat) in unshown.iter().chain(&unshown[..2]).enumerate() {
        let name = format!("+{}", chat.trim_end_matches("@s.whatsapp.net"));
        let (state, now) = if n < 4 { (Composing, vec![name.as_str()]) } else { (Paused, vec![]) };
        typing.heard(chat, chat, "", state);
        typed(&typing, chat, &now).await;
    }

    // Once it takes them, it finds it fell behind and tells its interface how things are in every
    // chat its window shows: nobody typing in Ana's, and nothing for those that didn't change.
    watcher.until_typing(&[]).await;
    watcher.run_for(ms(100)).await;
    let told: Vec<(&str, usize)> =
        watcher.told.iter().map(|(chat, who)| (chat.as_str(), who.as_array().map_or(0, Vec::len))).collect();
    assert_eq!(told, [(OPEN, 1), (OPEN, 0)], "{:?}", watcher.told);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_whose_interface_reads_nothing_holds_one_typing_for_each_chat_and_ends_on_the_newest() {
    // Its windows aren't dropped for falling behind while it reads nothing.
    let (bed, typing, _hearing) = typing_bed_paced(NEVER, Timing { behind: Duration::from_secs(60), ..brisk() }).await;
    let room = 2;
    let mut watcher = bed.watcher(room);
    watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    watcher.watch("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5})).await;
    // A hundred changes in each of two chats it shows, while its interface reads nothing.
    for n in 0..100 {
        typing.heard(GROUP, &person(9 + n % 2), "", if n % 3 == 0 { Recording } else { Composing });
        typing.heard(OPEN, OPEN, "", if n % 2 == 0 { Composing } else { Paused });
        let until = Instant::now() + ms(4);
        while let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await {
            watcher.client.handle(ready).await;
        }
        assert!(watcher.lines.len() <= room);
    }
    // jawd tells the newest, which the connection takes while its interface still reads nothing.
    typed(&typing, GROUP, &["Rui", "Zé"]).await;
    typed(&typing, OPEN, &[]).await;
    let until = Instant::now() + ms(100);
    while let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await {
        watcher.client.handle(ready).await;
    }
    assert_eq!(watcher.lines.len(), room, "its queue is full");
    // It reads again, a line at a time: what's owed comes as there's room, one push a chat, as
    // things are now.
    let newest = want(&[(GROUP, &["Rui", "Zé"])]);
    let end = Instant::now() + TYPED;
    while watcher.typing_names() != newest {
        assert!(Instant::now() < end, "its interface was left with {:?}: {:?}", watcher.typing_names(), watcher.told);
        if let Ok(line) = watcher.lines.try_recv() {
            watcher.take(&line);
        }
        if let Ok(ready) = tokio::time::timeout(ms(10), watcher.client.ready()).await {
            watcher.client.handle(ready).await;
        }
    }
    tokio::time::sleep(ms(100)).await;
    watcher.run_for(ms(100)).await;
    watcher.drain();
    assert_eq!(watcher.typing_names(), newest);
    assert!(watcher.told.len() <= room + 2 + 2, "{} pushes: {:?}", watcher.told.len(), watcher.told);
    let stats = typing.stats();
    assert!(stats.merged >= 100, "{stats:?}");
}

/// Waits until the interface whose lines come on `lines` is pushed `names` typing in `chat`,
/// passing over what comes first, for `TYPED` at most.
async fn pushed_typing(lines: &mut mpsc::UnboundedReceiver<Value>, chat: &str, names: &[&str]) {
    let end = Instant::now() + TYPED;
    let mut came = Vec::new();
    loop {
        let Ok(Some(line)) = tokio::time::timeout_at(end.into(), lines.recv()).await else {
            panic!("the interface was never pushed {names:?} typing in {chat}, only {came:?}");
        };
        let who: Vec<&str> =
            line["params"]["who"].as_array().into_iter().flatten().filter_map(|who| who["name"].as_str()).collect();
        if line["method"] == "typing" && line["params"]["chat"] == chat && who == names {
            return;
        }
        came.push(line);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_writes_nothing_to_jaw_db_and_moves_no_seq_while_interfaces_are_told_it() {
    let (bed, typing, hearing) = typing_bed(ms(400)).await;
    let socket = serve(&bed);
    // Whatever the writer does after the seeding, indexing its words, is done first.
    let writes =
        |bed: &Bed| (bed.store.report().done, bed.store.storage().synced_commits, *bed.store.committed().borrow());
    let mut before = writes(&bed);
    loop {
        tokio::time::sleep(ms(300)).await;
        let now = writes(&bed);
        if now == before {
            break;
        }
        before = now;
    }
    let wal = || std::fs::metadata(bed.home.path().join("jaw.db-wal")).map(|m| m.len()).unwrap_or(0);
    let side = rusqlite::Connection::open(bed.home.path().join("jaw.db")).unwrap();
    let version = |db: &rusqlite::Connection| db.query_row("PRAGMA data_version", [], |r| r.get::<_, i64>(0)).unwrap();
    let (version_before, wal_before) = (version(&side), wal());

    // An interface watches its chat list and the group, then hands on each line jawd sends it.
    let (watched, answered) = tokio::sync::oneshot::channel();
    let (came, mut lines) = mpsc::unbounded_channel();
    let watching = tokio::task::spawn_blocking(move || {
        let (mut line, _) = Line::open(&socket, None);
        line.call("chats.watch", json!({"around": "top", "after": 3}));
        line.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5}));
        let _ = watched.send(line.stream.try_clone().unwrap());
        for push in std::mem::take(&mut line.pushes) {
            let _ = came.send(push);
        }
        line.stream.set_read_timeout(Some(TYPED)).unwrap();
        let mut text = String::new();
        while line.reader.read_line(&mut text).is_ok_and(|n| n > 0) {
            let _ = came.send(serde_json::from_str::<Value>(&text).unwrap());
            text.clear();
        }
    });
    let stream = tokio::time::timeout(TYPED, answered).await.expect("its watches were answered").unwrap();

    // Ana types and stops, Rui records and Zé types in the group, a message of Rui's ends his,
    // Zé's fades, and Ana's ends as you go away, each once the interface has been told the last.
    typing.heard(OPEN, OPEN, "", Composing);
    pushed_typing(&mut lines, OPEN, &["Ana"]).await;
    typing.heard(OPEN, OPEN, "", Paused);
    pushed_typing(&mut lines, OPEN, &[]).await;
    typing.heard(GROUP, &person(9), "", Recording);
    typing.heard(GROUP, &person(10), "", Composing);
    pushed_typing(&mut lines, GROUP, &["Rui", "Zé"]).await;
    typing.message(GROUP, &person(9), "");
    pushed_typing(&mut lines, GROUP, &["Zé"]).await;
    pushed_typing(&mut lines, GROUP, &[]).await;
    typing.heard(OPEN, OPEN, "", Recording);
    pushed_typing(&mut lines, OPEN, &["Ana"]).await;
    hearing.send_replace(false);
    pushed_typing(&mut lines, OPEN, &[]).await;
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    watching.await.unwrap();

    assert_eq!(writes(&bed), before, "the writer's transactions, synced commits and the feed's seq");
    assert_eq!(version(&side), version_before, "nothing committed to jaw.db");
    assert_eq!(wal(), wal_before, "nothing written to its WAL");
    assert_eq!(typing.stats().chats, 0);
}

/// The tree's and the roster's answers on a fresh connection, their watch ids left out.
async fn tree_and_roster_answers(bed: &Bed) -> (Value, Value) {
    let mut watcher = bed.watcher(64);
    let mut tree = watcher.call("folders.watch", json!({})).await.unwrap();
    let roster = json!({"folder": "all", "around": "top", "after": 20});
    let mut roster = watcher.call("roster.watch", roster).await.unwrap();
    for answer in [&mut tree, &mut roster] {
        answer.as_object_mut().unwrap().remove("watch");
    }
    (tree, roster)
}

/// The folder tree shows folders, so typing reaches it nowhere; a roster shows its people's own
/// one-to-one chats, so it's told who's typing in those and in no other chat, by its answer and
/// then by pushes, while its rows and pushes say what they say with nobody typing; a folder's chat
/// list beside them is told who's typing in its chats; and nothing is written (the typing rebase
/// audit's T4, and its O11).
#[tokio::test(flavor = "multi_thread")]
async fn the_tree_is_told_no_typing_and_a_roster_only_its_peoples_own_chats_while_a_folders_chat_list_is_told_its() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    let quiet = tree_and_roster_answers(&bed).await;
    let committed = *bed.store.committed().borrow();
    typing.heard(OPEN, OPEN, "", Composing);
    typing.heard(GROUP, &person(9), "", Recording);
    typed(&typing, OPEN, &["Ana"]).await;
    typed(&typing, GROUP, &["Rui"]).await;

    // Ana types in her chat, whose person the roster shows, and Rui records in the group: the tree
    // answers as it did with nobody typing, and so does the roster but for Ana's typing in her chat.
    let typed_in = tree_and_roster_answers(&bed).await;
    assert_eq!(typed_in.0, quiet.0, "the tree's answer with someone typing");
    let mut rows = typed_in.1.clone();
    rows.as_object_mut().unwrap().remove("typing");
    assert_eq!(rows, quiet.1, "the roster's rows with someone typing");
    assert_eq!(answered_typing(&typed_in.1), want(&[(OPEN, &["Ana"])]), "the roster's typing: {}", typed_in.1);
    let keys = |answer: &Value| {
        let mut keys: Vec<String> = answer.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(&typed_in.0), ["favourites", "rows", "seq"], "{}", typed_in.0);
    assert_eq!(keys(&typed_in.1), ["rows", "seq", "total", "typing"], "{}", typed_in.1);
    let ana = typed_in.1["rows"].as_array().unwrap().iter().find(|row| row["chat"] == OPEN);
    assert!(ana.is_some(), "the roster shows Ana, whose chat she types in: {}", typed_in.1);

    // A connection watching only them is told the typing in Ana's chat as it changes, none of the
    // group's, and their pushes say what they say with nobody typing.
    let mut watcher = bed.watcher(64);
    watcher.watch("folders.watch", json!({})).await;
    watcher.watch("roster.watch", json!({"folder": "all", "around": "top", "after": 20})).await;
    assert_eq!(watcher.typing_names(), want(&[(OPEN, &["Ana"])]));
    typing.heard(GROUP, &person(10), "", Composing);
    typing.heard(OPEN, OPEN, "", Paused);
    typed(&typing, GROUP, &["Rui", "Zé"]).await;
    watcher.until_typing(&[]).await;
    typing.heard(OPEN, OPEN, "", Recording);
    watcher.until_typing(&[(OPEN, &["Ana"])]).await;
    let renamed = Change::Person(named(OPEN, "Ana Ribeiro"));
    let written = Change::Message(Message { live: true, ..text(OPEN, "t-roster", 20_000, Some(OPEN), "olá") });
    let seq = bed.apply(vec![renamed, written]).await;
    watcher.settle(seq).await;
    watcher.run_for(ms(300)).await;
    let told: Vec<&str> = watcher.told.iter().map(|(chat, _)| chat.as_str()).collect();
    assert_eq!(told, [OPEN, OPEN], "the roster is told Ana's pause and recording in her chat alone");
    let methods: Vec<&str> = watcher.pushes.iter().map(|(method, _)| method.as_str()).collect();
    assert!(methods.contains(&"folders") && methods.contains(&"roster"), "{:?}", watcher.pushes);
    for (method, params) in watcher.pushes.iter().filter(|(method, _)| method != "typing") {
        let keys = keys(params);
        let theirs: &[&str] = match method.as_str() {
            "folders" => &["favourites", "remove", "seq", "upsert", "watch"],
            "roster" => &["remove", "seq", "total", "upsert", "watch"],
            other => panic!("a {other} push to the tree and a roster"),
        };
        assert!(keys.iter().all(|key| theirs.contains(&key.as_str())), "a {method} push: {params}");
    }

    // A chat list of the groups folder beside them is told who's typing in its chats: by its
    // answer, then by a push as it changes.
    let groups = json!({"folder": "groups", "around": "top", "after": 3});
    let groups = watcher.call("chats.watch", groups).await.unwrap();
    assert_eq!(answered_typing(&groups), want(&[(GROUP, &["Rui", "Zé"])]), "{groups}");
    typing.heard(GROUP, &person(9), "", Paused);
    watcher.until_typing(&[(OPEN, &["Ana"]), (GROUP, &["Zé"])]).await;
    let told: Vec<&str> = watcher.told.iter().map(|(chat, _)| chat.as_str()).collect();
    assert_eq!(told, [OPEN, OPEN, GROUP], "the group's chat list is told the group's, once");
    assert_eq!(*bed.store.committed().borrow(), seq, "typing wrote nothing past the two changes");
    assert!(committed < seq);
}

/// A photo in the open chat reaches its conversation before a library window of photos across
/// chats that was watched first, as a lone message reaches it before the chat list.
#[tokio::test(flavor = "multi_thread")]
async fn a_photo_in_the_open_chat_reaches_its_conversation_before_the_library() {
    let bed = Bed::new(brisk());
    bed.apply(vec![Change::Message(text(OPEN, "first", 1_000, Some(OPEN), "hi"))]).await;
    let mut watcher = bed.watcher(64);
    // Watched first, so it would be read first if the order of watching decided.
    let library = watcher.watch("library.watch", json!({"kind": "photo", "around": "top", "after": 20})).await;
    let messages = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 50})).await;
    let lone = Message { live: true, text: None, ..photo(OPEN, "lone", 2_000, OPEN) };
    let seq = bed.apply(vec![Change::Message(lone)]).await;
    while watcher.seen < seq || watcher.client.pending() {
        let ready =
            tokio::time::timeout(Duration::from_secs(10), watcher.client.ready()).await.expect("nothing happened");
        if let Ready::Changes(Ok(changes)) = &ready {
            watcher.seen = watcher.seen.max(changes.seq);
        }
        watcher.client.handle(ready).await;
    }
    let mut order = Vec::new();
    while let Ok(line) = watcher.lines.try_recv() {
        let FromJawd::Push(push) = serde_json::from_slice(&line).unwrap() else { panic!("a reply among the pushes") };
        order.push(push.params["watch"].as_str().unwrap().to_string());
    }
    assert_eq!(order, [messages, library]);
}

/// A photo of `sender`'s in `chat`, captioned.
fn photo(chat: &str, id: &str, at: i64, sender: &str) -> Message {
    let media = json!({"media": {"mime": "image/jpeg", "w": 1600, "h": 1200, "bytes": 200_000}});
    Message { kind: "image", detail: Some(media.to_string()), ..text(chat, id, at, Some(sender), "na praia") }
}

/// The library's photos of Ana's chat and across chats.
fn ana_photos() -> Value {
    json!({"kind": "photo", "chat": OPEN, "around": "top", "after": 10})
}

fn every_photo() -> Value {
    json!({"kind": "photo", "around": "top", "after": 10})
}

/// The answers of the library's windows of Ana's chat and across chats on a fresh connection,
/// their watch ids left out.
async fn library_answers(bed: &Bed) -> (Value, Value) {
    let mut watcher = bed.watcher(64);
    let mut ana = watcher.call("library.watch", ana_photos()).await.unwrap();
    let mut every = watcher.call("library.watch", every_photo()).await.unwrap();
    for answer in [&mut ana, &mut every] {
        answer.as_object_mut().unwrap().remove("watch");
    }
    (ana, every)
}

/// A library window shows items, not a chat, so typing reaches none, of one chat or across chats:
/// they answer and push as they do with nobody typing, while a conversation beside them is told
/// who's typing in its chat, a library window of that chat watched meanwhile leaving it owed what
/// changed there, and nothing is written.
#[tokio::test(flavor = "multi_thread")]
async fn library_windows_are_told_no_typing_while_a_conversation_beside_them_is() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    bed.apply(vec![
        Change::Message(photo(OPEN, "p-ana", 11_000, OPEN)),
        Change::Message(photo(GROUP, "p-rui", 12_000, &person(9))),
    ])
    .await;
    // Their captions' words are indexed, which tells library windows, before anything is compared.
    bed.store.indexed().await.unwrap();
    let quiet = library_answers(&bed).await;
    let committed = *bed.store.committed().borrow();
    typing.heard(OPEN, OPEN, "", Composing);
    typing.heard(GROUP, &person(9), "", Recording);
    typed(&typing, OPEN, &["Ana"]).await;
    typed(&typing, GROUP, &["Rui"]).await;

    // Ana types in her chat and Rui records in the group, beside their photos: the library's
    // windows answer as they did with nobody typing, with no typing of their own.
    let typed_in = library_answers(&bed).await;
    assert_eq!(typed_in, quiet, "the library windows' answers with someone typing");
    let keys = |answer: &Value| {
        let mut keys: Vec<String> = answer.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    let rows = |answer: &Value| -> Vec<String> {
        answer["rows"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
    };
    for answer in [&typed_in.0, &typed_in.1] {
        assert_eq!(keys(answer), ["counts", "older", "rows", "seq", "total"], "{answer}");
    }
    assert_eq!(rows(&typed_in.0), ["p-ana"]);
    assert_eq!(rows(&typed_in.1), ["p-rui", "p-ana"]);

    // A connection watching only them is told none of the typing that follows, and their pushes
    // say what they say with nobody typing.
    let mut watcher = bed.watcher(64);
    let ana = watcher.watch("library.watch", ana_photos()).await;
    let every = watcher.watch("library.watch", every_photo()).await;
    typing.heard(GROUP, &person(10), "", Composing);
    typing.heard(OPEN, OPEN, "", Paused);
    typed(&typing, GROUP, &["Rui", "Zé"]).await;
    typed(&typing, OPEN, &[]).await;
    typing.heard(OPEN, OPEN, "", Recording);
    typed(&typing, OPEN, &["Ana"]).await;
    let taken = Change::Message(Message { live: true, ..photo(OPEN, "p-ana-2", 20_000, OPEN) });
    bed.apply(vec![taken]).await;
    bed.store.indexed().await.unwrap();
    let seq = *bed.store.committed().borrow();
    watcher.settle(seq).await;
    watcher.run_for(ms(300)).await;
    assert!(watcher.told.is_empty(), "typing told to library windows: {:?}", watcher.told);
    assert_eq!(watcher.windows[&ana].ids(), ["p-ana-2", "p-ana"]);
    assert_eq!(watcher.windows[&every].ids(), ["p-ana-2", "p-rui", "p-ana"]);
    assert!(!watcher.pushes.is_empty());
    for (method, params) in &watcher.pushes {
        assert_eq!(method, "library", "a {method} push to library windows: {params}");
        let theirs = ["counts", "older", "remove", "seq", "total", "upsert", "watch"];
        assert!(keys(params).iter().all(|key| theirs.contains(&key.as_str())), "a library push: {params}");
    }

    // A conversation of Ana's chat beside them is told who's typing there by its answer, and once
    // she stops, by a push, though a library window of her chat is watched before the connection
    // has heard she stopped.
    let conversation = json!({"chat": OPEN, "around": "newest", "before": 5});
    let conversation = watcher.call("messages.watch", conversation).await.unwrap();
    assert_eq!(answered_typing(&conversation), want(&[(OPEN, &["Ana"])]), "{conversation}");
    typing.heard(OPEN, OPEN, "", Paused);
    typed(&typing, OPEN, &[]).await;
    watcher.watch("library.watch", json!({"kind": "photo", "chat": OPEN, "around": "top", "after": 5})).await;
    watcher.until_typing(&[]).await;
    let told: Vec<&str> = watcher.told.iter().map(|(chat, _)| chat.as_str()).collect();
    assert_eq!(told, [OPEN], "only the conversation is told, once");
    assert_eq!(*bed.store.committed().borrow(), seq, "typing wrote nothing past the photo and its words");
    assert!(committed < seq);
}

/// A chat whose conversation is unwatched leaves every window, so jawd forgets who's typing there,
/// as its interface does, and tells it again once the chat comes into a chat list (round one's T2).
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_whose_conversation_is_unwatched_is_forgotten_and_told_again_once_it_comes_into_a_chat_list() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    let mut watcher = bed.watcher(64);
    watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    let conversation =
        watcher.watch("messages.watch", json!({"chat": person(5), "around": "newest", "before": 5})).await;
    typing.heard(&person(5), &person(5), "", Composing);
    watcher.until_typing(&[(&person(5), &["+351910000005"])]).await;

    // Its conversation is closed, so no window shows it: jawd forgets, as its interface does.
    watcher.call("unwatch", json!({"watch": conversation})).await.unwrap();
    watcher.run_for(ms(100)).await;
    assert_eq!(watcher.typing_names(), want(&[]));
    // Pinned, it comes into the chat list with its typing as it was, and is told who's typing there.
    let seq = bed.apply(vec![Change::Chat(Chat { id: person(5), pinned: Some(1), ..Chat::default() })]).await;
    watcher.settle(seq).await;
    watcher.until_typing(&[(&person(5), &["+351910000005"])]).await;
}

/// A connection told to watch again for falling behind forgets who's typing in the chats its
/// windows showed, and tells a chat again once it's back in a window (round one's T2).
#[tokio::test(flavor = "multi_thread")]
async fn a_connection_told_to_watch_again_forgets_whos_typing_and_tells_a_chat_once_it_comes_back() {
    let (bed, typing, _hearing) = typing_bed(NEVER).await;
    let mut watcher = bed.watcher(2);
    let list = watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    let conversation =
        watcher.watch("messages.watch", json!({"chat": person(5), "around": "newest", "before": 5})).await;
    typing.heard(&person(5), &person(5), "", Composing);
    watcher.until_typing(&[(&person(5), &["+351910000005"])]).await;

    // Its interface stops reading while Ana writes, a message every 60 ms, until its queue has been
    // full for `behind`.
    for n in 0..6i64 {
        bed.apply(vec![Change::Message(text(OPEN, &format!("x{n}"), 20_000 + n, Some(OPEN), "olá"))]).await;
        let until = Instant::now() + ms(60);
        while let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await {
            watcher.client.handle(ready).await;
        }
    }
    let until = Instant::now() + ms(600);
    while let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await {
        watcher.client.handle(ready).await;
    }
    // Reading again, it's told to watch both again, and watches the chat list only.
    watcher.drain();
    watcher.run_for(ms(500)).await;
    assert!(watcher.resynced.contains(&list) && watcher.resynced.contains(&conversation), "{:?}", watcher.resynced);
    assert_eq!(watcher.typing_names(), want(&[]));
    watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    // Pinned, the chat comes in with its typing as it was, and is told who's typing there.
    let seq = bed.apply(vec![Change::Chat(Chat { id: person(5), pinned: Some(1), ..Chat::default() })]).await;
    watcher.settle(seq).await;
    watcher.until_typing(&[(&person(5), &["+351910000005"])]).await;
}

/// A one-to-one chat's person's presence as the protocol says it.
fn presence_of(person: i64, state: &str, at: Option<i64>, known: bool) -> Value {
    let mut presence = json!({"person": format!("person:{person}"), "state": state, "known": known});
    if let Some(at) = at {
        presence["at"] = json!(at);
    }
    presence
}

impl Watcher {
    /// Runs the client, its interface reading, until that holds `presence` for `chat`.
    async fn until_presence(&mut self, chat: &str, presence: &Value) {
        let end = Instant::now() + TYPED;
        loop {
            self.drain();
            if self.presences.get(chat) == Some(presence) {
                return;
            }
            let held = self.presences.get(chat);
            assert!(Instant::now() < end, "its interface was never told {presence} for {chat}, only {held:?}");
            if let Ok(ready) = tokio::time::timeout_at(end.into(), self.client.ready()).await {
                if let Ready::Changes(Ok(changes)) = &ready {
                    self.seen = self.seen.max(changes.seq);
                }
                self.client.handle(ready).await;
            }
        }
    }
}

/// A conversation of a one-to-one chat is told its person's presence: by its answer, then by a
/// `presence` push for each change M6 writes of them (whether jawd follows them, online, when they
/// were last seen), for that chat alone; a group's conversation has none; watching again says how
/// they are now; a closed conversation is told nothing more; and telling it writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_is_told_its_persons_presence_by_its_answer_and_by_a_push_as_it_changes_writing_nothing() {
    use crate::store::folders::{Presence, Step};
    let (bed, _typing, _hearing) = typing_bed(NEVER).await;
    let ana: i64 = bed
        .store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [OPEN], |r| r.get(0))?)
        })
        .await
        .unwrap();
    let mut watcher = bed.watcher(64);
    watcher.watch("chats.watch", json!({"around": "top", "after": 3})).await;
    let conversation = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await;
    let group = watcher.call("messages.watch", json!({"chat": GROUP, "around": "newest", "before": 5})).await.unwrap();
    assert_eq!(group.get("presence"), None, "a group's conversation has no presence: {group}");
    let nothing_said = presence_of(ana, "hidden", None, false);
    assert_eq!(watcher.presences, BTreeMap::from([(OPEN.to_string(), nothing_said)]));
    // What the writer does after a write, indexing words and merging the index, done before her
    // interface is told anything: it runs only as the test drives it.
    let writes =
        |bed: &Bed| (bed.store.report().done, bed.store.storage().synced_commits, *bed.store.committed().borrow());
    let settled = async |bed: &Bed| {
        let mut before = writes(bed);
        loop {
            tokio::time::sleep(ms(300)).await;
            let now = writes(bed);
            if now == before {
                return now;
            }
            before = now;
        }
    };

    // jawd follows her, she comes online, goes, seen a moment before, and jawd stops following her.
    let seen = 1_792_000_000_000;
    let changes = [
        (Step::Following { people: vec![ana] }, presence_of(ana, "hidden", None, true)),
        (Step::Presence { jid: OPEN.into(), state: Presence::Online }, presence_of(ana, "online", None, true)),
        (
            Step::Presence { jid: OPEN.into(), state: Presence::Away(Some(seen)) },
            presence_of(ana, "seen", Some(seen), true),
        ),
        (Step::Following { people: Vec::new() }, presence_of(ana, "seen", Some(seen), false)),
    ];
    let wants: Vec<(String, Value)> = changes.iter().map(|(_, want)| (OPEN.to_string(), want.clone())).collect();
    for (step, want) in changes {
        let seq = bed.apply(vec![Change::Folders(step)]).await;
        let written = settled(&bed).await;
        watcher.settle(seq).await;
        watcher.until_presence(OPEN, &want).await;
        watcher.run_for(ms(150)).await;
        assert_eq!(writes(&bed), written, "telling its interface {want} wrote nothing");
    }
    assert_eq!(watcher.presence_told, wants, "a push for each change, for her chat alone");

    // Watching her chat again says how she is now, as a fresh watch does, and answering it writes
    // nothing.
    let written = settled(&bed).await;
    let mut fresh = bed.watcher(64);
    let again = fresh.call("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await.unwrap();
    assert_eq!(again["presence"], presence_of(ana, "seen", Some(seen), false), "{again}");
    fresh.run_for(ms(300)).await;
    assert_eq!(writes(&bed), written, "answering with her presence wrote nothing");

    // Closed, her conversation is told nothing more of her.
    watcher.call("unwatch", json!({"watch": conversation})).await.unwrap();
    let online = Step::Presence { jid: OPEN.into(), state: Presence::Online };
    let seq = bed.apply(vec![Change::Folders(online)]).await;
    watcher.settle(seq).await;
    watcher.run_for(ms(300)).await;
    assert_eq!(watcher.presence_told, wants, "nothing told once it's closed");
    assert!(watcher.presences.is_empty(), "{:?}", watcher.presences);
}

/// The person jaw.db knows `jid` as, by their n.
async fn person_by_jid(bed: &Bed, jid: &'static str) -> i64 {
    bed.store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [jid], |r| r.get(0))?)
        })
        .await
        .unwrap()
}

/// A conversation watched again from `since`, as after a reconnect, says its person's presence as a
/// fresh watch does, though none of its rows changed meanwhile (the presence audit's T2).
#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_watched_again_from_since_says_its_persons_presence_as_a_fresh_watch_does() {
    use crate::store::folders::{Presence, Step};
    let (bed, _typing, _hearing) = typing_bed(NEVER).await;
    let ana = person_by_jid(&bed, OPEN).await;
    let conversation = json!({"chat": OPEN, "around": "newest", "before": 5});
    let held = bed.watcher(64).call("messages.watch", conversation.clone()).await.unwrap();
    assert_eq!(held["presence"], presence_of(ana, "hidden", None, false), "{held}");
    let have: Vec<Value> = held["rows"].as_array().unwrap().iter().map(|row| row["id"].clone()).collect();

    // While its interface is gone, jawd follows her and she comes online, which moves none of its rows.
    let following = Step::Following { people: vec![ana] };
    let online = Step::Presence { jid: OPEN.into(), state: Presence::Online };
    bed.apply(vec![Change::Folders(following), Change::Folders(online)]).await;

    // Back, it watches again from `since` with the rows it holds; a fresh interface watches the same.
    let mut again = conversation.clone();
    again["since"] = held["seq"].clone();
    again["have"] = json!(have);
    let again = bed.watcher(64).call("messages.watch", again).await.unwrap();
    let fresh = bed.watcher(64).call("messages.watch", conversation).await.unwrap();
    assert_eq!(fresh["presence"], presence_of(ana, "online", None, true), "a fresh watch says how she is: {fresh}");
    assert_eq!(again["presence"], fresh["presence"], "and so does one from since: {again}");
    assert_eq!(again["rows"], json!([]), "none of the rows it holds changed since: {again}");
}

/// A move of a one-to-one chat's conversation answers with its person's presence, as its watch does,
/// faint while jawd isn't following them (the presence audit's T3).
#[tokio::test(flavor = "multi_thread")]
async fn a_conversations_move_answers_with_its_persons_presence_as_its_watch_does() {
    use crate::store::folders::{Presence, Step};
    let (bed, _typing, _hearing) = typing_bed(NEVER).await;
    let ana = person_by_jid(&bed, OPEN).await;
    let seen = 1_792_000_000_000;
    bed.apply(vec![Change::Folders(Step::Presence { jid: OPEN.into(), state: Presence::Away(Some(seen)) })]).await;
    let mut watcher = bed.watcher(64);
    let conversation = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await;
    let faint = presence_of(ana, "seen", Some(seen), false);
    assert_eq!(watcher.presences.get(OPEN), Some(&faint), "the watch's answer");
    let moved = watcher.call("watch.move", json!({"watch": conversation, "around": "newest", "before": 2})).await;
    let moved = moved.unwrap();
    assert_eq!(moved["presence"], faint, "the move's answer: {moved}");
    assert_eq!(watcher.presences.get(OPEN), Some(&faint), "which its interface holds");
}

/// Runs a connection for `long` while its interface reads nothing.
async fn unread(watcher: &mut Watcher, long: Duration) {
    let until = Instant::now() + long;
    while let Ok(ready) = tokio::time::timeout_at(until.into(), watcher.client.ready()).await {
        if let Ready::Changes(Ok(changes)) = &ready {
            watcher.seen = watcher.seen.max(changes.seq);
        }
        watcher.client.handle(ready).await;
    }
}

/// A presence push that finds its connection's queue full waits for room, as window updates do,
/// and goes then as the person is: one read of her conversation finds her message, whose push takes
/// the queue's only room, and her coming online, followed, which waits (the presence audit's T4).
#[tokio::test(flavor = "multi_thread")]
async fn a_presence_push_that_finds_no_room_waits_for_it_and_goes_as_they_are_then() {
    use crate::store::folders::{Presence, Step};
    // Its window isn't dropped for falling behind while it reads nothing.
    let (bed, _typing, _hearing) = typing_bed_paced(NEVER, Timing { behind: Duration::from_secs(60), ..brisk() }).await;
    let ana = person_by_jid(&bed, OPEN).await;
    let mut watcher = bed.watcher(1);
    watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await;
    assert_eq!(watcher.presences.get(OPEN), Some(&presence_of(ana, "hidden", None, false)));
    // Its connection takes all that was written before, up to a message in another chat, so the
    // next read of her conversation is the one that finds what follows.
    let elsewhere = bed.apply(vec![Change::Message(text(&person(5), "other", 30_000, Some(&person(5)), "olá"))]).await;
    watcher.settle(elsewhere).await;
    let came = text(OPEN, "now", 20_000, Some(OPEN), "cheguei");
    let following = Step::Following { people: vec![ana] };
    let online = Step::Presence { jid: OPEN.into(), state: Presence::Online };
    let seq = bed.apply(vec![Change::Message(came), Change::Folders(following), Change::Folders(online)]).await;
    // Its connection takes the change while its interface reads nothing.
    let end = Instant::now() + TYPED;
    while watcher.seen < seq || watcher.lines.is_empty() {
        assert!(Instant::now() < end, "the change was never taken");
        unread(&mut watcher, ms(20)).await;
    }
    // Long enough for the presence it owes to find no room.
    unread(&mut watcher, ms(300)).await;
    assert_eq!(watcher.lines.len(), 1, "its queue is full");
    // It reads the message, and is told she's online.
    let online = presence_of(ana, "online", None, true);
    let end = Instant::now() + TYPED;
    while watcher.presences.get(OPEN) != Some(&online) {
        assert!(Instant::now() < end, "its interface was left with {:?}: {:?}", watcher.presences, watcher.pushes);
        if let Ok(line) = watcher.lines.try_recv() {
            watcher.take(&line);
        }
        if let Ok(ready) = tokio::time::timeout(ms(10), watcher.client.ready()).await {
            watcher.client.handle(ready).await;
        }
    }
    assert_eq!(watcher.pushes.first().map(|(method, _)| method.as_str()), Some("messages"), "{:?}", watcher.pushes);
    assert_eq!(watcher.presence_told, [(OPEN.to_string(), online)], "her presence once there was room");
}

/// A library window shows nobody's presence, of a chat, of a person or across chats, so none has
/// jawd follow anyone, while a conversation beside them shows its person's (the presence landing
/// audit's T1).
#[tokio::test(flavor = "multi_thread")]
async fn library_windows_show_nobodys_presence_while_a_conversation_beside_them_shows_its_persons() {
    let (bed, _typing, _hearing) = typing_bed(NEVER).await;
    bed.apply(vec![
        Change::Message(photo(OPEN, "p-ana", 11_000, OPEN)),
        Change::Message(photo(GROUP, "p-rui", 12_000, &person(9))),
    ])
    .await;
    bed.store.indexed().await.unwrap();
    let mut watcher = bed.watcher(64);
    watcher.watch("library.watch", ana_photos()).await;
    watcher.watch("library.watch", every_photo()).await;
    watcher.watch("library.watch", json!({"kind": "photo", "person": "person:1", "around": "top", "after": 10})).await;
    assert_eq!(bed.feed.showing(), (Vec::new(), Vec::new()), "library windows show nobody's presence");
    let conversation = watcher.watch("messages.watch", json!({"chat": OPEN, "around": "newest", "before": 5})).await;
    let (open, listed) = bed.feed.showing();
    assert_eq!((open.len(), listed), (1, Vec::new()), "a conversation beside them shows its person's alone");
    watcher.call("unwatch", json!({"watch": conversation})).await.unwrap();
    assert_eq!(bed.feed.showing(), (Vec::new(), Vec::new()), "and once it's closed, nobody's again");
}
