use std::path::Path;
use std::time::Duration;

use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::*;
use crate::store::contacts::CONTACTS;
use crate::store::kinds::KINDS;
use crate::store::outbox::{Step, Written};
use crate::store::write::links::{EVENTS, PLACES, VOICES};
use crate::store::write::{Packed, now_ms};
use crate::store::{
    Amend, Amendment, Applied, Change, Chat, Lane, Message, Person, Store, index_against_text, read, schema,
};
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

/// Ana's photo `id`, sent to be opened once, as the converter keeps it: what it is, no caption.
fn once_photo(id: &str) -> Message {
    Message {
        kind: "image",
        text: None,
        summary: "View once photo".into(),
        view_once: true,
        ..message(ANA, id, AT, "", None)
    }
}

/// Ana's photo `id` with `caption`, as an ordinary photo's row keeps it.
fn captioned(id: &str, caption: &str) -> Message {
    Message {
        kind: "image",
        spans: Some(serde_json::json!([{ "t": "text", "s": caption }]).to_string()),
        ..message(ANA, id, AT, caption, None)
    }
}

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// What message `id`'s row says: its text, spans and summary.
async fn said(store: &Store, id: &'static str) -> (Option<String>, Option<String>, Option<String>) {
    query(store, move |db| {
        Ok(db.query_row("SELECT text, spans, summary FROM messages WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?)
    })
    .await
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
    let reacted = Amend {
        chat: ANA.into(),
        target: "goes".into(),
        sender: Some(RUI.into()),
        mine: false,
        at: AT + 20,
        what: Amendment::React { emoji: "❤".into() },
        raw: None,
    };
    apply(&store, vec![Change::Amend(reacted)]).await;
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER + 99 }]).await.changed, 0, "before its time");
    assert_eq!(ids(&store, ANA).await, ["stays", "goes"]);
    let seq = store.committed().borrow().to_owned();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER + 100 }]).await.changed, 1);
    assert_eq!(ids(&store, ANA).await, ["stays"]);
    assert!(tombstoned(&store, "goes").await);
    let (reactions, outbox): (i64, i64) = query(&store, |db| {
        Ok(db.query_row("SELECT (SELECT count(*) FROM reactions), (SELECT count(*) FROM outbox)", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?)
    })
    .await;
    assert_eq!((reactions, outbox), (0, 0), "its reaction goes with it, and no device is told");
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
    let once = Message { expires_at: Some(LATER + 1_000), ..once_photo("once") };
    apply(&store, vec![Change::Message(once.clone())]).await;
    let kept = |id: &'static str| {
        query(&store, move |db| {
            Ok(db.query_row("SELECT expires_at, view_once FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, bool>(1)?))
            })?)
        })
    };
    // History's copy, without the wrapper the live one came in, so with its caption, and with no
    // timer of its own.
    let plain = Message { view_once: false, expires_at: None, live: false, ..captioned("once", "guess where") };
    apply(&store, vec![Change::Message(plain)]).await;
    assert_eq!(kept("once").await, (Some(LATER + 1_000), true));
    assert_eq!(said(&store, "once").await, (None, None, Some("View once photo".into())), "and says only what it is");
    apply(&store, vec![Change::Message(Message { expires_at: Some(LATER + 5_000), ..once.clone() })]).await;
    assert_eq!(kept("once").await, (Some(LATER + 1_000), true), "a later time doesn't put it off");
    apply(&store, vec![Change::Message(Message { expires_at: Some(LATER + 500), ..once.clone() })]).await;
    assert_eq!(kept("once").await, (Some(LATER + 500), true), "a sooner one brings it forward");
    assert_eq!(store.soonest().take(), Some(LATER + 500), "and wakes the timer");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_edit_gives_a_view_once_photo_its_caption_whether_it_comes_before_the_photo_or_after() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let edit = |target: &str| Amend {
        chat: ANA.into(),
        target: target.into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 5,
        what: Amendment::Edit {
            text: Some("a caption after all".into()),
            spans: None,
            summary: "a caption after all".into(),
        },
        raw: None,
    };
    apply(&store, vec![Change::Message(once_photo("once"))]).await;
    let seq = store.committed().borrow().to_owned();
    assert_eq!(apply(&store, vec![Change::Amend(edit("once"))]).await.changed, 0, "the phones can't edit one");
    assert_eq!(said(&store, "once").await, (None, None, Some("View once photo".into())));
    assert_eq!(events(&store, seq).await, [], "nor are windows told of anything");
    // An edit that comes before its photo stands in for it, without the wrapper, as edits are sent;
    // the photo, to be opened once, then says only what it is, though the edit is newer.
    let shown = Message { edited_at: Some(AT + 5), ..captioned("later", "a caption after all") };
    apply(&store, vec![Change::Edited { amend: edit("later"), shown: Box::new(shown) }]).await;
    apply(&store, vec![Change::Message(once_photo("later"))]).await;
    assert_eq!(said(&store, "later").await, (None, None, Some("View once photo".into())));
    let marked: bool =
        query(&store, |db| Ok(db.query_row("SELECT view_once FROM messages WHERE id = 'later'", [], |r| r.get(0))?))
            .await;
    assert!(marked, "and is one to open once");
}

/// `message`, from Ana, as WhatsApp sends it: its raw protobuf, packed as jaw.db keeps it.
fn raw_of(id: &str, message: e2e::Message) -> Vec<u8> {
    let info = WebMessageInfo {
        key: MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some(id.into()), participant: None },
        message: Some(message),
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    };
    Packed::new(&info.encode_to_vec()).0
}

/// `inner` in a view-once wrapper.
fn wrapped(inner: e2e::Message) -> e2e::Message {
    e2e::Message {
        view_once_message_v2: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(inner)) })),
        ..Default::default()
    }
}

/// A message as an earlier jawd kept it: its ID, kind, text, spans and summary, whether it's marked
/// view-once, and its raw protobuf.
type Earlier<'a> = (&'a str, &'a str, Option<&'a str>, Option<&'a str>, &'a str, bool, Option<Vec<u8>>);

/// How many blocks of index `table`'s segments hold `word`.
fn blocks_holding(db: &Connection, table: &str, word: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}_data WHERE instr(block, CAST(?1 AS BLOB)) > 0"), [word], |r| {
        r.get(0)
    })
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_earlier_jawds_view_once_messages_are_kept_as_this_ones_and_their_captions_leave_the_index() {
    let home = tempfile::tempdir().unwrap();
    let caption = "quokkacaption at https://wallabysecret.example/zz";
    let caption_spans = serde_json::json!([{ "t": "text", "s": "quokkacaption at " },
        { "t": "link", "s": "https://wallabysecret.example/zz", "url": "https://wallabysecret.example/zz" }])
    .to_string();
    let photo = |caption: &str| e2e::Message {
        image_message: Some(Box::new(e2e::ImageMessage {
            caption: Some(caption.into()),
            mimetype: Some("image/jpeg".into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let text = |said: &str| e2e::Message { conversation: Some(said.into()), ..Default::default() };
    {
        // jaw.db as main's jawd left it, at migration 30: it marked whatever came in a view-once
        // wrapper and kept a view-once photo's caption as its text, and couldn't read the last.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..30] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 30).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
            INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
            INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 0);
            INSERT INTO sync_state (key, value) VALUES ('me', '351910000001@s.whatsapp.net');"
        ))
        .unwrap();
        let rows: [Earlier; 6] = [
            (
                "ai",
                "text",
                Some("Lisbon is a city."),
                None,
                "",
                true,
                Some(raw_of("ai", wrapped(text("Lisbon is a city.")))),
            ),
            (
                "photo",
                "image",
                Some(caption),
                Some(caption_spans.as_str()),
                "",
                true,
                Some(raw_of("photo", wrapped(photo(caption)))),
            ),
            (
                "plain",
                "image",
                Some("an ordinary photo"),
                None,
                "",
                false,
                Some(raw_of("plain", photo("an ordinary photo"))),
            ),
            ("copied", "text", Some("a copy comes for me"), None, "", true, Some(raw_of("copied", wrapped(text("x"))))),
            (
                "unread",
                "unknown",
                None,
                None,
                crate::convert::kinds::UNKNOWN,
                true,
                Some(raw_of("unread", wrapped(text("now read")))),
            ),
            // With no raw protobuf to open it from, its caption is all there is of it.
            ("rawless", "image", Some("kept"), None, "", true, None),
        ];
        for (n, (id, kind, said, spans, summary, once, raw)) in rows.into_iter().enumerate() {
            let summary = Some(summary).filter(|s| !s.is_empty());
            db.execute(
                "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, summary, view_once, raw, v)
                VALUES (?1, 1, ?2, ?3, ?1, 1, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
                rusqlite::params![n as i64 + 1, id, AT + n as i64, kind, said, spans, summary, once, raw],
            )
            .unwrap();
        }
        // And a disappearing message with a link, for an erasure to merge links_fts after.
        let goes = linked("goes", "numbatgoes", 1);
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, expires_at, raw, v)
            VALUES (7, 1, 'goes', ?1, 7, 1, 'text', ?2, ?3, ?4, ?5, 0)",
            rusqlite::params![AT + 6, goes.text, goes.spans, goes.expires_at, raw_of("goes", text("see"))],
        )
        .unwrap();
        db.execute_batch(
            "UPDATE chats SET last_message = 2, last_at = 1792000000001 WHERE n = 1;
            INSERT INTO backfills (name, after, upto) VALUES ('kinds', 0, 5);",
        )
        .unwrap();
        assert!(blocks_holding(&db, "messages_fts", "quokkacaption") > 0, "the caption's words are there to look for");
        assert!(blocks_holding(&db, "links_fts", "wallabysecret") > 0, "and its link's");
    }
    let store = Store::open(home.path()).unwrap();
    assert_eq!(store.backfill_left(VIEW_ONCE).unwrap(), 7, "status counts what's to be looked at");
    let seq = store.committed().borrow().to_owned();
    // An erasure with links has the writer merge links_fts whole, and the next whole merge waits
    // ten minutes after that one began, so the one the backfill leaves to do stays to be done.
    store.apply_durable(vec![Change::Expire { n: 7, at: LATER + 1 }]).await.unwrap();
    assert!(merged(&store).await, "the erasure's merge is done");
    let kept = |id: &'static str| {
        query(&store, move |db| {
            Ok(db.query_row("SELECT kind, view_once, text FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, Option<String>>(2)?))
            })?)
        })
    };
    let shown = |kind: &str, text: &str| (kind.to_string(), false, Some(text.to_string()));
    let backfilled = |name: &'static str| {
        let store = store.clone();
        async move {
            let mut steps = 0;
            while store.backfill_left(name).unwrap() > 0 {
                steps += 1;
                assert!(steps < 100, "{name} never ended");
                store.apply(Lane::Background, vec![Change::Backfill(name)]).await.unwrap();
            }
        }
    };
    // Before this backfill reaches them, one jaw can read now converts, and a copy of another
    // comes: neither is a photo, a video or a voice message, so neither has anything to open.
    backfilled(KINDS).await;
    assert_eq!(kept("unread").await, shown("text", "now read"), "one jaw can read now shows as it is");
    apply(
        &store,
        vec![Change::Message(Message { live: false, ..message(ANA, "copied", AT + 3, "a copy comes for me", None) })],
    )
    .await;
    assert_eq!(kept("copied").await, shown("text", "a copy comes for me"), "as one a copy came for does");
    backfilled(VIEW_ONCE).await;
    assert!(merge_noted(&store).await.is_some(), "the backfill notes the merge its caption's link leaves to do");
    assert_eq!(kept("ai").await, shown("text", "Lisbon is a city."), "an answer in the wrapper shows as it is");
    assert_eq!(kept("plain").await, shown("image", "an ordinary photo"), "an ordinary photo is as it was");
    assert_eq!(kept("rawless").await, ("image".to_string(), true, Some("kept".to_string())), "nothing is lost");
    assert_eq!(kept("photo").await, ("image".to_string(), true, None));
    assert_eq!(said(&store, "photo").await, (None, None, Some("View once photo".into())), "it says only what it is");
    let told: Vec<i64> =
        events(&store, seq).await.into_iter().filter(|(kind, _)| kind == "message").map(|(_, n)| n).collect();
    for n in [1, 2] {
        assert!(told.contains(&n), "windows are told of message {n}: {told:?}");
    }
    assert!(!told.contains(&3), "and of nothing for the ordinary photo");
    let raw: Vec<u8> =
        query(&store, |db| Ok(db.query_row("SELECT raw FROM messages WHERE id = 'photo'", [], |r| r.get(0))?)).await;
    let info = WebMessageInfo::decode(crate::store::unpack(&raw).unwrap().as_slice()).unwrap();
    let opened = crate::convert::once_caption(&info.message.unwrap());
    assert_eq!(opened[0]["s"], "quokkacaption at ", "its raw protobuf keeps its caption for the opening");
    // Killed now, jawd started again still merges links_fts whole, as the backfill noted it.
    let killed_now = killed(home.path());
    let again = Store::open(killed_now.path()).unwrap();
    assert!(merge_noted(&again).await.is_some(), "a kill keeps the merge noted");
    assert!(merged(&again).await, "and started again, jawd does it");
    again.truncate().await.unwrap();
    let db = Connection::open(killed_now.path().join("jaw.db")).unwrap();
    assert_eq!(blocks_holding(&db, "messages_fts", "quokkacaption"), 0, "the caption's words left the index");
    assert_eq!(blocks_holding(&db, "links_fts", "wallabysecret"), 0, "and its link's words theirs");
    assert_eq!(index_against_text(&db), (0, 0), "the index holds what the messages say, and no more");
    let links: i64 = db.query_row("SELECT count(*) FROM links WHERE message = 2", [], |r| r.get(0)).unwrap();
    assert_eq!(links, 0, "its link went with its caption");
    drop(db);
    // Taken again, as a backfill's step can be, it changes nothing.
    let twice = killed(killed_now.path());
    let db = Connection::open(twice.path().join("jaw.db")).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) VALUES ('once', 0, 7)", []).unwrap();
    drop(db);
    let store = Store::open(twice.path()).unwrap();
    let seq = store.committed().borrow().to_owned();
    let (mut changed, mut steps) = (0, 0);
    while store.backfill_left(VIEW_ONCE).unwrap() > 0 {
        steps += 1;
        assert!(steps < 100, "the backfill never ended");
        changed += store.apply(Lane::Background, vec![Change::Backfill(VIEW_ONCE)]).await.unwrap().changed;
    }
    assert_eq!(changed, 0, "done once, it's done");
    assert!(events(&store, seq).await.iter().all(|(kind, _)| kind != "message"), "and tells windows nothing");
}

