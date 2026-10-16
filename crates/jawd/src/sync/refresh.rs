//! What jawd brings in once it's connected, each run: the contacts and push names whatsmeow keeps,
//! every group with its members, app state never synced into this jaw.db (an account linked
//! before jawd kept app state has it all in whatsmeow and none here), the phone numbers of LIDs
//! jaw has none for, and avatars for the chats at the top of the list.

use super::Avatars;
use crate::convert::people;
use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// Where sync remembers each app-state collection's version once it's synced, after this prefix.
pub const APP_STATE: &str = "app_state:";
/// whatsmeow's app-state collections.
const COLLECTIONS: [&str; 5] = ["critical_block", "critical_unblock_low", "regular_high", "regular", "regular_low"];
/// The most LIDs asked about at once.
const LIDS: usize = 5_000;

/// Which app-state collections to fetch again in full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resync {
    /// None: the account was linked in this run, so whatsmeow syncs its app state in full by itself.
    None,
    /// Those jaw.db has never had in full.
    Missing,
    /// All of them: jawd didn't stop cleanly, so whatsmeow may have moved a version on past events
    /// jawd never stored. Applying them again changes nothing that's already right.
    All,
}

/// Brings the account's people, groups and app state in.
pub async fn run(store: Store, engine: Supervisor, avatars: Avatars, resync: Resync) {
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
    if resync != Resync::None {
        let names: Vec<String> = COLLECTIONS
            .iter()
            .filter(|name| resync == Resync::All || matches!(store.remembered(&format!("{APP_STATE}{name}")), Ok(None)))
            .map(|name| name.to_string())
            .collect();
        if !names.is_empty() {
            eprintln!("jawd: fetching app state again in full ({resync:?}): {}", names.join(", "));
            if let Err(e) = engine.call(pb::command::Kind::AppStateResync(pb::AppStateResync { names })).await {
                eprintln!("jawd: can't fetch the app state: {e}");
            }
        }
    }
    lids(&store, &engine).await;
    avatars.seed(&store).await;
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
