//! What jawd brings in each time an engine connects: the contacts and push names whatsmeow keeps,
//! every group with its members, every app-state collection again in full, the phone numbers of
//! LIDs jaw has none for, and avatars for the chats at the top of the list.

use super::{Avatars, Merges};
use crate::convert::people;
use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// whatsmeow's app-state collections.
pub const COLLECTIONS: [&str; 5] = ["critical_block", "critical_unblock_low", "regular_high", "regular", "regular_low"];
/// The most LIDs asked about at once.
const LIDS: usize = 5_000;

/// Brings the account's people, groups and, when `resync`, app state in.
pub async fn run(store: Store, engine: Supervisor, avatars: Avatars, merges: Merges, resync: bool) {
    match engine.call(pb::command::Kind::Contacts(pb::Contacts {})).await {
        Ok(pb::reply::Kind::ContactList(list)) => store_changes(&store, people::contacts(&list), "contacts").await,
        Ok(other) => eprintln!("jawd: the engine answered contacts with {other:?}"),
        Err(e) => eprintln!("jawd: can't read the contacts whatsmeow keeps: {e}"),
    }
    match engine.call(pb::command::Kind::Groups(pb::Groups {})).await {
        Ok(pb::reply::Kind::GroupList(list)) => {
            let changes = list.groups.iter().flat_map(people::group).collect();
            store_changes(&store, changes, "groups").await;
        }
        Ok(other) => eprintln!("jawd: the engine answered groups with {other:?}"),
        Err(e) => eprintln!("jawd: can't list the groups: {e}"),
    }
    if resync && !resynced(&engine).await {
        tokio::spawn(resync_again(engine.clone()));
    }
    lids(&store, &engine).await;
    merges.poke();
    avatars.seed(&store).await;
}

/// Asks the engine to fetch every app-state collection again in full, saying whether it did. It
/// waits for its turn with jaw's own patches, and holds it until every mutation is kept.
async fn resynced(engine: &Supervisor) -> bool {
    let names: Vec<String> = COLLECTIONS.iter().map(|name| name.to_string()).collect();
    let turn = engine.app_state().take().await;
    let started = std::time::Instant::now();
    let fetched = engine.call(pb::command::Kind::AppStateResync(pb::AppStateResync { names })).await;
    drop(turn);
    match fetched {
        Ok(_) => {
            eprintln!("jawd: fetched app state again in full in {:.1} s", started.elapsed().as_secs_f64());
            true
        }
        Err(e) => {
            eprintln!("jawd: can't fetch the app state again: {e}");
            false
        }
    }
}

/// The first wait before fetching app state again after it failed; it doubles to RESYNC_MOST.
const RESYNC_FIRST: std::time::Duration = std::time::Duration::from_secs(1);
const RESYNC_MOST: std::time::Duration = std::time::Duration::from_secs(60);
/// How many times a failed refetch is tried again before it waits for the next engine.
const RESYNC_TRIES: u32 = 10;

/// Tries the full refetch again while the engine that failed it runs, since until it succeeds
/// what an engine stopped in the middle of left behind stays missing. An engine started again
/// fetches it itself.
async fn resync_again(engine: Supervisor) {
    let restarts = engine.status().restarts;
    let mut pause = RESYNC_FIRST;
    for _ in 0..RESYNC_TRIES {
        tokio::time::sleep(pause).await;
        if engine.status().restarts != restarts || resynced(&engine).await {
            return;
        }
        pause = (pause * 2).min(RESYNC_MOST);
    }
    eprintln!("jawd: gave up fetching the app state again; the next engine to connect fetches it");
}

async fn store_changes(store: &Store, changes: Vec<Change>, what: &str) {
    if let Err(e) = store.apply(Lane::Background, changes).await {
        eprintln!("jawd: can't store the {what}: {e:#}");
    }
}

/// Asks whatsmeow's LID map for the phone numbers of people jaw knows only by LID.
async fn lids(store: &Store, engine: &Supervisor) {
    let lonely: anyhow::Result<Vec<String>> = store
        .reads()
        .query(Priority::Background, |db| {
            let lids = db
                .prepare_cached(
                    "SELECT j.jid FROM person_jids j WHERE j.jid LIKE '%@lid' AND NOT EXISTS (
                        SELECT 1 FROM person_jids k WHERE k.person = j.person AND k.jid LIKE '%@s.whatsapp.net')
                    LIMIT ?1",
                )?
                .query_map([LIDS as i64], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(lids)
        })
        .await;
    let lids = match lonely {
        Ok(lids) if lids.is_empty() => return,
        Ok(lids) => lids,
        Err(e) => {
            eprintln!("jawd: can't read who's known only by LID: {e:#}");
            return;
        }
    };
    match engine.call(pb::command::Kind::LidLookup(pb::LidLookup { lids })).await {
        Ok(pb::reply::Kind::LidPairs(pairs)) => store_changes(store, people::lid_pairs(&pairs), "LIDs' numbers").await,
        Ok(other) => eprintln!("jawd: the engine answered a LID lookup with {other:?}"),
        Err(e) => eprintln!("jawd: can't read whatsmeow's LID map: {e}"),
    }
}
