use std::collections::HashSet;

use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::web_message_info::StubType;

use super::*;
use crate::store::media::{Keys, MediaCopy};

const ANA: &str = "351910000002@s.whatsapp.net";
const GROUP: &str = "120363000000000001@g.us";
const AT: i64 = 1_792_000_000_000;

fn me() -> Me {
    Me::new(["351910000001:12@s.whatsapp.net".to_string(), "100000000009@lid".to_string()])
}

fn info(chat: &str, message: e2e::Message) -> WebMessageInfo {
    WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(false),
            id: Some("3EB0AA".into()),
            participant: (!is_person(chat)).then(|| "100000000001@lid".into()),
        },
        message: Some(message),
        message_timestamp: Some((AT / 1000) as u64),
        push_name: Some("Ana".into()),
        ..Default::default()
    }
}

/// The one message row converting `m` from Ana gives.
fn row(m: e2e::Message) -> Message {
    rows(&info(ANA, m), true).into_iter().next().expect("a message row")
}

fn rows(info: &WebMessageInfo, live: bool) -> Vec<Message> {
    message(info, &me(), live)
        .into_iter()
        .filter_map(|c| match c {
            Change::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn amends(info: &WebMessageInfo) -> Vec<Amend> {
    message(info, &me(), true)
        .into_iter()
        .filter_map(|c| match c {
            Change::Amend(a) => Some(a),
            _ => None,
        })
        .collect()
}

fn context(f: impl FnOnce(&mut e2e::ContextInfo)) -> Option<Box<e2e::ContextInfo>> {
    let mut context = e2e::ContextInfo::default();
    f(&mut context);
    Some(Box::new(context))
}

fn key(id: &str) -> MessageKey {
    MessageKey { remote_jid: Some(ANA.into()), from_me: Some(true), id: Some(id.into()), participant: None }
}

#[test]
fn text_is_kept_with_its_spans_its_sender_and_its_raw_protobuf() {
    let m = row(e2e::Message { conversation: Some("did you get the *photos*?".into()), ..Default::default() });
    assert_eq!((m.kind, m.chat.as_str(), m.id.as_str(), m.at), ("text", ANA, "3EB0AA", AT));
    assert_eq!((m.sender.as_deref(), m.mine, m.status), (Some(ANA), false, None));
    assert_eq!(m.text.as_deref(), Some("did you get the *photos*?"));
    assert_eq!(m.summary, "did you get the photos?");
    assert_eq!(
        m.spans.as_deref(),
        Some(r#"[{"t":"text","s":"did you get the "},{"t":"bold","s":"photos"},{"t":"text","s":"?"}]"#)
    );
    assert!(m.live, "a live message says it's live");
    let raw = unpack(&m);
    assert_eq!(raw.message.unwrap().conversation.as_deref(), Some("did you get the *photos*?"));
}

fn unpack(m: &Message) -> WebMessageInfo {
    let packed = m.raw.as_ref().expect("a raw protobuf");
    WebMessageInfo::decode(crate::store::unpack_for_tests(packed).as_slice()).unwrap()
}

#[test]
fn extended_text_marks_mentions_and_links_and_summarises_its_quote() {
    let quoted = e2e::Message { conversation: Some("Saturday at 9?".into()), ..Default::default() };
    let m = row(e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some("@100000000009 yes, see https://jaw.dev".into()),
            matched_text: Some("https://jaw.dev".into()),
            title: Some("jaw".into()),
            context_info: context(|c| {
                c.stanza_id = Some("3EB0QQ".into());
                c.participant = Some("351910000003:2@s.whatsapp.net".into());
                c.quoted_message = Some(Box::new(quoted));
                c.mentioned_jid = vec!["100000000009@lid".into()];
            }),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!(m.kind, "text");
    assert!(m.mentions_me, "a mention of the account's LID is a mention of it");
    let spans: serde_json::Value = serde_json::from_str(m.spans.as_deref().unwrap()).unwrap();
    assert_eq!(spans[0]["t"], "mention");
    assert_eq!(spans[0]["person"], "100000000009@lid");
    assert_eq!(spans[2]["url"], "https://jaw.dev");
    let reply: serde_json::Value = serde_json::from_str(m.reply.as_deref().unwrap()).unwrap();
    assert_eq!(
        reply,
        serde_json::json!({"id": "3EB0QQ", "from": "351910000003@s.whatsapp.net", "summary": "Saturday at 9?"})
    );
    let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail["link"]["title"], "jaw");
}

#[test]
fn a_link_preview_is_its_absolute_url_title_description_and_its_thumbnails_hash() {
    let previewed = |matched: &str, title: Option<&str>, thumbnail: Option<Vec<u8>>| {
        row(e2e::Message {
            extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
                text: Some(format!("vê {matched}")),
                matched_text: Some(matched.into()),
                title: title.map(Into::into),
                description: Some("  ".into()),
                jpeg_thumbnail: thumbnail,
                ..Default::default()
            })),
            ..Default::default()
        })
    };
    let m = previewed("www.jaw.dev", Some("Jaw"), Some(b"a thumbnail".to_vec()));
    let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
    assert_eq!(
        detail["link"],
        serde_json::json!({
            "url": "https://www.jaw.dev",
            "title": "Jaw",
            "description": null,
            "thumbnail": crate::store::thumbnails::sha256(b"a thumbnail"),
        })
    );
    assert_eq!(link_thumbnail(&e2e::Message::default()), None);
    let only_thumbnail = previewed("https://jaw.dev", None, Some(vec![1]));
    assert!(only_thumbnail.detail.unwrap().contains("\"thumbnail\""), "a thumbnail alone is a preview");
    assert_eq!(previewed("https://jaw.dev", None, None).detail, None, "nothing to show, no preview");
}

#[test]
fn a_photo_is_its_caption_or_photo() {
    let image = |caption: Option<&str>| e2e::Message {
        image_message: Some(Box::new(e2e::ImageMessage {
            caption: caption.map(str::to_string),
            mimetype: Some("image/jpeg".into()),
            width: Some(1600),
            height: Some(1200),
            file_length: Some(204_800),
            ..Default::default()
        })),
        ..Default::default()
    };
    let bare = row(image(None));
    assert_eq!((bare.kind, bare.summary.as_str(), bare.text), ("image", "Photo", None));
    let detail: serde_json::Value = serde_json::from_str(bare.detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail["media"], serde_json::json!({"mime": "image/jpeg", "bytes": 204800, "w": 1600, "h": 1200}));
    let captioned = row(image(Some("the lake")));
    assert_eq!((captioned.summary.as_str(), captioned.text.as_deref()), ("the lake", Some("the lake")));
}

#[test]
fn videos_gifs_and_video_notes_are_told_apart_with_their_length() {
    let video = |gif: bool, seconds: u32| e2e::VideoMessage {
        gif_playback: Some(gif),
        seconds: Some(seconds),
        ..Default::default()
    };
    let m = row(e2e::Message { video_message: Some(Box::new(video(false, 75))), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("video", "Video, 1:15"));
    let m = row(e2e::Message { video_message: Some(Box::new(video(true, 3))), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("gif", "GIF"));
    let m = row(e2e::Message { ptv_message: Some(Box::new(video(false, 9))), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("video_note", "Video message, 0:09"));
}

#[test]
fn voice_messages_and_audio_say_how_long_they_are() {
    let audio = |ptt: bool, seconds: u32| e2e::Message {
        audio_message: Some(Box::new(e2e::AudioMessage {
            ptt: Some(ptt),
            seconds: Some(seconds),
            ..Default::default()
        })),
        ..Default::default()
    };
    let m = row(audio(true, 14));
    assert_eq!((m.kind, m.summary.as_str()), ("voice", "Voice message, 0:14"));
    let m = row(audio(false, 3_725));
    assert_eq!((m.kind, m.summary.as_str()), ("audio", "Audio, 1:02:05"));
}

#[test]
fn a_document_is_found_by_its_name_and_its_caption() {
    let m = row(e2e::Message {
        document_message: Some(Box::new(e2e::DocumentMessage {
            file_name: Some("tickets.pdf".into()),
            page_count: Some(2),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str(), m.text.as_deref()), ("document", "tickets.pdf", Some("tickets.pdf")));
    let captioned = row(e2e::Message {
        document_with_caption_message: Some(Box::new(e2e::FutureProofMessage {
            message: Some(Box::new(e2e::Message {
                document_message: Some(Box::new(e2e::DocumentMessage {
                    file_name: Some("tickets.pdf".into()),
                    caption: Some("for Saturday".into()),
                    ..Default::default()
                })),
                ..Default::default()
            })),
        })),
        ..Default::default()
    });
    assert_eq!(captioned.summary, "for Saturday");
    assert_eq!(captioned.text.as_deref(), Some("for Saturday\ntickets.pdf"));
}

#[test]
fn stickers_locations_and_contacts_have_a_line_each() {
    let m = row(e2e::Message { sticker_message: Some(Box::new(e2e::StickerMessage::default())), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("sticker", "Sticker"));
    let m = row(e2e::Message {
        location_message: Some(Box::new(e2e::LocationMessage {
            degrees_latitude: Some(38.7),
            degrees_longitude: Some(-9.1),
            name: Some("Miradouro".into()),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("location", "Location: Miradouro"));
    let m = row(e2e::Message {
        live_location_message: Some(Box::new(e2e::LiveLocationMessage::default())),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("live_location", "Live location"));
    let card = |name: &str| e2e::ContactMessage {
        display_name: Some(name.into()),
        vcard: Some("BEGIN:VCARD".into()),
        ..Default::default()
    };
    let m = row(e2e::Message { contact_message: Some(Box::new(card("Rui"))), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("contact", "Contact: Rui"));
    let m = row(e2e::Message {
        contacts_array_message: Some(Box::new(e2e::ContactsArrayMessage {
            contacts: vec![card("Rui"), card("Sofia")],
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str(), m.text.as_deref()), ("contact", "2 contacts", Some("Rui, Sofia")));
}

#[test]
fn a_poll_of_any_version_is_its_question_and_options() {
    let poll = e2e::PollCreationMessage {
        name: Some("Dinner where?".into()),
        options: vec![
            e2e::poll_creation_message::Option { option_name: Some("Porto".into()), ..Default::default() },
            e2e::poll_creation_message::Option { option_name: Some("Lisbon".into()), ..Default::default() },
        ],
        selectable_options_count: Some(1),
        ..Default::default()
    };
    for m in [
        e2e::Message { poll_creation_message: Some(Box::new(poll.clone())), ..Default::default() },
        e2e::Message { poll_creation_message_v3: Some(Box::new(poll.clone())), ..Default::default() },
    ] {
        let m = row(m);
        assert_eq!((m.kind, m.summary.as_str()), ("poll", "Poll: Dinner where?"));
        assert_eq!(m.text.as_deref(), Some("Dinner where?\nPorto\nLisbon"));
        let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
        assert_eq!(detail["poll"]["options"][1]["name"], "Lisbon");
        assert_eq!((&detail["poll"]["max"], &detail["poll"]["voters"]), (&1.into(), &0.into()));
        assert_eq!(m.poll, ["Porto", "Lisbon"], "its options kept, to count the votes in each");
    }
    // 0, or more than it has, is any of them.
    for selectable in [Some(0), Some(3), None] {
        let any = e2e::PollCreationMessage { selectable_options_count: selectable, ..poll.clone() };
        let m = row(e2e::Message { poll_creation_message: Some(Box::new(any)), ..Default::default() });
        let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
        assert_eq!(detail["poll"]["max"], 2, "{selectable:?}");
    }
}

#[test]
fn the_votes_history_keeps_on_a_poll_are_each_its_voters_with_their_time_and_their_own_id() {
    let poll = e2e::PollCreationMessage {
        name: Some("Dinner where?".into()),
        options: vec![e2e::poll_creation_message::Option { option_name: Some("Porto".into()), ..Default::default() }],
        ..Default::default()
    };
    let mut kept = info(GROUP, e2e::Message { poll_creation_message_v3: Some(Box::new(poll)), ..Default::default() });
    let vote = |from_me: bool, participant: &str, id: &str, at: i64| PollUpdate {
        poll_update_message_key: Some(MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(from_me),
            id: Some(id.into()),
            participant: (!participant.is_empty()).then(|| participant.into()),
        }),
        vote: Some(e2e::PollVoteMessage { selected_options: vec![vec![0xab, 0x01]] }),
        sender_timestamp_ms: Some(at),
        ..Default::default()
    };
    kept.poll_updates = vec![vote(false, "100000000002@lid", "3EB0V1", AT + 5), vote(true, "", "3EB0V2", AT + 9)];
    let [rui, mine] = message(&kept, &me(), false)
        .into_iter()
        .filter_map(|c| match c {
            Change::Amend(a) => Some(a),
            _ => None,
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    assert_eq!(
        (rui.target.as_str(), rui.sender.as_deref(), rui.mine, rui.at),
        ("3EB0AA", Some("100000000002@lid"), false, AT + 5)
    );
    assert_eq!(rui.what, Amendment::Vote { options: vec!["ab01".into()], id: "3EB0V1".into() });
    assert_eq!((mine.sender.as_deref(), mine.mine), (Some("351910000001@s.whatsapp.net"), true), "yours, as you");
    assert_eq!(mine.what, Amendment::Vote { options: vec!["ab01".into()], id: "3EB0V2".into() });
}

#[test]
fn events_invites_calls_and_albums_have_a_line_each() {
    let m = row(e2e::Message {
        event_message: Some(Box::new(e2e::EventMessage {
            name: Some("Climbing".into()),
            start_time: Some(1_792_100_000),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("event", "Event: Climbing"));
    let m = row(e2e::Message {
        group_invite_message: Some(Box::new(e2e::GroupInviteMessage {
            group_name: Some("Lisbon".into()),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("invite", "Invitation to join Lisbon"));
    let m = row(e2e::Message {
        call_log_messsage: Some(e2e::CallLogMessage {
            call_outcome: Some(e2e::call_log_message::CallOutcome::Missed as i32),
            is_video: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("call", "Missed video call"));
    let m = row(e2e::Message {
        album_message: Some(Box::new(e2e::AlbumMessage {
            expected_image_count: Some(3),
            expected_video_count: Some(1),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!((m.kind, m.summary.as_str()), ("album", "Album: 3 photos and 1 video"));
}

#[test]
fn a_kind_jaw_doesnt_know_keeps_its_protobuf_and_says_so() {
    let m = row(e2e::Message { request_location_message: Some(Default::default()), ..Default::default() });
    assert_eq!((m.kind, m.summary.as_str()), ("unknown", kinds::UNKNOWN));
    assert!(unpack(&m).message.unwrap().request_location_message.is_some(), "its raw protobuf is kept whole");
}

#[test]
fn encryption_plumbing_alone_makes_no_row() {
    let m = e2e::Message {
        sender_key_distribution_message: Some(Default::default()),
        message_context_info: Some(Default::default()),
        ..Default::default()
    };
    assert!(rows(&info(GROUP, m), true).is_empty());
}

#[test]
fn device_sent_ephemeral_and_view_once_wrappers_are_unwrapped() {
    let text = e2e::Message { conversation: Some("from my laptop".into()), ..Default::default() };
    let sent = e2e::Message {
        device_sent_message: Some(Box::new(e2e::DeviceSentMessage {
            message: Some(Box::new(text)),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut mine = info(ANA, sent);
    mine.key.from_me = Some(true);
    let m = rows(&mine, true).remove(0);
    assert_eq!(
        (m.text.as_deref(), m.mine, m.sender.as_deref(), m.status),
        (Some("from my laptop"), true, None, Some("sent"))
    );

    let image = e2e::Message {
        image_message: Some(Box::new(e2e::ImageMessage {
            context_info: context(|c| c.expiration = Some(86_400)),
            ..Default::default()
        })),
        ..Default::default()
    };
    let wrapped = e2e::Message {
        ephemeral_message: Some(Box::new(e2e::FutureProofMessage {
            message: Some(Box::new(e2e::Message {
                view_once_message_v2: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(image)) })),
                ..Default::default()
            })),
        })),
        ..Default::default()
    };
    let m = row(wrapped);
    assert_eq!((m.kind, m.view_once, m.expires_at), ("image", true, Some(AT + 86_400_000)));
}

#[test]
fn edits_deletes_reactions_and_votes_amend_the_message_they_name() {
    let edit = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("3EB0OLD")),
            r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
            edited_message: Some(Box::new(e2e::Message { conversation: Some("_fixed_".into()), ..Default::default() })),
            timestamp_ms: Some(AT + 5),
            ..Default::default()
        })),
        ..Default::default()
    };
    let changes = message(&info(ANA, edit), &me(), true);
    let [Change::Person(_), Change::Edited { amend, shown }] = changes.as_slice() else {
        panic!("an edit and what it shows")
    };
    assert_eq!((amend.chat.as_str(), amend.target.as_str(), amend.at), (ANA, "3EB0OLD", AT + 5));
    let Amendment::Edit { text, spans, summary } = &amend.what else { panic!("{:?}", amend.what) };
    assert_eq!((text.as_deref(), summary.as_str()), (Some("_fixed_"), "fixed"));
    assert_eq!(spans.as_deref(), Some(r#"[{"t":"italic","s":"fixed"}]"#));
    assert_eq!((shown.id.as_str(), shown.edited_at, shown.summary.as_str()), ("3EB0OLD", Some(AT + 5), "fixed"));
    assert!(shown.raw.is_some(), "what stands in for the message keeps the edit's protobuf");

    let revoke = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("3EB0OLD")),
            r#type: Some(e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    assert_eq!(amends(&info(ANA, revoke))[0].what, Amendment::Revoke);

    let react = |text: &str| e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(key("3EB0OLD")),
            text: Some(text.into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let [amend] = amends(&info(GROUP, react("❤"))).try_into().unwrap();
    assert_eq!((amend.sender.as_deref(), amend.mine), (Some("100000000001@lid"), false));
    assert_eq!(amend.what, Amendment::React { emoji: "❤".into() });
    let mut mine = info(GROUP, react(""));
    mine.key.from_me = Some(true);
    let [amend, own] = amends(&mine).try_into().unwrap();
    assert_eq!(
        (amend.sender.as_deref(), amend.mine),
        (Some("351910000001@s.whatsapp.net"), true),
        "the account reacts as itself"
    );
    assert_eq!(
        (own.target.as_str(), own.what),
        ("3EB0AA", Amendment::DeleteForMe),
        "and its reaction's own ID is no row"
    );

    let vote = e2e::Message {
        poll_update_message: Some(e2e::PollUpdateMessage {
            poll_creation_message_key: Some(key("3EB0POLL")),
            ..Default::default()
        }),
        ..Default::default()
    };
    let [amend] = amends(&info(GROUP, vote)).try_into().unwrap();
    assert_eq!(amend.what, Amendment::Keep { kind: "vote".into() });
    assert!(rows(&info(GROUP, e2e::Message::default()), true).is_empty());
}

#[test]
fn a_reaction_is_timed_by_its_senders_clock_to_the_millisecond() {
    let react = e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(key("3EB0OLD")),
            text: Some("❤".into()),
            sender_timestamp_ms: Some(AT + 750),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(amends(&info(ANA, react))[0].at, AT + 750, "not the second its envelope says");
}

#[test]
fn your_reactions_edits_and_deletions_from_your_other_devices_leave_their_own_ids_no_row_to_wait_for() {
    let edit = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("3EB0OLD")),
            r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
            edited_message: Some(Box::new(e2e::Message { conversation: Some("fixed".into()), ..Default::default() })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let revoke = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("3EB0OLD")),
            r#type: Some(e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    for (what, message) in [("an edit", edit), ("a deletion", revoke)] {
        let mut from_phone = info(ANA, message.clone());
        from_phone.key.from_me = Some(true);
        let own = amends(&from_phone).into_iter().find(|a| a.target == "3EB0AA");
        assert_eq!(own.map(|a| a.what), Some(Amendment::DeleteForMe), "{what} of yours");
        assert!(amends(&info(ANA, message)).iter().all(|a| a.target != "3EB0AA"), "{what} of Ana's");
    }
}

#[test]
fn a_message_history_keeps_as_deleted_for_everyone_is_a_deleted_row_saying_who_deleted_it() {
    let mut stub = info(ANA, e2e::Message::default());
    stub.message = None;
    stub.message_stub_type = Some(StubType::Revoke as i32);
    let changes = message(&stub, &me(), false);
    let [Change::Person(_), Change::Amend(revoke), Change::Message(row)] = changes.as_slice() else {
        panic!("the revoke, then the row: {changes:?}")
    };
    assert_eq!((revoke.target.as_str(), &revoke.what), ("3EB0AA", &Amendment::Revoke), "a copy jaw has is revoked");
    assert_eq!(
        (row.kind, row.deleted, row.text.as_deref(), row.summary.as_str()),
        ("text", true, None, "This message was deleted")
    );
    stub.key.from_me = Some(true);
    assert_eq!(rows(&stub, false)[0].summary, "You deleted this message");
}

#[test]
fn a_disappearing_timer_sets_the_chat_and_leaves_a_line() {
    let setting = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            r#type: Some(e2e::protocol_message::Type::EphemeralSetting as i32),
            ephemeral_expiration: Some(604_800),
            ..Default::default()
        })),
        ..Default::default()
    };
    let changes = message(&info(ANA, setting), &me(), true);
    assert!(changes.iter().any(|c| matches!(c, Change::Chat(chat) if chat.ephemeral == Some(604_800))));
    let line = rows(&info(ANA, e2e::Message::default()), true);
    assert!(line.is_empty());
    let m = changes.iter().find_map(|c| if let Change::Message(m) = c { Some(m) } else { None }).unwrap();
    assert_eq!((m.kind, m.summary.as_str()), ("system", "Disappearing messages turned on: 7 days"));
}

#[test]
fn history_has_stub_lines_and_reactions_and_says_it_isnt_live() {
    let mut stub = info(GROUP, e2e::Message::default());
    stub.message = None;
    stub.message_stub_type = Some(StubType::GroupParticipantAdd as i32);
    stub.message_stub_parameters = vec!["351910000003@s.whatsapp.net".into()];
    let m = rows(&stub, false).remove(0);
    assert_eq!((m.kind, m.summary.as_str()), ("system", "+351910000003 added"));
    let mut call = stub.clone();
    call.message_stub_type = Some(StubType::CallMissedVoice as i32);
    assert_eq!(rows(&call, false)[0].kind, "call");

    let mut text = info(ANA, e2e::Message { conversation: Some("hi".into()), ..Default::default() });
    text.reactions = vec![wa_proto::wa_web_protobufs_web::Reaction {
        key: Some(MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(true),
            id: Some("R1".into()),
            participant: None,
        }),
        text: Some("👍".into()),
        sender_timestamp_ms: Some(AT + 9),
        ..Default::default()
    }];
    let changes = message(&text, &me(), false);
    let m = changes.iter().find_map(|c| if let Change::Message(m) = c { Some(m) } else { None }).unwrap();
    assert!(!m.live, "history isn't live");
    let amend = changes.iter().find_map(|c| if let Change::Amend(a) = c { Some(a) } else { None }).unwrap();
    assert_eq!((amend.target.as_str(), amend.mine, amend.at), ("3EB0AA", true, AT + 9));
}

#[test]
fn a_push_name_names_the_sender_and_a_live_lid_chat_goes_under_its_number() {
    let changes =
        message(&info(GROUP, e2e::Message { conversation: Some("olá".into()), ..Default::default() }), &me(), true);
    let Change::Person(person) = &changes[0] else { panic!("the sender's push name first") };
    assert_eq!(
        (person.jids.as_slice(), person.push_name.as_deref()),
        (&["100000000001@lid".to_string()][..], Some("Ana"))
    );

    let live = pb::Message {
        info: Some(pb::MessageInfo {
            chat: "100000000001@lid".into(),
            sender: "100000000001@lid".into(),
            sender_alt: "351910000002:3@s.whatsapp.net".into(),
            id: "3EB0LL".into(),
            timestamp_ms: AT + 999,
            push_name: "Ana".into(),
            ..Default::default()
        }),
        raw: e2e::Message { conversation: Some("hi".into()), ..Default::default() }.encode_to_vec(),
        ..Default::default()
    };
    let web = web_message(&live).unwrap();
    assert_eq!((web.key.remote_jid.as_deref(), web.key.participant.as_deref()), (Some(ANA), None));
    assert_eq!(web.message_timestamp, Some((AT / 1000) as u64));
    let m = rows(&web, true).remove(0);
    assert_eq!((m.chat.as_str(), m.sender.as_deref()), (ANA, Some(ANA)));
}

const COMMUNITY: &str = "120363000000000009@g.us";
const ANNOUNCEMENTS: &str = "120363000000000008@g.us";

/// What history keeps of `stub` in `chat`, with `parameters`, from 100000000001@lid in a group.
fn stub(chat: &str, stub: StubType, parameters: &[&str]) -> WebMessageInfo {
    let mut info = info(chat, e2e::Message::default());
    info.message = None;
    info.message_stub_type = Some(stub as i32);
    info.message_stub_parameters = parameters.iter().map(|p| p.to_string()).collect();
    info
}

#[test]
fn community_events_and_the_other_stubs_history_brings_read_as_what_happened() {
    let (natal, bairro) = ("Festa de Natal", "Bairro de Alvalade");
    let cases: &[(&str, StubType, &[&str], &str)] = &[
        (
            GROUP,
            StubType::GroupParticipantLinkedGroupJoin,
            &["351910000003@s.whatsapp.net"],
            "+351910000003 joined from the community",
        ),
        (
            GROUP,
            StubType::GeneralChatAdd,
            &["linked_group_join", "Geral", "true"],
            "100000000001 joined from the community",
        ),
        (COMMUNITY, StubType::CommunityLinkSubGroup, &[GROUP, natal], "Group added to the community: Festa de Natal"),
        (
            ANNOUNCEMENTS,
            StubType::CommunityLinkSiblingGroup,
            &[GROUP, natal],
            "Group added to the community: Festa de Natal",
        ),
        (
            COMMUNITY,
            StubType::CommunityUnlinkSubGroup,
            &[GROUP, natal],
            "Group removed from the community: Festa de Natal",
        ),
        (
            ANNOUNCEMENTS,
            StubType::CommunityUnlinkSiblingGroup,
            &[GROUP, natal],
            "Group removed from the community: Festa de Natal",
        ),
        (
            GROUP,
            StubType::CommunityLinkParentGroupRich,
            &[COMMUNITY, bairro],
            "This group was added to the community Bairro de Alvalade",
        ),
        (
            GROUP,
            StubType::CommunityLinkParentGroup,
            &[COMMUNITY, bairro],
            "This group was added to the community Bairro de Alvalade",
        ),
        (
            GROUP,
            StubType::CommunityUnlinkParentGroup,
            &[COMMUNITY, bairro],
            "This group was removed from the community Bairro de Alvalade",
        ),
        (
            ANNOUNCEMENTS,
            StubType::CommunityParentGroupDeleted,
            &[bairro],
            "The community Bairro de Alvalade was deactivated",
        ),
        (ANNOUNCEMENTS, StubType::CommunityCreate, &[COMMUNITY, bairro], "Community created: Bairro de Alvalade"),
        (
            ANNOUNCEMENTS,
            StubType::CommunityParticipantPromote,
            &["100000000003@lid"],
            "100000000003 made community admin",
        ),
        (
            ANNOUNCEMENTS,
            StubType::CommunityParticipantDemote,
            &["100000000003@lid"],
            "100000000003 no longer community admin",
        ),
        (
            ANNOUNCEMENTS,
            StubType::CommunityParticipantAddRich,
            &[COMMUNITY, bairro],
            "You were added to the community Bairro de Alvalade",
        ),
        (
            ANNOUNCEMENTS,
            StubType::SubGroupParticipantAddRich,
            &[GROUP, natal],
            "You were added to the group Festa de Natal",
        ),
        (ANNOUNCEMENTS, StubType::CagMaskedThreadCreated, &[], "A thread was started"),
        (GROUP, StubType::PinnedMessageInChat, &["351910000003@s.whatsapp.net"], "+351910000003 pinned a message"),
        (
            GROUP,
            StubType::ChangeUsername,
            &["", "ana.ribeiro", "100000000003@lid", "~Ana"],
            "Ana set their username to @ana.ribeiro",
        ),
        (
            GROUP,
            StubType::ChangeUsername,
            &["ana", "@ana.ribeiro", "100000000003@lid", "Ana Ribeiro"],
            "Ana Ribeiro changed their username to @ana.ribeiro",
        ),
        (
            ANA,
            StubType::ChangeUsername,
            &["ana.ribeiro", "", "100000000003@lid", ""],
            "100000000003 removed their username",
        ),
        (ANA, StubType::BizPrivacyModeToFb, &[""], "This business uses a secure service from Meta to manage this chat"),
        (
            ANA,
            StubType::BizPrivacyModeInitFb,
            &[""],
            "This business uses a secure service from Meta to manage this chat",
        ),
        (ANA, StubType::BizCoexPrivacyInit, &[""], "This business uses a secure service from Meta to manage this chat"),
        (
            ANA,
            StubType::BizCoexPrivacyTransition,
            &[""],
            "This business uses a secure service from Meta to manage this chat",
        ),
        (ANA, StubType::BizPrivacyModeInitBsp, &[""], "This business works with other companies to manage this chat"),
        (ANA, StubType::BizPrivacyModeToBsp, &[""], "This business works with other companies to manage this chat"),
        (ANA, StubType::BlockContact, &["true"], "You blocked this contact"),
        (ANA, StubType::BlockContact, &["false"], "You unblocked this contact"),
        (ANA, StubType::DisappearingMode, &["7776000", "100000000002@lid"], "Disappearing messages turned on: 90 days"),
        (GROUP, StubType::ChangeLimitSharing, &["true", "1", "100000000003@lid"], "Advanced chat privacy turned on"),
        (ANA, StubType::ChangeLimitSharing, &["false", "1", "100000000002@lid"], "Advanced chat privacy turned off"),
        (GROUP, StubType::GroupMembershipJoinApprovalMode, &["on", "true"], "New members need admin approval to join"),
        (
            GROUP,
            StubType::GroupMembershipJoinApprovalMode,
            &["on", "false"],
            "New members don't need admin approval to join",
        ),
        (ANNOUNCEMENTS, StubType::EventUpdated, &["Magusto"], "Event updated: Magusto"),
        (ANNOUNCEMENTS, StubType::EventCanceled, &["Magusto"], "Event cancelled: Magusto"),
        (ANA, StubType::ScheduledMessageCreated, &[], "Message scheduled"),
        ("1792000000@broadcast", StubType::BroadcastCreate, &["2"], "Broadcast list created with 2 recipients"),
        (
            ANA,
            StubType::IndividualChangeNumber,
            &["351910000004@s.whatsapp.net", "100000000002@lid"],
            "This contact changed their number",
        ),
    ];
    for &(chat, kind, parameters, said) in cases {
        let m = rows(&stub(chat, kind, parameters), false).remove(0);
        assert_eq!((m.kind, m.summary.as_str()), ("system", said), "{kind:?} {parameters:?}");
        assert_eq!(m.detail, Some(json!({ "system": said }).to_string()), "{kind:?}");
        assert!(m.raw.is_some(), "{kind:?} keeps its protobuf");
    }
    let m = rows(&stub(GROUP, StubType::GroupChangeNoFrequentlyForwarded, &[]), false).remove(0);
    assert_eq!((m.kind, m.summary.as_str()), ("system", kinds::UNKNOWN), "a stub jawd doesn't know yet says so");
}

/// The kind, text and summary of the row converting `m` from Ana gives.
fn business(m: e2e::Message) -> (&'static str, Option<String>, String) {
    let m = row(m);
    (m.kind, m.text, m.summary)
}

#[test]
fn a_businesss_templates_buttons_lists_and_answers_to_them_are_their_text() {
    use e2e::template_message::{Format, HydratedFourRowTemplate, hydrated_four_row_template::Title};
    let hydrated = HydratedFourRowTemplate {
        title: Some(Title::HydratedTitleText("Your order has shipped".into())),
        hydrated_content_text: Some("It arrives on Thursday.".into()),
        hydrated_footer_text: Some("Reply STOP to stop".into()),
        ..Default::default()
    };
    let template = e2e::TemplateMessage {
        format: Some(Format::HydratedFourRowTemplate(Box::new(hydrated.clone()))),
        ..Default::default()
    };
    assert_eq!(
        business(e2e::Message { template_message: Some(Box::new(template)), ..Default::default() }),
        (
            "text",
            Some("Your order has shipped\n\nIt arrives on Thursday.\n\nReply STOP to stop".into()),
            "Your order has shipped It arrives on Thursday. Reply STOP to stop".into()
        ),
    );
    let video_titled = HydratedFourRowTemplate {
        title: Some(Title::VideoMessage(Default::default())),
        hydrated_content_text: Some("Our new range".into()),
        ..Default::default()
    };
    let template = e2e::TemplateMessage { hydrated_template: Some(Box::new(video_titled)), ..Default::default() };
    assert_eq!(
        business(e2e::Message { template_message: Some(Box::new(template)), ..Default::default() }),
        ("text", Some("Our new range".into()), "Our new range".into()),
        "the hydrated template outside the format, with a video for its title"
    );

    let interactive = e2e::InteractiveMessage {
        header: Some(Box::new(e2e::interactive_message::Header {
            title: Some("Welcome".into()),
            ..Default::default()
        })),
        body: Some(e2e::interactive_message::Body { text: Some("Choose a plan".into()) }),
        footer: Some(Box::new(e2e::interactive_message::Footer {
            text: Some("Cancel any time".into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let expected = (
        "text",
        Some("Welcome\n\nChoose a plan\n\nCancel any time".into()),
        "Welcome Choose a plan Cancel any time".into(),
    );
    assert_eq!(
        business(e2e::Message { interactive_message: Some(Box::new(interactive.clone())), ..Default::default() }),
        expected
    );
    let template = e2e::TemplateMessage {
        format: Some(Format::InteractiveMessageTemplate(Box::new(interactive))),
        ..Default::default()
    };
    assert_eq!(business(e2e::Message { template_message: Some(Box::new(template)), ..Default::default() }), expected);

    let buttons = e2e::ButtonsMessage {
        header: Some(e2e::buttons_message::Header::Text("Delivery".into())),
        content_text: Some("When suits you?".into()),
        footer_text: Some("Pick one".into()),
        ..Default::default()
    };
    assert_eq!(
        business(e2e::Message { buttons_message: Some(Box::new(buttons)), ..Default::default() }),
        ("text", Some("Delivery\n\nWhen suits you?\n\nPick one".into()), "Delivery When suits you? Pick one".into())
    );

    let day = |title: &str| e2e::list_message::Row { title: Some(title.into()), ..Default::default() };
    let list = e2e::ListMessage {
        description: Some("Which day?".into()),
        button_text: Some("See days".into()),
        sections: vec![e2e::list_message::Section { title: None, rows: vec![day("Monday"), day("Tuesday")] }],
        ..Default::default()
    };
    let (kind, text, summary) = business(e2e::Message { list_message: Some(Box::new(list)), ..Default::default() });
    assert_eq!((kind, text.as_deref()), ("text", Some("Which day?\n\n- Monday\n- Tuesday")));
    assert!(summary.starts_with("Which day?") && summary.contains("Tuesday"), "{summary}");

    let quoted = context(|c| {
        c.stanza_id = Some("3EB0LIST".into());
        c.participant = Some(ANA.into());
    });
    let chose =
        e2e::ListResponseMessage { title: Some("Tuesday".into()), context_info: quoted.clone(), ..Default::default() };
    let m = row(e2e::Message { list_response_message: Some(Box::new(chose)), ..Default::default() });
    assert_eq!((m.kind, m.text.as_deref()), ("text", Some("Tuesday")));
    assert!(m.reply.as_deref().is_some_and(|r| r.contains("3EB0LIST")), "an answer replies to what it answers");
    let pressed = e2e::TemplateButtonReplyMessage {
        selected_display_text: Some("Track my order".into()),
        context_info: quoted.clone(),
        ..Default::default()
    };
    let m = row(e2e::Message { template_button_reply_message: Some(Box::new(pressed)), ..Default::default() });
    assert_eq!((m.text.as_deref(), m.reply.is_some()), (Some("Track my order"), true));
    let pressed = e2e::ButtonsResponseMessage {
        response: Some(e2e::buttons_response_message::Response::SelectedDisplayText("Morning".into())),
        context_info: quoted,
        ..Default::default()
    };
    let m = row(e2e::Message { buttons_response_message: Some(Box::new(pressed)), ..Default::default() });
    assert_eq!((m.text.as_deref(), m.reply.is_some()), (Some("Morning"), true));
    let answered = e2e::InteractiveResponseMessage {
        body: Some(e2e::interactive_response_message::Body { text: Some("Plan B".into()), ..Default::default() }),
        ..Default::default()
    };
    assert_eq!(
        business(e2e::Message { interactive_response_message: Some(Box::new(answered)), ..Default::default() }),
        ("text", Some("Plan B".into()), "Plan B".into())
    );
}

#[test]
fn meta_ais_answers_are_their_text_and_so_are_their_edits() {
    use wa_proto::waai_common_deprecated::{AiRichResponseSubMessage, AiRichResponseSubMessageType};
    let part = |text: &str| AiRichResponseSubMessage {
        message_type: Some(AiRichResponseSubMessageType::AiRichResponseText as i32),
        message_text: Some(text.into()),
        ..Default::default()
    };
    let answer = e2e::Message {
        rich_response_message: Some(Box::new(e2e::AiRichResponseMessage {
            submessages: vec![part("Lisbon is the capital."), part("It has seven hills.")],
            ..Default::default()
        })),
        ..Default::default()
    };
    let m = row(answer.clone());
    let said = "Lisbon is the capital. It has seven hills.";
    assert_eq!(
        (m.kind, m.text.as_deref(), m.summary.as_str()),
        ("text", Some("Lisbon is the capital.\n\nIt has seven hills."), said)
    );
    let edit = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("3EB0AI")),
            r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
            edited_message: Some(Box::new(answer)),
            timestamp_ms: Some(AT + 5),
            ..Default::default()
        })),
        ..Default::default()
    };
    let changes = message(&info(ANA, edit), &me(), false);
    let Some(Change::Edited { shown, .. }) = changes.iter().find(|c| matches!(c, Change::Edited { .. })) else {
        panic!("an edit: {changes:?}")
    };
    assert_eq!((shown.kind, shown.summary.as_str()), ("text", said), "what stands in for it");
}

#[test]
fn a_comment_on_an_announcement_is_its_message_replying_to_the_announcement() {
    let comment = e2e::Message {
        comment_message: Some(Box::new(e2e::CommentMessage {
            message: Some(Box::new(e2e::Message { conversation: Some("count me in".into()), ..Default::default() })),
            target_message_key: Some(MessageKey {
                remote_jid: Some(ANNOUNCEMENTS.into()),
                from_me: Some(false),
                id: Some("3EB0NEWS".into()),
                participant: Some("100000000004@lid".into()),
            }),
        })),
        ..Default::default()
    };
    let m = rows(&info(ANNOUNCEMENTS, comment), false).remove(0);
    assert_eq!((m.kind, m.text.as_deref()), ("text", Some("count me in")));
    let reply: serde_json::Value = serde_json::from_str(m.reply.as_deref().expect("a reply")).unwrap();
    assert_eq!(reply, json!({ "id": "3EB0NEWS", "from": "100000000004@lid", "summary": "" }));
}

#[test]
fn shared_history_placeholders_and_status_notices_are_lines() {
    let metadata = || e2e::MessageHistoryMetadata { message_count: Some(50), ..Default::default() };
    let notice = e2e::Message {
        message_history_notice: Some(Box::new(e2e::MessageHistoryNotice {
            message_history_metadata: Some(metadata()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let bundle = e2e::Message {
        message_history_bundle: Some(Box::new(e2e::MessageHistoryBundle {
            message_history_metadata: Some(metadata()),
            ..Default::default()
        })),
        ..Default::default()
    };
    for m in [notice, bundle] {
        let m = rows(&info(GROUP, m), false).remove(0);
        assert_eq!((m.kind, m.summary.as_str()), ("system", "50 recent messages shared with new members"));
    }
    let placeholder = e2e::Message { placeholder_message: Some(Default::default()), ..Default::default() };
    assert_eq!(
        (row(placeholder.clone()).kind, row(placeholder).summary.as_str()),
        ("system", "Open WhatsApp on your phone to see this message")
    );

    let mention = e2e::Message {
        status_mention_message: Some(Box::new(e2e::FutureProofMessage {
            message: Some(Box::new(e2e::Message {
                protocol_message: Some(Box::new(e2e::ProtocolMessage {
                    key: Some(MessageKey {
                        remote_jid: Some("status@broadcast".into()),
                        from_me: Some(false),
                        id: Some("3EB0STATUS".into()),
                        participant: None,
                    }),
                    r#type: Some(e2e::protocol_message::Type::StatusMentionMessage as i32),
                    ..Default::default()
                })),
                ..Default::default()
            })),
        })),
        ..Default::default()
    };
    let response = e2e::Message { status_notification_message: Some(Default::default()), ..Default::default() };
    for (message, theirs, yours) in [
        (mention, "Mentioned you in their status", "You mentioned them in your status"),
        (response, "Responded to your status", "You responded to their status"),
    ] {
        let m = rows(&info(ANA, message.clone()), false).remove(0);
        assert_eq!((m.kind, m.summary.as_str()), ("system", theirs));
        let mut mine = info(ANA, message);
        mine.key.from_me = Some(true);
        assert_eq!(rows(&mine, false).remove(0).summary, yours);
    }
}

#[test]
fn an_invitation_to_follow_or_run_a_channel_is_an_invite() {
    let follow = e2e::Message {
        newsletter_follower_invite_message_v2: Some(Box::new(e2e::NewsletterFollowerInviteMessage {
            newsletter_jid: Some("120363000000000007@newsletter".into()),
            newsletter_name: Some("Jaw news".into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let m = row(follow);
    assert_eq!((m.kind, m.summary.as_str()), ("invite", "Invitation to follow Jaw news"));
    let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail, json!({ "invite": { "channel": "120363000000000007@newsletter", "name": "Jaw news" } }));
    let admin = e2e::Message {
        newsletter_admin_invite_message: Some(Box::new(e2e::NewsletterAdminInviteMessage {
            newsletter_jid: Some("120363000000000007@newsletter".into()),
            newsletter_name: Some("Jaw news".into()),
            caption: Some("help me run it?".into()),
            invite_expiration: Some(1_792_600_000),
            ..Default::default()
        })),
        ..Default::default()
    };
    let m = row(admin);
    assert_eq!((m.kind, m.text.as_deref(), m.summary.as_str()), ("invite", Some("help me run it?"), "help me run it?"));
    let detail: serde_json::Value = serde_json::from_str(m.detail.as_deref().unwrap()).unwrap();
    assert_eq!(detail["invite"]["expires"], json!(1_792_600_000_000_i64));
    assert_eq!(detail["invite"]["admin"], json!(true));
}

#[test]
fn a_status_shared_with_a_group_is_what_it_shows() {
    let status = e2e::Message {
        group_status_message_v2: Some(Box::new(e2e::FutureProofMessage {
            message: Some(Box::new(e2e::Message {
                image_message: Some(Box::new(e2e::ImageMessage {
                    caption: Some("sunset".into()),
                    ..Default::default()
                })),
                ..Default::default()
            })),
        })),
        ..Default::default()
    };
    let m = rows(&info(GROUP, status), false).remove(0);
    assert_eq!((m.kind, m.text.as_deref()), ("image", Some("sunset")));
}

#[test]
fn encrypted_edits_and_answers_to_events_wait_for_their_message_and_make_no_row() {
    let target = MessageKey {
        remote_jid: Some(GROUP.into()),
        from_me: Some(true),
        id: Some("3EB0EVENT".into()),
        participant: Some("100000000001@lid".into()),
    };
    let edit = e2e::Message {
        secret_encrypted_message: Some(e2e::SecretEncryptedMessage {
            target_message_key: Some(target.clone()),
            enc_payload: Some(vec![1, 2, 3]),
            secret_enc_type: Some(e2e::secret_encrypted_message::SecretEncType::MessageEdit as i32),
            ..Default::default()
        }),
        ..Default::default()
    };
    let answer = e2e::Message {
        enc_event_response_message: Some(e2e::EncEventResponseMessage {
            event_creation_message_key: Some(target),
            enc_payload: Some(vec![4, 5, 6]),
            ..Default::default()
        }),
        ..Default::default()
    };
    for (message, kind) in [(edit, "secret"), (answer, "rsvp")] {
        let theirs = info(GROUP, message.clone());
        assert!(rows(&theirs, false).is_empty(), "{kind} makes no row");
        let [kept] = amends(&theirs).try_into().unwrap();
        assert_eq!((kept.target.as_str(), &kept.what), ("3EB0EVENT", &Amendment::Keep { kind: kind.into() }));
        assert!(kept.raw.is_some(), "{kind} keeps its protobuf to wait with");
        let mut mine = info(GROUP, message);
        mine.key.from_me = Some(true);
        let own = amends(&mine).into_iter().find(|a| a.target == "3EB0AA").map(|a| a.what);
        assert_eq!(own, Some(Amendment::DeleteForMe), "a {kind} of yours leaves its own ID no row to wait for");
    }
}

/// A message row's own fields, as the protocol's fixture of one has them with a sender: a detail
/// key named alike would come twice in the row, since its detail is flattened into it.
fn row_fields() -> HashSet<String> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../../protocol/fixtures/messages.push.json")).unwrap();
    let mut row: jaw_proto::windows::MessageRow =
        serde_json::from_value(fixture["params"]["upsert"][0].clone()).unwrap();
    row.detail.clear();
    row.from = Some(jaw_proto::windows::From { id: "person:1".into(), name: "Ana".into(), colour: 1 });
    serde_json::to_value(&row).unwrap().as_object().unwrap().keys().cloned().collect()
}

#[test]
fn no_kind_of_message_has_a_detail_key_named_like_one_of_its_rows_own_fields() {
    let fields = row_fields();
    assert!(fields.contains("kind") && fields.contains("from") && fields.contains("status"), "{fields:?}");
    let link = e2e::ExtendedTextMessage {
        text: Some("see https://jaw.example".into()),
        matched_text: Some("https://jaw.example".into()),
        title: Some("jaw".into()),
        ..Default::default()
    };
    let kinds = [
        e2e::Message { extended_text_message: Some(Box::new(link)), ..Default::default() },
        e2e::Message { image_message: Some(Default::default()), ..Default::default() },
        e2e::Message { video_message: Some(Default::default()), ..Default::default() },
        e2e::Message { ptv_message: Some(Default::default()), ..Default::default() },
        e2e::Message { audio_message: Some(Default::default()), ..Default::default() },
        e2e::Message { document_message: Some(Default::default()), ..Default::default() },
        e2e::Message { sticker_message: Some(Default::default()), ..Default::default() },
        e2e::Message { location_message: Some(Default::default()), ..Default::default() },
        e2e::Message { live_location_message: Some(Default::default()), ..Default::default() },
        e2e::Message { contact_message: Some(Default::default()), ..Default::default() },
        e2e::Message { contacts_array_message: Some(Default::default()), ..Default::default() },
        e2e::Message { poll_creation_message: Some(Default::default()), ..Default::default() },
        e2e::Message { event_message: Some(Default::default()), ..Default::default() },
        e2e::Message { group_invite_message: Some(Default::default()), ..Default::default() },
        e2e::Message { call_log_messsage: Some(Default::default()), ..Default::default() },
        e2e::Message { album_message: Some(Default::default()), ..Default::default() },
        e2e::Message { keep_in_chat_message: Some(Default::default()), ..Default::default() },
        e2e::Message { pin_in_chat_message: Some(Default::default()), ..Default::default() },
        e2e::Message { newsletter_follower_invite_message_v2: Some(Default::default()), ..Default::default() },
        e2e::Message { newsletter_admin_invite_message: Some(Default::default()), ..Default::default() },
        e2e::Message { message_history_notice: Some(Default::default()), ..Default::default() },
        e2e::Message { message_history_bundle: Some(Default::default()), ..Default::default() },
        e2e::Message { placeholder_message: Some(Default::default()), ..Default::default() },
        e2e::Message { status_mention_message: Some(Default::default()), ..Default::default() },
        e2e::Message { status_notification_message: Some(Default::default()), ..Default::default() },
    ];
    let mut with_detail = 0;
    for m in kinds {
        let Some(row) = rows(&info(ANA, m), true).into_iter().next() else { continue };
        let Some(detail) = row.detail.as_deref() else { continue };
        let detail: serde_json::Map<String, serde_json::Value> = serde_json::from_str(detail).unwrap();
        with_detail += 1;
        for key in detail.keys() {
            assert!(!fields.contains(key), "a {} message's detail has {key}, which its row has of its own", row.kind);
        }
    }
    assert!(with_detail >= 15, "only {with_detail} kinds came with a detail");
}

#[test]
fn a_file_a_message_carries_is_described_for_downloading_with_only_whole_hashes_and_keys() {
    let photo = row(e2e::Message {
        image_message: Some(Box::new(e2e::ImageMessage {
            mimetype: Some("image/jpeg".into()),
            width: Some(1600),
            height: Some(1200),
            file_length: Some(204_800),
            file_sha256: Some(vec![1; 32]),
            file_enc_sha256: Some(vec![2; 31]),
            media_key: Some(vec![3; 32]),
            direct_path: Some("/v/t62.7118-24/photo.enc".into()),
            jpeg_thumbnail: Some(vec![0xFF, 0xD8, 0xFF, 0xD9]),
            ..Default::default()
        })),
        ..Default::default()
    });
    assert_eq!(
        photo.media.as_deref(),
        Some(&MediaCopy {
            keys: Keys::Image,
            mime: Some("image/jpeg".into()),
            bytes: Some(204_800),
            w: Some(1600),
            h: Some(1200),
            sha256: Some(vec![1; 32]),
            media_key: Some(vec![3; 32]),
            direct_path: Some("/v/t62.7118-24/photo.enc".into()),
            thumbnail: true,
            ..Default::default()
        }),
        "a hash that isn't 32 bytes is no hash"
    );
    let note = row(e2e::Message {
        ptv_message: Some(Box::new(e2e::VideoMessage {
            seconds: Some(9),
            jpeg_thumbnail: Some(Vec::new()),
            ..Default::default()
        })),
        ..Default::default()
    });
    let note = note.media.unwrap();
    assert_eq!(
        (note.keys, note.seconds, note.thumbnail),
        (Keys::Video, Some(9.0), false),
        "an empty thumbnail is none"
    );
    let voice = row(e2e::Message {
        audio_message: Some(Box::new(e2e::AudioMessage { ptt: Some(true), seconds: Some(14), ..Default::default() })),
        ..Default::default()
    });
    assert_eq!(voice.media.map(|m| (m.keys, m.seconds)), Some((Keys::Audio, Some(14.0))));
    let document = row(e2e::Message {
        document_message: Some(Box::new(e2e::DocumentMessage { title: Some("Rent".into()), ..Default::default() })),
        ..Default::default()
    });
    let document = document.media.unwrap();
    assert_eq!((document.keys, document.name.as_deref()), (Keys::Document, Some("Rent")), "named by its title");
    let sticker = row(e2e::Message {
        sticker_message: Some(Box::new(e2e::StickerMessage { png_thumbnail: Some(vec![0x89]), ..Default::default() })),
        ..Default::default()
    });
    let sticker = sticker.media.unwrap();
    assert_eq!((sticker.keys, sticker.thumbnail), (Keys::Image, true), "a sticker is encrypted as a photo is");
    assert!(row(e2e::Message { conversation: Some("no file".into()), ..Default::default() }).media.is_none());
}

#[test]
fn an_hd_copy_amends_the_message_it_names_and_is_no_row_of_its_own() {
    let child = |association: AssociationType| e2e::Message {
        associated_child_message: Some(Box::new(e2e::FutureProofMessage {
            message: Some(Box::new(e2e::Message {
                image_message: Some(Box::new(e2e::ImageMessage {
                    width: Some(4032),
                    height: Some(3024),
                    file_sha256: Some(vec![9; 32]),
                    ..Default::default()
                })),
                message_context_info: Some(e2e::MessageContextInfo {
                    message_association: Some(e2e::MessageAssociation {
                        association_type: Some(association as i32),
                        parent_message_key: Some(MessageKey {
                            remote_jid: Some(ANA.into()),
                            from_me: Some(false),
                            id: Some("3EB0SD".into()),
                            participant: None,
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            })),
        })),
        ..Default::default()
    };
    let hd = info(ANA, child(AssociationType::HdImageDualUpload));
    assert!(rows(&hd, true).is_empty(), "the HD copy is no message of its own");
    let [amend] = amends(&hd).try_into().unwrap();
    assert_eq!((amend.chat.as_str(), amend.target.as_str()), (ANA, "3EB0SD"));
    let Amendment::Hd { copy } = &amend.what else { panic!("{:?}", amend.what) };
    assert_eq!((copy.keys, copy.w, copy.sha256.as_deref()), (Keys::Image, Some(4032), Some(&[9; 32][..])));

    let album = info(ANA, child(AssociationType::MediaAlbum));
    let [photo] = rows(&album, true).try_into().unwrap();
    assert_eq!((photo.id.as_str(), photo.kind), ("3EB0AA", "image"), "any other association leaves a photo of its own");
    assert!(amends(&album).is_empty());
}
