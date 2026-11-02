//! The data rules sync's writes follow, each tested as a case: row keys never used twice, counts
//! derived from their source, one chat per person, and copies of a message that never undo what's
//! newer.

use prost::Message as _;
use rusqlite::Connection;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    MarkChatAsReadAction, SyncActionMessage, SyncActionMessageRange, SyncActionValue,
};

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
    let phone = Read { chat: ANA.into(), at: AT + 10, ids: Vec::new(), phone: Some((7, 2)), here: false };
    store.apply(Lane::Background, vec![Change::Read(phone)]).await.unwrap();
    let ana = chat(&store, ANA).await;
    assert_eq!((ana.unread, ana.mentions), (7, 2), "the phone's counts stand in");
    apply(&store, vec![Change::Message(Message { mentions_me: true, ..message(ANA, "new", AT + 20, "@me") })]).await;
    let ana = chat(&store, ANA).await;
    assert_eq!((ana.unread, ana.mentions), (8, 3));
    apply(
        &store,
        vec![Change::Read(Read { chat: ANA.into(), at: 0, ids: vec!["new".into()], phone: None, here: false })],
    )
    .await;
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
    merged(&store).await;
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

/// A chat's disappearing timer set to `seconds` at `at`.
fn timer(chat: &str, seconds: u32, at: i64) -> Change {
    Change::Chat(Chat { id: chat.into(), ephemeral: Some(seconds), ephemeral_at: Some(at), ..Chat::default() })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disappearing_timer_set_before_the_chats_never_replaces_it_and_one_said_again_moves_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![timer(ANA, 604_800, AT)]).await;
    assert_eq!(chat(&store, ANA).await.ephemeral, 604_800);
    // History's conversation, made before the setting, comes after it.
    apply(&store, vec![timer(ANA, 0, AT - 1)]).await;
    assert_eq!(chat(&store, ANA).await.ephemeral, 604_800, "an older setting replaced a newer one");
    let v = chat(&store, ANA).await.v;
    let seq = store.committed().borrow().to_owned();
    apply(&store, vec![timer(ANA, 604_800, AT), timer(ANA, 604_800, AT + 5)]).await;
    assert_eq!(chat(&store, ANA).await.v, v, "the same timer again moved the chat's version");
    assert_eq!(events(&store, seq).await, [], "the same timer again wrote an event");
    apply(&store, vec![timer(ANA, 86_400, AT + 10)]).await;
    assert_eq!(chat(&store, ANA).await.ephemeral, 86_400, "a newer setting is taken");
    // The setting at AT + 5 said again, after the newer one: it's older now, so it changes nothing.
    apply(&store, vec![timer(ANA, 604_800, AT + 5)]).await;
    assert_eq!(chat(&store, ANA).await.ephemeral, 86_400);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_chats_merged_into_one_keep_the_disappearing_timer_set_last() {
    for (lid_at, number_at, kept) in [(AT + 1, AT, 86_400), (AT, AT + 1, 0)] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        apply(
            &store,
            vec![
                Change::Message(message(ANA_LID, "l1", AT, "under the LID")),
                Change::Message(message(ANA, "p1", AT + 1, "under the number")),
                timer(ANA_LID, 86_400, lid_at),
                timer(ANA, 0, number_at),
            ],
        )
        .await;
        joined(&store).await;
        merged(&store).await;
        assert_eq!(
            chat(&store, ANA).await.ephemeral,
            kept,
            "the LID's timer set at {lid_at}, the number's at {number_at}"
        );
    }
}

