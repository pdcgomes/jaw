use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action as action;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::drain::{self, Row};
use super::*;
use crate::engine::EngineError;
use crate::store::read::{self, Around};

const ANA: &str = "351910000002@s.whatsapp.net";
const GROUP: &str = "120363000000000001@g.us";
const AT: i64 = 1_792_000_000_000;

#[test]
fn connection_events_set_the_account_and_connection() {
    let mut live = Live::default();
    let connected = pb::Connection {
        state: pb::connection::State::Connected as i32,
        jid: "351910000001:12@s.whatsapp.net".into(),
        push_name: "Pedro".into(),
        ..Default::default()
    };
    apply(&mut live, &connected);
    assert_eq!(live.account.state, AccountState::Linked);
    assert_eq!(live.account.me.as_ref().unwrap().phone, "+351910000001");
    assert_eq!(live.connection.state, ConnectionState::Online);
    let banned = pb::Connection {
        state: pb::connection::State::TemporaryBan as i32,
        reason: "sent to too many".into(),
        until_ms: 99,
        ..Default::default()
    };
    apply(&mut live, &banned);
    assert_eq!(live.connection.state, ConnectionState::Banned);
    assert_eq!((live.connection.until, live.connection.reason.as_deref()), (Some(99), Some("sent to too many")));
    apply(&mut live, &pb::Connection { state: pb::connection::State::LoggedOut as i32, ..Default::default() });
    assert_eq!(live.account.state, AccountState::Unlinked);
}

#[test]
fn a_history_notification_is_found_where_the_phone_puts_it() {
    let notification = e2e::HistorySyncNotification { chunk_order: Some(3), ..Default::default() };
    let protocol = e2e::ProtocolMessage { history_sync_notification: Some(notification.clone()), ..Default::default() };
    let direct = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    let sent = e2e::Message {
        device_sent_message: Some(Box::new(e2e::DeviceSentMessage {
            message: Some(Box::new(direct.clone())),
            ..Default::default()
        })),
        ..Default::default()
    };
    let text = e2e::Message { conversation: Some("hi".into()), ..Default::default() };
    assert_eq!(drain::history_notification(&direct), Some(&notification));
    assert_eq!(drain::history_notification(&sent), Some(&notification));
    assert_eq!(drain::history_notification(&text), None);
}

#[test]
fn catching_up_counts_what_arrives_and_ends_with_one_summary() {
    let live: Shared = Arc::default();
    let (pushes, mut pushed) = broadcast::channel(8);
    {
        let mut live = live.lock().unwrap();
        live.catching_up = Some(Instant::now());
        live.catch_up = Some(CatchUp { total: 3, messages: 3, receipts: 0, stored: 0, seconds: None });
        live.connection.state = ConnectionState::CatchingUp;
        assert!(live.catching_up());
    }
    for chat in [ANA, ANA, GROUP] {
        caught(&live, &pushes, chat);
    }
    assert!(pushed.try_recv().is_err(), "progress goes out every PROGRESS_EVERY messages, not each one");
    let (connection, summary) = caught_up(&mut live.lock().unwrap(), &pb::OfflineDone { count: 3 });
    assert_eq!(connection.state, ConnectionState::Online);
    assert_eq!(summary.as_deref(), Some("While you were away: 3 messages in 2 chats"));
    assert!(!live.lock().unwrap().catching_up());
    assert_eq!(live.lock().unwrap().catch_up.as_ref().unwrap().stored, 3);
}

fn me() -> convert::Me {
    convert::Me::new(["351910000001@s.whatsapp.net".to_string(), "100000000009@lid".to_string()])
}

fn live_message(chat: &str, sender: &str, id: &str, at: i64, message: e2e::Message) -> pb::Message {
    pb::Message {
        info: Some(pb::MessageInfo {
            chat: chat.into(),
            sender: sender.into(),
            from_me: sender.is_empty(),
            group: chat.ends_with("@g.us"),
            id: id.into(),
            timestamp_ms: at,
            push_name: if sender.is_empty() { String::new() } else { "Ana".into() },
            ..Default::default()
        }),
        raw: message.encode_to_vec(),
        ..Default::default()
    }
}

fn text(words: &str) -> e2e::Message {
    e2e::Message { conversation: Some(words.into()), ..Default::default() }
}

async fn query<T: Send + 'static>(
    store: &Store,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..500 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("waited too long for {what}");
}

/// Waits for the converter to empty the inbox, once the inbox has what the test put in it: the
/// writer takes the inbox's queue before the live one, so an empty live write commits after it.
async fn drained(store: &Store) {
    store.apply(Lane::Live, Vec::new()).await.unwrap();
    until("the inbox to empty", || store.count("inbox").unwrap() == 0).await;
}

