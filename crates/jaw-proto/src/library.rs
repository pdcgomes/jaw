//! The library: every photo, video, voice message, file and link shared, across all chats or in
//! one, as a window jawd keeps (`library.watch`), newest first and paged by keyset. Its rows are
//! items: a message's photo, video, voice message or file, or one of its links, so a message can
//! give several.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::find::Span;
use crate::windows::{Around, From};

/// A kind of item, or every kind at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Photo,
    /// Videos, GIFs and video messages.
    Video,
    Voice,
    /// Documents and audio files.
    File,
    Link,
    /// The newest items of each kind, unpaged, for a shelf of each.
    All,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Photo => "photo",
            Kind::Video => "video",
            Kind::Voice => "voice",
            Kind::File => "file",
            Kind::Link => "link",
            Kind::All => "all",
        }
    }
}

/// `library.watch`'s params. Every filter can be absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryWatch {
    pub kind: Kind,
    /// One chat's items, by its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// One sender's items: `person:` and a number, or `me` for yours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Items sent at or after this time, in milliseconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    /// Items sent at or before this time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    /// Words each item must have, matched as `find` matches messages.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// `top`, or an item's `id`.
    pub around: Around,
    /// Items newer than the anchor.
    #[serde(default)]
    pub before: u32,
    /// Items older than the anchor; for `all`, the newest of each kind.
    #[serde(default)]
    pub after: u32,
}

/// How many items of each kind match a window's filters but its kind: exact with no filter but a
/// chat, and counted up to `COUNTED` otherwise, where that many means at least that many.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub photo: u64,
    pub video: u64,
    pub voice: u64,
    pub file: u64,
    /// Absent while jawd doesn't keep links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<u64>,
}

/// The most items of a kind counted when a window has a filter other than its chat.
pub const COUNTED: u64 = 1_000;

/// What `library.watch` returns, and `watch.move` on a library window: the rows the client doesn't
/// hold as they are, and those it holds that have left the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryWatched<R = LibraryRow> {
    pub watch: String,
    /// The feed's seq the rows are as of.
    pub seq: u64,
    /// How many items of the window's kind match, as `counts` counts them.
    pub total: u64,
    /// Whether more items come after the window's last row.
    pub older: bool,
    pub counts: Counts,
    pub rows: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    /// What the window can't hold yet and why, in words to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The `library` push: what changed in a library window. `total`, `older` and `counts` come when
/// they changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryPush<R = LibraryRow> {
    pub watch: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub older: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<Counts>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<R>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
}

/// An item, as the library shows it. A library window reads from the largest `sort` down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryRow {
    /// Its message's ID, with `~` and its place among the message's links for a link.
    pub id: String,
    /// Its message's version.
    pub v: u64,
    /// Its message's time, then its place in that millisecond, and a link's place.
    pub sort: String,
    pub kind: Kind,
    /// Its message's ID, to go to.
    pub message: String,
    pub chat: String,
    pub chat_name: String,
    /// Who sent it; absent on yours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<From>,
    pub mine: bool,
    pub at: i64,
    /// Its message's caption, on one line, with the words the window's text matched marked.
    pub caption: Vec<Span>,
    /// A photo's or a video's, as message rows have it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<Voice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<File>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
}

/// A voice message: how long it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
}

/// A file: its name, what it is as shown (`PDF`, `GPX`), and its file once downloaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A link: where it goes, and its preview's words where it had one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub url: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
