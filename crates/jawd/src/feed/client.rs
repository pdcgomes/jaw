//! A connection's windows. It keeps, for each window, the hash of every row its interface holds;
//! changes from the feed mark the windows they touch, and a flush reads each marked window again
//! and sends only the rows whose hash changed and the ids that left. What it keeps is bounded by
//! its windows however far behind its interface is, and the lines it sends go into a bounded
//! queue it never waits on: when the queue is full the windows stay marked, and once the queue
//! has had no room for `Timing::behind` they're dropped and the interface is told to watch again.
//! A window whose read fails stays marked and is read again at its usual pace, and one that has
//! failed for `Timing::behind` is dropped the same way.
//!
//! Connections watching the same window (the same folder or chat, anchor and stretch) share its
//! reads: a flush takes the window's last read if it has every change the connection has heard
//! of, or the read under way if that was asked for since, or else asks for one that the others can
//! take in turn, and each connection diffs it against its own hashes. So a commit costs a read for
//! each distinct window, however many interfaces watch it.
//!
//! A window's last read is kept for the connections that haven't taken it yet, each row as the
//! JSON it's sent as. The reads kept are counted in bytes, and past `KEPT_MOST` the oldest are let
//! go, after which a window asks for a read again; a connection watches at most `MOST_WINDOWS`.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Instant;

use jaw_proto::windows::{
    Around, ChatRow, ChatsPush, ChatsWatch, ChatsWatched, Favourite, FolderRow, FoldersPush, FoldersWatch,
    FoldersWatched, MessageRow, MessagesPush, MessagesWatch, MessagesWatched, RosterRow, RosterWatch, Watch, WatchMove,
};
use jaw_proto::{Error, Windows, code};
use rusqlite::Connection;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::value::RawValue;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::{mpsc, watch};

use super::window::{
    self, ChatAnchor, ChatSpec, MessageAnchor, MessageSpec, PersonAnchor, RosterSpec, Rows, Snapshot, Spec,
};
use super::{CatchingUp, Changes, Timing};
use crate::store::lists::{self, List};
use crate::store::{Store, read};
use crate::work::Priority;

/// The most rows a window keeps on either side of its anchor.
const MOST_ROWS: u32 = 1_000;
/// The most windows a connection can watch at once.
pub(super) const MOST_WINDOWS: usize = 32;
/// The most bytes the reads kept for windows to share can hold: about 30 windows of 1,000 short
/// messages, at half a kilobyte a row, or a couple of hundred of the size jaw shows.
const KEPT_MOST: u64 = 16 << 20;

/// What a client waits for.
pub enum Ready {
    Changes(Result<Arc<Changes>, RecvError>),
    Flush,
    /// The queue to the interface has room again.
    Room,
    /// The queue has had no room for too long.
    Behind,
}

pub struct Client {
    store: Store,
    changes: Option<broadcast::Receiver<Arc<Changes>>>,
    catching_up: CatchingUp,
    timing: Timing,
    out: mpsc::Sender<Vec<u8>>,
    shared: Arc<Shared>,
    /// The seq hello said the interface last saw, for the watches it sends again.
    since: Option<u64>,
    /// The newest change it has heard of from the feed.
    heard: i64,
    windows: BTreeMap<String, Window>,
    next: u64,
    /// Since when a flush has waited for room in the queue.
    blocked: Option<Instant>,
    /// Whether a reply is waiting for room ahead of anything its windows would send.
    held: bool,
    /// Watches dropped for falling behind, owed a `resync`.
    resync: Vec<String>,
}

/// What every connection shares: each distinct window's reads, the bytes of those kept, and how
/// many windows are watched and read.
pub(super) struct Shared {
    /// Window reads asked for, and the times a window took instead a read its slot kept, or one
    /// under way.
    reads: AtomicU64,
    taken: AtomicU64,
    joined: AtomicU64,
    /// Windows watched on every connection.
    windows: AtomicU64,
    /// The bytes the slots' kept reads hold, the most they may, and the kept reads let go to stay
    /// under it.
    kept: Arc<AtomicU64>,
    kept_most: AtomicU64,
    let_go: AtomicU64,
    /// Orders kept reads, so the oldest are let go first.
    stamps: AtomicU64,
    slots: Mutex<HashMap<Spec, Weak<Slot>>>,
    /// Whose presence each window shows, by the window's own number, and a count of the changes.
    showing: Mutex<HashMap<u64, Showing>>,
    shown: watch::Sender<u64>,
    windows_made: AtomicU64,
    /// Makes every window read fail on its reader, as a broken row would.
    #[cfg(test)]
    pub(super) failing: std::sync::atomic::AtomicBool,
    /// Holds each window read's answer back this many milliseconds after its snapshot is taken.
    #[cfg(test)]
    pub(super) slow: AtomicU64,
    /// Makes the next shared read's task panic once its read has answered.
    #[cfg(test)]
    pub(super) panicking: std::sync::atomic::AtomicBool,
    /// Stops the next flush that isn't held, once it has a window due and before it looks for room
    /// in the queue: it notifies the first and waits for the second.
    #[cfg(test)]
    pub(super) pausing: Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            reads: AtomicU64::default(),
            taken: AtomicU64::default(),
            joined: AtomicU64::default(),
            windows: AtomicU64::default(),
            kept: Arc::default(),
            kept_most: AtomicU64::new(KEPT_MOST),
            let_go: AtomicU64::default(),
            stamps: AtomicU64::default(),
            slots: Mutex::default(),
            showing: Mutex::default(),
            shown: watch::channel(0).0,
            windows_made: AtomicU64::default(),
            #[cfg(test)]
            failing: Default::default(),
            #[cfg(test)]
            slow: AtomicU64::default(),
            #[cfg(test)]
            panicking: Default::default(),
            #[cfg(test)]
            pausing: Mutex::default(),
        }
    }
}

/// The reads of one window, shared by every connection watching it, and held by each of their
/// windows: it goes once none is.
struct Slot {
    spec: Spec,
    state: Mutex<SlotState>,
}

