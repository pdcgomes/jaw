//! `find`, the palette's one search: chats, people and folders by name from jawd's memory, then
//! messages and links from jaw.db's full-text indexes, best first; and `chat.opened`, which ranks the
//! chats you open often first.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What `find` can answer with. Media and files come in M3; until then they find none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Chat,
    Person,
    Folder,
    Message,
    Link,
    Media,
    File,
}

/// `find`'s params.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Find {
    /// What's typed. Empty finds the chats you open most.
    pub query: String,
    /// The kinds wanted; every kind when it's empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<Kind>,
    /// A chat's id, to find text only in that chat. Names are found anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// The most results of each kind; 10 when absent, and at most 200.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// `find`'s result.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Found {
    /// Names first, best match first; then messages, newest first; then links, newest first.
    pub results: Vec<Hit>,
    /// How many of each kind matched, for each kind that matched any. Text kinds are counted up
    /// to `COUNTED`; a count of that many means at least that many.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counts: BTreeMap<Kind, u32>,
}

/// The most matches of a text kind `find` counts.
pub const COUNTED: u32 = 1_000;

/// One thing `find` found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub kind: Kind,
    /// Ranks results of the same kind: how well the name matched, raised for chats you open often.
    /// Text kinds are ranked by time and have 0. For an empty query, a chat's frecency in
    /// hundredths of an open.
    pub score: u32,
    /// The text that matched, with the match marked: a name, a phone number, or a stretch of a
    /// message around its first match.
    pub text: Vec<Span>,
    /// Which of a chat's or person's names matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Field>,
    /// For a message, its chat's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_name: Option<String>,
    /// The folders a chat or person is in, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// What it found: a chat as `chats.watch` sends it, a message as `messages.watch` does, a
    /// person (`id`, `name`, `phone`) or a folder (`id`, `name`, `parent`).
    pub row: Value,
}

/// A stretch of text: plain, or where the query matched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub t: SpanKind,
    pub s: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Text,
    Mark,
}

/// The name of a chat or person that matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// The chat's name, or the person's as in the phone's contacts.
    Name,
    /// What they call themselves.
    PushName,
    /// A business's verified name.
    VerifiedName,
    Phone,
}

/// `chat.opened`'s params: you opened this chat yourself, from the palette, the chat list or a
/// favourite.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatOpened {
    pub chat: String,
}
