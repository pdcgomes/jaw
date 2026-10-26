use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};

use super::*;
use crate::store::schema;

/// A xorshift generator, so a failing seed can be run again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, of: &'a [T]) -> &'a T {
        &of[self.below(of.len())]
    }
}

const KINDS: [&str; 10] =
    ["text", "image", "video", "gif", "video_note", "voice", "audio", "document", "sticker", "location"];
const WORDS: [&str; 8] = ["praia", "jantar", "fotos", "relatório", "domingo", "lisboa", "pdf", "olá"];

fn migrated() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    schema::migrate(&mut db).unwrap();
    db.execute_batch(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1), (2, 'Rui', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, 'ana@s.whatsapp.net', 1), (2, 'rui@s.whatsapp.net', 2),
            (3, '100000000002@lid', 2);
        INSERT INTO chats (n, id, kind, v) VALUES (1, 'ana@s.whatsapp.net', 'direct', 1), (2, 'g1@g.us', 'group', 1),
            (3, 'g2@g.us', 'group', 1);",
    )
    .unwrap();
    db
}

/// A made-up message's words: none, or a few of WORDS.
fn said(rng: &mut Rng) -> Option<String> {
    match rng.below(3) {
        0 => None,
        _ => Some((0..1 + rng.below(3)).map(|_| *rng.pick(&WORDS)).collect::<Vec<_>>().join(" ")),
    }
}

/// Writes a message as the writer does, waiting in fts_backlog first when it's history.
fn insert(db: &Connection, rng: &mut Rng, next: &mut i64, history: bool) {
    *next += 1;
    let n = *next;
    if history {
        db.execute("INSERT INTO fts_backlog (n) VALUES (?1)", [n]).unwrap();
    }
    let (sender, mine): (Option<i64>, bool) = match rng.below(4) {
        0 => (None, true),
        1 => (Some(1), false),
        2 => (Some(2), false),
        _ => (Some(3), false),
    };
    db.execute(
        "INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, deleted, view_once, v)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, 1)",
        params![
            n,
            1 + rng.below(3) as i64,
            format!("3EB0{n:018}"),
            1_790_000_000_000 + n * 1_000,
            (1_790_000_000_000 + n * 1_000) << 20,
            sender,
            mine,
            rng.pick(&KINDS),
            said(rng),
            rng.below(10) == 0,
        ],
    )
    .unwrap();
}

/// Something done to a message: a new one, live or from history; one deleted for you, deleted for
/// everyone, edited, seen once, turned into another kind or moved to another chat as a merge
/// moves it; history's words indexed.
fn act(db: &Connection, rng: &mut Rng, next: &mut i64) {
    let some: Option<i64> = db
        .query_row(
            "SELECT n FROM messages WHERE n >= ?1 ORDER BY n LIMIT 1",
            [1 + rng.below(*next as usize) as i64],
            |r| r.get(0),
        )
        .ok();
    match (rng.below(10), some) {
        (0..=2, _) | (_, None) => insert(db, rng, next, false),
        (3, _) => insert(db, rng, next, true),
        (4, Some(n)) => {
            db.execute("DELETE FROM messages WHERE n = ?1", [n]).unwrap();
        }
        (5, Some(n)) => {
            db.execute("UPDATE messages SET deleted = 1, text = NULL WHERE n = ?1", [n]).unwrap();
        }
        (6, Some(n)) => {
            db.execute("UPDATE messages SET text = ?2 WHERE n = ?1", params![n, said(rng)]).unwrap();
        }
        (7, Some(n)) => {
            db.execute(
                "UPDATE messages SET kind = ?2, view_once = ?3 WHERE n = ?1",
                params![n, rng.pick(&KINDS), rng.below(4) == 0],
            )
            .unwrap();
        }
        (8, Some(n)) => {
            db.execute("UPDATE messages SET chat = ?2 WHERE n = ?1", params![n, 1 + rng.below(3) as i64]).unwrap();
        }
        _ => {
            indexed(db, 1 + rng.below(4));
        }
    }
}

