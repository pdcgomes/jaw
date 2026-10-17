//! `chat.info` and `message.info`: a chat's members, for completing `@` names, and how far a message
//! of yours has got with each recipient. A message's list comes from the receipts each recipient
//! sent, which last as long as the message, never from `recipients`, which go once it has got as
//! far as it can.

use std::collections::BTreeMap;

use jaw_proto::send::{ChatInfo, ChatInfoResult, Info, Member, MessageInfo, Recipient};
use jaw_proto::{Error, code};
use rusqlite::{Connection, OptionalExtension};

use super::actions::{Target, find};
use super::outbox::me;
use super::{Inner, no_chat, query};
use crate::store::read::shown;

pub(super) const CHAT: &str = "SELECT n, kind, person, community, ephemeral FROM chats WHERE id = ?1";
pub(super) const GROUP_MEMBERS: &str =
    "SELECT j.person, j.jid, g.role FROM group_members g JOIN person_jids j ON j.n = g.member WHERE g.chat = ?1";
pub(super) const PERSON: &str = "SELECT name, push_name, verified_name FROM people WHERE n = ?1";
pub(super) const PHONE: &str =
    "SELECT jid FROM person_jids WHERE person = ?1 AND jid LIKE '%@s.whatsapp.net' ORDER BY n LIMIT 1";
pub(super) const PERSON_OF: &str = "SELECT person FROM person_jids WHERE jid = ?1";