/// jaw.db in `home` as main's jawd left it, with 64 view-once photos whose captions it kept as text.
fn main_with_captions(home: &Path) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..30] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 30).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 0);
        INSERT INTO sync_state (key, value) VALUES ('me', '351910000001@s.whatsapp.net');"
    ))
    .unwrap();
    for n in 1..=64i64 {
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, view_once, raw, v)
            VALUES (?1, 1, ?2, ?3, ?1, 1, 'image', ?4, 1, x'00', 0)",
            rusqlite::params![n, format!("once{n:02}"), AT + n, format!("a caption of {n}")],
        )
        .unwrap();
    }
}

/// A writer that counts each caption it takes out as 5 ms, past a transaction's 3 ms, as a long
/// one can cost at a million messages, though taking one out costs far less here.
fn costly_captions(home: &Path) -> Store {
    let limits = crate::store::Limits { secure_delete_least: Duration::from_millis(5), ..crate::store::LIMITS };
    Store::open_with(home, limits).unwrap()
}

/// What the `once` backfill leaves of each message: its ID, mark, text, spans and summary.
async fn once_kept(store: &Store) -> Vec<(String, bool, Option<String>, Option<String>, Option<String>)> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT id, view_once, text, spans, summary FROM messages ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_once_backfill_killed_halfway_and_started_again_ends_as_one_never_killed() {
    let (home, never) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    main_with_captions(home.path());
    main_with_captions(never.path());
    // A caption to a transaction, so the 32 steps commit 32 times and the kill comes after one.
    let store = costly_captions(home.path());
    assert_eq!(store.apply(Lane::Background, vec![Change::Backfill(VIEW_ONCE); 32]).await.unwrap().transactions, 32);
    let killed = killed(home.path());
    let again = Store::open(killed.path()).unwrap();
    assert_eq!(again.backfill_left(VIEW_ONCE).unwrap(), 32, "it goes on from where the kill left it");
    let whole = Store::open(never.path()).unwrap();
    for store in [&again, &whole] {
        while store.backfill_left(VIEW_ONCE).unwrap() > 0 {
            store.apply(Lane::Background, vec![Change::Backfill(VIEW_ONCE); 64]).await.unwrap();
        }
    }
    let kept = once_kept(&whole).await;
    assert!(kept.iter().all(|(_, once, text, _, _)| *once && text.is_none()), "{kept:?}");
    assert_eq!(once_kept(&again).await, kept);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_once_backfill_counts_a_caption_at_what_the_costliest_took_so_no_transaction_takes_two_past_its_time() {
    let home = tempfile::tempdir().unwrap();
    main_with_captions(home.path());
    let store = costly_captions(home.path());
    let applied = store.apply(Lane::Background, vec![Change::Backfill(VIEW_ONCE); 64]).await.unwrap();
    assert_eq!(applied.quarantined, 0);
    assert_eq!(applied.transactions, 64, "a step, and its transaction, stop at the caption past their time");
    let captioned: i64 =
        query(&store, |db| Ok(db.query_row("SELECT count(*) FROM messages WHERE text IS NOT NULL", [], |r| r.get(0))?))
            .await;
    assert_eq!(captioned, 0, "and each was taken out, one to a transaction");
}

/// A background transaction ends after its first secure delete, whatever its pace would allow, so
/// an inbox row or a live write waits behind one erasure at most: 32 erasures given to the writer
/// at once go in 32 transactions, though each may take a second.
#[tokio::test(flavor = "multi_thread")]
async fn each_erasure_in_the_background_is_a_transaction_of_its_own_whatever_its_pace_allows() {
    let home = tempfile::tempdir().unwrap();
    let second = Duration::from_secs(1);
    let limits = crate::store::Limits { batch_time: second, yield_after: second, ..crate::store::LIMITS };
    let store = Store::open_with(home.path(), limits).unwrap();
    let due =
        (0..32i64).map(|k| Change::Message(message(ANA, &format!("d{k:02}"), AT + k, &format!("due{k}"), Some(LATER))));
    apply(&store, due.collect()).await;
    store.indexed().await.unwrap();
    let ns: Vec<i64> = query(&store, |db| {
        Ok(db
            .prepare("SELECT n FROM messages ORDER BY n")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    let erasures = ns.into_iter().map(|n| Change::Expire { n, at: LATER }).collect();
    let applied = store.apply(Lane::Background, erasures).await.unwrap();
    assert_eq!((applied.changed, applied.transactions), (32, 32), "{applied:?}");
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

/// What waits for message `id` in pending, and what was set aside for it in quarantine.
async fn waiting_for(store: &Store, id: &'static str) -> (i64, i64) {
    query(store, move |db| {
        let (named, past) = (format!("{ANA} {id} "), format!("{ANA} {id}!"));
        Ok((
            db.query_row("SELECT count(*) FROM pending WHERE target = ?1", [id], |r| r.get(0))?,
            db.query_row(
                "SELECT count(*) FROM quarantine WHERE ref = ?1 OR (ref >= ?2 AND ref < ?3)",
                [format!("{ANA} {id}"), named, past],
                |r| r.get(0),
            )?,
        ))
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_erasure_takes_what_waits_for_its_message_and_what_was_set_aside_for_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // An edit sealed with its message's secret, which this jawd keeps for a later one.
    let kept = |target: &str| Amend {
        chat: ANA.into(),
        target: target.into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 5,
        what: Amendment::Keep { kind: "edit".into() },
        raw: None,
    };
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "goes", AT, "gone at its time", Some(LATER))),
            Change::Message(message(ANA, "stays", AT + 1, "here to stay", None)),
            Change::Amend(kept("goes")),
            Change::Amend(kept("stays")),
        ],
    )
    .await;
    {
        // Set aside past what waits: a receipt of it opened elsewhere, a vote, an answer and an edit.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        for id in ["goes", "stays"] {
            let insert = "INSERT INTO quarantine (at, source, ref, error) VALUES (?1, ?2, ?3, 'set aside')";
            db.execute(insert, rusqlite::params![AT, "opened", format!("{ANA} {id}")]).unwrap();
            for source in ["vote", "answer", "edit"] {
                db.execute(insert, rusqlite::params![AT, source, format!("{ANA} {id} {RUI} {source}")]).unwrap();
            }
        }
    }
    assert_eq!(waiting_for(&store, "goes").await, (1, 4));
    let n = n_of(&store, "goes").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    assert!(tombstoned(&store, "goes").await);
    assert_eq!(waiting_for(&store, "goes").await, (0, 0), "what waited for it, or was set aside for it, went with it");
    assert_eq!(waiting_for(&store, "stays").await, (1, 4), "what waits for another message stays");
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
    // Writes before it, as a running jawd's, whose WAL keeps the frames they and the message wrote.
    // None search indexes, whose merges would free pages holding its words under FAST before it
    // goes (as the design says), and none after, which would write over the pages it freed.
    for k in 0..100 {
        apply(&store, vec![Change::Remember { key: format!("around{k}"), value: "x".repeat(200) }]).await;
    }
    store.indexed().await.unwrap();
    let before = files(home.path());
    let window = &packed.0[packed.0.len() / 2..packed.0.len() / 2 + 64];
    assert!(holds(&before, b"quokkaflummox") && holds(&before, b"wombatgalore"), "the words are there to look for");
    assert!(holds(&before, window), "and so is its raw protobuf");
    let n = n_of(&store, "goes").await.unwrap();
    let erased = store.apply_durable(vec![Change::Expire { n, at: LATER + 100 }]).await.unwrap();
    assert_eq!(erased.changed, 1);
    // The links' indexes are contentless, so their words go once the writer has merged them whole.
    store.indexed().await.unwrap();
    assert!(merged(&store).await, "the merges it waited for are done");
    // Nothing truncates the WAL here, as at a clean stop: the writer empties it once it's idle.
    assert!(wal_emptied(&store).await, "the WAL the erasure left is emptied");
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

/// When sync_state says a message was erased, while the WAL is still to empty after it.
async fn wal_noted(store: &Store) -> Option<String> {
    query(store, |db| {
        Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [WAL_ERASED], |r| r.get(0)).optional()?)
    })
    .await
}

/// Waits up to 10 s for the writer to have emptied the WAL that erasures left, saying whether it did.
async fn wal_emptied(store: &Store) -> bool {
    for _ in 0..500 {
        if wal_noted(store).await.is_none() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wal_an_erasure_leaves_is_emptied_once_no_reader_holds_it_and_a_kill_keeps_that_to_do() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(ANA, "goes", AT, "quokkawal gone at its time", Some(LATER)))]).await;
    store.indexed().await.unwrap();
    // A reader in the middle of a read holds the WAL's frames, so it can't be emptied meanwhile.
    let reader = Connection::open(home.path().join("jaw.db")).unwrap();
    reader.execute_batch("BEGIN; SELECT count(*) FROM messages;").unwrap();
    let n = n_of(&store, "goes").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    // Long enough for the writer, a second without a commit, to have tried.
    tokio::time::sleep(crate::store::WAL_QUIET + Duration::from_millis(1_500)).await;
    assert!(wal_noted(&store).await.is_some(), "it's still to empty while the reader holds it");
    let killed = killed(home.path());
    reader.execute_batch("COMMIT").unwrap();
    assert!(wal_emptied(&store).await, "and emptied once the reader lets go");
    assert!(!holds(&files(home.path()), b"quokkawal"), "with nothing of the message left in the files");
    let left: Option<String> = Connection::open(killed.path().join("jaw.db"))
        .unwrap()
        .query_row("SELECT value FROM sync_state WHERE key = ?1", [WAL_ERASED], |r| r.get(0))
        .optional()
        .unwrap();
    assert!(left.is_some(), "a kill before then leaves it to do");
    let again = Store::open(killed.path()).unwrap();
    assert!(wal_emptied(&again).await, "and started again, jawd does it");
}

