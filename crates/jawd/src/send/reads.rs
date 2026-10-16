//! Read receipts. `read` moves the chat's read position forward in a synced commit, which counts
//! its unread messages again and owes WhatsApp a receipt for the messages from others between how
//! far it was told and there. A chat's receipts go at once, then at most once every
//! `Timing::read_every`, however fast it's read, each naming every message owed since the last;
//! what the engine couldn't send waits for the connection to come back. whatsmeow sends them as
//! read-self when read receipts are off on the phone, so only your own devices hear.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jaw_proto::send::Read;
use jaw_proto::{Error, code};
use serde_json::Value;

use super::{Inner, durably, engine, no_chat, now_ms, query};
use crate::engine::pb;
use crate::store::outbox::{self, Owed, Step};
use crate::store::{Change, Lane, read};

#[derive(Default)]
pub(super) struct Reads {
    chats: std::sync::Mutex<HashMap<i64, Flushing>>,
}

/// A chat's receipts: when they last went, whether a flush is on its way, and whether the chat was
/// read again since that flush began.
#[derive(Default)]
struct Flushing {
    last: Option<Instant>,
    due: bool,
    again: bool,
}

pub(super) async fn read(inner: &Arc<Inner>, read: Read) -> Result<Value, Error> {
    let (chat, id) = (read.chat.clone(), read.up_to.clone());
    let found = query(inner, move |db| Ok((read::chat_n(db, &chat)?, read::message_place(db, &chat, &id)?))).await?;
    let n = match found {
        (None, _) => return Err(no_chat(&read.chat)),
        (Some(_), None) => {
            return Err(Error::new(code::NOT_FOUND, format!("jawd has no message {} in {}", read.up_to, read.chat)));
        }
        (Some(n), Some(_)) => n,
    };
    durably(inner, vec![Change::Outbox(Step::Read { chat: read.chat, id: read.up_to })], "keep how far you've read")
        .await?;
    want(inner, n);
    Ok(Value::Object(Default::default()))
}

/// Sends the receipts owed by every chat, as jawd starts.
pub(super) async fn resume(inner: &Arc<Inner>) -> anyhow::Result<()> {
    for chat in inner.store.reads().query(crate::work::Priority::Interactive, outbox::owing).await? {
        want(inner, chat);
    }
    Ok(())
}

/// Chat `chat` owes receipts: they go now, or once `read_every` has passed since the last.
fn want(inner: &Arc<Inner>, chat: i64) {
    let wait = {
        let mut chats = inner.reads.chats.lock().expect("read receipts");
        let flushing = chats.entry(chat).or_default();
        if flushing.due {
            flushing.again = true;
            return;
        }
        flushing.due = true;
        flushing.last.map_or(Duration::ZERO, |last| inner.timing.read_every.saturating_sub(last.elapsed()))
    };
    let inner = inner.clone();
    tokio::spawn(async move {
        tokio::time::sleep(wait).await;
        let mut pause = inner.timing.retry_first;
        loop {
            inner.reads.chats.lock().expect("read receipts").entry(chat).or_default().again = false;
            if let Err(e) = tell(&inner, chat).await {
                eprintln!("jawd: can't send read receipts, so they wait for the connection: {e:#}");
                tokio::select! {
                    _ = tokio::time::sleep(pause) => {}
                    _ = inner.online.notified() => {}
                }
                pause = (pause * 2).min(inner.timing.retry_most);
                continue;
            }
            {
                let mut chats = inner.reads.chats.lock().expect("read receipts");
                let flushing = chats.entry(chat).or_default();
                flushing.last = Some(Instant::now());
                if !flushing.again {
                    flushing.due = false;
                    return;
                }
            }
            tokio::time::sleep(inner.timing.read_every).await;
        }
    });
}

/// Tells WhatsApp what `chat` owes, one receipt per sender in a group, then keeps how far it was told.
async fn tell(inner: &Inner, chat: i64) -> anyhow::Result<()> {
    let Some(owed) =
        inner.store.reads().query(crate::work::Priority::Interactive, move |db| outbox::owed(db, chat)).await?
    else {
        return Ok(());
    };
    for mark in receipts(&owed) {
        engine(inner, pb::command::Kind::MarkRead(mark)).await?;
    }
    let told = Step::Told { chat, at: owed.upto.0, n: owed.upto.1 };
    inner.store.apply(Lane::Live, vec![Change::Outbox(told)]).await?;
    Ok(())
}

/// The receipts for what's owed: one for a one-to-one chat, one per sender in a group, where a
/// receipt names who sent the messages it's for.
fn receipts(owed: &Owed) -> Vec<pb::MarkRead> {
    let mut by_sender: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, sender) in &owed.messages {
        let sender = if owed.group { sender.clone() } else { Some(String::new()) };
        if let Some(sender) = sender {
            by_sender.entry(sender).or_default().push(id.clone());
        }
    }
    let at_ms = now_ms();
    by_sender.into_iter().map(|(sender, ids)| pb::MarkRead { chat: owed.jid.clone(), sender, ids, at_ms }).collect()
}
