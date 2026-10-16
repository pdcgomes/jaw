//! jaw.db, through rusqlite with SQLite compiled in. One writer thread owns the write connection
//! and takes the inbox first: a message is committed, then acked, before WhatsApp hears it
//! arrived. Live changes (conversions as messages arrive, and actions) come next, each in a
//! transaction of its own. Background changes and history go in transactions of at most 1,000
//! rows or 10 ms, with any inbox and live writes that came meanwhile taken between them. Queries go
//! to the read pool, so a page never waits behind a sync.

pub mod bench;
pub mod read;
mod schema;
mod write;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, bounded, select_biased, unbounded};
use rusqlite::{Connection, params};
use tokio::sync::{oneshot, watch};

pub use self::read::Reads;
use self::write::Stamps;
pub use self::write::{Change, Chat, Message, Opened, Packed, Person, frecency};
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

/// How a batch of changes went into the database.
#[derive(Clone, Copy, Debug, Default)]
pub struct Applied {
    /// The changes that changed something; the rest matched what was there.
    pub changed: usize,
    pub transactions: usize,
    pub took: Duration,
    /// The feed's last seq once they were committed.
    pub seq: i64,
}

/// Which of the writer's queues a batch of changes joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Messages converted as they arrive, and what someone just did: one transaction, after the
    /// inbox and ahead of any background batch.
    Live,
    /// History and other bulk work: short transactions, waiting for room behind earlier batches.
    Background,
}

enum Realtime {
    /// An engine event to keep; `ack` goes once it's committed.
    Inbox { kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack> },
}

type Done = oneshot::Sender<Result<Applied, String>>;

struct Live {
    changes: Vec<Change>,
    done: Done,
}

enum Background {
    History { rows: Vec<HistoryRow>, done: oneshot::Sender<Result<Written, String>> },
    Changes { changes: Vec<Change>, done: Done },
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    realtime: Sender<Realtime>,
    live: Sender<Live>,
    background: Sender<Background>,
    reads: Reads,
    committed: watch::Receiver<i64>,
}

const BATCH_ROWS: usize = 1_000;
const BATCH_TIME: Duration = Duration::from_millis(10);
const WRITER_CACHE_KIB: i64 = 32 * 1024;

