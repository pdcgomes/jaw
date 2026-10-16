//! Two one-to-one chats found to be one person's, once their phone number and LID are joined, are
//! merged: the writer moves what's small at once and sends new writes to the chat kept, and this
//! task moves the messages a short background transaction at a time until none are left. A step
//! that fails is kept in quarantine and tried again later and later; a merge whose steps keep
//! failing is left until jawd next starts, and `status` counts it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::Shared;
use crate::store::{Change, Lane, Merging, Store, merge_reference};
use crate::work::Priority;

/// Wakes the merging task after changes that can join two people.
#[derive(Clone, Default)]
pub struct Merges(Arc<Notify>);

impl Merges {
    pub fn poke(&self) {
        self.0.notify_one();
    }
}

/// How a failing merge is tried again: after `first`, doubling to `most`, and left after `tries`
/// failures in a row.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub first: Duration,
    pub most: Duration,
    pub tries: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { first: Duration::from_secs(1), most: Duration::from_secs(60), tries: 8 }
    }
}

/// A merge whose last steps failed: how many in a row, and when it may be tried again.
struct Failing {
    failures: u32,
    next: Instant,
}

/// Merges chats whenever a merge is waiting, until jawd stops.
pub async fn run(store: Store, merges: Merges, live: Shared, timing: Timing) {
    let mut failing: HashMap<i64, Failing> = HashMap::new();
    loop {
        let waiting = match waiting(&store).await {
            Ok(waiting) => waiting,
            Err(e) => {
                eprintln!("jawd: can't read which chats are being merged: {e:#}");
                tokio::time::sleep(timing.first).await;
                continue;
            }
        };
        failing.retain(|gone, _| waiting.iter().any(|m| m.gone == *gone));
        let stuck = failing.values().filter(|f| f.failures >= timing.tries).count();
        live.lock().expect("live state").merges_stuck = stuck as u64;
        let now = Instant::now();
        let due = waiting
            .into_iter()
            .find(|m| failing.get(&m.gone).is_none_or(|f| f.failures < timing.tries && f.next <= now));
        let Some(merging) = due else {
            match failing.values().filter(|f| f.failures < timing.tries).map(|f| f.next).min() {
                Some(next) => {
                    let _ = tokio::time::timeout_at(next, merges.0.notified()).await;
                }
                None => merges.0.notified().await,
            }
            continue;
        };
        let gone = merging.gone;
        let reference = merging.reference.clone();
        let failed = match store.apply(Lane::Background, vec![Change::MergeStep(Some(merging))]).await {
            Ok(applied) if applied.quarantined == 0 => None,
            Ok(_) => Some("the step failed and is kept in quarantine".to_string()),
            Err(e) => Some(format!("{e:#}")),
        };
        match failed {
            None => {
                failing.remove(&gone);
            }
            Some(why) => {
                let entry = failing.entry(gone).or_insert(Failing { failures: 0, next: now });
                entry.failures += 1;
                if entry.failures >= timing.tries {
                    eprintln!(
                        "jawd: merging {reference} failed {} times in a row ({why}), so it's left until jawd starts again",
                        entry.failures
                    );
                } else {
                    let pause = timing.first.saturating_mul(1 << (entry.failures - 1).min(16)).min(timing.most);
                    entry.next = Instant::now() + pause;
                    eprintln!("jawd: can't merge {reference} yet ({why}), so it's tried again in {pause:?}");
                }
            }
        }
    }
}

/// The merges waiting, first made first.
async fn waiting(store: &Store) -> anyhow::Result<Vec<Merging>> {
    store
        .reads()
        .query(Priority::Background, |db| {
            Ok(db
                .prepare_cached(
                    "SELECT m.gone, g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep
                    ORDER BY m.gone",
                )?
                .query_map([], |r| {
                    let (gone, gone_jid, keep_jid): (i64, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
                    Ok(Merging { gone, reference: merge_reference(&gone_jid, &keep_jid) })
                })?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await
}

#[cfg(test)]
mod tests;