/// Joins Ana's number and LID, so her LID chat starts being merged into her number's, and says
/// whether chats are being merged.
async fn joined(store: &Store) {
    apply(store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
}

async fn merging(store: &Store) -> bool {
    query(store, |db| Ok(db.query_row("SELECT EXISTS (SELECT 1 FROM chat_merges)", [], |r| r.get(0))?)).await
}

/// Takes merge steps until no merge waits, failing rather than going on for ever when one never ends.
async fn merged(store: &Store) {
    for _ in 0..50 {
        if !merging(store).await {
            return;
        }
        store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    panic!("the merge never finished");
}

/// In Ana's chat `chat`, a message and a photo, each as one copy of them says: with a time to go
/// and to open once, or with no time and as an ordinary photo, its caption edited since.
fn copies(chat: &str, timed: bool) -> Vec<Change> {
    const LATER: i64 = 4_102_444_800_000;
    let goes = Message { expires_at: timed.then_some(LATER), ..message(chat, "goes", AT, "gone in time") };
    let photo = Message { kind: "image", ..message(chat, "once", AT + 1, "") };
    let photo = if timed {
        Message { text: None, summary: "View once photo".into(), view_once: true, ..photo }
    } else {
        let caption = "a caption";
        Message {
            text: Some(caption.into()),
            spans: Some(serde_json::json!([{ "t": "text", "s": caption }]).to_string()),
            summary: caption.into(),
            edited_at: Some(AT + 50),
            raw: Some(Packed::new(b"the photo and its caption")),
            ..photo
        }
    };
    vec![Change::Message(goes), Change::Message(photo)]
}

#[tokio::test(flavor = "multi_thread")]
async fn copies_folded_into_one_keep_the_sooner_time_to_go_the_view_once_mark_and_its_opening() {
    for (timed, plain) in [(ANA_LID, ANA), (ANA, ANA_LID)] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let mut changes = copies(timed, true);
        changes.extend(copies(plain, false));
        apply(&store, changes).await;
        let (opened, until) = (
            query(&store, move |db| {
                Ok(db.query_row(
                    "SELECT m.n FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = ?1 AND m.id = 'once'",
                    [timed],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .await,
            AT + 3_600_000_000,
        );
        let open = expiry::Once::Opened { n: opened, until, jid: timed.into(), id: "once".into(), receipt: vec![] };
        assert_eq!(apply(&store, vec![Change::Once(open)]).await.changed, 1);
        joined(&store).await;
        merged(&store).await;
        let kept: (Option<i64>, bool, Option<String>, Option<String>, String, Option<i64>) = query(&store, |db| {
            Ok(db.query_row(
                "SELECT g.expires_at, o.view_once, o.text, o.spans, o.summary, p.until
                FROM messages g JOIN messages o ON o.id = 'once' LEFT JOIN opened p ON p.message = o.n
                WHERE g.id = 'goes'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )?)
        })
        .await;
        assert_eq!(
            kept,
            (Some(4_102_444_800_000), true, None, None, "View once photo".into(), Some(until)),
            "the copies with a time and the mark under {timed}, the others under {plain}"
        );
        let once = n_of(&store, "once").await;
        assert_eq!(apply(&store, vec![Change::Once(expiry::Once::Closed { n: once })]).await.changed, 1);
        let left: i64 =
            query(&store, |db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'once'", [], |r| r.get(0))?))
                .await;
        assert_eq!(left, 0, "closing it erases it, opened under {timed}");
    }
}

/// A fold that brings the copy kept a sooner time to go than the timer waits for tells the timer,
/// which erases the message then. The copy that goes is given its time here behind the writer's
/// back, so only the fold's commit says it, as when the timer has set that copy aside after its
/// erasure failed.
#[tokio::test(flavor = "multi_thread")]
async fn a_fold_bringing_a_sooner_time_to_go_wakes_the_timer_and_the_message_goes_at_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let later = Message { expires_at: Some(now() + 3_600_000), ..message(ANA, "goes", AT, "gone at the sooner time") };
    apply(
        &store,
        vec![Change::Message(later), Change::Message(message(ANA_LID, "goes", AT, "gone at the sooner time"))],
    )
    .await;
    let _timer = crate::expiry::start(store.clone());
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let soon = now() + 500;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    db.execute(
        "UPDATE messages SET expires_at = ?1 WHERE id = 'goes' AND chat = (SELECT n FROM chats WHERE id = ?2)",
        rusqlite::params![soon, ANA_LID],
    )
    .unwrap();
    drop(db);
    joined(&store).await;
    merged(&store).await;
    let left = || async {
        query(&store, |db| {
            Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'goes'", [], |r| r.get::<_, i64>(0))?)
        })
        .await
    };
    for _ in 0..250 {
        if left().await == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(left().await, 0, "the timer slept on to the later time");
    assert!(now() >= soon, "and it went no sooner than its time");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_names_a_message_still_in_a_chat_being_merged_finds_it_there() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "p1", AT, "under the number")),
            Change::Message(message(ANA_LID, "kept", AT + 1, "stays")),
            Change::Message(message(ANA_LID, "deleted", AT + 2, "deleted for me")),
            Change::Message(message(ANA_LID, "revoked", AT + 3, "deleted for everyone")),
            Change::Message(message(ANA_LID, "edited", AT + 4, "before")),
        ],
    )
    .await;
    joined(&store).await;
    assert!(merging(&store).await, "the LID chat's messages haven't moved yet");
    let edit = Amendment::Edit { text: Some("after".into()), spans: None, summary: "after".into() };
    apply(
        &store,
        vec![
            amend(ANA_LID, "deleted", AT + 10, None, Amendment::DeleteForMe),
            amend(ANA, "revoked", AT + 11, Some(ANA), Amendment::Revoke),
            amend(ANA_LID, "edited", AT + 12, Some(ANA), edit),
        ],
    )
    .await;
    merged(&store).await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let shown: Vec<(&str, &str, bool)> = rows.iter().map(|r| (r.id.as_str(), r.summary.as_str(), r.deleted)).collect();
    assert_eq!(
        shown.iter().map(|r| r.0).collect::<Vec<_>>(),
        ["p1", "kept", "revoked", "edited"],
        "the deleted one stays gone"
    );
    assert!(shown[2].2, "deleted for everyone");
    assert_eq!(shown[3].1, "after");
    assert_eq!(store.count("pending").unwrap(), 0, "nothing waits for a message jaw has");
    apply(&store, vec![amend(ANA_LID, "deleted", AT + 10, None, Amendment::DeleteForMe)]).await;
    apply(&store, vec![Change::Message(message(ANA_LID, "deleted", AT + 2, "deleted for me"))]).await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert!(rows.iter().all(|r| r.id != "deleted"), "a replay and a copy don't bring it back");
    assert_eq!(query(&store, |db| read::message_count(db, ANA)).await, 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn during_a_merge_what_names_a_message_shows_at_once_before_any_step_moves_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "p1", AT, "under the number")),
            Change::Message(message(ANA_LID, "revoked", AT + 1, "deleted for everyone")),
            Change::Message(message(ANA_LID, "edited", AT + 2, "before")),
            Change::Message(mine(ANA_LID, "both", AT + 3)),
            Change::Message(message(ANA_LID, "read up to", AT + 4, "read on the phone")),
        ],
    )
    .await;
    joined(&store).await;
    let edit = Amendment::Edit { text: Some("after".into()), spans: None, summary: "after".into() };
    apply(
        &store,
        vec![
            amend(ANA, "revoked", AT + 10, Some(ANA), Amendment::Revoke),
            amend(ANA_LID, "edited", AT + 11, Some(ANA), edit),
            Change::Message(Message { status: Some("read"), ..mine(ANA, "both", AT + 3) }),
            Change::Read(Read { chat: ANA.into(), at: 0, ids: vec!["read up to".into()], ..Read::default() }),
        ],
    )
    .await;
    assert!(merging(&store).await, "no merge step has run");
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    let shown =
        |id: &str| rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id} isn't in the kept chat yet"));
    assert!(shown("revoked").deleted, "a revoke, through located");
    assert_eq!(shown("edited").summary, "after", "an edit, through located");
    assert_eq!(shown("both").status.as_deref(), Some("read"), "a copy, through upsert_message");
    assert_eq!(rows.iter().filter(|r| r.id == "both").count(), 1);
    let both: i64 =
        query(&store, |db| Ok(db.query_row("SELECT count(*) FROM messages WHERE id = 'both'", [], |r| r.get(0))?))
            .await;
    assert_eq!(both, 1, "one row, not one in each chat");
    let read_at: Option<i64> =
        query(&store, |db| Ok(db.query_row("SELECT read_at FROM chats WHERE id = ?1", [ANA], |r| r.get(0))?)).await;
    assert_eq!(read_at, Some(AT + 4), "a mark-read naming the message, through set_read");
    assert_eq!(store.count("pending").unwrap(), 0, "nothing waits for a message jaw has");
}

