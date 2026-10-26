use rusqlite::Connection;
use serde_json::{Value, json};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::*;
use crate::store::links::POLLS;
use crate::store::read::Around;
use crate::store::{Amend, Amendment, Applied, Change, Lane, Message, Packed, Person, Store, read, schema};
use crate::work::{Priority, Report};

const GROUP: &str = "120363000000000001@g.us";
const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const RUI: &str = "351910000003@s.whatsapp.net";
const ME: &str = "351910000001@s.whatsapp.net";
const AT: i64 = 1_792_000_000_000;
const DINNER: &[&str] = &["Ramen", "Tasca do Chico", "Pizza"];

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Ana's poll `id` in `chat`, asking about dinner with `options`, of which a vote chooses `max`.
fn poll_in(chat: &str, id: &str, options: &[&str], max: u32) -> Message {
    let shown: Vec<Value> = options.iter().map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect();
    Message {
        chat: chat.into(),
        id: id.into(),
        at: AT,
        sender: Some(ANA.into()),
        kind: "poll",
        summary: "Poll: Jantar?".into(),
        detail: Some(
            json!({ "poll": { "question": "Jantar?", "options": shown, "max": max, "voters": 0 } }).to_string(),
        ),
        poll: options.iter().map(|name| name.to_string()).collect(),
        live: true,
        ..Message::default()
    }
}

fn poll(id: &str) -> Change {
    Change::Message(poll_in(GROUP, id, DINNER, 1))
}

/// `voter`'s vote in poll `poll` in `chat`, made at `at` as message `id`, choosing `chose`.
fn vote_in(chat: &str, poll: &str, voter: &str, at: i64, id: &str, chose: &[&str]) -> Change {
    Change::Amend(Amend {
        chat: chat.into(),
        target: poll.into(),
        sender: Some(voter.into()),
        mine: voter == ME,
        at,
        what: Amendment::Vote { options: chose.iter().map(|name| hex(&option_hash(name))).collect(), id: id.into() },
        raw: Some(Packed::new(id.as_bytes())),
    })
}

fn vote(poll: &str, voter: &str, at: i64, id: &str, chose: &[&str]) -> Change {
    vote_in(GROUP, poll, voter, at, id, chose)
}

