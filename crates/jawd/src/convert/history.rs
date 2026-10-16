//! A history blob from the phone: each conversation's chat as the phone has it (its name, pin,
//! archive, mute, unread counts and community) and its messages, then the push names and the LIDs
//! of phone numbers the blob carries. A one-to-one conversation the phone keeps by LID goes under
//! its phone number's JID when the blob gives it, as live messages do.

use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync};

use super::{Me, is_person, message, user_jid};
use crate::store::{Change, Chat, Person};

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
        unread: c.unread_count,
        mentions: c.unread_mention_count,
        community: c.parent_group_id.clone(),
        ephemeral: c.ephemeral_expiration,
        ..Chat::default()
    }));
    for entry in &c.messages {
        let Some(info) = entry.message.as_ref() else { continue };
        if info.key.remote_jid.as_deref() == Some(id.as_str()) {
            changes.extend(message(info, me, false));
        } else {
            let mut info = info.clone();
            info.key.remote_jid = Some(id.clone());
            changes.extend(message(&info, me, false));
        }
    }
    changes
}
