//! A history blob from the phone: each conversation's chat as the phone has it (its name, pin,
//! archive, mute, where it was read and community) and its messages, then the push names and the
//! LIDs of phone numbers the blob carries. A one-to-one conversation the phone keeps by LID goes under
//! its phone number's JID when the blob gives it, as live messages do.

use std::collections::HashSet;

use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync};

use super::{Me, is_person, message, user_jid};
use crate::store::{Change, Chat, Person, Read};

/// The changes in a blob, a conversation at a time; the people it names come last.
pub fn blob(history: &HistorySync, me: &Me) -> Vec<Vec<Change>> {
    let mut parts: Vec<Vec<Change>> = history.conversations.iter().map(|c| conversation(c, me)).collect();
    let mut people: Vec<Change> = history
        .phone_number_to_lid_mappings
        .iter()
        .filter_map(|m| {
            let jids: Vec<String> = [m.pn_jid.as_deref(), m.lid_jid.as_deref()]
                .into_iter()
                .flatten()
                .filter(|j| is_person(j))
                .map(user_jid)
                .collect();
            (jids.len() == 2).then(|| Change::Person(Person { jids, ..Person::default() }))
        })
        .collect();
    people.extend(history.pushnames.iter().filter_map(|p| {
        let jid = p.id.as_deref().filter(|j| is_person(j))?;
        let name = p.pushname.clone().filter(|n| !n.is_empty() && n != "-")?;
        Some(Change::Person(Person { jids: vec![user_jid(jid)], push_name: Some(name), ..Person::default() }))
    }));
    if !people.is_empty() {
        parts.push(people);
    }
    parts
}

/// Seconds or milliseconds since the epoch, as milliseconds: the phone's fields use both.
fn ms(time: u64) -> i64 {
    let time = time.min(i64::MAX as u64 / 1000) as i64;
    if time < 100_000_000_000 { time * 1000 } else { time }
}

fn conversation(c: &Conversation, me: &Me) -> Vec<Change> {
    let mut id = c.id.clone();
    let pn = c.pn_jid.as_deref().filter(|j| j.ends_with("@s.whatsapp.net")).map(user_jid);
    let lid = c.lid_jid.as_deref().filter(|j| j.ends_with("@lid")).map(user_jid);
    if id.ends_with("@lid")
        && let Some(pn) = &pn
    {
        id = pn.clone();
    }
    let mut changes = Vec::new();
    if let (Some(pn), Some(lid)) = (&pn, &lid) {
        changes.push(Change::Person(Person { jids: vec![pn.clone(), lid.clone()], ..Person::default() }));
    }
    let kind = if c.is_parent_group.unwrap_or(false) {
        Some("community")
    } else if c.is_default_subgroup.unwrap_or(false) {
        Some("announcement")
    } else {
        None
    };
    let name = c.name.clone().or_else(|| c.display_name.clone()).filter(|n| !n.is_empty() && !is_person(&id));
    changes.push(Change::Chat(Chat {
        id: id.clone(),
        kind,
        name,
        pinned: c.pinned.map(|at| if at > 0 { ms(u64::from(at)) } else { 0 }),
        archived: c.archived,
        muted_until: c.mute_end_time.map(ms),
        marked_unread: c.marked_as_unread,
        community: c.parent_group_id.clone(),
        ephemeral: c.ephemeral_expiration,
        ..Chat::default()
    }));
    let mut messages = Vec::new();
    for entry in &c.messages {
        let Some(info) = entry.message.as_ref() else { continue };
        if info.key.remote_jid.as_deref() == Some(id.as_str()) {
            messages.extend(message(info, me, false));
        } else {
            let mut info = info.clone();
            info.key.remote_jid = Some(id.clone());
            messages.extend(message(&info, me, false));
        }
    }
    changes.push(Change::Read(read_position(c, &id, &messages)));
    changes.extend(messages);
    changes
}

/// Where the phone had the conversation read up to: just before the oldest of its unread messages
/// from others among those the blob carries, or, when it carries fewer than the phone counts, as
/// of the conversation's newest moment with the phone's own counts standing in.
fn read_position(c: &Conversation, chat: &str, changes: &[Change]) -> Read {
    let rows = changes.iter().filter_map(|change| match change {
        Change::Message(m) => Some(m),
        Change::Edited { shown, .. } => Some(shown.as_ref()),
        _ => None,
    });
    let mut newest =
        c.conversation_timestamp.map(ms).unwrap_or_default().max(c.last_msg_timestamp.map(ms).unwrap_or_default());
    let mut theirs = Vec::new();
    for m in rows {
        newest = newest.max(m.at);
        if !m.mine && !matches!(m.kind, "system" | "call") {
            theirs.push(m.at);
        }
    }
    theirs.sort_unstable_by(|a, b| b.cmp(a));
    let unread = c.unread_count.unwrap_or(0) as usize;
    let read = Read { chat: chat.to_string(), at: newest, ..Read::default() };
    match unread {
        0 => read,
        n if n <= theirs.len() => Read { at: theirs[n - 1] - 1, ..read },
        n => Read { phone: Some((n as u32, c.unread_mention_count.unwrap_or(0))), ..read },
    }
}

/// What a blob carries besides the messages that became rows, whose raws are kept with them: every
/// conversation's settings and the messages that didn't (edits, reactions and the like), the push
/// names, statuses, past participants and everything jaw doesn't read yet.
pub fn remainder(history: &HistorySync, parts: &[Vec<Change>]) -> HistorySync {
    let rows: HashSet<&str> = parts
        .iter()
        .flatten()
        .filter_map(|change| match change {
            Change::Message(m) => Some(m.id.as_str()),
            _ => None,
        })
        .collect();
    let mut rest = history.clone();
    for conversation in &mut rest.conversations {
        conversation.messages.retain(|entry| {
            let id = entry.message.as_ref().and_then(|m| m.key.id.as_deref()).unwrap_or_default();
            !rows.contains(id)
        });
    }
    rest
}
