use rusqlite::Connection;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_e2e::event_response_message::EventResponseType;
use wa_proto::wa_web_protobufs_web::EventResponse;

use super::*;
use crate::convert::Me;
use crate::convert::secret::Between;
use crate::store::write::{Packed, links};
use crate::store::{Applied, Change, Lane, Person, Store, read, schema};
use crate::work::{Priority, Report};

const GROUP: &str = "120363000000000001@g.us";
const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const RUI: &str = "351910000003@s.whatsapp.net";
const RUI_LID: &str = "100000000003@lid";
const ME: &str = "351910000001@s.whatsapp.net";
const ME_LID: &str = "100000000001@lid";
const AT: i64 = 1_792_000_000_000;
/// Saturday's start and end, in seconds since the epoch, as an event says them.
const START: i64 = 1_792_231_200;
const END: i64 = 1_792_252_800;
const SECRET: [u8; 32] = [7; 32];

use EventResponseType::{Going, Maybe, NotGoing};

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

/// Takes the events backfill's steps until it's done, failing at once should a step fail, which is
/// rolled back and so never moves on, and once it has taken more than it can need.
async fn backfilled(store: &Store) {
    let mut steps = store.backfill_steps(links::EVENTS);
    while store.backfill_left(links::EVENTS).unwrap() > 0 {
        steps.take("the backfill never ended");
        let applied = apply(store, vec![Change::Backfill(links::EVENTS)]).await;
        assert_eq!(applied.quarantined, 0, "a step failed: {applied:?}");
    }
}

/// Takes the merges' steps until none is left, failing as `backfilled` does.
async fn merged(store: &Store) {
    let mut steps = store.merge_steps();
    while store.count("chat_merges").unwrap() > 0 {
        steps.take("the merge never finished");
        let applied = apply(store, vec![Change::MergeStep(None)]).await;
        assert_eq!(applied.quarantined, 0, "a step failed: {applied:?}");
    }
}

async fn opened(home: &std::path::Path) -> Store {
    let store = Store::open(home).unwrap();
    apply(&store, vec![Change::Remember { key: "me".into(), value: format!("{ME} {ME_LID}") }]).await;
    store
}

fn key(id: &str, from: &str) -> MessageKey {
    MessageKey {
        remote_jid: Some(GROUP.into()),
        from_me: Some(from == ME),
        id: Some(id.into()),
        participant: (from != ME).then(|| from.into()),
    }
}

/// What the converter makes of `info` arriving live, as the drain hands the writer.
fn live(info: WebMessageInfo) -> Vec<Change> {
    convert::message(&info, &Me::new([ME.to_string(), ME_LID.to_string()]), true)
}