/// The converter running on `store` as jawd runs it.
fn converting(store: &Store) -> tokio::task::JoinHandle<()> {
    let live: Shared = Arc::default();
    live.lock().unwrap().me = me();
    let (offered, _) = mpsc::unbounded_channel();
    tokio::spawn(drain::inbox(store.clone(), live, offered, Avatars::new(), Merges::default()))
}

fn scalar(home: &Path, sql: &str) -> i64 {
    Connection::open(home.join("jaw.db")).unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_inbox_is_converted_once_even_when_jawd_stops_in_the_middle() {
    let home = tempfile::tempdir().unwrap();
    let reaction = e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(false),
                id: Some("m2".into()),
                participant: None,
            }),
            text: Some("❤".into()),
            sender_timestamp_ms: Some(AT + 1),
            ..Default::default()
        }),
        ..Default::default()
    };
    let events = [
        live_message(ANA, ANA, "m1", AT, text("first")),
        live_message(ANA, "", "r1", AT + 1_000, reaction),
        live_message(ANA, ANA, "m2", AT + 2_000, text("second")),
        live_message(GROUP, "100000000001@lid", "g1", AT + 3_000, text("olá")),
    ];
    {
        let store = Store::open(home.path()).unwrap();
        for event in &events {
            store.inbox("message", 0, event.encode_to_vec(), None);
        }
        until("the inbox", || store.count("inbox").unwrap() == 4).await;
        // jawd stops after the first of the converter's transactions: the first message and its
        // row are in, the rest still waits.
        let rows: Vec<Row> = query(&store, |db| {
            Ok(db
                .prepare("SELECT seq, kind, payload FROM inbox ORDER BY seq")?
                .query_map([], |r| Ok(Row { seq: r.get(0)?, kind: r.get(1)?, payload: r.get(2)? }))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
        let mut changes = drain::convert_rows(&rows, &me()).changes;
        let cut = changes.iter().position(|c| matches!(c, Change::Drained(_))).unwrap() + 1;
        changes.truncate(cut);
        store.apply(Lane::Live, changes).await.unwrap();
        assert_eq!((store.count("messages").unwrap(), store.count("inbox").unwrap()), (1, 3));
    }
    let store = Store::open(home.path()).unwrap();
    let converter = converting(&store);
    drained(&store).await;
    converter.abort();
    assert_eq!(store.count("messages").unwrap(), 3, "each message once");
    assert_eq!(
        scalar(home.path(), "SELECT count(*) FROM reactions WHERE emoji = '❤'"),
        1,
        "the reaction waited for its message"
    );
    assert_eq!(store.count("pending").unwrap(), 0);
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["m1", "m2"]);
    assert_eq!(rows[1].reactions.get(), r#"[{"emoji":"❤","n":1,"mine":true}]"#);
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    let ana = chats.iter().find(|c| c.id == ANA).unwrap();
    assert_eq!(ana.unread, 2, "two of Ana's messages arrived live");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_kept_raw_by_an_earlier_jawd_is_converted_and_removed() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        for n in 0..2_500i64 {
            let info = WebMessageInfo {
                key: MessageKey {
                    remote_jid: Some(ANA.into()),
                    from_me: Some(n % 2 == 0),
                    id: Some(format!("h{n}")),
                    participant: None,
                },
                message: Some(text(&format!("history {n}"))),
                message_timestamp: Some((AT / 1000 - 10_000 + n) as u64),
                ..Default::default()
            };
            db.execute(
                "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![ANA, format!("h{n}"), AT + n, info.encode_to_vec()],
            )
            .unwrap();
        }
        db.execute("INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, 'broken', 0, x'ffff')", [ANA]).unwrap();
    }
    let live: Shared = Arc::default();
    drain::history_raw(store.clone(), live).await;
    assert_eq!(store.count("messages").unwrap(), 2_500);
    assert_eq!(store.count("history_raw").unwrap(), 1, "what doesn't decode stays raw");
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    assert_eq!((chats[0].unread, chats[0].last.as_ref().unwrap().summary.as_str()), (0, "history 2499"));
    store.indexed().await.unwrap();
    let found = query(&store, |db| read::search(db, "history 1234", None, None, 5)).await;
    assert_eq!(found.hits[0].message.id, "h1234");
}

fn app_state(index: &[&str], f: impl FnOnce(&mut action::SyncActionValue)) -> Vec<u8> {
    let mut value = action::SyncActionValue { timestamp: Some(AT), ..Default::default() };
    f(&mut value);
    pb::AppState { index: index.iter().map(|s| s.to_string()).collect(), value: value.encode_to_vec() }.encode_to_vec()
}

#[tokio::test(flavor = "multi_thread")]
async fn app_state_pins_archives_and_marks_chats_as_the_phone_has_them() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let chat = |n: usize| format!("3519100000{n:02}@s.whatsapp.net");
    for n in 0..5 {
        store.inbox(
            "message",
            0,
            live_message(&chat(n), &chat(n), &format!("m{n}"), AT + n as i64 * 1000, text("hi")).encode_to_vec(),
            None,
        );
    }
    let pin = |n: usize, at: i64| {
        app_state(&["pin_v1", &chat(n)], |v| {
            v.timestamp = Some(at);
            v.pin_action = Some(action::PinAction { pinned: Some(true) });
        })
    };
    store.inbox("app_state", 0, pin(0, AT + 10), None);
    store.inbox("app_state", 0, pin(1, AT + 20), None);
    store.inbox(
        "app_state",
        0,
        app_state(&["archive", &chat(4)], |v| {
            v.archive_chat_action = Some(action::ArchiveChatAction { archived: Some(true), ..Default::default() })
        }),
        None,
    );
    store.inbox(
        "app_state",
        0,
        app_state(&["markChatAsRead", &chat(2)], |v| {
            v.timestamp = Some(AT + 10_000);
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction { read: Some(true), ..Default::default() })
        }),
        None,
    );
    store.inbox(
        "app_state",
        0,
        app_state(&["markChatAsRead", &chat(3)], |v| {
            v.mark_chat_as_read_action = Some(action::MarkChatAsReadAction { read: Some(false), ..Default::default() })
        }),
        None,
    );
    store.inbox("app_state", 0, app_state(&["time_format"], |_| {}), None);
    let converter = converting(&store);
    drained(&store).await;
    converter.abort();
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    let ids: Vec<&str> = chats.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, [chat(1), chat(0), chat(3), chat(2)], "pinned first, the latest pinned on top");
    let read = chats.iter().find(|c| c.id == chat(2)).unwrap();
    let unread = chats.iter().find(|c| c.id == chat(3)).unwrap();
    assert_eq!((read.unread, read.marked_unread, unread.unread, unread.marked_unread), (0, false, 1, true));
    let archived = query(&store, |db| read::chats(db, true, None, 10)).await;
    assert_eq!(archived[0].id, chat(4));
    assert_eq!(store.count("app_state").unwrap(), 6, "every mutation is kept raw, the ones jaw doesn't use too");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_senders_lid_joins_their_number_and_contacts_name_them() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut from_lid = live_message(GROUP, "100000000001@lid", "g1", AT, text("olá"));
    from_lid.info.as_mut().unwrap().sender_alt = "351910000002:4@s.whatsapp.net".into();
    store.inbox("message", 0, from_lid.encode_to_vec(), None);
    let converter = converting(&store);
    drained(&store).await;
    converter.abort();
    let contacts = pb::ContactList {
        contacts: vec![pb::Contact { jid: ANA.into(), full_name: "Ana Ribeiro".into(), ..Default::default() }],
    };
    store.apply(Lane::Background, convert::people::contacts(&contacts)).await.unwrap();
    assert_eq!(store.count("people").unwrap(), 1, "the LID and the number are one person");
    let rows = query(&store, |db| read::messages(db, GROUP, Around::Newest, 10)).await;
    assert_eq!(rows[0].from.as_ref().unwrap().name, "Ana Ribeiro");
}

