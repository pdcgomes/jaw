use std::path::Path;
use std::time::Duration;

use prost::Message as _;
use rusqlite::{Connection, OptionalExtension};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::*;
use crate::store::kinds::KINDS;
use crate::store::write::{Packed, now_ms};
use crate::store::{Amend, Amendment, Applied, Change, Chat, Lane, Message, Store, index_against_text, read, schema};
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

/// Message `id` of Ana's with a link to `site`, due at LATER + `k`.
fn linked(id: &str, site: &str, k: i64) -> Message {
    let spans = serde_json::json!([{ "t": "text", "s": "see " },
        { "t": "link", "s": format!("https://{site}.example"), "url": format!("https://{site}.example") }]);
    Message {
        spans: Some(spans.to_string()),
        ..message(ANA, id, AT + k, &format!("see https://{site}.example"), Some(LATER + k))
    }
}

/// When sync_state says links_fts is to be merged whole for an erasure, if it does.
async fn merge_noted(store: &Store) -> Option<String> {
    query(store, |db| {
        Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [LINKS_ERASED], |r| r.get(0)).optional()?)
    })
    .await
}

/// Waits up to five seconds for the whole merge sync_state notes to be done.
async fn merged(store: &Store) -> bool {
    for _ in 0..250 {
        if merge_noted(store).await.is_none() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

/// Killed after an erasure's commit, before the whole merge of links_fts it leaves to do has begun,
/// jawd started again still does it: the erasure notes it in the same commit. The erasure's own
/// secure-delete has taken its link's words out already, in the SQLite jaw bundles.
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
    assert!(merge_noted(&store).await.is_some(), "the erasure notes the merge it leaves to do");
    let killed = killed(home.path());
    let left = files(killed.path());
    assert!(!holds(&left, b"wombatsecond") && holds(&left, b"numbatstays"), "its link's words went with it");
    let again = Store::open(killed.path()).unwrap();
    assert!(merge_noted(&again).await.is_some(), "a kill keeps the merge noted");
    assert!(merged(&again).await, "and started again, jawd does it");
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
