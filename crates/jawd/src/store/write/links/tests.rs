use rusqlite::Connection;
use serde_json::json;

use super::*;
use crate::store::read::{self, Around};
use crate::store::{Amend, Amendment, Applied, Change, Lane, Message, Person, Store, schema};
use crate::work::{Priority, Report};

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const AT: i64 = 1_792_000_000_000;

/// A link of `links` as a test compares it: its message's ID, its place, its chat's JID, its time,
/// its URL, host, title and description.
type Link = (String, i64, String, i64, String, String, Option<String>, Option<String>);

async fn query<T: Send + 'static>(store: &Store, query: impl FnOnce(&Connection) -> Result<T> + Send + 'static) -> T {
    store.reads().query(Priority::Interactive, query).await.unwrap()
}

async fn apply(store: &Store, changes: Vec<Change>) -> Applied {
    store.apply(Lane::Live, changes).await.unwrap()
}

async fn links(store: &Store) -> Vec<Link> {
    query(store, |db| {
        Ok(db
            .prepare(
                "SELECT m.id, l.place, c.id, l.at, l.url, l.host, l.title, l.description
                FROM links l JOIN messages m ON m.n = l.message JOIN chats c ON c.n = l.chat ORDER BY m.id, l.place",
            )?
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })?
            .collect::<rusqlite::Result<_>>()?)
    })
    .await
}

/// The messages whose links match `words`, newest first, by ID.
async fn found(store: &Store, words: &'static str) -> Vec<String> {
    query(store, move |db| {
        let ts = read::linked(db, words, None, 100)?;
        let mut ids = Vec::new();
        for t in ts {
            ids.push(db.query_row("SELECT id FROM messages WHERE t = ?1", [t], |r| r.get(0))?);
        }
        Ok(ids)
    })
    .await
}

fn link(s: &str) -> serde_json::Value {
    let url = if s.starts_with("www.") { format!("https://{s}") } else { s.to_string() };
    json!({ "t": "link", "s": s, "url": url })
}

fn text(s: &str) -> serde_json::Value {
    json!({ "t": "text", "s": s })
}

/// Ana's message `id`, whose spans are `spans`, with a link preview of `preview` if it has one.
fn message(id: &str, at: i64, spans: serde_json::Value, preview: Option<serde_json::Value>) -> Message {
    let words: String = spans.as_array().unwrap().iter().map(|span| span["s"].as_str().unwrap()).collect();
    Message {
        chat: ANA.into(),
        id: id.into(),
        at,
        sender: Some(ANA.into()),
        kind: "text",
        text: Some(words.clone()),
        summary: words,
        spans: Some(spans.to_string()),
        detail: preview.map(|link| json!({ "link": link }).to_string()),
        live: true,
        ..Message::default()
    }
}

fn owned(id: &str, place: i64, at: i64, url: &str, host: &str, title: Option<&str>, description: Option<&str>) -> Link {
    (id.into(), place, ANA.into(), at, url.into(), host.into(), title.map(Into::into), description.map(Into::into))
}

