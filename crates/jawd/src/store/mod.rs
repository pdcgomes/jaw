//! jaw.db, through rusqlite with SQLite compiled in. One writer thread owns the write connection
//! and takes realtime writes first: a message is committed, then acked, before WhatsApp hears it
//! arrived. History goes in transactions of at most 1,000 rows or 10 ms, with any realtime writes
//! that came meanwhile taken between them.

mod schema;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, bounded, select_biased, unbounded};
use rusqlite::{Connection, params};
use tokio::sync::oneshot;

use crate::engine::Ack;

/// A message from history, as it's kept until it's converted.
pub struct HistoryRow {
    pub chat: String,
    pub id: String,
    pub at_ms: i64,
    /// WAWebProtobufsWeb.WebMessageInfo.
    pub raw: Vec<u8>,
}

/// How a batch of history went into the database.
#[derive(Clone, Copy, Debug, Default)]
pub struct Written {
    pub rows: usize,
    pub transactions: usize,
    pub took: Duration,
}

enum Realtime {
    /// An engine event to keep; `ack` goes once it's committed.
    Inbox { kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack> },
}

enum Background {
    History { rows: Vec<HistoryRow>, done: oneshot::Sender<Result<Written, String>> },
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    realtime: Sender<Realtime>,
    background: Sender<Background>,
}

const BATCH_ROWS: usize = 1_000;
const BATCH_TIME: Duration = Duration::from_millis(10);

impl Store {
    /// Opens jaw.db in `home`, bringing its schema up to date, and starts the writer.
    pub fn open(home: &Path) -> Result<Store> {
        let path = home.join("jaw.db");
        let mut db = Connection::open(&path).with_context(|| format!("can't open {}", path.display()))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "temp_store", "MEMORY")?;
        db.busy_timeout(Duration::from_secs(5))?;
        schema::migrate(&mut db)?;
        let checkpointer = Connection::open(&path)?;
        checkpointer.busy_timeout(Duration::from_secs(5))?;
        let (realtime, realtime_rx) = unbounded();
        let (background, background_rx) = bounded(16);
        let commits = Arc::new(AtomicU64::new(0));
        let counted = commits.clone();
        std::thread::Builder::new()
            .name("jawd-writer".into())
            .spawn(move || write(db, realtime_rx, background_rx, counted))?;
        std::thread::Builder::new().name("jawd-checkpoint".into()).spawn(move || checkpoint(checkpointer, commits))?;
        Ok(Store { path, realtime, background })
    }

    /// Keeps an engine event, then sends its ack. It never waits: realtime writes aren't bounded.
    pub fn inbox(&self, kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack>) {
        let _ = self.realtime.send(Realtime::Inbox { kind, at_ms, payload, ack });
    }

    /// Stores history rows, waiting for room behind earlier batches, and says how it went.
    pub async fn history(&self, rows: Vec<HistoryRow>) -> Result<Written> {
        let (done, written) = oneshot::channel();
        let background = self.background.clone();
        tokio::task::spawn_blocking(move || background.send(Background::History { rows, done }))
            .await?
            .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?;
        written.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// How many rows `table` holds, read on a connection of its own so the writer doesn't wait.
    pub fn count(&self, table: &str) -> Result<u64> {
        let db = Connection::open(&self.path)?;
        let count: i64 = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))?;
        Ok(count.max(0) as u64)
    }
}

/// How often the checkpointer copies the WAL into the database, when anything was committed.
const CHECKPOINT_EVERY: Duration = Duration::from_secs(1);

