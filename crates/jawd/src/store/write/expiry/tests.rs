use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};

use super::*;
use crate::store::write::{Packed, now_ms};
use crate::store::{Amend, Amendment, Applied, Change, Chat, Lane, Message, Store, index_against_text, read};
use crate::work::Priority;

const ANA: &str = "351910000002@s.whatsapp.net";
const RUI: &str = "351910000003@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;
/// A time to go after the tests run, 1 January 2100, so a message given it is written.
const LATER: i64 = 4_102_444_800_000;

fn message(chat: &str, id: &str, at: i64, words: &str, expires_at: Option<i64>) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind: "text",
        text: Some(words.into()),
        summary: words.into(),
        expires_at,
        live: true,
        ..Message::default()
    }
}

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

async fn n_of(store: &Store, id: &'static str) -> Option<i64> {
    query(store, move |db| Ok(db.query_row("SELECT n FROM messages WHERE id = ?1", [id], |r| r.get(0)).optional()?))
        .await
}

async fn ids(store: &Store, chat: &'static str) -> Vec<String> {
    query(store, move |db| read::messages(db, chat, read::Around::Newest, 100))
        .await
        .into_iter()
        .map(|row| row.id)
        .collect()
}

async fn tombstoned(store: &Store, id: &'static str) -> bool {
    query(store, move |db| Ok(db.prepare("SELECT 1 FROM tombstones WHERE id = ?1")?.exists([id])?)).await
}

/// The feed's events after `seq`.
async fn events(store: &Store, after: i64) -> Vec<(String, i64)> {
    query(store, move |db| Ok(read::events(db, after, 100)?.into_iter().map(|e| (e.kind.to_string(), e.key)).collect()))
        .await
}

/// A chat as the chat list shows it: its newest message, its count and what's unread.
async fn chat_shows(store: &Store, id: &'static str) -> (Option<String>, i64, i64) {
    query(store, move |db| {
        Ok(db.query_row(
            "SELECT m.id, c.message_count, c.unread FROM chats c LEFT JOIN messages m ON m.n = c.last_message
            WHERE c.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_goes_at_its_time_once_however_often_its_erasure_comes_and_no_copy_brings_it_back() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "stays", AT, "before it", None)),
            Change::Message(message(ANA, "goes", AT + 10, "kiwi", Some(LATER + 100))),
        ],
    )
    .await;
    let n = n_of(&store, "goes").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER + 99 }]).await.changed, 0, "before its time");
    assert_eq!(ids(&store, ANA).await, ["stays", "goes"]);
    let seq = store.committed().borrow().to_owned();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER + 100 }]).await.changed, 1);
    assert_eq!(ids(&store, ANA).await, ["stays"]);
    assert!(tombstoned(&store, "goes").await);
    assert!(events(&store, seq).await.contains(&("message".into(), n)), "windows are told it went");
    assert_eq!(chat_shows(&store, ANA).await, (Some("stays".into()), 1, 1), "the chat shows the one before it");
    let seq = store.committed().borrow().to_owned();
    let again = apply(&store, vec![Change::Expire { n, at: LATER + 200 }, Change::Expire { n, at: LATER + 100 }]).await;
    assert_eq!((again.changed, again.quarantined), (0, 0), "erased once, the erasure again changes nothing");
    assert_eq!(events(&store, seq).await, [], "nor tells windows anything");
    let late = Amend {
        chat: ANA.into(),
        target: "goes".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 300,
        what: Amendment::React { emoji: "👍".into() },
        raw: None,
    };
    apply(
        &store,
        vec![
            Change::Message(Message { live: false, ..message(ANA, "goes", AT + 10, "kiwi", Some(LATER + 100)) }),
            Change::Amend(late),
        ],
    )
    .await;
    assert_eq!(ids(&store, ANA).await, ["stays"], "history's copy doesn't bring it back");
    let pending: i64 = query(&store, |db| Ok(db.query_row("SELECT count(*) FROM pending", [], |r| r.get(0))?)).await;
    assert_eq!(pending, 0, "a reaction for it waits for nothing");
    assert_eq!(events(&store, seq).await, [], "and neither tells windows anything");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_only_ever_brings_a_messages_time_sooner_and_never_makes_a_view_once_one_ordinary() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let once = Message { view_once: true, ..message(ANA, "once", AT, "", Some(LATER + 1_000)) };
    apply(&store, vec![Change::Message(once.clone())]).await;
    let kept = |id: &'static str| {
        query(&store, move |db| {
            Ok(db.query_row("SELECT expires_at, view_once FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, bool>(1)?))
            })?)
        })
    };
    // History's copy, without the wrapper the live one came in and with no timer of its own.
    let plain = Message { view_once: false, expires_at: None, live: false, ..once.clone() };
    apply(&store, vec![Change::Message(plain)]).await;
    assert_eq!(kept("once").await, (Some(LATER + 1_000), true));
    apply(&store, vec![Change::Message(Message { expires_at: Some(LATER + 5_000), ..once.clone() })]).await;
    assert_eq!(kept("once").await, (Some(LATER + 1_000), true), "a later time doesn't put it off");
    apply(&store, vec![Change::Message(Message { expires_at: Some(LATER + 500), ..once.clone() })]).await;
    assert_eq!(kept("once").await, (Some(LATER + 500), true), "a sooner one brings it forward");
    assert_eq!(store.soonest().take(), Some(LATER + 500), "and wakes the timer");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_that_comes_after_its_time_is_never_shown_and_what_waited_for_it_goes() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let waiting = Amend {
        chat: ANA.into(),
        target: "late".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT,
        what: Amendment::React { emoji: "👍".into() },
        raw: None,
    };
    apply(&store, vec![Change::Message(message(ANA, "stays", AT, "here", None)), Change::Amend(waiting)]).await;
    let seq = store.committed().borrow().to_owned();
    let gone = now_ms() - 1;
    apply(&store, vec![Change::Message(message(ANA, "late", AT + 1, "too late", Some(gone)))]).await;
    assert_eq!(ids(&store, ANA).await, ["stays"]);
    assert!(tombstoned(&store, "late").await, "its tombstone keeps a later copy out");
    let pending: i64 = query(&store, |db| Ok(db.query_row("SELECT count(*) FROM pending", [], |r| r.get(0))?)).await;
    assert_eq!(pending, 0, "the reaction that waited for it goes");
    assert!(events(&store, seq).await.iter().all(|(kind, _)| kind != "message"), "no window hears of it");
}

