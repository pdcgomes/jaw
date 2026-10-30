//! People and groups, as the engine describes them: contacts and push names from whatsmeow's store,
//! LIDs joined to phone numbers, and groups with their members and communities.

use serde_json::json;

use super::{Me, is_person, kinds, user_jid};
use crate::engine::pb;
use crate::store::{Change, Chat, Members, Message, Person};

/// What the line a group's timer changing leaves has for its ID, before the change's time: WhatsApp
/// gives the change none, and its time is the same each time the change comes again.
pub const TIMER_LINE: &str = "jaw-timer-";

/// Someone known by one or two JIDs, with whatever names are known.
fn person(jids: &[&str], name: Option<&str>, push_name: Option<&str>, verified: Option<&str>) -> Option<Change> {
    let jids: Vec<String> = jids.iter().filter(|jid| is_person(jid)).map(|jid| user_jid(jid)).collect();
    if jids.is_empty() {
        return None;
    }
    let known = |s: Option<&str>| s.filter(|s| !s.is_empty()).map(str::to_string);
    Some(Change::Person(Person {
        jids,
        name: known(name),
        push_name: known(push_name),
        verified_name: known(verified),
    }))
}

pub fn contacts(list: &pb::ContactList) -> Vec<Change> {
    list.contacts
        .iter()
        .filter_map(|c| {
            let name = if c.full_name.is_empty() { &c.first_name } else { &c.full_name };
            person(&[&c.jid, &c.lid], Some(name), Some(&c.push_name), Some(&c.business_name))
        })
        .collect()
}

pub fn push_name(p: &pb::PushName) -> Option<Change> {
    person(&[&p.jid, &p.jid_alt], None, Some(&p.push_name), None)
}

pub fn lid_pairs(pairs: &pb::LidPairs) -> Vec<Change> {
    pairs.pairs.iter().filter_map(|p| person(&[&p.lid, &p.phone], None, None, None)).collect()
}

/// A group's kind as the protocol names it.
fn group_kind(g: &pb::GroupInfo) -> &'static str {
    if g.is_parent {
        "community"
    } else if g.is_default_sub {
        "announcement"
    } else {
        "group"
    }
}

fn role(p: &pb::Participant) -> &'static str {
    if p.super_admin {
        "superadmin"
    } else if p.admin {
        "admin"
    } else {
        "member"
    }
}

/// A group as GetJoinedGroups or joining one describes it: the chat, its members, and each
/// member's LID joined to their phone number.
pub fn group(g: &pb::GroupInfo) -> Vec<Change> {
    let mut changes = vec![Change::Chat(Chat {
        id: g.jid.clone(),
        kind: Some(group_kind(g)),
        name: Some(g.name.clone()).filter(|n| !n.is_empty()),
        community: Some(g.linked_parent.clone()),
        ephemeral: Some(g.ephemeral_seconds),
        ..Chat::default()
    })];
    for p in &g.participants {
        if !p.phone.is_empty() && !p.lid.is_empty() {
            changes.extend(person(&[&p.lid, &p.phone], None, None, None));
        }
    }
    if !g.is_parent {
        let set = g.participants.iter().filter(|p| !p.jid.is_empty()).map(|p| (user_jid(&p.jid), role(p))).collect();
        changes.push(Change::Members(Members { chat: g.jid.clone(), all: true, set, remove: Vec::new() }));
    }
    changes
}

