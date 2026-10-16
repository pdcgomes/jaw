//! jaw.db's schema, as numbered migrations run in order in one transaction at start; `PRAGMA
//! user_version` says how many have run.

use anyhow::{Context, Result, bail};
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 1: what the engine sends, kept before WhatsApp hears it arrived, and history as it came.
    "
    CREATE TABLE inbox (
        seq INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        at INTEGER NOT NULL,
        payload BLOB NOT NULL
    );
    CREATE TABLE history_raw (
        chat TEXT NOT NULL,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        raw BLOB NOT NULL,
        PRIMARY KEY (chat, id)
    ) WITHOUT ROWID;
    ",
];

pub fn migrate(db: &mut Connection) -> Result<()> {
    let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let version = usize::try_from(version).unwrap_or(usize::MAX);
    if version > MIGRATIONS.len() {
        bail!("jaw.db is from a newer jawd (schema {version}, this jawd knows {})", MIGRATIONS.len());
    }
    if version == MIGRATIONS.len() {
        return Ok(());
    }
    let tx = db.transaction()?;
    for (n, migration) in MIGRATIONS.iter().enumerate().skip(version) {
        tx.execute_batch(migration).with_context(|| format!("jaw.db's migration {}", n + 1))?;
    }
    tx.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once_and_refuse_a_newer_database() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&mut db).unwrap();
        migrate(&mut db).unwrap();
        let version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        db.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1).unwrap();
        assert!(migrate(&mut db).unwrap_err().to_string().contains("newer jawd"));
    }
}
