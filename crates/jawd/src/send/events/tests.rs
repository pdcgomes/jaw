use jaw_proto::code;
use serde_json::{Value, json};

use super::*;
use crate::convert::secret::{self, Between};
use crate::engine::demo;
use crate::send::places::tests::{CLIMBING, query, sending, sent};
use crate::store::{Chat, Lane, read};

const SATURDAY: i64 = 1_792_231_200_000;

#[tokio::test(flavor = "multi_thread")]
async fn an_event_goes_with_its_name_times_and_place_and_a_secret_of_its_own_and_shows_at_once() {
    let home = tempfile::tempdir().unwrap();
    let (_store, sending, world) = sending(home.path()).await;
    let params = json!({
        "chat": CLIMBING, "name": " Sintra ", "description": "Bring the rope", "start": SATURDAY + 999,
        "end": SATURDAY + 6 * 3_600_000, "place": "Cabo da Roca", "call": false,
    });
    let answer = sending.call("send.event", params).await.unwrap();
    let row = &answer["row"];
    assert_eq!((&row["kind"], &row["status"], &row["spans"]), (&json!("event"), &json!("pending"), &json!([])));
    assert_eq!(
        row["event"],
        json!({
            "name": "Sintra", "description": "Bring the rope", "start": SATURDAY, "end": SATURDAY + 6 * 3_600_000,
            "place": { "lat": null, "lon": null, "name": "Cabo da Roca", "address": null, "url": null },
            "call": null, "cancelled": false, "going": 0, "maybe": 0, "not_going": 0, "mine": null,
        }),
        "shown at once, to the second, as WhatsApp keeps it"
    );
    let pin = json!({ "lat": 38.7803, "lon": -9.4989, "name": "Cabo da Roca", "address": "Colares" });
    let pinned =
        sending.call("send.event", json!({ "chat": CLIMBING, "name": "Sunset", "start": SATURDAY, "place": pin }));
    assert_eq!(pinned.await.unwrap()["row"]["event"]["place"]["lat"], 38.7803, "a place as send.location takes it");
    let sent = sent(&world, 2).await;
    let event = sent[0].event_message.as_ref().expect("an event goes as one");
    assert_eq!((event.name.as_deref(), event.description.as_deref()), (Some("Sintra"), Some("Bring the rope")));
    assert_eq!((event.start_time, event.end_time), (Some(SATURDAY / 1000), Some(SATURDAY / 1000 + 6 * 3_600)));
    assert_eq!(event.location.as_ref().and_then(|l| l.name.as_deref()), Some("Cabo da Roca"));
    assert!(event.join_link.is_none() && event.context_info.is_none(), "no call link, and no timer in this chat");
    let secrets: Vec<&[u8]> = sent
        .iter()
        .map(|m| m.message_context_info.as_ref().and_then(|c| c.message_secret.as_deref()).expect("a secret"))
        .collect();
    assert_eq!(secrets[0].len(), 32);
    assert_ne!(secrets[0], secrets[1], "each its own");
    let second = sent[1].event_message.as_ref().unwrap().location.as_ref().unwrap();
    assert_eq!((second.degrees_latitude, second.address.as_deref()), (Some(38.7803), Some("Colares")));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_that_cant_be_one_is_refused_and_nothing_of_it_kept() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let with = |more: Value| {
        let mut params = json!({ "chat": CLIMBING, "name": "Sintra", "start": SATURDAY });
        params.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        params
    };
    let refused = [
        (json!({ "chat": CLIMBING, "start": SATURDAY }), code::INVALID_PARAMS, "no name"),
        (with(json!({ "name": "  " })), code::INVALID_PARAMS, "a name that's only spaces"),
        (json!({ "chat": CLIMBING, "name": "Sintra" }), code::INVALID_PARAMS, "no start"),
        (with(json!({ "start": 0 })), code::INVALID_PARAMS, "a start at no time"),
        (with(json!({ "end": SATURDAY - 1 })), code::INVALID_PARAMS, "an end before its start"),
        (with(json!({ "name": "a".repeat(256) })), code::LIMIT, "a name too long"),
        (with(json!({ "description": "a".repeat(2_049) })), code::LIMIT, "a description too long"),
        (with(json!({ "place": "a".repeat(1_025) })), code::LIMIT, "a place's name too long"),
        (with(json!({ "place": { "lat": 91.0, "lon": 0.0 } })), code::INVALID_PARAMS, "a place past the pole"),
        (with(json!({ "place": { "name": "Cabo" } })), code::INVALID_PARAMS, "a pin without its degrees"),
        (with(json!({ "call": true })), code::NOT_ALLOWED, "a call link, which jawd can't make"),
        (with(json!({ "chat": "351919999999@s.whatsapp.net" })), code::NOT_FOUND, "a chat jaw hasn't"),
    ];
    for (params, code, why) in refused {
        let error = sending.call("send.event", params).await.expect_err(why);
        assert_eq!(error.code, code, "{why}: {}", error.message);
    }
    let error = sending.call("send.event", with(json!({ "place": { "lat": 91.0, "lon": 0.0 } }))).await.unwrap_err();
    assert_eq!(error.data.as_ref().map(|d| d["param"].clone()), Some(json!("place.lat")), "{error:?}");
    let pin = json!({ "place": { "lat": 1.0, "lon": 1.0, "address": "a".repeat(1_025) } });
    let error = sending.call("send.event", with(pin)).await.unwrap_err();
    assert_eq!(error.data, Some(json!({ "limit": "place.address", "most": 1_024 })), "{error:?}");
    let longest = with(json!({ "name": "a".repeat(255), "description": "d".repeat(2_048), "end": SATURDAY }));
    assert!(sending.call("send.event", longest).await.is_ok(), "the longest name and description, ending as it starts");
    assert_eq!(store.count("messages").unwrap(), 1, "only that kept");
    assert_eq!(sent(&world, 1).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_asked_again_with_its_key_goes_once_and_in_a_chat_with_disappearing_messages_with_its_timer() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    store
        .apply(Lane::Live, vec![Change::Chat(Chat { id: CLIMBING.into(), ephemeral: Some(86_400), ..Chat::default() })])
        .await
        .unwrap();
    let event = json!({ "chat": CLIMBING, "name": "Sintra", "start": SATURDAY, "key": "jaw-event-1" });
    let (a, b) = tokio::join!(sending.call("send.event", event.clone()), sending.call("send.event", event.clone()));
    let first = a.unwrap();
    assert_eq!(first["message"], b.unwrap()["message"]);
    let again = sending.call("send.event", event).await.unwrap();
    assert_eq!((&again["message"], &again["row"]["kind"]), (&first["message"], &json!("event")));
    let sent = sent(&world, 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!((world.sends().len(), store.count("messages").unwrap()), (1, 1), "it went once");
    let timer = sent[0].event_message.as_ref().unwrap().context_info.as_ref().and_then(|c| c.expiration);
    assert_eq!(timer, Some(86_400), "with the chat's timer");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_to_your_event_opens_with_the_secret_it_went_with() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let answer = sending.call("send.event", json!({ "chat": CLIMBING, "name": "Sintra", "start": SATURDAY })).await;
    let id = answer.unwrap()["message"].as_str().unwrap().to_string();
    let sent = sent(&world, 1).await;
    let secret = sent[0].message_context_info.as_ref().unwrap().message_secret.clone().unwrap();
    let rui = "351910000003@s.whatsapp.net";
    let plain = e2e::EventResponseMessage { response: Some(1), timestamp_ms: Some(SATURDAY - 1), ..Default::default() };
    let between = Between { id: &id, author: demo::ME, who: rui };
    let payload = secret::seal(&secret, secret::EVENT_RESPONSE, between, [5; 12], &plain.encode_to_vec());
    let event_key =
        MessageKey { remote_jid: Some(CLIMBING.into()), from_me: Some(true), id: Some(id.clone()), participant: None };
    let info = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(CLIMBING.into()),
            from_me: Some(false),
            id: Some("r1".into()),
            participant: Some(rui.into()),
        },
        message: Some(e2e::Message {
            enc_event_response_message: Some(e2e::EncEventResponseMessage {
                event_creation_message_key: Some(event_key),
                enc_payload: Some(payload),
                enc_iv: Some(vec![5; 12]),
            }),
            ..Default::default()
        }),
        message_timestamp: Some((SATURDAY / 1000) as u64),
        ..Default::default()
    };
    let me = convert::Me::new([demo::ME.to_string()]);
    store.apply(Lane::Live, convert::message(&info, &me, true)).await.unwrap();
    let row = query(&store, move |db| Ok(read::message(db, CLIMBING, &id)?.unwrap().0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["event"]["going"], 1, "Rui's going, sealed with the secret your event went with");
}
