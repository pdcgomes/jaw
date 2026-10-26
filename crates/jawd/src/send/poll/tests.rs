use std::path::Path;
use std::time::Duration;

use jaw_proto::code;
use rusqlite::Connection;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, Replies, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::send::{Sending, Timing, link};
use crate::store::{Chat, Lane, Message, Store, read};
use crate::work::Priority;

const CLIMBING: &str = "120363012345678901@g.us";
const RUI: &str = "351913333333@s.whatsapp.net";
const DINNER: &[&str] = &["Ramen", "Tasca do Chico", "Pizza"];

/// jawd's sending on a jaw.db in `home`, through the demo engine, connected, with the climbing
/// group's chat.
async fn sending(home: &Path) -> (Store, Sending, Arc<World>) {
    let store = Store::open(home).unwrap();
    let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
    let (pushes, _) = broadcast::channel(256);
    let (events, incoming) = mpsc::channel(1024);
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let mut connected = pushes.subscribe();
    let sync = crate::sync::run(
        incoming,
        store.clone(),
        live.clone(),
        pushes.clone(),
        Some(supervisor.clone()),
        home.into(),
        Default::default(),
    );
    tokio::spawn(sync);
    let attention = Arc::new(crate::attention::Attention::default());
    let timing =
        Timing { retry_first: Duration::from_millis(5), retry_most: Duration::from_millis(20), ..Timing::default() };
    let sending = Sending::start(
        store.clone(),
        Some(supervisor),
        live,
        &pushes,
        attention.focus(),
        timing,
        link::Previews::off(),
    )
    .await
    .unwrap();
    let online = async {
        while let Ok(push) = connected.recv().await {
            if push.method == "connection" && push.params["state"] == "online" {
                return;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
    let chat = Change::Chat(Chat { id: CLIMBING.into(), ..Chat::default() });
    let me = Change::Remember { key: "me".into(), value: demo::ME.into() };
    store.apply(Lane::Live, vec![chat, me]).await.unwrap();
    (store, sending, world)
}

async fn query<T: Send + 'static>(
    store: &Store,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// Waits until `holds` says so of jaw.db, for five seconds at most.
async fn eventually(
    store: &Store,
    what: &str,
    holds: impl Fn(&Connection) -> anyhow::Result<bool> + Clone + Send + 'static,
) {
    let began = std::time::Instant::now();
    loop {
        let holds = holds.clone();
        if query(store, move |db| holds(db)).await {
            return;
        }
        assert!(began.elapsed() < Duration::from_secs(5), "never: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Rui's poll `id` about dinner, of which a vote chooses `max`, as it arrives.
async fn ruis_poll(store: &Store, id: &str, max: u32) {
    let shown: Vec<Value> = DINNER.iter().map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect();
    let poll = Message {
        chat: CLIMBING.into(),
        id: id.into(),
        at: now_ms() - 60_000,
        sender: Some(RUI.into()),
        kind: "poll",
        summary: "Poll: Jantar?".into(),
        detail: Some(
            json!({ "poll": { "question": "Jantar?", "options": shown, "max": max, "voters": 0 } }).to_string(),
        ),
        poll: DINNER.iter().map(|name| name.to_string()).collect(),
        live: true,
        ..Message::default()
    };
    store.apply(Lane::Live, vec![Change::Message(poll)]).await.unwrap();
}

/// Poll `id` in the climbing group as a window shows it: each option's votes and whether you chose
/// it, and how many voted.
async fn shown(store: &Store, id: &'static str) -> (Vec<(String, i64, bool)>, i64) {
    let row = query(store, move |db| Ok(read::message(db, CLIMBING, id)?.expect("the poll is there").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().expect("a poll's detail").get()).unwrap();
    let poll = &detail["poll"];
    let options = poll["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["name"].as_str().unwrap().to_string(), o["votes"].as_i64().unwrap(), o["mine"] == true))
        .collect();
    (options, poll["voters"].as_i64().unwrap())
}

fn tally(votes: [i64; 3], mine: &[usize]) -> Vec<(String, i64, bool)> {
    DINNER.iter().zip(votes).enumerate().map(|(at, (name, n))| (name.to_string(), n, mine.contains(&at))).collect()
}

/// The polls the demo engine was asked to send, as each went.
fn polls_sent(world: &World) -> Vec<e2e::Message> {
    world.sends().into_iter().map(|send| e2e::Message::decode(send.raw.as_slice()).unwrap()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_goes_with_a_secret_of_its_own_as_version_3_when_a_vote_chooses_one() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let one = sending
        .call("send.poll", json!({ "chat": CLIMBING, "question": " Jantar? ", "options": ["Ramen", " Pizza "] }))
        .await
        .unwrap();
    assert_eq!(one["row"]["kind"], "poll");
    assert_eq!(
        one["row"]["poll"],
        json!({ "question": "Jantar?", "options": [
            { "name": "Ramen", "votes": 0, "mine": false },
            { "name": "Pizza", "votes": 0, "mine": false },
        ], "max": 1, "voters": 0 }),
        "shown at once, trimmed, with no votes"
    );
    let two = json!({ "chat": CLIMBING, "question": "Quando?", "options": ["Sex", "Sáb", "Dom"], "max": 2 });
    sending.call("send.poll", two).await.unwrap();
    let any = json!({ "chat": CLIMBING, "question": "Quem?", "options": ["Eu", "Tu"], "max": 2 });
    sending.call("send.poll", any).await.unwrap();
    world.until(|asked| asked.iter().filter(|kind| matches!(kind, pb::command::Kind::Send(_))).count() == 3).await;
    let sent = polls_sent(&world);
    let secret =
        |message: &e2e::Message| message.message_context_info.as_ref().unwrap().message_secret.clone().unwrap();
    let v3 = sent[0].poll_creation_message_v3.as_ref().expect("a poll a vote chooses one of goes as version 3");
    assert_eq!(v3.name.as_deref(), Some("Jantar?"));
    assert_eq!(v3.selectable_options_count, Some(1));
    let names: Vec<_> = v3.options.iter().map(|o| o.option_name.clone().unwrap()).collect();
    assert_eq!(names, ["Ramen", "Pizza"]);
    let first = sent[1].poll_creation_message.as_ref().expect("one choosing more goes as the first version");
    assert_eq!(first.selectable_options_count, Some(2));
    assert_eq!(sent[2].poll_creation_message.as_ref().unwrap().selectable_options_count, Some(0), "0 for any");
    let secrets: Vec<Vec<u8>> = sent.iter().map(secret).collect();
    assert!(secrets.iter().all(|secret| secret.len() == 32), "32 bytes each");
    assert!(secrets[0] != secrets[1] && secrets[1] != secrets[2] && secrets[0] != secrets[2], "each its own");
    let kept: Vec<Option<i64>> = query(&store, |db| {
        Ok(db
            .prepare("SELECT detail ->> '$.poll.max' FROM messages WHERE kind = 'poll' ORDER BY t")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [Some(1), Some(2), Some(2)]);
    assert_eq!(store.count("poll_options").unwrap(), 7, "each option kept, to count the votes in it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_that_cant_be_one_is_refused_and_nothing_of_it_kept() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let poll = |question: &str, options: Vec<String>, max: Option<u32>| json!({ "chat": CLIMBING, "question": question, "options": options, "max": max });
    let names = |n: usize| (1..=n).map(|i| format!("opção {i}")).collect::<Vec<_>>();
    let refused = [
        (poll(" ", names(2), None), code::INVALID_PARAMS, "it asks nothing"),
        (poll("Jantar?", names(1), None), code::INVALID_PARAMS, "one option"),
        (poll("Jantar?", vec!["Ramen".into(), " ".into()], None), code::INVALID_PARAMS, "an option saying nothing"),
        (poll("Jantar?", vec!["Ramen".into(), "Ramen ".into()], None), code::INVALID_PARAMS, "one named twice"),
        (poll("Jantar?", names(13), None), code::LIMIT, "13 options"),
        (poll("Jantar?", vec!["Ramen".into(), "a".repeat(101)], None), code::LIMIT, "an option too long"),
        (poll(&"?".repeat(256), names(2), None), code::LIMIT, "a question too long"),
        (poll("Jantar?", names(2), Some(0)), code::INVALID_PARAMS, "a vote choosing none"),
        (poll("Jantar?", names(2), Some(3)), code::INVALID_PARAMS, "more than it has"),
    ];
    for (params, code, why) in refused {
        let error = sending.call("send.poll", params).await.expect_err(why);
        assert_eq!(error.code, code, "{why}: {}", error.message);
    }
    let lost = json!({ "chat": "351919999999@s.whatsapp.net", "question": "Jantar?", "options": names(2) });
    assert_eq!(sending.call("send.poll", lost).await.unwrap_err().code, code::NOT_FOUND, "a chat jaw hasn't");
    assert!(sending.call("send.poll", poll("Jantar?", names(12), Some(12))).await.is_ok(), "12, any of them");
    assert_eq!(store.count("messages").unwrap(), 1, "only that one kept");
    world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::Send(_)))).await;
    assert_eq!(polls_sent(&world).len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_asked_twice_with_one_key_goes_once() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let params = json!({ "chat": CLIMBING, "question": "Jantar?", "options": DINNER, "key": "jaw-poll-1" });
    let (a, b) = tokio::join!(sending.call("send.poll", params.clone()), sending.call("send.poll", params.clone()));
    assert_eq!(a.unwrap()["message"], b.unwrap()["message"]);
    let again = sending.call("send.poll", params).await.unwrap();
    world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::Send(_)))).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(polls_sent(&world).len(), 1);
    assert_eq!(store.count("messages").unwrap(), 1);
    assert_eq!(again["row"]["kind"], "poll");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_goes_through_the_engine_naming_its_options_by_their_hashes_shows_at_once_and_counts_once_sent() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    ruis_poll(&store, "p1", 1).await;
    world.gate(true);
    let answer = sending.call("poll.vote", json!({ "message": "p1", "options": ["Tasca do Chico"] })).await.unwrap();
    assert_eq!(answer, json!({}));
    world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::PollVote(_)))).await;
    assert_eq!(shown(&store, "p1").await, (tally([0, 1, 0], &[1]), 1), "shown before WhatsApp has it");
    assert_eq!(store.count("poll_voters").unwrap(), 0, "and kept as yours only once it has");
    let vote = world.votes().remove(0);
    assert_eq!((vote.chat.as_str(), vote.poll_id.as_str()), (CLIMBING, "p1"));
    assert_eq!((vote.poll_sender.as_str(), vote.poll_from_me), (RUI, false), "the poll named as it was sent");
    assert_eq!(vote.options, [option_hash("Tasca do Chico")], "each option by the SHA-256 of its name");
    assert!(vote.id.starts_with("3EB0") && vote.id.len() == 22, "with an ID of its own: {}", vote.id);
    assert!(vote.at_ms > 0);

    world.gate(false);
    eventually(&store, "the vote kept", |db| {
        Ok(db.query_row("SELECT count(*) FROM poll_voters", [], |r| r.get::<_, i64>(0))? == 1)
    })
    .await;
    assert_eq!(shown(&store, "p1").await, (tally([0, 1, 0], &[1]), 1), "the same, from the vote kept");

    // Changed, it replaces your last; taken back, none of the options has it.
    sending.call("poll.vote", json!({ "message": "p1", "options": ["Pizza"], "chat": CLIMBING })).await.unwrap();
    eventually(&store, "the change kept", |db| {
        Ok(db.query_row("SELECT count(*) FROM poll_votes WHERE position = 2", [], |r| r.get::<_, i64>(0))? == 1)
    })
    .await;
    assert_eq!(shown(&store, "p1").await, (tally([0, 0, 1], &[2]), 1));
    sending.call("poll.vote", json!({ "message": "p1", "options": [] })).await.unwrap();
    eventually(&store, "the vote taken back", |db| {
        Ok(db.query_row("SELECT count(*) FROM poll_votes", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
    assert_eq!(shown(&store, "p1").await, (tally([0, 0, 0], &[]), 0));
    assert_eq!(world.votes().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_choice_in_a_poll_taking_one_is_error_4_and_a_vote_naming_what_it_hasnt_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    ruis_poll(&store, "p1", 1).await;
    ruis_poll(&store, "p2", 2).await;
    let vote = |params: Value| sending.call("poll.vote", params);
    let two = vote(json!({ "message": "p1", "options": ["Ramen", "Pizza"] })).await.unwrap_err();
    assert_eq!(two.code, code::NOT_ALLOWED, "{}", two.message);
    let unknown = vote(json!({ "message": "p1", "options": ["Sushi"] })).await.unwrap_err();
    assert_eq!(unknown.code, code::INVALID_PARAMS, "{}", unknown.message);
    let three = vote(json!({ "message": "p2", "options": DINNER })).await.unwrap_err();
    assert_eq!(three.code, code::NOT_ALLOWED, "more than the two p2 takes");
    assert_eq!(vote(json!({ "message": "nope", "options": [] })).await.unwrap_err().code, code::NOT_FOUND);
    let text = Message {
        chat: CLIMBING.into(),
        id: "t1".into(),
        at: now_ms(),
        sender: Some(RUI.into()),
        kind: "text",
        text: Some("olá".into()),
        summary: "olá".into(),
        live: true,
        ..Message::default()
    };
    store.apply(Lane::Live, vec![Change::Message(text)]).await.unwrap();
    assert_eq!(vote(json!({ "message": "t1", "options": [] })).await.unwrap_err().code, code::NOT_ALLOWED);
    assert!(world.votes().is_empty(), "none of them went");

    // The same option twice is one choice, and two in a poll taking two are fine.
    vote(json!({ "message": "p1", "options": ["Ramen", "Ramen"] })).await.unwrap();
    vote(json!({ "message": "p2", "options": ["Ramen", "Pizza"] })).await.unwrap();
    world.until(|asked| asked.iter().filter(|kind| matches!(kind, pb::command::Kind::PollVote(_))).count() == 2).await;
    let votes = world.votes();
    assert_eq!(votes[0].options.len(), 1);
    assert_eq!(votes[1].options, [option_hash("Ramen"), option_hash("Pizza")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_asked_twice_with_one_key_goes_once_and_one_whatsapp_refuses_leaves_no_vote() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    ruis_poll(&store, "p1", 1).await;
    let params = json!({ "message": "p1", "options": ["Ramen"], "key": "jaw-vote-1" });
    let (a, b) = tokio::join!(sending.call("poll.vote", params.clone()), sending.call("poll.vote", params));
    assert_eq!((a.unwrap(), b.unwrap()), (json!({}), json!({})));
    eventually(&store, "the vote kept", |db| {
        Ok(db.query_row("SELECT count(*) FROM poll_voters", [], |r| r.get::<_, i64>(0))? == 1)
    })
    .await;
    assert_eq!(world.votes().len(), 1);

    world.refuse_next(1);
    sending.call("poll.vote", json!({ "message": "p1", "options": ["Pizza"] })).await.unwrap();
    world.until(|asked| asked.iter().filter(|kind| matches!(kind, pb::command::Kind::PollVote(_))).count() == 2).await;
    eventually(&store, "the refused vote let go", |db| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'vote'", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
    assert_eq!(shown(&store, "p1").await, (tally([1, 0, 0], &[0]), 1), "your last vote, as WhatsApp has it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_from_your_phone_newer_than_one_waiting_to_go_is_the_one_shown_and_kept() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    ruis_poll(&store, "p1", 1).await;
    world.gate(true);
    sending.call("poll.vote", json!({ "message": "p1", "options": ["Ramen"] })).await.unwrap();
    world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::PollVote(_)))).await;
    assert_eq!(shown(&store, "p1").await, (tally([1, 0, 0], &[0]), 1));
    let hex = |name: &str| option_hash(name).iter().map(|b| format!("{b:02x}")).collect::<String>();
    let phone = crate::store::Amend {
        chat: CLIMBING.into(),
        target: "p1".into(),
        sender: Some(demo::ME.into()),
        mine: true,
        at: now_ms() + 1_000,
        what: Amendment::Vote { options: vec![hex("Pizza")], id: "3EB0PHONE".into() },
        raw: None,
    };
    store.apply(Lane::Live, vec![Change::Amend(phone)]).await.unwrap();
    assert_eq!(shown(&store, "p1").await, (tally([0, 0, 1], &[2]), 1), "the phone's, newer than the one waiting");
    world.gate(false);
    eventually(&store, "the vote waiting to go gone", |db| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'vote'", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
    assert_eq!(shown(&store, "p1").await, (tally([0, 0, 1], &[2]), 1), "and kept so: the older never replaces it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_in_a_poll_deleted_for_everyone_before_it_goes_is_withdrawn() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    ruis_poll(&store, "p1", 1).await;
    world.fail_chats(&[CLIMBING.to_string()]);
    sending.call("poll.vote", json!({ "message": "p1", "options": ["Ramen"] })).await.unwrap();
    world.until(|asked| asked.iter().any(|kind| matches!(kind, pb::command::Kind::PollVote(_)))).await;
    let revoke = crate::store::Amend {
        chat: CLIMBING.into(),
        target: "p1".into(),
        sender: Some(RUI.into()),
        mine: false,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    };
    store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
    world.stop_failing_chats();
    eventually(&store, "the vote let go", |db| {
        Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'vote'", [], |r| r.get::<_, i64>(0))? == 0)
    })
    .await;
    assert_eq!(world.given(), 0, "WhatsApp was given nothing");
    assert_eq!(store.count("poll_voters").unwrap(), 0);
}

/// Your poll about dinner in the climbing group, once WhatsApp has it.
async fn your_poll(store: &Store, sending: &Sending) -> String {
    let poll = json!({ "chat": CLIMBING, "question": "Jantar?", "options": DINNER });
    let sent = sending.call("send.poll", poll).await.unwrap();
    eventually(store, "the poll sent", outbox_empty).await;
    sent["message"].as_str().unwrap().to_string()
}

fn outbox_empty(db: &Connection) -> anyhow::Result<bool> {
    Ok(db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))? == 0)
}

/// The kinds of the outbox's rows, oldest first.
async fn outbox_kinds(store: &Store) -> Vec<String> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT kind FROM outbox ORDER BY n")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_in_your_poll_is_refused_while_your_deletion_of_it_for_everyone_is_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let id = your_poll(&store, &sending).await;
    // Your deletion of it for everyone fails on the way, so it stays on its way, and windows show
    // the poll deleted.
    world.fail_chats(&[CLIMBING.to_string()]);
    sending.call("delete", json!({ "message": id, "chat": CLIMBING, "for": "everyone" })).await.unwrap();
    // Whatever the vote names, as for a poll deleted.
    for options in [json!(["Ramen"]), json!(["Sushi"])] {
        let vote = json!({ "message": id, "chat": CLIMBING, "options": options });
        let refused = sending.call("poll.vote", vote).await.unwrap_err();
        assert_eq!(refused.code, code::NOT_ALLOWED, "{options}: {}", refused.message);
    }
    assert_eq!(outbox_kinds(&store).await, ["revoke"], "nothing written");
    // Once you've deleted it for you too and WhatsApp can be reached, the deletion goes, and no vote
    // after it.
    sending.call("delete", json!({ "message": id, "chat": CLIMBING, "for": "me" })).await.unwrap();
    world.stop_failing_chats();
    eventually(&store, "the outbox emptied", outbox_empty).await;
    assert!(world.votes().is_empty(), "{:?}", world.votes());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_whose_poll_is_deleted_for_everyone_once_it_has_found_it_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let (store, sending, world) = sending(home.path()).await;
    let yours = your_poll(&store, &sending).await;
    ruis_poll(&store, "p1", 1).await;
    world.fail_chats(&[CLIMBING.to_string()]);
    // Each vote finds its poll as it was, and the poll is then deleted for everyone before its step:
    // yours by you, from another interface at the same moment, and Rui's by him.
    for (poll, mine) in [(yours.as_str(), true), ("p1", false)] {
        let (stopped, go) = sending.pause_next_action();
        let voting = tokio::spawn({
            let (sending, params) =
                (sending.clone(), json!({ "message": poll, "chat": CLIMBING, "options": ["Ramen"] }));
            async move { sending.call("poll.vote", params).await }
        });
        stopped.notified().await;
        if mine {
            sending.call("delete", json!({ "message": poll, "chat": CLIMBING, "for": "everyone" })).await.unwrap();
        } else {
            let his = crate::store::Amend {
                chat: CLIMBING.into(),
                target: poll.into(),
                sender: Some(RUI.into()),
                mine: false,
                at: now_ms(),
                what: Amendment::Revoke,
                raw: None,
            };
            store.apply(Lane::Live, vec![Change::Amend(his)]).await.unwrap();
        }
        go.notify_one();
        let refused = voting.await.unwrap().unwrap_err();
        assert_eq!(refused.code, code::NOT_ALLOWED, "{poll}: {}", refused.message);
    }
    assert_eq!(outbox_kinds(&store).await, ["revoke"], "nothing written behind the deletions");
}
