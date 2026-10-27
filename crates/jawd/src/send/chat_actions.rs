//! Chat actions from jaw: `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
//! Each is done to its row at once, in one synced commit with the app-state patches that tell your
//! other devices, before it answers (store::chat_actions); the patches then go through the outbox
//! after the chat's earlier sends, as a deletion for you does. Each patch is built as whatsmeow's
//! builders build it and carries the time jawd wrote, later than any chat action's before it, so
//! WhatsApp's echo of it through app state is known by that time and finds the row as it is; what
//! WhatsApp sends for its index before the echo is held back (store::chat_actions).

use std::sync::Arc;

use jaw_proto::send::{ChatArchive, ChatMute, ChatPin};
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
use super::outbox::{bad_param, message_id, queue};
use super::{Inner, durably, internal, no_chat, now_ms, query};
use crate::convert::app_state::FOREVER;
use crate::engine::pb;
use crate::store::Change;
use crate::store::chat_actions::{Act, Acted, Does, Outcome, PINNED_MOST, Patch, Setting};

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
    act(inner, chat.id.clone(), doing, |at| {
        let patch = queued(pinned(&chat.id, pin.pinned, at))?;
        Ok(Does::Set(Setting::Pin { at: pin.pinned.then_some(at), patch }))
    })
    .await
}

pub(super) async fn archive(inner: &Arc<Inner>, archive: ChatArchive) -> Result<Value, Error> {
    linked(inner)?;
    let chat = named(inner, &archive.chat).await?;
    let doing = if archive.archived { "archive the chat" } else { "unarchive the chat" };
    act(inner, chat.id.clone(), doing, |at| {
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
    act(inner, chat.id.clone(), doing, |at| {
        Ok(Does::Set(Setting::Mute { until, patch: queued(muted(&chat.id, until, at))? }))
    })
    .await
}

/// Commits a chat action, synced, and queues the patches it wrote after the chat's earlier sends.
/// `does` makes the action done at `at`, later than every chat action jawd has done since it
/// started, so no two of jaw's patches for an index carry one time, which its echo is known by.
async fn act(
    inner: &Arc<Inner>,
    chat: String,
    doing: &str,
    does: impl FnOnce(i64) -> Result<Does, Error>,
) -> Result<Value, Error> {
    let outcome = Outcome::default();
    let mut last = inner.enqueue.lock().await;
    let at = now_ms().max(*last + 1);
    *last = at;
    let act = Act { chat: chat.clone(), does: does(at)?, outcome: outcome.clone() };
    durably(inner, vec![Change::ChatAction(act)], doing).await?;
    match outcome.take() {
        Some(Acted::Queued { chat, outbox, .. }) => {
            for outbox in outbox {
                queue(inner, outbox, chat.clone(), false);
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
}
