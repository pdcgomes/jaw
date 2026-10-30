use rusqlite::{Connection, params};
use serde_json::{Value, json};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::*;
use crate::store::{Applied, Lane, Packed, Store, schema};
use crate::work::Priority;

const ANA: &str = "351910000002@s.whatsapp.net";
const ME: &str = "351910000001@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;
const MARTA: &str =
    "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Marta Lopes\r\nTEL;waid=351213456789:+351 21 345 6789\r\nEND:VCARD";
const RUI: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Rui Costa\r\nTEL;waid=351913333333:+351913333333\r\nEND:VCARD";
/// The ID of a card whose conversion panics, as `backfilled` does for it in tests.
pub(super) const PANICS: &str = "3EB0PANICS";
/// The ID of a card whose conversion fails once it has written, as `backfilled` does for it in tests.
pub(super) const FAILS: &str = "3EB0FAILS";

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

fn card(name: Option<&str>, vcard: &str) -> e2e::ContactMessage {
    e2e::ContactMessage { display_name: name.map(str::to_string), vcard: Some(vcard.into()), ..Default::default() }
}

/// Ana's message `id`, as jawd keeps it raw.
fn from_ana(id: &str, message: e2e::Message) -> Vec<u8> {
    let info = WebMessageInfo {
        key: MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some(id.into()), participant: None },
        message: Some(message),
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    };
    Packed::new(&info.encode_to_vec()).0
}

fn one(id: &str, card: e2e::ContactMessage) -> Vec<u8> {
    from_ana(id, e2e::Message { contact_message: Some(Box::new(card)), ..Default::default() })
}

fn several(id: &str, cards: Vec<e2e::ContactMessage>) -> Vec<u8> {
    let array = e2e::ContactsArrayMessage { contacts: cards, ..Default::default() };
    from_ana(id, e2e::Message { contacts_array_message: Some(Box::new(array)), ..Default::default() })
}

/// A message row as an earlier jawd left it.
struct Earlier {
    id: &'static str,
    kind: &'static str,
    text: Option<&'static str>,
    summary: &'static str,
    detail: Value,
    raw: Vec<u8>,
    deleted: bool,
}

/// A jaw.db at version 25, before contact cards were read again, holding Ana's chat and `rows` in
/// it in order, the last its newest, none changed since it was made.
fn earlier(home: &std::path::Path, rows: &[Earlier]) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..25] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 25).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 0);
        INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');"
    ))
    .unwrap();
    for (i, row) in rows.iter().enumerate() {
        let n = i as i64 + 1;
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, deleted, v)
            VALUES (?1, 1, ?2, ?3, ?1, 1, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![n, row.id, AT + n, row.kind, row.text, row.summary, row.detail.to_string(), row.raw, row.deleted],
        )
        .unwrap();
    }
    let newest = rows.len() as i64;
    db.execute("UPDATE chats SET last_message = ?1, last_at = ?2 WHERE n = 1", params![newest, AT + newest]).unwrap();
}

async fn backfill(store: &Store) -> Applied {
    store.apply(Lane::Background, vec![Change::Backfill(CONTACTS)]).await.unwrap()
}

