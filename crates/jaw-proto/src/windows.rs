//! Windows: a stretch of rows around an anchor, which jawd keeps current. A watch returns its rows,
//! then pushes upserts and removals; rows carry a `sort` that compares bytewise and a version `v`,
//! so applying an update twice, or two out of order, ends the same.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Where a window is anchored: the chat list's `top`, a chat's `newest` or first `unread`
/// message, or a row by its `id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Around {
    Top,
    Newest,
    Unread,
    Row(String),
}

impl std::convert::From<String> for Around {
    fn from(around: String) -> Around {
        match around.as_str() {
            "top" => Around::Top,
            "newest" => Around::Newest,
            "unread" => Around::Unread,
            _ => Around::Row(around),
        }
    }
}

impl std::convert::From<Around> for String {
    fn from(around: Around) -> String {
        match around {
            Around::Top => "top".into(),
            Around::Newest => "newest".into(),
            Around::Unread => "unread".into(),
            Around::Row(id) => id,
        }
    }
}

fn all() -> String {
    "all".into()
}

/// `chats.watch`'s params.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatsWatch {
    #[serde(default = "all")]
    pub folder: String,
    pub around: Around,
    #[serde(default)]
    pub before: u32,
    #[serde(default)]
    pub after: u32,
    /// Rows in `have` unchanged since this seq are left out; hello's `since` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// The rows the client already holds for this window, when it watches again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub have: Vec<String>,
}

/// `messages.watch`'s params.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesWatch {
    pub chat: String,
    pub around: Around,
    #[serde(default)]
    pub before: u32,
    #[serde(default)]
    pub after: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub have: Vec<String>,
}

/// `watch.move`'s params.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatchMove {
    pub watch: String,
    pub around: Around,
    #[serde(default)]
    pub before: u32,
    #[serde(default)]
    pub after: u32,
}

/// `unwatch`'s params, and the `resync` push's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watch {
    pub watch: String,
}

/// What `chats.watch` returns, and `watch.move` on a chat list: the rows the client doesn't
/// hold as they are, and those it holds that have left the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatsWatched {
    pub watch: String,
    /// The feed's seq the rows are as of.
    pub seq: u64,
    pub total: u64,
    pub rows: Vec<ChatRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// What `messages.watch` returns, and `watch.move` on a chat's messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesWatched {
    pub watch: String,
    pub seq: u64,
    pub total: u64,
    /// Whether there are messages before the window.
    pub older: bool,
    /// How many messages come after the window's last row, counted up to 1,000.
    pub newer: u64,
    pub rows: Vec<MessageRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// The `chats` push: what changed in a chat-list window. `total` comes when it changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatsPush {
    pub watch: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<ChatRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// The `messages` push: what changed in a messages window. `total`, `older` and `newer` come
/// when they changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesPush {
    pub watch: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newer: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<MessageRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// A chat, as the chat list shows it. A chat list reads from the largest `sort` down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatRow {
    /// Its JID.
    pub id: String,
    pub v: u64,
    pub sort: String,
    /// `direct`, `group`, `community`, `announcement`, `broadcast` or `self`.
    pub kind: String,
    pub name: String,
    pub avatar: Option<String>,
    pub last: Option<Last>,
    pub unread: u64,
    pub mentions: u64,
    pub pinned: bool,
    pub muted_until: Option<i64>,
    pub archived: bool,
    pub marked_unread: bool,
    pub draft: Option<String>,
    /// Who's typing or recording now, by person `id`.
    #[serde(default)]
    pub typing: Vec<String>,
    /// For a group in a community, the community's `id`.
    pub community: Option<String>,
    /// For a community, its groups' unread total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_unread: Option<u64>,
}

/// A chat's newest message, for its line in the chat list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Last {
    /// Who sent it; absent on your own.
    pub from: Option<String>,
    pub mine: bool,
    pub summary: String,
    pub kind: String,
    /// On your own: `pending`, `sent`, `delivered`, `read`, `played` or `failed`.
    pub status: Option<String>,
    pub at: i64,
}

/// A message, render-ready. A chat's messages read from the smallest `sort` up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub v: u64,
    pub sort: String,
    pub chat: String,
    /// When it was sent, in milliseconds since the epoch.
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<From>,
    pub mine: bool,
    pub kind: String,
    /// The text and caption, parsed into spans.
    pub spans: Value,
    pub reply: Option<Value>,
    /// How many times it was forwarded, 0 if it wasn't.
    pub forwarded: u32,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    pub status: Option<String>,
    pub edited: bool,
    pub deleted: bool,
    pub expires_at: Option<i64>,
    pub view_once: bool,
    pub starred: bool,
    /// What its kind needs to be drawn: `media`, `voice`, `link`, `poll`, `location`, `contact`,
    /// `event` or `system`.
    #[serde(flatten)]
    pub detail: Map<String, Value>,
}

/// Who sent a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct From {
    /// `person:` and a number.
    pub id: String,
    pub name: String,
    /// Which of the interface's sender colours to draw their name in.
    pub colour: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub n: u32,
    pub mine: bool,
}
