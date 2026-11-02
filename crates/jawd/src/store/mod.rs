//! jaw.db, through rusqlite with SQLite compiled in. One writer thread owns the write connection
//! and takes the inbox first: what the engine hands over is committed and synced to the disk, and
//! only then acked, before WhatsApp hears it arrived. Live changes (conversions as messages arrive,
//! and actions) come next, each in a transaction of its own, or in the next inbox group's synced
//! commit while rows keep coming, so a stream of them never holds a live change back. Background
//! changes and history go in transactions of at most 1,000 rows or 3 ms with their commit, ending
//! early, once they've had 1 ms, when an inbox row or a live write is waiting, which is taken
//! before the next, so history's small chats go several to a transaction while a catch-up's rows
//! keep coming. Every message's text, live or history, is
//! indexed for search afterwards, in steps as short, while nothing else is waiting, so FTS5's work
//! at commit never lands in one an ack or a live write waits on. Queries go to the read pool, so a
//! page never waits behind a sync.
//!
//! Every write transaction begins IMMEDIATE. One that fails for a reason that passes (a full disk,
//! an I/O error, a lock) is tried again with back-off, never dropped, and while the inbox can't
//! commit its rows are kept and nothing else is written. The checkpointer copies the WAL into the
//! database while frames wait, background work waits for it once the WAL passes WAL_SOFT, and past
//! WAL_HARD the writer starts the WAL again itself between inbox groups and live writes, so the WAL
//! stays under 64 MiB however fast history or messages come.

pub mod bench;
pub mod files;
pub mod library;
pub mod lists;
mod overlay;
#[cfg(test)]
mod plans;
pub mod read;
#[cfg(test)]
mod rules;
mod schema;
pub mod thumbnails;
mod vfs;
mod write;

use std::cell::Cell;
use std::ffi::c_int;
use std::fmt::Display;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, select_biased, unbounded};
use rusqlite::hooks::Wal as WalFile;
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, ffi, params};
use tokio::sync::{Notify, oneshot, watch};

pub use self::read::Reads;
#[cfg(test)]
pub(crate) use self::tests::index_against_text;
pub use self::thumbnails::Thumbnails;
use self::write::Stamps;
pub use self::write::chat_actions;
pub use self::write::folders;
pub use self::write::layout;
pub use self::write::unpack;
#[cfg(test)]
pub use self::write::unpack_for_tests;
pub use self::write::{
    Amend, Amendment, Change, Chat, Drained, Members, Merging, Message, Opened, Packed, Person, Read, frecency,
    merge_reference,
};
pub use self::write::{contacts, copies, expiry, kinds, links, media, outbox, outgoing, polls};
use crate::engine::Ack;
use crate::work::{Report, Samples};

/// jaw.db's migrations, for tests that build a database at an earlier version.
#[cfg(test)]
pub fn schema_for_tests() -> &'static [&'static str] {
    schema::MIGRATIONS
}

/// Brings a database up to date, as opening a store does.
#[cfg(test)]
pub fn migrate_for_tests(db: &mut Connection) -> Result<()> {
    schema::migrate(db)
}

/// The steps a test takes of a backfill, or of the merges, until they're done: at most one more than
/// the rows they have left to look at or to move, since a step that doesn't fail takes one at least,
/// or ends what it's for. A busy machine, which takes longer over each, never fails the test, and a
/// step that fails each time, rolled back and kept in quarantine and so never moving on, fails it
/// once they've all gone, where the test would otherwise take it again for good.
#[cfg(test)]
pub struct Steps {
    most: u64,
    taken: u64,
}

#[cfg(test)]
impl Steps {
    /// One more step, failing with `never` once as many have gone as can be needed.
    pub fn take(&mut self, never: &str) {
        assert!(self.taken < self.most, "{never}: {} steps, and still not done", self.taken);
        self.taken += 1;
    }
}

#[cfg(test)]
impl Store {
    /// The steps backfill `name` can need from here.
    pub fn backfill_steps(&self, name: &str) -> Steps {
        Steps { most: self.backfill_left(name).unwrap() + 1, taken: 0 }
    }

    /// The steps the merges waiting can need from here: each moves a message of a chat being merged
    /// away, or ends its merge.
    pub fn merge_steps(&self) -> Steps {
        let moving = self.count("messages WHERE chat IN (SELECT gone FROM chat_merges)").unwrap();
        Steps { most: moving + self.count("chat_merges").unwrap() + 1, taken: 0 }
    }

    /// The steps taking what waits for events can need from here: each takes a row of pending, or
    /// one set aside for an event, or ends what's left for its event.
    pub fn opening_steps(&self) -> Steps {
        let rows =
            self.count("pending").unwrap() + self.count("quarantine WHERE source IN ('answer', 'edit')").unwrap();
        Steps { most: rows + self.count("openings").unwrap() + 1, taken: 0 }
    }
}

/// How a batch of changes went into the database.
#[derive(Clone, Debug, Default)]
pub struct Applied {
    /// The changes that changed something; the rest matched what was there.
    pub changed: usize,
    /// The groups of changes that failed and were kept in quarantine instead.
    pub quarantined: usize,
    /// Why the first of those failed, as the quarantine keeps it.
    pub error: Option<String>,
    pub transactions: usize,
    pub took: Duration,
    /// The feed's last seq once they were committed.
    pub seq: i64,
}

/// The layouts kept: how many interfaces keep one, the bytes of the biggest that isn't a book, how
/// many are books, and the bytes of the biggest book.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutsKept {
    pub kept: u64,
    pub largest: u64,
    pub books: u64,
    pub largest_book: u64,
}

/// Which of the writer's queues a batch of changes joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Messages converted as they arrive, and what someone just did: one transaction, after the
    /// inbox or in an inbox group's, and ahead of any background batch.
    Live,
    /// History and other bulk work: short transactions, waiting for room behind earlier batches.
    Background,
}

/// An engine event to keep; `ack` goes once it's committed and synced.
struct Inbox {
    kind: &'static str,
    at_ms: i64,
    payload: Vec<u8>,
    ack: Option<Ack>,
    queued: Instant,
}

type Done = oneshot::Sender<Result<Applied, String>>;

enum Live {
    Changes {
        changes: Vec<Change>,
        done: Done,
        queued: Instant,
        /// Whether its commit must reach the disk before it's said to be done.
        durable: bool,
    },
    /// Copies the whole WAL into the database and empties it, as at a clean stop.
    Truncate { done: oneshot::Sender<Result<(), String>> },
}

/// A live write made in an inbox group's transaction, answered once that commits.
struct Shared {
    changes: Vec<Change>,
    done: Done,
    queued: Instant,
    durable: bool,
    began: Instant,
    made: Made,
}

/// What one transaction made of its changes.
#[derive(Default)]
struct Made {
    changed: usize,
    quarantined: usize,
    error: Option<String>,
    taken: usize,
}

/// Whether the transaction about to commit on `db` is flushed past the drive's cache before it
/// returns, as the connection itself says: at FULL, through jaw's VFS, with fullfsync off, so
/// SQLite asks its sync at SQLITE_SYNC_NORMAL, which the VFS makes a full flush. A commit is counted
/// synced, and an ack sent, only for what it ran under.
fn commits_synced(db: &Connection) -> Result<bool> {
    let synchronous: i64 = db.prepare_cached("PRAGMA synchronous")?.query_row([], |r| r.get(0))?;
    let fullfsync: i64 = db.prepare_cached("PRAGMA fullfsync")?.query_row([], |r| r.get(0))?;
    Ok(synchronous >= 2 && fullfsync == 0 && vfs::opened_through(db))
}

/// What the writer has done lately, for `status`.
#[derive(Default)]
struct Stats {
    transactions: u64,
    /// Commits synced to the disk, each before the acks or answers that waited on it.
    synced: u64,
    /// How long inbox and live writes waited for the writer.
    waits: Samples,
    /// How long each transaction took, commit included.
    runs: Samples,
    /// How long background work waited for the checkpointer, each time it did, and in all.
    held: Samples,
    held_times: u64,
    held_for: Duration,
    /// Live writes made in an inbox group's commit, and such commits made again without them.
    shared: u64,
    shared_failed: u64,
}

/// The WAL as the writer and the checkpointer last saw it, for each other and for `status`.
struct Wal {
    /// Its size in frames past which background transactions wait for the checkpointer, and past
    /// which they wait until it has started again from its beginning.
    soft: u64,
    hard: u64,
    /// Frames in the WAL after the writer's last commit.
    frames: AtomicU64,
    /// The checkpointer's latest pass: the frames in the WAL then, and how many were in jaw.db.
    log: AtomicU64,
    copied: AtomicU64,
    /// The most frames the WAL has held since jawd started.
    peak: AtomicU64,
    /// Whether background work is waiting for the WAL to start again, so the checkpointer copies
    /// however few frames wait.
    held: AtomicBool,
    /// Whether the writer copies the last few frames itself, so the checkpointer leaves them.
    finishing: AtomicBool,
    /// How often the checkpointer has woken, for tests of an idle jawd, and flushed jaw.db itself,
    /// when its passes were refused the checkpoint lock, and each of the writer's tries to start
    /// the WAL again.
    #[cfg(test)]
    woke: AtomicU64,
    #[cfg(test)]
    flushed: AtomicU64,
    #[cfg(test)]
    refused: Mutex<Vec<Instant>>,
    #[cfg(test)]
    restarts: Mutex<Vec<Restart>>,
}

/// One of the writer's tries to start the WAL again, for tests: when it began and ended, whether it
/// did, whether a reader held it up, whether the WAL was at the edge, how many frames it held, and
/// how many inbox rows were committed and not yet acked as it began.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct Restart {
    began: Instant,
    ended: Instant,
    restarted: bool,
    held_up: bool,
    at_edge: bool,
    frames: u64,
    unacked: usize,
}

impl Wal {
    fn new(soft: u64, hard: u64) -> Wal {
        Wal {
            soft,
            hard,
            frames: AtomicU64::new(0),
            log: AtomicU64::new(0),
            copied: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            held: AtomicBool::new(false),
            finishing: AtomicBool::new(false),
            #[cfg(test)]
            woke: AtomicU64::new(0),
            #[cfg(test)]
            flushed: AtomicU64::new(0),
            #[cfg(test)]
            refused: Mutex::default(),
            #[cfg(test)]
            restarts: Mutex::default(),
        }
    }

    /// Frames committed and not yet copied into jaw.db, as far as is known.
    fn waiting(&self) -> u64 {
        let (frames, log, copied) = (
            self.frames.load(Ordering::Relaxed),
            self.log.load(Ordering::Relaxed),
            self.copied.load(Ordering::Relaxed),
        );
        // A pass from before the WAL started again knows nothing of the frames since.
        if log > frames { frames } else { frames - copied.min(frames) }
    }

    fn passed(&self, (log, copied): (u64, u64)) {
        self.log.store(log, Ordering::Relaxed);
        self.copied.store(copied, Ordering::Relaxed);
    }
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
    realtime: Sender<Inbox>,
    live: Sender<Live>,
    background: Sender<Background>,
    reads: Reads,
    committed: watch::Receiver<i64>,
    copies_taken: watch::Receiver<u64>,
    stats: Arc<Mutex<Stats>>,
    inboxed: Arc<Notify>,
    opening: Arc<Notify>,
    wal: Arc<Wal>,
    trouble: watch::Receiver<Option<String>>,
    thumbnails: Arc<Thumbnails>,
    /// Whether barriers fence jaw.db's checkpoints, rather than full flushes.
    fenced: bool,
    files: Arc<files::Files>,
    soonest: Arc<Soonest>,
    /// How many messages have been erased since jaw.db was opened, disappeared or opened alike.
    erased: Arc<AtomicU64>,
    /// The view-once messages open here erased since sending last looked, by n and ID.
    closed: Arc<Mutex<Vec<(i64, String)>>>,
}

/// The soonest time a message the writer has committed since the timer last looked goes, which
/// wakes the timer when it's sooner than the time it waits for.
#[derive(Default)]
pub struct Soonest {
    at: Mutex<Option<i64>>,
    noted: Notify,
}

impl Soonest {
    fn note(&self, at: i64) {
        let mut soonest = self.at.lock().expect("the soonest expiry");
        *soonest = Some(soonest.map_or(at, |was| was.min(at)));
        drop(soonest);
        self.noted.notify_one();
    }

    /// What's been noted since the last take, and nothing until more is.
    pub fn take(&self) -> Option<i64> {
        self.at.lock().expect("the soonest expiry").take()
    }

    /// Waits until a time is noted, at once if one was since the last wait.
    pub async fn noted(&self) {
        self.noted.notified().await;
    }
}

const BATCH_ROWS: usize = 1_000;
/// What a background transaction may take, its commit included, since a live write waits for it.
const BATCH_TIME: Duration = Duration::from_millis(3);
/// What a background transaction's work has at least before it gives way to an inbox row or a live
/// write waiting, its commit coming after. Giving way after every change, while a catch-up's rows
/// kept coming, left history's small chats one change to a transaction, each writing the same pages
/// again. A step of indexing still gives way at once, since FTS5 does most of its work at commit.
const YIELD_AFTER: Duration = Duration::from_millis(1);
/// The pause between two steps of indexing for search. A writer that never pauses looks like a
/// batch job to the OS, which then lets other threads ahead of it when a live write comes.
const INDEX_PAUSE: Duration = Duration::from_millis(1);
/// How long the writer goes without an inbox group, or a live write converting several inbox rows,
/// before it indexes for search: a catch-up's or a flood's writes come closer together than this,
/// and a step between two of them would put FTS5's work back where they wait. A lone message is
/// found this long after its commit.
const LIVE_PAUSE: Duration = Duration::from_millis(10);
/// While those keep coming closer than LIVE_PAUSE, a step of indexing still goes this often, in a
/// gap between two of them, so search keeps up with a long stream; in a flood, the inbox rows
/// waiting end it after a row.
const STREAM_STEP: Duration = Duration::from_millis(100);
/// The pages of FTS5 merging a step does, about a millisecond's worth.
const MERGE_PAGES: i64 = 64;
/// How long after one whole merge of a contentless index, links_fts or library_fts, began another
/// may, to take out the words of messages erased since: a message's words are erased from one only
/// by a merge, and a merge of the whole of it costs every page it has.
const WHOLE_MERGE_EVERY: Duration = Duration::from_secs(600);
/// A step's worth of merging each contentless index, by its place in `expiry::CONTENTLESS`; with a
/// negative rank, the first step of merging every segment it has into one.
const CONTENTLESS_MERGES: [&str; expiry::CONTENTLESS.len()] = [
    "INSERT INTO links_fts (links_fts, rank) VALUES ('merge', ?1)",
    "INSERT INTO library_fts (library_fts, rank) VALUES ('merge', ?1)",
];
const WRITER_CACHE_KIB: i64 = 32 * 1024;
/// How long any connection waits for a lock before it fails, which the writer then tries again.
const BUSY_MS: i64 = 5_000;
/// The most inbox rows one synced commit takes.
const INBOX_GROUP: usize = 256;
/// How long the writer goes on making the live writes waiting between two inbox groups, or taking
/// them into a group's transaction, after the first, since the inbox rows waiting wait for them.
const SHARE_TIME: Duration = Duration::from_millis(1);
/// How long the writer waits before trying a commit that failed again, doubling each time.
const RETRY_FIRST: Duration = Duration::from_millis(10);
const RETRY_MOST: Duration = Duration::from_secs(1);
/// How long indexing for search waits, at most, before trying a step that failed again.
const INDEX_RETRY_MOST: Duration = Duration::from_secs(60);

/// The WAL's size in frames, of a 4 KiB page each, past which background transactions wait for
/// the checkpointer; past WAL_HARD they wait until the WAL has started again from its beginning,
/// and the writer starts it again itself after an inbox group or a live write. Each start costs
/// three syncs, two in a pause that inbox rows and live writes wait for and one in the transaction
/// after, which syncs the WAL's new header, each sent to the drive and fenced on the Mac's own and
/// flushed in full elsewhere (`vfs`), so the WAL starts again as seldom as WAL_LIMIT allows, and the
/// file stays under it while no reader holds its frames for long.
const WAL_SOFT: u64 = 12_288;
const WAL_HARD: u64 = 15_360;
/// Frames few enough for the writer to copy itself between two transactions.
const WAL_NEAR: u64 = 64;
/// How long the checkpointer has to copy the last frames by itself, which it can between two live
/// writes, before the writer copies them, syncing twice while live writes wait.
const FINISH_AFTER: Duration = Duration::from_millis(50);
/// How often, 10 ms apart, the writer tries to empty the WAL while another checkpoint runs.
const TRUNCATE_TRIES: u32 = 500;
/// Past WAL_HARD, how long the writer waits for readers still on the WAL's frames as it starts the
/// WAL again, and, if one holds them longer, how long before it tries again. Inbox rows and live
/// writes arriving meanwhile wait for it, so it waits far less than background work can.
const RESTART_READERS: Duration = Duration::from_millis(20);
/// How long the writer waits to empty the WAL again after a reader held its frames.
const WAL_AGAIN: Duration = Duration::from_secs(1);
/// How long the writer goes without a commit before it empties the WAL erasures left: a backlog's
/// or a backfill's transactions follow one another closer than that, so none waits behind it, and
/// a jawd too busy ever to go IDLE, when the checkpointer empties it, still has moments that long.
const WAL_QUIET: Duration = Duration::from_secs(1);
const RESTART_AGAIN: Duration = Duration::from_millis(50);
/// Frames past WAL_HARD from which the writer, refused the checkpoint lock, waits for the
/// checkpointer's pass to end rather than commit more, for RESTART_MOST at most: what's left before
/// the file passes WAL_LIMIT, 928 frames past WAL_HARD, is then room for one more transaction. A
/// pass syncs the WAL as it begins, which a busy drive can take hundreds of ms over.
const WAL_EDGE: u64 = 384;
const RESTART_MOST: Duration = Duration::from_secs(1);
/// What the WAL file is cut back to each time it starts again.
const WAL_LIMIT: i64 = 64 * 1024 * 1024;
/// Fewer frames than this wait for more before the checkpointer copies them, unless background
/// work is waiting: each pass syncs twice whatever it copies, and every sync an inbox commit shares
/// the drive with makes its ack later.
const WAL_EASY: u64 = 1_024;
/// How long jaw.db goes without a commit before the checkpointer empties the WAL.
const IDLE: Duration = Duration::from_secs(10);
/// While background work waits for the checkpointer, how often the writer looks again.
const HOLD_POLL: Duration = Duration::from_millis(1);

/// The WAL's limits, the writer's busy timeout and how long jaw.db goes without a commit before the
/// WAL is emptied, which tests make smaller, when indexing goes on among live writes, and what a
/// background transaction takes, which tests make longer.
#[derive(Clone, Copy)]
struct Limits {
    wal_soft: u64,
    wal_hard: u64,
    busy_ms: i64,
    idle: Duration,
    live_pause: Duration,
    stream_step: Duration,
    batch_time: Duration,
    yield_after: Duration,
    whole_merge_every: Duration,
    /// The least a secure delete counts as when work paces itself, none in jawd: a test's makes
    /// one cost what a million messages make it cost.
    secure_delete_least: Duration,
}

const LIMITS: Limits = Limits {
    wal_soft: WAL_SOFT,
    wal_hard: WAL_HARD,
    busy_ms: BUSY_MS,
    idle: IDLE,
    live_pause: LIVE_PAUSE,
    stream_step: STREAM_STEP,
    batch_time: BATCH_TIME,
    yield_after: YIELD_AFTER,
    whole_merge_every: WHOLE_MERGE_EVERY,
    secure_delete_least: Duration::ZERO,
};

/// What a connection to jaw.db is for, which decides how it's set up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Writer,
    Reader,
    Checkpointer,
}

/// Opens jaw.db at `path` for `role`. Every setting is read back, since SQLite ignores a PRAGMA it
/// doesn't know or a value it can't take, and a connection that isn't as asked isn't used.
fn connect(path: &Path, role: Role) -> Result<Connection> {
    let flags = match role {
        Role::Reader => OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
        Role::Writer | Role::Checkpointer => OpenFlags::default(),
    };
    vfs::register();
    let db = Connection::open_with_flags_and_vfs(path, flags, vfs::NAME)
        .with_context(|| format!("can't open {}", path.display()))?;
    pragma(&db, "busy_timeout", BUSY_MS)?;
    // With these two SQLite asks a commit's sync at SQLITE_SYNC_NORMAL, which jaw's VFS makes a
    // full flush, and a checkpoint's and a new WAL header's at SQLITE_SYNC_FULL, which it sends to
    // the drive and fences, since on macOS a checkpoint whose syncs stop at the drive's cache
    // unordered can leave jaw.db malformed after a power cut. Set for good from the start, before a
    // new jaw.db is made a WAL, which syncs it, and whichever connection closes last checkpoints:
    // setting a flag PRAGMA expires every statement prepared on the connection, so the writer
    // switches commits between synced and not with synchronous alone.
    pragma(&db, "fullfsync", 0)?;
    pragma(&db, "checkpoint_fullfsync", 1)?;
    pragma(&db, "temp_store", 2)?;
    db.set_prepared_statement_cache_capacity(64);
    match role {
        Role::Reader => check(&db, "journal_mode", "wal")?,
        Role::Writer => {
            pragma(&db, "journal_mode", "wal")?;
            pragma(&db, "journal_size_limit", WAL_LIMIT)?;
            pragma(&db, "synchronous", 1)?;
            pragma(&db, "foreign_keys", 1)?;
            // No commit pays for a checkpoint; the checkpointer does them on a connection of its own.
            pragma(&db, "wal_autocheckpoint", 0)?;
            // History lands all over the indexes, so the writer keeps more of them than SQLite's 2 MB.
            pragma(&db, "cache_size", -WRITER_CACHE_KIB)?;
            // What's deleted is zeroed in the pages written anyway; an erasure zeroes the pages it
            // frees too (`expiry::erasing`).
            db.execute_batch("PRAGMA secure_delete = FAST").context("can't set secure_delete")?;
            check(&db, "secure_delete", &expiry::FAST.to_string())?;
        }
        Role::Checkpointer => {
            check(&db, "journal_mode", "wal")?;
            pragma(&db, "journal_size_limit", WAL_LIMIT)?;
            pragma(&db, "synchronous", 2)?;
        }
    }
    Ok(db)
}

