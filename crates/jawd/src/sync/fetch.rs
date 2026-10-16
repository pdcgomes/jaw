//! History blobs, fetched a blob at a time as the phone offers them and converted in the
//! background lane a conversation at a time. The message that offered a blob stays in the inbox
//! until the blob's rows are in, and a blob already downloaded to the spool is used again, so a
//! restart in the middle of history loses nothing and fetches nothing twice.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use jaw_proto::HistoryProgress;
use prost::Message as _;
use tokio::sync::mpsc;
use wa_proto::wa_web_protobufs_history_sync::{HistorySync, history_sync::HistorySyncType};

use super::Shared;
use crate::convert;
use crate::engine::supervisor::Supervisor;
use crate::engine::{EngineError, pb};
use crate::store::{Change, Drained, Lane, Store};

/// A history notification found in the inbox.
pub struct Offer {
    /// Its inbox row, removed once the blob's rows are in.
    pub seq: i64,
    /// The message's ID, which names the blob in the spool.
    pub id: String,
    /// WAWebProtobufsE2E.HistorySyncNotification.
    pub notification: Vec<u8>,
}

/// Changes handed to the writer at once, so a big conversation doesn't wait as one batch.
const CHUNK: usize = 2_000;
/// How long to wait before asking the engine again after it couldn't fetch, at first and at most.
const RETRY_FIRST: Duration = Duration::from_secs(2);
const RETRY_MOST: Duration = Duration::from_secs(60);

/// Fetches and stores each blob offered, in order, until jawd stops.
pub async fn run(
    mut offers: mpsc::UnboundedReceiver<Offer>,
    store: Store,
    engine: Supervisor,
    live: Shared,
    spool: PathBuf,
) {
    while let Some(offer) = offers.recv().await {
        let mut pause = RETRY_FIRST;
        loop {
            match one(&offer, &store, &engine, &live, &spool).await {
                Ok(()) => break,
                Err(Failed::Later(why)) => {
                    eprintln!("jawd: history {} waits: {why}; trying again in {pause:?}", offer.id);
                    tokio::time::sleep(pause).await;
                    pause = (pause * 2).min(RETRY_MOST);
                }
                Err(Failed::Never(why)) => {
                    eprintln!("jawd: history {} can't be had, so its notification is dropped: {why}", offer.id);
                    if let Err(e) =
                        store.apply(Lane::Background, vec![Change::Drained(Drained::Inbox(offer.seq))]).await
                    {
                        eprintln!("jawd: can't drop history {}'s notification: {e:#}", offer.id);
                    }
                    break;
                }
            }
        }
    }
}

enum Failed {
    /// Trying again later might work: the engine is restarting, or WhatsApp didn't answer.
    Later(String),
    Never(String),
}

async fn one(
    offer: &Offer,
    store: &Store,
    engine: &Supervisor,
    live: &Shared,
    spool: &std::path::Path,
) -> Result<(), Failed> {
    let path = spool.join(format!("{}.pb", offer.id.replace(['/', '\\'], "_")));
    let started = Instant::now();
    if !path.exists() {
        let command = pb::command::Kind::HistoryDownload(pb::HistoryDownload {
            notification: offer.notification.clone(),
            path: path.display().to_string(),
        });
        match engine.call(command).await {
            Ok(pb::reply::Kind::HistoryFile(_)) => {}
            Ok(other) => return Err(Failed::Never(format!("the engine answered with {other:?}"))),
            Err(EngineError::Failed { message, retry: false }) => return Err(Failed::Never(message)),
            Err(e) => return Err(Failed::Later(e.to_string())),
        }
    }
    let downloaded = started.elapsed();
    let data =
        tokio::fs::read(&path).await.map_err(|e| Failed::Later(format!("can't read {}: {e}", path.display())))?;
    let bytes = data.len() as u64;
    let me = live.lock().expect("live state").me.clone();
    let decoded = tokio::task::spawn_blocking(move || {
        let history = HistorySync::decode(data.as_slice())?;
        let parts = convert::history::blob(&history, &me);
        Ok::<_, prost::DecodeError>((
            history.sync_type,
            history.chunk_order,
            history.progress,
            history.conversations.len(),
            parts,
        ))
    })
    .await
    .map_err(|e| Failed::Later(format!("converting it stopped: {e}")))?;
    let (kind, chunk, progress, conversations, parts) = match decoded {
        Ok(decoded) => decoded,
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(Failed::Later(format!("the blob doesn't decode, so it's fetched again: {e}")));
        }
    };
    let messages: usize = parts.iter().flatten().filter(|c| matches!(c, Change::Message(_))).count();
    let storing = Instant::now();
    let mut transactions = 0;
    for part in parts {
        let mut part = part;
        while !part.is_empty() {
            let rest = part.split_off(part.len().min(CHUNK));
            let applied = store.apply(Lane::Background, part).await.map_err(|e| Failed::Later(format!("{e:#}")))?;
            transactions += applied.transactions;
            part = rest;
        }
    }
    store
        .apply(Lane::Background, vec![Change::Drained(Drained::Inbox(offer.seq))])
        .await
        .map_err(|e| Failed::Later(format!("{e:#}")))?;
    let stored = storing.elapsed();
    let _ = tokio::fs::remove_file(&path).await;
    let kind = HistorySyncType::try_from(kind).map(|t| t.as_str_name()).unwrap_or("?");
    eprintln!(
        "jawd: history {kind} chunk {}: {conversations} chats, {messages} messages, {:.1} MB, fetched in {:.2} s and stored in {:.2} s ({:.0} a second, {transactions} transactions); the phone says {}%",
        chunk.unwrap_or_default(),
        bytes as f64 / 1e6,
        downloaded.as_secs_f64(),
        stored.as_secs_f64(),
        messages as f64 / stored.as_secs_f64().max(1e-6),
        progress.unwrap_or_default()
    );
    let mut live = live.lock().expect("live state");
    let history = live.history.get_or_insert_with(HistoryProgress::default);
    history.blobs += 1;
    history.conversations += conversations as u64;
    history.messages += messages as u64;
    history.bytes += bytes;
    history.download_seconds += downloaded.as_secs_f64();
    history.store_seconds += stored.as_secs_f64();
    history.progress = history.progress.max(progress.unwrap_or_default());
    Ok(())
}
