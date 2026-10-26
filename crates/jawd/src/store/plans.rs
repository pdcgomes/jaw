//! Every SQL statement jawd's code holds, with the plan SQLite makes for it on jaw.db's schema,
//! pinned in plans.txt beside this file. A statement that's new or plans differently fails the test
//! until plans.txt is written again (`JAW_PLANS=write cargo test -p jawd plans`) and the change is
//! looked at; one that scans a table or sorts in a temporary b-tree fails it unless it's allowed
//! below, with why.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Statements allowed to scan or sort, by a piece of their text, and why.
const ALLOWED: &[(&str, &str)] = &[
    ("FROM person_jids j WHERE j.jid LIKE '%@lid'", "once each time an engine connects, stopping at its limit"),
    ("SELECT count(*) FROM quarantine", "quarantine holds what failed, a handful of rows"),
    ("SELECT gone, keep FROM chat_merges ORDER BY gone LIMIT 1", "chats being merged, rarely more than one"),
    (
        "SELECT m.gone, g.id, k.id FROM chat_merges m",
        "chats being merged, a row for each person whose number and LID were just joined, gone once merged",
    ),
    ("SELECT n FROM fts_backlog ORDER BY n DESC LIMIT", "a scan down the rowid that stops at its limit"),
    ("json_group_array(json_object('emoji'", "one message's reactions, grouped by emoji"),
    ("SELECT coalesce(max(seq), 0) FROM events", "once, when the writer starts"),
    (
        "SELECT coalesce(max(seq), 0) FROM inbox",
        "once, when the converter starts; the inbox holds what isn't converted yet",
    ),
    ("FROM inbox WHERE seq > ?1 ORDER BY seq", "a range of the rowid"),
    ("SELECT count(*) FROM inbox", "the table jawd status counts, which holds what isn't converted yet"),
    ("FROM sqlite_sequence", "one row per table with AUTOINCREMENT"),
    ("DELETE FROM events WHERE seq IN", "a range of the rowid, stopping at its limit"),
    ("SELECT EXISTS (SELECT 1 FROM fts_backlog)", "stops at its first row, once when the writer starts"),
    (
        "SELECT n, id, name, person, frecency, opened_at, last_at FROM chats",
        "the palette's index is built from every chat once, at start",
    ),
    (
        "SELECT n, name, push_name, verified_name FROM people",
        "the palette's index is built from every person once, at start",
    ),
    (
        "SELECT person, jid FROM person_jids WHERE jid LIKE '%@s.whatsapp.net'",
        "the palette's numbers, read once at start",
    ),
    ("SELECT folder, chat, person FROM folder_items", "the palette's folders, read once at start"),
    (
        "SELECT n, builtin, rule IS NOT NULL, parent, name FROM folders",
        "the palette's folders, read whole at start and when one changes: tens of rows",
    ),
    ("FROM sqlite_schema", "the schema's fingerprint, read at start and after each migration step"),
    (
        "FROM outbox WHERE state = 'unconfirmed'",
        "once at start; the outbox holds at most 1,000 waiting, and 1,000 actions kept for a late receipt",
    ),
    (
        "SELECT count(*), coalesce(sum(state = 'failed'), 0) FROM outbox",
        "the outbox's bound, checked in each send's transaction; it holds at most 1,000 waiting, and 1,000 \
        actions kept for a late receipt",
    ),
    (
        "FROM outbox WHERE state = 'failed' AND kind IN ('reaction', 'edit', 'revoke', 'vote')",
        "the actions kept for a late receipt, pruned as one more is kept, as the writer starts and hourly; the \
        outbox holds at most 1,000 waiting, and 1,000 of these",
    ),
    ("SELECT n FROM chats WHERE told_at IS NOT NULL", "the chats owing read receipts, once at start"),
    ("SELECT message FROM settled LIMIT", "stops at its limit"),
    ("SELECT chat, member FROM regroup LIMIT", "stops at its limit"),
    (
        "SELECT count(*) FROM interface_state",
        "interface_state holds a row for each of 16 interfaces at most, counted when one keeps its first",
    ),
    (
        "SELECT count(*), coalesce(max(octet_length(value)) FILTER (WHERE value ->> '$.format' IS NOT 2), 0)",
        "jawd status, on a table of 16 rows at most",
    ),
    (
        "FROM folders f LEFT JOIN folder_counts k ON k.folder = f.n",
        "the folder tree's window, read whole: the five built-in folders, the smart ones and yours, tens of rows",
    ),
    ("SELECT s.slot, s.folder, f.builtin", "the favourite slots, nine at most"),
    ("FROM favourites s CROSS JOIN folder_people p", "the favourite slots, nine at most, and whom they hold"),
    (
        "SELECT count(*), coalesce(sum(unread), 0), coalesce(sum(mentions), 0) FROM",
        "a smart folder counted as it's read: its rule's index walked, stopping at a thousand",
    ),
    ("FROM folders WHERE rule IS NOT NULL AND n <> ?1", "the smart folders, a handful, put in order"),
    (
        "WHERE (name IS NOT NULL OR direct_chats > 0) AND (about_at IS NULL OR about_at < ?1) ORDER BY about_at LIMIT",
        "people_about_due from the longest unasked, stopping at its limit; what's due comes first",
    ),
    (
        "SELECT n, followed FROM people WHERE presence IS NOT NULL OR followed <> 0",
        "people_present whole: those followed, two hundred at most, and those heard of since",
    ),
    (
        "WHEN 'communities' THEN c.archived = 0 AND (c.kind = 'community' OR c.community IS NOT NULL) ELSE c.archived <> 0 END",
        "the folders' background check, hourly, and the rebuild it asks for when they've drifted: each chat once \
        for each built-in folder",
    ),
    ("EXCEPT SELECT", "the folders' background check, hourly, against a fresh derivation of the tree and what's filed"),
    ("WHERE f.sort IS NOT c.sort OR f.unread IS NOT c.unread", "the folders' background check, hourly"),
    ("p.direct_chats IS NOT (SELECT count(*)", "the folders' background check, hourly, and the rebuild it asks for"),
    (
        "direct_chats IS NOT (SELECT count(*) FROM chats c",
        "the rebuild the folders' check asks for when they've drifted",
    ),
    ("NOT EXISTS (SELECT 1 FROM folder_counts WHERE folder = f.n)", "the folders' background check, hourly"),
    ("DELETE FROM folder_chats", "the rebuild the folders' check asks for when they've drifted"),
    ("DELETE FROM folder_people", "the rebuild the folders' check asks for when they've drifted"),
    ("DELETE FROM folder_paths", "the rebuild the folders' check asks for when they've drifted"),
    ("DELETE FROM folder_counts WHERE folder IN", "the rebuild the folders' check asks for when they've drifted"),
    ("INSERT OR IGNORE INTO folder_counts (folder) SELECT n FROM folders", "the rebuild the folders' check asks for"),
    (
        "UPDATE folder_counts SET unread = 0, mentions = 0, chats = 0, lone = 0",
        "the rebuild the folders' check asks for",
    ),
    (
        "INSERT INTO folder_paths (folder, ancestor) SELECT folder, ancestor FROM up",
        "the rebuild the folders' check asks for",
    ),
    ("SELECT n AS rebuilt FROM folders", "the rebuild the folders' check asks for, every folder once"),
    (
        "FROM chats c CROSS JOIN messages m ON m.chat = c.n AND m.id = ?1",
        "an action naming a message by its ID alone looks for it in each chat, a search of messages_by_id each; \
        chats are few next to messages",
    ),
    (
        "FROM message_links WHERE message =",
        "one message's links, each place counting the distinct URLs its spans mark before it, a handful",
    ),
    (
        "coalesce(sum(bytes), 0) FROM thumbnails",
        "once at start, and for jawd status; the thumbnails are as many as fit in 256 MB, tens of thousands at most",
    ),
];

