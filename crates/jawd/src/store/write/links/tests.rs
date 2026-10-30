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

/// The messages whose links match `words`, newest first, by ID, once what was written is indexed.
async fn found(store: &Store, words: &'static str) -> Vec<String> {
    store.indexed().await.unwrap();
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
    let mut steps = store.merge_steps();
    while query(&store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?)).await
        > 0
    {
        steps.take("the merge never finished");
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
        let applied = store.apply(Lane::Background, vec![Change::Backfill(LINKS); 64]).await.unwrap();
        assert_eq!(applied.quarantined, 0, "a step failed");
        transactions += applied.transactions;
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

/// `links` is exactly what the views derive from every message, and links_fts holds exactly one
/// document for each message with links that isn't waiting in fts_backlog, its postings those of a
/// fresh index of their text, and none for one that is; `at` says where, if not.
fn links_exact(home: &std::path::Path, at: &str) {
    let db = Connection::open(home.join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    db.execute_batch("BEGIN").unwrap();
    let rows = |sql: &str| -> Vec<String> {
        db.prepare(sql)
            .unwrap()
            .query_map([], |r| {
                Ok((0..r.as_ref().column_count()).map(|i| format!("{:?}|", r.get_ref(i).unwrap())).collect::<String>())
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    let columns = "message, place, chat, at, sender, mine, url, host, title, description";
    let kept = rows(&format!("SELECT {columns} FROM links ORDER BY message, place"));
    assert_eq!(kept, rows(&format!("SELECT {columns} FROM message_links ORDER BY message, place")), "links at {at}");
    db.execute_batch(
        "DROP TABLE IF EXISTS temp.postings; DROP TABLE IF EXISTS temp.fresh_postings; DROP TABLE IF EXISTS temp.fresh;
        CREATE VIRTUAL TABLE temp.postings USING fts5vocab (main, links_fts, 'instance');
        CREATE VIRTUAL TABLE temp.fresh USING fts5 (text, content = '', contentless_delete = 1,
            tokenize = 'unicode61 remove_diacritics 2');
        INSERT INTO temp.fresh (rowid, text) SELECT t, text FROM message_link_text
            WHERE text IS NOT NULL AND message NOT IN (SELECT n FROM main.fts_backlog);
        CREATE VIRTUAL TABLE temp.fresh_postings USING fts5vocab (temp, fresh, 'instance');",
    )
    .unwrap();
    let postings = "term, doc, col, offset";
    assert_eq!(
        rows(&format!("SELECT {postings} FROM temp.postings ORDER BY {postings}")),
        rows(&format!("SELECT {postings} FROM temp.fresh_postings ORDER BY {postings}")),
        "links_fts's postings at {at}"
    );
    let documents = rows("SELECT rowid FROM links_fts ORDER BY rowid");
    let linked = rows(
        "SELECT m.t FROM messages m WHERE EXISTS (SELECT 1 FROM links l WHERE l.message = m.n)
            AND m.n NOT IN (SELECT n FROM fts_backlog) ORDER BY m.t",
    );
    assert_eq!(documents, linked, "links_fts's documents at {at}");
    db.execute_batch("COMMIT").unwrap();
    db.execute("INSERT INTO links_fts (links_fts) VALUES ('integrity-check')", []).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_messages_links_wait_with_its_text_to_be_indexed_and_are_found_as_they_last_read() {
    let home = tempfile::tempdir().unwrap();
    let quiet = std::time::Duration::from_secs(5);
    let limits = crate::store::Limits { live_pause: quiet, stream_step: quiet, ..crate::store::LIMITS };
    let store = Store::open_with(home.path(), limits).unwrap();
    // A catch-up's conversions, several inbox rows each, keep indexing waiting meanwhile.
    let converting = |changes: Vec<Change>| {
        let mut changes = changes;
        changes.extend([
            Change::Drained(crate::store::Drained::Inbox(1)),
            Change::Drained(crate::store::Drained::Inbox(2)),
        ]);
        changes
    };
    apply(&store, converting(vec![Change::Message(message("m1", AT, json!([text("vê "), link("jaw.dev/um")]), None))]))
        .await;
    assert_eq!(links(&store).await.len(), 1, "its links are kept as it commits");
    let waiting = |store: &Store| {
        let store = store.clone();
        async move {
            query(&store, |db| {
                Ok(db.query_row(
                    "SELECT (SELECT count(*) FROM fts_backlog), (SELECT count(*) FROM links_fts)",
                    [],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
                )?)
            })
            .await
        }
    };
    assert_eq!(waiting(&store).await, (1, 0), "its document waits with its text, not in the commit");
    links_exact(home.path(), "m1 waiting");
    // Written, edited and deleted before they're indexed, in one transaction and in several.
    let edit = |id: &str, spans: serde_json::Value, at: i64| {
        let words: String = spans.as_array().unwrap().iter().map(|span| span["s"].as_str().unwrap()).collect();
        Change::Amend(Amend {
            chat: ANA.into(),
            target: id.into(),
            sender: Some(ANA.into()),
            mine: false,
            at,
            what: Amendment::Edit { text: Some(words.clone()), spans: Some(spans.to_string()), summary: words },
            raw: None,
        })
    };
    apply(
        &store,
        converting(vec![
            Change::Message(message("m2", AT + 1, json!([link("https://antes.pt")]), None)),
            edit("m2", json!([text("depois "), link("https://depois.pt")]), AT + 2),
            Change::Message(message("m3", AT + 3, json!([link("https://apagado.pt")]), None)),
        ]),
    )
    .await;
    apply(&store, converting(vec![Change::DeleteMessage { chat: ANA.into(), id: "m3".into() }])).await;
    let preview = json!({ "url": "https://previsto.pt", "title": "Só o título" });
    apply(&store, converting(vec![Change::Message(message("m4", AT + 4, json!([text("olá")]), Some(preview)))])).await;
    assert_eq!(waiting(&store).await, (3, 0));
    links_exact(home.path(), "all waiting");
    store.indexed().await.unwrap();
    assert_eq!(waiting(&store).await, (0, 3), "indexed by the writer's own steps");
    links_exact(home.path(), "indexed");
    assert_eq!(found(&store, "\"depois\"").await, ["m2"], "as it last read");
    assert!(found(&store, "\"antes\"").await.is_empty());
    assert!(found(&store, "\"apagado\"").await.is_empty());
    assert_eq!(found(&store, "\"titulo\"").await, ["m4"], "a preview's title, accent-blind");
    // An edit once it's indexed rewrites its document in its own commit, as an edit of its text does.
    apply(&store, vec![edit("m1", json!([text("vê "), link("jaw.dev/dois")]), AT + 10)]).await;
    links_exact(home.path(), "m1 edited");
    assert_eq!(found(&store, "\"dois\"").await, ["m1"]);
    assert!(found(&store, "\"um\"").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_still_waiting_to_be_indexed_when_the_backfill_links_it_gets_one_document() {
    let home = tempfile::tempdir().unwrap();
    before_links(home.path(), 300);
    let store = Store::open(home.path()).unwrap();
    // History converted before links, the first step's messages not indexed yet: they wait in
    // fts_backlog, which the writer, finding it empty as it started, doesn't look at before a commit.
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO messages_fts (messages_fts, rowid, text) SELECT 'delete', t, text FROM messages WHERE n <= {STEP};
        INSERT INTO fts_backlog (n) SELECT n FROM messages WHERE n <= {STEP};"
    ))
    .unwrap();
    store.apply(Lane::Background, vec![Change::Backfill(LINKS)]).await.unwrap();
    // The writer indexes what waits as soon as it's idle, so it may have taken some of the step's
    // messages by now: each of those has its document, and none still waiting has one.
    let (linked, indexed, documents, waiting): (i64, i64, i64, i64) = db
        .query_row(
            "SELECT (SELECT count(DISTINCT message) FROM links),
                (SELECT count(DISTINCT message) FROM links WHERE message NOT IN (SELECT n FROM fts_backlog)),
                (SELECT count(*) FROM links_fts),
                (SELECT count(*) FROM links_fts WHERE rowid IN (SELECT m.t FROM fts_backlog b JOIN messages m ON m.n = b.n))",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(linked, STEP / 3 + 1, "the first step's links are in");
    assert_eq!((documents, waiting), (indexed, 0), "their words wait for the index step");
    let mut steps = store.backfill_steps(LINKS);
    while backfill_left(&store).await.is_some() {
        steps.take("the backfill never ended");
        store.apply(Lane::Background, vec![Change::Backfill(LINKS)]).await.unwrap();
    }
    store.indexed().await.unwrap();
    links_exact(home.path(), "backfilled and indexed");
    assert_eq!(found(&store, "\"site63\"").await, ["m63"], "once, from the index step");
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

const URLS: &[&str] = &[
    "https://jaw.dev",
    "jaw.dev",
    "www.jaw.dev/sobre",
    "https://www.jaw.dev/sobre",
    "http://exemplo.pt:8080/a?b=1#c",
    "HTTPS://Maiusculas.PT/X",
    "https://ana:segredo@anfitriao.pt/x",
    "https://[2001:db8::1]:8443/v6",
    "outro.pt/caminho",
];

fn random_spans(rng: &mut Rng) -> (serde_json::Value, Vec<&'static str>) {
    let mut spans = vec![text("olha ")];
    let mut used = Vec::new();
    for _ in 0..rng.below(4) {
        let url = URLS[rng.below(URLS.len() as u64) as usize];
        spans.extend([link(url), text(" e ")]);
        used.push(url);
    }
    (serde_json::Value::Array(spans), used)
}

fn random_message(rng: &mut Rng, chat: &str, sender: &str, id: &str, at: i64) -> Message {
    let (spans, used) = random_spans(rng);
    let preview = match rng.below(4) {
        0 if !used.is_empty() => Some(json!({ "url": used[rng.below(used.len() as u64) as usize],
            "title": format!("Titulo {}", rng.below(5)), "description": null })),
        1 => Some(json!({ "url": URLS[rng.below(URLS.len() as u64) as usize], "title": null,
            "description": format!("Descricao {}", rng.below(5)) })),
        _ => None,
    };
    Message { chat: chat.into(), sender: Some(sender.into()), ..message(id, at, spans, preview) }
}

fn random_edit(rng: &mut Rng, chat: &str, sender: &str, id: &str, at: i64) -> Change {
    let (spans, _) = random_spans(rng);
    let words: String = spans.as_array().unwrap().iter().map(|span| span["s"].as_str().unwrap()).collect();
    Change::Amend(Amend {
        chat: chat.into(),
        target: id.into(),
        sender: Some(sender.into()),
        mine: false,
        at,
        what: Amendment::Edit { text: Some(words.clone()), spans: Some(spans.to_string()), summary: words },
        raw: None,
    })
}

/// The database expert's scene: random copies, edits, deletions for everyone and for you, replays
/// and merges of Ana's two chats, with indexing as the writer does it between them, and links and
/// their index checked exactly after each.
#[tokio::test(flavor = "multi_thread")]
async fn links_and_their_index_stay_exact_through_random_copies_edits_deletions_replays_merges_and_indexing() {
    const GROUP: &str = "120363000000000001@g.us";
    const RUI: &str = "351910000003@s.whatsapp.net";
    for seed in [0x5eed, 2] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let mut rng = Rng(seed);
        let (mut done, mut at, mut joined) = (Vec::<Change>::new(), AT, false);
        for step in 0..100 {
            at += 1 + rng.below(50) as i64;
            let (chat, sender) = [(ANA, ANA), (ANA_LID, ANA_LID), (GROUP, RUI)][rng.below(3) as usize];
            let id = format!("m{}", rng.below(12));
            let change = match rng.below(100) {
                0..=34 => {
                    let sent = at - rng.below(3000) as i64;
                    let mut m = random_message(&mut rng, chat, sender, &id, sent);
                    m.live = rng.below(2) == 0;
                    Change::Message(m)
                }
                35..=54 => random_edit(&mut rng, chat, sender, &id, at),
                55..=64 => Change::Amend(Amend {
                    chat: chat.into(),
                    target: id,
                    sender: Some(sender.into()),
                    mine: false,
                    at,
                    what: Amendment::Revoke,
                    raw: None,
                }),
                65..=72 => Change::DeleteMessage { chat: chat.into(), id },
                73..=84 if !done.is_empty() => done[rng.below(done.len() as u64) as usize].clone(),
                85..=89 => {
                    store.indexed().await.unwrap();
                    links_exact(home.path(), &format!("seed {seed} step {step}, indexed"));
                    continue;
                }
                90..=94 if !joined => {
                    joined = true;
                    Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })
                }
                _ => Change::MergeStep(None),
            };
            let applied = store.apply(Lane::Live, vec![change.clone()]).await.unwrap();
            assert_eq!(applied.quarantined, 0, "seed {seed} step {step}: {change:?}");
            done.push(change);
            links_exact(home.path(), &format!("seed {seed} step {step}"));
        }
        let mut steps = store.merge_steps();
        while query(&store, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get::<_, i64>(0))?))
            .await
            > 0
        {
            steps.take(&format!("seed {seed}: the merge never finished"));
            store.apply(Lane::Live, vec![Change::MergeStep(None)]).await.unwrap();
        }
        store.indexed().await.unwrap();
        links_exact(home.path(), &format!("seed {seed}, merged and indexed"));
    }
}

/// The database expert's scene: the backfill, taken with live writes meanwhile, jawd stopped
/// between steps and a step taken again, links every message once.
#[tokio::test(flavor = "multi_thread")]
async fn the_backfill_among_writes_restarts_and_repeats_leaves_links_and_their_index_exact() {
    let home = tempfile::tempdir().unwrap();
    before_links(home.path(), 700);
    let mut rng = Rng(7);
    let mut store = Store::open(home.path()).unwrap();
    let (mut step, mut steps) = (0, store.backfill_steps(LINKS));
    while store.backfill_left(LINKS).unwrap() > 0 {
        steps.take("the backfill never ended");
        step += 1;
        for _ in 0..3 {
            let n = 1 + rng.below(700) as i64;
            let id = format!("m{n}");
            let change = match rng.below(3) {
                0 => random_edit(&mut rng, ANA, ANA, &id, AT + 10_000 + step),
                1 => Change::Message(random_message(&mut rng, ANA, ANA, &id, AT + n)),
                _ => Change::DeleteMessage { chat: ANA.into(), id },
            };
            store.apply(Lane::Live, vec![change]).await.unwrap();
        }
        let applied = store.apply(Lane::Background, vec![Change::Backfill(LINKS)]).await.unwrap();
        assert_eq!(applied.quarantined, 0);
        if step % 4 == 0 {
            drop(store);
            store = Store::open(home.path()).unwrap();
        }
        if step % 5 == 0 {
            let raw = Connection::open(home.path().join("jaw.db")).unwrap();
            raw.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            raw.execute("UPDATE backfills SET after = max(0, after - 64) WHERE name = 'links'", []).unwrap();
        }
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
        let after: i64 = db
            .query_row("SELECT coalesce((SELECT after FROM backfills WHERE name = 'links'), 1 << 62)", [], |r| r.get(0))
            .unwrap();
        let wrong: i64 = db
            .query_row(
                "SELECT (SELECT count(*) FROM (SELECT message, place, url FROM links WHERE message <= ?1
                    EXCEPT SELECT message, place, url FROM message_links WHERE message <= ?1))
                + (SELECT count(*) FROM (SELECT message, place, url FROM message_links WHERE message <= ?1
                    EXCEPT SELECT message, place, url FROM links WHERE message <= ?1))",
                [after],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(wrong, 0, "step {step}: links up to {after} aren't what the views derive");
    }
    store.indexed().await.unwrap();
    links_exact(home.path(), "after the backfill");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_links_index_is_merged_in_the_writers_own_steps_as_the_messages_index_is() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    // Each indexed in a step of its own, so each step writes a segment of each index.
    for k in 0..20 {
        let spans = json!([text("vê "), link(&format!("https://site{k}.pt/a"))]);
        apply(&store, vec![Change::Message(message(&format!("m{k}"), AT + k, spans, None))]).await;
        store.indexed().await.unwrap();
    }
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    let settings = |table: &str| -> Vec<(String, i64)> {
        db.prepare(&format!("SELECT k, v FROM {table} WHERE k IN ('automerge', 'crisismerge') ORDER BY k"))
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    let merging = settings("messages_fts_config");
    assert_eq!(merging, [("automerge".into(), 0), ("crisismerge".into(), 64)], "nothing merged at commit");
    assert_eq!(settings("links_fts_config"), merging, "the links' index merges as the messages' does");
    for merge in [
        "INSERT INTO messages_fts (messages_fts, rank) VALUES ('merge', 64)",
        "INSERT INTO links_fts (links_fts, rank) VALUES ('merge', 64)",
    ] {
        let before = db.total_changes();
        db.execute(merge, []).unwrap();
        assert_eq!(db.total_changes(), before + 1, "segments were left to merge: {merge}");
    }
    assert_eq!(found(&store, "\"site7\"").await, ["m7"]);
}
