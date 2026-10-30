use std::collections::{BTreeMap, BTreeSet};
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
use crate::store::{Drained, Packed};

const ANA: &str = "351910000002@s.whatsapp.net";
const GROUP: &str = "120363000000000001@g.us";
const AT: i64 = 1_792_000_000_000;
/// Messages whose conversion panics once it has made its changes, or a notification its offer, in
/// the inbox or in history: those whose ID starts with this, each test's with IDs of its own. Their
/// panic says which message it is, but for those whose ID starts with PANICS_PLAINLY, whose panic
/// says only a literal.
const PANICS: &str = "3EB0PANICS";
const PANICS_PLAINLY: &str = "3EB0PANICSPLAINLY";

/// How many times each message of PANICS has panicked as it was converted, by its ID.
static PANICKED: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());
/// The messages of PANICS that convert after all, as they would for a jawd without the fault.
static MENDED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The hook each conversion runs once it has made its changes or its offer, which panics for a
/// message of PANICS, counting each time.
pub(super) fn panic_if_asked(id: &str) {
    if !id.starts_with(PANICS) || MENDED.lock().unwrap().contains(id) {
        return;
    }
    *PANICKED.lock().unwrap().entry(id.to_string()).or_default() += 1;
    if id.starts_with(PANICS_PLAINLY) {
        panic!("a conversion that panics as a test asks, saying only this");
    }
    panic!("the conversion of {id} panics once it has made its changes, as a test asks");
}

/// How many times the message `id` of PANICS has panicked as it was converted, or for BATCH_PANICS
/// a batch.
fn panicked(id: &str) -> usize {
    PANICKED.lock().unwrap().get(id).copied().unwrap_or_default()
}

/// Has the message `id` of PANICS convert from now on.
fn mend(id: &str) {
    MENDED.lock().unwrap().insert(id.to_string());
}

/// An inbox row of this kind makes its whole batch's conversion panic, outside every row's own
/// catch, as only the batch's own bookkeeping could.
const BATCH_PANICS: &str = "test: a batch that panics";

/// The hook each batch's conversion runs before its rows, which panics for a batch holding a row of
/// BATCH_PANICS, counting each time.
pub(super) fn panic_for_the_batch_if_asked(rows: &[Row]) {
    if rows.iter().any(|row| row.kind == BATCH_PANICS) {
        *PANICKED.lock().unwrap().entry(BATCH_PANICS.to_string()).or_default() += 1;
        panic!("a batch whose conversion panics outside its rows', as a test asks");
    }
}

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

/// Live state as jawd has it once jaw.db keeps the account's JIDs.
fn knowing() -> Shared {
    let live: Shared = Arc::default();
    live.lock().unwrap().me.send_replace(me());
    live
}

/// The converter running on `store` as jawd runs it.
fn converting(store: &Store) -> tokio::task::JoinHandle<()> {
    let (offered, _) = mpsc::unbounded_channel();
    tokio::spawn(drain::inbox(store.clone(), knowing(), offered, Avatars::new(), Merges::default()))
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

/// What the quarantine keeps: each row's source, reference, when and why.
async fn quarantined(store: &Store) -> Vec<(String, String, i64, String)> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT source, ref, at, error FROM quarantine ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

/// What a column of a table holds, in order.
async fn column<T: rusqlite::types::FromSql + Send + 'static>(store: &Store, sql: &'static str) -> Vec<T> {
    query(store, move |db| Ok(db.prepare(sql)?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)).await
}

