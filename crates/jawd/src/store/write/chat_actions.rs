//! Chat actions from jaw: a chat pinned, archived, muted, or marked unread or read, and a message
//! starred, from any interface. Each is done to its row at once, with a new version and its event,
//! and the app-state patches that tell your other devices go into the outbox in the same
//! transaction, to go after the chat's earlier sends, as a deletion for you does. What's already so
//! changes nothing and queues nothing, so asking again after a lost answer, or two interfaces asking
//! at once, end the same. Each patch queued is its setting's latest until its echo comes back
//! (`app_state_awaiting`), and WhatsApp sends app state back in the order it took it, so what it
//! sends under the index the patch named meanwhile came before the patch: it's kept raw but not
//! applied (`held_back`), since the patch sets every device as the row already is. WhatsApp may
//! name a one-to-one chat by any of its person's JIDs, and a chat merged away by the JID it had, so
//! a setting waits as jaw keeps its chat (`wait_key`), keeping the index its patch named, and a
//! chat's waits go with it into the chat it merges into (`merging`). WhatsApp keeps such a chat's
//! setting under each index it was named by, and hands them over in no order of their own, so
//! across them the newest stands (`newest_held`), as the owner chose, while under one index
//! WhatsApp's order does. A patch WhatsApp
//! refuses for good, or that runs out of tries, while it's
//! its index's latest, is undone here (`undo`): what it set goes back to how a chat is when nothing
//! has set it, since fetching its collection again brings only what WhatsApp holds, and the fetch
//! then sets it as the phone has it.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension, params};
use wa_proto::wa_web_protobuf_sync_action::SyncActionValue;

use super::outbox::{OUTBOX_MOST, SHOWN_DELETED, app_state_row, waiting};
use super::{
    Amendment, Chat, Doing, Read, Stamps, apply_amendment, chat_jid, located, now_ms, set_read, their_jids,
    upsert_chat, user_jid,
};

/// The most chats pinned at once, as WhatsApp's apps allow; archived chats don't count.
pub const PINNED_MOST: u64 = 3;

const CHAT_STATE: &str =
    "SELECT n, kind, pinned, archived, muted_until, marked_unread, unread FROM chats WHERE id = ?1";
/// Pinned chats sort first, so the unarchived ones are a short range of chats_by_sort. A chat being
/// merged away is left out: the chat it goes into has its pin, so the pair counts once.
const PINNED_ELSEWHERE: &str = "SELECT count(*) FROM chats WHERE archived = 0 AND sort >= '1~' AND n <> ?1
    AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = chats.n)";
const STARRED: &str = "SELECT starred FROM messages WHERE n = ?1";
const AWAITED_AT: &str = "SELECT at FROM app_state_awaiting WHERE key = ?1";
const AWAIT: &str = "INSERT INTO app_state_awaiting (key, named, at, outbox) VALUES (?1, ?2, ?3, ?4)
    ON CONFLICT (key) DO UPDATE SET named = excluded.named, at = excluded.at, outbox = excluded.outbox, echo_by = NULL";
/// What an index waits for: the index jaw's latest patch for it named, the time it carries, where
/// its echo has to be once WhatsApp has taken it, and whether its outbox row is still there.
const AWAITED: &str = "SELECT a.named, a.at, a.echo_by, EXISTS (SELECT 1 FROM outbox o WHERE o.n = a.outbox)
    FROM app_state_awaiting a WHERE a.key = ?1";
const WAITED: &str = "DELETE FROM app_state_awaiting WHERE key = ?1";
/// The waits whose echo had to be at or before an inbox row the converter has drained.
const PASSED: &str = "DELETE FROM app_state_awaiting WHERE echo_by IS NOT NULL AND echo_by <= ?1";
/// The waits whose keys lie in a range, each with all it waits for.
const WAITING_UNDER: &str =
    "SELECT key, named, at, outbox, echo_by FROM app_state_awaiting WHERE key >= ?1 AND key < ?2";
/// A wait under another key, unless one there already waits for a patch as late.
const MOVED: &str = "INSERT INTO app_state_awaiting (key, named, at, outbox, echo_by) VALUES (?1, ?2, ?3, ?4, ?5)
    ON CONFLICT (key) DO UPDATE SET named = excluded.named, at = excluded.at, outbox = excluded.outbox,
        echo_by = excluded.echo_by
    WHERE excluded.at > app_state_awaiting.at";
/// What WhatsApp last sent for each index whose key lies in a range.
const HELD_UNDER: &str = "SELECT key, value FROM app_state WHERE key >= ?1 AND key < ?2";

/// The settings jaw's patches set, each naming its chat second in its index.
const SETTINGS: [&str; 5] = ["pin_v1", "archive", "mute", "markChatAsRead", "star"];

/// A chat action as the writer takes it: the chat as the interface named it, what to do, and where
/// to say what it came to. Each patch is an AppStatePatch the caller made, carrying the time it's
/// done at.
#[derive(Clone, Debug)]
pub struct Act {
    pub chat: String,
    pub does: Does,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub enum Does {
    /// Something set on the chat.
    Set(Setting),
    /// Message `id` in the chat starred, or its star taken off.
    Star { id: String, starred: bool, patch: Patch },
}

#[derive(Clone, Debug)]
pub enum Setting {
    /// Pinned at `at`, the time its patch carries, which is positive, or unpinned when `None`.
    Pin { at: Option<i64>, patch: Patch },
    /// Archived or brought back. Archiving a pinned chat unpins it, as WhatsApp's apps do, and so
    /// does bringing one back while PINNED_MOST others are pinned, with `unpin` after `patch`;
    /// either needs `unpin`, which is otherwise unused.
    Archive { archived: bool, patch: Patch, unpin: Option<Patch> },
    /// Muted until `until`, in milliseconds (`FOREVER` for ever), or unmuted at 0.
    Mute { until: i64, patch: Patch },
    /// Marked unread; or the mark taken off and the chat read here up to `newest`, its newest
    /// message, which owes WhatsApp its read receipts. With none of its messages here, nothing is
    /// read until WhatsApp's echo of the patch reads it, as any device's read does, so a refused
    /// patch leaves the phone's count as it was.
    MarkUnread { unread: bool, newest: Option<String>, patch: Patch },
}

/// An app-state patch to queue: its outbox row's own ID, the AppStatePatch, encoded, and the index
/// and time it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: String,
    pub raw: Vec<u8>,
    /// Its index as `app_state` keys it: the index's strings as JSON.
    pub key: String,
    /// Its SyncActionValue's timestamp.
    pub at: i64,
}

/// What a chat action came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acted {
    /// Done, its patches queued as these outbox rows, in the order they go; `chat` is the chat's JID
    /// as jaw keeps it, and `chat_n` its n.
    Queued { chat: String, chat_n: i64, outbox: Vec<i64> },
    /// It was so already: nothing was written or queued.
    Already,
    /// The chat, or the message, isn't there.
    Gone,
    /// The message a star names is deleted for everyone, or being, so nothing was written.
    Deleted,
    /// The chat is a community's own row, whose groups carry these.
    Community,
    /// Pinning it would pin more than PINNED_MOST chats.
    Pinned,
    /// The outbox holds as many as it may, so nothing was written.
    Full { waiting: u64, failed: u64 },
}

/// Where a chat action says what it came to. The last attempt at its transaction is the one that
/// stands.
#[derive(Clone, Debug, Default)]
pub struct Outcome(Arc<Mutex<Option<Acted>>>);

impl Outcome {
    pub fn take(&self) -> Option<Acted> {
        self.0.lock().expect("a chat action's outcome").take()
    }

    fn put(&self, acted: Acted) {
        *self.0.lock().expect("a chat action's outcome") = Some(acted);
    }
}

/// What a refused patch set, to put back: the chat's field, or the message's star.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Undo {
    Pin { chat: String },
    Archive { chat: String },
    Mute { chat: String },
    MarkUnread { chat: String },
    Star { chat: String, id: String },
}

/// A chat as a chat action finds it.
struct State {
    n: i64,
    community: bool,
    pinned: i64,
    archived: bool,
    muted_until: i64,
    marked: bool,
    unread: i64,
}

fn state(db: &Connection, jid: &str) -> Result<Option<State>> {
    Ok(db
        .prepare_cached(CHAT_STATE)?
        .query_row([jid], |r| {
            Ok(State {
                n: r.get(0)?,
                community: r.get::<_, String>(1)? == "community",
                pinned: r.get(2)?,
                archived: r.get(3)?,
                muted_until: r.get(4)?,
                marked: r.get(5)?,
                unread: r.get(6)?,
            })
        })
        .optional()?)
}

/// Where a chat action came from, for the quarantine.
pub(super) fn provenance(act: &Act) -> (&'static str, String, Option<&[u8]>) {
    let (what, patch) = match &act.does {
        Does::Set(Setting::Pin { patch, .. }) => ("pin", patch),
        Does::Set(Setting::Archive { patch, .. }) => ("archive", patch),
        Does::Set(Setting::Mute { patch, .. }) => ("mute", patch),
        Does::Set(Setting::MarkUnread { patch, .. }) => ("mark", patch),
        Does::Star { patch, .. } => ("star", patch),
    };
    ("chat action", format!("{} {what} {}", act.chat, patch.id), Some(patch.raw.as_slice()))
}

pub(super) fn apply(db: &Connection, stamps: &mut Stamps, act: &Act) -> Result<bool> {
    let jid = chat_jid(db, &act.chat)?;
    let (acted, changed) = match &act.does {
        Does::Set(setting) => set(db, stamps, &jid, setting)?,
        Does::Star { id, starred, patch } => star(db, stamps, &jid, id, *starred, patch)?,
    };
    act.outcome.put(acted);
    Ok(changed)
}

/// Sets `setting` on chat `jid`, unless it's so already, and queues its patches.
fn set(db: &Connection, stamps: &mut Stamps, jid: &str, setting: &Setting) -> Result<(Acted, bool)> {
    let Some(chat) = state(db, jid)? else { return Ok((Acted::Gone, false)) };
    if chat.community {
        return Ok((Acted::Community, false));
    }
    let mut row = Chat { id: jid.to_string(), ..Chat::default() };
    let mut patches = Vec::new();
    let mut read = None;
    match setting {
        Setting::Pin { at, patch } => {
            if let Some(at) = at.filter(|&at| at <= 0) {
                bail!("a chat is pinned at a positive time, not {at}");
            }
            if at.is_some() == (chat.pinned > 0) {
                return Ok((Acted::Already, false));
            }
            if at.is_some() && pinned_elsewhere(db, chat.n)? >= PINNED_MOST {
                return Ok((Acted::Pinned, false));
            }
            row.pinned = Some(at.unwrap_or(0));
            patches.push(patch);
        }
        Setting::Archive { archived, patch, unpin } => {
            if *archived == chat.archived {
                return Ok((Acted::Already, false));
            }
            row.archived = Some(*archived);
            patches.push(patch);
            if chat.pinned > 0 && (*archived || pinned_elsewhere(db, chat.n)? >= PINNED_MOST) {
                let Some(unpin) = unpin else {
                    bail!("archiving or bringing back {jid} unpins it, so it needs its unpin patch");
                };
                row.pinned = Some(0);
                patches.push(unpin);
            }
        }
        Setting::Mute { until, patch } => {
            if *until == chat.muted_until {
                return Ok((Acted::Already, false));
            }
            row.muted_until = Some(*until);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: true, patch, .. } => {
            if chat.marked {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(true);
            patches.push(patch);
        }
        Setting::MarkUnread { unread: false, newest, patch } => {
            if !chat.marked && chat.unread == 0 {
                return Ok((Acted::Already, false));
            }
            row.marked_unread = Some(false);
            read = newest.as_ref().map(|id| Read {
                chat: jid.to_string(),
                ids: vec![id.clone()],
                here: true,
                ..Read::default()
            });
            patches.push(patch);
        }
    }
    if let Some(full) = full(db, patches.len())? {
        return Ok((full, false));
    }
    let mut changed = upsert_chat(db, stamps, &row)?;
    if let Some(read) = &read {
        changed |= set_read(db, stamps, read)?;
    }
    let outbox = patches.iter().map(|patch| queue(db, jid, patch)).collect::<Result<Vec<_>>>()?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat.n, outbox }, changed))
}

/// Queues `patch` for chat `jid` as an outbox row, and keeps it as its setting's latest, which what
/// WhatsApp sends for the setting waits behind until the patch's echo, known by the index it names
/// and its time, comes.
fn queue(db: &Connection, jid: &str, patch: &Patch) -> Result<i64> {
    let outbox = app_state_row(db, jid, &patch.id, &patch.raw)?;
    let index: Vec<String> = serde_json::from_str(&patch.key)?;
    let key = wait_key(db, &index)?;
    if awaited_at(db, &key)? == Some(patch.at) {
        bail!(
            "{key} awaits jaw's patch at {} already, and no two of its patches for an index carry one time",
            patch.at
        );
    }
    db.prepare_cached(AWAIT)?.execute(params![key, serde_json::to_string(&index)?, patch.at, outbox])?;
    Ok(outbox)
}

