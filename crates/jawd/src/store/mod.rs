//! jaw.db, through rusqlite with SQLite compiled in. One writer thread owns the write connection
//! and takes the inbox first: a message is committed, then acked, before WhatsApp hears it
//! arrived. Live changes (conversions as messages arrive, and actions) come next, each in a
//! transaction of its own. Background changes and history go in transactions of at most 1,000
//! rows or 3 ms with their commit, ending early as soon as an inbox or live write is waiting, which
//! is taken before the next. Text written in the background is indexed for search afterwards, in
//! steps as short, while nothing else is waiting. Queries go to the read pool, so a page never waits
//! behind a sync.

pub mod bench;
#[cfg(test)]
mod plans;
pub mod read;
#[cfg(test)]
mod rules;
mod schema;
mod write;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, bounded, select_biased, unbounded};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{Notify, oneshot, watch};

pub use self::read::Reads;
use self::write::Stamps;
pub use self::write::{
    Amend, Amendment, Change, Chat, Drained, Members, Message, Opened, Packed, Person, Read, frecency,
};
#[cfg(test)]
pub use self::write::{unpack, unpack_for_tests};
use crate::engine::Ack;
use crate::work::{Report, Samples};

/// How a batch of changes went into the database.
#[derive(Clone, Copy, Debug, Default)]
pub struct Applied {
    /// The changes that changed something; the rest matched what was there.
    pub changed: usize,
    /// The groups of changes that failed and were kept in quarantine instead.
    pub quarantined: usize,
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
    Inbox { kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack>, queued: Instant },
}

type Done = oneshot::Sender<Result<Applied, String>>;

struct Live {
    changes: Vec<Change>,
    done: Done,
    queued: Instant,
    /// Whether its commit must reach the disk before it's said to be done.
    durable: bool,
}

/// What one transaction made of its changes.
#[derive(Default)]
struct Made {
    changed: usize,
    quarantined: usize,
    taken: usize,
}

/// What the writer has done lately, for `status`.
#[derive(Default)]
struct Stats {
    transactions: u64,
    /// How long inbox and live writes waited for the writer.
    waits: Samples,
    /// How long each transaction took, commit included.
    runs: Samples,
}