/// What `jawd status` says jaw.db holds.
fn stored(home: &Path, store: &Store) -> jaw_proto::Stored {
    let (pushes, _) = broadcast::channel(1);
    let state = crate::state::State::new(
        home,
        Err(("demo", "not started".into())),
        Vec::new(),
        knowing(),
        Some(store.clone()),
        pushes,
    )
    .unwrap();
    state.status().stored.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_inbox_row_whose_conversion_panics_is_kept_in_quarantine_once_and_the_rows_after_it_go_on() {
    let home = tempfile::tempdir().unwrap();
    let event = |chat: &str, id: &str, at: i64| {
        let sender = if chat == GROUP { "100000000001@lid" } else { chat };
        live_message(chat, sender, id, at, text(&format!("words of {id}"))).encode_to_vec()
    };
    // The first one's panic says which it is, the second's only a literal.
    let (said, plainly) = ("3EB0PANICSINBOX", "3EB0PANICSPLAINLYINBOX");
    let kept = {
        let store = Store::open(home.path()).unwrap();
        // From before the converter started, so the background lane takes them.
        for (id, at) in [("m1", AT), (said, AT + 1_000), ("m3", AT + 2_000)] {
            store.inbox("message", 0, event(ANA, id, at), None);
        }
        until("the inbox", || store.count("inbox").unwrap() == 3).await;
        let converter = converting(&store);
        until("the row after it", || store.count("messages WHERE id = 'm3'").unwrap() == 1).await;
        // As they arrive, so the live lane takes them.
        for (id, at) in [(plainly, AT + 3_000), ("m5", AT + 4_000)] {
            store.inbox("message", 0, event(GROUP, id, at), None);
        }
        until("the live row after it", || store.count("messages WHERE id = 'm5'").unwrap() == 1).await;
        // A converter that tried a row that panicked again at once would have by now.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!((panicked(said), panicked(plainly)), (1, 1), "each is tried once in this start");
        converter.abort();
        let _ = converter.await;
        let ids: Vec<String> = column(&store, "SELECT id FROM messages ORDER BY id").await;
        assert_eq!(ids, ["m1", "m3", "m5"], "nothing of either panicking message is kept");
        assert_eq!(
            column::<i64>(&store, "SELECT seq FROM inbox ORDER BY seq").await,
            [2, 4],
            "each stays in the inbox"
        );
        let kept = quarantined(&store).await;
        let named: Vec<_> = kept
            .iter()
            .map(|(source, reference, _, error)| (source.as_str(), reference.as_str(), error.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                (
                    "inbox",
                    "2",
                    "converting it panicked: the conversion of 3EB0PANICSINBOX panics once it has made its changes, \
                    as a test asks"
                ),
                ("inbox", "4", "converting it panicked: a conversion that panics as a test asks, saying only this"),
            ],
            "each is kept in quarantine, saying why"
        );
        let status = stored(home.path(), &store);
        assert_eq!((status.inbox, status.quarantined), (2, 2), "jawd status counts them");
        kept
    };
    // jawd starts again, with a row behind them.
    let store = Store::open(home.path()).unwrap();
    store.inbox("message", 0, event(ANA, "m6", AT + 5_000), None);
    until("the inbox", || store.count("inbox").unwrap() == 3).await;
    let converter = converting(&store);
    until("the row after them", || store.count("messages WHERE id = 'm6'").unwrap() == 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!((panicked(said), panicked(plainly)), (2, 2), "and once more at the next start");
    converter.abort();
    let ids: Vec<String> = column(&store, "SELECT id FROM messages ORDER BY id").await;
    assert_eq!(ids, ["m1", "m3", "m5", "m6"]);
    assert_eq!(column::<i64>(&store, "SELECT seq FROM inbox ORDER BY seq").await, [2, 4]);
    let again = quarantined(&store).await;
    assert_eq!(again.len(), 2, "each is kept once: {again:?}");
    for ((source, reference, at, error), before) in again.iter().zip(&kept) {
        assert_eq!((source, reference, error), (&before.0, &before.1, &before.3));
        assert!(*at > before.2, "{reference} was tried again at the start, and panicked again");
    }
    let status = stored(home.path(), &store);
    assert_eq!((status.inbox, status.quarantined), (2, 2));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_history_row_whose_conversion_panics_is_kept_in_quarantine_once_and_the_rows_after_it_go_on() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let panics = "3EB0PANICSHISTORY";
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        for (n, id) in ["h1", panics, "h3"].into_iter().enumerate() {
            let info = WebMessageInfo {
                key: MessageKey {
                    remote_jid: Some(ANA.into()),
                    from_me: Some(false),
                    id: Some(id.into()),
                    participant: None,
                },
                message: Some(text(&format!("words of {id}"))),
                message_timestamp: Some((AT / 1000) as u64 + n as u64),
                ..Default::default()
            };
            db.execute(
                "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![ANA, id, AT, info.encode_to_vec()],
            )
            .unwrap();
        }
    }
    let mut kept: Vec<(String, String, i64, String)> = Vec::new();
    // Converted at two starts: the second takes the row that panicked again, and keeps it once.
    for start in 0..2 {
        tokio::time::timeout(WAIT, drain::history_raw(store.clone(), knowing())).await.unwrap_or_else(|_| {
            panic!("the pass over the history kept raw didn't end: {} panics so far", panicked(panics))
        });
        assert_eq!(panicked(panics), start + 1, "tried once in each start");
        let ids: Vec<String> = column(&store, "SELECT id FROM messages ORDER BY id").await;
        assert_eq!(ids, ["h1", "h3"], "the row after it is converted, and nothing of it is kept");
        let left: Vec<String> = column(&store, "SELECT id FROM history_raw").await;
        assert_eq!(left, [panics], "it stays raw");
        let now = quarantined(&store).await;
        let named: Vec<_> = now
            .iter()
            .map(|(source, reference, _, error)| (source.as_str(), reference.as_str(), error.as_str()))
            .collect();
        assert_eq!(
            named,
            [(
                "history_raw",
                format!("{ANA} 3EB0PANICSHISTORY").as_str(),
                "converting it panicked: the conversion of 3EB0PANICSHISTORY panics once it has made its changes, as \
                a test asks"
            )],
            "at start {start}"
        );
        if let Some(before) = kept.first() {
            assert!(now[0].2 > before.2, "tried again at the second start");
        }
        assert_eq!(stored(home.path(), &store).quarantined, 1);
        kept = now;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// A history notification from the phone, as the engine hands it over, naming the blob at `path`.
fn notification(id: &str, path: &str) -> pb::Message {
    let notification = e2e::HistorySyncNotification { direct_path: Some(path.into()), ..Default::default() };
    let protocol = e2e::ProtocolMessage { history_sync_notification: Some(notification), ..Default::default() };
    let message = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    live_message("351910000001@s.whatsapp.net", "", id, AT, message)
}

#[test]
fn a_batch_keeps_what_each_row_converts_to_besides_its_changes_and_nothing_of_a_row_that_panics() {
    let name = |name: &str| {
        app_state(&["setting_pushName"], |v| {
            v.push_name_setting = Some(action::PushNameSetting { name: Some(name.into()) })
        })
    };
    let preview = |id: &str, url: &str, jpeg: &[u8]| {
        live_message(ANA, ANA, id, AT, previewed(&format!("vê {url}"), url, Some(jpeg.to_vec()))).encode_to_vec()
    };
    let (thumbnail, its_own) = (jpeg(40, 30), jpeg(30, 40));
    let rows = [
        ("message", notification("3EB0NOTE", "here").encode_to_vec()),
        ("picture", pb::Picture { jid: ANA.into(), picture_id: "1".into(), ..Default::default() }.encode_to_vec()),
        ("app_state", name("Pedro")),
        ("message", preview("p1", "www.8a.nu/sintra", &thumbnail)),
        // Each panics once it has made what it makes: a preview's thumbnail, a notification's offer.
        ("message", preview("3EB0PANICSBATCH", "www.8a.nu/peneda", &its_own)),
        ("message", notification("3EB0PANICSBATCHNOTE", "there").encode_to_vec()),
        ("app_state", name("Pedro Gomes")),
    ];
    let rows: Vec<Row> =
        rows.into_iter().zip(1..).map(|((kind, payload), seq)| Row { seq, kind: kind.into(), payload }).collect();
    let drain::Converted { changes, offers, my_name, pictures, thumbnails } = drain::convert_rows(&rows, &me());
    let offers: Vec<(i64, &str)> = offers.iter().map(|offer| (offer.seq, offer.id.as_str())).collect();
    assert_eq!(offers, [(1, "3EB0NOTE")], "the notification's offer, and not the one's that panicked");
    assert_eq!(pictures, [ANA], "the chat whose picture changed");
    assert_eq!(my_name.as_deref(), Some("Pedro Gomes"), "the later of the two names");
    let sha = crate::store::thumbnails::sha256(&thumbnail);
    assert_eq!(thumbnails, [(sha, thumbnail)], "the preview's thumbnail, and not the one's that panicked");
    let previews: Vec<&str> = changes
        .iter()
        .filter_map(|change| match change {
            Change::Thumbnail { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(previews, ["p1"]);
    let ends: Vec<String> = changes
        .iter()
        .filter_map(|change| match change {
            Change::Drained(Drained::Inbox(seq)) => Some(format!("drained {seq}")),
            Change::Quarantined { source: "inbox", reference, .. } => Some(format!("quarantined {reference}")),
            _ => None,
        })
        .collect();
    assert_eq!(ends, ["drained 2", "drained 3", "drained 4", "quarantined 5", "quarantined 6", "drained 7"]);
    assert_eq!((panicked("3EB0PANICSBATCH"), panicked("3EB0PANICSBATCHNOTE")), (1, 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_batch_whose_conversion_panics_outside_its_rows_is_read_again_after_a_pause_that_doubles() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.inbox(BATCH_PANICS, 0, Vec::new(), None);
    until("the inbox", || store.count("inbox").unwrap() == 1).await;
    let converter = converting(&store);
    until("the first try", || panicked(BATCH_PANICS) >= 1).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    converter.abort();
    // Tried again after 10 ms, then 20, 40, 80, 160, 320 and 640 ms more.
    let tries = panicked(BATCH_PANICS);
    assert!((2..=9).contains(&tries), "tried {tries} times in a second");
    assert_eq!(store.count("inbox").unwrap(), 1, "its row stays");
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
    drain::history_raw(store.clone(), knowing()).await;
    assert_eq!(store.count("messages").unwrap(), 2_500);
    assert_eq!(store.count("history_raw").unwrap(), 1, "what doesn't decode stays raw");
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    assert_eq!((chats[0].unread, chats[0].last.as_ref().unwrap().summary.as_str()), (0, "history 2499"));
    store.indexed().await.unwrap();
    let found = query(&store, |db| read::search(db, "history 1234", None, None, 5)).await;
    assert_eq!(found.hits[0].message.id, "h1234");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_kept_raw_that_doesnt_become_a_message_is_kept_whole() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let info = |id: &str, message: Option<e2e::Message>| WebMessageInfo {
        key: MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some(id.into()), participant: None },
        message,
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    };
    let edit = e2e::Message {
        protocol_message: Some(Box::new(e2e::ProtocolMessage {
            key: Some(MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(false),
                id: Some("said".into()),
                participant: None,
            }),
            r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
            edited_message: Some(Box::new(text("said better"))),
            timestamp_ms: Some(AT + 5),
            ..Default::default()
        })),
        ..Default::default()
    };
    let last_place = WebMessageInfo {
        final_live_location: Some(e2e::LiveLocationMessage {
            degrees_latitude: Some(38.7),
            degrees_longitude: Some(-9.1),
            ..Default::default()
        }),
        duration: Some(3600),
        ..info("place", None)
    };
    let rows = [
        ("said", info("said", Some(text("said")))),
        ("edit", info("edit", Some(edit))),
        ("place", last_place),
        ("shell", info("shell", None)),
    ];
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        for (id, info) in &rows {
            db.execute(
                "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![ANA, id, AT, info.encode_to_vec()],
            )
            .unwrap();
        }
    }
    drain::history_raw(store.clone(), knowing()).await;
    assert_eq!(store.count("history_raw").unwrap(), 0);
    let kept: Vec<(String, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT id, raw FROM history_kept ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    let kept: Vec<(String, Vec<u8>)> =
        kept.into_iter().map(|(id, raw)| (id, crate::store::unpack(&raw).unwrap())).collect();
    let expected: Vec<(String, Vec<u8>)> = ["edit", "place", "shell"]
        .iter()
        .map(|id| {
            let (_, info) = rows.iter().find(|(row, _)| row == id).unwrap();
            (format!("history_raw {ANA} {id}"), info.encode_to_vec())
        })
        .collect();
    assert_eq!(kept, expected, "each kept as it was, and the message that became a row isn't kept twice");
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.iter().map(|r| r.summary.as_str()).collect::<Vec<_>>(), ["said better"]);
}

/// Migration 1, as the spike ran it on 16 October: the engine's events kept raw, and history as it
/// came. A jaw.db at version 1 holds nothing else, the account's JIDs included.
const VERSION_1: &str = "
    CREATE TABLE inbox (
        seq INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        payload BLOB NOT NULL
    );
    CREATE TABLE history_raw (
        chat TEXT NOT NULL,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        raw BLOB NOT NULL,
        PRIMARY KEY (chat, id)
    ) WITHOUT ROWID;
    ";

#[tokio::test(flavor = "multi_thread")]
async fn what_a_version_1_jaw_db_holds_waits_for_the_accounts_jids_and_is_converted_with_them() {
    const RUI: &str = "351910000003@s.whatsapp.net";
    let connected = crate::engine::demo::connected();
    let account = match &connected.kind {
        Some(pb::event::Kind::Connection(said)) => said.jid.clone(),
        _ => unreachable!("the demo engine says it's connected"),
    };
    let mentioning = |words: &str| e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some(words.into()),
            context_info: Some(Box::new(e2e::ContextInfo {
                mentioned_jid: vec![account.clone()],
                ..Default::default()
            })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let reaction = |chat: &str, id: &str, participant: Option<&str>, emoji: &str| e2e::Message {
        reaction_message: Some(e2e::ReactionMessage {
            key: Some(MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(false),
                id: Some(id.into()),
                participant: participant.map(str::to_string),
            }),
            text: Some(emoji.into()),
            sender_timestamp_ms: Some(AT),
            ..Default::default()
        }),
        ..Default::default()
    };
    let read_by = |reader: &str, at: i64| pb::Receipt {
        chat: GROUP.into(),
        sender: reader.into(),
        group: true,
        ids: vec!["mine".into()],
        timestamp_ms: at,
        r#type: "read".into(),
        ..Default::default()
    };
    let members = [account.as_str(), ANA, RUI].map(|jid| pb::Participant { jid: jid.into(), ..Default::default() });
    let group = pb::GroupInfo {
        jid: GROUP.into(),
        name: "Climbing".into(),
        participants: members.into(),
        ..Default::default()
    };
    // What the spike's engine handed over: your reactions, from your phone, to a message of Ana's,
    // to Rui's in the group, and to one of Ana's that comes after it; Rui mentioning you; and a
    // message of yours in the group, which both the others read.
    let inbox: Vec<(&str, Vec<u8>)> = vec![
        ("group", group.encode_to_vec()),
        ("message", live_message(ANA, ANA, "a1", AT + 20_000, text("did you get the photos?")).encode_to_vec()),
        ("message", live_message(ANA, "", "ra1", AT + 21_000, reaction(ANA, "a1", None, "❤")).encode_to_vec()),
        ("message", live_message(GROUP, RUI, "g1", AT + 10_000, mentioning("are you in?")).encode_to_vec()),
        (
            "message",
            live_message(GROUP, "", "rg1", AT + 11_000, reaction(GROUP, "g1", Some(RUI), "👍")).encode_to_vec(),
        ),
        ("message", live_message(GROUP, "", "mine", AT, text("on my way")).encode_to_vec()),
        ("receipt", read_by(ANA, AT + 30_000).encode_to_vec()),
        ("receipt", read_by(RUI, AT + 31_000).encode_to_vec()),
        ("message", live_message(ANA, "", "rlater", AT + 41_000, reaction(ANA, "later", None, "🙏")).encode_to_vec()),
        ("message", live_message(ANA, ANA, "later", AT + 40_000, text("and the lake")).encode_to_vec()),
    ];
    // And the history it kept raw: a message of Ana's you reacted to, and Rui mentioning you.
    let h1 = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(false),
            id: Some("h1".into()),
            participant: None,
        },
        message: Some(text("from before")),
        message_timestamp: Some(((AT - 200_000) / 1000) as u64),
        reactions: vec![wa_proto::wa_web_protobufs_web::Reaction {
            key: Some(MessageKey {
                remote_jid: Some(ANA.into()),
                from_me: Some(true),
                id: Some("rh1".into()),
                participant: None,
            }),
            text: Some("😂".into()),
            sender_timestamp_ms: Some(AT - 199_000),
            ..Default::default()
        }],
        ..Default::default()
    };
    let h2 = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("h2".into()),
            participant: Some(RUI.into()),
        },
        message: Some(mentioning("you said Saturday?")),
        message_timestamp: Some(((AT - 100_000) / 1000) as u64),
        ..Default::default()
    };
    let history = [(ANA, "h1", h1), (GROUP, "h2", h2)];
    let home = tempfile::tempdir().unwrap();
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.execute_batch(VERSION_1).unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        for (kind, payload) in &inbox {
            db.execute("INSERT INTO inbox (kind, at, payload) VALUES (?1, 0, ?2)", rusqlite::params![kind, payload])
                .unwrap();
        }
        for (chat, id, info) in &history {
            db.execute(
                "INSERT INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, 0, ?3)",
                rusqlite::params![chat, id, info.encode_to_vec()],
            )
            .unwrap();
        }
    }
    let store = Store::open(home.path()).unwrap();
    let (events, incoming) = mpsc::channel(8);
    let home_path = home.path().to_path_buf();
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        None,
        home_path,
        Default::default(),
        crate::feed::Typing::default(),
    ));
    // An engine says which account is linked once it has connected, a while after jawd starts.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        (store.count("inbox").unwrap(), store.count("history_raw").unwrap()),
        (inbox.len() as u64, history.len() as u64),
        "nothing is converted before jaw.db keeps the account's JIDs"
    );
    events.send(crate::engine::Incoming { event: connected, ack: None }).await.unwrap();
    until("what the spike left to be converted", || {
        store.count("inbox").unwrap() == 0 && store.count("history_raw").unwrap() == 0
    })
    .await;
    running.abort();
    assert_eq!(store.remembered(ME).unwrap().as_deref(), Some(account.as_str()));
    let reactions: Vec<(String, String, bool, String)> = query(&store, |db| {
        Ok(db
            .prepare(
                "SELECT m.id, r.emoji, r.mine, j.jid FROM reactions r JOIN messages m ON m.n = r.message
                JOIN person_jids j ON j.n = r.sender ORDER BY m.id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    let yours = |id: &str, emoji: &str| (id.to_string(), emoji.to_string(), true, account.clone());
    assert_eq!(
        reactions,
        [yours("a1", "❤"), yours("g1", "👍"), yours("h1", "😂"), yours("later", "🙏")],
        "every reaction of yours is kept, as yours"
    );
    let mentioning_you: Vec<String> = query(&store, |db| {
        Ok(db
            .prepare("SELECT id FROM messages WHERE mentions_me ORDER BY id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(mentioning_you, ["g1", "h2"]);
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    let climbing = chats.iter().find(|c| c.id == GROUP).unwrap();
    assert_eq!(
        (climbing.unread, climbing.mentions),
        (1, 1),
        "Rui's mention came after your message; history's is read"
    );
    let rows = query(&store, |db| read::messages(db, GROUP, Around::Newest, 10)).await;
    let mine = rows.iter().find(|r| r.id == "mine").unwrap();
    assert_eq!(mine.status.as_deref(), Some("read"), "read by everyone in the group but you");
    assert_eq!((store.count("pending").unwrap(), store.count("quarantine").unwrap()), (0, 0));
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

/// Every row of every table in jaw.db, as text, but the feed's events and person `except`'s row.
fn every_row(home: &Path, except: i64) -> Vec<String> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name <> 'events' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut rows = Vec::new();
    for table in tables {
        let only = if table == "people" { format!(" WHERE n <> {except}") } else { String::new() };
        let mut statement = db.prepare(&format!("SELECT * FROM \"{table}\"{only}")).unwrap();
        let columns = statement.column_count();
        let found = statement
            .query_map([], |r| {
                (0..columns).map(|i| r.get::<_, rusqlite::types::Value>(i)).collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        rows.extend(found.map(|row| format!("{table}: {:?}", row.unwrap())));
    }
    rows.sort();
    rows
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_and_recording_heard_from_whatsapp_write_nothing_to_jaw_db() {
    const RUI: &str = "351910000003@s.whatsapp.net";
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let named = |jid: &str, name: &str| {
        Change::Person(crate::store::Person { jids: vec![jid.into()], name: Some(name.into()), ..Default::default() })
    };
    let said = |chat: &str, id: &str| {
        Change::Message(crate::store::Message {
            chat: chat.into(),
            id: id.into(),
            at: AT,
            sender: Some(ANA.into()),
            kind: "text",
            text: Some("olá".into()),
            summary: "olá".into(),
            live: true,
            ..Default::default()
        })
    };
    store
        .apply(Lane::Live, vec![named(ANA, "Ana"), named(RUI, "Rui"), said(ANA, "a1"), said(GROUP, "g1")])
        .await
        .unwrap();
    let rui: i64 = scalar(home.path(), &format!("SELECT person FROM person_jids WHERE jid = '{RUI}'"));
    let feed = crate::feed::Feed::start(store.clone(), Arc::new(|| false), crate::feed::Timing::default());
    let following = crate::presence::Following::default();
    let (pushes, _) = broadcast::channel(16);
    let (_focus, focused) = watch::channel(true);
    let looking = crate::presence::Looking {
        focused,
        pushes: pushes.subscribe(),
        reach: Arc::new(|| (true, 0)),
        connections: watch::channel(0).1,
    };
    following.start(store.clone(), None, feed, looking, crate::presence::Limits::default());
    // Typing's task runs as jawd's does, WhatsApp heard, so it names whoever types as it would.
    let typing = crate::feed::Typing::new(crate::feed::typing::Timing {
        fade: Duration::from_secs(300),
        pace: Duration::from_millis(20),
    });
    let hearing = watch::channel(true).0;
    typing.start(
        store.clone(),
        watch::channel(convert::Me::new(["351910000001@s.whatsapp.net".into()])).1,
        hearing.subscribe(),
    );
    let seen = typing.watching();
    let (events, incoming) = mpsc::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        pushes,
        None,
        home.path().to_path_buf(),
        following.clone(),
        typing.clone(),
    ));
    // What jaw.db holds once what the writer does after the messages, indexing their words, is done.
    let mut before = every_row(home.path(), rui);
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let now = every_row(home.path(), rui);
        if now == before {
            break;
        }
        before = now;
    }
    let last: i64 = scalar(home.path(), "SELECT coalesce(max(seq), 0) FROM events");

    // Ana types and records in her one-to-one chat and in a group, which is told, and stops; then
    // Rui comes online, whose presence goes the way hers would have, so once it's written they've
    // all been taken.
    let send = |kind| crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(kind) }, ack: None };
    let said = |chat: &str, state: pb::chat_presence::State| {
        let typing =
            pb::Typing { chat: chat.into(), sender: ANA.into(), state: state as i32, sender_alt: String::new() };
        send(pb::event::Kind::Typing(typing))
    };
    for chat in [ANA, GROUP] {
        events.send(said(chat, pb::chat_presence::State::Composing)).await.unwrap();
        events.send(said(chat, pb::chat_presence::State::Recording)).await.unwrap();
    }
    typed(&seen, &[(ANA, 1), (GROUP, 1)], "her recording, heard through the engine's events").await;
    for chat in [ANA, GROUP] {
        events.send(said(chat, pb::chat_presence::State::Paused)).await.unwrap();
    }
    typed(&seen, &[(ANA, 0), (GROUP, 0)], "her stopping").await;
    assert_eq!(typing.stats().heard, 6, "{:?}", typing.stats());
    let online = pb::PresenceUpdate { jid: RUI.into(), unavailable: false, last_seen_ms: 0 };
    events.send(send(pb::event::Kind::Presence(online))).await.unwrap();
    until("Rui's presence written", || {
        scalar(home.path(), &format!("SELECT count(*) FROM people WHERE n = {rui} AND presence = 'online'")) == 1
    })
    .await;
    running.abort();

    let after = every_row(home.path(), rui);
    let gone: Vec<&String> = before.iter().filter(|row| !after.contains(row)).collect();
    let came: Vec<&String> = after.iter().filter(|row| !before.contains(row)).collect();
    assert!(gone.is_empty() && came.is_empty(), "typing changed rows: {gone:#?} became {came:#?}");
    let written: Vec<(String, i64)> = Connection::open(home.path().join("jaw.db"))
        .unwrap()
        .prepare("SELECT type, key FROM events WHERE seq > ?1 ORDER BY seq")
        .unwrap()
        .query_map([last], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(written, [("presence".to_string(), rui)], "only Rui's presence reached the feed");
    assert_eq!(following.status().heard, 1, "only presence is heard, to be written");
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
    assert_eq!(crate::store::index_against_text(&db), (0, 0), "postings stale, and missing");
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
    let changes = walked(&history.encode_to_vec());
    let chat =
        changes.iter().find_map(|c| if let Change::HistoryChat { chat, .. } = c { Some(chat) } else { None }).unwrap();
    assert_eq!((chat.id.as_str(), chat.pinned, chat.muted_until), (ANA, Some(AT), Some(AT + 3_600_000)));
    let read = changes.iter().find_map(|c| if let Change::Read(read) = c { Some(read) } else { None }).unwrap();
    assert_eq!(read.phone, Some((4, 0)), "the phone counts four unread, more than the blob carries");
    let message = changes.iter().find_map(|c| if let Change::Message(m) = c { Some(m) } else { None }).unwrap();
    assert_eq!((message.chat.as_str(), message.live), (ANA, false));
    let last: Vec<&[String]> = changes[changes.len() - 2..]
        .iter()
        .filter_map(|c| if let Change::Person(person) = c { Some(person.jids.as_slice()) } else { None })
        .collect();
    assert_eq!(
        last,
        [
            &["351910000003@s.whatsapp.net".to_string(), "100000000003@lid".to_string()][..],
            &["351910000003@s.whatsapp.net".to_string()][..]
        ],
        "the blob's LID map and push names come last"
    );
}

/// Every change a history blob gives, in order.
fn walked(data: &[u8]) -> Vec<Change> {
    let mut changes = Vec::new();
    convert::history::walk(data, &me(), 2_000, |chunk| {
        changes.extend(chunk);
        true
    })
    .expect("the blob walks");
    changes
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

#[test]
fn your_own_played_receipt_says_a_message_was_opened_on_another_device_and_anyone_elses_says_theirs() {
    let receipt = |kind: &str, from_me: bool| pb::Receipt {
        chat: ANA.into(),
        sender: if from_me { "351910000001@s.whatsapp.net".into() } else { ANA.into() },
        from_me,
        ids: vec!["once".into(), "twice".into()],
        timestamp_ms: AT,
        r#type: kind.into(),
        ..Default::default()
    };
    let opened = |changes: Vec<Change>| -> Vec<String> {
        changes
            .into_iter()
            .filter_map(|c| match c {
                Change::Amend(a) if a.what == crate::store::Amendment::Opened && a.mine => Some(a.target),
                _ => None,
            })
            .collect()
    };
    assert_eq!(opened(drain::receipt_changes(&receipt("played-self", true))), ["once", "twice"]);
    assert_eq!(opened(drain::receipt_changes(&receipt("played", true))), ["once", "twice"]);
    assert_eq!(opened(drain::receipt_changes(&receipt("played", false))), Vec::<String>::new(), "theirs: a status");
    assert_eq!(opened(drain::receipt_changes(&receipt("read-self", true))), Vec::<String>::new());
    assert!(drain::receipt_changes(&receipt("played-self", true)).iter().any(|c| matches!(c, Change::Read(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clean_stop_is_noted_and_the_next_start_notes_it_is_running_before_it_returns() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    assert_eq!(store.remembered(CLEAN).unwrap(), None, "a database jawd never stopped cleanly");
    stop(&store, None).await;
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("1"));
    running(&store).await.unwrap();
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("0"), "noted before the engine can start");
}

/// An engine for the stop: it says hello once `gate` lets it (at once without one), answers the
/// disconnect unless `deaf`, and notes what jaw.db's clean marker says at the moment it's ended.
#[derive(Clone)]
struct Ending {
    store: Store,
    gate: Option<Arc<tokio::sync::Notify>>,
    deaf: bool,
    marker_when_ended: Arc<Mutex<Vec<Option<String>>>>,
}

impl Ending {
    fn new(store: &Store) -> Ending {
        Ending { store: store.clone(), gate: None, deaf: false, marker_when_ended: Arc::default() }
    }

    fn ended(&self) -> Vec<Option<String>> {
        self.marker_when_ended.lock().unwrap().clone()
    }
}

/// Notes the clean marker when the engine holding it is ended.
struct Ended(Ending);

impl Drop for Ended {
    fn drop(&mut self) {
        let marker = self.0.store.remembered(CLEAN).unwrap();
        self.0.marker_when_ended.lock().unwrap().push(marker);
    }
}

impl crate::engine::Engine for Ending {
    fn kind(&self) -> &'static str {
        "ending"
    }

    fn start(
        &self,
        _events: mpsc::Sender<crate::engine::Incoming>,
    ) -> crate::engine::BoxFuture<'_, anyhow::Result<crate::engine::Instance>> {
        let engine = self.clone();
        Box::pin(async move {
            if let Some(gate) = &engine.gate {
                gate.notified().await;
            }
            let (commands, mut outgoing) = crate::engine::Commands::for_tests();
            let deaf = engine.deaf;
            tokio::spawn(async move {
                let mut unanswered = Vec::new();
                while let Some(out) = outgoing.recv().await {
                    if let crate::engine::Outgoing::Command { reply, .. } = out {
                        if deaf {
                            unanswered.push(reply);
                        } else {
                            let _ = reply.send(Ok(pb::reply::Kind::Done(pb::Done {})));
                        }
                    }
                }
            });
            let ended = Ended(engine);
            let exited: crate::engine::BoxFuture<'static, String> = Box::pin(async move {
                let _ended = ended;
                std::future::pending().await
            });
            let hello =
                pb::Hello { protocol: crate::engine::PROTOCOL, program: "ending".into(), whatsmeow: String::new() };
            Ok(crate::engine::Instance { hello, commands, exited })
        })
    }
}

fn supervising(engine: &Ending) -> (Supervisor, mpsc::Receiver<crate::engine::Incoming>) {
    let (events, incoming) = mpsc::channel(64);
    let backoff = crate::engine::supervisor::Backoff::default();
    (Supervisor::spawn(Arc::new(engine.clone()), events, backoff), incoming)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_clean_stop_is_noted_only_once_the_engine_that_disconnected_has_been_ended() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    running(&store).await.unwrap();
    let engine = Ending::new(&store);
    let (supervisor, _incoming) = supervising(&engine);
    until("the engine to run", || supervisor.status().runs == 1).await;
    stop(&store, Some(&supervisor)).await;
    assert_eq!(engine.ended(), [Some("0".to_string())], "the marker said running while the engine ran");
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_that_says_hello_after_the_disconnect_found_none_leaves_the_stop_unclean() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    running(&store).await.unwrap();
    let gate = Arc::new(tokio::sync::Notify::new());
    let engine = Ending { gate: Some(gate.clone()), ..Ending::new(&store) };
    let (supervisor, _incoming) = supervising(&engine);
    let disconnected = disconnect(&supervisor).await;
    assert_eq!(disconnected, Some(0), "no engine was running to disconnect");
    gate.notify_one();
    until("the engine to say hello", || supervisor.status().runs == 1).await;
    supervisor.stop().await;
    noted_clean(&store, Some(&supervisor), disconnected).await;
    assert_eq!(engine.ended(), [Some("0".to_string())]);
    assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("0"), "an engine ran that never disconnected");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_that_doesnt_disconnect_in_time_is_ended_and_the_stop_stays_unclean() {
    // jawd's bound to stop is checked on the quickest of three stops, since other processes' load
    // can stall any one of them past it.
    let mut took = Vec::new();
    for _ in 0..3 {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        running(&store).await.unwrap();
        let engine = Ending { deaf: true, ..Ending::new(&store) };
        let (supervisor, _incoming) = supervising(&engine);
        until("the engine to run", || supervisor.status().runs == 1).await;
        let began = std::time::Instant::now();
        stop(&store, Some(&supervisor)).await;
        took.push(began.elapsed());
        assert_eq!(engine.ended().len(), 1, "the engine was ended");
        assert_eq!(store.remembered(CLEAN).unwrap().as_deref(), Some("0"));
    }
    let quickest = took.iter().min().unwrap();
    assert!(*quickest < DISCONNECTING + Duration::from_millis(500), "within jawd's bound to stop: {took:?}");
}

#[test]
fn a_live_poll_vote_whose_decrypted_vote_cant_be_read_stays_in_the_inbox() {
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
    let mut live = live_message(GROUP, "100000000001@lid", "v1", AT, vote.clone());
    live.decrypted = vec![1, 2, 3];
    let converted =
        drain::convert_rows(&[Row { seq: 7, kind: "message".into(), payload: live.encode_to_vec() }], &me());
    assert!(converted.changes.is_empty(), "nothing converted, and the row isn't drained");
    live.decrypted = Vec::new();
    let converted =
        drain::convert_rows(&[Row { seq: 8, kind: "message".into(), payload: live.encode_to_vec() }], &me());
    assert!(converted.changes.is_empty(), "nor one the engine couldn't decrypt");
}

#[test]
fn a_live_poll_vote_is_what_the_engine_decrypted_timed_by_its_voters_clock() {
    let vote = e2e::Message {
        poll_update_message: Some(e2e::PollUpdateMessage {
            poll_creation_message_key: Some(MessageKey {
                remote_jid: Some(GROUP.into()),
                from_me: Some(true),
                id: Some("poll".into()),
                participant: None,
            }),
            sender_timestamp_ms: Some(AT + 750),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut live = live_message(GROUP, "100000000001@lid", "v1", AT, vote);
    live.decrypted = e2e::PollVoteMessage { selected_options: vec![vec![0xca, 0xfe]] }.encode_to_vec();
    let converted =
        drain::convert_rows(&[Row { seq: 7, kind: "message".into(), payload: live.encode_to_vec() }], &me());
    let amends: Vec<&crate::store::Amend> = converted
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Amend(amend) => Some(amend),
            _ => None,
        })
        .collect();
    assert_eq!(amends.len(), 1, "{:?}", converted.changes);
    assert_eq!((amends[0].target.as_str(), amends[0].sender.as_deref()), ("poll", Some("100000000001@lid")));
    assert_eq!(amends[0].at, AT + 750, "by the voter's clock, which orders two votes of theirs");
    assert_eq!(amends[0].what, crate::store::Amendment::Vote { options: vec!["cafe".into()], id: "v1".into() });
    assert!(
        converted.changes.iter().any(|change| matches!(change, Change::Drained(crate::store::Drained::Inbox(7)))),
        "and its row is drained"
    );
}

#[test]
fn a_live_answer_or_edit_sealed_with_its_events_secret_is_drained_to_open_and_another_secret_edit_stays() {
    let event =
        MessageKey { remote_jid: Some(GROUP.into()), from_me: Some(false), id: Some("e1".into()), participant: None };
    let answer = e2e::Message {
        enc_event_response_message: Some(e2e::EncEventResponseMessage {
            event_creation_message_key: Some(event.clone()),
            enc_payload: Some(vec![1; 18]),
            enc_iv: Some(vec![2; 12]),
        }),
        ..Default::default()
    };
    let edit = |kind: e2e::secret_encrypted_message::SecretEncType| e2e::Message {
        secret_encrypted_message: Some(e2e::SecretEncryptedMessage {
            target_message_key: Some(event.clone()),
            enc_payload: Some(vec![1; 30]),
            enc_iv: Some(vec![2; 12]),
            secret_enc_type: Some(kind as i32),
            ..Default::default()
        }),
        ..Default::default()
    };
    use e2e::secret_encrypted_message::SecretEncType;
    let rows: Vec<Row> = [answer, edit(SecretEncType::EventEdit), edit(SecretEncType::MessageEdit)]
        .into_iter()
        .enumerate()
        .map(|(i, message)| Row {
            seq: i as i64 + 1,
            kind: "message".into(),
            payload: live_message(GROUP, "100000000003@lid", &format!("s{i}"), AT, message).encode_to_vec(),
        })
        .collect();
    let converted = drain::convert_rows(&rows, &me());
    let drained: Vec<i64> = converted
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Drained(crate::store::Drained::Inbox(seq)) => Some(*seq),
            _ => None,
        })
        .collect();
    assert_eq!(drained, [1, 2], "the answer and the event's edit go on, sealed; a message's edit stays in the inbox");
    let kept: Vec<&str> = converted
        .changes
        .iter()
        .filter_map(|change| match change {
            Change::Amend(crate::store::Amend {
                what: crate::store::Amendment::Keep { kind }, raw: Some(_), ..
            }) => Some(kind.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(kept, ["rsvp", "secret"], "each with its raw protobuf, to open with the event's secret");
}

/// What a scripted engine answers when asked for a history blob, by its notification's direct path.
#[derive(Clone)]
enum Blob {
    Here(Vec<u8>),
    /// WhatsApp didn't answer; asking again later may work.
    NotYet,
    /// WhatsApp no longer has it.
    Gone,
    /// The engine couldn't ask: it went, or none was running, as while jawd stops.
    NoEngine(EngineError),
}

/// An engine for tests: each instance says it's connected once it's running, answers what sync
/// asks with little, writes the history blobs it has, notes what it was asked, and stops when told.
#[derive(Clone, Default)]
struct Scripted {
    asked: Arc<Mutex<Vec<&'static str>>>,
    /// How many of the next app-state refetches fail.
    failing_resyncs: Arc<Mutex<usize>>,
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
                let mut failing = self.failing_resyncs.lock().unwrap();
                if *failing > 0 {
                    *failing -= 1;
                    ("resync", Err(EngineError::Failed { message: "no answer".into(), retry: true }))
                } else {
                    ("resync", Ok(pb::reply::Kind::Done(pb::Done {})))
                }
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
                    Some(Blob::NoEngine(e)) => Err(e),
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
        Default::default(),
        crate::feed::Typing::default(),
    ));
    until("the first engine's resync", || engine.asked("resync") == 1).await;
    engine.send(connected()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!((engine.asked("contacts"), engine.asked("resync")), (1, 1), "the same engine connecting again");
    engine.stop.notify_one();
    until("the engine started again to resync", || engine.asked("resync") == 2).await;
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_app_state_refetch_that_fails_is_tried_again_while_its_engine_runs() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    *engine.failing_resyncs.lock().unwrap() = 2;
    let (supervisor, incoming) = supervised(&engine);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        Some(supervisor.clone()),
        home.path().to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    // Tried again after 1 s and then 2 s more.
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while engine.asked("resync") < 3 {
        assert!(std::time::Instant::now() < deadline, "tried {} times", engine.asked("resync"));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(supervisor.status().restarts, 0, "by the same engine");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(engine.asked("resync"), 3, "and no more once it has");
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_app_state_refetch_that_fails_spends_no_try_while_whatsapp_cant_be_reached() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    *engine.failing_resyncs.lock().unwrap() = 1;
    let (supervisor, incoming) = supervised(&engine);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        Some(supervisor.clone()),
        home.path().to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    until("the first engine's resync", || engine.asked("resync") == 1).await;
    engine.send(connection_event(pb::connection::State::Disconnected).event).await;
    // Out of reach for longer than the wait before the first try again, 1 s.
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    assert_eq!(engine.asked("resync"), 1, "no try is spent while WhatsApp can't be reached");
    engine.send(connected()).await;
    until("the refetch tried again once WhatsApp could be reached", || engine.asked("resync") == 2).await;
    assert_eq!(supervisor.status().restarts, 0, "by the same engine");
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
        knowing(),
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
async fn a_history_blob_the_engine_couldnt_ask_for_is_never_quarantined() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    notifications(&store, 3).await;
    let engine = Scripted::default();
    engine.blobs.lock().unwrap().extend([
        ("went".to_string(), Blob::NoEngine(EngineError::Gone)),
        ("stopped".to_string(), Blob::NoEngine(EngineError::Unavailable("it's stopped".into()))),
        ("here".to_string(), Blob::Here(blob("h1"))),
    ]);
    let (supervisor, _incoming) = supervised(&engine);
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(
        offers,
        store.clone(),
        supervisor,
        knowing(),
        home.path().join("spool"),
        Merges::default(),
    ));
    for work in [offer(1, "went"), offer(2, "stopped"), offer(3, "here")] {
        offered.send(work).unwrap();
    }
    until("the blob after them", || store.count("messages").unwrap() == 1).await;
    until("its notification's drain, which commits after its rows", || store.count("inbox").unwrap() == 2).await;
    until("each asked for", || engine.asked("history") >= 3).await;
    fetching.abort();
    assert_eq!(store.count("quarantine").unwrap(), 0, "neither is a failure for good");
    assert_eq!(store.count("inbox").unwrap(), 2, "both wait to be asked for again");
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
    store.apply(Lane::Background, walked(&blob("h1"))).await.unwrap();
    assert!(file.exists(), "the blob stays until its drain is committed");
    let engine = Scripted::default();
    let (supervisor, _incoming) = supervised(&engine);
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(offers, store.clone(), supervisor, knowing(), spool, Merges::default()));
    offered.send(offer(1, "here")).unwrap();
    until("the notification's drain", || store.count("inbox").unwrap() == 0).await;
    // The blob goes once its drain has committed, so after the inbox empties.
    until("the blob to leave the spool", || !file.exists()).await;
    fetching.abort();
    assert_eq!(engine.asked("history"), 0, "the blob came from the spool");
    assert_eq!(store.count("messages").unwrap(), 1, "nothing twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blob_stays_in_the_spool_while_its_drain_cant_commit_and_goes_once_it_has() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    notifications(&store, 1).await;
    // Nothing but settings, so the commit that drains it is the only write it makes.
    let settings = wa_proto::wa_web_protobufs_history_sync::HistorySync {
        sync_type: 2,
        global_settings: Some(Default::default()),
        ..Default::default()
    };
    let engine = Scripted::default();
    engine.blobs.lock().unwrap().insert("here".to_string(), Blob::Here(settings.encode_to_vec()));
    let (supervisor, _incoming) = supervised(&engine);
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    // Another connection holds jaw.db's write lock, so the drain can't commit, as with a full disk,
    // and the writer tries it again until it can.
    let outside = Connection::open(home.path().join("jaw.db")).unwrap();
    outside.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut trouble = store.trouble();
    let spool = home.path().join("spool");
    let file = spool.join("offer-here.pb");
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(offers, store.clone(), supervisor, knowing(), spool, Merges::default()));
    offered.send(offer(1, "here")).unwrap();
    tokio::time::timeout(Duration::from_secs(30), trouble.wait_for(Option::is_some)).await.unwrap().unwrap();
    assert!(file.exists(), "the blob stays while its drain can't commit");
    assert_eq!(store.count("inbox").unwrap(), 1, "and so does its notification");
    outside.execute_batch("ROLLBACK").unwrap();
    until("the notification's drain", || store.count("inbox").unwrap() == 0).await;
    until("the blob to leave the spool", || !file.exists()).await;
    fetching.abort();
    assert_eq!(store.count("history_kept").unwrap(), 1, "what it carried, kept before it went");
    assert_eq!(engine.asked("history"), 1, "fetched once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_notification_whose_conversion_panics_keeps_its_blob_in_the_spool_for_a_jawd_that_converts_it() {
    let home = tempfile::tempdir().unwrap();
    let spool = home.path().join("spool");
    std::fs::create_dir_all(&spool).unwrap();
    let note = "3EB0PANICSNOTE";
    let its_blob = spool.join(format!("{note}.pb"));
    {
        let store = Store::open(home.path()).unwrap();
        store.inbox("message", 0, notification(note, "kept").encode_to_vec(), None);
        store.inbox("message", 0, live_message(ANA, ANA, "after", AT, text("after it")).encode_to_vec(), None);
        store.inbox("message", 0, notification("3EB0NOTEAFTER", "later").encode_to_vec(), None);
        until("the inbox", || store.count("inbox").unwrap() == 3).await;
        // An earlier run downloaded the notification's blob and stopped before its walk ended,
        // another drained a blob's notification and stopped before removing the blob, and another
        // stopped in the middle of a download.
        std::fs::write(&its_blob, blob("h1")).unwrap();
        let done = spool.join("3EB0DONE.pb");
        std::fs::write(&done, blob("h0")).unwrap();
        let rest = Packed::new(b"what the blob carried besides its messages");
        store.apply_durable(vec![Change::HistoryKept { id: "3EB0DONE".into(), raw: rest }]).await.unwrap();
        let part = spool.join("3EB0HALF.pb.part");
        std::fs::write(&part, b"half a blob").unwrap();
        let engine = Scripted::default();
        engine.blobs.lock().unwrap().insert("later".into(), Blob::Here(blob("h2")));
        let (supervisor, _incoming) = supervised(&engine);
        until("the engine to run", || supervisor.status().state.name() == "running").await;
        let (fetcher, offers) = mpsc::unbounded_channel();
        let fetching =
            tokio::spawn(fetch::run(offers, store.clone(), supervisor, knowing(), spool.clone(), Merges::default()));
        let (offered, mut handed) = mpsc::unbounded_channel();
        let converter =
            tokio::spawn(drain::inbox(store.clone(), knowing(), offered, Avatars::new(), Merges::default()));
        // What the converter hands the fetcher goes through the test: the offers, then that the
        // inbox has been taken, which sweeps the spool. The fetcher is handed that first, so the
        // sweep is done once the offer after it is stored.
        let mut later = Vec::new();
        loop {
            match tokio::time::timeout(WAIT, handed.recv()).await.expect("the converter reaches the inbox's end") {
                Some(fetch::Work::Offer(offer)) => later.push(offer),
                Some(fetch::Work::Offered) => break,
                None => panic!("the converter stopped"),
            }
        }
        let ids: Vec<(i64, &str)> = later.iter().map(|offer| (offer.seq, offer.id.as_str())).collect();
        assert_eq!(ids, [(3, "3EB0NOTEAFTER")], "the notification after it is offered, and not the one that panicked");
        let before = stored(home.path(), &store);
        let all = blob("h1").len() + blob("h0").len() + b"half a blob".len();
        assert_eq!(
            (before.spool_files, before.spool_bytes),
            (3, all as u64),
            "jawd status counts every file in the spool before the sweep, the download cut short among them"
        );
        fetcher.send(fetch::Work::Offered).unwrap();
        for offer in later {
            fetcher.send(fetch::Work::Offer(offer)).unwrap();
        }
        until("the offer after the sweep", || store.count("inbox").unwrap() == 1).await;
        converter.abort();
        fetching.abort();
        assert_eq!(column::<String>(&store, "SELECT id FROM messages ORDER BY id").await, ["after", "h2"]);
        assert_eq!(column::<i64>(&store, "SELECT seq FROM inbox").await, [1], "the notification stays in the inbox");
        let kept: Vec<(String, String)> =
            quarantined(&store).await.into_iter().map(|(source, reference, ..)| (source, reference)).collect();
        assert_eq!(kept, [("inbox".to_string(), "1".to_string())], "kept in quarantine");
        assert!(its_blob.exists(), "the blob it names stays in the spool");
        assert!(!done.exists() && !part.exists(), "what's done with goes");
        let status = stored(home.path(), &store);
        assert_eq!((status.spool_files, status.spool_bytes), (1, blob("h1").len() as u64), "jawd status counts it");
    }
    // A jawd whose conversion of the notification doesn't panic walks the blob from the spool.
    mend(note);
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let (supervisor, _incoming) = supervised(&engine);
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(offers, store.clone(), supervisor, knowing(), spool, Merges::default()));
    let converter = tokio::spawn(drain::inbox(store.clone(), knowing(), offered, Avatars::new(), Merges::default()));
    until("the notification's drain", || store.count("inbox").unwrap() == 0).await;
    converter.abort();
    fetching.abort();
    assert_eq!(engine.asked("history"), 0, "the blob came from the spool");
    assert_eq!(column::<String>(&store, "SELECT id FROM messages ORDER BY id").await, ["after", "h1", "h2"]);
    assert_eq!(store.count("quarantine").unwrap(), 0, "its drain took its quarantine row with it");
    assert!(!its_blob.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sweep_that_cant_read_which_blobs_are_in_keeps_every_blob() {
    let home = tempfile::tempdir().unwrap();
    let spool = home.path().join("spool");
    std::fs::create_dir_all(&spool).unwrap();
    let store = Store::open(home.path()).unwrap();
    let rest = Packed::new(b"what the blob carried besides its messages");
    store.apply_durable(vec![Change::HistoryKept { id: "3EB0DONE".into(), raw: rest }]).await.unwrap();
    let (done, waiting, part) =
        (spool.join("3EB0DONE.pb"), spool.join("3EB0WAITING.pb"), spool.join("3EB0HALF.pb.part"));
    std::fs::write(&done, blob("h0")).unwrap();
    std::fs::write(&waiting, blob("h1")).unwrap();
    std::fs::write(&part, b"half a blob").unwrap();
    // The sweep's read of which blobs are in fails.
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        db.execute("ALTER TABLE history_kept RENAME TO history_kept_away", []).unwrap();
    }
    let engine = Scripted::default();
    engine.blobs.lock().unwrap().insert("later".into(), Blob::Here(blob("h2")));
    let (supervisor, _incoming) = supervised(&engine);
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching =
        tokio::spawn(fetch::run(offers, store.clone(), supervisor, knowing(), spool.clone(), Merges::default()));
    // The fetcher sweeps before it takes the offer after it, so the sweep is done once that
    // offer's blob is walked.
    offered.send(fetch::Work::Offered).unwrap();
    offered.send(offer(1, "later")).unwrap();
    until("the blob offered after the sweep", || store.count("messages WHERE id = 'h2'").unwrap() == 1).await;
    fetching.abort();
    assert!(!part.exists(), "a download cut short goes");
    assert!(waiting.exists(), "a blob whose rows aren't in stays");
    assert!(done.exists(), "and so does one whose rows are in, as the sweep can't know it");
}

/// A connection event as the engine sends it, for the account 351910000001 (Pedro).
fn connection_event(state: pb::connection::State) -> crate::engine::Incoming {
    let linked = !matches!(state, pb::connection::State::Unlinked);
    let connection = pb::Connection {
        state: state as i32,
        jid: if linked { "351910000001:12@s.whatsapp.net".into() } else { String::new() },
        lid: if linked { "100000000001:12@lid".into() } else { String::new() },
        push_name: if linked { "Pedro".into() } else { String::new() },
        ..Default::default()
    };
    crate::engine::Incoming {
        event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Connection(connection)) },
        ack: None,
    }
}

/// Runs sync on `store` with the engine's `events`, and returns the live state once it's taken
/// them, with the `connection` pushes interfaces were sent.
async fn after(
    store: &Store,
    home: &Path,
    live: Live,
    events: Vec<crate::engine::Incoming>,
) -> (Shared, Vec<jaw_proto::Connection>) {
    let live: Shared = Arc::new(Mutex::new(live));
    let (tx, incoming) = mpsc::channel(16);
    let (pushes, mut told) = broadcast::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        live.clone(),
        pushes,
        None,
        home.to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    for event in events {
        tx.send(event).await.unwrap();
    }
    drop(tx);
    running.await.unwrap();
    let mut connections = Vec::new();
    while let Ok(push) = told.try_recv() {
        if push.method == "connection" {
            connections.push(serde_json::from_value(push.params).unwrap());
        }
    }
    (live, connections)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_folder_starts_unlinked() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let live = starting(&store, true);
    assert_eq!(live.account, Account { state: AccountState::Unlinked, me: None });
    assert_eq!(live.connection.state, ConnectionState::Offline);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_linked_at_the_last_connection_starts_linked_and_connecting() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    use pb::connection::State;
    after(&store, home.path(), starting(&store, true), vec![connection_event(State::Connected)]).await;
    let pedro = Me { id: "351910000001:12@s.whatsapp.net".into(), name: "Pedro".into(), phone: "+351910000001".into() };
    let live = starting(&store, true);
    assert_eq!(live.account, Account { state: AccountState::Linked, me: Some(pedro.clone()) });
    assert_eq!(live.connection.state, ConnectionState::Connecting, "linked, connecting");
    assert_eq!(starting(&store, false).connection.state, ConnectionState::Offline, "with no engine to connect");
    assert!(
        live.me.borrow().jids.contains(&"351910000001@s.whatsapp.net".to_string()),
        "its own JIDs, for mentions of it"
    );
    let (live, _) = after(&store, home.path(), live, vec![connection_event(State::Connecting)]).await;
    let live = live.lock().unwrap();
    assert_eq!(live.account, Account { state: AccountState::Linked, me: Some(pedro) }, "as the engine says too");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_logged_out_while_jawd_was_off_ends_up_logged_out_and_the_next_start_unlinked() {
    use pb::connection::State;
    // Logged out on the phone: session.db still has the device, so the engine connects, and
    // WhatsApp says so. Or the device went from session.db, and the engine says unlinked at once.
    for (how, events) in [
        ("on the phone", vec![connection_event(State::Connecting), connection_event(State::LoggedOut)]),
        ("with session.db's device gone", vec![connection_event(State::Unlinked)]),
    ] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        after(&store, home.path(), starting(&store, true), vec![connection_event(State::Connected)]).await;
        let live = starting(&store, true);
        assert_eq!(live.account.state, AccountState::Linked, "{how}: linked as far as jawd knew");
        let (live, told) = after(&store, home.path(), live, events).await;
        let ended = {
            let live = live.lock().unwrap();
            (live.account.clone(), live.connection.state)
        };
        assert_eq!(ended, (Account { state: AccountState::Unlinked, me: None }, ConnectionState::LoggedOut), "{how}");
        let last = told.last().unwrap();
        assert_eq!(last.state, ConnectionState::LoggedOut, "{how}: interfaces told it was linked hear it's logged out");
        if how != "on the phone" {
            assert_eq!(last.reason.as_deref(), Some(NOT_LINKED), "and why");
        }
        let next = starting(&store, true);
        assert_eq!(next.account, Account { state: AccountState::Unlinked, me: None }, "{how}: the next start");
        assert_eq!(next.connection.state, ConnectionState::Offline);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_with_no_device_is_offline_to_an_interface_never_told_the_account_was_linked() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (live, told) =
        after(&store, home.path(), starting(&store, true), vec![connection_event(pb::connection::State::Unlinked)])
            .await;
    assert_eq!(live.lock().unwrap().connection.state, ConnectionState::Offline);
    assert_eq!(told.iter().map(|c| c.state).collect::<Vec<_>>(), [ConnectionState::Offline], "nothing was logged out");
    assert_eq!(store.remembered(ACCOUNT).unwrap(), None, "and nothing written");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_account_is_kept_in_the_order_the_engine_said_it() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut other = connection_event(State::Connected);
    if let Some(pb::event::Kind::Connection(c)) = other.event.kind.as_mut() {
        (c.jid, c.lid, c.push_name) = ("351910000009:3@s.whatsapp.net".into(), String::new(), "Rui".into());
    }
    let events = vec![connection_event(State::Connected), connection_event(State::LoggedOut), other];
    after(&store, home.path(), starting(&store, true), events).await;
    assert_eq!(starting(&store, true).account.me.unwrap().name, "Rui", "the account linked after the logout");
    after(&store, home.path(), starting(&store, true), vec![connection_event(State::LoggedOut)]).await;
    assert_eq!(starting(&store, true).account.state, AccountState::Unlinked);
}

/// The next `connection` push's state, if one comes within 400 ms.
async fn next_connection(told: &mut broadcast::Receiver<Push>) -> Option<ConnectionState> {
    loop {
        let push = tokio::time::timeout(Duration::from_millis(400), told.recv()).await.ok()?.unwrap();
        if push.method == "connection" {
            return Some(serde_json::from_value::<jaw_proto::Connection>(push.params).unwrap().state);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_engine_next_word_waits_until_the_account_it_said_before_is_stored() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let live: Shared = Arc::new(Mutex::new(starting(&store, true)));
    let (tx, incoming) = mpsc::channel(16);
    let (pushes, mut told) = broadcast::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        live,
        pushes,
        None,
        home.path().to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(connection_event(State::LoggedOut)).await.unwrap();
    assert_eq!(next_connection(&mut told).await, Some(ConnectionState::Online));
    let waiting = next_connection(&mut told).await;
    assert_eq!(waiting, None, "the logout isn't taken while the connection's account can't be stored");
    holder.execute_batch("COMMIT").unwrap();
    assert_eq!(next_connection(&mut told).await, Some(ConnectionState::LoggedOut), "and is once it is");
    drop(tx);
    running.await.unwrap();
    assert_eq!(store.remembered(ACCOUNT).unwrap().as_deref(), Some(""), "the logout last");
}

/// A small JPEG, as a link preview's thumbnail comes.
fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::from_pixel(w, h, image::Rgb([200, 60, 20]))
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 60))
        .unwrap();
    out
}

fn previewed(text: &str, url: &str, thumbnail: Option<Vec<u8>>) -> e2e::Message {
    e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some(text.into()),
            matched_text: Some(url.into()),
            title: Some("Escalada em Sintra".into()),
            description: Some("Os melhores blocos".into()),
            jpeg_thumbnail: thumbnail,
            ..Default::default()
        })),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_preview_that_arrives_shows_its_title_and_its_thumbnail_once_its_file_is_whole() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let thumbnail = jpeg(40, 30);
    let sha = crate::store::thumbnails::sha256(&thumbnail);
    let events = [
        live_message(ANA, ANA, "p1", AT, previewed("vê www.8a.nu/sintra", "www.8a.nu/sintra", Some(thumbnail.clone()))),
        live_message(ANA, ANA, "p2", AT + 1, previewed("e https://8a.nu/peneda", "https://8a.nu/peneda", None)),
    ];
    for event in &events {
        store.inbox("message", 0, event.encode_to_vec(), None);
    }
    let converter = converting(&store);
    drained(&store).await;
    until("its thumbnail to be kept", || store.thumbnails_kept().unwrap().0 == 1).await;
    converter.abort();
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let link = |n: usize| {
        let detail: serde_json::Value = serde_json::from_str(rows[n].detail.as_ref().unwrap().get()).unwrap();
        detail["link"].clone()
    };
    assert_eq!(
        link(0),
        serde_json::json!({
            "url": "https://www.8a.nu/sintra",
            "title": "Escalada em Sintra",
            "description": "Os melhores blocos",
            "preview": format!("media/links/{sha}.jpg"),
        })
    );
    assert_eq!(std::fs::read(home.path().join(format!("media/links/{sha}.jpg"))).unwrap(), thumbnail);
    assert_eq!(link(1)["title"], "Escalada em Sintra");
    assert!(link(1).get("preview").is_none(), "no thumbnail came with it");
    assert_eq!(store.thumbnails_kept().unwrap(), (1, thumbnail.len() as u64));
    assert_eq!(scalar(home.path(), "SELECT count(*) FROM links"), 2, "each preview's URL is a link");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_message_and_its_batch_are_in_before_its_previews_thumbnail_is_written_and_synced() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let slow = Duration::from_secs(2);
    store.thumbnails().slow(slow);
    let thumbnail = jpeg(40, 30);
    let events = [
        live_message(ANA, ANA, "p1", AT, previewed("vê www.8a.nu/sintra", "www.8a.nu/sintra", Some(thumbnail.clone()))),
        live_message(ANA, ANA, "p2", AT + 1, previewed("e https://8a.nu/peneda", "https://8a.nu/peneda", None)),
    ];
    for event in &events {
        store.inbox("message", 0, event.encode_to_vec(), None);
    }
    let began = Instant::now();
    let converter = converting(&store);
    drained(&store).await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert!(began.elapsed() < slow, "the batch waited {:?} for its thumbnail's file", began.elapsed());
    let preview = |row: &read::MessageRow| {
        let detail: serde_json::Value = serde_json::from_str(row.detail.as_ref().unwrap().get()).unwrap();
        detail["link"].get("preview").cloned()
    };
    assert_eq!((rows.len(), preview(&rows[0])), (2, None), "both are in, p1 without its thumbnail yet");
    until("its thumbnail to be kept", || store.thumbnails_kept().unwrap().0 == 1).await;
    converter.abort();
    let shown = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let sha = crate::store::thumbnails::sha256(&thumbnail);
    assert_eq!(preview(&shown[0]), Some(serde_json::json!(format!("media/links/{sha}.jpg"))));
    assert!(shown[0].v > rows[0].v, "it takes a new version as its thumbnail shows, so windows show it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_preview_whose_thumbnail_cant_be_kept_shows_without_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // A file in the way of the folder the thumbnails go in, as a disk that won't take them is.
    std::fs::create_dir_all(home.path().join("media")).unwrap();
    std::fs::write(home.path().join("media/links"), b"not a folder").unwrap();
    let event = live_message(ANA, ANA, "p1", AT, previewed("vê https://8a.nu", "https://8a.nu", Some(jpeg(8, 8))));
    store.inbox("message", 0, event.encode_to_vec(), None);
    let converter = converting(&store);
    drained(&store).await;
    converter.abort();
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let detail: serde_json::Value = serde_json::from_str(rows[0].detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["link"]["title"], "Escalada em Sintra", "the message is in, with its preview");
    assert!(detail["link"].get("preview").is_none(), "but not its thumbnail");
    assert_eq!(store.thumbnails_kept().unwrap(), (0, 0));
}

/// Measures live messages with link previews, alternately with a thumbnail of their own and
/// without, each from its inbox row's hand-over to its row being in jaw.db, the converter running
/// as jawd runs it; each thumbnail is kept before the next message comes, so each is timed alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement: cargo test --release -p jawd measure_live_previews -- --ignored --nocapture"]
async fn measure_live_previews_with_and_without_a_thumbnail() {
    let runs: i64 = std::env::var("JAW_MEASURE_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(120);
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let converter = converting(&store);
    let (mut with, mut without) = (Vec::new(), Vec::new());
    for k in 0..runs {
        let thumbnail = (k % 2 == 0).then(|| {
            let mut out = Vec::new();
            image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([(x * 6) as u8, (y * 8) as u8, k as u8]))
                .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 60))
                .unwrap();
            out
        });
        let id = format!("p{k}");
        let event =
            live_message(ANA, ANA, &id, AT + k, previewed("vê www.8a.nu/sintra", "www.8a.nu/sintra", thumbnail));
        let began = Instant::now();
        store.inbox("message", 0, event.encode_to_vec(), None);
        loop {
            let id = id.clone();
            let found =
                query(&store, move |db| Ok(db.prepare_cached("SELECT 1 FROM messages WHERE id = ?1")?.exists([id])?))
                    .await;
            if found {
                break;
            }
            tokio::time::sleep(Duration::from_micros(100)).await;
        }
        if k % 2 == 0 {
            with.push(began.elapsed())
        } else {
            without.push(began.elapsed())
        }
        let kept = (k / 2 + 1) as u64;
        until("its thumbnail to be kept", || store.thumbnails_kept().unwrap().0 == kept).await;
    }
    converter.abort();
    for (what, mut took) in [("with a thumbnail", with), ("without", without)] {
        took.sort();
        let at = |q: f64| took[((took.len() - 1) as f64 * q).round() as usize].as_secs_f64() * 1e3;
        eprintln!(
            "a live preview {what}, hand-over to its row: p50 {:.2} ms, p99 {:.2} ms, worst {:.2} ms, of {}",
            at(0.5),
            at(0.99),
            at(1.0),
            took.len()
        );
    }
}

