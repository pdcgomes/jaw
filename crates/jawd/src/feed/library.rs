//! Library windows: the items of a kind, or the newest of every kind, with a chat, a sender, a
//! stretch of time and words to narrow them, newest first, from the top or around an item. A read
//! is part of the window's one read transaction, so what an interface is sent matches a fresh
//! query, as every window's does.

use std::collections::HashSet;

use anyhow::Result;
use jaw_proto::find::Span;
use jaw_proto::library::{self as proto, Counts, Kind};
use rusqlite::Connection;
use serde_json::Value;

use super::window::from_row;
use crate::find::{Words, snippet};
use crate::store::library::{self as store, Item, Scope, Sender, Shelf, Toward};
use crate::store::read;

/// A stretch of a kind's items: its newest `after`, or `before` items newer than one and `after`
/// older; or, for every kind, the newest `after` of each.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct LibrarySpec {
    pub kind: Kind,
    /// A chat's id.
    pub chat: Option<String>,
    pub person: Whose,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub text: String,
    /// None for the top.
    pub anchor: Option<ItemAnchor>,
    pub before: usize,
    pub after: usize,
}

/// Whose items a window holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Whose {
    Anyone,
    Me,
    /// A person by their n, whose JIDs are read each time, as they change.
    Person(i64),
}

/// The item a window is anchored on, and its place, which the window keeps if the item goes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ItemAnchor {
    pub id: String,
    pub t: i64,
}

/// What a library window holds until jawd keeps links, in words to show.
pub(super) const NO_LINKS: &str = "Links come once jaw keeps them; until then the library has no links.";

/// A library window as one read found it.
pub(super) struct Library {
    pub total: u64,
    pub older: bool,
    pub counts: Counts,
    pub rows: Vec<(i64, proto::LibraryRow)>,
    /// The window's chat, by n, if it's one chat's and the chat exists.
    pub chat: Option<i64>,
    /// The people who sent its rows, and the chats they're in.
    pub senders: HashSet<i64>,
    pub chats: HashSet<i64>,
    pub note: Option<String>,
}

/// Each kind of item jaw keeps, as a window asks for it.
fn shelf(kind: Kind) -> Option<Shelf> {
    match kind {
        Kind::Photo => Some(Shelf::Photo),
        Kind::Video => Some(Shelf::Video),
        Kind::Voice => Some(Shelf::Voice),
        Kind::File => Some(Shelf::File),
        Kind::Link | Kind::All => None,
    }
}

/// Up to `limit` items of `shelf` from just past `from` toward older or newer ones, nearest first,
/// with the window's words if it has any.
fn page(
    db: &Connection,
    shelf: Shelf,
    scope: &Scope,
    words: Option<&str>,
    toward: Toward,
    from: i64,
    limit: usize,
) -> Result<Vec<(i64, i64)>> {
    Ok(match words {
        Some(words) => store::words(db, words, shelf, scope, toward, from, limit)?,
        None => store::keys(db, shelf, scope, toward, from, limit)?,
    })
}

/// Reads the window `spec` describes.
pub(super) fn read(db: &Connection, spec: &LibrarySpec) -> Result<Library> {
    let mut library = Library {
        total: 0,
        older: false,
        counts: Counts { link: None, ..Counts::default() },
        rows: Vec::new(),
        chat: None,
        senders: HashSet::new(),
        chats: HashSet::new(),
        note: matches!(spec.kind, Kind::Link | Kind::All).then(|| NO_LINKS.to_string()),
    };
    let chat = match &spec.chat {
        Some(id) => match read::chat_n(db, id)? {
            Some(n) => Some(n),
            // A chat removed since it was watched, merged into another, has nothing left here.
            None => return Ok(library),
        },
        None => None,
    };
    library.chat = chat;
    let sender = match spec.person {
        Whose::Anyone => Sender::Anyone,
        Whose::Me => Sender::Me,
        Whose::Person(n) => store::person(db, n)?.unwrap_or(Sender::Jids(Vec::new())),
    };
    let (since, until) = Scope::times(spec.since, spec.until);
    let scope = Scope { chat, sender, since, until };
    let words = Words::new(&spec.text);
    let query = (!words.is_empty()).then(|| words.fts());
    let counts = match &query {
        Some(query) => store::word_counts(db, query, &scope)?,
        None => store::counts(db, &scope)?,
    };
    let count = |shelf: Shelf| counts.get(&shelf).copied().unwrap_or(0);
    library.counts = Counts {
        photo: count(Shelf::Photo),
        video: count(Shelf::Video),
        voice: count(Shelf::Voice),
        file: count(Shelf::File),
        link: None,
    };
    let query = query.as_deref();
    let keys = match (spec.kind, shelf(spec.kind)) {
        (Kind::All, _) => {
            library.total = Shelf::ALL.into_iter().map(count).sum();
            let mut keys = Vec::new();
            for shelf in Shelf::ALL {
                keys.extend(page(db, shelf, &scope, query, Toward::Older, i64::MAX, spec.after)?);
            }
            keys
        }
        (_, None) => Vec::new(),
        (_, Some(shelf)) => {
            library.total = count(shelf);
            match &spec.anchor {
                None => {
                    let mut keys = page(db, shelf, &scope, query, Toward::Older, i64::MAX, spec.after + 1)?;
                    library.older = keys.len() > spec.after;
                    keys.truncate(spec.after);
                    keys
                }
                Some(anchor) => {
                    let mut keys = page(db, shelf, &scope, query, Toward::Newer, anchor.t, spec.before)?;
                    keys.reverse();
                    // From the anchor down: the anchor first, if it's still an item here.
                    let mut from =
                        page(db, shelf, &scope, query, Toward::Older, anchor.t.saturating_add(1), spec.after + 2)?;
                    let kept = spec.after + usize::from(from.first().is_some_and(|&(_, t)| t == anchor.t));
                    library.older = from.len() > kept;
                    from.truncate(kept);
                    keys.append(&mut from);
                    keys
                }
            }
        }
    };
    let ns: Vec<i64> = keys.iter().map(|&(n, _)| n).collect();
    let items = store::items(db, &ns)?;
    let names = store::chat_names(db, items.iter().map(|item| item.chat_n))?;
    for item in items {
        library.chats.insert(item.chat_n);
        if let Some(from) = &item.from {
            library.senders.insert(from.id);
        }
        let chat_name = names.get(&item.chat_n).cloned().unwrap_or_else(|| item.chat.clone());
        library.rows.push((item.n, row(item, chat_name, &words)));
    }
    Ok(library)
}

