//! The design's promise that no migration drops what can't be had again: a table of jaw's own or
//! a raw one, or a column of jaw's own on another table (see "Migrations" in the design). For every
//! migration, those made since too, a database at the version before it gets a row in each such
//! table and a value of its own in each such column; after the migration each such table is still
//! there and each row is there with every column it had unchanged, unless the migration dropped
//! the column by a decision named in `dropped`.

use rusqlite::types::Value;

use super::tests::number;
use super::*;

/// What a table holds, as its class says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// As WhatsApp sent it, the source of truth.
    Raw,
    /// Fetched from WhatsApp, which can be asked again, but for any columns of jaw's own.
    Fetched,
    /// jaw's own: never dropped, and copied as it is by every rebuild.
    Own,
    /// Derived from other tables, and rebuilt from them.
    Derived,
}

use Class::{Derived, Fetched, Own, Raw};

impl Class {
    /// What a column of a table of this class that must outlive a migration is.
    fn kept(self) -> &'static str {
        match self {
            Raw => "raw",
            Own => "jaw's own",
            Fetched | Derived => "jaw's own on a table that isn't",
        }
    }
}

/// Every table jaw.db has had, its class, and for a fetched or derived one its columns of jaw's own.
/// A migration that makes a table gives it a line here, or this test fails.
const CLASSES: &[(&str, Class, &[&str])] = &[
    ("inbox", Raw, &[]),
    ("history_raw", Raw, &[]),
    ("app_state", Raw, &[]),
    ("pending", Raw, &[]),
    ("history_kept", Raw, &[]),
    ("people", Fetched, &["followed"]),
    ("person_jids", Fetched, &[]),
    ("chats", Fetched, &["draft", "frecency", "opened_at", "read_n", "told_at", "told_n"]),
    // Derived from raw but for what amendments did to it, which keeps no raw (migration 6).
    ("messages", Own, &[]),
    ("media", Fetched, &["path", "preview_small", "preview_large", "state", "progress"]),
    ("reactions", Own, &[]),
    ("receipts", Own, &[]),
    ("poll_options", Fetched, &[]),
    ("poll_votes", Own, &[]),
    ("group_members", Fetched, &[]),
    ("folders", Own, &[]),
    ("folder_items", Own, &[]),
    ("favourites", Own, &[]),
    ("outbox", Own, &[]),
    ("sync_state", Own, &[]),
    ("quarantine", Own, &[]),
    ("tombstones", Own, &[]),
    ("chat_merges", Own, &[]),
    ("recipients", Own, &[]),
    ("settled", Own, &[]),
    ("regroup", Own, &[]),
    ("send_keys", Own, &[]),
    ("interface_state", Own, &[]),
    ("settings", Own, &[]),
    ("backfills", Own, &[]),
    ("thumbnails", Own, &[]),
    ("poll_voters", Own, &[]),
    ("event_answers", Own, &[]),
    ("events", Derived, &[]),
    ("fts_backlog", Derived, &[]),
    ("messages_fts", Derived, &[]),
    ("messages_fts_data", Derived, &[]),
    ("messages_fts_idx", Derived, &[]),
    ("messages_fts_docsize", Derived, &[]),
    ("messages_fts_config", Derived, &[]),
    ("links", Derived, &[]),
    ("links_fts", Derived, &[]),
    ("links_fts_data", Derived, &[]),
    ("links_fts_idx", Derived, &[]),
    ("links_fts_docsize", Derived, &[]),
    ("links_fts_config", Derived, &[]),
    ("folder_paths", Derived, &[]),
    ("folder_chats", Derived, &[]),
    ("folder_people", Derived, &[]),
    ("folder_counts", Derived, &[]),
    ("media_files", Own, &[]),
    ("media_made", Own, &[]),
    ("media_cache", Derived, &[]),
    ("media_drop", Own, &[]),
];

