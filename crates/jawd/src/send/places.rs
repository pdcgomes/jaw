//! Places of yours: `send.location` sends a pin, with the place's name and address when it's one,
//! as a message of yours as `send.text`'s is: answered once it and its outbox row are synced, keyed,
//! queued in its chat's order and shown at once, as the converter reads what WhatsApp would bring.
//! Live locations aren't sent.

use std::sync::Arc;

use jaw_proto::send::{SendLocation, Sent};
use jaw_proto::{Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info};

use super::outbox::{CHAT_TO_SEND, answer, bad_param, check_key, enqueued, me, message_id};
use super::{Inner, composing, durably, internal, no_chat, now_ms, query};
use crate::convert;
use crate::store::Change;
use crate::store::outbox::{Step, Written, keyed};

/// The longest name and address a place of yours has, in characters.
const PLACE_MOST: usize = 1_024;

/// What a place is called, unless it's only spaces.
fn called(what: &str, said: Option<&str>) -> Result<Option<String>, Error> {
    let said = said.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    if said.as_ref().is_some_and(|s| s.chars().count() > PLACE_MOST) {
        let mut error = Error::new(code::LIMIT, format!("a location's {what} is {PLACE_MOST} characters at most"));
        error.data = Some(serde_json::json!({ "limit": what, "most": PLACE_MOST }));
        return Err(error);
    }
    Ok(said)
}

/// The chat's disappearing timer, as the message carries it, when it's on.
fn expiring(ephemeral: u32) -> Option<Box<e2e::ContextInfo>> {
    (ephemeral > 0).then(|| Box::new(e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() }))
}

pub(super) async fn send_location(inner: &Arc<Inner>, location: SendLocation) -> Result<Sent, Error> {
    let degrees = |what: &str, at: f64, most: f64| {
        if at.is_finite() && (-most..=most).contains(&at) {
            Ok(at)
        } else {
            Err(bad_param(what, format!("{what} is from -{most} to {most} degrees")))
        }
    };
    let (lat, lon) = (degrees("lat", location.lat, 90.0)?, degrees("lon", location.lon, 180.0)?);
    let name = called("name", location.name.as_deref())?;
    let address = called("address", location.address.as_deref())?;
    check_key(&location.key)?;
    let chat = location.chat.as_str();
    let _place = composing(inner, &[chat]).await;
    let (asked, wanted) = (chat.to_string(), location.key.clone());
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
    let raw = e2e::Message {
        location_message: Some(Box::new(e2e::LocationMessage {
            degrees_latitude: Some(lat),
            degrees_longitude: Some(lon),
            name,
            address,
            context_info: expiring(ephemeral),
            ..Default::default()
        })),
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
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of that location"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "location",
        raw: raw.encode_to_vec(),
        key: location.key,
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the location, so it isn't sent").await?;
    enqueued(inner, written).await
}

#[cfg(test)]
mod tests;