/// What changed in a group. A change of its disappearing timer leaves its line, as the phones
/// show one.
pub fn group_change(c: &pb::GroupChange, me: &Me) -> Vec<Change> {
    let mut changes = Vec::new();
    if !c.sender.is_empty() && !c.sender_pn.is_empty() {
        changes.extend(person(&[&c.sender, &c.sender_pn], None, None, None));
    }
    let chat = Chat {
        id: c.jid.clone(),
        name: c.name.clone().filter(|n| !n.is_empty()),
        ephemeral: c.ephemeral_seconds,
        ephemeral_at: (c.timestamp_ms > 0).then_some(c.timestamp_ms),
        ..Chat::default()
    };
    if chat.name.is_some() || chat.ephemeral.is_some() {
        changes.push(Change::Chat(chat));
    }
    if let Some(seconds) = c.ephemeral_seconds.filter(|_| c.timestamp_ms > 0) {
        let sentence = kinds::ephemeral_sentence(seconds);
        let mine = !c.sender.is_empty() && me.is(&c.sender);
        changes.push(Change::Message(Message {
            chat: c.jid.clone(),
            id: format!("{TIMER_LINE}{}", c.timestamp_ms),
            at: c.timestamp_ms,
            sender: (!mine && !c.sender.is_empty()).then(|| user_jid(&c.sender)),
            mine,
            kind: "system",
            summary: sentence.clone(),
            detail: Some(json!({ "system": sentence }).to_string()),
            live: true,
            ..Message::default()
        }));
    }
    if !c.linked.is_empty() {
        changes.push(Change::Chat(Chat { id: c.linked.clone(), community: Some(c.jid.clone()), ..Chat::default() }));
    }
    if !c.unlinked.is_empty() {
        changes.push(Change::Chat(Chat { id: c.unlinked.clone(), community: Some(String::new()), ..Chat::default() }));
    }
    let mut set: Vec<(String, &'static str)> = c.join.iter().map(|jid| (user_jid(jid), "member")).collect();
    set.extend(c.promote.iter().map(|jid| (user_jid(jid), "admin")));
    set.extend(c.demote.iter().map(|jid| (user_jid(jid), "member")));
    if !set.is_empty() || !c.leave.is_empty() {
        changes.push(Change::Members(Members { chat: c.jid.clone(), all: false, set, remove: c.leave.clone() }));
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_community_its_announcement_group_and_a_group_in_it_are_told_apart() {
        let community =
            pb::GroupInfo { jid: "1@g.us".into(), name: "Lisbon".into(), is_parent: true, ..Default::default() };
        let announcements = pb::GroupInfo {
            jid: "2@g.us".into(),
            linked_parent: "1@g.us".into(),
            is_default_sub: true,
            ..Default::default()
        };
        let climbing = pb::GroupInfo {
            jid: "3@g.us".into(),
            name: "Climbing".into(),
            linked_parent: "1@g.us".into(),
            participants: vec![pb::Participant {
                jid: "100000000001@lid".into(),
                phone: "351910000002@s.whatsapp.net".into(),
                lid: "100000000001@lid".into(),
                admin: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let kinds: Vec<(Option<&str>, Option<String>)> = [&community, &announcements, &climbing]
            .iter()
            .map(|g| match &group(g)[0] {
                Change::Chat(chat) => (chat.kind, chat.community.clone()),
                _ => panic!("a group's first change is its chat"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                (Some("community"), Some(String::new())),
                (Some("announcement"), Some("1@g.us".into())),
                (Some("group"), Some("1@g.us".into()))
            ]
        );
        let changes = group(&climbing);
        let Change::Person(member) = &changes[1] else { panic!("the member's LID and number are joined") };
        assert_eq!(member.jids, ["100000000001@lid", "351910000002@s.whatsapp.net"]);
        let Change::Members(members) = &changes[2] else { panic!("its members follow") };
        assert!(members.all);
        assert_eq!(members.set, [("100000000001@lid".to_string(), "admin")]);
        assert!(
            group(&community).iter().all(|c| !matches!(c, Change::Members(_))),
            "a community's members are its groups'"
        );
    }

    #[test]
    fn a_group_change_names_only_what_changed() {
        let change = pb::GroupChange {
            jid: "3@g.us".into(),
            name: Some("Climbing on Saturdays".into()),
            join: vec!["351910000003:4@s.whatsapp.net".into()],
            leave: vec!["351910000004@s.whatsapp.net".into()],
            ..Default::default()
        };
        let changes = group_change(&change, &Me::default());
        let Change::Chat(chat) = &changes[0] else { panic!("the chat first") };
        assert_eq!((chat.name.as_deref(), chat.ephemeral, chat.kind), (Some("Climbing on Saturdays"), None, None));
        let Change::Members(members) = &changes[1] else { panic!("then its members") };
        assert!(!members.all);
        assert_eq!(members.set, [("351910000003@s.whatsapp.net".to_string(), "member")]);
        assert_eq!(members.remove, ["351910000004@s.whatsapp.net"]);
        let unlinked = group_change(
            &pb::GroupChange { jid: "1@g.us".into(), unlinked: "3@g.us".into(), ..Default::default() },
            &Me::default(),
        );
        let [Change::Chat(chat)] = unlinked.as_slice() else { panic!("an unlinked group leaves its community") };
        assert_eq!((chat.id.as_str(), chat.community.as_deref()), ("3@g.us", Some("")));
    }

    #[test]
    fn a_groups_timer_changing_says_when_it_was_set_and_leaves_its_line_once_however_often_it_comes() {
        let me = Me::new(["351910000001@s.whatsapp.net".to_string()]);
        let change = |sender: &str| pb::GroupChange {
            jid: "3@g.us".into(),
            sender: sender.into(),
            timestamp_ms: 1_790_000_000_123,
            ephemeral_seconds: Some(604_800),
            ..Default::default()
        };
        let from_ana = group_change(&change("100000000002@lid"), &me);
        let [Change::Chat(chat), Change::Message(line)] = from_ana.as_slice() else { panic!("{from_ana:?}") };
        assert_eq!((chat.ephemeral, chat.ephemeral_at), (Some(604_800), Some(1_790_000_000_123)));
        assert_eq!((line.id.as_str(), line.at, line.kind), ("jaw-timer-1790000000123", 1_790_000_000_123, "system"));
        assert_eq!(line.summary, "Disappearing messages turned on: 7 days");
        assert_eq!((line.sender.as_deref(), line.mine), (Some("100000000002@lid"), false));
        let again = group_change(&change("100000000002@lid"), &me);
        assert!(matches!(&again[1], Change::Message(m) if m.id == line.id), "the same change, the same line");
        let mine = group_change(&change("351910000001:2@s.whatsapp.net"), &me);
        assert!(matches!(&mine[1], Change::Message(m) if m.mine && m.sender.is_none()), "{mine:?}");
        let untimed = group_change(&pb::GroupChange { timestamp_ms: 0, ..change("100000000002@lid") }, &me);
        assert!(untimed.iter().all(|c| !matches!(c, Change::Message(_))), "no time, no line: {untimed:?}");
    }

    #[test]
    fn contacts_and_push_names_become_people_by_every_jid_known() {
        let list = pb::ContactList {
            contacts: vec![
                pb::Contact {
                    jid: "351910000002@s.whatsapp.net".into(),
                    first_name: "Ana".into(),
                    full_name: "Ana Ribeiro".into(),
                    push_name: "Ana R".into(),
                    lid: "100000000001@lid".into(),
                    ..Default::default()
                },
                pb::Contact { jid: "120363@g.us".into(), ..Default::default() },
            ],
        };
        let people = contacts(&list);
        let [Change::Person(ana)] = people.as_slice() else { panic!("one person; a group isn't one") };
        assert_eq!(ana.jids, ["351910000002@s.whatsapp.net", "100000000001@lid"]);
        assert_eq!(
            (ana.name.as_deref(), ana.push_name.as_deref(), ana.verified_name.as_deref()),
            (Some("Ana Ribeiro"), Some("Ana R"), None)
        );
        let Some(Change::Person(rui)) = push_name(&pb::PushName {
            jid: "100000000002:3@lid".into(),
            push_name: "Rui".into(),
            ..Default::default()
        }) else {
            panic!("a push name is a person")
        };
        assert_eq!(
            (rui.jids.as_slice(), rui.push_name.as_deref()),
            (&["100000000002@lid".to_string()][..], Some("Rui"))
        );
    }
}
