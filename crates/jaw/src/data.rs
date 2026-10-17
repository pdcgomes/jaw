//! What the interface knows of jawd: the rows it draws, the updates that bring them, and what it
//! asks for. The rows are shaped after protocol/README.md's and read from its JSON, so the client
//! crate can hand them over as they come; what the interface asks for goes through `Jawd`, which
//! never waits, since answers arrive as updates on the main thread's channel. `crate::demo` is a
//! made-up account behind both.

pub use jaw_client::Place;
use jaw_client::{Row, Update as Window};
use jaw_proto::{Connection, LinkStep};
use serde::Deserialize;
use std::collections::HashMap;

/// A chat-list row: the protocol's chat row, as much of it as the interface draws.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Chat {
    /// Its JID.
    pub id: String,
    pub v: u64,
    /// Pinned chats first, then newest: rows show in descending bytewise order of `sort`.
    pub sort: String,
    #[serde(default)]
    pub kind: ChatKind,
    pub name: String,
    #[serde(default)]
    pub last: Option<Last>,
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub mentions: u32,
    #[serde(default)]
    pub pinned: bool,
    /// Muted until then, in milliseconds since the epoch; `i64::MAX` is for ever.
    #[serde(default)]
    pub muted_until: Option<i64>,
    #[serde(default)]
    pub archived: bool,
    /// Marked unread by hand, as on the phone.
    #[serde(default)]
    pub marked_unread: bool,
    #[serde(default)]
    pub draft: Option<String>,
    /// The names of who's typing or recording now.
    #[serde(default)]
    pub typing: Vec<String>,
    /// For a group in a community: the community.
    #[serde(default)]
    pub community: Option<String>,
    /// The IDs of your folders it sits in directly.
    #[serde(default)]
    pub folders: Vec<String>,
}

/// A folder in the tree, as `folders.watch` sends it: built in, smart or yours.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct FolderRow {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    pub name: String,
    #[serde(default)]
    pub position: i64,
    /// A smart folder's rule; null for the others.
    #[serde(default)]
    pub smart: Option<serde_json::Value>,
    /// Its chats' unread total and mentions, each chat counted once.
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub mentions: u32,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub favourite: Option<u8>,
}

impl FolderRow {
    /// One of the five always there: All, Unread, Groups, Communities, Archived.
    pub fn built_in(&self) -> bool {
        matches!(self.id.as_str(), "all" | "unread" | "groups" | "communities" | "archived")
    }

    /// One of yours, which you can rename, move and file into.
    pub fn yours(&self) -> bool {
        !self.built_in() && self.smart.is_none()
    }
}

/// Someone in the roster, as `roster.watch` sends them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct RosterRow {
    pub id: String,
    #[serde(default)]
    pub sort: String,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub presence: Presence,
    #[serde(default)]
    pub chat: Option<String>,
    #[serde(default)]
    pub folders: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    Typing,
    Recording,
    Seen,
    #[default]
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Presence {
    pub state: PresenceState,
    #[serde(default)]
    pub at: Option<i64>,
    /// Whether jawd follows this person now; when not, what's shown is what was last known.
    #[serde(default = "yes")]
    pub known: bool,
}

impl Default for Presence {
    fn default() -> Presence {
        Presence { state: PresenceState::Hidden, at: None, known: true }
    }
}

fn yes() -> bool {
    true
}

impl Presence {
    /// Online, typing or recording.
    pub fn around(&self) -> bool {
        matches!(self.state, PresenceState::Online | PresenceState::Typing | PresenceState::Recording)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    #[default]
    Direct,
    Group,
    Community,
    Announcement,
    Broadcast,
    #[serde(rename = "self")]
    Myself,
    #[serde(other)]
    Other,
}

impl Quote {
    /// Who the quote is from, as the screen names them: a JID as its number until jawd names it.
    pub fn who(&self) -> String {
        match self.from.as_deref() {
            Some(jid) if jid.contains('@') => match jid.split_once('@') {
                Some((user, "s.whatsapp.net")) => format!("+{user}"),
                _ => "someone".into(),
            },
            Some(name) if !name.is_empty() => name.to_string(),
            _ => "someone".into(),
        }
    }
}

impl Chat {
    pub fn group(&self) -> bool {
        matches!(self.kind, ChatKind::Group | ChatKind::Community | ChatKind::Announcement)
    }

