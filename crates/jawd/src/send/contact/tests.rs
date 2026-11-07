use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jaw_proto::{ConnectionState, Push, code};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, Replies, World};
use crate::engine::pb;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::send::{Makers, Sending, Timing, link};
use crate::store::{Chat, Lane, Store, read};
use crate::work::Priority;

const ANA: &str = "351912345678@s.whatsapp.net";
const CLIMBING: &str = "120363012345678901@g.us";
/// Two cards as the Contacts app exports them, each ending its last line.
const MARTA: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Lopes;Marta;;;\r\nFN:Marta Lopes\r\n\
    TEL;TYPE=CELL;waid=351213456789:+351 21 345 6789\r\nEND:VCARD\r\n";
const RUI: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Costa;Rui;;;\r\nFN:Rui Costa\r\n\
    TEL;TYPE=CELL;waid=351913333333:+351 913 333 333\r\nEND:VCARD\r\n";

/// How long a test's own connection to jaw.db waits for the write lock beside a running jawd,
/// whose writer, on a busy 2-core machine, can keep it more than 5 s. Its transactions take the
/// lock as they begin, before any read: a deferred one's first insert has met a snapshot a commit
/// of jawd's made stale (SQLITE_BUSY_SNAPSHOT), which no wait cures.
const SIDE_WAITS: Duration = Duration::from_secs(60);

/// jawd's sending on a jaw.db in `home`, through the demo engine, with Ana's chat and the climbing
/// group's.
struct Running {
    store: Store,
    sending: Sending,
    world: Arc<World>,
    pushes: broadcast::Sender<Push>,
    _attention: Arc<crate::attention::Attention>,
}

