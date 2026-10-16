//! The outbox's lane. A message is committed, synced, with its outbox row before `send.text`
//! answers, and once it's committed the answer is a success; then it joins its chat's queue, which
//! sends one message at a time with the ID jawd made. As on the phone, a message waits, pending, for
//! as long as WhatsApp can't be reached; while it can, a send that fails on the way (the network,
//! the server busy) is tried again after a pause that doubles, without holding a place other chats
//! could send in. A refusal (a number not on WhatsApp, a group you've left, a message the server
//! won't take) fails it until `send.retry`, unless an earlier attempt may have got through: then it
//! waits, unconfirmed, for a receipt, and fails only if none comes within a day. A restart queues
//! what's left again, in order.

use std::sync::Arc;
use std::time::Duration;

use jaw_proto::send::{SendRetry, SendText, Sent};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::{Inner, TEXT_MOST, durably, engine, internal, no_chat, now_ms, query, retryable};
use crate::convert::{self, Me};
use crate::engine::{EngineError, pb};
use crate::feed;
use crate::store::outbox::{self, Enqueued, Outboxed, Step, Written};
use crate::store::{Change, Lane, Person, read};
use crate::work::{Next, Priority};

pub(super) const CHAT_EPHEMERAL: &str = "SELECT ephemeral FROM chats WHERE id = ?1";

/// The longest key an interface may give a message.
const KEY_MOST: usize = 128;
/// How long a message WhatsApp refused, when it may already have gone, waits for a receipt.
pub(super) const UNCONFIRMED_FOR: Duration = Duration::from_secs(24 * 3600);
/// How many attempts while WhatsApp can be reached, about a day's worth at the longest pause,
/// before a failure that might pass is taken as one that won't.
const ATTEMPTS_MOST: i64 = 1_500;

/// A new message ID in whatsmeow's format for messages from this kind of device: `3EB0` and 18
/// upper-case hex digits, from 72 random bits.
pub(super) fn message_id() -> std::io::Result<String> {
    let mut bytes = [0u8; 9];
    // SAFETY: getentropy fills the buffer it's given, which may be up to 256 bytes.
    if unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(bytes.iter().fold(String::from("3EB0"), |mut id, byte| {
        id.push_str(&format!("{byte:02X}"));
        id
    }))
}

/// Text as WhatsApp sends it: plain, or with the chat's disappearing timer when it has one, so it
/// disappears as the chat's other messages do.
fn text_message(text: &str, ephemeral: u32) -> e2e::Message {
    if ephemeral == 0 {
        return e2e::Message { conversation: Some(text.into()), ..Default::default() };
    }
    let context = e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() };
    let extended = e2e::ExtendedTextMessage {
        text: Some(text.into()),
        context_info: Some(Box::new(context)),
        ..Default::default()
    };
    e2e::Message { extended_text_message: Some(Box::new(extended)), ..Default::default() }
}

/// The answer for message `id` in `chat`, with its row if it can be read; a message that's committed
/// is answered as sent on its way whatever the read does.
async fn answer(inner: &Inner, chat: String, id: String) -> Sent {
    let message = id.clone();
    let row = inner
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(read::message(db, &chat, &message)?.map(|(row, _)| feed::message_row(row)))
        })
        .await;
    match row {
        Ok(row) => Sent { message: id, row },
        Err(e) => {
            eprintln!("jawd: {id} is on its way, but its row couldn't be read for the answer: {e:#}");
            Sent { message: id, row: None }
        }
    }
}

