//! Windows: a stretch of rows around an anchor, which jawd keeps current. A watch returns its rows,
//! then pushes upserts and removals; rows carry a `sort` that compares bytewise and a version `v`,
//! so applying an update twice, or two out of order, ends the same. Who's typing in the window's
//! chats, and a one-to-one chat's person's presence, come with each watch's answer and then as
//! `typing` and `presence` pushes, with no `v`.

use std::collections::BTreeMap;

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
    /// The communities whose groups the window brings; every community's when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<Vec<String>>,
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
    /// For a chat list, the communities whose groups it brings now; as it was when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<Vec<String>>,
}

/// `unwatch`'s params, and the `resync` push's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watch {
    pub watch: String,
}

/// What `chats.watch` returns, and `watch.move` on a chat list: the rows the client doesn't
/// hold as they are, and those it holds that have left the window. The rows are jaw-proto's own
/// unless an interface reads them into rows of its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatsWatched<R = ChatRow> {
    pub watch: String,
    /// The feed's seq the rows are as of.
    pub seq: u64,
    pub total: u64,
    pub rows: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    /// Who's typing in the window's chats now, where someone is. It replaces what the interface
    /// held for every chat in the window, so a chat left out has nobody typing.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub typing: Typists,
}

/// What `messages.watch` returns, and `watch.move` on a chat's messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesWatched<R = MessageRow> {
    pub watch: String,
    pub seq: u64,
    pub total: u64,
    /// Whether there are messages before the window.
    pub older: bool,
    /// How many messages come after the window's last row, counted up to 1,000.
    pub newer: u64,
    pub rows: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    /// Who's typing in the window's chat now, if anyone is, as a chat list's answer has it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub typing: Typists,
    /// A one-to-one chat's person's presence now; absent for any other chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence: Option<PersonPresence>,
}

/// The presence of the person of a one-to-one chat, as its conversation shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonPresence {
    /// `person:` and a number.
    pub person: String,
    #[serde(flatten)]
    pub presence: Presence,
}

/// The `presence` push: the presence of the person of a one-to-one chat the connection has open,
/// as it is now that it changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresencePush {
    pub chat: String,
    #[serde(flatten)]
    pub presence: PersonPresence,
}

/// Who's typing in each chat where someone is, by the chat's `id`.
pub type Typists = BTreeMap<String, Vec<Typist>>;

/// Someone typing or recording in a chat.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Typist {
    /// `person:` and a number.
    pub id: String,
    /// Their name as a message from them shows it.
    pub name: String,
    pub state: TypistState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypistState {
    Typing,
    Recording,
}

/// The `typing` push: who's typing in a chat now, oldest first; empty once nobody is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingPush {
    pub chat: String,
    pub who: Vec<Typist>,
}

/// The `chats` push: what changed in a chat-list window. `total` comes when it changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatsPush<R = ChatRow> {
    pub watch: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// The `messages` push: what changed in a messages window. `total`, `older` and `newer` come
/// when they changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessagesPush<R = MessageRow> {
    pub watch: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newer: Option<u64>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<R>,
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
    /// For a group in a community, the community's `id`.
    pub community: Option<String>,
    /// Its disappearing-message timer in seconds, 0 when it's off.
    #[serde(default)]
    pub ephemeral: u32,
    /// For a community, the unread messages of the groups under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_unread: Option<u64>,
    /// For a community, how many of the unread messages of the groups under it mention you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_mentions: Option<u64>,
    /// The IDs of your folders it sits in directly, or its person does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// Its favourite slot, 1 to 9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourite: Option<u8>,
}

/// `folders.watch`'s params: the whole tree, so only what watching again needs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FoldersWatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub have: Vec<String>,
}

/// A folder in the tree: built in (`all`, `unread`, `groups`, `communities`, `archived`), smart
/// (`smart:` and a number) or yours (`folder:` and a number).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderRow {
    pub id: String,
    #[serde(default)]
    pub v: u64,
    /// Null at the top.
    pub parent: Option<String>,
    pub name: String,
    /// Its place among its siblings of its kind.
    pub position: u32,
    /// A smart folder's rule; null for the rest.
    pub smart: Option<Value>,
    /// Its chats' and its subfolders' chats' unread messages and mentions, each chat counted once.
    pub unread: u64,
    pub mentions: u64,
    /// The chats and people in it and under it, each once, a person with a chat of their own as
    /// that chat. A smart folder's are counted up to 1,000.
    pub count: u64,
    /// Its favourite slot, 1 to 9.
    pub favourite: Option<u8>,
}

/// A favourite slot and what Alt and its number go to: a folder, or a chat with its name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favourite {
    pub slot: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// What `folders.watch` returns: every folder, and the favourite slots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoldersWatched<R = FolderRow> {
    pub watch: String,
    pub seq: u64,
    pub rows: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default)]
    pub favourites: Vec<Favourite>,
}

/// The `folders` push: the folders that changed, and the favourite slots whole when they did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoldersPush<R = FolderRow> {
    pub watch: String,
    pub seq: u64,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favourites: Option<Vec<Favourite>>,
}

fn yes() -> bool {
    true
}

/// `roster.watch`'s params.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RosterWatch {
    #[serde(default = "all")]
    pub folder: String,
    pub around: Around,
    #[serde(default)]
    pub before: u32,
    #[serde(default)]
    pub after: u32,
    /// False leaves out people who aren't online.
    #[serde(default = "yes")]
    pub offline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub have: Vec<String>,
}

/// Someone in a roster. A roster reads from the smallest `sort` up: around first, then by when
/// last seen, then by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RosterRow {
    /// `person:` and a number.
    pub id: String,
    pub v: u64,
    pub sort: String,
    pub name: String,
    pub phone: Option<String>,
    pub about: Option<String>,
    pub avatar: Option<String>,
    pub presence: Presence,
    /// Their one-to-one chat's `id`, if there is one.
    pub chat: Option<String>,
    /// The IDs of your folders they're filed in.
    #[serde(default)]
    pub folders: Vec<String>,
}

/// Someone's presence: `online`, `seen` (with `at`) or `hidden`, and `known` false when jawd isn't
/// following them now, so it's what was last known. Typing and recording show from the chat's
/// typing, which jaw.db never holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    pub known: bool,
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
