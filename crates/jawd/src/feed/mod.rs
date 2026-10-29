//! The change feed and windows. One task reads the store's events as the writer commits them and
//! tells every connection which rows changed. Each connection's `Client` keeps its windows, marks
//! the ones a change touches, reads each again and sends what differs from what its interface
//! holds: the first change after a quiet spell at once, then the rest of a burst when the feed has
//! been quiet for 5 ms or every 50 ms, each window read no more than once each 50 ms, and one read
//! shared by every connection watching the same window. A window is always what a fresh query
//! gives, so it can't drift, and a connection keeps only its windows' rows' hashes: a slow
//! interface never holds jawd up, it only gets fewer, larger batches, and one that stops reading
//! for long is told to watch again. Who's typing in the chats a connection's windows show comes
//! with each watch's answer and then as `typing` pushes, from what `typing` keeps in memory.

pub mod bench;
mod client;
#[cfg(test)]
mod tests;
pub mod typing;
mod window;

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, watch};

pub use self::client::{Client, Ready};
pub use self::typing::Typing;
pub(crate) use self::window::{chat_row, folder_row, message_row, now_ms};
use crate::store::{Store, read};
use crate::work::Priority;

/// Which rows the writer changed, from one read of the events.
#[derive(Debug)]
pub struct Changes {
    /// The last seq they reach.
    pub seq: i64,
    /// When the feed read them, just after they were committed: a client busy when they came
    /// still counts their wait from then.
    pub at: Instant,
    /// Chats changed or removed, by n.
    pub chats: HashSet<i64>,
    /// Chats with messages changed or removed, by n.
    pub messages: HashSet<i64>,
    pub people: HashSet<i64>,
    /// Every JID the people changed have now, `hashed`, which the rows that mention or quote them
    /// name them by. Whoever comes to have a JID, by a new person or a number and a LID found to
    /// be one, is among the people changed, so a row naming it is told the name it now shows.
    pub jids: HashSet<u64>,
    /// People whose presence changed, by n, which only rosters show.
    pub presence: HashSet<i64>,
    /// Whether a folder or a favourite slot changed.
    pub folders: bool,
}

/// A JID as `Changes` and windows hold it, in a word rather than its text: two JIDs that hash
/// alike only mark a window that didn't need it.
pub(super) fn hashed(jid: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    jid.hash(&mut hasher);
    hasher.finish()
}

/// How a client's windows are paced.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// How long the feed is quiet before the rest of a burst goes.
    pub quiet: Duration,
    /// The longest a change waits during a burst, the shortest time between a window's reads, and
    /// so its batches, and how long a window must go unread for a change to go at once.
    pub burst: Duration,
    /// How often the chat list reorders while catching up.
    pub hold: Duration,
    /// How long an interface can leave its updates unread, or a window's reads keep failing, before
    /// its windows are dropped and it's told to watch them again.
    pub behind: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            quiet: Duration::from_millis(5),
            burst: Duration::from_millis(50),
            hold: Duration::from_secs(2),
            behind: Duration::from_secs(5),
        }
    }
}

/// Whether a catch-up after time offline is under way.
pub type CatchingUp = Arc<dyn Fn() -> bool + Send + Sync>;

/// While sync says a catch-up is under way: from WhatsApp's preview of what's waiting until it
/// says it's all been sent.
pub fn catching_up(live: crate::sync::Shared) -> CatchingUp {
    Arc::new(move || {
        live.lock().expect("live state").catch_up.as_ref().is_some_and(|catch_up| catch_up.seconds.is_none())
    })
}

/// How many reads of the events a client can fall behind before it loses track of which windows
/// they touched, and reads them all again.
const CHANGES_KEPT: usize = 1024;
/// The most events one read takes.
const EVENTS_AT_ONCE: usize = 4096;

#[derive(Clone)]
pub struct Feed {
    store: Store,
    changes: broadcast::Sender<Arc<Changes>>,
    committed: watch::Receiver<i64>,
    catching_up: CatchingUp,
    timing: Timing,
    shared: Arc<client::Shared>,
    typing: Typing,
}

impl Feed {
    /// Starts the task that reads the store's events as they're committed.
    pub fn start(store: Store, catching_up: CatchingUp, timing: Timing) -> Feed {
        Feed::keeping(store, catching_up, timing, CHANGES_KEPT)
    }

    /// Starts the feed keeping `kept` reads of the events for a client that falls behind.
    fn keeping(store: Store, catching_up: CatchingUp, timing: Timing, kept: usize) -> Feed {
        let (changes, _) = broadcast::channel(kept);
        let committed = store.committed();
        tokio::spawn(follow(store.clone(), committed.clone(), changes.clone()));
        Feed { store, changes, committed, catching_up, timing, shared: Arc::default(), typing: Typing::default() }
    }