/// The WAL a backlog of erasures leaves is emptied only once the writer has gone a second without
/// a commit, so no batch of the backlog waits behind it: between erasures a few hundred
/// milliseconds apart it's still to empty, and it's emptied a second after the last.
#[tokio::test(flavor = "multi_thread")]
async fn the_wal_a_backlog_of_erasures_leaves_is_emptied_only_once_a_second_goes_without_a_commit() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let due =
        (0..8i64).map(|k| Change::Message(message(ANA, &format!("d{k}"), AT + k, &format!("due{k}"), Some(LATER))));
    apply(&store, due.collect()).await;
    store.indexed().await.unwrap();
    let ns: Vec<i64> = query(&store, |db| {
        Ok(db
            .prepare("SELECT n FROM messages ORDER BY n")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    for (k, n) in ns.into_iter().enumerate() {
        store.apply(Lane::Background, vec![Change::Expire { n, at: LATER }]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(wal_noted(&store).await.is_some(), "the WAL is still to empty after erasure {k}");
    }
    let last = std::time::Instant::now();
    assert!(wal_emptied(&store).await, "it's emptied once they're done");
    assert!(last.elapsed() >= Duration::from_millis(500), "a second after the last, not before");
}

/// Message `id` of Rui's, `raw` its raw protobuf, written and then deleted for you, as an ordinary
/// deletion is; says whether jaw.db and its WAL still hold any of `raw` after it.
async fn deleted_leaves(store: &Store, home: &Path, id: &'static str, raw: &[u8]) -> bool {
    let packed = Packed::new(raw);
    apply(store, vec![Change::Message(Message { raw: Some(packed.clone()), ..message(RUI, id, AT, "mango", None) })])
        .await;
    store.indexed().await.unwrap();
    apply(store, vec![Change::DeleteMessage { chat: RUI.into(), id: id.into() }]).await;
    store.truncate().await.unwrap();
    holds(&files(home), &packed.0[packed.0.len() / 2..packed.0.len() / 2 + 32])
}

#[tokio::test(flavor = "multi_thread")]
async fn the_writer_runs_with_fast_secure_delete_and_an_erasure_leaves_it_so() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // A raw protobuf small enough to sit in its row's page, which the deletion writes anyway.
    assert!(!deleted_leaves(&store, home.path(), "r0", &noise(3, 300)).await, "deleted, it's zeroed where it was");
    apply(&store, vec![Change::Message(message(ANA, "goes", AT, "kiwi", Some(LATER + 1)))]).await;
    let n = n_of(&store, "goes").await.unwrap();
    apply(&store, vec![Change::Expire { n, at: LATER + 1 }]).await;
    // The next change to the FTS index is no secure delete: an ordinary message deleted for you.
    assert!(!deleted_leaves(&store, home.path(), "r1", &noise(4, 300)).await, "and still is after an erasure");
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
/// kill inside a transaction leaves jaw.db as its last commit did, and a batch goes in the
/// background lane's transactions, each committing the next of its erasures in order, so a kill
/// leaves the erasures of a batch up to one of them: taken here after whole batches, and after
/// erasures made one at a time on a copy.
#[tokio::test(flavor = "multi_thread")]
async fn erasing_killed_after_any_of_its_commits_and_started_again_ends_as_one_never_killed() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, expiring()).await;
    store.indexed().await.unwrap();
    let mut kills = vec![killed(home.path())];
    let within = killed(home.path());
    {
        let store = Store::open(within.path()).unwrap();
        let due: Vec<i64> = query(&store, |db| {
            Ok(db
                .prepare(DUE)?
                .query_map(rusqlite::params![LATER + 1_000, 1_000], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
        assert_eq!(due.len(), 100);
        for (k, n) in due.into_iter().enumerate() {
            store.apply(Lane::Background, vec![Change::Expire { n, at: LATER + 1_000 }]).await.unwrap();
            if [0, 1, 2, 15, 30, 31, 32, 33, 63, 64, 98].contains(&k) {
                kills.push(killed(within.path()));
            }
        }
    }
    let mut batches = 0;
    loop {
        let batch = crate::expiry::erase_due(&store, LATER + 1_000, &[]).await.unwrap();
        if batch.erased == 0 {
            break;
        }
        batches += 1;
        kills.push(killed(home.path()));
    }
    assert_eq!(batches, 4, "the hundred due, 32 at a time");
    let never_killed = left_by(&store).await;
    assert_eq!(never_killed.0.len(), 20, "the ten not due yet and the ten with no time: {:?}", never_killed.0);
    assert_eq!(never_killed.1.len(), 100);
    assert_eq!(never_killed.3, 20, "the links of the messages left");
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    assert_eq!(index_against_text(&db), (0, 0));
    store.indexed().await.unwrap();
    let kept: Vec<String> = (100..120).map(|k| format!("site{k}")).collect();
    assert_eq!(link_words(home.path()), kept, "the links' index holds only the words of the links left");
    assert!(kills.len() >= 15, "a kill after each of {} batches and erasures", kills.len() - 1);
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

/// Message `id` of Ana's with a link to `site`, due at LATER + `k`.
fn linked(id: &str, site: &str, k: i64) -> Message {
    let spans = serde_json::json!([{ "t": "text", "s": "see " },
        { "t": "link", "s": format!("https://{site}.example"), "url": format!("https://{site}.example") }]);
    Message {
        spans: Some(spans.to_string()),
        ..message(ANA, id, AT + k, &format!("see https://{site}.example"), Some(LATER + k))
    }
}

/// When sync_state says a contentless index is to be merged whole for an erasure, if it does.
async fn merge_noted(store: &Store) -> Option<String> {
    query(store, |db| {
        Ok(db
            .query_row("SELECT value FROM sync_state WHERE key IN (?1, ?2)", [LINKS_ERASED, LIBRARY_ERASED], |r| {
                r.get(0)
            })
            .optional()?)
    })
    .await
}

/// Waits up to ten seconds for the whole merges sync_state notes to be done.
async fn merged(store: &Store) -> bool {
    for _ in 0..500 {
        if merge_noted(store).await.is_none() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// Killed after an erasure's commit, before the whole merge of links_fts it leaves to do has begun,
/// jawd started again still does it: the erasure notes it in the same commit. A merge step has
/// taken its link's words out of their segment already, which holds three, a third of them erased:
/// FTS5 rewrites a level once a tenth of its entries are deleted.
#[tokio::test(flavor = "multi_thread")]
async fn the_merge_an_erasure_leaves_to_do_is_noted_with_it_so_a_kill_before_it_begins_loses_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let stays = Message { expires_at: None, ..linked("stays", "numbatstays", 3) };
    let (first, second) = (linked("first", "quokkafirst", 1), linked("second", "wombatsecond", 2));
    apply(&store, vec![Change::Message(first), Change::Message(second), Change::Message(stays)]).await;
    store.indexed().await.unwrap();
    let first = n_of(&store, "first").await.unwrap();
    store.apply_durable(vec![Change::Expire { n: first, at: LATER + 1 }]).await.unwrap();
    assert!(merged(&store).await, "the first erasure's merge is done");
    // The next whole merge waits ten minutes after that one began, so a kill now finds it to do.
    let second = n_of(&store, "second").await.unwrap();
    store.apply_durable(vec![Change::Expire { n: second, at: LATER + 2 }]).await.unwrap();
    store.indexed().await.unwrap();
    assert!(merge_noted(&store).await.is_some(), "the erasure notes the merge it leaves to do");
    let killed = killed(home.path());
    let left = files(killed.path());
    assert!(!holds(&left, b"wombatsecond") && holds(&left, b"numbatstays"), "its link's words went with it");
    let again = Store::open(killed.path()).unwrap();
    assert!(merge_noted(&again).await.is_some(), "a kill keeps the merge noted");
    assert!(merged(&again).await, "and started again, jawd does it");
}

/// The pages of jaw.db that hold `needle`, each with the b-tree that has it, or none for a page no
/// b-tree has, a free one.
pub(crate) fn pages_holding(home: &Path, needle: &[u8]) -> Vec<(usize, Option<String>)> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
    let bytes = std::fs::read(home.join("jaw.db")).unwrap();
    let mut pages = Vec::new();
    for (k, _) in bytes.chunks(size as usize).enumerate().filter(|(_, page)| holds(page, needle)) {
        let owner =
            db.query_row("SELECT name FROM dbstat WHERE pageno = ?1", [k as i64 + 1], |r| r.get(0)).optional().unwrap();
        pages.push((k + 1, owner));
    }
    pages
}

/// How many blocks of each search index's segments hold `word`.
fn index_blocks(home: &Path, word: &str) -> Vec<(&'static str, i64)> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    ["messages_fts", "links_fts", "library_fts"]
        .into_iter()
        .map(|index| (index, blocks_holding(&db, index, word)))
        .collect()
}

/// The free pages of jaw.db that hold `needle`, once all that's committed is in it: what pages
/// freed under FAST before a change keep until they're used again, as the design says.
pub(crate) async fn free_holding(store: &Store, home: &Path, needle: &[u8]) -> Vec<usize> {
    store.truncate().await.unwrap();
    pages_holding(home, needle).into_iter().filter(|(_, owner)| owner.is_none()).map(|(page, _)| page).collect()
}

/// Asserts that `needle`, which a change took out, is nowhere in jaw.db or its WAL but in pages
/// that were free and held it before the change (`before`).
pub(crate) fn bytes_gone_but_for(home: &Path, needle: &[u8], before: &[usize], what: &str) {
    let left: Vec<_> = pages_holding(home, needle)
        .into_iter()
        .filter(|(page, owner)| owner.is_some() || !before.contains(page))
        .collect();
    assert!(left.is_empty(), "{what}: left in these pages, by their b-tree, those free before {before:?}: {left:?}");
    let wal = std::fs::read(home.join("jaw.db-wal")).unwrap_or_default();
    assert!(!holds(&wal, needle), "{what}: left in the WAL");
}

/// Waits for what taking words out leaves to do: indexing and merging, each contentless index's
/// whole merge, and emptying the WAL.
pub(crate) async fn settled(store: &Store) {
    store.indexed().await.unwrap();
    assert!(merged(store).await, "the whole merges it left to do");
    assert!(wal_emptied(store).await, "the WAL it left to empty");
}

/// Asserts that `word`, which a change took out, is nowhere in jaw.db or its WAL but in pages that
/// were free and held it before the change (`before`), and in no index's blocks.
fn gone_but_for(home: &Path, word: &str, before: &[usize], what: &str) {
    bytes_gone_but_for(home, word.as_bytes(), before, &format!("{what}: {word}"));
    let blocks: Vec<_> = index_blocks(home, word).into_iter().filter(|(_, blocks)| *blocks > 0).collect();
    assert!(blocks.is_empty(), "{what}: {word} is left in {blocks:?}");
}

/// Where `word`, which a change took out, is left, as `gone_but_for` looks: none when it's gone.
fn left_of(home: &Path, word: &str, before: &[usize]) -> Vec<String> {
    let mut left = Vec::new();
    let pages: Vec<_> = pages_holding(home, word.as_bytes())
        .into_iter()
        .filter(|(page, owner)| owner.is_some() || !before.contains(page))
        .collect();
    if !pages.is_empty() {
        left.push(format!("in these pages, by their b-tree: {pages:?}"));
    }
    if holds(&std::fs::read(home.join("jaw.db-wal")).unwrap_or_default(), word.as_bytes()) {
        left.push("in the WAL".into());
    }
    let blocks: Vec<_> = index_blocks(home, word).into_iter().filter(|(_, blocks)| *blocks > 0).collect();
    if !blocks.is_empty() {
        left.push(format!("in {blocks:?}"));
    }
    left
}

/// A photo with `caption`, as WhatsApp sends one, with its thumbnail if it has one.
fn photo_of(caption: &str, thumbnail: Option<Vec<u8>>) -> e2e::Message {
    e2e::Message {
        image_message: Some(Box::new(e2e::ImageMessage {
            caption: Some(caption.into()),
            mimetype: Some("image/jpeg".into()),
            jpeg_thumbnail: thumbnail,
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// Forty of Ana's messages that stay, in the same segments as what a test changes.
fn forty_others() -> Vec<Change> {
    (0..40i64)
        .map(|k| Change::Message(message(ANA, &format!("o{k:02}"), AT + k, &format!("other words number{k}"), None)))
        .collect()
}

/// An erased photo's caption leaves the library's index, which holds what items say apart from the
/// messages': it's contentless, so a deletion leaves the words in their segment until a merge
/// rewrites it, and the writer merges it whole after the erasure. Among sixty captions in three
/// segments, as an index holds them, so it isn't one a segment holds alone.
#[tokio::test(flavor = "multi_thread")]
async fn an_erased_photos_caption_leaves_the_librarys_index_holding_sixty() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    for batch in 0..3i64 {
        let photos = (0..20i64).map(|k| {
            let goes = (batch, k) == (1, 7);
            let caption =
                if goes { "zebrafinch on the beach".to_string() } else { format!("finch{batch}x{k} on the beach") };
            let photo = captioned(&format!("p{batch}{k:02}"), &caption);
            Change::Message(Message { at: AT + batch * 100 + k, expires_at: goes.then_some(LATER), ..photo })
        });
        apply(&store, photos.collect()).await;
        store.indexed().await.unwrap();
    }
    let library = |home: &Path| index_blocks(home, "zebrafinch")[2].1;
    assert!(library(home.path()) > 0, "the library holds its caption");
    let before = free_holding(&store, home.path(), b"zebrafinch").await;
    let n = n_of(&store, "p107").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    settled(&store).await;
    gone_but_for(home.path(), "zebrafinch", &before, "its caption");
    let found: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM library_fts WHERE library_fts MATCH 'beach'", [], |r| r.get(0))?)
    })
    .await;
    assert_eq!(found, 59, "the others are still found");
}

/// An erased message's links leave both contentless indexes, links_fts and the library's, and no
/// page their whole merges free keeps their words, as the merges zero what they free while an
/// erasure's words wait in an index. Among three hundred messages with links in six segments.
#[tokio::test(flavor = "multi_thread")]
async fn an_erased_messages_links_leave_both_contentless_indexes_and_no_page_their_merges_free() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    for batch in 0..6i64 {
        let links = (0..50i64).map(|k| {
            let goes = (batch, k) == (2, 20);
            let site = if goes { "wombatgalore".to_string() } else { format!("site{batch}x{k}") };
            let message = linked(&format!("l{batch}{k:02}"), &site, 0);
            Change::Message(Message { at: AT + batch * 100 + k, expires_at: goes.then_some(LATER), ..message })
        });
        apply(&store, links.collect()).await;
        store.indexed().await.unwrap();
    }
    let blocks = index_blocks(home.path(), "wombatgalore");
    assert!(blocks[1].1 > 0 && blocks[2].1 > 0, "both hold its link's words: {blocks:?}");
    let before = free_holding(&store, home.path(), b"wombatgalore").await;
    let n = n_of(&store, "l220").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    settled(&store).await;
    gone_but_for(home.path(), "wombatgalore", &before, "its link's words");
    let found: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM links_fts WHERE links_fts MATCH 'example'", [], |r| r.get(0))?)
    })
    .await;
    assert_eq!(found, 299, "the others are still found");
}

/// While erasures' words wait in a contentless index for its whole merge, which comes ten minutes
/// after the last one began, a merge step taking them out before it zeroes the pages it frees too:
/// FTS5 rewrites a level once a tenth of its entries are deleted, as sixty of three hundred links,
/// erased together, make it. FTS5 keeps a term on a leaf as what follows the part it shares with
/// the term before it, so each erased word begins with two letters no other term has, and what
/// follows its first letter, which FTS5 always keeps, is what's looked for: every one is checked.
/// That finds what pages freed under FAST before the erasures kept, too, among them pages used
/// again since by b-trees that never hold a message's words, which keep some of it in what they
/// don't use (the design's caveat), so those are left out as free ones are.
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_taking_erasures_words_out_before_their_whole_merge_zeroes_the_pages_it_frees() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // A first erasure, whose whole merges go at once, so the next wait ten minutes after they began.
    apply(&store, vec![Change::Message(linked("first", "quokkafirst", 1))]).await;
    store.indexed().await.unwrap();
    let first = n_of(&store, "first").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n: first, at: LATER + 1 }]).await.changed, 1);
    settled(&store).await;
    let goes = |batch: i64, k: i64| {
        let i = (batch * 10 + k / 5) as u8;
        (k % 5 == 0).then(|| format!("{}{}numbat{batch}x{k}", char::from(b'a' + i / 26), char::from(b'a' + i % 26)))
    };
    for batch in 0..6i64 {
        let links = (0..50i64).map(|k| {
            let site = goes(batch, k).unwrap_or_else(|| format!("site{batch}x{k}"));
            let message = linked(&format!("w{batch}{k:02}"), &site, 0);
            Change::Message(Message { at: AT + batch * 100 + k, expires_at: goes(batch, k).map(|_| LATER), ..message })
        });
        apply(&store, links.collect()).await;
        store.indexed().await.unwrap();
    }
    store.truncate().await.unwrap();
    let words: Vec<String> = (0..6i64)
        .flat_map(|batch| (0..50i64).filter_map(move |k| goes(batch, k).map(|word| word[1..].to_string())))
        .collect();
    let mut each = words.clone();
    each.sort();
    each.dedup();
    assert_eq!(each.len(), 60, "sixty words, each its own");
    let before: Vec<Vec<usize>> = words
        .iter()
        .map(|word| {
            let pages = pages_holding(home.path(), word.as_bytes());
            pages.into_iter().filter(|(_, owner)| !holds_words(owner.as_deref())).map(|(page, _)| page).collect()
        })
        .collect();
    let due: Vec<i64> = query(&store, |db| {
        Ok(db
            .prepare("SELECT n FROM messages WHERE expires_at IS NOT NULL")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(due.len(), 60);
    let erasures = due.into_iter().map(|n| Change::Expire { n, at: LATER }).collect();
    assert_eq!(apply(&store, erasures).await.changed, 60);
    store.indexed().await.unwrap();
    assert!(merge_noted(&store).await.is_some(), "their whole merges wait for their ten minutes");
    assert!(wal_emptied(&store).await);
    for (word, before) in words.iter().zip(&before) {
        let left: Vec<_> = pages_holding(home.path(), word.as_bytes())
            .into_iter()
            .filter(|(page, _)| !before.contains(page))
            .collect();
        assert!(
            left.is_empty(),
            "{word}: left in these pages, by their b-tree, none holding it before but its own: {left:?}"
        );
        let wal = std::fs::read(home.path().join("jaw.db-wal")).unwrap_or_default();
        assert!(!holds(&wal, word.as_bytes()), "{word}: left in the WAL");
        let blocks: Vec<_> = index_blocks(home.path(), word).into_iter().filter(|(_, blocks)| *blocks > 0).collect();
        assert!(blocks.is_empty(), "{word}: left in {blocks:?}");
    }
}

/// Whether b-tree `owner` is one that holds what a message says while it's there: its row, its
/// links' and the indexes on either, and search's indexes.
fn holds_words(owner: Option<&str>) -> bool {
    owner.is_some_and(|name| {
        ["messages", "links", "library_fts", "sqlite_autoindex_messages", "sqlite_autoindex_links"]
            .iter()
            .any(|table| name == *table || name.starts_with(&format!("{table}_")))
    })
}

/// An edit of a message with a time to go takes what it said first out of messages_fts's segments
/// as an erasure does, rather than marking it deleted there, and its erasure takes the rest, so
/// nothing it said is left. Among forty other messages, so a segment holds several.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_of_a_disappearing_message_takes_its_first_text_out_and_its_erasure_the_rest() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, forty_others()).await;
    apply(&store, vec![Change::Message(message(ANA, "goes", AT + 50, "alphaoriginal remark here", Some(LATER)))]).await;
    store.indexed().await.unwrap();
    let first = free_holding(&store, home.path(), b"alphaoriginal").await;
    let edit = Amendment::Edit {
        text: Some("betaedited remark here".into()),
        spans: None,
        summary: "betaedited remark here".into(),
    };
    let amend = Amend {
        chat: ANA.into(),
        target: "goes".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 60,
        what: edit,
        raw: None,
    };
    assert_eq!(apply(&store, vec![Change::Amend(amend)]).await.changed, 1);
    settled(&store).await;
    gone_but_for(home.path(), "alphaoriginal", &first, "its first text, once edited");
    assert_eq!(said(&store, "goes").await.0.as_deref(), Some("betaedited remark here"));
    let edited = free_holding(&store, home.path(), b"betaedited").await;
    let n = n_of(&store, "goes").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    settled(&store).await;
    gone_but_for(home.path(), "betaedited", &edited, "its edited text, once erased");
    gone_but_for(home.path(), "alphaoriginal", &first, "its first text, once erased");
}

/// Whatever changes what a message with a time to go, or one opening once, says, or takes it away,
/// takes its earlier words out as an erasure takes them, so they don't outlive it: an edit, an
/// event's edit, a revoke, a deletion for you, a copy bringing a newer edit, and a photo's view-once
/// copy after a plain one kept its caption. Each among forty other messages in its segments.
#[tokio::test(flavor = "multi_thread")]
async fn whatever_changes_what_a_disappearing_or_view_once_message_said_leaves_nothing_of_it() {
    let amend = |what: Amendment| {
        Change::Amend(Amend {
            chat: ANA.into(),
            target: "goes".into(),
            sender: Some(ANA.into()),
            mine: false,
            at: AT + 60,
            what,
            raw: None,
        })
    };
    let goes = message(ANA, "goes", AT + 50, "gnuold words here", Some(LATER));
    let event = Message {
        kind: "event",
        detail: Some(serde_json::json!({ "event": { "name": "gnuold words here" } }).to_string()),
        ..goes.clone()
    };
    let plain = Message { expires_at: None, ..captioned("goes", "gnuold words here") };
    let cases: Vec<(&str, Message, Change)> = vec![
        (
            "an edit",
            goes.clone(),
            amend(Amendment::Edit { text: Some("newer words".into()), spans: None, summary: "newer words".into() }),
        ),
        (
            "an event's edit",
            event,
            amend(Amendment::EventEdit {
                text: Some("newer words".into()),
                spans: None,
                summary: "newer words".into(),
                detail: Some(serde_json::json!({ "event": { "name": "newer words" } }).to_string()),
            }),
        ),
        ("a revoke", goes.clone(), amend(Amendment::Revoke)),
        ("a deletion for you", goes.clone(), Change::DeleteMessage { chat: ANA.into(), id: "goes".into() }),
        (
            "a copy with a newer edit",
            goes.clone(),
            Change::Message(Message {
                text: Some("newer words".into()),
                summary: "newer words".into(),
                edited_at: Some(AT + 60),
                ..goes.clone()
            }),
        ),
        (
            "a view-once copy",
            plain.clone(),
            Change::Message(Message {
                text: None,
                spans: None,
                summary: "View once photo".into(),
                view_once: true,
                ..plain
            }),
        ),
    ];
    for (what, message, change) in cases {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        apply(&store, forty_others()).await;
        apply(&store, vec![Change::Message(message)]).await;
        store.indexed().await.unwrap();
        assert!(index_blocks(home.path(), "gnuold")[0].1 > 0, "{what}: its words are in the index");
        let before = free_holding(&store, home.path(), b"gnuold").await;
        assert_eq!(apply(&store, vec![change]).await.changed, 1, "{what}");
        settled(&store).await;
        gone_but_for(home.path(), "gnuold", &before, what);
        let others: i64 = query(&store, |db| {
            Ok(db.query_row("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'other'", [], |r| r.get(0))?)
        })
        .await;
        assert_eq!(others, 40, "{what}: the others are still found");
    }
}

/// Ana's LID, under which her messages came before her number and LID were joined.
const ANA_LID: &str = "100000000002@lid";

/// Joins Ana's number and LID, and takes merge steps until her LID's chat has merged into her
/// number's.
async fn chats_joined(store: &Store) {
    let joined = Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() };
    apply(store, vec![Change::Person(joined)]).await;
    let merging = || async {
        query(store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?)).await
    };
    for _ in 0..50 {
        if merging().await == 0 {
            return;
        }
        store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    panic!("the chats never merged");
}

/// Two copies of a photo, one plain with its caption and one sent to be opened once, folded into
/// one as Ana's chats merge, leave nothing of the caption the plain copy's row kept, whichever chat
/// holds which: a fold where either copy opens once or has a time to go takes what the copy that
/// goes says, and what the one kept said, as an erasure takes it, its caption taken out of the one
/// kept as the fold marks it. The raw protobuf the one kept keeps holds its caption by design, for
/// its opening, so the row's says another here. The copy to open once brings a raw protobuf whose
/// thumbnail spills past a page. Among forty captioned photos in the indexes' segments.
#[tokio::test(flavor = "multi_thread")]
async fn a_photo_folded_with_its_view_once_copy_leaves_nothing_of_the_caption_its_plain_copy_kept() {
    for (plain_in, once_in) in [(ANA_LID, ANA), (ANA, ANA_LID)] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let others =
            (0..40i64).map(|k| Change::Message(captioned(&format!("o{k:02}"), &format!("photo{k} by the sea"))));
        apply(&store, others.collect()).await;
        let caption = "gnucaption by the sea";
        let plain = Message {
            chat: plain_in.into(),
            sender: Some(plain_in.into()),
            raw: Some(Packed(raw_of("once", photo_of("by the sea", None)))),
            ..captioned("once", caption)
        };
        let raw = Packed(raw_of("once", wrapped(photo_of("by the sea", Some(noise(11, 20_000))))));
        let window = raw.0[raw.0.len() / 2..raw.0.len() / 2 + 64].to_vec();
        let once = Message {
            chat: once_in.into(),
            sender: Some(once_in.into()),
            text: None,
            spans: None,
            summary: "View once photo".into(),
            view_once: true,
            raw: Some(raw),
            ..captioned("once", "")
        };
        apply(&store, vec![Change::Message(plain), Change::Message(once)]).await;
        store.indexed().await.unwrap();
        let blocks = index_blocks(home.path(), "gnucaption");
        assert!(blocks[0].1 > 0 && blocks[2].1 > 0, "the plain copy's caption is found: {blocks:?}");
        let before = free_holding(&store, home.path(), b"gnucaption").await;
        let raw_before = free_holding(&store, home.path(), &window).await;
        chats_joined(&store).await;
        settled(&store).await;
        gone_but_for(home.path(), "gnucaption", &before, &format!("the caption the plain copy under {plain_in} kept"));
        if once_in == ANA_LID {
            bytes_gone_but_for(home.path(), &window, &raw_before, "the raw protobuf of the copy that went");
        }
        let said = said(&store, "once").await;
        assert_eq!(said, (None, None, Some("View once photo".into())), "the photo opens once, plain under {plain_in}");
    }
}

