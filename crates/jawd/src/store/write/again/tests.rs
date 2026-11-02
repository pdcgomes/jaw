use rusqlite::Connection;
use serde_json::{Value, json};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::*;
use crate::convert::Me;
use crate::store::read::{self, Around};
use crate::store::write::{Packed, links};
use crate::store::{Applied, Lane, Store, schema};
use crate::work::Priority;

const ANA: &str = "351910000002@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;
const VCARD: &str = "BEGIN:VCARD\nVERSION:3.0\nFN:Marta Lopes\nTEL;waid=351213456789:+351 21 345 6789\nEND:VCARD";

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// Message `n` of Ana's chat as WhatsApp sent it, its raw protobuf packed as jaw.db keeps one.
fn raw(n: i64, message: e2e::Message) -> Vec<u8> {
    let info = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(false),
            id: Some(format!("m{n}")),
            participant: None,
        },
        message: Some(message),
        message_timestamp: Some(((AT + n) / 1000) as u64),
        ..Default::default()
    };
    Packed::new(&info.encode_to_vec()).0
}

fn location(n: i64) -> e2e::Message {
    e2e::Message {
        location_message: Some(Box::new(e2e::LocationMessage {
            degrees_latitude: Some(38.7118),
            degrees_longitude: Some(-9.1301),
            name: Some(format!("Miradouro {n}")),
            address: Some(format!("Largo de Santa Luzia {n}, Lisboa")),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn contact() -> e2e::Message {
    e2e::Message {
        contact_message: Some(Box::new(e2e::ContactMessage {
            display_name: Some("Marta".into()),
            vcard: Some(VCARD.into()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// A message's row as the test writes it: its kind, text, summary, detail, raw protobuf and whether
/// it was deleted for everyone.
type Row = (&'static str, Option<String>, Option<String>, Option<String>, Option<Vec<u8>>, bool);

/// A jaw.db at the version before a location was found by its address too, its feed at seq 1,000,
/// with `count` of Ana's messages as the converter wrote them then: every tenth a location, found by
/// its name alone, every fifteenth a contact card, one location deleted for everyone, and text
/// otherwise; each with its raw protobuf.
fn before_places(home: &std::path::Path, count: i64) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    let places = schema::MIGRATIONS.iter().position(|m| m.contains("SELECT 'places'")).expect("the places migration");
    for migration in &schema::MIGRATIONS[..places] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", places as i64).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 1);
        INSERT INTO events (seq, at, type, key, chat) VALUES (1000, {AT}, 'chat', 1, 1);"
    ))
    .unwrap();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, deleted, v)
            VALUES (?1, 1, 'm' || ?1, ?2, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
        )
        .unwrap();
    for n in 1..=count {
        let at = AT + n;
        let row: Row = if n == 20 {
            ("location", None, Some("This message was deleted".into()), None, None, true)
        } else if n % 10 == 0 {
            let detail = json!({ "location": {
                "lat": 38.7118, "lon": -9.1301, "name": format!("Miradouro {n}"),
                "address": format!("Largo de Santa Luzia {n}, Lisboa"), "url": null,
            }});
            let name = format!("Miradouro {n}");
            (
                "location",
                Some(name.clone()),
                Some(format!("Location: {name}")),
                Some(detail.to_string()),
                Some(raw(n, location(n))),
                false,
            )
        } else if n % 15 == 0 {
            let detail = json!({ "contact": { "name": "Marta", "vcard": VCARD } });
            (
                "contact",
                Some("Marta".into()),
                Some("Contact: Marta".into()),
                Some(detail.to_string()),
                Some(raw(n, contact())),
                false,
            )
        } else {
            let text = e2e::Message { conversation: Some(format!("olá {n}")), ..Default::default() };
            ("text", Some(format!("olá {n}")), None, None, Some(raw(n, text)), false)
        };
        let (kind, text, summary, detail, raw, deleted) = row;
        insert.execute(rusqlite::params![n, at, kind, text, summary, detail, raw, deleted]).unwrap();
    }
}

async fn backfill_left(store: &Store) -> Option<(i64, i64)> {
    query(store, |db| {
        Ok(db
            .query_row("SELECT after, upto FROM backfills WHERE name = 'places'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })
    .await
}

/// Each message's ID, version, text, spans and detail, by its n.
async fn rows(store: &Store) -> Vec<(String, i64, Option<String>, Option<String>, Option<Value>)> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT id, v, text, spans, detail FROM messages ORDER BY n")?
            .query_map([], |r| {
                let detail: Option<String> = r.get(4)?;
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, detail.map(|d| serde_json::from_str(&d).unwrap())))
            })?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

/// The IDs of Ana's messages search finds for `words`, once what was written is indexed.
async fn found(store: &Store, words: &'static str) -> Vec<String> {
    store.indexed().await.unwrap();
    query(store, move |db| {
        Ok(read::search(db, words, None, None, 100)?.hits.into_iter().map(|hit| hit.message.id).collect())
    })
    .await
}

async fn step(store: &Store) -> Applied {
    store.apply(Lane::Background, vec![Change::Backfill(links::PLACES)]).await.unwrap()
}

/// Takes the backfill's steps until it's done, failing at once should a step fail, which is rolled
/// back and so never moves on, and once it has taken more than it can need.
async fn backfilled(store: &Store) {
    let mut steps = store.backfill_steps(links::PLACES);
    while backfill_left(store).await.is_some() {
        steps.take("the backfill never ended");
        let applied = step(store).await;
        assert_eq!(applied.quarantined, 0, "a step failed: {applied:?}");
    }
}

async fn events(store: &Store) -> usize {
    query(store, |db| Ok(read::events(db, 0, 100_000)?.len())).await
}

/// The newest event's seq, from which `told_since` reads.
async fn last_event(store: &Store) -> i64 {
    query(store, |db| Ok(db.query_row("SELECT coalesce(max(seq), 0) FROM events", [], |r| r.get(0))?)).await
}

/// The messages told since event `since`, by their n, once for each time each was told.
async fn told_since(store: &Store, since: i64) -> Vec<i64> {
    query(store, move |db| {
        Ok(db
            .prepare("SELECT key FROM events WHERE type = 'message' AND seq > ?1 ORDER BY key, seq")?
            .query_map([since], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_locations_held_before_are_read_again_a_step_at_a_time_and_none_twice() {
    let home = tempfile::tempdir().unwrap();
    before_places(home.path(), 200);
    // A location whose raw protobuf reads now as another kind keeps the row it has.
    let text = e2e::Message { conversation: Some("olá 40".into()), ..Default::default() };
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE messages SET raw = ?1 WHERE n = 40", [raw(40, text)]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    assert_eq!(backfill_left(&store).await, Some((0, 200)), "the migration leaves the backfill to do");
    assert_eq!(store.backfill_left(links::PLACES).unwrap(), 200, "as jawd status says");
    assert!(found(&store, "luzia").await.is_empty(), "a location from before is found by its name alone");
    let before = rows(&store).await;

    step(&store).await;
    let (after, _) = backfill_left(&store).await.expect("more to look at");
    assert!(
        (10..=64).contains(&after),
        "a step looks at 64 messages at most, ending once it has read some again for a while: {after}"
    );
    let first = rows(&store).await;
    let location = &first[9];
    assert_eq!(
        location.2.as_deref(),
        Some("Miradouro 10\nLargo de Santa Luzia 10, Lisboa"),
        "found by its address too"
    );
    assert_eq!(location.3.as_deref(), Some("[]"), "and its text isn't what it shows");
    assert_eq!(location.4.as_ref().unwrap()["location"]["address"], "Largo de Santa Luzia 10, Lisboa");
    assert!(location.1 > before[9].1, "at a new version, so windows show it again");
    assert_eq!(first[14], before[14], "a contact card isn't this backfill's");
    assert_eq!(first[69], before[69], "a location the step hasn't reached is as it was");
    assert_eq!(first[0], before[0], "and a text message isn't touched");

    backfilled(&store).await;
    let after = rows(&store).await;
    for (n, (now, was)) in after.iter().zip(&before).enumerate() {
        let n = n as i64 + 1;
        let read_again = n != 20 && n != 40 && n % 10 == 0;
        assert_eq!(now.1 > was.1, read_again, "m{n}: {now:?}");
    }
    assert_eq!(after[19], before[19], "the one deleted for everyone has nothing to read again");
    assert_eq!(after[39], before[39], "and one whose raw reads as another kind is left as it is");
    assert_eq!(found(&store, "luzia 190").await, ["m190"], "found by its address");
    assert_eq!(found(&store, "marta").await.len(), 7, "each card by its name, as before");

    // Taken again, from the start, it finds nothing more to do.
    let (seen, events_before) = (rows(&store).await, events(&store).await);
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) VALUES ('places', 0, 200)", []).unwrap();
    backfilled(&store).await;
    assert_eq!(rows(&store).await, seen, "nothing read twice");
    assert_eq!(events(&store).await, events_before, "and nothing told again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_the_converter_has_written_again_since_and_a_window_on_one_read_again() {
    let home = tempfile::tempdir().unwrap();
    before_places(home.path(), 30);
    let store = Store::open(home.path()).unwrap();
    // History brings m10 again before the backfill reaches it, and the converter writes it as it
    // reads one now.
    let info = WebMessageInfo::decode(unpack(&raw(10, location(10))).unwrap().as_slice()).unwrap();
    let again = crate::convert::message(&info, &Me::default(), false);
    store.apply(Lane::Background, again).await.unwrap();
    let written = rows(&store).await[9].clone();
    assert_eq!(written.2.as_deref(), Some("Miradouro 10\nLargo de Santa Luzia 10, Lisboa"));

    let window = query(&store, |db| read::messages(db, ANA, Around::Newest, 50)).await;
    let shown = |rows: &[read::MessageRow], id: &str| rows.iter().find(|r| r.id == id).map(|r| r.v).unwrap();
    backfilled(&store).await;
    assert_eq!(rows(&store).await[9], written, "left as the converter wrote it");
    let read_again = query(&store, |db| read::messages(db, ANA, Around::Newest, 50)).await;
    assert!(shown(&read_again, "m30") > shown(&window, "m30"), "a window reading m30 again sees its new version");
    let place = read_again.iter().find(|r| r.id == "m30").unwrap();
    assert_eq!(place.spans.get(), "[]", "a location's row shows no text of its own");
    let detail: Value = serde_json::from_str(place.detail.as_ref().unwrap().get()).unwrap();
    let fields = json!({
        "lat": 38.7118, "lon": -9.1301, "name": "Miradouro 30", "address": "Largo de Santa Luzia 30, Lisboa", "url": null,
    });
    assert_eq!(detail["location"], fields, "the protocol's fields");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_whose_reading_again_panics_is_kept_in_quarantine_and_the_rest_are_read_again() {
    let home = tempfile::tempdir().unwrap();
    before_places(home.path(), 30);
    let panics = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(false),
            id: Some(kinds::tests::PANICS.into()),
            participant: None,
        },
        message: Some(location(10)),
        message_timestamp: Some(((AT + 10) / 1000) as u64),
        ..Default::default()
    };
    let raw_kept = Packed::new(&panics.encode_to_vec()).0;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE messages SET raw = ?1 WHERE n = 10", [&raw_kept]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    let before = rows(&store).await;
    backfilled(&store).await;
    let after = rows(&store).await;
    assert_eq!(after[9], before[9], "the one whose reading panicked is as it was");
    assert_eq!(
        after[29].2.as_deref(),
        Some("Miradouro 30\nLargo de Santa Luzia 30, Lisboa"),
        "and the rest read again"
    );
    let kept: Vec<(String, Option<Vec<u8>>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, raw FROM quarantine WHERE source = 'places'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [(format!("{ANA} m10"), Some(raw_kept))], "kept in quarantine with its raw protobuf");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_reading_64_locations_in_a_row_again_ends_once_it_has_for_a_while() {
    let home = tempfile::tempdir().unwrap();
    before_places(home.path(), 70);
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE messages SET kind = 'location', raw = ?1 WHERE n <= 64", [raw(1, location(1))]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    step(&store).await;
    let (after, _) = backfill_left(&store).await.expect("more to look at");
    assert!((1..64).contains(&after), "a step reading 64 in a row again ends once it has for a while: {after}");
    backfilled(&store).await;
    let read: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM messages WHERE n <= 64 AND text LIKE '%Largo%'", [], |r| r.get(0))?)
    })
    .await;
    assert_eq!(read, 63, "and the next goes on from the first it didn't read, all but the one deleted read again");
}

/// One of Ana's voice messages, 14 seconds long, and its waveform as the phones send one.
fn voice(n: i64) -> e2e::Message {
    e2e::Message {
        audio_message: Some(Box::new(e2e::AudioMessage {
            ptt: Some(true),
            seconds: Some(14),
            mimetype: Some("audio/ogg; codecs=opus".into()),
            waveform: Some((0..64).map(|i| ((i * 7 + n) % 101) as u8).collect()),
            ..Default::default()
        })),
        ..Default::default()
    }
}

/// A jaw.db at the version before a voice message's row carried its length and waveform, its feed at
/// seq 1,000, with `count` of Ana's messages as the converter wrote them then: every fifth a voice
/// message, its detail its file's alone, one deleted for everyone, and text otherwise.
fn before_voices(home: &std::path::Path, count: i64) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    let voices = schema::MIGRATIONS.iter().position(|m| m.contains("SELECT 'voices'")).expect("the voices migration");
    for migration in &schema::MIGRATIONS[..voices] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", voices as i64).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 1);
        INSERT INTO events (seq, at, type, key, chat) VALUES (1000, {AT}, 'chat', 1, 1);"
    ))
    .unwrap();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, deleted, v)
            VALUES (?1, 1, 'm' || ?1, ?2, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
        )
        .unwrap();
    for n in 1..=count {
        let at = AT + n;
        let row: Row = if n == 10 {
            ("voice", None, Some("This message was deleted".into()), None, None, true)
        } else if n % 5 == 0 {
            let detail = json!({ "media": { "mime": "audio/ogg; codecs=opus", "seconds": 14 } });
            ("voice", None, Some("Voice message, 0:14".into()), Some(detail.to_string()), Some(raw(n, voice(n))), false)
        } else {
            let text = e2e::Message { conversation: Some(format!("olá {n}")), ..Default::default() };
            ("text", Some(format!("olá {n}")), None, None, Some(raw(n, text)), false)
        };
        let (kind, text, summary, detail, raw, deleted) = row;
        insert.execute(rusqlite::params![n, at, kind, text, summary, detail, raw, deleted]).unwrap();
    }
}

async fn voices_left(store: &Store) -> Option<(i64, i64)> {
    query(store, |db| {
        Ok(db
            .query_row("SELECT after, upto FROM backfills WHERE name = 'voices'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })
    .await
}

/// Takes the voices backfill's steps until it's done, as `backfilled` takes the locations'.
async fn voices_backfilled(store: &Store) {
    let mut steps = store.backfill_steps(links::VOICES);
    while voices_left(store).await.is_some() {
        steps.take("the backfill never ended");
        let applied = store.apply(Lane::Background, vec![Change::Backfill(links::VOICES)]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step failed: {applied:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_voice_messages_held_before_take_their_length_and_waveform_a_step_at_a_time_and_none_twice() {
    let home = tempfile::tempdir().unwrap();
    before_voices(home.path(), 150);
    // A voice message whose raw protobuf reads now as another kind keeps the row it has.
    let text = e2e::Message { conversation: Some("olá 40".into()), ..Default::default() };
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE messages SET raw = ?1 WHERE n = 40", [raw(40, text)]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    assert_eq!(voices_left(&store).await, Some((0, 150)), "the migration leaves the backfill to do");
    assert_eq!(store.backfill_left(links::VOICES).unwrap(), 150, "as jawd status says");
    let before = rows(&store).await;
    assert!(before[4].4.as_ref().unwrap().get("voice").is_none(), "a voice message from before has no voice");
    let since = last_event(&store).await;

    store.apply(Lane::Background, vec![Change::Backfill(links::VOICES)]).await.unwrap();
    let (after, _) = voices_left(&store).await.expect("more to look at");
    assert!((5..=64).contains(&after), "a step looks at 64 messages at most: {after}");
    let first = rows(&store).await;
    let waveform: Vec<u8> = (0..64).map(|i| ((i * 7 + 5) % 101) as u8).collect();
    assert_eq!(first[4].4.as_ref().unwrap()["voice"], json!({ "seconds": 14, "waveform": waveform }));
    assert_eq!(first[4].4.as_ref().unwrap()["media"]["seconds"], 14, "its file's detail as it was");
    assert!(first[4].1 > before[4].1, "at a new version, so windows show it again");
    assert_eq!(first[0], before[0], "a text message isn't touched");
    assert_eq!(first[144], before[144], "and a voice message the step hasn't reached is as it was");

    voices_backfilled(&store).await;
    let after = rows(&store).await;
    for (n, (now, was)) in after.iter().zip(&before).enumerate() {
        let n = n as i64 + 1;
        let read_again = n != 10 && n != 40 && n % 5 == 0;
        assert_eq!(now.1 > was.1, read_again, "m{n}: {now:?}");
        if read_again {
            assert_eq!(now.4.as_ref().unwrap()["voice"]["seconds"], 14, "m{n}");
        }
    }
    assert_eq!(after[9], before[9], "the one deleted for everyone has nothing to read again");
    assert_eq!(after[39], before[39], "and one whose raw reads as another kind is left as it is");
    let again: Vec<i64> = (1..=150).filter(|n| n % 5 == 0 && *n != 10 && *n != 40).collect();
    assert_eq!(told_since(&store, since).await, again, "each voice message read again is told, once");
    let window = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let shown = window.iter().find(|r| r.id == "m150").unwrap();
    let detail: Value = serde_json::from_str(shown.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["voice"]["waveform"].as_array().map(Vec::len), Some(64), "a window shows its waveform");

    // Taken again, from the start, it finds nothing more to do.
    let (seen, events_before) = (rows(&store).await, events(&store).await);
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) VALUES ('voices', 0, 150)", []).unwrap();
    voices_backfilled(&store).await;
    assert_eq!(rows(&store).await, seen, "nothing read twice");
    assert_eq!(events(&store).await, events_before, "and nothing told again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voice_message_whose_reading_again_panics_is_kept_in_quarantine_and_the_rest_are_read_again() {
    let home = tempfile::tempdir().unwrap();
    before_voices(home.path(), 30);
    let panics = WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(false),
            id: Some(kinds::tests::PANICS.into()),
            participant: None,
        },
        message: Some(voice(5)),
        message_timestamp: Some(((AT + 5) / 1000) as u64),
        ..Default::default()
    };
    let raw_kept = Packed::new(&panics.encode_to_vec()).0;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute("UPDATE messages SET raw = ?1 WHERE n = 5", [&raw_kept]).unwrap();
    drop(db);
    let store = Store::open(home.path()).unwrap();
    let before = rows(&store).await;
    voices_backfilled(&store).await;
    let after = rows(&store).await;
    assert_eq!(after[4], before[4], "the one whose reading panicked is as it was");
    assert_eq!(after[29].4.as_ref().unwrap()["voice"]["seconds"], 14, "and the rest read again");
    let kept: Vec<(String, Option<Vec<u8>>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, raw FROM quarantine WHERE source = 'voices'")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(kept, [(format!("{ANA} m5"), Some(raw_kept))], "kept in quarantine with its raw protobuf");
}
