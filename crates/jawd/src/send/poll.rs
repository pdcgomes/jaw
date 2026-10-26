//! Polls: `send.poll` sends one as a message of yours with a fresh secret of its own, which whatsmeow
//! keeps as it sends it, and `poll.vote` votes in one through the engine, which has every poll's
//! secret and encrypts the vote with it. A vote is an action of yours on the poll, as a reaction is:
//! it goes as a message of its own after its chat's earlier sends, is shown at once, and is kept as
//! done once WhatsApp has it. A poll's tallies are counted from the votes jaw keeps whenever a
//! window reads it, never kept by hand.

use std::sync::Arc;

use jaw_proto::send::{PollVote, SendPoll, Sent};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::actions::{already, enqueue, find};
use super::outbox::{CHAT_TO_SEND, answer, bad_param, check_key, enqueued, me, message_id};
use super::{Inner, composing, durably, internal, no_chat, now_ms, query};
use crate::convert;
use crate::engine::pb;
use crate::store::outbox::{Step, Written, keyed};
use crate::store::polls::option_hash;
use crate::store::{Amendment, Change};

/// The most options a poll has, and the longest question and option, in characters, as WhatsApp's
/// phones allow.
const OPTIONS_MOST: usize = 12;
const QUESTION_MOST: usize = 255;
const OPTION_MOST: usize = 100;

/// The protocol's error for something longer than WhatsApp takes, saying which and how long.
fn too_long(what: &str, most: usize, message: String) -> Error {
    let mut error = Error::new(code::LIMIT, message);
    error.data = Some(serde_json::json!({ "limit": what, "most": most }));
    error
}

/// A new secret for a poll, 32 random bytes, as WhatsApp's phones make one: each vote in it is
/// encrypted with a key made from it.
fn secret() -> std::io::Result<Vec<u8>> {
    let mut bytes = vec![0u8; 32];
    // SAFETY: getentropy fills the buffer it's given, which may be up to 256 bytes.
    if unsafe { libc::getentropy(bytes.as_mut_ptr().cast(), bytes.len()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(bytes)
}

/// A poll as WhatsApp's phones send one: the question and its options, how many a vote may choose
/// (0 for any number), version 3 when that's one and the first otherwise, the chat's disappearing
/// timer, and the poll's secret.
fn poll_message(question: &str, options: &[String], max: usize, ephemeral: u32, secret: Vec<u8>) -> e2e::Message {
    let creation = e2e::PollCreationMessage {
        name: Some(question.into()),
        options: options
            .iter()
            .map(|name| e2e::poll_creation_message::Option { option_name: Some(name.clone()), ..Default::default() })
            .collect(),
        selectable_options_count: Some(if max >= options.len() { 0 } else { max as u32 }),
        context_info: (ephemeral > 0)
            .then(|| Box::new(e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() })),
        ..Default::default()
    };
    let mut message = e2e::Message {
        message_context_info: Some(e2e::MessageContextInfo { message_secret: Some(secret), ..Default::default() }),
        ..Default::default()
    };
    if max == 1 {
        message.poll_creation_message_v3 = Some(Box::new(creation));
    } else {
        message.poll_creation_message = Some(Box::new(creation));
    }
    message
}

pub(super) async fn send_poll(inner: &Arc<Inner>, poll: SendPoll) -> Result<Sent, Error> {
    let question = poll.question.trim().to_string();
    if question.is_empty() {
        return Err(bad_param("question", "a poll asks something"));
    }
    if question.chars().count() > QUESTION_MOST {
        return Err(too_long(
            "question",
            QUESTION_MOST,
            format!("a poll's question is {QUESTION_MOST} characters at most"),
        ));
    }
    let options: Vec<String> = poll.options.iter().map(|option| option.trim().to_string()).collect();
    if options.len() < 2 || options.iter().any(String::is_empty) {
        return Err(bad_param("options", "a poll has two options at least, each saying something"));
    }
    if options.len() > OPTIONS_MOST {
        return Err(too_long("options", OPTIONS_MOST, format!("a poll has {OPTIONS_MOST} options at most")));
    }
    if options.iter().any(|option| option.chars().count() > OPTION_MOST) {
        return Err(too_long("option", OPTION_MOST, format!("an option is {OPTION_MOST} characters at most")));
    }
    if options.iter().enumerate().any(|(n, option)| options[..n].contains(option)) {
        return Err(bad_param("options", "each option is named once, since a vote names an option by its name"));
    }
    let max = poll.max.unwrap_or(1) as usize;
    if max == 0 || max > options.len() {
        return Err(bad_param("max", format!("a vote chooses 1 to {} of these options", options.len())));
    }
    check_key(&poll.key)?;
    let _place = composing(inner, &[poll.chat.as_str()]).await;
    let (chat, key) = (poll.chat.clone(), poll.key.clone());
    let (ephemeral, keyed, me) = query(inner, move |db| {
        let ephemeral: Option<u32> = db.prepare_cached(CHAT_TO_SEND)?.query_row([&chat], |r| r.get(0)).optional()?;
        let keyed = key.as_deref().map(|key| keyed(db, key)).transpose()?.flatten();
        Ok((ephemeral, keyed, me(db)?))
    })
    .await?;
    if let Some((chat, id)) = keyed {
        return Ok(answer(inner, chat, id).await);
    }
    let Some(ephemeral) = ephemeral else { return Err(no_chat(&poll.chat)) };
    let secret = secret().map_err(|e| internal("make the poll's secret", e.into()))?;
    let raw = poll_message(&question, &options, max, ephemeral, secret);
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(poll.chat.clone()), from_me: Some(true), id: Some(id), participant: None },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, &me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that poll"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "poll",
        raw: raw.encode_to_vec(),
        key: poll.key.clone(),
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the poll, so it isn't sent").await?;
    enqueued(inner, written).await
}

