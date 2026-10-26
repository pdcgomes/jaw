//! `jawd bench send`: `send.text` answered, its synced commit included, and a send's round trip
//! through the demo engine, whose recipients answer at once: from asking to sent (the engine's
//! reply committed) and to read (two receipts kept in the inbox and converted). Then each message
//! action answered, with the recipients' receipts held back, as they come much later in real use:
//! a reply with a mention, a reaction, an edit, a deletion for everyone and for you, and a forward
//! to one chat and to five, each with its synced commit; and `message.info` and `chat.info`, which
//! only read.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use super::{Sending, Timing};
use crate::engine::demo::{self, DemoEngine, Replies};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::store::{Change, Chat, Lane, Store};
use crate::work::Priority;

pub struct Plan {
    pub count: u32,
    pub trips: u32,
    pub chats: u32,
}

fn percentile(samples: &mut [Duration], q: usize) -> Duration {
    samples.sort_unstable();
    samples.get(samples.len() * q / 100).or(samples.last()).copied().unwrap_or_default()
}

fn chat(n: u32) -> String {
    format!("3519{n:08}@s.whatsapp.net")
}

pub async fn run(home: &Path, plan: Plan) -> Result<()> {
    let store = Store::open(home)?;
    let engine = DemoEngine::with(vec![demo::connected()]);
    let world = engine.world();
    world.replying(Replies { delivered: Duration::ZERO, read: Duration::ZERO });
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let live: crate::sync::Shared = Arc::default();
    let (pushes, _) = broadcast::channel(256);
    let sync = crate::sync::run(
        incoming,
        store.clone(),
        live.clone(),
        pushes.clone(),
        Some(supervisor.clone()),
        home.to_path_buf(),
    );
    tokio::spawn(sync);
    let chats: Vec<Change> =
        (0..plan.chats.max(1)).map(|n| Change::Chat(Chat { id: chat(n), ..Chat::default() })).collect();
    store.apply(Lane::Live, chats).await?;
    let attention = std::sync::Arc::new(crate::attention::Attention::default());
    let sending = Sending::start(
        store.clone(),
        Some(supervisor),
        live,
        &pushes,
        attention.focus(),
        Timing::default(),
        super::link::Previews::off(),
    )
    .await?;

    let mut answered = Vec::with_capacity(plan.count as usize);
    let started = Instant::now();
    for n in 0..plan.count {
        let asked = Instant::now();
        let params = json!({ "chat": chat(n % plan.chats.max(1)), "text": format!("message {n}, on its way") });
        sending.call("send.text", params).await.map_err(|e| anyhow::anyhow!(e.message)).context("send.text")?;
        answered.push(asked.elapsed());
    }
    let took = started.elapsed();
    let mut committed = store.committed();
    let (mut to_sent, mut to_read) = (Vec::new(), Vec::new());
    for n in 0..plan.trips {
        let asked = Instant::now();
        let to = chat(plan.chats.max(1) + n);
        store.apply(Lane::Live, vec![Change::Chat(Chat { id: to.clone(), ..Chat::default() })]).await?;
        let sent = sending.call("send.text", json!({ "chat": to, "text": "there and back" })).await;
        let id = sent.map_err(|e| anyhow::anyhow!(e.message))?["message"].as_str().unwrap_or_default().to_string();
        let mut seen_sent = false;
        loop {
            let (chat, message) = (to.clone(), id.clone());
            let status = store
                .reads()
                .query(Priority::Interactive, move |db| {
                    Ok(crate::store::read::message(db, &chat, &message)?.and_then(|(row, _)| row.status))
                })
                .await?;
            if !seen_sent && matches!(status.as_deref(), Some("sent" | "delivered" | "read")) {
                to_sent.push(asked.elapsed());
                seen_sent = true;
            }
            if status.as_deref() == Some("read") {
                to_read.push(asked.elapsed());
                break;
            }
            tokio::time::timeout(Duration::from_secs(10), committed.changed())
                .await
                .context("a send that never came back")??;
        }
    }
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "send.text answered, its synced commit included: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms ({} sends to {} chats in {:.2} s)",
        ms(percentile(&mut answered, 50)),
        ms(percentile(&mut answered, 99)),
        ms(percentile(&mut answered, 100)),
        plan.count,
        plan.chats,
        took.as_secs_f64()
    );
    println!(
        "a send's round trip through the demo engine: to sent p50 {:.2} ms, p99 {:.2} ms; to read p50 {:.2} ms, p99 {:.2} ms ({} trips)",
        ms(percentile(&mut to_sent, 50)),
        ms(percentile(&mut to_sent, 99)),
        ms(percentile(&mut to_read, 50)),
        ms(percentile(&mut to_read, 99)),
        plan.trips
    );
    world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    actions(&store, &sending, plan.count.clamp(1, 150)).await
}