pub(super) async fn send_text(inner: &Arc<Inner>, send: SendText) -> Result<Sent, Error> {
    if send.reply.is_some() || !send.mentions.is_empty() {
        let mut error = Error::new(code::INVALID_PARAMS, "replies and mentions come with the next milestone");
        error.data = Some(serde_json::json!({ "param": if send.reply.is_some() { "reply" } else { "mentions" } }));
        return Err(error);
    }
    if send.text.trim().is_empty() {
        return Err(Error::new(code::INVALID_PARAMS, "there's nothing to send"));
    }
    if send.text.chars().count() > TEXT_MOST {
        return Err(Error::new(code::LIMIT, format!("a message can be {TEXT_MOST} characters at most")));
    }
    if let Some(key) = &send.key
        && (key.is_empty() || key.len() > KEY_MOST)
    {
        return Err(Error::new(code::INVALID_PARAMS, format!("a key is 1 to {KEY_MOST} bytes")));
    }
    let (chat, key) = (send.chat.clone(), send.key.clone());
    let (ephemeral, keyed) = query(inner, move |db| {
        let ephemeral: Option<u32> = db.prepare_cached(CHAT_EPHEMERAL)?.query_row([&chat], |r| r.get(0)).optional()?;
        let keyed = key.as_deref().map(|key| outbox::keyed(db, key)).transpose()?.flatten();
        Ok((ephemeral, keyed))
    })
    .await?;
    if let Some((chat, id)) = keyed {
        return Ok(answer(inner, chat, id).await);
    }
    let Some(ephemeral) = ephemeral else { return Err(no_chat(&send.chat)) };
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let raw = text_message(&send.text, ephemeral);
    let web = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(send.chat.clone()),
            from_me: Some(true),
            id: Some(id.clone()),
            participant: None,
        },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, &Me::default(), true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that text"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "text",
        raw: raw.encode_to_vec(),
        key: send.key.clone(),
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the message, so it isn't sent").await?;
    match written.take() {
        Some(Enqueued::Queued { id, chat, outbox, .. }) => {
            if let Some(outbox) = outbox {
                queue(inner, outbox, chat.clone(), false);
            }
            drop(_order);
            Ok(answer(inner, chat, id).await)
        }
        Some(Enqueued::Full { waiting, failed }) => {
            let mut error = Error::new(
                code::LIMIT,
                format!(
                    "{waiting} messages of yours are waiting to send, {failed} of them failed; \
                    jawd takes more once some have gone or been retried"
                ),
            );
            error.data = Some(serde_json::json!({ "limit": "outbox", "waiting": waiting, "failed": failed }));
            Err(error)
        }
        None => Err(Error::new(code::INTERNAL, "jawd kept nothing of the message, so it isn't sent")),
    }
}

/// Sends outbox row `outbox` after `chat`'s earlier sends, `chat` being the JID jaw keeps the chat
/// under, so sends named by a number and by a LID share one queue. `resumed` is for a row left from
/// before jawd started, which may have gone already.
fn queue(inner: &Arc<Inner>, outbox: i64, chat: String, resumed: bool) {
    let inner = inner.clone();
    inner.sends.clone().push_again(chat, move || attempt(inner.clone(), outbox, resumed));
}

/// Queues what the outbox held when jawd stopped, oldest first, so each chat's go in order, and
/// gives what waits for a receipt the rest of its time.
pub(super) async fn resume(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let queued = inner.store.reads().query(Priority::Interactive, outbox::queued).await?;
    for (outbox, chat) in queued {
        queue(inner, outbox, chat, true);
    }
    let now = now_ms();
    for (outbox, until) in inner.store.reads().query(Priority::Interactive, outbox::unconfirmed).await? {
        let left = Duration::from_millis(until.saturating_sub(now).max(0) as u64).min(UNCONFIRMED_FOR);
        unconfirmed(inner, outbox, left);
    }
    Ok(())
}

/// Fails outbox row `outbox` after `left`, if no receipt has come for it by then.
fn unconfirmed(inner: &Arc<Inner>, outbox: i64, left: Duration) {
    let inner = inner.clone();
    tokio::spawn(async move {
        tokio::time::sleep(left).await;
        let error = "WhatsApp refused it when it may already have gone, and no receipt came".to_string();
        let failed = Step::Failed { outbox, error, next_at: None };
        if let Err(e) = inner.store.apply(Lane::Live, vec![Change::Outbox(failed)]).await {
            eprintln!("jawd: can't note that outbox row {outbox} had no receipt: {e:#}");
        }
    });
}

/// Whether a message has got at least as far as the server.
fn reached_server(status: Option<&str>) -> bool {
    matches!(status, Some("sent" | "delivered" | "read" | "played"))
}

/// The pause after `attempts` failed attempts.
fn pause(inner: &Inner, attempts: i64) -> Duration {
    let doubled = inner.timing.retry_first.saturating_mul(1 << attempts.clamp(0, 16));
    doubled.min(inner.timing.retry_most)
}

