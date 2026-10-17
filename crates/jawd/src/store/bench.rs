//! `jawd bench store`: a made-up account's jaw.db built through the writer (a few chats with
//! 100,000 messages and a long tail of small ones), then its pages, chat list and search timed
//! through the read pool, as an interface would ask for them. `jawd bench inbox`: live writes timed
//! while the inbox takes a steady stream of rows.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use prost::Message as _;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::read::{self, Around, MessageCursor};
use super::{Change, Chat, Lane, Message, Packed, Person, Store};
use crate::work::Priority;

/// What to build and how often to time each query.
pub struct Plan {
    pub messages: u32,
    pub chats: u32,
    pub runs: u32,
}

/// The chats with the most messages, each a tenth of them.
pub(crate) const BIG_CHATS: usize = 3;
const PAGE: usize = 100;
pub(crate) const DAY_MS: i64 = 24 * 3600 * 1000;
pub(crate) const SPAN_MS: i64 = 3 * 365 * DAY_MS;
/// Messages in each batch handed to the writer.
pub(crate) const BATCH: usize = 2_000;

const BUILDING: u8 = 0;
const INDEXING: u8 = 1;
const DONE: u8 = 2;

const WORDS: &[&str] = &[
    "não",
    "sim",
    "obrigado",
    "amanhã",
    "então",
    "você",
    "está",
    "café",
    "jantar",
    "já",
    "também",
    "até",
    "logo",
    "olá",
    "bom",
    "dia",
    "noite",
    "casa",
    "praia",
    "fim",
    "de",
    "semana",
    "e",
    "o",
    "a",
    "que",
    "para",
    "com",
    "um",
    "uma",
    "the",
    "and",
    "see",
    "you",
    "tomorrow",
    "thanks",
    "photos",
    "Sunday",
    "lake",
    "meeting",
    "call",
    "later",
    "dinner",
    "tickets",
    "train",
    "Lisbon",
    "Porto",
    "climbing",
    "Saturday",
    "at",
    "9",
    "ok",
    "haha",
    "yes",
    "no",
    "maybe",
    "coffee",
    "work",
    "late",
    "home",
    "soon",
    "love",
    "happy",
    "birthday",
    "parabéns",
    "beijinhos",
    "saudades",
    "coração",
    "pão",
    "irmã",
    "avó",
    "mãe",
    "pai",
    "férias",
    "música",
    "concerto",
    "filme",
    "série",
];

pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// A message's words: mostly a few, sometimes a paragraph, as people write.
fn text(rng: &mut Rng) -> String {
    let words = match rng.below(100) {
        0..60 => 1 + rng.below(6),
        60..90 => 7 + rng.below(14),
        90..99 => 21 + rng.below(40),
        _ => 61 + rng.below(140),
    };
    let mut text = String::new();
    for n in 0..words {
        if n > 0 {
            text.push(' ');
        }
        // One word in a million is one nothing else has, for a search that finds few.
        if rng.below(1_000_000) == 0 {
            text.push_str("zéfiro");
        } else {
            text.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
        }
    }
    text
}

/// How many messages each chat has: a tenth of them in each big chat, the rest in a long tail.
pub(crate) fn sizes(messages: usize, chats: usize) -> Vec<usize> {
    let big = (messages / 10).max(1);
    let mut sizes = vec![big; BIG_CHATS.min(chats)];
    let rest = messages.saturating_sub(big * sizes.len());
    let tail = chats - sizes.len();
    let weights: Vec<f64> = (0..tail).map(|n| 1.0 / ((n + 2) as f64).powf(1.1)).collect();
    let total: f64 = weights.iter().sum();
    let mut given = 0;
    for weight in &weights {
        let size = ((weight / total * rest as f64) as usize).max(1);
        sizes.push(size);
        given += size;
    }
    if let Some(last) = sizes.last_mut() {
        *last = (*last + rest).saturating_sub(given).max(1);
    }
    sizes
}

pub(crate) fn phone(n: usize) -> String {
    format!("3519{n:08}@s.whatsapp.net")
}

