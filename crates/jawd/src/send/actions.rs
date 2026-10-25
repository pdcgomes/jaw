//! Actions of yours on a message: `react`, `edit` and `delete`. Each goes to WhatsApp as a message of
//! its own, with an ID of its own, from an outbox row committed and synced before it answers, after
//! its chat's earlier sends, tried again and refused as a message is; and it's done to the
//! message's row only once WhatsApp has it, while windows show it meanwhile (store::outbox). A
//! deletion for you is done at once, and its outbox row tells your other devices by app state.
//!
//! An action names its message by ID, with its chat if the interface gives it; without one, jawd
//! looks for the ID in each chat, a search of the messages' index each.

use std::sync::Arc;
use std::time::Duration;

use jaw_proto::send::{Delete, DeleteFor, Edit, React};
use jaw_proto::sync::{Notice, Tone};
use jaw_proto::{Error, Push, code};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{DeleteMessageForMeAction, SyncActionValue};
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::outbox::{bad_param, check_key, message_id, queue};
use super::{Inner, TEXT_MOST, durably, internal, now_ms, query};
use crate::convert::{kinds, spans};
use crate::engine::pb;
use crate::store::outbox::{Enqueued, Step, Written, keyed};
use crate::store::{Amendment, Change};

/// A message as an action finds it, by ID in a chat, or in a chat being merged into it.
pub(super) const TARGET_IN: &str =
    "SELECT coalesce(k.id, c.id), c.kind, m.mine, j.jid, m.at, m.kind, m.deleted, m.status,
        m.n, coalesce(k.n, c.n), m.view_once, m.forwarded, m.raw, m.text, m.edited_at IS NOT NULL
    FROM chats c JOIN messages m ON m.chat = c.n AND m.id = ?2 LEFT JOIN person_jids j ON j.n = m.sender
    LEFT JOIN chat_merges cm ON cm.gone = c.n LEFT JOIN chats k ON k.n = cm.keep
    WHERE c.id = ?1";
/// A message as an action finds it by its ID alone, in whichever chats have one: a search of the
/// messages' index for each chat, chats being few next to messages, which CROSS JOIN keeps SQLite
/// from scanning instead.
pub(super) const TARGET_ANYWHERE: &str = "SELECT coalesce(k.id, c.id), c.kind, m.mine, j.jid, m.at, m.kind, m.deleted,
        m.status, m.n, coalesce(k.n, c.n), m.view_once, m.forwarded, m.raw, m.text, m.edited_at IS NOT NULL
    FROM chats c CROSS JOIN messages m ON m.chat = c.n AND m.id = ?1 LEFT JOIN person_jids j ON j.n = m.sender
    LEFT JOIN chat_merges cm ON cm.gone = c.n LEFT JOIN chats k ON k.n = cm.keep
    LIMIT 4";

/// A chat by its JID as the writer finds it: the chat it's being merged into, or itself.
pub(super) const CHAT_KEPT: &str = "SELECT coalesce(k.id, c.id) FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n
    LEFT JOIN chats k ON k.n = m.keep WHERE c.id = ?1";

/// How long after sending it WhatsApp lets you edit your message, and delete it for everyone.
pub(super) const EDITABLE: Duration = Duration::from_secs(15 * 60);
pub(super) const DELETABLE: Duration = Duration::from_secs(60 * 3600);
/// The longest emoji jawd takes for a reaction, in bytes: WhatsApp's longest, a family of four
/// with their skin tones, is 41.
const EMOJI_MOST: usize = 64;

/// A message an action is done to.
#[derive(Clone, Debug)]
pub(super) struct Target {
    /// Its chat's JID, as the writer finds it.
    pub chat: String,
    /// Whether its chat has members, a group's or a community's.
    pub group: bool,
    pub id: String,
    pub mine: bool,
    /// Who sent it, by the JID it came from; `None` for yours.
    pub sender: Option<String>,
    pub at: i64,
    pub kind: String,
    pub deleted: bool,
    pub status: Option<String>,
    /// Its n, and its chat's, as the writer finds it.
    pub n: i64,
    pub chat_n: i64,
    pub view_once: bool,
    /// How many times it had been forwarded when it came.
    pub forwarded: u32,
    /// Its WebMessageInfo, packed, as it came: an edit changes the row's text, not this.
    pub raw: Option<Vec<u8>>,
    pub text: Option<String>,
    pub edited: bool,
}