/// Indexes up to `most` of history's messages as the writer's steps do; says whether more wait.
fn indexed(db: &Connection, most: usize) -> bool {
    let mut stamps = crate::store::write::Stamps::load(db).unwrap();
    stamps.begin();
    crate::store::write::index(db, &mut stamps, most, |_| false).unwrap().1
}

/// What `shelves` should hold, counted afresh from messages.
fn counted(db: &Connection) -> BTreeMap<(i64, String), i64> {
    let mut counted = BTreeMap::new();
    let mut select = db.prepare("SELECT chat, shelf FROM messages WHERE shelf IS NOT NULL").unwrap();
    let rows = select.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).unwrap();
    for row in rows {
        let (chat, shelf) = row.unwrap();
        *counted.entry((chat, shelf.clone())).or_insert(0) += 1;
        *counted.entry((0, shelf)).or_insert(0) += 1;
    }
    counted
}

fn kept(db: &Connection) -> BTreeMap<(i64, String), i64> {
    let mut select = db.prepare("SELECT chat, shelf, items FROM shelves").unwrap();
    select.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?))).unwrap().map(Result::unwrap).collect()
}

/// The t of every message `word` should find in `index`: messages_fts has every message's words,
/// library_fts only items', and neither those waiting in fts_backlog.
fn expected(db: &Connection, index: &str, word: &str) -> BTreeSet<i64> {
    let items = if index == "library_fts" { "AND shelf IS NOT NULL" } else { "" };
    let sql = format!(
        "SELECT t, text FROM messages WHERE text IS NOT NULL {items} AND NOT EXISTS (SELECT 1 FROM fts_backlog b WHERE b.n = messages.n)"
    );
    let mut select = db.prepare(&sql).unwrap();
    let rows = select.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).unwrap();
    let folded = crate::find::Words::new(word);
    rows.map(Result::unwrap).filter(|(_, text)| !folded.marks(text).is_empty()).map(|(t, _)| t).collect()
}

fn found(db: &Connection, index: &str, word: &str) -> BTreeSet<i64> {
    let sql = format!("SELECT rowid FROM {index} WHERE {index} MATCH ?1");
    let mut select = db.prepare(&sql).unwrap();
    select.query_map([crate::find::Words::new(word).fts()], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

/// The t of every item of `shelf` in `scope` that `word` should find in library_fts, its words
/// indexed.
fn expected_in(db: &Connection, word: &str, shelf: Shelf, scope: &Scope) -> BTreeSet<i64> {
    let mut select = db
        .prepare(
            "SELECT t, text, chat, sender, mine FROM messages WHERE shelf = ?1 AND text IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM fts_backlog b WHERE b.n = messages.n)",
        )
        .unwrap();
    let rows = select
        .query_map([shelf.as_str()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get::<_, Option<i64>>(3)?, r.get(4)?))
        })
        .unwrap();
    let folded = crate::find::Words::new(word);
    rows.map(Result::unwrap)
        .filter(|(t, text, chat, sender, mine)| {
            !folded.marks(text).is_empty() && holds(scope, *chat, *sender, *mine, *t)
        })
        .map(|(t, ..)| t)
        .collect()
}

fn check(db: &Connection, what: &str) {
    assert_eq!(kept(db), counted(db), "{what}: shelves");
    for index in ["messages_fts", "library_fts"] {
        db.execute(&format!("INSERT INTO {index} ({index}, rank) VALUES ('integrity-check', 0)"), []).unwrap();
        for word in WORDS {
            assert_eq!(found(db, index, word), expected(db, index, word), "{what}: {index} for {word}");
        }
    }
    // Each item's tags follow its kind, chat and sender as they change.
    let open = Scope { chat: None, sender: Sender::Anyone, since: 0, until: i64::MAX };
    let mut scopes: Vec<Scope> = (1..=3).map(|chat| Scope { chat: Some(chat), ..open.clone() }).collect();
    scopes.extend(
        [Sender::Me, Sender::Jids(vec![1]), Sender::Jids(vec![2, 3])].map(|sender| Scope { sender, ..open.clone() }),
    );
    for word in WORDS {
        let query = crate::find::Words::new(word).fts();
        for shelf in Shelf::ALL {
            for scope in &scopes {
                let found: BTreeSet<i64> = words(db, &query, shelf, scope, Toward::Older, i64::MAX, 10_000)
                    .unwrap()
                    .into_iter()
                    .map(|(_, t)| t)
                    .collect();
                assert_eq!(found, expected_in(db, word, shelf, scope), "{what}: {word} in {shelf:?} {scope:?}");
            }
        }
    }
}

