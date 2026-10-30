//! What a message of yours adds to its text: the message it answers, quoted as WhatsApp quotes one,
//! and the people it mentions. WhatsApp writes a mention in the text as `@` and the user part of
//! the person's JID, which each phone shows as their name, so the `@name` an interface wrote goes
//! as that, and the row's spans show the name again when they're read.

use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::start::PERSON_JIDS;
use crate::convert::{self, Me};
use crate::store::unpack;

pub(super) const QUOTED: &str = "SELECT m.mine, j.jid, m.deleted, m.raw, coalesce(m.text, m.summary, ''),
        m.kind, m.text, m.edited_at IS NOT NULL, m.view_once
    FROM messages m LEFT JOIN person_jids j ON j.n = m.sender WHERE m.chat = ?1 AND m.id = ?2";
pub(super) const QUOTED_MERGING: &str = "SELECT m.mine, j.jid, m.deleted, m.raw, coalesce(m.text, m.summary, ''),
        m.kind, m.text, m.edited_at IS NOT NULL, m.view_once
    FROM chat_merges c JOIN messages m ON m.chat = c.gone AND m.id = ?2 LEFT JOIN person_jids j ON j.n = m.sender
    WHERE c.keep = ?1 LIMIT 1";
pub(super) const MEMBER_JID: &str =
    "SELECT j.jid FROM person_jids j JOIN group_members g ON g.chat = ?1 AND g.member = j.n
    WHERE j.person = ?2 ORDER BY j.n LIMIT 1";
pub(super) const NAMES: &str = "SELECT name, push_name, verified_name FROM people WHERE n = ?1";
pub(super) const CHAT_NAME: &str = "SELECT name FROM chats WHERE n = ?1";

/// The message a reply answers, as its quote goes to WhatsApp.
#[derive(Clone, Debug)]
pub(super) struct Quoted {
    pub id: String,
    /// Who wrote it: its sender's JID, or yours; empty when jawd doesn't know yours yet.
    pub participant: String,
    /// What it said, out of its wrappers and without its own quote.
    pub message: e2e::Message,
}

pub(super) enum Quote {
    Found(Box<Quoted>),
    Missing,
    Deleted,
}

/// A message as a reply quotes it: who wrote it, whether it was deleted, its raw protobuf, a line
/// of what it says, its kind and text, and whether it was edited.
struct Found {
    mine: bool,
    sender: Option<String>,
    deleted: bool,
    raw: Option<Vec<u8>>,
    line: String,
    kind: String,
    text: Option<String>,
    edited: bool,
    view_once: bool,
}

/// Message `id` in chat `chat` (by n, as the writer finds it), or in a chat being merged into it,
/// as a reply to it quotes it: an edited one as its row says it now, since an edit changes the
/// row's text and not the raw protobuf it came as.
pub(super) fn quoted(db: &Connection, chat: i64, id: &str, me: &Me) -> anyhow::Result<Quote> {
    let read = |sql: &str| -> rusqlite::Result<Option<Found>> {
        db.prepare_cached(sql)?
            .query_row(params![chat, id], |r| {
                Ok(Found {
                    mine: r.get(0)?,
                    sender: r.get(1)?,
                    deleted: r.get(2)?,
                    raw: r.get(3)?,
                    line: r.get(4)?,
                    kind: r.get(5)?,
                    text: r.get(6)?,
                    edited: r.get(7)?,
                    view_once: r.get(8)?,
                })
            })
            .optional()
    };
    let Some(found) = read(QUOTED)?.or(read(QUOTED_MERGING)?) else {
        return Ok(Quote::Missing);
    };
    if found.deleted {
        return Ok(Quote::Deleted);
    }
    let participant = if found.mine { me.jid().map(str::to_string) } else { found.sender };
    let said = found
        .raw
        .and_then(|packed| unpack(&packed).ok())
        .and_then(|raw| WebMessageInfo::decode(raw.as_slice()).ok())
        .and_then(|info| info.message)
        .map(|message| if found.view_once { convert::once_quoted(&message) } else { convert::bare(&message) });
    let text = |text: &str| e2e::Message { conversation: Some(text.into()), ..Default::default() };
    let message = match (said, found.kind.as_str(), found.text.as_deref()) {
        (_, "text", Some(said)) if found.edited => text(said),
        (Some(message), _, Some(caption)) if found.edited => convert::captioned(&message, caption),
        (Some(message), ..) => message,
        (None, ..) => text(&found.line),
    };
    Ok(Quote::Found(Box::new(Quoted { id: id.to_string(), participant: participant.unwrap_or_default(), message })))
}

/// A message's mentions, as WhatsApp takes them: each person's JID in the group, and the text with
/// each `@name` written as `@` and that JID's user part.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Mentioned {
    pub jids: Vec<String>,
    pub text: String,
}

