//! The change feed and windows. One task reads the store's events as the writer commits them and
//! tells every connection which rows changed. Each connection's `Client` keeps its windows, marks
//! the ones a change touches, reads each again and sends what differs from what its interface
//! holds: the first change after a quiet spell at once, then the rest of a burst when the feed has
//! been quiet for 5 ms or every 50 ms, each window read no more than once each 50 ms. A window is
//! always what a fresh query gives, so it can't drift, and a connection keeps only its windows'
//! rows' hashes: a slow interface never holds jawd up, it only gets fewer, larger batches, and one
//! that stops reading for long is told to watch again.

pub mod bench;
mod client;
#[cfg(test)]
mod tests;
mod window;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc, watch};

pub use self::client::{Client, Ready};
pub(crate) use self::window::{chat_row, message_row};
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
    reads: Arc<AtomicU64>,
}

impl Feed {
    /// Starts the task that reads the store's events as they're committed.
    pub fn start(store: Store, catching_up: CatchingUp, timing: Timing) -> Feed {
        let (changes, _) = broadcast::channel(CHANGES_KEPT);
        let committed = store.committed();
        tokio::spawn(follow(store.clone(), committed.clone(), changes.clone()));
        Feed { store, changes, committed, catching_up, timing, reads: Arc::default() }
    }

    /// The feed's last committed seq.
    pub fn seq(&self) -> u64 {
        (*self.committed.borrow()).max(0) as u64
    }

    /// A connection's windows, sending their updates to `out` as lines.
    pub fn client(&self, out: mpsc::Sender<Vec<u8>>) -> Client {
        let changes = self.changes.subscribe();
        Client::new(self.store.clone(), changes, self.catching_up.clone(), self.timing, out, self.reads.clone())
    }

    /// How many times any connection has read one of its windows again since jawd started.
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
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
            let events =
                store.reads().query(Priority::Interactive, move |db| read::events(db, since, EVENTS_AT_ONCE)).await;
            let events = match events {
                Ok(events) => events,
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