/// Whether `needle` is anywhere in `haystack`.
fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// jaw.db and its WAL, every byte of each.
fn files(home: &Path) -> Vec<u8> {
    let mut bytes = std::fs::read(home.join("jaw.db")).unwrap();
    bytes.extend(std::fs::read(home.join("jaw.db-wal")).unwrap_or_default());
    bytes
}

/// Bytes nothing compresses, `n` of them, from `seed`.
fn noise(seed: u64, n: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_erased_messages_words_and_raw_protobuf_are_gone_from_jaw_db_and_its_wal() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // Its raw protobuf, as a photo's thumbnail makes one, spills past a page.
    let raw = noise(7, 20_000);
    let packed = Packed::new(&raw);
    let words = "quokkaflummox at https://wombatgalore.example/zz";
    let goes = Message {
        raw: Some(packed.clone()),
        spans: Some(
            serde_json::json!([{ "t": "text", "s": "quokkaflummox at " },
                { "t": "link", "s": "https://wombatgalore.example/zz", "url": "https://wombatgalore.example/zz" }])
            .to_string(),
        ),
        ..message(ANA, "goes", AT + 10, words, Some(LATER + 100))
    };
    apply(&store, vec![Change::Message(message(ANA, "stays", AT, "an ordinary day", None)), Change::Message(goes)])
        .await;
    store.indexed().await.unwrap();
    store.truncate().await.unwrap();
    let before = files(home.path());
    let window = &packed.0[packed.0.len() / 2..packed.0.len() / 2 + 64];
    assert!(holds(&before, b"quokkaflummox") && holds(&before, b"wombatgalore"), "the words are there to look for");
    assert!(holds(&before, window), "and so is its raw protobuf");
    let n = n_of(&store, "goes").await.unwrap();
    let erased = store.apply_durable(vec![Change::Expire { n, at: LATER + 100 }]).await.unwrap();
    assert_eq!(erased.changed, 1);
    // The links' index is contentless, so its words go once the writer has merged it whole.
    store.indexed().await.unwrap();
    let noted: Option<String> = query(&store, |db| {
        Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [LINKS_ERASED], |r| r.get(0)).optional()?)
    })
    .await;
    assert_eq!(noted, None, "the merge it waited for is done");
    store.truncate().await.unwrap();
    let after = files(home.path());
    assert!(!holds(&after, b"quokkaflummox"), "its words are left in jaw.db or its WAL");
    assert!(!holds(&after, b"wombatgalore"), "its link's words are left in jaw.db or its WAL");
    assert!(!holds(&after, window), "its raw protobuf is left in jaw.db or its WAL");
    assert!(holds(&after, b"ordinary"), "the message that stays is still there");
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    assert_eq!(index_against_text(&db), (0, 0), "the index holds what the messages say, and no more");
    db.execute("INSERT INTO links_fts (links_fts, rank) VALUES ('integrity-check', 0)", []).unwrap();
    let secure: i64 = db.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap();
    assert_eq!(secure, 0, "a connection of the test's own, which jawd's settings don't reach");
    let left: i64 = db.query_row("SELECT count(*) FROM links", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "its link went with it");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_writer_runs_with_fast_secure_delete_and_an_erasure_leaves_it_so() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(ANA, "goes", AT, "kiwi", Some(LATER + 1)))]).await;
    let n = n_of(&store, "goes").await.unwrap();
    apply(&store, vec![Change::Expire { n, at: LATER + 1 }]).await;
    // The next change to the FTS index is no secure delete: an ordinary message deleted for you.
    apply(&store, vec![Change::Message(message(RUI, "r1", AT, "mango", None))]).await;
    store.indexed().await.unwrap();
    apply(&store, vec![Change::DeleteMessage { chat: RUI.into(), id: "r1".into() }]).await;
    store.truncate().await.unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let config: Vec<(String, i64)> = db
        .prepare("SELECT k, v FROM messages_fts_config WHERE k = 'secure-delete' UNION ALL SELECT k, v FROM links_fts_config WHERE k = 'secure-delete'")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        config,
        [("secure-delete".to_string(), 0), ("secure-delete".to_string(), 0)],
        "each index's is off again"
    );
}