/// Names in format strings that aren't constants, and what to plan them with.
const STAND_INS: &[(&str, &str)] = &[("column", "read_at"), ("table", "inbox")];

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                walk(&path, out);
            } else if name.ends_with(".rs")
                && !matches!(name.as_str(), "tests.rs" | "rules.rs" | "plans.rs" | "bench.rs")
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// The string literals in Rust source, in order, outside comments and test modules.
fn literals(source: &str) -> Vec<String> {
    let source = source.split("#[cfg(test)]\nmod tests").next().unwrap_or(source);
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            'r' if chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'"') => {
                let start = i + 3;
                let mut end = start;
                while end + 1 < chars.len() && !(chars[end] == '"' && chars[end + 1] == '#') {
                    end += 1;
                }
                out.push(chars[start..end].iter().collect());
                i = end + 2;
            }
            '\'' => {
                // A char literal ('x' or '\x'), not a lifetime.
                if chars.get(i + 1) == Some(&'\\') {
                    i += 4;
                } else if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            '"' => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        match chars.get(i + 1) {
                            Some('\n') => {
                                i += 2;
                                while i < chars.len() && chars[i].is_whitespace() {
                                    i += 1;
                                }
                                continue;
                            }
                            Some('n') => text.push('\n'),
                            Some(other) => text.push(*other),
                            None => {}
                        }
                        i += 2;
                    } else {
                        text.push(chars[i]);
                        i += 1;
                    }
                }
                out.push(text);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// The text a name in a format string stands for: a `const NAME: &str = "..."` or a