/// The rows the test writes, in an order that keeps every foreign key, each column's value as SQL.
/// A column a table doesn't have at a version is left out, so one row serves every version; every
/// column of jaw's own or raw has a value in some row that's neither NULL nor its default, so a
/// migration that dropped it, or wrote its default over it, shows.
const SEEDS: &[(&str, &[(&str, &str)])] = &[
    (
        "people",
        &[
            ("n", "1"),
            ("name", "'Ana'"),
            ("push_name", "'Aninha'"),
            ("verified_name", "'Ana Lda'"),
            ("about", "'Lisboa'"),
            ("about_at", "1790000000000"),
            ("avatar", "'media/avatars/ana.jpg'"),
            ("avatar_id", "'a1'"),
            ("last_seen", "1790000000100"),
            ("followed", "1"),
            ("v", "3"),
        ],
    ),
    ("people", &[("n", "2"), ("name", "'Rui'"), ("v", "3")]),
    ("people", &[("n", "3"), ("v", "3")]),
    ("person_jids", &[("n", "1"), ("jid", "'351910000002@s.whatsapp.net'"), ("person", "1")]),
    ("person_jids", &[("n", "2"), ("jid", "'351910000003@s.whatsapp.net'"), ("person", "2")]),
    ("person_jids", &[("n", "3"), ("jid", "'351910000001@s.whatsapp.net'"), ("person", "3")]),
    (
        "chats",
        &[
            ("n", "1"),
            ("id", "'351910000002@s.whatsapp.net'"),
            ("kind", "'direct'"),
            ("person", "1"),
            ("draft", "'até já'"),
            ("frecency", "2.5"),
            ("opened_at", "1790000000500"),
            ("last_at", "1790000000000"),
            ("last_message", "1"),
            ("read_at", "1790000000000"),
            ("read_n", "1"),
            ("told_at", "1790000000000"),
            ("told_n", "1"),
            ("v", "4"),
        ],
    ),
    (
        "chats",
        &[
            ("n", "2"),
            ("id", "'120363000000000001@g.us'"),
            ("kind", "'group'"),
            ("name", "'Escalada'"),
            ("last_at", "1790000000400"),
            ("last_message", "4"),
            ("v", "4"),
        ],
    ),
    // Ana's chat under her LID, being merged into the one under her number.
    ("chats", &[("n", "3"), ("id", "'100000000002@lid'"), ("kind", "'direct'"), ("person", "1"), ("v", "4")]),
    (
        "messages",
        &[
            ("n", "1"),
            ("chat", "1"),
            ("id", "'m1'"),
            ("at", "1790000000000"),
            ("t", "1790000000000 << 20"),
            ("mine", "1"),
            ("kind", "'text'"),
            ("status", "'read'"),
            ("text", "'olá'"),
            ("spans", "'[{\"t\":\"text\",\"s\":\"olá\"}]'"),
            ("edited_at", "1790000000050"),
            ("starred", "1"),
            ("v", "5"),
            ("raw", "x'0a01'"),
        ],
    ),
    (
        "messages",
        &[
            ("n", "2"),
            ("chat", "2"),
            ("id", "'m2'"),
            ("at", "1790000000200"),
            ("t", "1790000000200 << 20"),
            ("sender", "2"),
            ("kind", "'image'"),
            ("text", "'a foto'"),
            ("summary", "'Foto'"),
            ("reply", "'{\"id\":\"m4\"}'"),
            ("detail", "'{\"w\":640}'"),
            ("forwarded", "1"),
            ("expires_at", "1790086400200"),
            ("view_once", "1"),
            ("mentions_me", "1"),
            ("v", "5"),
            ("raw", "x'0a02'"),
            ("envelope", "x'0b02'"),
        ],
    ),
    (
        "messages",
        &[
            ("n", "3"),
            ("chat", "2"),
            ("id", "'m3'"),
            ("at", "1790000000300"),
            ("t", "1790000000300 << 20"),
            ("sender", "2"),
            ("kind", "'text'"),
            ("deleted", "1"),
            ("v", "5"),
        ],
    ),
    (
        "messages",
        &[
            ("n", "4"),
            ("chat", "2"),
            ("id", "'m4'"),
            ("at", "1790000000400"),
            ("t", "1790000000400 << 20"),
            ("mine", "1"),
            ("kind", "'text'"),
            ("status", "'delivered'"),
            ("text", "'sábado?'"),
            ("v", "5"),
        ],
    ),
    (
        "media",
        &[
            ("message", "2"),
            ("variant", "'sd'"),
            ("mime", "'image/jpeg'"),
            ("bytes", "48000"),
            ("w", "640"),
            ("h", "480"),
            ("name", "'foto.jpg'"),
            ("sha256", "x'0101010101010101010101010101010101010101010101010101010101010101'"),
            ("thumb", "x'02'"),
            ("path", "'media/m2.jpg'"),
            ("preview_small", "'media/m2.small.jpg'"),
            ("preview_large", "'media/m2.large.jpg'"),
            ("state", "'expired'"),
            ("progress", "0.5"),
        ],
    ),
    ("reactions", &[("message", "2"), ("sender", "3"), ("emoji", "'👍'"), ("at", "1790000000250"), ("mine", "1")]),
    (
        "receipts",
        &[
            ("message", "4"),
            ("recipient", "2"),
            ("delivered_at", "1790000000410"),
            ("read_at", "1790000000420"),
            ("played_at", "1790000000430"),
        ],
    ),
    ("poll_votes", &[("message", "2"), ("voter", "3"), ("position", "1"), ("at", "1790000000260")]),
    ("folders", &[("n", "20"), ("name", "'Family'"), ("position", "3"), ("favourite", "4"), ("v", "6")]),
    ("folders", &[("n", "21"), ("parent", "20"), ("name", "'Cousins'"), ("position", "1"), ("v", "6")]),
    (
        "folders",
        &[
            ("n", "22"),
            ("name", "'Unread at work'"),
            ("position", "2"),
            ("smart", "'{\"unread\":true}'"),
            ("rule", "'{\"unread\":true}'"),
            ("v", "6"),
        ],
    ),
    ("folder_items", &[("folder", "21"), ("chat", "2"), ("position", "2")]),
    ("folder_items", &[("folder", "20"), ("person", "1")]),
    ("favourites", &[("slot", "4"), ("folder", "20")]),
    ("favourites", &[("slot", "2"), ("chat", "2")]),
    (
        "pending",
        &[
            ("n", "1"),
            ("chat", "'120363000000000001@g.us'"),
            ("target", "'m9'"),
            ("kind", "'edit'"),
            ("at", "1790000000600"),
            ("raw", "x'0d'"),
            ("sender", "'351910000003@s.whatsapp.net'"),
            ("mine", "1"),
            ("effect", "'{\"edit\":\"sábado\"}'"),
        ],
    ),
    (
        "outbox",
        &[
            ("id", "'o1'"),
            ("message", "1"),
            ("chat", "'351910000002@s.whatsapp.net'"),
            ("jid", "'351910000002@s.whatsapp.net'"),
            ("kind", "'text'"),
            ("raw", "x'0e'"),
            ("state", "'unconfirmed'"),
            ("attempts", "2"),
            ("next_at", "1790000009000"),
            ("created_at", "1790000001000"),
            ("error", "'no answer yet'"),
        ],
    ),
    (
        "outbox",
        &[
            ("id", "'o2'"),
            ("chat", "'120363000000000001@g.us'"),
            ("jid", "'120363000000000001@g.us'"),
            ("kind", "'reaction'"),
            ("raw", "x'0f'"),
            ("next_at", "1790000009000"),
            ("created_at", "1790000002000"),
            ("target", "2"),
            ("target_deleted", "1"),
            ("effect", "'{\"react\":\"👍\"}'"),
        ],
    ),
    ("sync_state", &[("key", "'me'"), ("value", "'351910000001@s.whatsapp.net'")]),
    ("app_state", &[("key", "'[\"pin_v1\",\"351910000002@s.whatsapp.net\"]'"), ("value", "x'10'")]),
    ("inbox", &[("seq", "7"), ("kind", "'message'"), ("at", "1790000000700"), ("payload", "x'11'")]),
    (
        "history_raw",
        &[("chat", "'120363000000000001@g.us'"), ("id", "'h1'"), ("at", "1790000000800"), ("raw", "x'12'")],
    ),
    (
        "quarantine",
        &[
            ("n", "1"),
            ("at", "1790000000900"),
            ("source", "'inbox'"),
            ("ref", "'6'"),
            ("error", "'a message jaw can not read yet'"),
            ("raw", "x'13'"),
        ],
    ),
    ("tombstones", &[("chat", "1"), ("id", "'gone'"), ("at", "1790000000910")]),
    ("history_kept", &[("n", "1"), ("id", "'blob1'"), ("at", "1790000000920"), ("raw", "x'14'")]),
    ("chat_merges", &[("gone", "3"), ("keep", "1")]),
    ("recipients", &[("message", "4"), ("member", "2"), ("chat", "2")]),
    ("settled", &[("message", "4")]),
    ("regroup", &[("chat", "2"), ("member", "2"), ("after", "4")]),
    (
        "send_keys",
        &[
            ("key", "'k1'"),
            ("message", "1"),
            ("jid", "'351910000002@s.whatsapp.net'"),
            ("id", "'m1'"),
            ("at", "1790000000930"),
        ],
    ),
    (
        "interface_state",
        &[
            ("interface", "'jaw'"),
            ("key", "'layout'"),
            ("value", "'{\"format\":1,\"focused\":0,\"tree\":{\"chat\":\"351910000002@s.whatsapp.net\"}}'"),
            ("v", "1790000000940"),
            ("updated_at", "1790000000950"),
        ],
    ),
    ("settings", &[("domain", "'jaw'"), ("key", "'theme'"), ("value", "'\"nord\"'"), ("at", "1790000000960")]),
    ("backfills", &[("name", "'links'"), ("after", "3"), ("upto", "9")]),
    (
        "thumbnails",
        &[
            ("sha256", "'abababababababababababababababababababababababababababababababab'"),
            ("bytes", "2048"),
            ("at", "1790000000970"),
        ],
    ),
    ("poll_voters", &[("message", "2"), ("voter", "3"), ("at", "1790000000980"), ("id", "'v1'")]),
    (
        "event_answers",
        &[
            ("message", "2"),
            ("who", "3"),
            ("answer", "'maybe'"),
            ("guests", "1"),
            ("at", "1790000000990"),
            ("id", "'a1'"),
        ],
    ),
    (
        "media_files",
        &[
            ("sha256", "x'0101010101010101010101010101010101010101010101010101010101010101'"),
            ("path", "'media/files/01.jpg'"),
            ("bytes", "48000"),
            ("small", "'media/files/01.small.jpg'"),
            ("large", "'media/files/01.large.jpg'"),
            ("previews_bytes", "9000"),
            ("thumbnail", "'media/files/01.thumb.jpg'"),
            ("thumbnail_bytes", "800"),
            ("used_at", "1790000001010"),
            ("made", "4"),
        ],
    ),
    ("media_made", &[("id", "1"), ("made", "5"), ("settled", "3")]),
    ("media_drop", &[("sha256", "x'0202020202020202020202020202020202020202020202020202020202020202'")]),
];

