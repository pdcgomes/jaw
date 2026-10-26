//! The rules folders follow, each a case: counts derived through every change and each chat
//! counted once, writes that change nothing twice, IDs never used again, the tree's refusals,
//! merges that carry what's filed and favourite, and the background check finding drift.

use rusqlite::Connection;

use super::*;
use crate::store::lists::{self, List};
use crate::store::read;
use crate::store::{Applied, Change, Chat, Lane, Message, Person, Read, Store};
use crate::work::Priority;

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const RUI: &str = "351910000003@s.whatsapp.net";
const INES: &str = "351910000004@s.whatsapp.net";
const CLIMBING: &str = "120363000000000001@g.us";
const AT: i64 = 1_792_000_000_000;

fn message(chat: &str, from: &str, id: &str, at: i64) -> Change {
    Change::Message(Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(from.into()),
        kind: "text",
        text: Some(id.into()),
        summary: id.into(),
        live: true,
        ..Message::default()
    })
}

fn mention(chat: &str, from: &str, id: &str, at: i64) -> Change {
    let Change::Message(m) = message(chat, from, id, at) else { unreachable!() };
    Change::Message(Message { mentions_me: true, ..m })
}

fn mine(chat: &str, id: &str, at: i64) -> Change {
    let Change::Message(m) = message(chat, chat, id, at) else { unreachable!() };
    Change::Message(Message { sender: None, mine: true, status: Some("sent"), ..m })
}

fn person(jid: &str, name: &str) -> Change {
    Change::Person(Person { jids: vec![jid.into()], name: Some(name.into()), ..Person::default() })
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

async fn query<T: Send + 'static>(
    store: &Store,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

/// Makes a folder step durably, as an interface's request is, and says what it came to.
async fn step(store: &Store, make: impl FnOnce(Done) -> Step) -> Outcome {
    let done = Done::default();
    store.apply_durable(vec![Change::Folders(make(done.clone()))]).await.unwrap();
    done.take().expect("the step says what it came to")
}

async fn create(store: &Store, name: &str, parent: Option<i64>) -> i64 {
    match step(store, |done| Step::Create { name: name.into(), parent, rule: None, done }).await {
        Outcome::Done(Some(n)) => n,
        other => panic!("{name} wasn't made: {other:?}"),
    }
}

async fn file(store: &Store, folder: i64, item: Item) {
    assert_eq!(step(store, |done| Step::File { folder, item, done }).await, Outcome::Done(None));
}

async fn person_n(store: &Store, jid: &'static str) -> i64 {
    query(store, move |db| Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [jid], |r| r.get(0))?))
        .await
}

async fn chat_n(store: &Store, jid: &'static str) -> i64 {
    query(store, move |db| Ok(db.query_row("SELECT n FROM chats WHERE id = ?1", [jid], |r| r.get(0))?)).await
}

/// A folder's (unread, mentions, count), as the tree shows it.
async fn counts(store: &Store, id: String) -> (i64, i64, i64) {
    let rows = query(store, |db| lists::folders(db, AT)).await;
    let row = rows.into_iter().find(|row| row.id == id).unwrap_or_else(|| panic!("no folder {id}"));
    (row.unread, row.mentions, row.count)
}

async fn yours(store: &Store, n: i64) -> (i64, i64, i64) {
    counts(store, format!("folder:{n}")).await
}

/// Asserts the counts and what they're kept through are what the tree, the items and the chats
/// make them, as the background check derives them.
async fn derived(store: &Store, when: &str) {
    let drift = query(store, lists::drift).await;
    assert!(drift.is_empty(), "{when}: {drift:?}");
}

/// The chats in a folder's chat list, by JID, in its order.
async fn listed(store: &Store, list: List) -> Vec<String> {
    query(store, move |db| lists::chats(db, &list, None, 100, AT)).await.into_iter().map(|row| row.id).collect()
}