#[tokio::test(flavor = "multi_thread")]
async fn each_url_a_message_gives_is_a_link_once_in_its_order_with_its_host_and_its_previews_title() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let spans = json!([
        text("olha "),
        link("https://www.Publico.pt/sociedade?x=1#topo"),
        text(" e "),
        link("www.jaw.dev/sobre"),
        text(" e "),
        link("https://ana:segredo@Exemplo.COM:8080/p"),
        text(" e de novo "),
        link("https://www.Publico.pt/sociedade?x=1#topo"),
    ]);
    let preview = json!({ "url": "www.jaw.dev/sobre", "title": "Jaw, um cliente", "description": "Rápido." });
    let bare = json!({ "url": "jaw.dev", "title": "Só a pré-visualização", "description": null });
    apply(
        &store,
        vec![
            Change::Message(message("m1", AT, spans, Some(preview))),
            Change::Message(message("m2", AT + 1, json!([text("vê jaw.dev")]), Some(bare))),
            Change::Message(message("m3", AT + 2, json!([text("nada aqui")]), None)),
        ],
    )
    .await;
    assert_eq!(
        links(&store).await,
        [
            owned("m1", 0, AT, "https://www.Publico.pt/sociedade?x=1#topo", "www.publico.pt", None, None),
            owned("m1", 1, AT, "https://www.jaw.dev/sobre", "www.jaw.dev", Some("Jaw, um cliente"), Some("Rápido.")),
            owned("m1", 2, AT, "https://ana:segredo@Exemplo.COM:8080/p", "exemplo.com", None, None),
            owned("m2", 0, AT + 1, "https://jaw.dev", "jaw.dev", Some("Só a pré-visualização"), None),
        ]
    );
    let sender: Option<String> = query(&store, |db| {
        Ok(db.query_row(
            "SELECT j.jid FROM links l JOIN person_jids j ON j.n = l.sender WHERE l.place = 1",
            [],
            |r| r.get(0),
        )?)
    })
    .await;
    assert_eq!(sender.as_deref(), Some(ANA), "who sent it");
    assert_eq!(found(&store, "\"cliente\"").await, ["m1"], "by the preview's title");
    assert_eq!(found(&store, "\"rapido\"").await, ["m1"], "by its description, accent-blind");
    assert_eq!(found(&store, "\"exemplo\"").await, ["m1"], "by its host");
    assert_eq!(found(&store, "\"jaw\"").await, ["m2", "m1"], "newest first");
    assert_eq!(found(&store, "\"sociedade\"").await, ["m1"], "by its URL");
    assert!(found(&store, "\"nada\"").await.is_empty(), "a message's own text isn't a link's");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_messages_links_follow_it_through_a_copy_an_edit_a_deletion_and_a_merge() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let shown = |at| owned("m1", 0, at, "https://www.jaw.dev", "www.jaw.dev", None, None);
    apply(&store, vec![Change::Message(message("m1", AT, json!([text("vê "), link("www.jaw.dev")]), None))]).await;
    assert_eq!(links(&store).await, [shown(AT)]);

    // A copy whose time is history's, to the second, moves its link's time with it.
    let mut copy = message("m1", AT - 400, json!([text("vê "), link("www.jaw.dev")]), None);
    copy.live = false;
    apply(&store, vec![Change::Message(copy)]).await;
    assert_eq!(links(&store).await, [shown(AT - 400)]);

    let edit = |spans: serde_json::Value, at: i64| {
        let words: String = spans.as_array().unwrap().iter().map(|span| span["s"].as_str().unwrap()).collect();
        Change::Amend(Amend {
            chat: ANA.into(),
            target: "m1".into(),
            sender: Some(ANA.into()),
            mine: false,
            at,
            what: Amendment::Edit { text: Some(words.clone()), spans: Some(spans.to_string()), summary: words },
            raw: None,
        })
    };
    apply(&store, vec![edit(json!([text("afinal "), link("https://jaw.dev/novo")]), AT + 10)]).await;
    assert_eq!(links(&store).await, [owned("m1", 0, AT - 400, "https://jaw.dev/novo", "jaw.dev", None, None)]);
    assert!(found(&store, "\"www\"").await.is_empty(), "the old link's words went with it");
    assert_eq!(found(&store, "\"novo\"").await, ["m1"]);

    // Her number and LID found to be one person merge her chats; the message moves, and its link.
    apply(
        &store,
        vec![Change::Message(Message {
            chat: ANA_LID.into(),
            sender: Some(ANA_LID.into()),
            ..message("l1", AT + 20, json!([link("www.lid.pt")]), None)
        })],
    )
    .await;
    apply(&store, vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await;
    while query(&store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?)).await
        > 0
    {
        apply(&store, vec![Change::MergeStep(None)]).await;
    }
    let moved = links(&store).await;
    assert_eq!(moved[0], owned("l1", 0, AT + 20, "https://www.lid.pt", "www.lid.pt", None, None), "{moved:?}");

    let revoke = Amend {
        chat: ANA.into(),
        target: "m1".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: AT + 30,
        what: Amendment::Revoke,
        raw: None,
    };
    apply(&store, vec![Change::Amend(revoke)]).await;
    assert_eq!(links(&store).await.len(), 1, "a deletion for everyone takes its links");
    assert!(found(&store, "\"novo\"").await.is_empty());
    apply(&store, vec![Change::DeleteMessage { chat: ANA.into(), id: "l1".into() }]).await;
    assert!(links(&store).await.is_empty(), "and so does a deletion for you");
    assert!(found(&store, "\"lid\"").await.is_empty());
    let check = Connection::open(home.path().join("jaw.db")).unwrap();
    check.execute("INSERT INTO links_fts (links_fts) VALUES ('integrity-check')", []).unwrap();
}

