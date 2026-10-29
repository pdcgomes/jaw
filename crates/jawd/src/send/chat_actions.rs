//! Chat actions from jaw: `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
//! Each is done to its row at once, in one synced commit with the app-state patches that tell your
//! other devices, before it answers (store::chat_actions); the patches then go through the outbox
//! after the chat's earlier sends, as a deletion for you does. Each patch is built as whatsmeow's
//! builders build it and carries the time jawd wrote, later than any chat action's before it, so
//! WhatsApp's echo of it through app state is known by that time and finds the row as it is; what
//! WhatsApp sends for its index before the echo is held back (store::chat_actions).
//! A patch saying a chat is read goes after the chat's read receipts (`receipts_first`); for a chat
//! jaw holds none of the messages of, its echo is what reads the chat (store::chat_actions). A patch
//! WhatsApp refuses for good, or that runs out of tries, is kept in quarantine, what it set is put
//! back, every interface is told which it was, and its collection is fetched again, so the chat ends
//! as the phone has it (`refused`).

use std::sync::Arc;

use jaw_proto::send::{ChatArchive, ChatMarkUnread, ChatMute, ChatPin, Star};
use jaw_proto::sync::Tone;
use jaw_proto::{AccountState, Error, code};
use prost::Message as _;
use rusqlite::OptionalExtension;
use serde_json::Value;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::{self, Target};
use super::outbox::{bad_param, message_id, queue, settle};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert::app_state::FOREVER;
use crate::engine::{EngineError, pb};
use crate::store::chat_actions::{Act, Acted, Does, Outcome, PINNED_MOST, Patch, Setting, Undo};
use crate::store::outbox::{Outboxed, Step};
use crate::store::{Change, read};

/// A chat an action names: the chat it's being merged into, or itself, with its kind.
pub(super) const CHAT_TO_ACT_ON: &str = "SELECT coalesce(k.id, c.id), coalesce(k.kind, c.kind) FROM chats c
    LEFT JOIN chat_merges m ON m.gone = c.n LEFT JOIN chats k ON k.n = m.keep WHERE c.id = ?1";
/// A chat's newest message, with who sent it.
pub(super) const NEWEST: &str = "SELECT m.id, m.at, m.mine, j.jid FROM chats c JOIN messages m ON m.n = c.last_message
    LEFT JOIN person_jids j ON j.n = m.sender WHERE c.id = ?1";

/// A chat's newest message, which an archive or a read names.
#[derive(Clone, Debug)]
pub(super) struct Newest {
    pub id: String,
    pub at: i64,
    pub mine: bool,
    /// Who sent it, by the JID it came from; `None` for yours.
    pub sender: Option<String>,
}

fn patch(name: &str, index: Vec<String>, version: i32, value: SyncActionValue) -> pb::AppStatePatch {
    pb::AppStatePatch { name: name.into(), index, version, value: value.encode_to_vec() }
}

/// Chat `chat` pinned at `at`, or unpinned (whatsmeow's BuildPin).
pub(super) fn pinned(chat: &str, pinned: bool, at: i64) -> pb::AppStatePatch {
    let action = PinAction { pinned: Some(pinned) };
    let value = SyncActionValue { timestamp: Some(at), pin_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["pin_v1".into(), chat.into()], 5, value)
}

/// Chat `chat` archived up to `range`, or brought back: BuildArchive's first mutation. Its second,
/// the unpin, goes as a patch of its own (`pinned`), since the engine sends one mutation a patch.
pub(super) fn archived(chat: &str, archived: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch {
    let action = ArchiveChatAction { archived: Some(archived), message_range: Some(range) };
    let value = SyncActionValue { timestamp: Some(at), archive_chat_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["archive".into(), chat.into()], 3, value)
}

/// Chat `chat` muted until `until`, for ever when it's negative, as jaw reads any negative end
/// back, or from FOREVER on; or unmuted at 0 (BuildMuteAbs).
pub(super) fn muted(chat: &str, until: i64, at: i64) -> pb::AppStatePatch {
    let action = match until {
        0 => MuteAction { muted: Some(false), ..Default::default() },
        ..0 | FOREVER.. => MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() },
        until => MuteAction { muted: Some(true), mute_end_timestamp: Some(until), ..Default::default() },
    };
    let value = SyncActionValue { timestamp: Some(at), mute_action: Some(action), ..Default::default() };
    patch("regular_high", vec!["mute".into(), chat.into()], 2, value)
}