/// The mentions of `people` (`person:` and a number each) in `text`, sent to group `chat`; a
/// person who isn't a member of it is `Err` with their ID.
pub(super) fn mentioned(
    db: &Connection,
    chat: i64,
    people: &[String],
    text: &str,
) -> anyhow::Result<Result<Mentioned, String>> {
    let mut jids: Vec<String> = Vec::new();
    let mut names: Vec<(String, String)> = Vec::new();
    for id in people {
        let Some(person) = id.strip_prefix("person:").and_then(|n| n.parse::<i64>().ok()) else {
            return Ok(Err(id.clone()));
        };
        let jid: Option<String> =
            db.prepare_cached(MEMBER_JID)?.query_row(params![chat, person], |r| r.get(0)).optional()?;
        let Some(jid) = jid else { return Ok(Err(id.clone())) };
        if jids.contains(&jid) {
            continue;
        }
        let user = jid.split(['@', ':']).next().unwrap_or_default().to_string();
        let (name, push_name, verified): (Option<String>, Option<String>, Option<String>) = db
            .prepare_cached(NAMES)?
            .query_row([person], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?
            .unwrap_or_default();
        let phones: Vec<String> =
            db.prepare_cached(PERSON_JIDS)?.query_map([person], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        let numbers =
            phones.iter().filter_map(|jid| jid.strip_suffix("@s.whatsapp.net")).map(|user| format!("+{user}"));
        for name in [name, push_name, verified].into_iter().flatten().chain(numbers) {
            names.push((name, user.clone()));
        }
        jids.push(jid);
    }
    // Longer names first, so "@Ana Ribeiro" isn't taken for "@Ana".
    names.sort_by_key(|(name, _)| std::cmp::Reverse(name.chars().count()));
    let mut text = text.to_string();
    for (name, user) in names {
        text = written_as(&text, &name, &user);
    }
    Ok(Ok(Mentioned { jids, text }))
}

/// Why `person`, as an interface named them, can't be mentioned in group `chat` (by n): said with
/// their name, or their number, and the group's name, as jaw shows them, rather than IDs.
pub(super) fn not_a_member(db: &Connection, chat: i64, person: &str) -> anyhow::Result<String> {
    let mut who = None;
    if let Some(n) = person.strip_prefix("person:").and_then(|n| n.parse::<i64>().ok()) {
        let names: Option<(Option<String>, Option<String>, Option<String>)> =
            db.prepare_cached(NAMES)?.query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
        who = names.and_then(|(name, push_name, verified)| name.or(push_name).or(verified));
        if who.is_none() {
            let jids: Vec<String> =
                db.prepare_cached(PERSON_JIDS)?.query_map([n], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            who = jids.iter().find_map(|jid| jid.strip_suffix("@s.whatsapp.net")).map(|user| format!("+{user}"));
        }
    }
    let group: Option<String> = db.prepare_cached(CHAT_NAME)?.query_row([chat], |r| r.get(0)).optional()?.flatten();
    Ok(format!(
        "{} isn't in {}, so they can't be mentioned there",
        who.as_deref().unwrap_or(person),
        group.as_deref().unwrap_or("this group")
    ))
}

/// `text` with each `@name` that isn't part of a longer word written as `@user`.
fn written_as(text: &str, name: &str, user: &str) -> String {
    let said = format!("@{name}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(&said) {
        let after = &rest[at + said.len()..];
        out.push_str(&rest[..at]);
        if after.chars().next().is_some_and(char::is_alphanumeric) {
            out.push_str(&said);
        } else {
            out.push('@');
            out.push_str(user);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Text as WhatsApp sends it: plain, or with what it adds, the chat's disappearing timer among it.
pub(super) fn text_message(text: &str, ephemeral: u32, quoted: Option<&Quoted>, mentioned: &[String]) -> e2e::Message {
    if ephemeral == 0 && quoted.is_none() && mentioned.is_empty() {
        return e2e::Message { conversation: Some(text.into()), ..Default::default() };
    }
    let mut context = e2e::ContextInfo {
        expiration: (ephemeral > 0).then_some(ephemeral),
        mentioned_jid: mentioned.to_vec(),
        ..Default::default()
    };
    if let Some(quoted) = quoted {
        context.stanza_id = Some(quoted.id.clone());
        context.participant = (!quoted.participant.is_empty()).then(|| quoted.participant.clone());
        context.quoted_message = Some(Box::new(quoted.message.clone()));
    }
    let extended = e2e::ExtendedTextMessage {
        text: Some(text.into()),
        context_info: Some(Box::new(context)),
        ..Default::default()
    };
    e2e::Message { extended_text_message: Some(Box::new(extended)), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_at_name_is_written_as_the_jids_user_but_not_inside_a_longer_word() {
        assert_eq!(written_as("@Ana vens? @Anabela não", "Ana", "351911"), "@351911 vens? @Anabela não");
        assert_eq!(written_as("olá @Inês Gomes, @Inês Gomes", "Inês Gomes", "1000"), "olá @1000, @1000");
        assert_eq!(written_as("sem ninguém", "Ana", "351911"), "sem ninguém");
    }
}