/// A chat merge's step ends after a fold that takes what a copy said out as an erasure takes it, so
/// an inbox row or a live write waits behind one such fold at most, as behind one of the timer's
/// erasures, rather than behind a step's two hundred: Ana's number's chat and her LID's, holding
/// the same thirty messages with a time to go among thirty plain ones, merge in steps of one
/// transaction each, each folding one of those at most, the plain ones before it going with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_chat_merge_folds_each_copy_with_a_time_to_go_in_a_transaction_of_its_own() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let copies = |chat: &str| -> Vec<Change> {
        (0..60i64)
            .map(|k| {
                let goes = (k % 2 == 0).then_some(LATER + k);
                Change::Message(message(
                    chat,
                    &format!("c{k:02}"),
                    AT + k,
                    &format!("copy{k} said in both chats"),
                    goes,
                ))
            })
            .collect()
    };
    apply(&store, copies(ANA_LID)).await;
    apply(&store, copies(ANA)).await;
    store.indexed().await.unwrap();
    // The merges left, and what's left in the chat going: its copies with a time to go, and all.
    let left = || async {
        query(&store, |db| {
            Ok(db.query_row(
                "SELECT (SELECT count(*) FROM chat_merges),
                    (SELECT count(*) FROM messages WHERE chat IN (SELECT gone FROM chat_merges)
                        AND expires_at IS NOT NULL),
                    (SELECT count(*) FROM messages WHERE chat IN (SELECT gone FROM chat_merges))",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
            )?)
        })
        .await
    };
    let joined = Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() };
    apply(&store, vec![Change::Person(joined)]).await;
    let mut was = left().await;
    assert_eq!(was, (1, 30, 60), "one chat merging into the other, with all sixty copies");
    let mut steps = Vec::new();
    while was.0 > 0 && steps.len() < 100 {
        let applied = store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        let now = left().await;
        steps.push((applied.transactions, was.1 - now.1, was.2 - now.2));
        was = now;
    }
    assert_eq!(was.0, 0, "the merge is done");
    let unpaced: Vec<_> = steps.iter().filter(|&&(transactions, timed, _)| transactions != 1 || timed > 1).collect();
    assert!(unpaced.is_empty(), "steps of more than one transaction or fold of a timed copy: {unpaced:?} of {steps:?}");
    assert_eq!(steps.len(), 31, "one step for each copy with a time to go, and one to end the merge: {steps:?}");
    let kept: (i64, i64, i64) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT count(DISTINCT chat), count(*), (SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'said')
            FROM messages WHERE id LIKE 'c__'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .await;
    assert_eq!(kept, (1, 60, 60), "each message once, in the chat kept, and found once");
}