#[tokio::test(flavor = "multi_thread")]
async fn live_messages_count_as_unread_and_mentions_until_you_write_and_history_never_does() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mention = e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some("@100000000009 are you in?".into()),
            context_info: Some(Box::new(e2e::ContextInfo {
                mentioned_jid: vec!["100000000009@lid".into()],
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let unread = |store: &Store| {
        let store = store.clone();
        async move {
            let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
            chats.iter().find(|c| c.id == GROUP).map(|c| (c.unread, c.mentions)).unwrap()
        }
    };
    let changes = |m: &pb::Message| convert::message(&convert::web_message(m).unwrap(), &me(), true);
    store.apply(Lane::Live, changes(&live_message(GROUP, "100000000001@lid", "a", AT, text("hi")))).await.unwrap();
    store.apply(Lane::Live, changes(&live_message(GROUP, "100000000001@lid", "b", AT + 1, mention))).await.unwrap();
    assert_eq!(unread(&store).await, (2, 1));
    let old = convert::web_message(&live_message(GROUP, "100000000001@lid", "h", AT - 99_000, text("old"))).unwrap();
    store.apply(Lane::Background, convert::message(&old, &me(), false)).await.unwrap();
    assert_eq!(unread(&store).await, (2, 1), "history doesn't count");
    store.apply(Lane::Live, changes(&live_message(GROUP, "", "c", AT + 2, text("yes")))).await.unwrap();
    assert_eq!(unread(&store).await, (0, 0), "writing in a chat reads it");
}

fn amend(target: &str, what: crate::store::Amendment) -> Change {
    Change::Amend(crate::store::Amend {
        chat: ANA.into(),
        target: target.into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 5,
        what,
        raw: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_deletes_and_stars_wait_for_their_message_and_keep_the_index_whole() {
    use crate::store::Amendment;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store
        .apply(
            Lane::Live,
            vec![
                amend(
                    "e",
                    Amendment::Edit { text: Some("fixed words".into()), spans: None, summary: "fixed words".into() },
                ),
                amend("s", Amendment::Star { starred: true }),
                amend("p", Amendment::Keep { kind: "vote".into() }),
            ],
        )
        .await
        .unwrap();
    assert_eq!(store.count("pending").unwrap(), 3);
    let history: Vec<Change> = ["e", "s", "d", "r"]
        .iter()
        .enumerate()
        .flat_map(|(n, id)| {
            let info = WebMessageInfo {
                key: MessageKey {
                    remote_jid: Some(ANA.into()),
                    from_me: Some(false),
                    id: Some(id.to_string()),
                    participant: None,
                },
                message: Some(text(&format!("typo words {id}"))),
                message_timestamp: Some((AT / 1000 + n as i64) as u64),
                ..Default::default()
            };
            convert::message(&info, &me(), false)
        })
        .collect();
    store.apply(Lane::Background, history).await.unwrap();
    assert_eq!(store.count("pending").unwrap(), 1, "only the vote, which this jawd keeps for a later one, still waits");
    store
        .apply(Lane::Background, vec![amend("d", Amendment::DeleteForMe), amend("r", Amendment::Revoke)])
        .await
        .unwrap();
    store.indexed().await.unwrap();
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let shown: Vec<(&str, &str, bool, bool, bool)> =
        rows.iter().map(|r| (r.id.as_str(), r.summary.as_str(), r.edited, r.starred, r.deleted)).collect();
    assert_eq!(
        shown,
        [
            ("e", "fixed words", true, false, false),
            ("s", "typo words s", false, true, false),
            ("r", "This message was deleted", false, false, true)
        ]
    );
    let found = |q: &'static str| {
        let store = store.clone();
        async move { query(&store, move |db| read::search(db, q, None, None, 10)).await.hits.len() }
    };
    assert_eq!(
        (found("fixed").await, found("typo").await),
        (1, 1),
        "the edit's words, not the typo's; the deleted aren't found"
    );
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    // FTS5's check against the content table would count messages without text, which aren't
    // indexed; this checks the index itself, and the searches above check what it holds.
    db.execute("INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 0)", []).unwrap();
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    assert_eq!(chats[0].last.as_ref().unwrap().summary, "This message was deleted");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_from_someone_new_to_a_message_stored_earlier_takes_its_own_version() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let changes = |m: &pb::Message| convert::message(&convert::web_message(m).unwrap(), &me(), true);
    store.apply(Lane::Live, changes(&live_message(GROUP, "100000000001@lid", "m1", AT, text("hi")))).await.unwrap();
    let reaction = e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(MessageKey {
                remote_jid: Some(GROUP.into()),
                from_me: Some(false),
                id: Some("m1".into()),
                participant: None,
            }),
            text: Some("👍".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut from_someone_new = live_message(GROUP, "100000000007@lid", "r1", AT + 1, reaction);
    from_someone_new.info.as_mut().unwrap().push_name.clear();
    let applied = store.apply(Lane::Live, changes(&from_someone_new)).await.unwrap();
    let events = query(&store, move |db| read::events(db, applied.seq - 3, 10)).await;
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds[kinds.len() - 3..], ["person", "message", "chat"], "the reactor, then the message, then its chat");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_a_message_jaw_never_had_stands_in_for_it_with_its_reactions() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let edit = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(MessageKey {
                remote_jid: Some(GROUP.into()),
                from_me: Some(false),
                id: Some("orig".into()),
                participant: None,
            }),
            r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
            edited_message: Some(Box::new(text("fixed words"))),
            timestamp_ms: Some(AT + 9_000),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut info = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("the-edit".into()),
            participant: Some("100000000001@lid".into()),
        },
        message: Some(edit),
        message_timestamp: Some((AT / 1000 + 8) as u64),
        ..Default::default()
    };
    info.reactions = vec![wa_proto::wa_web_protobufs_web::Reaction {
        key: Some(MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("re".into()),
            participant: Some("100000000002@lid".into()),
        }),
        text: Some("😂".into()),
        sender_timestamp_ms: Some(AT + 10_000),
        ..Default::default()
    }];
    store.apply(Lane::Background, convert::message(&info, &me(), false)).await.unwrap();
    let original = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("orig".into()),
            participant: Some("100000000001@lid".into()),
        },
        message: Some(text("typo wrods")),
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    };
    store.apply(Lane::Background, convert::message(&original, &me(), false)).await.unwrap();
    let rows = query(&store, |db| read::messages(db, GROUP, Around::Newest, 10)).await;
    let shown: Vec<(&str, &str, bool)> = rows.iter().map(|r| (r.id.as_str(), r.summary.as_str(), r.edited)).collect();
    assert_eq!(shown, [("orig", "fixed words", true)], "one message, as edited, even once the original arrives");
    assert_eq!(rows[0].reactions.get(), r#"[{"emoji":"😂","n":1,"mine":false}]"#);
    assert_eq!(store.count("pending").unwrap(), 0);
}

