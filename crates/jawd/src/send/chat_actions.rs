//! Chat actions from jaw: `chat.pin`, `chat.archive`, `chat.mute`, `chat.mark_unread` and `star`.
//! Each is done to its row at once, in one synced commit with the app-state patches that tell your
//! other devices, before it answers (store::chat_actions); the patches then go through the outbox
//! after the chat's earlier sends, as a deletion for you does. Each patch is built as whatsmeow's
//! builders build it and carries the time jawd wrote, so WhatsApp's echo of it through app state
//! finds the row as it is.

use prost::Message as _;
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobuf_sync_action::{
    ArchiveChatAction, MarkChatAsReadAction, MuteAction, PinAction, StarAction, SyncActionMessage,
    SyncActionMessageRange, SyncActionValue,
};

use super::actions::Target;
use crate::convert::app_state::FOREVER;
use crate::engine::pb;

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

/// Chat `chat` muted until `until`, for ever from FOREVER on, or unmuted at 0 (BuildMuteAbs).
pub(super) fn muted(chat: &str, until: i64, at: i64) -> pb::AppStatePatch {
    let action = match until {
        0 => MuteAction { muted: Some(false), ..Default::default() },
        until if until >= FOREVER => {
            MuteAction { muted: Some(true), mute_end_timestamp: Some(-1), ..Default::default() }
        }
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
            let action = value(&patch).mute_action.unwrap();
            (patch.name, patch.index, patch.version, action.muted, action.mute_end_timestamp)
        };
        let index = vec!["mute".to_string(), ANA.to_string()];
        assert_eq!(mute(AT + 3_600_000), ("regular_high".into(), index.clone(), 2, Some(true), Some(AT + 3_600_000)));
        assert_eq!(mute(FOREVER), ("regular_high".into(), index.clone(), 2, Some(true), Some(-1)), "for ever is -1");
        assert_eq!(mute(0), ("regular_high".into(), index, 2, Some(false), None), "unmuted, with no end");
    }

    #[test]
    fn an_archive_and_a_read_name_the_chats_newest_message_in_seconds_as_whatsmeow_does() {
        let newest = Newest { id: "g9".into(), at: AT, mine: false, sender: Some(RUI.into()) };
        let archive = archived(GROUP, true, range(GROUP, true, Some(&newest), AT + 5), AT + 5);
        assert_eq!(
            (archive.name.as_str(), archive.index.as_slice(), archive.version),
            ("regular_low", &["archive".to_string(), GROUP.to_string()][..], 3)
        );
        let action = value(&archive).archive_chat_action.unwrap();
        let named = action.message_range.unwrap();
        assert_eq!((action.archived, named.last_message_timestamp), (Some(true), Some(AT / 1000)));
        let key = named.messages[0].key.clone().unwrap();
        assert_eq!(
            (key.remote_jid.as_deref(), key.id.as_deref(), key.from_me, key.participant.as_deref()),
            (Some(GROUP), Some("g9"), Some(false), Some(RUI))
        );
        assert_eq!(named.messages[0].timestamp, Some(AT / 1000));
        let mine = Newest { id: "d1".into(), at: AT, mine: true, sender: None };
        let read = marked(ANA, false, range(ANA, false, Some(&mine), AT), AT);
        assert_eq!((read.name.as_str(), read.index[0].as_str(), read.version), ("regular_low", "markChatAsRead", 3));
        let action = value(&read).mark_chat_as_read_action.unwrap();
        assert_eq!(action.read, Some(true));
        let key = action.message_range.unwrap().messages[0].key.clone().unwrap();
        assert_eq!((key.from_me, key.participant), (Some(true), None), "yours names nobody");
        let unread = marked(ANA, true, range(ANA, false, None, AT), AT);
        assert_eq!(value(&unread).mark_chat_as_read_action.unwrap().read, Some(false));
        let empty = range(ANA, false, None, AT);
        assert_eq!((empty.last_message_timestamp, empty.messages.len()), (Some(AT / 1000), 0), "no message: now");
    }

    #[test]
    fn a_star_names_its_message_as_a_deletion_for_you_does() {
        let index = |target: &Target| starred(target, true, AT).index;
        assert_eq!(index(&target(GROUP, "g1", false, Some(RUI))), ["star", GROUP, "g1", "0", RUI]);
        assert_eq!(index(&target(GROUP, "g2", true, None)), ["star", GROUP, "g2", "1", "0"]);
        assert_eq!(index(&target(ANA, "d1", false, Some(ANA))), ["star", ANA, "d1", "0", "0"]);
        assert_eq!(index(&target(ANA, "d2", true, None)), ["star", ANA, "d2", "1", "0"]);
        let star = starred(&target(ANA, "d1", false, Some(ANA)), false, AT);
        assert_eq!(
            (star.name.as_str(), star.version, value(&star).star_action.unwrap().starred, value(&star).timestamp),
            ("regular_high", 2, Some(false), Some(AT))
        );
    }
}