/// `from`'s event `id` in the group, Sintra on Saturday at Cabo da Roca, with its secret.
fn event_from(id: &str, from: &str) -> WebMessageInfo {
    let event = e2e::EventMessage {
        name: Some("Sintra".into()),
        description: Some("Bring the rope".into()),
        start_time: Some(START),
        end_time: Some(END),
        location: Some(Box::new(e2e::LocationMessage { name: Some("Cabo da Roca".into()), ..Default::default() })),
        ..Default::default()
    };
    WebMessageInfo {
        key: key(id, from),
        message: Some(e2e::Message {
            event_message: Some(Box::new(event)),
            message_context_info: Some(e2e::MessageContextInfo {
                message_secret: Some(SECRET.to_vec()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    }
}

fn event(id: &str) -> Vec<Change> {
    live(event_from(id, ANA))
}

/// `who`'s answer to event `event`, as message `id` said at `at` by their clock, sealed as WhatsApp
/// seals it between `sealed`, the event's author and its answerer by the JIDs it took them as.
fn answer_info(
    event: &str,
    sealed: (&str, &str),
    who: &str,
    id: &str,
    at: i64,
    answer: EventResponseType,
) -> WebMessageInfo {
    let plain =
        e2e::EventResponseMessage { response: Some(answer as i32), timestamp_ms: Some(at), ..Default::default() };
    let between = Between { id: event, author: sealed.0, who: sealed.1 };
    let iv = [3; 12];
    let payload = secret::seal(&SECRET, secret::EVENT_RESPONSE, between, iv, &plain.encode_to_vec());
    let answering = e2e::EncEventResponseMessage {
        event_creation_message_key: Some(key(event, ANA)),
        enc_payload: Some(payload),
        enc_iv: Some(iv.to_vec()),
    };
    WebMessageInfo {
        key: key(id, who),
        message: Some(e2e::Message { enc_event_response_message: Some(answering), ..Default::default() }),
        message_timestamp: Some((at / 1000) as u64),
        ..Default::default()
    }
}

fn sealed_answer(
    event: &str,
    sealed: (&str, &str),
    who: &str,
    id: &str,
    at: i64,
    answer: EventResponseType,
) -> Vec<Change> {
    live(answer_info(event, sealed, who, id, at, answer))
}

/// `who`'s answer to Ana's event, sealed by the JIDs it came by.
fn answer(event: &str, who: &str, id: &str, at: i64, answer: EventResponseType) -> Vec<Change> {
    sealed_answer(event, (ANA, who), who, id, at, answer)
}

/// `who`'s edit of Ana's event, as message `id` at `at`, renaming it and cancelling it or not,
/// sealed between Ana and `who`.
fn edit(event: &str, who: &str, id: &str, at: i64, name: &str, cancelled: bool) -> Vec<Change> {
    live(edit_info(event, (ANA, who), id, at, name, cancelled))
}

/// The edit `edit` makes, sealed between `sealed`, the event's author and its editor by the JIDs it
/// took them as.
fn edit_info(event: &str, sealed: (&str, &str), id: &str, at: i64, name: &str, cancelled: bool) -> WebMessageInfo {
    let (author, who) = sealed;
    let edited = e2e::Message {
        event_message: Some(Box::new(e2e::EventMessage {
            name: Some(name.into()),
            start_time: Some(START + 3_600),
            is_canceled: Some(cancelled),
            ..Default::default()
        })),
        ..Default::default()
    };
    let between = Between { id: event, author, who };
    let iv = [4; 12];
    let payload = secret::seal(&SECRET, secret::EVENT_EDIT, between, iv, &edited.encode_to_vec());
    let editing = e2e::SecretEncryptedMessage {
        target_message_key: Some(key(event, ANA)),
        enc_payload: Some(payload),
        enc_iv: Some(iv.to_vec()),
        secret_enc_type: Some(e2e::secret_encrypted_message::SecretEncType::EventEdit as i32),
        ..Default::default()
    };
    WebMessageInfo {
        key: key(id, who),
        message: Some(e2e::Message { secret_encrypted_message: Some(editing), ..Default::default() }),
        message_timestamp: Some((at / 1000) as u64),
        ..Default::default()
    }
}

/// Event `id` as a window shows it: its detail's `event`, and its version.
async fn shown(store: &Store, id: &'static str) -> (Value, i64, bool) {
    let row = query(store, move |db| Ok(read::message(db, GROUP, id)?.expect("the event is there").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().expect("an event's detail").get()).unwrap();
    (detail["event"].clone(), row.v, row.edited)
}

/// Its counts: going, maybe, not going, and what you answered.
async fn counts(store: &Store, id: &'static str) -> (i64, i64, i64, Value) {
    let (event, ..) = shown(store, id).await;
    (
        event["going"].as_i64().unwrap(),
        event["maybe"].as_i64().unwrap(),
        event["not_going"].as_i64().unwrap(),
        event["mine"].clone(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_sealed_with_its_events_secret_counts_once_jaw_has_the_event_by_whichever_jids_it_was_sealed() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    // Rui's answer comes before the event, and waits, sealed.
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    assert_eq!(store.count("pending WHERE kind = 'rsvp'").unwrap(), 1, "it waits for its event, sealed");
    apply(&store, event("e1")).await;
    assert_eq!(counts(&store, "e1").await, (1, 0, 0, Value::Null), "it opens and counts once the event comes");
    assert_eq!(store.count("pending").unwrap(), 0);
    // WhatsApp may seal an answer for the event's author by their LID, or the answerer by theirs.
    apply(&store, sealed_answer("e1", (ANA_LID, RUI), RUI, "r2", AT + 20, Maybe)).await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 0, Value::Null), "sealed for Ana's LID, it opens");
    apply(&store, vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })]).await;
    apply(&store, sealed_answer("e1", (ANA, RUI_LID), RUI, "r3", AT + 30, NotGoing)).await;
    assert_eq!(
        counts(&store, "e1").await,
        (0, 0, 1, Value::Null),
        "and sealed for his LID, though it came by his number"
    );
    // What the event row keeps as it was converted says nothing of answers; windows count them.
    let detail: String =
        query(&store, |db| Ok(db.query_row("SELECT detail FROM messages WHERE id = 'e1'", [], |r| r.get(0))?)).await;
    assert!(detail.contains(r#""going":0"#), "{detail}");
}

#[tokio::test(flavor = "multi_thread")]
async fn each_persons_newest_answer_counts_by_its_time_then_its_id_and_one_older_or_the_same_again_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    apply(&store, answer("e1", ANA, "a1", AT + 11, Maybe)).await;
    assert_eq!(counts(&store, "e1").await, (1, 1, 0, Value::Null));
    apply(&store, answer("e1", RUI, "r2", AT + 20, NotGoing)).await;
    let (_, v, _) = shown(&store, "e1").await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 1, Value::Null), "his newer answer replaces his last");
    for late in [answer("e1", RUI, "r0", AT + 5, Going), answer("e1", RUI, "r2", AT + 20, NotGoing)] {
        let applied = apply(&store, late).await;
        assert_eq!(applied.changed, 0, "an older answer, or the same again, changes nothing");
        assert_eq!((counts(&store, "e1").await, shown(&store, "e1").await.1), ((0, 1, 1, Value::Null), v));
    }
    // Two of his in the same millisecond are ordered by their IDs.
    apply(&store, answer("e1", RUI, "r3", AT + 20, Going)).await;
    assert_eq!(counts(&store, "e1").await, (1, 1, 0, Value::Null));
    assert!(shown(&store, "e1").await.1 > v, "an answer that counts gives the event a new version");
    // When he answered is his phone's clock, as the answer says, however late it comes.
    let was = shown(&store, "e1").await.1;
    let mut late = answer_info("e1", (ANA, RUI), RUI, "r4", AT + 15, Maybe);
    late.message_timestamp = Some(((AT + 60_000) / 1000) as u64);
    apply(&store, live(late)).await;
    assert_eq!(
        (counts(&store, "e1").await, shown(&store, "e1").await.1),
        ((1, 1, 0, Value::Null), was),
        "made before his last, it changes nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn your_answer_is_yours_from_either_of_your_jids_and_one_of_none_counts_nowhere() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, sealed_answer("e1", (ANA, ME_LID), ME, "m1", AT + 10, Going)).await;
    assert_eq!(counts(&store, "e1").await, (1, 0, 0, Value::from("going")), "yours, sealed for your LID");
    apply(&store, sealed_answer("e1", (ANA, ME), ME, "m2", AT + 20, Maybe)).await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 0, Value::from("maybe")), "your newest, from your number");
    apply(&store, answer("e1", RUI, "r1", AT + 30, Going)).await;
    apply(&store, answer("e1", RUI, "r2", AT + 40, EventResponseType::Unknown)).await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 0, Value::from("maybe")), "an answer of none takes his back");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_brings_the_answers_on_its_event_opened_already() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let response = |id: &str, from: &str, at: i64, said: EventResponseType| EventResponse {
        event_response_message_key: Some(key(id, from)),
        timestamp_ms: Some(at),
        event_response_message: Some(e2e::EventResponseMessage {
            response: Some(said as i32),
            timestamp_ms: Some(at),
            extra_guest_count: Some(2),
        }),
        unread: None,
    };
    let mut info = event_from("e1", ANA);
    info.event_responses = vec![
        response("r1", RUI, AT + 10, Going),
        response("m1", ME, AT + 11, NotGoing),
        response("a1", ANA, AT + 12, Maybe),
    ];
    let history = convert::message(&info, &Me::new([ME.to_string()]), false);
    apply(&store, history.clone()).await;
    assert_eq!(counts(&store, "e1").await, (1, 1, 1, Value::from("not_going")));
    let guests: i64 =
        query(&store, |db| Ok(db.query_row("SELECT sum(guests) FROM event_answers", [], |r| r.get(0))?)).await;
    assert_eq!(guests, 6, "with the guests each brings");
    let v = shown(&store, "e1").await.1;
    assert_eq!(apply(&store, history).await.changed, 0, "history bringing it again changes nothing");
    assert_eq!(shown(&store, "e1").await.1, v);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_from_its_author_changes_what_the_event_shows_the_newest_winning_and_a_copy_arriving_again_keeps_it() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, edit("e1", ANA, "x1", AT + 60_000, "Sintra, later", false)).await;
    let (shows, v, edited) = shown(&store, "e1").await;
    assert_eq!(
        (&shows["name"], &shows["start"], edited),
        (&Value::from("Sintra, later"), &Value::from((START + 3_600) * 1000), true)
    );
    apply(&store, edit("e1", ANA, "x2", AT + 120_000, "Sintra, later", true)).await;
    let (shows, cancelled, _) = shown(&store, "e1").await;
    assert_eq!(shows["cancelled"], true, "an edit cancels it");
    assert!(cancelled > v);
    let summary: String =
        query(&store, |db| Ok(db.query_row("SELECT summary FROM messages WHERE id = 'e1'", [], |r| r.get(0))?)).await;
    assert_eq!(summary, "Event cancelled: Sintra, later");
    // An older edit arriving late, or the newest again, changes nothing.
    for late in [
        edit("e1", ANA, "x1", AT + 60_000, "Sintra, later", false),
        edit("e1", ANA, "x2", AT + 120_000, "Sintra, later", true),
    ] {
        assert_eq!(apply(&store, late).await.changed, 0);
        assert_eq!(shown(&store, "e1").await.1, cancelled);
    }
    // A copy of the event arriving again, as history brings one, keeps what its edit made it.
    apply(&store, convert::message(&event_from("e1", ANA), &Me::new([ME.to_string()]), false)).await;
    let (shows, ..) = shown(&store, "e1").await;
    assert_eq!((&shows["name"], &shows["cancelled"]), (&Value::from("Sintra, later"), &Value::from(true)));
}

#[tokio::test(flavor = "multi_thread")]
async fn what_doesnt_open_with_the_events_secret_or_isnt_its_authors_edit_is_kept_sealed_and_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    // Rui's answer sealed as if for another event.
    apply(&store, sealed_answer("e1", (RUI, RUI), RUI, "r1", AT + 10, Going)).await;
    // Rui can't edit Ana's event, though his edit opens with its secret.
    apply(&store, edit("e1", RUI, "x1", AT + 20, "Rui's now", true)).await;
    let (shows, _, edited) = shown(&store, "e1").await;
    assert_eq!(
        (counts(&store, "e1").await, &shows["name"], edited),
        ((0, 0, 0, Value::Null), &Value::from("Sintra"), false)
    );
    assert_eq!(
        store.count("pending WHERE kind IN ('rsvp', 'secret')").unwrap(),
        2,
        "each kept sealed, as jawd status counts them"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_deleted_for_everyone_counts_no_answer_and_takes_no_edit() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    let revoke = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(key("e1", ANA)),
            r#type: Some(e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    apply(
        &store,
        live(WebMessageInfo {
            key: key("d1", ANA),
            message: Some(revoke),
            message_timestamp: Some((AT / 1000) as u64 + 30),
            ..Default::default()
        }),
    )
    .await;
    apply(&store, answer("e1", ANA, "a1", AT + 40, Going)).await;
    apply(&store, edit("e1", ANA, "x1", AT + 50, "Back", false)).await;
    // History's answer on a copy of it, opened already, counts nothing either.
    let mut copy = event_from("e1", ANA);
    copy.event_responses = vec![EventResponse {
        event_response_message_key: Some(key("h1", RUI)),
        timestamp_ms: Some(AT + 60),
        event_response_message: Some(e2e::EventResponseMessage { response: Some(Maybe as i32), ..Default::default() }),
        unread: None,
    }];
    apply(&store, convert::message(&copy, &Me::new([ME.to_string()]), false)).await;
    let row = query(&store, |db| Ok(read::message(db, GROUP, "e1")?.expect("its row stays").0)).await;
    assert!(row.deleted && row.detail.is_none(), "it shows deleted, with no counts");
    assert_eq!(store.count("pending").unwrap(), 0, "and what came for it after has nothing to wait for");
    let answered: Vec<(String, String)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT id, answer FROM event_answers")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(answered, [("r1".to_string(), "going".to_string())], "only the answer from before it went");
}