impl Running {
    /// Connected to WhatsApp, or, with `connect` false, not until the test says so.
    async fn start(home: &Path, connect: bool) -> Running {
        let store = Store::open(home).unwrap();
        store.apply(Lane::Live, vec![Change::Remember { key: "me".into(), value: demo::ME.into() }]).await.unwrap();
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let (pushes, _) = broadcast::channel(256);
        let (events, incoming) = mpsc::channel(1024);
        let world: Arc<World> = Arc::default();
        world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
        let script = if connect { vec![demo::connected()] } else { Vec::new() };
        let engine = DemoEngine::sharing(script, world.clone());
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let mut connected = pushes.subscribe();
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live.clone(),
            pushes.clone(),
            Some(supervisor.clone()),
            home.into(),
            Default::default(),
            crate::feed::Typing::default(),
        );
        tokio::spawn(sync);
        let attention = Arc::new(crate::attention::Attention::default());
        let timing = Timing {
            retry_first: Duration::from_millis(5),
            retry_most: Duration::from_millis(20),
            ..Timing::default()
        };
        let makers = Makers { previews: link::Previews::off(), media: None };
        let sending = Sending::start(store.clone(), Some(supervisor), live, &pushes, attention.focus(), timing, makers)
            .await
            .unwrap();
        if connect {
            let online = async {
                while let Ok(push) = connected.recv().await {
                    if push.method == "connection" && push.params["state"] == "online" {
                        return;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
        }
        let chats = [ANA, CLIMBING].map(|id| Change::Chat(Chat { id: id.into(), ..Chat::default() }));
        store.apply(Lane::Live, chats.into()).await.unwrap();
        Running { store, sending, world, pushes, _attention: attention }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.sending.call(method, params).await
    }

    /// Sends `cards`, each a name and a vCard, to `chat`, and says the message's ID.
    async fn cards(&self, chat: &str, cards: &[(&str, &str)]) -> String {
        let cards: Vec<Value> = cards.iter().map(|(name, vcard)| json!({ "name": name, "vcard": vcard })).collect();
        let sent = self.call("send.contact", json!({ "chat": chat, "cards": cards })).await.unwrap();
        sent["message"].as_str().unwrap().to_string()
    }

    async fn query<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> T {
        self.store.reads().query(Priority::Interactive, query).await.unwrap()
    }

    /// Waits, a commit at a time, until `done` holds of jaw.db.
    async fn until(&self, what: &str, done: impl Fn(&Connection) -> anyhow::Result<bool> + Clone + Send + 'static) {
        let mut committed = self.store.committed();
        let waited = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let done = done.clone();
                if self.query(move |db| done(db)).await {
                    return;
                }
                committed.changed().await.unwrap();
            }
        });
        waited.await.unwrap_or_else(|_| panic!("{what} never happened"));
    }

    /// Waits until `holds` says so of jaw.db, looking every 10 ms for 30 s at most, for what a commit
    /// that writes no event changes, as the outbox's rows.
    async fn eventually(
        &self,
        what: &str,
        holds: impl Fn(&Connection) -> anyhow::Result<bool> + Clone + Send + 'static,
    ) {
        let began = std::time::Instant::now();
        loop {
            let holds = holds.clone();
            if self.query(move |db| holds(db)).await {
                return;
            }
            assert!(began.elapsed() < Duration::from_secs(30), "never: {what}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn until_status(&self, chat: &str, id: &str, status: &'static str) {
        let (chat, id) = (chat.to_string(), id.to_string());
        self.until(&format!("{id} {status}"), move |db| {
            Ok(read::message(db, &chat, &id)?.and_then(|(row, _)| row.status).as_deref() == Some(status))
        })
        .await;
    }

    /// What the demo engine was asked to send, in order, each time it was asked.
    fn sent(&self) -> Vec<e2e::Message> {
        self.world.sends().into_iter().map(|send| e2e::Message::decode(send.raw.as_slice()).unwrap()).collect()
    }

    async fn sends(&self, n: usize) {
        self.world.until(|asked| asked.iter().filter(|k| matches!(k, pb::command::Kind::Send(_))).count() >= n).await;
    }

    fn connection(&self, state: ConnectionState) {
        let params = serde_json::to_value(jaw_proto::Connection::new(state)).unwrap();
        let _ = self.pushes.send(Push { method: "connection".into(), params });
    }

    async fn outbox(&self) -> Vec<(String, String)> {
        self.query(|db| {
            Ok(db
                .prepare("SELECT kind, state FROM outbox ORDER BY n")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_goes_as_a_contact_message_and_several_as_one_array_shown_at_once_and_ticked_as_they_go() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    running.world.gate(true);
    let card = json!({ "chat": ANA, "cards": [{ "name": " Marta Lopes ", "vcard": MARTA }] });
    let one = running.call("send.contact", card).await.unwrap();
    assert_eq!((&one["row"]["kind"], &one["row"]["status"]), (&json!("contact"), &json!("pending")));
    assert_eq!(one["row"]["contact"], json!({ "name": "Marta Lopes", "vcard": MARTA }), "shown at once, trimmed");
    let both = json!({ "chat": ANA, "cards": [{ "name": "Rui", "vcard": RUI }, { "name": "Marta", "vcard": MARTA }] });
    let two = running.call("send.contact", both).await.unwrap();
    assert_eq!(
        two["row"]["contact"],
        json!({ "name": "2 contacts", "vcard": format!("{RUI}{MARTA}") }),
        "several shown as one card is, their vCards one after another"
    );
    running.world.gate(false);
    for sent in [&one, &two] {
        running.until_status(ANA, sent["message"].as_str().unwrap(), "read").await;
    }
    let sent = running.sent();
    let card = sent[0].contact_message.as_ref().expect("one card goes as a contact message");
    assert_eq!((card.display_name.as_deref(), card.vcard.as_deref()), (Some("Marta Lopes"), Some(MARTA)));
    assert!(card.context_info.is_none(), "a chat without disappearing messages gives it no timer");
    let array = sent[1].contacts_array_message.as_ref().expect("several go as an array of cards");
    assert_eq!(array.display_name.as_deref(), Some("2 contacts"));
    let cards: Vec<_> = array.contacts.iter().map(|c| (c.display_name.as_deref(), c.vcard.as_deref())).collect();
    assert_eq!(cards, [(Some("Rui"), Some(RUI)), (Some("Marta"), Some(MARTA))], "in the order asked");
    assert_eq!(running.outbox().await, [], "each gone from the outbox once the server had it");
    running.store.indexed().await.unwrap();
    let found = running.query(|db| read::search(db, "marta", None, None, 10)).await;
    assert_eq!(found.hits.len(), 2, "each found by the names on it");
}

#[tokio::test(flavor = "multi_thread")]
async fn cards_that_cant_go_are_refused_and_nothing_of_them_kept() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let card = |name: &str, vcard: &str| json!({ "name": name, "vcard": vcard });
    let ask = |cards: Vec<Value>| json!({ "chat": ANA, "cards": cards });
    let with_photo =
        MARTA.replace("END:VCARD", &format!("PHOTO;ENCODING=b;TYPE=JPEG:{}\r\nEND:VCARD", "A".repeat(VCARDS_MOST)));
    let refused = [
        (ask(vec![]), code::INVALID_PARAMS, None, "no cards"),
        (ask(vec![card("  ", MARTA)]), code::INVALID_PARAMS, None, "a card without a name"),
        (ask(vec![card("Marta", "Marta Lopes, +351 21 345 6789")]), code::INVALID_PARAMS, None, "not a vCard"),
        (ask(vec![card("Both", &format!("{MARTA}{RUI}"))]), code::INVALID_PARAMS, None, "two cards as one"),
        (ask(vec![card("Marta", &MARTA[..MARTA.len() - 11])]), code::INVALID_PARAMS, None, "a card left open"),
        (ask(vec![card(&"M".repeat(NAME_MOST + 1), MARTA)]), code::LIMIT, Some(("name", NAME_MOST)), "a name too long"),
        (ask(vec![card("Marta", MARTA); CARDS_MOST + 1]), code::LIMIT, Some(("cards", CARDS_MOST)), "too many"),
        (ask(vec![card("Marta", &with_photo)]), code::LIMIT, Some(("vcards", VCARDS_MOST)), "vCards too big"),
        (
            json!({ "chat": ANA, "cards": [card("Marta", MARTA)], "key": "" }),
            code::INVALID_PARAMS,
            None,
            "a key of nothing",
        ),
        (
            json!({ "chat": "351919999999@s.whatsapp.net", "cards": [card("Marta", MARTA)] }),
            code::NOT_FOUND,
            None,
            "no chat",
        ),
        (json!({ "chat": ANA, "people": ["person:4"] }), code::INVALID_PARAMS, None, "no cards asked"),
    ];
    for (params, code, limit, why) in refused {
        let error = running.call("send.contact", params).await.expect_err(why);
        assert_eq!(error.code, code, "{why}: {}", error.message);
        if let Some((limit, most)) = limit {
            assert_eq!(error.data, Some(json!({ "limit": limit, "most": most })), "{why}");
        }
    }
    assert_eq!(running.store.count("messages").unwrap(), 0, "nothing kept");
    let most = ask(vec![card("Marta", MARTA); CARDS_MOST]);
    assert!(running.call("send.contact", most).await.is_ok(), "{CARDS_MOST} cards go");
    running.sends(1).await;
    assert_eq!(running.sent()[0].contacts_array_message.as_ref().unwrap().contacts.len(), CARDS_MOST);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_asked_again_with_its_key_goes_once_and_cards_in_a_disappearing_chat_carry_its_timer() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let chat = Chat { id: CLIMBING.into(), ephemeral: Some(86_400), ..Chat::default() };
    running.store.apply(Lane::Live, vec![Change::Chat(chat)]).await.unwrap();
    let params = json!({ "chat": CLIMBING, "cards": [{ "name": "Marta", "vcard": MARTA }], "key": "jaw-card-1" });
    let (a, b) =
        tokio::join!(running.call("send.contact", params.clone()), running.call("send.contact", params.clone()));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a["message"], b["message"], "asked twice at once, one message");
    running.sends(1).await;
    let again = running.call("send.contact", params).await.unwrap();
    assert_eq!((&again["message"], &again["row"]["kind"]), (&a["message"], &json!("contact")), "and once it's gone");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!((running.world.sends().len(), running.store.count("messages").unwrap()), (1, 1), "it went once");
    running.cards(CLIMBING, &[("Rui", RUI), ("Marta", MARTA)]).await;
    running.sends(2).await;
    let sent = running.sent();
    let timer = sent[0].contact_message.as_ref().unwrap().context_info.as_ref().and_then(|c| c.expiration);
    let array_timer = sent[1].contacts_array_message.as_ref().unwrap().context_info.as_ref().and_then(|c| c.expiration);
    assert_eq!((timer, array_timer), (Some(86_400), Some(86_400)), "one card and several carry the chat's timer");
    running.world.gate(true);
    let held = running.cards(CLIMBING, &[("Rui", RUI)]).await;
    let row = |id: String| running.query(move |db| Ok(read::message(db, CLIMBING, &id)?.unwrap().0));
    assert_eq!(row(held.clone()).await.expires_at, None, "its timer waits for it to go");
    running.world.gate(false);
    let id = held.clone();
    running
        .until("its timer started", move |db| Ok(read::message(db, CLIMBING, &id)?.unwrap().0.expires_at.is_some()))
        .await;
    let gone = row(held).await;
    assert!(gone.expires_at.is_some_and(|at| at > gone.at), "and starts once the server has it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_or_a_poll_sent_reads_its_chat_and_leaves_what_was_typed_there_where_text_sent_is_what_was() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let theirs = crate::store::Message {
        chat: ANA.into(),
        id: "m1".into(),
        at: now_ms() - 10_000,
        sender: Some(ANA.into()),
        kind: "text",
        text: Some("tens o número da Marta?".into()),
        summary: "tens o número da Marta?".into(),
        live: true,
        ..crate::store::Message::default()
    };
    running.store.apply(Lane::Live, vec![Change::Message(theirs)]).await.unwrap();
    running.call("draft.set", json!({ "chat": ANA, "text": "aqui está" })).await.unwrap();
    let chat = || running.query(|db| Ok(read::chats(db, false, None, 10)?.into_iter().find(|c| c.id == ANA).unwrap()));
    assert_eq!(chat().await.unread, 1);
    running.cards(ANA, &[("Marta", MARTA)]).await;
    let after = chat().await;
    assert_eq!((after.unread, after.draft.as_deref()), (0, Some("aqui está")), "read, and what's typed stays");
    running.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::MarkRead(_)))).await;
    let poll = json!({ "chat": ANA, "question": "Jantar?", "options": ["Ramen", "Pizza"] });
    assert_eq!(running.call("send.poll", poll).await.unwrap()["row"]["kind"], "poll");
    assert_eq!(chat().await.draft.as_deref(), Some("aqui está"), "and stays after a poll too");
    running.call("send.text", json!({ "chat": ANA, "text": "aqui está" })).await.unwrap();
    assert_eq!(chat().await.draft, None, "text sent is what was typed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_that_fails_on_the_way_is_tried_again_and_one_whatsapp_refuses_fails_until_it_is_retried() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    running.world.fail_next(2);
    let id = running.cards(ANA, &[("Marta", MARTA)]).await;
    running.until_status(ANA, &id, "read").await;
    assert_eq!(running.world.sends().len(), 3, "tried again with the same ID until it went");
    assert!(running.world.sends().iter().all(|send| send.id == id));
    running.world.refuse_next(1);
    let refused = running.cards(ANA, &[("Rui", RUI)]).await;
    running.until_status(ANA, &refused, "failed").await;
    assert_eq!(running.outbox().await, [("contact".to_string(), "failed".to_string())], "kept to retry by hand");
    running.call("send.retry", json!({ "message": refused })).await.unwrap();
    running.until_status(ANA, &refused, "read").await;
    assert_eq!(running.world.sends().len(), 5);
    assert_eq!(running.outbox().await, []);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_deleted_for_everyone_while_it_waits_and_then_refused_leaves_nothing_of_it_to_send_again() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), false).await;
    running.world.replying(Replies { delivered: Duration::from_secs(3_600), read: Duration::from_secs(3_600) });
    let id = running.cards(ANA, &[("Marta", MARTA)]).await;
    running.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    running.world.refuse_next(1);
    running.world.gate(true);
    running.connection(ConnectionState::Online);
    running.sends(2).await;
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_ALLOWED, "not while its deletion is on its way: {}", retried.message);
    running.world.gate(false);
    running
        .eventually("the deletion kept", |db| {
            Ok(db.query_row("SELECT count(*) FROM outbox", [], |r| r.get::<_, i64>(0))? == 0)
        })
        .await;
    let sent = running.sent();
    assert!(sent[0].contact_message.is_some() && sent[1].protocol_message.is_some(), "the card, then its deletion");
    let (chat, message) = (ANA, id.clone());
    let row = running.query(move |db| Ok(read::message(db, chat, &message)?.unwrap().0)).await;
    assert!(row.deleted, "deleted, as nobody has it");
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "nothing of it is left to send again: {}", retried.message);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(running.world.sends().len(), 2, "and nothing more goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_waiting_to_go_deleted_for_you_still_goes_and_once_refused_leaves_nothing_to_retry() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), false).await;
    let id = running.cards(ANA, &[("Marta", MARTA)]).await;
    running.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    running.world.refuse_next(1);
    running.connection(ConnectionState::Online);
    running.sends(1).await;
    let card = running.sent().remove(0);
    assert_eq!(
        card.contact_message.and_then(|c| c.display_name).as_deref(),
        Some("Marta"),
        "it still went, as on the phone"
    );
    running
        .eventually("the refused card let go", |db| {
            Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'contact'", [], |r| r.get::<_, i64>(0))? == 0)
        })
        .await;
    let retried = running.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, code::NOT_FOUND, "nothing of it is left to retry: {}", retried.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn past_a_thousand_messages_waiting_a_card_is_refused_and_nothing_of_it_written() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), false).await;
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(SIDE_WAITS).unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
        for n in 0..1_000i64 {
            tx.execute(
                "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = ?3), ?1, ?2, ?2, 'text', 1, 'pending', 1)",
                rusqlite::params![format!("w{n}"), n, ANA],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), ?2, ?3, 'text', x'', ?1, 0, 0)",
                rusqlite::params![if n < 3 { "failed" } else { "queued" }, ANA, format!("w{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let params = json!({ "chat": CLIMBING, "cards": [{ "name": "Marta", "vcard": MARTA }], "key": "jaw-full" });
    let refused = running.call("send.contact", params.clone()).await.unwrap_err();
    assert_eq!(refused.code, code::LIMIT);
    assert_eq!(refused.data, Some(json!({ "limit": "outbox", "waiting": 1_000, "failed": 3 })));
    let (messages, keys) = running
        .query(|db| {
            let count =
                |table: &str| db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0));
            Ok((count("messages WHERE kind = 'contact'")?, count("send_keys")?))
        })
        .await;
    assert_eq!(
        (messages, keys),
        (0, 0),
        "neither the card nor its key kept, so asking again once there's room sends it"
    );
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(SIDE_WAITS).unwrap();
    db.execute("DELETE FROM outbox WHERE state = 'failed'", []).unwrap();
    assert!(running.call("send.contact", params).await.is_ok(), "taken once some have gone");
}