impl Store {
    /// Opens jaw.db in `home`, bringing its schema up to date, and starts the writer and readers.
    pub fn open(home: &Path) -> Result<Store> {
        let path = home.join("jaw.db");
        let mut db = Connection::open(&path).with_context(|| format!("can't open {}", path.display()))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "temp_store", "MEMORY")?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.set_prepared_statement_cache_capacity(64);
        // History lands all over the indexes, so the writer keeps more of them than SQLite's 2 MB.
        db.pragma_update(None, "cache_size", -WRITER_CACHE_KIB)?;
        schema::migrate(&mut db)?;
        let stamps = Stamps::load(&db)?;
        let reads = Reads::open(&path)?;
        let checkpointer = Connection::open(&path)?;
        checkpointer.busy_timeout(Duration::from_secs(5))?;
        let (realtime, realtime_rx) = unbounded();
        let (live, live_rx) = unbounded();
        let (background, background_rx) = bounded(16);
        let commits = Arc::new(AtomicU64::new(0));
        let (seq, committed) = watch::channel(stamps.last());
        let writer = Writer {
            db,
            stamps,
            realtime: realtime_rx,
            live: live_rx,
            commits: commits.clone(),
            seq,
            pruned: Instant::now(),
            commit: Duration::ZERO,
        };
        std::thread::Builder::new().name("jawd-writer".into()).spawn(move || writer.run(background_rx))?;
        std::thread::Builder::new().name("jawd-checkpoint".into()).spawn(move || checkpoint(checkpointer, commits))?;
        Ok(Store { path, realtime, live, background, reads, committed })
    }

    /// The feed's last committed seq, which changes as each transaction with events commits.
    pub fn committed(&self) -> watch::Receiver<i64> {
        self.committed.clone()
    }

    /// Keeps an engine event, then sends its ack. It never waits: realtime writes aren't bounded.
    pub fn inbox(&self, kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack>) {
        let _ = self.realtime.send(Realtime::Inbox { kind, at_ms, payload, ack });
    }

    /// Stores history rows, waiting for room behind earlier batches, and says how it went.
    pub async fn history(&self, rows: Vec<HistoryRow>) -> Result<Written> {
        let (done, written) = oneshot::channel();
        self.queue_background(Background::History { rows, done }).await?;
        written.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// Makes `changes` in `lane`, each with its event, and says how it went once they're committed.
    /// A change that fails rolls back its transaction; on the background lane, earlier
    /// transactions of the batch stay committed.
    pub async fn apply(&self, lane: Lane, changes: Vec<Change>) -> Result<Applied> {
        let (done, applied) = oneshot::channel();
        match lane {
            Lane::Live => {
                self.live.send(Live { changes, done }).map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?
            }
            Lane::Background => self.queue_background(Background::Changes { changes, done }).await?,
        }
        applied.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    async fn queue_background(&self, work: Background) -> Result<()> {
        let background = self.background.clone();
        tokio::task::spawn_blocking(move || background.send(work))
            .await?
            .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))
    }

    /// The read pool.
    pub fn reads(&self) -> &Reads {
        &self.reads
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

/// How often the writer removes events older than the feed keeps, when it has background work.
const PRUNE_EVERY: Duration = Duration::from_secs(3600);

struct Writer {
    db: Connection,
    stamps: Stamps,
    realtime: Receiver<Realtime>,
    live: Receiver<Live>,
    commits: Arc<AtomicU64>,
    /// The feed's last committed seq, for whoever follows the feed.
    seq: watch::Sender<i64>,
    pruned: Instant,
    /// What committing a background transaction has lately cost, so the next stops in time for
    /// its commit to fit in its 10 ms too.
    commit: Duration,
}

impl Writer {
    /// Inbox first, then live changes, then background work in short transactions. SQLite's own
    /// checkpoints are off, so no commit pays for one; the checkpointer does them on a connection
    /// of its own.
    fn run(mut self, background: Receiver<Background>) {
        if let Err(e) = self.db.pragma_update(None, "wal_autocheckpoint", 0) {
            eprintln!("jawd: can't turn SQLite's own checkpoints off: {e}");
        }
        self.prune();
        let (realtime, live) = (self.realtime.clone(), self.live.clone());
        loop {
            select_biased! {
                recv(realtime) -> write => match write {
                    Ok(write) => self.realtime_write(write),
                    Err(_) => return,
                },
                recv(live) -> write => match write {
                    Ok(write) => self.live_write(write),
                    Err(_) => return,
                },
                recv(background) -> write => match write {
                    Ok(Background::History { rows, done }) => {
                        let written = self.history(&rows);
                        let _ = done.send(written.map_err(|e| format!("{e:#}")));
                    }
                    Ok(Background::Changes { changes, done }) => {
                        let applied = self.background_changes(&changes);
                        let _ = done.send(applied.map_err(|e| format!("{e:#}")));
                        if self.pruned.elapsed() >= PRUNE_EVERY {
                            self.prune();
                        }
                    }
                    Err(_) => return,
                },
            }
        }
    }

    fn realtime_write(&mut self, write: Realtime) {
        realtime_write(&mut self.db, write);
        self.commits.fetch_add(1, Ordering::Relaxed);
    }

    fn live_write(&mut self, Live { changes, done }: Live) {
        let started = Instant::now();
        let applied = self.transaction(&changes, usize::MAX, Duration::MAX).map(|(changed, _)| Applied {
            changed,
            transactions: 1,
            took: started.elapsed(),
            seq: self.stamps.last(),
        });
        let _ = done.send(applied.map_err(|e| format!("{e:#}")));
    }

    /// Makes changes from the start of `changes` in one transaction, until `rows` are taken or
    /// `time` would be passed once the commit is counted, and says how many changed something and
    /// how many were taken.
    fn transaction(&mut self, changes: &[Change], rows: usize, time: Duration) -> Result<(usize, usize)> {
        let began = Instant::now();
        self.stamps.begin();
        let made = (|| {
            let tx = self.db.transaction()?;
            let (mut changed, mut taken) = (0, 0);
            let committing = self.commit.min(time / 2);
            for change in changes {
                changed += usize::from(write::apply(&tx, &mut self.stamps, change)?);
                taken += 1;
                if taken == rows || time.saturating_sub(began.elapsed()) <= committing {
                    break;
                }
            }
            let body = began.elapsed();
            tx.commit()?;
            let commit = began.elapsed() - body;
            if time < Duration::MAX {
                self.commit = (self.commit * 3 + commit) / 4;
            }
            Ok((changed, taken))
        })();
        match &made {
            Ok(_) => {
                self.commits.fetch_add(1, Ordering::Relaxed);
                let last = self.stamps.last();
                self.seq.send_if_modified(|seq| std::mem::replace(seq, last) != last);
            }
            Err(_) => self.stamps.rolled_back(),
        }
        made
    }

    /// Takes the inbox and live writes waiting, between two background transactions.
    fn between(&mut self) {
        loop {
            if let Ok(write) = self.realtime.try_recv() {
                self.realtime_write(write);
            } else if let Ok(write) = self.live.try_recv() {
                self.live_write(write);
            } else {
                return;
            }
        }
    }

    fn background_changes(&mut self, changes: &[Change]) -> Result<Applied> {
        let started = Instant::now();
        let mut applied = Applied::default();
        let mut rest = changes;
        while !rest.is_empty() {
            let (changed, taken) = self.transaction(rest, BATCH_ROWS, BATCH_TIME)?;
            applied.changed += changed;
            applied.transactions += 1;
            rest = &rest[taken..];
            applied.seq = self.stamps.last();
            self.between();
        }
        applied.took = started.elapsed();
        Ok(applied)
    }

    /// Inserts history rows in transactions of at most BATCH_ROWS or BATCH_TIME, doing any inbox
    /// and live writes that arrived between them.
    fn history(&mut self, rows: &[HistoryRow]) -> Result<Written> {
        let started = Instant::now();
        let mut written = Written::default();
        let mut rest = rows;
        while !rest.is_empty() {
            let began = Instant::now();
            let tx = self.db.transaction()?;
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
            self.commits.fetch_add(1, Ordering::Relaxed);
            written.rows += taken;
            written.transactions += 1;
            rest = &rest[taken..];
            self.between();
        }
        written.took = started.elapsed();
        Ok(written)
    }

    /// Removes events older than the feed keeps, a short transaction at a time.
    fn prune(&mut self) {
        self.pruned = Instant::now();
        loop {
            let pruned = (|| {
                let tx = self.db.transaction()?;
                let pruned = write::prune(&tx, BATCH_ROWS)?;
                tx.commit()?;
                anyhow::Ok(pruned)
            })();
            match pruned {
                Ok(n) if n == BATCH_ROWS => self.between(),
                Ok(_) => return,
                Err(e) => {
                    eprintln!("jawd: can't remove old events from the feed: {e:#}");
                    return;
                }
            }
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

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::read::{self, Around};
    use super::*;
    use crate::engine::demo::DemoEngine;
    use crate::engine::{Engine, Incoming, pb};
    use crate::work::Priority;

    fn text(chat: &str, id: &str, at: i64, words: &str) -> Message {
        Message {
            chat: chat.into(),
            id: id.into(),
            at,
            sender: Some(chat.into()),
            kind: "text",
            text: Some(words.into()),
            summary: words.into(),
            ..Message::default()
        }
    }

    fn messages(changes: impl IntoIterator<Item = Message>) -> Vec<Change> {
        changes.into_iter().map(Change::Message).collect()
    }

    async fn query<T: Send + 'static>(
        store: &Store,
        query: impl FnOnce(&Connection) -> Result<T> + Send + 'static,
    ) -> T {
        store.reads().query(Priority::Interactive, query).await.unwrap()
    }

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

    #[tokio::test(flavor = "multi_thread")]
    async fn a_database_from_the_spike_opens_with_its_inbox_and_history_kept() {
        let home = tempfile::tempdir().unwrap();
        {
            let db = Connection::open(home.path().join("jaw.db")).unwrap();
            db.pragma_update(None, "journal_mode", "WAL").unwrap();
            db.execute_batch(schema::MIGRATIONS[0]).unwrap();
            db.pragma_update(None, "user_version", 1).unwrap();
            db.execute("INSERT INTO inbox (kind, at, payload) VALUES ('message', 5, x'0102')", []).unwrap();
            db.execute("INSERT INTO history_raw (chat, id, at, raw) VALUES ('a@g.us', 'm1', 7, x'03')", []).unwrap();
        }
        let store = Store::open(home.path()).unwrap();
        assert_eq!((store.count("inbox").unwrap(), store.count("history_raw").unwrap()), (1, 1));
        let raw: Vec<u8> =
            query(&store, |db| Ok(db.query_row("SELECT raw FROM history_raw WHERE id = 'm1'", [], |r| r.get(0))?))
                .await;
        assert_eq!(raw, [3]);
        store.apply(Lane::Live, messages([text("a@g.us", "m1", 7, "hi")])).await.unwrap();
        assert_eq!(store.count("messages").unwrap(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pages_at_any_depth_come_in_order_with_no_gaps_or_repeats() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000001@s.whatsapp.net";
        // Every third message shares its millisecond with the one before, as history's
        // second-resolution times do; another chat's messages fall between them.
        let mut by_ms: Vec<Vec<Message>> = Vec::new();
        let mut expected = Vec::new();
        for n in 0..2_345i64 {
            let at = 1_700_000_000_000 + (n - n / 3) * 1000;
            if by_ms.last().is_none_or(|same| same[0].at != at) {
                by_ms.push(Vec::new());
            }
            let same = by_ms.last_mut().unwrap();
            same.push(text(chat, &format!("m{n}"), at, "hello"));
            same.push(text("351910000002@s.whatsapp.net", &format!("o{n}"), at, "other"));
            expected.push(format!("m{n}"));
        }
        // Older messages arriving after newer ones still land in their place; messages with the
        // same millisecond keep the order they came in.
        by_ms.reverse();
        store.apply(Lane::Background, messages(by_ms.into_iter().flatten())).await.unwrap();
        for page in [100, 7] {
            let mut ids = Vec::new();
            let mut around = Around::Newest;
            loop {
                let rows = query(&store, move |db| read::messages(db, chat, around, page)).await;
                let Some(first) = rows.first() else { break };
                around = Around::Before(first.cursor());
                ids.splice(0..0, rows.iter().map(|row| row.id.clone()));
            }
            assert_eq!(ids, expected, "walking back {page} at a time");
            let oldest = query(&store, move |db| read::messages(db, chat, Around::Newest, 2_345)).await[0].cursor();
            let mut ids = vec!["m0".to_string()];
            let mut around = Around::After(oldest);
            loop {
                let rows = query(&store, move |db| read::messages(db, chat, around, page)).await;
                let Some(last) = rows.last() else { break };
                around = Around::After(last.cursor());
                ids.extend(rows.iter().map(|row| row.id.clone()));
            }
            assert_eq!(ids, expected, "walking forward {page} at a time");
        }
        let nowhere = query(&store, |db| read::messages(db, "nobody@s.whatsapp.net", Around::Newest, 10)).await;
        assert!(nowhere.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_is_accent_blind_newest_first_and_pages_with_its_cursor() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (ana, rui) = ("351910000001@s.whatsapp.net", "351910000002@s.whatsapp.net");
        let mut changes = Vec::new();
        for n in 0..25i64 {
            let chat = if n % 2 == 0 { ana } else { rui };
            changes.push(text(chat, &format!("m{n}"), 1_000_000 + n * 1000, &format!("{n} ainda não sei")));
        }
        changes.push(text(ana, "plain", 5_000_000, "NAO, de todo"));
        changes.push(text(ana, "unrelated", 6_000_000, "nothing to see"));
        store.apply(Lane::Background, messages(changes)).await.unwrap();

        let first = query(&store, |db| read::search(db, "nao", None, None, 10)).await;
        let ids: Vec<&str> = first.hits.iter().map(|hit| hit.message.id.as_str()).collect();
        assert_eq!(ids[..3], ["plain", "m24", "m23"], "newest first");
        let hit = &first.hits[1];
        assert_eq!(&hit.text[hit.marks[0].0..hit.marks[0].1], "não");
        let mut seen: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let mut cursor = first.cursor;
        while let Some(before) = cursor {
            let page = query(&store, move |db| read::search(db, "nao", None, Some(before), 10)).await;
            seen.extend(page.hits.iter().map(|hit| hit.message.id.clone()));
            cursor = page.cursor;
        }
        assert_eq!(seen.len(), 26, "every match once: {seen:?}");
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 26);

        let in_rui = query(&store, move |db| read::search(db, "NÃO sei", Some(rui), None, 50)).await;
        assert_eq!(in_rui.hits.len(), 12);
        assert!(in_rui.hits.iter().all(|hit| hit.message.chat == rui));
        let typing = query(&store, |db| read::search(db, "ai", None, None, 50)).await;
        assert_eq!(typing.hits.len(), 25, "a word's start finds it as it's typed");
        let quoted = query(&store, |db| read::search(db, "\"não OR", None, None, 50)).await;
        assert!(quoted.hits.is_empty(), "FTS5's syntax in what's typed is only words");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_change_writes_its_event_and_its_version_goes_up() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "120363000000000001@g.us";
        let created = store
            .apply(
                Lane::Live,
                vec![Change::Chat(Chat { id: chat.into(), name: Some("Climbing".into()), ..Chat::default() })],
            )
            .await
            .unwrap();
        let mut hello = text(chat, "m1", 1_000, "hello");
        hello.sender = Some("351910000001:3@s.whatsapp.net".into());
        let sent = store.apply(Lane::Live, messages([hello.clone()])).await.unwrap();
        let events = query(&store, move |db| read::events(db, created.seq, 100)).await;
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["person", "message", "chat"], "the sender, the message and its chat's newest");
        assert_eq!(sent.seq, events.last().unwrap().seq);

        let rows = query(&store, move |db| read::messages(db, chat, Around::Newest, 10)).await;
        assert_eq!(rows[0].v, events[1].seq, "a row's version is the seq of its last change");
        let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
        assert_eq!((chats[0].v, chats[0].name.as_str()), (events[2].seq, "Climbing"));
        assert_eq!(chats[0].last.as_ref().unwrap().from.as_deref(), Some("+351910000001"));

        let again = store.apply(Lane::Live, messages([hello.clone()])).await.unwrap();
        assert_eq!((again.changed, again.seq), (0, sent.seq), "the same message again changes nothing");

        hello.status = Some("read");
        hello.mine = true;
        let read = store.apply(Lane::Live, messages([hello])).await.unwrap();
        let after = query(&store, move |db| read::messages(db, chat, Around::Newest, 10)).await;
        assert!(after[0].v > rows[0].v);
        assert_eq!(after[0].v, read.seq - 1, "the message, then its chat, whose newest it is");
        let mut earlier = text(chat, "m0", 500, "earlier");
        earlier.sender = Some("351910000001@s.whatsapp.net".into());
        let older = store.apply(Lane::Live, messages([earlier])).await.unwrap();
        let events = query(&store, move |db| read::events(db, read.seq, 100)).await;
        assert_eq!(events.len(), 1, "older history doesn't move its chat: {events:?}");
        assert_eq!((events[0].seq, events[0].kind.as_str()), (older.seq, "message"));
        let chats = query(&store, |db| read::chats(db, false, None, 10)).await;
        assert_eq!(chats[0].last.as_ref().unwrap().summary, "hello");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_chat_list_has_pinned_chats_first_then_the_newest_and_archived_ones_apart() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = |n: usize| format!("3519100000{n:02}@s.whatsapp.net");
        let mut changes: Vec<Change> =
            (0..30).map(|n| Change::Message(text(&chat(n), "m", 1_000 * n as i64, "hi"))).collect();
        changes.push(Change::Chat(Chat { id: chat(3), pinned: Some(99), ..Chat::default() }));
        changes.push(Change::Chat(Chat { id: chat(29), archived: Some(true), ..Chat::default() }));
        store.apply(Lane::Background, changes).await.unwrap();
        let first = query(&store, |db| read::chats(db, false, None, 10)).await;
        let ids: Vec<&str> = first.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids[..3], [chat(3), chat(28), chat(27)]);
        assert!(first[0].pinned && first[0].sort > first[1].sort && first[1].sort > first[2].sort);
        let after = first.last().unwrap().cursor();
        let second = query(&store, move |db| read::chats(db, false, Some(&after), 100)).await;
        assert_eq!(second.len(), 19);
        assert_eq!(second[0].id, chat(19));
        let archived = query(&store, |db| read::chats(db, true, None, 10)).await;
        assert_eq!(archived.iter().map(|row| row.id.clone()).collect::<Vec<_>>(), [chat(29)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_lid_and_a_phone_number_join_into_one_person() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let group = "120363000000000001@g.us";
        let mut from_lid = text(group, "m1", 1_000, "olá");
        from_lid.sender = Some("100000000001@lid".into());
        let contact =
            Person { jids: vec!["351910000001@s.whatsapp.net".into()], name: Some("Ana".into()), ..Person::default() };
        let before = store.apply(Lane::Live, vec![Change::Person(contact), Change::Message(from_lid)]).await.unwrap();
        assert_eq!(store.count("people").unwrap(), 2);
        let join = Person {
            jids: vec!["100000000001:2@lid".into(), "351910000001@s.whatsapp.net".into()],
            push_name: Some("Ana R".into()),
            ..Person::default()
        };
        store.apply(Lane::Live, vec![Change::Person(join)]).await.unwrap();
        assert_eq!(store.count("people").unwrap(), 1);
        let rows = query(&store, move |db| read::messages(db, group, Around::Newest, 10)).await;
        assert_eq!(rows[0].from.as_ref().unwrap().name, "Ana", "the contact's name, on what the LID sent");
        let events = query(&store, move |db| read::events(db, before.seq, 10)).await;
        assert_eq!(events.iter().filter(|e| e.kind == "person").count(), 2, "the person kept and the one gone");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_changes_go_ahead_of_a_background_batch() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let history: Vec<Message> =
            (0..20_000).map(|n| text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")).collect();
        let batch = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Background, messages(history)).await.unwrap() })
        };
        while store.count("messages").unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        store.apply(Lane::Live, messages([text("351910000002@s.whatsapp.net", "now", 5, "live")])).await.unwrap();
        assert!(!batch.is_finished(), "the live change waited for the whole background batch");
        let applied = batch.await.unwrap();
        assert_eq!(applied.changed, 20_000);
        assert!(applied.transactions >= 20, "{} transactions", applied.transactions);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_comes_back_by_its_id_with_its_raw_protobuf() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000001@s.whatsapp.net";
        let mut message = text(chat, "3EB0AA", 1_000, "*hi*");
        message.spans = Some(r#"[{"t":"bold","s":"hi"}]"#.into());
        message.raw = Some(Packed::new(b"a raw protobuf"));
        store.apply(Lane::Live, messages([message, text(chat, "3EB0BB", 2_000, "plain")])).await.unwrap();
        let (row, raw) = query(&store, move |db| read::message(db, chat, "3EB0AA")).await.unwrap();
        assert_eq!((row.id.as_str(), raw.as_deref()), ("3EB0AA", Some(&b"a raw protobuf"[..])));
        assert_eq!(row.spans.get(), r#"[{"t":"bold","s":"hi"}]"#);
        let (plain, raw) = query(&store, move |db| read::message(db, chat, "3EB0BB")).await.unwrap();
        assert_eq!((plain.spans.get(), raw), (r#"[{"t":"text","s":"plain"}]"#, None));
        assert!(query(&store, move |db| read::message(db, chat, "nope")).await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_database_from_before_message_counts_has_them_counted_and_then_kept() {
        let home = tempfile::tempdir().unwrap();
        let chat = "351910000001@s.whatsapp.net";
        {
            let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
            db.pragma_update(None, "journal_mode", "WAL").unwrap();
            let tx = db.transaction().unwrap();
            for migration in &schema::MIGRATIONS[..2] {
                tx.execute_batch(migration).unwrap();
            }
            tx.execute("INSERT INTO chats (n, id, kind, v) VALUES (1, ?1, 'direct', 1)", [chat]).unwrap();
            for n in 0..3 {
                tx.execute(
                    "INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, ?1, ?2, ?2, 'text', 1)",
                    params![format!("m{n}"), n],
                )
                .unwrap();
            }
            tx.pragma_update(None, "user_version", 2).unwrap();
            tx.commit().unwrap();
        }
        let store = Store::open(home.path()).unwrap();
        assert_eq!(query(&store, move |db| read::message_count(db, chat)).await, 3);
        store.apply(Lane::Live, messages([text(chat, "m3", 10, "one more")])).await.unwrap();
        store.apply(Lane::Live, messages([text(chat, "m3", 10, "one more, edited")])).await.unwrap();
        store.apply(Lane::Live, vec![Change::DeleteMessage { chat: chat.into(), id: "m0".into() }]).await.unwrap();
        assert_eq!(query(&store, move |db| read::message_count(db, chat)).await, 3, "one came, one changed, one went");
        assert_eq!(query(&store, |db| read::message_count(db, "nobody@s.whatsapp.net")).await, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_deleted_message_leaves_its_event_and_its_chat_shows_the_one_before_in_its_place() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000001@s.whatsapp.net";
        let mut committed = store.committed();
        let written = store
            .apply(Lane::Live, messages([text(chat, "m1", 1_000, "first"), text(chat, "m2", 2_000, "second")]))
            .await
            .unwrap();
        assert_eq!(*committed.borrow_and_update(), written.seq, "the feed hears of each commit");
        let before = query(&store, |db| read::chats(db, false, None, 10)).await.remove(0);
        let delete = |id: &str| Change::DeleteMessage { chat: chat.into(), id: id.into() };
        let deleted = store.apply(Lane::Live, vec![delete("m2")]).await.unwrap();
        assert_eq!(deleted.changed, 1);
        assert_eq!(*committed.borrow_and_update(), deleted.seq);
        let events = query(&store, move |db| read::events(db, written.seq, 10)).await;
        let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["message", "chat"], "the message gone, and its chat's newest");
        let rows = query(&store, move |db| read::messages(db, chat, Around::Newest, 10)).await;
        assert_eq!(rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["m1"]);
        let after = query(&store, |db| read::chats(db, false, None, 10)).await.remove(0);
        assert_eq!(after.last.as_ref().unwrap().summary, "first");
        assert_eq!(after.sort, before.sort, "deleting doesn't move the chat");
        assert!(after.v > before.v);
        let again = store.apply(Lane::Live, vec![delete("m2")]).await.unwrap();
        assert_eq!((again.changed, again.seq), (0, deleted.seq), "a message already gone changes nothing");
        assert!(!committed.has_changed().unwrap(), "nor wakes the feed");
        let quiet = store.count("events").unwrap();
        store.apply(Lane::Live, vec![delete("m1")]).await.unwrap();
        assert!(query(&store, |db| read::chats(db, false, None, 10)).await[0].last.is_none());
        assert_eq!(store.count("events").unwrap(), quiet + 2);
    }
}