    pub fn muted(&self, now: i64) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }
}

/// A chat's newest message, as its row in the list shows it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Last {
    /// Who sent it; nobody for your own. The list names them only in a group.
    #[serde(default)]
    pub from: Option<String>,
    pub summary: String,
    /// Its ticks, if it's yours.
    #[serde(default)]
    #[serde(deserialize_with = "tick")]
    pub status: Option<Tick>,
    pub at: i64,
}

/// A message row, render-ready: jawd has resolved names, parsed formatting into spans and totalled
/// reactions, so the interface only lays it out.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Message {
    pub id: String,
    pub v: u64,
    /// Its chat's JID.
    #[serde(default)]
    pub chat: String,
    /// Oldest first: rows show in ascending bytewise order of `sort`.
    pub sort: String,
    pub at: i64,
    /// The sender; absent on your own.
    #[serde(default)]
    pub from: Option<Sender>,
    #[serde(default)]
    pub mine: bool,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub spans: Vec<Span>,
    #[serde(default)]
    pub reply: Option<Quote>,
    /// How many times it had been forwarded before it came, 0 if it wasn't.
    #[serde(default)]
    pub forwarded: u32,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default, deserialize_with = "tick")]
    pub status: Option<Tick>,
    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub deleted: bool,
    /// One line for a kind the interface doesn't draw yet, such as "▣ photo".
    #[serde(default)]
    pub summary: Option<String>,
    /// For `system` and `call`: the sentence to show.
    #[serde(default)]
    pub system: Option<String>,
    /// For photos, stickers, GIFs, videos and video notes: what jawd knows of the file.
    #[serde(default)]
    pub media: Option<Box<Media>>,
    /// For an album: its photos and videos.
    #[serde(default)]
    pub album: Option<Album>,
    /// For a voice message: its length and waveform, and whether you've played it.
    #[serde(default)]
    pub voice: Option<Voice>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Voice {
    #[serde(default)]
    pub seconds: f64,
    /// 64 values from 0 to 100, as WhatsApp sends them.
    #[serde(default)]
    pub waveform: Vec<u8>,
    /// On someone else's: whether you've played it, which the played receipt said once.
    #[serde(default)]
    pub played: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    Playing,
    Paused,
    /// It played to its end.
    Ended,
    /// Another message or a recording took over.
    Stopped,
    Failed,
}

/// jawd's `playback` push: what's playing or paused, and where its playhead is.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Playback {
    /// The voice message, or none for the recording being listened to.
    #[serde(default)]
    pub message: Option<String>,
    pub state: PlayState,
    #[serde(default)]
    pub at: f64,
    #[serde(default)]
    pub seconds: f64,
    #[serde(default = "one")]
    pub speed: f64,
    /// Why it failed, in words to show.
    #[serde(default)]
    pub error: Option<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    Recording,
    /// The microphone has given only silence for a second, as when jawd isn't allowed it.
    Silent,
    Stopped,
    Cancelled,
    Failed,
}

/// jawd's `recording` push.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Recorded {
    pub state: RecordState,
    #[serde(default)]
    pub seconds: f64,
    /// While recording: how loud, 0 to 100, ten times a second.
    #[serde(default)]
    pub level: Option<u8>,
    /// Once stopped: its 64-point waveform.
    #[serde(default)]
    pub waveform: Vec<u8>,
    /// Once stopped: `stopped`, `limit` or `interrupted`, with `reason` for the last.
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// A message's file as an interface draws it: its size in pixels, how long it lasts, and the
/// previews jawd has written of it, as paths under jawd's folder.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Media {
    /// The file, once downloaded.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub previews: Previews,
    /// The small picture the message brought with it, there from the start.
    #[serde(default)]
    pub thumbnail: Option<String>,
}

/// Display-ready previews, each there once jawd has written it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Previews {
    #[serde(rename = "256", default)]
    pub small: Option<String>,
    #[serde(rename = "768", default)]
    pub large: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct Album {
    #[serde(default)]
    pub items: Vec<AlbumItem>,
}

