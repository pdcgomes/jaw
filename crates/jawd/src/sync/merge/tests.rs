use std::path::Path;

use rusqlite::Connection;
use rusqlite::types::Value;

use super::*;
use crate::store::outbox::{self, Owing};
use crate::store::{Amend, Amendment, Chat, Message, Person, Read};

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";

fn text(chat: &str, id: &str, at: i64) -> Message {
    Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind: "text",
        text: Some(id.into()),
        summary: id.into(),
        live: true,
        ..Message::default()
    }
}

fn message(chat: &str, id: &str, at: i64) -> Change {
    Change::Message(text(chat, id, at))
}

/// When each failed try was kept in quarantine, from the `tries` table the test's trigger fills.
async fn tries(store: &Store) -> Vec<i64> {
    store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db
                .prepare("SELECT at FROM tries ORDER BY rowid")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_step_that_keeps_failing_is_tried_again_later_and_later_then_left_tried_rarely_and_counted() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.apply(Lane::Live, vec![message(ANA, "p1", 1_000), message(ANA_LID, "l1", 2_000)]).await.unwrap();
    {
        // Every message the merge would move fails to, for a reason that won't pass.
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        db.execute_batch(
            "CREATE TRIGGER moving_fails BEFORE UPDATE OF chat ON messages
            BEGIN SELECT RAISE(ABORT, 'a merge step that always fails'); END;
            CREATE TABLE tries (at INTEGER);
            CREATE TRIGGER tried AFTER INSERT ON quarantine BEGIN INSERT INTO tries VALUES (new.at); END;
            CREATE TRIGGER tried_again AFTER UPDATE ON quarantine BEGIN INSERT INTO tries VALUES (new.at); END;",
        )
        .unwrap();
    }
    let person = Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() };
    store.apply(Lane::Live, vec![Change::Person(person)]).await.unwrap();
    let live: Shared = Arc::default();
    let timing = Timing {
        first: Duration::from_millis(40),
        most: Duration::from_millis(80),
        tries: 4,
        left: Duration::from_millis(800),
    };
    let began = Instant::now();
    let merging = tokio::spawn(run(store.clone(), Merges::default(), live.clone(), timing));
    let deadline = began + Duration::from_secs(10);
    while live.lock().unwrap().merges_stuck == 0 {
        assert!(Instant::now() < deadline, "the merge was never left");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let tried = tries(&store).await;
    assert_eq!(tried.len(), 4, "one try for each of its tries");
    let gaps: Vec<i64> = tried.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.iter().zip([40, 80, 80]).all(|(gap, pause)| *gap >= pause), "waits of 40, 80 and 80 ms, not {gaps:?}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(tries(&store).await.len(), 4, "and none soon after it's left");
    while tries(&store).await.len() < 5 {
        assert!(Instant::now() < deadline, "a merge left was never tried again");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let tried = tries(&store).await;
    assert!(tried[4] - tried[3] >= 800, "once it's waited as long as a merge left waits");
    assert_eq!(live.lock().unwrap().merges_stuck, 1, "still left, and counted once");
    let kept: Vec<(String, String)> = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db
                .prepare("SELECT ref, error FROM quarantine WHERE source = 'merge'")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
        .unwrap();
    assert_eq!(kept.len(), 1, "one row for the merge, however many tries");
    assert_eq!(kept[0].0, merge_reference(ANA_LID, ANA), "naming the chats");
    assert!(kept[0].1.contains("a merge step that always fails"), "{}", kept[0].1);
    assert_eq!(store.count("chat_merges").unwrap(), 1, "the merge waits for the next start, nothing undone");
    merging.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_that_succeeds_after_failing_clears_its_quarantine_row() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store.apply(Lane::Live, vec![message(ANA, "p1", 1_000), message(ANA_LID, "l1", 2_000)]).await.unwrap();
    let side = Connection::open(home.path().join("jaw.db")).unwrap();
    side.busy_timeout(Duration::from_secs(5)).unwrap();
    side.execute_batch(
        "CREATE TRIGGER moving_fails BEFORE UPDATE OF chat ON messages
        BEGIN SELECT RAISE(ABORT, 'not yet'); END;",
    )
    .unwrap();
    let person = Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() };
    store.apply(Lane::Live, vec![Change::Person(person)]).await.unwrap();
    let timing = Timing {
        first: Duration::from_millis(50),
        most: Duration::from_millis(50),
        tries: 100,
        left: Duration::from_secs(60),
    };
    let merging = tokio::spawn(run(store.clone(), Merges::default(), Arc::default(), timing));
    let deadline = Instant::now() + Duration::from_secs(10);
    while store.count("quarantine").unwrap() == 0 {
        assert!(Instant::now() < deadline, "the step never failed");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    side.execute_batch("DROP TRIGGER moving_fails").unwrap();
    while store.count("chat_merges").unwrap() > 0 {
        assert!(Instant::now() < deadline, "the merge never finished");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(store.count("quarantine").unwrap(), 0, "what failed has since succeeded");
    merging.abort();
}

/// One person's three one-to-one chats: their new number's, the oldest, their old number's, and
/// their LID's.
const NEW: &str = "351910000002@s.whatsapp.net";
const OLD: &str = "351910000001@s.whatsapp.net";
const LID: &str = "100000000009@lid";
const AT: i64 = 1_792_000_000_000;

fn mine(chat: &str, id: &str, at: i64) -> Message {
    Message { sender: None, mine: true, status: Some("sent"), ..text(chat, id, at) }
}

/// Something the person does to message `target`, sent to `chat`.
fn amend(chat: &str, target: &str, at: i64, what: Amendment) -> Change {
    Change::Amend(Amend {
        chat: chat.into(),
        target: target.into(),
        sender: Some(chat.into()),
        mine: false,
        at,
        what,
        raw: None,
    })
}

fn react(chat: &str, target: &str, at: i64, emoji: &str) -> Change {
    amend(chat, target, at, Amendment::React { emoji: emoji.into() })
}

fn edit(chat: &str, target: &str, at: i64) -> Change {
    amend(chat, target, at, Amendment::Edit { text: Some("edited".into()), spans: None, summary: "edited".into() })
}

fn receipt(chat: &str, target: &str, at: i64) -> Change {
    amend(chat, target, at, Amendment::Status { status: "read".into() })
}

/// A poll vote, which jaw keeps in pending for a later jawd that can read it.
fn vote(chat: &str, target: &str, at: i64) -> Change {
    amend(chat, target, at, Amendment::Keep { kind: "poll_vote".into() })
}

fn join(a: &str, b: &str) -> Change {
    Change::Person(Person { jids: vec![a.into(), b.into()], ..Person::default() })
}

async fn merges_run(store: &Store) {
    for _ in 0..50 {
        if store.count("chat_merges").unwrap() == 0 {
            return;
        }
        store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    panic!("the merge never finished");
}

/// Takes a step of the merge of chat `first` while it waits, or else of the first waiting, as the
/// merging task asks for one; says whether any merge waited.
async fn step(store: &Store, first: &str) -> bool {
    let waiting = waiting(store).await.unwrap();
    let first = format!("{first} into ");
    let Some(merging) = waiting.iter().find(|m| m.reference.starts_with(&first)).or(waiting.first()) else {
        return false;
    };
    let applied = store.apply(Lane::Background, vec![Change::MergeStep(Some(merging.clone()))]).await.unwrap();
    assert_eq!(applied.quarantined, 0, "a step of {} failed", merging.reference);
    true
}

/// Takes steps, those of chat `first`'s merge before any other's, until no merge waits.
async fn merged(store: &Store, first: &str) {
    for _ in 0..50 {
        if !step(store, first).await {
            return;
        }
    }
    panic!("the merges never finished");
}

/// Each merge waiting, as the JIDs of the chat going and the chat it goes into.
async fn merging(store: &Store) -> Vec<(String, String)> {
    waiting(store)
        .await
        .unwrap()
        .into_iter()
        .map(|m| {
            let (gone, keep) = m.reference.split_once(" into ").unwrap();
            (gone.to_string(), keep.to_string())
        })
        .collect()
}

/// The first column of each row `sql` reads.
async fn column<T: rusqlite::types::FromSql + Send + 'static>(store: &Store, sql: &'static str) -> Vec<T> {
    store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(db.prepare(sql)?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
        })
        .await
        .unwrap()
}

/// Everything jaw.db holds of a person's chats, a line per row, by JID and message ID rather than
/// by key, so runs that merged in another order, or were killed and started again, compare equal.
async fn digest(store: &Store) -> Vec<String> {
    const TABLES: &[(&str, &str)] = &[
        (
            "chat",
            "SELECT c.id, c.kind, c.pinned, c.unread, c.mentions, c.message_count, l.id, c.last_at, c.read_at, r.id,
                c.read_known, c.marked_unread
            FROM chats c LEFT JOIN messages l ON l.n = c.last_message LEFT JOIN messages r ON r.n = c.read_n
            ORDER BY c.id",
        ),
        (
            "merging",
            "SELECT g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep ORDER BY 1",
        ),
        (
            "message",
            "SELECT c.id, m.id, m.mine, m.at, m.status, m.deleted, m.text, m.summary, m.edited_at, m.starred
            FROM messages m JOIN chats c ON c.n = m.chat ORDER BY 1, 2",
        ),
        (
            "reaction",
            "SELECT c.id, m.id, j.jid, r.emoji, r.at FROM reactions r JOIN messages m ON m.n = r.message
                JOIN chats c ON c.n = m.chat JOIN person_jids j ON j.n = r.sender
            ORDER BY 1, 2, 3",
        ),
        (
            "receipt",
            "SELECT c.id, m.id, j.jid, r.delivered_at, r.read_at, r.played_at FROM receipts r
                JOIN messages m ON m.n = r.message JOIN chats c ON c.n = m.chat JOIN person_jids j ON j.n = r.recipient
            ORDER BY 1, 2, 3",
        ),
        ("pending", "SELECT chat, target, kind, at, effect FROM pending ORDER BY 1, 2, 3, 4"),
        ("tombstone", "SELECT c.id, t.id FROM tombstones t JOIN chats c ON c.n = t.chat ORDER BY 1, 2"),
        ("quarantine", "SELECT source, ref FROM quarantine ORDER BY 1, 2"),
        (
            "jid",
            "SELECT j.jid, count(*) FROM person_jids j JOIN person_jids k ON k.person = j.person GROUP BY j.jid ORDER BY 1",
        ),
    ];
    store
        .reads()
        .query(Priority::Interactive, |db| {
            let mut lines = Vec::new();
            for (table, sql) in TABLES {
                let mut statement = db.prepare(sql)?;
                let columns = statement.column_count();
                let mut rows = statement.query([])?;
                while let Some(row) = rows.next()? {
                    let values = (0..columns)
                        .map(|i| Ok(format!("{:?}", row.get::<_, Value>(i)?)))
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    lines.push(format!("{table} {}", values.join(" ")));
                }
            }
            Ok(lines)
        })
        .await
        .unwrap()
}

/// Fails, saying how, unless two digests are the same.
fn same(want: &[String], got: &[String], how: &str) {
    if want != got {
        let missing: Vec<&String> = want.iter().filter(|line| !got.contains(line)).take(12).collect();
        let extra: Vec<&String> = got.iter().filter(|line| !want.contains(line)).take(12).collect();
        panic!("{how}: {} lines for {}; missing {missing:#?}; extra {extra:#?}", got.len(), want.len());
    }
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

/// What reaches a person's chats, in order.
enum Then {
    Arrive(Vec<Change>),
    /// A step of a merge.
    Step,
    /// Steps until no merge waits.
    Rest,
}

/// The new number's chat with 2 messages, the old number's with 211 and the LID's with 411, one in
/// both of those, so each merge takes steps; their joins, the LID first to the old number and then
/// to the new; what's sent to the LID before, between and during the two merges; and last, the
/// messages that hadn't come, each to another of the person's JIDs.
fn three_chats() -> Vec<Then> {
    let mut chats = vec![message(NEW, "n0", AT), Change::Message(mine(NEW, "n1", AT + 1))];
    chats.push(Change::Message(mine(OLD, "both", AT + 50)));
    chats.extend((0..210).map(|i| message(OLD, &format!("o{i}"), AT + 100 + i)));
    chats.push(Change::Message(Message { live: false, ..mine(LID, "both", AT + 50) }));
    chats.extend((0..410).map(|i| message(LID, &format!("l{i}"), AT + 1_000 + i)));
    chats.push(Change::Chat(Chat { id: LID.into(), pinned: Some(AT + 60), ..Chat::default() }));
    vec![
        Then::Arrive(chats),
        Then::Arrive(vec![
            receipt(LID, "both", AT + 2_000),
            react(LID, "l5", AT + 2_001, "👍"),
            react(LID, "coming", AT + 2_002, "👍"),
            vote(LID, "poll", AT + 2_003),
        ]),
        // The LID's chat starts merging into the old number's, and a step moves 200 of its messages.
        Then::Arrive(vec![join(OLD, LID)]),
        Then::Step,
        Then::Arrive(vec![react(LID, "coming2", AT + 2_010, "😮"), edit(LID, "l300", AT + 2_011)]),
        // The old number's chat starts merging into the new number's.
        Then::Arrive(vec![join(LID, NEW)]),
        Then::Step,
        Then::Arrive(vec![
            edit(LID, "o3", AT + 2_020),
            amend(LID, "l250", AT + 2_021, Amendment::DeleteForMe),
            react(LID, "o205", AT + 2_022, "❤️"),
            receipt(LID, "n1", AT + 2_023),
            react(LID, "coming3", AT + 2_024, "🙏"),
            vote(LID, "l350", AT + 2_025),
        ]),
        Then::Rest,
        Then::Arrive(vec![
            message(LID, "coming", AT + 3_000),
            message(OLD, "coming2", AT + 3_001),
            message(NEW, "coming3", AT + 3_002),
        ]),
    ]
}

/// Plays `script` from `from` on, taking the steps of chat `first`'s merge before the other's,
/// then any steps left. With `kill`, keeps a copy of jaw.db as a kill would leave it after each
/// commit, with where in the script to go on from.
async fn play(
    store: &Store,
    home: &Path,
    script: &[Then],
    from: usize,
    first: &str,
    kill: bool,
) -> Vec<(usize, tempfile::TempDir)> {
    let mut kills = Vec::new();
    for (i, then) in script.iter().enumerate().skip(from) {
        match then {
            Then::Arrive(changes) => {
                store.apply(Lane::Live, changes.clone()).await.unwrap();
            }
            Then::Step => {
                assert!(step(store, first).await, "a merge waits");
            }
            Then::Rest => {
                while step(store, first).await {
                    if kill {
                        kills.push((i, killed(home)));
                    }
                }
                continue;
            }
        }
        if kill {
            kills.push((i + 1, killed(home)));
        }
    }
    merged(store, first).await;
    kills
}

/// What a script sends, all of it again.
fn again(script: &[Then]) -> Vec<Change> {
    script
        .iter()
        .flat_map(|then| match then {
            Then::Arrive(changes) => changes.clone(),
            Then::Step | Then::Rest => Vec::new(),
        })
        .filter(|change| !matches!(change, Change::Amend(Amend { what: Amendment::Keep { .. }, .. })))
        .collect()
}

/// What the person's one chat holds once both merges are done.
async fn one_chat_with_everything(store: &Store) {
    assert_eq!(column::<String>(store, "SELECT id FROM chats").await, [NEW], "one chat, the new number's");
    assert_eq!(store.count("chat_merges").unwrap(), 0);
    assert_eq!(store.count("quarantine").unwrap(), 0);
    let ids = column::<String>(store, "SELECT id FROM messages ORDER BY id").await;
    let mut want: Vec<String> = ["n0", "n1", "both", "coming", "coming2", "coming3"].map(String::from).into();
    want.extend((0..210).map(|i| format!("o{i}")));
    want.extend((0..410).filter(|&i| i != 250).map(|i| format!("l{i}")));
    want.sort();
    assert_eq!(ids, want, "every message once, but the one deleted for you");
    let statuses = column::<String>(store, "SELECT id || ' ' || status FROM messages WHERE mine ORDER BY id").await;
    assert_eq!(statuses, ["both read", "n1 read"], "the receipts sent to the LID");
    let reactions = column::<String>(
        store,
        "SELECT m.id || ' ' || r.emoji FROM reactions r JOIN messages m ON m.n = r.message ORDER BY m.id",
    )
    .await;
    assert_eq!(reactions, ["coming 👍", "coming2 😮", "coming3 🙏", "l5 👍", "o205 ❤️"]);
    let edited = column::<String>(store, "SELECT id FROM messages WHERE text = 'edited' ORDER BY id").await;
    assert_eq!(edited, ["l300", "o3"]);
    let tombstones =
        column::<String>(store, "SELECT c.id || ' ' || t.id FROM tombstones t JOIN chats c ON c.n = t.chat").await;
    assert_eq!(tombstones, [format!("{NEW} l250")], "the deletion sticks, in the chat that's left");
    let pending =
        column::<String>(store, "SELECT chat || ' ' || target || ' ' || kind FROM pending ORDER BY target").await;
    assert_eq!(
        pending,
        [format!("{NEW} l350 poll_vote"), format!("{NEW} poll poll_vote")],
        "only the votes wait, in the chat that's left"
    );
    let pinned = column::<i64>(store, "SELECT pinned FROM chats").await;
    assert_eq!(pinned, [AT + 60], "the LID chat's pin");
    let derived = column::<bool>(
        store,
        "SELECT c.message_count = (SELECT count(*) FROM messages WHERE chat = c.n)
            AND c.last_message = (SELECT n FROM messages WHERE chat = c.n ORDER BY at DESC, n DESC LIMIT 1)
            AND c.unread = min(1000, (SELECT count(*) FROM messages WHERE chat = c.n AND mine = 0
                AND (c.read_at IS NULL OR (at, n) > (c.read_at, coalesce(c.read_n, 9223372036854775807)))))
        FROM chats c",
    )
    .await;
    assert_eq!(derived, [true], "its count, newest message and unread as its rows give them");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_kept_by_one_merge_and_merged_away_by_another_leaves_the_person_one_chat() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // The new number's chat is the oldest, then the old number's, then the LID's.
    let changes = vec![
        Change::Chat(Chat { id: NEW.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: OLD.into(), kind: Some("direct"), ..Chat::default() }),
        Change::Chat(Chat { id: LID.into(), kind: Some("direct"), ..Chat::default() }),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    // The LID's chat starts merging into the old number's; before a step runs, the LID turns out
    // to be the new number's too, and the old number's chat starts merging into the new one's.
    store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
    store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
    assert_eq!(store.count("chat_merges").unwrap(), 2, "two merges wait");
    merges_run(&store).await;
    assert_eq!(store.count("chats").unwrap(), 1, "one person, one one-to-one chat");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_into_a_chat_that_starts_merging_itself_goes_on_into_the_chat_that_stays() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store
        .apply(Lane::Live, vec![message(NEW, "n0", AT), message(OLD, "o0", AT + 1), message(LID, "l0", AT + 2)])
        .await
        .unwrap();
    store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
    assert_eq!(merging(&store).await, [(LID.to_string(), OLD.to_string())]);
    store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
    let both = [(OLD.to_string(), NEW.to_string()), (LID.to_string(), NEW.to_string())];
    assert_eq!(merging(&store).await, both, "no chat both kept and going");
    // Until the LID's chat is gone, what's sent to it goes to the chat that stays.
    store.apply(Lane::Live, vec![message(LID, "l1", AT + 3)]).await.unwrap();
    let chat =
        column::<String>(&store, "SELECT c.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.id = 'l1'").await;
    assert_eq!(chat, [NEW]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_kept_as_the_oldest_then_merged_into_a_numbers_chat_takes_its_merge_with_it() {
    const HOSTED: &str = "100000000010@hosted.lid";
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // No number's chat at first, so the oldest is kept; then the number's joins, and it's kept.
    store
        .apply(Lane::Live, vec![message(LID, "l0", AT), message(HOSTED, "h0", AT + 1), message(OLD, "o0", AT + 2)])
        .await
        .unwrap();
    store.apply(Lane::Live, vec![join(LID, HOSTED)]).await.unwrap();
    assert_eq!(merging(&store).await, [(HOSTED.to_string(), LID.to_string())]);
    store.apply(Lane::Live, vec![join(HOSTED, OLD)]).await.unwrap();
    merges_run(&store).await;
    assert_eq!(column::<String>(&store, "SELECT id FROM chats").await, [OLD]);
    assert_eq!(column::<String>(&store, "SELECT id FROM messages ORDER BY id").await, ["h0", "l0", "o0"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn three_chats_merged_in_either_order_and_killed_after_any_commit_end_as_one_with_everything_in_it() {
    let script = three_chats();
    let mut reference: Option<Vec<String>> = None;
    for first in [OLD, LID] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let kills = play(&store, home.path(), &script, 0, first, true).await;
        assert_eq!(kills.len(), 13, "a kill after each of 7 arrivals, the 2 steps between them and the 4 steps left");
        one_chat_with_everything(&store).await;
        let straight = digest(&store).await;
        let reference = &*reference.get_or_insert_with(|| straight.clone());
        same(reference, &straight, &format!("{first}'s merge first"));
        for (k, (next, copy)) in kills.iter().enumerate() {
            let started = Store::open(copy.path()).unwrap();
            play(&started, copy.path(), &script, *next, first, false).await;
            same(reference, &digest(&started).await, &format!("{first}'s merge first, killed after commit {k}"));
        }
        // Everything sent again changes nothing; a vote is left out, as pending keeps each copy.
        store.apply(Lane::Live, again(&script)).await.unwrap();
        same(reference, &digest(&store).await, &format!("{first}'s merge first, everything sent again"));
    }
    // And sent again while both merges run, once the second has begun.
    let mut midway = three_chats();
    let sent = again(&midway[..7]);
    midway.insert(7, Then::Arrive(sent));
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    play(&store, home.path(), &midway, 0, OLD, false).await;
    same(reference.as_ref().unwrap(), &digest(&store).await, "everything sent again while both merges run");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_receipts_a_merged_chat_owes_are_owed_by_the_chat_it_ends_in() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let on_the_phone = |chat: &str| Change::Read(Read { chat: chat.into(), at: AT + 5_000, ..Read::default() });
    // The numbers' chats read on the phone, so WhatsApp knows; the LID's read here, its receipt owed.
    let here = Change::Read(Read { chat: LID.into(), ids: vec!["l0".into()], here: true, ..Read::default() });
    let changes = vec![message(NEW, "n0", AT), message(OLD, "o0", AT + 10), message(LID, "l0", AT + 20)];
    store.apply(Lane::Live, changes).await.unwrap();
    store.apply(Lane::Live, vec![on_the_phone(NEW), on_the_phone(OLD), here]).await.unwrap();
    store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
    store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
    merged(&store, OLD).await;
    let owed = store
        .reads()
        .query(Priority::Interactive, |db| {
            let n: i64 = db.query_row("SELECT n FROM chats WHERE id = ?1", [NEW], |r| r.get(0))?;
            outbox::owed(db, n)
        })
        .await
        .unwrap();
    let Owing::Receipts(owed) = owed else { panic!("the new number's chat owes the LID's receipt: {owed:?}") };
    assert_eq!(owed.messages.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["l0"]);
}

/// How each merge is named in quarantine.
async fn quarantined(store: &Store) -> Vec<String> {
    column(store, "SELECT ref FROM quarantine WHERE source = 'merge' ORDER BY ref").await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_that_failed_before_it_went_on_into_another_chat_is_quarantined_once_by_where_it_goes() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    store
        .apply(Lane::Live, vec![message(NEW, "n0", AT), message(OLD, "o0", AT + 1), message(LID, "l0", AT + 2)])
        .await
        .unwrap();
    let side = Connection::open(home.path().join("jaw.db")).unwrap();
    side.busy_timeout(Duration::from_secs(5)).unwrap();
    side.execute_batch(
        "CREATE TRIGGER stays BEFORE UPDATE OF chat ON messages WHEN old.id = 'l0'
        BEGIN SELECT RAISE(ABORT, 'not yet'); END;",
    )
    .unwrap();
    store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
    // The step as the merging task asks for it now, naming the old number's chat.
    let asked = waiting(&store).await.unwrap().remove(0);
    assert_eq!(asked.reference, merge_reference(LID, OLD));
    let failed = store.apply(Lane::Background, vec![Change::MergeStep(Some(asked.clone()))]).await.unwrap();
    assert_eq!(failed.quarantined, 1);
    store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
    assert_eq!(quarantined(&store).await, [merge_reference(LID, NEW)], "named by where it goes now");
    // A step asked for before the second join fails again by the name it had.
    let failed = store.apply(Lane::Background, vec![Change::MergeStep(Some(asked))]).await.unwrap();
    assert_eq!(failed.quarantined, 1);
    assert_eq!(quarantined(&store).await, [merge_reference(LID, NEW)], "still one row, by where it goes");
    side.execute_batch("DROP TRIGGER stays").unwrap();
    merged(&store, OLD).await;
    assert_eq!(column::<String>(&store, "SELECT id FROM chats").await, [NEW]);
    assert_eq!(store.count("quarantine").unwrap(), 0, "what failed has since succeeded");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chain_of_merges_jaw_db_already_holds_ends_in_one_chat_with_what_was_sent_meanwhile() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let changes = vec![
        message(NEW, "n0", AT),
        message(OLD, "o0", AT + 1),
        message(LID, "l0", AT + 2),
        message(LID, "l1", AT + 3),
    ];
    store.apply(Lane::Live, changes).await.unwrap();
    store.apply(Lane::Live, vec![join(OLD, LID)]).await.unwrap();
    store.apply(Lane::Live, vec![join(LID, NEW)]).await.unwrap();
    // As a jawd that didn't point merges on left it: the LID's chat merging into the old
    // number's, itself merging into the new number's.
    let side = Connection::open(home.path().join("jaw.db")).unwrap();
    side.busy_timeout(Duration::from_secs(5)).unwrap();
    side.execute(
        "UPDATE chat_merges SET keep = (SELECT n FROM chats WHERE id = ?1) WHERE gone = (SELECT n FROM chats WHERE id = ?2)",
        [OLD, LID],
    )
    .unwrap();
    // What's sent to the LID meanwhile lands in the old number's chat: a deletion for you of a
    // message it moves there, and a reaction to one not come yet.
    let changes = vec![amend(LID, "l1", AT + 10, Amendment::DeleteForMe), react(LID, "later", AT + 11, "👍")];
    store.apply(Lane::Live, changes).await.unwrap();
    let tombstones = "SELECT c.id || ' ' || t.id FROM tombstones t JOIN chats c ON c.n = t.chat";
    assert_eq!(column::<String>(&store, tombstones).await, [format!("{OLD} l1")]);
    assert_eq!(column::<String>(&store, "SELECT chat FROM pending").await, [OLD]);
    merged(&store, OLD).await;
    assert_eq!(column::<String>(&store, "SELECT id FROM chats").await, [NEW], "nothing cancelled");
    assert_eq!(column::<String>(&store, "SELECT id FROM messages ORDER BY id").await, ["l0", "n0", "o0"]);
    assert_eq!(column::<String>(&store, tombstones).await, [format!("{NEW} l1")]);
    assert_eq!(column::<String>(&store, "SELECT chat FROM pending").await, [NEW]);
    store.apply(Lane::Live, vec![message(LID, "l1", AT + 3), message(LID, "later", AT + 20)]).await.unwrap();
    assert_eq!(
        column::<String>(&store, "SELECT id FROM messages ORDER BY id").await,
        ["l0", "later", "n0", "o0"],
        "l1 stays deleted"
    );
    let reactions = "SELECT m.id || ' ' || r.emoji FROM reactions r JOIN messages m ON m.n = r.message";
    assert_eq!(column::<String>(&store, reactions).await, ["later 👍"]);
    assert_eq!(store.count("pending").unwrap(), 0);
}