/// How long a test waits for a push it expects, however busy the machine.
const WAIT: Duration = Duration::from_secs(10);

/// The supervisor of `engine`, pausing `pause` before it starts another, long enough to see the
/// pause on a busy machine.
fn supervised_pausing(engine: &Scripted, pause: Duration) -> (Supervisor, mpsc::Receiver<crate::engine::Incoming>) {
    let (events, incoming) = mpsc::channel(64);
    let backoff = crate::engine::supervisor::Backoff { first: pause, most: pause, settled: Duration::from_secs(60) };
    (Supervisor::spawn(Arc::new(engine.clone()), events, backoff), incoming)
}

/// The next `connection` push's state, if one comes within `within`.
async fn next_connection_within(told: &mut broadcast::Receiver<Push>, within: Duration) -> Option<ConnectionState> {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let push = tokio::time::timeout_at(deadline, told.recv()).await.ok()?.unwrap();
        if push.method == "connection" {
            return Some(serde_json::from_value::<jaw_proto::Connection>(push.params).unwrap().state);
        }
    }
}

/// What a running sync on `store` says, with `engine` supervised as `supervisor`.
fn syncing(
    store: &Store,
    home: &Path,
    supervisor: Supervisor,
    incoming: mpsc::Receiver<crate::engine::Incoming>,
) -> (Shared, broadcast::Receiver<Push>, tokio::task::JoinHandle<()>) {
    syncing_from(store, home, Some(supervisor), incoming, starting(store, true))
}