#[test]
fn the_counts_of_each_kind_and_items_words_follow_every_way_a_message_changes() {
    for seed in 1..=12u64 {
        let db = migrated();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed);
        let mut next = 0;
        for step in 0..300 {
            act(&db, &mut rng, &mut next);
            if step % 25 == 0 {
                check(&db, &format!("seed {seed}, step {step}"));
            }
        }
        // Everything history brought is indexed in the end, items' words with the rest.
        while indexed(&db, 64) {}
        check(&db, &format!("seed {seed}, all indexed"));
        // A chat deleted takes its messages and so its counts.
        db.execute("DELETE FROM chats WHERE n = 2", []).unwrap();
        check(&db, &format!("seed {seed}, a chat deleted"));
    }
}

#[test]
fn indexing_historys_words_tells_the_feed_of_each_item_it_makes_findable() {
    let db = migrated();
    db.execute_batch(&format!(
        "INSERT INTO fts_backlog (n) VALUES (1), (2), (3);
        INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, v) VALUES
            (1, 2, 'photo', 1, {}, 1, 0, 'image', 'na praia', 1),
            (2, 2, 'said', 2, {}, 1, 0, 'text', 'vamos à praia', 1),
            (3, 2, 'quiet', 3, {}, 1, 0, 'image', NULL, 1);",
        t(1),
        t(2),
        t(3)
    ))
    .unwrap();
    assert!(found(&db, "library_fts", "praia").is_empty(), "waiting to be indexed");
    let events = |db: &Connection| -> Vec<(String, i64, Option<i64>)> {
        let mut select = db.prepare("SELECT type, key, chat FROM events ORDER BY seq").unwrap();
        select.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    };
    assert!(events(&db).is_empty());
    assert!(!indexed(&db, 10), "all of it");
    assert_eq!(found(&db, "library_fts", "praia"), BTreeSet::from([t(1)]));
    assert_eq!(events(&db), [("message".into(), 1, Some(2))], "the item found by its words now, and only it");
}

#[test]
fn a_chats_count_of_a_kind_goes_with_its_last_item_of_that_kind() {
    let db = migrated();
    let mut next = 0;
    let mut rng = Rng(7);
    insert(&db, &mut rng, &mut next, false);
    db.execute_batch(
        "DELETE FROM messages;
        INSERT INTO messages (n, chat, id, at, t, mine, kind, v) VALUES (10, 1, 'a', 1, 1048576, 1, 'image', 1);",
    )
    .unwrap();
    assert_eq!(kept(&db), BTreeMap::from([((0, "photo".into()), 1), ((1, "photo".into()), 1)]));
    db.execute("UPDATE messages SET chat = 3 WHERE n = 10", []).unwrap();
    assert_eq!(kept(&db), BTreeMap::from([((0, "photo".into()), 1), ((3, "photo".into()), 1)]), "moved with it");
    db.execute("UPDATE messages SET deleted = 1 WHERE n = 10", []).unwrap();
    assert!(kept(&db).is_empty(), "a photo deleted for everyone is no item: {:?}", kept(&db));
}

/// A message's t, as the made-up ones have it.
fn t(n: i64) -> i64 {
    (1_790_000_000_000 + n * 1_000) << 20
}

/// Items of every kind in three chats, from you, Ana, and Rui by his number and his LID.
fn library() -> Connection {
    let db = migrated();
    let mut rng = Rng(11);
    let mut next = 0;
    for _ in 0..600 {
        insert(&db, &mut rng, &mut next, false);
    }
    db
}