/// Has the writer's commits on `db` reach the disk before they return, or not; it never expires a
/// statement, as a flag PRAGMA would.
fn sync_commits(db: &Connection, synced: bool) -> Result<()> {
    pragma(db, "synchronous", if synced { 2 } else { 1 })
}

/// Sets a PRAGMA and reads it back.
fn pragma(db: &Connection, name: &str, value: impl Display) -> Result<()> {
    let value = value.to_string();
    db.execute_batch(&format!("PRAGMA {name} = {value}")).with_context(|| format!("can't set {name}"))?;
    check(db, name, &value)
}

/// Fails unless a PRAGMA reads as `expected`.
fn check(db: &Connection, name: &str, expected: &str) -> Result<()> {
    let read = db
        .query_row(&format!("PRAGMA {name}"), [], |r| r.get::<_, Value>(0))
        .optional()
        .with_context(|| format!("can't read {name}"))?;
    let read = match read {
        Some(Value::Integer(n)) => n.to_string(),
        Some(Value::Text(text)) => text,
        other => format!("{other:?}"),
    };
    if !read.eq_ignore_ascii_case(expected) {
        bail!("jaw.db's {name} is {read} after setting it to {expected}");
    }
    Ok(())
}

thread_local! {
    /// The frames in the WAL after the last commit on this thread's connection, as SQLite says.
    static FRAMES: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// Whether this writer is in an inbox group's transaction that may take live writes.
    static SHARING: Cell<bool> = const { Cell::new(false) };
}
/// The key a test's `Remember` fails with, as a full disk, whenever it's in such a transaction.
#[cfg(test)]
const FAIL_SHARED: &str = "fail-in-a-shared-commit";
/// How often it has.
#[cfg(test)]
static FAILED_SHARED: AtomicU64 = AtomicU64::new(0);
/// The kind of a test's inbox row whose insert fails, as a full disk, while a durable live write
/// waits to go in its commit; and how often it has.
#[cfg(test)]
const FAIL_BEFORE_FIRST: &str = "fail-before-a-durable-write";
#[cfg(test)]
static FAILED_BEFORE_FIRST: AtomicU64 = AtomicU64::new(0);

fn note_frames(_: &WalFile, frames: c_int) -> rusqlite::Result<()> {
    FRAMES.with(|f| f.set(frames.max(0) as u64));
    Ok(())
}

/// Points every merge in a chain an earlier jawd left at the chat that stays (`write::flatten`), in a
/// transaction of its own before the writer takes any change; a jaw.db without one costs a search.
/// It's done again from the start at the next open if it fails or jawd stops before it commits.
fn flatten(db: &mut Connection, stamps: &mut Stamps) -> Result<()> {
    if !write::chained(db)? {
        return Ok(());
    }
    stamps.begin();
    let flattened = (|| {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let chats = write::flatten(&tx, stamps)?;
        write::settle(&tx, stamps)?;
        tx.commit()?;
        anyhow::Ok(chats)
    })();
    match flattened {
        Ok(1) => {
            eprintln!("jawd: pointed the merges going into a chat being merged itself at the chat that stays");
            Ok(())
        }
        Ok(chats) => {
            eprintln!(
                "jawd: pointed the merges going into {chats} chats being merged themselves at the chats that stay"
            );
            Ok(())
        }
        Err(e) => {
            stamps.rolled_back();
            Err(e.context("can't point jaw.db's chains of merges at the chats that stay"))
        }
    }
}

/// Runs a checkpoint, saying (busy, frames in the WAL, frames in jaw.db).
fn wal_checkpoint(db: &Connection, mode: &str) -> Result<(bool, u64, u64)> {
    Ok(db.query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |r| {
        Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)?.max(0) as u64, r.get::<_, i64>(2)?.max(0) as u64))
    })?)
}

/// Syncs jaw.db through `db`'s own file as a checkpoint's sync, as SQLite does after a checkpoint
/// that reaches the WAL's end: what's been copied into it is sent to the drive and fenced on the
/// Mac's own, and flushed in full elsewhere (`vfs`). A file of jaw.db's opened beside SQLite's
/// would, as it closed, let go of every lock this process holds on jaw.db, and another process
/// could then take the database from under it.
fn flush(db: &Connection) -> Result<()> {
    let mut file: *mut ffi::sqlite3_file = std::ptr::null_mut();
    // SAFETY: SQLITE_FCNTL_FILE_POINTER writes a pointer to the main database's file, which `db`
    // owns and keeps open while it is, into `file`.
    let found = unsafe {
        ffi::sqlite3_file_control(db.handle(), c"main".as_ptr(), ffi::SQLITE_FCNTL_FILE_POINTER, (&raw mut file).cast())
    };
    if found != ffi::SQLITE_OK || file.is_null() {
        bail!("SQLite didn't say which file jaw.db is ({found})");
    }
    // SAFETY: the file is open and its methods are its VFS's; only this thread uses `db`, and a
    // sync changes nothing SQLite keeps about the file.
    let synced = unsafe {
        (*file).pMethods.as_ref().and_then(|methods| methods.xSync).map(|sync| sync(file, ffi::SQLITE_SYNC_FULL))
    };
    match synced {
        Some(ffi::SQLITE_OK) => Ok(()),
        Some(code) => bail!("SQLite couldn't sync jaw.db ({code})"),
        None => bail!("jaw.db's file can't be synced"),
    }
}

/// How long the writer waits before it tries to start the WAL again, after a try that said (busy,
/// frames in the WAL, frames in jaw.db): not at all once it has, nor while the checkpointer's pass
/// holds the checkpoint lock, which SQLite refuses without waiting, with a log of -1, and which the
/// next commit finds free; RESTART_AGAIN behind a reader still on the WAL's frames, which the try
/// waited for already.
fn restart_again((busy, log, _): (bool, i64, i64)) -> Option<Duration> {
    (busy && log >= 0).then_some(RESTART_AGAIN)
}

/// Runs `restart`, a try at starting the WAL again saying (busy, frames in the WAL, frames in
/// jaw.db), and while it's refused the checkpoint lock with the WAL `at_edge`, again every
/// HOLD_POLL until RESTART_MOST after `began`.
fn restarting(
    began: Instant,
    at_edge: bool,
    mut restart: impl FnMut() -> Result<(bool, i64, i64)>,
) -> Result<(bool, i64, i64)> {
    loop {
        let tried = restart()?;
        if !(tried.0 && tried.1 < 0) || !at_edge || began.elapsed() >= RESTART_MOST {
            return Ok(tried);
        }
        std::thread::sleep(HOLD_POLL);
    }
}

impl Store {
    /// Opens jaw.db in `home`, bringing its schema up to date, and starts the writer and readers.
    pub fn open(home: &Path) -> Result<Store> {
        Store::open_with(home, LIMITS)
    }

    /// As `open`, with background work waiting for the checkpointer past `soft` frames in the WAL,
    /// and for it to start again past `hard`, as a test of what waits behind its bound makes them.
    #[cfg(test)]
    pub fn open_with_wal(home: &Path, soft: u64, hard: u64) -> Result<Store> {
        Store::open_with(home, Limits { wal_soft: soft, wal_hard: hard, ..LIMITS })
    }

    fn open_with(home: &Path, limits: Limits) -> Result<Store> {
        let path = home.join("jaw.db");
        let mut db = connect(&path, Role::Writer)?;
        let fenced = vfs::fenced(&db);
        pragma(&db, "busy_timeout", limits.busy_ms)?;
        schema::migrate(&mut db)?;
        // The read path finds its store's files by the name SQLite gives jaw.db, symlinks resolved.
        let files = files::Files::open(db.path().map_or(path.as_path(), Path::new));
        let mut stamps = Stamps::load(&db)?;
        stamps.count_secure_deletes_from(limits.secure_delete_least);
        flatten(&mut db, &mut stamps)?;
        let reads = Reads::open(&path)?;
        let checkpointer = connect(&path, Role::Checkpointer)?;
        let (realtime, realtime_rx) = unbounded();
        let (live, live_rx) = unbounded();
        let (background, background_rx) = bounded(16);
        let (kick, kicks) = bounded(1);
        let (seq, committed) = watch::channel(stamps.last());
        let (copies, copies_taken) = watch::channel(0);
        let (trouble, troubled) = watch::channel(None);
        let backlog = db.query_row("SELECT EXISTS (SELECT 1 FROM fts_backlog)", [], |r| r.get(0))?;
        let noted = |key: &str| -> Result<Option<i64>> {
            Ok(db
                .query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|at| at.parse().ok()))
        };
        let wal_erased = noted(expiry::WAL_ERASED)?;
        let mut wholes = expiry::CONTENTLESS.map(|(_, note)| Whole { note, erased: None, merging: None, merged: None });
        for whole in &mut wholes {
            whole.erased = noted(whole.note)?;
        }
        let kept: i64 = db.query_row("SELECT coalesce(sum(bytes), 0) FROM thumbnails", [], |r| r.get(0))?;
        let thumbnails = Arc::new(Thumbnails::open(home, kept.max(0) as u64));
        let stats = Arc::new(Mutex::new(Stats::default()));
        let inboxed = Arc::new(Notify::new());
        let opening = Arc::new(Notify::new());
        let wal = Arc::new(Wal::new(limits.wal_soft, limits.wal_hard));
        let soonest = Arc::new(Soonest::default());
        let erased = Arc::new(AtomicU64::new(0));
        let closed = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer {
            soonest: soonest.clone(),
            erased: erased.clone(),
            closed: closed.clone(),
            stats: stats.clone(),
            inboxed: inboxed.clone(),
            opening: opening.clone(),
            db,
            stamps,
            realtime: realtime_rx,
            live: live_rx,
            inbox: Vec::new(),
            synced: Some(false),
            wal: wal.clone(),
            kick,
            trouble,
            seq,
            copies,
            pruned: Instant::now(),
            changes_pace: Pace::new(&limits),
            index_pace: Pace::new(&limits),
            backlog,
            unmerged: true,
            wholes,
            whole_merge_every: limits.whole_merge_every,
            wal_erased,
            wal_again: None,
            committed_at: Instant::now(),
            indexed: Vec::new(),
            index_after: None,
            index_retry: RETRY_FIRST,
            near: None,
            holding: None,
            restart_after: None,
            busy: Duration::from_millis(limits.busy_ms.max(0) as u64),
            live_at: None,
            stepped_at: Instant::now(),
            live_pause: limits.live_pause,
            stream_step: limits.stream_step,
            shared_failing: None,
        };
        std::thread::Builder::new().name("jawd-writer".into()).spawn(move || writer.run(background_rx))?;
        let checkpointing = wal.clone();
        std::thread::Builder::new()
            .name("jawd-checkpoint".into())
            .spawn(move || checkpoint(checkpointer, checkpointing, kicks, limits.idle))?;
        Ok(Store {
            path,
            realtime,
            live,
            background,
            reads,
            committed,
            copies_taken,
            stats,
            inboxed,
            opening,
            wal,
            trouble: troubled,
            thumbnails,
            fenced,
            files,
            soonest,
            erased,
            closed,
        })
    }

    /// The soonest time a message committed since the timer last looked goes.
    pub fn soonest(&self) -> Arc<Soonest> {
        self.soonest.clone()
    }

    /// How many messages have been erased since jaw.db was opened, disappeared or opened alike.
    pub fn erased(&self) -> u64 {
        self.erased.load(Ordering::Relaxed)
    }

    /// The view-once messages open here that have been erased since this was last asked, by n and
    /// ID: at their time, opened elsewhere, closed or deleted.
    pub fn take_closed(&self) -> Vec<(i64, String)> {
        std::mem::take(&mut *self.closed.lock().expect("the view-once messages erased"))
    }

    /// The thumbnails of link previews, kept as files beside jaw.db.
    pub fn thumbnails(&self) -> &Arc<Thumbnails> {
        &self.thumbnails
    }

    /// What the read path knows of the files rows name, and whom it tells of the copies it shows.
    pub fn files(&self) -> &Arc<files::Files> {
        &self.files
    }

    /// The feed's last committed seq, which changes as each transaction with events commits.
    pub fn committed(&self) -> watch::Receiver<i64> {
        self.committed.clone()
    }

    /// What moves on after each commit that took outbox rows whose copies of a file may be on
    /// their way, as a deletion takes an HD copy's, for whatever makes, uploads or waits for one.
    pub fn copies_taken(&self) -> watch::Receiver<u64> {
        self.copies_taken.clone()
    }

    /// What's woken each time an engine event is committed to the inbox.
    pub fn inboxed(&self) -> Arc<Notify> {
        self.inboxed.clone()
    }

    /// What's woken once a commit leaves what waits for an event to be taken in steps
    /// (`Change::Opening`).
    pub fn opening(&self) -> Arc<Notify> {
        self.opening.clone()
    }

    /// Why jaw.db can't be written, while it can't: what arrives then waits on the phone.
    pub fn trouble(&self) -> watch::Receiver<Option<String>> {
        self.trouble.clone()
    }

    /// Keeps an engine event, then sends its ack once its commit has reached the disk. It never
    /// waits: realtime writes aren't bounded.
    pub fn inbox(&self, kind: &'static str, at_ms: i64, payload: Vec<u8>, ack: Option<Ack>) {
        let _ = self.realtime.send(Inbox { kind, at_ms, payload, ack, queued: Instant::now() });
    }

    /// Makes `changes` in `lane`, each with its event, and says how it went once they're committed.
    /// A change that fails rolls back its transaction; on the background lane, earlier
    /// transactions of the batch stay committed.
    pub async fn apply(&self, lane: Lane, changes: Vec<Change>) -> Result<Applied> {
        let (done, applied) = oneshot::channel();
        match lane {
            Lane::Live => self
                .live
                .send(Live::Changes { changes, done, queued: Instant::now(), durable: false })
                .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?,
            Lane::Background => self.queue_background(Background::Changes { changes, done }).await?,
        }
        applied.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// Makes `changes` in the live lane in one transaction that reaches the disk before it's said
    /// to be done, for what must survive a power cut: an action an interface is told is done, or
    /// the commit after which a spool file can go.
    pub async fn apply_durable(&self, changes: Vec<Change>) -> Result<Applied> {
        let (done, applied) = oneshot::channel();
        self.live
            .send(Live::Changes { changes, done, queued: Instant::now(), durable: true })
            .map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?;
        applied.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
    }

    /// Removes, at a clean stop, the backup taken before an upgrade once it's older than
    /// KEEP_BACKUP, and any half written, and says how many went.
    pub fn remove_old_backups(&self) -> Result<usize> {
        schema::remove_old_backups(self.path.parent().context("jaw.db has no folder")?, std::time::SystemTime::now())
    }

    /// Copies the whole WAL into jaw.db and empties it, as at a clean stop.
    pub async fn truncate(&self) -> Result<()> {
        let (done, truncated) = oneshot::channel();
        self.live.send(Live::Truncate { done }).map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?;
        truncated.await.map_err(|_| anyhow::anyhow!("jawd's writer has stopped"))?.map_err(|e| anyhow::anyhow!(e))
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
        let db = connect(&self.path, Role::Reader)?;
        Ok(db.query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    /// What jaw.db holds for the setting `key` of `domain`, as its JSON, read on a connection of its
    /// own.
    pub fn setting(&self, domain: &str, key: &str) -> Result<Option<String>> {
        let db = connect(&self.path, Role::Reader)?;
        Ok(db
            .query_row("SELECT value FROM settings WHERE domain = ?1 AND key = ?2", [domain, key], |r| r.get(0))
            .optional()?)
    }

    /// The two numbers `sql` reads, on a connection of its own so the writer doesn't wait.
    pub fn pair(&self, sql: &str) -> Result<(i64, i64)> {
        let db = connect(&self.path, Role::Reader)?;
        Ok(db.query_row(sql, [], |row| Ok((row.get(0)?, row.get(1)?)))?)
    }

    /// How many rows `table` holds, read on a connection of its own so the writer doesn't wait.
    pub fn count(&self, table: &str) -> Result<u64> {
        let db = connect(&self.path, Role::Reader)?;
        let count: i64 = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))?;
        Ok(count.max(0) as u64)
    }

    /// How many interfaces keep a layout and the bytes of the biggest that isn't a book, and how
    /// many of them keep a book and the bytes of the biggest, read on a connection of its own so
    /// the writer doesn't wait.
    pub fn layouts_kept(&self) -> Result<LayoutsKept> {
        let db = connect(&self.path, Role::Reader)?;
        let kept: [i64; 4] = db.query_row(
            "SELECT count(*), coalesce(max(octet_length(value)) FILTER (WHERE value ->> '$.format' IS NOT 2), 0),
                count(*) FILTER (WHERE value ->> '$.format' = 2),
                coalesce(max(octet_length(value)) FILTER (WHERE value ->> '$.format' = 2), 0)
            FROM interface_state",
            [],
            |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
        )?;
        let [kept, largest, books, largest_book] = kept.map(|n| n.max(0) as u64);
        Ok(LayoutsKept { kept, largest, books, largest_book })
    }

    /// Media out's copies on their way and the files of yours kept for them, read on a connection
    /// of its own so the writer doesn't wait.
    pub fn outgoing_counts(&self) -> Result<outgoing::Counts> {
        outgoing::counts(&connect(&self.path, Role::Reader)?)
    }

    /// How many transactions the writer has made, and the lengths of its latest, in microseconds,
    /// for a measurement to take each one's.
    #[cfg(test)]
    pub fn writer_runs(&self) -> (u64, Vec<u64>) {
        let stats = self.stats.lock().expect("the writer's stats");
        (stats.transactions, stats.runs.recent())
    }

    /// How many messages backfill `name` has still to look at, 0 once it's done, read on a
    /// connection of its own.
    pub fn backfill_left(&self, name: &str) -> Result<u64> {
        let db = connect(&self.path, Role::Reader)?;
        let left: Option<i64> =
            db.query_row("SELECT upto - after FROM backfills WHERE name = ?1", [name], |r| r.get(0)).optional()?;
        Ok(left.unwrap_or(0).max(0) as u64)
    }

    /// How many link previews' thumbnails are kept, and their bytes, read on a connection of its own.
    pub fn thumbnails_kept(&self) -> Result<(u64, u64)> {
        let db = connect(&self.path, Role::Reader)?;
        let (kept, bytes): (i64, i64) =
            db.query_row("SELECT count(*), coalesce(sum(bytes), 0) FROM thumbnails", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        Ok((kept.max(0) as u64, bytes.max(0) as u64))
    }

    /// The whole backups taken before upgrades beside jaw.db, and until when each is kept.
    fn backups(&self) -> Vec<jaw_proto::Backup> {
        let Some(folder) = self.path.parent() else { return Vec::new() };
        let ms = |time: std::time::SystemTime| {
            time.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        };
        schema::backups(folder)
            .unwrap_or_default()
            .into_iter()
            .filter(|backup| backup.whole)
            .map(|backup| jaw_proto::Backup {
                file: backup.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
                bytes: backup.bytes,
                taken_ms: ms(backup.taken),
                kept_until_ms: ms(backup.taken + schema::KEEP_BACKUP),
            })
            .collect()
    }

    /// The WAL, the commits synced, the backups kept and whether jaw.db can be written, for `status`.
    pub fn storage(&self) -> jaw_proto::Storage {
        let stats = self.stats.lock().expect("the writer's stats");
        let wal_bytes = std::fs::metadata(self.path.with_extension("db-wal")).map_or(0, |m| m.len());
        jaw_proto::Storage {
            wal_bytes,
            wal_frames: self.wal.frames.load(Ordering::Relaxed),
            frames_waiting: self.wal.waiting(),
            wal_peak_frames: self.wal.peak.load(Ordering::Relaxed),
            synced_commits: stats.synced,
            backups: self.backups(),
            background_waits: stats.held_times,
            background_waited_ms: stats.held_for.as_millis() as u64,
            shared_writes: stats.shared,
            shared_failed: stats.shared_failed,
            full_flush: !self.fenced,
            trouble: self.trouble.borrow().clone(),
        }
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
            wait_background: stats.held.percentiles(),
            run: stats.runs.percentiles(),
        }
    }
}

/// How often the writer removes events older than the feed keeps, when it has background work.
const PRUNE_EVERY: Duration = Duration::from_secs(3600);

/// A contentless index's whole merge, which takes the words erasures left in it out of its
/// segments.
struct Whole {
    /// Where sync_state keeps when an erasure last left words in it.
    note: &'static str,
    /// When an erasure last left words in it, as sync_state keeps it, until it has been merged
    /// whole since.
    erased: Option<i64>,
    /// The whole merge under way, as of the erasure it began after.
    merging: Option<i64>,
    /// When the last whole merge began.
    merged: Option<Instant>,
}

impl Whole {
    /// When it's next to be merged whole, `every` after the last whole merge began, if an erasure
    /// has left words in it since and none is under way.
    fn due(&self, every: Duration) -> Option<Instant> {
        let since = self.merged.map_or_else(Instant::now, |began| began + every);
        (self.erased.is_some() && self.merging.is_none()).then_some(since)
    }
}