/// What a running sync on `store` says, starting from `live`.
fn syncing_from(
    store: &Store,
    home: &Path,
    supervisor: Option<Supervisor>,
    incoming: mpsc::Receiver<crate::engine::Incoming>,
    live: Live,
) -> (Shared, broadcast::Receiver<Push>, tokio::task::JoinHandle<()>) {
    let live: Shared = Arc::new(Mutex::new(live));
    let (pushes, told) = broadcast::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        live.clone(),
        pushes,
        supervisor,
        home.to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    (live, told, running)
}

/// The phone's word that WhatsApp has more of a catch-up's messages waiting.
fn offline_preview(messages: u32) -> pb::Event {
    let preview = pb::OfflinePreview { total: messages, messages, ..Default::default() };
    pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) }
}

/// The next `n` `connection` pushes' states, as far as they come, each within WAIT.
async fn next_connections(told: &mut broadcast::Receiver<Push>, n: usize) -> Vec<ConnectionState> {
    let mut said = Vec::new();
    while said.len() < n
        && let Some(state) = next_connection_within(told, WAIT).await
    {
        said.push(state);
    }
    said
}

#[tokio::test(flavor = "multi_thread")]
async fn a_logout_that_cuts_a_catch_up_short_ends_it_so_the_account_linked_again_is_online() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(connection_event(State::LoggedOut)).await.unwrap();
    // The phone links jaw again: a fresh device, connected.
    tx.send(connection_event(State::Connected)).await.unwrap();
    let said = next_connections(&mut told, 4).await;
    let expected =
        [ConnectionState::Online, ConnectionState::CatchingUp, ConnectionState::LoggedOut, ConnectionState::Online];
    assert_eq!(said, expected, "linked again and online");
    {
        let live = live.lock().unwrap();
        assert_eq!(live.account.state, AccountState::Linked);
        assert!(!live.catching_up(), "the catch-up the logout cut short ended");
        assert!(live.catch_up.as_ref().unwrap().seconds.is_some(), "so the chat list and notifications go on");
    }
    assert!(!crate::feed::catching_up(live.clone())(), "as the feed and the notifications read it");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_drops_during_a_catch_up_ends_it_and_the_next_connection_is_online() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(connection_event(State::Disconnected)).await.unwrap();
    tx.send(connection_event(State::Connected)).await.unwrap();
    let said = next_connections(&mut told, 4).await;
    let expected =
        [ConnectionState::Online, ConnectionState::CatchingUp, ConnectionState::Offline, ConnectionState::Online];
    assert_eq!(said, expected, "the next connection isn't taken for part of the catch-up");
    assert!(!live.lock().unwrap().catching_up());
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_logout_jawd_asks_for_ends_a_catch_up_it_cuts_short() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    assert_eq!(next_connections(&mut told, 2).await, [ConnectionState::Online, ConnectionState::CatchingUp]);
    let pushed = logged_out(&store, &live, "you logged out").await.unwrap();
    assert_eq!(pushed.params["state"], "logged_out");
    assert!(!live.lock().unwrap().catching_up(), "the catch-up the logout cut short ended");
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online), "linked again");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_that_stops_has_jawd_reconnecting_until_the_next_says_how_it_is_and_ends_its_catch_up() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let (supervisor, incoming) = supervised_pausing(&engine, Duration::from_secs(1));
    let (live, mut told, running) = syncing(&store, home.path(), supervisor.clone(), incoming);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    let preview = pb::OfflinePreview { total: 10, messages: 10, ..Default::default() };
    engine.send(pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) }).await;
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::CatchingUp));
    engine.stop.notify_one();
    assert_eq!(
        next_connection_within(&mut told, WAIT).await,
        Some(ConnectionState::EngineRestarting),
        "while it starts again"
    );
    {
        let live = live.lock().unwrap();
        assert_eq!(live.connection.state, ConnectionState::EngineRestarting, "and a new hello says so too");
        assert!(!live.catching_up(), "the catch-up it cut short ended");
        assert!(live.catch_up.as_ref().unwrap().seconds.is_some(), "so the chat list and notifications go on");
        assert_eq!(live.account.state, AccountState::Linked, "the account is as it was");
    }
    let next = next_connection_within(&mut told, WAIT).await;
    assert_eq!(next, Some(ConnectionState::Online), "the next engine's word");
    assert_eq!(supervisor.status().restarts, 1);
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_stopped_engine_sent_comes_before_jawd_says_it_is_reconnecting_and_the_next_engines_after() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let (supervisor, incoming) = supervised_pausing(&engine, Duration::from_secs(1));
    let (_live, mut told, running) = syncing(&store, home.path(), supervisor.clone(), incoming);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    // Holding jaw.db's writer holds sync on the account the next Connected names, so what the
    // engine sends after it waits in `incoming` while the engine stops.
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    engine.send(connection_event(State::Connected).event).await;
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    engine.send(connection_event(State::Disconnected).event).await;
    engine.stop.notify_one();
    until("the engine to stop", || supervisor.status().state == crate::engine::supervisor::State::Restarting).await;
    holder.execute_batch("COMMIT").unwrap();
    let mut said = Vec::new();
    while said.len() < 3 {
        match next_connection_within(&mut told, WAIT).await {
            Some(state) => said.push(state),
            None => break,
        }
    }
    let expected = [ConnectionState::Offline, ConnectionState::EngineRestarting, ConnectionState::Online];
    assert_eq!(said, expected, "the stopped engine's last word, then reconnecting, then the next engine's");
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn what_whatsapp_said_of_the_account_stands_while_the_engine_starts_again() {
    use pb::connection::State;
    for (state, said) in [
        (State::LoggedOut, ConnectionState::LoggedOut),
        (State::ClientOutdated, ConnectionState::UpdateNeeded),
        (State::TemporaryBan, ConnectionState::Banned),
    ] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let engine = Scripted::default();
        let (supervisor, incoming) = supervised_pausing(&engine, Duration::from_secs(1));
        let (live, mut told, running) = syncing(&store, home.path(), supervisor.clone(), incoming);
        assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
        engine.send(connection_event(state).event).await;
        assert_eq!(next_connection_within(&mut told, WAIT).await, Some(said));
        engine.stop.notify_one();
        until("the engine to stop", || supervisor.status().restarts == 1).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(live.lock().unwrap().connection.state, said, "{said:?} stands while it starts again");
        running.abort();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_while_sync_waits_on_jaw_db_for_longer_than_the_pause_is_still_said_in_order_and_ends_its_catch_up() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let pause = Duration::from_millis(300);
    let (supervisor, incoming) = supervised_pausing(&engine, pause);
    let (live, mut told, running) = syncing(&store, home.path(), supervisor.clone(), incoming);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    engine.send(offline_preview(10)).await;
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::CatchingUp));
    // Holding jaw.db's writer holds sync on the account the next Connected names, while the engine
    // stops and for longer than the pause before the next would start.
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    engine.send(connection_event(State::Connected).event).await;
    assert_eq!(
        next_connection_within(&mut told, WAIT).await,
        Some(ConnectionState::CatchingUp),
        "taken, still catching up"
    );
    engine.stop.notify_one();
    until("the engine to stop", || supervisor.status().restarts == 1).await;
    tokio::time::sleep(pause * 4).await;
    assert_eq!(
        supervisor.status().state,
        crate::engine::supervisor::State::Restarting,
        "the next engine waits for sync to say the stop"
    );
    holder.execute_batch("COMMIT").unwrap();
    let said = next_connections(&mut told, 2).await;
    assert_eq!(
        said,
        [ConnectionState::EngineRestarting, ConnectionState::Online],
        "reconnecting, then the next engine's word"
    );
    assert!(!live.lock().unwrap().catching_up(), "the catch-up the stop cut short ended");
    running.abort();
}