/// A poll's options by name, in their order, and how many a vote may choose, as jaw keeps them.
fn options_of(db: &rusqlite::Connection, n: i64) -> anyhow::Result<(Vec<String>, usize)> {
    let detail: Option<String> =
        db.prepare_cached("SELECT detail FROM messages WHERE n = ?1")?.query_row([n], |r| r.get(0))?;
    let detail: Value = detail.and_then(|detail| serde_json::from_str(&detail).ok()).unwrap_or_default();
    let names: Vec<String> = db
        .prepare_cached("SELECT name FROM poll_options WHERE message = ?1 ORDER BY position")?
        .query_map([n], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    // A poll converted before votes were kept has its options in its row until the backfill.
    let names = if names.is_empty() {
        let shown = detail["poll"]["options"].as_array().cloned().unwrap_or_default();
        shown.iter().filter_map(|option| option["name"].as_str().map(str::to_string)).collect()
    } else {
        names
    };
    let max = detail["poll"]["max"].as_u64().map_or(1, |max| max as usize).clamp(1, names.len().max(1));
    Ok((names, max))
}

pub(super) async fn vote(inner: &Arc<Inner>, vote: PollVote) -> Result<Value, Error> {
    check_key(&vote.key)?;
    if already(inner, &vote.key).await? {
        return Ok(Value::Object(Default::default()));
    }
    let target = find(inner, vote.chat, vote.message).await?;
    let refused = |why: &str| Err(Error::new(code::NOT_ALLOWED, why));
    if target.kind != "poll" {
        return refused("only a poll can be voted in");
    }
    if target.deleted {
        return refused("a deleted poll can't be voted in");
    }
    let n = target.n;
    let (options, max) = query(inner, move |db| options_of(db, n)).await?;
    let mut chosen: Vec<String> = Vec::new();
    for name in vote.options {
        if !options.contains(&name) {
            return Err(bad_param("options", format!("{name:?} isn't one of the poll's options")));
        }
        if !chosen.contains(&name) {
            chosen.push(name);
        }
    }
    if chosen.len() > max {
        return refused(&if max == 1 {
            "this poll takes one choice, so a vote chooses one option at most".to_string()
        } else {
            format!("this poll takes {max} choices at most")
        });
    }
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let hashes: Vec<Vec<u8>> = chosen.iter().map(|name| option_hash(name)).collect();
    let command = pb::PollVote {
        chat: target.chat.clone(),
        id: id.clone(),
        poll_id: target.id.clone(),
        poll_sender: if target.mine { String::new() } else { target.sender.clone().unwrap_or_default() },
        poll_from_me: target.mine,
        options: hashes.clone(),
        at_ms: at,
    };
    let effect = Amendment::Vote {
        options: hashes.iter().map(|hash| hash.iter().map(|b| format!("{b:02x}")).collect()).collect(),
        id: id.clone(),
    };
    let written = Written::default();
    let step = Step::Act {
        chat: target.chat.clone(),
        target: target.id.clone(),
        id,
        effect,
        at,
        raw: command.encode_to_vec(),
        key: vote.key,
        written: written.clone(),
    };
    enqueue(inner, step, written, "keep the vote").await
}

#[cfg(test)]
mod tests;