#[derive(Default)]
struct SlotState {
    /// The newest read that succeeded, unless it was let go to keep the bytes kept under the most.
    kept: Option<Kept>,
    /// The read under way, if any.
    reading: Option<Reading>,
    asked: u64,
}

/// A read a slot keeps for its windows, its bytes counted with every slot's for as long as it's
/// kept.
struct Kept {
    found: Arc<Found>,
    stamp: u64,
    bytes: Arc<AtomicU64>,
}

impl Drop for Kept {
    fn drop(&mut self) {
        self.bytes.fetch_sub(self.found.bytes, Ordering::Relaxed);
    }
}

type Answer = Option<Result<Arc<Found>, Error>>;

struct Reading {
    /// Which read it is, so an older one ending doesn't clear a newer.
    asked: u64,
    /// The feed's last committed seq when it was asked for, so it has every change up to it.
    from: i64,
    answer: watch::Receiver<Answer>,
}

/// A read under way, forgotten by its slot when its task ends however it ends, so a task that
/// panicked leaves no read for windows to join and wait on until they're resynced.
struct UnderWay {
    slot: Arc<Slot>,
    asked: u64,
}

impl Drop for UnderWay {
    fn drop(&mut self) {
        let mut state = self.slot.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.reading.as_ref().is_some_and(|reading| reading.asked == self.asked) {
            state.reading = None;
        }
    }
}

/// A window as one read found it, each row as it's sent.
struct Found {
    /// It has every change up to this seq: the last committed when it was asked for, or the newest
    /// event it saw if that's later.
    fresh: i64,
    seq: i64,
    total: u64,
    older: bool,
    newer: u64,
    rows: Vec<Line>,
    chat: Option<i64>,
    senders: HashSet<i64>,
    named: HashSet<u64>,
    /// A folder tree's favourite slots, as they're sent, and their hash.
    favourites: Option<(u64, Box<RawValue>)>,
    /// Whose presence it shows.
    people: Vec<i64>,
    /// The window as it was read, its anchor where the read found it.
    spec: Spec,
    /// The bytes it holds, about.
    bytes: u64,
}

/// A row as a read found it: the JSON it's sent as, the hash of that, and what a connection needs
/// to know whether its interface holds it so.
struct Line {
    n: i64,
    hash: u64,
    id: String,
    sort: String,
    v: u64,
    json: Box<RawValue>,
}

impl Line {
    fn of(n: i64, row: &impl Row) -> Line {
        let json = serde_json::value::to_raw_value(row).expect("jawd's own types serialize");
        let mut hasher = DefaultHasher::new();
        hasher.write(json.get().as_bytes());
        Line { n, hash: hasher.finish(), id: row.id().to_string(), sort: row.sort().to_string(), v: row.v(), json }
    }

    fn bytes(&self) -> usize {
        size_of::<Line>() + self.id.capacity() + self.sort.capacity() + self.json.get().len()
    }
}

/// Whose presence a window shows: an open one-to-one chat's person, ahead of a roster's people,
/// which come in the roster's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Showing {
    pub chat: bool,
    pub people: Vec<i64>,
}

impl Shared {
    pub(super) fn reads(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }

    /// Whose presence the windows show, open chats' people first, each once.
    pub(super) fn showing(&self) -> Vec<i64> {
        let showing = self.showing.lock().expect("the windows' people");
        let mut seen = HashSet::new();
        let (chats, rosters): (Vec<&Showing>, Vec<&Showing>) = showing.values().partition(|s| s.chat);
        let mut out = Vec::new();
        for showing in chats.into_iter().chain(rosters) {
            out.extend(showing.people.iter().copied().filter(|n| seen.insert(*n)));
        }
        out
    }

    pub(super) fn shown(&self) -> watch::Receiver<u64> {
        self.shown.subscribe()
    }

    fn show(&self, window: u64, now: Showing) {
        let mut showing = self.showing.lock().expect("the windows' people");
        let changed = if now.people.is_empty() {
            showing.remove(&window).is_some()
        } else {
            showing.insert(window, now.clone()).as_ref() != Some(&now)
        };
        drop(showing);
        if changed {
            self.shown.send_modify(|n| *n += 1);
        }
    }