/// A live message from Ana in `chat`, as the engine hands it over.
fn arriving(chat: &str, id: &str) -> crate::engine::Incoming {
    let message = live_message(chat, ANA, id, AT, text("while you were away"));
    crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(message)) }, ack: None }
}

/// WhatsApp's word that what waited offline has all come, `count` things in all.
fn offline_done(count: u32) -> crate::engine::Incoming {
    let done = pb::OfflineDone { count };
    crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflineDone(done)) }, ack: None }
}

/// The next `n` `connection` and `notice` pushes, as far as they come, each within WAIT.
async fn next_told(told: &mut broadcast::Receiver<Push>, n: usize) -> Vec<Push> {
    let mut said = Vec::new();
    while said.len() < n {
        match tokio::time::timeout(WAIT, told.recv()).await {
            Ok(Ok(push)) if push.method == "connection" || push.method == "notice" => said.push(push),
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    said
}

/// What a `connection` push says: its state, and how far a catch-up has got of how much.
fn connection_of(push: &Push) -> (String, Option<u64>, Option<u64>) {
    assert_eq!(push.method, "connection", "{push:?}");
    let connection: jaw_proto::Connection = serde_json::from_value(push.params.clone()).unwrap();
    (push.params["state"].as_str().unwrap().to_string(), connection.done, connection.total)
}

fn said(state: &str, done: Option<u64>, total: Option<u64>) -> (String, Option<u64>, Option<u64>) {
    (state.to_string(), done, total)
}

#[tokio::test(flavor = "multi_thread")]
async fn pings_that_get_through_again_after_a_keepalive_timeout_say_online_with_nothing_written_to_jaw_db() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (_live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    until("the account and its JIDs to be stored", || store.remembered(ME).is_ok_and(|me| me.is_some())).await;
    let kept = || (store.remembered(ACCOUNT).unwrap(), store.remembered(ME).unwrap());
    let before = kept();
    // Holding jaw.db's writer holds sync on any write it waits for, so the word after one would wait.
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    // Named otherwise than at the connection, so that writing what a restore says would show.
    let mut restored = connection_event(State::KeepaliveRestored);
    if let Some(pb::event::Kind::Connection(c)) = restored.event.kind.as_mut() {
        (c.lid, c.push_name) = ("100000000077:12@lid".into(), "Someone else".into());
    }
    tx.send(restored).await.unwrap();
    tx.send(connection_event(State::Disconnected)).await.unwrap();
    let states = next_connections(&mut told, 3).await;
    let expected = [ConnectionState::Connecting, ConnectionState::Online, ConnectionState::Offline];
    assert_eq!(states, expected, "the restore is online, and waits on nothing in jaw.db");
    holder.execute_batch("ROLLBACK").unwrap();
    drop(tx);
    running.await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    store.apply(Lane::Live, Vec::new()).await.unwrap();
    assert_eq!(kept(), before, "nor writes anything there");
}

#[tokio::test(flavor = "multi_thread")]
async fn pings_that_get_through_again_during_a_catch_up_put_it_back_as_far_as_it_has_got_and_its_summary_comes_at_its_end()
 {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(arriving(ANA, "3EB0AWAY1")).await.unwrap();
    tx.send(arriving(GROUP, "3EB0AWAY2")).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    tx.send(arriving(ANA, "3EB0AWAY3")).await.unwrap();
    until("the message to be counted", || live.lock().unwrap().catch_up.as_ref().is_some_and(|c| c.stored == 3)).await;
    let connecting = live.lock().unwrap().connection.clone();
    assert_eq!(connecting, jaw_proto::Connection::new(ConnectionState::Connecting), "no progress said as connecting");
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    let pushed: Vec<_> = next_told(&mut told, 4).await.iter().map(connection_of).collect();
    let expected = [
        said("online", None, None),
        said("catching_up", Some(0), Some(10)),
        said("connecting", None, None),
        said("catching_up", Some(3), Some(10)),
    ];
    assert_eq!(pushed, expected, "the catch-up back, as far as it has got");
    assert!(crate::feed::catching_up(live.clone())(), "still held, as the feed and the notifications read it");
    tx.send(offline_done(3)).await.unwrap();
    let ended = next_told(&mut told, 2).await;
    assert_eq!(ended.len(), 2, "{ended:?}");
    assert_eq!(connection_of(&ended[0]), said("online", None, None), "it ends with WhatsApp's word");
    let summary = (ended[1].method.as_str(), ended[1].params["text"].as_str());
    assert_eq!(summary, ("notice", Some("While you were away: 3 messages in 2 chats")), "and its summary after");
    assert!(!live.lock().unwrap().catching_up());
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn pings_that_get_through_again_during_a_catch_up_put_back_the_messages_waiting_as_its_total_not_all_that_waits()
{
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (_live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    let preview = pb::OfflinePreview { total: 25, messages: 10, receipts: 15, ..Default::default() };
    let preview = pb::Event { seq: 0, kind: Some(pb::event::Kind::OfflinePreview(preview)) };
    tx.send(crate::engine::Incoming { event: preview, ack: None }).await.unwrap();
    tx.send(arriving(ANA, "3EB0AWAY1")).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    let pushed: Vec<_> = next_told(&mut told, 4).await.iter().map(connection_of).collect();
    let expected = [
        said("online", None, None),
        said("catching_up", Some(0), Some(10)),
        said("connecting", None, None),
        said("catching_up", Some(1), Some(10)),
    ];
    assert_eq!(pushed, expected, "the total the catch-up said before the timeout");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_keepalive_timeout_during_a_catch_up_holds_it_until_whatsapp_says_it_is_done_and_its_summary_comes_after() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    for (chat, id) in [(ANA, "3EB0AWAY1"), (GROUP, "3EB0AWAY2"), (ANA, "3EB0AWAY3")] {
        tx.send(arriving(chat, id)).await.unwrap();
    }
    let states = next_connections(&mut told, 3).await;
    assert_eq!(states, [ConnectionState::Online, ConnectionState::CatchingUp, ConnectionState::Connecting]);
    until("the messages to be counted", || live.lock().unwrap().catch_up.as_ref().is_some_and(|c| c.stored == 3)).await;
    {
        let live = live.lock().unwrap();
        assert!(live.catching_up(), "the timeout doesn't end the catch-up");
        assert_eq!(live.catch_up.as_ref().unwrap().seconds, None, "so the chat list and notifications wait for it");
    }
    assert!(crate::feed::catching_up(live.clone())(), "as the feed and the notifications read it");
    tx.send(offline_done(3)).await.unwrap();
    let ended = next_told(&mut told, 2).await;
    assert_eq!(ended.len(), 2, "{ended:?}");
    assert_eq!(connection_of(&ended[0]), said("online", None, None), "it ends with WhatsApp's word");
    let summary = (ended[1].method.as_str(), ended[1].params["text"].as_str());
    assert_eq!(summary, ("notice", Some("While you were away: 3 messages in 2 chats")), "and its summary after");
    assert!(!live.lock().unwrap().catching_up());
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_after_a_keepalive_timeout_during_a_catch_up_is_a_new_one_so_the_catch_up_ends_and_it_is_online() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    // whatsmeow reconnects after three minutes of failed pings, saying nothing of the old connection.
    tx.send(connection_event(State::Connected)).await.unwrap();
    let states = next_connections(&mut told, 4).await;
    let expected =
        [ConnectionState::Online, ConnectionState::CatchingUp, ConnectionState::Connecting, ConnectionState::Online];
    assert_eq!(states, expected, "the new connection isn't taken for part of the old one's catch-up");
    {
        let live = live.lock().unwrap();
        assert!(!live.catching_up(), "the catch-up the old connection cut short ended");
        assert!(live.catch_up.as_ref().unwrap().seconds.is_some(), "so the chat list and notifications go on");
    }
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connections_own_connected_after_its_keepalive_timeout_puts_its_catch_up_back_and_the_summary_comes_at_its_end()
 {
    use pb::connection::State;
    // Before the connection: the engine starting, or a connection that said Connected and dropped.
    for before in [vec![State::Connecting], vec![State::Connected, State::Disconnected]] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (tx, incoming) = mpsc::channel(16);
        let (_live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
        for state in &before {
            tx.send(connection_event(*state)).await.unwrap();
        }
        assert_eq!(next_connections(&mut told, before.len()).await.len(), before.len());
        // One connection, as whatsmeow can order it: the offline preview, the first keepalive timeout
        // (its keepalive runs from the handshake on), and only then Connected, which whatsmeow says
        // once the IQs after the handshake are answered.
        tx.send(crate::engine::Incoming { event: offline_preview(10), ack: None }).await.unwrap();
        tx.send(arriving(ANA, "3EB0LATE1")).await.unwrap();
        tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
        tx.send(arriving(GROUP, "3EB0LATE2")).await.unwrap();
        tx.send(connection_event(State::Connected)).await.unwrap();
        tx.send(arriving(ANA, "3EB0LATE3")).await.unwrap();
        tx.send(offline_done(3)).await.unwrap();
        let pushed = next_told(&mut told, 5).await;
        let connections: Vec<_> = pushed.iter().take(4).map(connection_of).collect();
        let expected = [
            said("catching_up", Some(0), Some(10)),
            said("connecting", None, None),
            said("catching_up", Some(2), Some(10)),
            said("online", None, None),
        ];
        assert_eq!(connections, expected, "after {before:?}, the catch-up goes on through the connection's Connected");
        let summary = pushed.get(4).map(|p| (p.method.as_str(), p.params["text"].as_str()));
        let whole = Some(("notice", Some("While you were away: 3 messages in 2 chats")));
        assert_eq!(summary, whole, "after {before:?}, and its summary comes at its end");
        drop(tx);
        running.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pings_that_get_through_again_are_taken_only_while_their_timeout_has_jawd_connecting() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (tx, incoming) = mpsc::channel(16);
    let (live, mut told, running) = syncing_from(&store, home.path(), None, incoming, starting(&store, true));
    // A restore heard after the connection dropped says nothing: the next push is the engine's next
    // word.
    for state in [State::Connected, State::KeepaliveTimeout, State::Disconnected, State::KeepaliveRestored] {
        tx.send(connection_event(state)).await.unwrap();
    }
    tx.send(connection_event(State::ConnectFailed)).await.unwrap();
    let states = next_connections(&mut told, 4).await;
    let expected =
        [ConnectionState::Online, ConnectionState::Connecting, ConnectionState::Offline, ConnectionState::Offline];
    assert_eq!(states, expected, "the restore after the connection dropped says nothing");
    // Nor one heard after jawd's own logout, which sync hears of outside its loop.
    tx.send(connection_event(State::Connected)).await.unwrap();
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    assert_eq!(next_connections(&mut told, 2).await, [ConnectionState::Online, ConnectionState::Connecting]);
    logged_out(&store, &live, "logged out").await.unwrap();
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    tx.send(connection_event(State::Unlinked)).await.unwrap();
    let after = next_connection_within(&mut told, WAIT).await;
    assert_eq!(
        after,
        Some(ConnectionState::Offline),
        "the restore after the logout says nothing, nor links the account"
    );
    drop(tx);
    running.await.unwrap();
}

/// The phone's message saying older history is ready, as it sends one to its own account.
fn phone_answers() -> crate::engine::Incoming {
    let notification = e2e::HistorySyncNotification { direct_path: Some("on-demand".into()), ..Default::default() };
    let protocol = e2e::ProtocolMessage { history_sync_notification: Some(notification), ..Default::default() };
    let raw = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    let me = "351910000001@s.whatsapp.net";
    let info = pb::MessageInfo {
        chat: me.into(),
        sender: me.into(),
        from_me: true,
        id: "3EB0PHONE".into(),
        ..Default::default()
    };
    let message = pb::Message { info: Some(info), raw: raw.encode_to_vec(), ..Default::default() };
    crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(message)) }, ack: None }
}

/// Sync with no engine, taking the events the test sends, already knowing the account's JIDs, and
/// giving the phone `patience` and giving a question up after `give_up`.
fn phoning(
    store: &Store,
    home: &Path,
    patience: Duration,
    give_up: Duration,
) -> (Shared, mpsc::Sender<crate::engine::Incoming>, broadcast::Receiver<Push>, tokio::task::JoinHandle<()>) {
    let mut live = starting(store, true);
    live.me.send_replace(me());
    live.phone_patience = patience;
    live.phone_give_up = give_up;
    let (tx, incoming) = mpsc::channel(16);
    let (live, told, running) = syncing_from(store, home, None, incoming, live);
    (live, tx, told, running)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_the_phone_leaves_unanswered_has_jawd_say_it_is_offline_until_it_answers() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_millis(300);
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, PHONE_GIVE_UP);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    let asked = Instant::now();
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    assert!(asked.elapsed() >= patience, "only once it has had its patience: {:?}", asked.elapsed());
    tx.send(phone_answers()).await.unwrap();
    let answered = next_connection_within(&mut told, WAIT).await;
    assert_eq!(answered, Some(ConnectionState::Online), "the phone's history says it's back");
    let asked = Instant::now();
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    assert!(asked.elapsed() >= patience, "a new question has its patience: {:?}", asked.elapsed());
    drop(tx);
    running.await.unwrap();
}

/// A receipt `sender` sent of Ana's message, of WhatsApp's `kind`.
fn receipt_from(sender: &str, kind: &str) -> crate::engine::Incoming {
    let receipt = pb::Receipt {
        chat: ANA.into(),
        sender: sender.into(),
        from_me: true,
        ids: vec!["3EB0ANA".into()],
        timestamp_ms: AT,
        r#type: kind.into(),
        ..Default::default()
    };
    crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Receipt(receipt)) }, ack: None }
}