/// Whether `scope` holds an item in `chat` from `sender` (yours when `mine`) at `t`.
fn holds(scope: &Scope, chat: i64, sender: Option<i64>, mine: bool, t: i64) -> bool {
    let sent = match &scope.sender {
        Sender::Anyone => true,
        Sender::Me => mine && sender.is_none(),
        Sender::Jids(jids) => !mine && sender.is_some_and(|sender| jids.contains(&sender)),
    };
    scope.chat.is_none_or(|c| c == chat) && sent && (scope.since..=scope.until).contains(&t)
}

/// Every item `scope` holds of `shelf`, newest first, read whole.
fn every(db: &Connection, shelf: Shelf, scope: &Scope) -> Vec<(i64, i64)> {
    let mut select =
        db.prepare("SELECT n, t, chat, sender, mine FROM messages WHERE shelf = ?1 ORDER BY t DESC").unwrap();
    let rows = select
        .query_map([shelf.as_str()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get(2)?, r.get::<_, Option<i64>>(3)?, r.get(4)?))
        })
        .unwrap();
    rows.map(Result::unwrap)
        .filter(|&(_, t, chat, sender, mine)| holds(scope, chat, sender, mine, t))
        .map(|(n, t, ..)| (n, t))
        .collect()
}

fn scopes() -> Vec<Scope> {
    let (since, until) = Scope::times(Some(1_790_000_100_000), Some(1_790_000_450_000));
    let mut scopes = Vec::new();
    for chat in [None, Some(2)] {
        for sender in [Sender::Anyone, Sender::Me, Sender::Jids(vec![1]), Sender::Jids(vec![2, 3])] {
            scopes.push(Scope { chat, sender: sender.clone(), since: 0, until: i64::MAX });
            scopes.push(Scope { chat, sender, since, until });
        }
    }
    scopes
}

#[test]
fn a_kinds_items_page_by_keyset_each_once_newest_first_in_every_scope() {
    let db = library();
    for scope in scopes() {
        for shelf in Shelf::ALL {
            let all = every(&db, shelf, &scope);
            let mut paged = Vec::new();
            let mut from = i64::MAX;
            loop {
                let page = keys(&db, shelf, &scope, Toward::Older, from, 7).unwrap();
                let Some(&(_, last)) = page.last() else { break };
                from = last;
                paged.extend(page);
            }
            assert_eq!(paged, all, "{shelf:?} in {scope:?}, older");
            // And back up from the oldest.
            let mut up = Vec::new();
            let mut from = i64::MIN;
            loop {
                let page = keys(&db, shelf, &scope, Toward::Newer, from, 5).unwrap();
                let Some(&(_, last)) = page.last() else { break };
                from = last;
                up.extend(page);
            }
            up.reverse();
            assert_eq!(up, all, "{shelf:?} in {scope:?}, newer");
        }
    }
}

#[test]
fn counts_are_kept_for_a_chat_or_all_and_counted_up_to_a_thousand_otherwise() {
    let db = library();
    for scope in scopes() {
        let counts = counts(&db, &scope).unwrap();
        for shelf in Shelf::ALL {
            assert_eq!(counts[&shelf], every(&db, shelf, &scope).len() as u64, "{shelf:?} in {scope:?}");
        }
    }
    let many = migrated();
    many.execute_batch(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1500)
        INSERT INTO messages (chat, id, at, t, mine, kind, v) SELECT 1, 'p' || x, x, x << 20, 1, 'image', 1 FROM c;",
    )
    .unwrap();
    let open = Scope { chat: None, sender: Sender::Anyone, since: 0, until: i64::MAX };
    assert_eq!(counts(&many, &open).unwrap()[&Shelf::Photo], 1500, "kept, exactly");
    let mine = Scope { sender: Sender::Me, ..open.clone() };
    assert_eq!(counts(&many, &mine).unwrap()[&Shelf::Photo], COUNTED, "counted, up to a thousand");
    let (since, until) = Scope::times(Some(1), None);
    assert_eq!(counts(&many, &Scope { since, until, ..open }).unwrap()[&Shelf::Photo], COUNTED);
}