/// A jaw.db at version 11, before links were kept, with `count` of Ana's messages, every third with
/// a link, and one with a link preview.
fn before_links(home: &std::path::Path, count: i64) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.pragma_update(None, "journal_mode", "WAL").unwrap();
    for migration in &schema::MIGRATIONS[..11] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 11).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, '{ANA}', 1);
        INSERT INTO chats (n, id, kind, person, v) VALUES (1, '{ANA}', 'direct', 1, 1);"
    ))
    .unwrap();
    let mut insert = db
        .prepare(
            "INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, detail, v)
            VALUES (?1, 1, ?2, ?3, ?3, 1, 'text', 'x', ?4, ?5, 1)",
        )
        .unwrap();
    for n in 1..=count {
        let spans = (n % 3 == 0).then(|| json!([text("vê "), link(&format!("https://site{n}.pt/a"))]).to_string());
        let detail = (n == 7).then(|| json!({ "link": { "url": "jaw.dev", "title": "Jaw" } }).to_string());
        insert.execute(rusqlite::params![n, format!("m{n}"), AT + n, spans, detail]).unwrap();
    }
}

async fn backfill_left(store: &Store) -> Option<(i64, i64)> {
    query(store, |db| {
        Ok(db
            .query_row("SELECT after, upto FROM backfills WHERE name = 'links'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_messages_held_before_links_were_kept_are_linked_a_short_step_at_a_time_and_none_twice() {
    let home = tempfile::tempdir().unwrap();
    before_links(home.path(), 600);
    let store = Store::open(home.path()).unwrap();
    assert_eq!(backfill_left(&store).await, Some((0, 600)), "the migration leaves the backfill to do");
    assert_eq!(store.backfill_left(LINKS).unwrap(), 600, "as jawd status says");
    assert!(links(&store).await.is_empty(), "the migration itself reads no message");

    // Changed before the backfill reaches it, a message is linked by the triggers, and the backfill
    // leaves it as it is.
    let mut copy = message("m300", AT + 300, json!([text("vê "), link("https://site300.pt/a")]), None);
    copy.sender = Some(ANA.into());
    apply(&store, vec![Change::Message(Message { at: AT + 299, ..copy })]).await;
    assert_eq!(links(&store).await.len(), 1);

    store.apply(Lane::Background, vec![Change::Backfill(LINKS)]).await.unwrap();
    assert_eq!(backfill_left(&store).await, Some((STEP, 600)), "a step looks at {STEP} messages");
    let first = links(&store).await;
    let expected = STEP as usize / 3 + 1 + 1;
    assert_eq!(first.len(), expected, "the first step's messages with a link, m7's preview and m300: {first:?}");
    drop(store);

    // Started again, it carries on from where it was.
    let store = Store::open(home.path()).unwrap();
    store.apply(Lane::Background, vec![Change::Backfill(LINKS); 600 / STEP as usize + 1]).await.unwrap();
    assert_eq!(backfill_left(&store).await, None, "done, its row goes");
    let all = links(&store).await;
    assert_eq!(all.len(), 200 + 1, "every third of 600, and m7's preview: {}", all.len());
    assert!(all.contains(&owned("m7", 0, AT + 7, "https://jaw.dev", "jaw.dev", Some("Jaw"), None)));
    assert!(all.contains(&owned("m300", 0, AT + 299, "https://site300.pt/a", "site300.pt", None, None)));
    assert_eq!(found(&store, "\"site599\"").await, Vec::<String>::new());
    assert_eq!(found(&store, "\"site597\"").await, ["m597"]);
    let docs: i64 = query(&store, |db| {
        Ok(db.query_row(
            "SELECT count(*) FROM messages m WHERE EXISTS (SELECT 1 FROM links_fts WHERE rowid = m.t)",
            [],
            |r| r.get(0),
        )?)
    })
    .await;
    assert_eq!(docs, 201, "one document for each message with links");

    let again = store.apply(Lane::Background, vec![Change::Backfill(LINKS)]).await.unwrap();
    assert_eq!(again.changed, 0);
    assert_eq!(links(&store).await, all, "a step after it's done changes nothing");
    assert_eq!(store.backfill_left(LINKS).unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_backfill_step_that_fails_is_kept_in_quarantine_and_leaves_the_backfill_where_it_was() {
    let home = tempfile::tempdir().unwrap();
    before_links(home.path(), 300);
    let store = Store::open(home.path()).unwrap();
    // A backfill a later jawd left, which this one doesn't know how to take a step of.
    let raw = Connection::open(home.path().join("jaw.db")).unwrap();
    raw.execute("INSERT INTO backfills (name, after, upto) VALUES ('ghost', 0, 1)", []).unwrap();
    drop(raw);
    let applied = store.apply(Lane::Background, vec![Change::Backfill("ghost")]).await.unwrap();
    assert_eq!(applied.quarantined, 1, "a backfill this jawd doesn't know fails, and is kept in quarantine");
    let ghost: Option<(i64, i64)> = query(&store, |db| {
        Ok(db
            .query_row("SELECT after, upto FROM backfills WHERE name = 'ghost'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })
    .await;
    assert_eq!(ghost, Some((0, 1)), "its row is where it was");
    let kept: i64 = query(&store, |db| {
        Ok(db.query_row("SELECT count(*) FROM quarantine WHERE source = 'backfill' AND ref = 'ghost'", [], |r| {
            r.get(0)
        })?)
    })
    .await;
    assert_eq!(kept, 1);
    store.apply(Lane::Background, vec![Change::Backfill(LINKS); 300 / STEP as usize + 1]).await.unwrap();
    assert_eq!(backfill_left(&store).await, None);
    assert_eq!(links(&store).await.len(), 100 + 1, "the links backfill goes on regardless");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thumbnail_shows_on_its_preview_once_its_row_is_in_and_its_message_takes_a_new_version() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let sha = "ab".repeat(32);
    let preview = json!({ "url": "https://jaw.dev", "title": "Jaw", "thumbnail": sha });
    let sent =
        apply(&store, vec![Change::Message(message("m1", AT, json!([link("https://jaw.dev")]), Some(preview)))]).await;
    let row = |store: &Store| {
        let store = store.clone();
        async move { query(&store, |db| Ok(read::messages(db, ANA, Around::Newest, 10)?.remove(0))).await }
    };
    let before = row(&store).await;
    let detail: serde_json::Value = serde_json::from_str(before.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["link"], json!({ "url": "https://jaw.dev", "title": "Jaw" }), "no file yet, and no hash");
    let kept =
        apply(&store, vec![Change::Thumbnail { sha256: sha.clone(), bytes: 1234, chat: ANA.into(), id: "m1".into() }])
            .await;
    let after = row(&store).await;
    let detail: serde_json::Value = serde_json::from_str(after.detail.as_ref().unwrap().get()).unwrap();
    assert_eq!(detail["link"]["preview"], format!("media/links/{sha}.jpg"));
    assert!(detail["link"].get("thumbnail").is_none(), "the hash that names it stays inside jawd");
    assert!(before.v <= sent.seq && after.v > sent.seq && after.v <= kept.seq, "a new version once it shows");
    let again =
        apply(&store, vec![Change::Thumbnail { sha256: sha, bytes: 1234, chat: ANA.into(), id: "m1".into() }]).await;
    assert_eq!((again.changed, row(&store).await.v), (0, after.v), "kept again, nothing changes");
}

/// Measures the links backfill on a jaw.db of many messages from before links were kept, and what
/// keeping links costs history as it's written:
/// `JAW_MEASURE_MESSAGES=1000000 cargo test --release -p jawd measure_the_links -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a measurement, a minute long at a million messages"]
async fn measure_the_links_backfill_and_what_links_cost_a_write() {
    let messages: i64 = std::env::var("JAW_MEASURE_MESSAGES").ok().and_then(|n| n.parse().ok()).unwrap_or(200_000);
    let home = tempfile::tempdir().unwrap();
    {
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        for migration in &schema::MIGRATIONS[..11] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", 11).unwrap();
        // One message in thirty-three with a link, one in a hundred with a preview too, as a busy
        // account's chats have them.
        db.execute_batch(&format!(
            "BEGIN;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2000)
            INSERT INTO people (n, name, v) SELECT x, 'Person ' || x, 1 FROM c;
            INSERT INTO person_jids (n, jid, person) SELECT n, printf('3519%08d@s.whatsapp.net', n), n FROM people;
            INSERT INTO chats (n, id, kind, person, v) SELECT n, printf('3519%08d@s.whatsapp.net', n), 'direct', n, 1 FROM people;
            WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {messages})
            INSERT INTO messages (n, chat, id, at, t, sender, kind, text, spans, detail, raw, v)
            SELECT x, 1 + x % 2000, printf('3EB0%016d', x), {AT} + x * 1000, ({AT} + x * 1000) << 20, 1 + x % 2000, 'text',
                'mensagem ' || x,
                iif(x % 33 = 0, json_array(json_object('t', 'text', 's', 'vê '),
                    json_object('t', 'link', 's', 'https://site' || x || '.pt/a', 'url', 'https://site' || x || '.pt/a')), NULL),
                iif(x % 100 = 0, json_object('link', json_object('url', 'https://site' || x || '.pt/a', 'title', 'Título ' || x)), NULL),
                randomblob(300), x
            FROM c;
            COMMIT;
            PRAGMA wal_checkpoint(TRUNCATE);"
        ))
        .unwrap();
    }
    let began = std::time::Instant::now();
    let store = Store::open(home.path()).unwrap();
    let migrated = began.elapsed();
    let began = std::time::Instant::now();
    let mut transactions = 0;
    while backfill_left(&store).await.is_some() {
        transactions += store.apply(Lane::Background, vec![Change::Backfill(LINKS); 64]).await.unwrap().transactions;
    }
    let backfilled = began.elapsed();
    let runs = store.report().run.unwrap();
    let linked = store.count("links").unwrap();
    eprintln!(
        "{messages} messages: migrated in {:.1} ms; backfilled {linked} links in {:.2} s ({:.0} messages a second), \
         {transactions} transactions, each {:.2} ms p50 and {:.2} ms p99 with its commit",
        migrated.as_secs_f64() * 1e3,
        backfilled.as_secs_f64(),
        messages as f64 / backfilled.as_secs_f64(),
        runs.p50_us as f64 / 1e3,
        runs.p99_us as f64 / 1e3,
    );

    // History written through the writer: with no links, with one in twenty, and with one in twenty
    // but their words not indexed, each on a jaw.db of its own.
    for (every, indexed) in [(0, true), (20, true), (20, false)] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        if !indexed {
            let side = Connection::open(home.path().join("jaw.db")).unwrap();
            let sql: String = side
                .query_row("SELECT sql FROM sqlite_schema WHERE name = 'links_derived_insert'", [], |r| r.get(0))
                .unwrap();
            let unindexed = sql.split("INSERT INTO links_fts").next().unwrap().to_string() + "END";
            side.execute_batch(&format!("DROP TRIGGER links_derived_insert; {unindexed};")).unwrap();
        }
        let batch: Vec<Change> = (0..20_000i64)
            .map(|k| {
                let spans = if every > 0 && k % every == 0 {
                    json!([text("vê "), link(&format!("https://novo{k}.pt/a"))])
                } else {
                    json!([text(&format!("mensagem {k}"))])
                };
                let mut m = message(&format!("h{k}"), AT - k * 1000, spans, None);
                m.live = false;
                Change::Message(m)
            })
            .collect();
        let began = std::time::Instant::now();
        store.apply(Lane::Background, batch).await.unwrap();
        let took = began.elapsed();
        eprintln!(
            "20,000 messages of history, {}{}: {:.2} s, {:.0} a second",
            if every == 0 { "none with a link".to_string() } else { format!("one in {every} with a link") },
            if indexed { "" } else { ", links not indexed" },
            took.as_secs_f64(),
            20_000.0 / took.as_secs_f64()
        );
    }
}
