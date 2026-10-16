//! Sending: `send.text` and `send.retry`, `chat.start`, `draft.set`, `read` and `typing`.

use serde::{Deserialize, Serialize};

use crate::windows::{ChatRow, MessageRow};

/// `send.text`'s params. Replies and mentions come with M2, and link previews with them; until
/// then a reply or mentions are refused with -32602, and `preview` changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendText {
    pub chat: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<String>,
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