/// A chat's shown values, and what its rows give: (unread, mentions, last message, last at,
/// message count), each pair to be equal.
async fn shown_and_derived(store: &Store, id: &'static str) -> [(i64, i64); 5] {
    query(store, move |db| {
        Ok(db.query_row(
            "SELECT c.unread, (SELECT count(*) FROM messages WHERE chat = c.n AND mine = 0),
                c.mentions, (SELECT coalesce(sum(mentions_me), 0) FROM messages WHERE chat = c.n AND mine = 0),
                c.last_message, (SELECT n FROM messages WHERE chat = c.n ORDER BY at DESC, n DESC LIMIT 1),
                c.last_at, (SELECT max(at) FROM messages WHERE chat = c.n),
                c.message_count, (SELECT count(*) FROM messages WHERE chat = c.n)
            FROM chats c WHERE c.id = ?1",
            [id],
            |r| {
                Ok([
                    (r.get(0)?, r.get(1)?),
                    (r.get(2)?, r.get(3)?),
                    (r.get(4)?, r.get(5)?),
                    (r.get(6)?, r.get(7)?),
                    (r.get(8)?, r.get(9)?),
                ])
            },
        )?)
    })
    .await
}

/// Ana's number chat with one message and her LID chat with 250 (every tenth mentioning you),
/// joined, and their merge stopped after its first step of 200: the half-moved state, with 201
/// messages in the number chat and 50 left in the LID chat. `before` is applied before the join.
async fn half_merged(before: Vec<Change>) -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut changes = vec![Change::Message(message(ANA, "p1", AT - 1, "under the number"))];
    for i in 0..250 {
        let words = format!("under the LID {i}");
        changes.push(Change::Message(Message {
            mentions_me: i % 10 == 0,
            ..message(ANA_LID, &format!("l{i}"), AT + i, &words)
        }));
    }
    changes.extend(before);
    apply(&store, changes).await;
    {
        // The first message of the second step can't move, so the merge stops after one of 200.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute_batch(
            "CREATE TRIGGER stays BEFORE UPDATE OF chat ON messages WHEN old.id = 'l200'
            BEGIN SELECT RAISE(ABORT, 'this one stays'); END;",
        )
        .unwrap();
    }
    joined(&store).await;
    store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    let failed = store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    assert_eq!(failed.quarantined, 1, "the second step can't move l200");
    (home, store)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_left_halfway_shows_each_chat_as_its_rows_are_and_reading_either_clears_both() {
    let (_home, store) = half_merged(Vec::new()).await;
    assert!(merging(&store).await);
    for id in [ANA, ANA_LID] {
        let shown = shown_and_derived(&store, id).await;
        assert!(shown.iter().all(|(shown, derived)| shown == derived), "{id}: {shown:?}");
    }
    assert_eq!(shown_and_derived(&store, ANA).await[0].0, 201, "p1 and the 200 moved");
    assert_eq!(shown_and_derived(&store, ANA_LID).await[0].0, 50, "the 50 left");
    assert_eq!(shown_and_derived(&store, ANA).await[3].0, AT + 199, "sorted by the newest it holds");
    // An edit of the LID chat's newest moves it at once, and each chat shows its newest after.
    let edit = Amendment::Edit { text: Some("edited".into()), spans: None, summary: "edited".into() };
    apply(&store, vec![amend(ANA_LID, "l249", AT + 300, Some(ANA_LID), edit)]).await;
    for id in [ANA, ANA_LID] {
        let shown = shown_and_derived(&store, id).await;
        assert!(shown.iter().all(|(shown, derived)| shown == derived), "{id} after the move: {shown:?}");
    }
    assert_eq!(shown_and_derived(&store, ANA_LID).await[3].0, AT + 248);
    apply(&store, vec![Change::Read(Read { chat: ANA_LID.into(), at: AT + 249, ..Read::default() })]).await;
    for id in [ANA, ANA_LID] {
        let shown = shown_and_derived(&store, id).await;
        assert_eq!((shown[0].0, shown[1].0), (0, 0), "{id}: read up to the LID chat's newest, nothing unread");
    }
}

