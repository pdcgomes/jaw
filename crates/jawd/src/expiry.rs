//! The timer for disappearing messages: one task waits for the soonest `expires_at` jaw.db holds and,
//! at it, has the writer erase what's due, a batch at a time, in the background lane's short
//! transactions (`store::expiry`); at start it erases what came due while jawd wasn't running. An
//! erasure acknowledges nothing, so its commit isn't synced: one a power cut loses is still due, and
//! goes at the next start, and media's cleaner makes a synced commit of its own before it deletes a
//! file, which makes the erasures before it durable first. The writer wakes it
//! when it commits a sooner time than the one it waits for, and while it waits for one, a commit
//! wakes it to look at the wall clock, since its sleep counts only time the Mac was awake and so runs
//! late after a sleep or a change of the clock. With nothing to expire it waits for the writer alone:
//! nothing polls.

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use jaw_proto::disappearing::DisappearingStatus;

use crate::store::{Change, Lane, Store, expiry};
use crate::work::Priority;

/// How soon after a commit woke the timer another may, so a flood's commits wake it once a second at
/// most.
const LOOK_EVERY: Duration = Duration::from_secs(1);
/// How long a message whose erasure failed, which is kept in quarantine with why, waits before it's
/// tried again, at first and at most, while the rest go at their times.
const RETRY_FIRST: Duration = Duration::from_secs(60);
const RETRY_MOST: Duration = Duration::from_secs(3600);

/// The timer, running.
#[derive(Default)]
pub struct Expiry {
    /// How far a test has moved the wall clock on past what the timer's sleep counts, as the Mac
    /// sleeping does, and how often the timer has looked again while it waited.
    #[cfg(test)]
    skew: std::sync::atomic::AtomicI64,
    #[cfg(test)]
    looks: std::sync::atomic::AtomicU64,
}

impl Expiry {
    /// What `status` says: the messages with a time to go and when the next does, read from jaw.db,
    /// and those erased since jawd started, disappeared and opened alike; `open` and `open_most` are
    /// the view-once messages'.
    pub fn status(&self, store: &Store) -> DisappearingStatus {
        let next = store.pair(expiry::NEXT_FOR_STATUS).ok().map(|(at, _)| at).filter(|&at| at > 0);
        DisappearingStatus {
            expiring: store.count(expiry::EXPIRING).unwrap_or(0),
            next,
            erased: store.erased(),
            ..DisappearingStatus::default()
        }
    }

    /// The wall clock, in milliseconds.
    fn now(&self) -> i64 {
        #[cfg(test)]
        let skew = self.skew.load(Ordering::Relaxed);
        #[cfg(not(test))]
        let skew = 0;
        now_ms() + skew
    }