enum Background {
    Changes {
        changes: Vec<Change>,
        done: Done,
    },
    /// Indexes everything waiting for search, then answers.
    Index {
        done: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    realtime: Sender<Realtime>,
    live: Sender<Live>,
    background: Sender<Background>,
    reads: Reads,
    committed: watch::Receiver<i64>,
    stats: Arc<Mutex<Stats>>,
    inboxed: Arc<Notify>,
}

const BATCH_ROWS: usize = 1_000;
/// What a background transaction may take, its commit included, since a live write waits for it.
const BATCH_TIME: Duration = Duration::from_millis(3);
/// The pause between two steps of indexing for search. A writer that never pauses looks like a
/// batch job to the OS, which then lets other threads ahead of it when a live write comes.
const INDEX_PAUSE: Duration = Duration::from_millis(1);
/// The pages of FTS5 merging a step does, about a millisecond's worth.
const MERGE_PAGES: i64 = 64;
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
        let backlog = db.query_row("SELECT EXISTS (SELECT 1 FROM fts_backlog)", [], |r| r.get(0))?;
        let stats = Arc::new(Mutex::new(Stats::default()));
        let inboxed = Arc::new(Notify::new());
        let writer = Writer {
            stats: stats.clone(),
            inboxed: inboxed.clone(),
            db,
            stamps,
            realtime: realtime_rx,
            live: live_rx,
            commits: commits.clone(),
            seq,
            pruned: Instant::now(),
            changes_pace: Pace::default(),
            index_pace: Pace::default(),
            backlog,
            unmerged: true,
            indexed: Vec::new(),
        };
        std::thread::Builder::new().name("jawd-writer".into()).spawn(move || writer.run(background_rx))?;
        std::thread::Builder::new().name("jawd-checkpoint".into()).spawn(move || checkpoint(checkpointer, commits))?;
        Ok(Store { path, realtime, live, background, reads, committed, stats, inboxed })
    }

    /// The feed's last committed seq, which changes as each transaction with events commits.
    pub fn committed(&self) -> watch::Receiver<i64> {
        self.committed.clone()
    }

    /// What's woken each time an engine event is committed to the inbox.
    pub fn inboxed(&self) -> Arc<Notify> {
        self.inboxed.clone()
    }

    /// Keeps an engine event, then sends its ack. It never waits: realtime writes aren't bounded.
    pub fn inbox(&self, kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack>) {
        let _ = self.realtime.send(Realtime::Inbox { kind, at_ms, payload, ack, queued: Instant::now() });
    }

    /// Makes `changes` in `lane`, each with its event, and says how it went once they're committed.
    /// A change that fails rolls back its transaction; on the background lane, earlier
    /// transactions of the batch stay committed.
    pub async fn apply(&self, lane: Lane, changes: Vec<Change>) -> Result<Applied> {
        let (done, applied) = oneshot::channel();
        match lane {
            Lane::Live => self
                .live
                .send(Live { changes, done, queued: Instant::now(), durable: false })
                .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?,
            Lane::Background => self.queue_background(Background::Changes { changes, done }).await?,
        }
        applied.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// Makes `changes` in the live lane in one transaction that reaches the disk before it's said
    /// to be done, for what must survive a power cut, such as the commit after which a spool file
    /// can go.
    pub async fn apply_durable(&self, changes: Vec<Change>) -> Result<Applied> {
        let (done, applied) = oneshot::channel();
        self.live
            .send(Live { changes, done, queued: Instant::now(), durable: true })
            .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?;
        applied.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    async fn queue_background(&self, work: Background) -> Result<()> {
        let background = self.background.clone();
        tokio::task::spawn_blocking(move || background.send(work))
            .await?
            .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))
    }

    /// Waits until every message written so far can be found by search.
    pub async fn indexed(&self) -> Result<()> {
        let (done, indexed) = oneshot::channel();
        self.queue_background(Background::Index { done }).await?;
        indexed.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// The read pool.
    pub fn reads(&self) -> &Reads {
        &self.reads
    }

    /// What sync remembered under `key`, read on a connection of its own.
    pub fn remembered(&self, key: &str) -> Result<Option<String>> {
        let db = Connection::open(&self.path)?;
        Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    /// How many rows `table` holds, read on a connection of its own so the writer doesn't wait.
    pub fn count(&self, table: &str) -> Result<u64> {
        let db = Connection::open(&self.path)?;
        let count: i64 = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))?;
        Ok(count.max(0) as u64)
    }
}

impl Report for Store {
    fn report(&self) -> jaw_proto::Pool {
        let stats = self.stats.lock().expect("the writer's stats");
        jaw_proto::Pool {
            name: "writer".into(),
            workers: 1,
            running: 0,
            queued_interactive: (self.realtime.len() + self.live.len()) as u32,
            queued_background: self.background.len() as u32,
            done: stats.transactions,
            wait_interactive: stats.waits.percentiles(),
            wait_background: None,
            run: stats.runs.percentiles(),
        }
    }
}

/// How often the checkpointer copies the WAL into the database, when anything was committed.
const CHECKPOINT_EVERY: Duration = Duration::from_secs(1);

/// How often the writer removes events older than the feed keeps, when it has background work.
const PRUNE_EVERY: Duration = Duration::from_secs(3600);