#[test]
fn words_find_items_by_caption_and_file_name_accents_aside_and_never_other_messages() {
    let db = migrated();
    db.execute_batch(&format!(
        "INSERT INTO messages (n, chat, id, at, t, sender, mine, kind, text, v) VALUES
            (1, 1, 'photo', 1, {}, 1, 0, 'image', 'na praia', 1),
            (2, 1, 'said', 2, {}, 1, 0, 'text', 'vamos à praia', 1),
            (3, 2, 'doc', 3, {}, 2, 0, 'document', 'Relatório Praia.pdf', 1),
            (4, 2, 'jantar', 4, {}, NULL, 1, 'video', 'o jantar', 1),
            (5, 3, 'pr', 5, {}, 2, 0, 'image', 'pr', 1);",
        t(1),
        t(2),
        t(3),
        t(4),
        t(5)
    ))
    .unwrap();
    let open = Scope { chat: None, sender: Sender::Anyone, since: 0, until: i64::MAX };
    // Every kind's matches, newest first, as each kind's search finds them.
    let ids = |query: &str, scope: &Scope| -> Vec<i64> {
        let query = crate::find::Words::new(query).fts();
        let mut found: Vec<(i64, i64)> = Shelf::ALL
            .into_iter()
            .flat_map(|shelf| words(&db, &query, shelf, scope, Toward::Older, i64::MAX, 10).unwrap())
            .collect();
        found.sort_by_key(|&(_, t)| -t);
        found.into_iter().map(|(n, _)| n).collect()
    };
    let photos = |query: &str| -> Vec<i64> {
        let query = crate::find::Words::new(query).fts();
        words(&db, &query, Shelf::Photo, &open, Toward::Older, i64::MAX, 10)
            .unwrap()
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    };
    assert_eq!(ids("praia", &open), [3, 1], "items, newest first, never the text message");
    assert_eq!(ids("RELATORIO", &open), [3], "a file's name, accent-blind");
    assert_eq!(photos("prai"), [1], "of one kind; three letters find the words they start");
    assert_eq!(ids("pr", &open), [5], "two letters find only themselves");
    assert_eq!(ids("jantar", &Scope { sender: Sender::Me, ..open.clone() }), [4], "yours");
    assert!(ids("jantar", &Scope { chat: Some(1), ..open.clone() }).is_empty(), "in one chat");
    assert_eq!(
        ids("praia", &Scope { sender: Sender::Jids(vec![2]), ..open.clone() }),
        [3],
        "from one of a person's JIDs"
    );
    assert!(ids("praia", &Scope { sender: Sender::Jids(vec![]), ..open.clone() }).is_empty(), "from someone with none");
    let (since, until) = Scope::times(Some(1_790_000_002_000), None);
    assert_eq!(ids("praia", &Scope { since, until, ..open.clone() }), [3], "in a stretch of time");
    let counted = word_counts(&db, &crate::find::Words::new("praia").fts(), &open).unwrap();
    assert_eq!((counted[&Shelf::Photo], counted[&Shelf::File], counted[&Shelf::Video]), (1, 1, 0));
    // A caption that says what a tag would is a caption, not a kind.
    db.execute(&format!("INSERT INTO messages (n, chat, id, at, t, mine, kind, text, v) VALUES (6, 1, 'odd', 6, {}, 1, 'document', 'sphoto c1 fme', 1)", t(6)), []).unwrap();
    assert!(photos("sphoto").is_empty() && ids("sphoto", &open) == [6]);
}