struct Writer {
    stats: Arc<Mutex<Stats>>,
    inboxed: Arc<Notify>,
    /// Woken once a commit leaves what waits for an event to be taken in steps.
    opening: Arc<Notify>,
    soonest: Arc<Soonest>,
    erased: Arc<AtomicU64>,
    closed: Arc<Mutex<Vec<(i64, String)>>>,
    db: Connection,
    stamps: Stamps,
    realtime: Receiver<Inbox>,
    live: Receiver<Live>,
    /// Inbox rows taken from the queue and not committed yet. They stay until they are.
    inbox: Vec<Inbox>,
    /// Whether commits reach the disk before they return, as they are now; None while unsure.
    synced: Option<bool>,
    wal: Arc<Wal>,
    /// Woken after each commit, so the checkpointer copies what it added.
    kick: Sender<()>,
    trouble: watch::Sender<Option<String>>,
    /// The feed's last committed seq, for whoever follows the feed.
    seq: watch::Sender<i64>,
    /// Moves on after each commit that took outbox rows whose copies of a file may be on their way.
    copies: watch::Sender<u64>,
    pruned: Instant,
    changes_pace: Pace,
    index_pace: Pace,
    /// Whether fts_backlog may hold messages to index.
    backlog: bool,
    /// Whether FTS5 may have segments to merge.
    unmerged: bool,
    /// Each contentless index's whole merge, by its place in `expiry::CONTENTLESS`, and how long
    /// after one began the next may.
    wholes: [Whole; expiry::CONTENTLESS.len()],
    whole_merge_every: Duration,
    /// When a message was last erased, or a caption taken out of its row, as sync_state keeps it,
    /// until the WAL has been emptied since; and, while a reader held it, when to try again.
    wal_erased: Option<i64>,
    wal_again: Option<Instant>,
    /// When the writer last committed.
    committed_at: Instant,
    /// Who waits for everything written to be indexed.
    indexed: Vec<oneshot::Sender<Result<(), String>>>,
    /// When indexing, after a step failed, is tried again, and how long the next failure waits.
    index_after: Option<Instant>,
    index_retry: Duration,
    /// Since when the checkpointer has had only a few frames left to copy, while background work
    /// waits for the WAL to start again.
    near: Option<Instant>,
    /// Since when background work, a batch or a step of indexing, has waited for the WAL to start
    /// again.
    holding: Option<Instant>,
    /// When the writer may next try to start the WAL again past WAL_HARD, after a reader kept it
    /// from it; and how long the writer's connection otherwise waits for a lock.
    restart_after: Option<Instant>,
    busy: Duration,
    /// When the last inbox group, or live write converting several inbox rows, was made and the last
    /// step of indexing ran, and how long indexing waits after the one and, while they keep coming,
    /// after the other.
    live_at: Option<Instant>,
    stepped_at: Instant,
    live_pause: Duration,
    stream_step: Duration,
    /// Why the last inbox commit with live writes in it failed, said once until the reason changes
    /// or one succeeds.
    shared_failing: Option<String>,
}

/// Makes the groups of `changes` in `tx`, each in a savepoint: one that fails is rolled back and
/// kept in quarantine with its error, and the rest go on. After each group `stop`, given the
/// changes taken so far, says whether to stop there.
fn make_groups(
    tx: &mut rusqlite::Transaction,
    stamps: &mut Stamps,
    changes: &[Change],
    groups: &[Range<usize>],
    mut stop: impl FnMut(usize, &Stamps) -> bool,
) -> Result<Made> {
    let start = groups.first().map_or(0, |g| g.start);
    let mut made = Made::default();
    for group in groups {
        let mark = stamps.mark();
        let mut savepoint = tx.savepoint()?;
        let mut failed = None;
        let mut changed = 0;
        for change in &changes[group.clone()] {
            match write::apply(&savepoint, stamps, change) {
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
                stamps.undo(mark);
                write::quarantine(tx, &changes[group.clone()], &e)?;
                made.quarantined += 1;
                if made.error.is_none() {
                    made.error = Some(format!("{e:#}"));
                }
            }
        }
        made.taken = group.end - start;
        if stop(made.taken, stamps) {
            break;
        }
    }
    Ok(made)
}

/// What a kind of background transaction's commit has lately cost for each row it took. SQLite
/// writes every page a transaction dirtied at commit, and history's rows land all over the
/// indexes, so a commit can cost more than the work before it. The next transaction stops in time
/// for its commit to fit in BATCH_TIME too. Only transactions of PACE_ROWS or more teach it, and
/// none is ended for its commit short of that: a commit costs much the same for one row as for a
/// few, so a short one's cost for each row would end the next after a row or two, which would
/// teach it the same again, and a stretch of commits slow for each row would keep every
/// transaction after it too short to teach it otherwise.
#[derive(Clone, Copy)]
struct Pace {
    commit_per_row: Duration,
    per_row: Duration,
    /// BATCH_TIME and YIELD_AFTER, as the writer's limits have them.
    time: Duration,
    yield_after: Duration,
}

const PACE_ROWS: usize = 16;

impl Pace {
    fn new(limits: &Limits) -> Pace {
        Pace {
            commit_per_row: Duration::ZERO,
            per_row: Duration::ZERO,
            time: limits.batch_time,
            yield_after: limits.yield_after,
        }
    }

    /// Whether a transaction that has run for `body` and taken `rows` has had BATCH_TIME, with its
    /// commit as rows have lately cost once it has PACE_ROWS, so a commit that once cost much can't
    /// keep every transaction after it too short to teach the pace otherwise.
    fn enough(&self, body: Duration, rows: usize) -> bool {
        body >= self.time || (rows >= PACE_ROWS && body + self.commit_per_row * rows as u32 >= self.time)
    }

    /// Whether a background transaction that has run for `body` and taken `rows` ends before its
    /// next group: at BATCH_ROWS, once it has had BATCH_TIME, or, while an inbox row or a live write
    /// is `waiting`, once its work has had YIELD_AFTER.
    fn ends(&self, body: Duration, rows: usize, waiting: bool) -> bool {
        rows >= BATCH_ROWS || self.enough(body, rows) || (waiting && body >= self.yield_after)
    }

    /// How many rows a transaction can take in BATCH_TIME, as rows have lately cost, and at least
    /// PACE_ROWS.
    fn rows(&self) -> usize {
        if self.per_row.is_zero() {
            return 100;
        }
        (self.time.as_nanos() / self.per_row.as_nanos()).clamp(PACE_ROWS as u128, BATCH_ROWS as u128) as usize
    }

    fn learn(&mut self, body: Duration, commit: Duration, rows: usize) {
        if rows >= PACE_ROWS {
            self.commit_per_row = (self.commit_per_row * 3 + commit / rows as u32) / 4;
            self.per_row = (self.per_row * 3 + (body + commit) / rows as u32) / 4;
        }
    }
}

impl Writer {
    /// Inbox first, then live changes, then background work in short transactions.
    fn run(mut self, background: Receiver<Background>) {
        self.db.wal_hook(Some(note_frames));
        self.prune();
        let (realtime, live) = (self.realtime.clone(), self.live.clone());
        loop {
            let idle = self.idle_for();
            select_biased! {
                recv(realtime) -> row => match row {
                    Ok(row) => self.take_inbox(row),
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
                default(idle) => self.index_step(),
            }
        }
    }

    /// How long the writer may wait for work before an indexing step is due.
    fn idle_for(&self) -> Duration {
        if !self.backlog && !self.unmerged {
            let due = [self.whole_merge_due(), self.wal_empty_due()].into_iter().flatten().min();
            return due.map_or(Duration::from_secs(3600), |due| {
                due.saturating_duration_since(Instant::now()).max(INDEX_PAUSE)
            });
        }
        self.index_due().map_or(INDEX_PAUSE, |due| due.saturating_duration_since(Instant::now()).max(INDEX_PAUSE))
    }

    /// When indexing may go on, if not now: after a step that failed has waited, and LIVE_PAUSE
    /// after the last inbox group or conversion of several inbox rows, so a catch-up's or a flood's
    /// writes don't wait behind steps, or STREAM_STEP after the last step, whichever is sooner.
    fn index_due(&self) -> Option<Instant> {
        let live = self.live_at.map(|at| (at + self.live_pause).min(self.stepped_at + self.stream_step));
        [self.index_after, live].into_iter().flatten().max()
    }

    /// Merges FTS5's segments or indexes some of fts_backlog, if the WAL has room; while it hasn't,
    /// indexing waits for the checkpointer as background batches do. A step that fails is tried
    /// again later, waiting longer each time.
    fn index_step(&mut self) {
        if self.index_due().is_some_and(|due| Instant::now() < due) {
            return;
        }
        if !self.room() {
            self.hold();
            return;
        }
        self.unhold();
        // Merging first keeps FTS5's levels short of a merge it would do all at once.
        let step = if self.unmerged || self.whole_merge_due().is_some_and(|due| due <= Instant::now()) {
            self.merge()
        } else if self.backlog {
            self.index()
        } else if self.wal_empty_due().is_some_and(|due| due <= Instant::now()) {
            self.empty_wal()
        } else {
            Ok(())
        };
        self.stepped_at = Instant::now();
        match &step {
            Ok(()) => (self.index_after, self.index_retry) = (None, RETRY_FIRST),
            Err(e) => {
                eprintln!(
                    "jawd: can't index messages for search, so it's tried again in {} ms: {e:#}",
                    self.index_retry.as_millis()
                );
                self.index_after = Some(Instant::now() + self.index_retry);
                self.index_retry = (self.index_retry * 2).min(INDEX_RETRY_MOST);
            }
        }
        if step.is_err() || (!self.backlog && !self.unmerged) {
            for done in self.indexed.drain(..) {
                let _ = done.send(step.as_ref().map(|_| ()).map_err(|e| format!("{e:#}")));
            }
        }
    }

    fn count(&self, waited: Option<Duration>, ran: Duration) {
        let mut stats = self.stats.lock().expect("the writer's stats");
        stats.transactions += 1;
        if let Some(waited) = waited {
            stats.waits.add(waited);
        }
        stats.runs.add(ran);
    }

    /// After a commit: what it left in the WAL, and the checkpointer woken to copy it.
    fn committed(&mut self) {
        self.committed_at = Instant::now();
        let frames = FRAMES.with(Cell::get);
        self.wal.frames.store(frames, Ordering::Relaxed);
        self.wal.peak.fetch_max(frames, Ordering::Relaxed);
        let _ = self.kick.try_send(());
        if let Some(at) = self.stamps.take_expiring() {
            self.soonest.note(at);
        }
        for (whole, at) in self.wholes.iter_mut().zip(self.stamps.take_left_in()) {
            whole.erased = at.or(whole.erased);
        }
        if let Some(at) = self.stamps.take_wal_erased() {
            self.wal_erased = Some(at);
        }
        self.erased.fetch_add(self.stamps.take_erased(), Ordering::Relaxed);
        let closed = self.stamps.take_closed();
        if !closed.is_empty() {
            self.closed.lock().expect("the view-once messages erased").extend(closed);
        }
    }

    /// When the WAL is next to be emptied, for what erasures since it last was left in its frames,
    /// if any did: WAL_QUIET after the last commit, or later while a reader held it.
    fn wal_empty_due(&self) -> Option<Instant> {
        let quiet = self.committed_at + WAL_QUIET;
        self.wal_erased.map(|_| self.wal_again.map_or(quiet, |again| again.max(quiet)))
    }

    /// Copies the whole WAL into jaw.db and empties it, once erasures have left what they took out
    /// in frames a running jawd keeps until it writes over them: with the short wait for readers
    /// that starting the WAL again has, and, while one holds its frames, again WAL_AGAIN later.
    /// Then lets go of the note that kept it to do, in a transaction of its own, whose frames hold
    /// none of what went.
    fn empty_wal(&mut self) -> Result<()> {
        let Some(noted) = self.wal_erased else { return Ok(()) };
        let began = Instant::now();
        self.db.busy_timeout(RESTART_READERS)?;
        let tried = wal_checkpoint(&self.db, "TRUNCATE");
        if let Err(e) = self.db.busy_timeout(self.busy) {
            eprintln!("jawd: can't give the writer its busy timeout back: {e}");
        }
        if tried?.0 {
            self.wal_again = Some(Instant::now() + WAL_AGAIN);
            return Ok(());
        }
        FRAMES.with(|f| f.set(0));
        self.wal.frames.store(0, Ordering::Relaxed);
        self.wal.passed((0, 0));
        self.synced(false)?;
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM sync_state WHERE key = ?1 AND value = ?2",
            params![expiry::WAL_ERASED, noted.to_string()],
        )?;
        tx.commit()?;
        (self.wal_erased, self.wal_again) = (None, None);
        self.committed();
        self.count(None, began.elapsed());
        Ok(())
    }

    /// When a contentless index is next to be merged whole, for the words erasures left in it since
    /// its last whole merge began, if any did.
    fn whole_merge_due(&self) -> Option<Instant> {
        self.wholes.iter().filter_map(|whole| whole.due(self.whole_merge_every)).min()
    }

    /// Makes the writer's commits reach the disk before they return, or go back to letting a later
    /// synced commit or the checkpointer's syncs do it. On macOS only F_FULLFSYNC gets past the
    /// drive's cache, which jaw's VFS makes of a FULL commit's sync. Each is read back before a
    /// commit, so one is never said to be synced when it isn't.
    fn synced(&mut self, synced: bool) -> Result<()> {
        if self.synced != Some(synced) {
            self.synced = None;
            sync_commits(&self.db, synced)?;
            self.synced = Some(synced);
        }
        Ok(())
    }

    /// Says why jaw.db can't be written, once each time it starts failing.
    fn failing(&self, e: &anyhow::Error) {
        let full = e.chain().any(|cause| {
            matches!(
                cause.downcast_ref::<rusqlite::Error>(),
                Some(rusqlite::Error::SqliteFailure(f, _)) if f.code == rusqlite::ErrorCode::DiskFull
            )
        });
        let why = if full {
            "jaw's disk is full, so what arrives waits on the phone until there's room".to_string()
        } else {
            format!("jaw can't write to jaw.db ({e:#}), so what arrives waits on the phone until it can")
        };
        self.trouble.send_if_modified(|trouble| {
            if trouble.is_none() {
                eprintln!("jawd: {why}");
            }
            let changed = trouble.as_deref() != Some(&why);
            *trouble = Some(why);
            changed
        });
    }

    fn recovered(&self) {
        self.trouble.send_if_modified(|trouble| {
            if trouble.take().is_some() {
                eprintln!("jawd: jaw.db can be written again");
                true
            } else {
                false
            }
        });
    }