pub(crate) fn lid(n: usize) -> String {
    format!("{}@lid", 100_000_000_000u64 + n as u64)
}

pub(crate) fn chat_id(n: usize) -> String {
    if n % 5 == 2 { format!("120363{n:012}@g.us") } else { phone(n) }
}

/// One made-up message, with a raw protobuf shaped like WhatsApp's.
pub(crate) fn message(rng: &mut Rng, chat: &str, group: bool, at: i64, count: u64) -> Message {
    let text = text(rng);
    let mine = rng.below(if group { 10 } else { 2 }) == 0;
    let sender = (!mine).then(|| if group { lid(rng.below(1_000) as usize) } else { chat.to_string() });
    let id = format!("3EB0{count:016X}");
    let secret: Vec<u8> = (0..4).flat_map(|_| rng.next().to_le_bytes()).collect();
    let info = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(chat.to_string()),
            from_me: Some(mine),
            id: Some(id.clone()),
            participant: sender.clone().filter(|_| group),
        },
        message: Some(e2e::Message {
            conversation: Some(text.clone()),
            message_context_info: Some(e2e::MessageContextInfo { message_secret: Some(secret), ..Default::default() }),
            ..Default::default()
        }),
        message_timestamp: Some((at / 1000) as u64),
        push_name: sender.as_ref().map(|_| "Ana Ribeiro".to_string()),
        ..Default::default()
    };
    let spans = (rng.below(20) == 0).then(|| serde_json::json!([{"t": "bold", "s": text}]).to_string());
    Message {
        chat: chat.to_string(),
        id,
        at,
        sender,
        mine,
        kind: "text",
        status: mine.then_some("read"),
        summary: text.chars().take(80).collect(),
        text: Some(text),
        spans,
        raw: Some(Packed::new(&info.encode_to_vec())),
        ..Message::default()
    }
}

pub(crate) struct Timings(pub(crate) Vec<Duration>);

impl Timings {
    pub(crate) fn line(&mut self, what: &str) -> String {
        if self.0.is_empty() {
            return format!("  {what:<44} none timed");
        }
        self.0.sort_unstable();
        let at = |q: usize| self.0[(self.0.len() * q / 100).min(self.0.len() - 1)];
        format!(
            "  {what:<44} p50 {:>6.2} ms  p99 {:>6.2} ms  max {:>6.2} ms",
            ms(at(50)),
            ms(at(99)),
            ms(*self.0.last().unwrap_or(&Duration::ZERO))
        )
    }
}

pub(crate) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Runs `query` on the read pool `runs` times, one after another, timing each round trip.
async fn time<T, F>(store: &Store, runs: u32, query: F) -> Result<Timings>
where
    T: Send + 'static,
    F: Fn(&rusqlite::Connection) -> Result<T> + Send + Sync + Clone + 'static,
{
    let mut timings = Vec::new();
    for _ in 0..runs {
        let query = query.clone();
        let started = Instant::now();
        store.reads().query(Priority::Interactive, query).await?;
        timings.push(started.elapsed());
    }
    Ok(Timings(timings))
}