/// An item's row.
fn row(item: Item, chat_name: String, words: &Words) -> proto::LibraryRow {
    let caption = match item.caption.trim() {
        "" => Vec::<Span>::new(),
        caption => snippet(caption, &words.marks(caption)),
    };
    let (mut media, mut voice, mut file) = (None, None, None);
    match item.shelf {
        Shelf::Photo | Shelf::Video => media = Some(item.media),
        Shelf::Voice => voice = Some(proto::Voice { seconds: item.media.get("seconds").and_then(Value::as_f64) }),
        Shelf::File => file = Some(file_of(&item.media, &item.summary)),
    }
    proto::LibraryRow {
        id: item.id.clone(),
        v: item.v.max(0) as u64,
        sort: store::sort(item.t),
        kind: match item.shelf {
            Shelf::Photo => Kind::Photo,
            Shelf::Video => Kind::Video,
            Shelf::Voice => Kind::Voice,
            Shelf::File => Kind::File,
        },
        message: item.id,
        chat: item.chat,
        chat_name,
        from: item.from.map(from_row),
        mine: item.mine,
        at: item.at,
        caption,
        media,
        voice,
        file,
        link: None,
    }
}

/// A file as the library shows it: its name (an audio file, which has none, by its line), what it
/// is, its size and pages, and its path once downloaded.
fn file_of(media: &serde_json::Map<String, Value>, summary: &str) -> proto::File {
    let text = |key: &str| media.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let name = text("name");
    let kind = name.as_deref().and_then(extension).or_else(|| text("mime").as_deref().and_then(mime_type));
    proto::File {
        name: name.unwrap_or_else(|| summary.to_string()),
        kind,
        bytes: media.get("bytes").and_then(Value::as_u64),
        pages: media.get("pages").and_then(Value::as_u64).and_then(|pages| u32::try_from(pages).ok()),
        path: text("path"),
    }
}

/// What a file is, by its name's extension: `PDF`, `GPX`.
fn extension(name: &str) -> Option<String> {
    let (_, extension) = name.rsplit_once('.')?;
    (!extension.is_empty() && extension.len() <= 5 && extension.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| extension.to_ascii_uppercase())
}

/// What a file is, by its MIME type, for one whose name doesn't say.
fn mime_type(mime: &str) -> Option<String> {
    let essence = mime.split(';').next().unwrap_or(mime).trim().to_ascii_lowercase();
    let known = match essence.as_str() {
        "audio/mpeg" => "MP3",
        "audio/mp4" | "audio/x-m4a" => "M4A",
        "audio/ogg" => "OGG",
        "audio/aac" => "AAC",
        "audio/wav" | "audio/x-wav" => "WAV",
        "application/pdf" => "PDF",
        "application/zip" => "ZIP",
        "text/plain" => "TXT",
        _ => {
            let (_, subtype) = essence.split_once('/')?;
            let subtype = subtype.trim_start_matches("x-");
            return (subtype.len() <= 5 && subtype.chars().all(|c| c.is_ascii_alphanumeric()))
                .then(|| subtype.to_ascii_uppercase());
        }
    };
    Some(known.to_string())
}

#[cfg(test)]
mod tests;