#[tokio::test(flavor = "multi_thread")]
async fn anything_the_phone_itself_sends_says_it_is_online_and_another_of_your_devices_doesnt() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_millis(300);
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, PHONE_GIVE_UP);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    until("the account's LID", || live.lock().unwrap().me.borrow().jids.contains(&"100000000001@lid".to_string()))
        .await;
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    tx.send(receipt_from("351910000001:5@s.whatsapp.net", "read-self")).await.unwrap();
    let elsewhere = next_connection_within(&mut told, patience).await;
    assert_eq!(elsewhere, None, "reading on WhatsApp Web, another of your devices, says nothing of the phone");
    tx.send(receipt_from("351910000001@s.whatsapp.net", "read-self")).await.unwrap();
    assert_eq!(
        next_connection_within(&mut told, WAIT).await,
        Some(ConnectionState::Online),
        "reading on the phone does"
    );
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    let mut sent = live_message(ANA, "", "3EB0FROMPHONE", AT, text("sent from the phone"));
    sent.info.as_mut().unwrap().sender = "100000000001@lid".into();
    let sent =
        crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(sent)) }, ack: None };
    tx.send(sent).await.unwrap();
    let answered = next_connection_within(&mut told, WAIT).await;
    assert_eq!(answered, Some(ConnectionState::Online), "and so does a message sent from it, by its LID too");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phones_patience_counts_only_online_so_a_reconnect_with_its_answer_on_the_way_says_nothing_of_it() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_secs(1);
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, PHONE_GIVE_UP);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    asked_phone(&live);
    tx.send(connection_event(State::Disconnected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Offline));
    tokio::time::sleep(patience * 2).await;
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    let early = next_connection_within(&mut told, patience / 4).await;
    assert_eq!(early, None, "the time offline didn't count, so it isn't offline at once");
    // The phone's answer, which waited for the connection.
    tx.send(phone_answers()).await.unwrap();
    assert_eq!(next_connection_within(&mut told, patience * 2).await, None, "and nothing flaps once it comes");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phones_patience_doesnt_count_while_a_keepalive_timeout_has_jawd_connecting_and_starts_again_whole_once_the_pings_get_through()
 {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_secs(1);
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, PHONE_GIVE_UP);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    asked_phone(&live);
    tokio::time::sleep(patience / 2).await;
    tx.send(connection_event(State::KeepaliveTimeout)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Connecting));
    assert_eq!(next_connection_within(&mut told, patience * 2).await, None, "nothing of the phone while connecting");
    let restored = Instant::now();
    tx.send(connection_event(State::KeepaliveRestored)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    assert_eq!(next_connection_within(&mut told, patience / 2).await, None, "not offline at once");
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    let waited = restored.elapsed();
    assert!(waited >= patience, "a whole patience again after the restore: {waited:?}");
    drop(tx);
    running.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_offline_doesnt_outlast_a_reconnect_or_an_engine_restart_and_comes_back_only_after_a_whole_patience() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let engine = Scripted::default();
    let (supervisor, incoming) = supervised_pausing(&engine, Duration::from_millis(200));
    let patience = Duration::from_secs(1);
    let mut live = starting(&store, true);
    live.phone_patience = patience;
    let (live, mut told, running) = syncing_from(&store, home.path(), Some(supervisor), incoming, live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    engine.send(connection_event(State::Disconnected).event).await;
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Offline));
    let back = Instant::now();
    engine.send(connection_event(State::Connected).event).await;
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online), "back, it's online");
    assert_eq!(next_connection_within(&mut told, patience / 4).await, None, "and not offline again at once");
    assert_eq!(
        next_connection_within(&mut told, WAIT).await,
        Some(ConnectionState::PhoneOffline),
        "the question waits"
    );
    assert!(back.elapsed() >= patience, "but has its whole patience again: {:?}", back.elapsed());
    engine.stop.notify_one();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::EngineRestarting));
    let restarted = Instant::now();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online), "the next engine's word");
    assert_eq!(next_connection_within(&mut told, patience / 4).await, None, "not offline again at once");
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    assert!(restarted.elapsed() >= patience, "only after its whole patience again: {:?}", restarted.elapsed());
    running.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_is_given_up_after_a_while_so_phone_offline_doesnt_stick() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (patience, give_up) = (Duration::from_millis(200), Duration::from_secs(1));
    let (live, tx, mut told, running) = phoning(&store, home.path(), patience, give_up);
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    let asked = Instant::now();
    asked_phone(&live);
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::PhoneOffline));
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online), "given up");
    assert!(asked.elapsed() >= give_up, "only once its time is up: {:?}", asked.elapsed());
    assert!(live.lock().unwrap().phone.borrow().is_none(), "nothing waits for the phone");
    assert_eq!(next_connection_within(&mut told, patience * 3).await, None);
    drop(tx);
    running.await.unwrap();
}

