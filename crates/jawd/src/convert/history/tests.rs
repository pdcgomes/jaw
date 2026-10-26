use prost::encoding::{WireType, encode_key, encode_varint};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_history_sync::{
    Conversation, GlobalSettings, GroupParticipant, HistorySync, HistorySyncMsg, PastParticipants,
    PhoneNumberToLidMapping, Pushname,
};
use wa_proto::wa_web_protobufs_web::web_message_info::StubType;
use wa_proto::wa_web_protobufs_web::{Reaction, WebMessageInfo};

use super::*;

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const GROUP: &str = "120363000000000001@g.us";
const AT: u64 = 1_792_000_000;

/// The walk's reference: the blob decoded whole, as jawd did before it walked blobs, kept to show
/// the walk gives the same changes and keeps the same rest.
mod decoded_whole {
    use std::collections::HashSet;

    use super::super::{Me, is_person, message, ms, user_jid};
    use super::{Conversation, HistorySync};
    use crate::store::{Change, Chat, Person, Read};

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
}

fn me() -> Me {
    Me::new(["351910000001:12@s.whatsapp.net".to_string(), "100000000001@lid".to_string()])
}

fn text(words: &str) -> e2e::Message {
    e2e::Message { conversation: Some(words.into()), ..Default::default() }
}

fn key(chat: &str, id: &str, mine: bool, participant: Option<&str>) -> MessageKey {
    MessageKey {
        remote_jid: Some(chat.into()),
        from_me: Some(mine),
        id: Some(id.into()),
        participant: participant.map(str::to_string),
    }
}

/// A message in a history blob, sent `s` seconds after AT.
fn entry(key: MessageKey, s: u64, f: impl FnOnce(&mut WebMessageInfo)) -> HistorySyncMsg {
    let mut info = WebMessageInfo { key, message_timestamp: Some(AT + s), ..Default::default() };
    f(&mut info);
    HistorySyncMsg { message: Some(info), msg_order_id: Some(s) }
}

/// A field this jawd's protobufs don't know, as a later WhatsApp might send it.
fn unknown(buf: &mut Vec<u8>) {
    encode_key(4_000, WireType::LengthDelimited, buf);
    encode_varint(5, buf);
    buf.extend_from_slice(b"later");
}