/// Columns of jaw's own or raw a migration dropped by decision, by its number: the table, the
/// column, and the column of the same row that has its values now, which is checked, or None
/// where they went elsewhere or nowhere, as each says.
fn dropped() -> Vec<(usize, &'static str, &'static str, Option<&'static str>)> {
    let folders = number(FOLDERS_AND_THE_ROSTER);
    vec![
        // What's waiting to be sent names its chat by JID, which stays when the chat's row goes.
        (7, "outbox", "chat", Some("jid")),
        // A smart folder's rule, a JSON object now.
        (folders, "folders", "smart", Some("rule")),
        // The slots are favourites' rows, which the folders' migration's own test checks.
        (folders, "folders", "favourite", None),
        // Nothing ever wrote it: a folder's chats sort by their newest message, its people as
        // rosters do.
        (folders, "folder_items", "position", None),
        (20, "media", "path", None),
        (20, "media", "preview_small", None),
        (20, "media", "preview_large", None),
    ]
}

/// Columns of jaw's own a migration kept but reset by decision, by its number: the table, the
/// column, and the SQL each value becomes, of the value it had as `old`. Its rows are checked
/// against what the decision says, not left out.
fn reset() -> Vec<(usize, &'static str, &'static str, &'static str)> {
    vec![
        // Media in: a state from before it means nothing now but expired and gone, and no
        // download runs across an upgrade.
        (20, "media", "state", "iif(old IN ('expired', 'gone'), old, 'remote')"),
        (20, "media", "progress", "NULL"),
    ]
}