pub(super) enum Found {
    One(Box<Target>),
    Missing,
    /// The ID is in more than one chat, so the action has to say which.
    Ambiguous,
}

fn target_row(id: &str, r: &rusqlite::Row) -> rusqlite::Result<Target> {
    let kind: String = r.get(1)?;
    Ok(Target {
        chat: r.get(0)?,
        group: matches!(kind.as_str(), "group" | "community" | "announcement"),
        id: id.to_string(),
        mine: r.get(2)?,
        sender: r.get(3)?,
        at: r.get(4)?,
        kind: r.get(5)?,
        deleted: r.get(6)?,
        status: r.get(7)?,
        n: r.get(8)?,
        chat_n: r.get(9)?,
        view_once: r.get(10)?,
        forwarded: r.get::<_, i64>(11)?.clamp(0, i64::from(u32::MAX)) as u32,
        raw: r.get(12)?,
        text: r.get(13)?,
        edited: r.get(14)?,
    })
}

/// Message `id`, in `chat` if it's given, or in whichever chat has it; a chat being merged and the
/// one it goes into are one chat.
pub(super) fn target(db: &Connection, chat: Option<&str>, id: &str) -> anyhow::Result<Found> {
    let mut resolved = None;
    if let Some(chat) = chat {
        if let Some(found) = db.prepare_cached(TARGET_IN)?.query_row([chat, id], |r| target_row(id, r)).optional()? {
            return Ok(Found::One(Box::new(found)));
        }
        let kept: Option<String> = db.prepare_cached(CHAT_KEPT)?.query_row([chat], |r| r.get(0)).optional()?;
        resolved = Some(kept.unwrap_or_else(|| chat.to_string()));
    }
    let mut found: Vec<Target> =
        db.prepare_cached(TARGET_ANYWHERE)?.query_map([id], |r| target_row(id, r))?.collect::<rusqlite::Result<_>>()?;
    if let Some(resolved) = &resolved {
        found.retain(|target| &target.chat == resolved);
    }
    found.dedup_by(|a, b| a.chat == b.chat);
    Ok(match found.len() {
        0 => Found::Missing,
        1 => Found::One(Box::new(found.remove(0))),
        _ => Found::Ambiguous,
    })
}

/// Finds the message an action names, or says why it can't.
pub(super) async fn find(inner: &Inner, chat: Option<String>, id: String) -> Result<Target, Error> {
    let (asked, named) = (id.clone(), chat.clone());
    match query(inner, move |db| target(db, named.as_deref(), &asked)).await? {
        Found::One(target) => Ok(*target),
        Found::Missing => Err(Error::new(
            code::NOT_FOUND,
            match chat {
                Some(chat) => format!("jawd has no message {id} in {chat}"),
                None => format!("jawd has no message {id}"),
            },
        )),
        Found::Ambiguous => Err(bad_param("chat", format!("{id} is a message in more than one chat; say which"))),
    }
}

/// Whether a message of `kind` is someone's, rather than a line about the chat.
fn someones(kind: &str) -> bool {
    !matches!(kind, "system" | "call")
}

/// The key of message `target`, as an action on it names it.
fn key_of(target: &Target) -> MessageKey {
    MessageKey {
        remote_jid: Some(target.chat.clone()),
        from_me: Some(target.mine),
        id: Some(target.id.clone()),
        participant: if target.group && !target.mine { target.sender.clone() } else { None },
    }
}

/// Whether an interface's key named an action already, which is then done.
async fn already(inner: &Inner, key: &Option<String>) -> Result<bool, Error> {
    let Some(key) = key.clone() else { return Ok(false) };
    Ok(query(inner, move |db| keyed(db, &key)).await?.is_some())
}

