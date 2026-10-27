//! App state, as whatsmeow decodes each mutation: its index and its SyncActionValue, turned into
//! changes to chats, messages and people. Pinned chats keep when they were pinned, so they sort
//! among themselves by it.

use prost::Message as _;
use wa_proto::wa_web_protobuf_sync_action::SyncActionValue;

use super::{is_person, user_jid};
use crate::store::{Amend, Amendment, Change, Chat, Person, Read};

/// Until when a chat muted for ever is muted: the last millisecond of the year 9999.
pub const FOREVER: i64 = 253_402_300_799_999;

/// Seconds since the epoch, as app state keeps a message range's times, in milliseconds.
fn ms(seconds: i64) -> i64 {
    if seconds.abs() < 100_000_000_000 { seconds.saturating_mul(1000) } else { seconds }
}

/// What one app-state mutation changes, and the account's own name if it's that.
#[derive(Default)]
pub struct Applied {
    pub changes: Vec<Change>,
    pub my_name: Option<String>,
}

pub fn mutation(index: &[String], value: &[u8]) -> Applied {
    let mut applied = Applied::default();
    let Ok(value) = SyncActionValue::decode(value) else { return applied };
    let at = value.timestamp.unwrap_or_default();
    let jid = index.get(1).filter(|jid| !jid.is_empty()).map(String::as_str);
    let chat = |chat: Chat| Change::Chat(Chat { id: jid.map(user_jid).unwrap_or_default(), ..chat });
    match (index.first().map(String::as_str), jid) {
        (Some("pin_v1"), Some(_)) => {
            let pinned = value.pin_action.and_then(|a| a.pinned).unwrap_or(false);
            applied.changes.push(chat(Chat { pinned: Some(if pinned { at.max(1) } else { 0 }), ..Chat::default() }));
        }
        (Some("archive"), Some(_)) => {
            let archived = value.archive_chat_action.and_then(|a| a.archived).unwrap_or(false);
            applied.changes.push(chat(Chat { archived: Some(archived), ..Chat::default() }));
        }
        (Some("mute"), Some(_)) => {
            let action = value.mute_action.unwrap_or_default();
            let until = match (action.muted.unwrap_or(false), action.mute_end_timestamp.unwrap_or(0)) {
                (false, _) => 0,
                (true, end) if end < 0 => FOREVER,
                (true, end) => end,
            };
            applied.changes.push(chat(Chat { muted_until: Some(until), ..Chat::default() }));
        }
        (Some("markChatAsRead"), Some(jid)) => {
            let action = value.mark_chat_as_read_action.unwrap_or_default();
            if action.read.unwrap_or(false) {
                // Read up to the newest message the range names, or its last message's time, which
                // a replay of the mutation can't move back.
                let range = action.message_range.unwrap_or_default();
                let last = range.last_message_timestamp.map(ms).unwrap_or(at);
                let ids = range.messages.iter().filter_map(|m| m.key.as_ref()?.id.clone()).collect();
                applied.changes.push(Change::Read(Read {
                    chat: user_jid(jid),
                    at: last,
                    ids,
                    phone: None,
                    here: false,
                }));
                // Read on any device, the chat isn't marked unread, even where jaw has read as far.
                applied.changes.push(chat(Chat { marked_unread: Some(false), ..Chat::default() }));
            } else {
                applied.changes.push(chat(Chat { marked_unread: Some(true), ..Chat::default() }));
            }
        }
        (Some("contact"), Some(jid)) if is_person(jid) => {
            let action = value.contact_action.unwrap_or_default();
            let mut jids = vec![jid.to_string()];
            jids.extend(action.lid_jid.into_iter().chain(action.pn_jid).filter(|j| is_person(j)));
            let name = action.full_name.or(action.first_name).filter(|n| !n.is_empty());
            applied.changes.push(Change::Person(Person { jids, name, ..Person::default() }));
        }
        (Some("lid_contact"), Some(jid)) if is_person(jid) => {
            let action = value.lid_contact_action.unwrap_or_default();
            let name = action.full_name.or(action.first_name).filter(|n| !n.is_empty());
            applied.changes.push(Change::Person(Person { jids: vec![jid.to_string()], name, ..Person::default() }));
        }
        (Some(kind @ ("star" | "deleteMessageForMe")), Some(chat)) => {
            let Some(id) = index.get(2).filter(|id| !id.is_empty()) else { return applied };
            let what = if kind == "star" {
                Amendment::Star { starred: value.star_action.and_then(|a| a.starred).unwrap_or(false) }
            } else {
                Amendment::DeleteForMe
            };
            applied.changes.push(Change::Amend(Amend {
                chat: user_jid(chat),
                target: id.clone(),
                sender: None,
                mine: index.get(3).is_some_and(|from_me| from_me == "1"),
                at,
                what,
                raw: None,
            }));
        }
        (Some("setting_pushName"), _) => {
            applied.my_name = value.push_name_setting.and_then(|s| s.name).filter(|n| !n.is_empty());
        }
        _ => {}
    }
    applied
}

#[cfg(test)]
mod tests {
    use wa_proto::wa_web_protobuf_sync_action as action;

    use super::*;

    fn value(f: impl FnOnce(&mut SyncActionValue)) -> Vec<u8> {
        let mut value = SyncActionValue { timestamp: Some(1_792_000_000_000), ..Default::default() };
        f(&mut value);
        value.encode_to_vec()
    }