/// `let name = format!("...")` in the same file, or a stand-in.
fn names(source: &str) -> HashMap<String, String> {
    fragments(source).into_iter().collect()
}

/// The names `names` finds, with their text.
fn fragments(source: &str) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = STAND_INS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    for (marker, close) in [("const ", ": &str ="), ("let ", " =")] {
        for (at, _) in source.match_indices(marker) {
            let rest = &source[at + marker.len()..];
            let Some(end) = rest.find(close) else { continue };
            let name = &rest[..end];
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let after = rest[end + close.len()..].trim_start();
            let after = after.strip_prefix("format!(").unwrap_or(after).trim_start();
            if (after.starts_with('"') || after.starts_with("r#\""))
                && let Some(text) = literals(after).into_iter().next()
            {
                names.push((name.to_string(), text));
            }
        }
    }
    names
}

fn expand(text: &str, names: &HashMap<String, String>) -> String {
    let mut text = text.to_string();
    for _ in 0..4 {
        for (name, value) in names {
            text = text.replace(&format!("{{{name}}}"), value);
        }
    }
    text
}

fn is_statement(text: &str) -> bool {
    let first = text.split_whitespace().next().unwrap_or_default().trim_end_matches(',');
    matches!(
        first,
        "SELECT"
            | "INSERT"
            | "UPDATE"
            | "DELETE"
            | "WITH"
            | "{CHAT_SELECT}"
            | "{PERSON_SELECT}"
            | "{MESSAGE_COLUMNS}"
            | "{from}"
            | "{CHAT}"
            | "{PERSON}"
    )
}

fn plan(db: &Connection, sql: &str) -> Result<Vec<String>, String> {
    let mut statement = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).map_err(|e| e.to_string())?;
    let nulls = rusqlite::params_from_iter(std::iter::repeat_n(rusqlite::types::Null, statement.parameter_count()));
    let rows: Vec<(i64, i64, String)> = statement
        .query_map(nulls, |r| Ok((r.get(0)?, r.get(1)?, r.get(3)?)))
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())?;
    let mut depth: HashMap<i64, usize> = HashMap::new();
    Ok(rows
        .into_iter()
        .map(|(id, parent, detail)| {
            let d = depth.get(&parent).map_or(0, |d| d + 1);
            depth.insert(id, d);
            format!("{}{detail}", "  ".repeat(d))
        })
        .collect())
}

