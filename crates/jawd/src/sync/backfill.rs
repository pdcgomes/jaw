//! The backfills migrations leave, done in the background lane while jaw runs: each a short
//! transaction at a time, from where the last left off as `backfills` keeps it, until its row goes;
//! so far the links of the messages jaw.db held before links were kept, the options of its polls,
//! the messages jawd couldn't show when they came, converted again, and the copies of the files of
//! the messages it held before media was kept. Code works with the data either way meanwhile. A
//! step that fails is kept in quarantine and tried again later and later.
//! The answers and edits waiting for an event beyond the few its own transaction takes are taken
//! here too, a step at a time as `openings` keeps them, once the writer says a commit has left some.

use std::time::Duration;

use crate::store::{Change, Lane, Store, links};
use crate::work::Priority;

/// How many steps go to the writer at once; it takes as many in each transaction as fit.
const STEPS: usize = 64;
/// How long a failing backfill waits before it's tried again, at first and at most.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MOST: Duration = Duration::from_secs(300);

/// Does the backfills left, and what's left to take for events, one after another, waiting once
/// none are until the writer says a commit has left more for events, for as long as jawd runs.
pub async fn run(store: Store) {
    let mut pause = RETRY_FIRST;
    let opening = store.opening();
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
                let opening =
                    db.prepare_cached("SELECT EXISTS (SELECT 1 FROM openings)")?.query_row([], |r| r.get(0))?;
                Ok((left, opening))
            })
            .await;
        let failed = match left {
            Ok((left, false)) if left.is_empty() => {
                opening.notified().await;
                continue;
            }
            Ok((left, opening)) => {
                let mut steps: Vec<Change> =
                    left.iter().flat_map(|&name| std::iter::repeat_n(Change::Backfill(name), STEPS)).collect();
                if opening {
                    steps.extend(std::iter::repeat_n(Change::Opening, STEPS));
                }
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