    fn index(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn only_chat(applied: Applied) -> Chat {
        match applied.changes.as_slice() {
            [Change::Chat(chat)] => chat.clone(),
            _ => panic!("expected one chat change"),
        }
    }

    const ANA: &str = "351910000002@s.whatsapp.net";

    #[test]
    fn a_pin_keeps_when_it_was_pinned_and_an_unpin_clears_it() {
        let pin = value(|v| v.pin_action = Some(action::PinAction { pinned: Some(true) }));
        let chat = only_chat(mutation(&index(&["pin_v1", ANA]), &pin));
        assert_eq!((chat.id.as_str(), chat.pinned), (ANA, Some(1_792_000_000_000)));
        let unpin = value(|v| v.pin_action = Some(action::PinAction { pinned: Some(false) }));
        assert_eq!(only_chat(mutation(&index(&["pin_v1", ANA]), &unpin)).pinned, Some(0));
    }

    #[test]
    fn archive_mute_and_read_set_the_chat_as_the_phone_has_it() {
        let archive = value(|v| {
            v.archive_chat_action = Some(action::ArchiveChatAction { archived: Some(true), ..Default::default() })
        });
        assert_eq!(only_chat(mutation(&index(&["archive", ANA]), &archive)).archived, Some(true));
        let forever = value(|v| {
            v.mute_action =
                Some(action::MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() })
        });
        assert_eq!(only_chat(mutation(&index(&["mute", ANA]), &forever)).muted_until, Some(FOREVER));
        let unmute = value(|v| v.mute_action = Some(action::MuteAction { muted: Some(false), ..Default::default() }));
        assert_eq!(only_chat(mutation(&index(&["mute", ANA]), &unmute)).muted_until, Some(0));
        let unread = value(|v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction { read: Some(false), ..Default::default() })
        });
        assert_eq!(only_chat(mutation(&index(&["markChatAsRead", ANA]), &unread)).marked_unread, Some(true));
    }

    #[test]
    fn marking_a_chat_read_reads_it_up_to_the_newest_message_its_range_names_and_unmarks_it() {
        let read = value(|v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction {
                read: Some(true),
                message_range: Some(action::SyncActionMessageRange {
                    last_message_timestamp: Some(1_791_999_000),
                    messages: vec![action::SyncActionMessage {
                        key: Some(wa_proto::wa_common::MessageKey { id: Some("3EB0AA".into()), ..Default::default() }),
                        timestamp: Some(1_791_999_000),
                    }],
                    ..Default::default()
                }),
            })
        });
        let applied = mutation(&index(&["markChatAsRead", ANA]), &read);
        let [Change::Read(read), Change::Chat(unmarked)] = applied.changes.as_slice() else {
            panic!("expected a read position and the chat unmarked")
        };
        assert_eq!(
            (read.chat.as_str(), read.at, read.ids.as_slice(), read.phone),
            (ANA, 1_791_999_000_000, &["3EB0AA".to_string()][..], None)
        );
        assert_eq!((unmarked.id.as_str(), unmarked.marked_unread), (ANA, Some(false)));
        let bare = value(|v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction { read: Some(true), ..Default::default() })
        });
        let applied = mutation(&index(&["markChatAsRead", ANA]), &bare);
        let [Change::Read(read), Change::Chat(_)] = applied.changes.as_slice() else {
            panic!("expected a read position and the chat unmarked")
        };
        assert_eq!(read.at, 1_792_000_000_000, "without a range, the mutation's own time");
    }

    #[test]
    fn a_contact_joins_its_phone_number_and_lid_under_its_name() {
        let contact = value(|v| {
            v.contact_action = Some(action::ContactAction {
                full_name: Some("Ana Ribeiro".into()),
                lid_jid: Some("100000000001@lid".into()),
                ..Default::default()
            })
        });
        let applied = mutation(&index(&["contact", ANA]), &contact);
        let [Change::Person(person)] = applied.changes.as_slice() else { panic!("expected a person") };
        assert_eq!(person.jids, [ANA, "100000000001@lid"]);
        assert_eq!(person.name.as_deref(), Some("Ana Ribeiro"));
    }

    #[test]
    fn stars_and_deletes_for_me_amend_the_message_they_name() {
        let star = value(|v| v.star_action = Some(action::StarAction { starred: Some(true) }));
        let applied = mutation(&index(&["star", ANA, "3EB0AA", "1", "0"]), &star);
        let [Change::Amend(amend)] = applied.changes.as_slice() else { panic!("expected an amendment") };
        assert_eq!((amend.chat.as_str(), amend.target.as_str(), amend.mine), (ANA, "3EB0AA", true));
        assert_eq!(amend.what, Amendment::Star { starred: true });
        let delete = value(|v| v.delete_message_for_me_action = Some(Default::default()));
        let applied = mutation(&index(&["deleteMessageForMe", ANA, "3EB0BB", "0", "0"]), &delete);
        let [Change::Amend(amend)] = applied.changes.as_slice() else { panic!("expected an amendment") };
        assert_eq!(amend.what, Amendment::DeleteForMe);
    }

    #[test]
    fn the_accounts_own_name_comes_back_on_its_own() {
        let name = value(|v| v.push_name_setting = Some(action::PushNameSetting { name: Some("Pedro".into()) }));
        let applied = mutation(&index(&["setting_pushName"]), &name);
        assert!(applied.changes.is_empty());
        assert_eq!(applied.my_name.as_deref(), Some("Pedro"));
    }

    #[test]
    fn what_jaw_doesnt_use_and_what_doesnt_decode_change_nothing() {
        let other = value(|v| v.timestamp = Some(5));
        assert!(mutation(&index(&["time_format"]), &other).changes.is_empty());
        assert!(mutation(&index(&["pin_v1", ANA]), b"\xff\xff").changes.is_empty());
    }
}