/// A table's columns, without the generated ones, and those of its primary key.
fn columns(db: &Connection, table: &str) -> (Vec<String>, Vec<String>) {
    let mut all = Vec::new();
    let mut key = Vec::new();
    let mut info = db.prepare(&format!("PRAGMA table_info(\"{table}\")")).unwrap();
    let mut rows = info.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let name: String = row.get(1).unwrap();
        if row.get::<_, i64>(5).unwrap() > 0 {
            key.push(name.clone());
        }
        all.push(name);
    }
    (all, key)
}

/// The tables at this version, with their class and columns of jaw's own, each with a class.
fn tables(db: &Connection, version: usize) -> Vec<(String, Class, &'static [&'static str])> {
    let names: Vec<String> = db
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let Some(&(_, class, own)) = CLASSES.iter().find(|(table, _, _)| *table == name) else {
                panic!("{name}, at version {version}, has no class: give it one in CLASSES, as its CREATE does")
            };
            (name, class, own)
        })
        .collect()
}

/// The columns of `table` that must outlive a migration, all of them for one of jaw's own or raw,
/// and with its key, which says whose they are, for any other.
fn kept(db: &Connection, table: &str, class: Class, own: &[&str]) -> Vec<String> {
    let (all, key) = columns(db, table);
    match class {
        Raw | Own => all,
        Fetched | Derived => {
            let own: Vec<String> = all.iter().filter(|c| own.contains(&c.as_str())).cloned().collect();
            if own.is_empty() { own } else { key.into_iter().chain(own).collect() }
        }
    }
}