    /// The windows watched, and the reads kept for them.
    pub(super) fn windows(&self) -> Windows {
        let distinct =
            self.slots.lock().expect("the feed's slots").values().filter(|slot| slot.strong_count() > 0).count();
        Windows {
            watched: self.windows.load(Ordering::Relaxed),
            distinct: distinct as u64,
            most: MOST_WINDOWS as u64,
            kept_bytes: self.kept.load(Ordering::Relaxed),
            kept_most: self.kept_most.load(Ordering::Relaxed),
            let_go: self.let_go.load(Ordering::Relaxed),
            reads: self.reads(),
            taken: self.taken.load(Ordering::Relaxed),
            joined: self.joined.load(Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(super) fn keep_at_most(&self, bytes: u64) {
        self.kept_most.store(bytes, Ordering::Relaxed);
    }

    #[cfg(test)]
    async fn pause(&self) {
        let pausing = self.pausing.lock().expect("the test's pause").take();
        if let Some((stopped, go)) = pausing {
            stopped.notify_one();
            go.notified().await;
        }
    }

    /// The slot for windows showing `spec`, made if none holds one.
    fn slot(&self, spec: &Spec) -> Arc<Slot> {
        let mut slots = self.slots.lock().expect("the feed's slots");
        if let Some(slot) = slots.get(spec).and_then(Weak::upgrade) {
            return slot;
        }
        slots.retain(|_, slot| slot.strong_count() > 0);
        let slot = Arc::new(Slot { spec: spec.clone(), state: Mutex::default() });
        slots.insert(spec.clone(), Arc::downgrade(&slot));
        slot
    }

    /// `found` as a slot keeps it, counted among the bytes kept.
    fn keep(&self, found: Arc<Found>) -> Kept {
        self.kept.fetch_add(found.bytes, Ordering::Relaxed);
        Kept { found, stamp: self.stamps.fetch_add(1, Ordering::Relaxed), bytes: self.kept.clone() }
    }

    /// Lets go of the oldest reads slots keep while they hold more bytes than the most. A window
    /// whose slot keeps none asks for a read of its own, which the others watching it can share,
    /// and it never takes one older than what its interface holds, so nothing depends on them.
    fn trim(&self) {
        let most = self.kept_most.load(Ordering::Relaxed);
        if self.kept.load(Ordering::Relaxed) <= most {
            return;
        }
        let slots: Vec<Arc<Slot>> =
            self.slots.lock().expect("the feed's slots").values().filter_map(Weak::upgrade).collect();
        let mut kept: Vec<(u64, Arc<Slot>)> = slots
            .into_iter()
            .filter_map(|slot| {
                let stamp = slot.state.lock().expect("a window's reads").kept.as_ref()?.stamp;
                Some((stamp, slot))
            })
            .collect();
        kept.sort_unstable_by_key(|(stamp, _)| *stamp);
        for (stamp, slot) in kept {
            if self.kept.load(Ordering::Relaxed) <= most {
                break;
            }
            let mut state = slot.state.lock().expect("a window's reads");
            if state.kept.as_ref().is_some_and(|kept| kept.stamp == stamp) {
                state.kept = None;
                self.let_go.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// The query that reads `spec`, or just the chats `only` names.
    fn query(
        &self,
        spec: Spec,
        only: Option<Vec<i64>>,
    ) -> impl FnOnce(&Connection) -> anyhow::Result<(Snapshot, Spec)> + Send + 'static {
        #[cfg(test)]
        let failing = self.failing.load(Ordering::Relaxed);
        #[cfg(test)]
        let slow = self.slow.load(Ordering::Relaxed);
        move |db| {
            #[cfg(test)]
            anyhow::ensure!(!failing, "the test made window reads fail");
            let read = window::query(db, spec, only);
            #[cfg(test)]
            std::thread::sleep(std::time::Duration::from_millis(slow));
            read
        }
    }
}

/// What a read of the window found, asked for when `from` was the last committed seq.
fn found(snapshot: Snapshot, spec: Spec, from: i64) -> Found {
    let rows: Vec<Line> = match &snapshot.rows {
        Rows::Chats(rows) => rows.iter().map(|(n, row)| Line::of(*n, row)).collect(),
        Rows::Messages(rows) => rows.iter().map(|(n, row)| Line::of(*n, row)).collect(),
        Rows::Folders(rows, _) => rows.iter().map(|(n, row)| Line::of(*n, row)).collect(),
        Rows::People(rows) => rows.iter().map(|(n, row)| Line::of(*n, row)).collect(),
    };
    let favourites = match &snapshot.rows {
        Rows::Folders(_, favourites) => {
            let json = serde_json::value::to_raw_value(favourites).expect("jawd's own types serialize");
            let mut hasher = DefaultHasher::new();
            hasher.write(json.get().as_bytes());
            Some((hasher.finish(), json))
        }
        _ => None,
    };
    let bytes = size_of::<Found>()
        + rows.iter().map(Line::bytes).sum::<usize>()
        + snapshot.senders.capacity() * size_of::<i64>()
        + snapshot.named.capacity() * size_of::<u64>();
    Found {
        fresh: from.max(snapshot.seq),
        seq: snapshot.seq,
        total: snapshot.total,
        older: snapshot.older,
        newer: snapshot.newer,
        rows,
        chat: snapshot.chat,
        senders: snapshot.senders,
        named: snapshot.named,
        favourites,
        people: snapshot.people,
        spec,
        bytes: bytes as u64,
    }
}

struct Window {
    spec: Spec,
    slot: Arc<Slot>,
    _watched: Watched,
    /// Its own number, for whose presence it shows.
    number: u64,
    state: State,
    /// When the first and the latest changes waiting to be sent came, if any are, and the newest
    /// one's seq.
    first: Option<Instant>,
    last: Option<Instant>,
    owed: Option<i64>,
    /// When it was last read, so the next read waits at least `Timing::burst`, whether or not that
    /// one sent anything.
    read: Option<Instant>,
    /// Since when its reads have failed.
    failing: Option<Instant>,
    /// When a chat list last took its order from a fresh query, and whether it owes one.
    reordered: Instant,
    reorder: bool,
}

/// Counts a window among every connection's for as long as it's watched.
struct Watched(Arc<Shared>);

impl Watched {
    fn new(shared: &Arc<Shared>) -> Watched {
        shared.windows.fetch_add(1, Ordering::Relaxed);
        Watched(shared.clone())
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.0.windows.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self._watched.0.show(self.number, Showing::default());
    }
}

/// What a window's interface holds, and what jawd needs to know which changes touch it.
#[derive(Default)]
struct State {
    held: HashMap<String, Held>,
    /// The read it was settled from has every change up to this seq.
    fresh: i64,
    total: u64,
    older: bool,
    newer: u64,
    /// A messages window's chat, by n, once it exists.
    chat: Option<i64>,
    senders: HashSet<i64>,
    /// The JIDs its rows mention or quote someone by, as the read it was settled from has them.
    named: HashSet<u64>,
    /// The hash of the favourite slots a folder tree's interface holds.
    favourites: Option<u64>,
    people: Vec<i64>,
}

/// A row as its interface holds it.
struct Held {
    /// Of the row as sent; 0 for a row the interface says it has, which nothing matches.
    hash: u64,
    sort: String,
    n: i64,
}

/// What an interface needs to go from what it holds to a read, the rows as the read keeps them.
struct Update<'a> {
    kind: Kind,
    seq: u64,
    total: u64,
    older: bool,
    newer: u64,
    upsert: Vec<&'a RawValue>,
    remove: Vec<String>,
    favourites: Option<&'a RawValue>,
}

/// What kind of window an update is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Chats,
    Messages,
    Folders,
    Roster,
}

impl Kind {
    fn of(spec: &Spec) -> Kind {
        match spec {
            Spec::Chats(_) => Kind::Chats,
            Spec::Messages(_) => Kind::Messages,
            Spec::Folders => Kind::Folders,
            Spec::Roster(_) => Kind::Roster,
        }
    }
}

trait Row: Serialize {
    fn id(&self) -> &str;
    fn sort(&self) -> &str;
    fn v(&self) -> u64;
}

impl Row for ChatRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.sort
    }
    fn v(&self) -> u64 {
        self.v
    }
}

impl Row for MessageRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.sort
    }
    fn v(&self) -> u64 {
        self.v
    }
}

