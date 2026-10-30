//! `chat.disappearing`: a chat's disappearing timer set from jaw (see "Disappearing and view-once
//! messages" in the protocol). The chat shows it at once, in a synced commit before the answer, and
//! the engine sets it on WhatsApp from the outbox, after the chat's earlier sends: in a one-to-one
//! chat as the setting message the phones send, with an ID jawd makes as it does a message's, so the
//! line written with it is that message and a copy history brings finds it; and in a group as the
//! group's setting, whose line comes when WhatsApp tells its members. If WhatsApp refuses it, it
//! goes back to what it was (`store::outbox`).

use std::sync::Arc;

use jaw_proto::disappearing::{ChatDisappearing, TIMERS};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

use super::{Inner, durably, internal, no_chat, now_ms, outbox, query};
use crate::convert;
use crate::convert::kinds::ephemeral_sentence;
use crate::convert::people::TIMER_LINE;
use crate::engine::pb;
use crate::store::outbox::{Enqueued, Step, Written};
use crate::store::{Change, Message};

/// The chat as the writer finds it, its kind, its timer and when that was set.
const CHAT: &str = "SELECT coalesce(k.id, c.id), coalesce(k.kind, c.kind), coalesce(k.ephemeral, c.ephemeral),
        coalesce(k.ephemeral_at, c.ephemeral_at)
    FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n LEFT JOIN chats k ON k.n = m.keep WHERE c.id = ?1";

pub(super) async fn set(inner: &Arc<Inner>, asked: ChatDisappearing) -> Result<Value, Error> {
    if !TIMERS.contains(&asked.seconds) {
        let mut error =
            Error::new(code::INVALID_PARAMS, "a timer is off, 24 hours, 7 days or 90 days, as the phones offer");
        error.data = Some(json!({ "param": "seconds", "allowed": TIMERS }));
        return Err(error);
    }
    let chat = asked.chat.clone();
    let found: Option<(String, String, i64, Option<i64>)> = query(inner, move |db| {
        Ok(db
            .prepare_cached(CHAT)?
            .query_row([&chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .optional()?)
    })
    .await?;
    let Some((chat, kind, ephemeral, set_at)) = found else { return Err(no_chat(&asked.chat)) };
    let direct = kind == "direct" && convert::is_person(&chat);
    if !direct && kind != "group" {
        return Err(Error::new(code::NOT_ALLOWED, "only a one-to-one chat or a group has a disappearing timer to set"));
    }
    if ephemeral == i64::from(asked.seconds) {
        return Ok(json!({}));
    }
    // In whole seconds, as the setting message carries its time, and after the chat's own setting,
    // so it's the newest here whatever clock set that.
    let at = (now_ms() / 1000 * 1000).max(set_at.map_or(0, |set_at| set_at / 1000 * 1000 + 1000));
    let id = if direct {
        outbox::message_id().map_err(|e| internal("make the setting's ID", e.into()))?
    } else {
        format!("{TIMER_LINE}{at}")
    };
    let raw =
        pb::Disappearing { chat: chat.clone(), seconds: asked.seconds, at_ms: at, id: id.clone() }.encode_to_vec();
    // A group's line comes as WhatsApp tells its members; a one-to-one chat's is this one.
    let line = direct.then(|| {
        let sentence = ephemeral_sentence(asked.seconds);
        Box::new(Message {
            chat: chat.clone(),
            id: id.clone(),
            at,
            mine: true,
            kind: "system",
            summary: sentence.clone(),
            detail: Some(json!({ "system": sentence }).to_string()),
            live: true,
            ..Message::default()
        })
    });
    let written = Written::default();
    let step = Step::Disappearing { chat, seconds: asked.seconds, at, id, raw, line, written: written.clone() };
    durably(inner, vec![Change::Outbox(step)], "keep the disappearing timer").await?;
    match written.take() {
        Some(Enqueued::Queued { outbox: Some(row), chat, .. }) => {
            outbox::queue(inner, row, chat, false);
            Ok(json!({}))
        }
        Some(Enqueued::Full { waiting, failed }) => Err(super::actions::full(waiting, failed)),
        Some(Enqueued::Gone) => Err(no_chat(&asked.chat)),
        None => Err(Error::new(code::NOT_ALLOWED, "the chat's timer was set again as jawd set it; set it again")),
        Some(_) => Err(Error::new(code::INTERNAL, "jawd kept nothing of the timer, so it isn't set")),
    }
}