/// Chat `chat` marked unread, or read up to `range` (BuildMarkChatAsRead).
pub(super) fn marked(chat: &str, unread: bool, range: SyncActionMessageRange, at: i64) -> pb::AppStatePatch {
    let action = MarkChatAsReadAction { read: Some(!unread), message_range: Some(range) };
    let value = SyncActionValue { timestamp: Some(at), mark_chat_as_read_action: Some(action), ..Default::default() };
    patch("regular_low", vec!["markChatAsRead".into(), chat.into()], 3, value)
}

/// Message `target` starred, or its star taken off (BuildStar), named as a deletion for you names
/// it: its chat, its ID, whether it's yours, and who sent it in a group, "0" for yours and in a
/// one-to-one chat.
pub(super) fn starred(target: &Target, starred: bool, at: i64) -> pb::AppStatePatch {
    let sender = match (&target.sender, target.group && !target.mine) {
        (Some(sender), true) => sender.clone(),
        _ => "0".into(),
    };
    let mine = if target.mine { "1" } else { "0" };
    let index = vec!["star".into(), target.chat.clone(), target.id.clone(), mine.into(), sender];
    let action = StarAction { starred: Some(starred) };
    let value = SyncActionValue { timestamp: Some(at), star_action: Some(action), ..Default::default() };
    patch("regular_high", index, 2, value)
}

/// The messages an archive or a read names: the chat's newest, at its time in seconds, as app state
/// keeps them, or with none the time it's done at (whatsmeow's newMessageRange).
pub(super) fn range(chat: &str, group: bool, newest: Option<&Newest>, at: i64) -> SyncActionMessageRange {
    let Some(newest) = newest else {
        return SyncActionMessageRange { last_message_timestamp: Some(at.div_euclid(1000)), ..Default::default() };
    };
    let seconds = newest.at.div_euclid(1000);
    let key = MessageKey {
        remote_jid: Some(chat.into()),
        from_me: Some(newest.mine),
        id: Some(newest.id.clone()),
        participant: if group && !newest.mine { newest.sender.clone() } else { None },
    };
    SyncActionMessageRange {
        last_message_timestamp: Some(seconds),
        messages: vec![SyncActionMessage { key: Some(key), timestamp: Some(seconds) }],
        ..Default::default()
    }
}

/// The chat an action names.
struct Named {
    /// Its JID as jaw keeps it.
    id: String,
    group: bool,
    newest: Option<Newest>,
}

/// Refuses what needs a linked account while none is.
fn linked(inner: &Inner) -> Result<(), Error> {
    match inner.live.lock().expect("live state").account.state {
        AccountState::Linked => Ok(()),
        AccountState::Unlinked | AccountState::Linking => {
            Err(Error::new(code::NOT_LINKED, "the account isn't linked; link it first"))
        }
    }
}