fn snapshot() -> (String, Vec<String>) {
    let mut db = Connection::open_in_memory().unwrap();
    super::schema::migrate(&mut db).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = String::new();
    let mut broken = Vec::new();
    // A file's own names first, then those of the files it reads them from, as store::lists does
    // store::read's.
    let everywhere: HashMap<String, String> =
        sources().iter().flat_map(|path| fragments(&std::fs::read_to_string(path).unwrap())).collect();
    for path in sources() {
        let source = std::fs::read_to_string(&path).unwrap();
        let mut names = everywhere.clone();
        names.extend(self::names(&source));
        // A fragment is planned in the statements it's part of, unless it's also used on its own.
        let parts: Vec<String> = fragments(&source)
            .into_iter()
            .filter(|(name, _)| {
                source.contains(&format!("{{{name}}}"))
                    && !source.contains(&format!("({name})"))
                    && !source.contains(&format!("(&{name})"))
            })
            .map(|(_, text)| text)
            .collect();
        let file = path.strip_prefix(root).unwrap().display().to_string();
        let mut seen = Vec::new();
        for literal in literals(&source) {
            if !is_statement(&literal) || parts.contains(&literal) {
                continue;
            }
            let sql = expand(&literal, &names).split_whitespace().collect::<Vec<_>>().join(" ");
            if seen.contains(&sql) {
                continue;
            }
            seen.push(sql.clone());
            let _ = writeln!(out, "-- {file}\n{sql}");
            if sql.contains('{') {
                let _ = writeln!(out, "    (built from names this test can't fill in; add them to STAND_INS)\n");
                broken.push(format!("{file}: {sql}\n    built from names this test can't fill in"));
                continue;
            }
            planned(&db, &file, &sql, &mut out, &mut broken);
        }
    }
    // Every statement a trigger runs, with what it reads of the row (new.x, old.x) as a parameter.
    let triggers: Vec<(String, String)> = db
        .prepare("SELECT name, sql FROM sqlite_schema WHERE type = 'trigger' ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for (name, trigger) in triggers {
        let body = trigger.split_once("BEGIN").map_or("", |(_, body)| body);
        let body = body.rsplit_once("END").map_or(body, |(body, _)| body);
        for statement in body.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let sql = row_values(statement).split_whitespace().collect::<Vec<_>>().join(" ");
            let _ = writeln!(out, "-- trigger {name}\n{sql}");
            planned(&db, &format!("trigger {name}"), &sql, &mut out, &mut broken);
        }
    }
    (out, broken)
}

/// `statement`, a trigger's, with each value it reads of the row it fires for as a parameter.
fn row_values(statement: &str) -> String {
    let mut out = String::with_capacity(statement.len());
    let mut rest = statement;
    while let Some(at) = ["new.", "old."].iter().filter_map(|row| rest.find(row)).min() {
        let starts = rest[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '.');
        if !starts {
            out.push_str(&rest[..at + 4]);
            rest = &rest[at + 4..];
            continue;
        }
        out.push_str(&rest[..at]);
        out.push('?');
        let after = &rest[at + 4..];
        rest = &after[after.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(after.len())..];
    }
    out.push_str(rest);
    out
}

/// Plans `sql`, from `file`, into `out`, noting in `broken` a scan or a sort it isn't allowed.
fn planned(db: &Connection, file: &str, sql: &str, out: &mut String, broken: &mut Vec<String>) {
    match plan(db, sql) {
        Ok(lines) => {
            for line in &lines {
                let _ = writeln!(out, "    {line}");
                let scan = line.trim_start().starts_with("SCAN ")
                    && !line.contains("VIRTUAL TABLE")
                    && !line.contains("(subquery")
                    && !line.contains("CONSTANT ROW");
                if (scan || line.contains("USE TEMP B-TREE")) && !ALLOWED.iter().any(|(text, _)| sql.contains(text)) {
                    broken.push(format!("{file}: {sql}\n    {}", line.trim()));
                }
            }
        }
        Err(e) => {
            let _ = writeln!(out, "    (a part of a statement, planned with the rest: {e})");
        }
    }
    out.push('\n');
}