/// Each message as it shows now: its ID, text, summary, detail and version.
async fn shown(store: &Store) -> Vec<(String, Option<String>, Option<String>, Value, i64)> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT id, text, summary, detail, v FROM messages ORDER BY n")?
            .query_map([], |r| {
                let detail: Option<String> = r.get(3)?;
                let detail = detail.map_or(Value::Null, |d| serde_json::from_str(&d).unwrap());
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, detail, r.get(4)?))
            })?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn cards_an_earlier_jawd_left_are_read_again_into_one_cards_name_and_vcard_at_a_new_version_with_their_events() {
    let home = tempfile::tempdir().unwrap();
    let both = vec![card(Some("Marta"), MARTA), card(Some("Rui"), RUI)];
    let contacts = json!({ "contacts": [{ "name": "Marta", "vcard": MARTA }, { "name": "Rui", "vcard": RUI }] });
    earlier(
        home.path(),
        &[
            Earlier {
                id: "named",
                kind: "contact",
                text: Some("Marta"),
                summary: "Contact: Marta",
                detail: json!({ "contact": { "name": "Marta", "vcard": MARTA } }),
                raw: one("named", card(Some("Marta"), MARTA)),
                deleted: false,
            },
            Earlier {
                id: "nameless",
                kind: "contact",
                text: None,
                summary: "Contact",
                detail: json!({ "contact": { "name": null, "vcard": MARTA } }),
                raw: one("nameless", card(None, MARTA)),
                deleted: false,
            },
            Earlier {
                id: "deleted",
                kind: "contact",
                text: None,
                summary: "This message was deleted",
                detail: json!({}),
                raw: several("deleted", both.clone()),
                deleted: true,
            },
            Earlier {
                id: "garbled",
                kind: "contact",
                text: Some("Marta, Rui"),
                summary: "2 contacts",
                detail: contacts.clone(),
                raw: vec![1, 2, 3],
                deleted: false,
            },
            Earlier {
                id: "words",
                kind: "text",
                text: Some("olá"),
                summary: "olá",
                detail: json!({ "contacts": [] }),
                raw: several("words", both.clone()),
                deleted: false,
            },
            Earlier {
                id: "both",
                kind: "contact",
                text: Some("Marta, Rui"),
                summary: "2 contacts",
                detail: contacts.clone(),
                raw: several("both", both.clone()),
                deleted: false,
            },
        ],
    );
    let store = Store::open(home.path()).unwrap();
    assert!(store.backfill_left(CONTACTS).unwrap() > 0, "the migration leaves the backfill");
    let before = shown(&store).await;
    let mut changed = 0;
    while store.backfill_left(CONTACTS).unwrap() > 0 {
        let applied = backfill(&store).await;
        assert_eq!(applied.quarantined, 0, "{applied:?}");
        changed += applied.changed;
    }
    assert!(changed > 0);
    let after = shown(&store).await;
    let was = |id: &str| before.iter().find(|row| row.0 == id).cloned().unwrap();
    let now = |id: &str| after.iter().find(|row| row.0 == id).cloned().unwrap();
    for id in ["named", "deleted", "garbled", "words"] {
        assert_eq!(now(id), was(id), "{id} left as it was");
    }
    let (_, text, summary, detail, v) = now("nameless");
    assert_eq!((text.as_deref(), summary.as_deref()), (Some("Marta Lopes"), Some("Contact: Marta Lopes")));
    assert_eq!(detail, json!({ "contact": { "name": "Marta Lopes", "vcard": MARTA } }), "named as its vCard names it");
    assert!(v > 0, "at a new version");
    let (_, text, summary, detail, both_v) = now("both");
    assert_eq!((text.as_deref(), summary.as_deref()), (Some("Marta, Rui"), Some("2 contacts")));
    assert_eq!(detail, json!({ "contact": { "name": "Marta, Rui", "vcard": format!("{MARTA}\r\n{RUI}") } }));
    let (event, chat_v, chat_event): (Option<i64>, i64, Option<i64>) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT (SELECT max(seq) FROM events WHERE type = 'message' AND key = 6), v,
                (SELECT max(seq) FROM events WHERE type = 'chat' AND key = 1)
            FROM chats WHERE n = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .await;
    assert_eq!(event, Some(both_v), "each new version has its event, so windows and a watch from since see it");
    assert!(chat_v > 0 && chat_event == Some(chat_v), "and its chat, whose newest it is, too");
    store.indexed().await.unwrap();
    let found = query(&store, |db| crate::store::read::search(db, "lopes", None, None, 10)).await;
    assert_eq!(found.hits.len(), 1, "found by the name it has now");

    // Read again, nothing changes twice.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    db.execute("INSERT INTO backfills (name, after, upto) VALUES ('contacts', 0, 6)", []).unwrap();
    drop(db);
    while store.backfill_left(CONTACTS).unwrap() > 0 {
        assert_eq!(backfill(&store).await.changed, 0, "a card read again changes nothing");
    }
    assert_eq!(shown(&store).await, after);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_whose_conversion_fails_or_panics_is_kept_in_quarantine_and_the_rest_are_read_again() {
    let home = tempfile::tempdir().unwrap();
    let nameless = |id: &'static str| Earlier {
        id,
        kind: "contact",
        text: None,
        summary: "Contact",
        detail: json!({ "contact": { "name": null, "vcard": RUI } }),
        raw: one(id, card(None, RUI)),
        deleted: false,
    };
    let rows = [nameless("c1"), nameless(PANICS), nameless("c3"), nameless(FAILS), nameless("c5")];
    let raws: Vec<Vec<u8>> = rows.iter().map(|row| row.raw.clone()).collect();
    earlier(home.path(), &rows);
    let store = Store::open(home.path()).unwrap();
    while store.backfill_left(CONTACTS).unwrap() > 0 {
        assert_eq!(backfill(&store).await.quarantined, 0, "no step fails for one card");
    }
    let names: Vec<(String, Option<String>)> =
        shown(&store).await.into_iter().map(|(id, text, ..)| (id, text)).collect();
    let rui = Some("Rui Costa".to_string());
    assert_eq!(
        names,
        [
            ("c1".into(), rui.clone()),
            (PANICS.into(), None),
            ("c3".into(), rui.clone()),
            (FAILS.into(), None),
            ("c5".into(), rui),
        ],
        "each read again but the two that couldn't be, which are as they were"
    );
    let kept: Vec<(String, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, raw FROM quarantine WHERE source = 'contacts' ORDER BY ref")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(
        kept,
        [(format!("{ANA} {FAILS}"), raws[3].clone()), (format!("{ANA} {PANICS}"), raws[1].clone())],
        "each kept in quarantine with its raw protobuf"
    );
}