pub async fn run(home: &Path, plan: Plan) -> Result<()> {
    let store = Store::open(home)?;
    let chats = plan.chats.max(BIG_CHATS as u32 + 1) as usize;
    let sizes = sizes(plan.messages as usize, chats);
    let total: usize = sizes.iter().sum();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let now = 1_792_000_000_000;

    // Contacts arrive by phone number; group members write from their LIDs, joined to them later.
    let people: Vec<Change> = (0..chats.max(1_000))
        .map(|n| {
            Change::Person(Person { jids: vec![phone(n)], name: Some(format!("Person {n}")), ..Person::default() })
        })
        .collect();
    store.apply(Lane::Background, people).await?;

    // A live message every 5 ms while history goes in and then while it's indexed for search,
    // timed from hand-over to commit.
    let phase = Arc::new(AtomicU8::new(BUILDING));
    let probe = {
        let (store, phase) = (store.clone(), phase.clone());
        tokio::spawn(async move {
            let mut rng = Rng(7);
            let mut timings = [Vec::new(), Vec::new()];
            let mut count = 1u64 << 40;
            loop {
                let now_in = phase.load(Ordering::Relaxed);
                if now_in == DONE {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
                count += 1;
                let live = message(&mut rng, &phone(999_999), false, now + count as i64, count);
                let started = Instant::now();
                store.apply(Lane::Live, vec![Change::Message(live)]).await?;
                timings[usize::from(now_in)].push(started.elapsed());
            }
            let [building, indexing] = timings;
            anyhow::Ok((Timings(building), Timings(indexing)))
        })
    };

    let started = Instant::now();
    let (mut count, mut transactions) = (0u64, 0);
    for (n, &size) in sizes.iter().enumerate() {
        let id = chat_id(n);
        let group = id.ends_with("@g.us");
        let name = group.then(|| format!("Group {n}"));
        let pinned = (n == 1).then_some(now);
        let mut batch = vec![Change::Chat(Chat { id: id.clone(), name, pinned, ..Chat::default() })];
        let begins = now - if n < BIG_CHATS { SPAN_MS } else { rng.below(SPAN_MS as u64) as i64 + DAY_MS };
        let mut times: Vec<i64> = (0..size).map(|_| begins + rng.below((now - begins) as u64) as i64).collect();
        times.sort_unstable();
        for at in times {
            count += 1;
            batch.push(Change::Message(message(&mut rng, &id, group, at, count)));
            if batch.len() == BATCH {
                transactions += store.apply(Lane::Background, std::mem::take(&mut batch)).await?.transactions;
            }
        }
        if !batch.is_empty() {
            transactions += store.apply(Lane::Background, batch).await?.transactions;
        }
    }
    let built = started.elapsed();
    phase.store(INDEXING, Ordering::Relaxed);
    let indexing = Instant::now();
    store.indexed().await?;
    let indexed = indexing.elapsed();
    phase.store(DONE, Ordering::Relaxed);
    let (mut live, mut live_indexing) = probe.await??;
    let writer = crate::work::Report::report(&store);

    // The LID map arrives: each member's LID joins the contact with their number.
    let joins: Vec<Change> =
        (0..1_000).map(|n| Change::Person(Person { jids: vec![lid(n), phone(n)], ..Person::default() })).collect();
    let joined = store.apply(Lane::Background, joins).await?;

    let storage = store.storage();
    store.truncate().await?;
    let bytes = std::fs::metadata(home.join("jaw.db"))?.len();
    println!(
        "Built {total} messages in {chats} chats ({} in each of the biggest {BIG_CHATS}, {} in the next, {} in the median one) through the writer's background lane in {:.1} s: {:.0} messages a second in {transactions} transactions. jaw.db is {:.0} MB, {:.0} bytes a message.",
        sizes[0],
        sizes[BIG_CHATS],
        sizes[chats / 2],
        built.as_secs_f64(),
        total as f64 / built.as_secs_f64(),
        bytes as f64 / 1e6,
        bytes as f64 / total as f64,
    );
    println!("{}, while history went in", live.line(&format!("a live message, {} of them", live.0.len())));
    println!(
        "{}, while it was indexed for search in {:.1} s",
        live_indexing.line(&format!("a live message, {} of them", live_indexing.0.len())),
        indexed.as_secs_f64()
    );
    println!(
        "  The WAL held {} frames at most, and its file was {:.1} MB before the last checkpoint; background work waited for the checkpointer {} times, {:.1} s in all.\n",
        storage.wal_peak_frames,
        storage.wal_bytes as f64 / 1e6,
        storage.background_waits,
        storage.background_waited_ms as f64 / 1e3,
    );
    println!("{}\n", jaw_client::pools(&[writer]));
    println!(
        "  1,000 LIDs joined to the contacts with their numbers in {:.1} ms, {} transactions\n",
        ms(joined.took),
        joined.transactions
    );

    // Walk the biggest chat from its newest message to its oldest, a page at a time.
    let biggest = chat_id(0);
    let mut walk = Vec::new();
    let mut around = Around::Newest;
    let (mut seen, mut middle, mut oldest, mut last): (
        usize,
        Option<MessageCursor>,
        Option<MessageCursor>,
        Option<i64>,
    ) = (0, None, None, None);
    let mut sample = None;
    loop {
        let chat = biggest.clone();
        let began = Instant::now();
        let page =
            store.reads().query(Priority::Interactive, move |db| read::messages(db, &chat, around, PAGE)).await?;
        walk.push(began.elapsed());
        let Some(first) = page.first() else { break };
        for row in page.iter().rev() {
            ensure!(last.is_none_or(|last| row.n < last), "the walk went back on itself at message {}", row.n);
            last = Some(row.n);
        }
        seen += page.len();
        if seen >= sizes[0] / 2 && middle.is_none() {
            middle = Some(first.cursor());
        }
        if oldest.is_none() && seen < sizes[0] && sizes[0] - seen <= PAGE {
            oldest = Some(first.cursor());
        }
        around = Around::Before(first.cursor());
        sample.get_or_insert_with(|| serde_json::to_string(page.last().expect("a page")).unwrap_or_default());
    }
    ensure!(seen == sizes[0], "the walk saw {seen} of the biggest chat's {} messages", sizes[0]);
    println!("Timings through the read pool, each a round trip from a tokio task, {} runs:", plan.runs);
    println!("{}", Timings(walk).line(&format!("every page of the biggest chat, {} pages", sizes[0].div_ceil(PAGE))));
    for (what, around) in [
        ("its newest 100 messages", Around::Newest),
        ("100 messages in its middle", Around::Before(middle.expect("a middle"))),
        ("the 100 after those", Around::After(middle.expect("a middle"))),
        ("its oldest 100 messages", Around::Before(oldest.expect("an oldest page"))),
    ] {
        let chat = biggest.clone();
        let mut timings = time(&store, plan.runs, move |db| read::messages(db, &chat, around, PAGE)).await?;
        println!("{}", timings.line(what));
    }
    let first_chats = store.reads().query(Priority::Interactive, |db| read::chats(db, false, None, PAGE)).await?;
    let mut timings = time(&store, plan.runs, |db| read::chats(db, false, None, PAGE)).await?;
    println!("{}", timings.line("the chat list's first 100 chats"));
    let next = first_chats.last().map(|row| row.cursor());
    let mut timings = time(&store, plan.runs, move |db| read::chats(db, false, next.as_ref(), PAGE)).await?;
    println!("{}", timings.line("the chat list's next 100"));
    let ids: Vec<String> =
        (0..plan.runs).map(|n| format!("3EB0{:016X}", 1 + (n as u64 * 7919) % sizes[0] as u64)).collect();
    let mut timings = Vec::new();
    for id in ids {
        let chat = biggest.clone();
        let began = Instant::now();
        let found = store.reads().query(Priority::Interactive, move |db| read::message(db, &chat, &id)).await?;
        timings.push(began.elapsed());
        ensure!(found.is_some_and(|(_, raw)| raw.is_some()), "a message the bench wrote wasn't found");
    }
    println!("{}", Timings(timings).line("a message by its ID, raw protobuf unpacked"));
    for (what, typed) in [
        ("a search for \"nao\", 50 hits", "nao"),
        ("a search for \"zefiro\"", "zefiro"),
        ("a search for \"cafe amanha\"", "cafe amanha"),
    ] {
        let found =
            store.reads().query(Priority::Interactive, move |db| read::search(db, typed, None, None, 50)).await?;
        let mut timings = time(&store, plan.runs, move |db| read::search(db, typed, None, None, 50)).await?;
        println!("{} ({} on the first page)", timings.line(what), found.hits.len());
        if let Some(cursor) = found.cursor {
            let mut timings =
                time(&store, plan.runs, move |db| read::search(db, typed, None, Some(cursor), 50)).await?;
            println!("{}", timings.line("  and its next page"));
        }
    }
    let since = joined.seq - 1_000;
    let mut timings = time(&store, plan.runs, move |db| read::events(db, since, 1_000)).await?;
    println!("{}", timings.line("the feed's last 1,000 events"));
    println!("\nThe biggest chat's newest message, as the read pool returns it:\n{}", sample.unwrap_or_default());
    Ok(())
}

/// What `jawd bench inbox` streams: each rate in turn, for `seconds`, with a live write every
/// `every_ms` meanwhile.
pub struct Stream {
    pub rates: Vec<u32>,
    pub seconds: u32,
    pub every_ms: u32,
}

/// Inbox rows handed over at each rate in turn, on a jaw.db of its own each time, as a catch-up
/// after reconnecting delivers them, while live writes are timed from hand-over to answer: every
/// `every_ms` a durable one (a send, a read, a draft) and, halfway between, a conversion. Each row
/// is timed from hand-over to its ack.
pub async fn stream(home: &Path, plan: Stream) -> Result<()> {
    for &rate in &plan.rates {
        let folder = home.join(format!("inbox-{rate}"));
        std::fs::create_dir_all(&folder)?;
        let store = Store::open(&folder)?;
        let rows = rate as usize * plan.seconds as usize;
        let (acks, mut acked) = tokio::sync::mpsc::unbounded_channel();
        let handed = Arc::new(std::sync::Mutex::new(Vec::with_capacity(rows)));
        let started = Instant::now();
        let feeder = {
            let (store, handed) = (store.clone(), handed.clone());
            std::thread::spawn(move || {
                for n in 0..rows {
                    let due = started + Duration::from_secs_f64(n as f64 / f64::from(rate));
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    handed.lock().expect("the hand-over times").push(Instant::now());
                    store.inbox("bench", 0, vec![0; 600], Some(crate::engine::Ack::to(n as u64, acks.clone())));
                }
            })
        };
        let collector = {
            let handed = handed.clone();
            tokio::spawn(async move {
                let mut to_ack = vec![Duration::ZERO; rows];
                for _ in 0..rows {
                    let Some(crate::engine::Outgoing::Ack(seq)) = acked.recv().await else {
                        anyhow::bail!("an inbox row was never acked");
                    };
                    let at = Instant::now();
                    to_ack[seq as usize] = at - handed.lock().expect("the hand-over times")[seq as usize];
                }
                Ok(to_ack)
            })
        };
        let length = Duration::from_secs(plan.seconds.into());
        let every = Duration::from_millis(plan.every_ms.max(1).into());
        let mut writes = Vec::new();
        let mut rng = Rng(11);
        let mut n = 0u32;
        while every * n / 2 < length {
            let due = started + every * n / 2;
            tokio::time::sleep_until(due.into()).await;
            let durable = n.is_multiple_of(2);
            let count = (1u64 << 40) + u64::from(n);
            let live =
                Change::Message(message(&mut rng, &phone(900_000 + n as usize % 50), false, count as i64, count));
            let store = store.clone();
            writes.push((
                durable,
                tokio::spawn(async move {
                    let began = Instant::now();
                    if durable {
                        store.apply_durable(vec![live]).await?;
                    } else {
                        store.apply(Lane::Live, vec![live]).await?;
                    }
                    anyhow::Ok(began.elapsed())
                }),
            ));
            n += 1;
        }
        let (mut durable, mut converted) = (Vec::new(), Vec::new());
        for (is_durable, write) in writes {
            let took = write.await??;
            if is_durable { durable.push(took) } else { converted.push(took) }
        }
        tokio::task::spawn_blocking(move || feeder.join()).await?.map_err(|_| anyhow::anyhow!("the feeder failed"))?;
        let to_ack = collector.await??;
        let storage = store.storage();
        println!(
            "{rate} inbox rows a second for {} s, {rows} rows in {} synced commits:",
            plan.seconds, storage.synced_commits
        );
        println!("{}", Timings(durable).line("a durable live write, hand-over to answer"));
        println!("{}", Timings(converted).line("a live conversion, hand-over to answer"));
        println!("{}", Timings(to_ack).line("an inbox row, hand-over to its ack"));
        store.truncate().await?;
        drop(store);
        std::fs::remove_dir_all(&folder)?;
    }
    Ok(())
}
