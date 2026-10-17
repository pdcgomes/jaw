//! `chat.info` and `message.info`: a chat's members, for completing `@` names, and how far a message
//! of yours has got with each recipient. A message's list comes from the receipts each recipient
//! sent, which last as long as the message, never from `recipients`, which go once it has got as
//! far as it can.

use std::collections::BTreeMap;

use jaw_proto::send::{ChatInfo, ChatInfoResult, Member};
use jaw_proto::{Error, code};
use rusqlite::{Connection, OptionalExtension};

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

pub(super) async fn chat_info(inner: &Inner, info: ChatInfo) -> Result<ChatInfoResult, Error> {
    let chat = info.chat.clone();
    match query(inner, move |db| members(db, &chat)).await? {
        Some(result) => Ok(result),
        None if info.chat.is_empty() => Err(Error::new(code::INVALID_PARAMS, "chat.info needs a chat")),
        None => Err(no_chat(&info.chat)),
    }
}