/// The key a wait for `index` is kept under, which what WhatsApp sends for the index and jaw's
/// patches for it both find: the index with its chat named by the JID jaw keeps the chat by, since
/// WhatsApp may name a one-to-one chat by any of its person's JIDs, and a chat merged away by the JID
/// it had; and a star's without its sender, since its chat and ID find the message. An index of
/// anything else is its key as it is.
pub(super) fn wait_key(db: &Connection, index: &[String]) -> Result<String> {
    let mut index = index.to_vec();
    if index.len() > 1 && SETTINGS.contains(&index[0].as_str()) {
        index[1] = chat_jid(db, &user_jid(&index[1]))?;
        if index[0] == "star" && index.len() == 5 {
            index[4] = "0".into();
        }
    }
    Ok(serde_json::to_string(&index)?)
}

/// The time jaw's latest patch for index `key` carries, while it waits for its echo.
fn awaited_at(db: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(db.prepare_cached(AWAITED_AT)?.query_row([key], |r| r.get(0)).optional()?)
}

/// Whether jaw's latest change to the mark, read or unread, of the chat jaw keeps by `jid` waits for
/// its echo.
pub(super) fn marking(db: &Connection, jid: &str) -> Result<bool> {
    Ok(awaited_at(db, &serde_json::to_string(&["markChatAsRead", jid])?)?.is_some())
}

/// What becomes of what WhatsApp sends for an index, kept raw whatever becomes of it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Held {
    /// Nothing holds it back: it's applied.
    No,
    /// It's held back, and nothing is applied.
    Back,
    /// What WhatsApp holds for the same setting under another of its indexes, naming the chat or a
    /// group message's sender by another JID, is newer, so that's applied instead, which changes
    /// nothing where it shows already: its index and its value.
    Instead { index: Vec<String>, value: Vec<u8> },
}