async fn open() -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    (home, store)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folders_counts_hold_each_chat_once_across_its_subfolders_and_follow_every_change() {
    let (_home, store) = open().await;
    apply(
        &store,
        vec![
            person(ANA, "Ana Ribeiro"),
            person(INES, "Inês Gomes"),
            message(ANA, ANA, "a1", AT),
            mention(ANA, ANA, "a2", AT + 1),
            Change::Chat(Chat { id: CLIMBING.into(), name: Some("Climbing".into()), ..Chat::default() }),
            message(CLIMBING, RUI, "c1", AT + 2),
            message(CLIMBING, RUI, "c2", AT + 3),
            message(CLIMBING, RUI, "c3", AT + 4),
        ],
    )
    .await;
    let (ana, ines) = (person_n(&store, ANA).await, person_n(&store, INES).await);
    let family = create(&store, "Family", None).await;
    let cousins = create(&store, "Cousins", Some(family)).await;
    file(&store, family, Item::Person(ana)).await;
    file(&store, cousins, Item::Chat(ANA.into())).await;
    file(&store, cousins, Item::Chat(CLIMBING.into())).await;
    file(&store, cousins, Item::Person(ines)).await;
    derived(&store, "filed").await;
    assert_eq!(yours(&store, family).await, (5, 1, 3), "Ana's chat once, Climbing, and Inês, who has no chat");
    assert_eq!(yours(&store, cousins).await, (5, 1, 3));
    assert_eq!(listed(&store, List::Folder(family)).await, [CLIMBING, ANA], "newest first");

    apply(&store, vec![message(CLIMBING, RUI, "c4", AT + 5)]).await;
    assert_eq!(yours(&store, family).await, (6, 1, 3), "a message arriving counts in every folder above");
    apply(&store, vec![Change::Read(Read { chat: ANA.into(), at: AT + 1, ..Read::default() })]).await;
    assert_eq!(yours(&store, family).await, (4, 0, 3), "reading Ana's chat counts down");
    derived(&store, "read").await;

    assert_eq!(
        step(&store, |done| Step::Unfile { folder: cousins, item: Item::Person(ana), done }).await,
        Outcome::Done(None)
    );
    assert_eq!(yours(&store, cousins).await, (4, 0, 2));
    assert_eq!(yours(&store, family).await, (4, 0, 3), "Ana is still filed in Family itself");
    derived(&store, "unfiled").await;

    let moved = step(&store, |done| Step::Move { folder: cousins, parent: None, position: None, done }).await;
    assert_eq!(moved, Outcome::Done(None));
    assert_eq!(yours(&store, family).await, (0, 0, 1), "only Ana, read, once Cousins moves out");
    derived(&store, "moved").await;

    apply(&store, vec![message(INES, INES, "i1", AT + 6)]).await;
    assert_eq!(yours(&store, cousins).await, (5, 0, 2), "Inês has a chat of her own now, counted once as it");
    derived(&store, "a chat of her own").await;

    let moved = step(&store, |done| Step::Move { folder: cousins, parent: Some(family), position: None, done }).await;
    assert_eq!(moved, Outcome::Done(None));
    assert_eq!(step(&store, |done| Step::Delete { folder: family, done }).await, Outcome::Done(None));
    let tree = query(&store, |db| lists::folders(db, AT)).await;
    let cousins_row = tree.iter().find(|row| row.n == cousins).unwrap();
    assert_eq!(cousins_row.parent, None, "Cousins takes Family's place");
    assert!(tree.iter().all(|row| row.n != family));
    assert_eq!(yours(&store, cousins).await, (5, 0, 2));
    derived(&store, "deleted").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_built_in_folders_are_counted_as_their_lists_list_them() {
    let (_home, store) = open().await;
    apply(
        &store,
        vec![
            message(ANA, ANA, "a1", AT),
            message(CLIMBING, RUI, "c1", AT + 1),
            mention(CLIMBING, RUI, "c2", AT + 2),
            message(RUI, RUI, "r1", AT + 3),
            Change::Chat(Chat { id: RUI.into(), archived: Some(true), ..Chat::default() }),
            Change::Chat(Chat { id: CLIMBING.into(), kind: Some("group"), ..Chat::default() }),
        ],
    )
    .await;
    apply(&store, vec![Change::Read(Read { chat: ANA.into(), at: AT, ..Read::default() })]).await;
    for (id, list, expected) in [
        ("all", List::All, (2, 1, 2)),
        ("unread", List::Unread, (2, 1, 1)),
        ("groups", List::Groups, (2, 1, 1)),
        ("communities", List::Communities, (0, 0, 0)),
        ("archived", List::Archived, (1, 0, 1)),
    ] {
        assert_eq!(counts(&store, id.into()).await, expected, "{id}");
        let counted = list.clone();
        let total = query(&store, move |db| lists::chat_count(db, &counted, AT)).await;
        assert_eq!(total as usize, listed(&store, list.clone()).await.len(), "{id}'s total is what it lists");
    }
    derived(&store, "built in").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn filing_unfiling_and_a_favourite_asked_again_change_nothing_twice() {
    let (_home, store) = open().await;
    apply(&store, vec![message(ANA, ANA, "a1", AT)]).await;
    let family = create(&store, "Family", None).await;
    let events = || query(&store, |db| Ok(db.query_row("SELECT max(seq) FROM events", [], |r| r.get::<_, i64>(0))?));
    file(&store, family, Item::Chat(ANA.into())).await;
    let after = events().await;
    file(&store, family, Item::Chat(ANA.into())).await;
    let ana = person_n(&store, ANA).await;
    file(&store, family, Item::Person(ana)).await;
    assert_eq!(events().await, after, "filing again, by her chat or by her, writes nothing");
    assert_eq!(store.count("folder_items").unwrap(), 1, "her chat filed her");
    let set = |slot| step(&store, move |done| Step::Favourite { slot, target: Some(Target::Folder(family)), done });
    assert_eq!(set(2).await, Outcome::Done(None));
    let after = events().await;
    assert_eq!(set(2).await, Outcome::Done(None));
    assert_eq!(events().await, after, "a favourite set again writes nothing");
    assert_eq!(set(5).await, Outcome::Done(None));
    let slots = query(&store, lists::favourites).await;
    assert_eq!(slots.iter().map(|f| f.slot).collect::<Vec<_>>(), [5], "a folder sits in one slot, the last asked");
    let unfile = || step(&store, |done| Step::Unfile { folder: family, item: Item::Chat(ANA.into()), done });
    assert_eq!(unfile().await, Outcome::Done(None));
    let after = events().await;
    assert_eq!(unfile().await, Outcome::Done(None));
    assert_eq!(events().await, after);
    derived(&store, "asked again").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_folders_id_is_never_used_again() {
    let (home, store) = open().await;
    let first = create(&store, "First", None).await;
    assert_eq!(step(&store, |done| Step::Delete { folder: first, done }).await, Outcome::Done(None));
    let second = create(&store, "Second", None).await;
    assert!(second > first, "{second} after {first}");
    assert_eq!(step(&store, |done| Step::Delete { folder: second, done }).await, Outcome::Done(None));
    drop(store);
    let store = Store::open(home.path()).unwrap();
    let third = create(&store, "Third", None).await;
    assert!(third > second, "{third} after {second}, across a restart");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tree_refuses_what_can_not_be_and_changes_nothing() {
    use jaw_proto::code::{NOT_ALLOWED, NOT_FOUND};
    let (_home, store) = open().await;
    apply(&store, vec![message(ANA, ANA, "a1", AT)]).await;
    let all: i64 =
        query(&store, |db| Ok(db.query_row("SELECT n FROM folders WHERE builtin = 'all'", [], |r| r.get(0))?)).await;
    let smart: i64 =
        query(&store, |db| Ok(db.query_row("SELECT n FROM folders WHERE rule IS NOT NULL LIMIT 1", [], |r| r.get(0))?))
            .await;
    let family = create(&store, "Family", None).await;
    let cousins = create(&store, "Cousins", Some(family)).await;
    let second = create(&store, "Second", Some(cousins)).await;
    let other = create(&store, "Other", Some(family)).await;
    let refused = |outcome: Outcome| match outcome {
        Outcome::Refused(code, _) => code,
        Outcome::Done(_) => 0,
    };
    let before = query(&store, |db| lists::folders(db, AT)).await;
    let cases: Vec<(&str, Step, i32)> = vec![
        (
            "into itself",
            Step::Move { folder: family, parent: Some(family), position: None, done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "into a folder under it",
            Step::Move { folder: family, parent: Some(second), position: None, done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "a built-in renamed",
            Step::Rename { folder: all, name: "Everything".into(), done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "a built-in moved",
            Step::Move { folder: all, parent: Some(family), position: None, done: Done::default() },
            NOT_ALLOWED,
        ),
        ("a built-in deleted", Step::Delete { folder: all, done: Done::default() }, NOT_ALLOWED),
        (
            "filed in a built-in",
            Step::File { folder: all, item: Item::Chat(ANA.into()), done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "filed in a smart one",
            Step::File { folder: smart, item: Item::Chat(ANA.into()), done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "into a smart one",
            Step::Move { folder: cousins, parent: Some(smart), position: None, done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "a sibling's name",
            Step::Create { name: "cousins".into(), parent: Some(family), rule: None, done: Done::default() },
            NOT_ALLOWED,
        ),
        (
            "renamed as a sibling",
            Step::Rename { folder: other, name: "COUSINS".into(), done: Done::default() },
            NOT_ALLOWED,
        ),
        ("no such folder", Step::Rename { folder: 9_999, name: "x".into(), done: Done::default() }, NOT_FOUND),
        (
            "no such chat",
            Step::File {
                folder: family,
                item: Item::Chat("351999999999@s.whatsapp.net".into()),
                done: Done::default(),
            },
            NOT_FOUND,
        ),
        ("no such person", Step::File { folder: family, item: Item::Person(9_999), done: Done::default() }, NOT_FOUND),
        (
            "in no such folder",
            Step::Create { name: "x".into(), parent: Some(9_999), rule: None, done: Done::default() },
            NOT_FOUND,
        ),
    ];
    for (what, asked, code) in cases {
        let done = match &asked {
            Step::Move { done, .. }
            | Step::Rename { done, .. }
            | Step::Delete { done, .. }
            | Step::File { done, .. }
            | Step::Create { done, .. } => done.clone(),
            _ => unreachable!(),
        };
        store.apply_durable(vec![Change::Folders(asked)]).await.unwrap();
        assert_eq!(refused(done.take().unwrap()), code, "{what}");
    }
    assert_eq!(query(&store, |db| lists::folders(db, AT)).await, before, "nothing refused changed anything");
    assert_eq!(store.count("quarantine").unwrap(), 0, "a refusal isn't a failure");
    // Deleting Cousins moves Second up beside a folder of the same name, which it refuses.
    let _ = create(&store, "Second", Some(family)).await;
    let clash = step(&store, |done| Step::Delete { folder: cousins, done }).await;
    assert_eq!(refused(clash), NOT_ALLOWED);
    derived(&store, "refused").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_named_as_one_in_it_deletes_and_its_subfolders_take_its_place() {
    let (_home, store) = open().await;
    let a = create(&store, "A", None).await;
    let family = create(&store, "Family", None).await;
    let b = create(&store, "B", None).await;
    let inner = create(&store, "Family", Some(family)).await;
    let other = create(&store, "Other", Some(family)).await;
    assert_eq!(step(&store, |done| Step::Delete { folder: family, done }).await, Outcome::Done(None));
    let tree = query(&store, |db| lists::folders(db, AT)).await;
    let mut top: Vec<(i64, i64)> = tree
        .iter()
        .filter(|row| row.parent.is_none() && row.id.starts_with("folder:"))
        .map(|row| (row.position, row.n))
        .collect();
    top.sort();
    assert_eq!(top, [(0, a), (1, inner), (2, other), (3, b)], "in Family's place, in their order");
    assert_eq!(tree.iter().find(|row| row.n == inner).unwrap().name, "Family");
    derived(&store, "taken its place").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn joining_a_persons_number_and_lid_carries_where_they_and_their_chat_are_filed_and_its_favourite() {
    let (_home, store) = open().await;
    apply(
        &store,
        vec![
            person(ANA, "Ana Ribeiro"),
            message(ANA_LID, ANA_LID, "l1", AT),
            message(ANA_LID, ANA_LID, "l2", AT + 1),
            message(ANA, ANA, "p1", AT + 2),
        ],
    )
    .await;
    let (family, work) = (create(&store, "Family", None).await, create(&store, "Work", None).await);
    let (number, lid) = (person_n(&store, ANA).await, person_n(&store, ANA_LID).await);
    file(&store, family, Item::Person(lid)).await;
    file(&store, work, Item::Person(number)).await;
    let slot = step(&store, |done| Step::Favourite { slot: 2, target: Some(Target::Chat(ANA_LID.into())), done }).await;
    assert_eq!(slot, Outcome::Done(None));
    assert_eq!(yours(&store, family).await, (2, 0, 1), "the LID chat, by the LID person");
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    derived(&store, "joined, merging").await;
    while store.count("chat_merges").unwrap() > 0 {
        store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        derived(&store, "a merge step").await;
    }
    let kept = chat_n(&store, ANA).await;
    let row = query(&store, move |db| read::chat(db, kept)).await.unwrap();
    assert_eq!(row.favourite, Some(2), "the LID chat's favourite came to the chat kept");
    assert_eq!(row.folders, {
        let mut both = vec![family, work];
        both.sort();
        both
    });
    assert_eq!(yours(&store, family).await, (3, 0, 1), "every message of hers, in the one chat");
    assert_eq!(yours(&store, work).await, (3, 0, 1));
    assert_eq!(listed(&store, List::Folder(family)).await, [ANA]);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_chats_merged_keep_their_opens_each_decayed_to_the_later() {
    let (_home, store) = open().await;
    apply(&store, vec![message(ANA, ANA, "p1", AT), message(ANA_LID, ANA_LID, "l1", AT + 1)]).await;
    let week = crate::store::write::FRECENCY_HALF_LIFE_MS;
    apply(&store, vec![Change::Opened(crate::store::Opened { chat: ANA_LID.into(), at: AT })]).await;
    apply(&store, vec![Change::Opened(crate::store::Opened { chat: ANA.into(), at: AT + week })]).await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    let (frecency, opened_at): (f64, i64) = query(&store, |db| {
        Ok(db.query_row("SELECT frecency, opened_at FROM chats WHERE id = ?1", [ANA], |r| Ok((r.get(0)?, r.get(1)?)))?)
    })
    .await;
    assert_eq!(opened_at, AT + week);
    assert!((frecency - 1.5).abs() < 1e-9, "one open now and one a week before it, at half: {frecency}");
}

#[tokio::test(flavor = "multi_thread")]
async fn smart_folders_hold_what_their_rules_find_by_index() {
    let (_home, store) = open().await;
    let day = 86_400_000;
    let now = AT + 3 * day;
    apply(
        &store,
        vec![
            message(ANA, ANA, "old", AT),
            mention(CLIMBING, RUI, "c1", AT + day),
            message(RUI, RUI, "r1", now - 1_000),
            Change::Chat(Chat {
                id: CLIMBING.into(),
                kind: Some("group"),
                muted_until: Some(i64::MAX),
                ..Chat::default()
            }),
        ],
    )
    .await;
    async fn smart(store: &Store, rule: &str, now: i64) -> Vec<String> {
        let rule = rule.to_string();
        let made = step(store, |done| Step::Create { name: rule.clone(), parent: None, rule: Some(rule), done }).await;
        let Outcome::Done(Some(n)) = made else { panic!("{made:?}") };
        query(store, move |db| lists::chats(db, &List::Smart(n), None, 100, now))
            .await
            .into_iter()
            .map(|r| r.id)
            .collect()
    }
    assert_eq!(smart(&store, r#"{"mentions":true}"#, now).await, [CLIMBING]);
    assert_eq!(smart(&store, r#"{"muted":true,"unread":true}"#, now).await, [CLIMBING]);
    assert_eq!(
        smart(&store, r#"{"waiting":86400}"#, now).await,
        [ANA],
        "a day unanswered; Rui's is a second old, Climbing a group"
    );
    assert_eq!(smart(&store, r#"{"kind":"direct"}"#, now).await, [RUI, ANA]);
    apply(&store, vec![mine(ANA, "reply", now - 500)]).await;
    assert!(smart(&store, r#"{"waiting":86400}"#, now).await.is_empty(), "answered");
    let slot = step(&store, |done| Step::Favourite { slot: 1, target: Some(Target::Chat(RUI.into())), done }).await;
    assert_eq!(slot, Outcome::Done(None));
    assert_eq!(smart(&store, r#"{"favourites":true}"#, now).await, [RUI]);
    let rows = query(&store, move |db| lists::folders(db, now)).await;
    let waiting = rows.iter().find(|row| row.name == "Waiting on me").unwrap();
    assert_eq!((waiting.id.starts_with("smart:"), waiting.count), (true, 0));
    let mentions = rows.iter().find(|row| row.name == "Mentions me").unwrap();
    assert_eq!((mentions.unread, mentions.mentions, mentions.count), (1, 1, 1), "counted as it's read");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_community_sorts_by_its_newest_group_and_counts_their_unread() {
    let (_home, store) = open().await;
    let community = "120363000000000010@g.us";
    let (news, party) = ("120363000000000011@g.us", "120363000000000012@g.us");
    apply(
        &store,
        vec![
            Change::Chat(Chat {
                id: community.into(),
                kind: Some("community"),
                name: Some("Bairro".into()),
                ..Chat::default()
            }),
            Change::Chat(Chat {
                id: news.into(),
                kind: Some("announcement"),
                community: Some(community.into()),
                ..Chat::default()
            }),
            Change::Chat(Chat {
                id: party.into(),
                kind: Some("group"),
                community: Some(community.into()),
                ..Chat::default()
            }),
            message(ANA, ANA, "a1", AT),
            message(party, RUI, "p1", AT + 10),
            message(news, RUI, "n1", AT + 5),
        ],
    )
    .await;
    let all = listed(&store, List::All).await;
    let at = |id: &str| all.iter().position(|listed| listed == id).unwrap();
    assert!(at(community) < at(news) && at(community) < at(ANA), "it sorts by its newest group's message: {all:?}");
    let k = chat_n(&store, community).await;
    let row = query(&store, move |db| read::chat(db, k)).await.unwrap();
    assert_eq!(row.children_unread, Some(2));
    assert_eq!(row.last.as_ref().map(|last| last.summary.as_str()), Some("p1"), "it shows its newest group's message");
    derived(&store, "a community").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_background_check_finds_counts_changed_by_hand_and_a_rebuild_mends_them() {
    let (home, store) = open().await;
    apply(&store, vec![message(ANA, ANA, "a1", AT), message(CLIMBING, RUI, "c1", AT + 1)]).await;
    let family = create(&store, "Family", None).await;
    let cousins = create(&store, "Cousins", Some(family)).await;
    file(&store, cousins, Item::Chat(CLIMBING.into())).await;
    file(&store, family, Item::Chat(ANA.into())).await;
    let right = yours(&store, family).await;
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        db.execute_batch("UPDATE folder_counts SET unread = unread + 7; UPDATE folder_chats SET paths = paths + 1")
            .unwrap();
    }
    let drift = query(&store, lists::drift).await;
    assert!(!drift.is_empty(), "the check sees counts changed by hand");
    store.apply(Lane::Background, vec![Change::Folders(Step::Rebuild)]).await.unwrap();
    derived(&store, "rebuilt").await;
    assert_eq!(yours(&store, family).await, right);
}

#[tokio::test(flavor = "multi_thread")]
async fn presence_and_about_texts_said_again_or_late_change_nothing() {
    let (_home, store) = open().await;
    apply(&store, vec![person(ANA, "Ana Ribeiro"), message(ANA, ANA, "a1", AT)]).await;
    let ana = person_n(&store, ANA).await;
    let say = |state| Change::Folders(Step::Presence { jid: format!("{}:3@s.whatsapp.net", &ANA[..12]), state });
    assert_eq!(apply(&store, vec![say(Presence::Online)]).await.changed, 1, "from any of her devices");
    assert_eq!(apply(&store, vec![say(Presence::Online)]).await.changed, 0);
    assert_eq!(apply(&store, vec![say(Presence::Typing)]).await.changed, 1);
    assert_eq!(apply(&store, vec![say(Presence::Online)]).await.changed, 0, "online while typing stays typing");
    assert_eq!(apply(&store, vec![say(Presence::Away(Some(AT + 50)))]).await.changed, 1);
    let roster = query(&store, |db| lists::roster(db, &List::All, None, 10, true, AT)).await;
    assert_eq!((roster[0].presence.clone(), roster[0].last_seen), (None, Some(AT + 50)));
    let about = |at: i64, text: &str| {
        Change::Folders(Step::About { asked: vec![ANA.into()], about: vec![(ANA.into(), text.into())], at })
    };
    apply(&store, vec![about(AT + 100, "Lisboa")]).await;
    apply(&store, vec![about(AT + 50, "older")]).await;
    let text: Option<String> =
        query(&store, move |db| Ok(db.query_row("SELECT about FROM people WHERE n = ?1", [ana], |r| r.get(0))?)).await;
    assert_eq!(text.as_deref(), Some("Lisboa"), "an answer asked for earlier doesn't undo a newer one");
    apply(&store, vec![Change::Folders(Step::Following { people: vec![ana] })]).await;
    assert_eq!(apply(&store, vec![Change::Folders(Step::Following { people: vec![ana] })]).await.changed, 0);
    apply(&store, vec![Change::Folders(Step::Unknown)]).await;
    let (presence, followed): (Option<String>, bool) = query(&store, move |db| {
        Ok(db.query_row("SELECT presence, followed FROM people WHERE n = ?1", [ana], |r| Ok((r.get(0)?, r.get(1)?)))?)
    })
    .await;
    assert_eq!((presence, followed), (None, false));
}