/// A blob of every kind of thing history brings: a one-to-one chat the phone keeps by LID with
/// messages both ways, an edit, a reaction in its own message and on a message, a deletion and a
/// shell with nothing in it; a group with its members, a system line, a missed call and a
/// disappearing-message setting; a chat with more unread than the blob carries and one with no
/// messages; then statuses, push names, the LID map, settings, past participants and fields this
/// jawd doesn't know, at every level.
fn varied() -> (HistorySync, Vec<u8>) {
    use e2e::protocol_message::Type;
    let protocol = |kind: Type, f: &dyn Fn(&mut e2e::ProtocolMessage)| {
        let mut protocol = e2e::ProtocolMessage { r#type: Some(kind as i32), ..Default::default() };
        f(&mut protocol);
        e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() }
    };
    let direct = Conversation {
        id: ANA_LID.into(),
        pn_jid: Some(ANA.into()),
        lid_jid: Some(ANA_LID.into()),
        unread_count: Some(1),
        pinned: Some(AT as u32),
        messages: vec![
            entry(key(ANA_LID, "3EB0AAAA0001", false, None), 1, |m| {
                m.message = Some(text("olá, did you see the photos?"));
                m.push_name = Some("Ana".into());
                m.reactions = vec![Reaction {
                    key: Some(key(ANA, "3EB0AAAA9001", true, None)),
                    text: Some("❤".into()),
                    sender_timestamp_ms: Some((AT as i64 + 5) * 1000),
                    ..Default::default()
                }];
            }),
            entry(key(ANA_LID, "3EB0BBBB0002", true, None), 2, |m| {
                m.message = Some(text("yes, the lake ones"));
                m.status = Some(4);
            }),
            entry(key(ANA_LID, "3EB0CCCC0003", true, None), 3, |m| {
                m.message = Some(protocol(Type::MessageEdit, &|p| {
                    p.key = Some(key(ANA, "3EB0BBBB0002", true, None));
                    p.edited_message = Some(Box::new(text("yes, the ones at the lake")));
                    p.timestamp_ms = Some((AT as i64 + 3) * 1000);
                }));
            }),
            entry(key(ANA_LID, "3EB0DDDD0004", false, None), 4, |m| {
                m.message = Some(e2e::Message {
                    reaction_message: Some(e2e::ReactionMessage {
                        key: Some(key(ANA, "3EB0BBBB0002", true, None)),
                        text: Some("😂".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
            }),
            entry(key(ANA_LID, "3EB0EEEE0005", false, None), 5, |m| {
                m.message = Some(protocol(Type::Revoke, &|p| p.key = Some(key(ANA, "3EB0AAAA0001", false, None))));
            }),
            entry(key(ANA_LID, "3EB0FFFF0006", false, None), 6, |_| {}),
            HistorySyncMsg { message: None, msg_order_id: Some(7) },
            entry(key(ANA_LID, "", false, None), 8, |m| m.message = Some(text("no ID"))),
            entry(key(ANA_LID, "3EB01111000A", false, None), 9, |m| m.message = Some(text("and later"))),
        ],
        ..Default::default()
    };
    let group = Conversation {
        id: GROUP.into(),
        name: Some("Lisbon Climbing".into()),
        unread_count: Some(2),
        unread_mention_count: Some(1),
        participant: vec![
            GroupParticipant { user_jid: "100000000003@lid".into(), rank: Some(1), ..Default::default() },
            GroupParticipant { user_jid: "100000000004@lid".into(), ..Default::default() },
        ],
        messages: vec![
            entry(key(GROUP, "3EB02222000B", false, Some("100000000003@lid")), 10, |m| {
                m.message_stub_type = Some(StubType::GroupParticipantAdd as i32);
                m.message_stub_parameters = vec!["100000000004@lid".into()];
            }),
            entry(key(GROUP, "3EB03333000C", false, Some("100000000003@lid")), 11, |m| {
                m.message = Some(text("Saturday at 9?"));
                m.push_name = Some("Rui".into());
            }),
            entry(key(GROUP, "3EB04444000D", false, Some("100000000004@lid")), 12, |m| {
                m.message_stub_type = Some(StubType::CallMissedGroupVoice as i32);
            }),
            entry(key(GROUP, "3EB05555000E", true, None), 13, |m| {
                m.message = Some(protocol(Type::EphemeralSetting, &|p| p.ephemeral_expiration = Some(86_400)));
            }),
        ],
        ..Default::default()
    };
    let behind = Conversation {
        id: "351910000005@s.whatsapp.net".into(),
        unread_count: Some(40),
        unread_mention_count: Some(0),
        marked_as_unread: Some(true),
        messages: vec![entry(key("351910000005@s.whatsapp.net", "3EB06666000F", false, None), 14, |m| {
            m.message = Some(text("one of forty"));
        })],
        ..Default::default()
    };
    let empty = Conversation {
        id: "120363000000000002@g.us".into(),
        name: Some("Quiet".into()),
        archived: Some(true),
        conversation_timestamp: Some(AT + 100),
        ..Default::default()
    };
    let head = HistorySync {
        sync_type: 2,
        chunk_order: Some(7),
        progress: Some(42),
        status_v3_messages: vec![WebMessageInfo {
            key: key("status@broadcast", "3EB07777STAT", false, Some(ANA)),
            message: Some(text("a status")),
            message_timestamp: Some(AT),
            ..Default::default()
        }],
        pushnames: vec![
            Pushname { id: Some("351910000003@s.whatsapp.net".into()), pushname: Some("Rui".into()) },
            Pushname { id: Some("351910000004@s.whatsapp.net".into()), pushname: Some("-".into()) },
        ],
        phone_number_to_lid_mappings: vec![PhoneNumberToLidMapping {
            pn_jid: Some("351910000003@s.whatsapp.net".into()),
            lid_jid: Some("100000000003@lid".into()),
        }],
        global_settings: Some(GlobalSettings { font_size: Some(2), ..Default::default() }),
        past_participants: vec![PastParticipants { group_jid: Some(GROUP.into()), past_participants: Vec::new() }],
        ..Default::default()
    };
    let conversations = vec![direct, group, behind, empty];
    // Encoded as the phone might: the head's fields, the conversations, and fields jaw doesn't know
    // in the blob, in a conversation and in a message that doesn't become a row.
    let mut data = head.encode_to_vec();
    unknown(&mut data);
    for (n, conversation) in conversations.iter().enumerate() {
        let mut encoded = conversation.encode_to_vec();
        if n == 0 {
            unknown(&mut encoded);
            let mut shell = HistorySyncMsg::default().encode_to_vec();
            unknown(&mut shell);
            encode_key(MESSAGES, WireType::LengthDelimited, &mut encoded);
            encode_varint(shell.len() as u64, &mut encoded);
            encoded.extend_from_slice(&shell);
        }
        encode_key(CONVERSATIONS, WireType::LengthDelimited, &mut data);
        encode_varint(encoded.len() as u64, &mut data);
        data.extend_from_slice(&encoded);
    }
    let mut history = head;
    history.conversations = conversations;
    history.conversations[0].messages.push(HistorySyncMsg::default());
    (history, data)
}

/// Every change walking `data` hands over, in the chunks it hands them in, and what it found.
fn walked(data: &[u8], chunk: usize) -> Result<(Vec<Vec<Change>>, Walked), Unwalked> {
    let mut chunks = Vec::new();
    let walked = walk(data, &me(), chunk, |changes| {
        chunks.push(changes);
        true
    })?;
    Ok((chunks, walked))
}

fn shown(changes: impl IntoIterator<Item = Change>) -> Vec<String> {
    changes.into_iter().map(|change| format!("{change:?}")).collect()
}

#[test]
fn a_blob_walked_a_message_at_a_time_gives_the_changes_and_keeps_the_rest_decoding_it_whole_did() {
    let (history, data) = varied();
    assert_eq!(HistorySync::decode(data.as_slice()).unwrap(), history, "the blob as encoded is the one built");
    let parts = decoded_whole::blob(&history, &me());
    let rest = decoded_whole::remainder(&history, &parts);
    let (chunks, walked) = walked(&data, 2_000).unwrap();
    assert_eq!(shown(chunks.into_iter().flatten()), shown(parts.iter().flatten().cloned()));
    assert_eq!(HistorySync::decode(walked.rest.as_slice()).unwrap(), rest, "the same rest");
    let rows = parts.iter().flatten().filter(|c| matches!(c, Change::Message(_))).count();
    assert_eq!(
        (walked.sync_type, walked.chunk_order, walked.progress, walked.conversations, walked.messages),
        (2, Some(7), Some(42), 4, rows)
    );
    // What became a row keeps its raw there, and only that leaves the rest.
    let kept: Vec<&str> = rest.conversations[0]
        .messages
        .iter()
        .map(|entry| entry.message.as_ref().and_then(|m| m.key.id.as_deref()).unwrap_or("(none)"))
        .collect();
    assert_eq!(kept, ["3EB0CCCC0003", "3EB0DDDD0004", "3EB0EEEE0005", "3EB0FFFF0006", "(none)", "", "(none)"]);
}

#[test]
fn the_rest_of_a_blob_keeps_the_fields_this_jawd_doesnt_know_at_every_level() {
    let (_, data) = varied();
    let (_, walked) = walked(&data, 2_000).unwrap();
    let mut field = Vec::new();
    unknown(&mut field);
    let found = walked.rest.windows(field.len()).filter(|w| *w == field.as_slice()).count();
    assert_eq!(found, 3, "in the blob, in a conversation, and in a message that didn't become a row");
    let prost = HistorySync::decode(data.as_slice()).unwrap().encode_to_vec();
    assert_eq!(prost.windows(field.len()).filter(|w| *w == field.as_slice()).count(), 0, "which prost drops");
}

#[test]
fn a_blob_comes_in_chunks_of_about_the_size_asked_in_order() {
    let (_, data) = varied();
    let (whole, _) = walked(&data, usize::MAX).unwrap();
    for chunk in [1, 2, 7] {
        let (chunks, walked) = walked(&data, chunk).unwrap();
        assert!(chunks.len() > 2, "{} chunks of {chunk}", chunks.len());
        for handed in &chunks[..chunks.len() - 1] {
            assert!(
                handed.len() >= chunk && handed.len() < chunk + 8,
                "{} changes for chunks of {chunk}",
                handed.len()
            );
        }
        assert_eq!(shown(chunks.into_iter().flatten()), shown(whole.iter().flatten().cloned()));
        assert_eq!(walked.conversations, 4);
    }
}

#[test]
fn a_walk_stops_when_its_changes_are_no_longer_taken() {
    let (_, data) = varied();
    let mut handed = 0;
    let walked = walk(&data, &me(), 3, |_| {
        handed += 1;
        handed < 2
    });
    assert!(matches!(walked, Err(Unwalked::Stopped)), "{walked:?}");
    assert_eq!(handed, 2, "nothing after the chunk refused");
}

#[test]
fn a_blob_walks_exactly_when_prost_decodes_it_whole_and_one_that_doesnt_hands_nothing_over() {
    let (_, data) = varied();
    let mut blobs: Vec<Vec<u8>> = (0..data.len()).map(|len| data[..len].to_vec()).collect();
    for at in 0..data.len() {
        for flip in [0x01, 0x08, 0x80] {
            let mut damaged = data.clone();
            damaged[at] ^= flip;
            blobs.push(damaged);
        }
    }
    let mut refused = 0;
    for blob in &blobs {
        let mut handed = Vec::new();
        let walked = walk(blob, &me(), 2_000, |changes| {
            handed.extend(changes);
            true
        });
        match HistorySync::decode(blob.as_slice()) {
            Ok(history) => {
                let walked = walked.unwrap_or_else(|e| panic!("prost decodes it whole, and the walk says {e:?}"));
                let parts = decoded_whole::blob(&history, &me());
                let rest = decoded_whole::remainder(&history, &parts);
                assert_eq!(shown(handed), shown(parts.iter().flatten().cloned()));
                assert_eq!(HistorySync::decode(walked.rest.as_slice()).unwrap(), rest);
            }
            Err(_) => {
                assert!(
                    matches!(walked, Err(Unwalked::Undecodable(_))),
                    "prost refuses it, and the walk says {walked:?}"
                );
                assert!(handed.is_empty(), "nothing of a blob that doesn't decode is handed over");
                refused += 1;
            }
        }
    }
    assert!(refused > data.len(), "{refused} of {} damaged blobs refused", blobs.len());
}