/// What becomes of what WhatsApp sent for `index`, at inbox row `seq` with its time `at`. While
/// jaw's latest patch for its setting waits for its echo, what WhatsApp sends under the index that
/// patch named came before the patch, since WhatsApp keeps one value an index and sends them back
/// in the order it took them, and the patch sets every device there as the row already is: it's
/// held back. The patch's echo, known by that index and its time, ends the wait; so does a mutation
/// past `echo_by`, where the echo had to be, or one after the patch's outbox row went without
/// going. What comes for a chat's setting under any other index meanwhile, naming the chat or a
/// group message's sender by another JID, and what ends a wait or comes with none, is applied only
/// when it's newer than all that's held for the setting (`newest_held`).
pub(super) fn held_back(db: &Connection, index: &[String], seq: i64, at: i64) -> Result<Held> {
    let key = wait_key(db, index)?;
    let own = serde_json::to_string(index)?;
    let awaited: Option<(String, i64, Option<i64>, bool)> = db
        .prepare_cached(AWAITED)?
        .query_row([&key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    let mut waiting = None;
    if let Some((named, awaited, echo_by, outboxed)) = awaited {
        let after = match echo_by {
            Some(echo_by) => seq > echo_by,
            None => !outboxed,
        };
        if (own == named && at == awaited) || after {
            db.prepare_cached(WAITED)?.execute([&key])?;
        } else if own == named {
            return Ok(Held::Back);
        } else {
            waiting = Some(Waiting { named, at: awaited });
        }
    }
    if !(index.len() > 1 && SETTINGS.contains(&index[0].as_str())) {
        return Ok(Held::No);
    }
    let kept = serde_json::from_str::<Vec<String>>(&key)?.swap_remove(1);
    Ok(newest_held(db, index, at, &kept, waiting.as_ref())?.unwrap_or(Held::No))
}

/// jaw's patch a setting waits for, while what WhatsApp sends for the setting comes under another
/// index: the index the patch named and the time it carries.
struct Waiting {
    named: String,
    at: i64,
}

/// What holds back the setting of a chat `index` names, which WhatsApp sent at time `at`, the chat
/// kept by `kept`: the newest that's held for the setting under some JID of the chat, if it's
/// newer, so what WhatsApp sent applies only when it's newer than each. One is newer than another
/// by its time, and at the same time by its index, the one that sorts last, so of two values held
/// at one millisecond the same stands whichever comes first. WhatsApp keeps a one-to-one chat's
/// setting under each of its person's JIDs that named it, and a group message's star under each of
/// its sender's, and hands them over in no order of their own, a full fetch each as it comes; so
/// across them the newest stands, as the owner chose on 1 November, while under one index
/// WhatsApp's own order does, since it keeps one value there. What's held under each index is what
/// WhatsApp last sent for it, but under the one jaw's waiting patch named (`waiting`), that patch at
/// its time, which WhatsApp ends with there and the chat already shows.
fn newest_held(
    db: &Connection,
    index: &[String],
    at: i64,
    kept: &str,
    waiting: Option<&Waiting>,
) -> Result<Option<Held>> {
    let mut newest = waiting.map(|waiting| (waiting.at, waiting.named.clone(), Held::Back));
    // A star's setting is its message's: its chat, its ID and whether it's yours, whichever JID
    // names its sender.
    let mut setting = index[..if index[0] == "star" { index.len().min(4) } else { 2 }].to_vec();
    let mut jids = their_jids(db, kept)?;
    jids.push(kept.to_string());
    jids.sort_unstable();
    jids.dedup();
    let own = serde_json::to_string(index)?;
    let mut held = db.prepare_cached(HELD_UNDER)?;
    for jid in jids {
        setting[1] = jid;
        // Its keys under the JID: its JSON up to the setting's last closing quote, then the `]` or
        // `,` after it, which both come before `~`.
        let under = serde_json::to_string(&setting)?;
        let from = &under[..under.len() - 1];
        let mut rows = held.query(params![from, format!("{from}~")])?;
        while let Some(row) = rows.next()? {
            let (key, value): (String, Vec<u8>) = (row.get(0)?, row.get(1)?);
            if key == own || waiting.is_some_and(|waiting| key == waiting.named) {
                continue;
            }
            let time = SyncActionValue::decode(value.as_slice()).map_or(0, |v| v.timestamp.unwrap_or_default());
            if newest.as_ref().is_none_or(|(newest, under, _)| (time, key.as_str()) > (*newest, under.as_str())) {
                let index = serde_json::from_str(&key)?;
                newest = Some((time, key, Held::Instead { index, value }));
            }
        }
    }
    Ok(newest.filter(|(time, under, _)| (*time, under.as_str()) > (at, own.as_str())).map(|(.., held)| held))
}

/// Chat `gone` is merging into `keep`, each named by its JID: what waited under `gone`'s JID waits
/// under `keep`'s, as what's said of `gone` goes to `keep` from here on, each wait still with the
/// index its patch named, under which WhatsApp ends with that patch. Of two waits for one setting,
/// the one for jaw's later patch stays, as a later patch's wait takes an earlier one's place.
pub(super) fn merging(db: &Connection, gone: &str, keep: &str) -> Result<()> {
    for setting in SETTINGS {
        // The keys of the setting naming `gone`: its JSON up to the JID's closing quote, then the
        // `]` or `,` after it, which both come before `~`.
        let under = serde_json::to_string(&[setting, gone])?;
        let from = &under[..under.len() - 1];
        let waits: Vec<(String, String, i64, i64, Option<i64>)> = db
            .prepare_cached(WAITING_UNDER)?
            .query_map(params![from, format!("{from}~")], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (key, named, at, outbox, echo_by) in waits {
            db.prepare_cached(WAITED)?.execute([&key])?;
            let mut index: Vec<String> = serde_json::from_str(&key)?;
            index[1] = keep.to_string();
            db.prepare_cached(MOVED)?.execute(params![serde_json::to_string(&index)?, named, at, outbox, echo_by])?;
        }
    }
    Ok(())
}

/// Inbox row `seq` is drained, and the converter has gone past every row before it: an echo that
/// had to be at or before it would have ended its wait, so a wait still on there is over. A patch
/// still to go has no `echo_by`, and waits however far the converter has got.
pub(super) fn passed(db: &Connection, seq: i64) -> Result<()> {
    db.prepare_cached(PASSED)?.execute([seq])?;
    Ok(())
}

/// Stars message `id` in chat `jid`, or takes its star off, unless it's so already, and queues its
/// patch. A message deleted for everyone, or being, isn't starred: the request checks on a reader,
/// but a deletion can commit after that read, and while a chat merges, the copy it read may not be
/// the one found here.
fn star(
    db: &Connection,
    stamps: &mut Stamps,
    jid: &str,
    id: &str,
    starred: bool,
    patch: &Patch,
) -> Result<(Acted, bool)> {
    let Some((chat, n)) = located(db, stamps, jid, id)? else { return Ok((Acted::Gone, false)) };
    if starred && db.prepare_cached(SHOWN_DELETED)?.query_row([n], |r| r.get::<_, bool>(0))? {
        return Ok((Acted::Deleted, false));
    }
    let was: bool = db.prepare_cached(STARRED)?.query_row([n], |r| r.get(0))?;
    if was == starred {
        return Ok((Acted::Already, false));
    }
    if let Some(full) = full(db, 1)? {
        return Ok((full, false));
    }
    let amendment = Amendment::Star { starred };
    let changed =
        apply_amendment(db, stamps, chat, n, Doing { what: &amendment, sender: None, mine: false, at: now_ms() })?;
    let outbox = queue(db, jid, patch)?;
    Ok((Acted::Queued { chat: jid.to_string(), chat_n: chat, outbox: vec![outbox] }, changed))
}

/// How many chats but `n` are pinned and not archived, a person's two chats counting once while
/// they merge.
fn pinned_elsewhere(db: &Connection, n: i64) -> Result<u64> {
    let count: i64 = db.prepare_cached(PINNED_ELSEWHERE)?.query_row([n], |r| r.get(0))?;
    Ok(count.max(0) as u64)
}

/// The outbox's answer when it has no room for `rows` more.
fn full(db: &Connection, rows: usize) -> Result<Option<Acted>> {
    let (waiting, failed) = waiting(db)?;
    Ok((waiting + rows as u64 > OUTBOX_MOST).then_some(Acted::Full { waiting, failed }))
}

/// Puts back what a refused patch set: the chat's field as it is when nothing has set it, or the
/// message unstarred. A chat or message that's gone has nothing to put back, and none is made.
pub(super) fn undo(db: &Connection, stamps: &mut Stamps, undo: &Undo) -> Result<bool> {
    let (chat, unset) = match undo {
        Undo::Star { chat, id } => {
            let Some((chat, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(false) };
            let unstarred = Amendment::Star { starred: false };
            return apply_amendment(
                db,
                stamps,
                chat,
                n,
                Doing { what: &unstarred, sender: None, mine: false, at: now_ms() },
            );
        }
        Undo::Pin { chat } => (chat, Chat { pinned: Some(0), ..Chat::default() }),
        Undo::Archive { chat } => (chat, Chat { archived: Some(false), ..Chat::default() }),
        Undo::Mute { chat } => (chat, Chat { muted_until: Some(0), ..Chat::default() }),
        Undo::MarkUnread { chat } => (chat, Chat { marked_unread: Some(false), ..Chat::default() }),
    };
    let jid = chat_jid(db, chat)?;
    if state(db, &jid)?.is_none() {
        return Ok(false);
    }
    upsert_chat(db, stamps, &Chat { id: jid, ..unset })
}

#[cfg(test)]
mod tests {
    use wa_proto::wa_web_protobuf_sync_action as action;
    use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync};

    use super::*;
    use crate::convert::app_state::FOREVER;
    use crate::store::outbox::{Step, Written};
    use crate::store::schema;
    use crate::store::{Amend, Change, Drained, Message, Person};

    const ANA: &str = "351910000002@s.whatsapp.net";
    const ANA_LID: &str = "100000000002@lid";
    const RUI: &str = "351910000003@s.whatsapp.net";
    const COMMUNITY: &str = "120363000000000009@g.us";
    const AT: i64 = 1_792_000_000_000;

    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        schema::migrate(&mut db).unwrap();
        db
    }

    /// Makes `changes` in one transaction as the writer does, settle and all.
    fn written(db: &Connection, changes: &[Change]) {
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        for change in changes {
            super::super::apply(db, &mut stamps, change).unwrap();
        }
        super::super::settle(db, &mut stamps).unwrap();
    }

    /// A patch standing in for one, for an index of its own.
    fn patch(id: &str) -> Patch {
        Patch { id: id.into(), raw: id.as_bytes().to_vec(), key: serde_json::to_string(&[id]).unwrap(), at: AT }
    }

    fn pin(at: Option<i64>, id: &str) -> Does {
        Does::Set(Setting::Pin { at, patch: patch(id) })
    }

    /// An archive or its undoing, with the unpin patch the handler builds for every one.
    fn archive(archived: bool, id: &str) -> Does {
        Does::Set(Setting::Archive { archived, patch: patch(id), unpin: Some(patch(&format!("{id} unpin"))) })
    }

    /// Does `does` to `chat` as the writer does, and says what it came to.
    fn acted(db: &Connection, chat: &str, does: Does) -> Acted {
        let outcome = Outcome::default();
        written(db, &[Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() })]);
        outcome.take().expect("a chat action says what it came to")
    }

    /// Does `does` to `chat` as the writer does, for a caller's mistake: it fails, saying nothing of
    /// what it came to, and what it did is rolled back, as the writer's savepoint is.
    fn refused(db: &Connection, chat: &str, does: Does) -> anyhow::Error {
        let outcome = Outcome::default();
        let mut stamps = Stamps::load(db).unwrap();
        stamps.begin();
        let act = Change::ChatAction(Act { chat: chat.into(), does, outcome: outcome.clone() });
        let tx = db.unchecked_transaction().unwrap();
        let error = super::super::apply(&tx, &mut stamps, &act).expect_err("a caller's mistake fails");
        drop(tx);
        assert_eq!(outcome.take(), None);
        error
    }

    fn chats(db: &Connection, ids: &[&str]) {
        let changes: Vec<Change> =
            ids.iter().map(|id| Change::Chat(Chat { id: id.to_string(), ..Chat::default() })).collect();
        written(db, &changes);
    }

    /// Chat `id`'s pinned, archived, muted_until, marked_unread, unread and v.
    fn chat(db: &Connection, id: &str) -> (i64, bool, i64, bool, i64, i64) {
        db.query_row(
            "SELECT pinned, archived, muted_until, marked_unread, unread, v FROM chats WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap()
    }

    /// The app-state rows queued, oldest first, as (jid, id, raw).
    fn queued(db: &Connection) -> Vec<(String, String, Vec<u8>)> {
        db.prepare("SELECT jid, id, raw FROM outbox WHERE kind = 'app_state' ORDER BY n")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Puts `rows` reactions in the outbox, waiting to go.
    fn outbox_holding(db: &Connection, rows: u64) {
        for n in 0..rows {
            db.execute(
                "INSERT INTO outbox (jid, id, kind, raw, next_at, created_at) VALUES (?1, ?2, 'reaction', x'', 0, 0)",
                rusqlite::params![ANA, format!("w{n}")],
            )
            .unwrap();
        }
    }

    /// Pins `pinned` from jaw, then has `archived` pinned and archived, as the phone may leave one.
    fn pinned_and_one_archived(db: &Connection, pinned: &[&str], archived: &str) {
        chats(db, pinned);
        for id in pinned {
            assert!(matches!(acted(db, id, pin(Some(AT), id)), Acted::Queued { .. }), "{id}");
        }
        written(
            db,
            &[Change::Chat(Chat { id: archived.into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })],
        );
    }

    /// How many chats are pinned and not archived.
    fn pinned_unarchived(db: &Connection) -> i64 {
        db.query_row("SELECT count(*) FROM chats WHERE pinned > 0 AND archived = 0", [], |r| r.get(0)).unwrap()
    }

    /// Ana's chats under her number and under her LID, both pinned, the second being merged into the
    /// first, and one more chat pinned: three pinned rows for two conversations.
    fn pinned_while_merging(db: &Connection) {
        let pinned = |id: &str| Change::Chat(Chat { id: id.into(), pinned: Some(AT), ..Chat::default() });
        written(db, &[pinned(ANA), pinned(ANA_LID), pinned(RUI)]);
        written(db, &[Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]);
        let merging: i64 = db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0)).unwrap();
        assert_eq!((merging, pinned_unarchived(db)), (1, 3), "Ana's two chats merging, all three pinned");
    }

    fn from_ana(id: &str, at: i64) -> Change {
        Change::Message(Message {
            chat: ANA.into(),
            id: id.into(),
            at,
            sender: Some(ANA.into()),
            kind: "text",
            text: Some(id.into()),
            summary: id.into(),
            live: true,
            ..Message::default()
        })
    }

    /// One of yours in Ana's chat, sent from another of your devices, arriving live.
    fn mine_elsewhere(id: &str, at: i64) -> Change {
        Change::Message(Message {
            chat: ANA.into(),
            id: id.into(),
            at,
            mine: true,
            kind: "text",
            status: Some("sent"),
            text: Some(id.into()),
            summary: id.into(),
            live: true,
            ..Message::default()
        })
    }

    #[test]
    fn a_pin_is_written_with_its_time_a_new_version_and_its_patch_and_asked_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let (.., v) = chat(&db, ANA);
        let Acted::Queued { chat: kept, outbox, .. } = acted(&db, ANA, pin(Some(AT), "p1")) else {
            panic!("not queued")
        };
        assert_eq!((kept.as_str(), outbox.len()), (ANA, 1));
        let (pinned, .., pinned_v) = chat(&db, ANA);
        assert_eq!(pinned, AT, "pinned when jawd said, the time its patch carries");
        assert!(pinned_v > v, "with a new version");
        let event: String = db.query_row("SELECT type FROM events WHERE seq = ?1", [pinned_v], |r| r.get(0)).unwrap();
        assert_eq!(event, "chat", "and its event, in the same transaction");
        assert_eq!(queued(&db), [(ANA.to_string(), "p1".to_string(), b"p1".to_vec())]);
        assert_eq!(acted(&db, ANA, pin(Some(AT + 5), "p2")), Acted::Already, "pinned already");
        let (pinned, .., again_v) = chat(&db, ANA);
        assert_eq!((pinned, again_v, queued(&db).len()), (AT, pinned_v, 1), "nothing written or queued");
        assert!(matches!(acted(&db, ANA, pin(None, "p3")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).0, 0);
        assert_eq!(acted(&db, ANA, pin(None, "p4")), Acted::Already);
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn at_most_three_chats_are_pinned_and_archived_ones_dont_count() {
        let db = db();
        let ids = [
            "351910000011@s.whatsapp.net",
            "351910000012@s.whatsapp.net",
            "351910000013@s.whatsapp.net",
            "351910000014@s.whatsapp.net",
            "351910000015@s.whatsapp.net",
        ];
        chats(&db, &ids);
        written(
            &db,
            &[Change::Chat(Chat { id: ids[4].into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })],
        );
        for (n, id) in ids[..3].iter().enumerate() {
            assert!(matches!(acted(&db, id, pin(Some(AT + n as i64), id)), Acted::Queued { .. }), "{id}");
        }
        assert_eq!(acted(&db, ids[3], pin(Some(AT + 9), "fourth")), Acted::Pinned);
        assert_eq!((chat(&db, ids[3]).0, queued(&db).len()), (0, 3), "not pinned, and nothing queued");
        assert!(matches!(acted(&db, ids[0], pin(None, "unpin")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ids[3], pin(Some(AT + 10), "now")), Acted::Queued { .. }), "a place came free");
    }

    #[test]
    fn archiving_a_pinned_chat_unpins_it_with_a_second_patch_after_the_first() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        let Acted::Queued { outbox, .. } = acted(&db, ANA, archive(true, "a1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 2);
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (0, true), "archived, and unpinned as WhatsApp's apps do");
        let ids: Vec<String> = queued(&db).into_iter().map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["a1", "a1 unpin"], "the archive first, then the unpin");
        assert_eq!(acted(&db, ANA, archive(true, "a2")), Acted::Already);
        let Acted::Queued { outbox, .. } = acted(&db, RUI, archive(true, "r1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "a chat that isn't pinned has nothing to unpin");
        assert!(matches!(acted(&db, ANA, archive(false, "a3")), Acted::Queued { .. }));
        assert!(!chat(&db, ANA).1);
    }

    #[test]
    fn bringing_back_a_pinned_chat_while_three_others_are_pinned_unpins_it_with_a_second_patch_after_the_first() {
        let db = db();
        let [a, b, c, d] = [
            "351910000021@s.whatsapp.net",
            "351910000022@s.whatsapp.net",
            "351910000023@s.whatsapp.net",
            "351910000024@s.whatsapp.net",
        ];
        pinned_and_one_archived(&db, &[a, b, d], c);
        let without = Does::Set(Setting::Archive { archived: false, patch: patch("c0"), unpin: None });
        assert!(refused(&db, c, without).to_string().contains("unpin patch"), "a fourth needs its unpin");
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 2);
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (0, false), "brought back, and unpinned, as it would be a fourth");
        let ids: Vec<String> = queued(&db).into_iter().filter(|(jid, ..)| jid == c).map(|(_, id, _)| id).collect();
        assert_eq!(ids, ["c1", "c1 unpin"], "the archive patch first, then the unpin");
        assert_eq!(pinned_unarchived(&db), 3);
    }

    #[test]
    fn bringing_back_a_pinned_chat_with_fewer_than_three_others_pinned_keeps_it_pinned_with_one_patch() {
        let db = db();
        let [a, b, c] = ["351910000021@s.whatsapp.net", "351910000022@s.whatsapp.net", "351910000023@s.whatsapp.net"];
        pinned_and_one_archived(&db, &[a, b], c);
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "its unpin patch isn't used");
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (AT, false), "brought back, and still pinned");
        assert_eq!(pinned_unarchived(&db), 3);
    }

    #[test]
    fn a_chat_being_merged_away_isnt_counted_so_another_can_be_pinned_while_a_persons_two_chats_merge() {
        let db = db();
        pinned_while_merging(&db);
        let [d, e] = ["351910000031@s.whatsapp.net", "351910000032@s.whatsapp.net"];
        chats(&db, &[d, e]);
        assert!(matches!(acted(&db, d, pin(Some(AT), "d1")), Acted::Queued { .. }), "Ana's two chats count once");
        assert_eq!(chat(&db, d).0, AT);
        assert_eq!(acted(&db, e, pin(Some(AT), "e1")), Acted::Pinned, "and they still count");
    }

    #[test]
    fn bringing_back_a_pinned_chat_while_a_persons_two_chats_merge_keeps_its_pin() {
        let db = db();
        pinned_while_merging(&db);
        let c = "351910000033@s.whatsapp.net";
        written(&db, &[Change::Chat(Chat { id: c.into(), pinned: Some(AT), archived: Some(true), ..Chat::default() })]);
        let Acted::Queued { outbox, .. } = acted(&db, c, archive(false, "c1")) else { panic!("not queued") };
        assert_eq!(outbox.len(), 1, "its unpin patch isn't used");
        let (pinned, archived, ..) = chat(&db, c);
        assert_eq!((pinned, archived), (AT, false), "brought back, and still pinned, as a third");
    }

    #[test]
    fn a_mute_sets_until_when_and_the_same_again_does_nothing() {
        let db = db();
        chats(&db, &[ANA]);
        let mute = |until: i64, id: &str| Does::Set(Setting::Mute { until, patch: patch(id) });
        assert!(matches!(acted(&db, ANA, mute(AT, "m1")), Acted::Queued { .. }));
        assert_eq!(chat(&db, ANA).2, AT);
        assert_eq!(acted(&db, ANA, mute(AT, "m2")), Acted::Already);
        assert!(matches!(acted(&db, ANA, mute(crate::convert::app_state::FOREVER, "m3")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, mute(0, "m4")), Acted::Queued { .. }));
        assert_eq!((chat(&db, ANA).2, queued(&db).len()), (0, 3));
    }

    #[test]
    fn marking_unread_sets_the_mark_and_marking_read_takes_it_off_and_reads_up_to_the_newest() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        assert_eq!(chat(&db, ANA).4, 2, "two unread");
        let mark = |unread: bool, id: &str| {
            Does::Set(Setting::MarkUnread { unread, newest: Some("m2".into()), patch: patch(id) })
        };
        assert!(matches!(acted(&db, ANA, mark(true, "u1")), Acted::Queued { .. }));
        assert!(chat(&db, ANA).3);
        assert_eq!(acted(&db, ANA, mark(true, "u2")), Acted::Already);
        let Acted::Queued { chat_n, .. } = acted(&db, ANA, mark(false, "r1")) else { panic!("not queued") };
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "the mark gone, and read up to the newest");
        let owes: bool =
            db.query_row("SELECT told_at IS NOT NULL FROM chats WHERE n = ?1", [chat_n], |r| r.get(0)).unwrap();
        assert!(owes, "read here, its read receipts are owed");
        assert_eq!(acted(&db, ANA, mark(false, "r2")), Acted::Already, "nothing marked and nothing unread");
        assert_eq!(queued(&db).len(), 2);
    }

    /// Ana's chat as the phone left it: three unread, none of them here, and marked unread if
    /// `marked`.
    fn counted_on_the_phone(db: &Connection, marked: bool) {
        written(db, &[Change::Read(Read { chat: ANA.into(), at: AT, phone: Some((3, 0)), ..Read::default() })]);
        if marked {
            written(db, &[Change::Chat(Chat { id: ANA.into(), marked_unread: Some(true), ..Chat::default() })]);
        }
        let (.., is_marked, unread, _) = chat(db, ANA);
        assert_eq!((is_marked, unread), (marked, 3), "the phone's count, with none of its messages here");
    }

    #[test]
    fn marking_read_a_chat_jaw_holds_none_of_the_messages_of_takes_the_mark_off_and_leaves_the_read_to_its_echo() {
        let db = db();
        counted_on_the_phone(&db, true);
        let v = chat(&db, ANA).5;
        let at = AT + 1_500;
        assert!(matches!(acted(&db, ANA, mark_read_at(ANA, at, "r1")), Acted::Queued { .. }));
        let (.., marked, unread, read_v) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 3), "the mark taken off, and the phone's count left");
        assert!(read_v > v, "with a new version");
        arrives(&db, &["markChatAsRead", ANA], &read_value(at), 1);
        let (.., marked, unread, echoed_v) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "WhatsApp's echo of the patch reads it, clearing the count");
        assert!(echoed_v > read_v, "with a version of its own");
        let read_at: i64 = db.query_row("SELECT read_at FROM chats WHERE id = ?1", [ANA], |r| r.get(0)).unwrap();
        assert_eq!(read_at, at - at % 1_000, "read at the patch's time, to the second its range gives");
        assert_eq!(acted(&db, ANA, mark_read_at(ANA, at + 1, "r2")), Acted::Already, "read now, it's so already");
        assert_eq!(queued(&db).len(), 1, "and no second patch");
    }

    #[test]
    fn marking_read_again_before_the_echo_queues_a_second_patch_and_only_its_echo_reads_the_chat() {
        let db = db();
        counted_on_the_phone(&db, false);
        let index = ["markChatAsRead", ANA];
        let (t1, t2) = (AT + 1_000, AT + 2_000);
        assert!(matches!(acted(&db, ANA, mark_read_at(ANA, t1, "r1")), Acted::Queued { .. }));
        let again = acted(&db, ANA, mark_read_at(ANA, t2, "r2"));
        assert!(matches!(again, Acted::Queued { .. }), "its count still there, it isn't so already: {again:?}");
        assert_eq!(queued(&db).len(), 2);
        let before = chat(&db, ANA);
        arrives(&db, &index, &read_value(t1), 1);
        assert_eq!(chat(&db, ANA), before, "the first patch's echo, taken before the second, is held back");
        arrives(&db, &index, &read_value(t2), 2);
        assert_eq!((chat(&db, ANA).4, awaited(&db, &index)), (0, None), "the second's reads it, and ends the wait");
    }

    #[test]
    fn a_read_on_another_device_while_jaws_mark_unread_waits_leaves_the_mark_so_marking_read_then_isnt_so_already() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        let index = ["markChatAsRead", ANA];
        let (unread_at, read_at) = (AT + 1_000, AT + 2_000);
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", unread_at, "u1")), Acted::Queued { .. }));
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread, queued(&db).len()), (true, 2, 1), "marked, two unread, and its patch queued");
        // Your phone reads the chat before jaw's patch reaches WhatsApp, and its read-self receipt comes.
        let receipt = Read { chat: ANA.into(), at: AT + 500, ids: vec!["m2".into()], ..Read::default() };
        written(&db, &[Change::Read(receipt)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!(
            (marked, unread),
            (true, 0),
            "read up to its newest, and still marked, since WhatsApp takes jaw's mark unread after the read"
        );
        let again = acted(&db, ANA, mark_at(ANA, false, "m2", read_at, "r1"));
        assert!(matches!(again, Acted::Queued { .. }), "marking it read isn't so already: {again:?}");
        assert_eq!((chat(&db, ANA).3, queued(&db).len()), (false, 2), "the mark taken off, and its patch queued");
        arrives(&db, &index, &unread_value(unread_at), 1);
        assert!(!chat(&db, ANA).3, "the mark unread's echo, taken before the mark read, is held back");
        arrives(&db, &index, &read_value(read_at), 2);
        assert_eq!((chat(&db, ANA).3, awaited(&db, &index)), (false, None), "the mark read's ends the wait");
        written(&db, &[from_ana("m3", AT + 3_000)]);
        arrives(&db, &index, &unread_value(AT + 4_000), 3);
        assert!(chat(&db, ANA).3, "your phone marks it unread");
        let receipt = Read { chat: ANA.into(), at: AT + 5_000, ids: vec!["m3".into()], ..Read::default() };
        written(&db, &[Change::Read(receipt)]);
        assert!(!chat(&db, ANA).3, "with nothing of jaw's on its way, a read on another device takes the mark off");
    }

    #[test]
    fn one_of_yours_sent_elsewhere_while_jaws_mark_unread_waits_leaves_the_mark_so_marking_read_isnt_so_already() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        let index = ["markChatAsRead", ANA];
        let (unread_at, read_at) = (AT + 1_000, AT + 2_000);
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", unread_at, "u1")), Acted::Queued { .. }));
        // You answer Ana from your phone before jaw's patch reaches WhatsApp.
        written(&db, &[mine_elsewhere("p1", AT + 500)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!(
            (marked, unread),
            (true, 0),
            "read up to your answer, and still marked, since WhatsApp takes jaw's mark unread after it"
        );
        let again = acted(&db, ANA, mark_at(ANA, false, "p1", read_at, "r1"));
        assert!(matches!(again, Acted::Queued { .. }), "marking it read isn't so already: {again:?}");
        arrives(&db, &index, &unread_value(unread_at), 1);
        arrives(&db, &index, &read_value(read_at), 2);
        assert_eq!((chat(&db, ANA).3, awaited(&db, &index)), (false, None), "the mark read's echo ends the wait");
        arrives(&db, &index, &unread_value(AT + 4_000), 3);
        assert!(chat(&db, ANA).3, "your phone marks it unread");
        written(&db, &[mine_elsewhere("p2", AT + 5_000)]);
        assert!(
            !chat(&db, ANA).3,
            "with nothing of jaw's on its way, one of yours from another device takes the mark off"
        );
    }

    #[test]
    fn a_history_blob_read_on_your_phone_converted_while_jaws_mark_unread_waits_reads_the_chat_and_leaves_the_mark() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", AT + 2_000, "u1")), Acted::Queued { .. }));
        // The conversation as your phone had it once you had read it there, read and not marked,
        // converted only now.
        let read_there = Conversation {
            id: ANA.into(),
            conversation_timestamp: Some(((AT + 1_000) / 1_000) as u64),
            unread_count: Some(0),
            marked_as_unread: Some(false),
            ..Default::default()
        };
        written(&db, &from_history(read_there));
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!(
            (marked, unread),
            (true, 0),
            "read up to its newest, and still marked, since WhatsApp takes jaw's mark unread after the read"
        );
        let again = acted(&db, ANA, mark_at(ANA, false, "m2", AT + 3_000, "r1"));
        assert!(matches!(again, Acted::Queued { .. }), "marking it read isn't so already: {again:?}");
    }

    #[test]
    fn a_refused_mark_read_of_a_chat_jaw_holds_none_of_the_messages_of_leaves_the_phones_count() {
        let db = db();
        counted_on_the_phone(&db, true);
        let Acted::Queued { outbox, .. } = acted(&db, ANA, mark_read_at(ANA, AT + 1_000, "r1")) else {
            panic!("not queued")
        };
        let undo = Some(Undo::MarkUnread { chat: ANA.into() });
        let refused = Step::Refused { outbox: outbox[0], error: "server returned error 400".into(), undo };
        written(&db, &[Change::Outbox(refused)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 3), "the mark as nothing had set it, and the phone's count as it was");
        assert_eq!(awaited(&db, &["markChatAsRead", ANA]), None, "the wait over");
    }

    #[test]
    fn marking_read_is_so_already_only_for_a_chat_with_no_mark_and_nothing_unread() {
        let db = db();
        let untouched = "351910000004@s.whatsapp.net";
        written(
            &db,
            &[from_ana("m1", AT), Change::Chat(Chat { id: RUI.into(), marked_unread: Some(true), ..Chat::default() })],
        );
        chats(&db, &[untouched]);
        let read = |newest: Option<&str>, id: &str| {
            let newest = newest.map(str::to_string);
            Does::Set(Setting::MarkUnread { unread: false, newest, patch: patch(id) })
        };
        assert!(matches!(acted(&db, ANA, read(Some("m1"), "a1")), Acted::Queued { .. }), "unread, though not marked");
        assert_eq!(chat(&db, ANA).4, 0, "and read");
        assert!(matches!(acted(&db, RUI, read(None, "r1")), Acted::Queued { .. }), "marked, though nothing's unread");
        assert!(!chat(&db, RUI).3, "and the mark taken off");
        assert_eq!(acted(&db, untouched, read(None, "u1")), Acted::Already, "neither marked nor unread");
        assert_eq!(queued(&db).len(), 2);
    }

    #[test]
    fn a_star_marks_the_message_with_a_new_version_and_the_same_again_does_nothing() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let star = |starred: bool, id: &str| Does::Star { id: "m1".into(), starred, patch: patch(id) };
        let message = |db: &Connection| -> (bool, i64) {
            db.query_row("SELECT starred, v FROM messages WHERE id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let (_, before) = message(&db);
        assert!(matches!(acted(&db, ANA, star(true, "s1")), Acted::Queued { .. }));
        let (starred, after) = message(&db);
        assert!(starred && after > before);
        assert_eq!(acted(&db, ANA, star(true, "s2")), Acted::Already);
        let missing = Does::Star { id: "nope".into(), starred: true, patch: patch("s3") };
        assert_eq!(acted(&db, ANA, missing), Acted::Gone);
        assert!(matches!(acted(&db, ANA, star(false, "s4")), Acted::Queued { .. }));
        assert_eq!((message(&db).0, queued(&db).len()), (false, 2));
    }

    #[test]
    fn a_star_without_room_in_the_outbox_is_refused_as_full_and_writes_nothing() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        outbox_holding(&db, OUTBOX_MOST);
        let message = |db: &Connection| -> (bool, i64) {
            db.query_row("SELECT starred, v FROM messages WHERE id = 'm1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
        };
        let before = message(&db);
        let star = Does::Star { id: "m1".into(), starred: true, patch: patch("s1") };
        assert_eq!(acted(&db, ANA, star), Acted::Full { waiting: 1_000, failed: 0 });
        assert_eq!((message(&db), queued(&db).len()), (before, 0), "nothing written or queued");
    }

    #[test]
    fn a_star_of_a_message_deleted_for_everyone_or_being_writes_nothing_but_its_star_can_go() {
        let db = db();
        let yours = Message {
            chat: ANA.into(),
            id: "m1".into(),
            at: AT,
            mine: true,
            kind: "text",
            status: Some("sent"),
            text: Some("m1".into()),
            summary: "m1".into(),
            live: true,
            ..Message::default()
        };
        written(&db, &[Change::Message(yours), from_ana("m2", AT + 1)]);
        let star = |id: &str, starred: bool, by: &str| Does::Star { id: id.into(), starred, patch: patch(by) };
        assert!(matches!(acted(&db, ANA, star("m1", true, "s1")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, star("m2", true, "s2")), Acted::Queued { .. }));
        // Your deletion of m1 for everyone is queued, and Ana deletes m2 for everyone.
        let deleting = Change::Outbox(Step::Act {
            chat: ANA.into(),
            target: "m1".into(),
            id: "x1".into(),
            effect: Amendment::Revoke,
            at: AT + 2,
            raw: vec![1],
            key: None,
            written: Written::default(),
        });
        let hers = Amend {
            chat: ANA.into(),
            target: "m2".into(),
            sender: Some(ANA.into()),
            mine: false,
            at: AT + 3,
            what: Amendment::Revoke,
            raw: None,
        };
        written(&db, &[deleting, Change::Amend(hers)]);
        let message = |id: &str| -> (bool, bool, i64) {
            db.query_row("SELECT starred, deleted, v FROM messages WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
        };
        assert_eq!((message("m1").1, message("m2").1), (false, true), "m1 being deleted, m2 deleted");
        for (id, unstar, again) in [("m1", "u1", "s3"), ("m2", "u2", "s4")] {
            assert!(matches!(acted(&db, ANA, star(id, false, unstar)), Acted::Queued { .. }), "{id}'s star can go");
            let unstarred = message(id);
            assert!(!unstarred.0, "{id} unstarred");
            let rows = queued(&db).len();
            assert_eq!(acted(&db, ANA, star(id, true, again)), Acted::Deleted, "{id}");
            assert_eq!((message(id), queued(&db).len()), (unstarred, rows), "{id}: nothing written or queued");
        }
    }

    #[test]
    fn a_chat_jaw_doesnt_have_or_a_communitys_own_row_is_left_alone() {
        let db = db();
        written(&db, &[Change::Chat(Chat { id: COMMUNITY.into(), kind: Some("community"), ..Chat::default() })]);
        assert_eq!(acted(&db, ANA, pin(Some(AT), "p1")), Acted::Gone);
        assert_eq!(acted(&db, COMMUNITY, pin(Some(AT), "p2")), Acted::Community);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!((chats, queued(&db).len()), (1, 0), "no chat made, nothing queued");
    }

    #[test]
    fn without_room_in_the_outbox_for_its_patches_nothing_is_written() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        outbox_holding(&db, OUTBOX_MOST - 1);
        let archive =
            Does::Set(Setting::Archive { archived: true, patch: patch("a1"), unpin: Some(patch("a1 unpin")) });
        assert_eq!(acted(&db, ANA, archive), Acted::Full { waiting: 999, failed: 0 }, "two places, one left");
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (AT, false), "nothing written");
        assert!(matches!(acted(&db, RUI, pin(Some(AT), "r1")), Acted::Queued { .. }), "the last place");
        assert_eq!(acted(&db, RUI, pin(None, "r2")), Acted::Full { waiting: 1_000, failed: 0 });
    }

    #[test]
    fn archiving_a_pinned_chat_without_its_unpin_patch_or_pinning_at_a_time_that_isnt_positive_is_refused() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        written(&db, &[Change::Chat(Chat { id: ANA.into(), pinned: Some(AT), ..Chat::default() })]);
        let without = Does::Set(Setting::Archive { archived: true, patch: patch("a1"), unpin: None });
        assert!(refused(&db, ANA, without).to_string().contains("unpin patch"));
        let (pinned, archived, ..) = chat(&db, ANA);
        assert_eq!((pinned, archived), (AT, false), "neither archived nor unpinned");
        for at in [0, -1] {
            assert!(refused(&db, RUI, pin(Some(at), "p1")).to_string().contains("positive"), "pinned at {at}");
        }
        assert_eq!((chat(&db, RUI).0, queued(&db).len()), (0, 0), "not pinned, and nothing queued");
    }

    #[test]
    fn undoing_puts_back_how_a_chat_is_when_nothing_set_it_and_makes_no_chat() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        let set = Chat {
            id: ANA.into(),
            pinned: Some(AT),
            archived: Some(true),
            muted_until: Some(AT),
            marked_unread: Some(true),
            ..Chat::default()
        };
        let star = Amend {
            chat: ANA.into(),
            target: "m1".into(),
            sender: None,
            mine: false,
            at: AT,
            what: Amendment::Star { starred: true },
            raw: None,
        };
        written(&db, &[Change::Chat(set), Change::Amend(star)]);
        let before = chat(&db, ANA).5;
        let undone = |db: &Connection, undos: &[Undo]| {
            let mut stamps = Stamps::load(db).unwrap();
            stamps.begin();
            for one in undos {
                undo(db, &mut stamps, one).unwrap();
            }
            super::super::settle(db, &mut stamps).unwrap();
        };
        undone(
            &db,
            &[
                Undo::Pin { chat: ANA.into() },
                Undo::Archive { chat: ANA.into() },
                Undo::Mute { chat: ANA.into() },
                Undo::MarkUnread { chat: ANA.into() },
                Undo::Star { chat: ANA.into(), id: "m1".into() },
            ],
        );
        let (pinned, archived, muted, marked, _, v) = chat(&db, ANA);
        assert_eq!((pinned, archived, muted, marked), (0, false, 0, false));
        assert!(v > before, "with a new version, so every window is sent it");
        let starred: bool = db.query_row("SELECT starred FROM messages WHERE id = 'm1'", [], |r| r.get(0)).unwrap();
        assert!(!starred);
        undone(&db, &[Undo::Pin { chat: RUI.into() }, Undo::Star { chat: RUI.into(), id: "x".into() }]);
        let chats: i64 = db.query_row("SELECT count(*) FROM chats", [], |r| r.get(0)).unwrap();
        assert_eq!(chats, 1, "a chat that isn't there isn't made");
    }

    /// A chat pinned at `at`, or unpinned, as jaw's patch and the phone's own mutation carry it.
    fn pin_value(pinned: bool, at: i64) -> action::SyncActionValue {
        action::SyncActionValue {
            timestamp: Some(at),
            pin_action: Some(action::PinAction { pinned: Some(pinned) }),
            ..Default::default()
        }
    }

    /// A chat muted until `until`, for ever at FOREVER, or unmuted at 0.
    fn mute_value(until: i64, at: i64) -> action::SyncActionValue {
        let mute = match until {
            0 => action::MuteAction { muted: Some(false), ..Default::default() },
            FOREVER => action::MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() },
            until => action::MuteAction { muted: Some(true), mute_end_timestamp: Some(until), ..Default::default() },
        };
        action::SyncActionValue { timestamp: Some(at), mute_action: Some(mute), ..Default::default() }
    }

    /// jaw's patch for `index` carrying `value`, as the handler queues it, `id` for its outbox row.
    fn patch_of(index: &[&str], value: &action::SyncActionValue, id: &str) -> Patch {
        Patch {
            id: id.into(),
            raw: value.encode_to_vec(),
            key: serde_json::to_string(index).unwrap(),
            at: value.timestamp.unwrap(),
        }
    }

    /// Chat `chat` pinned from jaw at `at`, or unpinned.
    fn pin_at(chat: &str, pinned: bool, at: i64, id: &str) -> Does {
        let patch = patch_of(&["pin_v1", chat], &pin_value(pinned, at), id);
        Does::Set(Setting::Pin { at: pinned.then_some(at), patch })
    }

    /// A chat read at `at`, naming no message, as jaw's patch for a chat it holds none of the
    /// messages of carries it.
    fn read_value(at: i64) -> action::SyncActionValue {
        let range = action::SyncActionMessageRange { last_message_timestamp: Some(at / 1000), ..Default::default() };
        action::SyncActionValue {
            timestamp: Some(at),
            mark_chat_as_read_action: Some(action::MarkChatAsReadAction {
                read: Some(true),
                message_range: Some(range),
            }),
            ..Default::default()
        }
    }

    /// A chat marked unread at `at`, naming no message, as jaw's patch carries it.
    fn unread_value(at: i64) -> action::SyncActionValue {
        let mut value = read_value(at);
        if let Some(action) = value.mark_chat_as_read_action.as_mut() {
            action.read = Some(false);
        }
        value
    }

    /// Chat `chat`, none of whose messages jaw holds, marked read from jaw at `at`.
    fn mark_read_at(chat: &str, at: i64, id: &str) -> Does {
        let patch = patch_of(&["markChatAsRead", chat], &read_value(at), id);
        Does::Set(Setting::MarkUnread { unread: false, newest: None, patch })
    }

    /// Chat `chat` marked unread from jaw at `at`, or read up to `newest`, its newest message.
    fn mark_at(chat: &str, unread: bool, newest: &str, at: i64, id: &str) -> Does {
        let value = if unread { unread_value(at) } else { read_value(at) };
        let patch = patch_of(&["markChatAsRead", chat], &value, id);
        Does::Set(Setting::MarkUnread { unread, newest: Some(newest.into()), patch })
    }

    /// What WhatsApp sends for `index`, converted from inbox row `seq` as the drain converts it.
    fn arrives(db: &Connection, index: &[&str], value: &action::SyncActionValue, seq: i64) {
        let index: Vec<String> = index.iter().map(|part| part.to_string()).collect();
        let value = value.encode_to_vec();
        let applied = crate::convert::app_state::mutation(&index, &value);
        let state = Change::AppState { index, value, seq, at: applied.at, changes: applied.changes };
        written(db, &[state, Change::Drained(Drained::Inbox(seq))]);
    }

    /// Engine events waiting in the inbox as rows `seqs`.
    fn inboxed(db: &Connection, seqs: &[i64]) {
        for seq in seqs {
            db.execute("INSERT INTO inbox (seq, kind, at, payload) VALUES (?1, 'app_state', 0, x'')", [seq]).unwrap();
        }
    }

    /// WhatsApp has taken the patch at outbox row `outbox`, as the engine's answer says.
    fn sent(db: &Connection, outbox: i64) {
        written(db, &[Change::Outbox(Step::Sent { outbox })]);
    }

    /// What `index` waits for of jaw's: the time its latest patch carries, that patch's outbox row,
    /// and where its echo has to be.
    fn awaited(db: &Connection, index: &[&str]) -> Option<(i64, i64, Option<i64>)> {
        db.query_row(
            "SELECT at, outbox, echo_by FROM app_state_awaiting WHERE key = ?1",
            [serde_json::to_string(index).unwrap()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .unwrap()
    }

    /// What `app_state` keeps raw for `index`.
    fn kept(db: &Connection, index: &[&str]) -> Option<Vec<u8>> {
        db.query_row("SELECT value FROM app_state WHERE key = ?1", [serde_json::to_string(index).unwrap()], |r| {
            r.get(0)
        })
        .optional()
        .unwrap()
    }

    #[test]
    fn an_echo_from_before_jaws_next_change_is_held_back_so_asking_again_isnt_so_already() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let (t1, t2, t3) = (AT, AT + 1, AT + 2);
        assert!(matches!(acted(&db, ANA, pin_at(ANA, true, t1, "p1")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, ANA, pin_at(ANA, false, t2, "p2")), Acted::Queued { .. }));
        let unpinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(true, t1), 1);
        assert_eq!(chat(&db, ANA), unpinned, "the pin's echo, taken before the unpin, leaves the row as it is");
        assert_eq!(kept(&db, &index), Some(pin_value(true, t1).encode_to_vec()), "and is kept raw");
        let again = acted(&db, ANA, pin_at(ANA, true, t3, "p3"));
        assert!(matches!(again, Acted::Queued { .. }), "pinning again isn't so already: {again:?}");
        let pinned = chat(&db, ANA);
        assert_eq!(pinned.0, t3);
        arrives(&db, &index, &pin_value(false, t2), 2);
        assert_eq!(chat(&db, ANA), pinned, "the unpin's echo changes nothing");
        arrives(&db, &index, &pin_value(true, t3), 3);
        assert_eq!(chat(&db, ANA), pinned, "nor does the third pin's");
        assert_eq!(awaited(&db, &index), None, "which ends the wait");
    }

    #[test]
    fn an_earlier_echo_cant_unpin_a_chat_pinned_again_so_a_fourth_is_still_refused() {
        let db = db();
        let [a, b, c, d] = [
            "351910000041@s.whatsapp.net",
            "351910000042@s.whatsapp.net",
            "351910000043@s.whatsapp.net",
            "351910000044@s.whatsapp.net",
        ];
        chats(&db, &[a, b, c, d]);
        for (n, id) in [a, b, c].into_iter().enumerate() {
            assert!(matches!(acted(&db, id, pin_at(id, true, AT + n as i64, id)), Acted::Queued { .. }), "{id}");
        }
        assert!(matches!(acted(&db, a, pin_at(a, false, AT + 10, "a unpinned")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, a, pin_at(a, true, AT + 11, "a again")), Acted::Queued { .. }));
        let pinned = chat(&db, a);
        let index = ["pin_v1", a];
        arrives(&db, &index, &pin_value(true, AT), 1);
        arrives(&db, &index, &pin_value(false, AT + 10), 2);
        assert_eq!(chat(&db, a), pinned, "its earlier echoes change nothing");
        assert_eq!(acted(&db, d, pin_at(d, true, AT + 12, "d")), Acted::Pinned, "so three are pinned still");
        arrives(&db, &index, &pin_value(true, AT + 11), 3);
        assert_eq!((chat(&db, a), pinned_unarchived(&db)), (pinned, 3), "and stay so once its last echo comes");
    }

    #[test]
    fn what_the_phone_sets_while_jaws_patch_is_on_its_way_is_kept_raw_but_held_back_until_its_echo() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["mute", ANA];
        let mute = Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&index, &mute_value(FOREVER, AT), "m1") });
        assert!(matches!(acted(&db, ANA, mute), Acted::Queued { .. }));
        let muted = chat(&db, ANA);
        // The phone unmutes it meanwhile, by a later clock; WhatsApp takes jaw's mute after it all the same.
        let phone = mute_value(0, AT + 5_000);
        arrives(&db, &index, &phone, 1);
        assert_eq!(chat(&db, ANA), muted, "held back, since jaw's mute is what every device ends with");
        assert_eq!(kept(&db, &index), Some(phone.encode_to_vec()), "but kept raw");
        arrives(&db, &index, &mute_value(FOREVER, AT), 2);
        assert_eq!(
            (chat(&db, ANA), awaited(&db, &index)),
            (muted, None),
            "jaw's echo changes nothing and ends the wait"
        );
        arrives(&db, &index, &mute_value(0, AT + 9_000), 3);
        assert_eq!(chat(&db, ANA).2, 0, "the phone's next change is applied as it comes");
    }

    #[test]
    fn once_whatsapp_has_taken_jaws_patch_what_comes_past_where_its_echo_had_to_be_is_applied() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        // WhatsApp answers once the patch's echo is in the inbox, here with a row after it.
        inboxed(&db, &[6, 7]);
        sent(&db, outbox[0]);
        assert_eq!(awaited(&db, &index), Some((AT, outbox[0], Some(7))), "its echo is at 7 or before");
        let pinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(false, AT - 5), 6);
        assert!(awaited(&db, &index).is_some(), "held back, and the wait goes on");
        arrives(&db, &index, &pin_value(false, AT - 1), 7);
        assert_eq!(chat(&db, ANA), pinned, "at 7 or before, another time came before jaw's patch: held back");
        assert_eq!(awaited(&db, &index), None, "and, the converter past where its echo had to be, the wait is over");
        arrives(&db, &index, &pin_value(false, AT + 1), 8);
        assert_eq!(chat(&db, ANA).0, 0, "past 7, it came after: applied");
    }

    #[test]
    fn what_comes_past_where_an_echo_had_to_be_is_applied_while_a_row_before_it_waits_in_the_inbox() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        inboxed(&db, &[6, 7]);
        sent(&db, outbox[0]);
        let pinned = chat(&db, ANA);
        arrives(&db, &index, &pin_value(false, AT - 5), 6);
        assert_eq!(chat(&db, ANA), pinned, "held back");
        // Row 7, a history notification, stays in the inbox until its blob is in, so the converter
        // goes past it without draining it.
        arrives(&db, &index, &pin_value(false, AT + 1), 8);
        assert_eq!(
            (chat(&db, ANA).0, awaited(&db, &index)),
            (0, None),
            "past 7, it came after: applied, the wait over"
        );
    }

    #[test]
    fn the_converter_passing_where_an_echo_had_to_be_ends_its_wait_but_not_one_still_to_go() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        let (pin, mute, rui) = (["pin_v1", ANA], ["mute", ANA], ["pin_v1", RUI]);
        let Acted::Queued { outbox: pinned, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else {
            panic!("not queued")
        };
        let muting =
            Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&mute, &mute_value(FOREVER, AT + 1), "m1") });
        let Acted::Queued { outbox: muted, .. } = acted(&db, ANA, muting) else { panic!("not queued") };
        assert!(matches!(acted(&db, RUI, pin_at(RUI, true, AT + 2, "r1")), Acted::Queued { .. }));
        inboxed(&db, &[6]);
        sent(&db, pinned[0]);
        inboxed(&db, &[7]);
        sent(&db, muted[0]);
        let echo_by = |index: &[&str]| awaited(&db, index).map(|(.., echo_by)| echo_by);
        assert_eq!((echo_by(&pin), echo_by(&mute), echo_by(&rui)), (Some(Some(6)), Some(Some(7)), Some(None)));
        written(&db, &[Change::Drained(Drained::Inbox(5))]);
        assert_eq!(
            (echo_by(&pin), echo_by(&mute)),
            (Some(Some(6)), Some(Some(7))),
            "before where either echo had to be, both wait"
        );
        written(&db, &[Change::Drained(Drained::Inbox(6))]);
        assert_eq!(
            (echo_by(&pin), echo_by(&mute)),
            (None, Some(Some(7))),
            "at 6, the pin's echo would have ended its wait: it's over"
        );
        written(&db, &[Change::Drained(Drained::Inbox(8))]);
        assert_eq!(echo_by(&mute), None, "past 7, so is the mute's");
        assert_eq!(echo_by(&rui), Some(None), "and Rui's pin, still to go, waits however far the converter has got");
    }

    #[test]
    fn a_patch_failed_for_good_rather_than_refused_still_ends_its_indexs_wait_with_it() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        let failed = |next_at| Step::Failed { outbox: outbox[0], error: "websocket not connected".into(), next_at };
        written(&db, &[Change::Outbox(failed(Some(AT + 1_000)))]);
        assert_eq!(awaited(&db, &index), Some((AT, outbox[0], None)), "one to try again still waits");
        written(&db, &[Change::Outbox(failed(None))]);
        assert_eq!((queued(&db).len(), awaited(&db, &index)), (0, None), "its row gone, unsent, and the wait with it");
        arrives(&db, &index, &pin_value(false, AT - 1), 1);
        assert_eq!(chat(&db, ANA).0, 0, "so what comes is applied");
    }

    #[test]
    fn once_jaws_patch_has_gone_without_going_what_comes_for_its_index_is_applied() {
        let db = db();
        chats(&db, &[ANA]);
        let index = ["pin_v1", ANA];
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "p1")) else { panic!("not queued") };
        // No step leaves a wait whose outbox row has gone, so the row is taken out by hand.
        db.execute("DELETE FROM outbox WHERE n = ?1", [outbox[0]]).unwrap();
        arrives(&db, &index, &pin_value(false, AT - 1), 1);
        assert_eq!((chat(&db, ANA).0, awaited(&db, &index)), (0, None), "so what comes is applied, the wait over");
    }

    #[test]
    fn a_refused_patch_is_put_back_only_while_it_is_the_latest_for_its_index() {
        let db = db();
        chats(&db, &[ANA, RUI]);
        let refused = |outbox: i64, chat: &str| {
            let undo = Some(Undo::Pin { chat: chat.into() });
            Change::Outbox(Step::Refused { outbox, error: "server returned error 400".into(), undo })
        };
        let Acted::Queued { outbox, .. } = acted(&db, ANA, pin_at(ANA, true, AT, "a1")) else { panic!("not queued") };
        written(&db, &[refused(outbox[0], ANA)]);
        assert_eq!(
            (chat(&db, ANA).0, awaited(&db, &["pin_v1", ANA])),
            (0, None),
            "the latest: put back, its wait over"
        );
        let Acted::Queued { outbox: first, .. } = acted(&db, RUI, pin_at(RUI, true, AT + 1, "r1")) else {
            panic!("not queued")
        };
        assert!(matches!(acted(&db, RUI, pin_at(RUI, false, AT + 2, "r2")), Acted::Queued { .. }));
        assert!(matches!(acted(&db, RUI, pin_at(RUI, true, AT + 3, "r3")), Acted::Queued { .. }));
        let pinned = chat(&db, RUI);
        written(&db, &[refused(first[0], RUI)]);
        assert_eq!(
            chat(&db, RUI),
            pinned,
            "one a later patch superseded puts nothing back: the later one's intent stands"
        );
        assert_eq!(awaited(&db, &["pin_v1", RUI]).map(|(at, ..)| at), Some(AT + 3), "whose wait goes on");
        let kept: i64 = db.query_row("SELECT count(*) FROM quarantine", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, 2, "and both are kept in quarantine");
    }

    /// A chat archived at `at`, or brought back, as the phone's mutation carries it.
    fn archive_value(archived: bool, at: i64) -> action::SyncActionValue {
        action::SyncActionValue {
            timestamp: Some(at),
            archive_chat_action: Some(action::ArchiveChatAction { archived: Some(archived), message_range: None }),
            ..Default::default()
        }
    }

    /// Every change a history blob holding only `conversation` gives, in order, as the converter
    /// hands them to the writer.
    fn from_history(conversation: Conversation) -> Vec<Change> {
        let blob = HistorySync { sync_type: 2, conversations: vec![conversation], ..Default::default() };
        let mut changes = Vec::new();
        crate::convert::history::walk(&blob.encode_to_vec(), &crate::convert::Me::default(), 2_000, |chunk| {
            changes.extend(chunk);
            true
        })
        .unwrap();
        changes
    }

    #[test]
    fn a_history_blob_built_before_jaws_changes_fills_in_only_the_settings_app_state_hasnt_said() {
        let db = db();
        chats(&db, &[ANA]);
        let (linked, pin) = (AT - 60_000, ["pin_v1", ANA]);
        arrives(&db, &pin, &pin_value(true, linked), 1);
        assert_eq!(chat(&db, ANA).0, linked, "pinned by app state when the account was linked");
        assert!(matches!(acted(&db, ANA, pin_at(ANA, false, AT, "u1")), Acted::Queued { .. }));
        arrives(&db, &pin, &pin_value(false, AT), 2);
        assert_eq!((chat(&db, ANA).0, awaited(&db, &pin)), (0, None), "unpinned from jaw, and its echo has come");
        let mute = ["mute", ANA];
        let muting =
            Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&mute, &mute_value(FOREVER, AT + 1), "m1") });
        assert!(matches!(acted(&db, ANA, muting), Acted::Queued { .. }));
        // The conversation as the phone had it when the account was linked, converted only now.
        let then = Conversation {
            id: ANA.into(),
            pinned: Some((linked / 1_000) as u32),
            archived: Some(true),
            mute_end_time: Some((linked / 1_000 + 3_600) as u64),
            ..Default::default()
        };
        written(&db, &from_history(then));
        let (pinned, archived, muted, ..) = chat(&db, ANA);
        assert_eq!(pinned, 0, "jaw's unpin stands, app state having said so");
        assert_eq!(kept(&db, &pin), Some(pin_value(false, AT).encode_to_vec()), "as WhatsApp's latest has it");
        assert_eq!(muted, FOREVER, "as does jaw's mute, still on its way");
        assert!(archived, "and what app state has said nothing of, history fills in");
    }

    #[test]
    fn history_leaves_a_setting_app_state_said_under_another_of_the_jids_the_blob_gives_the_conversation() {
        let db = db();
        chats(&db, &[ANA]);
        written(&db, &[Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]);
        arrives(&db, &["archive", ANA_LID], &archive_value(false, AT), 1);
        let then = Conversation {
            id: ANA_LID.into(),
            pn_jid: Some(ANA.into()),
            lid_jid: Some(ANA_LID.into()),
            archived: Some(true),
            ..Default::default()
        };
        written(&db, &from_history(then));
        assert!(!chat(&db, ANA).1, "app state brought it back by her LID, under which the blob names the conversation");
    }

    /// Ana's LID joined to her number, under which jaw keeps her chat.
    fn her_lid_joined(db: &Connection) {
        written(db, &[Change::Person(Person { jids: vec![ANA.into(), ANA_LID.into()], ..Person::default() })]);
    }

    #[test]
    fn a_receipt_naming_her_lid_while_jaws_mark_unread_waits_reads_the_chat_and_leaves_the_mark() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        her_lid_joined(&db);
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", AT + 1_000, "u1")), Acted::Queued { .. }));
        // Your phone reads the chat before jaw's patch reaches WhatsApp, and its receipt names her LID.
        let receipt = Read { chat: ANA_LID.into(), at: AT + 500, ids: vec!["m2".into()], ..Read::default() };
        written(&db, &[Change::Read(receipt)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!(
            (marked, unread),
            (true, 0),
            "read, and still marked: jaw's mark is looked for under her chat's JID"
        );
        let read = acted(&db, ANA, mark_at(ANA, false, "m2", AT + 2_000, "r1"));
        assert!(matches!(read, Acted::Queued { .. }), "marking it read isn't so already: {read:?}");
    }

    #[test]
    fn history_leaves_a_setting_app_state_said_under_the_jid_jaw_keeps_the_chat_by_when_the_blob_names_another() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        arrives(&db, &["pin_v1", ANA], &pin_value(false, AT), 1);
        // The conversation as the phone had it at link time, pinned, named by her LID alone.
        let then =
            Conversation { id: ANA_LID.into(), pinned: Some(((AT - 60_000) / 1_000) as u32), ..Default::default() };
        written(&db, &from_history(then));
        assert_eq!(chat(&db, ANA).0, 0, "app state unpinned it by her number, under which jaw keeps the chat");
    }

    /// A message starred at `at`, or its star taken off, as jaw's patch and the phone's own carry it.
    fn star_value(starred: bool, at: i64) -> action::SyncActionValue {
        action::SyncActionValue {
            timestamp: Some(at),
            star_action: Some(action::StarAction { starred: Some(starred) }),
            ..Default::default()
        }
    }

    /// Message `id` starred from jaw at `at`, or its star taken off, under `index`.
    fn star_at(index: &[&str], starred: bool, at: i64, id: &str) -> Does {
        Does::Star { id: index[2].into(), starred, patch: patch_of(index, &star_value(starred, at), id) }
    }

    /// Whether message `id` is starred.
    fn starred(db: &Connection, id: &str) -> bool {
        db.query_row("SELECT starred FROM messages WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    /// How many indexes wait for a patch of jaw's.
    fn waits(db: &Connection) -> i64 {
        db.query_row("SELECT count(*) FROM app_state_awaiting", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn what_the_phone_sends_under_her_lid_while_jaws_pin_under_her_number_waits_is_held_back() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        let (number, lid) = (["pin_v1", ANA], ["pin_v1", ANA_LID]);
        assert!(matches!(acted(&db, ANA, pin_at(ANA, true, AT, "p1")), Acted::Queued { .. }));
        // The phone's unpin, from before jaw's pin reached WhatsApp, names her LID.
        arrives(&db, &lid, &pin_value(false, AT - 1_000), 1);
        assert_eq!(chat(&db, ANA).0, AT, "held back, since WhatsApp takes jaw's pin after it, whichever JID it names");
        assert_eq!(kept(&db, &lid), Some(pin_value(false, AT - 1_000).encode_to_vec()), "but kept raw");
        let unpin = acted(&db, ANA, pin_at(ANA, false, AT + 1_000, "u1"));
        assert!(matches!(unpin, Acted::Queued { .. }), "unpinning it isn't so already: {unpin:?}");
        arrives(&db, &number, &pin_value(true, AT), 2);
        assert_eq!(
            (chat(&db, ANA).0, queued(&db).len()),
            (0, 2),
            "the pin's echo, taken before the unpin, is held back"
        );
        arrives(&db, &number, &pin_value(false, AT + 1_000), 3);
        assert_eq!((chat(&db, ANA).0, awaited(&db, &number)), (0, None), "and the unpin's ends the wait");
    }

    #[test]
    fn a_read_the_phone_sends_under_her_lid_while_jaws_mark_unread_waits_is_held_back_so_marking_read_isnt_so_already()
    {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        her_lid_joined(&db);
        let number = ["markChatAsRead", ANA];
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", AT + 1_000, "u1")), Acted::Queued { .. }));
        // The phone's read, from before jaw's mark by its time as well as by when WhatsApp took it,
        // names her LID.
        arrives(&db, &["markChatAsRead", ANA_LID], &read_value(AT + 500), 1);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (true, 2), "held back, since jaw's mark unread is later");
        let read = acted(&db, ANA, mark_at(ANA, false, "m2", AT + 2_000, "r1"));
        assert!(matches!(read, Acted::Queued { .. }), "marking it read isn't so already: {read:?}");
        arrives(&db, &number, &unread_value(AT + 1_000), 2);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "the mark unread's echo, taken before the mark read, is held back");
        arrives(&db, &number, &read_value(AT + 2_000), 3);
        assert_eq!(awaited(&db, &number), None, "and the mark read's ends the wait");
    }

    #[test]
    fn history_leaves_a_setting_app_state_said_under_another_jid_of_the_chats_person_than_the_blob_names() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        arrives(&db, &["archive", ANA_LID], &archive_value(false, AT), 1);
        // The conversation as the phone had it at link time, archived, named by her number alone.
        let then = Conversation { id: ANA.into(), archived: Some(true), ..Default::default() };
        written(&db, &from_history(then));
        assert!(!chat(&db, ANA).1, "app state brought it back by her LID, another JID of her chat's person");
    }

    /// Ana's chat under her number with m1 and m2 from her, and her LID's chat of its own, marked
    /// unread from jaw, as the chat it was, before her LID and number are joined and it merges
    /// into hers.
    fn marked_under_her_lid_then_merged(db: &Connection) {
        written(db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        chats(db, &[ANA_LID]);
        assert!(matches!(acted(db, ANA_LID, mark_at(ANA_LID, true, "m2", AT + 1_000, "u1")), Acted::Queued { .. }));
        her_lid_joined(db);
        let merging: i64 = db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0)).unwrap();
        assert_eq!((merging, chat(db, ANA).3), (1, true), "her LID's chat merging into her number's, which is marked");
    }

    #[test]
    fn a_read_on_the_phone_after_her_chats_merge_while_jaws_mark_unread_waits_leaves_the_mark() {
        let db = db();
        marked_under_her_lid_then_merged(&db);
        let receipt = Read { chat: ANA.into(), at: AT + 500, ids: vec!["m2".into()], ..Read::default() };
        written(&db, &[Change::Read(receipt)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (true, 0), "read, and still marked, since jaw's mark unread is on its way");
        let read = acted(&db, ANA, mark_at(ANA, false, "m2", AT + 2_000, "r1"));
        assert!(matches!(read, Acted::Queued { .. }), "marking it read isn't so already: {read:?}");
        arrives(&db, &["markChatAsRead", ANA_LID], &unread_value(AT + 1_000), 1);
        assert!(!chat(&db, ANA).3, "the mark unread's echo, under her LID as jaw sent it, is held back");
        arrives(&db, &["markChatAsRead", ANA], &read_value(AT + 2_000), 2);
        assert_eq!(awaited(&db, &["markChatAsRead", ANA]), None, "and the mark read's ends the wait");
    }

    #[test]
    fn one_of_yours_from_elsewhere_after_her_chats_merge_while_jaws_mark_unread_waits_leaves_the_mark() {
        let db = db();
        marked_under_her_lid_then_merged(&db);
        written(&db, &[mine_elsewhere("p1", AT + 500)]);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (true, 0), "read up to your answer, and still marked");
        let read = acted(&db, ANA, mark_at(ANA, false, "p1", AT + 2_000, "r1"));
        assert!(matches!(read, Acted::Queued { .. }), "marking it read isn't so already: {read:?}");
        arrives(&db, &["markChatAsRead", ANA_LID], &unread_value(AT + 1_000), 1);
        assert!(!chat(&db, ANA).3, "the mark unread's echo, under her LID as jaw sent it, is held back");
    }

    #[test]
    fn the_phones_unpin_by_her_number_after_her_chats_merge_while_jaws_pin_under_her_lid_waits_is_held_back() {
        let db = db();
        chats(&db, &[ANA, ANA_LID]);
        assert!(matches!(acted(&db, ANA_LID, pin_at(ANA_LID, true, AT, "p1")), Acted::Queued { .. }));
        her_lid_joined(&db);
        assert_eq!(chat(&db, ANA).0, AT, "her number's chat takes the pin as her LID's merges into it");
        arrives(&db, &["pin_v1", ANA], &pin_value(false, AT - 1_000), 1);
        assert_eq!(chat(&db, ANA).0, AT, "the phone's unpin by her number, from before jaw's pin, is held back");
        let unpin = acted(&db, ANA, pin_at(ANA, false, AT + 1_000, "u1"));
        assert!(matches!(unpin, Acted::Queued { .. }), "unpinning it isn't so already: {unpin:?}");
        arrives(&db, &["pin_v1", ANA_LID], &pin_value(true, AT), 2);
        assert_eq!(chat(&db, ANA).0, 0, "the pin's echo, under her LID as jaw sent it, is held back");
        arrives(&db, &["pin_v1", ANA], &pin_value(false, AT + 1_000), 3);
        assert_eq!(awaited(&db, &["pin_v1", ANA]), None, "and the unpin's ends the wait");
    }

    #[test]
    fn of_two_waits_for_one_setting_as_her_chats_merge_the_one_for_jaws_later_patch_stays_whichever_chat_it_was() {
        let (t1, t2) = (AT + 3_600_000, AT + 7_200_000);
        let mute = |chat: &str, until: i64, at: i64, id: &str| {
            Does::Set(Setting::Mute { until, patch: patch_of(&["mute", chat], &mute_value(until, at), id) })
        };
        for (first, later) in [(ANA_LID, ANA), (ANA, ANA_LID)] {
            let db = db();
            chats(&db, &[ANA, ANA_LID]);
            assert!(matches!(acted(&db, first, mute(first, t1, AT, "m1")), Acted::Queued { .. }));
            assert!(matches!(acted(&db, later, mute(later, t2, AT + 1, "m2")), Acted::Queued { .. }));
            her_lid_joined(&db);
            assert_eq!((chat(&db, ANA).2, waits(&db)), (t2, 1), "one chat, muted until the later end, one wait");
            arrives(&db, &["mute", first], &mute_value(t1, AT), 1);
            assert_eq!(chat(&db, ANA).2, t2, "the echo of jaw's earlier patch, under {first}, is held back");
            arrives(&db, &["mute", later], &mute_value(t2, AT + 1), 2);
            assert_eq!((chat(&db, ANA).2, waits(&db)), (t2, 0), "and the later's, under {later}, ends the wait");
        }
    }

    #[test]
    fn a_phone_unstar_under_her_lid_while_jaws_star_waits_is_held_back_so_unstarring_isnt_so_already() {
        let db = db();
        written(&db, &[from_ana("m1", AT)]);
        her_lid_joined(&db);
        let index = ["star", ANA, "m1", "0", "0"];
        assert!(matches!(acted(&db, ANA, star_at(&index, true, AT, "s1")), Acted::Queued { .. }));
        arrives(&db, &["star", ANA_LID, "m1", "0", "0"], &star_value(false, AT - 1_000), 1);
        assert!(starred(&db, "m1"), "the phone's unstar under her LID, from before jaw's star, is held back");
        let unstar = acted(&db, ANA, star_at(&index, false, AT + 1_000, "s2"));
        assert!(matches!(unstar, Acted::Queued { .. }), "unstarring it isn't so already: {unstar:?}");
        arrives(&db, &index, &star_value(true, AT), 2);
        assert!(!starred(&db, "m1"), "the star's echo, taken before the unstar, is held back");
    }

    #[test]
    fn a_phone_unstar_naming_the_sender_by_another_of_his_jids_while_jaws_star_waits_is_held_back() {
        let db = db();
        let (group, rui_lid) = ("120363000000000005@g.us", "100000000003@lid");
        let from_rui = Message {
            chat: group.into(),
            id: "g1".into(),
            at: AT,
            sender: Some(RUI.into()),
            kind: "text",
            text: Some("g1".into()),
            summary: "g1".into(),
            live: true,
            ..Message::default()
        };
        written(&db, &[Change::Message(from_rui)]);
        written(&db, &[Change::Person(Person { jids: vec![RUI.into(), rui_lid.into()], ..Person::default() })]);
        let index = ["star", group, "g1", "0", RUI];
        assert!(matches!(acted(&db, group, star_at(&index, true, AT, "s1")), Acted::Queued { .. }));
        // The phone's unstar, from before jaw's star reached WhatsApp, names Rui by his LID.
        arrives(&db, &["star", group, "g1", "0", rui_lid], &star_value(false, AT - 1_000), 1);
        assert!(starred(&db, "g1"), "held back: the message is the one jaw starred, whoever it says sent it");
        arrives(&db, &index, &star_value(true, AT), 2);
        assert_eq!((starred(&db, "g1"), waits(&db)), (true, 0), "and jaw's echo ends the wait");
    }

    #[test]
    fn a_full_fetch_holding_her_pin_under_both_her_jids_ends_as_the_later_has_it_in_either_order() {
        let (unpin, pin) = (pin_value(false, AT), pin_value(true, AT - 60_000));
        for number_first in [true, false] {
            let db = db();
            chats(&db, &[ANA]);
            her_lid_joined(&db);
            // jaw's unpin under her number, and the phone's older pin under her LID (round two's P11).
            if number_first {
                arrives(&db, &["pin_v1", ANA], &unpin, 1);
                arrives(&db, &["pin_v1", ANA_LID], &pin, 2);
            } else {
                arrives(&db, &["pin_v1", ANA_LID], &pin, 1);
                arrives(&db, &["pin_v1", ANA], &unpin, 2);
            }
            assert_eq!(chat(&db, ANA).0, 0, "unpinned, as the later has it, her number's first: {number_first}");
            assert_eq!(kept(&db, &["pin_v1", ANA_LID]), Some(pin.encode_to_vec()), "and the older kept raw");
        }
    }

    #[test]
    fn across_her_jids_the_newest_held_stands_and_under_one_whatsapps_order_says_what_it_holds() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        arrives(&db, &["mute", ANA], &mute_value(0, AT), 1);
        arrives(&db, &["mute", ANA_LID], &mute_value(FOREVER, AT + 60_000), 2);
        assert_eq!(chat(&db, ANA).2, FOREVER, "the phone's later mute, under her LID, stands");
        arrives(&db, &["mute", ANA], &mute_value(0, AT + 59_000), 3);
        assert_eq!(chat(&db, ANA).2, FOREVER, "an unmute under her number a second earlier isn't newer");
        // Under her LID again, by a clock that's behind: that's what WhatsApp holds there now, which
        // leaves her number's unmute the newest it holds.
        let hour = AT + 3_600_000;
        arrives(&db, &["mute", ANA_LID], &mute_value(hour, AT + 40_000), 4);
        assert_eq!(chat(&db, ANA).2, 0, "so the unmute, kept raw till now, stands");
        arrives(&db, &["mute", ANA], &mute_value(hour, AT + 80_000), 5);
        assert_eq!(chat(&db, ANA).2, hour, "and a newer change under either applies as it comes");
    }

    #[test]
    fn a_messages_star_held_under_two_of_its_jids_ends_as_the_later_has_it() {
        let db = db();
        written(&db, &[from_ana("m1", AT - 120_000)]);
        her_lid_joined(&db);
        arrives(&db, &["star", ANA, "m1", "0", "0"], &star_value(true, AT), 1);
        arrives(&db, &["star", ANA_LID, "m1", "0", "0"], &star_value(false, AT - 60_000), 2);
        assert!(starred(&db, "m1"), "the phone's older unstar, naming her chat by her LID, is kept raw");
        let (group, rui_lid) = ("120363000000000005@g.us", "100000000003@lid");
        let from_rui = Message {
            chat: group.into(),
            id: "g1".into(),
            at: AT - 120_000,
            sender: Some(RUI.into()),
            kind: "text",
            text: Some("g1".into()),
            summary: "g1".into(),
            live: true,
            ..Message::default()
        };
        written(&db, &[Change::Message(from_rui)]);
        written(&db, &[Change::Person(Person { jids: vec![RUI.into(), rui_lid.into()], ..Person::default() })]);
        arrives(&db, &["star", group, "g1", "0", rui_lid], &star_value(true, AT), 3);
        arrives(&db, &["star", group, "g1", "0", RUI], &star_value(false, AT - 60_000), 4);
        assert!(starred(&db, "g1"), "nor is an older unstar naming its sender by his number");
    }

    #[test]
    fn a_read_the_phone_sends_under_her_lid_later_than_jaws_mark_unread_stands_and_the_marks_echo_leaves_it() {
        let db = db();
        written(&db, &[from_ana("m1", AT), from_ana("m2", AT + 10)]);
        her_lid_joined(&db);
        let number = ["markChatAsRead", ANA];
        assert!(matches!(acted(&db, ANA, mark_at(ANA, true, "m2", AT + 1_000, "u1")), Acted::Queued { .. }));
        // The phone's read names her LID, and is later than jaw's mark by its time, though WhatsApp
        // takes jaw's after it (round two's P4): across her JIDs the later time stands.
        arrives(&db, &["markChatAsRead", ANA_LID], &read_value(AT + 1_500), 1);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!((marked, unread), (false, 0), "read and unmarked, as the later of the two has it");
        let read = acted(&db, ANA, mark_at(ANA, false, "m2", AT + 2_000, "r1"));
        assert_eq!(read, Acted::Already, "so marking it read is so already");
        arrives(&db, &number, &unread_value(AT + 1_000), 2);
        let (.., marked, unread, _) = chat(&db, ANA);
        assert_eq!(
            (marked, unread, awaited(&db, &number)),
            (false, 0, None),
            "the mark unread's echo, older than the phone's read, ends the wait and leaves the chat read"
        );
    }

    #[test]
    fn while_jaws_mute_waits_a_change_under_her_lid_is_weighed_against_jaws_time_not_what_her_number_held() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        let number = ["mute", ANA];
        let muting =
            Does::Set(Setting::Mute { until: FOREVER, patch: patch_of(&number, &mute_value(FOREVER, AT), "m1") });
        assert!(matches!(acted(&db, ANA, muting), Acted::Queued { .. }));
        // The phone unmutes it under her number by a later clock, and WhatsApp takes jaw's mute after it.
        arrives(&db, &number, &mute_value(0, AT + 5_000), 1);
        assert_eq!(chat(&db, ANA).2, FOREVER, "held back: under her number, jaw's mute is what WhatsApp ends with");
        // Then it mutes it for an hour under her LID, later than jaw's mute but earlier than that unmute.
        let hour = AT + 3_600_000;
        arrives(&db, &["mute", ANA_LID], &mute_value(hour, AT + 2_000), 2);
        assert_eq!(chat(&db, ANA).2, hour, "later than jaw's mute, which is what's held under her number");
        arrives(&db, &number, &mute_value(FOREVER, AT), 3);
        assert_eq!(
            (chat(&db, ANA).2, awaited(&db, &number)),
            (hour, None),
            "jaw's echo, older than the hour's mute, ends the wait and changes nothing"
        );
    }

    #[test]
    fn of_two_values_held_under_her_jids_at_one_millisecond_the_same_stands_in_any_order() {
        let db = db();
        chats(&db, &[ANA]);
        her_lid_joined(&db);
        // Her number says unmuted and her LID muted for ever, both at AT, handed over by one full
        // fetch after another, in either order (round three's C1): her number's index sorts last.
        let (number, lid) = ((["mute", ANA], mute_value(0, AT)), (["mute", ANA_LID], mute_value(FOREVER, AT)));
        for (n, (index, value)) in [&number, &lid, &lid, &number, &number, &lid].into_iter().enumerate() {
            arrives(&db, index, value, n as i64 + 1);
            if n % 2 == 1 {
                assert_eq!(chat(&db, ANA).2, 0, "unmuted, as her number has it, after fetch {}", n / 2 + 1);
            }
        }
    }

    #[test]
    fn the_phones_later_unpin_under_the_jid_jaws_pin_named_is_held_back_whether_or_not_her_chats_merge_meanwhile() {
        for merging in [false, true] {
            let db = db();
            chats(&db, &[ANA_LID]);
            if merging {
                chats(&db, &[ANA]);
            }
            assert!(matches!(acted(&db, ANA_LID, pin_at(ANA_LID, true, AT, "p1")), Acted::Queued { .. }));
            let kept = if merging {
                her_lid_joined(&db);
                ANA
            } else {
                ANA_LID
            };
            // Later than jaw's pin by its time, but taken by WhatsApp first, under the JID jaw's pin
            // named, where WhatsApp ends with jaw's pin (round three's D0, and D1 with the merge).
            arrives(&db, &["pin_v1", ANA_LID], &pin_value(false, AT + 1_000), 1);
            assert_eq!(chat(&db, kept).0, AT, "held back, her chats merged: {merging}");
            let unpin = acted(&db, kept, pin_at(kept, false, AT + 2_000, "u1"));
            assert!(matches!(unpin, Acted::Queued { .. }), "so unpinning it isn't so already: {unpin:?}");
            arrives(&db, &["pin_v1", ANA_LID], &pin_value(true, AT), 2);
            assert_eq!(chat(&db, kept).0, 0, "and the pin's echo leaves jaw's unpin, her chats merged: {merging}");
        }
    }

    #[test]
    fn after_her_chats_merge_the_phones_later_unpin_under_her_number_stands_while_jaws_pin_under_her_lid_waits() {
        let db = db();
        chats(&db, &[ANA, ANA_LID]);
        assert!(matches!(acted(&db, ANA_LID, pin_at(ANA_LID, true, AT, "p1")), Acted::Queued { .. }));
        her_lid_joined(&db);
        // Her number, which jaw keeps the chat by now, isn't what jaw's pin named, so what comes
        // under it is weighed by its time (round three's E1).
        arrives(&db, &["pin_v1", ANA], &pin_value(false, AT + 1_000), 1);
        assert_eq!(chat(&db, ANA).0, 0, "unpinned, as the later of the two has it");
        let pin = acted(&db, ANA, pin_at(ANA, true, AT + 2_000, "p2"));
        assert!(matches!(pin, Acted::Queued { .. }), "so pinning it isn't so already: {pin:?}");
        arrives(&db, &["pin_v1", ANA_LID], &pin_value(true, AT), 2);
        assert_eq!(chat(&db, ANA).0, AT + 2_000, "and the first pin's echo, under her LID, leaves jaw's second");
    }

    #[test]
    fn the_phones_later_unstar_naming_the_sender_by_another_of_his_jids_stands_while_jaws_star_waits() {
        let db = db();
        let (group, rui_lid) = ("120363000000000005@g.us", "100000000003@lid");
        let from_rui = Message {
            chat: group.into(),
            id: "g1".into(),
            at: AT - 120_000,
            sender: Some(RUI.into()),
            kind: "text",
            text: Some("g1".into()),
            summary: "g1".into(),
            live: true,
            ..Message::default()
        };
        written(&db, &[Change::Message(from_rui)]);
        written(&db, &[Change::Person(Person { jids: vec![RUI.into(), rui_lid.into()], ..Person::default() })]);
        let index = ["star", group, "g1", "0", RUI];
        assert!(matches!(acted(&db, group, star_at(&index, true, AT, "s1")), Acted::Queued { .. }));
        // The phone's unstar names Rui by his LID, not as jaw's star did, and is later by its time
        // (round three's F1).
        arrives(&db, &["star", group, "g1", "0", rui_lid], &star_value(false, AT + 1_000), 1);
        assert!(!starred(&db, "g1"), "unstarred, as the later of the two has it");
        let star = acted(&db, group, star_at(&index, true, AT + 2_000, "s2"));
        assert!(matches!(star, Acted::Queued { .. }), "so starring it isn't so already: {star:?}");
        arrives(&db, &index, &star_value(true, AT), 2);
        assert!(starred(&db, "g1"), "and the first star's echo leaves jaw's second");
    }

    #[test]
    fn after_her_chats_merge_what_the_phone_sent_under_the_jid_jaws_pin_named_counts_as_jaws_pin() {
        let db = db();
        chats(&db, &[ANA, ANA_LID]);
        assert!(matches!(acted(&db, ANA_LID, pin_at(ANA_LID, true, AT, "p1")), Acted::Queued { .. }));
        her_lid_joined(&db);
        // Held back under her LID, which jaw's pin named, so WhatsApp ends there with jaw's pin.
        arrives(&db, &["pin_v1", ANA_LID], &pin_value(false, AT + 1_000), 1);
        assert_eq!(chat(&db, ANA).0, AT, "held back");
        // The phone pins it under her number, later than jaw's pin, though not than the unpin it
        // replaces under her LID.
        arrives(&db, &["pin_v1", ANA], &pin_value(true, AT + 500), 2);
        assert_eq!(chat(&db, ANA).0, AT + 500, "pinned at its time: what's held under her LID is jaw's pin");
    }

    #[test]
    fn a_patch_carrying_the_time_its_index_already_awaits_is_refused() {
        let db = db();
        chats(&db, &[ANA]);
        assert!(matches!(acted(&db, ANA, pin_at(ANA, true, AT, "p1")), Acted::Queued { .. }));
        let error = refused(&db, ANA, pin_at(ANA, false, AT, "p2"));
        assert!(error.to_string().contains("carry one time"), "{error}");
        assert_eq!((chat(&db, ANA).0, queued(&db).len()), (AT, 1), "nothing written or queued");
    }

    /// Each statement a chat action runs, and what the converter, a read on another device and jawd
    /// status read of what each index waits for, as SQLite plans them on a fresh database with no
    /// statistics: each reads an index and sorts nothing. The count is store.count's, of a table
    /// with a row an index at most.
    #[test]
    fn every_statement_a_chat_action_runs_reads_an_index_and_sorts_nothing() {
        let db = db();
        let statements = [
            ("a chat's state", CHAT_STATE, "SEARCH chats USING INDEX sqlite_autoindex_chats_1 (id=?)"),
            (
                "chats pinned elsewhere",
                PINNED_ELSEWHERE,
                "SEARCH chats USING INDEX chats_by_sort (archived=? AND sort>?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH m USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("starred", STARRED, "SEARCH messages USING INTEGER PRIMARY KEY (rowid=?)"),
            (
                "shown deleted",
                SHOWN_DELETED,
                "SEARCH t USING INTEGER PRIMARY KEY (rowid=?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH ra USING INDEX outbox_by_target (target=?)\nCORRELATED SCALAR SUBQUERY 4\nSEARCH ma USING COVERING INDEX chat_merges_by_keep (keep=?)\nCORRELATED SCALAR SUBQUERY 2\nSEARCH mx USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH ga USING INDEX messages_by_id (chat=? AND id=?)\nCORRELATED SCALAR SUBQUERY 3\nSEARCH rb USING INDEX outbox_by_target (target=?)\nCORRELATED SCALAR SUBQUERY 6\nSEARCH mk USING INTEGER PRIMARY KEY (rowid=?)\nSEARCH gk USING INDEX messages_by_id (chat=? AND id=?)\nCORRELATED SCALAR SUBQUERY 5\nSEARCH rc USING INDEX outbox_by_target (target=?)",
            ),
            ("the time an index awaits", AWAITED_AT, "SEARCH app_state_awaiting USING PRIMARY KEY (key=?)"),
            ("an index awaiting a patch", AWAIT, ""),
            (
                "what an index awaits",
                AWAITED,
                "SEARCH a USING PRIMARY KEY (key=?)\nCORRELATED SCALAR SUBQUERY 1\nSEARCH o USING INTEGER PRIMARY KEY (rowid=?)",
            ),
            ("an index's wait over", WAITED, "SEARCH app_state_awaiting USING PRIMARY KEY (key=?)"),
            (
                "the waits the converter has gone past",
                PASSED,
                "SEARCH app_state_awaiting USING INDEX app_state_awaiting_by_echo (echo_by>? AND echo_by<?)",
            ),
            (
                "the waits for a setting under a chat's JID, as the chat merges away",
                WAITING_UNDER,
                "SEARCH app_state_awaiting USING PRIMARY KEY (key>? AND key<?)",
            ),
            ("a wait under the JID of the chat kept", MOVED, ""),
            (
                "what WhatsApp holds for a setting under a JID of its chat",
                HELD_UNDER,
                "SEARCH app_state USING PRIMARY KEY (key>? AND key<?)",
            ),
            (
                "the indexes awaiting, for jawd status",
                "SELECT count(*) FROM app_state_awaiting",
                "SCAN app_state_awaiting USING COVERING INDEX sqlite_autoindex_app_state_awaiting_2",
            ),
        ];
        for (what, sql, expected) in statements {
            let mut explain = db.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let count = explain.parameter_count();
            let plan: Vec<String> = explain
                .query_map(rusqlite::params_from_iter(std::iter::repeat_n(0, count)), |r| r.get(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(plan.join("\n"), expected, "{what}");
        }
    }
}