/// The writer: realtime first, then background in short transactions. SQLite's own checkpoints are
/// off, so no commit pays for one; the checkpointer does them on a connection of its own.
fn write(mut db: Connection, realtime: Receiver<Realtime>, background: Receiver<Background>, commits: Arc<AtomicU64>) {
    if let Err(e) = db.pragma_update(None, "wal_autocheckpoint", 0) {
        eprintln!("jawd: can't turn SQLite's own checkpoints off: {e}");
    }
    loop {
        select_biased! {
            recv(realtime) -> write => match write {
                Ok(write) => {
                    realtime_write(&mut db, write);
                    commits.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => return,
            },
            recv(background) -> write => match write {
                Ok(Background::History { rows, done }) => {
                    let written = history(&mut db, &rows, &realtime);
                    commits.fetch_add(1, Ordering::Relaxed);
                    let _ = done.send(written.map_err(|e| format!("{e:#}")));
                }
                Err(_) => return,
            },
        }
    }
}

/// Copies the WAL into the database once a second, if anything was committed, without waiting for
/// readers or holding up the writer, which keeps appending meanwhile. It ends when the writer has.
fn checkpoint(db: Connection, commits: Arc<AtomicU64>) {
    let mut seen = 0;
    while Arc::strong_count(&commits) > 1 {
        std::thread::sleep(CHECKPOINT_EVERY);
        let now = commits.load(Ordering::Relaxed);
        if now == seen {
            continue;
        }
        seen = now;
        if let Err(e) = db.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(())) {
            eprintln!("jawd: a checkpoint failed: {e}");
        }
    }
}

fn realtime_write(db: &mut Connection, write: Realtime) {
    match write {
        Realtime::Inbox { kind, at_ms, payload, ack } => {
            let stored =
                db.execute("INSERT INTO inbox (kind, at, payload) VALUES (?1, ?2, ?3)", params![kind, at_ms, payload]);
            match stored {
                Ok(_) => {
                    if let Some(ack) = ack {
                        ack.send();
                    }
                }
                // Without its ack, the engine keeps waiting and WhatsApp keeps the message.
                Err(e) => eprintln!("jawd: can't keep a {kind} in the inbox, so it isn't acked: {e}"),
            }
        }
    }
}

/// Inserts history rows in transactions of at most BATCH_ROWS or BATCH_TIME, doing any realtime
/// writes that arrived between them.
fn history(db: &mut Connection, rows: &[HistoryRow], realtime: &Receiver<Realtime>) -> Result<Written> {
    let started = Instant::now();
    let mut written = Written::default();
    let mut rest = rows;
    while !rest.is_empty() {
        let began = Instant::now();
        let tx = db.transaction()?;
        let mut taken = 0;
        {
            let mut insert =
                tx.prepare_cached("INSERT OR IGNORE INTO history_raw (chat, id, at, raw) VALUES (?1, ?2, ?3, ?4)")?;
            for row in rest {
                insert.execute(params![row.chat, row.id, row.at_ms, row.raw])?;
                taken += 1;
                if taken == BATCH_ROWS || began.elapsed() >= BATCH_TIME {
                    break;
                }
            }
        }
        tx.commit()?;
        written.rows += taken;
        written.transactions += 1;
        rest = &rest[taken..];
        while let Ok(write) = realtime.try_recv() {
            realtime_write(db, write);
        }
    }
    written.took = started.elapsed();
    Ok(written)
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::engine::demo::DemoEngine;
    use crate::engine::{Engine, Incoming, pb};

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_is_acked_only_once_it_is_committed() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let _instance = DemoEngine::new().start(tx).await.unwrap();
        let mut stored = 0;
        while stored < 3 {
            let Incoming { event, ack } = rx.recv().await.unwrap();
            if let Some(pb::event::Kind::Message(message)) = &event.kind {
                let before = store.count("inbox").unwrap();
                assert_eq!(before, stored, "the next message came before the last was committed");
                store.inbox(
                    "message",
                    message.info.as_ref().unwrap().timestamp_ms,
                    prost::Message::encode_to_vec(message),
                    ack,
                );
                stored += 1;
            }
        }
        while store.count("inbox").unwrap() < 3 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn history_goes_in_short_transactions() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let rows: Vec<HistoryRow> = (0..2_500)
            .map(|n| HistoryRow { chat: format!("chat{}", n % 7), id: format!("id{n}"), at_ms: n, raw: vec![1; 300] })
            .collect();
        let written = store.history(rows).await.unwrap();
        assert_eq!(written.rows, 2_500);
        assert!(written.transactions >= 3, "{} transactions", written.transactions);
        assert_eq!(store.count("history_raw").unwrap(), 2_500);
        let again = store
            .history(vec![HistoryRow { chat: "chat0".into(), id: "id0".into(), at_ms: 0, raw: vec![] }])
            .await
            .unwrap();
        assert_eq!(again.rows, 1);
        assert_eq!(store.count("history_raw").unwrap(), 2_500, "a message seen again is kept once");
    }
}