/// Fills `pending` with what's kept sealed as `kind`, up to one short of its bound, for events jaw
/// hasn't seen.
fn fill_sealed(home: &std::path::Path, kind: &str) {
    let raw = Connection::open(home.join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    raw.execute(
        "WITH RECURSIVE k(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM k WHERE i < ?1)
        INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
        SELECT ?2, 'x' || i, ?5, ?3, x'00', ?4, 0, json_object('do', 'keep', 'kind', ?5)
        FROM k",
        rusqlite::params![ANSWERS_WAITING, GROUP, AT, RUI, kind],
    )
    .unwrap();
}

/// Fills `pending` with sealed answers, as `fill_sealed` does.
fn fill_pending(home: &std::path::Path) {
    fill_sealed(home, "rsvp");
}

#[tokio::test(flavor = "multi_thread")]
async fn past_their_bound_the_oldest_answer_waiting_is_set_aside_whole_and_counts_once_its_event_comes() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    fill_pending(home.path());
    let bound = ANSWERS_WAITING as u64;
    assert_eq!(store.count("pending WHERE kind = 'rsvp'").unwrap(), bound);
    apply(&store, answer("e2", ANA, "a1", AT + 20, Maybe)).await;
    assert_eq!(store.count("pending WHERE kind = 'rsvp'").unwrap(), bound, "pending stays within its bound");
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 1, "as jawd status counts it");
    apply(&store, event("e1")).await;
    assert_eq!(counts(&store, "e1").await, (1, 0, 0, Value::Null), "it comes back with its event, and counts");
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 0);
    // Opened answers waiting are bounded alike.
    let history = |event: &str, who: &str| {
        let mut info = event_from(event, ANA);
        info.event_responses = vec![EventResponse {
            event_response_message_key: Some(key("h1", who)),
            timestamp_ms: Some(AT),
            event_response_message: Some(e2e::EventResponseMessage {
                response: Some(Going as i32),
                ..Default::default()
            }),
            unread: None,
        }];
        convert::message(&info, &Me::new([ME.to_string()]), false)
            .into_iter()
            .filter(|change| matches!(change, Change::Amend(_)))
            .collect::<Vec<_>>()
    };
    apply(&store, history("e3", RUI)).await;
    assert_eq!(store.count("pending WHERE kind = 'answer'").unwrap(), 1, "an opened one waits for its event too");
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    raw.execute(
        "WITH RECURSIVE k(i) AS (SELECT 2 UNION ALL SELECT i + 1 FROM k WHERE i < ?1)
        INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
        SELECT ?2, 'y' || i, 'answer', ?3, x'', ?4, 0,
            json_object('do', 'answer', 'answer', 'going', 'guests', 0, 'id', 'h' || i)
        FROM k",
        rusqlite::params![ANSWERS_WAITING, GROUP, AT, RUI],
    )
    .unwrap();
    drop(raw);
    apply(&store, history("e4", ANA)).await;
    assert_eq!(store.count("pending WHERE kind = 'answer'").unwrap(), bound, "and are kept within their bound alike");
    let kept: String = query(&store, |db| {
        Ok(db.query_row("SELECT ref FROM quarantine WHERE source = 'answer'", [], |r| r.get::<_, String>(0))?)
    })
    .await;
    assert_eq!(kept, format!("{GROUP} e3 {RUI} h1"), "the oldest set aside, by its event, its answerer and its ID");
}