    /// Runs `attempt` until it succeeds or fails for a reason that won't pass, waiting from 10 ms
    /// doubling to 1 s between tries. The inbox goes first even then, and nothing else meanwhile.
    fn retrying<T>(&mut self, mut attempt: impl FnMut(&mut Writer) -> Result<T>) -> Result<T> {
        let mut pause = RETRY_FIRST;
        loop {
            match attempt(self) {
                Ok(made) => {
                    self.recovered();
                    return Ok(made);
                }
                Err(e) if write::transient(&e) => {
                    self.failing(&e);
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(RETRY_MOST);
                    self.commit_inbox(false);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn take_inbox(&mut self, row: Inbox) {
        self.inbox.push(row);
        self.commit_inbox(true);
    }

    /// Commits the inbox rows waiting, a group at a time, until none are. With `share`, a stream of
    /// rows never holds live writes back: after each group, those waiting go, in order, for
    /// SHARE_TIME or at least one, each in a transaction of its own, until one that must reach the
    /// disk, which goes in the next group's synced commit, its flush theirs.
    fn commit_inbox(&mut self, share: bool) {
        let mut first = None;
        while self.commit_group(share, &mut first) {
            if share {
                first = self.between_groups();
            }
        }
        if let Some(write) = first {
            self.live_write(write);
        }
    }

    /// Between two inbox groups, makes the live writes waiting, in order, for SHARE_TIME or at
    /// least one, up to the first durable one, which it hands back for the next group's commit.
    fn between_groups(&mut self) -> Option<Live> {
        let began = Instant::now();
        while began.elapsed() < SHARE_TIME {
            match self.live.try_recv() {
                Ok(write @ Live::Changes { durable: true, .. }) => return Some(write),
                Ok(write) => self.live_write(write),
                Err(_) => break,
            }
        }
        None
    }

    /// Commits a group of the inbox rows waiting in one synced transaction, then sends their acks,
    /// and says whether there was one. With `share`, `first` and the live writes waiting after it
    /// go in the same transaction, after the rows, for SHARE_TIME or at least one, and are
    /// answered with them. A commit that fails with live writes in it is made again without them,
    /// which then go one by one as before. One that fails with none, for whatever reason, keeps
    /// the rows and is tried again from 10 ms doubling to 1 s until it succeeds, and nothing else
    /// is written meanwhile: without its ack the engine waits, and WhatsApp keeps the message.
    fn commit_group(&mut self, share: bool, first: &mut Option<Live>) -> bool {
        let mut pause = RETRY_FIRST;
        loop {
            while self.inbox.len() < INBOX_GROUP {
                match self.realtime.try_recv() {
                    Ok(row) => self.inbox.push(row),
                    Err(_) => break,
                }
            }
            if self.inbox.is_empty() {
                return false;
            }
            let began = Instant::now();
            let (mut shared, mut after) = (Vec::new(), None);
            match self.store_inbox(share, first, &mut shared, &mut after) {
                Ok(synced) => {
                    self.recovered();
                    self.committed();
                    self.live_at = Some(Instant::now());
                    let ran = began.elapsed();
                    let mut stats = self.stats.lock().expect("the writer's stats");
                    stats.transactions += 1;
                    stats.synced += u64::from(synced);
                    stats.runs.add(ran);
                    for row in self.inbox.drain(..) {
                        stats.waits.add(began.saturating_duration_since(row.queued));
                        if let Some(ack) = row.ack {
                            ack.send();
                        }
                    }
                    for write in &shared {
                        stats.waits.add(write.began.saturating_duration_since(write.queued));
                    }
                    stats.shared += shared.len() as u64;
                    drop(stats);
                    self.inboxed.notify_one();
                    if !shared.is_empty() {
                        self.shared_failing = None;
                        self.unmerged |= shared.iter().any(|write| write::touches_fts(&write.changes));
                        self.backlog = true;
                        let last = self.stamps.last();
                        self.seq.send_if_modified(|seq| std::mem::replace(seq, last) != last);
                        if self.stamps.opened_later() {
                            self.opening.notify_one();
                        }
                        if self.stamps.copies_taken() {
                            self.copies.send_modify(|n| *n += 1);
                        }
                        for write in shared {
                            let _ = write.done.send(Ok(Applied {
                                changed: write.made.changed,
                                quarantined: write.made.quarantined,
                                error: write.made.error,
                                transactions: 1,
                                took: write.began.elapsed(),
                                seq: last,
                            }));
                        }
                    }
                    self.bound_wal();
                    if let Some(truncate) = after {
                        self.live_write(truncate);
                    }
                    return true;
                }
                Err(e) if first.is_some() || !shared.is_empty() || after.is_some() => {
                    let why = format!("{e:#}");
                    if self.shared_failing.as_ref() != Some(&why) {
                        eprintln!(
                            "jawd: an inbox commit with live writes in it failed, so they go after it alone: {why}"
                        );
                        self.shared_failing = Some(why);
                    }
                    self.stats.lock().expect("the writer's stats").shared_failed += 1;
                    self.commit_group(false, &mut None);
                    if let Some(write) = first.take() {
                        self.live_write(write);
                    }
                    for Shared { changes, done, queued, durable, .. } in shared {
                        self.live_write(Live::Changes { changes, done, queued, durable });
                    }
                    if let Some(truncate) = after {
                        self.live_write(truncate);
                    }
                    return true;
                }
                Err(e) => {
                    self.failing(&e);
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(RETRY_MOST);
                }
            }
        }
    }

    /// Commits the inbox rows waiting, and with `share`, `first` and the live writes waiting after
    /// it, taken into `shared` up to a Truncate, which is left in `after`; and says whether the
    /// commit ran synced. One that would run unsynced is refused.
    fn store_inbox(
        &mut self,
        share: bool,
        first: &mut Option<Live>,
        shared: &mut Vec<Shared>,
        after: &mut Option<Live>,
    ) -> Result<bool> {
        self.synced(true)?;
        if share {
            self.stamps.begin();
        }
        #[cfg(test)]
        SHARING.with(|sharing| sharing.set(share));
        let stored = (|| {
            let mut tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut insert = tx.prepare_cached("INSERT INTO inbox (kind, at, payload) VALUES (?1, ?2, ?3)")?;
                for row in &self.inbox {
                    #[cfg(test)]
                    if row.kind == FAIL_BEFORE_FIRST && first.is_some() {
                        FAILED_BEFORE_FIRST.fetch_add(1, Ordering::Relaxed);
                        let full = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL);
                        return Err(
                            rusqlite::Error::SqliteFailure(full, Some("failed before its live write".into())).into()
                        );
                    }
                    insert.execute(params![row.kind, row.at_ms, row.payload])?;
                }
            }
            let sharing = Instant::now();
            loop {
                let write = match first.take() {
                    Some(write) => write,
                    None if !share || (!shared.is_empty() && sharing.elapsed() >= SHARE_TIME) => break,
                    None => match self.live.try_recv() {
                        Ok(write) => write,
                        Err(_) => break,
                    },
                };
                match write {
                    Live::Changes { changes, done, queued, durable } => {
                        let groups = write::groups(&changes);
                        shared.push(Shared {
                            changes,
                            done,
                            queued,
                            durable,
                            began: Instant::now(),
                            made: Made::default(),
                        });
                        let write = shared.last_mut().expect("the write just taken");
                        write.made = make_groups(&mut tx, &mut self.stamps, &write.changes, &groups, |_, _| false)?;
                    }
                    truncate => {
                        *after = Some(truncate);
                        break;
                    }
                }
            }
            if !shared.is_empty() {
                write::settle(&tx, &mut self.stamps)?;
            }
            let synced = commits_synced(&tx)?;
            if !synced {
                bail!("the inbox's commit would run unsynced, so nothing is acked");
            }
            tx.commit()?;
            Ok(synced)
        })();
        #[cfg(test)]
        SHARING.with(|sharing| sharing.set(false));
        if stored.is_err() && share {
            self.stamps.rolled_back();
        }
        stored
    }

    /// Whether a background transaction may begin, as the WAL stands. Past the soft limit it waits
    /// for the checkpointer to copy every frame, after which the next transaction starts the WAL
    /// again from its beginning. Live writes landing during the checkpointer's passes can keep it
    /// from ever getting there, so after FINISH_AFTER with only a few frames left the writer copies
    /// those itself, where no commit can land meanwhile. Past the hard limit it waits for that
    /// whatever holds the frames.
    fn room(&mut self) -> bool {
        let frames = self.wal.frames.load(Ordering::Relaxed);
        let waiting = self.wal.waiting();
        if frames < self.wal.soft || waiting == 0 {
            self.near = None;
            self.wal.finishing.store(false, Ordering::Relaxed);
            return true;
        }
        if waiting > WAL_NEAR {
            return false;
        }
        let near = *self.near.get_or_insert_with(Instant::now);
        if near.elapsed() < FINISH_AFTER {
            return false;
        }
        self.wal.finishing.store(true, Ordering::Relaxed);
        match wal_checkpoint(&self.db, "PASSIVE") {
            // The checkpointer is in the middle of a pass, and leaves the rest once it's done.
            Ok((true, _, _)) => false,
            Ok((false, log, copied)) => {
                self.wal.passed((log, copied));
                // Otherwise a reader holds the frames left.
                copied >= log || frames < self.wal.hard
            }
            Err(e) => {
                eprintln!("jawd: the writer's checkpoint failed: {e:#}");
                frames < self.wal.hard
            }
        }
    }

    /// Waits until the WAL has room for a background transaction, taking inbox and live writes
    /// meanwhile.
    fn wait_for_room(&mut self) {
        if !self.room() {
            self.hold();
            let (realtime, live) = (self.realtime.clone(), self.live.clone());
            while !self.room() {
                select_biased! {
                    recv(realtime) -> row => match row {
                        Ok(row) => self.take_inbox(row),
                        Err(_) => break,
                    },
                    recv(live) -> write => match write {
                        Ok(write) => self.live_write(write),
                        Err(_) => break,
                    },
                    default(HOLD_POLL) => {}
                }
            }
        }
        self.unhold();
    }

    /// Background work waits for the WAL to start again, so the checkpointer copies however few
    /// frames wait. It's woken as the wait begins: it may have looked at the last commit before
    /// anything waited and left its few frames, and nothing else commits to wake it.
    fn hold(&mut self) {
        if self.holding.is_none() {
            self.holding = Some(Instant::now());
            self.wal.held.store(true, Ordering::Relaxed);
            let _ = self.kick.try_send(());
        }
    }

    /// Background work has room again, after waiting for it if it did.
    fn unhold(&mut self) {
        let Some(began) = self.holding.take() else { return };
        self.wal.held.store(false, Ordering::Relaxed);
        let held = began.elapsed();
        let mut stats = self.stats.lock().expect("the writer's stats");
        stats.held.add(held);
        stats.held_times += 1;
        stats.held_for += held;
    }

    /// Merges MERGE_PAGES of FTS5's segments, messages', links' or the library's items', if any
    /// level has enough to merge; or begins merging a contentless index whole, once an erasure has
    /// left words in it. Each merge of one an erasure has left words in, until its whole merge is
    /// done, zeroes the pages it frees, which hold what it takes out of the segments it rewrites.
    fn merge(&mut self) -> Result<()> {
        let began = Instant::now();
        self.synced(false)?;
        let every = self.whole_merge_every;
        let whole = self.wholes.iter().position(|whole| whole.due(every).is_some_and(|due| due <= Instant::now()));
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut merged = false;
        let mut wal = None;
        if let Some(index) = whole {
            // Every segment of the index made one, a step at a time as the steps after this go on
            // with it, so the words of the messages erased since, which a contentless index keeps
            // until a merge, go.
            expiry::zeroing(&tx, || Ok(tx.execute(CONTENTLESS_MERGES[index], [-MERGE_PAGES])?))?;
            merged = true;
        } else {
            // The messages' index first, then the links' and the items', each once the one before
            // it has nothing to merge, a step's worth. An erasure takes its words out of the
            // messages' segments itself.
            let merges = [("INSERT INTO messages_fts (messages_fts, rank) VALUES ('merge', ?1)", false)];
            let erased = self.wholes.iter().map(|whole| whole.erased.is_some());
            for (merge, erased) in merges.into_iter().chain(CONTENTLESS_MERGES.into_iter().zip(erased)) {
                let before = tx.total_changes();
                let step = || Ok(tx.execute(merge, [MERGE_PAGES])?);
                if erased {
                    expiry::zeroing(&tx, step)?;
                } else {
                    step()?;
                }
                merged = tx.total_changes() > before + 1;
                if merged {
                    break;
                }
            }
            if !merged {
                // Each whole merge under way is done: the erasures before it began left nothing,
                // and the pages it freed are zeroed in the WAL, whose emptying writes them over
                // jaw.db's.
                for whole in &self.wholes {
                    if let Some(began) = whole.merging {
                        tx.execute(
                            "DELETE FROM sync_state WHERE key = ?1 AND value = ?2",
                            params![whole.note, began.to_string()],
                        )?;
                        wal = Some(expiry::wal_noted(&tx)?);
                    }
                }
            }
        }
        tx.commit()?;
        if wal.is_some() {
            self.wal_erased = wal;
        }
        if let Some(index) = whole {
            let whole = &mut self.wholes[index];
            (whole.merging, whole.merged) = (whole.erased, Some(Instant::now()));
        } else if !merged {
            for whole in &mut self.wholes {
                if let Some(began) = whole.merging.take() {
                    whole.erased = whole.erased.filter(|&erased| erased != began);
                }
            }
        }
        self.unmerged = merged;
        self.committed();
        self.count(None, began.elapsed());
        Ok(())
    }

    /// Indexes a short transaction's worth of fts_backlog for search, telling the feed of the items
    /// it made findable.
    fn index(&mut self) -> Result<()> {
        let began = Instant::now();
        self.synced(false)?;
        self.stamps.begin();
        let indexed = (|| {
            let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (realtime, live, pace) = (&self.realtime, &self.live, self.index_pace);
            let indexed = write::index(&tx, &mut self.stamps, pace.rows(), |taken| {
                !realtime.is_empty() || !live.is_empty() || pace.enough(began.elapsed(), taken)
            })?;
            let body = began.elapsed();
            tx.commit()?;
            anyhow::Ok((indexed, body))
        })();
        let ((taken, more), body) = match indexed {
            Ok(indexed) => indexed,
            Err(e) => {
                self.stamps.rolled_back();
                return Err(e);
            }
        };
        self.index_pace.learn(body, began.elapsed() - body, taken);
        self.unmerged = true;
        self.committed();
        let last = self.stamps.last();
        self.seq.send_if_modified(|seq| std::mem::replace(seq, last) != last);
        self.count(None, began.elapsed());
        self.backlog = more;
        Ok(())
    }

    fn live_write(&mut self, write: Live) {
        let (changes, done, queued, durable) = match write {
            Live::Changes { changes, done, queued, durable } => (changes, done, queued, durable),
            Live::Truncate { done } => {
                let truncated = self.retrying(Writer::truncate);
                let _ = done.send(truncated.map_err(|e| format!("{e:#}")));
                return;
            }
        };
        let started = Instant::now();
        self.unmerged |= write::touches_fts(&changes);
        let waited = started - queued;
        let groups = write::groups(&changes);
        let applied = self
            .retrying(|w| {
                w.synced(durable)?;
                w.transaction(&changes, &groups, false, durable)
            })
            .map(|made| Applied {
                changed: made.changed,
                quarantined: made.quarantined,
                error: made.error,
                transactions: 1,
                took: started.elapsed(),
                seq: self.stamps.last(),
            });
        self.count(Some(waited), started.elapsed());
        // A conversion of several inbox rows at once is a catch-up's or a flood's; a lone message's,
        // or what someone just did, leaves indexing to go on between writes.
        if changes.iter().filter(|c| matches!(c, Change::Drained(Drained::Inbox(_)))).nth(1).is_some() {
            self.live_at = Some(Instant::now());
        }
        let _ = done.send(applied.map_err(|e| format!("{e:#}")));
        self.bound_wal();
    }

    /// Past WAL_HARD, starts the WAL again before anything more is written. Inbox rows and live
    /// writes don't wait for the checkpointer as background work does, and while they keep coming
    /// its passes never reach the last frame before the next commit, so the WAL would only grow.
    /// The writer copies what the checkpointer's last pass left and waits up to RESTART_READERS
    /// for readers still on the WAL's frames, so its next transaction writes the WAL from its
    /// beginning: what arrives meanwhile waits for two syncs, and that transaction makes a third as
    /// it syncs the WAL's new header, once each WAL_HARD frames. While the
    /// checkpointer's pass runs it tries again after its next commit, unless the WAL is WAL_EDGE
    /// frames past WAL_HARD, when it waits for the pass, and behind a reader that holds the frames
    /// longer, it tries again RESTART_AGAIN later. While background work waits for the WAL to start
    /// again, it starts it past WAL_SOFT: the checkpointer can't copy the last frames while inbox
    /// rows and live writes keep coming, and a reader on the WAL keeps background work's next
    /// transaction from starting it, so the wait would last until they had taken the WAL from
    /// WAL_SOFT to WAL_HARD, which a first run's took seconds over. A try that doesn't start the
    /// WAL again wakes the checkpointer as it ends: a pass refused the lock while the try held it
    /// has gone back to sleep, and while background work waits, nothing may commit to wake it
    /// before its idle timeout.
    fn bound_wal(&mut self) {
        let (began, frames) = (Instant::now(), self.wal.frames.load(Ordering::Relaxed));
        let bound = if self.holding.is_some() { self.wal.soft } else { self.wal.hard };
        if frames < bound || self.restart_after.is_some_and(|at| began < at) {
            return;
        }
        let at_edge = frames >= self.wal.hard + WAL_EDGE;
        let tried = self.restart_wal(began, at_edge);
        #[cfg(test)]
        self.wal.restarts.lock().unwrap().push(Restart {
            began,
            ended: Instant::now(),
            restarted: matches!(tried, Ok((false, _, _))),
            held_up: matches!(tried, Ok((true, log, _)) if log >= 0),
            at_edge,
            frames,
            unacked: self.inbox.len(),
        });
        if let Err(e) = self.db.busy_timeout(self.busy) {
            eprintln!("jawd: can't give the writer its busy timeout back: {e}");
        }
        let again = tried.unwrap_or_else(|e| {
            eprintln!("jawd: the writer couldn't start the WAL again: {e:#}");
            (true, 0, 0)
        });
        if again.0 {
            let _ = self.kick.try_send(());
        }
        self.restart_after = restart_again(again).map(|wait| Instant::now() + wait);
    }

    /// A try at starting the WAL again, begun at `began` with the WAL `at_edge` or not, saying
    /// (busy, frames in the WAL, frames in jaw.db).
    fn restart_wal(&mut self, began: Instant, at_edge: bool) -> Result<(bool, i64, i64)> {
        self.db.busy_timeout(RESTART_READERS)?;
        let db = &self.db;
        let tried = restarting(began, at_edge, || {
            Ok(db.query_row("PRAGMA wal_checkpoint(RESTART)", [], |r| {
                Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, r.get(2)?))
            })?)
        })?;
        if tried.1 >= 0 {
            self.wal.passed((tried.1 as u64, tried.2.max(0) as u64));
        }
        Ok(tried)
    }