/// Commits an action's step, synced, and queues its outbox row after its chat's earlier sends.
async fn enqueue(inner: &Arc<Inner>, step: Step, written: Written, what: &str) -> Result<Value, Error> {
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], what).await?;
    match written.take() {
        Some(Enqueued::Queued { outbox, chat, .. }) => {
            if let Some(outbox) = outbox {
                queue(inner, outbox, chat, false);
            }
            Ok(Value::Object(Default::default()))
        }
        Some(Enqueued::Full { waiting, failed }) => Err(full(waiting, failed)),
        Some(Enqueued::Gone) => Err(Error::new(code::NOT_FOUND, "the message went before jawd could do that")),
        Some(Enqueued::Many(_)) | None => {
            Err(Error::new(code::INTERNAL, format!("jawd kept nothing, so it couldn't {what}")))
        }
    }
}

/// The protocol's error for an outbox holding as many as it may.
pub(super) fn full(waiting: u64, failed: u64) -> Error {
    let mut error = Error::new(
        code::LIMIT,
        format!(
            "{waiting} messages and actions of yours are waiting to send, {failed} of them failed; \
            jawd takes more once some have gone or been retried"
        ),
    );
    error.data = Some(serde_json::json!({ "limit": "outbox", "waiting": waiting, "failed": failed }));
    error
}

/// Queues an action on `target` whose effect is `effect` and whose message to WhatsApp is `raw`.
async fn act(
    inner: &Arc<Inner>,
    target: &Target,
    effect: Amendment,
    at: i64,
    raw: e2e::Message,
    key: Option<String>,
    what: &str,
) -> Result<Value, Error> {
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let written = Written::default();
    let step = Step::Act {
        chat: target.chat.clone(),
        target: target.id.clone(),
        id,
        effect,
        at,
        raw: raw.encode_to_vec(),
        key,
        written: written.clone(),
    };
    enqueue(inner, step, written, what).await
}

pub(super) async fn react(inner: &Arc<Inner>, react: React) -> Result<Value, Error> {
    check_key(&react.key)?;
    if react.emoji.len() > EMOJI_MOST || react.emoji.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(bad_param("emoji", "a reaction is one emoji"));
    }
    if already(inner, &react.key).await? {
        return Ok(Value::Object(Default::default()));
    }
    let target = find(inner, react.chat, react.message).await?;
    if target.deleted {
        return Err(Error::new(code::NOT_ALLOWED, "a deleted message can't be reacted to"));
    }
    if !someones(&target.kind) {
        return Err(Error::new(code::NOT_ALLOWED, "a line about the chat can't be reacted to"));
    }
    let at = now_ms();
    let raw = e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(key_of(&target)),
            text: Some(react.emoji.clone()),
            sender_timestamp_ms: Some(at),
            ..Default::default()
        }),
        ..Default::default()
    };
    let effect = Amendment::React { emoji: react.emoji };
    act(inner, &target, effect, at, raw, react.key, "keep the reaction").await
}

pub(super) async fn edit(inner: &Arc<Inner>, edit: Edit) -> Result<Value, Error> {
    check_key(&edit.key)?;
    if edit.text.trim().is_empty() {
        return Err(bad_param("text", "there's nothing to change it to; delete it instead"));
    }
    if edit.text.chars().count() > TEXT_MOST {
        return Err(Error::new(code::LIMIT, format!("a message can be {TEXT_MOST} characters at most")));
    }
    if already(inner, &edit.key).await? {
        return Ok(Value::Object(Default::default()));
    }
    let target = find(inner, edit.chat, edit.message).await?;
    let refused = |why: &str| Err(Error::new(code::NOT_ALLOWED, why));
    if !target.mine {
        return refused("only your own messages can be edited");
    }
    if target.deleted {
        return refused("a deleted message can't be edited");
    }
    if target.kind != "text" {
        return refused("only a text message can be edited so far");
    }
    if target.status.as_deref() == Some("failed") {
        return refused("it didn't go, so there's nothing to edit; send it again first");
    }
    let at = now_ms();
    if at - target.at > EDITABLE.as_millis() as i64 {
        return refused("a message can be edited for 15 minutes after it's sent");
    }
    let body = kinds::body(&e2e::Message { conversation: Some(edit.text.clone()), ..Default::default() }, &[]);
    let effect = Amendment::Edit {
        spans: body.text.as_deref().and_then(|_| spans::json(&body.spans)),
        text: body.text,
        summary: body.summary,
    };
    let protocol = e2e::ProtocolMessage {
        key: Some(MessageKey {
            remote_jid: Some(target.chat.clone()),
            from_me: Some(true),
            id: Some(target.id.clone()),
            participant: None,
        }),
        r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
        edited_message: Some(Box::new(e2e::Message { conversation: Some(edit.text), ..Default::default() })),
        timestamp_ms: Some(at),
        ..Default::default()
    };
    let inner_message = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    let raw = e2e::Message {
        edited_message: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(inner_message)) })),
        ..Default::default()
    };
    act(inner, &target, effect, at, raw, edit.key, "keep the edit").await
}