/// Times each message action, `count` of each, as an interface asks it.
async fn actions(store: &Store, sending: &Sending, count: u32) -> Result<()> {
    const GROUP: &str = "120363999999999999@g.us";
    const ANA: &str = "351929999999@s.whatsapp.net";
    let members = vec![(ANA.to_string(), "member"), ("351910000001@s.whatsapp.net".to_string(), "member")];
    let setup = vec![
        Change::Remember { key: "me".into(), value: "351910000001@s.whatsapp.net".into() },
        Change::Members(crate::store::Members { chat: GROUP.into(), all: true, set: members, remove: Vec::new() }),
        Change::Person(crate::store::Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Default::default() }),
    ];
    store.apply(Lane::Live, setup).await?;
    let theirs: Vec<Change> = (0..count * 2)
        .map(|n| {
            Change::Message(crate::store::Message {
                chat: GROUP.into(),
                id: format!("theirs{n}"),
                at: super::now_ms() - 60_000,
                sender: Some(ANA.into()),
                kind: "text",
                text: Some(format!("message {n}")),
                summary: format!("message {n}"),
                live: true,
                ..Default::default()
            })
        })
        .collect();
    store.apply(Lane::Live, theirs).await?;
    let ana: i64 = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [ANA], |r| r.get(0))?)
        })
        .await?;
    let mut mine = Vec::with_capacity(count as usize);
    let mut timed: Vec<(&str, Vec<Duration>)> = Vec::new();
    let mut time = async |what: &'static str, method: &str, params: serde_json::Value| -> Result<serde_json::Value> {
        let asked = Instant::now();
        let answer = sending.call(method, params).await.map_err(|e| anyhow::anyhow!(e.message)).context(what)?;
        let took = asked.elapsed();
        match timed.iter_mut().find(|(name, _)| *name == what) {
            Some((_, samples)) => samples.push(took),
            None => timed.push((what, vec![took])),
        }
        Ok(answer)
    };
    for n in 0..count {
        let params = json!({ "chat": GROUP, "text": "@Ana vens?", "reply": format!("theirs{n}"), "mentions": [format!("person:{ana}")] });
        let sent = time("send.text, a reply with a mention", "send.text", params).await?;
        mine.push(sent["message"].as_str().unwrap_or_default().to_string());
    }
    for n in 0..count {
        time("react", "react", json!({ "message": format!("theirs{n}"), "emoji": "👍", "chat": GROUP })).await?;
        time("react, found without its chat", "react", json!({ "message": format!("theirs{n}"), "emoji": "❤️" }))
            .await?;
        time("edit", "edit", json!({ "message": mine[n as usize], "text": "@Ana vens já?", "chat": GROUP })).await?;
        time("message.info", "message.info", json!({ "message": mine[n as usize], "chat": GROUP })).await?;
        time("chat.info", "chat.info", json!({ "chat": GROUP })).await?;
        time(
            "forward, to one chat",
            "forward",
            json!({ "messages": [format!("theirs{n}")], "to": [chat(0)], "chat": GROUP }),
        )
        .await?;
        let five: Vec<String> = (0..5).map(chat).collect();
        time(
            "forward, to five chats",
            "forward",
            json!({ "messages": [format!("theirs{n}")], "to": five, "chat": GROUP }),
        )
        .await?;
        time(
            "delete, for everyone",
            "delete",
            json!({ "message": mine[n as usize], "for": "everyone", "chat": GROUP }),
        )
        .await?;
        let theirs = format!("theirs{}", count + n);
        time("delete, for you", "delete", json!({ "message": theirs, "for": "me", "chat": GROUP })).await?;
    }
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    for (what, samples) in &mut timed {
        println!(
            "{what} answered: p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms ({} asked)",
            ms(percentile(samples, 50)),
            ms(percentile(samples, 99)),
            ms(percentile(samples, 100)),
            samples.len()
        );
    }
    Ok(())
}