/// The chat `chat` names, with its newest message: error 3 for a chat jawd doesn't have, and 4 for
/// a community's own row, whose groups carry these.
async fn named(inner: &Inner, chat: &str) -> Result<Named, Error> {
    let asked = chat.to_string();
    let found = query(inner, move |db| {
        let Some((id, kind)) = db
            .prepare_cached(CHAT_TO_ACT_ON)?
            .query_row([&asked], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .optional()?
        else {
            return Ok(None);
        };
        let newest = db
            .prepare_cached(NEWEST)?
            .query_row([&id], |r| Ok(Newest { id: r.get(0)?, at: r.get(1)?, mine: r.get(2)?, sender: r.get(3)? }))
            .optional()?;
        Ok(Some((id, kind, newest)))
    })
    .await?;
    match found {
        None => Err(no_chat(chat)),
        Some((_, kind, _)) if kind == "community" => Err(community()),
        Some((id, kind, newest)) => Ok(Named { id, group: matches!(kind.as_str(), "group" | "announcement"), newest }),
    }
}

fn community() -> Error {
    Error::new(code::NOT_ALLOWED, "a community's own row has none of these; its groups do")
}

fn pinned_most() -> Error {
    let mut error = Error::new(
        code::LIMIT,
        format!("{PINNED_MOST} chats are pinned already, as many as WhatsApp allows; unpin one first"),
    );
    error.data = Some(serde_json::json!({ "limit": "pinned", "most": PINNED_MOST }));
    error
}

/// A patch to queue, with an ID of its own for its outbox row, and the index and time it carries.
fn queued(patch: pb::AppStatePatch) -> Result<Patch, Error> {
    let id = message_id().map_err(|e| internal("make an ID for the patch", e.into()))?;
    let key = serde_json::to_string(&patch.index).map_err(|e| internal("name the patch's index", e.into()))?;
    let value =
        SyncActionValue::decode(patch.value.as_slice()).map_err(|e| internal("read the patch's time", e.into()))?;
    Ok(Patch { id, raw: patch.encode_to_vec(), key, at: value.timestamp.unwrap_or_default() })
}

pub(super) async fn pin(inner: &Arc<Inner>, pin: ChatPin) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &pin.chat).await?;
    let doing = if pin.pinned { "pin the chat" } else { "unpin the chat" };
    act(inner, chat.id.clone(), doing, false, |at| {
        let patch = queued(pinned(&chat.id, pin.pinned, at))?;
        Ok(Does::Set(Setting::Pin { at: pin.pinned.then_some(at), patch }))
    })
    .await
}

pub(super) async fn archive(inner: &Arc<Inner>, archive: ChatArchive) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &archive.chat).await?;
    let doing = if archive.archived { "archive the chat" } else { "unarchive the chat" };
    act(inner, chat.id.clone(), doing, false, |at| {
        let range = range(&chat.id, chat.group, chat.newest.as_ref(), at);
        let unpin = Some(queued(pinned(&chat.id, false, at))?);
        let patch = queued(archived(&chat.id, archive.archived, range, at))?;
        Ok(Does::Set(Setting::Archive { archived: archive.archived, patch, unpin }))
    })
    .await
}

pub(super) async fn mute(inner: &Arc<Inner>, mute: ChatMute) -> Result<Value, Error> {
    let until = match mute.until {
        None => 0,
        Some(until) if until == -1 || until >= FOREVER => FOREVER,
        Some(until) if until <= now_ms() => {
            return Err(bad_param("until", "that time has passed; mute until a later one, or -1 for ever"));
        }
        Some(until) => until,
    };
    linked(inner)?;
    let chat = named(inner, &mute.chat).await?;
    let doing = if until == 0 { "unmute the chat" } else { "mute the chat" };
    act(inner, chat.id.clone(), doing, false, |at| {
        Ok(Does::Set(Setting::Mute { until, patch: queued(muted(&chat.id, until, at))? }))
    })
    .await
}

pub(super) async fn mark_unread(inner: &Arc<Inner>, mark: ChatMarkUnread) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &mark.chat).await?;
    let doing = if mark.unread { "mark the chat unread" } else { "mark the chat read" };
    act(inner, chat.id.clone(), doing, !mark.unread, |at| {
        let range = range(&chat.id, chat.group, chat.newest.as_ref(), at);
        let patch = queued(marked(&chat.id, mark.unread, range, at))?;
        let newest = chat.newest.as_ref().map(|newest| newest.id.clone());
        Ok(Does::Set(Setting::MarkUnread { unread: mark.unread, newest, patch }))
    })
    .await
}

pub(super) async fn star(inner: &Arc<Inner>, star: Star) -> Result<Value, Error> {
    linked(inner)?;
    let target = actions::find(inner, star.chat, star.message).await?;
    if star.starred && !actions::someones(&target.kind) {
        return Err(Error::new(code::NOT_ALLOWED, "a line about the chat can't be starred"));
    }
    if star.starred && target.deleted {
        return Err(Error::new(code::NOT_ALLOWED, "a deleted message can't be starred"));
    }
    let doing = if star.starred { "star the message" } else { "unstar the message" };
    act(inner, target.chat.clone(), doing, false, |at| {
        let patch = queued(starred(&target, star.starred, at))?;
        Ok(Does::Star { id: target.id.clone(), starred: star.starred, patch })
    })
    .await
}