/// Poll `id` as a window shows it: each option's votes and whether you chose it, how many voted,
/// and its version.
async fn shown_in(store: &Store, chat: String, id: &'static str) -> (Vec<(String, i64, bool)>, i64, i64) {
    let row = query(store, move |db| Ok(read::message(db, &chat, id)?.expect("the poll is there").0)).await;
    let detail: Value = serde_json::from_str(row.detail.as_ref().expect("a poll's detail").get()).unwrap();
    let poll = &detail["poll"];
    let options = poll["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["name"].as_str().unwrap().to_string(), o["votes"].as_i64().unwrap(), o["mine"] == true))
        .collect();
    (options, poll["voters"].as_i64().unwrap(), row.v)
}

async fn shown(store: &Store, id: &'static str) -> (Vec<(String, i64, bool)>, i64, i64) {
    shown_in(store, GROUP.into(), id).await
}

fn tally(votes: [i64; 3], mine: Option<usize>) -> Vec<(String, i64, bool)> {
    DINNER.iter().zip(votes).enumerate().map(|(at, (name, n))| (name.to_string(), n, mine == Some(at))).collect()
}

async fn count(store: &Store, what: &'static str) -> u64 {
    store.count(what).unwrap()
}

async fn opened(home: &std::path::Path) -> Store {
    let store = Store::open(home).unwrap();
    apply(&store, vec![Change::Remember { key: "me".into(), value: ME.into() }]).await;
    store
}

#[tokio::test(flavor = "multi_thread")]
async fn each_voters_newest_vote_counts_and_one_older_the_same_again_or_naming_no_option_of_it_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, vec![poll("p1")]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([0, 0, 0], None), 0));

    apply(&store, vec![vote("p1", ANA, AT + 10, "v1", &["Ramen"])]).await;
    apply(&store, vec![vote("p1", ME, AT + 11, "v2", &["Tasca do Chico"])]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([1, 1, 0], Some(1)), 2), "yours is marked as yours");

    // Ana changes her mind: her newer vote replaces her last.
    apply(&store, vec![vote("p1", ANA, AT + 20, "v3", &["Pizza"])]).await;
    let (options, voters, v) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([0, 1, 1], Some(1)), 2));

    // An older vote arriving late, and the same vote again, change nothing, not even its version.
    for late in [vote("p1", ANA, AT + 5, "v0", &["Ramen"]), vote("p1", ANA, AT + 20, "v3", &["Pizza"])] {
        let applied = apply(&store, vec![late]).await;
        assert_eq!(applied.changed, 0);
        assert_eq!(shown(&store, "p1").await, (tally([0, 1, 1], Some(1)), 2, v));
    }

    // Two votes of hers in the same millisecond are ordered by their IDs.
    apply(&store, vec![vote("p1", ANA, AT + 20, "v4", &["Ramen"])]).await;
    let (options, _, newer) = shown(&store, "p1").await;
    assert_eq!(options, tally([1, 1, 0], Some(1)));
    assert!(newer > v, "a vote that counts gives the poll a new version");

    // A vote choosing nothing takes hers back; one naming an option the poll hasn't is no choice.
    apply(&store, vec![vote("p1", ANA, AT + 30, "v5", &[])]).await;
    apply(&store, vec![vote("p1", RUI, AT + 31, "v6", &["Sushi"])]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([0, 1, 0], Some(1)), 1), "only you chose anything now");
    apply(&store, vec![vote("p1", RUI, AT + 32, "v7", &["Ramen", "Pizza"])]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([1, 1, 1], Some(1)), 2), "each option a vote chose counts once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_vote_that_comes_before_its_poll_waits_and_counts_once_the_poll_comes() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    apply(&store, vec![vote("p1", ANA, AT + 10, "v1", &["Ramen"]), vote("p1", RUI, AT + 11, "v2", &["Pizza"])]).await;
    assert_eq!(count(&store, "pending WHERE kind = 'vote'").await, 2, "as jawd status counts them");
    // Ana's newer vote comes before her older one is applied; the older never replaces it.
    apply(&store, vec![vote("p1", ANA, AT + 5, "v0", &["Tasca do Chico"])]).await;

    apply(&store, vec![poll("p1")]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([1, 0, 1], None), 2));
    assert_eq!(count(&store, "pending").await, 0, "nothing waits once applied");
}

#[tokio::test(flavor = "multi_thread")]
async fn past_its_bound_the_oldest_vote_waiting_is_set_aside_whole_and_counts_once_its_poll_comes() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // Ana's vote, which came twice, then the rest of the bound, votes for polls that never come.
    apply(&store, vec![vote("p1", ANA, AT + 10, "v1", &["Ramen"])]).await;
    apply(&store, vec![vote("p1", ANA, AT + 10, "v1", &["Ramen"])]).await;
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.execute(
        "WITH RECURSIVE k (i) AS (SELECT 3 UNION ALL SELECT i + 1 FROM k WHERE i < ?1)
        INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect)
        SELECT ?2, 'x' || i, 'vote', ?3, x'00', ?4, 0, json_object('do', 'vote', 'options', json_array(), 'id', 'w' || i)
        FROM k",
        rusqlite::params![VOTES_WAITING, GROUP, AT, RUI],
    )
    .unwrap();
    drop(raw);
    let bound = VOTES_WAITING as u64;
    assert_eq!(count(&store, "pending WHERE kind = 'vote'").await, bound);
    assert_eq!(count(&store, "quarantine").await, 0, "the bound itself is kept");

    apply(&store, vec![vote("p2", RUI, AT + 20, "v2", &["Pizza"])]).await;
    assert_eq!(count(&store, "pending WHERE kind = 'vote'").await, bound, "pending stays within its bound");
    assert_eq!(count(&store, "quarantine WHERE source = 'vote'").await, 1, "as jawd status counts it");
    let kept: String = query(&store, |db| {
        Ok(db.query_row("SELECT ref FROM quarantine WHERE source = 'vote'", [], |r| r.get::<_, String>(0))?)
    })
    .await;
    assert_eq!(kept, format!("{GROUP} p1 {ANA} v1"), "the oldest, Ana's, by its poll, its voter and its ID");

    // The second time it came is set aside next, as the same row.
    apply(&store, vec![vote("p3", RUI, AT + 30, "v3", &["Pizza"])]).await;
    assert_eq!(count(&store, "quarantine WHERE source = 'vote'").await, 1);
    assert_eq!(count(&store, "pending WHERE kind = 'vote'").await, bound);

    apply(&store, vec![poll("p1")]).await;
    let (options, voters, _) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([1, 0, 0], None), 1), "it comes back with its poll, and counts");
    assert_eq!(count(&store, "quarantine WHERE source = 'vote'").await, 0);
    assert_eq!(count(&store, "pending WHERE kind = 'vote'").await, bound);
}