struct Writer {
    stats: Arc<Mutex<Stats>>,
    inboxed: Arc<Notify>,
    db: Connection,
    stamps: Stamps,
    realtime: Receiver<Realtime>,
    live: Receiver<Live>,
    commits: Arc<AtomicU64>,
    /// The feed's last committed seq, for whoever follows the feed.
    seq: watch::Sender<i64>,
    pruned: Instant,
    changes_pace: Pace,
    index_pace: Pace,
    /// Whether fts_backlog may hold messages to index.
    backlog: bool,
    /// Whether FTS5 may have segments to merge.
    unmerged: bool,
    /// Who waits for everything written to be indexed.
    indexed: Vec<oneshot::Sender<Result<(), String>>>,
}

/// What a kind of background transaction's commit has lately cost for each row it took. SQLite
/// writes every page a transaction dirtied at commit, and history's rows land all over the
/// indexes, so a commit can cost more than the work before it. The next transaction stops in time
/// for its commit to fit in BATCH_TIME too.
#[derive(Clone, Copy, Default)]
struct Pace {
    commit_per_row: Duration,
    per_row: Duration,
}

impl Pace {
    fn enough(&self, body: Duration, rows: usize) -> bool {
        body + self.commit_per_row * rows as u32 >= BATCH_TIME
    }

    /// How many rows a transaction can take in BATCH_TIME, as rows have lately cost.
    fn rows(&self) -> usize {
        if self.per_row.is_zero() {
            return 100;
        }
        (BATCH_TIME.as_nanos() / self.per_row.as_nanos()).clamp(1, BATCH_ROWS as u128) as usize
    }