/// One of an album's photos or videos.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AlbumItem {
    pub id: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub media: Media,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Sender {
    #[serde(deserialize_with = "id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub colour: u8,
}

/// A status jawd may have more of than jaw knows: those it doesn't are no ticks, rather than a
/// row it can't read.
fn tick<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Tick>, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(d)?.and_then(|v| serde_json::from_value(v).ok()))
}

/// An optional ID that's a string, or a number.
fn some_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s),
        Some(other) => Some(other.to_string()),
    })
}

/// An ID that's a string, or a number where a fixture writes one.
fn id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => Ok(s),
        other => Ok(other.to_string()),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Text,
    Image,
    Video,
    Gif,
    VideoNote,
    Voice,
    Audio,
    Document,
    Sticker,
    Location,
    LiveLocation,
    Contact,
    Poll,
    Event,
    Album,
    System,
    Call,
    #[serde(other)]
    Unknown,
}

/// A run of text with one kind of formatting.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Span {
    pub t: SpanKind,
    pub s: String,
    /// For a mention: the person mentioned.
    #[serde(default, deserialize_with = "some_id")]
    pub person: Option<String>,
    /// For a link: where it goes, which can differ from the text shown.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Text,
    Bold,
    Italic,
    Strike,
    Mono,
    Quote,
    List,
    Link,
    Mention,
    #[serde(other)]
    Other,
}

/// The message a reply quotes, as one line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Quote {
    /// The message quoted.
    #[serde(default)]
    pub id: Option<String>,
    /// Who wrote it: a name, or jawd's JID for them until it names them.
    #[serde(default, deserialize_with = "some_id")]
    pub from: Option<String>,
    #[serde(default)]
    pub colour: u8,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub n: u32,
    #[serde(default)]
    pub mine: bool,
}

/// How far one of yours has got; in groups, the least far any recipient has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tick {
    Pending,
    Sent,
    Delivered,
    Read,
    Played,
    Failed,
}

/// What `find` finds: names first, then what was said and shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundKind {
    Chat,
    Person,
    Folder,
    Message,
    Link,
    Media,
    File,
}

/// A run of what matched: `mark` where the query did.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Mark {
    pub t: MarkKind,
    pub s: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Text,
    Mark,
    #[serde(other)]
    Other,
}

/// Someone with no chat yet, as `find` returns them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
}

/// What the library holds: each kind it has a strip entry for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Photo,
    Video,
    Voice,
    File,
    Link,
}

impl LibraryKind {
    pub const ALL: [LibraryKind; 5] =
        [LibraryKind::Photo, LibraryKind::Video, LibraryKind::Voice, LibraryKind::File, LibraryKind::Link];

    /// Its name on the kind strip.
    pub fn plural(self) -> &'static str {
        match self {
            LibraryKind::Photo => "photos",
            LibraryKind::Video => "videos",
            LibraryKind::Voice => "voice",
            LibraryKind::File => "files",
            LibraryKind::Link => "links",
        }
    }
}

/// A file in the library: its name and what it is, as shown.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct FileInfo {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub pages: Option<u32>,
    #[serde(default)]
    pub path: Option<String>,
}

/// A link in the library: where it goes, and its preview's words.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct LinkInfo {
    pub url: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// One thing shared: a message's photo, video, voice message or file, one of an album's
/// photos, or one of a message's links.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LibraryRow {
    pub id: String,
    #[serde(default)]
    pub sort: String,
    pub kind: LibraryKind,
    pub message: String,
    pub chat: String,
    #[serde(default)]
    pub chat_name: String,
    #[serde(default)]
    pub from: Option<Sender>,
    #[serde(default)]
    pub mine: bool,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub media: Option<Box<Media>>,
    #[serde(default)]
    pub voice: Option<Voice>,
    #[serde(default)]
    pub file: Option<FileInfo>,
    #[serde(default)]
    pub link: Option<LinkInfo>,
    #[serde(default)]
    pub caption: Vec<Mark>,
}

/// A library window's answer or push: rows changed or gone, or with `whole` every row, and the
/// counts that came with them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryWindow {
    pub upsert: Vec<LibraryRow>,
    pub remove: Vec<String>,
    pub whole: bool,
    pub total: Option<u32>,
    pub older: Option<bool>,
    pub counts: Option<HashMap<LibraryKind, u32>>,
}