/// Whether a patch says its chat is read: it goes after the chat's read receipts, since WhatsApp's
/// echo of it at jaw's own position settles the receipts jaw owes there.
pub(super) fn reads_the_chat(patch: &pb::AppStatePatch) -> bool {
    patch.index.first().is_some_and(|kind| kind == "markChatAsRead")
        && SyncActionValue::decode(patch.value.as_slice())
            .ok()
            .and_then(|value| value.mark_chat_as_read_action)
            .and_then(|action| action.read)
            .unwrap_or(false)
}

/// Sends the read receipts chat `chat` owes, at once, before its patch saying it's read; a failure is
/// the patch's, tried again as it is. The chat is found as the writer finds it now, as the patch's
/// echo will find it, since it may have merged into another since the patch was queued.
pub(super) async fn receipts_first(inner: &Inner, chat: &str) -> Result<(), EngineError> {
    let failed =
        |why: String| EngineError::Failed { message: format!("its read receipts didn't go first: {why}"), retry: true };
    let jid = chat.to_string();
    let found = query(inner, move |db| match read::chat_named(db, &jid)? {
        Some(named) => read::chat_n(db, &named),
        None => Ok(None),
    });
    let Some(n) = found.await.map_err(|e| failed(e.message))? else {
        return Ok(());
    };
    super::reads::tell_now(inner, n).await.map_err(|e| failed(format!("{e:#}")))
}

/// How many times the fetch after a patch is given up on is tried, while WhatsApp can be reached and
/// the engine that gave it up runs; the next engine to start fetches every collection in full anyway.
const FETCH_TRIES: u32 = 10;

/// What putting back a refused patch undoes, by its index; nothing for a deletion for you, whose
/// message stays deleted here.
pub(super) fn undo_of(index: &[String]) -> Option<Undo> {
    let chat = index.get(1)?.clone();
    match index.first()?.as_str() {
        "pin_v1" => Some(Undo::Pin { chat }),
        "archive" => Some(Undo::Archive { chat }),
        "mute" => Some(Undo::Mute { chat }),
        "markChatAsRead" => Some(Undo::MarkUnread { chat }),
        "star" => Some(Undo::Star { chat, id: index.get(2)?.clone() }),
        _ => None,
    }
}

/// What a patch did, as the notice for one given up on names it.
pub(super) fn what_it_did(patch: &pb::AppStatePatch) -> &'static str {
    let value = SyncActionValue::decode(patch.value.as_slice()).unwrap_or_default();
    let on = |set: Option<bool>| set.unwrap_or(false);
    match patch.index.first().map(String::as_str) {
        Some("pin_v1") if on(value.pin_action.as_ref().and_then(|a| a.pinned)) => "pinning a chat",
        Some("pin_v1") => "unpinning a chat",
        Some("archive") if on(value.archive_chat_action.as_ref().and_then(|a| a.archived)) => "archiving a chat",
        Some("archive") => "unarchiving a chat",
        Some("mute") if on(value.mute_action.as_ref().and_then(|a| a.muted)) => "muting a chat",
        Some("mute") => "unmuting a chat",
        Some("markChatAsRead") if on(value.mark_chat_as_read_action.as_ref().and_then(|a| a.read)) => {
            "marking a chat read"
        }
        Some("markChatAsRead") => "marking a chat unread",
        Some("star") if on(value.star_action.as_ref().and_then(|a| a.starred)) => "starring a message",
        Some("star") => "unstarring a message",
        Some("deleteMessageForMe") => "deleting a message for you",
        _ => "a change",
    }
}

/// Why jaw gave up on an app-state patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GivenUp {
    /// WhatsApp refused it for good.
    Refused,
    /// It ran out of tries on failures that could be retried, so WhatsApp may have it after all.
    OutOfTries,
    /// Its outbox row doesn't decode, so it never went.
    Undecodable,
}

