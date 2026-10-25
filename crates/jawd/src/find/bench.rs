//! `jawd bench find`: a made-up account in jaw.db, built as `jawd bench store` builds one but with
//! people's names, group names and chats opened over the last month; then a keystroke's `find`
//! timed for one to six letters typed, as the palette would ask, and the index's memory reported.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use jaw_proto::find::{Find, Kind};

use super::{Cancel, Finder};
use crate::store::bench::{BATCH, BIG_CHATS, DAY_MS, Rng, SPAN_MS, Timings, chat_id, lid, message, ms, phone, sizes};
use crate::store::{Change, Chat, Lane, Opened, Person, Store};

/// What to build and how often to time each keystroke.
pub struct Plan {
    pub messages: u32,
    pub chats: u32,
    pub people: u32,
    pub runs: u32,
}

pub(crate) const FIRST: &[&str] = &[
    "Ana",
    "Rui",
    "Sofia",
    "Inês",
    "João",
    "Pedro",
    "Marta",
    "Tiago",
    "Beatriz",
    "Gonçalo",
    "Mariana",
    "Duarte",
    "Leonor",
    "Tomás",
    "Catarina",
    "Francisco",
    "Rita",
    "Miguel",
    "Joana",
    "André",
    "Lúcia",
    "Nuno",
    "Helena",
    "Vítor",
    "Alice",
    "Artur",
    "Tom",
    "Emma",
    "Oliver",
    "Grace",
    "Harry",
    "Chloe",
    "James",
    "Lily",
    "Jack",
    "Amelia",
    "Lurdes",
    "Fátima",
    "Conceição",
    "Sebastião",
];

pub(crate) const LAST: &[&str] = &[
    "Ribeiro",
    "Costa",
    "Gomes",
    "Silva",
    "Santos",
    "Ferreira",
    "Pereira",
    "Oliveira",
    "Rodrigues",
    "Martins",
    "Sousa",
    "Fernandes",
    "Gonçalves",
    "Lopes",
    "Marques",
    "Almeida",
    "Carvalho",
    "Teixeira",
    "Moreira",
    "Correia",
    "Mendes",
    "Nunes",
    "Soares",
    "Vieira",
    "Monteiro",
    "Cardoso",
    "Rocha",
    "Reis",
    "Baker",
    "Smith",
    "Jones",
    "Taylor",
    "Brown",
    "Wilson",
    "Evans",
    "Thomas",
    "Roberts",
    "Walker",
    "Wright",
    "Hughes",
];

pub(crate) const GROUPS: &[&str] = &[
    "Lisbon Climbing",
    "Família Gomes",
    "Jantar de sexta",
    "Book club",
    "Work: platform",
    "Escola da Rita",
    "Porto trip",
    "Futebol de quinta",
    "Condomínio",
    "Board games",
    "Viagem aos Açores",
    "Running Monsanto",
];

/// What's typed, a letter at a time: names, a group, words in messages, and a word nothing has.
const TYPED: &[&str] = &["ribeir", "marta s", "lisbon", "jantar", "obriga", "zefiro", "ines g"];

fn name(rng: &mut Rng) -> String {
    let first = FIRST[rng.below(FIRST.len() as u64) as usize];
    let last = LAST[rng.below(LAST.len() as u64) as usize];
    format!("{first} {last}")
}

