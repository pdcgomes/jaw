use rusqlite::{Connection, params};
use serde_json::json;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_web::web_message_info::StubType;
use wa_proto::waai_common_deprecated::{AiRichResponseSubMessage, AiRichResponseSubMessageType};

use std::time::Duration;

use super::*;
use crate::store::{Applied, LIMITS, Lane, Limits, Store, schema};
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

const TUESDAY: &str = "It ships on _Tuesday_ now";
/// The ID of a message whose conversion panics, as `backfilled` does for it in tests.
pub(in crate::store::write) const PANICS: &str = "3EB0PANICS";
/// The ID of a message whose conversion fails once it has written, as `backfilled` does for it in
/// tests.
pub(super) const FAILS: &str = "3EB0FAILS";
/// The ID of a message whose conversion fills the disk once it has written, the first time only.
pub(super) const FILLS: &str = "3EB0FILLS";
pub(super) static FILLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A jaw.db at version 15 holding, in a group, Ana's templates with `ids`, each of kind unknown as
/// an earlier jawd left it, its n its place among them.
fn templates(home: &std::path::Path, ids: &[String]) -> Vec<Vec<u8>> {
    group_rows(home, &ids.iter().map(|id| monday(GROUP, id)).collect::<Vec<_>>())
}

/// A jaw.db at version 15 holding, in a group, Ana's `messages`, each of kind unknown as an earlier
/// jawd left it, its n its place among them; their raw protobufs.
fn group_rows(home: &std::path::Path, messages: &[WebMessageInfo]) -> Vec<Vec<u8>> {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..15] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 15).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, v) VALUES (1, '{GROUP}', 'group', 0);
        INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');"
    ))
    .unwrap();
    let mut raws = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        let (n, id, raw) = (i as i64 + 1, message.key.id.clone().unwrap(), packed(message));
        db.execute(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, raw, v)
            VALUES (?1, 1, ?2, ?3, ?1, 1, 'unknown', ?4, ?5, 0)",
            params![n, id, AT + n, UNKNOWN, raw],
        )
        .unwrap();
        raws.push(raw);
    }
    raws
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_meeting_many_to_convert_ends_once_it_has_converted_for_a_while_and_the_next_goes_on() {
    let home = tempfile::tempdir().unwrap();
    templates(home.path(), &(1..=64).map(|n| format!("t{n}")).collect::<Vec<_>>());
    let store = Store::open(home.path()).unwrap();
    backfill(&store).await;
    let (after, converted): (i64, i64) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT (SELECT after FROM backfills WHERE name = 'kinds'),
                (SELECT count(*) FROM messages WHERE kind = 'text'),
                (SELECT max(n) FROM messages WHERE kind = 'text')",
            [],
            |r| {
                assert_eq!(r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, "what it converted is the first in a row");
                Ok((r.get(0)?, r.get(1)?))
            },
        )?)
    })
    .await;
    assert!((1..64).contains(&converted), "a step converting 64 in a row ends once it has for a while: {converted}");
    assert_eq!(after, converted, "and the next goes on from the first it didn't convert");
    let mut steps = store.backfill_steps(KINDS);
    while store.backfill_left(KINDS).unwrap() > 0 {
        steps.take("the backfill never ended");
        backfill(&store).await;
    }
    assert_eq!(store.count("messages WHERE kind = 'text'").unwrap(), 64);
}

