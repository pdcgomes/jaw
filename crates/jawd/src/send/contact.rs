//! Contact cards of yours: `send.contact` sends one card or several in one message, each a name and
//! a vCard, as a message of yours as `send.text` sends one: answered once it and its outbox row are
//! synced, keyed, queued in its chat's order, carrying the chat's disappearing timer, and shown at
//! once as the converter reads what WhatsApp would bring back. One card goes as WhatsApp's
//! ContactMessage, several as a ContactsArrayMessage, as the phones send them.

use std::sync::Arc;

use jaw_proto::send::{Card, SendContact, Sent};
use jaw_proto::{Error, code, vcard};
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

/// The most cards one message shares, the longest name a card goes by, in characters, and the most
/// bytes its cards' vCards take together.
const CARDS_MOST: usize = 64;
const NAME_MOST: usize = 256;
const VCARDS_MOST: usize = 256 * 1024;

/// The protocol's error for something past a limit, saying which and how far it goes.
fn past(what: &str, most: usize, message: String) -> Error {
    let mut error = Error::new(code::LIMIT, message);
    error.data = Some(serde_json::json!({ "limit": what, "most": most }));
    error
}

/// The cards as they're sent, each name trimmed, or why they can't be.
fn checked(cards: Vec<Card>) -> Result<Vec<Card>, Error> {
    if cards.is_empty() {
        return Err(bad_param("cards", "a contact message shares one card at least"));
    }
    if cards.len() > CARDS_MOST {
        return Err(past("cards", CARDS_MOST, format!("one message shares {CARDS_MOST} cards at most")));
    }
    if cards.iter().map(|card| card.vcard.len()).sum::<usize>() > VCARDS_MOST {
        let most = format!("a message's vCards take {} KiB at most together", VCARDS_MOST / 1024);
        return Err(past("vcards", VCARDS_MOST, most));
    }
    let mut checked = Vec::with_capacity(cards.len());
    for card in cards {
        let name = card.name.trim().to_string();
        if name.is_empty() {
            return Err(bad_param("cards", "each card has the name it goes by"));
        }
        if name.chars().count() > NAME_MOST {
            return Err(past("name", NAME_MOST, format!("a card's name is {NAME_MOST} characters at most")));
        }
        if !vcard::is_one(&card.vcard) {
            return Err(bad_param("cards", "each card's vcard is one vCard, from BEGIN:VCARD to END:VCARD"));
        }
        checked.push(Card { name, vcard: card.vcard });
    }
    Ok(checked)
}

/// A message of `cards`, with the chat's disappearing timer when it's on: one card as a
/// ContactMessage, several as a ContactsArrayMessage named for how many there are.
fn contact_message(cards: Vec<Card>, ephemeral: u32) -> e2e::Message {
    let timer =
        (ephemeral > 0).then(|| Box::new(e2e::ContextInfo { expiration: Some(ephemeral), ..Default::default() }));
    let mut cards: Vec<e2e::ContactMessage> = cards
        .into_iter()
        .map(|card| e2e::ContactMessage {
            display_name: Some(card.name),
            vcard: Some(card.vcard),
            ..Default::default()
        })
        .collect();
    if cards.len() == 1 {
        let card = e2e::ContactMessage { context_info: timer, ..cards.remove(0) };
        return e2e::Message { contact_message: Some(Box::new(card)), ..Default::default() };
    }
    let array = e2e::ContactsArrayMessage {
        display_name: Some(format!("{} contacts", cards.len())),
        contacts: cards,
        context_info: timer,
    };
    e2e::Message { contacts_array_message: Some(Box::new(array)), ..Default::default() }
}

pub(super) async fn send_contact(inner: &Arc<Inner>, send: SendContact) -> Result<Sent, Error> {
    let cards = checked(send.cards)?;
    check_key(&send.key)?;
    let _place = composing(inner, &[send.chat.as_str()]).await;
    let (chat, key) = (send.chat.clone(), send.key.clone());
    let (ephemeral, keyed, me) = query(inner, move |db| {
        let ephemeral: Option<u32> = db.prepare_cached(CHAT_TO_SEND)?.query_row([&chat], |r| r.get(0)).optional()?;
        let keyed = key.as_deref().map(|key| keyed(db, key)).transpose()?.flatten();
        Ok((ephemeral, keyed, me(db)?))
    })
    .await?;
    if let Some((chat, id)) = keyed {
        return Ok(answer(inner, chat, id).await);
    }
    let Some(ephemeral) = ephemeral else { return Err(no_chat(&send.chat)) };
    let raw = contact_message(cards, ephemeral);
    let id = message_id().map_err(|e| internal("make a message ID", e.into()))?;
    let at = now_ms();
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(send.chat.clone()), from_me: Some(true), id: Some(id), participant: None },
        message: Some(raw.clone()),
        message_timestamp: Some((at / 1000).max(0) as u64),
        status: Some(web_message_info::Status::Pending as i32),
        ..Default::default()
    };
    let Some(Change::Message(mut message)) =
        convert::message(&web, &me, true).into_iter().find(|c| matches!(c, Change::Message(_)))
    else {
        return Err(Error::new(code::INTERNAL, "jawd couldn't make a message of those cards"));
    };
    message.at = at;
    message.expires_at = None;
    let written = Written::default();
    let step = Step::Enqueue {
        message: Box::new(message),
        kind: "contact",
        raw: raw.encode_to_vec(),
        key: send.key,
        written: written.clone(),
    };
    let _order = inner.enqueue.lock().await;
    durably(inner, vec![Change::Outbox(step)], "keep the cards, so they aren't sent").await?;
    enqueued(inner, written).await
}

#[cfg(test)]
mod tests;