#[test]
fn a_history_blob_keeps_each_chat_as_the_phone_has_it() {
    use wa_proto::wa_web_protobufs_history_sync::{
        Conversation, HistorySync, HistorySyncMsg, PhoneNumberToLidMapping, Pushname,
    };
    let message = |chat: &str, id: &str| HistorySyncMsg {
        message: Some(WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(false),
                id: Some(id.into()),
                participant: None,
            },
            message: Some(text("from the phone")),
            message_timestamp: Some((AT / 1000) as u64),
            ..Default::default()
        }),
        ..Default::default()
    };
    let history = HistorySync {
        sync_type: 2,
        conversations: vec![Conversation {
            id: "100000000001@lid".into(),
            pn_jid: Some(ANA.into()),
            lid_jid: Some("100000000001@lid".into()),
            pinned: Some((AT / 1000) as u32),
            mute_end_time: Some((AT / 1000 + 3600) as u64),
            unread_count: Some(4),
            messages: vec![message("100000000001@lid", "h1")],
            ..Default::default()
        }],
        pushnames: vec![Pushname { id: Some("351910000003@s.whatsapp.net".into()), pushname: Some("Rui".into()) }],
        phone_number_to_lid_mappings: vec![PhoneNumberToLidMapping {
            pn_jid: Some("351910000003@s.whatsapp.net".into()),
            lid_jid: Some("100000000003@lid".into()),
        }],
        ..Default::default()
    };
    let parts = convert::history::blob(&history, &me());
    let chat = parts[0].iter().find_map(|c| if let Change::Chat(chat) = c { Some(chat) } else { None }).unwrap();
    assert_eq!((chat.id.as_str(), chat.pinned, chat.muted_until), (ANA, Some(AT), Some(AT + 3_600_000)));
    let read = parts[0].iter().find_map(|c| if let Change::Read(read) = c { Some(read) } else { None }).unwrap();
    assert_eq!(read.phone, Some((4, 0)), "the phone counts four unread, more than the blob carries");
    let message = parts[0].iter().find_map(|c| if let Change::Message(m) = c { Some(m) } else { None }).unwrap();
    assert_eq!((message.chat.as_str(), message.live), (ANA, false));
    assert_eq!(parts[1].len(), 2, "the blob's LID map and push names");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_converter_drains_the_inbox_to_empty_then_converts_one_more() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let converter = converting(&store);
    for n in 0..3 {
        store.inbox("message", 0, live_message(ANA, ANA, &format!("m{n}"), AT + n, text("hi")).encode_to_vec(), None);
    }
    until("the inbox to drain", || store.count("inbox").unwrap() == 0 && store.count("messages").unwrap() == 3).await;
    store.inbox("message", 0, live_message(ANA, ANA, "m3", AT + 9, text("one more")).encode_to_vec(), None);
    until("one more to be converted", || store.count("messages").unwrap() == 4).await;
    assert_eq!(store.count("inbox").unwrap(), 0);
    converter.abort();
}