/// What the library asks jawd for: a kind or all of them, the filters, and where its window is.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LibraryAsk {
    /// The interface's number for the ask, so only the latest ask's answer is kept.
    #[serde(skip)]
    pub id: u64,
    pub kind: Option<LibraryKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// `top`, or an item's `id`.
    pub around: String,
    pub before: u32,
    pub after: u32,
}

/// One of a chat's members, as `chat.info` gives them, for completing `@` names.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Member {
    #[serde(rename = "person")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub admin: bool,
}

/// `message.info`'s answer: when it was sent, and how far it has got with each recipient.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MessageInfo {
    pub message: String,
    #[serde(default)]
    pub sent: Option<i64>,
    #[serde(default)]
    pub recipients: Vec<Recipient>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Recipient {
    pub person: String,
    pub name: String,
    #[serde(default)]
    pub delivered: Option<i64>,
    #[serde(default)]
    pub read: Option<i64>,
    #[serde(default)]
    pub played: Option<i64>,
}

/// A folder, as `find` returns it; built-in folders have their fixed IDs.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

/// What a result found, read from its `row` by its `kind`.
#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Chat(Chat),
    Person(Person),
    Folder(Folder),
    Message(Message),
    /// A link, a photo or a file: nothing finds them before M2 and M3.
    Shared,
}

/// One of `find`'s results.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawFound")]
pub struct Found {
    pub kind: FoundKind,
    /// Ranks results of a kind; messages have 0 and come newest first.
    pub score: i64,
    /// What matched, marked: the name, or a stretch of the message around its first match.
    pub text: Vec<Mark>,
    /// For a message, its chat's name.
    pub chat_name: Option<String>,
    /// The folders a chat or person is in.
    pub folders: Vec<String>,
    pub hit: Hit,
}

#[derive(Deserialize)]
struct RawFound {
    kind: FoundKind,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    text: Vec<Mark>,
    #[serde(default)]
    chat_name: Option<String>,
    #[serde(default)]
    folders: Vec<String>,
    row: serde_json::Value,
}

impl TryFrom<RawFound> for Found {
    type Error = serde_json::Error;

    fn try_from(r: RawFound) -> Result<Found, serde_json::Error> {
        let hit = match r.kind {
            FoundKind::Chat => Hit::Chat(serde_json::from_value(r.row)?),
            FoundKind::Person => Hit::Person(serde_json::from_value(r.row)?),
            FoundKind::Folder => Hit::Folder(serde_json::from_value(r.row)?),
            FoundKind::Message => Hit::Message(serde_json::from_value(r.row)?),
            FoundKind::Link | FoundKind::Media | FoundKind::File => Hit::Shared,
        };
        Ok(Found { kind: r.kind, score: r.score, text: r.text, chat_name: r.chat_name, folders: r.folders, hit })
    }
}

/// How many of each kind matched; for messages, 1,000 means at least that many.
pub type Counts = std::collections::BTreeMap<FoundKind, u32>;

