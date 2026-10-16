//! Two one-to-one chats found to be one person's, once their phone number and LID are joined, are
//! merged: the writer moves what's small at once and sends new writes to the chat kept, and this
//! task moves the messages a short background transaction at a time until none are left.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// Wakes the merging task after changes that can join two people.
#[derive(Clone, Default)]
pub struct Merges(Arc<Notify>);

impl Merges {
    pub fn poke(&self) {
        self.0.notify_one();
    }
}

/// How long to wait before trying a merge step again that couldn't be stored.
const RETRY: Duration = Duration::from_secs(1);

/// Merges chats whenever a merge is waiting, until jawd stops.
pub async fn run(store: Store, merges: Merges) {
    loop {
        match waiting(&store).await {
            Ok(true) => {
                if let Err(e) = store.apply(Lane::Background, vec![Change::MergeStep]).await {
                    eprintln!("jawd: can't merge a chat into another yet, so it's tried again in {RETRY:?}: {e:#}");
                    tokio::time::sleep(RETRY).await;
                }
            }
            Ok(false) => merges.0.notified().await,
            Err(e) => {
                eprintln!("jawd: can't read which chats are being merged: {e:#}");
                tokio::time::sleep(RETRY).await;
            }
        }
    }
}

async fn waiting(store: &Store) -> anyhow::Result<bool> {
    store
        .reads()
        .query(Priority::Background, |db| {
            Ok(db.query_row("SELECT EXISTS (SELECT 1 FROM chat_merges)", [], |r| r.get(0))?)
        })
        .await
}