/// A jaw.db at version 12, before votes were kept, with three of Ana's polls, one of them a vote
/// choosing any of and one whose raw protobuf doesn't read, converted then without their options
/// kept, and a message that isn't one.
fn before_votes(home: &std::path::Path) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..12] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 12).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, v) VALUES (1, '{GROUP}', 'group', 1);"
    ))
    .unwrap();
    let raw = |id: &str, selectable: u32| {
        let creation = e2e::PollCreationMessage {
            name: Some("Jantar?".into()),
            options: DINNER
                .iter()
                .map(|name| e2e::poll_creation_message::Option {
                    option_name: Some(name.to_string()),
                    ..Default::default()
                })
                .collect(),
            selectable_options_count: Some(selectable),
            ..Default::default()
        };
        let web = WebMessageInfo {
            key: MessageKey {
                remote_jid: Some(GROUP.into()),
                from_me: Some(false),
                id: Some(id.into()),
                participant: Some(ANA.into()),
            },
            message: Some(e2e::Message { poll_creation_message: Some(Box::new(creation)), ..Default::default() }),
            ..Default::default()
        };
        Packed::new(&web.encode_to_vec()).0
    };
    let detail = json!({ "poll": { "question": "Jantar?", "options": DINNER.iter()
        .map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect::<Vec<_>>() } })
    .to_string();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, summary, detail, raw, v)
            VALUES (?1, 1, ?2, ?3, ?3, 1, ?4, 'x', ?5, ?6, 1)",
        )
        .unwrap();
    insert.execute(rusqlite::params![1, "p1", AT + 1, "poll", detail, raw("p1", 1)]).unwrap();
    insert.execute(rusqlite::params![2, "t2", AT + 2, "text", None::<String>, None::<Vec<u8>>]).unwrap();
    insert.execute(rusqlite::params![3, "p3", AT + 3, "poll", detail, raw("p3", 0)]).unwrap();
    insert.execute(rusqlite::params![4, "p4", AT + 4, "poll", detail, Packed::new(b"not a poll").0]).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_jaw_held_before_votes_were_kept_is_given_its_options_by_the_backfill_or_first_by_a_vote() {
    let home = tempfile::tempdir().unwrap();
    before_votes(home.path());
    let store = opened(home.path()).await;
    assert_eq!(store.backfill_left(POLLS).unwrap(), 4, "the migration leaves the backfill to do, as status says");
    assert_eq!(count(&store, "poll_options").await, 0, "and reads no message itself");

    // A vote in p3 before the backfill reaches it gives it its options first, and counts.
    apply(&store, vec![vote("p3", RUI, AT + 10, "v1", &["Ramen", "Pizza"])]).await;
    let (options, voters, _) = shown(&store, "p3").await;
    assert_eq!((options, voters), (tally([1, 0, 1], None), 1));
    let max = |store: &Store, id: &'static str| {
        let store = store.clone();
        async move {
            query(&store, move |db| {
                Ok(db.query_row("SELECT detail ->> '$.poll.max' FROM messages WHERE id = ?1", [id], |r| {
                    r.get::<_, Option<i64>>(0)
                })?)
            })
            .await
        }
    };
    assert_eq!(max(&store, "p3").await, Some(3), "a vote in it may choose any of its options");

    let (_, _, before) = shown(&store, "p1").await;
    let applied = store.apply(Lane::Background, vec![Change::Backfill(POLLS)]).await.unwrap();
    assert!(applied.changed > 0);
    assert_eq!(store.backfill_left(POLLS).unwrap(), 0, "a step looks at more messages than these");
    assert_eq!(count(&store, "poll_options").await, 9, "each poll's options, p3's not twice");
    assert_eq!(max(&store, "p1").await, Some(1));
    let (options, voters, after) = shown(&store, "p1").await;
    assert_eq!((options, voters), (tally([0, 0, 0], None), 0));
    assert!(after > before, "what it shows changed, so it takes a new version");
    let (options, _, _) = shown(&store, "p3").await;
    assert_eq!(options, tally([1, 0, 1], None), "the vote that came first is as it was");
    let again = store.apply(Lane::Background, vec![Change::Backfill(POLLS)]).await.unwrap();
    assert_eq!(again.changed, 0, "a step after it's done changes nothing");

    // A poll whose raw protobuf doesn't read has the options its row shows, and votes count in it.
    assert_eq!(count(&store, "quarantine").await, 0, "nothing failed");
    apply(&store, vec![vote("p4", RUI, AT + 20, "v2", &["Pizza"])]).await;
    let (options, voters, _) = shown(&store, "p4").await;
    assert_eq!((options, voters), (tally([0, 0, 1], None), 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merged_polls_votes_are_each_voters_newest_of_both_copies() {
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    // Ana's poll in her chat under her number and a copy under her LID, each with votes.
    apply(
        &store,
        vec![Change::Message(poll_in(ANA, "p1", DINNER, 1)), Change::Message(poll_in(ANA_LID, "p1", DINNER, 1))],
    )
    .await;
    apply(
        &store,
        vec![
            vote_in(ANA, "p1", ANA, AT + 10, "a1", &["Ramen"]),
            vote_in(ANA_LID, "p1", ANA, AT + 20, "a2", &["Pizza"]),
            vote_in(ANA, "p1", ME, AT + 30, "m2", &["Tasca do Chico"]),
            vote_in(ANA_LID, "p1", ME, AT + 15, "m1", &["Ramen"]),
        ],
    )
    .await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    while count(&store, "chat_merges").await > 0 {
        apply(&store, vec![Change::MergeStep(None)]).await;
    }
    let kept: String = query(&store, |db| {
        Ok(db.query_row("SELECT c.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.id = 'p1'", [], |r| {
            r.get::<_, String>(0)
        })?)
    })
    .await;
    assert_eq!(count(&store, "messages WHERE id = 'p1'").await, 1, "one poll");
    let (options, voters, _) = shown_in(&store, kept.clone(), "p1").await;
    assert_eq!((options, voters), (tally([0, 1, 1], Some(1)), 2), "Ana's Pizza and your Tasca, the newest of each");
    // A vote of yours older than your newest, arriving now, still changes nothing.
    apply(&store, vec![vote_in(&kept, "p1", ME, AT + 20, "m3", &["Ramen"])]).await;
    let (options, _, _) = shown_in(&store, kept, "p1").await;
    assert_eq!(options, tally([0, 1, 1], Some(1)));
}

#[test]
fn an_options_hash_is_the_sha_256_of_its_name_and_reads_back_from_hex() {
    let hash = option_hash("abc");
    assert_eq!(hex(&hash), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(unhex(&hex(&hash)), Some(hash));
    assert_eq!(unhex("abc"), None, "an odd length");
    assert_eq!(unhex("zz"), None, "not hex");
}

/// A poll's raw protobuf as jaw keeps it, packed: `id` in chat 1's JID, asking about dinner.
fn packed_poll(chat: &str, id: &str) -> Vec<u8> {
    let creation = e2e::PollCreationMessage {
        name: Some("Jantar?".into()),
        options: DINNER
            .iter()
            .map(|name| e2e::poll_creation_message::Option {
                option_name: Some(name.to_string()),
                ..Default::default()
            })
            .collect(),
        selectable_options_count: Some(1),
        ..Default::default()
    };
    let web = WebMessageInfo {
        key: MessageKey { remote_jid: Some(chat.into()), from_me: Some(false), id: Some(id.into()), participant: None },
        message: Some(e2e::Message { poll_creation_message_v3: Some(Box::new(creation)), ..Default::default() }),
        ..Default::default()
    };
    Packed::new(&web.encode_to_vec()).0
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement: cargo test --release -p jawd measure_the_polls -- --ignored --nocapture"]
async fn measure_the_polls_backfill_and_what_votes_cost() {
    let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
    let home = tempfile::tempdir().unwrap();
    {
        let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..12] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 12).unwrap();
        db.execute_batch(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            INSERT INTO chats (n, id, kind, person, v) SELECT n, printf('3519%08d@s.whatsapp.net', n), 'direct', n, 1
            FROM people;",
        )
        .unwrap();
        // One message in five hundred a poll, the rest text, each with a raw protobuf.
        let shown = json!({ "poll": { "question": "Jantar?", "options": DINNER.iter()
            .map(|name| json!({ "name": name, "votes": 0, "mine": false })).collect::<Vec<_>>() } })
        .to_string();
        let filler = Packed::new(&[7u8; 300]).0;
        let writing = db.transaction().unwrap();
        {
            let mut insert = writing
                .prepare(
                    "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, summary, detail, raw, v)
                    VALUES (?1, ?2, ?3, ?4, ?4 << 20, ?2, ?5, 'x', 'x', ?6, ?7, ?1)",
                )
                .unwrap();
            for n in 1..=messages {
                let chat = 1 + n % 2000;
                let id = format!("3EB0{n:016}");
                let poll = n % 500 == 0;
                let raw =
                    if poll { packed_poll(&format!("3519{chat:08}@s.whatsapp.net"), &id) } else { filler.clone() };
                let (kind, detail) = if poll { ("poll", Some(shown.as_str())) } else { ("text", None) };
                insert.execute(rusqlite::params![n, chat, id, AT + n * 1000, kind, detail, raw]).unwrap();
            }
        }
        writing.commit().unwrap();
        db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    }
    let began = std::time::Instant::now();
    let store = Store::open(home.path()).unwrap();
    let migrated = began.elapsed();
    let began = std::time::Instant::now();
    let mut transactions = 0;
    while store.backfill_left(POLLS).unwrap() > 0 {
        transactions += store.apply(Lane::Background, vec![Change::Backfill(POLLS); 64]).await.unwrap().transactions;
    }
    let backfilled = began.elapsed();
    let runs = store.report().run.unwrap();
    eprintln!(
        "{messages} messages, {} polls: migrated in {:.1} ms; polls backfilled in {:.2} s, {transactions} transactions, \
         each {:.2} ms p50 and {:.2} ms p99 with its commit",
        messages / 500,
        migrated.as_secs_f64() * 1e3,
        backfilled.as_secs_f64(),
        runs.p50_us as f64 / 1e3,
        runs.p99_us as f64 / 1e3,
    );

    // A chat whose newest hundred messages are polls, each with fifty voters, read as a window.
    let home = tempfile::tempdir().unwrap();
    let store = opened(home.path()).await;
    let polls: Vec<Change> = (0..100)
        .map(|k| Change::Message(Message { at: AT + k, ..poll_in(GROUP, &format!("p{k}"), DINNER, 1) }))
        .collect();
    apply(&store, polls).await;
    let votes: Vec<Change> = (0..100)
        .flat_map(|k| {
            (0..50).map(move |v| {
                let voter = format!("35192{v:07}@s.whatsapp.net");
                vote(&format!("p{k}"), &voter, AT + 1000 + v, &format!("v{k}-{v}"), &[DINNER[(v % 3) as usize]])
            })
        })
        .collect();
    let began = std::time::Instant::now();
    store.apply(Lane::Background, votes).await.unwrap();
    let voted = began.elapsed();
    let mut reads = Vec::new();
    for _ in 0..50 {
        let began = std::time::Instant::now();
        let rows = query(&store, |db| Ok(read::messages(db, GROUP, Around::Newest, 100)?.len())).await;
        reads.push(began.elapsed());
        assert_eq!(rows, 100);
    }
    reads.sort();
    eprintln!(
        "5,000 votes in 100 polls: {:.1} us a vote; a window of the 100 polls read in {:.2} ms p50, {:.2} ms worst of 50",
        voted.as_secs_f64() * 1e6 / 5000.0,
        reads[25].as_secs_f64() * 1e3,
        reads[49].as_secs_f64() * 1e3,
    );
}