/// An engine that takes requests for older history, noting each with whether jawd was waiting for
/// the phone's answer when it came, and fails them while `failing`.
#[derive(Clone)]
struct Asking {
    live: Shared,
    asked: Arc<Mutex<Vec<(pb::HistoryRequest, bool)>>>,
    failing: Arc<std::sync::atomic::AtomicBool>,
}

impl crate::engine::Engine for Asking {
    fn kind(&self) -> &'static str {
        "asking"
    }

    fn start(
        &self,
        _events: mpsc::Sender<crate::engine::Incoming>,
    ) -> crate::engine::BoxFuture<'_, anyhow::Result<crate::engine::Instance>> {
        let engine = self.clone();
        Box::pin(async move {
            let (commands, mut outgoing) = crate::engine::Commands::for_tests();
            tokio::spawn(async move {
                while let Some(out) = outgoing.recv().await {
                    let crate::engine::Outgoing::Command { kind: pb::command::Kind::HistoryRequest(request), reply } =
                        out
                    else {
                        continue;
                    };
                    let waiting = engine.live.lock().unwrap().phone.borrow().is_some();
                    engine.asked.lock().unwrap().push((request, waiting));
                    let answer = if engine.failing.load(std::sync::atomic::Ordering::SeqCst) {
                        Err(EngineError::Failed { message: "websocket not connected".into(), retry: true })
                    } else {
                        Ok(pb::reply::Kind::Done(pb::Done {}))
                    };
                    let _ = reply.send(answer);
                }
            });
            let hello =
                pb::Hello { protocol: crate::engine::PROTOCOL, program: "asking".into(), whatsmeow: String::new() };
            Ok(crate::engine::Instance { hello, commands, exited: Box::pin(std::future::pending()) })
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn older_history_is_asked_for_before_the_oldest_message_whatsapp_has_and_waited_for_only_once_the_phone_keeps_more()
 {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let message = |id: &str, at: i64, mine: bool, status: Option<&'static str>| {
        Change::Message(crate::store::Message {
            chat: ANA.into(),
            id: id.into(),
            at,
            sender: (!mine).then(|| ANA.into()),
            mine,
            kind: "text",
            status,
            text: Some(id.into()),
            summary: id.into(),
            ..crate::store::Message::default()
        })
    };
    store
        .apply(
            Lane::Live,
            vec![
                Change::Chat(crate::store::Chat { id: ANA.into(), ..crate::store::Chat::default() }),
                message("3EB0UNSENT", AT, true, Some("pending")),
                message("3EB0REFUSED", AT + 1_000, true, Some("failed")),
                message("3EB0ANA", AT + 2_000, false, None),
                message("3EB0SENT", AT + 3_000, true, Some("sent")),
            ],
        )
        .await
        .unwrap();
    let live: Shared = Arc::new(Mutex::new(starting(&store, true)));
    let engine = Asking { live: live.clone(), asked: Arc::default(), failing: Arc::default() };
    let (events, _incoming) = mpsc::channel(8);
    let supervisor = Supervisor::spawn(Arc::new(engine.clone()), events, crate::engine::supervisor::Backoff::default());
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    let asked = || engine.asked.lock().unwrap().clone();
    assert!(matches!(history_more(&store, &supervisor, &live, ANA.into()).await, Ok(())));
    let (request, _) = asked()[0].clone();
    assert_eq!(
        (request.oldest_id.as_str(), request.oldest_from_me, request.oldest_ms),
        ("3EB0ANA", false, AT + 2_000),
        "before Ana's, the oldest the phone has: what you sent that's still to go, or failed, it hasn't got"
    );
    assert!(live.lock().unwrap().phone.borrow().is_none(), "nothing waits: the phone hasn't said it keeps more");
    live.lock().unwrap().phone_keeps(vec![(ANA.into(), true)]);
    assert!(matches!(history_more(&store, &supervisor, &live, ANA.into()).await, Ok(())));
    assert!(asked()[1].1, "once it has, the question is noted before it goes");
    assert!(live.lock().unwrap().phone.borrow().is_some(), "and its answer waited for");
    live.lock().unwrap().forget_question();
    engine.failing.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(history_more(&store, &supervisor, &live, ANA.into()).await.is_err());
    assert!(asked()[2].1, "noted before it went");
    assert!(live.lock().unwrap().phone.borrow().is_none(), "and dropped once it couldn't");
}

/// `blob(id)`, its conversation saying `end` of what the phone keeps of Ana's chat.
fn blob_ending(
    id: &str,
    end: wa_proto::wa_web_protobufs_history_sync::conversation::EndOfHistoryTransferType,
) -> Vec<u8> {
    let mut history = wa_proto::wa_web_protobufs_history_sync::HistorySync::decode(blob(id).as_slice()).unwrap();
    history.conversations[0].end_of_history_transfer = Some(true);
    history.conversations[0].end_of_history_transfer_type = Some(end as i32);
    history.encode_to_vec()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_history_blob_says_which_chats_the_phone_keeps_more_of() {
    use wa_proto::wa_web_protobufs_history_sync::conversation::EndOfHistoryTransferType as End;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    notifications(&store, 2).await;
    let engine = Scripted::default();
    engine.blobs.lock().unwrap().extend([
        ("more".to_string(), Blob::Here(blob_ending("h1", End::CompleteOnDemandSyncButMoreMsgRemainOnPrimary))),
        ("none".to_string(), Blob::Here(blob_ending("h2", End::CompleteAndNoMoreMessageRemainOnPrimary))),
    ]);
    let (supervisor, _incoming) = supervised(&engine);
    until("the engine to run", || supervisor.status().state.name() == "running").await;
    let live = knowing();
    let (offered, offers) = mpsc::unbounded_channel();
    let fetching = tokio::spawn(fetch::run(
        offers,
        store.clone(),
        supervisor,
        live.clone(),
        home.path().join("spool"),
        Merges::default(),
    ));
    let keeps = || live.lock().unwrap().more_on_phone.contains(ANA);
    offered.send(offer(1, "more")).unwrap();
    until("the phone to keep more of Ana's chat", keeps).await;
    offered.send(offer(2, "none")).unwrap();
    until("and then none of it", || !keeps()).await;
    fetching.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_put_to_the_phone_before_a_logout_is_forgotten_with_the_device_it_came_from() {
    use pb::connection::State;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let patience = Duration::from_millis(300);
    let mut live = starting(&store, true);
    live.phone_patience = patience;
    let live: Shared = Arc::new(Mutex::new(live));
    let (tx, incoming) = mpsc::channel(16);
    let (pushes, mut told) = broadcast::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        live.clone(),
        pushes,
        None,
        home.path().to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    ));
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online));
    asked_phone(&live);
    tx.send(connection_event(State::LoggedOut)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::LoggedOut));
    tx.send(connection_event(State::Connected)).await.unwrap();
    assert_eq!(next_connection_within(&mut told, WAIT).await, Some(ConnectionState::Online), "linked again");
    assert_eq!(next_connection_within(&mut told, patience * 3).await, None, "nothing waits for the phone");
    drop(tx);
    running.await.unwrap();
}