fn copy_of(chat: &str, id: &str, mine: bool, status: Option<i32>) -> WebMessageInfo {
    WebMessageInfo {
        key: MessageKey { remote_jid: Some(chat.into()), from_me: Some(mine), id: Some(id.into()), participant: None },
        message: Some(text("the same message again")),
        message_timestamp: Some((AT / 1000) as u64),
        status,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_arriving_again_never_undoes_a_delete_for_me_or_takes_a_status_back() {
    use wa_proto::wa_web_protobufs_web::web_message_info::Status;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let read_copy = copy_of(ANA, "mine", true, Some(Status::Read as i32));
    store.apply(Lane::Live, convert::message(&read_copy, &me(), true)).await.unwrap();
    store.apply(Lane::Live, convert::message(&copy_of(ANA, "gone", false, None), &me(), true)).await.unwrap();
    store.apply(Lane::Live, vec![amend("gone", crate::store::Amendment::DeleteForMe)]).await.unwrap();
    let delivered_copy = copy_of(ANA, "mine", true, Some(Status::DeliveryAck as i32));
    store.apply(Lane::Background, convert::message(&delivered_copy, &me(), false)).await.unwrap();
    store.apply(Lane::Background, convert::message(&copy_of(ANA, "gone", false, None), &me(), false)).await.unwrap();
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let shown: Vec<(&str, Option<&str>)> = rows.iter().map(|r| (r.id.as_str(), r.status.as_deref())).collect();
    assert_eq!(shown, [("mine", Some("read"))]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_to_someones_lid_goes_into_the_chat_jaw_has_with_their_number() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.apply(Lane::Live, convert::message(&copy_of(ANA, "pn", false, None), &me(), true)).await.unwrap();
    let joined = convert::people::lid_pairs(&pb::LidPairs {
        pairs: vec![pb::LidPair { lid: "100000000001@lid".into(), phone: ANA.into() }],
    });
    store.apply(Lane::Live, joined).await.unwrap();
    store
        .apply(Lane::Live, convert::message(&copy_of("100000000001@lid", "lid", false, None), &me(), true))
        .await
        .unwrap();
    assert_eq!(store.count("chats").unwrap(), 1, "one conversation, one chat");
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["pn", "lid"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn receipts_move_your_message_forward_even_when_they_come_first() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let receipt = |kind: &str, at: i64| pb::Receipt {
        chat: ANA.into(),
        sender: ANA.to_string(),
        ids: vec!["mine".into()],
        timestamp_ms: at,
        r#type: kind.into(),
        ..Default::default()
    };
    store.apply(Lane::Live, drain::receipt_changes(&receipt("read", AT + 2))).await.unwrap();
    assert_eq!(store.count("pending").unwrap(), 1, "a receipt for a message jaw doesn't have yet waits for it");
    store.apply(Lane::Live, convert::message(&copy_of(ANA, "mine", true, None), &me(), true)).await.unwrap();
    store.apply(Lane::Live, drain::receipt_changes(&receipt("", AT + 1))).await.unwrap();
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows[0].status.as_deref(), Some("read"), "delivered after read doesn't take it back");
    let receipts: (Option<i64>, Option<i64>) = query(&store, |db| {
        Ok(db.query_row("SELECT delivered_at, read_at FROM receipts", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
    })
    .await;
    assert_eq!(receipts, (Some(AT + 1), Some(AT + 2)));
    let read_elsewhere = drain::receipt_changes(&receipt("read-self", AT + 3));
    assert!(matches!(read_elsewhere.as_slice(), [Change::Read(read)] if read.at == AT + 3 && read.ids == ["mine"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_stop_is_noted_and_the_next_start_notes_it_is_running_before_it_returns() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    assert_eq!(store.remembered(CLEAN).unwrap(), None, "a database jawd never stopped cleanly");
    stopped(&store, None).await;
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("1"));
    running(&store).await.unwrap();
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("0"), "noted before the engine can start");
}

#[test]
fn a_live_poll_vote_stays_in_the_inbox_with_what_the_engine_decrypted() {
    let vote = e2e::Message {
        poll_update_message: Some(e2e::PollUpdateMessage {
            poll_creation_message_key: Some(MessageKey {
                remote_jid: Some(GROUP.into()),
                from_me: Some(true),
                id: Some("poll".into()),
                participant: None,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut live = live_message(GROUP, "100000000001@lid", "v1", AT, vote);
    live.decrypted = vec![1, 2, 3];
    let converted =
        drain::convert_rows(&[Row { seq: 7, kind: "message".into(), payload: live.encode_to_vec() }], &me());
    assert!(converted.changes.is_empty(), "nothing converted, and the row isn't drained");
}

/// What a scripted engine answers when asked for a history blob, by its notification's direct path.
#[derive(Clone)]
enum Blob {
    Here(Vec<u8>),
    /// WhatsApp didn't answer; asking again later may work.
    NotYet,
    /// WhatsApp no longer has it.
    Gone,
}

/// An engine for tests: each instance says it's connected once it's running, answers what sync
/// asks with little, writes the history blobs it has, notes what it was asked, and stops when told.
#[derive(Clone, Default)]
struct Scripted {
    asked: Arc<Mutex<Vec<&'static str>>>,
    blobs: Arc<Mutex<std::collections::HashMap<String, Blob>>>,
    stop: Arc<tokio::sync::Notify>,
    events: Arc<Mutex<Option<mpsc::Sender<crate::engine::Incoming>>>>,
}

fn connected() -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Connection(pb::Connection {
            state: pb::connection::State::Connected as i32,
            jid: "351910000001@s.whatsapp.net".into(),
            ..Default::default()
        })),
    }
}

impl Scripted {
    fn asked(&self, what: &str) -> usize {
        self.asked.lock().unwrap().iter().filter(|asked| **asked == what).count()
    }

    async fn send(&self, event: pb::Event) {
        let events = self.events.lock().unwrap().clone().expect("an engine is running");
        events.send(crate::engine::Incoming { event, ack: None }).await.unwrap();
    }

    fn answer(&self, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
        use pb::command::Kind;
        let (name, answer) = match kind {
            Kind::Contacts(_) => ("contacts", Ok(pb::reply::Kind::ContactList(Default::default()))),
            Kind::Groups(_) => ("groups", Ok(pb::reply::Kind::GroupList(Default::default()))),
            Kind::LidLookup(_) => ("lids", Ok(pb::reply::Kind::LidPairs(Default::default()))),
            Kind::AppStateResync(resync) => {
                assert_eq!(resync.names.len(), refresh::COLLECTIONS.len(), "every collection");
                ("resync", Ok(pb::reply::Kind::Done(pb::Done {})))
            }
            Kind::HistoryDownload(download) => {
                let notification = e2e::HistorySyncNotification::decode(download.notification.as_slice()).unwrap();
                let path = notification.direct_path.unwrap_or_default();
                let answer = match self.blobs.lock().unwrap().get(&path).cloned() {
                    Some(Blob::Here(blob)) => {
                        std::fs::create_dir_all(Path::new(&download.path).parent().unwrap()).unwrap();
                        std::fs::write(&download.path, blob).unwrap();
                        Ok(pb::reply::Kind::HistoryFile(pb::HistoryFile { path: download.path, ..Default::default() }))
                    }
                    Some(Blob::NotYet) => Err(EngineError::Failed { message: "no answer".into(), retry: true }),
                    Some(Blob::Gone) | None => Err(EngineError::Failed { message: "404".into(), retry: false }),
                };
                ("history", answer)
            }
            _ => ("other", Err(EngineError::Failed { message: "not scripted".into(), retry: false })),
        };
        self.asked.lock().unwrap().push(name);
        answer
    }
}

impl crate::engine::Engine for Scripted {
    fn kind(&self) -> &'static str {
        "scripted"
    }

    fn start(
        &self,
        events: mpsc::Sender<crate::engine::Incoming>,
    ) -> crate::engine::BoxFuture<'_, anyhow::Result<crate::engine::Instance>> {
        let engine = self.clone();
        Box::pin(async move {
            *engine.events.lock().unwrap() = Some(events.clone());
            let (commands, mut outgoing) = crate::engine::Commands::for_tests();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let _ = events.send(crate::engine::Incoming { event: connected(), ack: None }).await;
            });
            let serving = engine.clone();
            let served = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        out = outgoing.recv() => match out {
                            Some(crate::engine::Outgoing::Command { kind, reply }) => {
                                let _ = reply.send(serving.answer(kind));
                            }
                            Some(crate::engine::Outgoing::Ack(_)) => {}
                            None => return,
                        },
                        () = serving.stop.notified() => return,
                    }
                }
            });
            let exited: crate::engine::BoxFuture<'static, String> = Box::pin(async move {
                let _ = served.await;
                "told to stop".to_string()
            });
            let hello =
                pb::Hello { protocol: crate::engine::PROTOCOL, program: "scripted".into(), whatsmeow: String::new() };
            Ok(crate::engine::Instance { hello, commands, exited })
        })
    }
}