/// A whole merge that comes after the WAL an erasure left was emptied, as one waiting for its time
/// after the last whole merge does, has the WAL emptied again once it's done, so jaw.db takes the
/// pages it zeroed at once rather than at the checkpointer's next pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_whole_merge_after_the_wal_an_erasure_left_was_emptied_has_it_emptied_again() {
    let home = tempfile::tempdir().unwrap();
    let limits = crate::store::Limits { whole_merge_every: Duration::from_secs(8), ..crate::store::LIMITS };
    let store = Store::open_with(home.path(), limits).unwrap();
    for batch in 0..6i64 {
        let links = (0..50i64).map(|k| {
            let goes = (batch, k) == (2, 20);
            let site = if goes { "wombatlater".to_string() } else { format!("site{batch}x{k}") };
            let message = linked(&format!("l{batch}{k:02}"), &site, 0);
            let expires_at = (goes || (batch, k) == (0, 0)).then_some(LATER);
            Change::Message(Message { at: AT + batch * 100 + k, expires_at, ..message })
        });
        apply(&store, links.collect()).await;
        store.indexed().await.unwrap();
    }
    // The first erasure's whole merges go at once, so the next wait eight seconds after they began.
    let first = n_of(&store, "l000").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n: first, at: LATER }]).await.changed, 1);
    settled(&store).await;
    let before = free_holding(&store, home.path(), b"wombatlater").await;
    let n = n_of(&store, "l220").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n, at: LATER }]).await.changed, 1);
    assert!(wal_emptied(&store).await, "the WAL the erasure left is emptied first");
    assert!(merge_noted(&store).await.is_some(), "while its whole merges wait");
    assert!(merged(&store).await, "they're done");
    assert!(wal_emptied(&store).await, "and the WAL they left is emptied too");
    gone_but_for(home.path(), "wombatlater", &before, "its link's words");
}

/// Whether a merge of `index`'s segments is under way, as FTS5's structure record (its row 10)
/// says: a level some of whose segments are the inputs of a merge not done yet.
fn merging_in(db: &Connection, index: &str) -> bool {
    let record: Vec<u8> =
        db.query_row(&format!("SELECT block FROM {index}_data WHERE id = 10"), [], |r| r.get(0)).unwrap();
    // Its cookie, then, in a contentless_delete index's, the mark of the record's second version,
    // whose segments each say their origins and tombstones too.
    let second = record[4..8] == [0xff, 0x00, 0x00, 0x01];
    let mut at = if second { 8 } else { 4 };
    let mut varint = || {
        let mut value = 0u64;
        for k in 0..9 {
            let byte = record[at];
            at += 1;
            if k == 8 {
                return (value << 8) | u64::from(byte);
            }
            value = (value << 7) | u64::from(byte & 0x7f);
            if byte < 0x80 {
                break;
            }
        }
        value
    };
    let (levels, segments, _writes) = (varint(), varint(), varint());
    let (mut merging, mut counted) = (false, 0);
    for _ in 0..levels {
        let (inputs, held) = (varint(), varint());
        merging |= inputs > 0;
        counted += held;
        for _ in 0..held * if second { 8 } else { 3 } {
            varint();
        }
    }
    assert_eq!((counted, at), (segments, record.len()), "{index}'s structure record reads as FTS5 writes it");
    merging
}