#[tokio::test(flavor = "multi_thread")]
async fn past_their_bound_the_oldest_edit_waiting_is_set_aside_whole_and_opens_once_its_event_comes() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // Ana's edit comes before her event, and waits, sealed, and then as many as fill the bound.
    apply(&store, edit("e1", ANA, "x1", AT + 60_000, "Sintra, later", true)).await;
    fill_sealed(home.path(), "secret");
    let bound = ANSWERS_WAITING as u64;
    assert_eq!(store.count("pending WHERE kind = 'secret'").unwrap(), bound);
    apply(&store, edit("e2", RUI, "x2", AT + 60_000, "Rui's now", false)).await;
    assert_eq!(store.count("pending WHERE kind = 'secret'").unwrap(), bound, "pending stays within its bound");
    let kept: Vec<String> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref FROM quarantine WHERE source = 'edit'")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [format!("{GROUP} e1 {ANA} x1")], "the oldest set aside, by its event, its editor and its ID");
    apply(&store, event("e1")).await;
    let (shows, _, edited) = shown(&store, "e1").await;
    assert_eq!(
        (&shows["name"], &shows["cancelled"], edited),
        (&Value::from("Sintra, later"), &Value::from(true), true),
        "it comes back with its event, and opens"
    );
    assert_eq!(store.count("quarantine WHERE source = 'edit'").unwrap(), 0, "as jawd status counts them");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_set_aside_in_a_chat_merged_since_comes_back_with_its_event() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let text = |chat: &str, id: &str| {
        live(WebMessageInfo {
            key: direct(chat, id),
            message: Some(e2e::Message { conversation: Some("olá".into()), ..Default::default() }),
            message_timestamp: Some((AT / 1000) as u64),
            ..Default::default()
        })
    };
    apply(&store, [text(ANA, "t1"), text(ANA_LID, "t2")].concat()).await;
    apply(&store, in_chat(ANA_LID, edit_info("e1", (ANA_LID, ANA_LID), "x1", AT + 60_000, "Sintra, later", false)))
        .await;
    fill_sealed(home.path(), "secret");
    apply(&store, edit("e9", RUI, "x9", AT + 60_000, "Rui's now", false)).await;
    assert_eq!(store.count("quarantine WHERE source = 'edit'").unwrap(), 1, "hers is set aside");
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    merged(&store).await;
    let kept: String = query(&store, |db| {
        Ok(db.query_row("SELECT ref FROM quarantine WHERE source = 'edit'", [], |r| r.get::<_, String>(0))?)
    })
    .await;
    assert_eq!(kept, format!("{ANA} e1 {ANA_LID} x1"), "it waits in the chat kept");
    apply(&store, in_chat(ANA, event_from("e1", ANA))).await;
    let row = query(&store, |db| Ok(read::message(db, ANA, "e1")?.expect("the event").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(
        (&detail["event"]["name"], row.edited),
        (&Value::from("Sintra, later"), true),
        "and opens with it there"
    );
    assert_eq!(store.count("quarantine WHERE source = 'edit'").unwrap(), 0);
}

/// What's set aside for event `id`, by name.
async fn set_aside_for(store: &Store, id: &'static str) -> Vec<String> {
    query(store, move |db| {
        Ok(db
            .prepare("SELECT ref FROM quarantine WHERE source = 'answer' AND raw ->> '$.target' = ?1 ORDER BY ref")?
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_set_aside_once_pending_has_given_its_n_again_is_kept_beside_the_one_set_aside_with_it_before() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let waiting_as = |home: &std::path::Path| -> i64 {
        let raw = Connection::open(home.join("jaw.db")).unwrap();
        raw.query_row("SELECT n FROM pending WHERE target = 'e1'", [], |r| r.get(0)).unwrap()
    };
    // Rui's sealed answer to e1, which jaw hasn't seen, waits, and past the bound it's set aside.
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    let first = waiting_as(home.path());
    fill_pending(home.path());
    apply(&store, answer("e2", ANA, "a2", AT + 20, Maybe)).await;
    assert_eq!(set_aside_for(&store, "e1").await.len(), 1, "his first is set aside");
    // What waits goes as its events come, so pending gives its n's again from the first; Rui's newer
    // answer waits as his first did, and is set aside past the bound in turn.
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    raw.execute("DELETE FROM pending", []).unwrap();
    drop(raw);
    apply(&store, answer("e1", RUI, "r2", AT + 40, NotGoing)).await;
    assert_eq!(waiting_as(home.path()), first, "it waits under the n the first had");
    fill_pending(home.path());
    apply(&store, answer("e3", ANA, "a3", AT + 50, Maybe)).await;
    assert_eq!(
        set_aside_for(&store, "e1").await,
        [format!("{GROUP} e1 {RUI} r1"), format!("{GROUP} e1 {RUI} r2")],
        "both kept, each by its own ID, neither in place of the other"
    );
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 0);
    apply(&store, event("e1")).await;
    assert_eq!(counts(&store, "e1").await, (0, 0, 1, Value::Null), "both come back with it, and his newest counts");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_set_aside_under_a_name_another_holds_is_named_apart_and_the_same_one_again_is_kept_once() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let drained = || {
        let raw = Connection::open(home.path().join("jaw.db")).unwrap();
        raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
        raw.execute("DELETE FROM pending", []).unwrap();
    };
    let past_the_bound = |by: &'static str| {
        fill_pending(home.path());
        apply(&store, answer("e9", ANA, by, AT + 90, Maybe))
    };
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    past_the_bound("a1").await;
    drained();
    // Another answer of his under the same ID, as a phone that gave it again would send.
    apply(&store, answer("e1", RUI, "r1", AT + 30, Maybe)).await;
    past_the_bound("a2").await;
    let aside = set_aside_for(&store, "e1").await;
    assert_eq!(aside.len(), 2, "kept apart: {aside:?}");
    assert_eq!(aside[0], format!("{GROUP} e1 {RUI} r1"));
    assert!(aside[1].starts_with(&format!("{GROUP} e1 {RUI} r1 #")), "named by what it holds after: {aside:?}");
    // The same one again is the row already kept, and leaves pending all the same.
    drained();
    apply(&store, answer("e1", RUI, "r1", AT + 30, Maybe)).await;
    past_the_bound("a3").await;
    assert_eq!(set_aside_for(&store, "e1").await, aside, "kept once");
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 0, "and gone from pending");
    apply(&store, event("e1")).await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 0, Value::Null), "his newest counts once both come back");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_whose_two_names_others_hold_stays_in_pending_rather_than_go_where_nothing_keeps_it() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    // Rows that aren't his hold both names his answer would be set aside under, its own by its ID
    // and the one after it by what it holds, as only a hash's collision could make them.
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    let whole: String = raw.query_row(OLDEST, rusqlite::params!["rsvp", 1], |r| r.get(6)).unwrap();
    let named = format!("{GROUP} e1 {RUI} r1");
    for name in [named.clone(), format!("{named} #{}", digest(&whole))] {
        raw.execute(
            "INSERT INTO quarantine (at, source, ref, error, raw) VALUES (0, 'answer', ?1, 'held by another', jsonb('{}'))",
            [name],
        )
        .unwrap();
    }
    drop(raw);
    fill_pending(home.path());
    apply(&store, answer("e2", ANA, "a2", AT + 20, Maybe)).await;
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 1, "past the bound, it stays in pending");
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 2, "where nothing else keeps it");
}

#[tokio::test(flavor = "multi_thread")]
async fn one_persons_answers_from_his_number_and_his_lid_count_once_and_their_join_gives_the_event_a_new_version() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    // Before jaw knows they're one person, each JID's answer counts. Rui didn't send the event, so
    // his joining changes nothing else its row shows.
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    apply(&store, answer("e1", RUI_LID, "r2", AT + 20, NotGoing)).await;
    assert_eq!(counts(&store, "e1").await, (1, 0, 1, Value::Null));
    let v = shown(&store, "e1").await.1;
    apply(&store, vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })]).await;
    assert_eq!(counts(&store, "e1").await, (0, 0, 1, Value::Null), "once one person, his newest alone");
    assert!(shown(&store, "e1").await.1 > v, "and the event shows it with a new version");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_set_aside_in_a_chat_merged_since_comes_back_with_its_event() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // A chat under Ana's LID and one under her number, which merge once jaw knows they're hers.
    let direct = |chat: &str, id: &str| {
        live(WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(false),
                id: Some(id.into()),
                participant: None,
            },
            message: Some(e2e::Message { conversation: Some("olá".into()), ..Default::default() }),
            message_timestamp: Some((AT / 1000) as u64),
            ..Default::default()
        })
    };
    apply(&store, [direct(ANA, "t1"), direct(ANA_LID, "t2")].concat()).await;
    let in_her_lid_chat = |changes: Vec<Change>| -> Vec<Change> {
        changes
            .into_iter()
            .map(|change| match change {
                Change::Amend(mut amend) => {
                    amend.chat = ANA_LID.into();
                    Change::Amend(amend)
                }
                other => other,
            })
            .collect()
    };
    apply(&store, in_her_lid_chat(answer("e1", ANA_LID, "a1", AT + 10, Going))).await;
    fill_pending(home.path());
    apply(&store, answer("e9", RUI, "r9", AT + 50, Going)).await;
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 1, "hers is set aside");
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    merged(&store).await;
    let kept: String = query(&store, |db| {
        Ok(db.query_row("SELECT ref FROM quarantine WHERE source = 'answer'", [], |r| r.get::<_, String>(0))?)
    })
    .await;
    assert_eq!(kept, format!("{ANA} e1 {ANA_LID} {}", kept.rsplit(' ').next().unwrap()), "it waits in the chat kept");
    let mut theirs = event_from("e1", ANA);
    theirs.key =
        MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some("e1".into()), participant: None };
    apply(&store, live(theirs)).await;
    let row = query(&store, |db| Ok(read::message(db, ANA, "e1")?.expect("the event").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["event"]["going"], 1, "it came back with its event and counts");
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 0);
}

