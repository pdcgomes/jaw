//! The backfills migrations leave, done in the background lane while jaw runs: each a short
//! transaction at a time, from where the last left off as `backfills` keeps it, until its row goes;
//! so far the links of the messages jaw.db held before links were kept, the options of its polls,
//! the messages jawd couldn't show when they came, converted again, and the copies of the files of
//! the messages it held before media was kept. Code works with the data either way meanwhile. A
//! step that fails is kept in quarantine and tried again later and later.

use std::time::Duration;

use crate::store::{Change, Lane, Store, links};
use crate::work::Priority;

/// How many steps go to the writer at once; it takes as many in each transaction as fit.
const STEPS: usize = 64;
/// How long a failing backfill waits before it's tried again, at first and at most.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MOST: Duration = Duration::from_secs(300);

/// Does the backfills left, one after another, until none are.
pub async fn run(store: Store) {
    let mut pause = RETRY_FIRST;
    loop {
        let left = store
            .reads()
            .query(Priority::Background, |db| {
                let mut left = Vec::new();
                for &name in links::BACKFILLS {
                    if db.prepare_cached("SELECT 1 FROM backfills WHERE name = ?1")?.exists([name])? {
                        left.push(name);
                    }
                }
                Ok(left)
            })
            .await;
        let failed = match left {
            Ok(left) if left.is_empty() => return,
            Ok(left) => {
                let steps = left.iter().flat_map(|&name| std::iter::repeat_n(Change::Backfill(name), STEPS)).collect();
                match store.apply(Lane::Background, steps).await {
                    Ok(applied) if applied.quarantined == 0 => None,
                    Ok(_) => Some("a step failed and is kept in quarantine".to_string()),
                    Err(e) => Some(format!("{e:#}")),
                }
            }
            Err(e) => Some(format!("{e:#}")),
        };
        match failed {
            None => pause = RETRY_FIRST,
            Some(why) => {
                eprintln!("jawd: a backfill can't go on yet ({why}), so it's tried again in {pause:?}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(RETRY_MOST);
            }
        }
    }
}