/// Unread messages and mentions in each of Ana's two chats.
async fn unread(store: &Store) -> [(i64, i64); 2] {
    let (number, lid) = (shown_and_derived(store, ANA).await, shown_and_derived(store, ANA_LID).await);
    [(number[0].0, number[1].0), (lid[0].0, lid[1].0)]
}

#[tokio::test(flavor = "multi_thread")]
async fn during_a_merge_a_message_of_yours_in_the_kept_chat_reads_the_chat_merging_into_it_too() {
    let live = mine(ANA, "reply", AT + 400);
    let from_history = Message { live: false, ..mine(ANA_LID, "reply", AT + 400) };
    for (how, reply) in [("live, to her number", live), ("from history, to her LID", from_history)] {
        let (_home, store) = half_merged(Vec::new()).await;
        assert_eq!(unread(&store).await, [(201, 20), (50, 5)]);
        apply(&store, vec![Change::Message(reply)]).await;
        assert_eq!(unread(&store).await, [(0, 0), (0, 0)], "a reply {how} reads everything before it");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn during_a_merge_unmarking_the_kept_chat_by_either_jid_unmarks_the_chat_merging_into_it() {
    for jid in [ANA, ANA_LID] {
        let marked = |id: &str| Change::Chat(Chat { id: id.into(), marked_unread: Some(true), ..Chat::default() });
        let (_home, store) = half_merged(vec![marked(ANA), marked(ANA_LID)]).await;
        let both = || async { (chat(&store, ANA).await.marked_unread, chat(&store, ANA_LID).await.marked_unread) };
        assert_eq!(both().await, (true, true));
        apply(&store, vec![Change::Chat(Chat { id: jid.into(), marked_unread: Some(false), ..Chat::default() })]).await;
        assert_eq!(both().await, (false, false), "unmarked through app state by {jid}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn when_a_merge_starts_the_chat_merging_counts_from_where_the_kept_chat_was_read() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let mut changes: Vec<Change> =
        (0..50).map(|i| Change::Message(message(ANA_LID, &format!("l{i}"), AT + i, "under the LID"))).collect();
    // Answered later under her number, which reads her number chat up to then.
    changes.push(Change::Message(mine(ANA, "reply", AT + 100)));
    apply(&store, changes).await;
    assert_eq!(unread(&store).await, [(0, 0), (50, 0)]);
    joined(&store).await;
    assert!(merging(&store).await, "no step has run");
    assert_eq!(unread(&store).await, [(0, 0), (0, 0)], "the LID chat's messages came before the reply");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_step_applies_what_waited_for_a_message_and_drops_one_deleted_for_me() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![
            Change::Message(message(ANA_LID, "deleted", AT, "deleted for me")),
            Change::Message(message(ANA_LID, "revoked", AT + 1, "deleted for everyone")),
            Change::Message(message(ANA, "p1", AT + 2, "under the number")),
        ],
    )
    .await;
    joined(&store).await;
    // As if they'd been written before this jawd looked in chats being merged: a tombstone and a
    // revoke waiting under the kept chat for messages the merge hasn't moved.
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute("INSERT INTO tombstones (chat, id, at) SELECT n, 'deleted', 1 FROM chats WHERE id = ?1", [ANA])
            .unwrap();
        db.execute(
            "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect) VALUES (?1, 'revoked', 'revoke', ?2, x'', ?1, 0, ?3)",
            rusqlite::params![ANA, AT + 5, serde_json::to_string(&Amendment::Revoke).unwrap()],
        )
        .unwrap();
    }
    merged(&store).await;
    let rows = query(&store, |db| read::messages(db, ANA, Around::Newest, 10)).await;
    assert_eq!(rows.iter().map(|r| (r.id.as_str(), r.deleted)).collect::<Vec<_>>(), [("revoked", true), ("p1", false)]);
    assert_eq!(store.count("pending").unwrap(), 0);
}

/// Each message of Ana's chat `chat` as (its ID, what it says, its reactions).
async fn shown(store: &Store, chat: &'static str) -> Vec<(String, String, String)> {
    let rows = query(store, move |db| read::messages(db, chat, Around::Newest, 20)).await;
    rows.into_iter().map(|r| (r.id, r.summary, r.reactions.get().to_string())).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn what_waited_in_a_chat_merged_away_for_a_message_the_kept_chat_has_is_done_to_it_as_the_merge_starts() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // Her messages in her chat under her number, and what was done to them addressed to her chat
    // under her LID before jaw knew the two were one person's: a reaction and an edit waiting
    // there, and a deletion for you kept there.
    apply(
        &store,
        vec![
            Change::Message(message(ANA, "reacted", AT, "reacted to")),
            Change::Message(message(ANA, "edited", AT + 1, "before")),
            Change::Message(message(ANA, "deleted", AT + 2, "deleted for you")),
            Change::Message(message(ANA_LID, "l1", AT + 3, "under the LID")),
        ],
    )
    .await;
    let edit = Amendment::Edit { text: Some("after".into()), spans: None, summary: "after".into() };
    apply(
        &store,
        vec![
            amend(ANA_LID, "reacted", AT + 10, Some(ANA_LID), Amendment::React { emoji: "👍".into() }),
            amend(ANA_LID, "edited", AT + 11, Some(ANA_LID), edit),
            amend(ANA_LID, "deleted", AT + 12, None, Amendment::DeleteForMe),
        ],
    )
    .await;
    assert_eq!(store.count("pending").unwrap(), 2, "waiting under the chat they were addressed to");
    let done = [n_of(&store, "reacted").await, n_of(&store, "edited").await, n_of(&store, "deleted").await];
    let before = store.committed().borrow().to_owned();
    joined(&store).await;
    assert!(merging(&store).await, "no merge step has run");
    let rows = shown(&store, ANA).await;
    assert_eq!(rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["reacted", "edited"], "the deleted one went");
    assert!(rows[0].2.contains("👍"), "the reaction: {rows:?}");
    assert_eq!(rows[1].1, "after", "the edit");
    assert_eq!(store.count("pending").unwrap(), 0, "nothing waits for a message jaw has");
    let changed = events(&store, before).await;
    assert!(done.iter().all(|&n| changed.contains(&("message".into(), n))), "each with its event: {changed:?}");
    merged(&store).await;
    let rows = shown(&store, ANA).await;
    assert_eq!(rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["reacted", "edited", "l1"]);
    let again = Change::Message(message(ANA_LID, "deleted", AT + 2, "deleted for you"));
    apply(&store, vec![again]).await;
    assert_eq!(shown(&store, ANA).await.len(), 3, "a copy addressed to either chat doesn't bring it back");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_that_brings_a_jid_of_hers_jaw_has_not_seen_finds_what_she_did_from_it_as_her_chats_merge() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let other = "100000000202@lid";
    apply(
        &store,
        vec![Change::Message(message(ANA, "m1", AT, "hi")), Change::Message(message(ANA_LID, "l1", AT + 1, "hi"))],
    )
    .await;
    apply(&store, vec![amend(ANA_LID, "m1", AT + 10, Some(other), Amendment::React { emoji: "👍".into() })]).await;
    apply(
        &store,
        vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into(), other.into()], ..Person::default() })],
    )
    .await;
    assert_eq!(store.count("quarantine").unwrap(), 0, "the join went in");
    assert!(merging(&store).await);
    assert!(shown(&store, ANA).await[0].2.contains("👍"), "her reaction from the JID the join brought");
    let people: i64 = query(&store, move |db| {
        Ok(db.query_row(
            "SELECT count(DISTINCT person) FROM person_jids WHERE jid IN (?1, ?2, ?3)",
            [ANA, ANA_LID, other],
            |r| r.get(0),
        )?)
    })
    .await;
    assert_eq!(people, 1, "one person by all three");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reaction_waiting_under_a_jid_jaw_has_no_chat_by_lands_once_that_jid_leads_to_her_chat() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // Ana's message in her chat under her number, and her reaction to it addressed to her LID,
    // which jaw has no chat by and doesn't know is hers: it waits under her LID.
    apply(&store, vec![Change::Message(message(ANA, "m1", AT, "hi"))]).await;
    apply(&store, vec![amend(ANA_LID, "m1", AT + 10, Some(ANA_LID), Amendment::React { emoji: "👍".into() })]).await;
    assert_eq!(store.count("pending").unwrap(), 1);
    joined(&store).await;
    assert!(shown(&store, ANA).await[0].2.contains("👍"), "it lands once her LID leads to her chat");
    assert_eq!(store.count("pending").unwrap(), 0);

    // Rui, known by his number and his LID, with no chat yet: his reaction addressed to his LID
    // waits under it, and lands once his chat comes under his number.
    let rui_lid = "100000000003@lid";
    apply(&store, vec![Change::Person(Person { jids: vec![RUI.into(), rui_lid.into()], ..Person::default() })]).await;
    apply(&store, vec![amend(rui_lid, "r1", AT + 20, Some(rui_lid), Amendment::React { emoji: "🎉".into() })]).await;
    assert_eq!(store.count("pending").unwrap(), 1);
    apply(&store, vec![Change::Message(message(RUI, "r1", AT + 5, "olá"))]).await;
    assert!(shown(&store, RUI).await[0].2.contains("🎉"), "it lands in the chat his LID leads to");
    assert_eq!(store.count("pending").unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_copies_of_a_message_folded_into_one_keep_the_file_downloaded_for_either() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(mine(ANA, "both", AT)), Change::Message(mine(ANA_LID, "both", AT))]).await;
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO media (message, variant, keys, sha256) SELECT m.n, 'sd', 'image', zeroblob(32) FROM messages m
                JOIN chats c ON c.n = m.chat WHERE c.id = '{ANA}';
            INSERT INTO media (message, variant, keys, sha256, direct_path, wanted) SELECT m.n, 'sd', 'image',
                zeroblob(32), '/v/both', 5 FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = '{ANA_LID}';
            INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (zeroblob(32), 'media/files/both.jpg', 9, 1);"
        ))
        .unwrap();
    }
    joined(&store).await;
    merged(&store).await;
    let kept: (i64, Option<String>, Option<i64>, String, i64) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT count(*), max(md.direct_path), max(md.wanted), max(f.path), (SELECT count(*) FROM media_drop)
            FROM media md JOIN media_files f ON f.sha256 = md.sha256",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?)
    })
    .await;
    assert_eq!(
        kept,
        (1, Some("/v/both".into()), Some(5), "media/files/both.jpg".into(), 0),
        "one copy, with what either knew of how to fetch it, still naming the file in the cache"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_folded_into_another_is_wanted_after_any_want_a_job_took_of_either() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(mine(ANA, "both", AT)), Change::Message(mine(ANA_LID, "both", AT))]).await;
    {
        // A job took each copy at 5, and an interface then asked for the one in Ana's number's chat.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO media (message, variant, keys, sha256, wanted, asked) SELECT m.n, 'sd', 'image',
                zeroblob(32), 6, 1 FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = '{ANA}';
            INSERT INTO media (message, variant, keys, sha256, wanted) SELECT m.n, 'sd', 'image', zeroblob(32), 5
                FROM messages m JOIN chats c ON c.n = m.chat WHERE c.id = '{ANA_LID}';"
        ))
        .unwrap();
    }
    joined(&store).await;
    merged(&store).await;
    let kept = |store: Store| async move {
        query(&store, |db| {
            Ok(db.query_row("SELECT message, wanted, asked FROM media", [], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, bool>(2)?))
            })?)
        })
        .await
    };
    let (n, wanted, asked) = kept(store.clone()).await;
    assert_eq!((wanted, asked), (Some(7), true), "wanted after either copy's want, and still asked for");
    for taken in [5, 6] {
        let settled =
            media::Step::Settled { message: n, variant: "sd", wanted: taken, state: Some("remote"), error: None };
        apply(&store, vec![Change::Media(settled)]).await;
    }
    assert_eq!(kept(store.clone()).await, (n, Some(7), true), "no job that took a want before the fold settles it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_deleted_for_everyone_stays_deleted_through_a_fold_with_its_copy_edited_since() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let copy = |chat: &str| Message {
        raw: Some(Packed::new(b"what she said")),
        envelope: Some(Packed::new(b"the engine's event")),
        ..message(chat, "m", AT, "olá")
    };
    apply(&store, vec![Change::Message(copy(ANA)), Change::Message(copy(ANA_LID))]).await;
    // Deleted for everyone in her number's chat, which is kept, and edited since in her LID's.
    apply(&store, vec![amend(ANA, "m", AT + 10, Some(ANA), Amendment::Revoke)]).await;
    let edit = Amendment::Edit { text: Some("olá zebra".into()), spans: None, summary: "olá zebra".into() };
    apply(&store, vec![amend(ANA_LID, "m", AT + 20, Some(ANA_LID), edit)]).await;
    joined(&store).await;
    merged(&store).await;
    let kept: (bool, Option<String>, Option<String>, Option<String>, bool, bool) = query(&store, |db| {
        Ok(db.query_row(
            "SELECT deleted, text, spans, summary, raw IS NULL, envelope IS NULL FROM messages WHERE id = 'm'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?)
    })
    .await;
    assert_eq!(
        kept,
        (true, None, None, Some("This message was deleted".into()), true, true),
        "still deleted, with none of its copy's words or its raw protobuf"
    );
    store.indexed().await.unwrap();
    let found = query(&store, |db| read::search(db, "zebra", None, None, 10)).await;
    assert!(found.hits.is_empty(), "and search finds nothing of it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_kept_in_quarantine_leaves_it_once_it_converts() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.inbox("message", 0, b"a history notification".to_vec(), None);
    apply(&store, Vec::new()).await;
    let failed = |source: &'static str, reference: &str| Change::Quarantined {
        source,
        reference: reference.into(),
        error: "can't be had yet".into(),
    };
    apply(&store, vec![failed("inbox", "1"), failed("history_raw", &format!("{ANA} h1"))]).await;
    assert_eq!(store.count("quarantine").unwrap(), 2);
    apply(
        &store,
        vec![
            Change::Drained(Drained::Inbox(1)),
            Change::Drained(Drained::History { chat: ANA.into(), id: "h1".into() }),
        ],
    )
    .await;
    assert_eq!(store.count("quarantine").unwrap(), 0, "what failed has since converted");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_merged_away_leaves_a_key_no_new_chat_takes() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(
        &store,
        vec![Change::Message(message(ANA, "p1", AT, "number")), Change::Message(message(ANA_LID, "l1", AT + 1, "lid"))],
    )
    .await;
    let gone: i64 =
        query(&store, |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [ANA_LID], |r| r.get(0))?)).await;
    joined(&store).await;
    merged(&store).await;
    apply(&store, vec![Change::Message(message(RUI, "r1", AT + 2, "someone new"))]).await;
    let rui = chat(&store, RUI).await;
    assert!(rui.n > gone, "Rui's chat took {} after the merged chat's {gone}", rui.n);
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

