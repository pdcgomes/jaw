use rusqlite::{Connection, params};
use serde_json::json;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::web_message_info::StubType;
use wa_proto::waai_common_deprecated::{AiRichResponseSubMessage, AiRichResponseSubMessageType};

use super::*;
use crate::store::{Applied, Lane, Store, schema};
use crate::work::{Priority, Report};

const GROUP: &str = "120363000000000001@g.us";
const COMMUNITY: &str = "120363000000000009@g.us";
const ANA: &str = "351910000002@s.whatsapp.net";
const RUI: &str = "100000000003@lid";
const ME: &str = "351910000001@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// What jawd keeps of message `id` in `chat`: from `from`, or yours without one.
fn web(chat: &str, id: &str, from: Option<&str>, message: Option<e2e::Message>) -> WebMessageInfo {
    WebMessageInfo {
        key: MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(from.is_none()),
            id: Some(id.into()),
            participant: from.filter(|_| chat.ends_with("@g.us")).map(Into::into),
        },
        message,
        message_timestamp: Some((AT / 1000) as u64),
        ..Default::default()
    }
}

fn stub(chat: &str, id: &str, from: &str, stub: StubType, parameters: &[&str]) -> WebMessageInfo {
    let mut info = web(chat, id, Some(from), None);
    info.message_stub_type = Some(stub as i32);
    info.message_stub_parameters = parameters.iter().map(|p| p.to_string()).collect();
    info
}

fn packed(info: &WebMessageInfo) -> Vec<u8> {
    Packed::new(&info.encode_to_vec()).0
}

/// `info` as a newer WhatsApp sends it, with a field this jawd's protobufs don't know, which
/// decoding it and encoding it again would drop.
fn packed_with_more(info: &WebMessageInfo) -> Vec<u8> {
    let mut raw = info.encode_to_vec();
    raw.extend([0xF8, 0xF0, 0x04, 0x01]);
    Packed::new(&raw).0
}

fn secret_edit(chat: &str, id: &str, from: Option<&str>) -> WebMessageInfo {
    let edit = e2e::SecretEncryptedMessage {
        target_message_key: Some(MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(true),
            id: Some("3EB0EVENT".into()),
            participant: None,
        }),
        enc_payload: Some(vec![9, 9, 9]),
        secret_enc_type: Some(e2e::secret_encrypted_message::SecretEncType::MessageEdit as i32),
        ..Default::default()
    };
    web(chat, id, from, Some(e2e::Message { secret_encrypted_message: Some(edit), ..Default::default() }))
}

/// Meta AI's edit `id` of its answer `target`, made at `at`, saying `said`.
fn answer_edit(id: &str, target: &str, at: i64, said: &str) -> WebMessageInfo {
    let answer = e2e::AiRichResponseMessage {
        submessages: vec![AiRichResponseSubMessage {
            message_type: Some(AiRichResponseSubMessageType::AiRichResponseText as i32),
            message_text: Some(said.into()),
            ..Default::default()
        }],
        ..Default::default()
    };
    let edit = e2e::ProtocolMessage {
        key: Some(MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(false),
            id: Some(target.into()),
            participant: None,
        }),
        r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
        edited_message: Some(Box::new(e2e::Message {
            rich_response_message: Some(Box::new(answer)),
            ..Default::default()
        })),
        timestamp_ms: Some(at),
        ..Default::default()
    };
    web(ANA, id, Some(ANA), Some(e2e::Message { protocol_message: Some(Box::new(edit)), ..Default::default() }))
}

/// A row as an earlier jawd left it: its n, chat, ID, sender's JID n, whether it's yours, kind,
/// text, summary, when it was last edited, whether it's deleted, and its raw protobuf.
type Left = (
    i64,
    i64,
    &'static str,
    Option<i64>,
    bool,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    Option<i64>,
    bool,
    Option<Vec<u8>>,
);