    fn looked(&self) {
        #[cfg(test)]
        self.looks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Starts the timer, once jaw.db's state is durable (`sync::running`).
pub fn start(store: Store) -> Arc<Expiry> {
    let expiry = Arc::new(Expiry::default());
    tokio::spawn(run(store, expiry.clone()));
    expiry
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// What one batch came to: whether more may be due, and which it found due and couldn't erase, kept
/// in quarantine instead; and for tests how many it erased, which `status` has from the store.
#[derive(Debug, Default)]
pub struct Batch {
    #[cfg(test)]
    pub erased: usize,
    pub more: bool,
    pub failed: Vec<i64>,
}

/// Erases the messages due by `now`, soonest first, but for those in `skip`: a batch's worth, in the
/// background lane's transactions, each a few milliseconds at most with its commit and ending early
/// once a live write or an inbox row waits, so no erasure holds them back for more than one. Each
/// commits at NORMAL: a kill or a power cut leaves the erasures of the transactions before it, and
/// the rest still due.
pub async fn erase_due(store: &Store, now: i64, skip: &[i64]) -> Result<Batch> {
    let skipping = skip.to_vec();
    let due: Vec<i64> = store
        .reads()
        .query(Priority::Interactive, move |db| {
            let found: Vec<i64> = db
                .prepare_cached(expiry::DUE)?
                .query_map(rusqlite::params![now, (expiry::BATCH + skipping.len()) as i64], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(found.into_iter().filter(|n| !skipping.contains(n)).take(expiry::BATCH).collect())
        })
        .await?;
    if due.is_empty() {
        return Ok(Batch::default());
    }
    let applied = store.apply(Lane::Background, due.iter().map(|&n| Change::Expire { n, at: now }).collect()).await?;
    let failed = if applied.quarantined > 0 {
        let looked = due.clone();
        store
            .reads()
            .query(Priority::Interactive, move |db| {
                let mut still = Vec::new();
                for n in looked {
                    if db.prepare_cached(expiry::STILL_DUE)?.exists(rusqlite::params![n, now])? {
                        still.push(n);
                    }
                }
                Ok(still)
            })
            .await?
    } else {
        Vec::new()
    };
    Ok(Batch {
        #[cfg(test)]
        erased: applied.changed,
        more: due.len() == expiry::BATCH,
        failed,
    })
}

/// The soonest time a message goes, as jaw.db has it now, but for those in `skip`.
async fn next(store: &Store, skip: &[i64]) -> Result<Option<i64>> {
    let skipping = skip.to_vec();
    store
        .reads()
        .query(Priority::Interactive, move |db| {
            let soonest: Vec<(i64, i64)> = db
                .prepare_cached(expiry::NEXT)?
                .query_map(rusqlite::params![i64::MIN, (skipping.len() + 1) as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(soonest.into_iter().find(|(n, _)| !skipping.contains(n)).map(|(_, at)| at))
        })
        .await
}

/// Erases what's due, then waits for the next, until jawd stops.
async fn run(store: Store, expiry: Arc<Expiry>) {
    let soonest = store.soonest();
    let mut committed = store.committed();
    // Each message whose erasure failed: when it's tried again, and the wait after that.
    let mut failing: HashMap<i64, (Instant, Duration)> = HashMap::new();
    loop {
        failing.retain(|_, (again, _)| *again > Instant::now());
        let skip: Vec<i64> = failing.keys().copied().collect();
        match erase_due(&store, expiry.now(), &skip).await {
            Ok(batch) => {
                for n in batch.failed {
                    let pause = failing.get(&n).map_or(RETRY_FIRST, |(_, pause)| (*pause * 2).min(RETRY_MOST));
                    eprintln!(
                        "jawd: couldn't erase message {n} at its time, so it's kept in quarantine and tried again"
                    );
                    failing.insert(n, (Instant::now() + pause, pause));
                }
                if batch.more {
                    continue;
                }
            }
            Err(e) => {
                eprintln!("jawd: can't erase the messages whose time has come, so it's tried again: {e:#}");
                tokio::time::sleep(RETRY_FIRST).await;
                continue;
            }
        }
        let skip: Vec<i64> = failing.keys().copied().collect();
        let due = match next(&store, &skip).await {
            Ok(due) => due,
            Err(e) => {
                eprintln!("jawd: can't read when the next message goes, so it's read again: {e:#}");
                tokio::time::sleep(RETRY_FIRST).await;
                continue;
            }
        };
        let retry = failing.values().map(|(again, _)| *again).min();
        if !wait(&expiry, &soonest, &mut committed, due, retry).await {
            return;
        }
    }
}

/// Waits until `due`, by the wall clock, or `retry`, or a sooner time is committed; says whether
/// jawd goes on.
async fn wait(
    expiry: &Expiry,
    soonest: &crate::store::Soonest,
    committed: &mut tokio::sync::watch::Receiver<i64>,
    mut due: Option<i64>,
    retry: Option<Instant>,
) -> bool {
    let mut look_after = Instant::now();
    loop {
        if due.is_some_and(|due| due <= expiry.now()) || retry.is_some_and(|retry| retry <= Instant::now()) {
            return true;
        }
        expiry.looked();
        let sleep = due.map(|due| Duration::from_millis((due - expiry.now()).max(0) as u64));
        let sleep = match (sleep, retry) {
            (Some(sleep), Some(retry)) => Some(sleep.min(retry.saturating_duration_since(Instant::now()))),
            (sleep, retry) => sleep.or(retry.map(|retry| retry.saturating_duration_since(Instant::now()))),
        };
        let looking = due.is_some() && Instant::now() >= look_after;
        tokio::select! {
            () = async {
                match sleep {
                    Some(sleep) => tokio::time::sleep(sleep).await,
                    None => std::future::pending().await,
                }
            } => {}
            () = soonest.noted() => {
                if let Some(at) = soonest.take()
                    && due.is_none_or(|due| at < due)
                {
                    due = Some(at);
                }
            }
            changed = committed.changed(), if looking => {
                if changed.is_err() {
                    return false;
                }
                look_after = Instant::now() + LOOK_EVERY;
            }
            () = tokio::time::sleep_until(look_after.into()), if due.is_some() && !looking => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;
    use rusqlite::OptionalExtension;
    use wa_proto::wa_web_protobufs_e2e as e2e;

    use super::*;
    use crate::store::outbox::{Step, Written};
    use crate::store::{Lane, Message};

    const ANA: &str = "351910000002@s.whatsapp.net";

    fn message(id: &str, expires_at: i64) -> Message {
        Message {
            chat: ANA.into(),
            id: id.into(),
            at: now_ms(),
            sender: Some(ANA.into()),
            kind: "text",
            text: Some(id.into()),
            summary: id.into(),
            expires_at: Some(expires_at),
            live: true,
            ..Message::default()
        }
    }

    async fn there(store: &Store, id: &'static str) -> bool {
        store
            .reads()
            .query(Priority::Interactive, move |db| {
                Ok(db
                    .query_row("SELECT 1 FROM messages WHERE id = ?1", [id], |r| r.get::<_, i64>(0))
                    .optional()?
                    .is_some())
            })
            .await
            .unwrap()
    }

    /// Waits up to `within` for message `id` to go, saying whether it did.
    async fn gone(store: &Store, id: &'static str, within: Duration) -> bool {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if !there(store, id).await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_committed_to_go_sooner_than_the_one_the_timer_waits_for_goes_at_its_own_time() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, vec![Change::Message(message("later", now_ms() + 60_000))]).await.unwrap();
        let expiry = start(store.clone());
        tokio::time::sleep(Duration::from_millis(200)).await;
        let sooner = now_ms() + 300;
        store.apply(Lane::Live, vec![Change::Message(message("sooner", sooner))]).await.unwrap();
        assert!(gone(&store, "sooner", Duration::from_secs(10)).await, "it waited for the later one");
        assert!(now_ms() >= sooner, "it went before its time");
        assert!(there(&store, "later").await);
        let status = expiry.status(&store);
        assert_eq!((status.expiring, status.erased), (1, 1));
        assert!(status.next.is_some_and(|next| next > now_ms() + 50_000), "{status:?}");
    }

    /// A message of yours that goes with a timer is given its time to go once WhatsApp has it,
    /// which wakes the timer as any sooner time committed does, so it goes then with nothing else
    /// committed (round four's T8): waiting for a later message, the timer would sleep on to that.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_of_yours_given_its_time_once_whatsapp_has_it_wakes_the_timer_and_goes_at_it() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, vec![Change::Message(message("later", now_ms() + 3_600_000))]).await.unwrap();
        // It goes a second after WhatsApp has it, as the timer it goes with says.
        let goes_as = e2e::Message {
            extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
                text: Some("gone a second after it went".into()),
                context_info: Some(Box::new(e2e::ContextInfo { expiration: Some(1), ..Default::default() })),
                ..Default::default()
            })),
            ..Default::default()
        };
        let mine =
            Message { sender: None, mine: true, status: Some("pending"), expires_at: None, ..message("mine", 0) };
        let enqueue = Step::Enqueue {
            message: Box::new(mine),
            kind: "text",
            raw: goes_as.encode_to_vec(),
            key: None,
            written: Written::default(),
        };
        store.apply(Lane::Live, vec![Change::Outbox(enqueue)]).await.unwrap();
        let expiry = start(store.clone());
        tokio::time::sleep(Duration::from_millis(300)).await;
        let outbox: i64 = store
            .reads()
            .query(Priority::Interactive, |db| Ok(db.query_row("SELECT n FROM outbox", [], |r| r.get(0))?))
            .await
            .unwrap();
        let answered = now_ms();
        store.apply(Lane::Live, vec![Change::Outbox(Step::Sent { outbox })]).await.unwrap();
        assert!(gone(&store, "mine", Duration::from_secs(10)).await, "the timer slept on to the later one");
        assert!(now_ms() >= answered + 1_000, "it went no sooner than a second after WhatsApp had it");
        assert!(there(&store, "later").await);
        let until = Instant::now() + Duration::from_secs(5);
        while expiry.status(&store).erased < 1 && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(expiry.status(&store).erased, 1, "the timer erased it");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn what_came_due_while_the_timer_wasnt_running_goes_once_it_starts_none_of_it_synced() {
        let before = tempfile::tempdir().unwrap();
        let stopped = Store::open(before.path()).unwrap();
        let soon = now_ms() + 200;
        let changes = (0..70).map(|k| Change::Message(message(&format!("due{k:02}"), soon + k))).collect();
        stopped.apply(Lane::Live, changes).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        // jawd started again on what the stop left, which no writer of this run has noted anything of.
        let home = tempfile::tempdir().unwrap();
        let db = rusqlite::Connection::open(before.path().join("jaw.db")).unwrap();
        db.execute("VACUUM INTO ?1", [home.path().join("jaw.db").to_str().unwrap()]).unwrap();
        let store = Store::open(home.path()).unwrap();
        let synced = store.storage().synced_commits;
        let expiry = start(store.clone());
        assert!(gone(&store, "due69", Duration::from_secs(10)).await, "the last of three batches");
        // The writer counts a transaction's erasures once it has committed, as a reader may
        // already see them gone.
        let until = Instant::now() + Duration::from_secs(5);
        while expiry.status(&store).erased < 70 && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(expiry.status(&store), DisappearingStatus { erased: 70, ..DisappearingStatus::default() });
        assert_eq!(store.storage().synced_commits - synced, 0, "an erasure acknowledges nothing, so none is synced");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_backlog_of_erasures_goes_in_short_transactions_and_a_live_write_goes_in_between_them() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let due = now_ms() + 60_000;
        // Long messages, each erasure taking its 9,000 words out of search's segments.
        let long = |k: i64| {
            let words: String = (0..9_000).map(|w| format!("long{k}word{w} ")).collect();
            Message { text: Some(words), ..message(&format!("long{k:02}"), due + k) }
        };
        store.apply(Lane::Live, (0..32).map(|k| Change::Message(long(k))).collect()).await.unwrap();
        store.indexed().await.unwrap();
        let synced = store.storage().synced_commits;
        let erasing = {
            let store = store.clone();
            tokio::spawn(async move { erase_due(&store, due + 1_000, &[]).await.unwrap() })
        };
        while there(&store, "long00").await {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        store.apply(Lane::Live, vec![Change::Message(timeless("live"))]).await.unwrap();
        let left: i64 = store
            .reads()
            .query(Priority::Interactive, |db| {
                Ok(db.query_row("SELECT count(*) FROM messages WHERE id LIKE 'long%'", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert!(left >= 8, "the live write waited for a short transaction, not for the backlog: {left} left");
        assert_eq!(erasing.await.unwrap().erased, 32);
        assert_eq!(store.storage().synced_commits, synced, "an erasure acknowledges nothing, so none is synced");
    }

    /// A message without a time to go, which wakes the timer only by its commit.
    fn timeless(id: &str) -> Message {
        Message { expires_at: None, ..message(id, 0) }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_whose_time_passed_while_the_timers_sleep_didnt_count_it_goes_at_the_next_commit() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        store.apply(Lane::Live, vec![Change::Message(message("asleep", now_ms() + 3_600_000))]).await.unwrap();
        let expiry = start(store.clone());
        tokio::time::sleep(Duration::from_millis(300)).await;
        // The Mac slept an hour and a minute: its wall clock moved on, the timer's sleep didn't.
        expiry.skew.store(3_660_000, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(there(&store, "asleep").await, "nothing has woken the timer yet");
        store.apply(Lane::Live, vec![Change::Message(timeless("wakes"))]).await.unwrap();
        assert!(gone(&store, "asleep", Duration::from_secs(5)).await, "a commit had it look at the wall clock");
        assert!(there(&store, "wakes").await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn with_nothing_to_erase_the_timer_waits_for_the_writer_alone_however_much_is_committed() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let expiry = start(store.clone());
        tokio::time::sleep(Duration::from_millis(300)).await;
        let looks = expiry.looks.load(Ordering::Relaxed);
        for k in 0..20 {
            store.apply(Lane::Live, vec![Change::Message(timeless(&format!("m{k:02}")))]).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        assert_eq!(expiry.looks.load(Ordering::Relaxed), looks, "neither commits nor time woke it");
        store.apply(Lane::Live, vec![Change::Message(message("goes", now_ms() + 100))]).await.unwrap();
        assert!(gone(&store, "goes", Duration::from_secs(5)).await, "a time to go did");
    }
}