/// The triggers whose statements may sort, and why. ALLOWED's texts let through any statement that
/// contains them, so one written for the folders' rebuild would let a trigger's statement through
/// too; a trigger's needs its own line here.
const TRIGGERS_SORTING: &[(&str, &str)] = &[(
    "links_derived_insert",
    "one message's links, each place counting the distinct URLs its spans mark before it, a handful",
)];

/// Each trigger's statements but those that only raise, as the plan test pins them.
fn trigger_statements(db: &Connection) -> Vec<(String, String)> {
    let triggers: Vec<(String, String)> = db
        .prepare("SELECT name, sql FROM sqlite_schema WHERE type = 'trigger' ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut out = Vec::new();
    for (name, trigger) in triggers {
        let body = trigger.split_once("BEGIN").map_or("", |(_, body)| body);
        let body = body.rsplit_once("END").map_or(body, |(body, _)| body);
        for statement in body.split(';').map(str::trim).filter(|s| !s.is_empty() && !s.contains("RAISE(")) {
            out.push((name.clone(), row_values(statement).split_whitespace().collect::<Vec<_>>().join(" ")));
        }
    }
    out
}

#[test]
fn every_triggers_statements_plan_without_scanning_a_table_or_sorting_in_a_temporary_b_tree() {
    let mut db = Connection::open_in_memory().unwrap();
    super::schema::migrate(&mut db).unwrap();
    let statements = trigger_statements(&db);
    assert!(statements.iter().any(|(name, _)| name == "chats_recounted"), "the folder counts' triggers are planned");
    let mut broken = Vec::new();
    for (name, sql) in statements {
        let lines = plan(&db, &sql).unwrap_or_else(|e| panic!("{name}: {sql}\n    doesn't plan: {e}"));
        for line in lines {
            let scan = line.trim_start().starts_with("SCAN ")
                && !line.contains("VIRTUAL TABLE")
                && !line.contains("CONSTANT ROW")
                && !line.contains("(subquery");
            let sorts =
                line.contains("USE TEMP B-TREE") && TRIGGERS_SORTING.iter().all(|(trigger, _)| *trigger != name);
            if scan || sorts {
                broken.push(format!("{name}: {sql}\n    {}", line.trim()));
            }
        }
    }
    assert!(broken.is_empty(), "trigger statements that scan or sort:\n{}", broken.join("\n"));
}

#[test]
fn every_statement_plans_as_pinned_without_scanning_a_table_or_sorting_in_a_temporary_b_tree() {
    let (now, broken) = snapshot();
    let pinned = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/store/plans.txt");
    if std::env::var("JAW_PLANS").as_deref() == Ok("write") {
        std::fs::write(&pinned, &now).unwrap();
    }
    assert!(
        broken.is_empty(),
        "statements that scan or sort, which need an index or a reason in ALLOWED:\n{}",
        broken.join("\n")
    );
    let was = std::fs::read_to_string(&pinned).unwrap_or_default();
    if was != now {
        let first = was
            .lines()
            .zip(now.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(was.lines().count().min(now.lines().count()));
        let around = |text: &str| text.lines().skip(first.saturating_sub(2)).take(6).collect::<Vec<_>>().join("\n");
        panic!(
            "a statement is new or plans differently from plans.txt; look at it, then JAW_PLANS=write cargo test -p jawd plans\n\
            pinned:\n{}\nnow:\n{}",
            around(&was),
            around(&now)
        );
    }
}