/// What `info` makes arriving live in Ana's one-to-one chat `chat`.
fn in_chat(chat: &str, mut info: WebMessageInfo) -> Vec<Change> {
    (info.key.remote_jid, info.key.participant) = (Some(chat.into()), None);
    live(info)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_events_copy_in_a_chat_merged_folds_in_its_answers_each_answerers_newest_counting_and_its_newer_edit() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // Ana's event in her number's chat and in her LID's, which merges into it once jaw knows
    // they're hers; each copy's answers sealed for Ana as it came, and her edit of the one that goes.
    apply(&store, in_chat(ANA, event_from("e1", ANA))).await;
    apply(&store, in_chat(ANA_LID, event_from("e1", ANA))).await;
    apply(&store, in_chat(ANA, answer_info("e1", (ANA, ANA), ANA, "a2", AT + 20, Going))).await;
    apply(&store, in_chat(ANA_LID, answer_info("e1", (ANA_LID, ANA_LID), ANA_LID, "a1", AT + 10, NotGoing))).await;
    apply(&store, in_chat(ANA_LID, answer_info("e1", (ANA_LID, ME), ME, "m1", AT + 30, Maybe))).await;
    apply(&store, in_chat(ANA_LID, edit_info("e1", (ANA_LID, ANA_LID), "x1", AT + 60_000, "Sintra, later", true)))
        .await;
    assert_eq!(store.count("event_answers").unwrap(), 3, "each copy's answers opened");
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    merged(&store).await;
    assert_eq!(store.count("messages WHERE id = 'e1'").unwrap(), 1, "one event, in her number's chat");
    let row = query(&store, |db| Ok(read::message(db, ANA, "e1")?.expect("the event").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().expect("an event's detail").get()).unwrap();
    let event = &detail["event"];
    assert_eq!(
        (&event["going"], &event["maybe"], &event["not_going"], &event["mine"]),
        (&Value::from(1), &Value::from(1), &Value::from(0), &Value::from("maybe")),
        "her newest answer, from either copy, and yours: {event}"
    );
    assert_eq!(
        (&event["name"], &event["start"], &event["cancelled"], row.edited),
        (&Value::from("Sintra, later"), &Value::from((START + 3_600) * 1000), &Value::from(true), true),
        "and what her edit made it, where and when as well as its name"
    );
}

/// `count` answers to Ana's event `event`, going, each sealed for real by an answerer of its own and
/// waiting in `pending` as a live one does, written beside the writer as `fill_sealed` writes its.
fn waiting_for(home: &std::path::Path, event: &str, count: usize) {
    waiting_in(home, GROUP, event, ANA, count, 0, AT + 1_000);
}

/// As `waiting_for`, in chat `chat`, sealed for Ana as `author`, the JID WhatsApp took the event's
/// author as, by answerers numbered from `from`, the first answering at `at` and each the next
/// millisecond.
fn waiting_in(home: &std::path::Path, chat: &str, event: &str, author: &str, count: usize, from: usize, at: i64) {
    let mut raw = Connection::open(home.join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    let tx = raw.transaction().unwrap();
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
                VALUES (?1, ?2, 'rsvp', ?3, ?4, ?5, 0, json_object('do', 'keep', 'kind', 'rsvp'))",
            )
            .unwrap();
        for i in from..from + count {
            let who = format!("3519300{i:05}@s.whatsapp.net");
            let at = at + (i - from) as i64;
            let mut info = answer_info(event, (author, &who), &who, &format!("{event}-{i}"), at, Going);
            info.key.remote_jid = Some(chat.into());
            let sealed = Packed::new(&info.encode_to_vec()).0;
            insert.execute(rusqlite::params![chat, event, at / 1000 * 1000, sealed, who]).unwrap();
        }
    }
    tx.commit().unwrap();
}