/// What arrives on the main thread's channel from jawd, or from the made-up account.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    /// jawd answered hello: whether the account is linked, and the connection.
    Hello {
        /// jawd's folder, which every path it sends is under.
        root: String,
        linked: bool,
        /// Every identity of yours: your person ID, and the JID your number makes.
        me: Vec<String>,
        connection: Connection,
    },
    Connection(Connection),
    Link(LinkStep),
    /// The chat list's window: a watch's or a move's rows, or a push, as jaw-client's models take
    /// them.
    Chats(Window<Chat>),
    /// The open chat's window, for `chat`; updates for any other chat are from a window since
    /// closed.
    Messages {
        chat: String,
        update: Window<Message>,
    },
    /// A message whose `send.text` lost its answer to a dropped connection: jawd may have it, and
    /// is asked again with its key once it's back, which answers with the message if it has it.
    Unsure {
        chat: String,
        key: String,
    },
    /// jawd has the message composed under `key`, as `message`, and it's on its way.
    Sent {
        key: String,
        message: String,
    },
    /// A message jawd didn't take, given back to the box with its key, and why.
    Unsent {
        chat: String,
        key: String,
        text: String,
        why: String,
    },
    /// jawd couldn't start the chat asked for, and why.
    NotStarted(String),
    /// The answer to the `find` the interface numbered `id`.
    Found {
        id: u64,
        results: Vec<Found>,
        counts: Counts,
    },
    /// A `find` that failed, or was cancelled by a newer one.
    FindFailed {
        id: u64,
        cancelled: bool,
        why: String,
    },
    /// The chat a person now has, started from the palette.
    ChatStarted {
        chat: Box<Chat>,
    },
    /// Something to say once, in the footer.
    Notice(String),
    /// jaw is starting jawd; `failed` says why it couldn't.
    Starting {
        failed: Option<String>,
    },
    /// jawd doesn't answer; jaw tries again in `retry_in` milliseconds.
    Unreachable {
        why: String,
        retry_in: i64,
    },
    Playback(Playback),
    Recorded(Recorded),
    /// jawd turned down something asked of voice, and why.
    VoiceRefused(String),
    /// The folder tree: rows changed or gone, or with `whole` every row.
    Folders {
        upsert: Vec<FolderRow>,
        remove: Vec<String>,
        whole: bool,
    },
    /// The roster for `folder`: rows changed or gone, or with `whole` every row.
    Roster {
        folder: String,
        upsert: Vec<RosterRow>,
        remove: Vec<String>,
        whole: bool,
    },
    /// A folder just made, to fill or pick.
    FolderMade(Box<FolderRow>),
    /// The library's window, for the ask numbered `asked`: rows changed or gone, or with `whole`
    /// every row.
    Library {
        asked: u64,
        window: LibraryWindow,
    },
    /// jawd can't serve the library yet, and why.
    LibraryRefused(String),
    /// jawd can't show this folder's chats yet; the list is back on All.
    FolderRefused {
        folder: String,
        why: String,
    },
    /// A chat's members, for completing `@` names.
    Members {
        chat: String,
        members: Vec<Member>,
    },
    /// `message.info`'s answer for the sheet, or why there's none.
    Info {
        message: String,
        info: Result<MessageInfo, String>,
    },
}

impl Update {
    /// Takes `next` into this update when the two apply as one, as a burst of upserts to the same
    /// window does; gives it back when they don't. jaw-client merges a frame's updates itself; this
    /// does the same for the made-up account, which sends each as it happens.
    pub fn absorb(&mut self, next: Update) -> Option<Update> {
        fn merge<R: Row>(into: &mut Window<R>, next: Window<R>) -> Option<Window<R>> {
            if next.moved.is_some() || next.remove.iter().any(|id| into.upsert.iter().any(|r| r.id() == id)) {
                return Some(next);
            }
            into.seq = into.seq.max(next.seq);
            into.remove.extend(next.remove);
            into.upsert.extend(next.upsert);
            into.total = next.total.or(into.total);
            into.older = next.older.or(into.older);
            into.newer = next.newer.or(into.newer);
            None
        }
        match (self, next) {
            (Update::Chats(into), Update::Chats(next)) => merge(into, next).map(Update::Chats),
            (Update::Messages { chat, update }, Update::Messages { chat: other, update: next }) if *chat == other => {
                merge(update, next).map(|update| Update::Messages { chat: other, update })
            }
            (_, next) => Some(next),
        }
    }
}