pub async fn run(home: &Path, plan: Plan) -> Result<()> {
    let store = Store::open(home)?;
    let chats = plan.chats.max(BIG_CHATS as u32 + 1) as usize;
    let people = (plan.people as usize).max(chats);
    let sizes = sizes(plan.messages as usize, chats);
    let total: usize = sizes.iter().sum();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let now = super::now_ms();

    // Contacts by phone number; one in seven is known only by what they call themselves.
    let contacts: Vec<Change> = (0..people)
        .map(|n| {
            let shown = name(&mut rng);
            let (name, push_name) = if n % 7 == 3 { (None, Some(shown)) } else { (Some(shown), None) };
            Change::Person(Person { jids: vec![phone(n)], name, push_name, ..Person::default() })
        })
        .collect();
    store.apply(Lane::Background, contacts).await?;

    let started = Instant::now();
    let mut count = 0u64;
    for (n, &size) in sizes.iter().enumerate() {
        let id = chat_id(n);
        let group = id.ends_with("@g.us");
        let name = group.then(|| format!("{} {}", GROUPS[n % GROUPS.len()], n / GROUPS.len()));
        let mut batch = vec![Change::Chat(Chat { id: id.clone(), name, ..Chat::default() })];
        let begins = now - if n < BIG_CHATS { SPAN_MS } else { rng.below(SPAN_MS as u64) as i64 + DAY_MS };
        let mut times: Vec<i64> = (0..size).map(|_| begins + rng.below((now - begins) as u64) as i64).collect();
        times.sort_unstable();
        for at in times {
            count += 1;
            batch.push(Change::Message(message(&mut rng, &id, group, at, count)));
            if batch.len() == BATCH {
                store.apply(Lane::Background, std::mem::take(&mut batch)).await?;
            }
        }
        if !batch.is_empty() {
            store.apply(Lane::Background, batch).await?;
        }
    }
    let joins: Vec<Change> =
        (0..1_000).map(|n| Change::Person(Person { jids: vec![lid(n), phone(n)], ..Person::default() })).collect();
    store.apply(Lane::Background, joins).await?;
    // A month of opens, most of them of a few chats.
    let opens: Vec<Change> = (0..600)
        .map(|_| {
            let chat = (rng.below(40) * rng.below(50) / 49) as usize;
            Change::Opened(Opened { chat: chat_id(chat), at: now - rng.below(30 * DAY_MS as u64) as i64 })
        })
        .collect();
    store.apply(Lane::Background, opens).await?;
    store.indexed().await?;
    println!(
        "Built {total} messages in {chats} chats, {people} people and 600 opens through the writer in {:.1} s.",
        started.elapsed().as_secs_f64()
    );

    let loading = Instant::now();
    let finder = Finder::start(store.clone()).await?;
    println!(
        "The name index loaded in {:.1} ms and holds about {:.1} MB.\n",
        ms(loading.elapsed()),
        finder.bytes() as f64 / 1e6
    );

    let all = Vec::new();
    let names = vec![Kind::Chat, Kind::Person, Kind::Folder];
    let messages = vec![Kind::Message];
    println!(
        "A keystroke's find, each a round trip from a tokio task, {} runs of each of {} words typed a letter at a time:",
        plan.runs,
        TYPED.len()
    );
    for letters in 1..=6 {
        for (what, kinds) in [("everything", &all), ("names only", &names), ("messages only", &messages)] {
            let mut timings = Vec::new();
            let mut slowest = (Duration::ZERO, String::new());
            for typed in TYPED {
                let query: String = typed.chars().take(letters).collect();
                for _ in 0..plan.runs {
                    let find = Find { query: query.clone(), kinds: kinds.clone(), chat: None, limit: None };
                    let began = Instant::now();
                    finder.find(find, Arc::new(Cancel::default())).await?;
                    timings.push(began.elapsed());
                    slowest = slowest.max((began.elapsed(), query.clone()));
                }
            }
            let what = format!("{letters} letter{}, {what}", if letters == 1 { "" } else { "s" });
            println!("{}  slowest \"{}\"", Timings(timings).line(&what), slowest.1);
        }
    }

    let empty = Find::default();
    let mut timings = Vec::new();
    for _ in 0..plan.runs {
        let began = Instant::now();
        finder.find(empty.clone(), Arc::new(Cancel::default())).await?;
        timings.push(began.elapsed());
    }
    println!("{}", Timings(timings).line("nothing typed: the chats opened most"));

    for typed in ["ar", "lisbon", "jantar"] {
        let found = finder.find(Find { query: typed.into(), ..Find::default() }, Arc::new(Cancel::default())).await?;
        ensure!(!found.results.is_empty(), "\"{typed}\" found nothing");
        let first: Vec<String> = found
            .results
            .iter()
            .take(4)
            .map(|hit| format!("{:?} {}", hit.kind, hit.text.iter().map(|span| span.s.as_str()).collect::<String>()))
            .collect();
        println!("\n\"{typed}\": {:?}; first: {}", found.counts, first.join(" · "));
    }
    // Typing fast: each keystroke cancels the last, and only the last answers.
    let mut cancelled = 0;
    let began = Instant::now();
    let mut pending: Option<(Arc<Cancel>, tokio::task::JoinHandle<Result<_>>)> = None;
    for letters in 1..=6 {
        let query: String = "jantar".chars().take(letters).collect();
        let cancel = Arc::new(Cancel::default());
        if let Some((previous, task)) = pending.take() {
            previous.cancel();
            if task.await?.is_err() {
                cancelled += 1;
            }
        }
        let (finder, find) = (finder.clone(), Find { query, ..Find::default() });
        let task = tokio::spawn({
            let cancel = cancel.clone();
            async move { finder.find(find, cancel).await }
        });
        pending = Some((cancel, task));
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    if let Some((_, task)) = pending {
        task.await??;
    }
    println!(
        "\n\"jantar\" typed a letter every 2 ms: {cancelled} of 5 earlier finds cancelled, the last answered {:.1} ms after the first letter",
        ms(began.elapsed())
    );
    Ok(())
}
