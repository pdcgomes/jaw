use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jaw_proto::{FromJawd, code};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::window::{chat_row, message_row};
use super::*;
use crate::state::State;
use crate::store::read::{self, Around};
use crate::store::{Change, Chat, Lane, Message, Person, Read, Store};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn brisk() -> Timing {
    Timing { quiet: ms(1), burst: ms(10), hold: ms(300), behind: ms(300) }
}

struct Bed {
    home: tempfile::TempDir,
    store: Store,
    feed: Feed,
    catching_up: Arc<AtomicBool>,
}

impl Bed {
    fn new(timing: Timing) -> Bed {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let catching_up = Arc::new(AtomicBool::new(false));
        let holding = catching_up.clone();
        let feed = Feed::start(store.clone(), Arc::new(move || holding.load(Ordering::Relaxed)), timing);
        Bed { home, store, feed, catching_up }
    }

    async fn apply(&self, changes: Vec<Change>) -> i64 {
        self.store.apply(Lane::Live, changes).await.unwrap().seq
    }

    fn watcher(&self, room: usize) -> Watcher {
        let (out, lines) = mpsc::channel(room);
        Watcher {
            client: self.feed.client(out),
            lines,
            windows: HashMap::new(),
            seen: self.feed.seq() as i64,
            resynced: Vec::new(),
            batches: HashMap::new(),
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

/// A window as an interface holds it, built only from results and pushes.
#[derive(Clone, Debug, Default, PartialEq)]
struct Model {
    chats: bool,
    rows: HashMap<String, Value>,
    total: u64,
    older: bool,
    newer: u64,
    seq: u64,
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
    }

    /// Its rows in the list's order: chats from the largest sort down, messages from the smallest up.
    fn ordered(&self) -> Vec<Value> {
        let mut rows: Vec<Value> = self.rows.values().cloned().collect();
        rows.sort_by(|a, b| a["sort"].as_str().cmp(&b["sort"].as_str()));
        if self.chats {
            rows.reverse();
        }
        rows
    }

    fn ids(&self) -> Vec<String> {
        self.ordered().iter().map(|row| row["id"].as_str().unwrap().to_string()).collect()
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
struct Watcher {
    client: Client,
    lines: mpsc::Receiver<Vec<u8>>,
    windows: HashMap<String, Model>,
    seen: i64,
    resynced: Vec<String>,
    batches: HashMap<String, usize>,
}

impl Watcher {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, jaw_proto::Error> {
        let result = self.client.call(method, params).await?;
        if let Some(watch) = result["watch"].as_str() {
            let model = self.windows.entry(watch.to_string()).or_default();
            model.chats = result.get("older").is_none();
            model.take(&result, "rows");
        }
        Ok(result)
    }

    async fn watch(&mut self, method: &str, params: Value) -> String {
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
        let watch = push.params["watch"].as_str().unwrap().to_string();
        match push.method.as_str() {
            "resync" => {
                self.windows.remove(&watch);
                self.resynced.push(watch);
            }
            "chats" | "messages" => {
                *self.batches.entry(watch.clone()).or_default() += 1;
                self.windows.get_mut(&watch).expect("a push for a window it watches").take(&push.params, "upsert");
            }
            method => panic!("an unexpected {method} push"),
        }
    }

    /// Runs the client for `long`, taking what it sends.
    async fn run_for(&mut self, long: Duration) {
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
    async fn settle(&mut self, seq: i64) {
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
                Model { chats: false, rows: rows.collect(), total, older, newer, seq: 0 }
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
        let message = text(&chat, &id, at, sender.as_deref(), &format!("words {}", self.rng.next() % 1000));
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

async fn anchor_on(bed: &Bed, chat: &str, back: usize) -> Option<(String, i64, i64)> {
    let chat = chat.to_string();
    let rows = bed
        .store
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
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed, OPEN, 8).await, before: 3, after: 3 },
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
            // A new name changes rows whose own versions don't move, so it's checked on its own.
            let renamed = matches!(changes[..], [Change::Person(_)]);
            let seq = bed.apply(changes).await;
            if world.rng.below(20) == 0 {
                // Scrolling: the anchored windows move to another message or chat.
                let (watch, want) = watched[4].clone();
                let moved = match want {
                    Want::Messages { chat, before, after, .. } => {
                        let anchor = anchor_on(&bed, &chat, world.rng.below(12)).await;
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
    let socket = jaw_proto::local::socket(bed.home.path());
    let listener = crate::sock::listen(&socket).unwrap();
    let state = State::new(
        bed.home.path(),
        Err(("demo", "no engine in this test".into())),
        Vec::new(),
        Arc::default(),
        Some(bed.store.clone()),
        broadcast::channel(16).0,
    )
    .unwrap()
    .with_feed(bed.feed.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    socket
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
            Want::Messages { chat: OPEN.into(), anchor: anchor_on(&bed, OPEN, 8).await, before: 3, after: 3 },
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