    fn learn(&mut self, body: Duration, commit: Duration, rows: usize) {
        if rows > 0 {
            self.commit_per_row = (self.commit_per_row * 3 + commit / rows as u32) / 4;
            self.per_row = (self.per_row * 3 + (body + commit) / rows as u32) / 4;
        }
    }
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
            let idle = if self.backlog || self.unmerged { INDEX_PAUSE } else { Duration::from_secs(3600) };
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
                    Ok(Background::Changes { changes, done }) => {
                        let applied = self.background_changes(&changes);
                        let _ = done.send(applied.map_err(|e| format!("{e:#}")));
                        if self.pruned.elapsed() >= PRUNE_EVERY {
                            self.prune();
                        }
                    }
                    Ok(Background::Index { done }) => {
                        if self.backlog || self.unmerged {
                            self.indexed.push(done);
                        } else {
                            let _ = done.send(Ok(()));
                        }
                    }
                    Err(_) => return,
                },
                // Merging first keeps FTS5's levels short of a merge it would do all at once.
                default(idle) => {
                    let step = if self.unmerged { self.merge() } else if self.backlog { self.index() } else { Ok(()) };
                    if let Err(e) = &step {
                        eprintln!("jawd: can't index messages for search: {e:#}");
                        (self.backlog, self.unmerged) = (false, false);
                    }
                    if !self.backlog && !self.unmerged {
                        for done in self.indexed.drain(..) {
                            let _ = done.send(step.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")));
                        }
                    }
                }
            }
        }
    }

    fn realtime_write(&mut self, write: Realtime) {
        let Realtime::Inbox { queued, .. } = &write;
        let began = Instant::now();
        let waited = began - *queued;
        if realtime_write(&mut self.db, write) {
            self.inboxed.notify_one();
        }
        self.commits.fetch_add(1, Ordering::Relaxed);
        self.count(Some(waited), began.elapsed());
    }

    fn count(&self, waited: Option<Duration>, ran: Duration) {
        let mut stats = self.stats.lock().expect("the writer's stats");
        stats.transactions += 1;
        if let Some(waited) = waited {
            stats.waits.add(waited);
        }
        stats.runs.add(ran);
    }

    /// Merges MERGE_PAGES of FTS5's segments, if any level has enough to merge.
    fn merge(&mut self) -> Result<()> {
        let began = Instant::now();
        let before = self.db.total_changes();
        self.db.execute("INSERT INTO messages_fts (messages_fts, rank) VALUES ('merge', ?1)", [MERGE_PAGES])?;
        self.unmerged = self.db.total_changes() > before + 1;
        self.commits.fetch_add(1, Ordering::Relaxed);
        self.count(None, began.elapsed());
        Ok(())
    }

    /// Indexes a short transaction's worth of fts_backlog for search.
    fn index(&mut self) -> Result<()> {
        let began = Instant::now();
        let tx = self.db.transaction()?;
        let (realtime, live, pace) = (&self.realtime, &self.live, self.index_pace);
        let (taken, more) = write::index(&tx, pace.rows(), |taken| {
            !realtime.is_empty() || !live.is_empty() || pace.enough(began.elapsed(), taken)
        })?;
        let body = began.elapsed();
        tx.commit()?;
        self.index_pace.learn(body, began.elapsed() - body, taken);
        self.unmerged = true;
        self.count(None, began.elapsed());
        self.commits.fetch_add(1, Ordering::Relaxed);
        self.backlog = more;
        Ok(())
    }

    /// Makes the writer's commits reach the disk before they return, or go back to letting the
    /// checkpointer's syncs do it. On macOS only F_FULLFSYNC gets past the drive's cache.
    fn durable(&mut self, durable: bool) {
        let (synchronous, fullfsync) = if durable { ("FULL", "ON") } else { ("NORMAL", "OFF") };
        if let Err(e) = self
            .db
            .pragma_update(None, "synchronous", synchronous)
            .and_then(|_| self.db.pragma_update(None, "fullfsync", fullfsync))
        {
            eprintln!("jawd: can't set synchronous={synchronous}: {e}");
        }
    }

    fn live_write(&mut self, Live { changes, done, queued, durable }: Live) {
        let started = Instant::now();
        self.unmerged = true;
        let waited = started - queued;
        let groups = write::groups(&changes);
        if durable {
            self.durable(true);
        }
        let applied = self.transaction(&changes, &groups, false).map(|made| Applied {
            changed: made.changed,
            quarantined: made.quarantined,
            transactions: 1,
            took: started.elapsed(),
            seq: self.stamps.last(),
        });
        if durable {
            self.durable(false);
        }
        self.count(Some(waited), started.elapsed());
        let _ = done.send(applied.map_err(|e| format!("{e:#}")));
    }

    /// Makes the groups of `changes` in one transaction, all of them for a live one; a background
    /// one stops between groups at BATCH_ROWS, at BATCH_TIME with its commit, or as soon as an inbox
    /// or live write is waiting. Each group runs in a savepoint: one that fails is rolled back and
    /// kept in quarantine with its error, and the rest go on. Says how many changes were taken.
    fn transaction(&mut self, changes: &[Change], groups: &[Range<usize>], background: bool) -> Result<Made> {
        let began = Instant::now();
        self.stamps.begin();
        let start = groups.first().map_or(0, |g| g.start);
        let made = (|| {
            let mut tx = self.db.transaction()?;
            let mut made = Made::default();
            for group in groups {
                let mark = self.stamps.mark();
                let mut savepoint = tx.savepoint()?;
                let mut failed = None;
                let mut changed = 0;
                for change in &changes[group.clone()] {
                    match write::apply(&savepoint, &mut self.stamps, change, background) {
                        Ok(c) => changed += usize::from(c),
                        Err(e) if write::transient(&e) => return Err(e),
                        Err(e) => {
                            failed = Some(e);
                            break;
                        }
                    }
                }
                match failed {
                    None => {
                        savepoint.commit()?;
                        made.changed += changed;
                    }
                    Some(e) => {
                        savepoint.rollback()?;
                        drop(savepoint);
                        self.stamps.undo(mark);
                        write::quarantine(&tx, &changes[group.clone()], &e)?;
                        made.quarantined += 1;
                    }
                }
                made.taken = group.end - start;
                if background
                    && (made.taken >= BATCH_ROWS
                        || !self.realtime.is_empty()
                        || !self.live.is_empty()
                        || self.changes_pace.enough(began.elapsed(), made.taken))
                {
                    break;
                }
            }
            write::settle(&tx, &mut self.stamps)?;
            let body = began.elapsed();
            tx.commit()?;
            if background {
                self.changes_pace.learn(body, began.elapsed() - body, made.taken);
                self.count(None, began.elapsed());
            }
            Ok(made)
        })();
        match &made {
            Ok(_) => {
                self.commits.fetch_add(1, Ordering::Relaxed);
                let last = self.stamps.last();
                self.seq.send_if_modified(|seq| std::mem::replace(seq, last) != last);
                self.backlog |= background;
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
        let groups = write::groups(changes);
        let mut rest = groups.as_slice();
        while !rest.is_empty() {
            let made = self.transaction(changes, rest, true)?;
            applied.changed += made.changed;
            applied.quarantined += made.quarantined;
            applied.transactions += 1;
            let first = rest[0].start;
            rest = &rest[rest.iter().take_while(|g| g.end <= first + made.taken).count()..];
            applied.seq = self.stamps.last();
            self.between();
        }
        applied.took = started.elapsed();
        Ok(applied)
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

/// Makes a realtime write, saying whether it was committed.
fn realtime_write(db: &mut Connection, write: Realtime) -> bool {
    match write {
        Realtime::Inbox { kind, at_ms, payload, ack, .. } => {
            let stored =
                db.execute("INSERT INTO inbox (kind, at, payload) VALUES (?1, ?2, ?3)", params![kind, at_ms, payload]);
            match stored {
                Ok(_) => {
                    if let Some(ack) = ack {
                        ack.send();
                    }
                    true
                }
                // Without its ack, the engine keeps waiting and WhatsApp keeps the message.
                Err(e) => {
                    eprintln!("jawd: can't keep a {kind} in the inbox, so it isn't acked: {e}");
                    false
                }
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
        store.indexed().await.unwrap();

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

    fn inbox_rows(store: &Store, n: usize) -> impl std::future::Future<Output = ()> + '_ {
        for _ in 0..n {
            store.inbox("message", 0, vec![1], None);
        }
        async move {
            while store.count("inbox").unwrap() < n as u64 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_conversion_that_fails_is_quarantined_with_its_row_kept_and_the_rest_go_on() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        inbox_rows(&store, 3).await;
        let chat = "351910000001@s.whatsapp.net";
        let batch = vec![
            Change::Message(text(chat, "a", 1_000, "first")),
            Change::Drained(Drained::Inbox(1)),
            Change::Message(text(chat, "b", 2_000, "second")),
            Change::Person(Person::default()),
            Change::Drained(Drained::Inbox(2)),
            Change::Message(text(chat, "c", 3_000, "third")),
            Change::Drained(Drained::Inbox(3)),
        ];
        for lane in [Lane::Live, Lane::Background] {
            let applied = store.apply(lane, batch.clone()).await.unwrap();
            assert_eq!(applied.quarantined, 1, "in the {lane:?} lane");
        }
        let ids: Vec<String> = query(&store, move |db| read::messages(db, chat, Around::Newest, 10))
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["a", "c"], "the failed row's message rolled back with it, and the rows after it went in");
        let left: Vec<i64> = query(&store, |db| {
            Ok(db.prepare("SELECT seq FROM inbox")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
        })
        .await;
        assert_eq!(left, [2], "the row that didn't convert stays");
        let kept: (String, String, String) = query(&store, |db| {
            Ok(db.query_row("SELECT source, ref, error FROM quarantine", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?)
        })
        .await;
        assert_eq!((kept.0.as_str(), kept.1.as_str()), ("inbox", "2"));
        assert!(kept.2.contains("needs at least one JID"), "{}", kept.2);
        let events: Vec<i64> = query(&store, |db| read::events(db, 0, 100)).await.into_iter().map(|e| e.seq).collect();
        assert!(events.windows(2).all(|w| w[1] == w[0] + 1), "no seq is skipped or given twice: {events:?}");
        store.apply(Lane::Live, messages([text(chat, "d", 4_000, "fourth")])).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_inboxs_seq_never_goes_back_once_it_has_drained() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        inbox_rows(&store, 3).await;
        store.apply(Lane::Live, (1..=3).map(|seq| Change::Drained(Drained::Inbox(seq))).collect()).await.unwrap();
        assert_eq!(store.count("inbox").unwrap(), 0);
        store.inbox("message", 0, vec![1], None);
        while store.count("inbox").unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let seq: i64 = query(&store, |db| Ok(db.query_row("SELECT seq FROM inbox", [], |r| r.get(0))?)).await;
        assert_eq!(seq, 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_database_at_version_4_keeps_its_inbox_and_its_largest_seq() {
        let home = tempfile::tempdir().unwrap();
        {
            let mut db = Connection::open(home.path().join("jaw.db")).unwrap();
            let tx = db.transaction().unwrap();
            for migration in &schema::MIGRATIONS[..4] {
                tx.execute_batch(migration).unwrap();
            }
            for kind in ["a", "b", "c"] {
                tx.execute("INSERT INTO inbox (kind, at, payload) VALUES (?1, 0, x'01')", [kind]).unwrap();
            }
            tx.execute("DELETE FROM inbox WHERE seq < 3", []).unwrap();
            tx.pragma_update(None, "user_version", 4).unwrap();
            tx.commit().unwrap();
        }
        let store = Store::open(home.path()).unwrap();
        let kept: (i64, String, i64) = query(&store, |db| {
            Ok(db.query_row(
                "SELECT i.seq, i.kind, s.seq FROM inbox i, sqlite_sequence s WHERE s.name = 'inbox'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .await;
        assert_eq!(kept, (3, "c".to_string(), 3));
        store.apply(Lane::Live, vec![Change::Drained(Drained::Inbox(3))]).await.unwrap();
        store.inbox("message", 0, vec![1], None);
        while store.count("inbox").unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let seq: i64 = query(&store, |db| Ok(db.query_row("SELECT seq FROM inbox", [], |r| r.get(0))?)).await;
        assert_eq!(seq, 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_deleted_for_me_from_the_phone_goes_as_one_deleted_here_does() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000001@s.whatsapp.net";
        store
            .apply(Lane::Live, messages([text(chat, "m1", 1_000, "first"), text(chat, "m2", 2_000, "second")]))
            .await
            .unwrap();
        let before = query(&store, |db| read::chats(db, false, None, 10)).await.remove(0);
        let delete = |id: &str| {
            Change::Amend(Amend {
                chat: chat.into(),
                target: id.into(),
                sender: None,
                mine: true,
                at: 3_000,
                what: Amendment::DeleteForMe,
                raw: None,
            })
        };
        assert_eq!(store.apply(Lane::Live, vec![delete("m2")]).await.unwrap().changed, 1);
        assert_eq!(query(&store, move |db| read::message_count(db, chat)).await, 1);
        let after = query(&store, |db| read::chats(db, false, None, 10)).await.remove(0);
        assert_eq!(after.last.as_ref().unwrap().summary, "first");
        assert_eq!(after.sort, before.sort, "deleting doesn't move the chat");
        store.apply(Lane::Live, vec![delete("m2")]).await.unwrap();
        assert_eq!(store.count("pending").unwrap(), 0, "a message deleted for me leaves a tombstone, not a wait");
        store.apply(Lane::Live, vec![delete("m9")]).await.unwrap();
        store
            .apply(Lane::Live, messages([text(chat, "m2", 2_000, "second"), text(chat, "m9", 9_000, "later")]))
            .await
            .unwrap();
        assert_eq!(query(&store, move |db| read::message_count(db, chat)).await, 1, "neither comes back");
    }
}