    /// Tells who's typing from `typing` rather than a `Typing` of its own, before any connection
    /// has its windows.
    #[cfg(test)]
    pub(crate) fn with_typing(mut self, typing: Typing) -> Feed {
        self.typing = typing;
        self
    }

    /// Who's typing, which connections' windows tell their interfaces.
    pub fn typing(&self) -> &Typing {
        &self.typing
    }

    /// The feed's last committed seq.
    pub fn seq(&self) -> u64 {
        (*self.committed.borrow()).max(0) as u64
    }

    /// A connection's windows, sending their updates to `out` as lines.
    pub fn client(&self, out: mpsc::Sender<Vec<u8>>) -> Client {
        let changes = self.changes.subscribe();
        let (store, catching_up, typing) = (self.store.clone(), self.catching_up.clone(), self.typing.watching());
        Client::new(store, changes, catching_up, self.timing, out, self.shared.clone(), typing)
    }

    /// How many times any connection has asked to read one of its windows again since jawd
    /// started, a read shared by several counting once.
    pub fn reads(&self) -> u64 {
        self.shared.reads()
    }

    /// The windows interfaces watch, and the reads kept for them to share, for `jawd status`.
    pub fn windows(&self) -> jaw_proto::Windows {
        self.shared.windows()
    }

    /// Whose presence the windows show now, by person n, each once: open one-to-one chats' people,
    /// and rosters' people who aren't among them, each in its roster's order.
    pub fn showing(&self) -> (Vec<i64>, Vec<i64>) {
        self.shared.showing()
    }

    /// Changes each time whose presence the windows show does.
    pub fn shown(&self) -> watch::Receiver<u64> {
        self.shared.shown()
    }

    /// Makes every window read fail on its reader, or read again.
    #[cfg(test)]
    pub(super) fn fail_reads(&self, failing: bool) {
        self.shared.failing.store(failing, std::sync::atomic::Ordering::Relaxed);
    }

    /// Holds each window read's answer back `ms` after its snapshot is taken.
    #[cfg(test)]
    pub(super) fn slow_reads(&self, ms: u64) {
        self.shared.slow.store(ms, std::sync::atomic::Ordering::Relaxed);
    }

    /// Makes the next shared read's task panic once its read has answered.
    #[cfg(test)]
    pub(super) fn panic_next_read(&self) {
        self.shared.panicking.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Lets the reads kept for windows to share hold at most `bytes`.
    #[cfg(test)]
    pub(super) fn keep_at_most(&self, bytes: u64) {
        self.shared.keep_at_most(bytes);
    }

    /// Stops the next flush that isn't held, once it has a window due and before it looks for room
    /// in its queue: it notifies the first when it stops, and goes on once the second is notified.
    #[cfg(test)]
    pub(super) fn pause_next_flush(&self) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
        let pause = (Arc::default(), Arc::default());
        *self.shared.pausing.lock().expect("the test's pause") = Some(pause.clone());
        pause
    }
}

/// Reads the events each commit wrote and sends which rows they touched. It never waits on a
/// client: one that falls behind loses the oldest, and reads its windows again.
async fn follow(store: Store, mut committed: watch::Receiver<i64>, changes: broadcast::Sender<Arc<Changes>>) {
    let mut seen = *committed.borrow_and_update();
    loop {
        let last = *committed.borrow_and_update();
        while seen < last {
            let since = seen;
            let read = store
                .reads()
                .query(Priority::Interactive, move |db| {
                    let events = read::events(db, since, EVENTS_AT_ONCE)?;
                    let people: Vec<i64> =
                        events.iter().filter(|event| event.kind == "person").map(|event| event.key).collect();
                    let jids = if people.is_empty() { Vec::new() } else { read::jids_of(db, &people)? };
                    Ok((events, jids))
                })
                .await;
            let (events, jids) = match read {
                Ok(read) => read,
                Err(e) => {
                    eprintln!("jawd: the feed can't read what changed: {e:#}");
                    break;
                }
            };
            let Some(newest) = events.last() else { break };
            let mut batch = Changes {
                seq: newest.seq,
                at: Instant::now(),
                chats: HashSet::new(),
                messages: HashSet::new(),
                people: HashSet::new(),
                jids: jids.iter().map(|jid| hashed(jid)).collect(),
                presence: HashSet::new(),
                folders: false,
            };
            for event in &events {
                match (event.kind.as_str(), event.chat) {
                    ("chat", _) => {
                        batch.chats.insert(event.key);
                    }
                    ("message", Some(chat)) => {
                        batch.messages.insert(chat);
                    }
                    ("person", _) => {
                        batch.people.insert(event.key);
                    }
                    ("presence", _) => {
                        batch.presence.insert(event.key);
                    }
                    ("folder" | "favourite", _) => batch.folders = true,
                    _ => {}
                }
            }
            seen = batch.seq;
            let _ = changes.send(Arc::new(batch));
        }
        if committed.changed().await.is_err() {
            return;
        }
    }
}
