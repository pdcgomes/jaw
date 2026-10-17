//! Read receipts. `read` moves the chat's one read position in a synced commit, as sync moves it
//! for WhatsApp, which counts its unread messages again, and owes WhatsApp a receipt for the
//! messages from others between how far it was told and there; so does a message you send here. A chat's receipts go at once, then at most once every
//! `Timing::read_every`, however fast it's read, each naming the messages owed since the last, a
//! thousand to a receipt, oldest first, with how far WhatsApp was told never past what it heard;
//! what the engine couldn't send waits for the connection to come back. whatsmeow sends them as
//! read-self when read receipts are off on the phone, so only your own devices hear.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use jaw_proto::send::Read;
use jaw_proto::{Error, code};
use serde_json::Value;

use rusqlite::{Connection, OptionalExtension};

use super::{Inner, durably, engine, no_chat, now_ms, query};
use crate::engine::pb;
use crate::store::outbox::{self, Owed, Owing, Step};
use crate::store::{Change, Lane};

/// A chat by its JID, or the chat it's being merged into, as the writer finds it.
pub(super) const CHAT_BY_JID: &str =
    "SELECT coalesce(m.keep, c.n) FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n WHERE c.id = ?1";
const MESSAGE_IN: &str = "SELECT 1 FROM messages WHERE chat = ?1 AND id = ?2";
const MESSAGE_MERGING_INTO: &str =
    "SELECT 1 FROM chat_merges c JOIN messages m ON m.chat = c.gone AND m.id = ?2 WHERE c.keep = ?1";

/// Chat `chat`, by JID, as the writer finds it, and whether it has message `id`, there or still in
/// a chat being merged into it.
pub(super) fn read_target(db: &Connection, chat: &str, id: &str) -> anyhow::Result<Option<(i64, bool)>> {
    let Some(n): Option<i64> = db.prepare_cached(CHAT_BY_JID)?.query_row([chat], |r| r.get(0)).optional()? else {
        return Ok(None);
    };
    let found = db.prepare_cached(MESSAGE_IN)?.exists(rusqlite::params![n, id])?
        || db.prepare_cached(MESSAGE_MERGING_INTO)?.exists(rusqlite::params![n, id])?;
    Ok(Some((n, found)))
}

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
    let n = match query(inner, move |db| read_target(db, &chat, &id)).await? {
        None => return Err(no_chat(&read.chat)),
        Some((_, false)) => {
            return Err(Error::new(code::NOT_FOUND, format!("jawd has no message {} in {}", read.up_to, read.chat)));
        }
        Some((n, true)) => n,
    };
    let read = crate::store::Read { chat: read.chat, ids: vec![read.up_to], here: true, ..Default::default() };
    durably(inner, vec![Change::Read(read)], "keep how far you've read").await?;
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
pub(super) fn want(inner: &Arc<Inner>, chat: i64) {
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
            match tell(&inner, chat).await {
                // More than one receipt's worth is owed: the next lot goes straight after.
                Ok(Told::More) => continue,
                Ok(Told::All) => {}
                // What's owed is worked out once the merge is done, by the chat that's left.
                Ok(Told::Merging { into }) => {
                    tokio::time::sleep(inner.timing.read_every).await;
                    if into != chat {
                        inner.reads.chats.lock().expect("read receipts").remove(&chat);
                        want(&inner, into);
                        return;
                    }
                    continue;
                }
                Err(e) => {
                    eprintln!("jawd: can't send read receipts, so they wait for the connection: {e:#}");
                    tokio::select! {
                        _ = tokio::time::sleep(pause) => {}
                        _ = inner.online.notified() => {}
                    }
                    pause = (pause * 2).min(inner.timing.retry_most);
                    continue;
                }
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

/// What a lot of receipts came to.
enum Told {
    All,
    More,
    Merging { into: i64 },
}

/// Tells WhatsApp the next lot of what `chat` owes, oldest first, one receipt per sender in a group,
/// then keeps how far it was told, which is never past a message it named.
async fn tell(inner: &Inner, chat: i64) -> anyhow::Result<Told> {
    let owed =
        match inner.store.reads().query(crate::work::Priority::Interactive, move |db| outbox::owed(db, chat)).await? {
            Owing::Nothing => return Ok(Told::All),
            Owing::Merging { into } => return Ok(Told::Merging { into }),
            Owing::Receipts(owed) => owed,
        };
    let more = owed.messages.len() == outbox::RECEIPTS_AT_ONCE;
    for mark in receipts(&owed) {
        engine(inner, pb::command::Kind::MarkRead(mark)).await?;
    }
    let told = Step::Told { chat, from: owed.from, upto: owed.upto };
    inner.store.apply(Lane::Live, vec![Change::Outbox(told)]).await?;
    Ok(if more { Told::More } else { Told::All })
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