/// What every interface is told of `patch`, given up on for `why` with `error`.
fn notice_for(why: GivenUp, patch: Option<&pb::AppStatePatch>, error: &str) -> String {
    let what = patch.map_or("a change", what_it_did);
    let deletion = patch.is_some_and(|patch| patch.index.first().is_some_and(|kind| kind == "deleteMessageForMe"));
    match why {
        // Its message stays deleted here, so it mustn't read as put back.
        GivenUp::Refused if deletion => {
            format!("WhatsApp refused telling your other devices you deleted a message: {error}")
        }
        GivenUp::Refused => format!("WhatsApp refused {what}: {error}"),
        GivenUp::OutOfTries => format!("jaw couldn't confirm {what} with WhatsApp: {error}"),
        GivenUp::Undecodable => format!("jaw couldn't tell your other devices of {what}: {error}"),
    }
}

/// Keeps app-state outbox row `row`, given up on for `why`, in quarantine with `error` and its
/// patch, puts back what the patch set, tells every interface, and fetches the patch's collection
/// again, so the chat ends as the phone has it. The store puts the field back only while the patch
/// is its index's latest, since a later patch's intent stands; the fetch then brings back what was
/// held back for the index. A deletion for you puts nothing back and fetches nothing, its message
/// staying deleted here; a patch that doesn't decode, whose collection nobody knows, fetches every
/// collection.
pub(super) async fn refused(
    inner: &Inner,
    row: &Outboxed,
    patch: Option<&pb::AppStatePatch>,
    error: String,
    why: GivenUp,
) {
    let undo = patch.and_then(|patch| undo_of(&patch.index));
    // Empty names are every collection.
    let fetch = match patch {
        Some(patch) if undo.is_some() => Some(vec![patch.name.clone()]),
        Some(_) => None,
        None => Some(Vec::new()),
    };
    let text = notice_for(why, patch, &error);
    eprintln!("jawd: app state {} in {}: {text}", row.id, row.chat);
    settle(inner, vec![Change::Outbox(Step::Refused { outbox: row.outbox, error, undo })]).await;
    super::notice(inner, text, Tone::Warning);
    if let Some(names) = fetch {
        fetch_again(inner, names);
    }
}

/// Asks the engine running now to fetch collections `names` again in full, trying again while it
/// runs. Each try waits until WhatsApp can be reached, as a send does, so tries are spent only
/// then, and for its turn with jaw's own patches and sync's full fetches of app state, which it
/// holds until every mutation is kept.
fn fetch_again(inner: &Inner, names: Vec<String>) {
    let Some(engine) = inner.engine.clone() else { return };
    let (timing, mut connected) = (inner.timing, inner.connected.subscribe());
    tokio::spawn(async move {
        let restarts = engine.status().restarts;
        let mut pause = timing.retry_first;
        for tried in 0..FETCH_TRIES {
            if tried > 0 {
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(timing.retry_most);
            }
            let reachable = connected.wait_for(|connected| *connected).await.is_ok();
            if !reachable || engine.status().restarts != restarts {
                return;
            }
            let resync = pb::command::Kind::AppStateResync(pb::AppStateResync { names: names.clone() });
            let turn = engine.app_state().take().await;
            let answer = engine.call(resync).await;
            drop(turn);
            match answer {
                Ok(_) => return,
                Err(e) => eprintln!("jawd: can't fetch app state again after giving up on a patch: {e}"),
            }
        }
        eprintln!(
            "jawd: stopped fetching app state again after giving up on a patch; the next engine to start fetches it"
        );
    });
}

