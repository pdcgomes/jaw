//! History blobs, fetched a blob at a time as the phone offers them and converted in the
//! background lane a conversation at a time. The message that offered a blob stays in the inbox
//! until the blob's rows are in, and a blob already downloaded to the spool is used again, so a
//! restart in the middle of history loses nothing and fetches nothing twice. What a blob carries
//! besides the messages kept as rows is kept raw in `history_kept` before the blob goes.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jaw_proto::HistoryProgress;
use prost::Message as _;
use tokio::sync::mpsc;
use wa_proto::wa_web_protobufs_history_sync::{HistorySync, history_sync::HistorySyncType};

use super::{Merges, Shared};
use crate::convert;
use crate::engine::supervisor::Supervisor;
use crate::engine::{EngineError, pb};
use crate::store::{Change, Drained, Lane, Packed, Store};

/// What the converter hands the fetcher.
pub enum Work {
    Offer(Offer),
    /// Every notification left in the inbox from before has been offered, so spool files no
    /// offer names are left from blobs whose rows are already in.
    Offered,
}

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

/// An offer waiting its turn: one that failed waits behind the others until its pause is over.
struct Waiting {
    offer: Offer,
    pause: Duration,
    until: Instant,
}

/// Fetches and stores each blob offered, oldest first, until jawd stops. A blob that can't be had
/// yet goes behind the others and is tried again after a pause that doubles each time, so it never
/// holds up the blobs after it; one that can never be had has its notification kept in quarantine.
pub async fn run(
    mut offers: mpsc::UnboundedReceiver<Work>,
    store: Store,
    engine: Supervisor,
    live: Shared,
    spool: PathBuf,
    merges: Merges,
) {
    let mut named = HashSet::new();
    let mut swept = false;
    let mut queue: VecDeque<Waiting> = VecDeque::new();
    loop {
        let now = Instant::now();
        let ready = queue.iter().position(|waiting| waiting.until <= now);
        let work = match (ready, queue.iter().map(|waiting| waiting.until).min()) {
            (Some(_), _) => offers.try_recv().ok(),
            (None, Some(until)) => tokio::select! {
                work = offers.recv() => match work {
                    Some(work) => Some(work),
                    None => return,
                },
                () = tokio::time::sleep_until(until.into()) => None,
            },
            (None, None) => match offers.recv().await {
                Some(work) => Some(work),
                None => return,
            },
        };
        match work {
            Some(Work::Offer(offer)) => {
                if !swept {
                    named.insert(spool_name(&offer.id));
                }
                queue.push_back(Waiting { offer, pause: RETRY_FIRST, until: now });
                continue;
            }
            Some(Work::Offered) => {
                if !swept {
                    swept = true;
                    sweep(&spool, &named).await;
                }
                continue;
            }
            None => {}
        }
        let Some(mut waiting) = ready.and_then(|i| queue.remove(i)) else { continue };
        let offer = &waiting.offer;
        match one(offer, &store, &engine, &live, &spool).await {
            Ok(()) => merges.poke(),
            Err(Failed::Later(why)) => {
                eprintln!(
                    "jawd: history {} waits: {why}; the blobs after it go first, and it's tried again in {:?}",
                    offer.id, waiting.pause
                );
                waiting.until = Instant::now() + waiting.pause;
                waiting.pause = (waiting.pause * 2).min(RETRY_MOST);
                queue.push_back(waiting);
            }
            Err(Failed::Never(why)) => {
                eprintln!("jawd: history {} can't be had: {why}; its notification is kept in quarantine", offer.id);
                let failed =
                    vec![Change::Quarantined { source: "inbox", reference: offer.seq.to_string(), error: why }];
                if let Err(e) = store.apply(Lane::Background, failed).await {
                    eprintln!("jawd: can't keep history {}'s notification in quarantine yet: {e:#}", offer.id);
                    waiting.until = Instant::now() + waiting.pause;
                    queue.push_back(waiting);
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

/// A blob's file name in the spool.
fn spool_name(id: &str) -> String {
    format!("{}.pb", id.replace(['/', '\\'], "_"))
}

/// Removes spool files that no notification in the inbox names: blobs whose rows are in, and
/// .part files a download left behind.
async fn sweep(spool: &Path, named: &HashSet<String>) {
    let Ok(mut entries) = tokio::fs::read_dir(spool).await else { return };
    let mut removed = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if !named.contains(&name) && tokio::fs::remove_file(entry.path()).await.is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        eprintln!("jawd: removed {removed} files from the spool whose history is already in");
    }
}

async fn one(offer: &Offer, store: &Store, engine: &Supervisor, live: &Shared, spool: &Path) -> Result<(), Failed> {
    let path = spool.join(spool_name(&offer.id));
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
        let rest = Packed::new(&convert::history::remainder(&history, &parts).encode_to_vec());
        Ok::<_, prost::DecodeError>((
            history.sync_type,
            history.chunk_order,
            history.progress,
            history.conversations.len(),
            parts,
            rest,
        ))
    })
    .await
    .map_err(|e| Failed::Later(format!("converting it stopped: {e}")))?;
    let (kind, chunk, progress, conversations, parts, rest) = match decoded {
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
    // The blob goes only once what it carried besides its messages, and the notification's removal,
    // are on disk; a power cut before that brings the notification back, and the blob is still
    // there for it.
    store
        .apply_durable(vec![
            Change::HistoryKept { id: offer.id.clone(), raw: rest },
            Change::Drained(Drained::Inbox(offer.seq)),
        ])
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
