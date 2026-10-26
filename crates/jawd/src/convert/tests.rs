use std::collections::HashSet;

use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::web_message_info::StubType;

use super::*;

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
    }
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
