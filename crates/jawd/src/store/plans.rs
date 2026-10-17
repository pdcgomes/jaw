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
    ("SELECT n, parent, name FROM folders", "the palette's folders, read once at start"),
    ("FROM sqlite_schema", "the schema's fingerprint, read at start and after each migration step"),
    ("FROM outbox WHERE state = 'unconfirmed'", "once at start; the outbox holds at most 1,000 messages"),
    (
        "SELECT count(*), coalesce(sum(state = 'failed'), 0) FROM outbox",
        "the outbox's bound, checked in each send's transaction; it holds at most 1,000 messages",
    ),
    ("SELECT n FROM chats WHERE told_at IS NOT NULL", "the chats owing read receipts, once at start"),
    ("SELECT message FROM settled LIMIT", "stops at its limit"),
    ("SELECT chat, member FROM regroup LIMIT", "stops at its limit"),
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
    let first = text.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "SELECT"
            | "INSERT"
            | "UPDATE"
            | "DELETE"
            | "WITH"
            | "{CHAT_COLUMNS}"
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
    for path in sources() {
        let source = std::fs::read_to_string(&path).unwrap();
        let names = names(&source);
        // A fragment is planned in the statements it's part of, unless it's also used on its own.
        let parts: Vec<String> = fragments(&source)
            .into_iter()
            .filter(|(name, _)| !source.contains(&format!("({name})")))
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
            match plan(&db, &sql) {
                Ok(lines) => {
                    for line in &lines {
                        let _ = writeln!(out, "    {line}");
                        let scan = line.trim_start().starts_with("SCAN ")
                            && !line.contains("VIRTUAL TABLE")
                            && !line.contains("(subquery")
                            && !line.contains("CONSTANT ROW");
                        if (scan || line.contains("USE TEMP B-TREE"))
                            && !ALLOWED.iter().any(|(text, _)| sql.contains(text))
                        {
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
    }
    (out, broken)
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