/// Commits a chat action, synced, and queues the patches it wrote after the chat's earlier sends; a
/// chat read here owes its read receipts, which `owes_receipts` says. `does` makes the action done
/// at `at`, later than every chat action jawd has done since it started, so no two of jaw's patches
/// for an index carry one time, which its echo is known by.
async fn act(
    inner: &Arc<Inner>,
    chat: String,
    doing: &str,
    owes_receipts: bool,
    does: impl FnOnce(i64) -> Result<Does, Error>,
) -> Result<Value, Error> {
    let outcome = Outcome::default();
    let mut last = inner.enqueue.lock().await;
    let at = next(*last, now_ms());
    *last = at;
    let act = Act { chat: chat.clone(), does: does(at)?, outcome: outcome.clone() };
    durably(inner, vec![Change::ChatAction(act)], doing).await?;
    match outcome.take() {
        Some(Acted::Queued { chat, chat_n, outbox }) => {
            for outbox in outbox {
                queue(inner, outbox, chat.clone(), false);
            }
            if owes_receipts {
                super::reads::want(inner, chat_n);
            }
            Ok(Value::Object(Default::default()))
        }
        Some(Acted::Already) => Ok(Value::Object(Default::default())),
        Some(Acted::Gone) => Err(Error::new(code::NOT_FOUND, format!("{chat} went before jawd could {doing}"))),
        Some(Acted::Community) => Err(community()),
        Some(Acted::Pinned) => Err(pinned_most()),
        Some(Acted::Full { waiting, failed }) => Err(actions::full(waiting, failed)),
        None => Err(Error::new(code::INTERNAL, format!("jawd kept nothing, so it couldn't {doing}"))),
    }
}