impl Row for Chat {
    fn id(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
    fn sort(&self) -> &str {
        &self.sort
    }
}

impl Row for Message {
    fn id(&self) -> &str {
        &self.id
    }
    fn v(&self) -> u64 {
        self.v
    }
    fn sort(&self) -> &str {
        &self.sort
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    Composing,
    Paused,
}

/// What the interface asks of jawd. Nothing here waits: what comes of it arrives as `Update`s.
pub trait Jawd {
    /// The chat list on All, or the same window moved.
    /// A folder's chat list, or the same window moved.
    fn watch_chats(&mut self, folder: &str, place: Place);
    /// The library's window for what's asked, which replaces any earlier one; `Update::Library`
    /// keeps it current.
    fn watch_library(&mut self, ask: &LibraryAsk);
    fn unwatch_library(&mut self);
    /// The folder tree, kept current with `Update::Folders`.
    fn watch_folders(&mut self);
    /// A folder's people, kept current with `Update::Roster`.
    fn watch_roster(&mut self, folder: &str);
    fn folder_create(&mut self, name: &str, parent: Option<&str>);
    fn folder_rename(&mut self, folder: &str, name: &str);
    /// Nests a folder in `parent`, or puts it at the top with `None`.
    fn folder_move(&mut self, folder: &str, parent: Option<&str>);
    fn folder_delete(&mut self, folder: &str);
    /// Files a chat or a person into a folder, or takes it out.
    fn folder_add(&mut self, folder: &str, item: &str);
    fn folder_remove(&mut self, folder: &str, item: &str);
    /// The open chat's messages: a new window, or the same one moved.
    fn watch_messages(&mut self, chat: &str, place: Place);
    /// You opened a chat yourself, which the palette ranks by; passing through it doesn't count.
    fn opened(&mut self, chat: &str);
    /// Sends text to a chat, as a reply to `reply` and mentioning the people in `mentions`. `key`
    /// is the interface's name for the message, made once when it was composed: asked again with
    /// it, jawd answers with the message it has rather than sending another.
    fn send_text(&mut self, chat: &str, text: &str, key: &str, reply: Option<&str>, mentions: &[String]);
    /// Reacts to a message with `emoji`, or takes yours away with an empty one.
    fn react(&mut self, message: &str, emoji: &str);
    fn edit(&mut self, message: &str, text: &str);
    /// Deletes a message for you, or for everyone.
    fn delete(&mut self, message: &str, everyone: bool);
    fn forward(&mut self, messages: &[String], to: &[String]);
    /// Asks for a message's receipts; they come as `Update::Info`.
    fn message_info(&mut self, message: &str);
    /// Asks for a chat's members; they come as `Update::Members`.
    fn chat_info(&mut self, chat: &str);
    fn draft(&mut self, chat: &str, text: &str);
    /// Everything in the chat up to and including `up_to` has been seen.
    fn read(&mut self, chat: &str, up_to: &str);
    fn typing(&mut self, chat: &str, state: Typing);
    /// Whether the terminal is in front, so jawd says you're online only while it is.
    fn focus(&mut self, focused: bool);
    /// Starts linking by QR code, or by a code typed on the phone for `phone`.
    fn link_start(&mut self, phone: Option<String>);
    /// The palette's search, numbered so only the answer to the latest is kept: one kind or all
    /// of them, messages in one chat or everywhere, at most `limit` of each kind.
    fn find(&mut self, id: u64, query: &str, kind: Option<FoundKind>, chat: Option<&str>, limit: usize);
    /// A chat with someone who has none yet; it comes back as `Update::ChatStarted`.
    fn start_chat(&mut self, person: &str);
    /// A chat with a phone number, which jawd checks with WhatsApp first.
    fn start_chat_by_phone(&mut self, phone: &str);
    /// Sends a failed message of yours again.
    fn retry(&mut self, message: &str);
    /// Mutes a chat until then, `i64::MAX` for ever, or unmutes it with `None`.
    fn mute(&mut self, chat: &str, until: Option<i64>);
    fn archive(&mut self, chat: &str, archived: bool);
    fn pin(&mut self, chat: &str, pinned: bool);
    fn mark_unread(&mut self, chat: &str, unread: bool);
    /// Plays a voice message, or with `None` the stopped recording, from `at` seconds or, without
    /// it, where it paused; `playback` pushes say how it goes.
    fn voice_play(&mut self, message: Option<&str>, at: Option<f64>, speed: f64);
    fn voice_pause(&mut self);
    fn voice_seek(&mut self, at: f64);
    fn voice_speed(&mut self, speed: f64);
    /// Records a voice message for `chat`; `recording` pushes say how it goes.
    fn voice_record(&mut self, chat: &str);
    /// Stops recording and keeps it, to listen to or send.
    fn voice_stop(&mut self);
    /// Sends the recording, stopping it first if it's still going.
    fn voice_send(&mut self);
    fn voice_cancel(&mut self);
    /// What came from jawd since the last frame, for a jawd that's woken the loop for it; the
    /// made-up account sends its updates on the channel itself.
    fn drain(&mut self) -> Vec<Update> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_answer_reads_into_results_by_kind() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/fixtures/find.result.json")).unwrap();
        let results: Vec<Found> = serde_json::from_value(v["result"]["results"].clone()).unwrap();
        let kinds: Vec<FoundKind> = results.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [FoundKind::Chat, FoundKind::Person, FoundKind::Folder, FoundKind::Message]);
        assert!(matches!(&results[0].hit, Hit::Chat(c) if c.name == "Ana Ribeiro"));
        assert_eq!(results[0].text[0], Mark { t: MarkKind::Mark, s: "A".into() });
        assert_eq!(results[0].folders, ["Family"]);
        assert!(matches!(&results[1].hit, Hit::Person(p) if p.phone.as_deref() == Some("+351913000031")));
        assert!(matches!(&results[2].hit, Hit::Folder(f) if f.id == "archived"));
        let Hit::Message(m) = &results[3].hit else { panic!("{:?}", results[3]) };
        assert_eq!((m.chat.as_str(), m.from.as_ref().unwrap().id.as_str()), ("120363000000000001@g.us", "12"));
        assert_eq!(results[3].chat_name.as_deref(), Some("Lisbon Climbing"));
        let counts: Counts = serde_json::from_value(v["result"]["counts"].clone()).unwrap();
        assert_eq!(counts[&FoundKind::Message], 214);
    }

