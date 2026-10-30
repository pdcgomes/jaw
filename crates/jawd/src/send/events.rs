//! Events of yours: `send.event` sends one as the phones make it, with a fresh secret of its own,
//! which every answer to it and every edit of it is sealed with, as a message of yours as
//! `send.text`'s is: answered once it and its outbox row are synced, keyed, queued in its chat's
//! order and shown at once, as the converter reads what WhatsApp would bring. Its start and end go
//! to the second, as WhatsApp keeps them. A call link isn't made: the phones ask WhatsApp's server
//! for one, which whatsmeow can't yet.

use std::sync::Arc;

use jaw_proto::send::{Place, SendEvent, Sent};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::outbox::{CHAT_TO_SEND, answer, bad_param, check_key, enqueued, me, message_id};
use super::places::{called, expiring, pinned};
use super::poll::{secret, too_long};
use super::{Inner, composing, durably, internal, no_chat, now_ms, query};
use crate::convert;
use crate::store::Change;
use crate::store::outbox::{Step, Written, keyed};

/// The longest name and description of an event of yours, in characters.
const NAME_MOST: usize = 255;
const DESCRIPTION_MOST: usize = 2_048;

/// Where an event is, as WhatsApp keeps it: a pin, or a place's name alone.
fn place_of(place: Option<&Place>) -> Result<Option<e2e::LocationMessage>, Error> {
    Ok(match place {
        None => None,
        Some(Place::Named(name)) => {
            called("place", Some(name))?.map(|name| e2e::LocationMessage { name: Some(name), ..Default::default() })
        }
        Some(Place::At { lat, lon, name, address }) => {
            Some(pinned("place.", (*lat, *lon), name.as_deref(), address.as_deref())?)
        }
    })
}

pub(super) async fn send_event(inner: &Arc<Inner>, event: SendEvent) -> Result<Sent, Error> {
    let name = event.name.trim().to_string();
    if name.is_empty() {
        return Err(bad_param("name", "an event is called something"));
    }
    if name.chars().count() > NAME_MOST {
        return Err(too_long("name", NAME_MOST, format!("an event's name is {NAME_MOST} characters at most")));
    }
    let description = event.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    if description.as_ref().is_some_and(|d| d.chars().count() > DESCRIPTION_MOST) {
        let why = format!("an event's description is {DESCRIPTION_MOST} characters at most");
        return Err(too_long("description", DESCRIPTION_MOST, why));
    }
    if event.start <= 0 {
        return Err(bad_param("start", "an event starts at a time, in milliseconds since the epoch"));
    }
    if event.end.is_some_and(|end| end < event.start) {
        return Err(bad_param("end", "an event ends when it starts or after"));
    }
    let place = place_of(event.place.as_ref())?;
    if event.call == Some(true) {
        return Err(Error::new(
            code::NOT_ALLOWED,
            "jawd can't add a WhatsApp call link yet: the phones ask WhatsApp for one, and jaw's engine can't",
        ));
    }
    check_key(&event.key)?;
    let chat = event.chat.as_str();
    let _place = composing(inner, &[chat]).await;
    let (asked, wanted) = (chat.to_string(), event.key.clone());
    let (ephemeral, keyed, me) = query(inner, move |db| {
        let ephemeral: Option<u32> = db.prepare_cached(CHAT_TO_SEND)?.query_row([&asked], |r| r.get(0)).optional()?;
        let keyed = wanted.as_deref().map(|key| keyed(db, key)).transpose()?.flatten();
        Ok((ephemeral, keyed, me(db)?))
    })
    .await?;
    if let Some((chat, id)) = keyed {
        return Ok(answer(inner, chat, id).await);
    }
    let Some(ephemeral) = ephemeral else { return Err(no_chat(chat)) };
    let secret = secret().map_err(|e| internal("make the event's secret", e.into()))?;
    let raw = e2e::Message {
        event_message: Some(Box::new(e2e::EventMessage {
            name: Some(name),
            description,
            location: place.map(Box::new),
            start_time: Some(event.start / 1000),
            end_time: event.end.map(|end| end / 1000),
            context_info: expiring(ephemeral),
            ..Default::default()
        })),
        message_context_info: Some(e2e::MessageContextInfo { message_secret: Some(secret), ..Default::default() }),
        ..Default::default()
    };
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(chat.to_string()), from_me: Some(true), id: Some(id), participant: None },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, &me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that event"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "event",
        raw: raw.encode_to_vec(),
        key: event.key,
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the event, so it isn't sent").await?;
    enqueued(inner, written).await
}

#[cfg(test)]
mod tests;