impl Row for FolderRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
}

impl Row for RosterRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.sort
    }
    fn v(&self) -> u64 {
        self.v
    }
}

/// Whether two sets have anything in common, looking up the smaller's in the larger.
fn meet<T: Eq + Hash>(a: &HashSet<T>, b: &HashSet<T>) -> bool {
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    small.iter().any(|item| large.contains(item))
}

/// The rows of `rows` an interface holding `held` lacks or holds otherwise, leaving out those it
/// holds that haven't changed since `since`; the ids it holds that aren't in `rows`; and what it
/// holds afterwards.
fn changed<'a>(
    held: &HashMap<String, Held>,
    rows: &'a [Line],
    since: Option<i64>,
) -> (Vec<&'a RawValue>, Vec<String>, HashMap<String, Held>) {
    let mut now = HashMap::with_capacity(rows.len());
    let mut upsert = Vec::new();
    for line in rows {
        let same = held
            .get(&line.id)
            .is_some_and(|held| held.hash == line.hash || since.is_some_and(|since| line.v as i64 <= since));
        now.insert(line.id.clone(), Held { hash: line.hash, sort: line.sort.clone(), n: line.n });
        if !same {
            upsert.push(&*line.json);
        }
    }
    let mut remove: Vec<String> = held.keys().filter(|id| !now.contains_key(*id)).cloned().collect();
    remove.sort();
    (upsert, remove, now)
}

/// What an interface holding `held` needs to hold what `found` found, and what it holds then.
/// The folder tree's rows have no versions its counts keep, so it's always compared row by row.
fn settle<'a>(
    held: &HashMap<String, Held>,
    favourites: Option<u64>,
    found: &'a Found,
    since: Option<i64>,
) -> (Update<'a>, State) {
    let kind = Kind::of(&found.spec);
    let since = since.filter(|_| kind != Kind::Folders);
    let (upsert, remove, held) = changed(held, &found.rows, since);
    let update = Update {
        kind,
        seq: found.seq.max(0) as u64,
        total: found.total,
        older: found.older,
        newer: found.newer,
        upsert,
        remove,
        favourites: found.favourites.as_ref().filter(|(hash, _)| favourites != Some(*hash)).map(|(_, json)| &**json),
    };
    let state = State {
        held,
        fresh: found.fresh,
        total: found.total,
        older: found.older,
        newer: found.newer,
        chat: found.chat,
        senders: found.senders.clone(),
        named: found.named.clone(),
        favourites: found.favourites.as_ref().map(|(hash, _)| *hash),
        people: found.people.clone(),
    };
    (update, state)
}

/// `update` as a watch's or a move's result.
fn result(watch: String, update: Update) -> Value {
    let Update { kind, seq, total, older, newer, upsert: rows, remove, favourites } = update;
    let value = match kind {
        Kind::Chats | Kind::Roster => serde_json::to_value(ChatsWatched { watch, seq, total, rows, remove }),
        Kind::Messages => serde_json::to_value(MessagesWatched { watch, seq, total, older, newer, rows, remove }),
        Kind::Folders => {
            let favourites: Vec<Favourite> =
                favourites.map(|json| serde_json::from_str(json.get()).unwrap_or_default()).unwrap_or_default();
            serde_json::to_value(FoldersWatched { watch, seq, rows, remove, favourites })
        }
    };
    value.expect("jawd's own types serialize")
}

/// `update` as a push to an interface holding `was`, unless nothing in it changed.
fn push(watch: String, update: Update, was: &State) -> Option<Vec<u8>> {
    let Update { kind, seq, total, older, newer, upsert, remove, favourites } = update;
    let total = (total != was.total).then_some(total);
    match kind {
        Kind::Chats | Kind::Roster => {
            if upsert.is_empty() && remove.is_empty() && total.is_none() {
                return None;
            }
            let method = if kind == Kind::Chats { "chats" } else { "roster" };
            return Some(line(method, ChatsPush { watch, seq, total, upsert, remove }));
        }
        Kind::Folders => {
            if upsert.is_empty() && remove.is_empty() && favourites.is_none() {
                return None;
            }
            let favourites: Option<Vec<Favourite>> =
                favourites.map(|json| serde_json::from_str(json.get()).unwrap_or_default());
            return Some(line("folders", FoldersPush { watch, seq, upsert, remove, favourites }));
        }
        Kind::Messages => {}
    }
    let older = (older != was.older).then_some(older);
    let newer = (newer != was.newer).then_some(newer);
    if upsert.is_empty() && remove.is_empty() && total.is_none() && older.is_none() && newer.is_none() {
        return None;
    }
    Some(line("messages", MessagesPush { watch, seq, total, older, newer, upsert, remove }))
}

/// A push, as `jaw_proto::Push` carries one, with its params written as they are rather than made
/// a `Value` first, so the rows reads keep as JSON go as they are.
#[derive(Serialize)]
struct Pushed<'a, P> {
    method: &'a str,
    params: P,
}

fn line(method: &str, params: impl Serialize) -> Vec<u8> {
    let mut line = serde_json::to_vec(&Pushed { method, params }).expect("jawd's own types serialize");
    line.push(b'\n');
    line
}

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn sides(before: u32, after: u32) -> Result<(usize, usize), Error> {
    if before > MOST_ROWS || after > MOST_ROWS {
        return Err(Error::new(
            code::INVALID_PARAMS,
            format!("a window keeps at most {MOST_ROWS} rows on each side of its anchor"),
        ));
    }
    Ok((before as usize, after as usize))
}

