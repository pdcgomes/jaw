//! The outbox's lane. A message is committed, synced, with its outbox row before `send.text`
//! answers; then it joins its chat's queue, which sends one message at a time with the ID jawd made,
//! so a message is sent once however often it's tried: WhatsApp knows a message by its ID, and one
//! sent again with the same ID is the same message to its recipients. A send that fails is tried
//! again after a pause that doubles, for as long as `Timing::give_up` allows, then it's failed until
//! `send.retry`; a restart queues what's left again, in order.

use std::sync::Arc;
use std::time::Instant;

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
use crate::store::outbox::{self, Outboxed, Step};
use crate::store::{Change, Lane, Person, read};

pub(super) const CHAT_EPHEMERAL: &str = "SELECT ephemeral FROM chats WHERE id = ?1";

/// The most messages waiting to send at once; more is refused until some have gone.
pub(super) const WAITING_MOST: u64 = 1_000;

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
    let chat = send.chat.clone();
    let found = query(inner, move |db| {
        let ephemeral: Option<u32> = db.prepare_cached(CHAT_EPHEMERAL)?.query_row([&chat], |r| r.get(0)).optional()?;
        Ok((ephemeral, outbox::waiting(db)?))
    })
    .await?;
    let ephemeral = match found {
        (None, _) => return Err(no_chat(&send.chat)),
        (Some(_), waiting) if waiting >= WAITING_MOST => {
            return Err(Error::new(
                code::LIMIT,
                format!("{waiting} messages are waiting to send; try again once they've gone"),
            ));
        }
        (Some(ephemeral), _) => ephemeral,
    };
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
    let step = Step::Enqueue { message: Box::new(message), kind: "text", raw: raw.encode_to_vec() };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the message, so it isn't sent").await?;
    let (chat, message) = (send.chat.clone(), id.clone());
    let (row, outboxed) = query(inner, move |db| {
        let row = read::message(db, &chat, &message)?.map(|(row, _)| feed::message_row(row));
        Ok((row, outbox::outboxed_by_id(db, &message)?))
    })
    .await?;
    if let Some(outboxed) = outboxed {
        queue(inner, outboxed.outbox, send.chat);
    }
    Ok(Sent { message: id, row })
}

/// Sends outbox row `outbox` after `chat`'s earlier sends.
fn queue(inner: &Arc<Inner>, outbox: i64, chat: String) {
    inner.sends.push(chat, deliver(inner.clone(), outbox));
}

/// Queues what the outbox held when jawd stopped, oldest first, so each chat's go in order.
pub(super) async fn resume(inner: &Arc<Inner>) -> anyhow::Result<()> {
    let queued = inner.store.reads().query(crate::work::Priority::Interactive, outbox::queued).await?;
    for (outbox, chat) in queued {
        queue(inner, outbox, chat);
    }
    Ok(())
}

/// Whether a message has got at least as far as the server.
fn reached_server(status: Option<&str>) -> bool {
    matches!(status, Some("sent" | "delivered" | "read" | "played"))
}

/// Sends outbox row `outbox` until the server has it, or it's failed. Each attempt reads the row
/// again, so a message sent, failed or deleted meanwhile isn't sent.
async fn deliver(inner: Arc<Inner>, outbox: i64) {
    let began = Instant::now();
    let mut pause = inner.timing.retry_first;
    loop {
        let row = match inner
            .store
            .reads()
            .query(crate::work::Priority::Interactive, move |db| outbox::outboxed_by_n(db, outbox))
            .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return,
            Err(e) => {
                eprintln!("jawd: can't read outbox row {outbox}, so it's tried again in {pause:?}: {e:#}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(inner.timing.retry_most);
                continue;
            }
        };
        if !row.queued {
            return;
        }
        if reached_server(row.status.as_deref()) {
            settle(&inner, vec![Change::Outbox(Step::Sent { outbox, at: None })]).await;
            return;
        }
        let send = pb::Send { chat: row.chat.clone(), id: row.id.clone(), raw: row.raw.clone() };
        let failure = match engine(&inner, pb::command::Kind::Send(send)).await {
            Ok(pb::reply::Kind::Sent(sent)) => {
                settle(&inner, sent_changes(&row, &sent)).await;
                return;
            }
            Ok(other) => {
                EngineError::Failed { message: format!("the engine answered a send with {other:?}"), retry: false }
            }
            Err(e) => e,
        };
        let again = retryable(&failure) && began.elapsed() + pause <= inner.timing.give_up;
        let next_at = again.then(|| now_ms() + pause.as_millis() as i64);
        let failed = Step::Failed { outbox, error: failure.to_string(), next_at };
        if let Err(e) = inner.store.apply(Lane::Live, vec![Change::Outbox(failed)]).await {
            eprintln!("jawd: can't note that {} didn't send: {e:#}", row.id);
        }
        if !again {
            eprintln!("jawd: {} in {} didn't send, and waits for a retry: {failure}", row.id, row.chat);
            return;
        }
        tokio::select! {
            _ = tokio::time::sleep(pause) => {}
            _ = inner.online.notified() => {}
        }
        pause = (pause * 2).min(inner.timing.retry_most);
    }
}

/// What the server's answer changes: the message is sent, at the server's time; and when WhatsApp
/// delivered it to another of the person's JIDs, such as their LID for a phone number, the two are
/// joined, so receipts addressed to either find the chat.
fn sent_changes(row: &Outboxed, sent: &pb::Sent) -> Vec<Change> {
    let at = (sent.at_ms > 0).then_some(sent.at_ms);
    let mut changes = vec![Change::Outbox(Step::Sent { outbox: row.outbox, at })];
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

/// Queues a failed message of yours again.
pub(super) async fn retry(inner: &Arc<Inner>, retry: SendRetry) -> Result<Value, Error> {
    let id = retry.message.clone();
    let Some(row) = query(inner, move |db| outbox::outboxed_by_id(db, &id)).await? else {
        return Err(Error::new(
            code::NOT_FOUND,
            format!("jawd has nothing of yours waiting to send as {}", retry.message),
        ));
    };
    let done = Value::Object(Default::default());
    if row.queued {
        return Ok(done);
    }
    if reached_server(row.status.as_deref()) {
        durably(inner, vec![Change::Outbox(Step::Sent { outbox: row.outbox, at: None })], "keep that it was sent")
            .await?;
        return Ok(done);
    }
    durably(inner, vec![Change::Outbox(Step::Retry { outbox: row.outbox })], "queue the message again").await?;
    queue(inner, row.outbox, row.chat);
    Ok(done)
}
