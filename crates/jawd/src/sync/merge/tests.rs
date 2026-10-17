use rusqlite::Connection;

use super::*;
use crate::store::{Message, Person};

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";

fn message(chat: &str, id: &str, at: i64) -> Change {
    Change::Message(Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind: "text",
        text: Some(id.into()),
        summary: id.into(),
        live: true,
        ..Message::default()
    })
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