/// A message from someone, as sync takes it from the engine, ends their typing in its chat, by
/// whichever of their JIDs it comes; one of yours ends nobody's, and nor does theirs elsewhere.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_from_someone_ends_their_typing_in_its_chat_as_sync_takes_it_from_the_engine() {
    const ANA_LID: &str = "100000000002@lid";
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let ana =
        crate::store::Person { jids: vec![ANA.into(), ANA_LID.into()], name: Some("Ana".into()), ..Default::default() };
    let changes = vec![
        Change::Person(ana),
        Change::Chat(crate::store::Chat { id: ANA.into(), ..Default::default() }),
        Change::Chat(crate::store::Chat { id: GROUP.into(), kind: Some("group"), ..Default::default() }),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    let timing = crate::feed::typing::Timing { fade: Duration::from_secs(300), pace: Duration::from_millis(20) };
    let typing = crate::feed::Typing::new(timing);
    let hearing = watch::channel(true).0;
    typing.start(
        store.clone(),
        watch::channel(convert::Me::new(["351910000001@s.whatsapp.net".into()])).1,
        hearing.subscribe(),
    );
    let seen = typing.watching();
    let typists = |chat: &str| seen.now(chat).map_or(0, |who| who.len());
    let (events, incoming) = mpsc::channel(64);
    let running = tokio::spawn(run(
        incoming,
        store.clone(),
        Arc::default(),
        broadcast::channel(16).0,
        None,
        home.path().to_path_buf(),
        Default::default(),
        typing.clone(),
    ));
    typing.heard(ANA, ANA, "", pb::chat_presence::State::Composing);
    typing.heard(GROUP, ANA, "", pb::chat_presence::State::Composing);
    typed(&seen, &[(ANA, 1), (GROUP, 1)], "her typing").await;

    // One of yours in her chat ends nothing; hers in the group, by her LID, ends hers there only.
    let send = |event: pb::Event| crate::engine::Incoming { event, ack: None };
    let mut mine = crate::engine::demo::text(ANA, "351910000001@s.whatsapp.net", "m1", AT, "olá");
    if let Some(pb::event::Kind::Message(message)) = &mut mine.kind {
        message.info.as_mut().unwrap().from_me = true;
    }
    events.send(send(mine)).await.unwrap();
    events.send(send(crate::engine::demo::text(GROUP, ANA_LID, "m2", AT + 1, "a caminho"))).await.unwrap();
    typed(&seen, &[(GROUP, 0)], "her group typing's end").await;
    assert_eq!(typists(ANA), 1, "yours ended nobody's, and hers in the group not hers here");
    events.send(send(crate::engine::demo::text(ANA, ANA, "m3", AT + 2, "cheguei"))).await.unwrap();
    typed(&seen, &[(ANA, 0)], "her typing's end in her chat").await;
    running.abort();
}

/// Typing as the engine hands it to sync's loop, with the other JID WhatsApp gave the sender by.
fn chat_state(chat: &str, sender: &str, alt: &str, state: pb::chat_presence::State) -> crate::engine::Incoming {
    let typing = pb::Typing { chat: chat.into(), sender: sender.into(), state: state as i32, sender_alt: alt.into() };
    crate::engine::Incoming { event: pb::Event { seq: 0, kind: Some(pb::event::Kind::Typing(typing)) }, ack: None }
}

/// A started `Typing` that WhatsApp is heard by, with sync's loop taking the engine's events, on a
/// jaw.db that knows Ana by her number in her chat and in a group, but not by her LID.
async fn typing_through_sync(
    home: &Path,
) -> (crate::feed::Typing, crate::feed::typing::Watching, watch::Sender<bool>, mpsc::Sender<crate::engine::Incoming>) {
    let store = Store::open(home).unwrap();
    let ana = crate::store::Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Default::default() };
    let changes = vec![
        Change::Person(ana),
        Change::Chat(crate::store::Chat { id: ANA.into(), ..Default::default() }),
        Change::Chat(crate::store::Chat { id: GROUP.into(), kind: Some("group"), ..Default::default() }),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    let timing = crate::feed::typing::Timing { fade: Duration::from_secs(300), pace: Duration::from_millis(20) };
    let typing = crate::feed::Typing::new(timing);
    let hearing = watch::channel(true).0;
    typing.start(
        store.clone(),
        watch::channel(convert::Me::new(["351910000001@s.whatsapp.net".into()])).1,
        hearing.subscribe(),
    );
    let seen = typing.watching();
    let (events, incoming) = mpsc::channel(64);
    let running = run(
        incoming,
        store,
        Arc::default(),
        broadcast::channel(16).0,
        None,
        home.into(),
        Default::default(),
        typing.clone(),
    );
    tokio::spawn(running);
    (typing, seen, hearing, events)
}

/// A message by a LID jaw doesn't know someone by, with the number they typed by beside it, ends
/// their typing in its chat: sync hands WhatsApp's other JID on with the message (round one's T1).
#[tokio::test(flavor = "multi_thread")]
async fn a_message_by_a_lid_jaw_doesnt_know_with_the_number_beside_it_ends_their_typing_through_sync() {
    let home = tempfile::tempdir().unwrap();
    let (_typing, seen, _hearing, events) = typing_through_sync(home.path()).await;
    events.send(chat_state(GROUP, ANA, "", pb::chat_presence::State::Composing)).await.unwrap();
    typed(&seen, &[(GROUP, 1)], "her typing").await;
    let mut by_lid = crate::engine::demo::text(GROUP, "100000000009@lid", "m1", AT, "olá");
    if let Some(pb::event::Kind::Message(message)) = &mut by_lid.kind {
        message.info.as_mut().unwrap().sender_alt = ANA.into();
    }
    events.send(crate::engine::Incoming { event: by_lid, ack: None }).await.unwrap();
    typed(&seen, &[(GROUP, 0)], "her typing's end by a LID jaw doesn't know her by").await;
}

/// Typing comes through sync's loop as the engine says it, by a JID jaw doesn't know with the one
/// it knows beside it, as whatsmeow gives a chat state's sender: in a group, and in her own chat
/// named by that LID; a pause by either ends it, and so does her message by the number in the
/// group, in order with the typing before it.
#[tokio::test(flavor = "multi_thread")]
async fn typing_by_a_lid_jaw_doesnt_know_is_hers_by_the_number_beside_it_through_syncs_loop() {
    const LID: &str = "100000000009@lid";
    let home = tempfile::tempdir().unwrap();
    let (typing, seen, _hearing, events) = typing_through_sync(home.path()).await;
    events.send(chat_state(GROUP, "100000000009:4@lid", ANA, pb::chat_presence::State::Composing)).await.unwrap();
    events.send(chat_state(LID, LID, ANA, pb::chat_presence::State::Recording)).await.unwrap();
    typed(&seen, &[(GROUP, 1), (ANA, 1)], "her typing by a LID, as hers").await;
    let ana = seen.now(ANA).unwrap();
    assert_eq!((ana[0].name.as_str(), ana[0].state), ("Ana", jaw_proto::windows::TypistState::Recording));
    events.send(chat_state(LID, LID, ANA, pb::chat_presence::State::Paused)).await.unwrap();
    typed(&seen, &[(ANA, 0)], "her pause in her chat by the LID").await;
    events
        .send(crate::engine::Incoming { event: crate::engine::demo::text(GROUP, ANA, "m1", AT, "olá"), ack: None })
        .await
        .unwrap();
    typed(&seen, &[(GROUP, 0)], "her typing's end by her message in the group").await;
    assert_eq!(typing.stats().heard, 3);
}

/// Waits until connections are told `chats` each have so many typing, saying `what` if they never are.
async fn typed(seen: &crate::feed::typing::Watching, chats: &[(&str, usize)], what: &str) {
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while chats.iter().any(|(chat, n)| seen.now(chat).map_or(0, |who| who.len()) != *n) {
        assert!(std::time::Instant::now() < end, "{what} never came");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn what_waits_for_an_event_past_what_its_own_transaction_takes_is_taken_in_the_background_once_the_writer_says() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    tokio::spawn(backfill::run(store.clone()));
    // With nothing to do, the background waits to be told.
    tokio::time::sleep(Duration::from_millis(300)).await;
    // A hundred answers history brought opened wait for Ana's event e1, which jaw hasn't seen yet.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute(
        "WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 100)
        INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
        SELECT ?1, 'e1', 'answer', ?2 + i, x'', printf('3519300%05d@s.whatsapp.net', i), 0,
            json_object('do', 'answer', 'answer', 'going', 'guests', 0, 'id', 'h' || i)
        FROM k",
        rusqlite::params![GROUP, AT],
    )
    .unwrap();
    drop(db);
    let event = crate::store::Message {
        chat: GROUP.into(),
        id: "e1".into(),
        at: AT,
        sender: Some(ANA.into()),
        kind: "event",
        text: Some("Sintra".into()),
        summary: "Event: Sintra".into(),
        live: true,
        ..Default::default()
    };
    store.apply(Lane::Live, vec![Change::Message(event)]).await.unwrap();
    let began = Instant::now();
    while store.count("event_answers").unwrap() < 100 {
        let answered = store.count("event_answers").unwrap();
        assert!(began.elapsed() < Duration::from_secs(30), "the background never took the rest: {answered} answered");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(store.count("openings").unwrap(), 0, "and nothing is left to take");
}

/// Each end of a connection is counted under the same lock as the state that follows it, so
/// presence reads the two together: Connecting, Disconnected, StreamReplaced, Connected after a
/// keepalive timeout, a logout and an engine's stop each end one; a keepalive timeout and its
/// restore end none, nor does a restore heard late; and the count never moves while the state still
/// says WhatsApp can be reached, but at the Connected that begins a new connection.
#[test]
fn audit_each_connections_end_is_counted_with_its_state_and_a_keepalive_timeout_ends_none() {
    use pb::connection::State;
    let event = |state: State| pb::Connection {
        state: state as i32,
        jid: "351910000001:12@s.whatsapp.net".into(),
        ..Default::default()
    };
    let reach = |live: &Live| (crate::presence::reachable(live.connection.state), live.ended());
    let mut live = Live::default();
    let steps: &[(State, bool, u64)] = &[
        (State::Connecting, false, 1),
        (State::Connected, true, 1),
        (State::KeepaliveTimeout, false, 1),
        (State::KeepaliveRestored, true, 1),
        (State::KeepaliveTimeout, false, 1),
        (State::KeepaliveRestored, true, 1),
        // whatsmeow ends a timeout that lasts by connecting again, with no Disconnected.
        (State::KeepaliveTimeout, false, 1),
        (State::Connected, true, 2),
        // A drop and its reconnect, with no Connecting between.
        (State::Disconnected, false, 3),
        (State::Connected, true, 3),
        // A restore heard after the word that changed it is let go.
        (State::KeepaliveRestored, true, 3),
        (State::StreamReplaced, false, 4),
        (State::Connecting, false, 5),
        (State::Connected, true, 5),
        (State::LoggedOut, false, 6),
    ];
    for (n, &(state, reachable, ended)) in steps.iter().enumerate() {
        apply(&mut live, &event(state));
        assert_eq!(reach(&live), (reachable, ended), "after step {n}, {state:?}");
    }
    let shared: Shared = Arc::new(Mutex::new(live));
    let (pushes, _) = broadcast::channel(16);
    restarting(&shared, &pushes);
    assert_eq!(reach(&shared.lock().unwrap()), (false, 7), "the engine stopping ends one too");
    assert_eq!(*shared.lock().unwrap().connections().borrow(), 7, "and presence's watch says so");
}