pub(super) async fn delete(inner: &Arc<Inner>, delete: Delete) -> Result<Value, Error> {
    check_key(&delete.key)?;
    if already(inner, &delete.key).await? {
        return Ok(Value::Object(Default::default()));
    }
    let target = find(inner, delete.chat, delete.message).await?;
    match delete.for_ {
        DeleteFor::Me => delete_for_me(inner, &target, delete.key).await,
        DeleteFor::Everyone => delete_for_everyone(inner, &target, delete.key).await,
    }
}

async fn delete_for_everyone(inner: &Arc<Inner>, target: &Target, key: Option<String>) -> Result<Value, Error> {
    let refused = |why: &str| Err(Error::new(code::NOT_ALLOWED, why));
    if target.deleted {
        return Ok(Value::Object(Default::default()));
    }
    if !someones(&target.kind) {
        return refused("a line about the chat can only be deleted for you");
    }
    if !target.mine {
        return refused("only your own messages can be deleted for everyone");
    }
    if target.status.as_deref() == Some("failed") {
        return refused("it didn't go, so delete it for you");
    }
    let at = now_ms();
    if at - target.at > DELETABLE.as_millis() as i64 {
        return refused("a message can be deleted for everyone for 60 hours after it's sent");
    }
    let protocol = e2e::ProtocolMessage {
        key: Some(MessageKey {
            remote_jid: Some(target.chat.clone()),
            from_me: Some(true),
            id: Some(target.id.clone()),
            participant: None,
        }),
        r#type: Some(e2e::protocol_message::Type::Revoke as i32),
        ..Default::default()
    };
    let raw = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    act(inner, target, Amendment::Revoke, at, raw, key, "keep the deletion").await
}

/// The app-state mutation that deletes a message for you on your other devices, as WhatsApp's own
/// clients send it.
pub(super) fn deleted_for_me(target: &Target, now: i64) -> pb::AppStatePatch {
    let participant = match (&target.sender, target.group && !target.mine) {
        (Some(sender), true) => sender.clone(),
        _ => "0".into(),
    };
    let index = vec![
        "deleteMessageForMe".into(),
        target.chat.clone(),
        target.id.clone(),
        if target.mine { "1" } else { "0" }.into(),
        participant,
    ];
    let value = SyncActionValue {
        timestamp: Some(now),
        delete_message_for_me_action: Some(DeleteMessageForMeAction {
            delete_media: Some(false),
            message_timestamp: Some(target.at.div_euclid(1000)),
        }),
        ..Default::default()
    };
    pb::AppStatePatch { name: "regular_high".into(), index, version: 3, value: value.encode_to_vec() }
}

async fn delete_for_me(inner: &Arc<Inner>, target: &Target, key: Option<String>) -> Result<Value, Error> {
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let written = Written::default();
    let step = Step::DeleteForMe {
        chat: target.chat.clone(),
        target: target.id.clone(),
        id,
        raw: deleted_for_me(target, now_ms()).encode_to_vec(),
        key,
        written: written.clone(),
    };
    enqueue(inner, step, written, "delete it for you").await
}

/// Says to every interface that WhatsApp refused an action, which the row it showed on is back as
/// it was without; a message refused shows failed on its own row instead.
pub(super) fn refused(inner: &Inner, kind: &str, why: &str) {
    let what = match kind {
        "reaction" => "your reaction",
        "edit" => "your edit",
        "revoke" => "deleting your message for everyone",
        "app_state" => "telling your other devices you deleted a message",
        _ => return,
    };
    let notice = Notice { text: format!("WhatsApp refused {what}: {why}"), tone: Tone::Warning };
    let params = serde_json::to_value(notice).expect("jawd's own types serialize");
    let _ = inner.pushes.send(Push { method: "notice".into(), params });
}