/// A copy of jaw.db as a kill now would leave it, every commit so far and nothing more, in a
/// folder of its own.
fn killed(home: &Path) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("VACUUM INTO ?1", [copy.path().join("jaw.db").to_str().unwrap()]).unwrap();
    copy
}

/// Each chat's ID, newest message, count, unread and mentions.
type Shows = (String, Option<String>, i64, i64, i64);

/// What erasing leaves to compare runs by: every message left, every tombstone, each chat as the
/// chat list shows it, and the links left.
async fn left_by(store: &Store) -> (Vec<String>, Vec<String>, Vec<Shows>, i64) {
    query(store, |db| {
        let strings = |sql: &str| -> Result<Vec<String>> {
            Ok(db.prepare(sql)?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
        };
        let chats = db
            .prepare(
                "SELECT c.id, m.id, c.message_count, c.unread, c.mentions FROM chats c
                LEFT JOIN messages m ON m.n = c.last_message ORDER BY c.id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let links: i64 = db.query_row("SELECT count(*) FROM links", [], |r| r.get(0))?;
        Ok((
            strings("SELECT id FROM messages ORDER BY id")?,
            strings("SELECT id FROM tombstones ORDER BY id")?,
            chats,
            links,
        ))
    })
    .await
}

/// Messages in two chats, a hundred of which are due by LATER + 1_000, some of them each chat's newest,
/// and some not due yet.
fn expiring() -> Vec<Change> {
    let mut changes = Vec::new();
    for k in 0..120i64 {
        let chat = if k % 2 == 0 { ANA } else { RUI };
        let expires = if k < 100 {
            Some(LATER + 1 + k)
        } else if k < 110 {
            Some(LATER + 1_000_000)
        } else {
            None
        };
        let words = format!("word{k} see https://site{k}.example");
        let spans = serde_json::json!([{ "t": "text", "s": format!("word{k} see ") },
            { "t": "link", "s": format!("https://site{k}.example"), "url": format!("https://site{k}.example") }]);
        changes.push(Change::Message(Message {
            spans: Some(spans.to_string()),
            mentions_me: k % 7 == 0,
            ..message(chat, &format!("m{k:03}"), AT + k, &words, expires)
        }));
    }
    changes.push(Change::Chat(Chat { id: ANA.into(), ..Chat::default() }));
    changes
}

/// The hosts links_fts holds a word of, as `siteN`.
fn link_words(home: &Path) -> Vec<String> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.execute_batch("CREATE VIRTUAL TABLE temp.words USING fts5vocab (main, links_fts, row)").unwrap();
    db.prepare("SELECT term FROM temp.words WHERE term GLOB 'site*' ORDER BY term")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// Erases what's due by `at`, a batch at a time, as the timer does, until none is; says how many
/// batches it took.
async fn erase_all(store: &Store, at: i64) -> usize {
    let mut batches = 0;
    loop {
        let batch = crate::expiry::erase_due(store, at, &[]).await.unwrap();
        assert!(batch.failed.is_empty(), "{batch:?}");
        if batch.erased == 0 {
            return batches;
        }
        batches += 1;
        assert!(batches < 10, "erasing never ended");
    }
}

/// Killed after any of the erasures' commits, and started again, erasing ends as it does never
/// killed: every due message gone, once, with its tombstone, and each chat showing what's left. A
/// kill inside a transaction leaves jaw.db as its last commit did, so these are every kill.
#[tokio::test(flavor = "multi_thread")]
async fn erasing_killed_after_any_of_its_commits_and_started_again_ends_as_one_never_killed() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, expiring()).await;
    store.indexed().await.unwrap();
    let mut kills = vec![killed(home.path())];
    loop {
        let batch = crate::expiry::erase_due(&store, LATER + 1_000, &[]).await.unwrap();
        if batch.erased == 0 {
            break;
        }
        kills.push(killed(home.path()));
    }
    let never_killed = left_by(&store).await;
    assert_eq!(never_killed.0.len(), 20, "the ten not due yet and the ten with no time: {:?}", never_killed.0);
    assert_eq!(never_killed.1.len(), 100);
    assert_eq!(never_killed.3, 20, "the links of the messages left");
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    assert_eq!(index_against_text(&db), (0, 0));
    store.indexed().await.unwrap();
    let kept: Vec<String> = (100..120).map(|k| format!("site{k}")).collect();
    assert_eq!(link_words(home.path()), kept, "the links' index holds only the words of the links left");
    assert!(kills.len() >= 4, "a kill after each of {} batches", kills.len() - 1);
    for (at, killed) in kills.iter().enumerate() {
        let store = Store::open(killed.path()).unwrap();
        erase_all(&store, LATER + 1_000).await;
        assert_eq!(left_by(&store).await, never_killed, "killed after commit {at}");
        let db = Connection::open(killed.path().join("jaw.db")).unwrap();
        assert_eq!(index_against_text(&db), (0, 0), "killed after commit {at}");
        store.indexed().await.unwrap();
        assert_eq!(link_words(killed.path()), kept, "killed after commit {at}");
    }
}

