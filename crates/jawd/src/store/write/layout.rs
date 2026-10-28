//! What interfaces keep of how they were laid out (`interface_state`): each interface's layout, a
//! tree of split panes or a book of layouts on a grid, by its name and a key, with the version the
//! interface gave it. A row is replaced only by one with a newer version, whichever format either
//! is in, so asking again, or a write that comes late, changes nothing; and an interface with no
//! row gets one only while fewer than KEPT_MOST have one.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use jaw_proto::layout::KEPT_MOST;
use rusqlite::{Connection, OptionalExtension, params};

/// What an interface keeps: its name, what it is (`layout`), its JSON as jawd keeps it, the
/// version the interface gave it and when it was asked for, in milliseconds since the epoch.
#[derive(Clone, Debug)]
pub struct Keep {
    pub interface: String,
    pub key: &'static str,
    pub value: String,
    pub v: i64,
    pub at: i64,
    pub kept: Kept,
}

/// Where a write says what it came to, for whoever asked for it. The last attempt at its
/// transaction is the one that stands.
#[derive(Clone, Debug, Default)]
pub struct Kept(Arc<Mutex<Option<Keeping>>>);

impl Kept {
    pub fn take(&self) -> Option<Keeping> {
        self.0.lock().expect("a write's result").take()
    }