    /// Copies the whole WAL into jaw.db and empties it. SQLite doesn't wait for another
    /// checkpoint to finish, so it tries again while the checkpointer's pass runs.
    fn truncate(&mut self) -> Result<()> {
        for _ in 0..TRUNCATE_TRIES {
            if !wal_checkpoint(&self.db, "TRUNCATE")?.0 {
                FRAMES.with(|f| f.set(0));
                self.wal.frames.store(0, Ordering::Relaxed);
                self.wal.passed((0, 0));
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        bail!("a reader or another checkpoint kept the WAL from being emptied")
    }

    /// Makes the groups of `changes` in one transaction, all of them for a live one; a background
    /// one stops between groups at BATCH_ROWS, at BATCH_TIME with its commit, or, once an inbox row
    /// or a live write is waiting, once its work has had YIELD_AFTER, the next group counted as
    /// costing what the costliest secure delete in it has (an erasure, or a caption taken out of
    /// search), so it stops before one that would take it past its time. Each group runs in a
    /// savepoint: one that fails is rolled back and kept in quarantine with its error, and the rest
    /// go on. A durable one is refused unless it commits synced. Says how many changes were taken.
    fn transaction(
        &mut self,
        changes: &[Change],
        groups: &[Range<usize>],
        background: bool,
        durable: bool,
    ) -> Result<Made> {
        let began = Instant::now();
        self.stamps.begin();
        let made = (|| {
            let mut tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (realtime, live, pace) = (&self.realtime, &self.live, self.changes_pace);
            let made = make_groups(&mut tx, &mut self.stamps, changes, groups, |taken, stamps| {
                let waiting = !realtime.is_empty() || !live.is_empty();
                background && pace.ends(began.elapsed() + stamps.costliest(), taken, waiting)
            })?;
            write::settle(&tx, &mut self.stamps)?;
            let synced = commits_synced(&tx)?;
            if durable && !synced {
                bail!("a durable change's commit would run unsynced, so it isn't made");
            }
            let body = began.elapsed();
            tx.commit()?;
            if synced {
                self.stats.lock().expect("the writer's stats").synced += 1;
            }
            if background {
                self.changes_pace.learn(body, began.elapsed() - body, made.taken);
                self.count(None, began.elapsed());
            }
            Ok(made)
        })();
        match &made {
            Ok(_) => {
                self.committed();
                let last = self.stamps.last();
                self.seq.send_if_modified(|seq| std::mem::replace(seq, last) != last);
                if self.stamps.copies_taken() {
                    self.copies.send_modify(|n| *n += 1);
                }
                self.backlog = true;
                if self.stamps.opened_later() {
                    self.opening.notify_one();
                }
            }
            Err(_) => self.stamps.rolled_back(),
        }
        made
    }

    /// Takes the inbox and live writes waiting, between two background transactions.
    fn between(&mut self) {
        loop {
            if let Ok(row) = self.realtime.try_recv() {
                self.take_inbox(row);
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
            self.wait_for_room();
            let made = self.retrying(|w| {
                w.synced(false)?;
                w.transaction(changes, rest, true, false)
            })?;
            applied.changed += made.changed;
            applied.quarantined += made.quarantined;
            applied.error = applied.error.take().or(made.error);
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
            self.wait_for_room();
            let pruned = self.retrying(|w| {
                w.synced(false)?;
                let tx = w.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let pruned = write::prune(&tx, BATCH_ROWS)?;
                tx.commit()?;
                w.committed();
                anyhow::Ok(pruned)
            });
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

/// Copies the WAL into jaw.db once WAL_EASY frames wait, or any while background work waits for
/// the WAL to start again, and until none do, without waiting for readers or holding up the
/// writer, which keeps appending meanwhile; and says how far it has got. Once the writer copies
/// the last few frames itself, it leaves them. SQLite syncs jaw.db only after a pass that reaches
/// the WAL's end, which none does while the writer keeps appending, so after one that doesn't it
/// syncs jaw.db itself: otherwise every page copied since the WAL last started again would wait to
/// be sent to the drive as it next does, and what arrives with it. Once jaw.db has gone IDLE
/// without a commit, it empties the WAL, and then sleeps until the next commit, so an idle jawd
/// never wakes for it. It ends when the writer has.
fn checkpoint(db: Connection, wal: Arc<Wal>, kicks: Receiver<()>, idle: Duration) {
    let mut emptied = false;
    loop {
        let kicked =
            if emptied { kicks.recv().map_err(|_| RecvTimeoutError::Disconnected) } else { kicks.recv_timeout(idle) };
        #[cfg(test)]
        wal.woke.fetch_add(1, Ordering::Relaxed);
        match kicked {
            Ok(()) => emptied = false,
            Err(RecvTimeoutError::Timeout) => {
                match wal_checkpoint(&db, "TRUNCATE") {
                    Ok((false, _, _)) => {
                        wal.frames.store(0, Ordering::Relaxed);
                        wal.passed((0, 0));
                        emptied = true;
                    }
                    Ok((true, _, _)) => {}
                    Err(e) => eprintln!("jawd: can't empty the WAL: {e:#}"),
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !wal.held.load(Ordering::Relaxed) && wal.waiting() < WAL_EASY {
            continue;
        }
        let mut last = None;
        loop {
            if wal.finishing.load(Ordering::Relaxed) && wal.waiting() <= WAL_NEAR {
                break;
            }
            let _ = kicks.try_recv();
            let copied = wal.copied.load(Ordering::Relaxed);
            let passed = match wal_checkpoint(&db, "PASSIVE") {
                // The writer's checkpoint holds the lock and says how far it got once it's done; a
                // restart of its that didn't start the WAL again wakes this thread as it ends.
                Ok((true, _, _)) => {
                    #[cfg(test)]
                    wal.refused.lock().unwrap().push(Instant::now());
                    break;
                }
                Ok((_, log, copied)) => (log, copied),
                Err(e) => {
                    eprintln!("jawd: a checkpoint failed: {e:#}");
                    break;
                }
            };
            wal.passed(passed);
            let reached = passed.1 >= passed.0 && wal.frames.load(Ordering::Relaxed) <= passed.0;
            if passed.1 != copied && !reached {
                match flush(&db) {
                    Ok(()) => {
                        #[cfg(test)]
                        wal.flushed.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => eprintln!("jawd: the checkpointer couldn't flush jaw.db: {e:#}"),
                }
            }
            if passed.1 >= passed.0 {
                break;
            }
            // A reader still needs the frames left, so give it a moment.
            if last == Some(passed) {
                std::thread::sleep(HOLD_POLL);
            }
            last = Some(passed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd;
    use std::sync::OnceLock;

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

    /// FTS5's postings, each word at each place, against those of a fresh index, made with the same
    /// tokenizer, of the text of every message not waiting in fts_backlog: how many only FTS5 holds,
    /// stale, and how many only the text has, missing. FTS5's own integrity-check, run first, can't
    /// tell: at rank 0 it checks only the index's structure, which a stale index keeps, and at rank
    /// 1 it wants every message indexed, those without text too.
    pub(crate) fn index_against_text(db: &Connection) -> (i64, i64) {
        db.execute("INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 0)", []).unwrap();
        db.execute_batch(
            "DROP TABLE IF EXISTS temp.postings;
            DROP TABLE IF EXISTS temp.text_postings;
            DROP TABLE IF EXISTS temp.text_index;
            CREATE VIRTUAL TABLE temp.text_index USING fts5 (text, tokenize = 'unicode61 remove_diacritics 2');
            INSERT INTO temp.text_index (rowid, text) SELECT t, text FROM main.messages
                WHERE text IS NOT NULL AND n NOT IN (SELECT n FROM main.fts_backlog);
            CREATE VIRTUAL TABLE temp.postings USING fts5vocab (main, messages_fts, instance);
            CREATE VIRTUAL TABLE temp.text_postings USING fts5vocab (temp, text_index, instance);",
        )
        .unwrap();
        db.query_row(
            "SELECT (SELECT count(*) FROM (SELECT term, doc, offset FROM temp.postings
                    EXCEPT SELECT term, doc, offset FROM temp.text_postings)),
                (SELECT count(*) FROM (SELECT term, doc, offset FROM temp.text_postings
                    EXCEPT SELECT term, doc, offset FROM temp.postings))",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn the_index_check_finds_a_stale_index_that_fts5s_own_check_passes() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let mut stamps = Stamps::load(&db).unwrap();
        stamps.begin();
        let ana = "351910000002@s.whatsapp.net";
        let photo = Message { kind: "image", text: None, summary: "Photo".into(), ..text(ana, "p1", 2_000, "") };
        for message in [text(ana, "m1", 1_000, "jantar amanhã"), text(ana, "m2", 1_500, "jantar hoje"), photo] {
            write::apply(&db, &mut stamps, &Change::Message(message)).unwrap();
        }
        assert_eq!(index_against_text(&db), (0, 0), "waiting in fts_backlog, so in neither");
        while write::index(&db, &mut stamps, 1, |_| false).unwrap().1 {}
        assert_eq!(index_against_text(&db), (0, 0));
        let rank_1 = db.execute("INSERT INTO messages_fts (messages_fts, rank) VALUES ('integrity-check', 1)", []);
        assert!(rank_1.is_err(), "FTS5's check against its content wants the photo, which has no text, indexed");
        db.execute_batch(
            "DROP TRIGGER messages_fts_update; UPDATE messages SET text = 'almoço amanhã' WHERE id = 'm1'",
        )
        .unwrap();
        assert_eq!(index_against_text(&db), (1, 1), "jantar in m1 is stale and almoço missing, past FTS5's own check");
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

    #[test]
    fn a_live_messages_text_waits_to_be_indexed_so_its_commit_does_none_of_fts5s_work() {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let mut stamps = Stamps::load(&db).unwrap();
        stamps.begin();
        let live = Message { live: true, ..text("351910000002@s.whatsapp.net", "m1", 1_000, "jantar amanhã") };
        write::apply(&db, &mut stamps, &Change::Message(live)).unwrap();
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        let matching = "SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'jantar'";
        assert_eq!((count("SELECT count(*) FROM fts_backlog"), count(matching)), (1, 0), "waiting, not in FTS5");
        assert_eq!(write::index(&db, &mut stamps, 10, |_| false).unwrap(), (1, false));
        assert_eq!((count("SELECT count(*) FROM fts_backlog"), count(matching)), (0, 1), "indexed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_messages_are_found_once_indexed_as_they_last_read_whatever_happened_to_them_before() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let ana = "351910000002@s.whatsapp.net";
        let edit =
            Amendment::Edit { text: Some("almoço amanhã".into()), spans: None, summary: "almoço amanhã".into() };
        let edited = Amend {
            chat: ana.into(),
            target: "edited".into(),
            sender: Some(ana.into()),
            mine: false,
            at: 9_000,
            what: edit,
            raw: None,
        };
        // Written, edited and deleted in one live transaction, while their text waits to be indexed.
        let applied = store
            .apply(
                Lane::Live,
                vec![
                    Change::Message(text(ana, "kept", 1_000, "jantar hoje")),
                    Change::Message(text(ana, "edited", 2_000, "jantar ontem")),
                    Change::Message(text(ana, "gone", 3_000, "jantar secreto")),
                    Change::Amend(edited),
                    Change::DeleteMessage { chat: ana.into(), id: "gone".into() },
                ],
            )
            .await
            .unwrap();
        assert_eq!((applied.changed, applied.quarantined), (5, 0));
        store.indexed().await.unwrap();
        let found = |words: &'static str| {
            let store = store.clone();
            async move {
                let found = query(&store, move |db| read::search(db, words, None, None, 10)).await;
                found.hits.into_iter().map(|hit| hit.message.id).collect::<Vec<_>>()
            }
        };
        assert_eq!(found("jantar").await, ["kept"], "the edited one says otherwise now, and the deleted one is gone");
        assert_eq!(found("almoco").await, ["edited"]);
        assert_eq!(store.count("fts_backlog").unwrap(), 0);
        let db = Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(index_against_text(&db), (0, 0), "postings stale, and missing");
    }

    /// Messages `n` and `n + 1` converted from their inbox rows in one live write, as a catch-up's
    /// conversions are.
    fn converted(n: i64) -> Vec<Change> {
        (n..n + 2)
            .flat_map(|n| {
                let message = text("351910000002@s.whatsapp.net", &format!("m{n}"), n, "jantar amanhã");
                [Change::Message(message), Change::Drained(Drained::Inbox(n))]
            })
            .collect()
    }

    /// A lone message in a live write of its own, which drains no inbox row.
    fn lone(n: i64) -> Vec<Change> {
        messages([text("351910000002@s.whatsapp.net", &format!("m{n}"), n, "jantar amanhã")])
    }

    /// How many messages search finds while live writes made by `write`, further apart than the
    /// writer's idle pause, keep coming, once it finds 100 or after 30 s, and how many were written.
    async fn found_while_writing(store: &Store, write: fn(i64) -> Vec<Change>) -> (usize, i64) {
        let streaming = Arc::new(AtomicBool::new(true));
        let stream = {
            let (store, streaming) = (store.clone(), streaming.clone());
            tokio::spawn(async move {
                let mut n = 0;
                while streaming.load(Ordering::Relaxed) {
                    let changes = write(n);
                    n += changes.iter().filter(|c| matches!(c, Change::Message(_))).count() as i64;
                    store.apply(Lane::Live, changes).await.unwrap();
                    tokio::time::sleep(INDEX_PAUSE * 3).await;
                }
                n
            })
        };
        let began = Instant::now();
        let mut found = 0;
        while found < 100 && began.elapsed() < Duration::from_secs(30) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            found = query(store, |db| read::search(db, "jantar", None, None, 200)).await.hits.len();
        }
        streaming.store(false, Ordering::Relaxed);
        (found, stream.await.unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn indexing_for_search_waits_for_a_pause_in_a_catch_ups_writes_and_goes_on_once_they_stop() {
        let home = tempfile::tempdir().unwrap();
        let live_pause = Duration::from_secs(5);
        let store = Store::open_with(home.path(), Limits { live_pause, stream_step: live_pause, ..LIMITS }).unwrap();
        // A catch-up's writes further apart than the writer's idle pause, closer than the live one.
        let (mut written, mut widest) = (None::<Instant>, Duration::ZERO);
        for n in 0..50 {
            widest = widest.max(written.map_or(Duration::ZERO, |at| at.elapsed()));
            store.apply(Lane::Live, converted(2 * n)).await.unwrap();
            written = Some(Instant::now());
            tokio::time::sleep(INDEX_PAUSE * 3).await;
        }
        assert!(widest < live_pause, "two writes were {widest:?} apart, past the live pause");
        assert_eq!(store.count("fts_backlog").unwrap(), 100, "no step of indexing between a catch-up's writes");
        let stopped = Instant::now();
        store.indexed().await.unwrap();
        assert!(stopped.elapsed() >= live_pause / 2, "indexed only after the pause");
        let found = query(&store, |db| read::search(db, "jantar", None, None, 200)).await;
        assert_eq!((found.hits.len(), store.count("fts_backlog").unwrap()), (100, 0));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_long_catch_up_has_its_text_indexed_as_it_goes() {
        let home = tempfile::tempdir().unwrap();
        let limits = Limits { live_pause: Duration::from_secs(60), stream_step: Duration::from_millis(50), ..LIMITS };
        let store = Store::open_with(home.path(), limits).unwrap();
        let (found, written) = found_while_writing(&store, converted).await;
        assert!(found >= 100, "{found} of {written} found while a catch-up's writes kept coming");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_writes_of_lone_messages_leave_indexing_to_go_on_between_them() {
        let home = tempfile::tempdir().unwrap();
        let limits = Limits { live_pause: Duration::from_secs(60), stream_step: Duration::from_secs(60), ..LIMITS };
        let store = Store::open_with(home.path(), limits).unwrap();
        let (found, written) = found_while_writing(&store, lone).await;
        assert!(found >= 100, "{found} of {written} found while lone messages kept coming");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_live_write_taken_into_an_inbox_commit_is_indexed_with_nothing_written_after_it() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let streaming = Arc::new(AtomicBool::new(true));
        let feeder = {
            let (store, streaming) = (store.clone(), streaming.clone());
            std::thread::spawn(move || {
                let mut n = 0;
                while streaming.load(Ordering::Relaxed) {
                    store.inbox("message", n, vec![1, 2, 3], None);
                    n += 1;
                    std::thread::sleep(Duration::from_micros(200));
                }
            })
        };
        tokio::time::sleep(Duration::from_millis(200)).await;
        let before = store.storage().shared_writes;
        let live = Message { live: true, ..text("351910000002@s.whatsapp.net", "rode", 5_000, "jantar amanhã") };
        store.apply_durable(messages([live])).await.unwrap();
        let shared = store.storage().shared_writes - before;
        tokio::time::sleep(Duration::from_millis(50)).await;
        streaming.store(false, Ordering::Relaxed);
        feeder.join().unwrap();
        assert_eq!(shared, 1, "the write went in an inbox group's commit");
        store.indexed().await.unwrap();
        let found = query(&store, |db| read::search(db, "jantar", None, None, 10)).await;
        assert_eq!(found.hits.len(), 1, "{} still waiting in fts_backlog", store.count("fts_backlog").unwrap());
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

    #[test]
    fn background_work_gives_way_to_what_waits_only_once_it_has_had_its_millisecond() {
        let pace = Pace {
            commit_per_row: Duration::from_micros(10),
            per_row: Duration::from_micros(40),
            ..Pace::new(&LIMITS)
        };
        let us = Duration::from_micros;
        assert!(!pace.ends(us(200), 10, true), "a moment in, it goes on though an inbox row waits");
        assert!(pace.ends(us(1_000), 10, true), "it has had its millisecond");
        assert!(!pace.ends(us(1_000), 10, false), "with nothing waiting it goes on to BATCH_TIME");
        assert!(pace.ends(us(2_900), 20, false), "its commit would end it past BATCH_TIME");
        assert!(pace.ends(Duration::ZERO, BATCH_ROWS, false), "BATCH_ROWS is as many as it takes");
    }

    #[test]
    fn a_commit_that_once_cost_much_cant_keep_transactions_too_short_to_teach_the_pace_again() {
        let (us, ms) = (Duration::from_micros, Duration::from_millis);
        let slow = Pace { commit_per_row: ms(1), per_row: ms(1), ..Pace::new(&LIMITS) };
        assert!(!slow.ends(us(500), PACE_ROWS - 1, false), "its estimate doesn't end it short of PACE_ROWS");
        assert!(slow.ends(us(500), PACE_ROWS, false), "at PACE_ROWS it does");
        assert!(slow.ends(ms(3), 1, false), "its own work still ends it at BATCH_TIME");
        assert_eq!(slow.rows(), PACE_ROWS, "a step of indexing takes as many");
    }

    #[test]
    fn only_a_transaction_of_many_rows_teaches_the_pace_what_its_commit_costs_for_each() {
        let (us, mut pace) = (Duration::from_micros, Pace::new(&LIMITS));
        pace.learn(us(100), us(2_000), 1);
        assert_eq!(pace.commit_per_row, Duration::ZERO, "a commit of one row costs what one of a few does");
        pace.learn(us(1_000), us(400), 40);
        assert_eq!((pace.commit_per_row, pace.per_row), (us(10) / 4, us(35) / 4));
    }

    /// Inbox rows handed over one at a time until `stop`, each only once the one before was acked,
    /// as jaw-engine hands them over: says how many went, and the longest an ack took.
    fn one_at_a_time(store: &Store, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<(u64, Duration)> {
        let store = store.clone();
        std::thread::spawn(move || {
            let (acks, mut acked) = mpsc::unbounded_channel();
            let (mut sent, mut slowest) = (0u64, Duration::ZERO);
            while !stop.load(Ordering::Relaxed) {
                let began = Instant::now();
                store.inbox("message", 0, vec![7; 200], Some(crate::engine::Ack::to(sent, acks.clone())));
                if acked.blocking_recv().is_none() {
                    break;
                }
                slowest = slowest.max(began.elapsed());
                sent += 1;
            }
            (sent, slowest)
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn small_chats_of_history_go_many_to_a_transaction_while_inbox_rows_come_one_at_a_time() {
        let home = tempfile::tempdir().unwrap();
        // A transaction's millisecond made long, so even a machine too busy for more than a change
        // in a millisecond takes dozens while the rows wait.
        let (batch_time, yield_after) = (Duration::from_millis(200), Duration::from_millis(100));
        let store = Store::open_with(home.path(), Limits { batch_time, yield_after, ..LIMITS }).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let rows = one_at_a_time(&store, stop.clone());
        tokio::time::sleep(Duration::from_millis(100)).await;
        // A first pass of history, each chat's newest two, every message a change of its own.
        let history = (0..1_000i64).map(|n| {
            text(&format!("3519300{:05}@s.whatsapp.net", n / 2), &format!("h{n}"), n, "the newest of a small chat")
        });
        let applied = store.apply(Lane::Background, messages(history)).await.unwrap();
        stop.store(true, Ordering::Relaxed);
        let (sent, slowest) = rows.join().unwrap();
        assert_eq!(applied.changed, 1_000);
        assert!(sent >= 3, "only {sent} inbox rows came while the history went in");
        // Giving way after a change, as before, took over 800.
        assert!(
            applied.transactions * 10 <= 1_000,
            "{} transactions for 1,000 changes, {sent} rows meanwhile",
            applied.transactions
        );
        assert!(slowest < Duration::from_secs(2), "an ack of {sent} waited {slowest:?}");
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
            assert!(
                applied.error.as_deref().is_some_and(|why| why.contains("a person needs at least one JID")),
                "why, in the {lane:?} lane: {:?}",
                applied.error
            );
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
    async fn a_row_kept_in_quarantine_is_a_step_of_its_own() {
        let chat = "351910000001@s.whatsapp.net";
        let batch = vec![
            Change::Quarantined {
                source: "inbox",
                reference: "1".into(),
                error: "converting it panicked: as asked".into(),
            },
            Change::Message(text(chat, "b", 2_000, "second")),
            Change::Person(Person::default()),
            Change::Drained(Drained::Inbox(2)),
        ];
        for lane in [Lane::Live, Lane::Background] {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            inbox_rows(&store, 2).await;
            let applied = store.apply(lane, batch.clone()).await.unwrap();
            assert_eq!(applied.quarantined, 1, "the row after it failed, in the {lane:?} lane");
            let kept: Vec<(String, String, String)> = query(&store, |db| {
                Ok(db
                    .prepare("SELECT source, ref, error FROM quarantine ORDER BY ref")?
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await;
            let named: Vec<_> =
                kept.iter().map(|(source, reference, _)| (source.as_str(), reference.as_str())).collect();
            assert_eq!(named, [("inbox", "1"), ("inbox", "2")], "each under its own name, in the {lane:?} lane");
            assert!(kept[0].2.contains("panicked") && kept[1].2.contains("needs at least one JID"), "{kept:?}");
            assert_eq!(store.count("inbox").unwrap(), 2, "both rows stay");
        }
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

    #[test]
    fn a_pragma_sqlite_ignores_or_doesnt_know_is_refused() {
        let db = Connection::open_in_memory().unwrap();
        assert!(pragma(&db, "synchronous", "'FULL ff'").is_err(), "SQLite takes it and sets NORMAL");
        assert!(pragma(&db, "synchronus", 2).is_err(), "a misspelled PRAGMA does nothing");
        pragma(&db, "synchronous", 2).unwrap();
    }

    #[test]
    fn every_connection_to_jaw_db_is_as_asked_and_its_checkpoints_reach_the_disk() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let writer = connect(&path, Role::Writer).unwrap();
        let (reader, checkpointer) =
            (connect(&path, Role::Reader).unwrap(), connect(&path, Role::Checkpointer).unwrap());
        for db in [&writer, &reader, &checkpointer] {
            check(db, "fullfsync", "0").unwrap();
            check(db, "checkpoint_fullfsync", "1").unwrap();
            check(db, "busy_timeout", "5000").unwrap();
            check(db, "journal_mode", "wal").unwrap();
            assert!(vfs::opened_through(db), "a connection that isn't through jaw's VFS");
        }
        for (name, value) in [("synchronous", "1"), ("wal_autocheckpoint", "0"), ("journal_size_limit", "67108864")] {
            check(&writer, name, value).unwrap();
        }
        check(&checkpointer, "synchronous", "2").unwrap();
        assert!(reader.execute_batch("CREATE TABLE nope (x)").is_err(), "readers can't write");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn inbox_rows_and_durable_changes_are_synced_before_they_are_answered_and_live_ones_are_not() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let inboxed = store.inboxed();
        // An inbox commit that would run unsynced is refused and tried again, so it never lands.
        let landed = |notified| async move {
            tokio::time::timeout(Duration::from_secs(5), notified).await.expect("the inbox row was never committed");
        };
        let notified = inboxed.notified();
        store.inbox("message", 0, vec![1], None);
        landed(notified).await;
        assert_eq!(store.storage().synced_commits, 1, "counted from the setting the commit ran under");
        let chat = "351910000001@s.whatsapp.net";
        store.apply(Lane::Live, messages([text(chat, "m1", 1_000, "converted")])).await.unwrap();
        assert_eq!(store.storage().synced_commits, 1, "a conversion commits at NORMAL");
        store.apply_durable(messages([text(chat, "m2", 2_000, "done")])).await.unwrap();
        assert_eq!(store.storage().synced_commits, 2);
        let notified = inboxed.notified();
        store.inbox("message", 0, vec![2], None);
        landed(notified).await;
        assert_eq!(store.storage().synced_commits, 3, "after a NORMAL commit, the next inbox row is synced again");
    }

    #[test]
    fn switching_the_writers_commits_between_synced_and_not_keeps_its_statements_prepared() {
        let home = tempfile::tempdir().unwrap();
        let db = connect(&home.path().join("jaw.db"), Role::Writer).unwrap();
        db.execute_batch("CREATE TABLE t (x INTEGER)").unwrap();
        for synced in [true, false, true, false] {
            sync_commits(&db, synced).unwrap();
            assert_eq!(commits_synced(&db).unwrap(), synced, "FULL through jaw's VFS, or NORMAL");
            let mut insert = db.prepare_cached("INSERT INTO t (x) VALUES (?1)").unwrap();
            insert.execute([1]).unwrap();
            assert_eq!(
                insert.get_status(rusqlite::StatementStatus::RePrepare),
                0,
                "a statement made again after a switch costs a live write what preparing its triggers does"
            );
        }
    }

    #[test]
    fn a_commit_is_counted_synced_only_through_jaws_vfs_with_fullfsync_off() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let db = connect(&path, Role::Writer).unwrap();
        sync_commits(&db, true).unwrap();
        assert!(commits_synced(&db).unwrap(), "FULL through jaw's VFS, fullfsync off");
        // With fullfsync on, SQLite asks a commit's sync at FULL, which the VFS fences.
        pragma(&db, "fullfsync", 1).unwrap();
        assert!(!commits_synced(&db).unwrap(), "a commit whose sync the VFS fences");
        // Through SQLite's own VFS, a FULL commit with fullfsync off is a plain fsync on macOS.
        let own = Connection::open(&path).unwrap();
        pragma(&own, "synchronous", 2).unwrap();
        pragma(&own, "fullfsync", 0).unwrap();
        assert!(!commits_synced(&own).unwrap(), "a commit through SQLite's own VFS");
    }

    /// A sync jaw's VFS made of a file in a test's folder: the file's name, the thread that asked,
    /// the flags SQLite asked it with and what it was made of.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Synced {
        file: String,
        thread: String,
        asked: c_int,
        made: Option<vfs::Made>,
    }

    impl Synced {
        fn of(&self, file: &str, asked: c_int, made: vfs::Made) -> bool {
            self.file == file && self.asked & 0x0f == asked && self.made == Some(made)
        }
    }

    /// Every sync of the files in `home` since `mark` of what was asked of them, by `thread`, or by
    /// any thread with none.
    fn syncs_since(home: &Path, mark: usize, thread: Option<&str>) -> Vec<Synced> {
        vfs::noted::events(home)[mark..]
            .iter()
            .filter_map(|event| match event {
                vfs::noted::Event::Sync { path, thread: by, asked, made } if thread.is_none_or(|t| t == by) => {
                    Some(Synced {
                        file: Path::new(path).file_name().unwrap().to_string_lossy().into_owned(),
                        thread: by.clone(),
                        asked: *asked,
                        made: *made,
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// What a checkpoint's syncs, and a new WAL header's, are made of in `home`: fenced where the
    /// drive takes barriers and they order jaw.db, and flushed in full elsewhere.
    fn checkpoints_made(store: &Store, home: &Path) -> vfs::Made {
        if !store.storage().full_flush && vfs::noted::takes_barriers(home) {
            vfs::Made::Fenced
        } else {
            vfs::Made::Full
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_commit_before_an_ack_is_flushed_in_full_and_every_checkpoint_and_new_wal_header_fenced() {
        let home = tempfile::tempdir().unwrap();
        vfs::noted::watch(home.path());
        let store = Store::open(home.path()).unwrap();
        let fenced = checkpoints_made(&store, home.path());
        let (normal, full) = (ffi::SQLITE_SYNC_NORMAL, ffi::SQLITE_SYNC_FULL);
        let inboxed = store.inboxed();
        let chat = "351910000001@s.whatsapp.net";
        let mut n = 0;
        let mut write = |what: &'static str| {
            n += 1;
            let store = store.clone();
            let inboxed = inboxed.clone();
            let home = home.path().to_path_buf();
            async move {
                let mark = vfs::noted::events(&home).len();
                match what {
                    "inbox" => {
                        let notified = inboxed.notified();
                        store.inbox("message", n, vec![1], None);
                        tokio::time::timeout(Duration::from_secs(10), notified).await.expect("the inbox row landed");
                    }
                    "durable" => {
                        store.apply_durable(messages([text(chat, &format!("d{n}"), n, "done")])).await.unwrap();
                    }
                    "live" => {
                        store
                            .apply(Lane::Live, messages([text(chat, &format!("l{n}"), n, "converted")]))
                            .await
                            .unwrap();
                    }
                    _ => {
                        store
                            .apply(Lane::Background, messages([text(chat, &format!("h{n}"), n, "history")]))
                            .await
                            .unwrap();
                    }
                }
                mark
            }
        };
        // Each commit that precedes an answer or an ack has SQLite ask its WAL's sync at NORMAL,
        // which is flushed past the drive's cache before it returns, after normal commits as before
        // them, and the rest don't sync at all.
        for what in ["inbox", "live", "durable", "background", "live", "inbox", "durable", "durable", "live"] {
            let mark = write(what).await;
            let synced = syncs_since(home.path(), mark, Some("jawd-writer"));
            if matches!(what, "inbox" | "durable") {
                let flushed = synced.iter().any(|s| s.of("jaw.db-wal", normal, vfs::Made::Full));
                assert!(flushed, "{what}: the WAL wasn't flushed: {synced:?}");
            } else {
                assert!(synced.is_empty(), "{what}: a normal commit synced {synced:?}");
            }
        }
        // The writer's own checkpoint has SQLite ask at FULL for the WAL before it copies and for
        // jaw.db after, each sent to the drive and fenced.
        let mark = vfs::noted::events(home.path()).len();
        store.truncate().await.unwrap();
        let synced = syncs_since(home.path(), mark, Some("jawd-writer"));
        let both = synced.iter().any(|s| s.of("jaw.db-wal", full, fenced))
            && synced.iter().any(|s| s.of("jaw.db", full, fenced));
        assert!(both, "the writer's checkpoint: {synced:?}");
        // With the WAL emptied, the next commit, the live write or a step of indexing before it,
        // writes its new header, asked at FULL and fenced before its frames go in, and a normal
        // commit syncs nothing else.
        let emptied = vfs::noted::events(home.path())[mark..]
            .iter()
            .position(|e| matches!(e, vfs::noted::Event::Truncate { path, size: 0 } if path.ends_with("jaw.db-wal")))
            .expect("the writer's checkpoint emptied the WAL")
            + mark;
        write("live").await;
        let synced = syncs_since(home.path(), emptied, Some("jawd-writer"));
        let header: Vec<_> = synced.iter().map(|s| (s.file.as_str(), s.asked & 0x0f, s.made)).collect();
        assert_eq!(header, [("jaw.db-wal", full, Some(fenced))], "the WAL's new header: {synced:?}");
        // So does the checkpointer's, once enough frames wait for it.
        let mark = vfs::noted::events(home.path()).len();
        let words = "palavras ".repeat(400);
        let history = (0..2_000).map(|k| text(chat, &format!("p{k}"), 10_000 + k, &words));
        store.apply(Lane::Background, messages(history)).await.unwrap();
        let began = Instant::now();
        while !syncs_since(home.path(), mark, Some("jawd-checkpoint")).iter().any(|s| s.file == "jaw.db") {
            assert!(began.elapsed() < Duration::from_secs(60), "the checkpointer never copied the WAL");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let checkpointed = syncs_since(home.path(), mark, Some("jawd-checkpoint"));
        let fences = checkpointed.iter().all(|s| s.asked & 0x0f == full && s.made == Some(fenced));
        assert!(fences, "the checkpointer's syncs: {checkpointed:?}");
        let wal = checkpointed.iter().any(|s| s.file == "jaw.db-wal");
        assert!(wal, "the checkpointer copies only what it has synced: {checkpointed:?}");
        // Neither jaw.db nor its WAL is ever synced only as far as the drive's cache, unfenced, by
        // any connection, and every sync SQLite asks at NORMAL, a commit's, is a full flush.
        let unordered: Vec<_> = syncs_since(home.path(), 0, None)
            .into_iter()
            .filter(|s| matches!(s.file.as_str(), "jaw.db" | "jaw.db-wal"))
            .filter(|s| s.made.is_none() || (s.asked & 0x0f == normal && s.made != Some(vfs::Made::Full)))
            .collect();
        assert!(unordered.is_empty(), "synced neither in full nor fenced: {unordered:?}");
    }

    /// Checks the order a power cut needs in what was asked of a folder's files: before the WAL
    /// starts again, at its new header (32 bytes at its start), or is cut to nothing, every page
    /// written to jaw.db since jaw.db was last synced has been synced, fenced or in full, as only
    /// that sends them to the drive; the new header is synced before a frame is written after it;
    /// and no sync failed. Says how often the WAL started again and was emptied.
    fn assert_ordered(events: &[vfs::noted::Event]) -> (usize, usize) {
        use vfs::noted::Event;
        let name = |path: &str| Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
        let (mut pages, mut header, mut started, mut emptied) = (0, false, 0, 0);
        for (k, event) in events.iter().enumerate() {
            match event {
                Event::Write { path, at, len } => match name(path).as_str() {
                    "jaw.db" => pages += 1,
                    "jaw.db-wal" if *at == 0 && *len == 32 => {
                        assert_eq!(pages, 0, "event {k}: the WAL started again over pages of jaw.db not synced");
                        header = true;
                        started += 1;
                    }
                    "jaw.db-wal" => {
                        assert!(!header, "event {k}: a frame went in before the WAL's new header was synced")
                    }
                    _ => {}
                },
                Event::Sync { path, made, .. } => {
                    assert!(made.is_some(), "event {k}: a sync failed: {event:?}");
                    match name(path).as_str() {
                        "jaw.db" => pages = 0,
                        "jaw.db-wal" => header = false,
                        _ => {}
                    }
                }
                Event::Truncate { path, size } if *size == 0 && name(path) == "jaw.db-wal" => {
                    assert_eq!(pages, 0, "event {k}: the WAL was emptied over pages of jaw.db not synced");
                    emptied += 1;
                }
                Event::Truncate { .. } => {}
            }
        }
        (started, emptied)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wal_starts_again_or_is_emptied_only_once_every_page_copied_into_jaw_db_is_synced() {
        let home = tempfile::tempdir().unwrap();
        vfs::noted::watch(home.path());
        let quiet = Duration::from_secs(60);
        let idle = Duration::from_millis(300);
        let limits = Limits { wal_soft: SOFT, wal_hard: HARD, live_pause: quiet, stream_step: quiet, idle, ..LIMITS };
        let store = Store::open_with(home.path(), limits).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let rows = pouring(&store, stop.clone());
        let history: Vec<Message> =
            (0..5_000).map(|n| text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")).collect();
        let batch = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Background, messages(history)).await.unwrap() })
        };
        // Inbox rows, live writes, durable ones and history together, until the writer has started
        // the WAL again three times, however busy the machine, or for 30 s.
        let (began, mut n) = (Instant::now(), 0);
        while restarted(&store) < 3 && began.elapsed() < Duration::from_secs(30) {
            store.apply(Lane::Live, conversion(n * 10, 10)).await.unwrap();
            let done = text("351910000002@s.whatsapp.net", &format!("d{n}"), n, "done");
            store.apply_durable(messages([done])).await.unwrap();
            n += 1;
        }
        stop.store(true, Ordering::Relaxed);
        let (sent, acked, _) = rows.join().unwrap();
        assert_eq!(acked, sent, "every inbox row was acked");
        assert_eq!(batch.await.unwrap().changed, 5_000);
        // Then, idle, the checkpointer empties the WAL.
        let began = Instant::now();
        while std::fs::metadata(home.path().join("jaw.db-wal")).map_or(1, |m| m.len()) > 0 {
            assert!(began.elapsed() < Duration::from_secs(20), "the idle WAL was never emptied");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let (started, emptied) = assert_ordered(&vfs::noted::events(home.path()));
        assert!(started >= 3 && emptied >= 1, "the WAL started again {started} times and was emptied {emptied}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn off_the_macs_own_drive_every_sync_of_jaw_db_is_a_full_flush() {
        let home = tempfile::tempdir().unwrap();
        vfs::noted::flush_in_full(home.path());
        vfs::noted::watch(home.path());
        let store = Store::open(home.path()).unwrap();
        assert!(store.storage().full_flush, "status says barriers fence jaw.db's checkpoints");
        let chat = "351910000001@s.whatsapp.net";
        store.apply_durable(messages([text(chat, "d1", 1, "done")])).await.unwrap();
        store.truncate().await.unwrap();
        store.apply(Lane::Live, messages([text(chat, "l1", 2, "converted")])).await.unwrap();
        let synced = syncs_since(home.path(), 0, None);
        assert!(synced.iter().any(|s| s.asked & 0x0f == ffi::SQLITE_SYNC_FULL), "no checkpoint synced: {synced:?}");
        let fenced: Vec<_> = synced.iter().filter(|s| s.made != Some(vfs::Made::Full)).collect();
        assert!(fenced.is_empty(), "synced other than in full: {fenced:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_inbox_commit_that_fails_keeps_its_row_and_holds_everything_else_back_until_it_goes_in() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let outside = Connection::open(home.path().join("jaw.db")).unwrap();
        outside.busy_timeout(Duration::from_secs(5)).unwrap();
        outside
            .execute_batch(
                "CREATE TRIGGER inbox_fails BEFORE INSERT ON inbox BEGIN SELECT RAISE(FAIL, 'the disk is full'); END",
            )
            .unwrap();
        let mut trouble = store.trouble();
        store.inbox("message", 7, b"kept".to_vec(), None);
        tokio::time::timeout(Duration::from_secs(10), trouble.wait_for(Option::is_some)).await.unwrap().unwrap();
        assert!(trouble.borrow().as_deref().unwrap().contains("the disk is full"));
        assert!(store.storage().trouble.is_some(), "status says so too");
        let chat = "351910000001@s.whatsapp.net";
        let background = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Background, messages([text(chat, "b", 1_000, "b")])).await })
        };
        let live = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Live, messages([text(chat, "l", 2_000, "l")])).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!background.is_finished() && !live.is_finished(), "nothing is written while the inbox can't be");
        assert_eq!(store.count("inbox").unwrap(), 0);
        outside.execute_batch("DROP TRIGGER inbox_fails").unwrap();
        background.await.unwrap().unwrap();
        live.await.unwrap().unwrap();
        let kept: (String, i64, Vec<u8>) = query(&store, |db| {
            Ok(db.query_row("SELECT kind, at, payload FROM inbox", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)
        })
        .await;
        assert_eq!(kept, ("message".to_string(), 7, b"kept".to_vec()));
        tokio::time::timeout(Duration::from_secs(5), trouble.wait_for(Option::is_none)).await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wal_stays_bounded_while_history_pours_in_and_live_writes_never_wait_for_it() {
        let home = tempfile::tempdir().unwrap();
        let (soft, hard) = (256, 512);
        let store = Store::open_with(home.path(), Limits { wal_soft: soft, wal_hard: hard, ..LIMITS }).unwrap();
        let history: Vec<Message> =
            (0..20_000).map(|n| text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")).collect();
        let batch = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Background, messages(history)).await.unwrap() })
        };
        let mut waited = Vec::new();
        while waited.is_empty() || !batch.is_finished() {
            let (started, live) = (Instant::now(), waited.len() as i64);
            store
                .apply(Lane::Live, messages([text("351910000002@s.whatsapp.net", &format!("l{live}"), live, "live")]))
                .await
                .unwrap();
            waited.push(started.elapsed());
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Live writes kept going in while the history did, however quickly it went: had they
        // waited for it, there would be two, one before it and one after. They're judged on the
        // median, since other processes' load can stall any one, and the slowest on what a wait
        // for the WAL to start again allows, RESTART_MOST and two syncs, with room for that load.
        waited.sort();
        let (median, most) = (waited[waited.len() / 2], waited[waited.len() - 1]);
        let report = format!("{} live writes waited {median:?} at the median, {most:?} at most", waited.len());
        assert!(waited.len() >= 20, "live writes waited for the history: {report}");
        assert!(median < Duration::from_secs(1), "{report}");
        assert!(most < RESTART_MOST + Duration::from_secs(2), "{report}");
        assert_eq!(batch.await.unwrap().changed, 20_000);
        let storage = store.storage();
        assert!(storage.background_waits > 0, "background work never waited for the checkpointer");
        // One background transaction can begin just under the hard limit.
        assert!(storage.wal_peak_frames < hard + 400, "the WAL held {} frames", storage.wal_peak_frames);
        assert!(storage.wal_bytes < (hard + 400) * 4_120, "the WAL file is {} bytes", storage.wal_bytes);
        // The history's text indexed first, so no step of it begins the WAL again as it's emptied.
        store.indexed().await.unwrap();
        store.truncate().await.unwrap();
        assert_eq!(std::fs::metadata(home.path().join("jaw.db-wal")).unwrap().len(), 0);
        assert_eq!(store.count("messages").unwrap(), 20_000 + waited.len() as u64);
    }

    /// Inbox rows handed over about every millisecond until `stop`, each acked once committed: says
    /// how many went, how many were acked within 10 s of the last, and the longest an ack took.
    fn pouring(store: &Store, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<(u64, u64, Duration)> {
        let store = store.clone();
        std::thread::spawn(move || {
            let (acks, mut acked) = mpsc::unbounded_channel();
            let (mut sent, mut received, mut slowest) = (Vec::new(), 0u64, Duration::ZERO);
            let mut stopped: Option<Instant> = None;
            loop {
                while let Ok(crate::engine::Outgoing::Ack(n)) = acked.try_recv() {
                    slowest = slowest.max(sent.get(n as usize).map_or(Duration::ZERO, Instant::elapsed));
                    received += 1;
                }
                if !stop.load(Ordering::Relaxed) {
                    let n = sent.len() as u64;
                    sent.push(Instant::now());
                    store.inbox("message", 0, vec![7; 2_000], Some(crate::engine::Ack::to(n, acks.clone())));
                } else if received == sent.len() as u64
                    || stopped.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(10)
                {
                    return (sent.len() as u64, received, slowest);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    }

    /// Live writes as the converter makes them during a catch-up, ten messages and their inbox
    /// rows' drains each, until `stop`; says how many.
    async fn converting(store: &Store, stop: &AtomicBool) -> i64 {
        let mut written = 0;
        while !stop.load(Ordering::Relaxed) {
            store.apply(Lane::Live, conversion(written * 10, 10)).await.unwrap();
            written += 1;
        }
        written
    }

    /// A live write of the converter's during a catch-up: `count` messages from the `from`th and
    /// their inbox rows' drains, after which indexing waits for the catch-up to end.
    fn conversion(from: i64, count: i64) -> Vec<Change> {
        (from..from + count)
            .flat_map(|n| {
                let chat = format!("3519100{:05}@s.whatsapp.net", n % 300);
                let words = format!("message {n} of a catch-up, with words enough to fill a page or two");
                [Change::Message(text(&chat, &format!("c{n}"), n, &words)), Change::Drained(Drained::Inbox(n))]
            })
            .collect()
    }

    /// Live writes until `until` holds of the store, or for 15 s.
    async fn converting_until(store: &Store, until: impl Fn(&Store) -> bool + Send + 'static) {
        let stop = Arc::new(AtomicBool::new(false));
        let watch = {
            let (store, stop) = (store.clone(), stop.clone());
            tokio::spawn(async move {
                let began = Instant::now();
                while !until(&store) && began.elapsed() < Duration::from_secs(15) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                stop.store(true, Ordering::Relaxed);
            })
        };
        converting(store, &stop).await;
        watch.await.unwrap();
    }

    const SOFT: u64 = 256;
    const HARD: u64 = 512;

    /// A store whose WAL is bounded at SOFT and HARD frames, during a catch-up with nothing else
    /// going on: indexing waits for it to end.
    fn bounded_store(home: &Path) -> Store {
        let quiet = Duration::from_secs(60);
        let limits = Limits { wal_soft: SOFT, wal_hard: HARD, live_pause: quiet, stream_step: quiet, ..LIMITS };
        Store::open_with(home, limits).unwrap()
    }

    /// How often the writer has started the WAL again itself.
    fn restarted(store: &Store) -> usize {
        store.wal.restarts.lock().unwrap().iter().filter(|r| r.restarted).count()
    }

    /// The WAL was started again, each time only once it held HARD frames, since each start costs
    /// live writes three syncs and no background work waited for it, and never held much more
    /// than HARD frames: one transaction can begin just under it.
    fn assert_bounded(store: &Store) {
        let (storage, restarted) = (store.storage(), restarted(store));
        assert!(restarted >= 2, "the WAL started again {restarted} times");
        let early: Vec<u64> =
            store.wal.restarts.lock().unwrap().iter().map(|r| r.frames).filter(|&f| f < HARD).collect();
        assert!(early.is_empty(), "the writer tried to start the WAL again at {early:?} frames");
        assert!(storage.wal_peak_frames < HARD + 400, "the WAL held {} frames", storage.wal_peak_frames);
        assert!(storage.wal_bytes < (HARD + 400) * 4_120, "the WAL file is {} bytes", storage.wal_bytes);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wal_stays_bounded_while_inbox_rows_keep_coming_and_each_is_acked_in_a_moment() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        let stop = Arc::new(AtomicBool::new(false));
        let rows = pouring(&store, stop.clone());
        // Until the WAL has started again twice, however busy the machine, or for 15 s.
        let began = Instant::now();
        while restarted(&store) < 2 && began.elapsed() < Duration::from_secs(15) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        stop.store(true, Ordering::Relaxed);
        let (rows, acked, slowest) = rows.join().unwrap();
        assert_bounded(&store);
        assert_eq!(acked, rows);
        assert!(slowest < Duration::from_secs(2), "an ack of {rows} took {slowest:?}");
        // Each try comes after its group's acks, which would otherwise wait for it.
        let unacked: Vec<usize> = store.wal.restarts.lock().unwrap().iter().map(|r| r.unacked).collect();
        assert!(unacked.iter().all(|&n| n == 0), "the tries began with {unacked:?} rows not yet acked");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_wal_stays_bounded_while_live_writes_keep_coming() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        converting_until(&store, |store| restarted(store) >= 2).await;
        assert_bounded(&store);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn background_work_waiting_for_the_wal_has_it_started_again_at_the_next_live_write_past_wal_soft() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        let history: Vec<Message> =
            (0..20_000).map(|n| text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")).collect();
        let stop = Arc::new(AtomicBool::new(false));
        let batch = {
            let (store, stop) = (store.clone(), stop.clone());
            tokio::spawn(async move {
                let applied = store.apply(Lane::Background, messages(history)).await.unwrap();
                stop.store(true, Ordering::Relaxed);
                applied
            })
        };
        // A catch-up's conversions all the while the history goes in.
        converting(&store, &stop).await;
        assert_eq!(batch.await.unwrap().changed, 20_000);
        let storage = store.storage();
        assert!(storage.background_waits > 0, "the history never waited for the WAL");
        let early: Vec<u64> = store
            .wal
            .restarts
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.restarted && r.frames < HARD)
            .map(|r| r.frames)
            .collect();
        assert!(!early.is_empty(), "the history waited each time until the live writes took the WAL to WAL_HARD");
        assert!(early.iter().all(|&frames| frames >= SOFT), "the WAL started again at {early:?} frames");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reader_on_the_wals_frames_holds_acks_up_only_a_moment_and_the_wal_starts_again_once_it_lets_go() {
        // Room for a try at the edge, which waits up to RESTART_MOST for the checkpointer's pass
        // before its own copy and flushes. Judged on the quickest of three runs, since other
        // processes' I/O can slow the synced commits that drain the rows queued meanwhile.
        let ack_most = Duration::from_secs(2) + RESTART_MOST;
        let mut slowest = Vec::new();
        for _ in 0..3 {
            slowest.push(a_reader_on_the_wals_frames(ack_most).await);
        }
        assert!(*slowest.iter().min().unwrap() < ack_most, "the slowest acks of three runs took {slowest:?}");
    }

    /// One run of the test above, with all but how long acks took checked; says the slowest ack.
    async fn a_reader_on_the_wals_frames(ack_most: Duration) -> Duration {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        let stop = Arc::new(AtomicBool::new(false));
        let rows = pouring(&store, stop.clone());
        let stopping = AtomicBool::new(false);
        let _ = tokio::time::timeout(Duration::from_secs(1), converting(&store, &stopping)).await;
        // A reader whose snapshot needs the WAL's frames, as a long query's would, held until the
        // WAL is well past its bound, and past WAL_EASY frames waiting, so the checkpointer looks
        // again and again behind it as the writer tries to start the WAL again, and for a second
        // longer than an ack may take, so acks held up for the whole hold would show.
        let reader = Connection::open_with_flags(home.path().join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        let (held_from, flushed) = (Instant::now(), store.wal.flushed.load(Ordering::Relaxed));
        converting_until(&store, move |store| {
            store.storage().wal_frames > 2 * WAL_EASY && held_from.elapsed() > ack_most + Duration::from_secs(1)
        })
        .await;
        let (held, flushes) = (store.storage().wal_peak_frames, store.wal.flushed.load(Ordering::Relaxed) - flushed);
        let tries: Vec<_> =
            store.wal.restarts.lock().unwrap().iter().filter(|t| t.began >= held_from).copied().collect();
        let restarted_held = restarted(&store);
        reader.execute_batch("COMMIT").unwrap();
        drop(reader);
        let _ = tokio::time::timeout(Duration::from_secs(2), converting(&store, &stopping)).await;
        stop.store(true, Ordering::Relaxed);
        let (rows, acked, slowest) = rows.join().unwrap();
        let storage = store.storage();
        assert_eq!(acked, rows);
        assert!(held > 2 * WAL_EASY, "the reader never held the WAL past its bound: {held} frames");
        assert!(tries.len() >= 2 && tries.iter().all(|t| !t.restarted), "{tries:?} behind the reader");
        // The WAL grew from short of WAL_HARD to past the edge.
        let edges: Vec<bool> = tries.iter().map(|t| t.at_edge).collect();
        assert!(edges.contains(&false) && edges.contains(&true), "the tries were at the edge {edges:?}");
        assert!(flushes <= 2, "the checkpointer flushed jaw.db {flushes} times behind the reader");
        for pair in tries.windows(2) {
            let after = pair[1].began - pair[0].ended;
            assert!(after >= RESTART_AGAIN, "a try came {after:?} after one the reader held up");
        }
        assert!(restarted(&store) > restarted_held, "the WAL didn't start again once the reader let go");
        assert!(storage.wal_frames < HARD + 400, "the WAL is {} frames once the reader let go", storage.wal_frames);
        slowest
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn once_the_writer_has_started_the_wal_again_it_waits_its_whole_busy_timeout_for_a_lock() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        converting_until(&store, |store| restarted(store) > 0).await;
        assert!(restarted(&store) > 0, "the WAL never started again");
        let mut trouble = store.trouble();
        trouble.borrow_and_update();
        let holder = Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let live = {
            let store = store.clone();
            let after = text("351910000001@s.whatsapp.net", "after", 1, "after the restart");
            tokio::spawn(async move { store.apply(Lane::Live, messages([after])).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        holder.execute_batch("COMMIT").unwrap();
        live.await.unwrap().unwrap();
        assert!(!trouble.has_changed().unwrap(), "a lock held 300 ms was trouble: {:?}", trouble.borrow());
    }

    #[test]
    fn a_restart_refused_the_checkpoint_lock_waits_for_the_pass_only_at_the_edge_and_not_for_long() {
        let tries = |at_edge: bool, refusals: u32, then: (bool, i64, i64)| {
            let mut tries = 0;
            let tried = restarting(Instant::now(), at_edge, || {
                tries += 1;
                Ok(if tries <= refusals { (true, -1, -1) } else { then })
            })
            .unwrap();
            (tried, tries)
        };
        assert_eq!(tries(true, 3, (false, 30, 30)), ((false, 30, 30), 4), "at the edge it waits for the pass");
        assert_eq!(tries(false, 3, (false, 30, 30)), ((true, -1, -1), 1), "short of it the next commit tries");
        assert_eq!(tries(true, 0, (true, 30, 20)), ((true, 30, 20), 1), "a reader is waited for once");
        let began = Instant::now();
        let refused = restarting(began, true, || {
            anyhow::ensure!(began.elapsed() < Duration::from_secs(5), "still trying after 5 s");
            Ok((true, -1, -1))
        })
        .unwrap();
        assert_eq!(refused, (true, -1, -1));
        assert!(began.elapsed() < RESTART_MOST + Duration::from_millis(500), "it gave up after {:?}", began.elapsed());
    }

    #[test]
    fn a_restart_refused_the_checkpoint_lock_is_tried_again_at_the_next_commit_and_one_behind_a_reader_later() {
        assert_eq!(restart_again((false, 30, 30)), None, "it started again");
        assert_eq!(restart_again((true, -1, -1)), None, "the checkpointer's pass held the lock");
        assert_eq!(restart_again((true, 30, 20)), Some(RESTART_AGAIN), "a reader held frames 21 to 30");
        assert_eq!(restart_again((true, 30, 30)), Some(RESTART_AGAIN), "a reader kept it from starting again");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reader_that_lets_go_while_a_restart_waits_for_it_doesnt_keep_the_wal_from_starting_again() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        let (mut n, mut seen) = (0, 0);
        for _ in 0..20 {
            // Short of WAL_HARD by less than the write that follows the reader adds.
            while !(HARD - 64..HARD).contains(&store.storage().wal_frames) {
                store.apply(Lane::Live, conversion(n, 1)).await.unwrap();
                n += 1;
            }
            let reader =
                Connection::open_with_flags(home.path().join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            reader.execute_batch("BEGIN").unwrap();
            let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
            let tries = store.wal.restarts.lock().unwrap().len();
            store.apply(Lane::Live, conversion(n, 500)).await.unwrap();
            n += 500;
            tokio::time::sleep(Duration::from_millis(2)).await;
            let letting_go = Instant::now();
            reader.execute_batch("COMMIT").unwrap();
            let let_go = Instant::now();
            drop(reader);
            let began = Instant::now();
            while store.wal.restarts.lock().unwrap().len() == tries && began.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let Some(tried) = store.wal.restarts.lock().unwrap().get(tries).copied() else { continue };
            // Only a try that began while the reader held its snapshot, and saw it let go well
            // within RESTART_READERS, says whether it waited for it.
            if tried.began < letting_go && let_go < tried.began + RESTART_READERS * 3 / 4 {
                let into = let_go - tried.began;
                assert!(tried.restarted, "a reader that let go {into:?} into the try kept the WAL from starting again");
                seen += 1;
                if seen == 3 {
                    break;
                }
            }
        }
        assert!(seen > 0, "no try began while the reader held its snapshot and saw it let go");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn background_work_behind_a_reader_that_held_restarts_up_gets_room_once_it_lets_go_with_nothing_committing() {
        let home = tempfile::tempdir().unwrap();
        let store = bounded_store(home.path());
        let history: Vec<Message> =
            (0..20_000).map(|n| text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")).collect();
        let batch = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Background, messages(history)).await.unwrap() })
        };
        let began = Instant::now();
        while !store.wal.held.load(Ordering::Relaxed) {
            assert!(began.elapsed() < Duration::from_secs(20), "the history never waited for the WAL");
            tokio::time::sleep(Duration::from_micros(200)).await;
        }
        // A reader whose snapshot the live writes after it leave behind, as a search's would.
        let reader = Connection::open_with_flags(home.path().join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        // Live writes, far enough apart for the writer to try to start the WAL again after each,
        // until more frames wait behind the reader than the writer copies itself, so background
        // work waits without a commit that would wake the checkpointer; then until the
        // checkpointer's pass was refused while the last try held the lock waiting for the reader;
        // then nothing.
        let (mut write, mut behind) = (0, false);
        loop {
            assert!(write < 100, "no try held the checkpointer's pass up behind the reader");
            let tries = store.wal.restarts.lock().unwrap().len();
            store.apply(Lane::Live, conversion(write * 10, 10)).await.unwrap();
            write += 1;
            tokio::time::sleep(RESTART_READERS + RESTART_AGAIN + Duration::from_millis(20)).await;
            let refused = store.wal.refused.lock().unwrap().clone();
            let held_up = store.wal.restarts.lock().unwrap()[tries..]
                .iter()
                .any(|t| t.held_up && refused.iter().any(|&at| (t.began..=t.ended).contains(&at)));
            let waiting = store.wal.waiting() > WAL_NEAR && store.wal.held.load(Ordering::Relaxed);
            if behind && waiting && held_up {
                break;
            }
            behind = waiting;
        }
        reader.execute_batch("COMMIT").unwrap();
        drop(reader);
        let let_go = Instant::now();
        while store.wal.held.load(Ordering::Relaxed) && let_go.elapsed() < IDLE / 5 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert!(
            !store.wal.held.load(Ordering::Relaxed),
            "background work still waited {:?} after the reader let go, as if for the checkpointer's idle timeout",
            let_go.elapsed()
        );
        assert_eq!(batch.await.unwrap().changed, 20_000);
    }

    /// A store whose WAL has room for the checkpointer's passes, each once WAL_EASY frames wait,
    /// while the writer appends, and whose passes sync as slowly as on a busy drive, since a pass
    /// fenced rather than flushed is over too soon for the writer to land a commit in it reliably.
    fn roomy_store(home: &Path) -> Store {
        vfs::noted::slow(home, Duration::from_millis(25));
        let quiet = Duration::from_secs(60);
        let limits =
            Limits { wal_soft: 8 * WAL_EASY, wal_hard: 12 * WAL_EASY, live_pause: quiet, stream_step: quiet, ..LIMITS };
        Store::open_with(home, limits).unwrap()
    }

    /// One live write of messages enough to fill WAL_EASY frames and more.
    fn burst(from: i64, messages: i64) -> Vec<Change> {
        (from..from + messages)
            .map(|n| {
                let chat = format!("3519200{:05}@s.whatsapp.net", n % 300);
                let words = format!("message {n} of a burst, {}", "with words enough to fill a page or two ".repeat(8));
                Change::Message(text(&chat, &format!("b{n}"), n, &words))
            })
            .collect()
    }

    /// How often the checkpointer has flushed jaw.db, once it hasn't for 300 ms.
    async fn flushes_settled(store: &Store) -> u64 {
        let (began, mut last) = (Instant::now(), (store.wal.flushed.load(Ordering::Relaxed), Instant::now()));
        while began.elapsed() < Duration::from_secs(10) {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let flushed = store.wal.flushed.load(Ordering::Relaxed);
            if flushed != last.0 {
                last = (flushed, Instant::now());
            } else if last.1.elapsed() >= Duration::from_millis(300) {
                break;
            }
        }
        last.0
    }

    /// Live writes all the while, and among them bursts long enough to copy for one of those
    /// writes to land in their pass, until the checkpointer has flushed jaw.db after one, or for
    /// twelve bursts: it takes three, however busy the machine, and a busy machine makes each
    /// burst slower, so counting bursts rather than seconds leaves it nothing to run out of.
    async fn outrunning_a_pass(store: &Store) {
        let stop = Arc::new(AtomicBool::new(false));
        let writes = {
            let (store, stop) = (store.clone(), stop.clone());
            tokio::spawn(async move { converting(&store, &stop).await })
        };
        let (mut bursts, mut n) = (0, 100_000);
        while store.wal.flushed.load(Ordering::Relaxed) == 0 && bursts < 12 {
            store.apply(Lane::Live, burst(n, 4_000)).await.unwrap();
            (bursts, n) = (bursts + 1, n + 4_000);
        }
        stop.store(true, Ordering::Relaxed);
        writes.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_checkpointer_flushes_jaw_db_after_a_pass_the_writer_outran_and_not_after_one_that_reached_the_end() {
        let home = tempfile::tempdir().unwrap();
        let store = roomy_store(home.path());
        outrunning_a_pass(&store).await;
        let flushed = flushes_settled(&store).await;
        assert!(flushed > 0, "the checkpointer never flushed jaw.db while the writer appended");
        // A burst it copies with nothing appended meanwhile, so SQLite flushes jaw.db at the end.
        store.apply(Lane::Live, burst(0, 8_000)).await.unwrap();
        let began = Instant::now();
        while store.storage().frames_waiting > 0 && began.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(store.storage().frames_waiting, 0, "the burst wasn't copied");
        assert_eq!(flushes_settled(&store).await, flushed, "it flushed jaw.db after SQLite had");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pass_a_reader_holds_back_flushes_jaw_db_once_not_each_time_it_looks_again() {
        let home = tempfile::tempdir().unwrap();
        let store = roomy_store(home.path());
        store.apply(Lane::Live, burst(0, 100)).await.unwrap();
        let reader = Connection::open_with_flags(home.path().join("jaw.db"), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        let flushed = flushes_settled(&store).await;
        store.apply(Lane::Live, burst(100, 8_000)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let (held, waiting) = (store.wal.flushed.load(Ordering::Relaxed) - flushed, store.storage().frames_waiting);
        reader.execute_batch("COMMIT").unwrap();
        assert!(waiting >= WAL_EASY, "the reader didn't hold the checkpointer back: {waiting} frames waited");
        assert!(held <= 1, "it flushed jaw.db {held} times behind the reader");
    }

    const LOCK_DB: &str = "JAW_STORE_TEST_LOCK_DB";

    /// Whether a process of its own, this test binary again, can take jaw.db at `path` for writing
    /// now, as a `sqlite3` beside jawd would to empty the WAL and delete it as it closed. It can't
    /// while any connection of this process's holds jaw.db's shared lock.
    fn taken_elsewhere(path: &Path) -> bool {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["store::tests::a_process_beside_jawd_takes_jaw_db_if_it_can", "--exact", "--ignored", "--nocapture"])
            .env(LOCK_DB, path)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(said.contains("TOOK") || said.contains("REFUSED"), "the other process said {said}");
        said.contains("TOOK")
    }

    /// What `taken_elsewhere` runs in a process of its own; run on its own, it does nothing.
    #[test]
    #[ignore = "run by taken_elsewhere, in a process of its own"]
    fn a_process_beside_jawd_takes_jaw_db_if_it_can() {
        let Ok(path) = std::env::var(LOCK_DB) else { return };
        let file = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
        // SQLite's shared lock is a read lock on the 510 bytes after its pending byte and the one
        // after that, and a writer's exclusive lock a write lock on them.
        // SAFETY: a flock is plain data, all zeros a valid one.
        let mut lock: libc::flock = unsafe { std::mem::zeroed() };
        lock.l_type = libc::F_WRLCK as _;
        lock.l_whence = libc::SEEK_SET as _;
        lock.l_start = 0x4000_0002;
        lock.l_len = 510;
        // SAFETY: fcntl only reads the flock, and changes nothing but this file's locks.
        let took = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &raw const lock) } == 0;
        println!("{}", if took { "TOOK" } else { "REFUSED" });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_checkpointer_flushes_jaw_db_without_letting_go_of_this_processs_locks_on_it() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let store = roomy_store(home.path());
        // A connection beside the store's, as a reader that outlives the writer is, holding
        // jaw.db's shared lock.
        let beside = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let _: i64 = beside.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        assert!(!taken_elsewhere(&path), "another process took jaw.db from under this one's connections");
        outrunning_a_pass(&store).await;
        assert!(store.wal.flushed.load(Ordering::Relaxed) > 0, "the checkpointer never flushed jaw.db");
        // The writer and the checkpointer each let go of the WAL's state as they end, once the
        // store has gone.
        let wal = store.wal.clone();
        drop(store);
        let began = Instant::now();
        while Arc::strong_count(&wal) > 1 {
            assert!(began.elapsed() < Duration::from_secs(10), "the writer and the checkpointer never ended");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!taken_elsewhere(&path), "another process took jaw.db once the checkpointer had ended");
        drop(beside);
    }

    /// Each sync of a file whose methods `sync_recorded` stands in for, with its flags.
    static SYNCS: Mutex<Vec<(usize, c_int)>> = Mutex::new(Vec::new());
    static SYNC: OnceLock<unsafe extern "C" fn(*mut ffi::sqlite3_file, c_int) -> c_int> = OnceLock::new();

    unsafe extern "C" fn sync_recorded(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
        SYNCS.lock().unwrap().push((file as usize, flags));
        // SAFETY: the file's own sync, which this stands in for, on the file it was asked for.
        unsafe { SYNC.get().expect("the file's own sync")(file, flags) }
    }

    #[test]
    fn the_checkpointers_flush_is_a_checkpoints_sync_of_jaw_db_itself() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("jaw.db");
        let _writer = connect(&path, Role::Writer).unwrap();
        let db = connect(&path, Role::Checkpointer).unwrap();
        let mut file: *mut ffi::sqlite3_file = std::ptr::null_mut();
        // SAFETY: as in `flush`.
        let found = unsafe {
            ffi::sqlite3_file_control(
                db.handle(),
                c"main".as_ptr(),
                ffi::SQLITE_FCNTL_FILE_POINTER,
                (&raw mut file).cast(),
            )
        };
        assert_eq!(found, ffi::SQLITE_OK);
        // SAFETY: the file is open, and its methods are its VFS's, which last as long as the process.
        let (own, mut recording) = unsafe { ((*file).pMethods, *(*file).pMethods) };
        SYNC.get_or_init(|| recording.xSync.expect("jaw.db's file syncs"));
        recording.xSync = Some(sync_recorded);
        // SAFETY: only this thread uses `db`, and its file has its own methods back before
        // `recording` goes.
        unsafe { (*file).pMethods = &raw const recording };
        let flushed = flush(&db);
        // SAFETY: as above.
        unsafe { (*file).pMethods = own };
        flushed.unwrap();
        let syncs = std::mem::take(&mut *SYNCS.lock().unwrap());
        assert_eq!(
            syncs,
            [(file as usize, ffi::SQLITE_SYNC_FULL)],
            "the flush didn't ask jaw.db's own file for a checkpoint's sync, at FULL"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn background_work_waiting_for_the_wal_gets_room_with_nothing_else_committing() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open_with(home.path(), Limits { wal_soft: 256, wal_hard: 512, ..LIMITS }).unwrap();
        // Batches apart, so the checkpointer has looked at each commit before the next batch waits.
        for batch in 0..50 {
            let history = (0..100).map(|n| {
                let n = batch * 100 + n;
                text("351910000001@s.whatsapp.net", &format!("h{n}"), n, "history")
            });
            assert_eq!(store.apply(Lane::Background, messages(history)).await.unwrap().changed, 100);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let storage = store.storage();
        assert!(storage.background_waits > 0, "background work never waited for the checkpointer");
        assert!(
            u128::from(storage.background_waited_ms) < (IDLE / 2).as_millis(),
            "background work waited {} ms in {} waits, as if for the checkpointer's idle timeout",
            storage.background_waited_ms,
            storage.background_waits
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn indexing_for_search_waiting_for_the_wal_gets_room_with_nothing_else_committing() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open_with(home.path(), Limits { wal_soft: 256, wal_hard: 512, ..LIMITS }).unwrap();
        let history = (0..10_000).map(|n| {
            let words = format!("history h{n}x with words for search, número {} and more", n * 7919 % 10_007);
            text("351910000001@s.whatsapp.net", &format!("h{n}"), n, &words)
        });
        store.apply(Lane::Background, messages(history)).await.unwrap();
        let waits = store.storage().background_waits;
        // Indexed with nothing else committing, so only the writer's index steps wake the
        // checkpointer whenever they pass the soft limit.
        indexed_with_short_waits(&store).await;
        let waits = store.storage().background_waits - waits;
        assert!(waits > 0, "indexing never waited for the checkpointer");
        let found = query(&store, |db| read::search(db, "numero 7919", None, None, 10)).await;
        assert_eq!(found.hits.len(), 1, "the history is found once it's indexed");
    }

    /// How long a test waits for what it's waiting for before calling it a hang: what it checks is
    /// each step's own bound, never how long a busy machine takes over all of them.
    const HANG: Duration = Duration::from_secs(300);

    /// Waits for fts_backlog to be indexed, however long a busy machine takes over it, checking
    /// each of the writer's waits for the checkpointer as it ends: every one lasts a pass of the
    /// checkpointer's, not the idle timeout after which it empties the WAL by itself. Only a hang
    /// is timed, at HANG.
    async fn indexed_with_short_waits(store: &Store) {
        let longest = || store.stats.lock().expect("the writer's stats").held.most().unwrap_or(0);
        let indexing = tokio::spawn({
            let store = store.clone();
            async move { store.indexed().await }
        });
        let began = Instant::now();
        loop {
            let done = indexing.is_finished();
            let most = longest();
            assert!(most < IDLE.as_micros() as u64 / 2, "a wait for the checkpointer took {most} µs");
            if done {
                break;
            }
            let waits = store.storage().background_waits;
            assert!(
                began.elapsed() < HANG,
                "indexing hadn't finished after {HANG:?}, in {waits} waits for the checkpointer"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        indexing.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_idle_store_empties_its_wal_and_then_its_checkpointer_sleeps_until_the_next_commit() {
        let home = tempfile::tempdir().unwrap();
        let idle = Duration::from_millis(50);
        let store = Store::open_with(home.path(), Limits { idle, ..LIMITS }).unwrap();
        let wal = home.path().join("jaw.db-wal");
        for round in 0..2 {
            let id = format!("m{round}");
            store.apply(Lane::Live, messages([text("351910000002@s.whatsapp.net", &id, round, "hi")])).await.unwrap();
            store.indexed().await.unwrap();
            let committed = Instant::now();
            while std::fs::metadata(&wal).map_or(0, |m| m.len()) > 0 {
                assert!(committed.elapsed() < Duration::from_secs(10), "the WAL was never emptied");
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let woke = store.wal.woke.load(Ordering::Relaxed);
            tokio::time::sleep(idle * 10).await;
            // Indexing ends with a merge step that writes nothing but still kicks the checkpointer: on a
            // busy machine it can commit after the WAL was emptied, and the idle timer it starts runs out once.
            let woken = store.wal.woke.load(Ordering::Relaxed) - woke;
            assert!(woken <= 1, "the checkpointer woke {woken} times with nothing to do");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn live_writes_go_in_with_a_stream_of_inbox_rows_and_are_never_held_back_by_it() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (acks, mut acked) = tokio::sync::mpsc::unbounded_channel();
        // 500 rows a second, each flush shorter than the gap to the next row's commit, for 2 s and
        // then on until 20 live writes have gone in while they came, however long a busy machine
        // takes over them, or for HANG.
        let (wanted, made) = (20, Arc::new(AtomicU64::new(0)));
        let feeder = {
            let (store, made) = (store.clone(), made.clone());
            std::thread::spawn(move || {
                let started = Instant::now();
                let mut n = 0;
                while n < 1_000 || (made.load(Ordering::Relaxed) < wanted && started.elapsed() < HANG) {
                    if let Some(wait) = (started + Duration::from_millis(2 * n)).checked_duration_since(Instant::now())
                    {
                        std::thread::sleep(wait);
                    }
                    store.inbox("message", 0, vec![1], Some(crate::engine::Ack::to(n, acks.clone())));
                    n += 1;
                }
                n
            })
        };
        let chat = "351910000001@s.whatsapp.net";
        let (mut live, mut slowest) = (0, Duration::ZERO);
        while !feeder.is_finished() {
            let started = Instant::now();
            let message = text(chat, &format!("l{live}"), live, "live");
            let applied = if live % 2 == 0 {
                store.apply_durable(vec![Change::Message(message), Change::Person(Person::default())]).await.unwrap()
            } else {
                store.apply(Lane::Live, messages([message])).await.unwrap()
            };
            slowest = slowest.max(started.elapsed());
            assert_eq!(applied.quarantined, usize::from(live % 2 == 0), "write {live}");
            assert_eq!(applied.error.is_some(), live % 2 == 0, "write {live} says why only when it was quarantined");
            live += 1;
            made.store(live as u64, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let rows = feeder.join().unwrap();
        assert!(live as u64 >= wanted, "only {live} live writes in {HANG:?} of rows");
        assert!(slowest < Duration::from_millis(250), "a live write waited {slowest:?} for the stream of rows");
        for _ in 0..rows {
            let ack = tokio::time::timeout(Duration::from_secs(5), acked.recv()).await.unwrap();
            assert!(matches!(ack, Some(crate::engine::Outgoing::Ack(_))));
        }
        assert_eq!(store.count("inbox").unwrap(), rows);
        assert_eq!(store.count("messages").unwrap(), live as u64, "each live write's message, and nothing twice");
        let kept: i64 =
            query(&store, |db| Ok(db.query_row("SELECT count(*) FROM quarantine", [], |r| r.get(0))?)).await;
        assert_eq!(
            kept, 1,
            "the durable writes' bad change, kept once however often it came, while their messages went in"
        );
        let events: Vec<i64> =
            query(&store, |db| read::events(db, 0, 1_000)).await.into_iter().map(|e| e.seq).collect();
        assert!(events.windows(2).all(|w| w[1] == w[0] + 1), "no seq is skipped or given twice: {events:?}");
    }

    /// A full disk inside an inbox group's synced commit, with durable live writes taken into it,
    /// the failing one first or after another: the row goes again alone, and so does each write,
    /// in order, each answered once it's made; none is dropped, and none is made twice.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_full_disk_inside_an_inbox_commit_drops_none_of_the_durable_writes_in_it_and_answers_each() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000001@s.whatsapp.net";
        for (case, order) in [("first", ["fails", "a", "b"]), ("between", ["a", "fails", "b"])] {
            // The writer takes an inbox row and waits on a lock held outside, so the writes queue
            // behind it and go into its commit.
            let outside = Connection::open(home.path().join("jaw.db")).unwrap();
            outside.execute_batch("BEGIN IMMEDIATE").unwrap();
            store.inbox("message", 0, vec![1], None);
            tokio::time::sleep(Duration::from_millis(100)).await;
            let failed_before = FAILED_SHARED.load(Ordering::Relaxed);
            let mut answers = Vec::new();
            for what in order {
                let changes = match what {
                    "fails" => vec![Change::Remember { key: FAIL_SHARED.into(), value: case.into() }],
                    _ => messages([text(chat, &format!("{case}-{what}"), 1_000, what)]),
                };
                let (done, applied) = oneshot::channel();
                store.live.send(Live::Changes { changes, done, queued: Instant::now(), durable: true }).unwrap();
                answers.push((what, applied));
            }
            outside.execute_batch("COMMIT").unwrap();
            for (what, applied) in answers {
                let applied = tokio::time::timeout(Duration::from_secs(10), applied)
                    .await
                    .unwrap_or_else(|_| panic!("{case}: {what} was never answered"))
                    .unwrap_or_else(|_| panic!("{case}: {what} was dropped, its caller told the writer stopped"))
                    .unwrap_or_else(|e| panic!("{case}: {what} failed: {e}"));
                assert_eq!(applied.quarantined, 0, "{case}: {what}");
            }
            if case == "first" {
                assert!(FAILED_SHARED.load(Ordering::Relaxed) > failed_before, "the inbox's commit failed with it");
            }
            assert_eq!(store.remembered(FAIL_SHARED).unwrap().as_deref(), Some(case), "{case}: the failing write made");
        }
        let kept: Vec<String> = query(&store, |db| {
            Ok(db
                .prepare("SELECT id FROM messages ORDER BY n")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
        assert_eq!(kept, ["first-a", "first-b", "between-a", "between-b"], "each write's message once, in order");
        assert_eq!(store.count("inbox").unwrap(), 2, "each inbox row once");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn inbox_commits_that_fail_with_live_writes_in_them_ack_nothing_early_and_make_each_write_once_in_order() {
        use super::outbox::{Enqueued, Step, Written};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (acks, mut acked) = tokio::sync::mpsc::unbounded_channel();
        let rows = 1_500u64;
        let feeder = {
            let store = store.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                for n in 0..rows {
                    if let Some(wait) = (started + Duration::from_millis(2 * n)).checked_duration_since(Instant::now())
                    {
                        std::thread::sleep(wait);
                    }
                    store.inbox("message", 0, vec![1; 600], Some(crate::engine::Ack::to(n, acks.clone())));
                }
            })
        };
        let checker = {
            let store = store.clone();
            tokio::spawn(async move {
                let (mut seen, mut early) = (0u64, 0u64);
                while seen < rows {
                    let ack = tokio::time::timeout(Duration::from_secs(10), acked.recv()).await.unwrap();
                    assert!(matches!(ack, Some(crate::engine::Outgoing::Ack(_))));
                    seen += 1;
                    if store.count("inbox").unwrap() < seen {
                        early += 1;
                    }
                }
                early
            })
        };
        let (chat, to) = ("351910000001@s.whatsapp.net", "351910000002@s.whatsapp.net");
        let failed_before = FAILED_SHARED.load(Ordering::Relaxed);
        let (mut answers, mut sends) = (Vec::new(), Vec::new());
        let mut i = 0i64;
        // Six live writes at a time straight into the live queue, three of them durable; two of
        // them fail any inbox commit they're taken into, and one of those also sends a message.
        while !feeder.is_finished() {
            for kind in 0..6 {
                let durable = matches!(kind, 0 | 2 | 5);
                let mut changes = vec![
                    Change::Message(text(chat, &format!("w{i:05}"), 1_000 + i, "live")),
                    Change::Remember { key: "order".into(), value: i.to_string() },
                ];
                // The send goes before the change that fails, so a rolled-back attempt says what it
                // wrote before the attempt that stands says again.
                if kind == 2 {
                    let mine = Message {
                        mine: true,
                        sender: None,
                        status: Some("pending"),
                        ..text(to, &format!("s{i:05}"), 1_000 + i, "sent")
                    };
                    let written = Written::default();
                    changes.push(Change::Outbox(Step::Enqueue {
                        message: Box::new(mine),
                        kind: "text",
                        raw: vec![1],
                        key: None,
                        written: written.clone(),
                    }));
                    sends.push((i, written));
                }
                if kind == 2 || kind == 4 {
                    changes.push(Change::Remember { key: FAIL_SHARED.into(), value: "x".into() });
                }
                let (done, applied) = oneshot::channel();
                store.live.send(Live::Changes { changes, done, queued: Instant::now(), durable }).unwrap();
                answers.push((i, applied));
                i += 1;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
        let mut seqs = Vec::new();
        for (i, applied) in answers {
            let applied = applied.await.unwrap().unwrap();
            assert_eq!(applied.quarantined, 0, "write {i}");
            seqs.push(applied.seq);
        }
        assert_eq!(checker.await.unwrap(), 0, "acks seen before their rows could be read");
        assert!(FAILED_SHARED.load(Ordering::Relaxed) > failed_before, "no shared commit failed");
        assert!(store.storage().shared_failed > 0 && store.storage().shared_writes > 0);
        assert_eq!(store.count("inbox").unwrap(), rows, "each row once");
        let made: Vec<(String, i64)> = query(&store, |db| {
            let mut made = db.prepare(
                "SELECT m.id, e.seq FROM messages m JOIN events e ON e.type = 'message' AND e.key = m.n
                WHERE m.id LIKE 'w%' ORDER BY m.n",
            )?;
            Ok(made.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await;
        assert_eq!(made.len() as i64, i, "each write's message once");
        for (n, (id, seq)) in made.iter().enumerate() {
            assert_eq!(id, &format!("w{n:05}"), "writes made in the order they came");
            assert!(*seq <= seqs[n], "write {n}'s answer, seq {}, is before its event, {seq}", seqs[n]);
        }
        assert!(made.windows(2).all(|w| w[0].1 < w[1].1), "events in the writes' order");
        assert!(seqs.windows(2).all(|w| w[0] <= w[1]), "answers' seqs never go back");
        let order: String =
            query(&store, |db| Ok(db.query_row("SELECT value FROM sync_state WHERE key = 'order'", [], |r| r.get(0))?))
                .await;
        assert_eq!(order, (i - 1).to_string(), "the last write's value stands");
        let events: Vec<i64> =
            query(&store, |db| read::events(db, 0, 100_000)).await.into_iter().map(|e| e.seq).collect();
        assert!(events.windows(2).all(|w| w[1] == w[0] + 1), "no seq skipped or given twice");
        assert_eq!(store.count("outbox").unwrap(), sends.len() as u64, "each send queued once");
        for (i, written) in sends {
            let Some(Enqueued::Queued { message: Some(message), outbox: Some(outbox), .. }) = written.take() else {
                panic!("send {i} says nothing was queued");
            };
            let kept: (String, i64) = query(&store, move |db| {
                Ok(db.query_row(
                    "SELECT m.id, o.message FROM messages m, outbox o WHERE m.n = ?1 AND o.n = ?2",
                    [message, outbox],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .await;
            assert_eq!(kept, (format!("s{i:05}"), message), "send {i} says what the commit that kept it wrote");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_durable_write_waiting_for_an_inbox_commit_that_fails_before_taking_it_is_made_once_after() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (acks, mut acked) = tokio::sync::mpsc::unbounded_channel();
        let rows = 1_000u64;
        // Rows whose insert fails while a durable write waits for their commit, and goes in alone.
        let feeder = {
            let store = store.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                for n in 0..rows {
                    if let Some(wait) = (started + Duration::from_millis(2 * n)).checked_duration_since(Instant::now())
                    {
                        std::thread::sleep(wait);
                    }
                    store.inbox(FAIL_BEFORE_FIRST, 0, vec![1; 600], Some(crate::engine::Ack::to(n, acks.clone())));
                }
            })
        };
        let checker = {
            let store = store.clone();
            tokio::spawn(async move {
                let (mut seen, mut early) = (0u64, 0u64);
                while seen < rows {
                    let ack = tokio::time::timeout(Duration::from_secs(10), acked.recv()).await.unwrap();
                    assert!(matches!(ack, Some(crate::engine::Outgoing::Ack(_))));
                    seen += 1;
                    if store.count("inbox").unwrap() < seen {
                        early += 1;
                    }
                }
                early
            })
        };
        let chat = "351910000001@s.whatsapp.net";
        let failed_before = FAILED_BEFORE_FIRST.load(Ordering::Relaxed);
        let mut answers = Vec::new();
        let mut i = 0i64;
        // A conversion, a durable write and another conversion at a time, so the writer makes the
        // first between two groups and hands the durable one to the next group's commit.
        while !feeder.is_finished() {
            for durable in [false, true, false] {
                let id = format!("w{i:05}");
                let changes = vec![Change::Message(text(chat, &id, 1_000 + i, "live"))];
                let (done, applied) = oneshot::channel();
                store.live.send(Live::Changes { changes, done, queued: Instant::now(), durable }).unwrap();
                let store = store.clone();
                answers.push(tokio::spawn(async move {
                    let applied = applied.await.expect("a live write was dropped").unwrap();
                    let kept: i64 = query(&store, move |db| {
                        Ok(db.query_row("SELECT count(*) FROM messages WHERE id = ?1", [&id], |r| r.get(0))?)
                    })
                    .await;
                    (applied, kept)
                }));
                i += 1;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut seqs = Vec::new();
        for (n, answer) in answers.into_iter().enumerate() {
            let (applied, kept) = answer.await.unwrap();
            assert_eq!((applied.quarantined, kept), (0, 1), "write {n} answered before it was kept");
            seqs.push(applied.seq);
        }
        assert_eq!(checker.await.unwrap(), 0, "acks seen before their rows could be read");
        assert!(FAILED_BEFORE_FIRST.load(Ordering::Relaxed) > failed_before, "no commit failed before its write");
        assert_eq!(store.count("inbox").unwrap(), rows, "each row once");
        let made: Vec<(String, i64)> = query(&store, |db| {
            let mut made = db.prepare(
                "SELECT m.id, e.seq FROM messages m JOIN events e ON e.type = 'message' AND e.key = m.n ORDER BY m.n",
            )?;
            Ok(made.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await;
        assert_eq!(made.len() as i64, i, "each write's message once");
        for (n, (id, seq)) in made.iter().enumerate() {
            assert_eq!(id, &format!("w{n:05}"), "writes made in the order they came");
            assert!(*seq <= seqs[n], "write {n}'s answer, seq {}, is before its event, {seq}", seqs[n]);
        }
        let events: Vec<i64> =
            query(&store, |db| read::events(db, 0, 100_000)).await.into_iter().map(|e| e.seq).collect();
        assert!(events.windows(2).all(|w| w[1] == w[0] + 1), "no seq skipped or given twice");
    }

    /// Where `chat` was read, its unread count, and whether WhatsApp is owed receipts for it.
    async fn read_position(store: &Store, chat: &'static str) -> (Option<i64>, Option<i64>, i64, bool) {
        query(store, move |db| {
            Ok(db.query_row(
                "SELECT read_at, read_n, unread, told_at IS NOT NULL FROM chats WHERE id = ?1",
                [chat],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?)
        })
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_rolled_back_into_quarantine_leaves_where_its_chat_was_read() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000009@s.whatsapp.net";
        let theirs = Message { live: true, ..text(chat, "o1", 1_000, "hello") };
        store.apply(Lane::Live, messages([theirs])).await.unwrap();
        let before = read_position(&store, chat).await;
        assert_eq!(before.2, 1);
        // Your own message, live from another device, in a group whose last change fails.
        let mine = Message { mine: true, sender: None, live: true, ..text(chat, "x", 2_000, "mine") };
        let group =
            vec![Change::Message(mine), Change::Person(Person::default()), Change::Drained(Drained::Inbox(999_999))];
        assert_eq!(store.apply(Lane::Live, group.clone()).await.unwrap().quarantined, 1);
        assert_eq!(
            read_position(&store, chat).await,
            before,
            "a rolled-back message of yours moved where the chat was read"
        );
        // And in a transaction where a group before it moved the chat, and stays.
        let read = Message { mine: true, sender: None, live: true, ..text(chat, "y", 1_500, "read") };
        let mut changes = vec![Change::Message(read), Change::Drained(Drained::Inbox(999_998))];
        changes.extend(group);
        assert_eq!(store.apply(Lane::Live, changes).await.unwrap().quarantined, 1);
        let after = read_position(&store, chat).await;
        assert_eq!((after.0, after.2), (Some(1_500), 0), "read up to the message that stayed, and no further");
        let ids: Vec<String> = query(&store, move |db| read::messages(db, chat, Around::Newest, 10))
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["o1", "y"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_send_whose_group_is_rolled_back_owes_no_receipts_and_reads_nothing() {
        use super::outbox::{Step, Written, owing};
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let chat = "351910000008@s.whatsapp.net";
        let theirs = Message { live: true, ..text(chat, "o1", 1_000, "hello") };
        store.apply(Lane::Live, messages([theirs])).await.unwrap();
        let before = read_position(&store, chat).await;
        let mine = Message { mine: true, sender: None, status: Some("pending"), ..text(chat, "s1", 2_000, "sent") };
        let send = Step::Enqueue {
            message: Box::new(mine),
            kind: "text",
            raw: vec![1],
            key: None,
            written: Written::default(),
        };
        let group =
            vec![Change::Outbox(send), Change::Person(Person::default()), Change::Drained(Drained::Inbox(999_999))];
        assert_eq!(store.apply_durable(group).await.unwrap().quarantined, 1);
        assert_eq!(read_position(&store, chat).await, before, "a send rolled back moved where the chat was read");
        assert_eq!(store.count("outbox").unwrap(), 0);
        assert!(query(&store, owing).await.is_empty(), "receipts owed for a send that was rolled back");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn writes_that_meet_a_lock_held_past_the_busy_timeout_are_tried_again_and_all_land() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open_with(home.path(), Limits { busy_ms: 50, ..LIMITS }).unwrap();
        let holder = Connection::open(home.path().join("jaw.db")).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut trouble = store.trouble();
        let chat = "351910000001@s.whatsapp.net";
        let live = {
            let store = store.clone();
            tokio::spawn(async move { store.apply(Lane::Live, messages([text(chat, "l", 1_000, "live")])).await })
        };
        let durable = {
            let store = store.clone();
            tokio::spawn(async move { store.apply_durable(messages([text(chat, "d", 2_000, "done")])).await })
        };
        store.inbox("message", 3, b"kept".to_vec(), None);
        tokio::time::timeout(Duration::from_secs(10), trouble.wait_for(Option::is_some)).await.unwrap().unwrap();
        assert!(trouble.borrow().as_deref().unwrap().contains("locked"), "{:?}", trouble.borrow());
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!live.is_finished() && !durable.is_finished(), "ten busy timeouts later, nothing has given up");
        holder.execute_batch("COMMIT").unwrap();
        live.await.unwrap().unwrap();
        durable.await.unwrap().unwrap();
        let ids: Vec<String> = query(&store, move |db| read::messages(db, chat, Around::Newest, 10))
            .await
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, ["l", "d"]);
        let kept: Vec<u8> = query(&store, |db| Ok(db.query_row("SELECT payload FROM inbox", [], |r| r.get(0))?)).await;
        assert_eq!(kept, b"kept");
        tokio::time::timeout(Duration::from_secs(5), trouble.wait_for(Option::is_none)).await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_backup_taken_before_an_upgrade_stays_for_its_time_and_goes_at_a_clean_stop_after() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let backup = home.path().join("jaw.db.before-v2");
        std::fs::write(&backup, b"a backup").unwrap();
        std::fs::write(home.path().join("jaw.db.before-v3.part"), b"half a backup").unwrap();
        assert_eq!(store.remove_old_backups().unwrap(), 1, "only the half-written one goes");
        assert!(backup.is_file(), "a backup younger than its time stays");
        let shown = store.storage().backups;
        assert_eq!(shown.len(), 1);
        assert_eq!((shown[0].file.as_str(), shown[0].bytes), ("jaw.db.before-v2", 8));
        let taken = std::time::SystemTime::now() - schema::KEEP_BACKUP - Duration::from_secs(60);
        std::fs::File::options().write(true).open(&backup).unwrap().set_modified(taken).unwrap();
        assert_eq!(store.remove_old_backups().unwrap(), 1);
        assert!(!backup.exists() && home.path().join("jaw.db").is_file());
        assert!(store.storage().backups.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_setting_is_kept_once_with_a_synced_commit_and_read_back_by_its_domain_and_key() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        assert_eq!(store.setting("jaw", "theme").unwrap(), None, "a fresh jaw.db holds no settings");
        let keep = |domain: &str, value: &str, at| Change::Settings {
            domain: domain.into(),
            values: vec![("theme".into(), value.into())],
            at,
        };
        let synced = store.storage().synced_commits;
        store.apply_durable(vec![keep("jaw", "\"gruvbox\"", 1)]).await.unwrap();
        assert_eq!(store.storage().synced_commits, synced + 1);
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        assert_eq!(store.setting("jaw-mac", "theme").unwrap(), None, "each domain is its own");
        store.apply_durable(vec![keep("jaw", "\"gruvbox\"", 2)]).await.unwrap();
        let at: i64 = query(&store, |db| {
            Ok(db.query_row("SELECT at FROM settings WHERE domain = 'jaw' AND key = 'theme'", [], |r| r.get(0))?)
        })
        .await;
        assert_eq!(at, 1, "the same value again changes nothing, not even when it was chosen");
        store.apply_durable(vec![keep("jaw-mac", "\"nord\"", 3)]).await.unwrap();
        assert_eq!(store.setting("jaw-mac", "theme").unwrap().as_deref(), Some("\"nord\""));
        assert_eq!(
            store.setting("jaw", "theme").unwrap().as_deref(),
            Some("\"gruvbox\""),
            "another domain's leaves it"
        );
        let refused = store.apply_durable(vec![keep("jaw", "not json", 4)]).await.unwrap();
        assert_eq!(refused.quarantined, 1, "a value that isn't JSON is refused by the table's check");
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""));
        let both = |messages: &str, at| Change::Settings {
            domain: "jaw".into(),
            values: vec![("theme".into(), "\"nord\"".into()), ("messages".into(), messages.into())],
            at,
        };
        let refused = store.apply_durable(vec![both("not json", 5)]).await.unwrap();
        assert_eq!(refused.quarantined, 1);
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"gruvbox\""), "kept all or none");
        assert_eq!(store.setting("jaw", "messages").unwrap(), None);
        store.apply_durable(vec![both("\"irc\"", 6)]).await.unwrap();
        assert_eq!(store.setting("jaw", "theme").unwrap().as_deref(), Some("\"nord\""));
        assert_eq!(store.setting("jaw", "messages").unwrap().as_deref(), Some("\"irc\""));
        let at: i64 = query(&store, |db| {
            Ok(db.query_row("SELECT at FROM settings WHERE domain = 'jaw' AND key = 'theme'", [], |r| r.get(0))?)
        })
        .await;
        assert_eq!(at, 6, "a value that changes is chosen anew");
    }

    #[test]
    fn the_settings_table_is_strict_and_holds_json_and_a_time() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let insert = |key: &str, value: &str, at: rusqlite::types::Value| {
            db.execute(
                "INSERT INTO settings (domain, key, value, at) VALUES ('jaw', ?1, ?2, ?3)",
                rusqlite::params![key, value, at],
            )
        };
        assert!(insert("theme", "\"nord\"", 1.into()).is_ok());
        assert!(insert("messages", "irc", 1.into()).is_err(), "a value that isn't JSON");
        assert!(insert("font", "\"x\"", "soon".to_string().into()).is_err(), "a time that isn't an integer");
        assert!(insert("size", "\"x\"", rusqlite::types::Value::Null).is_err(), "no time");
    }

    #[test]
    fn the_settings_table_holds_names_of_1_to_64_bytes_and_values_of_4_kib_at_most() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        let insert = |domain: &str, key: &str, value: &str| {
            db.execute(
                "INSERT INTO settings (domain, key, value, at) VALUES (?1, ?2, ?3, 1)",
                rusqlite::params![domain, key, value],
            )
        };
        assert!(
            insert(&"d".repeat(64), &"k".repeat(64), &format!("\"{}\"", "x".repeat(4_094))).is_ok(),
            "64, 64 and 4,096 bytes"
        );
        assert!(insert("", "theme", "\"x\"").is_err(), "no domain");
        assert!(insert(&"d".repeat(65), "theme", "\"x\"").is_err(), "a domain of 65 bytes");
        assert!(insert("jaw", "", "\"x\"").is_err(), "no key");
        assert!(insert("jaw", &"k".repeat(65), "\"x\"").is_err(), "a key of 65 bytes");
        assert!(insert("jaw", "long", &format!("\"{}\"", "x".repeat(4_095))).is_err(), "4,097 bytes");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_settings_change_that_fails_is_quarantined_with_its_json_so_it_can_be_read_back() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let values = vec![("theme".to_string(), "\"nord\"".to_string()), ("messages".into(), "not json".into())];
        let change = Change::Settings { domain: "jaw".into(), values: values.clone(), at: 1 };
        assert_eq!(store.apply_durable(vec![change]).await.unwrap().quarantined, 1);
        let (source, reference, raw): (String, String, Option<Vec<u8>>) = query(&store, |db| {
            Ok(db.query_row("SELECT source, ref, raw FROM quarantine", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)
        })
        .await;
        assert_eq!((source.as_str(), reference.as_str()), ("settings", "jaw"));
        let kept: serde_json::Value = serde_json::from_slice(&raw.expect("the change kept as its JSON")).unwrap();
        assert_eq!(kept["domain"], "jaw");
        let read: Vec<(String, String)> = serde_json::from_value(kept["values"].clone()).unwrap();
        assert_eq!(read, values, "every setting the change carried, each value as it was asked");
    }
}