/// Meta AI's answer `id` in Ana's chat, saying `said`, quoting a question of yours, mentioning you,
/// disappearing in 90 days, forwarded twice and to be seen once.
fn answer(id: &str, said: &str) -> WebMessageInfo {
    let context = e2e::ContextInfo {
        stanza_id: Some("3EB0Q".into()),
        participant: Some(ME.into()),
        quoted_message: Some(Box::new(e2e::Message {
            conversation: Some("where is it?".into()),
            ..Default::default()
        })),
        mentioned_jid: vec![ME.into()],
        expiration: Some(7_776_000),
        is_forwarded: Some(true),
        forwarding_score: Some(2),
        ..Default::default()
    };
    let answer = e2e::AiRichResponseMessage {
        submessages: vec![AiRichResponseSubMessage {
            message_type: Some(AiRichResponseSubMessageType::AiRichResponseText as i32),
            message_text: Some(said.into()),
            ..Default::default()
        }],
        context_info: Some(Box::new(context)),
        ..Default::default()
    };
    let answer = e2e::Message { rich_response_message: Some(Box::new(answer)), ..Default::default() };
    let once = e2e::FutureProofMessage { message: Some(Box::new(answer)) };
    web(ANA, id, Some(ANA), Some(e2e::Message { view_once_message_v2: Some(Box::new(once)), ..Default::default() }))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_coming_onto_an_earlier_jawds_unreadable_stand_in_shows_and_keeps_what_it_brought() {
    let home = tempfile::tempdir().unwrap();
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..15] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 15).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 0);
            INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
            INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 0);
            INSERT INTO sync_state (key, value) VALUES ('me', '{ME}');"
        ))
        .unwrap();
        // Two of Meta AI's edits, each standing in for its answer, which an earlier jawd couldn't read.
        for (n, id) in [(1, "ai1"), (2, "ai2")] {
            let raw = packed(&answer_edit(&format!("x{n}"), id, AT + 5, "Lisbon is the capital."));
            db.execute(
                "INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, edited_at, raw, v)
                VALUES (?1, 1, ?2, ?3, ?1, 1, 'unknown', ?4, ?5, ?6, 0)",
                params![n, id, AT, UNKNOWN, AT + 5, raw],
            )
            .unwrap();
        }
    }
    let store = Store::open(home.path()).unwrap();
    // Before the backfill reaches them, the answers come: ai1 live, with the engine's envelope, so
    // its row keeps the edit's raw protobuf; ai2 by history, whose raw protobuf its row takes.
    let me = Me::new([ME.to_string()]);
    // The live one a few seconds after what history brings, which history's copy reads past.
    let mut ai1 = answer("ai1", "Lisbon is a city.");
    ai1.message_timestamp = Some((AT + 10_000) as u64 / 1000);
    let mut live = convert::message(&ai1, &me, true);
    for change in &mut live {
        if let Change::Message(message) = change {
            message.envelope = Some(Packed::new(b"the engine's event"));
        }
    }
    store.apply(Lane::Live, live).await.unwrap();
    store.apply(Lane::Background, convert::message(&answer("ai2", "Lisbon is a city."), &me, false)).await.unwrap();
    // What a copy brought: its quote, forwarding, mention of you, expiry and view-once, and when
    // the message was edited.
    type Brought = (Option<String>, i64, bool, Option<i64>, bool, Option<i64>);
    let read = |n: i64| {
        let store = store.clone();
        async move {
            query(&store, move |db| {
                Ok(db.query_row(
                    "SELECT kind, text, summary, reply ->> 'id', forwarded, mentions_me, expires_at, view_once, edited_at
                    FROM messages WHERE n = ?1",
                    [n],
                    |r| {
                        let brought: Brought = (r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?);
                        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, brought))
                    },
                )?)
            })
            .await
        }
    };
    let brought = |n: i64| -> Brought {
        let sent = if n == 1 { AT + 10_000 } else { AT };
        (Some("3EB0Q".to_string()), 2, true, Some(sent + 7_776_000_000), true, Some(AT + 5))
    };
    let mentions = |store: &Store| {
        let store = store.clone();
        async move {
            query(&store, |db| Ok(db.query_row("SELECT mentions FROM chats WHERE n = 1", [], |r| r.get::<_, i64>(0))?))
                .await
        }
    };
    for n in [1, 2] {
        let (kind, text, summary, has) = read(n).await;
        assert_eq!(
            (kind.as_str(), text, summary.as_deref()),
            ("text", None, Some(UNKNOWN)),
            "{n}, as the copy left it"
        );
        assert_eq!(has, brought(n), "{n} has what its copy brought");
    }
    let mentioned = mentions(&store).await;
    assert!(mentioned >= 1, "the live answer mentions you, after where history's copy read to");

    let mut steps = store.backfill_steps(KINDS);
    while store.backfill_left(KINDS).unwrap() > 0 {
        steps.take("the backfill never ended");
        backfill(&store).await;
    }
    for (n, said) in [(1, "Lisbon is the capital."), (2, "Lisbon is a city.")] {
        let (kind, text, summary, has) = read(n).await;
        assert_eq!((kind.as_str(), text.as_deref(), summary), ("text", Some(said), None), "{n} shows what jaw can now");
        assert_eq!(has, brought(n), "{n} keeps what its copy brought");
    }
    assert_eq!(mentions(&store).await, mentioned, "and its chat counts the mention still");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_rolled_back_leaves_its_chat_no_version_without_an_event() {
    let home = tempfile::tempdir().unwrap();
    // The chat's newest, a template whose conversion fails once it has written, then Ana's
    // placeholder, which counted as unread and now won't, so the chat is counted again.
    let placeholder = e2e::Message { placeholder_message: Some(Default::default()), ..Default::default() };
    group_rows(home.path(), &[monday(GROUP, FAILS), web(GROUP, "p2", Some(ANA), Some(placeholder))]);
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.execute_batch(
        "UPDATE messages SET at = 1792000000009 WHERE n = 1;
        UPDATE chats SET last_message = 1, last_at = 1792000000009, unread = 2 WHERE n = 1;",
    )
    .unwrap();
    drop(db);
    // A background transaction made long, so both go in one however busy the machine.
    let (batch_time, yield_after) = (Duration::from_millis(500), Duration::from_millis(250));
    let store = Store::open_with(home.path(), Limits { batch_time, yield_after, ..LIMITS }).unwrap();
    let applied = store.apply(Lane::Background, vec![Change::Backfill(KINDS); 4]).await.unwrap();
    assert_eq!(applied.transactions, 1, "both in one transaction, which counts the chat again at its end");
    assert_eq!(store.backfill_left(KINDS).unwrap(), 0);
    let (v, unread, event): (i64, i64, Option<i64>) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT v, unread, (SELECT max(seq) FROM events WHERE type = 'chat' AND key = 1) FROM chats WHERE n = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .await;
    assert_eq!(unread, 1, "her template still counts, her placeholder no longer");
    assert_eq!(event, Some(v), "the chat's version has its event, so windows learn its count");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disk_filling_as_a_message_converts_fails_its_step_which_is_tried_again_and_converts_it() {
    let home = tempfile::tempdir().unwrap();
    templates(home.path(), &["t1".to_string(), FILLS.to_string()]);
    let store = Store::open(home.path()).unwrap();
    let mut steps = store.backfill_steps(KINDS);
    while store.backfill_left(KINDS).unwrap() > 0 {
        steps.take("the backfill never ended");
        backfill(&store).await;
    }
    assert!(FILLED.load(std::sync::atomic::Ordering::Relaxed), "the disk filled once");
    assert_eq!(store.count("messages WHERE kind = 'text'").unwrap(), 2, "both converted, that one once it didn't");
    assert_eq!(store.count("quarantine").unwrap(), 0, "and nothing kept in quarantine for it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_whose_conversion_fails_or_panics_is_kept_in_quarantine_and_the_rest_are_converted() {
    let home = tempfile::tempdir().unwrap();
    let ids: Vec<String> = ["t1", PANICS, "t3", FAILS, "t5"].iter().map(|id| id.to_string()).collect();
    let raws = templates(home.path(), &ids);
    let store = Store::open(home.path()).unwrap();
    let mut steps = store.backfill_steps(KINDS);
    while store.backfill_left(KINDS).unwrap() > 0 {
        steps.take("the backfill never ended");
        assert_eq!(backfill(&store).await.quarantined, 0, "no step fails for one message");
    }
    let rows: Vec<(String, String, i64)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT id, kind, v FROM messages ORDER BY n")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    let kinds: Vec<(&str, &str)> = rows.iter().map(|(id, kind, _)| (id.as_str(), kind.as_str())).collect();
    assert_eq!(kinds, [("t1", "text"), (PANICS, "unknown"), ("t3", "text"), (FAILS, "unknown"), ("t5", "text")]);
    assert!(rows[1].2 == 0 && rows[3].2 == 0, "what failed, after writing too, is as it was");
    let events = store.count("events WHERE type = 'message' AND key IN (2, 4)").unwrap();
    assert_eq!(events, 0, "with no event");
    let kept: Vec<(String, Vec<u8>)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT ref, raw FROM quarantine WHERE source = 'kinds' ORDER BY ref")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(
        kept,
        [(format!("{GROUP} {FAILS}"), raws[3].clone()), (format!("{GROUP} {PANICS}"), raws[1].clone())],
        "each kept in quarantine with its raw protobuf"
    );
}

/// Ana's business template `id` in `chat`, which an earlier jawd couldn't show.
fn monday(chat: &str, id: &str) -> WebMessageInfo {
    template_saying(chat, id, MONDAY)
}

/// Ana's business template `id` in `chat`, saying `words`.
fn template_saying(chat: &str, id: &str, words: &str) -> WebMessageInfo {
    let template = e2e::TemplateMessage {
        hydrated_template: Some(Box::new(e2e::template_message::HydratedFourRowTemplate {
            hydrated_content_text: Some(words.into()),
            ..Default::default()
        })),
        ..Default::default()
    };
    web(chat, id, Some(ANA), Some(e2e::Message { template_message: Some(Box::new(template)), ..Default::default() }))
}

const MONDAY: &str = "Your order ships on *Monday*: https://jaw.example/monday";

/// The text, spans and summary, as a row keeps it, that converting `message` gives its row or what
/// stands in for it.
fn said(message: &WebMessageInfo) -> (Option<String>, Option<String>, Option<String>) {
    let changes = convert::message(message, &Me::new([ME.to_string()]), false);
    let shown = changes
        .iter()
        .find_map(|change| match change {
            Change::Message(message) => Some(message.clone()),
            Change::Edited { shown, .. } => Some(shown.as_ref().clone()),
            _ => None,
        })
        .unwrap();
    let summary = (shown.text.as_ref() != Some(&shown.summary)).then_some(shown.summary);
    (shown.text, shown.spans, summary)
}

/// Ana's edit, made at `at`, of her message `target` in `chat`, to say `TUESDAY`.
fn tuesday(chat: &str, target: &str, at: i64) -> WebMessageInfo {
    edited_to(chat, target, at, TUESDAY)
}

/// An edit's words that sum up to the sentence jawd says of what it can't show.
const SAYS: &str = "*jaw* can't show this yet";

/// Ana's edit, made at `at`, of her message `target` in `chat`, to say `words`.
fn edited_to(chat: &str, target: &str, at: i64, words: &str) -> WebMessageInfo {
    let edit = e2e::ProtocolMessage {
        key: Some(MessageKey {
            remote_jid: Some(chat.into()),
            from_me: Some(true),
            id: Some(target.into()),
            participant: None,
        }),
        r#type: Some(e2e::protocol_message::Type::MessageEdit as i32),
        edited_message: Some(Box::new(e2e::Message { conversation: Some(words.into()), ..Default::default() })),
        timestamp_ms: Some(at),
        ..Default::default()
    };
    web(
        chat,
        &format!("{target}-edit"),
        Some(ANA),
        Some(e2e::Message { protocol_message: Some(Box::new(edit)), ..Default::default() }),
    )
}

/// Rui's comment on an announcement, a poll.
fn poll_comment(id: &str) -> WebMessageInfo {
    let option =
        |name: &str| e2e::poll_creation_message::Option { option_name: Some(name.into()), ..Default::default() };
    let poll = e2e::PollCreationMessage {
        name: Some("Jantar?".into()),
        options: vec![option("Ramen"), option("Pizza")],
        selectable_options_count: Some(1),
        ..Default::default()
    };
    let comment = e2e::CommentMessage {
        message: Some(Box::new(e2e::Message { poll_creation_message: Some(Box::new(poll)), ..Default::default() })),
        target_message_key: Some(MessageKey {
            remote_jid: Some(GROUP.into()),
            from_me: Some(false),
            id: Some("3EB0NEWS".into()),
            participant: Some(ANA.into()),
        }),
    };
    web(GROUP, id, Some(RUI), Some(e2e::Message { comment_message: Some(Box::new(comment)), ..Default::default() }))
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
        // Ana's template, then her edit of it, the other way round: the edit first standing in
        // for it, and the template keeping the edit's words when history brought it.
        (
            14,
            1,
            "t14",
            Some(1),
            false,
            "unknown",
            Some(TUESDAY),
            None,
            Some(AT + 50),
            false,
            Some(packed(&monday(GROUP, "t14"))),
        ),
        // The same template, which the test edits through this jawd's own writer.
        (15, 1, "t15", Some(1), false, "unknown", None, unknown, None, false, Some(packed(&monday(GROUP, "t15")))),
        (16, 1, "c16", Some(2), false, "unknown", None, unknown, None, false, Some(packed(&poll_comment("c16")))),
        // Ana's template, then an edit of it jaw couldn't read, which left nothing to show.
        (
            17,
            1,
            "t17",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            Some(AT + 70),
            false,
            Some(packed(&monday(GROUP, "t17"))),
        ),
        // Ana's template, and a message jaw showed, each edited later to words that sum up to the
        // sentence jawd says of what it can't show; their spans and summary are set below.
        (
            18,
            1,
            "t18",
            Some(1),
            false,
            "unknown",
            Some(SAYS),
            None,
            Some(AT + 80),
            false,
            Some(packed(&monday(GROUP, "t18"))),
        ),
        (
            19,
            1,
            "t19",
            Some(1),
            false,
            "text",
            Some(SAYS),
            None,
            Some(AT + 90),
            false,
            Some(packed(&web(
                GROUP,
                "t19",
                Some(ANA),
                Some(e2e::Message { conversation: Some("olá de novo".into()), ..Default::default() }),
            ))),
        ),
        // A template whose own words sum up to that sentence.
        (
            20,
            1,
            "t20",
            Some(1),
            false,
            "unknown",
            None,
            unknown,
            None,
            false,
            Some(packed(&template_saying(GROUP, "t20", SAYS))),
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
    // Rows 14, 18 and 19 read as their edits do, spans and summary too.
    for (n, edit) in [
        (14, tuesday(GROUP, "t14", AT + 50)),
        (18, edited_to(GROUP, "t18", AT + 80, SAYS)),
        (19, edited_to(GROUP, "t19", AT + 90, SAYS)),
    ] {
        let (_, spans, summary) = said(&edit);
        db.execute("UPDATE messages SET spans = ?1, summary = ?2 WHERE n = ?3", params![spans, summary, n]).unwrap();
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

/// How many messages read as jaw can't show them: of kind unknown, and of any other kind saying so
/// with no text, system lines jawd couldn't word among them, as the backfill takes them.
async fn unshown_count(store: &Store) -> (i64, i64) {
    query(store, |db| {
        Ok(db.query_row(
            "SELECT count(*) FILTER (WHERE kind = 'unknown'),
                count(*) FILTER (WHERE kind <> 'unknown' AND text IS NULL AND summary = ?1)
            FROM messages WHERE NOT deleted",
            [UNKNOWN],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })
    .await
}

/// Measures the backfill, on a copy of a real jaw.db in `JAW_MEASURE_HOME` (opened with no engine,
/// so nothing else writes to it), or on one made of `JAW_MEASURE_MESSAGES` messages from before,
/// one in a thousand of a kind jawd couldn't show and one in a thousand a line it couldn't word, or
/// with `JAW_MEASURE_DENSE` set, that many first in a row, a business's templates with two links,
/// each step then timed in a transaction of its own:
/// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_kinds -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, a minute long at a million messages"]
async fn measure_the_kinds_backfill() {
    let made = tempfile::tempdir().unwrap();
    let dense: i64 = std::env::var("JAW_MEASURE_DENSE").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
    let home = match std::env::var_os("JAW_MEASURE_HOME") {
        Some(home) => std::path::PathBuf::from(home),
        None => {
            let messages: i64 =
                std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
            let special = |x: i64| if dense > 0 { x <= dense } else { x % 1000 == 1 || x % 1000 == 2 };
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
                        CASE WHEN x <= ?2 OR ?2 = 0 AND x % 1000 = 1 THEN 'unknown'
                            WHEN ?2 = 0 AND x % 1000 = 2 THEN 'system' ELSE 'text' END,
                        iif(x <= ?2 OR ?2 = 0 AND x % 1000 IN (1, 2), NULL, 'mensagem ' || x),
                        iif(x <= ?2 OR ?2 = 0 AND x % 1000 IN (1, 2), ?1, NULL),
                        iif(?2 = 0 AND x % 1000 = 2, json_object('system', ?1), NULL),
                        randomblob(300), x
                    FROM c"
                ),
                params![UNKNOWN, dense],
            )
            .unwrap();
            // Each of those keeps the raw protobuf it came as: a business's template from the
            // chat's person, or a group linked into a community.
            let said = if dense > 0 {
                "Your order has shipped: https://jaw.example/track, and https://jaw.example/help if it hasn't come"
            } else {
                "Your order has shipped"
            };
            let template = e2e::TemplateMessage {
                hydrated_template: Some(Box::new(e2e::template_message::HydratedFourRowTemplate {
                    hydrated_content_text: Some(said.into()),
                    ..Default::default()
                })),
                ..Default::default()
            };
            let template = e2e::Message { template_message: Some(Box::new(template)), ..Default::default() };
            db.execute_batch("BEGIN").unwrap();
            let mut raw = db.prepare("UPDATE messages SET raw = ?2 WHERE n = ?1").unwrap();
            for x in (1..=messages).filter(|&x| special(x)) {
                let (chat, id) = (format!("3519{:08}@s.whatsapp.net", 1 + x % 2000), format!("3EB0{x:016}"));
                let info = if dense > 0 || x % 1000 == 1 {
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
    let steps = if dense > 0 { 1 } else { 64 };
    while store.backfill_left(KINDS).unwrap() > 0 {
        let applied = store.apply(Lane::Background, vec![Change::Backfill(KINDS); steps]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step failed");
        (transactions, changed) = (transactions + applied.transactions, changed + applied.changed);
    }
    let backfilled = began.elapsed();
    let runs = store.report().run.unwrap();
    let after = unshown_count(&store).await;
    let waiting: i64 =
        query(&store, move |db| Ok(db.query_row("SELECT count(*) - ?1 FROM pending", [pending], |r| r.get(0))?)).await;
    eprintln!(
        "{messages} messages, {} of kind unknown and {} of other kinds saying so: migrated in {:.1} ms; backfilled in \
         {:.2} s ({:.0} messages a second), {changed} steps changing some, in {transactions} transactions, each {:.2} ms p50 and \
         {:.2} ms p99 with its commit (the writer's last 1,024 transactions, not its steps); {} of kind unknown and \
         {} of others saying so left, {waiting} more waiting in pending",
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
    assert_eq!(store.backfill_left(KINDS).unwrap(), 20, "the migration leaves the backfill to do, as status says");

    // A copy of Rui's encrypted edit converted since waits already, and isn't kept twice.
    let me = Me::new([ME.to_string()]);
    let copy = convert::message(&secret_edit(GROUP, "e3", Some(RUI)), &me, true);
    store.apply(Lane::Live, copy.into_iter().filter(|c| matches!(c, Change::Amend(_))).collect()).await.unwrap();
    // Ana edits her template t15 through this jawd before the backfill reaches it.
    store.apply(Lane::Live, convert::message(&tuesday(GROUP, "t15", AT + 60), &me, true)).await.unwrap();
    let edited: (String, Option<String>, Option<i64>) = query(&store, |db| {
        Ok(db.query_row("SELECT kind, text, edited_at FROM messages WHERE n = 15", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?)
    })
    .await;
    assert_eq!(
        edited,
        ("unknown".into(), Some(TUESDAY.into()), Some(AT + 60)),
        "the edit lands, on a row jaw can't show"
    );
    let before = shown(&store).await;

    let mut changed = 0;
    let mut steps = store.backfill_steps(KINDS);
    while store.backfill_left(KINDS).unwrap() > 0 {
        steps.take("the backfill never ended");
        let applied = backfill(&store).await;
        assert_eq!(applied.quarantined, 0, "{applied:?}");
        changed += applied.changed;
    }
    assert!(changed > 0);
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
    // A template edited after it came keeps its edit's words, spans and summary, whichever way
    // round they came, and none of the template's links, as does one whose edit's words sum up to
    // the sentence jawd says of what it can't show; one edited by an edit jaw couldn't read, which
    // left nothing to keep, reads as the template does, its links and all. Each keeps when it was
    // edited, if it was. A template whose own words sum up to that sentence is text saying them.
    let edit = said(&tuesday(GROUP, "t15", AT + 60));
    let template = said(&monday(GROUP, "t17"));
    let says = said(&edited_to(GROUP, "t18", AT + 80, SAYS));
    let saying = said(&template_saying(GROUP, "t20", SAYS));
    assert!(edit.1.is_some() && edit.2.is_some() && template.1.is_some() && template.2.is_some());
    assert_eq!((says.0.as_deref(), says.2.as_deref()), (Some(SAYS), Some(UNKNOWN)), "its words sum up to the sentence");
    assert_eq!(saying, says, "the template's own words, alike");
    for (n, words, edited_at) in [
        (14, &edit, Some(AT + 50)),
        (15, &edit, Some(AT + 60)),
        (17, &template, Some(AT + 70)),
        (18, &says, Some(AT + 80)),
        (20, &saying, None),
    ] {
        let kept: (String, Option<String>, Option<String>, Option<String>, Option<i64>) = query(&store, move |db| {
            Ok(db.query_row("SELECT kind, text, spans, summary, edited_at FROM messages WHERE n = ?1", [n], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?)
        })
        .await;
        let expected = ("text".to_string(), words.0.clone(), words.1.clone(), words.2.clone(), edited_at);
        assert_eq!(kept, expected, "message {n}");
    }
    assert_eq!(row(16).map(|r| r.1), Some("poll".into()), "a comment that's a poll is a poll");
    assert_eq!(store.count("poll_options WHERE message = 16").unwrap(), 2, "with its options");
    for n in [1, 2, 5, 6, 12, 14, 15, 16, 17, 18, 20] {
        assert!(row(n).unwrap().4 > was(n).4, "message {n} takes a new version");
    }

    // Each converted row's event, at its new version, names its chat, so windows on it show it; so
    // does each removed row's, so they let it go, and the chat's whose newest it was.
    let event = |kind: &'static str, key: i64| {
        let store = store.clone();
        async move {
            query(&store, move |db| {
                Ok(db.query_row(
                    "SELECT max(seq), chat FROM events WHERE type = ?1 AND key = ?2",
                    params![kind, key],
                    |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
                )?)
            })
            .await
        }
    };
    let chat_of = |n: i64| left.iter().find(|row| row.0 == n).map(|row| row.1);
    for n in [1, 2, 5, 6, 12, 14, 15, 16, 17, 18, 20] {
        assert_eq!(event("message", n).await, (Some(row(n).unwrap().4), chat_of(n)), "message {n}'s event");
    }
    for n in [3, 4] {
        assert_eq!(event("message", n).await.1, Some(1), "message {n}, gone, has its event");
    }
    // A row whose raw protobuf converts to nothing at all stays, so nothing it said is lost.
    // Nor is one jaw shows whose later edit's words sum up to the sentence (19) taken at all.
    for n in [7, 8, 9, 10, 11, 13, 19] {
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
    for (chat, v) in [(2, chats[1].1), (3, chats[2].1)] {
        assert_eq!(event("chat", chat).await, (Some(v), Some(chat)), "chat {chat}'s event");
    }

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
        Ok(db
            .prepare(
                "SELECT m.id || ' ' || l.url FROM links l JOIN messages m ON m.n = l.message ORDER BY m.n, l.place",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(
        linked,
        ["t2 https://jaw.example/track", "t17 https://jaw.example/monday"],
        "the templates' links, not those of t14's and t15's, which read as their edits do"
    );

    // Taken again from the start, it changes nothing.
    let settled = shown(&store).await;
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.execute("INSERT INTO backfills (name, after, upto) VALUES ('kinds', 0, 20)", []).unwrap();
    drop(raw);
    let again = backfill(&store).await;
    assert_eq!((again.changed, again.quarantined), (0, 0), "{again:?}");
    assert_eq!(shown(&store).await, settled);
    assert_eq!(store.count("pending WHERE kind = 'secret'").unwrap(), 2);
}

/// A photo Ana shared in the group's status, with its file's keys and size, which an earlier jawd
/// couldn't show.
fn status_photo(id: &str) -> WebMessageInfo {
    let photo = e2e::ImageMessage {
        mimetype: Some("image/jpeg".into()),
        file_sha256: Some(vec![7; 32]),
        file_enc_sha256: Some(vec![8; 32]),
        media_key: Some(vec![9; 32]),
        direct_path: Some("/v/t62.7118-24/status".into()),
        file_length: Some(2_048),
        width: Some(640),
        height: Some(480),
        ..Default::default()
    };
    let shared = e2e::FutureProofMessage {
        message: Some(Box::new(e2e::Message { image_message: Some(Box::new(photo)), ..Default::default() })),
    };
    web(GROUP, id, Some(ANA), Some(e2e::Message { group_status_message: Some(Box::new(shared)), ..Default::default() }))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_photo_converted_again_keeps_its_files_copy_as_one_arriving_would() {
    let home = tempfile::tempdir().unwrap();
    group_rows(home.path(), &[status_photo("3EB0STATUS")]);
    let store = Store::open(home.path()).unwrap();
    while store.backfill_left(KINDS).unwrap() > 0 {
        backfill(&store).await;
    }
    let copy: String = query(&store, |db| {
        Ok(db.query_row(
            "SELECT json_object('kind', m.kind, 'keys', md.keys, 'sha256', hex(md.sha256), 'path', md.direct_path,
                'bytes', md.bytes, 'w', md.w, 'h', md.h, 'wanted', md.wanted, 'arrived', md.arrived)
            FROM messages m LEFT JOIN media md ON md.message = m.n AND md.variant = 'sd' WHERE m.id = '3EB0STATUS'",
            [],
            |r| r.get(0),
        )?)
    })
    .await;
    let copy: serde_json::Value = serde_json::from_str(&copy).unwrap();
    assert_eq!(
        copy,
        json!({
            "kind": "image", "keys": "image", "sha256": "07".repeat(32), "path": "/v/t62.7118-24/status",
            "bytes": 2_048, "w": 640, "h": 480, "wanted": null, "arrived": 0,
        }),
        "its copy, as a photo from history brings one: wanted once it's shown or asked for"
    );
}
