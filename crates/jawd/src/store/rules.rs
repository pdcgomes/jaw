//! The data rules sync's writes follow, each tested as a case: row keys never used twice, counts
//! derived from their source, one chat per person, and copies of a message that never undo what's
//! newer.

use rusqlite::Connection;

use super::read::{self, Around};
use super::*;
use crate::work::Priority;

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const RUI: &str = "351910000003@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;

fn message(chat: &str, id: &str, at: i64, words: &str) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind: "text",
        text: Some(words.into()),
        summary: words.into(),
        live: true,
        ..Message::default()
    }
}

fn mine(chat: &str, id: &str, at: i64) -> Message {
    Message { sender: None, mine: true, status: Some("sent"), ..message(chat, id, at, "from me") }
}

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

async fn chat(store: &Store, id: &'static str) -> read::ChatRow {
    let n: i64 =
        query(store, move |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [id], |r| r.get(0))?)).await;
    query(store, move |db| read::chat(db, n)).await.unwrap()
}

async fn n_of(store: &Store, id: &'static str) -> i64 {
    query(store, move |db| Ok(db.query_row("SELECT n FROM messages WHERE id = ?1", [id], |r| r.get(0))?)).await
}

/// The feed's events after `seq`, as (type, key).
async fn events(store: &Store, after: i64) -> Vec<(String, i64)> {
    query(store, move |db| Ok(read::events(db, after, 100)?.into_iter().map(|e| (e.kind.to_string(), e.key)).collect()))
        .await
}

fn amend(chat: &str, target: &str, at: i64, sender: Option<&str>, what: Amendment) -> Change {
    Change::Amend(Amend {
        chat: chat.into(),
        target: target.into(),
        sender: sender.map(str::to_string),
        mine: false,
        at,
        what,
        raw: None,
    })
}

fn read_at(chat: &str, at: i64) -> Change {
    Change::Read(Read { chat: chat.into(), at, ..Read::default() })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_messages_key_is_never_used_again_in_its_transaction_or_the_next() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(RUI, "x", AT, "x")), Change::Message(message(ANA, "m1", AT + 1, "m1"))])
        .await;
    let newest = n_of(&store, "m1").await;
    let before = store.committed().borrow().to_owned();
    apply(
        &store,
        vec![
            Change::DeleteMessage { chat: ANA.into(), id: "m1".into() },
            Change::Message(message(RUI, "new", AT + 2, "new")),
        ],
    )
    .await;
    let new = n_of(&store, "new").await;
    assert!(new > newest, "the new message took {new}, after the deleted {newest}");
    let seen = events(&store, before).await;
    assert!(seen.contains(&("message".into(), newest)), "the deleted message's event: {seen:?}");
    assert!(seen.contains(&("message".into(), new)), "the new message's own event: {seen:?}");
    apply(&store, vec![Change::DeleteMessage { chat: RUI.into(), id: "new".into() }]).await;
    store.apply(Lane::Background, vec![Change::Message(message(RUI, "later", AT + 3, "later"))]).await.unwrap();
    assert!(n_of(&store, "later").await > new, "a history message after a deletion takes a new key too");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_joined_into_another_leaves_a_key_that_is_never_used_again() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into()], ..Person::default() })]).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA_LID.into()], ..Person::default() })]).await;
    let people = || {
        query(&store, |db| {
            Ok(db
                .prepare("SELECT n FROM people ORDER BY n")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()?)
        })
    };
    let [_, joined] = people().await[..] else { panic!("two people") };
    apply(
        &store,
        vec![
            Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() }),
            Change::Person(Person { jids: vec![RUI.into()], ..Person::default() }),
        ],
    )
    .await;
    let after = people().await;
    assert_eq!(after.len(), 2);
    assert!(after[1] > joined, "Rui took {}, after {joined}, which the join removed", after[1]);
}

