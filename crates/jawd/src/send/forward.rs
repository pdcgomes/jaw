//! `forward`: messages sent again, each to each chat, as new messages of yours, in order, marked
//! forwarded once more than the original had been, as WhatsApp's phones send them. Media goes as
//! it came, its file left where WhatsApp keeps it, and a message edited says what it says now. All
//! of them are on disk, synced, before it answers, as `send.text`'s one is, and each then goes as a
//! message of yours does, after its chat's earlier sends, with its own outbox row and ticks. A
//! forward is chosen from a list of chats rather than sent in one, so it reads none of them.

use std::sync::Arc;

use jaw_proto::send::{Forward, Forwarded};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::actions::{Target, find, full};
use super::outbox::{CHAT_TO_SEND, bad_param, check_key, me, message_id, queue};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert;
use crate::store::outbox::{Enqueued, Forwarding, Step, Written, forwarded_by_key};
use crate::store::{Change, unpack};

/// The most chats a forward goes to at once, as the phones allow.
pub(super) const TO_MOST: usize = 5;

/// What a message can be forwarded as: the kinds that carry WhatsApp's forwarding mark.
fn forwardable(kind: &str) -> bool {
    matches!(
        kind,
        "text"
            | "image"
            | "video"
            | "gif"
            | "video_note"
            | "voice"
            | "audio"
            | "document"
            | "sticker"
            | "location"
            | "contact"
    )
}

/// What `target` says, as it's sent again, or why it can't be: an edited one says what its row
/// says now.
fn content(target: &Target) -> Result<e2e::Message, Error> {
    let refused = |why: &str| Error::new(code::NOT_ALLOWED, format!("{} can't be forwarded: {why}", target.id));
    if target.deleted {
        return Err(refused("it was deleted"));
    }
    if target.view_once {
        return Err(refused("it can be viewed once"));
    }
    let said = target
        .raw
        .as_deref()
        .and_then(|packed| unpack(packed).ok())
        .and_then(|raw| WebMessageInfo::decode(raw.as_slice()).ok())
        .and_then(|info| info.message)
        .map(|message| convert::bare(&message));
    let text = |text: &str| e2e::Message { conversation: Some(text.into()), ..Default::default() };
    let message = match (said, target.kind.as_str(), target.text.as_deref()) {
        (_, "text", Some(said)) if target.edited => text(said),
        (Some(message), _, Some(caption)) if target.edited => convert::captioned(&message, caption),
        (Some(message), _, _) => message,
        (None, "text", Some(said)) => text(said),
        (None, ..) => return Err(refused("jawd hasn't what it said")),
    };
    if !forwardable(convert::kinds::body(&message, &[]).kind) {
        return Err(refused("WhatsApp doesn't forward that kind"));
    }
    Ok(message)
}

pub(super) async fn forward(inner: &Arc<Inner>, forward: Forward) -> Result<Forwarded, Error> {
    check_key(&forward.key)?;
    if forward.messages.is_empty() {
        return Err(bad_param("messages", "there's nothing to forward"));
    }
    let mut to: Vec<String> = Vec::new();
    for chat in forward.to {
        if !to.contains(&chat) {
            to.push(chat);
        }
    }
    if to.is_empty() {
        return Err(bad_param("to", "a forward needs a chat to go to"));
    }
    if to.len() > TO_MOST {
        let mut error = Error::new(code::LIMIT, format!("a forward goes to {TO_MOST} chats at most"));
        error.data = Some(serde_json::json!({ "limit": "forward", "most": TO_MOST }));
        return Err(error);
    }
    if let Some(key) = forward.key.clone()
        && let Some(ids) = query(inner, move |db| forwarded_by_key(db, &key)).await?
    {
        return Ok(Forwarded { messages: ids });
    }
    let mut originals = Vec::with_capacity(forward.messages.len());
    for id in forward.messages {
        let target = find(inner, forward.chat.clone(), id).await?;
        originals.push((content(&target)?, target.forwarded));
    }
    let chats = to.clone();
    let (found, me) = query(inner, move |db| {
        let mut found = Vec::with_capacity(chats.len());
        for chat in &chats {
            let ephemeral: Option<u32> = db.prepare_cached(CHAT_TO_SEND)?.query_row([chat], |r| r.get(0)).optional()?;
            found.push(ephemeral);
        }
        Ok((found, me(db)?))
    })
    .await?;
    let mut sending = Vec::with_capacity(to.len() * originals.len());
    for (chat, ephemeral) in to.iter().zip(found) {
        let Some(ephemeral) = ephemeral else { return Err(no_chat(chat)) };
        for (message, forwarded) in &originals {
            let raw = convert::forwarded(message, forwarded.saturating_add(1), ephemeral)
                .ok_or_else(|| Error::new(code::NOT_ALLOWED, "WhatsApp doesn't forward that kind"))?;
            sending.push(forwarding(chat, raw, &me)?);
        }
    }
    let written = Written::default();
    let step = Step::Forward { sending, key: forward.key, written: written.clone() };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the forward, so it isn't sent").await?;
    match written.take() {
        Some(Enqueued::Many(all)) => {
            let mut ids = Vec::with_capacity(all.len());
            for enqueued in all {
                let Enqueued::Queued { id, chat, outbox, .. } = enqueued else { continue };
                if let Some(outbox) = outbox {
                    queue(inner, outbox, chat, false);
                }
                ids.push(id);
            }
            Ok(Forwarded { messages: ids })
        }
        Some(Enqueued::Full { waiting, failed }) => Err(full(waiting, failed)),
        _ => Err(Error::new(code::INTERNAL, "jawd kept nothing of the forward, so it isn't sent")),
    }
}

/// A forwarded message for `chat`, with an ID of its own, pending, as its row shows it.
fn forwarding(chat: &str, raw: e2e::Message, me: &convert::Me) -> Result<Forwarding, Error> {
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(chat.into()), from_me: Some(true), id: Some(id), participant: None },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of what's forwarded"));
    };
    message.at = at;
    message.expires_at = None;
    let kind = message.kind;
    Ok(Forwarding { message, kind, raw: raw.encode_to_vec() })
}