/// A person as a chat's member, as `chat.info` gives them.
fn member(db: &Connection, person: i64, jid: Option<&str>, admin: bool) -> anyhow::Result<Option<Member>> {
    let names: Option<(Option<String>, Option<String>, Option<String>)> =
        db.prepare_cached(PERSON)?.query_row([person], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((name, push_name, verified)) = names else { return Ok(None) };
    let phone: Option<String> = db.prepare_cached(PHONE)?.query_row([person], |r| r.get(0)).optional()?;
    let phone = phone.or_else(|| jid.filter(|jid| jid.ends_with("@s.whatsapp.net")).map(str::to_string));
    Ok(Some(Member {
        person: format!("person:{person}"),
        name: shown(name, push_name, verified, phone.as_deref().or(jid)),
        phone: phone.and_then(|jid| jid.strip_suffix("@s.whatsapp.net").map(|user| format!("+{user}"))),
        admin,
    }))
}

/// A chat's members: a group's, each once however many JIDs they're in it by, and for a one-to-one
/// chat the other person and you, by name.
fn members(db: &Connection, chat: &str) -> anyhow::Result<Option<ChatInfoResult>> {
    type Found = (i64, String, Option<i64>, Option<String>, u32);
    let found: Option<Found> = db
        .prepare_cached(CHAT)?
        .query_row([chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .optional()?;
    let Some((n, kind, person, community, ephemeral)) = found else { return Ok(None) };
    let mut by_person: BTreeMap<i64, Member> = BTreeMap::new();
    if kind == "direct" {
        let mut people: Vec<(i64, Option<String>)> = person.map(|p| (p, None)).into_iter().collect();
        for jid in me(db)?.jids {
            let mine: Option<i64> = db.prepare_cached(PERSON_OF)?.query_row([&jid], |r| r.get(0)).optional()?;
            people.extend(mine.map(|p| (p, Some(jid))));
        }
        for (person, jid) in people {
            if let Some(member) = member(db, person, jid.as_deref(), false)? {
                by_person.entry(person).or_insert(member);
            }
        }
    } else {
        let rows: Vec<(i64, String, String)> = db
            .prepare_cached(GROUP_MEMBERS)?
            .query_map([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (person, jid, role) in rows {
            let admin = role != "member";
            match by_person.get_mut(&person) {
                Some(member) => member.admin |= admin,
                None => {
                    if let Some(member) = member(db, person, Some(&jid), admin)? {
                        by_person.insert(person, member);
                    }
                }
            }
        }
    }
    let mut members: Vec<Member> = by_person.into_values().collect();
    members.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.person.cmp(&b.person)));
    Ok(Some(ChatInfoResult { chat: chat.to_string(), kind, members, community, ephemeral }))
}

/// Each recipient's receipts for a message: who, by person, and when it was delivered to them, read
/// and played, by each of their JIDs.
pub(super) const RECEIPTS: &str = "SELECT j.person, r.delivered_at, r.read_at, r.played_at FROM receipts r
    JOIN person_jids j ON j.n = r.recipient WHERE r.message = ?1";
/// Who a message of yours in a group went to, while sending still keeps it: until it has got as far
/// as it can, when everyone it counted has a receipt.
pub(super) const SENT_TO: &str =
    "SELECT j.person FROM recipients s JOIN person_jids j ON j.n = s.member WHERE s.message = ?1";
pub(super) const DIRECT_PERSON: &str = "SELECT person FROM chats WHERE n = ?1";

/// When a message was delivered to someone, read and played, each if it was.
type Times = (Option<i64>, Option<i64>, Option<i64>);

/// The earliest of two times either of which may not be known.
fn earliest(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// How far a message has got with each of its recipients: for one of yours, everyone with a receipt
/// for it, each by person whatever JID their receipts came by, and, while it's still on its way to
/// some, those sending kept it went to, or in a one-to-one chat the other person; never you.
fn recipients(db: &Connection, target: &Target) -> anyhow::Result<Info> {
    if !target.mine {
        return Ok(Info { message: target.id.clone(), sent: Some(target.at), recipients: Vec::new() });
    }
    let went = matches!(target.status.as_deref(), Some("sent" | "delivered" | "read" | "played"));
    let mut times: BTreeMap<i64, Times> = BTreeMap::new();
    let rows: Vec<(i64, Times)> = db
        .prepare_cached(RECEIPTS)?
        .query_map([target.n], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
        .collect::<rusqlite::Result<_>>()?;
    for (person, (delivered, read, played)) in rows {
        let had = times.entry(person).or_default();
        *had = (earliest(had.0, delivered), earliest(had.1, read), earliest(had.2, played));
    }
    let others: Vec<i64> = if target.group {
        db.prepare_cached(SENT_TO)?.query_map([target.n], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    } else {
        db.prepare_cached(DIRECT_PERSON)?
            .query_row([target.chat_n], |r| r.get::<_, Option<i64>>(0))?
            .into_iter()
            .collect()
    };
    for person in others {
        times.entry(person).or_default();
    }
    for jid in me(db)?.jids {
        if let Some(mine) = db.prepare_cached(PERSON_OF)?.query_row([&jid], |r| r.get::<_, i64>(0)).optional()? {
            times.remove(&mine);
        }
    }
    let mut recipients = Vec::with_capacity(times.len());
    for (person, (delivered, read, played)) in times {
        let name = member(db, person, None, false)?.map(|m| m.name).unwrap_or_default();
        recipients.push(Recipient { person: format!("person:{person}"), name, delivered, read, played });
    }
    recipients.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.person.cmp(&b.person)));
    Ok(Info { message: target.id.clone(), sent: went.then_some(target.at), recipients })
}

pub(super) async fn message_info(inner: &Inner, info: MessageInfo) -> Result<Info, Error> {
    let target = find(inner, info.chat, info.message).await?;
    query(inner, move |db| recipients(db, &target)).await
}

pub(super) async fn chat_info(inner: &Inner, info: ChatInfo) -> Result<ChatInfoResult, Error> {
    let chat = info.chat.clone();
    match query(inner, move |db| members(db, &chat)).await? {
        Some(result) => Ok(result),
        None if info.chat.is_empty() => Err(Error::new(code::INVALID_PARAMS, "chat.info needs a chat")),
        None => Err(no_chat(&info.chat)),
    }
}