#[tokio::test(flavor = "multi_thread")]
async fn unread_is_counted_from_where_the_chat_was_read_and_a_replayed_mark_read_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(ANA, "a", AT, "a")), Change::Message(message(ANA, "b", AT + 10, "b"))])
        .await;
    assert_eq!(chat(&store, ANA).await.unread, 2, "nobody has said where it was read");
    apply(&store, vec![read_at(ANA, AT + 5)]).await;
    assert_eq!(chat(&store, ANA).await.unread, 1);
    apply(&store, vec![Change::Message(message(ANA, "c", AT + 20, "c"))]).await;
    assert_eq!(chat(&store, ANA).await.unread, 2);
    apply(&store, vec![read_at(ANA, AT + 5)]).await;
    assert_eq!(chat(&store, ANA).await.unread, 2, "a mark-read replayed by a resync");
    apply(&store, vec![Change::Message(mine(ANA, "me", AT + 30))]).await;
    assert_eq!(chat(&store, ANA).await.unread, 0, "writing in a chat reads it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_late_history_snapshot_keeps_what_live_messages_counted_and_deleting_an_unread_one_lowers_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "live1", AT + 100, "1")),
            Change::Message(message(ANA, "live2", AT + 200, "2")),
        ],
    )
    .await;
    // The phone said nothing was unread, as of before the live messages came.
    let history = Message { live: false, ..message(ANA, "old", AT, "old") };
    store.apply(Lane::Background, vec![read_at(ANA, AT), Change::Message(history)]).await.unwrap();
    assert_eq!(chat(&store, ANA).await.unread, 2);
    apply(&store, vec![Change::DeleteMessage { chat: ANA.into(), id: "live2".into() }]).await;
    assert_eq!(chat(&store, ANA).await.unread, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn history_counts_as_read_until_someone_says_where_the_chat_was_read_and_the_phones_count_stands_in() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let old = |id: &str, at: i64| Change::Message(Message { live: false, ..message(ANA, id, at, id) });
    store.apply(Lane::Background, vec![old("h1", AT), old("h2", AT + 10)]).await.unwrap();
    assert_eq!(chat(&store, ANA).await.unread, 0, "history kept raw, with nothing said of where it was read");
    let phone = Read { chat: ANA.into(), at: AT + 10, ids: Vec::new(), phone: Some((7, 2)) };
    store.apply(Lane::Background, vec![Change::Read(phone)]).await.unwrap();
    let ana = chat(&store, ANA).await;
    assert_eq!((ana.unread, ana.mentions), (7, 2), "the phone's counts stand in");
    apply(&store, vec![Change::Message(Message { mentions_me: true, ..message(ANA, "new", AT + 20, "@me") })]).await;
    let ana = chat(&store, ANA).await;
    assert_eq!((ana.unread, ana.mentions), (8, 3));
    apply(&store, vec![Change::Read(Read { chat: ANA.into(), at: 0, ids: vec!["new".into()], phone: None })]).await;
    let ana = chat(&store, ANA).await;
    assert_eq!((ana.unread, ana.mentions), (0, 0), "read up to the message a read receipt names");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_persons_two_chats_become_one_when_their_number_and_lid_are_joined() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA_LID, "l1", AT, "under the LID")),
            Change::Message(mine(ANA_LID, "both", AT + 10)),
            Change::Chat(Chat { id: ANA_LID.into(), pinned: Some(AT + 50), ..Chat::default() }),
            amend(ANA_LID, "both", AT + 20, Some(ANA_LID), Amendment::Status { status: "read".into() }),
            amend(ANA_LID, "coming", AT + 30, Some(ANA_LID), Amendment::React { emoji: "👍".into() }),
            Change::Message(message(ANA, "p1", AT + 40, "under the number")),
            Change::Message(Message { live: false, ..mine(ANA, "both", AT + 10) }),
        ],
    )
    .await;
    assert_eq!(query(&store, |db| read::chats(db, false, None, 10)).await.len(), 2);
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    while query(&store, |db| Ok(db.query_row("SELECT EXISTS (SELECT 1 FROM chat_merges)", [], |r| r.get(0))?)).await {
        store.apply(Lane::Background, vec![Change::MergeStep]).await.unwrap();
    }
    let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
    assert_eq!(chats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), [ANA]);
    assert!(chats[0].pinned, "the LID chat's pin");
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["l1", "both", "p1"], "one row per message");
    assert_eq!(rows[1].status.as_deref(), Some("read"), "the receipt under the LID");
    assert_eq!(query(&store, |db| read::message_count(db, ANA)).await, 3);
    apply(&store, vec![Change::Message(message(ANA_LID, "coming", AT + 60, "late"))]).await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.last().map(|r| r.id.as_str()), Some("coming"), "a message to the LID goes to the chat kept");
    assert!(rows.last().unwrap().reactions.get().contains("👍"), "with the reaction that waited under the LID");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_of_a_message_never_undoes_a_newer_edit_or_the_live_envelope() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let live = Message {
        envelope: Some(Packed::new(b"the engine's event")),
        raw: Some(Packed::new(b"made")),
        ..message(ANA, "m", AT, "first")
    };
    apply(&store, vec![Change::Message(live.clone())]).await;
    let edit = Amendment::Edit { text: Some("second".into()), spans: None, summary: "second".into() };
    apply(&store, vec![amend(ANA, "m", AT + 20, Some(ANA), edit)]).await;
    let older_edit = Message {
        edited_at: Some(AT + 10),
        live: false,
        raw: Some(Packed::new(b"history")),
        ..message(ANA, "m", AT, "older edit")
    };
    store.apply(Lane::Background, vec![Change::Message(older_edit)]).await.unwrap();
    let raws = || {
        query(&store, |db| {
            Ok(db.query_row("SELECT raw, envelope FROM messages WHERE id = 'm'", [], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
            })?)
        })
    };
    let (raw, envelope) = raws().await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows[0].summary, "second", "the newer edit stays");
    assert_eq!(
        write::unpack(&raw).unwrap(),
        b"history",
        "a history copy's raw says more than one made from the engine's"
    );
    assert_eq!(write::unpack(&envelope.unwrap()).unwrap(), b"the engine's event");
    apply(&store, vec![Change::Message(Message { envelope: Some(Packed::new(b"again")), ..live })]).await;
    let (raw, envelope) = raws().await;
    assert_eq!(write::unpack(&raw).unwrap(), b"history", "a live copy keeps history's raw");
    assert_eq!(write::unpack(&envelope.unwrap()).unwrap(), b"the engine's event", "the first envelope stays");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_is_removed_only_by_a_removal_newer_than_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let react = |emoji: &str, at: i64| amend(ANA, "m", at, Some(ANA), Amendment::React { emoji: emoji.into() });
    apply(&store, vec![Change::Message(message(ANA, "m", AT, "hi")), react("👍", AT + 20)]).await;
    let reactions =
        || query(&store, |db| Ok(db.query_row("SELECT count(*) FROM reactions", [], |r| r.get::<_, i64>(0))?));
    apply(&store, vec![react("", AT + 10)]).await;
    assert_eq!(reactions().await, 1, "a removal from before the reaction");
    apply(&store, vec![react("", AT + 30)]).await;
    assert_eq!(reactions().await, 0);
}