/// A jaw.db at version 15, before jawd knew community events and the other kinds it couldn't show,
/// holding the rows it made of them then, and others it showed.
fn before_kinds(home: &std::path::Path) -> Vec<Left> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..15] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 15).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0), (2, 'Rui', 0);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1), (2, '{RUI}', 2);
        INSERT INTO chats (n, id, kind, v) VALUES (1, '{GROUP}', 'group', 0), (2, '{COMMUNITY}', 'community', 0);
        INSERT INTO chats (n, id, kind, person, v) VALUES (3, '{ANA}', 'direct', 1, 0);
        INSERT INTO sync_state (key, value) VALUES ('me', '{ME} 100000000009@lid');"
    ))
    .unwrap();
    let unknown = Some(UNKNOWN);
    let template = e2e::TemplateMessage {
        hydrated_template: Some(Box::new(e2e::template_message::HydratedFourRowTemplate {
            hydrated_content_text: Some("Your order has shipped: https://jaw.example/track".into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let rows: Vec<Left> = vec![
        (
            1,
            2,
            "s1",
            Some(2),
            false,
            "system",
            None,
            unknown,
            None,
            false,
            Some(packed(&stub(COMMUNITY, "s1", RUI, StubType::CommunityLinkSubGroup, &[GROUP, "Festa de Natal"]))),
        ),
        (
            2,
            1,
            "t2",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed_with_more(&web(
                GROUP,
                "t2",
                Some(ANA),
                Some(e2e::Message { template_message: Some(Box::new(template)), ..Default::default() }),
            ))),
        ),
        (
            3,
            1,
            "e3",
            Some(2),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed(&secret_edit(GROUP, "e3", Some(RUI)))),
        ),
        (
            4,
            1,
            "e4",
            None,
            true,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed_with_more(&secret_edit(GROUP, "e4", None))),
        ),
        (
            5,
            3,
            "ai5",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            Some(AT + 5),
            false,
            Some(packed(&answer_edit("x5", "ai5", AT + 5, "Lisbon is the capital."))),
        ),
        (
            6,
            3,
            "ai6",
            Some(1),
            false,
            "unknown",
            Some("later words"),
            None,
            Some(AT + 9),
            false,
            Some(packed(&answer_edit("x6", "ai6", AT + 5, "first words"))),
        ),
        (
            7,
            1,
            "u7",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed(&web(
                GROUP,
                "u7",
                Some(ANA),
                Some(e2e::Message { request_location_message: Some(Default::default()), ..Default::default() }),
            ))),
        ),
        (8, 1, "b8", Some(1), false, "unknown", None, unknown, None, false, Some(Packed::new(b"not a protobuf").0)),
        (
            9,
            1,
            "t9",
            Some(1),
            false,
            "text",
            Some("olá"),
            None,
            None,
            false,
            Some(packed(&web(
                GROUP,
                "t9",
                Some(ANA),
                Some(e2e::Message { conversation: Some("olá".into()), ..Default::default() }),
            ))),
        ),
        (10, 1, "d10", Some(1), false, "unknown", None, Some("This message was deleted"), None, true, None),
        (
            11,
            1,
            "s11",
            Some(2),
            false,
            "system",
            None,
            unknown,
            None,
            false,
            Some(packed(&stub(GROUP, "s11", RUI, StubType::GroupChangeNoFrequentlyForwarded, &[]))),
        ),
        (
            12,
            3,
            "p12",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed(&web(
                ANA,
                "p12",
                Some(ANA),
                Some(e2e::Message { placeholder_message: Some(Default::default()), ..Default::default() }),
            ))),
        ),
        (
            13,
            1,
            "k13",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed(&web(
                GROUP,
                "k13",
                Some(ANA),
                Some(e2e::Message { sender_key_distribution_message: Some(Default::default()), ..Default::default() }),
            ))),
        ),
    ];
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, summary, detail, edited_at, deleted, raw, v)
            VALUES (?1, ?2, ?3, ?4, ?1, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)",
        )
        .unwrap();
    for (n, chat, id, sender, mine, kind, text, summary, edited_at, deleted, raw) in &rows {
        let detail = (*kind == "system").then(|| json!({ "system": summary }).to_string());
        insert
            .execute(params![n, chat, id, AT + n, sender, mine, kind, text, summary, detail, edited_at, deleted, raw])
            .unwrap();
    }
    db.execute_batch(
        "UPDATE chats SET last_message = 11, last_at = 1792000000011 WHERE n = 1;
        UPDATE chats SET last_message = 1, last_at = 1792000000001 WHERE n = 2;
        UPDATE chats SET last_message = 12, last_at = 1792000000012, unread = 3 WHERE n = 3;",
    )
    .unwrap();
    rows
}