#[test]
fn an_item_is_named_by_its_message_id_in_its_chat_or_in_any() {
    let db = library();
    let (n, id, chat): (i64, String, String) = db
        .query_row(
            "SELECT m.n, m.id, c.id FROM messages m JOIN chats c ON c.n = m.chat WHERE m.shelf = 'photo' LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(named(&db, &id, Some(&chat)).unwrap().iter().map(|f| f.0).collect::<Vec<_>>(), [n]);
    assert_eq!(named(&db, &id, None).unwrap().iter().map(|f| f.0).collect::<Vec<_>>(), [n]);
    assert!(named(&db, &id, Some("g2@g.us")).unwrap().is_empty() || chat == "g2@g.us");
    assert!(named(&db, "3EB0NOTHING", None).unwrap().is_empty());
}

#[test]
fn a_person_is_their_jids_or_you() {
    let db = migrated();
    assert_eq!(person(&db, 2).unwrap(), Some(Sender::Jids(vec![2, 3])));
    assert_eq!(person(&db, 9).unwrap(), None, "no such person");
    db.execute("INSERT INTO sync_state (key, value) VALUES ('me', 'x@lid ana@s.whatsapp.net')", []).unwrap();
    assert_eq!(person(&db, 1).unwrap(), Some(Sender::Me), "the account's own person is you");
}

const ANA: &str = "351910000002@s.whatsapp.net";
const ANA_LID: &str = "100000000002@lid";
const AT: i64 = 1_792_000_000_000;

/// A photo, a document or a text from Ana in `chat`, as the converter makes one.
fn from_ana(chat: &str, id: &str, at: i64, kind: &'static str, words: &str, live: bool) -> crate::store::Message {
    crate::store::Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: Some(chat.into()),
        kind,
        text: Some(words.into()),
        summary: words.into(),
        detail: (kind != "text").then(|| r#"{"media":{"mime":"image/jpeg","w":800,"h":600}}"#.to_string()),
        live,
        ..crate::store::Message::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_writer_keeps_the_counts_and_the_words_through_copies_deletions_and_a_merge() {
    use crate::store::{Amend, Amendment, Change, Lane, Person, Store};
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let live = |changes| store.apply(Lane::Live, changes);
    live(vec![
        Change::Message(from_ana(ANA, "p1", AT, "image", "na praia", true)),
        Change::Message(from_ana(ANA_LID, "p2", AT + 1, "image", "praia outra vez", true)),
        Change::Message(from_ana(ANA_LID, "d1", AT + 2, "document", "praia.pdf", true)),
        Change::Message(from_ana(ANA_LID, "gone", AT + 3, "image", "a praia apagada", true)),
        Change::Message(from_ana(ANA_LID, "mine", AT + 4, "image", "a praia, só para mim", true)),
        Change::Message(from_ana(ANA_LID, "t1", AT + 5, "text", "vamos à praia", true)),
    ])
    .await
    .unwrap();
    // A copy of one from history, and an old one only history has, whose words wait to be indexed.
    store
        .apply(
            Lane::Background,
            vec![
                Change::Message(from_ana(ANA_LID, "p2", AT + 1, "image", "praia outra vez", false)),
                Change::Message(from_ana(ANA_LID, "old", AT - 1_000, "image", "a praia antiga", false)),
            ],
        )
        .await
        .unwrap();
    let revoke = Amend {
        chat: ANA_LID.into(),
        target: "gone".into(),
        sender: Some(ANA_LID.into()),
        mine: false,
        at: AT + 10,
        what: Amendment::Revoke,
        raw: None,
    };
    live(vec![Change::Amend(revoke), Change::DeleteMessage { chat: ANA_LID.into(), id: "mine".into() }]).await.unwrap();
    // Her number and her LID are one person, so her two chats become one.
    live(vec![Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]).await.unwrap();
    let merging = || {
        store.reads().query(crate::work::Priority::Interactive, |db| {
            Ok(db.query_row("SELECT EXISTS (SELECT 1 FROM chat_merges)", [], |r| r.get::<_, bool>(0))?)
        })
    };
    while merging().await.unwrap() {
        store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
    }
    store.indexed().await.unwrap();
    let (kept, counted, found, kept_chat) = store
        .reads()
        .query(crate::work::Priority::Interactive, |db| {
            let chat: i64 = db.query_row("SELECT n FROM chats WHERE id = ?1", [ANA], |r| r.get(0))?;
            let scope = Scope { chat: Some(chat), sender: Sender::Anyone, since: 0, until: i64::MAX };
            let query = crate::find::Words::new("praia").fts();
            let mut found: Vec<i64> = Shelf::ALL
                .into_iter()
                .flat_map(|shelf| words(db, &query, shelf, &scope, Toward::Older, i64::MAX, 100).unwrap())
                .map(|(_, t)| t)
                .collect();
            found.sort_unstable();
            Ok((kept(db), counted(db), found, chat))
        })
        .await
        .unwrap();
    assert_eq!(kept, counted, "shelves, against a fresh count");
    assert_eq!(
        kept,
        BTreeMap::from([
            ((0, "file".into()), 1),
            ((0, "photo".into()), 3),
            ((kept_chat, "file".into()), 1),
            ((kept_chat, "photo".into()), 3)
        ]),
        "the photos and the file, once each, all in the chat kept"
    );
    assert_eq!(found.len(), 4, "every item's words, its tags moved with it to the chat kept: {found:?}");
}

#[test]
fn an_upgrade_counts_and_indexes_the_items_jaw_db_already_holds_and_leaves_historys_waiting() {
    let mut db = Connection::open_in_memory().unwrap();
    let before = schema::before_the_library();
    for migration in &schema::MIGRATIONS[..before] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", before as i64).unwrap();
    db.execute_batch(
        "INSERT INTO people (n, name, v) VALUES (1, 'Ana', 1), (2, 'Rui', 1);
        INSERT INTO person_jids (n, jid, person) VALUES (1, 'ana@s.whatsapp.net', 1), (2, 'rui@s.whatsapp.net', 2),
            (3, '100000000002@lid', 2);
        INSERT INTO chats (n, id, kind, v) VALUES (1, 'ana@s.whatsapp.net', 'direct', 1), (2, 'g1@g.us', 'group', 1),
            (3, 'g2@g.us', 'group', 1);",
    )
    .unwrap();
    let mut rng = Rng(5);
    let mut next = 0;
    for _ in 0..400 {
        let history = rng.below(5) == 0;
        insert(&db, &mut rng, &mut next, history);
    }
    db.execute_batch("UPDATE messages SET deleted = 1, text = NULL WHERE n % 17 = 0;").unwrap();
    schema::migrate(&mut db).unwrap();
    check(&db, "upgraded");
    assert!(!kept(&db).is_empty(), "it had items to count");
    while indexed(&db, 64) {}
    check(&db, "upgraded, and history indexed since");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_items_previews_a_power_cut_may_have_broken_are_shown_only_once_the_check_has_passed_them() {
    use crate::store::Store;
    use crate::work::Priority;
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    db.execute("INSERT INTO chats (id, kind, v) VALUES ('ana@s.whatsapp.net', 'direct', 1)", []).unwrap();
    // Three files, the second of which a start found suspect.
    let mut ns = Vec::new();
    for k in 1..=3u8 {
        let made = i64::from(k);
        db.execute(
            "INSERT INTO messages (chat, id, at, t, mine, kind, detail, v) VALUES (1, ?1, ?2, ?2, 1, 'image', '{\"media\":{}}', 1)",
            params![format!("3EB0FILE{k}"), made],
        )
        .unwrap();
        ns.push(db.last_insert_rowid());
        db.execute(
            "INSERT INTO media (message, variant, keys, sha256) VALUES (last_insert_rowid(), 'sd', 'image', ?1)",
            [vec![k; 32]],
        )
        .unwrap();
        db.execute(
            "INSERT INTO media_files (sha256, path, small, large, thumbnail, used_at, made)
            VALUES (?1, 'media/files/f.jpg', 'media/previews/s.jpg', 'media/previews/l.jpg', 'media/previews/t.jpg', 1, ?2)",
            params![vec![k; 32], made],
        )
        .unwrap();
    }
    store.files().suspect([vec![2u8; 32]]);
    let shown = || {
        let (store, ns) = (store.clone(), ns.clone());
        async move {
            let items = store.reads().query(Priority::Interactive, move |db| items(db, &ns)).await.unwrap();
            items
                .iter()
                .map(|item| (item.media.contains_key("previews"), item.media.contains_key("thumbnail")))
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(shown().await, [(true, true), (false, false), (true, true)], "the second isn't checked yet");
    store.files().passed(&[vec![2u8; 32]]);
    assert_eq!(shown().await, [(true, true); 3], "once the check has passed it");
}