/// The time to do a chat action at: `now`, or just after `last`, the last one's, while the clock
/// hasn't moved past it.
fn next(last: i64, now: i64) -> i64 {
    now.max(last + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANA: &str = "351912345678@s.whatsapp.net";
    const GROUP: &str = "120363012345678901@g.us";
    const RUI: &str = "351913333333@s.whatsapp.net";
    const AT: i64 = 1_792_000_000_123;

    fn value(patch: &pb::AppStatePatch) -> SyncActionValue {
        SyncActionValue::decode(patch.value.as_slice()).unwrap()
    }

    fn target(chat: &str, id: &str, mine: bool, sender: Option<&str>) -> Target {
        Target {
            chat: chat.into(),
            group: chat.ends_with("@g.us"),
            id: id.into(),
            mine,
            sender: sender.map(str::to_string),
            at: AT,
            kind: "text".into(),
            deleted: false,
            status: None,
            n: 1,
            chat_n: 1,
            view_once: false,
            forwarded: 0,
            raw: None,
            text: None,
            edited: false,
            deleting: false,
        }
    }

    #[test]
    fn a_pin_and_a_mute_are_the_mutations_whatsmeow_builds_with_the_time_jawd_wrote() {
        let pin = pinned(ANA, true, AT);
        assert_eq!(
            (pin.name.as_str(), pin.index.as_slice(), pin.version),
            ("regular_low", &["pin_v1".to_string(), ANA.to_string()][..], 5)
        );
        assert_eq!((value(&pin).timestamp, value(&pin).pin_action.unwrap().pinned), (Some(AT), Some(true)));
        assert_eq!(value(&pinned(ANA, false, AT)).pin_action.unwrap().pinned, Some(false));
        let mute = |until: i64| {
            let patch = muted(ANA, until, AT);
            let decoded = value(&patch);
            let action = decoded.mute_action.unwrap();
            (patch.name, patch.index, patch.version, decoded.timestamp, action.muted, action.mute_end_timestamp)
        };
        let index = vec!["mute".to_string(), ANA.to_string()];
        let until = |end: i64| ("regular_high".to_string(), index.clone(), 2, Some(AT), Some(true), Some(end));
        assert_eq!(mute(AT + 3_600_000), until(AT + 3_600_000));
        assert_eq!(mute(FOREVER), until(-1), "for ever is -1");
        assert_eq!(mute(i64::MAX), until(-1), "as is the terminal's always");
        assert_eq!(mute(-1), until(-1), "and the Mac's");
        assert_eq!(mute(-5), until(-1), "and any other negative, which jaw reads back as for ever");
        assert_eq!(mute(0), ("regular_high".into(), index, 2, Some(AT), Some(false), None), "unmuted, with no end");
    }

    #[test]
    fn an_archive_and_a_read_name_the_chats_newest_message_in_seconds_as_whatsmeow_does() {
        let newest = Newest { id: "g9".into(), at: AT + 500, mine: false, sender: Some(RUI.into()) };
        let at = AT + 5_000;
        let archive = archived(GROUP, true, range(GROUP, true, Some(&newest), at), at);
        assert_eq!(
            (archive.name.as_str(), archive.index.as_slice(), archive.version),
            ("regular_low", &["archive".to_string(), GROUP.to_string()][..], 3)
        );
        assert_eq!(value(&archive).timestamp, Some(at));
        let action = value(&archive).archive_chat_action.unwrap();
        let named = action.message_range.unwrap();
        let second = Some(1_792_000_000);
        assert_eq!((action.archived, named.last_message_timestamp), (Some(true), second), "its second, floored");
        let key = named.messages[0].key.clone().unwrap();
        assert_eq!(
            (key.remote_jid.as_deref(), key.id.as_deref(), key.from_me, key.participant.as_deref()),
            (Some(GROUP), Some("g9"), Some(false), Some(RUI))
        );
        assert_eq!(named.messages[0].timestamp, second);
        let brought_back = archived(GROUP, false, range(GROUP, true, Some(&newest), at), at);
        assert_eq!(value(&brought_back).archive_chat_action.unwrap().archived, Some(false));
        let mine = Newest { id: "d1".into(), at: AT, mine: true, sender: None };
        let read = marked(ANA, false, range(ANA, false, Some(&mine), AT), AT);
        assert_eq!(
            (read.name.as_str(), read.index.as_slice(), read.version),
            ("regular_low", &["markChatAsRead".to_string(), ANA.to_string()][..], 3)
        );
        assert_eq!(value(&read).timestamp, Some(AT));
        let action = value(&read).mark_chat_as_read_action.unwrap();
        assert_eq!(action.read, Some(true));
        let key = action.message_range.unwrap().messages[0].key.clone().unwrap();
        assert_eq!((key.from_me, key.participant), (Some(true), None), "yours names nobody");
        let theirs = Newest { id: "d2".into(), at: AT, mine: false, sender: Some(ANA.into()) };
        let key = range(ANA, false, Some(&theirs), AT).messages[0].key.clone().unwrap();
        assert_eq!((key.from_me, key.participant), (Some(false), None), "nor theirs in a one-to-one chat");
        let unread = marked(ANA, true, range(ANA, false, None, AT), AT);
        assert_eq!(value(&unread).mark_chat_as_read_action.unwrap().read, Some(false));
        let empty = range(ANA, false, None, AT);
        assert_eq!((empty.last_message_timestamp, empty.messages.len()), (Some(AT / 1000), 0), "no message: now");
    }

    #[test]
    fn a_queued_patch_names_the_index_and_carries_the_time_of_the_patch_it_queues() {
        let range = range(GROUP, true, None, AT + 2);
        for patch in [pinned(ANA, true, AT), muted(ANA, FOREVER, AT + 1), archived(GROUP, true, range, AT + 2)] {
            let queued = queued(patch).unwrap();
            let raw = pb::AppStatePatch::decode(queued.raw.as_slice()).unwrap();
            let carried = (serde_json::to_string(&raw.index).unwrap(), value(&raw).timestamp.unwrap());
            assert_eq!((queued.key, queued.at), carried, "{:?}", raw.index);
        }
    }

    #[test]
    fn a_star_names_its_message_as_a_deletion_for_you_does() {
        let index = |target: &Target| starred(target, true, AT).index;
        assert_eq!(index(&target(GROUP, "g1", false, Some(RUI))), ["star", GROUP, "g1", "0", RUI]);
        assert_eq!(index(&target(GROUP, "g2", true, None)), ["star", GROUP, "g2", "1", "0"]);
        assert_eq!(index(&target(ANA, "d1", false, Some(ANA))), ["star", ANA, "d1", "0", "0"]);
        assert_eq!(index(&target(ANA, "d2", true, None)), ["star", ANA, "d2", "1", "0"]);
        let star = starred(&target(ANA, "d1", false, Some(ANA)), true, AT);
        assert_eq!(
            (star.name.as_str(), star.version, value(&star).star_action.unwrap().starred, value(&star).timestamp),
            ("regular_high", 2, Some(true), Some(AT))
        );
        let unstar = starred(&target(ANA, "d1", false, Some(ANA)), false, AT);
        assert_eq!(value(&unstar).star_action.unwrap().starred, Some(false));
    }

    #[test]
    fn only_a_patch_saying_the_chat_is_read_goes_after_its_receipts() {
        let none = range(ANA, false, None, AT);
        assert!(reads_the_chat(&marked(ANA, false, none.clone(), AT)));
        assert!(!reads_the_chat(&marked(ANA, true, none, AT)));
        assert!(!reads_the_chat(&pinned(ANA, true, AT)));
    }

    #[test]
    fn a_refused_patch_puts_back_what_it_set_and_is_said_as_what_it_did() {
        let none = || range(ANA, false, None, AT);
        let undo = |patch: pb::AppStatePatch| undo_of(&patch.index);
        assert_eq!(undo(pinned(ANA, true, AT)), Some(Undo::Pin { chat: ANA.into() }));
        assert_eq!(undo(archived(ANA, true, none(), AT)), Some(Undo::Archive { chat: ANA.into() }));
        assert_eq!(undo(muted(ANA, FOREVER, AT)), Some(Undo::Mute { chat: ANA.into() }));
        assert_eq!(undo(marked(ANA, true, none(), AT)), Some(Undo::MarkUnread { chat: ANA.into() }));
        let star = starred(&target(ANA, "d1", false, Some(ANA)), true, AT);
        assert_eq!(undo(star), Some(Undo::Star { chat: ANA.into(), id: "d1".into() }));
        let deletion = pb::AppStatePatch {
            name: "regular_high".into(),
            index: ["deleteMessageForMe", ANA, "d1", "0", "0"].iter().map(|part| part.to_string()).collect(),
            version: 3,
            value: Vec::new(),
        };
        assert_eq!(undo_of(&deletion.index), None, "a message deleted for you stays deleted here");
        let said: Vec<&str> = [
            pinned(ANA, true, AT),
            pinned(ANA, false, AT),
            archived(ANA, true, none(), AT),
            muted(ANA, 0, AT),
            marked(ANA, false, none(), AT),
            starred(&target(ANA, "d1", false, None), false, AT),
            deletion,
        ]
        .iter()
        .map(what_it_did)
        .collect();
        assert_eq!(
            said,
            [
                "pinning a chat",
                "unpinning a chat",
                "archiving a chat",
                "unmuting a chat",
                "marking a chat read",
                "unstarring a message",
                "deleting a message for you"
            ]
        );
    }

    #[test]
    fn a_chat_action_is_done_later_than_the_last_whatever_the_clock_says() {
        assert_eq!(next(AT, AT - 5), AT + 1, "a clock gone back");
        assert_eq!(next(AT, AT), AT + 1, "the same millisecond");
        assert_eq!(next(AT, AT + 5), AT + 5, "a clock moved on: now");
    }

    #[test]
    fn a_patch_given_up_on_is_said_as_why_it_was() {
        let pin = pinned(ANA, true, AT);
        let deletion = actions::deleted_for_me(&target(ANA, "d1", false, Some(ANA)), AT);
        assert_eq!(notice_for(GivenUp::Refused, Some(&pin), "e"), "WhatsApp refused pinning a chat: e");
        assert_eq!(
            notice_for(GivenUp::Refused, Some(&deletion), "e"),
            "WhatsApp refused telling your other devices you deleted a message: e",
            "nothing put back, the message staying deleted here"
        );
        assert_eq!(
            notice_for(GivenUp::OutOfTries, Some(&pin), "e"),
            "jaw couldn't confirm pinning a chat with WhatsApp: e",
            "true whether or not WhatsApp took it"
        );
        assert_eq!(
            notice_for(GivenUp::OutOfTries, Some(&deletion), "e"),
            "jaw couldn't confirm deleting a message for you with WhatsApp: e"
        );
        assert_eq!(
            notice_for(GivenUp::Undecodable, None, "e"),
            "jaw couldn't tell your other devices of a change: e",
            "WhatsApp never saw it"
        );
    }
}