/// Writes each seed row whose table there is, with the columns it has at this version.
fn seed(db: &Connection, version: usize) {
    for (table, row) in SEEDS {
        let (has, _) = columns(db, table);
        if has.is_empty() {
            continue;
        }
        let row: Vec<&(&str, &str)> = row.iter().filter(|(column, _)| has.iter().any(|c| c == column)).collect();
        let names: Vec<String> = row.iter().map(|(column, _)| format!("\"{column}\"")).collect();
        let values: Vec<&str> = row.iter().map(|(_, value)| *value).collect();
        let set: Vec<String> = names.iter().map(|name| format!("{name} = excluded.{name}")).collect();
        let sql = format!(
            "INSERT INTO \"{table}\" ({}) VALUES ({}) ON CONFLICT DO UPDATE SET {}",
            names.join(", "),
            values.join(", "),
            set.join(", ")
        );
        db.execute(&sql, []).unwrap_or_else(|e| {
            panic!("a row of {table} at version {version} wasn't written, so give SEEDS what it needs: {e}\n{sql}")
        });
    }
}

/// Every row of `table`, as these columns hold it.
fn rows(db: &Connection, table: &str, columns: &[String]) -> Vec<Vec<Value>> {
    let names: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();
    let mut statement = db.prepare(&format!("SELECT {} FROM \"{table}\"", names.join(", "))).unwrap();
    let rows = statement
        .query_map([], |r| (0..columns.len()).map(|i| r.get::<_, Value>(i)).collect::<rusqlite::Result<Vec<_>>>())
        .unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}

