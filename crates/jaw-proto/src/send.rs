//! Sending: `send.text`, `send.poll` and `send.retry`, `chat.start`, `draft.set`, `read` and
//! `typing`; and the message actions: `react`, `edit`, `delete`, `forward`, `poll.vote`,
//! `message.info` and `chat.info`.

use serde::{Deserialize, Serialize};

use crate::windows::{ChatRow, MessageRow};

/// `send.text`'s params.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendText {
    pub chat: String,
    pub text: String,
    /// The interface's own name for this message, a random string it makes once for each message it
    /// composes: asking again with the same key, within a day, answers with the same message and
    /// sends nothing new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The ID of the message this one answers, in the same chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// The people mentioned, as `person:` and a number, each written in `text` as `@` and the name
    /// the interface showed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
    /// Whether text with a URL goes with a preview of its page; true when it's left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
}

/// `send.text`'s result, once the message is on disk and on its way: its ID, which WhatsApp will
/// know it by too, and its row as `messages.watch` sends it, pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sent {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<MessageRow>,
}

/// `send.poll`'s params: a poll's question, its options, each named once, and how many of them one
/// vote may choose, one when it's left out; answered as `send.text` is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendPoll {
    pub chat: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `poll.vote`'s params: your vote in poll `message`, the options chosen by name, none to take your
/// vote back; `chat` and `key` as the actions on a message have them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollVote {
    pub message: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `send.retry`'s params: a message of yours that failed, to send again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendRetry {
    pub message: String,
}

/// `chat.start`'s params: a phone number, in international form with or without its `+`, spaces
/// and dashes, which jawd checks with WhatsApp; or a person, as `person:` and a number.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatStart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
}

/// `chat.start`'s result: the chat, as `chats.watch` sends it, found or made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatStarted {
    pub chat: ChatRow,
}

/// `draft.set`'s params; empty text clears the draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSet {
    pub chat: String,
    pub text: String,
}

/// `read`'s params: you've read `chat` up to the message `up_to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Read {
    pub chat: String,
    pub up_to: String,
}

/// `typing`'s params.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Typing {
    pub chat: String,
    pub state: TypingState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingState {
    Composing,
    Recording,
    Paused,
}

/// `react`'s params: one reaction of yours a message, which another emoji replaces and an empty one
/// removes. `chat`, when given, is where `message` is, so jawd finds it at once; `key` is as
/// `send.text`'s, for an interface that can lose an answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct React {
    pub message: String,
    pub emoji: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `edit`'s params: your own text message's new text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub message: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `delete`'s params: a message deleted for you alone, or yours for everyone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delete {
    pub message: String,
    #[serde(rename = "for")]
    pub for_: DeleteFor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteFor {
    Me,
    Everyone,
}

/// `forward`'s params: each of `messages` to each of `to`, in order. `chat`, when given, is where
/// the messages are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forward {
    pub messages: Vec<String>,
    pub to: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// `forward`'s result: the new messages' IDs, chat by chat, each chat's in the order of `messages`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forwarded {
    pub messages: Vec<String>,
}

/// `message.info`'s params.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInfo {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

/// `message.info`'s result: when the message was sent, and for one of yours, how far it has got
/// with each recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub message: String,
    /// When it was sent, in milliseconds since the epoch; null for one of yours still on its way.
    pub sent: Option<i64>,
    pub recipients: Vec<Recipient>,
}

/// One recipient of a message of yours: when it was delivered to them, read and, for what's
/// played, played, each null until it was.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipient {
    pub person: String,
    pub name: String,
    pub delivered: Option<i64>,
    pub read: Option<i64>,
    pub played: Option<i64>,
}

/// `chat.info`'s params.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInfo {
    pub chat: String,
}

/// `chat.info`'s result: the chat's members, for completing `@` names, its community and its
/// disappearing timer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInfoResult {
    pub chat: String,
    pub kind: String,
    pub members: Vec<Member>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community: Option<String>,
    /// The disappearing-message timer in seconds, 0 when it's off.
    #[serde(default)]
    pub ephemeral: u32,
}

/// A chat's member: who they are, the name to show, their number where jawd knows it, and whether
/// they're one of a group's admins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub person: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    pub admin: bool,
}
