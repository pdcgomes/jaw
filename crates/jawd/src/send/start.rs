//! `chat.start`: the one-to-one chat with a phone number or a person, found or made. A number is
//! checked with WhatsApp first, which also gives the person's LID, so they're one person and one
//! chat whichever JID their messages come by; a person's chat is found through every JID they're
//! known by before one is made.

use std::sync::Arc;

use jaw_proto::send::{ChatStart, ChatStarted};
use jaw_proto::{Error, code};
use rusqlite::OptionalExtension;

use super::{Inner, durably, engine, engine_error, query};
use crate::engine::pb;
use crate::feed;
use crate::store::{Change, Chat, Person, read};

pub(super) const DIRECT_CHAT_BY_JID: &str = "SELECT c.n FROM person_jids j JOIN chats c ON c.person = j.person
    AND c.kind = 'direct' WHERE j.jid = ?1 ORDER BY c.n LIMIT 1";
pub(super) const DIRECT_CHAT_BY_PERSON: &str =
    "SELECT n FROM chats WHERE person = ?1 AND kind = 'direct' ORDER BY n LIMIT 1";
pub(super) const PERSON_JIDS: &str = "SELECT jid FROM person_jids WHERE person = ?1 ORDER BY n";

pub(super) async fn start(inner: &Arc<Inner>, start: ChatStart) -> Result<ChatStarted, Error> {
    let (jid, mut changes) = match (start.phone, start.person) {
        (Some(phone), None) => by_phone(inner, &phone).await?,
        (None, Some(person)) => match by_person(inner, &person).await? {
            Found::Chat(n) => return started(inner, n).await,
            Found::Person(jid) => (jid, Vec::new()),
        },
        _ => return Err(Error::new(code::INVALID_PARAMS, "chat.start takes a phone or a person, and one of them")),
    };
    changes.push(Change::Chat(Chat { id: jid.clone(), kind: Some("direct"), ..Chat::default() }));
    durably(inner, changes, "make the chat").await?;
    match query(inner, move |db| direct_chat(db, &jid)).await? {
        Some(n) => started(inner, n).await,
        None => Err(Error::new(code::INTERNAL, "jawd made the chat and then couldn't find it")),
    }
}

enum Found {
    Chat(i64),
    Person(String),
}

/// The direct chat with whoever has `jid`, under that JID or any other of theirs.
fn direct_chat(db: &rusqlite::Connection, jid: &str) -> anyhow::Result<Option<i64>> {
    Ok(db.prepare_cached(DIRECT_CHAT_BY_JID)?.query_row([jid], |r| r.get(0)).optional()?)
}

async fn started(inner: &Inner, n: i64) -> Result<ChatStarted, Error> {
    match query(inner, move |db| read::chat(db, n)).await? {
        Some(row) => Ok(ChatStarted { chat: feed::chat_row(row) }),
        None => Err(Error::new(code::NOT_FOUND, "the chat went before jawd could answer")),
    }
}

/// The person's chat if they have one, or the JID to make it with: their phone number's if jawd
/// knows it.
async fn by_person(inner: &Inner, person: &str) -> Result<Found, Error> {
    let n: i64 = person.strip_prefix("person:").and_then(|n| n.parse().ok()).ok_or_else(|| {
        Error::new(code::INVALID_PARAMS, format!("{person} isn't a person, which is person: and a number"))
    })?;
    let (chat, jids) = query(inner, move |db| {
        let chat: Option<i64> = db.prepare_cached(DIRECT_CHAT_BY_PERSON)?.query_row([n], |r| r.get(0)).optional()?;
        let jids: Vec<String> =
            db.prepare_cached(PERSON_JIDS)?.query_map([n], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok((chat, jids))
    })
    .await?;
    if let Some(chat) = chat {
        return Ok(Found::Chat(chat));
    }
    let jid = jids.iter().find(|jid| jid.ends_with("@s.whatsapp.net")).or(jids.first());
    match jid {
        Some(jid) => Ok(Found::Person(jid.clone())),
        None => Err(Error::new(code::NOT_FOUND, format!("jawd doesn't know {person}"))),
    }
}

/// The phone number's JID once WhatsApp says it's on WhatsApp, and the person to keep under it and
/// their LID.
async fn by_phone(inner: &Inner, phone: &str) -> Result<(String, Vec<Change>), Error> {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    let typed_well = phone.chars().all(|c| c.is_ascii_digit() || " +-().".contains(c));
    if !typed_well || !(6..=15).contains(&digits.len()) {
        return Err(Error::new(code::INVALID_PARAMS, format!("{phone} isn't a phone number in international form")));
    }
    let asked = pb::OnWhatsApp { phones: vec![digits.clone()] };
    let answer = engine(inner, pb::command::Kind::OnWhatsapp(asked)).await.map_err(engine_error)?;
    let pb::reply::Kind::OnWhatsappList(list) = answer else {
        return Err(Error::new(code::INTERNAL, "the engine didn't say who's on WhatsApp"));
    };
    let user = list.users.into_iter().find(|user| user.query == digits && user.is_in);
    let Some(user) = user else {
        return Err(Error::new(code::NOT_FOUND, format!("+{digits} isn't on WhatsApp")));
    };
    let pn = if user.phone.ends_with("@s.whatsapp.net") { user.phone } else { format!("{digits}@s.whatsapp.net") };
    let mut jids = vec![pn.clone()];
    jids.extend(Some(user.jid).filter(|jid| jid.ends_with("@lid")));
    let verified = Some(user.verified_name).filter(|name| !name.is_empty());
    let person = Person { jids, verified_name: verified, ..Person::default() };
    Ok((pn, vec![Change::Person(person)]))
}