#[tokio::test(flavor = "multi_thread")]
async fn a_message_history_keeps_as_deleted_for_everyone_takes_the_copy_jaw_has_with_it_and_waits_for_nothing() {
    use wa_proto::wa_web_protobufs_web::{WebMessageInfo, web_message_info::StubType};

    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let live = Message { envelope: Some(Packed::new(b"the engine's event")), ..message(ANA, "m1", AT, "segredo") };
    apply(&store, vec![Change::Message(live)]).await;
    let stub = |id: &str, from_me: bool| WebMessageInfo {
        key: wa_proto::wa_common::MessageKey {
            remote_jid: Some(ANA.into()),
            from_me: Some(from_me),
            id: Some(id.into()),
            participant: None,
        },
        message_timestamp: Some((AT / 1000) as u64),
        message_stub_type: Some(StubType::Revoke as i32),
        ..Default::default()
    };
    let me = crate::convert::Me::default();
    for (id, from_me) in [("m1", false), ("m2", true)] {
        store.apply(Lane::Background, crate::convert::message(&stub(id, from_me), &me, false)).await.unwrap();
    }
    let rows: Vec<(String, bool, Option<String>, String, bool, bool)> = query(&store, |db| {
        Ok(db
            .prepare("SELECT id, deleted, text, summary, envelope IS NULL, raw IS NOT NULL FROM messages ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await;
    assert_eq!(
        rows,
        [
            ("m1".into(), true, None, "This message was deleted".into(), true, false),
            ("m2".into(), true, None, "You deleted this message".into(), true, true),
        ],
        "the copy jaw had, its envelope with it, and one it hadn't"
    );
    assert_eq!(store.count("pending").unwrap(), 0);
    let found = query(&store, |db| read::search(db, "segredo", None, None, 10)).await;
    assert!(found.hits.is_empty(), "and search no longer finds what it said");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_read_on_any_device_loses_its_mark_even_where_jaw_had_read_it_already() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    apply(&store, vec![Change::Message(message(ANA, "a", AT, "a"))]).await;
    let here = Read { chat: ANA.into(), ids: vec!["a".into()], here: true, ..Read::default() };
    let marked = Change::Chat(Chat { id: ANA.into(), marked_unread: Some(true), ..Chat::default() });
    apply(&store, vec![Change::Read(here), marked]).await;
    let ana = chat(&store, ANA).await;
    assert!(ana.marked_unread && ana.unread == 0, "read here, the mark stays the phone's");
    // A read naming the message jaw has read up to, as jaw's own mark read comes back.
    let key =
        MessageKey { remote_jid: Some(ANA.into()), from_me: Some(false), id: Some("a".into()), participant: None };
    let range = SyncActionMessageRange {
        last_message_timestamp: Some(AT / 1000),
        messages: vec![SyncActionMessage { key: Some(key), timestamp: Some(AT / 1000) }],
        ..Default::default()
    };
    let value = SyncActionValue {
        timestamp: Some(AT + 5),
        mark_chat_as_read_action: Some(MarkChatAsReadAction { read: Some(true), message_range: Some(range) }),
        ..Default::default()
    };
    let index = ["markChatAsRead".to_string(), ANA.to_string()];
    apply(&store, crate::convert::app_state::mutation(&index, &value.encode_to_vec()).changes).await;
    assert!(!chat(&store, ANA).await.marked_unread, "read on any device, it isn't marked any more");
}