fn supervised(engine: &Scripted) -> (Supervisor, mpsc::Receiver<crate::engine::Incoming>) {
    let (events, incoming) = mpsc::channel(64);
    let quick = crate::engine::supervisor::Backoff {
        first: Duration::from_millis(10),
        most: Duration::from_millis(10),
        settled: Duration::from_secs(60),
    };
    (Supervisor::spawn(Arc::new(engine.clone()), events, quick), incoming)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_engine_that_connects_fetches_app_state_again_and_a_reconnection_of_the_same_one_doesnt() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let (supervisor, incoming) = supervised(&engine);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        Some(supervisor),
        home.path().to_path_buf(),
    ));
    until("the first engine's resync", || engine.asked("resync") == 1).await;
    engine.send(connected()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!((engine.asked("contacts"), engine.asked("resync")), (1, 1), "the same engine connecting again");
    engine.stop.notify_one();
    until("the engine started again to resync", || engine.asked("resync") == 2).await;
    running.abort();
}

fn blob(id: &str) -> Vec<u8> {
    use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync, HistorySyncMsg};
    HistorySync {
        sync_type: 2,
        conversations: vec![Conversation {
            id: ANA.into(),
            unread_count: Some(0),
            messages: vec![HistorySyncMsg {
                message: Some(WebMessageInfo {
                    key: MessageKey {
                        remote_jid: Some(ANA.into()),
                        from_me: Some(false),
                        id: Some(id.into()),
                        participant: None,
                    },
                    message: Some(text("from the phone")),
                    message_timestamp: Some((AT / 1000) as u64),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        global_settings: Some(Default::default()),
        ..Default::default()
    }
    .encode_to_vec()
}

fn offer(seq: i64, path: &str) -> fetch::Work {
    let notification = e2e::HistorySyncNotification { direct_path: Some(path.into()), ..Default::default() };
    fetch::Work::Offer(fetch::Offer { seq, id: format!("offer-{path}"), notification: notification.encode_to_vec() })
}

/// Puts `n` rows in the inbox for history notifications to name, numbered from 1.
async fn notifications(store: &Store, n: usize) {
    for _ in 0..n {
        store.inbox("message", 0, b"a history notification".to_vec(), None);
    }
    store.apply(Lane::Live, Vec::new()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_history_blob_that_cant_be_had_yet_waits_behind_the_ones_after_it_and_one_thats_gone_is_quarantined() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    notifications(&store, 3).await;
    let engine = Scripted::default();
    engine.blobs.lock().unwrap().extend([
        ("late".to_string(), Blob::NotYet),
        ("here".to_string(), Blob::Here(blob("h1"))),
        ("gone".to_string(), Blob::Gone),
    ]);
    let (supervisor, _incoming) = supervised(&engine);
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(
        offers,
        store.clone(),
        supervisor,
        Arc::default(),
        home.path().join("spool"),
        Merges::default(),
    ));
    for work in [offer(1, "late"), offer(2, "here"), offer(3, "gone")] {
        offered.send(work).unwrap();
    }
    until("the blob after the late one", || store.count("messages").unwrap() == 1).await;
    until("the gone one's notification in quarantine", || store.count("quarantine").unwrap() == 1).await;
    fetching.abort();
    let inbox: Vec<i64> = query(&store, |db| {
        Ok(db
            .prepare("SELECT seq FROM inbox ORDER BY seq")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(inbox, [1, 3], "the late one waits to be tried again; the gone one's row is kept with its quarantine");
    assert_eq!(store.count("history_kept").unwrap(), 1, "what the blob carried besides its message");
    let kept: Vec<u8> = query(&store, |db| Ok(db.query_row("SELECT raw FROM history_kept", [], |r| r.get(0))?)).await;
    let kept =
        wa_proto::wa_web_protobufs_history_sync::HistorySync::decode(crate::store::unpack(&kept).unwrap().as_slice())
            .unwrap();
    assert!(
        kept.global_settings.is_some() && kept.conversations[0].messages.is_empty(),
        "settings kept, rows' messages taken out"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_before_a_blobs_drain_uses_the_blob_in_the_spool_again_without_storing_anything_twice() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    notifications(&store, 1).await;
    let spool = home.path().join("spool");
    std::fs::create_dir_all(&spool).unwrap();
    let file = spool.join("offer-here.pb");
    std::fs::write(&file, blob("h1")).unwrap();
    // The run before stored the blob's rows and stopped before the commit that drains it.
    let history = wa_proto::wa_web_protobufs_history_sync::HistorySync::decode(blob("h1").as_slice()).unwrap();
    for part in convert::history::blob(&history, &me()) {
        store.apply(Lane::Background, part).await.unwrap();
    }
    assert!(file.exists(), "the blob stays until its drain is committed");
    let engine = Scripted::default();
    let (supervisor, _incoming) = supervised(&engine);
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching =
        tokio::spawn(fetch::run(offers, store.clone(), supervisor, Arc::default(), spool, Merges::default()));
    offered.send(offer(1, "here")).unwrap();
    until("the notification's drain", || store.count("inbox").unwrap() == 0).await;
    fetching.abort();
    assert_eq!(engine.asked("history"), 0, "the blob came from the spool");
    assert_eq!(store.count("messages").unwrap(), 1, "nothing twice");
    assert!(!file.exists());
}