/// Tries to send outbox row `outbox` once. It reads the row again first, so a message sent, failed
/// or deleted meanwhile isn't sent, and nothing is tried while WhatsApp can't be reached. What it
/// waits for before the next try, it waits for without holding a place in the sends' lane.
async fn attempt(inner: Arc<Inner>, outbox: i64, resumed: bool) -> Next {
    let mut connected = inner.connected.subscribe();
    if !*connected.borrow_and_update() {
        return Next::After(Box::pin(async move {
            let _ = connected.wait_for(|connected| *connected).await;
        }));
    }
    let row = match inner.store.reads().query(Priority::Interactive, move |db| outbox::outboxed_by_n(db, outbox)).await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Next::Done,
        Err(e) => {
            eprintln!("jawd: can't read outbox row {outbox}, so it's tried again: {e:#}");
            return Next::After(Box::pin(tokio::time::sleep(inner.timing.retry_most)));
        }
    };
    if row.state != "queued" {
        return Next::Done;
    }
    if reached_server(row.status.as_deref()) {
        settle(&inner, vec![Change::Outbox(Step::Sent { outbox })]).await;
        return Next::Done;
    }
    let may_have_gone = resumed || row.attempts > 0;
    let send = pb::Send { chat: row.chat.clone(), id: row.id.clone(), raw: row.raw.clone() };
    let failure = match engine(&inner, pb::command::Kind::Send(send)).await {
        Ok(pb::reply::Kind::Sent(sent)) => {
            settle(&inner, sent_changes(&row, &sent)).await;
            return Next::Done;
        }
        Ok(other) => {
            EngineError::Failed { message: format!("the engine answered a send with {other:?}"), retry: false }
        }
        Err(e) => e,
    };
    if may_have_gone {
        eprintln!("jawd: WhatsApp answered {} in {} sent again with: {failure}", row.id, row.chat);
    }
    let gives_up = !retryable(&failure) || row.attempts + 1 >= ATTEMPTS_MOST;
    let step = match (gives_up, may_have_gone) {
        (false, _) => {
            let next_at = now_ms() + pause(&inner, row.attempts + 1).as_millis() as i64;
            Step::Failed { outbox, error: failure.to_string(), next_at: Some(next_at) }
        }
        (true, true) => Step::Unconfirmed {
            outbox,
            error: failure.to_string(),
            until: now_ms() + UNCONFIRMED_FOR.as_millis() as i64,
        },
        (true, false) => Step::Failed { outbox, error: failure.to_string(), next_at: None },
    };
    let unconfirming = matches!(step, Step::Unconfirmed { .. });
    if let Err(e) = inner.store.apply(Lane::Live, vec![Change::Outbox(step)]).await {
        eprintln!("jawd: can't note that {} didn't send: {e:#}", row.id);
    }
    if unconfirming {
        unconfirmed(&inner, outbox, UNCONFIRMED_FOR);
    }
    if gives_up {
        eprintln!("jawd: WhatsApp refused {} in {}: {failure}", row.id, row.chat);
        return Next::Done;
    }
    let pause = pause(&inner, row.attempts + 1);
    Next::After(Box::pin(async move {
        tokio::select! {
            _ = tokio::time::sleep(pause) => {}
            _ = connected.changed() => {}
        }
    }))
}

/// What the server's answer changes: the message is sent; and when WhatsApp delivered it to another
/// of the person's JIDs, such as their LID for a phone number, the two are joined, so receipts
/// addressed to either find the chat.
fn sent_changes(row: &Outboxed, sent: &pb::Sent) -> Vec<Change> {
    let mut changes = vec![Change::Outbox(Step::Sent { outbox: row.outbox })];
    let to = convert::user_jid(&sent.chat);
    if convert::is_person(&to) && convert::is_person(&row.chat) && to != row.chat {
        changes.push(Change::Person(Person { jids: vec![row.chat.clone(), to], ..Person::default() }));
    }
    changes
}

/// Keeps what a send came to, trying again until it's kept: the server has the message, so it
/// mustn't be sent again for want of a commit.
async fn settle(inner: &Inner, changes: Vec<Change>) {
    let mut pause = inner.timing.retry_first;
    loop {
        match inner.store.apply(Lane::Live, changes.clone()).await {
            Ok(_) => return,
            Err(e) => {
                eprintln!("jawd: can't keep that a message was sent, so it's tried again in {pause:?}: {e:#}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(inner.timing.retry_most);
            }
        }
    }
}

/// Queues a failed message of yours again; one still on its way or waiting for a receipt has
/// nothing to retry.
pub(super) async fn retry(inner: &Arc<Inner>, retry: SendRetry) -> Result<Value, Error> {
    let id = retry.message.clone();
    let Some(row) = query(inner, move |db| outbox::outboxed_by_id(db, &id)).await? else {
        return Err(Error::new(
            code::NOT_FOUND,
            format!("jawd has nothing of yours waiting to send as {}", retry.message),
        ));
    };
    let done = Value::Object(Default::default());
    if row.state != "failed" {
        return Ok(done);
    }
    if reached_server(row.status.as_deref()) {
        durably(inner, vec![Change::Outbox(Step::Sent { outbox: row.outbox })], "keep that it was sent").await?;
        return Ok(done);
    }
    durably(inner, vec![Change::Outbox(Step::Retry { outbox: row.outbox })], "queue the message again").await?;
    queue(inner, row.outbox, row.chat, false);
    Ok(done)
}