fn list(folder: &str) -> Result<List, Error> {
    List::parse(folder).ok_or_else(|| {
        let mut error = Error::new(code::INVALID_PARAMS, format!("{folder} isn't a folder's ID"));
        error.data = Some(serde_json::json!({ "param": "folder" }));
        error
    })
}

fn unknown(watch: &str) -> Error {
    Error::new(code::NOT_FOUND, format!("this connection has no watch {watch}"))
}

async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// Waits for room in the queue; a queue whose interface has gone never has any.
async fn room(out: Option<mpsc::Sender<Vec<u8>>>) {
    if let Some(out) = out
        && out.reserve_owned().await.is_ok()
    {
        return;
    }
    std::future::pending().await
}

async fn recv(changes: &mut Option<broadcast::Receiver<Arc<Changes>>>) -> Result<Arc<Changes>, RecvError> {
    match changes {
        Some(changes) => changes.recv().await,
        None => std::future::pending().await,
    }
}

/// Runs `query` on a reader, ahead of background reads.
async fn read_on<T: Send + 'static>(
    store: &Store,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Error> {
    store
        .reads()
        .query(Priority::Interactive, query)
        .await
        .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't read the window: {e:#}")))
}

impl Window {
    fn new(spec: Spec, slot: Arc<Slot>, shared: &Arc<Shared>) -> Window {
        let now = Instant::now();
        Window {
            spec,
            slot,
            _watched: Watched::new(shared),
            number: shared.windows_made.fetch_add(1, Ordering::Relaxed),
            state: State::default(),
            first: None,
            last: None,
            owed: None,
            read: None,
            failing: None,
            reordered: now,
            reorder: false,
        }
    }

    /// Notes a change waiting since `at`, which the feed's events reach `seq` with.
    fn wait(&mut self, at: Instant, seq: i64) {
        self.first = Some(self.first.map_or(at, |first| first.min(at)));
        self.last = Some(self.last.map_or(at, |last| last.max(at)));
        self.owed = Some(self.owed.map_or(seq, |owed| owed.max(seq)));
    }

    /// The seq a read must have every change up to for this window to take it: every change its
    /// connection has `heard` of, since those that didn't mark it were judged by the rows it held
    /// and may touch the rows a newer read brings, and never older than what its interface holds.
    fn owed(&self, heard: i64) -> i64 {
        self.owed.unwrap_or(heard).max(heard).max(self.state.fresh)
    }

    /// When the changes waiting are due: the first after the window has gone unread for `burst` at
    /// once, so a lone message goes as soon as it's committed; those that follow it once the feed
    /// has been quiet for `quiet` or the first has waited `burst`, and never sooner than `burst`
    /// after the last read. Every read counts, so a stream of changes the window doesn't show (a
    /// count or a name further down the list) costs at most one read each `burst`.
    fn due(&self, timing: &Timing) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        match self.read {
            Some(read) if first < read + timing.burst => {
                Some((last + timing.quiet).min(first + timing.burst).max(read + timing.burst))
            }
            _ => Some(first),
        }
    }

    fn touched_by(&self, changes: &Changes) -> bool {
        match (&self.spec, self.state.chat) {
            (Spec::Chats(_) | Spec::Folders | Spec::Roster(_), _) => {
                !changes.chats.is_empty() || !changes.people.is_empty() || changes.folders
            }
            (Spec::Messages(_), Some(chat)) => {
                changes.messages.contains(&chat)
                    || changes.chats.contains(&chat)
                    || meet(&self.state.senders, &changes.people)
                    || meet(&self.state.named, &changes.jids)
            }
            (Spec::Messages(_), None) => !changes.chats.is_empty(),
        }
    }

    /// Whether it may take a new order now: always, except a chat list while catching up, which
    /// reorders once each `hold`.
    fn may_reorder(&self, now: Instant, holding: bool, timing: &Timing) -> bool {
        !holding || !matches!(self.spec, Spec::Chats(_)) || now >= self.reordered + timing.hold
    }

    /// When it owes a reorder it may take, as of `now`: paced by its reads like its changes, so a
    /// reorder whose read fails waits `burst` before it's tried again.
    fn reorder_at(&self, now: Instant, holding: bool, timing: &Timing) -> Option<Instant> {
        let at = match self.reorder {
            true if holding => self.reordered + timing.hold,
            true => now,
            false => return None,
        };
        Some(self.read.map_or(at, |read| at.max(read + timing.burst)))
    }

    /// Takes what its interface now holds, from a read that succeeded; `reordered` says when, if
    /// it took a new order.
    fn keep(&mut self, state: State, reordered: Option<Instant>) {
        let showing = Showing { chat: matches!(self.spec, Spec::Messages(_)), people: state.people.clone() };
        self._watched.0.show(self.number, showing);
        self.state = state;
        self.first = None;
        self.last = None;
        self.owed = None;
        self.failing = None;
        match reordered {
            Some(at) => {
                self.reordered = at;
                self.reorder = false;
            }
            None => self.reorder = true,
        }
    }
}

impl Client {
    pub(super) fn new(
        store: Store,
        changes: broadcast::Receiver<Arc<Changes>>,
        catching_up: CatchingUp,
        timing: Timing,
        out: mpsc::Sender<Vec<u8>>,
        shared: Arc<Shared>,
    ) -> Client {
        Client {
            store,
            changes: Some(changes),
            catching_up,
            timing,
            out,
            shared,
            since: None,
            heard: 0,
            windows: BTreeMap::new(),
            next: 1,
            blocked: None,
            held: false,
            resync: Vec::new(),
        }
    }

    /// Takes hello's `since`, the default for the watches this connection sends again.
    pub fn hello(&mut self, since: Option<u64>) {
        self.since = since;
    }

