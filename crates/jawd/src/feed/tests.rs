use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jaw_proto::{FromJawd, code};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::client::MOST_WINDOWS;
use super::window::{chat_row, message_row};
use super::*;
use crate::state::State;
use crate::store::read::{self, Around};
use crate::store::{Change, Chat, Lane, Message, Person, Read, Store};
use crate::work::Report;

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
                let model = self.windows.get_mut(&watch).expect("a push for a window it watches");
                // A window never goes back to an older read than its interface holds.
                let seq = push.params["seq"].as_u64().unwrap();
                assert!(seq >= model.seq, "{watch} went back from seq {} to {seq}: {}", model.seq, push.params);
                model.take(&push.params, "upsert");
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
            // A new name changes rows whose own versions don't move, so it's checked on its own.
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
    let sending =
        crate::send::Sending::start(bed.store.clone(), None, live.clone(), &pushes, attention.focus(), timing).await;
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