/// An erasure that comes while a contentless index is merged whole, after the erasure the merge
/// began for, may leave its words in the segment the merge is writing, so the merge's end keeps its
/// note, in sync_state and in the writer, and the next whole merge takes them out; and a kill
/// between the two leaves that to do. links_fts's pages are made small here, as FTS5 lets them be,
/// so its whole merge takes many of the writer's steps, as a large index's does: holding jaw.db's
/// write lock while it's under way keeps the writer between two of them while the second erasure
/// is sent. A first erasure's whole merge goes at once, so the one watched begins seconds after
/// the erasure it's for, watched from before its first step, however loaded the machine is. The
/// second's link's word sorts before the others', so the merge's first step has written it.
#[tokio::test(flavor = "multi_thread")]
async fn an_erasure_while_a_whole_merge_runs_is_kept_noted_for_the_next_and_a_kill_between_them_keeps_it() {
    let home = tempfile::tempdir().unwrap();
    let limits = crate::store::Limits { whole_merge_every: Duration::from_secs(4), ..crate::store::LIMITS };
    let store = Store::open_with(home.path(), limits).unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("INSERT INTO links_fts (links_fts, rank) VALUES ('pgsz', 32)", []).unwrap();
    let trail = |id: String, k: i64| {
        let path: Vec<String> = (0..8).map(|j| format!("trail{k}x{j}")).collect();
        let url = format!("https://site{k}.example/{}", path.join("/"));
        let spans = serde_json::json!([{ "t": "text", "s": "see " }, { "t": "link", "s": url, "url": url }]);
        let message = message(ANA, &id, AT + 1000 + k, &format!("see {url}"), None);
        Change::Message(Message { spans: Some(spans.to_string()), ..message })
    };
    for batch in 0..8i64 {
        let mut links: Vec<Change> = (0..100).map(|k| trail(format!("t{batch}{k:02}"), batch * 100 + k)).collect();
        if batch == 1 {
            links.insert(50, Change::Message(linked("zero", "wombatzero", 0)));
            links.insert(51, Change::Message(linked("first", "quokkafirst", 1)));
            links.insert(52, Change::Message(linked("second", "bilbysecond", 2)));
        }
        apply(&store, links).await;
        store.indexed().await.unwrap();
    }
    assert!(index_blocks(home.path(), "bilbysecond")[1].1 > 0, "links_fts holds the second's link");
    let zero = n_of(&store, "zero").await.unwrap();
    assert_eq!(apply(&store, vec![Change::Expire { n: zero, at: LATER }]).await.changed, 1);
    settled(&store).await;
    let before = free_holding(&store, home.path(), b"bilbysecond").await;
    let (first, second) = (n_of(&store, "first").await.unwrap(), n_of(&store, "second").await.unwrap());
    assert_eq!(apply(&store, vec![Change::Expire { n: first, at: LATER + 1 }]).await.changed, 1);
    let noted_first = store.remembered(LINKS_ERASED).unwrap();
    assert!(noted_first.is_some(), "the first erasure notes links_fts's whole merge");
    // The write lock taken between two of the whole merge's steps, once it's under way.
    let watching = tokio::task::spawn_blocking(move || {
        for _ in 0..100_000 {
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            if merging_in(&db, "links_fts") {
                return Some(db);
            }
            db.execute_batch("ROLLBACK").unwrap();
            std::thread::sleep(Duration::from_micros(200));
        }
        None
    });
    let watched = watching.await.unwrap();
    let db = watched.expect("links_fts's whole merge is seen under way, with more than its first step to go");
    let erasing = tokio::spawn({
        let store = store.clone();
        async move { store.apply(Lane::Live, vec![Change::Expire { n: second, at: LATER + 2 }]).await.unwrap() }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(erasing.await.unwrap().changed, 1);
    let noted_second = store.remembered(LINKS_ERASED).unwrap();
    assert!(noted_second.is_some() && noted_second != noted_first, "the second erasure notes it again");
    // The first whole merges done, as the writer has nothing more to merge.
    store.indexed().await.unwrap();
    assert!(!merging_in(&db, "links_fts"), "the first whole merge of links_fts is done");
    assert_eq!(store.remembered(LINKS_ERASED).unwrap(), noted_second, "its end keeps the second erasure's note");
    assert!(index_blocks(home.path(), "bilbysecond")[1].1 > 0, "and the segment it wrote, the second's word");
    let copy = killed(home.path());
    assert!(merged(&store).await, "the writer's next whole merge, a few seconds after the first began");
    settled(&store).await;
    gone_but_for(home.path(), "bilbysecond", &before, "an erasure during a whole merge, after the next");
    let found: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM links_fts WHERE links_fts MATCH 'example'", [], |r| r.get(0))?)
    })
    .await;
    assert_eq!(found, 800, "the others are still found");
    // Killed between the two whole merges, jawd started again does the second from the note.
    let note = |home: &Path| {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.query_row("SELECT value FROM sync_state WHERE key = ?1", [LINKS_ERASED], |r| r.get(0)).optional().unwrap()
    };
    assert_eq!(note(copy.path()), noted_second, "a kill between the two keeps the second erasure's note");
    assert!(index_blocks(copy.path(), "bilbysecond")[1].1 > 0);
    let again = Store::open(copy.path()).unwrap();
    settled(&again).await;
    gone_but_for(copy.path(), "bilbysecond", &[], "an erasure during a whole merge, killed before the next");
}

/// The kinds backfill making a row it couldn't show one to open once, which an edit had given
/// words, takes those words out as an erasure takes them: a photo sent to be opened once says only
/// what it is. Among forty other messages in the index's segments.
#[tokio::test(flavor = "multi_thread")]
async fn the_kinds_backfill_making_a_row_one_to_open_once_leaves_nothing_of_the_words_it_had() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, forty_others()).await;
    let unshown = Message {
        kind: "unknown",
        edited_at: Some(AT + 60),
        raw: Some(Packed(raw_of("goes", wrapped(photo_of("a caption", None))))),
        ..message(ANA, "goes", AT + 50, "gnuedit words here", None)
    };
    apply(&store, vec![Change::Message(unshown)]).await;
    store.indexed().await.unwrap();
    assert!(index_blocks(home.path(), "gnuedit")[0].1 > 0, "its words are in the index");
    let before = free_holding(&store, home.path(), b"gnuedit").await;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) SELECT 'kinds', 0, max(n) FROM messages", []).unwrap();
    let left =
        || db.query_row("SELECT count(*) FROM backfills WHERE name = 'kinds'", [], |r| r.get::<_, i64>(0)).unwrap();
    for _ in 0..20 {
        if left() == 0 {
            break;
        }
        store.apply(Lane::Background, vec![Change::Backfill(KINDS)]).await.unwrap();
    }
    assert_eq!(left(), 0, "the backfill is done");
    drop(db);
    settled(&store).await;
    gone_but_for(home.path(), "gnuedit", &before, "the words the edit gave it");
    assert_eq!(said(&store, "goes").await, (None, None, Some("View once photo".into())));
}

