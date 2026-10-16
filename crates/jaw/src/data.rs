//! What the interface knows of jawd: the rows it draws, the updates that bring them, and what it
//! asks for. The rows are shaped after protocol/README.md's and read from its JSON, so the client
//! crate can hand them over as they come; what the interface asks for goes through `Jawd`, which
//! never waits, since answers arrive as updates on the main thread's channel. `crate::demo` is a
//! made-up account behind both.

use jaw_proto::{Connection, LinkStep};
use serde::Deserialize;

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
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
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
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Sender {
    #[serde(deserialize_with = "id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub colour: u8,
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
    #[serde(default)]
    pub person: Option<String>,
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
    /// The name of who wrote it.
    pub from: String,
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
        linked: bool,
        me: Option<String>,
        connection: Connection,
    },
    Connection(Connection),
    Link(LinkStep),
    /// The chat list's window. `reset` replaces what it held, as a watch's first rows do.
    Chats {
        reset: bool,
        upsert: Vec<Chat>,
        remove: Vec<String>,
        total: usize,
    },
    /// The open chat's window, for `chat`; updates for any other chat are from a window since
    /// closed. `older` and `newer` come when they change: whether there's more before the window,
    /// and how many have arrived past its end while it doesn't follow the newest.
    Messages {
        chat: String,
        reset: bool,
        upsert: Vec<Message>,
        remove: Vec<String>,
        older: Option<bool>,
        newer: Option<u32>,
    },
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
        chat: Chat,
    },
    /// Something to say once, in the footer.
    Notice(String),
    /// jaw is starting jawd; `failed` says why it couldn't.
    Starting {
        failed: Option<String>,
    },
    /// jawd doesn't answer; jaw tries again at `retry_at`.
    Unreachable {
        why: String,
        retry_at: i64,
    },
}

impl Update {
    /// Takes `next` into this update when the two apply as one, as a burst of upserts to the same
    /// window does; gives it back when they don't.
    pub fn absorb(&mut self, next: Update) -> Option<Update> {
        let clash = |upsert: &[String], remove: &[String]| remove.iter().any(|id| upsert.contains(id));
        match (self, next) {
            (
                Update::Chats { upsert, remove, total, .. },
                Update::Chats { reset: false, upsert: more, remove: gone, total: now },
            ) if !clash(&upsert.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), &gone) => {
                upsert.extend(more);
                remove.extend(gone);
                *total = now;
                None
            }
            (
                Update::Messages { chat, upsert, remove, older, newer, .. },
                Update::Messages { chat: other, reset: false, upsert: more, remove: gone, older: o, newer: n },
            ) if *chat == other && !clash(&upsert.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), &gone) => {
                upsert.extend(more);
                remove.extend(gone);
                *older = o.or(*older);
                *newer = n.or(*newer);
                None
            }
            (_, next) => Some(next),
        }
    }
}

/// Where a messages window is anchored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Around {
    /// The newest, which the window then follows.
    Newest,
    Message(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    Composing,
    Paused,
}

/// What the interface asks of jawd. Nothing here waits: what comes of it arrives as `Update`s.
pub trait Jawd {
    /// The chat list on All, `before` and `after` rows around a chat or from the top.
    fn watch_chats(&mut self, around: Option<&str>, before: usize, after: usize);
    /// The open chat's messages: a new window, or the same one moved.
    fn watch_messages(&mut self, chat: &str, around: Around, before: usize, after: usize);
    /// You opened a chat yourself, which the palette ranks by; passing through it doesn't count.
    fn opened(&mut self, chat: &str);
    fn send_text(&mut self, chat: &str, text: &str);
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
    /// Mutes a chat until then, `i64::MAX` for ever, or unmutes it with `None`.
    fn mute(&mut self, chat: &str, until: Option<i64>);
    fn archive(&mut self, chat: &str, archived: bool);
    fn pin(&mut self, chat: &str, pinned: bool);
    fn mark_unread(&mut self, chat: &str, unread: bool);
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
        assert_eq!(message.spans[1], Span { t: SpanKind::Bold, s: "photos".into(), person: None });
        assert_eq!(message.reactions[0], Reaction { emoji: "❤".into(), n: 2, mine: true });

        let mut burst = Update::Messages {
            chat: "ana".into(),
            reset: false,
            upsert: vec![message.clone()],
            remove: vec![],
            older: None,
            newer: None,
        };
        let more = Update::Messages {
            chat: "ana".into(),
            reset: false,
            upsert: vec![],
            remove: vec!["old".into()],
            older: Some(true),
            newer: None,
        };
        assert_eq!(burst.absorb(more), None, "a burst to one window applies as one");
        assert!(matches!(&burst, Update::Messages { remove, older: Some(true), .. } if remove == &["old"]));
        let undo = Update::Messages {
            chat: "ana".into(),
            reset: false,
            upsert: vec![],
            remove: vec![message.id.clone()],
            older: None,
            newer: None,
        };
        assert!(burst.absorb(undo).is_some(), "removing what the burst brought keeps them apart");
        let other = Update::Chats { reset: false, upsert: vec![], remove: vec![], total: 1 };
        assert!(burst.absorb(other).is_some(), "and so do two windows");

        let poll: Message =
            serde_json::from_str(r#"{"id":"P","v":1,"sort":"1~1","at":1,"kind":"a_kind_from_later","spans":[]}"#)
                .unwrap();
        assert_eq!(poll.kind, Kind::Unknown, "a kind jaw doesn't know yet still reads");
    }
}