    fn put(&self, keeping: Keeping) {
        *self.0.lock().expect("a write's result") = Some(keeping);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keeping {
    /// Written: the interface had no row for it, or one with an older version.
    Written,
    /// A row with this version or a newer one was there, so nothing changed.
    Unchanged,
    /// The interface had no row, and `kept` interfaces have one, KEPT_MOST or more, so nothing was
    /// written.
    Full { kept: u64 },
}

const VERSION: &str = "SELECT v FROM interface_state WHERE interface = ?1 AND key = ?2";
const ROWS: &str = "SELECT count(*) FROM interface_state";
const KEEP: &str = "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
    ON CONFLICT (interface, key) DO UPDATE SET value = excluded.value, v = excluded.v, updated_at = excluded.updated_at
    WHERE excluded.v > interface_state.v";

/// Keeps `keep` unless a row with its version or a newer one is there, or there's no room for its
/// interface; says whether it wrote.
pub(super) fn apply(db: &Connection, keep: &Keep) -> Result<bool> {
    let had: Option<i64> =
        db.prepare_cached(VERSION)?.query_row(params![keep.interface, keep.key], |r| r.get(0)).optional()?;
    if had.is_none() {
        let rows: i64 = db.prepare_cached(ROWS)?.query_row([], |r| r.get(0))?;
        let kept = rows.max(0) as u64;
        if kept >= KEPT_MOST {
            keep.kept.put(Keeping::Full { kept });
            return Ok(false);
        }
    }
    let written = db.prepare_cached(KEEP)?.execute(params![keep.interface, keep.key, keep.value, keep.v, keep.at])? > 0;
    keep.kept.put(if written { Keeping::Written } else { Keeping::Unchanged });
    Ok(written)
}

/// How a write that failed is named in quarantine, where the layout it was to keep is kept.
pub(super) fn provenance(keep: &Keep) -> (&'static str, String, Option<&[u8]>) {
    ("interface_state", format!("{} {}", keep.interface, keep.key), Some(keep.value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::store::{Change, LayoutsKept, Store, schema};

    async fn kept(store: &Store, interface: &str, value: &str, v: i64, at: i64) -> Keeping {
        let kept = Kept::default();
        let keep = Keep { interface: interface.into(), key: "layout", value: value.into(), v, at, kept: kept.clone() };
        store.apply_durable(vec![Change::Layout(keep)]).await.unwrap();
        kept.take().expect("the write says what it came to")
    }

    fn row(home: &Path, interface: &str) -> Option<(String, i64, i64)> {
        let db = Connection::open(home.join("jaw.db")).unwrap();
        db.query_row(
            "SELECT value, v, updated_at FROM interface_state WHERE interface = ?1 AND key = 'layout'",
            [interface],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_a_newer_version_replaces_what_an_interface_keeps_so_asking_again_or_late_changes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        assert_eq!(kept(&store, "jaw", r#"{"n":5}"#, 5, 100).await, Keeping::Written);
        assert_eq!(kept(&store, "jaw", r#"{"n":5}"#, 5, 200).await, Keeping::Unchanged, "asked again");
        assert_eq!(kept(&store, "jaw", r#"{"n":3}"#, 3, 300).await, Keeping::Unchanged, "an older one, late");
        assert_eq!(kept(&store, "jaw", r#"{"m":5}"#, 5, 400).await, Keeping::Unchanged, "another of that version");
        assert_eq!(row(home.path(), "jaw"), Some((r#"{"n":5}"#.into(), 5, 100)), "as it was first written");
        assert_eq!(kept(&store, "jaw", r#"{"n":7}"#, 7, 500).await, Keeping::Written);
        assert_eq!(row(home.path(), "jaw"), Some((r#"{"n":7}"#.into(), 7, 500)));
        assert_eq!(row(home.path(), "jaw-mac"), None, "each interface's own");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_interface_with_nothing_kept_gets_a_row_only_while_fewer_than_16_have_one() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        for n in 0..KEPT_MOST {
            assert_eq!(kept(&store, &format!("app {n}"), "{}", 1, 1).await, Keeping::Written);
        }
        assert_eq!(kept(&store, "one more", "{}", 1, 1).await, Keeping::Full { kept: KEPT_MOST });
        assert_eq!(row(home.path(), "one more"), None);
        assert_eq!(
            kept(&store, "app 0", r#"{"n":2}"#, 2, 2).await,
            Keeping::Written,
            "one with a row still changes it"
        );
        assert_eq!(
            store.layouts_kept().unwrap(),
            LayoutsKept { kept: KEPT_MOST, largest: 7, books: 0, largest_book: 0 },
            "16 kept, the biggest {{\"n\":2}}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn status_tells_books_from_trees_by_their_format_and_counts_each_ones_biggest() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let tree = r#"{"format":1,"focused":0,"tree":{"chat":"1@g.us"}}"#;
        let book = r#"{"format":2,"layouts":[]}"#;
        let bigger = r#"{"format":2,"current":"Work","layouts":[]}"#;
        let formatless = format!("[{}0]", "0,".repeat(40));
        assert!(formatless.len() > tree.len().max(bigger.len()));
        for (interface, value) in [("jaw", tree), ("jaw-mac", book), ("jaw-mac 2", bigger), ("other", &formatless)] {
            assert_eq!(kept(&store, interface, value, 1, 1).await, Keeping::Written);
        }
        assert_eq!(
            store.layouts_kept().unwrap(),
            LayoutsKept { kept: 4, largest: formatless.len() as u64, books: 2, largest_book: bigger.len() as u64 },
            "a value of no format counts beside the trees"
        );
    }

    #[test]
    fn the_table_holds_json_of_64_kib_at_most_a_version_from_1_and_names_of_1_to_64_bytes() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let insert = |interface: &str, value: &str, v: i64| {
            db.execute(
                "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES (?1, 'layout', ?2, ?3, 0)",
                params![interface, value, v],
            )
        };
        assert!(insert("jaw", "{}", 1).is_ok());
        assert!(insert("a", "not json", 1).is_err(), "not JSON");
        assert!(insert("b", &format!("\"{}\"", "ã".repeat(32_768)), 1).is_err(), "65,538 bytes in 32,770 letters");
        assert!(insert("c", &format!("\"{}\"", "x".repeat(65_534)), 1).is_ok(), "65,536 bytes");
        assert!(insert("d", "{}", 0).is_err(), "a version of 0");
        assert!(insert(&"x".repeat(65), "{}", 1).is_err(), "a name of 65 bytes");
        assert!(insert("", "{}", 1).is_err(), "no name");
        assert!(insert("jaw", "{}", 2).is_err(), "a row for each interface and key");
        let typed = db.execute(
            "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES ('e', 'layout', '{}', 'x', 0)",
            [],
        );
        assert!(typed.is_err(), "a version that isn't a number, which only STRICT refuses");
    }

    #[test]
    fn the_tables_checks_hold_at_their_edges() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let insert = |interface: &str, key: &str, value: &str| {
            db.execute(
                "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES (?1, ?2, ?3, 1, 0)",
                params![interface, key, value],
            )
        };
        assert!(insert("a", "layout", &format!("\"{}\"", "x".repeat(65_534))).is_ok(), "65,536 bytes");
        assert!(insert("b", "layout", &format!("\"{}\"", "x".repeat(65_535))).is_err(), "65,537 bytes");
        assert!(insert("c", "", "{}").is_err(), "no key");
        assert!(insert("d", &"k".repeat(65), "{}").is_err(), "a key of 65 bytes");
        assert!(insert("e", &"k".repeat(64), "{}").is_ok(), "a key of 64 bytes");
    }

    #[test]
    fn rebuilt_to_take_a_book_the_table_keeps_every_row_it_had_as_it_was() {
        let mut db = Connection::open_in_memory().unwrap();
        // jaw.db as migration 16 left it, with the most it held then: 16 interfaces, one at 16 KiB.
        let before = schema::MIGRATIONS.len() - 1;
        for migration in &schema::MIGRATIONS[..before] {
            db.execute_batch(migration).unwrap();
        }
        db.pragma_update(None, "user_version", before as i64).unwrap();
        let rows: Vec<(String, String, i64, i64)> = (0..KEPT_MOST as i64)
            .map(|n| {
                let value =
                    if n == 0 { format!("\"{}\"", "ã".repeat(8_191)) } else { format!(r#"{{"format":1,"n":{n}}}"#) };
                (format!("app {n}"), value, 1_792_000_000_000 + n, 1_000 + n)
            })
            .collect();
        assert_eq!(rows[0].1.len(), 16_384);
        for (interface, value, v, at) in &rows {
            db.execute(
                "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES (?1, 'layout', ?2, ?3, ?4)",
                params![interface, value, v, at],
            )
            .unwrap();
        }
        let too_big = format!("\"{}\"", "x".repeat(16_383));
        let refused =
            "INSERT INTO interface_state (interface, key, value, v, updated_at) VALUES ('big', 'layout', ?1, 1, 0)";
        assert!(db.execute(refused, [&too_big]).is_err(), "16,385 bytes, before the rebuild");
        schema::migrate(&mut db).unwrap();
        let after: Vec<(String, String, i64, i64)> = db
            .prepare("SELECT interface, value, v, updated_at FROM interface_state WHERE key = 'layout' ORDER BY v")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(after, rows, "every row, as it was");
        assert!(db.execute(refused, [&too_big]).is_ok(), "and a book bigger than a tree may be now");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_layout_that_fails_to_be_written_is_quarantined_with_its_json_and_asking_again_keeps_it() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let outside = Connection::open(home.path().join("jaw.db")).unwrap();
        outside
            .execute_batch(
                "CREATE TRIGGER refuse BEFORE INSERT ON interface_state BEGIN SELECT RAISE(ABORT, 'no room'); END",
            )
            .unwrap();
        let said = Kept::default();
        let value = r#"{"n":5}"#;
        let keep =
            Keep { interface: "jaw".into(), key: "layout", value: value.into(), v: 5, at: 1, kept: said.clone() };
        let applied = store.apply_durable(vec![Change::Layout(keep)]).await.unwrap();
        assert_eq!((applied.changed, applied.quarantined, said.take()), (0, 1, None), "nothing written");
        let quarantined: (String, String, Vec<u8>) = outside
            .query_row("SELECT source, ref, raw FROM quarantine", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(quarantined, ("interface_state".into(), "jaw layout".into(), value.as_bytes().to_vec()));
        assert_eq!(row(home.path(), "jaw"), None);
        outside.execute_batch("DROP TRIGGER refuse").unwrap();
        assert_eq!(kept(&store, "jaw", value, 5, 2).await, Keeping::Written, "asked again, it's kept");
        assert_eq!(row(home.path(), "jaw"), Some((value.into(), 5, 2)));
    }
}