/// A backfill reading a message with a time to go again from its raw protobuf takes what its row
/// said before out as an erasure takes it: a location whose row has words an earlier converter
/// gave it, read again as this one reads it. Among forty other messages in the index's segments.
#[tokio::test(flavor = "multi_thread")]
async fn a_backfill_reading_a_disappearing_message_again_leaves_nothing_of_what_its_row_said() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, forty_others()).await;
    let location = e2e::Message {
        location_message: Some(Box::new(e2e::LocationMessage {
            degrees_latitude: Some(38.7118),
            degrees_longitude: Some(-9.1301),
            name: Some("Miradouro".into()),
            address: Some("Largo de Santa Luzia, Lisboa".into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let place = Message {
        kind: "location",
        summary: "Location: gnuplace Miradouro".into(),
        detail: Some(serde_json::json!({ "location": { "name": "gnuplace Miradouro" } }).to_string()),
        raw: Some(Packed(raw_of("place", location))),
        ..message(ANA, "place", AT + 50, "gnuplace Miradouro", Some(LATER))
    };
    apply(&store, vec![Change::Message(place)]).await;
    store.indexed().await.unwrap();
    let before = free_holding(&store, home.path(), b"gnuplace").await;
    // The places backfill left to do over every message, as a migration leaves it.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) SELECT 'places', 0, max(n) FROM messages", []).unwrap();
    let left =
        || db.query_row("SELECT count(*) FROM backfills WHERE name = 'places'", [], |r| r.get::<_, i64>(0)).unwrap();
    for _ in 0..20 {
        if left() == 0 {
            break;
        }
        store.apply(Lane::Background, vec![Change::Backfill(PLACES)]).await.unwrap();
    }
    assert_eq!(left(), 0, "the backfill is done");
    drop(db);
    settled(&store).await;
    gone_but_for(home.path(), "gnuplace", &before, "what its row said before");
    assert_eq!(said(&store, "place").await.0.as_deref(), Some("Miradouro\nLargo de Santa Luzia, Lisboa"));
}

/// The contacts backfill reading a card with a time to go again takes what its row said before out
/// as an erasure takes it: a card whose row has words and a link an earlier converter gave it, read
/// again into the name its vCard gives. Among thirty captioned photos and thirty messages with links,
/// so each index's segment holding its words holds others too.
#[tokio::test(flavor = "multi_thread")]
async fn the_contacts_backfill_reading_a_disappearing_card_again_leaves_nothing_of_what_its_row_said() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Marta Lopes\r\nTEL;waid=351213456789:+351 21 345 6789\r\nEND:VCARD";
    let contact = e2e::Message {
        contact_message: Some(Box::new(e2e::ContactMessage {
            display_name: Some("Marta Lopes".into()),
            vcard: Some(vcard.into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let card = Message { kind: "contact", raw: Some(Packed(raw_of("card", contact))), ..linked("card", "gnucard", 90) };
    let mut changes = several_of_each();
    changes.push(Change::Message(card));
    apply(&store, changes).await;
    store.indexed().await.unwrap();
    let blocks = index_blocks(home.path(), "gnucard");
    assert!(blocks.iter().all(|(_, held)| *held > 0), "its words are in every index: {blocks:?}");
    let before = free_holding(&store, home.path(), b"gnucard").await;
    // The contacts backfill left to do over every message, as its migration leaves it.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) SELECT 'contacts', 0, max(n) FROM messages", []).unwrap();
    let left =
        || db.query_row("SELECT count(*) FROM backfills WHERE name = 'contacts'", [], |r| r.get::<_, i64>(0)).unwrap();
    for _ in 0..20 {
        if left() == 0 {
            break;
        }
        store.apply(Lane::Background, vec![Change::Backfill(CONTACTS)]).await.unwrap();
    }
    assert_eq!(left(), 0, "the backfill is done");
    drop(db);
    settled(&store).await;
    gone_but_for(home.path(), "gnucard", &before, "what the card's row said before");
    assert_eq!(said(&store, "card").await.0.as_deref(), Some("Marta Lopes"));
    still_found(&store, "what stays").await;
}

/// The voices backfill reading a voice message with a time to go again, for its length and
/// waveform, takes what its row said before out as an erasure takes it (`read_again`): a
/// disappearing voice note whose row has words and a link, as no converter gives one, so what the
/// backfill takes out shows. Among thirty captioned photos and thirty messages with links, so each
/// index's segment holding its words holds others too.
#[tokio::test(flavor = "multi_thread")]
async fn the_voices_backfill_reading_a_disappearing_voice_note_again_leaves_nothing_of_what_its_row_said() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let audio = e2e::Message {
        audio_message: Some(Box::new(e2e::AudioMessage {
            ptt: Some(true),
            seconds: Some(14),
            mimetype: Some("audio/ogg; codecs=opus".into()),
            waveform: Some((0..64).map(|i| (i * 7 % 101) as u8).collect()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let voice = Message { kind: "voice", raw: Some(Packed(raw_of("voice", audio))), ..linked("voice", "gnuvoice", 90) };
    let mut changes = several_of_each();
    changes.push(Change::Message(voice));
    apply(&store, changes).await;
    store.indexed().await.unwrap();
    let blocks = index_blocks(home.path(), "gnuvoice");
    assert!(blocks.iter().all(|(_, held)| *held > 0), "its words are in every index: {blocks:?}");
    let before = free_holding(&store, home.path(), b"gnuvoice").await;
    // The voices backfill left to do over every message, as its migration leaves it.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) SELECT 'voices', 0, max(n) FROM messages", []).unwrap();
    let left =
        || db.query_row("SELECT count(*) FROM backfills WHERE name = 'voices'", [], |r| r.get::<_, i64>(0)).unwrap();
    for _ in 0..20 {
        if left() == 0 {
            break;
        }
        store.apply(Lane::Background, vec![Change::Backfill(VOICES)]).await.unwrap();
    }
    assert_eq!(left(), 0, "the backfill is done");
    drop(db);
    settled(&store).await;
    gone_but_for(home.path(), "gnuvoice", &before, "what the voice note's row said before");
    let (text, spans, _) = said(&store, "voice").await;
    assert_eq!((text, spans), (None, None), "read again, it says nothing in words");
    let detail: Option<String> =
        query(&store, |db| Ok(db.query_row("SELECT detail FROM messages WHERE id = 'voice'", [], |r| r.get(0))?)).await;
    assert!(detail.as_deref().is_some_and(|d| d.contains("waveform")), "and its row gives its waveform: {detail:?}");
    still_found(&store, "what stays").await;
}

/// A message of yours still waiting for the server, which goes with its chat's timer and so has a
/// time to go once it gets there, is taken as an erasure takes it, whatever takes it away first
/// (round three's I7): deleted for you, or revoked. Its words and its link's leave every index's
/// segments and no page freed since keeps them; the outbox row that still sends it holds them until
/// WhatsApp has it, and then goes as an erasure does too. Among thirty captioned photos and thirty
/// messages with links.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_of_yours_waiting_to_go_with_a_timer_deleted_or_revoked_leaves_nothing_of_what_it_said() {
    let goes_as = e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some("see https://gnuwaiting.example".into()),
            context_info: Some(Box::new(e2e::ContextInfo { expiration: Some(86_400), ..Default::default() })),
            ..Default::default()
        })),
        ..Default::default()
    };
    let gone = |what: Amendment| {
        Change::Amend(Amend {
            chat: ANA.into(),
            target: "mine".into(),
            sender: None,
            mine: true,
            at: AT + 95,
            what,
            raw: None,
        })
    };
    let cases = [
        ("a deletion for you", Change::DeleteMessage { chat: ANA.into(), id: "mine".into() }),
        ("a revoke", gone(Amendment::Revoke)),
    ];
    for (what, change) in cases {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let mine = Message {
            sender: None,
            mine: true,
            status: Some("pending"),
            expires_at: None,
            ..linked("mine", "gnuwaiting", 90)
        };
        let step = Step::Enqueue {
            message: Box::new(mine),
            kind: "text",
            raw: goes_as.encode_to_vec(),
            key: None,
            written: Written::default(),
        };
        let mut changes = several_of_each();
        changes.push(Change::Outbox(step));
        apply(&store, changes).await;
        store.indexed().await.unwrap();
        let blocks = index_blocks(home.path(), "gnuwaiting");
        assert!(blocks.iter().all(|(_, held)| *held > 0), "{what}: its words are in every index: {blocks:?}");
        let before = free_holding(&store, home.path(), b"gnuwaiting").await;
        assert_eq!(apply(&store, vec![change]).await.changed, 1, "{what}");
        settled(&store).await;
        let blocks: Vec<_> =
            index_blocks(home.path(), "gnuwaiting").into_iter().filter(|(_, held)| *held > 0).collect();
        assert!(blocks.is_empty(), "{what}: its words are left in {blocks:?}");
        let left: Vec<_> = pages_holding(home.path(), b"gnuwaiting")
            .into_iter()
            .filter(|(page, owner)| owner.as_deref() != Some("outbox") && !before.contains(page))
            .collect();
        assert!(left.is_empty(), "{what}: left but in the outbox row that sends it: {left:?}");
        still_found(&store, what).await;
        let outbox: i64 =
            query(&store, |db| Ok(db.query_row("SELECT n FROM outbox WHERE id = 'mine'", [], |r| r.get(0))?)).await;
        apply(&store, vec![Change::Outbox(Step::Sent { outbox })]).await;
        settled(&store).await;
        assert_eq!(store.count("outbox").unwrap(), 0, "{what}: WhatsApp has it");
        gone_but_for(home.path(), "gnuwaiting", &before, &format!("{what}, once WhatsApp has it"));
    }
}

/// How a case of the test below changes its message.
enum How {
    /// A change, in the live lane.
    Change(Box<Change>),
    /// A backfill, run to its end over every message, as its migration leaves it.
    Backfill(&'static str),
}

/// Whatever changes or takes away what a message with a time to go, or one opening once, says,
/// takes its words out of the contentless indexes too, by the whole merges it notes (round three's
/// T6): an edit, an event's edit, a revoke, a deletion for you, a copy with a newer edit, a
/// view-once copy after a plain one, and the places, events and kinds backfills reading a message
/// again, each of a captioned photo, whose words library_fts holds, or of a message with links,
/// whose words links_fts and library_fts hold, where the path can have one. Each among thirty
/// captioned photos and thirty messages with links in the same segments, so its index's segment
/// isn't rewritten when its entry goes and only a whole merge takes its words out.
#[tokio::test(flavor = "multi_thread")]
async fn whatever_changes_a_disappearing_photo_or_link_takes_its_words_out_of_the_contentless_indexes() {
    let amend = |what: Amendment| {
        How::Change(Box::new(Change::Amend(Amend {
            chat: ANA.into(),
            target: "goes".into(),
            sender: Some(ANA.into()),
            mine: false,
            at: AT + 95,
            what,
            raw: None,
        })))
    };
    let newer = || Amendment::Edit { text: Some("newer words".into()), spans: None, summary: "newer words".into() };
    let deleted = || How::Change(Box::new(Change::DeleteMessage { chat: ANA.into(), id: "goes".into() }));
    let copy_of = |message: &Message| {
        How::Change(Box::new(Change::Message(Message {
            text: Some("newer words".into()),
            spans: None,
            summary: "newer words".into(),
            edited_at: Some(AT + 95),
            ..message.clone()
        })))
    };
    let photo = Message { at: AT + 90, expires_at: Some(LATER), ..captioned("goes", "gnuchanged by the sea") };
    let link = linked("goes", "gnuchanged", 90);
    let event = Message {
        kind: "event",
        detail: Some(serde_json::json!({ "event": { "name": "see https://gnuchanged.example" } }).to_string()),
        raw: Some(Packed(raw_of(
            "goes",
            e2e::Message {
                event_message: Some(Box::new(e2e::EventMessage {
                    name: Some("Jantar de anos".into()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        ))),
        ..link.clone()
    };
    let place = Message {
        kind: "location",
        raw: Some(Packed(raw_of(
            "goes",
            e2e::Message {
                location_message: Some(Box::new(e2e::LocationMessage {
                    degrees_latitude: Some(38.7118),
                    degrees_longitude: Some(-9.1301),
                    name: Some("Miradouro".into()),
                    ..Default::default()
                })),
                ..Default::default()
            },
        ))),
        ..link.clone()
    };
    let unshown = Message {
        kind: "unknown",
        edited_at: Some(AT + 60),
        raw: Some(Packed(raw_of("goes", wrapped(photo_of("a caption", None))))),
        ..link.clone()
    };
    let plain = Message { at: AT + 90, ..captioned("goes", "gnuchanged by the sea") };
    let once = Message { text: None, spans: None, summary: "View once photo".into(), view_once: true, ..plain.clone() };
    let event_edit = Amendment::EventEdit {
        text: Some("newer words".into()),
        spans: None,
        summary: "newer words".into(),
        detail: Some(serde_json::json!({ "event": { "name": "newer words" } }).to_string()),
    };
    let cases: Vec<(&str, Message, How)> = vec![
        ("an edit of a captioned photo", photo.clone(), amend(newer())),
        ("an edit of a message with links", link.clone(), amend(newer())),
        ("an event's edit of an event with links", event.clone(), amend(event_edit)),
        ("a revoke of a captioned photo", photo.clone(), amend(Amendment::Revoke)),
        ("a revoke of a message with links", link.clone(), amend(Amendment::Revoke)),
        ("a deletion for you of a captioned photo", photo.clone(), deleted()),
        ("a deletion for you of a message with links", link.clone(), deleted()),
        ("a copy of a captioned photo with a newer edit", photo.clone(), copy_of(&photo)),
        ("a copy of a message with links with a newer edit", link.clone(), copy_of(&link)),
        ("a view-once copy of a captioned photo", plain, How::Change(Box::new(Change::Message(once)))),
        ("the places backfill reading a location with links again", place, How::Backfill(PLACES)),
        ("the events backfill reading an event with links again", event, How::Backfill(EVENTS)),
        ("the kinds backfill making a row with links one to open once", unshown, How::Backfill(KINDS)),
    ];
    // Every case is looked at before the test fails, so a rule switched off shows each it breaks.
    let mut problems = Vec::new();
    for (what, message, how) in cases {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let mut changes = several_of_each();
        changes.insert(30, Change::Message(message));
        apply(&store, changes).await;
        store.indexed().await.unwrap();
        let held: Vec<_> = index_blocks(home.path(), "gnuchanged").into_iter().filter(|(_, held)| *held > 0).collect();
        let contentless = held.iter().filter(|(index, _)| *index != "messages_fts").count();
        assert!(held.len() >= 2 && contentless > 0, "{what}: its words are in the contentless indexes: {held:?}");
        let before = free_holding(&store, home.path(), b"gnuchanged").await;
        match how {
            How::Change(change) => assert_eq!(apply(&store, vec![*change]).await.changed, 1, "{what}"),
            How::Backfill(name) => {
                let db = Connection::open(home.path().join("jaw.db")).unwrap();
                db.busy_timeout(Duration::from_secs(10)).unwrap();
                db.execute("INSERT INTO backfills (name, after, upto) SELECT ?1, 0, max(n) FROM messages", [name])
                    .unwrap();
                let left = || {
                    db.query_row("SELECT count(*) FROM backfills WHERE name = ?1", [name], |r| r.get::<_, i64>(0))
                        .unwrap()
                };
                for _ in 0..20 {
                    if left() == 0 {
                        break;
                    }
                    store.apply(Lane::Background, vec![Change::Backfill(name)]).await.unwrap();
                }
                assert_eq!(left(), 0, "{what}: the backfill is done");
            }
        }
        settled(&store).await;
        let left = left_of(home.path(), "gnuchanged", &before);
        if !left.is_empty() {
            problems.push(format!("{what}: its words are left {}", left.join("; ")));
        }
        still_found(&store, what).await;
    }
    assert!(problems.is_empty(), "{} of the cases leave words behind:\n{}", problems.len(), problems.join("\n"));
}

/// Thirty captioned photos and thirty messages with links that stay, put in the segments of every
/// index beside what a test changes: with others in it, a segment isn't rewritten when one of its
/// entries goes, so a contentless index keeps that one's words until it's merged whole.
pub(crate) fn several_of_each() -> Vec<Change> {
    let photos = (0..30i64).map(|k| {
        Change::Message(Message { at: AT + k, ..captioned(&format!("p{k:02}"), &format!("photo{k} by the lake")) })
    });
    let links = (0..30i64).map(|k| {
        Change::Message(Message { expires_at: None, ..linked(&format!("l{k:02}"), &format!("site{k}"), 40 + k) })
    });
    photos.chain(links).collect()
}

/// Asserts that every photo and link of `several_of_each` is still found in each index.
pub(crate) async fn still_found(store: &Store, what: &'static str) {
    let found = query(store, |db| {
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0));
        Ok((
            count("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'lake'")?,
            count("SELECT count(*) FROM links_fts WHERE links_fts MATCH 'example'")?,
            count("SELECT count(*) FROM library_fts WHERE library_fts MATCH 'lake OR example'")?,
        ))
    })
    .await;
    assert_eq!(found, (30, 30, 60), "{what}: the others are still found in messages_fts, links_fts and library_fts");
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

/// jaw.db as main's jawd leaves it at migration 30, built by SQL as the audit's measurements were:
/// `messages` messages in `chats` chats, texts of 3 to 25 words from 20,000, raw protobufs of 380 to
/// 620 bytes, a link in one in 22, a view-once photo as main kept it in one in 120 (60% of them
/// captioned, a quarter of those with a link), one in 200 to go within a week and `due` past their
/// time, and two of 9,000 words due before them; then migrated, its links made by main's backfill
/// and its indexes merged by the writer, and its WAL emptied, ready to copy.
async fn measured(home: &Path, messages: i64, chats: i64, due: i64) {
    let now = now_ms();
    let mut x = 0x2545_f491_4f6c_dd1du64;
    let mut next = move |most: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % most
    };
    {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..30] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 30).unwrap();
        db.execute(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < ?1)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c",
            [chats],
        )
        .unwrap();
        db.execute_batch(
            "INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            INSERT INTO chats (n, id, kind, person, v) SELECT n, printf('3519%08d@s.whatsapp.net', n), 'direct', n, 1
                FROM people;
            INSERT INTO sync_state (key, value) VALUES ('me', '351910000001@s.whatsapp.net');",
        )
        .unwrap();
        let every_due = (messages / due.max(1)).max(1);
        for chunk in (1..=messages + 2).collect::<Vec<i64>>().chunks(5_000) {
            db.execute_batch("BEGIN").unwrap();
            let mut insert = db
                .prepare(
                    "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, summary, view_once, expires_at,
                        raw, v)
                    VALUES (?1, ?2, printf('3EB0%016d', ?1), ?3, ?1, ?2, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
                )
                .unwrap();
            for &n in chunk {
                let long = n > messages;
                let words = if long { 9_000 } else { 3 + next(23) };
                let mut text: String = (0..words).map(|_| format!("w{}", next(20_000))).collect::<Vec<_>>().join(" ");
                let once = !long && n % 120 == 0;
                let captioned = once && next(10) < 6;
                let linked = !long && if once { captioned && next(4) == 0 } else { n % 22 == 0 };
                let mut spans = serde_json::json!([{ "t": "text", "s": text.clone() }]);
                if linked {
                    let url = format!("https://site{}.example/p{n}", next(5_000));
                    spans = serde_json::json!([{ "t": "text", "s": format!("{text} ") },
                        { "t": "link", "s": url, "url": url }]);
                    text = format!("{text} {url}");
                }
                let (text, spans, summary) = match (once, captioned) {
                    (true, false) => (None, None, Some("View once photo".to_string())),
                    _ => (Some(text), Some(spans.to_string()), None),
                };
                let expires_at = if long {
                    Some(n - messages)
                } else if n % every_due == 0 && n / every_due <= due {
                    Some(now - 60_000 + n % 1_000)
                } else if n % 200 == 0 {
                    Some(now + 3_600_000 + next(7 * 86_400_000) as i64)
                } else {
                    None
                };
                let raw: Vec<u8> = (0..380 + next(241)).map(|_| next(256) as u8).collect();
                let kind = if once { "image" } else { "text" };
                let row =
                    rusqlite::params![n, 1 + n % chats, AT + n, kind, text, spans, summary, once, expires_at, raw];
                insert.execute(row).unwrap();
            }
            drop(insert);
            db.execute_batch("COMMIT").unwrap();
        }
        db.execute("INSERT INTO backfills (name, after, upto) VALUES ('links', 0, ?1)", [messages + 2]).unwrap();
    }
    let store = Store::open(home).unwrap();
    while store.backfill_left(crate::store::links::LINKS).unwrap() > 0 {
        let links = vec![Change::Backfill(crate::store::links::LINKS); 64];
        assert_eq!(store.apply(Lane::Background, links).await.unwrap().quarantined, 0);
    }
    store.indexed().await.unwrap();
    store.truncate().await.unwrap();
}

/// A live write, a message, every 10 ms until `done` is set, each timed from its asking to its answer.
async fn live_messages(store: Store, done: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Vec<Duration> {
    let mut every = tokio::time::interval(Duration::from_millis(10));
    let mut took = Vec::new();
    while !done.load(std::sync::atomic::Ordering::Relaxed) {
        every.tick().await;
        let began = std::time::Instant::now();
        let id = format!("live{:06}", took.len());
        store.apply(Lane::Live, vec![Change::Message(message(RUI, &id, now_ms(), "a live one", None))]).await.unwrap();
        took.push(began.elapsed());
    }
    took
}

/// Adds the lengths of the writer's transactions since its `since`th to `into`, saying how many it
/// has made.
fn transactions_since(store: &Store, since: u64, into: &mut Vec<Duration>) -> u64 {
    let (count, recent) = store.writer_runs();
    let new = (count - since).min(recent.len() as u64) as usize;
    into.extend(recent[recent.len() - new..].iter().map(|&us| Duration::from_micros(us)));
    count
}

/// Durations as p50, p99 and the most, how many there were, and the share past 3 ms.
fn spread(mut took: Vec<Duration>) -> String {
    if took.is_empty() {
        return "none".into();
    }
    took.sort_unstable();
    let at = |q: f64| took[((took.len() - 1) as f64 * q).round() as usize].as_secs_f64() * 1e3;
    let past = took.iter().filter(|t| **t > Duration::from_millis(3)).count() as f64 / took.len() as f64;
    format!(
        "{:.2} / {:.2} / {:.2} ms (p50 / p99 / most) of {}, {:.0}% past 3 ms",
        at(0.5),
        at(0.99),
        at(1.0),
        took.len(),
        past * 100.0
    )
}

/// The load average, as the Mac says it.
fn load() -> String {
    std::process::Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The `once` backfill and erasing a backlog, each alone and beside a live write every 10 ms, as
/// the audit's P1 and P2 measured them, at the owner's shape (87,000 messages in 502 chats) or,
/// with `JAW_MEASURE_MESSAGES=1000000 JAW_MEASURE_CHATS=3000`, a million: alone, the writer's
/// transactions; beside, the live writes:
/// `cargo test --release -p jawd measure_erasing -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, minutes long at a million messages"]
async fn measure_erasing_a_backlog_and_the_once_backfill() {
    let number = |name: &str, or: i64| std::env::var(name).ok().and_then(|n| n.parse().ok()).unwrap_or(or);
    let (messages, chats) = (number("JAW_MEASURE_MESSAGES", 87_000), number("JAW_MEASURE_CHATS", 502));
    let made = tempfile::tempdir().unwrap();
    let began = std::time::Instant::now();
    measured(made.path(), messages, chats, 3_200).await;
    let bytes = std::fs::metadata(made.path().join("jaw.db")).unwrap().len();
    eprintln!(
        "{messages} messages in {chats} chats, {} MB, made in {:.0} s",
        bytes / 1_000_000,
        began.elapsed().as_secs_f64()
    );
    let copy = || {
        let home = tempfile::tempdir().unwrap();
        std::fs::copy(made.path().join("jaw.db"), home.path().join("jaw.db")).unwrap();
        home
    };
    let beside = |store: &Store| {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        (done.clone(), tokio::spawn(live_messages(store.clone(), done)))
    };
    let stop = |(done, writing): (std::sync::Arc<std::sync::atomic::AtomicBool>, tokio::task::JoinHandle<_>)| async move {
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        writing.await.unwrap()
    };
    let home = copy();
    let store = Store::open(home.path()).unwrap();
    let live = beside(&store);
    tokio::time::sleep(Duration::from_secs(5)).await;
    eprintln!("live writes alone, load {}: {}", load(), spread(stop(live).await));
    drop(store);
    for with_live in [false, true] {
        let home = copy();
        let store = Store::open(home.path()).unwrap();
        let live = with_live.then(|| beside(&store));
        let (mut took, mut seen) = (Vec::new(), store.writer_runs().0);
        let began = std::time::Instant::now();
        while store.backfill_left(VIEW_ONCE).unwrap() > 0 {
            let steps = vec![Change::Backfill(VIEW_ONCE); 64];
            assert_eq!(store.apply(Lane::Background, steps).await.unwrap().quarantined, 0);
            seen = transactions_since(&store, seen, &mut took);
        }
        let backfilled = began.elapsed().as_secs_f64();
        match live {
            Some(live) => eprintln!(
                "the once backfill beside live writes, load {}, {backfilled:.2} s: live writes {}",
                load(),
                spread(stop(live).await)
            ),
            None => eprintln!(
                "the once backfill alone, load {}, {backfilled:.2} s: its transactions {}",
                load(),
                spread(took)
            ),
        }
    }
    for with_live in [false, true] {
        let home = copy();
        let store = Store::open(home.path()).unwrap();
        let mut seen = store.writer_runs().0;
        let mut long = Vec::new();
        assert_eq!(crate::expiry::erase_due(&store, 1, &[]).await.unwrap().erased, 1);
        seen = transactions_since(&store, seen, &mut long);
        let deleted = Amend {
            chat: format!("3519{:08}@s.whatsapp.net", 1 + (messages + 2) % chats),
            target: format!("3EB0{:016}", messages + 2),
            sender: None,
            mine: true,
            at: now_ms(),
            what: Amendment::DeleteForMe,
            raw: None,
        };
        let began = std::time::Instant::now();
        assert_eq!(apply(&store, vec![Change::Amend(deleted)]).await.changed, 1);
        let deleting = began.elapsed().as_secs_f64() * 1e3;
        seen = store.writer_runs().0.max(seen);
        let live = with_live.then(|| beside(&store));
        let (mut took, mut batches) = (Vec::new(), 0);
        let began = std::time::Instant::now();
        loop {
            let batch = crate::expiry::erase_due(&store, now_ms(), &[]).await.unwrap();
            seen = transactions_since(&store, seen, &mut took);
            if batch.erased == 0 {
                break;
            }
            batches += 1;
        }
        let erasing = began.elapsed().as_secs_f64();
        match live {
            Some(live) => eprintln!(
                "the backlog beside live writes, load {}, {batches} batches in {erasing:.2} s: live writes {}",
                load(),
                spread(stop(live).await)
            ),
            None => eprintln!(
                "a long message erased {}, deleted for you {deleting:.2} ms; the backlog alone, load {}, {batches} \
                 batches in {erasing:.2} s: its transactions {}",
                spread(long),
                load(),
                spread(took)
            ),
        }
    }
}