fn opened(chat: &str, id: &str) -> Change {
    Change::Amend(Amend {
        chat: chat.into(),
        target: id.into(),
        sender: None,
        mine: true,
        at: AT + 50,
        what: Amendment::Opened,
        raw: None,
    })
}

fn view_once(chat: &str, id: &str, at: i64) -> Message {
    Message {
        kind: "image",
        text: None,
        summary: "View once photo".into(),
        view_once: true,
        ..message(chat, id, at, "", None)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_message_played_on_another_device_goes_here_too_and_nothing_else_does() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let voice = Message { kind: "voice", ..message(ANA, "voice", AT + 1, "", None) };
    let yours = Message { sender: None, mine: true, ..view_once(ANA, "yours", AT + 2) };
    apply(&store, vec![Change::Message(view_once(ANA, "once", AT)), Change::Message(voice), Change::Message(yours)])
        .await;
    let seq = store.committed().borrow().to_owned();
    apply(&store, vec![opened(ANA, "once"), opened(ANA, "voice"), opened(ANA, "yours")]).await;
    assert_eq!(ids(&store, ANA).await, ["voice", "yours"], "only someone else's view-once message goes");
    assert!(tombstoned(&store, "once").await);
    // Search's indexing, in the writer's idle moments, tells the feed of what it makes findable.
    store.indexed().await.unwrap();
    let n = events(&store, seq).await.len();
    apply(&store, vec![opened(ANA, "once")]).await;
    assert_eq!(events(&store, seq).await.len(), n, "the receipt again changes nothing");
    // One played before jaw has it goes as it arrives.
    apply(&store, vec![opened(RUI, "early")]).await;
    apply(&store, vec![Change::Message(view_once(RUI, "early", AT + 3))]).await;
    assert_eq!(ids(&store, RUI).await, Vec::<String>::new(), "it went as it came");
    assert!(tombstoned(&store, "early").await);
    let pending: i64 = query(&store, |db| Ok(db.query_row("SELECT count(*) FROM pending", [], |r| r.get(0))?)).await;
    assert_eq!(pending, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn receipts_waiting_for_view_once_messages_are_bounded_and_one_set_aside_still_finds_its_message() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let most = OPENED_WAITING as usize;
    let waiting: Vec<Change> = (0..most + 5).map(|k| opened(ANA, &format!("w{k:04}"))).collect();
    for chunk in waiting.chunks(200) {
        apply(&store, chunk.to_vec()).await;
    }
    let counts = query(&store, |db| {
        Ok(db.query_row(
            "SELECT (SELECT count(*) FROM pending WHERE kind = 'opened'),
                (SELECT count(*) FROM quarantine WHERE source = 'opened')",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?)
    })
    .await;
    assert_eq!(counts, (OPENED_WAITING, 5), "the oldest past the bound are set aside");
    apply(&store, vec![Change::Message(view_once(ANA, "w0000", AT)), Change::Message(view_once(ANA, "w0999", AT + 1))])
        .await;
    assert_eq!(ids(&store, ANA).await, Vec::<String>::new(), "both go: one set aside, one waiting");
    let aside: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM quarantine WHERE source = 'opened'", [], |r| r.get(0))?)
    })
    .await;
    assert_eq!(aside, 4, "the set-aside receipt went with its message");
}

/// What opening and closing leaves to compare runs by: whether the message is there, its
/// tombstone, what's open, and the played receipts waiting to go.
async fn once_left(store: &Store) -> (bool, bool, i64, Vec<(String, String, Option<i64>)>) {
    query(store, |db| {
        let there = db.prepare("SELECT 1 FROM messages WHERE id = 'once'")?.exists([])?;
        let tombstone = db.prepare("SELECT 1 FROM tombstones WHERE id = 'once'")?.exists([])?;
        let open: i64 = db.query_row("SELECT count(*) FROM opened", [], |r| r.get(0))?;
        let played = db
            .prepare("SELECT jid, id, target FROM outbox WHERE kind = 'played'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok((there, tombstone, open, played))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_message_opens_once_and_killed_at_either_commit_ends_as_one_never_killed() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(view_once(ANA, "once", AT))]).await;
    let n = n_of(&store, "once").await.unwrap();
    let mut kills = vec![killed(home.path())];
    let open =
        || Change::Once(Once::Opened { n, until: LATER, jid: ANA.into(), id: "once".into(), receipt: vec![1, 2, 3] });
    assert_eq!(store.apply_durable(vec![open()]).await.unwrap().changed, 1);
    kills.push(killed(home.path()));
    assert_eq!(once_left(&store).await, (true, false, 1, vec![(ANA.into(), "once".into(), Some(n))]));
    let expires: Option<i64> =
        query(&store, |db| Ok(db.query_row("SELECT expires_at FROM messages WHERE id = 'once'", [], |r| r.get(0))?))
            .await;
    assert_eq!(expires, Some(LATER), "it goes at `until` if nothing closes it");
    assert_eq!(store.apply_durable(vec![open()]).await.unwrap().changed, 0, "it's opened once");
    store.apply_durable(vec![Change::Once(Once::Closed { n })]).await.unwrap();
    kills.push(killed(home.path()));
    let never_killed = once_left(&store).await;
    assert_eq!(
        never_killed,
        (false, true, 0, vec![(ANA.into(), "once".into(), None)]),
        "erased, its receipt still to go"
    );
    for (at, killed) in kills.iter().enumerate().skip(1) {
        // A start erases what the stop left open, as sending's start does.
        let store = Store::open(killed.path()).unwrap();
        store.apply_durable(vec![Change::Once(Once::Restarted)]).await.unwrap();
        assert_eq!(once_left(&store).await, never_killed, "killed after commit {at}");
        assert_eq!(store.apply_durable(vec![open()]).await.unwrap().changed, 0, "killed after commit {at}");
    }
}