/// Takes the steps that take what's left waiting for events until none is, failing at once should
/// one fail, and once there have been more than they can need. Says how many there were.
async fn taken(store: &Store) -> usize {
    let (mut steps, mut taken) = (store.opening_steps(), 0);
    while store.count("openings").unwrap() > 0 {
        steps.take("what waited for the event was never all taken");
        let applied = store.apply(Lane::Background, vec![Change::Opening]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step failed: {applied:?}");
        taken += 1;
    }
    taken
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_arriving_with_more_waiting_than_its_transaction_takes_has_the_rest_taken_in_steps_from_where_it_got_to()
 {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // A hundred answers wait for e1, sealed, and past the bound the oldest of them are set aside.
    waiting_for(home.path(), "e1", 100);
    fill_pending(home.path());
    apply(&store, answer("e9", RUI, "r9", AT + 10, Going)).await;
    let aside = store.count("quarantine WHERE source = 'answer'").unwrap();
    assert_eq!(aside, SET_ASIDE_MOST as u64, "one change sets aside so many at most, the next more");
    apply(&store, event("e1")).await;
    assert_eq!(counts(&store, "e1").await.0, TAKEN_AT_ONCE as i64, "its own transaction takes so many, counting them");
    assert_eq!(store.count("openings").unwrap(), 1, "and leaves the rest to steps, as jawd status counts them");
    // A step goes on from where it got to, a jawd started again too.
    let applied = store.apply(Lane::Background, vec![Change::Opening]).await.unwrap();
    assert_eq!(applied.quarantined, 0);
    let stepped = counts(&store, "e1").await.0;
    assert!(stepped > TAKEN_AT_ONCE as i64, "a step takes more: {stepped}");
    drop(store);
    let store = Store::open(home.path()).unwrap();
    assert!(taken(&store).await >= 1);
    assert_eq!(counts(&store, "e1").await, (100, 0, 0, Value::Null), "each taken once, those set aside too");
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 0);
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 0);
    assert_eq!(store.count("openings").unwrap(), 0, "and nothing is left to take");
    assert_eq!(apply(&store, vec![Change::Opening]).await.changed, 0, "a step then does nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn steps_go_on_from_where_the_last_got_to_past_more_than_a_step_takes_of_what_doesnt_open() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // 150 answers to e1 sealed for Ana's LID, which jaw doesn't know is hers, wait ahead of ten
    // sealed for her number: more that stays sealed than a step takes, before what opens.
    waiting_in(home.path(), GROUP, "e1", ANA_LID, 150, 0, AT + 1_000);
    waiting_in(home.path(), GROUP, "e1", ANA, 10, 500, AT + 2_000);
    apply(&store, event("e1")).await;
    taken(&store).await;
    assert_eq!(counts(&store, "e1").await, (10, 0, 0, Value::Null), "what opens counts");
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 150, "what doesn't is kept");
    assert_eq!(store.count("unopened").unwrap(), 1, "and noted, to be tried again");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_was_sealed_between_jids_jaw_learns_are_its_people_only_afterwards_opens_once_it_does() {
    const DAN: &str = "351910000005@s.whatsapp.net";
    const BEA: &str = "351910000004@s.whatsapp.net";
    const BEA_LID: &str = "100000000004@lid";
    const CAL: &str = "351910000006@s.whatsapp.net";
    const CAL_LID: &str = "100000000006@lid";
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let joined =
        |jids: [&str; 2]| vec![Change::Person(Person { jids: jids.map(str::to_string).to_vec(), ..Person::default() })];
    // Ana's events, known by her number as they came. Dan's answer to e1 comes after it and Rui's to
    // e2 before it, each sealed for her LID; Bea's to e1 is sealed for hers though it comes by her
    // number; and Ana edits e1 by her LID. Jaw knows none of these JIDs as theirs yet.
    apply(&store, event("e1")).await;
    apply(&store, sealed_answer("e1", (ANA_LID, DAN), DAN, "d1", AT + 20, Going)).await;
    apply(&store, sealed_answer("e1", (ANA, BEA_LID), BEA, "b1", AT + 30, Maybe)).await;
    apply(&store, live(edit_info("e1", (ANA_LID, ANA_LID), "x1", AT + 60_000, "Sintra, later", false))).await;
    apply(&store, sealed_answer("e2", (ANA_LID, RUI), RUI, "r1", AT + 10, Going)).await;
    apply(&store, event("e2")).await;
    assert_eq!(
        (counts(&store, "e1").await, counts(&store, "e2").await),
        ((0, 0, 0, Value::Null), (0, 0, 0, Value::Null)),
        "none opens yet"
    );
    assert_eq!(store.count("pending WHERE kind IN ('rsvp', 'secret')").unwrap(), 4, "each kept sealed");
    assert_eq!(store.count("unopened").unwrap(), 2, "as jawd status counts their events");
    // Jaw learns Ana's LID is hers: what's sealed for her events by it opens.
    apply(&store, joined([ANA, ANA_LID])).await;
    taken(&store).await;
    let (shows, _, edited) = shown(&store, "e1").await;
    assert_eq!(
        (counts(&store, "e1").await, &shows["name"], edited),
        ((1, 0, 0, Value::Null), &"Sintra, later".into(), true)
    );
    assert_eq!(counts(&store, "e2").await, (1, 0, 0, Value::Null), "the one that waited for its event too");
    // Someone else's LID changes nothing of it, and asks for no step.
    apply(&store, joined([CAL, CAL_LID])).await;
    assert_eq!(store.count("openings").unwrap(), 0, "nothing of theirs is kept");
    // And Bea's, once jaw learns hers.
    apply(&store, joined([BEA, BEA_LID])).await;
    taken(&store).await;
    assert_eq!(counts(&store, "e1").await, (1, 1, 0, Value::Null));
    assert_eq!(store.count("pending").unwrap(), 0);
    assert_eq!(store.count("unopened").unwrap(), 0, "nothing kept for it stays sealed");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_join_leaves_to_try_again_is_taken_in_the_background_with_nothing_else_to_wake_it() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    tokio::spawn(crate::sync::backfill::run(store.clone()));
    // Five answers to Ana's event come sealed for her LID, which jaw doesn't know is hers, and are
    // kept; the background, with nothing to do, waits to be told.
    apply(&store, event("e1")).await;
    for i in 0..5 {
        let who = format!("3519300{i:05}@s.whatsapp.net");
        apply(&store, sealed_answer("e1", (ANA_LID, &who), &who, &format!("a{i}"), AT + 10 + i, Going)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(counts(&store, "e1").await.0, 0, "they stay sealed");
    // Jaw learns her LID is hers, and the join leaves them to be tried again in steps.
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    let began = std::time::Instant::now();
    while counts(&store, "e1").await.0 < 5 {
        let going = counts(&store, "e1").await.0;
        assert!(began.elapsed() < std::time::Duration::from_secs(30), "the background never took them: {going} going");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_kept_sealed_and_then_set_aside_past_the_bound_opens_once_jaw_learns_its_answerers_jid() {
    const BEA: &str = "351910000004@s.whatsapp.net";
    const BEA_LID: &str = "100000000004@lid";
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // Bea's answer to Ana's event comes by her number sealed for her LID, which jaw doesn't know is
    // hers, so it's kept; then pending passes its bound and sets it aside, the oldest waiting.
    apply(&store, event("e1")).await;
    apply(&store, sealed_answer("e1", (ANA, BEA_LID), BEA, "b1", AT + 30, Maybe)).await;
    assert_eq!(store.count("unopened").unwrap(), 1, "it's kept sealed");
    fill_pending(home.path());
    apply(&store, answer("e9", RUI, "r9", AT + 50, Going)).await;
    assert_eq!(set_aside_for(&store, "e1").await, [format!("{GROUP} e1 {BEA} b1")], "and set aside");
    // Jaw learns her LID is hers: it's tried again where it's set aside, as it would be in pending.
    apply(&store, vec![Change::Person(Person { jids: vec![BEA.into(), BEA_LID.into()], ..Person::default() })]).await;
    taken(&store).await;
    assert_eq!(counts(&store, "e1").await, (0, 1, 0, Value::Null), "it opens and counts");
    assert!(set_aside_for(&store, "e1").await.is_empty());
    assert_eq!(store.count("unopened").unwrap(), 0, "and nothing kept for its event stays sealed");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_waits_for_an_event_and_fails_to_be_taken_is_kept_in_quarantine_and_holds_up_nothing_after_it() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // An answer an earlier jawd kept wrong, which event_answers refuses, first among those that open.
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    raw.execute(
        "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect) VALUES (?1, 'e1', 'answer', ?2, x'', ?3, 0,
            json_object('do', 'answer', 'answer', 'perhaps', 'guests', 0, 'id', 'h0'))",
        rusqlite::params![GROUP, AT + 1, RUI],
    )
    .unwrap();
    drop(raw);
    waiting_for(home.path(), "e1", 30);
    apply(&store, event("e1")).await;
    taken(&store).await;
    assert_eq!(counts(&store, "e1").await, (30, 0, 0, Value::Null), "the rest are taken");
    let kept: Vec<(String, String)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, error FROM quarantine WHERE source = 'opening'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept.len(), 1, "the one that failed is kept in quarantine, with why: {kept:?}");
    assert!(kept[0].0.starts_with(&format!("{GROUP} e1 ")), "{kept:?}");
    assert_eq!(store.count("pending WHERE target = 'e1'").unwrap(), 1, "and left where it was");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_an_earlier_jawd_kept_for_an_event_its_backfill_reads_again_is_taken_a_few_at_once_too() {
    let home = tempfile::tempdir().unwrap();
    before_answers(home.path());
    waiting_for(home.path(), "e2", 40);
    let store = Store::open(home.path()).unwrap();
    backfilled(&store).await;
    assert_eq!(
        counts(&store, "e2").await.0,
        TAKEN_AT_ONCE as i64 - 1,
        "its step takes as the event's own transaction would, the kept one that doesn't open first"
    );
    assert_eq!(store.count("openings").unwrap(), 1, "and leaves the rest to steps");
    taken(&store).await;
    assert_eq!(counts(&store, "e2").await, (40, 0, 0, Value::Null));
    assert_eq!(store.count("pending WHERE target = 'e2'").unwrap(), 1, "the one that doesn't open stays kept");
}

/// An event arriving live with JAW_MEASURE_WAITING answers waiting for it (a comma-separated list),
/// each sealed by an answerer of its own: its own transaction, and the rest taken as sync::backfill
/// takes them, 64 steps to an apply, the writer fitting as many in each transaction as its budget
/// allows.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn measure_an_event_arriving_with_answers_waiting() {
    let list = std::env::var("JAW_MEASURE_WAITING").unwrap_or_else(|_| "100,1000,9999".into());
    for waiting in list.split(',').map(|n| n.trim().parse::<usize>().unwrap()) {
        let home = tempfile::tempdir().unwrap();
        let store = opened(home.path()).await;
        waiting_for(home.path(), "x1", waiting);
        let began = std::time::Instant::now();
        apply(&store, live(event_from("x1", ANA))).await;
        let arrived = began.elapsed();
        let began = std::time::Instant::now();
        let (mut transactions, mut applies) = (0, 0);
        while store.count("openings").unwrap() > 0 {
            let applied = store.apply(Lane::Background, vec![Change::Opening; 64]).await.unwrap();
            assert_eq!(applied.quarantined, 0, "a step failed");
            (transactions, applies) = (transactions + applied.transactions, applies + 1);
        }
        let rest = began.elapsed();
        let run = store.report().run.unwrap();
        assert_eq!(store.count("event_answers").unwrap() as usize, waiting, "each opened");
        eprintln!(
            "MEASURE: an event arriving with {waiting} sealed answers waiting: its own transaction applied in {:.2} ms; \
             the rest in {:.0} ms, {transactions} transactions in {applies} applies; the writer's last 1,024 \
             transactions {:.2} ms p50, {:.2} ms p99",
            arrived.as_secs_f64() * 1e3,
            rest.as_secs_f64() * 1e3,
            run.p50_us as f64 / 1e3,
            run.p99_us as f64 / 1e3,
        );
    }
}

/// Message `id` from the other side of one-to-one chat `chat`.
fn direct(chat: &str, id: &str) -> MessageKey {
    MessageKey { remote_jid: Some(chat.into()), from_me: Some(false), id: Some(id.into()), participant: None }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_deleted_for_everyone_stays_deleted_through_a_fold_with_its_copy_edited_since() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, in_chat(ANA, event_from("e1", ANA))).await;
    apply(&store, in_chat(ANA_LID, event_from("e1", ANA))).await;
    // Ana deletes it for everyone in her number's chat, which is kept, and her edit reaches her LID's.
    let revoke = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(direct(ANA, "e1")),
            r#type: Some(e2e::protocol_message::Type::Revoke as i32),
            ..Default::default()
        })),
        ..Default::default()
    };
    let deleting = WebMessageInfo {
        key: direct(ANA, "d1"),
        message: Some(revoke),
        message_timestamp: Some((AT / 1000) as u64 + 30),
        ..Default::default()
    };
    apply(&store, live(deleting)).await;
    apply(&store, in_chat(ANA_LID, edit_info("e1", (ANA_LID, ANA_LID), "x1", AT + 60_000, "Gone zebra", false))).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    merged(&store).await;
    let kept: (bool, Option<String>, Option<String>, bool) = query(&store, |db| {
        Ok(db.query_row("SELECT deleted, text, detail, raw IS NULL FROM messages WHERE id = 'e1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?)
    })
    .await;
    assert_eq!(kept, (true, None, None, true), "still deleted, with none of its copy's words, detail or raw protobuf");
    store.indexed().await.unwrap();
    let found = query(&store, |db| {
        Ok(read::search(db, "zebra", None, None, 10)?.hits.into_iter().map(|h| h.message.id).collect::<Vec<_>>())
    })
    .await;
    assert!(found.is_empty(), "and search finds nothing of it: {found:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fold_keeps_the_kept_copys_newer_answer_from_the_same_jid_and_its_newer_edit_whole() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, in_chat(ANA, event_from("e1", ANA))).await;
    apply(&store, in_chat(ANA_LID, event_from("e1", ANA))).await;
    // Your newer answer in her number's chat, which is kept, your older in her LID's, folded in.
    apply(&store, in_chat(ANA, answer_info("e1", (ANA, ME), ME, "m2", AT + 20, NotGoing))).await;
    apply(&store, in_chat(ANA_LID, answer_info("e1", (ANA_LID, ME), ME, "m1", AT + 10, Going))).await;
    // Her newer edit of the kept copy, and an older one of the copy that goes.
    apply(&store, in_chat(ANA, edit_info("e1", (ANA, ANA), "x2", AT + 120_000, "Kept edit", false))).await;
    apply(&store, in_chat(ANA_LID, edit_info("e1", (ANA_LID, ANA_LID), "x1", AT + 60_000, "Gone edit", true))).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    merged(&store).await;
    let row = query(&store, |db| Ok(read::message(db, ANA, "e1")?.expect("the event").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
    let event = &detail["event"];
    assert_eq!(event["mine"], "not_going", "your newer answer, from the kept copy: {event}");
    assert_eq!(
        (&event["name"], &event["cancelled"]),
        (&Value::from("Kept edit"), &Value::from(false)),
        "the kept copy's newer edit, whole"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn your_answer_of_none_is_no_answer_of_yours() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, sealed_answer("e1", (ANA, ME), ME, "m1", AT + 10, Going)).await;
    apply(&store, sealed_answer("e1", (ANA, ME), ME, "m2", AT + 20, EventResponseType::Unknown)).await;
    assert_eq!(counts(&store, "e1").await, (0, 0, 0, Value::Null), "your newest says none, so yours is none");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_from_a_jid_found_as_yours_though_the_accounts_list_lacks_it_is_yours() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await;
    apply(&store, event("e1")).await;
    apply(&store, sealed_answer("e1", (ANA, ME), ME, "m1", AT + 10, Going)).await;
    // History brings your newer answer from your LID, which the account's list doesn't have.
    let mut info = event_from("e1", ANA);
    info.event_responses = vec![EventResponse {
        event_response_message_key: Some(MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("m2".into()),
            participant: Some(ME_LID.into()),
        }),
        timestamp_ms: Some(AT + 20),
        event_response_message: Some(e2e::EventResponseMessage {
            response: Some(NotGoing as i32),
            timestamp_ms: Some(AT + 20),
            ..Default::default()
        }),
        unread: None,
    }];
    apply(&store, convert::message(&info, &Me::new([ME.to_string()]), false)).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ME.into(), ME_LID.into()], ..Person::default() })]).await;
    assert_eq!(
        counts(&store, "e1").await,
        (0, 0, 1, Value::from("not_going")),
        "once your LID is found as you, its answer is yours, and the newer"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_its_backfill_reads_again_as_its_chats_last_gives_the_chat_a_new_version() {
    let home = tempfile::tempdir().unwrap();
    before_answers(home.path());
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE chats SET last_message = 2, last_at = ?1 WHERE n = 1", [AT + 2]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    let chat_v =
        || query(&store, |db| Ok(db.query_row("SELECT v FROM chats WHERE n = 1", [], |r| r.get::<_, i64>(0))?));
    let before = chat_v().await;
    backfilled(&store).await;
    assert!(chat_v().await > before, "e2, the chat's last, was read again, and the chat list shows it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_that_counts_an_events_answers_again_gives_the_chat_it_last_shows_a_new_version_too() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, event("e1")).await;
    apply(&store, answer("e1", RUI, "r1", AT + 10, Going)).await;
    apply(&store, answer("e1", RUI_LID, "r2", AT + 20, NotGoing)).await;
    let chat_v =
        || query(&store, |db| Ok(db.query_row("SELECT v FROM chats WHERE id = ?1", [GROUP], |r| r.get::<_, i64>(0))?));
    let before = chat_v().await;
    apply(&store, vec![Change::Person(Person { jids: vec![RUI.into(), RUI_LID.into()], ..Person::default() })]).await;
    assert_eq!(counts(&store, "e1").await, (0, 0, 1, Value::Null), "his newest alone, once he's one person");
    assert!(chat_v().await > before, "and the chat whose last it is shows it with a new version");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_answer_set_aside_under_both_of_a_persons_chats_keeps_a_row_of_each_through_their_merge() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let text = |chat: &str, id: &str| {
        live(WebMessageInfo {
            key: direct(chat, id),
            message: Some(e2e::Message { conversation: Some("olá".into()), ..Default::default() }),
            message_timestamp: Some((AT / 1000) as u64),
            ..Default::default()
        })
    };
    apply(&store, [text(ANA, "t1"), text(ANA_LID, "t2")].concat()).await;
    // History brings Ana's event in each of her chats with her answer on it, opened, without the
    // event itself, so the answer waits under each.
    let answered = |chat: &str| {
        let mut info = event_from("e1", ANA);
        info.key = direct(chat, "e1");
        info.event_responses = vec![EventResponse {
            event_response_message_key: Some(MessageKey { participant: Some(ANA.into()), ..direct(chat, "h1") }),
            timestamp_ms: Some(AT + 10),
            event_response_message: Some(e2e::EventResponseMessage {
                response: Some(Going as i32),
                timestamp_ms: Some(AT + 10),
                ..Default::default()
            }),
            unread: None,
        }];
        convert::message(&info, &Me::new([ME.to_string()]), false)
            .into_iter()
            .filter(|change| matches!(change, Change::Amend(_)))
            .collect::<Vec<_>>()
    };
    apply(&store, answered(ANA_LID)).await;
    apply(&store, answered(ANA)).await;
    // Past the bound, both are set aside.
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    raw.execute(
        "WITH RECURSIVE k(i) AS (SELECT 3 UNION ALL SELECT i + 1 FROM k WHERE i < ?1)
        INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
        SELECT ?2, 'y' || i, 'answer', ?3, x'', ?4, 0,
            json_object('do', 'answer', 'answer', 'going', 'guests', 0, 'id', 'h' || i)
        FROM k",
        rusqlite::params![ANSWERS_WAITING, GROUP, AT, RUI],
    )
    .unwrap();
    drop(raw);
    for k in 0..2 {
        let mut info = event_from(&format!("z{k}"), ANA);
        info.event_responses = vec![EventResponse {
            event_response_message_key: Some(key(&format!("zr{k}"), RUI)),
            timestamp_ms: Some(AT),
            event_response_message: Some(e2e::EventResponseMessage {
                response: Some(Going as i32),
                ..Default::default()
            }),
            unread: None,
        }];
        let amends = convert::message(&info, &Me::new([ME.to_string()]), false)
            .into_iter()
            .filter(|change| matches!(change, Change::Amend(_)))
            .collect::<Vec<_>>();
        apply(&store, amends).await;
    }
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 2, "both copies set aside");
    let joined =
        apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })])
            .await;
    assert_eq!(
        joined.quarantined, 0,
        "the join goes in, the one renamed onto a name taken keeping its own row: {joined:?}"
    );
    merged(&store).await;
    assert_eq!(
        store.count(&format!("chats WHERE id = '{ANA_LID}'")).unwrap(),
        0,
        "her LID's chat merged into her number's"
    );
    assert_eq!(store.count("quarantine WHERE source = 'answer'").unwrap(), 2, "both still set aside, one each");
    let mut theirs = event_from("e1", ANA);
    theirs.key = direct(ANA, "e1");
    apply(&store, live(theirs)).await;
    let row = query(&store, |db| Ok(read::message(db, ANA, "e1")?.expect("the event").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["event"]["going"], 1, "they come back with their event, and count once");
}