/// Asserts each column held has a value of its own in some row: neither NULL nor its default.
fn filled(db: &Connection, table: &str, class: Class, own: &[&str], version: usize) {
    let (_, key) = columns(db, table);
    let mut info = db.prepare(&format!("PRAGMA table_info(\"{table}\")")).unwrap();
    let defaults: Vec<(String, Option<String>)> =
        info.query_map([], |r| Ok((r.get(1)?, r.get(4)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    for (column, default) in defaults {
        let held = match class {
            Raw | Own => true,
            Fetched | Derived => own.contains(&column.as_str()) && !key.contains(&column),
        };
        if !held {
            continue;
        }
        let default = default.unwrap_or_else(|| "NULL".into());
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM \"{table}\" WHERE \"{column}\" IS NOT NULL AND \"{column}\" IS NOT {default})"
        );
        let has: bool = db.query_row(&sql, [], |r| r.get(0)).unwrap();
        assert!(
            has,
            "at version {version}, no row of {table} has a {column} of its own, so a migration losing it would go \
             unseen: give it one in SEEDS"
        );
    }
}

/// Where each table's AUTOINCREMENT keys have reached.
fn sequences(db: &Connection) -> Vec<(String, i64)> {
    let there: bool = db
        .query_row("SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE name = 'sqlite_sequence')", [], |r| r.get(0))
        .unwrap();
    if !there {
        return Vec::new();
    }
    db.prepare("SELECT name, seq FROM sqlite_sequence")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// Runs migration `n` alone, as Runner runs a step: foreign keys off, checked before it commits.
fn run(db: &mut Connection, n: usize) {
    db.pragma_update(None, "foreign_keys", false).unwrap();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
    Runner::default().execute(&tx, MIGRATIONS[n - 1]).unwrap_or_else(|e| panic!("migration {n}: {e:#}"));
    let dangling: Option<String> =
        tx.prepare("PRAGMA foreign_key_check").unwrap().query_row([], |r| r.get(0)).optional().unwrap();
    assert_eq!(dangling, None, "migration {n} left a row that refers to nothing");
    tx.pragma_update(None, "user_version", n as i64).unwrap();
    assert_eq!(fingerprint(&tx).unwrap(), expected(n).unwrap(), "migration {n}");
    tx.commit().unwrap();
}

#[test]
fn no_migration_drops_a_table_or_column_of_jaws_own_or_raw_or_changes_a_row_of_it() {
    let dropped = dropped();
    let reset = reset();
    for n in 2..=MIGRATIONS.len() {
        let mut db = Connection::open_in_memory().unwrap();
        for migration in &MIGRATIONS[..n - 1] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", (n - 1) as i64).unwrap();
        seed(&db, n - 1);
        let dangling: Option<String> =
            db.prepare("PRAGMA foreign_key_check").unwrap().query_row([], |r| r.get(0)).optional().unwrap();
        assert_eq!(dangling, None, "a row SEEDS writes at version {} refers to nothing", n - 1);
        let mut before = Vec::new();
        for (table, class, own) in tables(&db, n - 1) {
            let held = kept(&db, &table, class, own);
            if held.is_empty() {
                continue;
            }
            filled(&db, &table, class, own, n - 1);
            let found = rows(&db, &table, &held);
            assert!(!found.is_empty(), "SEEDS has no row of {table}, which is {}, at version {}", class.kept(), n - 1);
            before.push((table, class, held, found));
        }
        let keys = sequences(&db);

        run(&mut db, n);

        let after: Vec<String> = tables(&db, n).into_iter().map(|(table, _, _)| table).collect();
        for (table, class, held, found) in before {
            assert!(
                after.contains(&table),
                "migration {n} dropped {table}, which holds what's {}: what can't be had again is never dropped",
                class.kept()
            );
            let (now, _) = columns(&db, &table);
            // Each column held, as the new table has it: itself, or the column a decision moved
            // its values to; one a decision dropped is left out.
            let mut from = Vec::new();
            let mut to = Vec::new();
            for (at, column) in held.iter().enumerate() {
                if now.contains(column) {
                    from.push(at);
                    to.push(column.clone());
                    continue;
                }
                let decided = dropped.iter().find(|(m, t, c, _)| *m == n && *t == table && c == column);
                match decided {
                    Some((_, _, _, Some(moved))) => {
                        from.push(at);
                        to.push(moved.to_string());
                    }
                    Some((_, _, _, None)) => {}
                    None => panic!(
                        "migration {n} dropped {table}.{column}, which is {}: keep it, or name it in dropped() with \
                         what became of it",
                        class.kept()
                    ),
                }
            }
            let resets: Vec<Option<&str>> = to
                .iter()
                .map(|column| reset.iter().find(|(m, t, c, _)| *m == n && *t == table && c == column).map(|r| r.3))
                .collect();
            let mut there = rows(&db, &table, &to);
            for row in found {
                let row: Vec<Value> = from
                    .iter()
                    .zip(&resets)
                    .map(|(&at, reset)| match reset {
                        Some(becomes) => db
                            .query_row(&format!("SELECT {becomes} FROM (SELECT ?1 AS old)"), [&row[at]], |r| r.get(0))
                            .unwrap(),
                        None => row[at].clone(),
                    })
                    .collect();
                match there.iter().position(|kept| *kept == row) {
                    Some(at) => {
                        there.swap_remove(at);
                    }
                    None => panic!("migration {n} lost or changed a row of {table}: {to:?} were {row:?}"),
                }
            }
        }
        let now = sequences(&db);
        for (table, seq) in keys {
            let kept = now.iter().find(|(name, _)| *name == table).map(|(_, seq)| *seq);
            assert!(
                kept.is_some_and(|kept| kept >= seq),
                "migration {n} took the keys of {table} back from {seq} to {kept:?}, so a key others hold could be used again"
            );
        }
    }
}