/// Each message's n and what it shows: its kind, text, summary, and version.
async fn shown(store: &Store) -> Vec<(i64, String, Option<String>, Option<String>, i64)> {
    query(store, |db| {
        Ok(db
            .prepare("SELECT n, kind, text, summary, v FROM messages ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

async fn backfill(store: &Store) -> Applied {
    store.apply(Lane::Background, vec![Change::Backfill(KINDS)]).await.unwrap()
}

/// How many messages read as jaw can't show them: of kind unknown, and system lines saying so.
async fn unshown_count(store: &Store) -> (i64, i64) {
    query(store, |db| {
        Ok(db.query_row(
            "SELECT count(*) FILTER (WHERE kind = 'unknown'), count(*) FILTER (WHERE kind = 'system' AND summary = ?1)
            FROM messages WHERE NOT deleted",
            [UNKNOWN],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })
    .await
}

/// Measures the backfill, on a copy of a real jaw.db in `JAW_MEASURE_HOME` (opened with no engine,
/// so nothing else writes to it), or on one made of `JAW_MEASURE_MESSAGES` messages from before,
/// one in a thousand of a kind jawd couldn't show and one in a thousand a line it couldn't word:
/// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_kinds -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, a minute long at a million messages"]
async fn measure_the_kinds_backfill() {
    let made = tempfile::tempdir().unwrap();
    let home = match std::env::var_os("JAW_MEASURE_HOME") {
        Some(home) => std::path::PathBuf::from(home),
        None => {
            let messages: i64 =
                std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
            let db = Connection::open(made.path().join("jaw.db")).unwrap();
            db.pragma_update(None, "journal_mode", "WAL").unwrap();
            for migration in &schema::MIGRATIONS[..15] {
                db.execute_batch(migration).unwrap();
            }
            db.pragma_update(None, "user_version", 15).unwrap();
            db.execute_batch(&format!(
                "INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');
                BEGIN;
                WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
                INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
                INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
                INSERT INTO chats (n, id, kind, person, v) SELECT n, printf('3519%08d@s.whatsapp.net', n), 'direct', n, 1 FROM people;
                COMMIT;"
            ))
            .unwrap();
            db.execute(
                &format!(
                    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages})
                    INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, v)
                    SELECT x, 1 + x % 2000, printf('3EB0%016d', x), {AT} + x * 1000, ({AT} + x * 1000) << 20, 1 + x % 2000,
                        CASE x % 1000 WHEN 1 THEN 'unknown' WHEN 2 THEN 'system' ELSE 'text' END,
                        iif(x % 1000 IN (1, 2), NULL, 'mensagem ' || x),
                        iif(x % 1000 IN (1, 2), ?1, NULL),
                        iif(x % 1000 = 2, json_object('system', ?1), NULL),
                        randomblob(300), x
                    FROM c"
                ),
                params![UNKNOWN],
            )
            .unwrap();
            // Each of those keeps the raw protobuf it came as: a business's template from the
            // chat's person, or a group linked into a community.
            let template = e2e::TemplateMessage {
                hydrated_template: Some(Box::new(e2e::template_message::HydratedFourRowTemplate {
                    hydrated_content_text: Some("Your order has shipped".into()),
                    ..Default::default()
                })),
                ..Default::default()
            };
            let template = e2e::Message { template_message: Some(Box::new(template)), ..Default::default() };
            db.execute_batch("BEGIN").unwrap();
            let mut raw = db.prepare("UPDATE messages SET raw = ?2 WHERE n = ?1").unwrap();
            for x in (1..=messages).filter(|x| x % 1000 == 1 || x % 1000 == 2) {
                let (chat, id) = (format!("3519{:08}@s.whatsapp.net", 1 + x % 2000), format!("3EB0{x:016}"));
                let info = if x % 1000 == 1 {
                    web(&chat, &id, Some(&chat), Some(template.clone()))
                } else {
                    stub(&chat, &id, &chat, StubType::CommunityLinkSubGroup, &[GROUP, "Festa"])
                };
                raw.execute(params![x, packed(&info)]).unwrap();
            }
            drop(raw);
            db.execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
            made.path().to_path_buf()
        }
    };
    let began = std::time::Instant::now();
    let store = Store::open(&home).unwrap();
    let migrated = began.elapsed();
    let (messages, pending): (i64, i64) = query(&store, |db| {
        Ok(db.query_row("SELECT (SELECT count(*) FROM messages), (SELECT count(*) FROM pending)", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?)
    })
    .await;
    let before = unshown_count(&store).await;
    let began = std::time::Instant::now();
    let (mut transactions, mut changed) = (0, 0);
    while store.backfill_left(KINDS).unwrap() > 0 {
        let applied = store.apply(Lane::Background, vec![Change::Backfill(KINDS); 64]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step failed");
        (transactions, changed) = (transactions + applied.transactions, changed + applied.changed);
    }
    let backfilled = began.elapsed();
    let runs = store.report().run.unwrap();
    let after = unshown_count(&store).await;
    let waiting: i64 =
        query(&store, move |db| Ok(db.query_row("SELECT count(*) - ?1 FROM pending", [pending], |r| r.get(0))?)).await;
    eprintln!(
        "{messages} messages, {} of kind unknown and {} lines jawd couldn't word: migrated in {:.1} ms; backfilled in \
         {:.2} s ({:.0} messages a second), {changed} steps changing some, in {transactions} transactions, each {:.2} ms p50 and \
         {:.2} ms p99 with its commit; {} of kind unknown and {} such lines left, {waiting} more waiting in pending",
        before.0,
        before.1,
        migrated.as_secs_f64() * 1e3,
        backfilled.as_secs_f64(),
        messages as f64 / backfilled.as_secs_f64(),
        runs.p50_us as f64 / 1e3,
        runs.p99_us as f64 / 1e3,
        after.0,
        after.1,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_encrypted_edit_arriving_again_waits_once_and_two_from_the_same_second_both_wait() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let me = Me::new([ME.to_string()]);
    let kept = |id: &str, live: bool| -> Vec<Change> {
        convert::message(&secret_edit(GROUP, id, Some(RUI)), &me, live)
            .into_iter()
            .filter(|change| matches!(change, Change::Amend(_)))
            .collect()
    };
    for (changes, lane) in [
        (kept("e1", true), Lane::Live),
        (kept("e1", true), Lane::Live),
        (kept("e1", false), Lane::Background),
        (kept("e2", true), Lane::Live),
    ] {
        store.apply(lane, changes).await.unwrap();
    }
    assert_eq!(
        store.count("pending WHERE kind = 'secret' AND target = '3EB0EVENT'").unwrap(),
        2,
        "e1 waits once however often it comes, and e2, from the same second, beside it"
    );
}

#[test]
fn the_backfill_looks_for_the_summary_jawd_gives_what_it_cant_show() {
    assert!(NEXT_UNSHOWN.contains(&format!("'{}'", UNKNOWN.replace('\'', "''"))), "{NEXT_UNSHOWN}");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_jawd_couldnt_show_is_converted_again_from_its_raw_protobuf_and_amendments_go_to_wait() {
    let home = tempfile::tempdir().unwrap();
    let left = before_kinds(home.path());
    let store = Store::open(home.path()).unwrap();
    assert_eq!(store.backfill_left(KINDS).unwrap(), 13, "the migration leaves the backfill to do, as status says");
    let before = shown(&store).await;

    // A copy of Rui's encrypted edit converted since waits already, and isn't kept twice.
    let copy = convert::message(&secret_edit(GROUP, "e3", Some(RUI)), &Me::new([ME.to_string()]), true);
    store.apply(Lane::Live, copy.into_iter().filter(|c| matches!(c, Change::Amend(_))).collect()).await.unwrap();

    let applied = backfill(&store).await;
    assert!(applied.changed > 0 && applied.quarantined == 0, "{applied:?}");
    assert_eq!(store.backfill_left(KINDS).unwrap(), 0, "a step looks at more messages than these, and it's done");
    let after = shown(&store).await;
    let row = |n: i64| after.iter().find(|row| row.0 == n).cloned();
    let was = |n: i64| before.iter().find(|row| row.0 == n).cloned().unwrap();

    let line = "Group added to the community: Festa de Natal";
    assert_eq!(row(1).map(|r| (r.1, r.3)), Some(("system".into(), Some(line.into()))));
    let detail: String =
        query(&store, |db| Ok(db.query_row("SELECT detail FROM messages WHERE n = 1", [], |r| r.get(0))?)).await;
    assert_eq!(detail, json!({ "system": line }).to_string(), "the line its detail says, as interfaces draw it");
    let shipped = "Your order has shipped: https://jaw.example/track";
    assert_eq!(row(2).map(|r| (r.1, r.2, r.3)), Some(("text".into(), Some(shipped.into()), None)));
    assert_eq!(
        row(5).map(|r| (r.1, r.2)),
        Some(("text".into(), Some("Lisbon is the capital.".into()))),
        "what stands in for Meta AI's answer"
    );
    assert_eq!(
        row(6).map(|r| (r.1, r.2)),
        Some(("text".into(), Some("later words".into()))),
        "a later edit jaw could show stays"
    );
    assert_eq!(
        row(12).map(|r| (r.1, r.3)),
        Some(("system".into(), Some("Open WhatsApp on your phone to see this message".into())))
    );
    for n in [1, 2, 5, 6, 12] {
        assert!(row(n).unwrap().4 > was(n).4, "message {n} takes a new version");
    }
    // A row whose raw protobuf converts to nothing at all stays, so nothing it said is lost.
    for n in [7, 8, 9, 10, 11, 13] {
        assert_eq!(row(n), Some(was(n)), "message {n}, which still can't be shown or was shown, is as it was");
    }
    assert!(row(3).is_none() && row(4).is_none(), "encrypted edits are no rows");

    // Each converted row keeps its raw protobuf byte for byte.
    let raws: Vec<(i64, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT n, raw FROM messages WHERE raw IS NOT NULL ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    for (n, raw) in &raws {
        let kept = left.iter().find(|row| row.0 == *n).and_then(|row| row.10.clone());
        assert_eq!(Some(raw.clone()), kept, "message {n}'s raw protobuf");
    }

    // The edits wait for the message they change, with the raw protobuf each row had, Rui's once;
    // yours leaves a tombstone, so receipts for it don't wait for it.
    let waiting: Vec<(Option<String>, bool, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare(
                "SELECT sender, mine, raw FROM pending WHERE target = '3EB0EVENT' AND kind = 'secret' ORDER BY mine, n",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(
        waiting.iter().map(|w| (w.0.clone(), w.1)).collect::<Vec<_>>(),
        [(Some(RUI.into()), false), (Some(ME.into()), true)]
    );
    assert_eq!(waiting[1].2, left[3].10.clone().unwrap(), "yours waits with the raw protobuf its row had");
    let tombstones: Vec<String> = query(&store, |db| {
        Ok(db.prepare("SELECT id FROM tombstones")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(tombstones, ["e4"]);

    // The chats show what changed: the community's newest line, and Ana's unread counted again,
    // her placeholder now a line that isn't.
    let chats: Vec<(i64, i64, i64)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT n, v, unread FROM chats ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert!(chats[1].1 > 0, "the community's newest message changed, so it takes a new version");
    assert_eq!(chats[2].2, 2, "Ana's two answers count, her placeholder no longer");

    // Search finds what a template said, and its link is kept.
    store.indexed().await.unwrap();
    let found: Vec<String> = query(&store, |db| {
        Ok(db
            .prepare(
                "SELECT m.id FROM messages_fts f JOIN messages m ON m.t = f.rowid WHERE messages_fts MATCH 'shipped'",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(found, ["t2"]);
    let linked: Vec<String> = query(&store, |db| {
        Ok(db.prepare("SELECT url FROM links")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(linked, ["https://jaw.example/track"]);

    // Taken again from the start, it changes nothing.
    let settled = shown(&store).await;
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.execute("INSERT INTO backfills (name, after, upto) VALUES ('kinds', 0, 13)", []).unwrap();
    drop(raw);
    let again = backfill(&store).await;
    assert_eq!((again.changed, again.quarantined), (0, 0), "{again:?}");
    assert_eq!(shown(&store).await, settled);
    assert_eq!(store.count("pending WHERE kind = 'secret'").unwrap(), 2);
}