    /// The rows in protocol/README.md, read as the interface reads them.
    #[test]
    fn the_protocols_rows_read_into_the_interfaces_and_bursts_of_them_apply_as_one() {
        let chat: Chat = serde_json::from_str(r#"{"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000","kind":"direct","name":"Ana Ribeiro","avatar":"media/avatars/5f2c.jpg","last":{"from":"Ana","summary":"did you get the photos from Sunday?","kind":"text","status":null,"at":1792145060000},"unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"draft":null,"typing":[],"community":null}"#).unwrap();
        assert_eq!(
            (chat.name.as_str(), chat.unread, chat.pinned, chat.kind),
            ("Ana Ribeiro", 2, true, ChatKind::Direct)
        );
        assert_eq!(chat.last.unwrap().summary, "did you get the photos from Sunday?");

        let message: Message = serde_json::from_str(r#"{"id":"3EB0A1B2C3D4E5F6","v":3,"sort":"1792145060000~000123","chat":"351912345678@s.whatsapp.net","at":1792145060000,"from":{"id":"person:12","name":"Ana","colour":3},"mine":false,"kind":"text","spans":[{"t":"text","s":"did you get the "},{"t":"bold","s":"photos"},{"t":"text","s":" from Sunday?"}],"reply":null,"reactions":[{"emoji":"❤","n":2,"mine":true}],"status":null,"edited":false,"deleted":false,"expires_at":null}"#).unwrap();
        assert_eq!(message.from.as_ref().map(|f| f.colour), Some(3));
        assert_eq!(message.spans[1], Span { t: SpanKind::Bold, s: "photos".into(), person: None, url: None });
        assert_eq!(message.reactions[0], Reaction { emoji: "❤".into(), n: 2, mine: true });

        let window = |upsert: Vec<Message>, remove: Vec<String>, older: Option<bool>| Update::Messages {
            chat: "ana".into(),
            update: Window { upsert, remove, older, ..Window::default() },
        };
        let mut burst = window(vec![message.clone()], vec![], None);
        assert_eq!(
            burst.absorb(window(vec![], vec!["old".into()], Some(true))),
            None,
            "a burst to one window applies as one"
        );
        assert!(
            matches!(&burst, Update::Messages { update, .. } if update.remove == ["old"] && update.older == Some(true))
        );
        let undo = window(vec![], vec![message.id.clone()], None);
        assert!(burst.absorb(undo).is_some(), "removing what the burst brought keeps them apart");
        assert!(burst.absorb(Update::Chats(Window::default())).is_some(), "and so do two windows");

        let poll: Message =
            serde_json::from_str(r#"{"id":"P","v":1,"sort":"1~1","at":1,"kind":"a_kind_from_later","spans":[]}"#)
                .unwrap();
        assert_eq!(poll.kind, Kind::Unknown, "a kind jaw doesn't know yet still reads");
    }
}