    /// Sends nothing while `held`, as while a reply waits for room in the queue: a push could then
    /// go ahead of the reply it depends on, a watch's result. Its windows still take their changes,
    /// and one due while held counts as blocked, so once its interface has left the queue full for
    /// `Timing::behind` it's told to watch again as when it holds no reply.
    pub fn hold(&mut self, held: bool) {
        self.held = held;
    }

    /// Answers `chats.watch`, `messages.watch`, `watch.move` or `unwatch`.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        match method {
            "chats.watch" => {
                let watch: ChatsWatch = params_of(method, params)?;
                let list = list(&watch.folder)?;
                self.room_for_another()?;
                self.folder_there(&list, &watch.folder).await?;
                let spec = self.chat_spec(list, watch.around, watch.before, watch.after).await?;
                self.watch(Spec::Chats(spec), watch.since, watch.have).await
            }
            "folders.watch" => {
                let watch: FoldersWatch = params_of(method, params)?;
                self.room_for_another()?;
                self.watch(Spec::Folders, watch.since, watch.have).await
            }
            "roster.watch" => {
                let watch: RosterWatch = params_of(method, params)?;
                let list = list(&watch.folder)?;
                self.room_for_another()?;
                self.folder_there(&list, &watch.folder).await?;
                let spec = self.roster_spec(list, watch.offline, watch.around, watch.before, watch.after).await?;
                self.watch(Spec::Roster(spec), watch.since, watch.have).await
            }
            "messages.watch" => {
                let watch: MessagesWatch = params_of(method, params)?;
                self.room_for_another()?;
                let spec = self.message_spec(watch.chat, watch.around, watch.before, watch.after).await?;
                self.watch(Spec::Messages(spec), watch.since, watch.have).await
            }
            "watch.move" => {
                let moving: WatchMove = params_of(method, params)?;
                let spec = match self.windows.get(&moving.watch).map(|window| &window.spec) {
                    Some(Spec::Chats(chats)) => {
                        let list = chats.list.clone();
                        Spec::Chats(self.chat_spec(list, moving.around, moving.before, moving.after).await?)
                    }
                    Some(Spec::Roster(roster)) => {
                        let (list, offline) = (roster.list.clone(), roster.offline);
                        let (around, before, after) = (moving.around, moving.before, moving.after);
                        Spec::Roster(self.roster_spec(list, offline, around, before, after).await?)
                    }
                    Some(Spec::Folders) => {
                        return Err(Error::new(code::INVALID_PARAMS, "the folder tree is one window, whole"));
                    }
                    Some(Spec::Messages(messages)) => {
                        let chat = messages.chat.clone();
                        Spec::Messages(self.message_spec(chat, moving.around, moving.before, moving.after).await?)
                    }
                    None => return Err(unknown(&moving.watch)),
                };
                self.relocate(moving.watch, spec).await
            }
            "unwatch" => {
                let Watch { watch } = params_of(method, params)?;
                self.windows.remove(&watch).ok_or_else(|| unknown(&watch))?;
                Ok(Value::Object(Default::default()))
            }
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        }
    }

    /// Refuses another window to a connection already watching the most.
    fn room_for_another(&self) -> Result<(), Error> {
        if self.windows.len() < MOST_WINDOWS {
            return Ok(());
        }
        let mut error = Error::new(
            code::LIMIT,
            format!("a connection can watch at most {MOST_WINDOWS} windows at once, so one has to be unwatched first"),
        );
        error.data = Some(serde_json::json!({ "limit": "windows", "most": MOST_WINDOWS }));
        Err(error)
    }

    async fn read<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, Error> {
        read_on(&self.store, query).await
    }

    /// Refuses a folder jawd doesn't have, error 3.
    async fn folder_there(&self, list: &List, folder: &str) -> Result<(), Error> {
        let asked = list.clone();
        if self.read(move |db| lists::exists(db, &asked)).await? {
            return Ok(());
        }
        Err(Error::new(code::NOT_FOUND, format!("jawd has no folder {folder}")))
    }

    async fn chat_spec(&self, list: List, around: Around, before: u32, after: u32) -> Result<ChatSpec, Error> {
        let (before, after) = sides(before, after)?;
        let anchor = match around {
            Around::Top => None,
            Around::Row(id) => {
                let chat = id.clone();
                match self.read(move |db| read::chat_place(db, &chat)).await? {
                    Some((_, cursor, _)) => Some(ChatAnchor { id, cursor }),
                    None => return Err(Error::new(code::NOT_FOUND, format!("jawd has no chat {id}"))),
                }
            }
            Around::Newest | Around::Unread => {
                return Err(Error::new(code::INVALID_PARAMS, "a chat list is anchored at top or on a chat's id"));
            }
        };
        Ok(ChatSpec { list, anchor, before, after })
    }

    async fn roster_spec(
        &self,
        list: List,
        offline: bool,
        around: Around,
        before: u32,
        after: u32,
    ) -> Result<RosterSpec, Error> {
        let (before, after) = sides(before, after)?;
        let anchor = match around {
            Around::Top => None,
            Around::Row(id) => {
                let n = id.strip_prefix("person:").and_then(|n| n.parse::<i64>().ok());
                let Some(n) = n else {
                    return Err(Error::new(code::INVALID_PARAMS, format!("{id} isn't a person's ID")));
                };
                match self.read(move |db| lists::person_place(db, n)).await? {
                    Some(cursor) => Some(PersonAnchor { n, cursor }),
                    None => return Err(Error::new(code::NOT_FOUND, format!("jawd has no {id}"))),
                }
            }
            Around::Newest | Around::Unread => {
                return Err(Error::new(code::INVALID_PARAMS, "a roster is anchored at top or on a person's id"));
            }
        };
        Ok(RosterSpec { list, offline, anchor, before, after })
    }

    async fn message_spec(&self, chat: String, around: Around, before: u32, after: u32) -> Result<MessageSpec, Error> {
        let (before, after) = sides(before, after)?;
        let anchor = match around {
            Around::Newest => None,
            Around::Unread => {
                let of = chat.clone();
                self.read(move |db| read::first_unread(db, &of)).await?.map(|(id, cursor)| MessageAnchor { id, cursor })
            }
            Around::Row(id) => {
                let (of, message) = (chat.clone(), id.clone());
                match self.read(move |db| read::message_place(db, &of, &message)).await? {
                    Some(cursor) => Some(MessageAnchor { id, cursor }),
                    None => return Err(Error::new(code::NOT_FOUND, format!("jawd has no message {id} in {chat}"))),
                }
            }
            Around::Top => {
                return Err(Error::new(
                    code::INVALID_PARAMS,
                    "a chat's messages are anchored at newest, unread or a message's id",
                ));
            }
        };
        Ok(MessageSpec { chat, anchor, before, after })
    }

    /// Reads `spec`, or just the chats `only` names, for this connection alone, with the feed's
    /// last committed seq when it was asked for.
    async fn snapshot(&self, spec: Spec, only: Option<Vec<i64>>) -> Result<(Snapshot, Spec, i64), Error> {
        let from = *self.store.committed().borrow();
        let (snapshot, spec) = self.read(self.shared.query(spec, only)).await?;
        Ok((snapshot, spec, from))
    }

    /// A read of `slot`'s window with every change up to `owed`: its last read if that has them,
    /// the one under way if it was asked for since, or else a new one, which every connection
    /// watching the same window can take in turn. A new read runs to its end even if this
    /// connection goes, since others may be waiting for it.
    async fn shared(&self, slot: &Arc<Slot>, owed: i64) -> Result<Arc<Found>, Error> {
        let mut answer = {
            let mut state = slot.state.lock().expect("a window's reads");
            if let Some(kept) = state.kept.as_ref().filter(|kept| kept.found.fresh >= owed) {
                self.shared.taken.fetch_add(1, Ordering::Relaxed);
                return Ok(kept.found.clone());
            }
            match state.reading.as_ref().filter(|reading| reading.from >= owed) {
                Some(reading) => {
                    self.shared.joined.fetch_add(1, Ordering::Relaxed);
                    reading.answer.clone()
                }
                None => {
                    let from = *self.store.committed().borrow();
                    let (tell, answer) = watch::channel(None);
                    state.asked += 1;
                    let under_way = UnderWay { slot: slot.clone(), asked: state.asked };
                    state.reading = Some(Reading { asked: state.asked, from, answer: answer.clone() });
                    self.shared.reads.fetch_add(1, Ordering::Relaxed);
                    let query = self.shared.query(slot.spec.clone(), None);
                    let (store, shared) = (self.store.clone(), self.shared.clone());
                    tokio::spawn(async move {
                        let read = read_on(&store, query).await;
                        #[cfg(test)]
                        if shared.panicking.swap(false, Ordering::Relaxed) {
                            panic!("the test made a window read's task panic");
                        }
                        let read = read.map(|(snapshot, spec)| Arc::new(found(snapshot, spec, from)));
                        let mut state = under_way.slot.state.lock().expect("a window's reads");
                        if state.reading.as_ref().is_some_and(|reading| reading.asked == under_way.asked) {
                            state.reading = None;
                        }
                        if let Ok(read) = &read
                            && state.kept.as_ref().is_none_or(|kept| kept.found.fresh <= read.fresh)
                        {
                            state.kept = Some(shared.keep(read.clone()));
                        }
                        drop(state);
                        shared.trim();
                        tell.send_replace(Some(read));
                    });
                    answer
                }
            }
        };
        let answer = answer
            .wait_for(Option::is_some)
            .await
            .map_err(|_| Error::new(code::INTERNAL, "jawd's read of the window stopped before it answered"))?;
        answer.clone().expect("it waited for an answer")
    }

    async fn watch(&mut self, spec: Spec, since: Option<u64>, have: Vec<String>) -> Result<Value, Error> {
        let (snapshot, spec, from) = self.snapshot(spec, None).await?;
        // Rows' versions are seqs, so a since past the feed's last is from a jaw.db since replaced.
        let since = since
            .or(self.since)
            .and_then(|since| i64::try_from(since).ok())
            .filter(|&since| since >= snapshot.oldest - 1 && since <= snapshot.seq);
        let have = have.into_iter().map(|id| (id, Held { hash: 0, sort: String::new(), n: 0 })).collect();
        let found = found(snapshot, spec.clone(), from);
        let (update, state) = settle(&have, None, &found, since);
        let watch = format!("w{}", self.next);
        self.next += 1;
        let slot = self.shared.slot(&spec);
        let mut window = Window::new(spec, slot, &self.shared);
        window.keep(state, Some(Instant::now()));
        self.windows.insert(watch.clone(), window);
        Ok(result(watch, update))
    }

    async fn relocate(&mut self, watch: String, spec: Spec) -> Result<Value, Error> {
        let (snapshot, spec, from) = self.snapshot(spec, None).await?;
        let slot = self.shared.slot(&spec);
        let window = self.windows.get_mut(&watch).ok_or_else(|| unknown(&watch))?;
        let found = found(snapshot, spec.clone(), from);
        let (update, state) = settle(&window.state.held, window.state.favourites, &found, None);
        window.spec = spec;
        window.slot = slot;
        window.keep(state, Some(Instant::now()));
        Ok(result(watch, update))
    }

    /// Waits for a change from the feed, a flush falling due, room for a blocked flush, or a
    /// blocked one waiting too long. Cancelling it loses nothing.
    pub async fn ready(&mut self) -> Ready {
        let flush_at = if self.blocked.is_none() { self.flush_at() } else { None };
        // tokio's timers tick each millisecond, which a flush already due shouldn't wait for.
        if flush_at.is_some_and(|at| at <= Instant::now()) {
            return Ready::Flush;
        }
        let behind_at = self.blocked.filter(|_| !self.windows.is_empty()).map(|since| since + self.timing.behind);
        // Room in the queue goes first to the reply a held client waits behind.
        let out = self.blocked.filter(|_| !self.held).map(|_| self.out.clone());
        tokio::select! {
            biased;
            _ = until(flush_at) => Ready::Flush,
            _ = room(out) => Ready::Room,
            _ = until(behind_at) => Ready::Behind,
            changes = recv(&mut self.changes) => Ready::Changes(changes),
        }
    }

    pub async fn handle(&mut self, ready: Ready) {
        match ready {
            Ready::Changes(Ok(changes)) => self.mark(&changes),
            Ready::Changes(Err(RecvError::Lagged(_))) => {
                // What was lost may reach past the last committed seq this has heard of, so only
                // a read asked for from now on will do.
                let now = Instant::now();
                for window in self.windows.values_mut() {
                    window.wait(now, i64::MAX);
                }
            }
            Ready::Changes(Err(RecvError::Closed)) => self.changes = None,
            Ready::Flush | Ready::Room => self.flush().await,
            Ready::Behind => {
                if self.blocked.is_some_and(|since| since.elapsed() >= self.timing.behind) {
                    self.resync.extend(std::mem::take(&mut self.windows).into_keys());
                }
            }
        }
    }

    fn mark(&mut self, changes: &Changes) {
        self.heard = self.heard.max(changes.seq);
        for window in self.windows.values_mut() {
            if window.touched_by(changes) {
                window.wait(changes.at, changes.seq);
            }
        }
    }

    fn flush_at(&self) -> Option<Instant> {
        if !self.resync.is_empty() {
            return Some(Instant::now());
        }
        let (now, holding) = (Instant::now(), (self.catching_up)());
        self.windows
            .values()
            .flat_map(|window| [window.due(&self.timing), window.reorder_at(now, holding, &self.timing)])
            .flatten()
            .min()
    }

    /// Sends what the first due window's interface lacks, conversations first, unless the queue
    /// is full or held. One window a flush: those still due come at once, but after any request
    /// the interface has sent meanwhile, so a request waits for one window's read at most.
    async fn flush(&mut self) {
        if self.held {
            self.blocked.get_or_insert_with(Instant::now);
            return;
        }
        while let Some(watch) = self.resync.first() {
            if self.out.try_send(line("resync", Watch { watch: watch.clone() })).is_err() {
                self.blocked.get_or_insert_with(Instant::now);
                return;
            }
            self.resync.remove(0);
        }
        let holding = (self.catching_up)();
        let now = Instant::now();
        let mut due: Vec<(bool, String)> = self
            .windows
            .iter()
            .filter(|(_, window)| {
                [window.due(&self.timing), window.reorder_at(now, holding, &self.timing)]
                    .into_iter()
                    .flatten()
                    .any(|due| due <= now)
            })
            .map(|(watch, window)| (matches!(window.spec, Spec::Chats(_)), watch.clone()))
            .collect();
        // Conversations first: a lone message shows there, and the chat list can wait a read.
        due.sort_by_key(|(chats, _)| *chats);
        #[cfg(test)]
        if !due.is_empty() {
            self.shared.pause().await;
        }
        if let Some((_, watch)) = due.into_iter().next()
            && (self.out.capacity() == 0 || !self.flush_one(watch, now, holding).await)
        {
            self.blocked.get_or_insert_with(Instant::now);
            return;
        }
        self.blocked = None;
    }

    /// Reads `watch` and sends what its interface lacks; false if the queue had no room for it.
    async fn flush_one(&mut self, watch: String, now: Instant, holding: bool) -> bool {
        let Some(window) = self.windows.get(&watch) else { return true };
        // A chat list held while catching up reads its chats where they are and keeps their order.
        let only = (!window.may_reorder(now, holding, &self.timing))
            .then(|| window.state.held.values().map(|held| held.n).collect());
        let reordered = only.is_none().then_some(now);
        let read = match only {
            None => self.shared(&window.slot.clone(), window.owed(self.heard)).await,
            Some(only) => {
                self.shared.reads.fetch_add(1, Ordering::Relaxed);
                let read = self.snapshot(window.spec.clone(), Some(only)).await;
                read.map(|(mut snapshot, spec, from)| {
                    // A chat list held while catching up keeps the order its interface has.
                    if let (Some(window), Rows::Chats(rows)) = (self.windows.get(&watch), &mut snapshot.rows) {
                        for (_, row) in rows.iter_mut() {
                            if let Some(held) = window.state.held.get(&row.id) {
                                row.sort.clone_from(&held.sort);
                            }
                        }
                    }
                    Arc::new(found(snapshot, spec, from))
                })
            }
        };
        let Some(window) = self.windows.get_mut(&watch) else { return true };
        window.read = Some(Instant::now());
        let found = match read {
            Ok(found) => found,
            // The window stays marked, so it's read again a `burst` later.
            Err(e) => {
                let failing = *window.failing.get_or_insert_with(|| {
                    eprintln!("jawd: can't bring {watch} up to date, and will keep trying: {}", e.message);
                    now
                });
                if failing.elapsed() >= self.timing.behind {
                    eprintln!("jawd: {watch} still can't be read, so its interface is told to watch it again");
                    self.windows.remove(&watch);
                    self.resync.push(watch);
                }
                return true;
            }
        };
        window.failing = None;
        if window.spec != found.spec {
            window.spec = found.spec.clone();
            window.slot = self.shared.slot(&window.spec);
        }
        let (update, state) = settle(&window.state.held, window.state.favourites, &found, None);
        match push(watch, update, &window.state) {
            None => window.keep(state, reordered),
            Some(line) => match self.out.try_send(line) {
                Ok(()) => window.keep(state, reordered),
                Err(_) => return false,
            },
        }
        true
    }

    /// Whether it has updates or resyncs it hasn't sent.
    #[cfg(test)]
    pub fn pending(&self) -> bool {
        !self.resync.is_empty() || self.windows.values().any(|window| window.first.is_some() || window.reorder)
    }
}