/// A jaw.db at the version before events' answers were kept, its feed at seq 1,000, as main's jawd
/// left it: Ana's events e1 and e2 in the group, converted as it converted them (a call link as
/// `link`, no place, nothing counted), e3 deleted for everyone, and what it kept sealed for e1,
/// Rui's answer and Ana's edit, and for e2, an answer that doesn't open; each with its raw.
fn before_answers(home: &std::path::Path) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    let events = schema::MIGRATIONS.iter().position(|m| m.contains("SELECT 'events'")).expect("the events migration");
    for migration in &schema::MIGRATIONS[..events] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", events as i64).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1), (2, 'Rui', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1), (2, '{RUI}', 2);
        INSERT INTO chats (n, id, kind, v) VALUES (1, '{GROUP}', 'group', 1);
        INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');
        INSERT INTO events (seq, at, type, key, chat) VALUES (1000, {AT}, 'chat', 1, 1);"
    ))
    .unwrap();
    let old = serde_json::json!({ "event": {
        "name": "Sintra", "description": "Bring the rope", "start": START * 1000, "end": END * 1000,
        "cancelled": false, "link": null,
    }})
    .to_string();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, deleted, v)
            VALUES (?1, 1, ?2, ?3, ?3, 1, 'event', ?4, ?5, ?6, ?7, ?8, 1)",
        )
        .unwrap();
    for (n, id) in [(1, "e1"), (2, "e2")] {
        let raw = Packed::new(&event_from(id, ANA).encode_to_vec()).0;
        insert
            .execute(rusqlite::params![n, id, AT + n, "Sintra\nBring the rope", "Event: Sintra", old, raw, false])
            .unwrap();
    }
    let no: Option<String> = None;
    insert
        .execute(rusqlite::params![3, "e3", AT + 3, no, "This message was deleted", no, None::<Vec<u8>>, true])
        .unwrap();
    let mut kept = db
        .prepare(
            "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, json_object('do', 'keep', 'kind', ?3))",
        )
        .unwrap();
    let sealed = |info: WebMessageInfo| Packed::new(&info.encode_to_vec()).0;
    kept.execute(rusqlite::params![
        GROUP,
        "e1",
        "rsvp",
        AT + 10,
        sealed(answer_info("e1", (ANA, RUI), RUI, "r1", AT + 10, Going)),
        RUI
    ])
    .unwrap();
    kept.execute(rusqlite::params![
        GROUP,
        "e1",
        "secret",
        AT + 20,
        sealed(edit_info("e1", (ANA, ANA), "x1", AT + 20, "Sintra, later", false)),
        ANA
    ])
    .unwrap();
    kept.execute(rusqlite::params![
        GROUP,
        "e2",
        "rsvp",
        AT + 30,
        sealed(answer_info("e2", (RUI, RUI), RUI, "r2", AT + 30, Going)),
        RUI
    ])
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_events_held_before_are_read_again_and_what_an_earlier_jawd_kept_sealed_for_them_opens() {
    let home = tempfile::tempdir().unwrap();
    before_answers(home.path());
    let store = Store::open(home.path()).unwrap();
    assert_eq!(store.backfill_left(links::EVENTS).unwrap(), 3, "the migration leaves the backfill to do");
    let before = shown(&store, "e2").await;
    assert_eq!(before.0["going"], 0, "counted as it's read, before its backfill too");
    backfilled(&store).await;
    let (e1, _, edited) = shown(&store, "e1").await;
    assert_eq!((&e1["name"], edited), (&Value::from("Sintra, later"), true), "its kept edit opened");
    assert_eq!(counts(&store, "e1").await, (1, 0, 0, Value::Null), "and its kept answer counts");
    let (e2, v, _) = shown(&store, "e2").await;
    assert_eq!(e2["place"]["name"], "Cabo da Roca", "read again, with its place");
    assert!(e2.get("link").is_none() && e2["call"].is_null(), "and its call link as the protocol names it: {e2}");
    assert!(v > before.1, "at a new version");
    assert_eq!(store.count("pending").unwrap(), 1, "an answer that doesn't open stays kept");
    store.indexed().await.unwrap();
    let found = query(&store, |db| {
        Ok(read::search(db, "cabo roca", None, None, 10)?.hits.into_iter().map(|h| h.message.id).collect::<Vec<_>>())
    })
    .await;
    assert_eq!(found, ["e2"], "found by its place; e1 shows its edit, which has none");
    let quarantined = store.count("quarantine").unwrap();
    assert_eq!(quarantined, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_event_edited_before_its_backfill_reaches_it_is_left_as_its_edit_made_it() {
    let home = tempfile::tempdir().unwrap();
    before_answers(home.path());
    let store = Store::open(home.path()).unwrap();
    // Ana's edit of e2 comes once this jawd runs, before the backfill has read e2 again, and opens
    // with its secret; e2's raw protobuf is the event as she first sent it.
    apply(&store, edit("e2", ANA, "x2", AT + 40, "Sintra, earlier", false)).await;
    assert_eq!(shown(&store, "e2").await.0["name"], "Sintra, earlier");
    backfilled(&store).await;
    let (e2, _, edited) = shown(&store, "e2").await;
    assert_eq!((&e2["name"], edited), (&Value::from("Sintra, earlier"), true), "read again, it would be as first sent");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_reading_64_events_in_a_row_again_ends_once_it_has_for_a_while_and_one_that_panics_is_kept_aside() {
    let home = tempfile::tempdir().unwrap();
    before_answers(home.path());
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let old: String = db.query_row("SELECT detail FROM messages WHERE n = 2", [], |r| r.get(0)).unwrap();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, v)
            VALUES (?1, 1, ?2, ?3, ?3, 1, 'event', 'Sintra', 'Event: Sintra', ?4, ?5, 1)",
        )
        .unwrap();
    for n in 10..=300_i64 {
        let id = if n == 40 { kinds::tests::PANICS.to_string() } else { format!("e{n}") };
        let raw = Packed::new(&event_from(&id, ANA).encode_to_vec()).0;
        insert.execute(rusqlite::params![n, id, AT + n, old, raw]).unwrap();
    }
    db.execute("UPDATE backfills SET upto = 300 WHERE name = 'events'", []).unwrap();
    db.execute("UPDATE sqlite_sequence SET seq = 300 WHERE name = 'messages'", []).unwrap();
    drop(insert);
    drop(db);
    let store = Store::open(home.path()).unwrap();
    let after = || {
        query(&store, |db| {
            Ok(db.query_row("SELECT after FROM backfills WHERE name = 'events'", [], |r| r.get::<_, i64>(0))?)
        })
    };
    // Steps until the first three rows are behind them, however few events each reads again on a
    // busy machine, then one whose 64 rows are all events.
    let (mut steps, mut began) = (store.backfill_steps(links::EVENTS), 0);
    while began < 10 {
        steps.take("the backfill never reached the events in a row");
        apply(&store, vec![Change::Backfill(links::EVENTS)]).await;
        began = after().await;
    }
    apply(&store, vec![Change::Backfill(links::EVENTS)]).await;
    let ended = after().await;
    assert!(
        (began + 1..began + 64).contains(&ended),
        "a step reading events in a row again ends once it has for a while, after one at least: {began} to {ended}"
    );
    backfilled(&store).await;
    let placed: i64 = query(&store, |db| {
        Ok(db.query_row(
            "SELECT count(*) FROM messages WHERE n >= 10 AND detail ->> '$.event.place.name' = 'Cabo da Roca'",
            [],
            |r| r.get(0),
        )?)
    })
    .await;
    assert_eq!(placed, 290, "each read again but the one that panicked");
    let kept: Vec<String> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref FROM quarantine WHERE source = 'events'")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [format!("{GROUP} {}", kinds::tests::PANICS)], "which is kept in quarantine");
}
