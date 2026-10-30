//! What the writer does with chats, people and messages. Each change writes its `events` row in
//! the same transaction and takes that event's seq as the row's `v`, so a row's version says when
//! it last changed: interfaces converge however updates arrive, and a client that saw seq S needs
//! only the rows whose `v` is past S.

mod again;
pub mod chat_actions;
pub mod contacts;
pub mod copies;
mod events;
pub mod expiry;
pub mod folders;
pub mod kinds;
pub mod layout;
pub mod links;
pub mod media;
pub mod outbox;
pub mod outgoing;
pub mod polls;

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// A change for the writer to make.
#[derive(Clone, Debug)]
pub enum Change {
    Chat(Chat),
    /// A conversation's chat as a history blob has it, `jids` being every JID the blob gives the
    /// conversation. Its settings (pinned, archived, muted, marked unread) are the phone's when it
    /// built the blob, which app state may have changed since, so each fills in only a setting app
    /// state has said nothing of for the chat: no value WhatsApp sent for its index, and no change
    /// of jaw's to it on its way.
    HistoryChat {
        chat: Chat,
        jids: Vec<String>,
    },
    Person(Person),
    Message(Message),
    Opened(Opened),
    /// A message gone, by its chat's JID and its ID, as deleting it for me does; its event stays,
    /// so the feed says it's gone. A chat whose newest it was shows the one before it, and keeps
    /// its place.
    DeleteMessage {
        chat: String,
        id: String,
    },
    /// Something done to a message: applied if it's there, kept in `pending` until it arrives if not.
    Amend(Amend),
    /// An edit, with the message as it now reads, which stands in for the message it edits when
    /// jaw doesn't have that one, as the phone shows it.
    Edited {
        amend: Amend,
        shown: Box<Message>,
    },
    Members(Members),
    /// A raw row that's been converted, removed in the transaction its conversion's rows go in.
    Drained(Drained),
    /// Something sync keeps between runs.
    Remember {
        key: String,
        value: String,
    },
    /// A frontend's settings changed together, in its domain, each as its key and its JSON, kept
    /// as jaw's own in one step, so they're kept all or none; `at` is when they were chosen, which
    /// a value already held doesn't change.
    Settings {
        domain: String,
        values: Vec<(String, String)>,
        at: i64,
    },
    /// A raw row that couldn't be converted, kept in quarantine with why.
    Quarantined {
        source: &'static str,
        reference: String,
        error: String,
    },
    /// An app-state mutation from inbox row `seq`: its index and its raw SyncActionValue, kept as
    /// the latest for its index, and `changes`, what it changes, made unless jaw has a patch for
    /// the index on its way, or something held for the chat's setting, under another of the chat's
    /// JIDs or jaw's, is as new or newer, which then stands (chat_actions::held_back). `at` is its
    /// time.
    AppState {
        index: Vec<String>,
        value: Vec<u8>,
        seq: i64,
        at: i64,
        changes: Vec<Change>,
    },
    /// Where a chat was read up to.
    Read(Read),
    /// What a history blob carried besides its conversations' messages, kept raw by the ID of the
    /// message that offered it.
    HistoryKept {
        id: String,
        raw: Packed,
    },
    /// Moves a few more messages of a chat being merged into the chat it's merged into, and
    /// finishes the merge once none are left: the merge named, or the first waiting.
    MergeStep(Option<Merging>),
    /// Sending and reading: the outbox and read positions.
    Outbox(outbox::Step),
    /// A chat pinned, archived, muted or marked, or a message starred, from jaw, with the app-state
    /// patches that tell your other devices queued in the same transaction.
    ChatAction(chat_actions::Act),
    /// What an interface keeps of how it was laid out.
    Layout(layout::Keep),
    /// One step of a backfill a migration left (`links::BACKFILLS`), from where the last left off.
    Backfill(&'static str),
    /// One step of taking what waits for the first event that has some left to take, from where the
    /// last left off (`events::step`).
    Opening,
    /// A link preview's thumbnail, written to media/links/ and synced: its row goes in, and message
    /// `id` in `chat`, which shows it, takes a new version.
    Thumbnail {
        sha256: String,
        bytes: u64,
        chat: String,
        id: String,
    },
    /// Folders and favourites, and presence and about texts.
    Folders(folders::Step),
    /// Media: downloads, previews, the phone asked again, and the cache.
    Media(media::Step),
    /// Media out: the copies of your files made and uploaded, and your files no copy needs.
    Outgoing(outgoing::Step),
    /// Message `n` erased if its time has come by `at`, as a disappearing message goes
    /// (`expiry::expire`).
    Expire {
        n: i64,
        at: i64,
    },
    /// A view-once message opened here, or shown no more (`expiry::once`).
    Once(expiry::Once),
}

/// A merge of one chat into another, as a merge step names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merging {
    /// The n of the chat being merged away.
    pub gone: i64,
    /// Which chats, as its quarantine row says if a step fails: see `merge_reference`.
    pub reference: String,
}

/// How a merge is named where it's kept in quarantine: the two chats' JIDs.
pub fn merge_reference(gone: &str, keep: &str) -> String {
    format!("{gone} into {keep}")
}

/// Where a chat was read up to: everything at or before `at`, or up to the newest of `ids` that
/// jaw has, which says it more exactly. It only moves forward once WhatsApp or you have said where
/// it is.
#[derive(Clone, Debug, Default)]
pub struct Read {
    /// The chat's JID.
    pub chat: String,
    /// In milliseconds since the epoch.
    pub at: i64,
    pub ids: Vec<String>,
    /// The phone's own unread and mention counts as of `at`, when it couldn't say where the chat
    /// was read; they stand until the position moves on.
    pub phone: Option<(u32, u32)>,
    /// You read it here, in jaw: WhatsApp hasn't been told, so read receipts are owed for what it
    /// reads, and a chat marked unread stays so until the phone says otherwise. Only `ids` say
    /// where, and one jaw doesn't have moves nothing.
    pub here: bool,
}

/// Something a later message or app state does to a message.
#[derive(Clone, Debug)]
pub struct Amend {
    /// The chat's JID.
    pub chat: String,
    /// The ID of the message it's done to.
    pub target: String,
    /// Who did it, by JID; the account's own JID when `mine`.
    pub sender: Option<String>,
    pub mine: bool,
    /// When, in milliseconds since the epoch.
    pub at: i64,
    pub what: Amendment,
    /// The raw protobuf of what did it, kept while it's pending.
    pub raw: Option<Packed>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Amendment {
    /// A reaction, or its removal when `emoji` is empty.
    React {
        emoji: String,
    },
    Edit {
        text: Option<String>,
        spans: Option<String>,
        summary: String,
    },
    /// Deleted for everyone.
    Revoke,
    Star {
        starred: bool,
    },
    DeleteForMe,
    /// Played on another of your devices, as your own played receipt says: someone else's
    /// view-once message is erased, as it goes from every device on the phones; any other message
    /// stays as it is.
    Opened,
    /// A receipt from a recipient: `delivered`, `read` or `played`.
    Status {
        status: String,
    },
    /// Kept in `pending` for a later jawd that knows what to do with it, such as a poll vote the
    /// engine couldn't decrypt.
    Keep {
        kind: String,
    },
    /// A vote in a poll, all its voter chose there now: each option by the SHA-256 of its name, in
    /// hex, none taking their vote back; `id` is the vote's own message's, which orders two votes
    /// sent in the same millisecond.
    Vote {
        options: Vec<String>,
        id: String,
    },
    /// The HD copy of a photo or video, sent as a message of its own that names this one.
    Hd {
        copy: Box<media::MediaCopy>,
    },
    /// An answer to an event, its answerer's latest: `going`, `maybe`, `not_going`, or `none` for
    /// one saying none of them, with the guests it brings; `id` is the answer's own message's,
    /// which orders two sent in the same millisecond.
    Answer {
        answer: String,
        guests: u32,
        id: String,
    },
    /// An event as an edit of it, opened, makes it: its text, spans, summary and detail, which the
    /// newest edit gives, by its time.
    EventEdit {
        text: Option<String>,
        spans: Option<String>,
        summary: String,
        detail: Option<String>,
    },
}

impl Amendment {
    fn kind(&self) -> &str {
        match self {
            Amendment::React { .. } => "reaction",
            Amendment::Edit { .. } => "edit",
            Amendment::Revoke => "revoke",
            Amendment::Star { .. } => "star",
            Amendment::DeleteForMe => "delete",
            Amendment::Opened => "opened",
            Amendment::Status { .. } => "receipt",
            Amendment::Keep { kind } => kind,
            Amendment::Vote { .. } => "vote",
            Amendment::Hd { .. } => "hd",
            Amendment::Answer { .. } => "answer",
            Amendment::EventEdit { .. } => "event_edit",
        }
    }
}

/// A group's members: every one of them when `all`, or some joining, changing role or leaving.
#[derive(Clone, Debug, Default)]
pub struct Members {
    pub chat: String,
    pub all: bool,
    /// Each member's JID and role: `member`, `admin` or `superadmin`.
    pub set: Vec<(String, &'static str)>,
    pub remove: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drained {
    Inbox(i64),
    History { chat: String, id: String },
}

/// You opened a chat yourself, so it ranks higher in the palette; a chat jaw hasn't seen is made.
#[derive(Clone, Debug, Default)]
pub struct Opened {
    /// Its JID.
    pub chat: String,
    /// When, in milliseconds since the epoch.
    pub at: i64,
}

/// How long until an open counts half as much.
pub const FRECENCY_HALF_LIFE_MS: i64 = 7 * 24 * 3600 * 1000;

/// A chat's frecency at `now`: `frecency` was its opens as they counted at `opened_at`, and each
/// counts half as much a half-life later.
pub fn frecency(frecency: f64, opened_at: Option<i64>, now: i64) -> f64 {
    match opened_at {
        Some(at) => frecency * 0.5f64.powf((now - at).max(0) as f64 / FRECENCY_HALF_LIFE_MS as f64),
        None => frecency,
    }
}

/// A chat as WhatsApp describes it. Fields left `None` keep what the chat has.
#[derive(Clone, Debug, Default)]
pub struct Chat {
    /// Its JID.
    pub id: String,
    /// `direct`, `group`, `community`, `announcement`, `broadcast` or `self`. A new chat without
    /// one takes it from its JID.
    pub kind: Option<&'static str>,
    pub name: Option<String>,
    /// When it was pinned, in milliseconds, or 0 to unpin it.
    pub pinned: Option<i64>,
    pub archived: Option<bool>,
    /// Until when it's muted, in milliseconds, or 0 to unmute it.
    pub muted_until: Option<i64>,
    pub marked_unread: Option<bool>,
    /// The JID of the community a group belongs to, or empty for none.
    pub community: Option<String>,
    /// The disappearing-message timer in seconds, or 0 for off.
    pub ephemeral: Option<u32>,
    /// When that timer was set, in milliseconds: a timer set before the chat's is older and changes
    /// nothing. `None` is as of now, for what says the timer as it is, such as a group's list.
    pub ephemeral_at: Option<i64>,
    /// What's typed in its box, or empty to clear it.
    pub draft: Option<String>,
    /// Its picture's path in jaw's folder, and WhatsApp's ID for it; empty when it has none.
    pub avatar: Option<String>,
    pub avatar_id: Option<String>,
}

/// Someone, by every JID they're known by. Fields left `None` keep what the person has; JIDs that
/// belonged to different people join them into one.
#[derive(Clone, Debug, Default)]
pub struct Person {
    /// Their phone number's JID, their LID, or both.
    pub jids: Vec<String>,
    /// As in the phone's contacts.
    pub name: Option<String>,
    /// What they call themselves.
    pub push_name: Option<String>,
    /// A business's verified name.
    pub verified_name: Option<String>,
}

/// A message, converted and ready to show. The whole row is replaced each time.
#[derive(Clone, Debug, Default)]
pub struct Message {
    /// Its chat's JID; a chat jaw hasn't seen is made.
    pub chat: String,
    /// Its WhatsApp ID, unique in its chat.
    pub id: String,
    /// When it was sent, in milliseconds since the epoch.
    pub at: i64,
    /// Who sent it, by JID; `None` for your own and for system lines.
    pub sender: Option<String>,
    pub mine: bool,
    /// As the protocol names kinds: `text`, `image`, `voice`, `poll`, `system`, `unknown` and so on.
    pub kind: &'static str,
    /// On your own: `pending`, `sent`, `delivered`, `read`, `played` or `failed`.
    pub status: Option<&'static str>,
    /// What search finds it by: its text, a caption, a poll's question or a document's name.
    pub text: Option<String>,
    /// The text parsed into the protocol's spans, as JSON; `None` when it's one plain span.
    pub spans: Option<String>,
    /// One line for the chat list and for quotes.
    pub summary: String,
    /// The quoted message, as the protocol's `reply` in JSON.
    pub reply: Option<String>,
    /// What its kind needs to be drawn (media, a poll, a location, a system line), as JSON.
    pub detail: Option<String>,
    pub forwarded: u32,
    pub edited_at: Option<i64>,
    pub deleted: bool,
    pub expires_at: Option<i64>,
    pub starred: bool,
    pub view_once: bool,
    /// Whether it arrived live rather than in history. History from others before where a chat is
    /// known to be read counts as read as it comes; one of yours reads the chat up to it.
    pub live: bool,
    pub mentions_me: bool,
    /// For one of yours, where it was sent from, which says how it reads its chat.
    pub sent: Sent,
    /// WhatsApp's WebMessageInfo, compressed.
    pub raw: Option<Packed>,
    /// For a live message, the engine's whole event, compressed, which a history copy never replaces.
    pub envelope: Option<Packed>,
    /// A poll's options, by name, in their order: a vote names each by the SHA-256 of its name.
    pub poll: Vec<String>,
    /// Its file, for a photo, a video, a voice message, a document or a sticker: its SD copy, or the
    /// only one.
    pub media: Option<Box<media::MediaCopy>>,
}

/// Where a message of yours was sent from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sent {
    /// Another of your devices, live or in history: it reads the chat up to it, and WhatsApp knows.
    #[default]
    Elsewhere,
    /// This jawd, in its chat, as replying on the phone is: it reads the chat up to it without
    /// WhatsApp being told, so receipts are owed for what it reads.
    Here,
    /// This jawd, as a forward, chosen from a list of chats rather than sent in one: where the chat
    /// was read stays as it was.
    Forwarded,
}

/// A raw protobuf compressed with zstd, one frame per row. Frames name the dictionary they were
/// made with, so rows packed with a dictionary later can sit beside these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packed(Vec<u8>);

const ZSTD_LEVEL: i32 = 3;

impl Packed {
    /// Compresses `raw`; do it before handing a message to the writer, which shouldn't spend its
    /// time on it.
    pub fn new(raw: &[u8]) -> Packed {
        thread_local! {
            static COMPRESSOR: RefCell<Option<zstd::bulk::Compressor<'static>>> = const { RefCell::new(None) };
        }
        COMPRESSOR.with(|cell| {
            let mut cell = cell.borrow_mut();
            if cell.is_none() {
                *cell = zstd::bulk::Compressor::new(ZSTD_LEVEL).ok();
            }
            match cell.as_mut().and_then(|compressor| compressor.compress(raw).ok()) {
                Some(packed) => Packed(packed),
                None => Packed(zstd::bulk::compress(raw, ZSTD_LEVEL).unwrap_or_default()),
            }
        })
    }
}

/// The raw protobuf a `Packed` holds, for tests outside the store.
#[cfg(test)]
pub fn unpack_for_tests(packed: &Packed) -> Vec<u8> {
    unpack(&packed.0).expect("a packed protobuf unpacks")
}

/// The raw protobuf a `Packed` holds.
pub fn unpack(packed: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(packed).context("a message's raw protobuf doesn't decompress")
}

/// The seqs handed out in the writer's transaction. A row changed twice in one transaction keeps
/// the seq of its first change and writes one event, since no reader sees between the two.
pub(super) struct Stamps {
    next: i64,
    began: i64,
    touched: HashMap<(&'static str, i64), i64>,
    /// The rows touched, in order, so a savepoint rolled back can be forgotten.
    order: Vec<(&'static str, i64)>,
    /// Chats whose read position may move and whose unread and mention counts are counted again
    /// before the transaction commits.
    unsettled: Vec<(i64, Unsettled)>,
    /// Each entry of `unsettled` as it was before each change to it, None for one added, so a
    /// savepoint rolled back moves no chat's read position.
    unsettling: Vec<(usize, Option<Unsettled>)>,
    /// Whether the transaction left what waits for an event to be taken in steps, which the
    /// background is told of once it commits.
    opening: bool,
    /// Whether the transaction took outbox rows whose copies of a file may be on their way, as a
    /// deletion takes an HD copy's, so whatever makes, uploads or waits for one is told once it
    /// commits.
    copies_taken: bool,
    /// The soonest time a message written in the transaction goes, for the timer that erases them.
    /// One a savepoint rolled back stays noted: it only wakes the timer, which looks again.
    expiring: Option<i64>,
    /// When the transaction erased a message whose links' words only a merge of links_fts takes
    /// out (`expiry`), as kept in sync_state.
    links_erased: Option<i64>,
}

/// What the transaction's messages said of where a chat was read, applied once before it commits.
#[derive(Clone, Copy, Default)]
struct Unsettled {
    /// The newest of your own messages from your other devices, which reads the chat up to it, and
    /// WhatsApp knows it.
    mine: Option<(i64, i64)>,
    /// The newest of yours sent from this jawd, which reads the chat up to it, WhatsApp untold.
    here: Option<(i64, i64)>,
    /// Whether one of yours arrived live, which unmarks the chat unless jaw's own change to the
    /// mark is on its way.
    mine_live: bool,
    /// Whether one of yours from your other devices came under another of the person's JIDs than
    /// the chat's own, which says nothing of the messages that came under the chat's own.
    mine_by_another: bool,
    /// The newest history message from others, read as it comes while nobody has said where the
    /// chat was read.
    history: Option<(i64, i64)>,
}

/// Where Stamps stood when a savepoint began.
pub(super) struct Mark {
    next: i64,
    touched: usize,
    unsettling: usize,
    copies_taken: bool,
}

impl Stamps {
    pub(super) fn load(db: &Connection) -> Result<Stamps> {
        let last: i64 = db.query_row("SELECT coalesce(max(seq), 0) FROM events", [], |row| row.get(0))?;
        Ok(Stamps {
            next: last + 1,
            began: last + 1,
            touched: HashMap::new(),
            order: Vec::new(),
            unsettled: Vec::new(),
            unsettling: Vec::new(),
            opening: false,
            copies_taken: false,
            expiring: None,
            links_erased: None,
        })
    }

    pub(super) fn begin(&mut self) {
        self.began = self.next;
        self.touched.clear();
        self.order.clear();
        self.unsettled.clear();
        self.unsettling.clear();
        self.expiring = None;
        self.links_erased = None;
        self.opening = false;
        self.copies_taken = false;
    }

    /// Notes that the transaction leaves what waits for an event to be taken in steps.
    fn opening(&mut self) {
        self.opening = true;
    }

    /// Whether the transaction left what waits for an event to be taken in steps.
    pub(super) fn opened_later(&self) -> bool {
        self.opening
    }

    pub(super) fn rolled_back(&mut self) {
        self.next = self.began;
        self.touched.clear();
        self.order.clear();
        self.unsettled.clear();
        self.unsettling.clear();
        self.copies_taken = false;
        self.expiring = None;
        self.links_erased = None;
    }

    /// Notes that a statement took `rows` outbox rows whose copies may be on their way.
    fn took_copies(&mut self, rows: usize) {
        self.copies_taken |= rows > 0;
    }

    /// Whether the transaction took outbox rows whose copies may be on their way.
    pub(super) fn copies_taken(&self) -> bool {
        self.copies_taken
    }

    /// Notes that a message written goes at `at`.
    pub(super) fn expires(&mut self, at: Option<i64>) {
        if let Some(at) = at {
            self.expiring = Some(self.expiring.map_or(at, |soonest| soonest.min(at)));
        }
    }

    /// The soonest time a message the committed transaction wrote goes, taken once.
    pub(super) fn take_expiring(&mut self) -> Option<i64> {
        self.expiring.take()
    }

    /// Notes that the transaction erased a message with links, at `at`, as sync_state keeps it.
    pub(super) fn links_erased(&mut self, at: i64) {
        self.links_erased = Some(at);
    }

    /// When the committed transaction last erased a message with links, taken once.
    pub(super) fn take_links_erased(&mut self) -> Option<i64> {
        self.links_erased.take()
    }

    /// Notes that chat `n`'s unread messages may have changed.
    fn unsettle(&mut self, n: i64) -> &mut Unsettled {
        let at = match self.unsettled.iter().position(|(chat, _)| *chat == n) {
            Some(at) => {
                self.unsettling.push((at, Some(self.unsettled[at].1)));
                at
            }
            None => {
                self.unsettling.push((self.unsettled.len(), None));
                self.unsettled.push((n, Unsettled::default()));
                self.unsettled.len() - 1
            }
        };
        &mut self.unsettled[at].1
    }

    pub(super) fn mark(&self) -> Mark {
        Mark {
            next: self.next,
            touched: self.order.len(),
            unsettling: self.unsettling.len(),
            copies_taken: self.copies_taken,
        }
    }

    /// Forgets what was stamped, unsettled and taken since `mark`, whose savepoint was rolled back.
    pub(super) fn undo(&mut self, mark: Mark) {
        for key in self.order.drain(mark.touched..) {
            self.touched.remove(&key);
        }
        self.next = mark.next;
        self.copies_taken = mark.copies_taken;
        for (at, was) in self.unsettling.drain(mark.unsettling..).rev() {
            match was {
                Some(was) => self.unsettled[at].1 = was,
                None => self.unsettled.truncate(at),
            }
        }
    }

    /// The seq a change to an existing row would take.
    fn seq(&self, kind: &'static str, key: Option<i64>) -> i64 {
        key.and_then(|key| self.touched.get(&(kind, key)).copied()).unwrap_or(self.next)
    }

    /// Writes the event for a row that changed with `seq`, unless it already has one.
    fn record(&mut self, db: &Connection, kind: &'static str, key: i64, chat: Option<i64>, seq: i64) -> Result<()> {
        if self.touched.contains_key(&(kind, key)) {
            return Ok(());
        }
        debug_assert_eq!(seq, self.next);
        db.prepare_cached("INSERT INTO events (seq, at, type, key, chat) VALUES (?1, ?2, ?3, ?4, ?5)")?
            .execute(params![seq, now_ms(), kind, key, chat])?;
        self.touched.insert((kind, key), seq);
        self.order.push((kind, key));
        self.next += 1;
        Ok(())
    }

    /// The last seq written, committed or not.
    pub(super) fn last(&self) -> i64 {
        self.next - 1
    }
}

/// Whether an error says the database can't be written right now (a full disk, an I/O error, a
/// lock), so the whole transaction fails and is tried again, rather than this change being bad.
pub(super) fn transient(e: &anyhow::Error) -> bool {
    use rusqlite::ErrorCode::*;
    e.chain().any(|cause| match cause.downcast_ref::<rusqlite::Error>() {
        Some(rusqlite::Error::SqliteFailure(failure, _)) => matches!(
            failure.code,
            DiskFull
                | SystemIoFailure
                | DatabaseBusy
                | DatabaseLocked
                | ReadOnly
                | CannotOpen
                | OutOfMemory
                | DatabaseCorrupt
                | NotADatabase
        ),
        _ => false,
    })
}

/// Where a group of changes came from, for the quarantine: the raw row it was converted from, or
/// the change itself and its raw protobuf, or a settings change's JSON, so it can be read back.
fn provenance(group: &[Change]) -> (&'static str, String, Option<Cow<'_, [u8]>>) {
    let (source, reference, raw): (&'static str, String, Option<&[u8]>) = match group.last() {
        Some(Change::Drained(Drained::Inbox(seq))) => ("inbox", seq.to_string(), None),
        Some(Change::Drained(Drained::History { chat, id })) => ("history_raw", format!("{chat} {id}"), None),
        Some(Change::Message(m)) => ("message", format!("{} {}", m.chat, m.id), m.raw.as_ref().map(|p| p.0.as_slice())),
        Some(Change::Edited { shown, .. }) => {
            ("message", format!("{} {}", shown.chat, shown.id), shown.raw.as_ref().map(|p| p.0.as_slice()))
        }
        Some(Change::Amend(a)) => {
            ("amendment", format!("{} {} {}", a.chat, a.target, a.what.kind()), a.raw.as_ref().map(|p| p.0.as_slice()))
        }
        Some(Change::Chat(c) | Change::HistoryChat { chat: c, .. }) => ("chat", c.id.clone(), None),
        Some(Change::Person(p)) => ("person", p.jids.join(" "), None),
        Some(Change::Members(m)) => ("members", m.chat.clone(), None),
        Some(Change::AppState { index, value, .. }) => ("app_state", index.join(" "), Some(value.as_slice())),
        Some(Change::Remember { key, .. }) => ("remember", key.clone(), None),
        Some(Change::Settings { domain, values, .. }) => {
            let change = serde_json::json!({ "domain": domain, "values": values }).to_string();
            return ("settings", domain.clone(), Some(Cow::Owned(change.into_bytes())));
        }
        Some(Change::Quarantined { source, reference, .. }) => (source, reference.clone(), None),
        Some(Change::Opened(o)) => ("opened", o.chat.clone(), None),
        Some(Change::DeleteMessage { chat, id }) => ("delete", format!("{chat} {id}"), None),
        Some(Change::Read(read)) => ("read", format!("{} {}", read.chat, read.at), None),
        Some(Change::HistoryKept { id, raw }) => ("history", id.clone(), Some(raw.0.as_slice())),
        Some(Change::MergeStep(merging)) => {
            ("merge", merging.as_ref().map(|m| m.reference.clone()).unwrap_or_default(), None)
        }
        Some(Change::Outbox(step)) => outbox::provenance(step),
        Some(Change::ChatAction(act)) => chat_actions::provenance(act),
        Some(Change::Layout(keep)) => layout::provenance(keep),
        Some(Change::Backfill(name)) => ("backfill", name.to_string(), None),
        Some(Change::Opening) => ("opening", String::new(), None),
        Some(Change::Thumbnail { sha256, chat, id, .. }) => ("thumbnail", format!("{chat} {id} {sha256}"), None),
        Some(Change::Folders(step)) => folders::provenance(step),
        Some(Change::Media(step)) => media::provenance(step),
        Some(Change::Outgoing(step)) => outgoing::provenance(step),
        Some(Change::Expire { n, .. }) => ("expire", n.to_string(), None),
        Some(Change::Once(once)) => ("once", format!("{once:?}"), None),
        None => ("nothing", String::new(), None),
    };
    (source, reference, raw.map(Cow::Borrowed))
}

/// Keeps a group of changes that failed, with its error, in place of it.
pub(super) fn quarantine(db: &Connection, group: &[Change], error: &anyhow::Error) -> Result<()> {
    let (source, reference, raw) = provenance(group);
    // A step asked for before its merge was pointed on is named by where it goes now, so the step
    // that ends the merge finds its row.
    let reference = match group.last() {
        Some(Change::MergeStep(Some(merging))) => merge_named(db, merging.gone)?.unwrap_or(reference),
        _ => reference,
    };
    eprintln!("jawd: kept a change that failed in quarantine ({source} {reference}): {error:#}");
    record_quarantine(db, source, &reference, &format!("{error:#}"), raw.as_deref())
}

fn record_quarantine(db: &Connection, source: &str, reference: &str, error: &str, raw: Option<&[u8]>) -> Result<()> {
    db.prepare_cached(
        "INSERT INTO quarantine (at, source, ref, error, raw) VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT (source, ref) DO UPDATE SET at = excluded.at, error = excluded.error, raw = coalesce(excluded.raw, raw)",
    )?
    .execute(params![now_ms(), source, reference, error, raw])?;
    Ok(())
}

/// The changes split where each raw row's conversion ends: at each Drained, or a Quarantined row
/// that didn't convert, the changes before it go with it. Changes after the last of those, or in a
/// batch without one, are a group each.
pub(super) fn groups(changes: &[Change]) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (i, change) in changes.iter().enumerate() {
        if matches!(change, Change::Drained(_) | Change::Quarantined { .. }) {
            groups.push(start..i + 1);
            start = i + 1;
        }
    }
    groups.extend((start..changes.len()).map(|i| i..i + 1));
    groups
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Whether making `changes` may write to FTS5 itself, where a new message's text and links wait in
/// fts_backlog: an edit, a deletion (one for you comes through app state), a send or a merge may
/// change a message that's indexed, and the links backfill indexes the links of messages that are.
pub(super) fn touches_fts(changes: &[Change]) -> bool {
    changes.iter().any(|change| match change {
        Change::Amend(amend) => {
            matches!(
                amend.what,
                Amendment::Edit { .. } | Amendment::Revoke | Amendment::DeleteForMe | Amendment::Opened
            )
        }
        Change::Edited { .. }
        | Change::DeleteMessage { .. }
        | Change::MergeStep(_)
        | Change::Outbox(_)
        | Change::Backfill(_)
        | Change::Opening
        | Change::Expire { .. }
        | Change::Once(_) => true,
        Change::AppState { changes, .. } => touches_fts(changes),
        _ => false,
    })
}

/// Makes one change inside the writer's open transaction, saying whether anything changed. A new
/// message's text waits in fts_backlog to be indexed for search.
pub(super) fn apply(db: &Connection, stamps: &mut Stamps, change: &Change) -> Result<bool> {
    match change {
        Change::Chat(chat) => upsert_chat(db, stamps, chat),
        Change::HistoryChat { chat, jids } => upsert_chat(db, stamps, &unsaid(db, chat, jids)?),
        Change::Person(person) => upsert_person(db, stamps, person),
        Change::Message(message) => upsert_message(db, stamps, message),
        Change::Opened(opened) => open_chat(db, stamps, opened),
        Change::DeleteMessage { chat, id } => delete_message(db, stamps, chat, id),
        Change::Amend(amend) => amend_message(db, stamps, amend),
        Change::Edited { amend, shown } => {
            if target(db, stamps, amend)?.is_some() {
                amend_message(db, stamps, amend)
            } else {
                upsert_message(db, stamps, shown)
            }
        }
        Change::Members(members) => set_members(db, stamps, members),
        // A row drained has been converted, so what an earlier failure kept of it goes too, as does
        // a wait for an echo that had to come by it.
        Change::Drained(Drained::Inbox(seq)) => {
            db.prepare_cached("DELETE FROM inbox WHERE seq = ?1")?.execute([seq])?;
            db.prepare_cached("DELETE FROM quarantine WHERE source = 'inbox' AND ref = ?1")?
                .execute([seq.to_string()])?;
            chat_actions::passed(db, *seq)?;
            Ok(false)
        }
        Change::Drained(Drained::History { chat, id }) => {
            db.prepare_cached("DELETE FROM history_raw WHERE chat = ?1 AND id = ?2")?.execute(params![chat, id])?;
            db.prepare_cached("DELETE FROM quarantine WHERE source = 'history_raw' AND ref = ?1")?
                .execute([format!("{chat} {id}")])?;
            Ok(false)
        }
        Change::Remember { key, value } => {
            #[cfg(test)]
            if key == super::FAIL_SHARED && super::SHARING.with(std::cell::Cell::get) {
                super::FAILED_SHARED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let full = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL);
                return Err(rusqlite::Error::SqliteFailure(full, Some("failed in a shared commit".into())).into());
            }
            db.prepare_cached("INSERT OR REPLACE INTO sync_state (key, value) VALUES (?1, ?2)")?
                .execute(params![key, value])?;
            Ok(false)
        }
        Change::Settings { domain, values, at } => {
            let mut keep = db.prepare_cached(
                "INSERT INTO settings (domain, key, value, at) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT (domain, key) DO UPDATE SET value = excluded.value, at = excluded.at
                WHERE value IS NOT excluded.value",
            )?;
            for (key, value) in values {
                keep.execute(params![domain, key, value, at])?;
            }
            Ok(false)
        }
        Change::Quarantined { source, reference, error } => {
            record_quarantine(db, source, reference, error, None)?;
            Ok(false)
        }
        Change::AppState { index, value, seq, at, changes } => {
            let key = serde_json::to_string(index)?;
            db.prepare_cached("INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)")?
                .execute(params![key, value])?;
            let instead;
            let changes = match chat_actions::held_back(db, index, *seq, *at)? {
                chat_actions::Held::No => changes,
                chat_actions::Held::Back => return Ok(false),
                chat_actions::Held::Instead { index, value } => {
                    instead = crate::convert::app_state::mutation(&index, &value).changes;
                    &instead
                }
            };
            let mut changed = false;
            for change in changes {
                changed |= apply(db, stamps, change)?;
            }
            Ok(changed)
        }
        Change::Read(read) => set_read(db, stamps, read),
        Change::HistoryKept { id, raw } => {
            db.prepare_cached(
                "INSERT INTO history_kept (id, at, raw) VALUES (?1, ?2, ?3)
                ON CONFLICT (id) DO UPDATE SET at = excluded.at, raw = excluded.raw",
            )?
            .execute(params![id, now_ms(), raw.0])?;
            Ok(false)
        }
        Change::MergeStep(merging) => merge_step(db, stamps, merging.as_ref().map(|m| m.gone)),
        Change::Outbox(step) => outbox::apply(db, stamps, step),
        Change::ChatAction(act) => chat_actions::apply(db, stamps, act),
        Change::Layout(keep) => layout::apply(db, keep),
        Change::Backfill(name) => links::backfill(db, stamps, name),
        Change::Opening => events::step(db, stamps),
        Change::Thumbnail { sha256, bytes, chat, id } => links::thumbnail(db, stamps, sha256, *bytes, chat, id),
        Change::Folders(step) => folders::apply(db, stamps, step),
        Change::Media(step) => media::apply(db, stamps, step),
        Change::Outgoing(step) => outgoing::apply(db, stamps, step),
        Change::Expire { n, at } => expiry::expire(db, stamps, *n, *at),
        Change::Once(once) => expiry::once(db, stamps, once),
    }
}

/// The most unread messages a chat counts; more show as this many.
pub const UNREAD_MOST: i64 = 1_000;

/// A position said as a time, T, is (T, THROUGH): every message up to T, however many share its
/// millisecond. One said by a message is that message's (at, n), which orders it exactly among
/// others at the same time.
pub(super) const THROUGH: i64 = i64::MAX;

/// Before every message: where a chat with no position is read up to.
const START: (i64, i64) = (i64::MIN, i64::MIN);

/// Where a chat was read up to and how far WhatsApp has been told, as settle and set_read read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChatRead {
    /// None: nothing read yet.
    pub(super) at: Option<(i64, i64)>,
    known: bool,
    phone_unread: Option<i64>,
    phone_mentions: Option<i64>,
    marked: bool,
    /// None while WhatsApp knows as much as jaw (`told_now`).
    told: Option<(i64, i64)>,
}

/// Chat `chat`'s read position, if it has a row.
pub(super) fn chat_read(db: &Connection, chat: i64) -> Result<Option<ChatRead>> {
    Ok(db
        .prepare_cached(
            "SELECT read_at, read_n, read_known, phone_unread, phone_mentions, marked_unread, told_at, told_n
            FROM chats WHERE n = ?1",
        )?
        .query_row([chat], |r| {
            let at: Option<i64> = r.get(0)?;
            let told: Option<i64> = r.get(6)?;
            Ok(ChatRead {
                at: at
                    .map(|at| Ok::<_, rusqlite::Error>((at, r.get::<_, Option<i64>>(1)?.unwrap_or(THROUGH))))
                    .transpose()?,
                known: r.get(2)?,
                phone_unread: r.get(3)?,
                phone_mentions: r.get(4)?,
                marked: r.get(5)?,
                told: told.map(|at| Ok::<_, rusqlite::Error>((at, r.get(7)?))).transpose()?,
            })
        })
        .optional()?)
}

/// Writes what changed of chat `chat`'s read position from `was` to `now`; marked_unread is the
/// caller's, since it's shown.
fn write_read(db: &Connection, chat: i64, was: &ChatRead, now: &ChatRead) -> Result<()> {
    if (&now.at, now.known, now.phone_unread, now.phone_mentions, &now.told)
        == (&was.at, was.known, was.phone_unread, was.phone_mentions, &was.told)
    {
        return Ok(());
    }
    db.prepare_cached(
        "UPDATE chats SET read_at = ?2, read_n = ?3, read_known = ?4, phone_unread = ?5, phone_mentions = ?6,
            told_at = ?7, told_n = ?8
        WHERE n = ?1",
    )?
    .execute(params![
        chat,
        now.at.map(|(at, _)| at),
        now.at.and_then(|(_, n)| (n != THROUGH).then_some(n)),
        now.known,
        now.phone_unread,
        now.phone_mentions,
        now.told.map(|(at, _)| at),
        now.told.map(|(_, n)| n),
    ])?;
    Ok(())
}

/// How far WhatsApp has been told chat `chat`, as `read` holds it, is read: where it was last told,
/// or, while it knows as much as jaw, the position, but before the phone's own unread messages
/// from others while its counts stand, so reading those here sends their receipts.
pub(super) fn told_now(db: &Connection, chat: i64, read: &ChatRead) -> Result<(i64, i64)> {
    if let Some(told) = read.told {
        return Ok(told);
    }
    let Some(at) = read.at else { return Ok(START) };
    let unread = read.phone_unread.unwrap_or(0).min(UNREAD_MOST);
    if unread <= 0 {
        return Ok(at);
    }
    let newest: Vec<(i64, i64)> = db
        .prepare_cached(
            "SELECT at, n FROM messages WHERE chat = ?1 AND (at, n) <= (?2, ?3) AND mine = 0
                AND kind NOT IN ('system', 'call')
            ORDER BY at DESC, n DESC LIMIT ?4",
        )?
        .query_map(params![chat, at.0, at.1, unread + 1], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(newest.get(unread as usize).copied().unwrap_or(START))
}

/// What's left of "told" once the position is `at`: never past it, and nothing once it's there,
/// since WhatsApp then knows as much as jaw.
fn told_within(told: Option<(i64, i64)>, at: Option<(i64, i64)>) -> Option<(i64, i64)> {
    let (told, at) = (told?, at?);
    (told < at).then_some(told)
}

/// "Told" as reading chat `chat` here leaves it, `was` before and `told` after: kept only if a
/// message from others lies between it and the position `at`, so a chat with nothing to tell
/// WhatsApp of owes nothing.
fn told_if_owed(
    db: &Connection,
    chat: i64,
    was: Option<(i64, i64)>,
    told: Option<(i64, i64)>,
    at: Option<(i64, i64)>,
) -> Result<Option<(i64, i64)>> {
    let (Some(from), Some(to), None) = (told, at, was) else { return Ok(told) };
    let owed: bool = db
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
                AND mine = 0 AND kind NOT IN ('system', 'call'))",
        )?
        .query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?;
    Ok(owed.then_some(from))
}

/// Moves where each chat the transaction touched was read up to, as its messages said, and counts
/// its unread messages and mentions again: messages from others after that position, and the
/// phone's own counts while they stand. Run once before the transaction commits, so a batch of
/// history moves and counts each chat once, in one update.
pub(super) fn settle(db: &Connection, stamps: &mut Stamps) -> Result<()> {
    let unsettled = std::mem::take(&mut stamps.unsettled);
    stamps.unsettling.clear();
    let chats: Vec<i64> = unsettled.iter().map(|(chat, _)| *chat).collect();
    settle_each(db, stamps, unsettled)?;
    // A chat being merged counts from the later of its position and its kept chat's, and is
    // marked unread only while that one is; both may just have moved, so it's counted after.
    let mut merging = Vec::new();
    for keep in chats {
        merging.extend(chats_merging_into(db, keep)?.into_iter().map(|gone| (gone, Unsettled::default())));
    }
    settle_each(db, stamps, merging)?;
    communities_follow(db, stamps)
}

/// A community's row sums up its groups, their unread messages, mentions and newest message, and
/// sits where the newest of them does, so it changes with each of its groups the transaction
/// changed.
fn communities_follow(db: &Connection, stamps: &mut Stamps) -> Result<()> {
    let groups: Vec<i64> = stamps.order.iter().filter(|(kind, _)| *kind == CHAT).map(|(_, n)| *n).collect();
    for group in groups {
        let community: Option<i64> = db
            .prepare_cached("SELECT k.n FROM chats c JOIN chats k ON k.id = c.community WHERE c.n = ?1")?
            .query_row([group], |r| r.get(0))
            .optional()?;
        if let Some(community) = community.filter(|k| !stamps.touched.contains_key(&(CHAT, *k))) {
            let seq = stamps.seq(CHAT, Some(community));
            db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute(params![community, seq])?;
            stamps.record(db, CHAT, community, Some(community), seq)?;
        }
    }
    Ok(())
}

fn settle_each(db: &Connection, stamps: &mut Stamps, unsettled: Vec<(i64, Unsettled)>) -> Result<()> {
    for (chat, moved) in unsettled {
        let Some(was) = chat_read(db, chat)? else { continue };
        let mut now = was.clone();
        let past = |place: Option<(i64, i64)>| place.filter(|&place| was.at.is_none_or(|read| place > read));
        let (mine, here) = (past(moved.mine), past(moved.here));
        let history = past(moved.history).filter(|_| !was.known);
        // WhatsApp knows what your other devices sent and what history brought; what you sent
        // here reads the chat without telling it, as replying does, so the receipts for what it
        // reads are owed.
        let known = mine.max(history);
        if here > known && now.told.is_none() {
            now.told = Some(told_now(db, chat, &was)?);
        }
        if let Some(at) = mine.max(here) {
            (now.at, now.phone_unread, now.phone_mentions) = (Some(at), None, None);
        }
        if let Some(at) = history {
            now.at = now.at.max(Some(at));
        }
        // One of yours from another device means the chat was read there, which takes its mark
        // off, unless jaw's own change to the mark is on its way: WhatsApp takes that after it.
        let mut marked = was.marked;
        if marked && moved.mine_live {
            let jid: String =
                db.prepare_cached("SELECT id FROM chats WHERE n = ?1")?.query_row([chat], |r| r.get(0))?;
            marked = chat_actions::marking(db, &jid)?;
        }
        // A chat being merged is the same conversation as the one it goes into, so reading that
        // one reads this too: it counts from the later of the two positions, and stays marked
        // unread only while that one is. Its own position and how far WhatsApp was told of it
        // stay as the merge found them (merge_reads), which only receipts sent move.
        let (mut counted_from, mut phone) = (now.at, (now.phone_unread, now.phone_mentions));
        let into: Option<(Option<i64>, Option<i64>, bool)> = db
            .prepare_cached(
                "SELECT k.read_at, k.read_n, k.marked_unread FROM chat_merges m JOIN chats k ON k.n = m.keep
                WHERE m.gone = ?1",
            )?
            .query_row([chat], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        match into {
            Some((kept_at, kept_n, kept_marked)) => {
                marked &= kept_marked;
                let kept = kept_at.map(|at| (at, kept_n.unwrap_or(THROUGH)));
                if let Some(kept) = kept.filter(|&kept| now.at.is_none_or(|read| kept > read)) {
                    (counted_from, phone) = (Some(kept), (None, None));
                }
            }
            None => {
                // While WhatsApp knew as much as jaw, it was told up to where jaw was read, and what
                // moved the position past a message that came under another JID says nothing of it.
                if now.told.is_none()
                    && now.at != was.at
                    && let Some(to) = now.at
                    && several_jids(db, chat)?
                {
                    let from = told_now(db, chat, &was)?;
                    if moved.mine_by_another || came_elsewhere(db, chat, Some(from), to)? {
                        now.told = Some(from);
                    }
                }
                // What your other devices sent, and what history brought, tell WhatsApp only of what
                // came under the JID they came by, so, as for a read (set_read), how far it was told
                // doesn't move for one of yours that came under another of the person's JIDs, nor
                // past a message from others that did, which a merge leaves owed.
                let known = match known {
                    Some(to) if !moved.mine_by_another && !came_elsewhere(db, chat, now.told, to)? => Some(to),
                    _ => None,
                };
                now.told = told_within(now.told.map(|told| told.max(known.unwrap_or(START))), now.at);
                now.told = told_if_owed(db, chat, was.told, now.told, now.at)?;
            }
        }
        let (from_at, from_n) = counted_from.unwrap_or(START);
        let (unread, mentions): (i64, i64) = db
            .prepare_cached(
                "SELECT count(*), coalesce(sum(mentions_me), 0) FROM (SELECT mentions_me FROM messages
                    WHERE chat = ?1 AND (at, n) > (?2, ?3) AND mine = 0 AND kind NOT IN ('system', 'call') LIMIT ?4)",
            )?
            .query_row(params![chat, from_at, from_n, UNREAD_MOST], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let unread = (unread + phone.0.unwrap_or(0)).min(UNREAD_MOST);
        let mentions = (mentions + phone.1.unwrap_or(0)).min(UNREAD_MOST);
        let seq = stamps.seq(CHAT, Some(chat));
        // Where it was read isn't shown, so moving it alone makes no new version of the chat.
        let shown = db
            .prepare_cached(
                "UPDATE chats SET unread = ?2, mentions = ?3, marked_unread = ?4, v = ?5
                WHERE n = ?1 AND (unread IS NOT ?2 OR mentions IS NOT ?3 OR marked_unread IS NOT ?4)",
            )?
            .execute(params![chat, unread, mentions, marked, seq])?;
        if shown > 0 {
            stamps.record(db, CHAT, chat, Some(chat), seq)?;
        }
        write_read(db, chat, &was, &now)?;
    }
    Ok(())
}

/// Moves where a chat was read up to. Until WhatsApp or you have said where it is, or while the
/// phone's own counts stand in for it, the position is taken as said, even if it's earlier;
/// after that it only moves forward. WhatsApp saying so means it knows, and unmarks the chat,
/// unless jaw's own change to the mark is on its way: WhatsApp takes that after this read, so the
/// mark stays as jaw left it. Your reading it here owes it receipts for what that reads, and leaves
/// the mark to the phone.
/// In a one-to-one chat whose person has more than one JID, WhatsApp saying so by one of them, the
/// chat's own or one merged into it, says only that what came under that JID was read: while a
/// message from others that came under another lies past how far WhatsApp was told, up to the
/// read, that stays, and while WhatsApp knew as much as jaw, how far it was told is taken as where
/// the chat was read before, and stays there. The receipts go, again for what the phone did read,
/// which is harmless, but never for what the phone's counts call unread (`outbox::owed`).
fn set_read(db: &Connection, stamps: &mut Stamps, read: &Read) -> Result<bool> {
    let jid = chat_jid(db, &read.chat)?;
    let by = user_jid(&read.chat);
    let own = by == jid;
    let chat = ensure_chat(db, stamps, &jid)?;
    let mut at = (read.at, THROUGH);
    if !read.ids.is_empty() {
        if merging_into(db, chat)? {
            for id in &read.ids {
                moved_from_merging(db, stamps, chat, id)?;
            }
        }
        let named: Vec<(i64, i64)> = db
            .prepare_cached("SELECT at, n FROM messages WHERE chat = ?1 AND id IN (SELECT value FROM json_each(?2))")?
            .query_map(params![chat, serde_json::to_string(&read.ids)?], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        match named.into_iter().max() {
            Some(newest) => at = newest,
            None if read.here => return Ok(false),
            None => {}
        }
    } else if read.here {
        return Ok(false);
    }
    let Some(was) = chat_read(db, chat)? else { return Ok(false) };
    let mut now = was.clone();
    let taken = !was.known || was.phone_unread.is_some() || was.at.is_none_or(|read| read < at);
    let several = !read.here && several_jids(db, chat)?;
    let told = match was.told {
        None if several => Some(told_now(db, chat, &was)?),
        told => told,
    };
    let speaks = !read.here && if several { !came_other(db, chat, told, at, &by)? } else { own };
    if read.here {
        if !taken {
            return Ok(false);
        }
        if now.told.is_none() {
            now.told = Some(told_now(db, chat, &was)?);
        }
        (now.at, now.known, now.phone_unread, now.phone_mentions) = (Some(at), true, None, None);
    } else if taken {
        let (phone_unread, phone_mentions) = read.phone.map_or((None, None), |(u, m)| (Some(u.into()), Some(m.into())));
        (now.at, now.known, now.phone_unread, now.phone_mentions) = (Some(at), true, phone_unread, phone_mentions);
        now.marked = now.marked && chat_actions::marking(db, &jid)?;
        if speaks {
            now.told = None;
        } else if several {
            now.told = told;
        }
    } else if speaks {
        // jaw is further on; WhatsApp now knows the chat is read this far.
        now.told = now.told.map(|told| told.max(at));
    }
    now.told = told_if_owed(db, chat, was.told, told_within(now.told, now.at), now.at)?;
    if now == was {
        return Ok(false);
    }
    write_read(db, chat, &was, &now)?;
    if now.marked != was.marked {
        let seq = stamps.seq(CHAT, Some(chat));
        db.prepare_cached("UPDATE chats SET marked_unread = ?2, v = ?3 WHERE n = ?1")?
            .execute(params![chat, now.marked, seq])?;
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    stamps.unsettle(chat);
    Ok(true)
}

/// Whether chat `chat` is a one-to-one chat whose person has more than one JID, so its messages
/// can have come under several.
fn several_jids(db: &Connection, chat: i64) -> Result<bool> {
    Ok(db
        .prepare_cached(
            "SELECT count(*) > 1 FROM person_jids
            WHERE person = (SELECT person FROM chats WHERE n = ?1 AND kind = 'direct')",
        )?
        .query_row([chat], |r| r.get(0))?)
}

/// Whether chat `chat` has a message from others after `told` and up to `to` that came under
/// another JID than `jid`.
fn came_other(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64), jid: &str) -> Result<bool> {
    let Some(from) = told else { return Ok(false) };
    Ok(db
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
                AND mine = 0 AND kind NOT IN ('system', 'call')
                AND sender IS NOT (SELECT j.n FROM person_jids j WHERE j.jid = ?6))",
        )?
        .query_row(params![chat, from.0, from.1, to.0, to.1, jid], |r| r.get(0))?)
}

/// Whether chat `chat` has a message from others after `told` and up to `to` that came under
/// another of its person's JIDs than the chat's own, as a merge brings them.
fn came_elsewhere(db: &Connection, chat: i64, told: Option<(i64, i64)>, to: (i64, i64)) -> Result<bool> {
    let Some(from) = told else { return Ok(false) };
    Ok(db
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND (at, n) <= (?4, ?5)
                AND mine = 0 AND kind NOT IN ('system', 'call') AND sender IN
                    (SELECT j.n FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.n = ?1 AND j.jid <> c.id))",
        )?
        .query_row(params![chat, from.0, from.1, to.0, to.1], |r| r.get(0))?)
}

/// Merges chat `gone`'s read position into `keep`'s, as a merge starts and again just before
/// `gone` is removed: the later position wins, one WhatsApp or you said over a guess, with its
/// phone counts; and "told" goes back to just before the oldest of `gone`'s messages from others
/// that WhatsApp hasn't been told of, since a receipt sent twice is harmless and one skipped is
/// lost. Once `gone`'s messages have all moved, that's none, so merging again changes nothing.
fn merge_reads(db: &Connection, stamps: &mut Stamps, keep: i64, gone: i64) -> Result<()> {
    let (Some(k), Some(g)) = (chat_read(db, keep)?, chat_read(db, gone)?) else { return Ok(()) };
    let from = told_now(db, gone, &g)?;
    let untold: Option<(i64, i64)> = db
        .prepare_cached(
            "SELECT at, n FROM messages WHERE chat = ?1 AND (at, n) > (?2, ?3) AND mine = 0
                AND kind NOT IN ('system', 'call')
            ORDER BY at, n LIMIT 1",
        )?
        .query_row(params![gone, from.0, from.1], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let mut told = told_now(db, keep, &k)?;
    if let Some((at, n)) = untold {
        told = told.min((at, n - 1));
    }
    let winner = if (g.known, g.at) > (k.known, k.at) { &g } else { &k };
    let mut now = ChatRead {
        at: winner.at,
        known: k.known || g.known,
        phone_unread: winner.phone_unread,
        phone_mentions: winner.phone_mentions,
        marked: k.marked,
        told: Some(told),
    };
    now.told = told_within(now.told, now.at);
    write_read(db, keep, &k, &now)?;
    stamps.unsettle(keep);
    Ok(())
}

/// The chats being merged into `keep`.
fn chats_merging_into(db: &Connection, keep: i64) -> Result<Vec<i64>> {
    Ok(db
        .prepare_cached("SELECT gone FROM chat_merges WHERE keep = ?1")?
        .query_map([keep], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// Counts an open in the chat's frecency, the chat found as every write finds it, so an open by a
/// JID being merged, or merged away, counts in the chat that stays. An open older than the chat's
/// last counts as decayed as it would be by now.
fn open_chat(db: &Connection, stamps: &mut Stamps, opened: &Opened) -> Result<bool> {
    let n = ensure_chat(db, stamps, &chat_jid(db, &opened.chat)?)?;
    let (was, opened_at): (f64, Option<i64>) = db
        .prepare_cached("SELECT frecency, opened_at FROM chats WHERE n = ?1")?
        .query_row([n], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let latest = opened_at.unwrap_or(opened.at).max(opened.at);
    let now = frecency(was, opened_at, latest) + frecency(1.0, Some(opened.at), latest);
    let seq = stamps.seq(CHAT, Some(n));
    db.prepare_cached("UPDATE chats SET frecency = ?2, opened_at = ?3, v = ?4 WHERE n = ?1")?
        .execute(params![n, now, latest, seq])?;
    stamps.record(db, CHAT, n, Some(n), seq)?;
    Ok(true)
}

fn delete_message(db: &Connection, stamps: &mut Stamps, chat: &str, id: &str) -> Result<bool> {
    let Some((chat, n)) = located(db, stamps, &chat_jid(db, chat)?, id)? else { return Ok(false) };
    delete_row(db, stamps, chat, n)
}

/// Deletes a message for me, leaving a tombstone so a copy arriving later doesn't bring it back.
fn delete_row(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64) -> Result<bool> {
    db.prepare_cached(
        "INSERT OR IGNORE INTO tombstones (chat, id, at) SELECT chat, id, ?2 FROM messages WHERE n = ?1",
    )?
    .execute(params![n, now_ms()])?;
    outbox::deleted_for_you(db, stamps, n)?;
    remove_row(db, stamps, chat, n)
}

/// Takes message `n` out of `chat`, with its event, so windows let it go. A chat whose newest it
/// was shows the one before it, and keeps its place.
fn remove_row(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64) -> Result<bool> {
    if db.prepare_cached("DELETE FROM messages WHERE n = ?1")?.execute([n])? == 0 {
        return Ok(false);
    }
    stamps.unsettle(chat);
    let seq = stamps.seq(MESSAGE, Some(n));
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    let seq = stamps.seq(CHAT, Some(chat));
    let shown = db
        .prepare_cached(
            "UPDATE chats SET v = ?3,
                last_message = (SELECT n FROM messages WHERE chat = ?1 ORDER BY at DESC, n DESC LIMIT 1)
            WHERE n = ?1 AND last_message = ?2",
        )?
        .execute(params![chat, n, seq])?;
    if shown > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

/// Indexes up to `limit` messages from fts_backlog for search, their text and their links' words,
/// and the library's items' and links' for its search too, the latest written first, stopping
/// early when `enough` says so, given how many it has taken. They go into FTS5 in the order of its
/// rowid, their time, since FTS5 writes out what it holds whenever a rowid comes lower than the
/// last. An item or a link found by its words now changes what a library window shows, so its
/// message's event is written, though its row is as it was. Says how many it took and whether the
/// backlog may have more.
pub(super) fn index(
    db: &Connection,
    stamps: &mut Stamps,
    limit: usize,
    mut enough: impl FnMut(usize) -> bool,
) -> Result<(usize, bool)> {
    // Whether each is an item with words, or has links, so the library's statements run only for those.
    let mut waiting: Vec<(i64, i64, i64, bool, bool)> = db
        .prepare_cached(
            "SELECT m.t, b.n, m.chat, m.shelf IS NOT NULL AND m.text IS NOT NULL,
                EXISTS (SELECT 1 FROM library_links l WHERE l.message = b.n)
            FROM (SELECT n FROM fts_backlog ORDER BY n DESC LIMIT ?1) b JOIN messages m ON m.n = b.n",
        )?
        .query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<rusqlite::Result<_>>()?;
    waiting.sort_unstable();
    let mut taken = 0;
    for &(_, n, ..) in &waiting {
        db.prepare_cached(
            "INSERT INTO messages_fts (rowid, text) SELECT t, text FROM messages WHERE n = ?1 AND text IS NOT NULL",
        )?
        .execute([n])?;
        db.prepare_cached("DELETE FROM fts_backlog WHERE n = ?1")?.execute([n])?;
        db.prepare_cached(links::INDEX_LINKS)?.execute([n])?;
        taken += 1;
        if enough(taken) {
            break;
        }
    }
    // library_fts takes links' words by ~t, so they go in newest first and before items' words,
    // which go in by t, and its rowids still only rise.
    let taken_rows = &waiting[..taken];
    let mut found = vec![0; taken];
    for (i, &(_, n, _, _, linked)) in taken_rows.iter().enumerate().rev() {
        if linked {
            found[i] += db
                .prepare_cached(
                    "INSERT OR REPLACE INTO library_fts (rowid, text, tags)
                    SELECT ~w.t, w.text, 'slink c' || m.chat || ' f' || coalesce(m.sender, iif(m.mine, 'me', 'none'))
                    FROM messages m JOIN message_link_text w ON w.message = m.n WHERE m.n = ?1 AND w.text IS NOT NULL",
                )?
                .execute([n])?;
        }
    }
    for (i, &(_, n, chat, item, _)) in taken_rows.iter().enumerate() {
        if item {
            found[i] += db
                .prepare_cached(
                    "INSERT OR REPLACE INTO library_fts (rowid, text, tags)
                    SELECT t, text, 's' || shelf || ' c' || chat || ' f' || coalesce(sender, iif(mine, 'me', 'none'))
                    FROM messages WHERE n = ?1 AND text IS NOT NULL AND shelf IS NOT NULL",
                )?
                .execute([n])?;
        }
        if found[i] > 0 {
            let seq = stamps.seq(MESSAGE, Some(n));
            stamps.record(db, MESSAGE, n, Some(chat), seq)?;
        }
    }
    Ok((taken, taken < waiting.len() || waiting.len() == limit))
}

/// A JID without its device: messages from someone's phone and their laptop are from them.
pub(super) fn user_jid(jid: &str) -> String {
    match jid.split_once('@') {
        Some((user, server @ ("s.whatsapp.net" | "lid" | "hosted" | "hosted.lid"))) => {
            let user = user.split([':', '.']).next().unwrap_or(user);
            format!("{user}@{server}")
        }
        _ => jid.to_string(),
    }
}

pub(super) fn is_person(jid: &str) -> bool {
    jid.ends_with("@s.whatsapp.net")
        || jid.ends_with("@lid")
        || jid.ends_with("@hosted")
        || jid.ends_with("@hosted.lid")
}

fn kind_of(jid: &str) -> &'static str {
    if jid.ends_with("@g.us") {
        "group"
    } else if jid.ends_with("@broadcast") {
        "broadcast"
    } else {
        "direct"
    }
}

/// The chat a JID names: its own, or the chat it's being merged into, or, when jaw has no chat by
/// that JID but has a one-to-one chat with the same person under another of their JIDs, that one,
/// so a conversation addressed by phone number and by LID stays one chat.
fn chat_jid(db: &Connection, jid: &str) -> Result<String> {
    let own: Option<String> = db
        .prepare_cached(
            "SELECT coalesce(k.id, c.id) FROM chats c LEFT JOIN chat_merges m ON m.gone = c.n
                LEFT JOIN chats k ON k.n = m.keep
            WHERE c.id = ?1",
        )?
        .query_row([jid], |r| r.get(0))
        .optional()?;
    if let Some(own) = own {
        return Ok(own);
    }
    if !is_person(jid) {
        return Ok(jid.to_string());
    }
    let other: Option<String> = db
        .prepare_cached(
            "SELECT c.id FROM person_jids j JOIN chats c ON c.person = j.person AND c.kind = 'direct'
            WHERE j.jid = ?1 AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = c.n) ORDER BY c.n LIMIT 1",
        )?
        .query_row([user_jid(jid)], |r| r.get(0))
        .optional()?;
    Ok(other.unwrap_or_else(|| jid.to_string()))
}

/// The chat's n, making the chat if jaw hasn't seen it; `jid` is already resolved by chat_jid.
fn ensure_chat(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    if let Some(n) =
        db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([jid], |r| r.get(0)).optional()?
    {
        return Ok(n);
    }
    upsert_chat(db, stamps, &Chat { id: jid.to_string(), ..Chat::default() })?;
    Ok(db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([jid], |r| r.get(0))?)
}

/// The n of `jid` in person_jids, making a person for it if jaw hasn't seen it.
fn ensure_jid(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    let jid = user_jid(jid);
    let find = "SELECT n FROM person_jids WHERE jid = ?1";
    if let Some(n) = db.prepare_cached(find)?.query_row([&jid], |r| r.get(0)).optional()? {
        return Ok(n);
    }
    let seq = stamps.seq(PERSON, None);
    let person: i64 =
        db.prepare_cached("INSERT INTO people (v) VALUES (?1) RETURNING n")?.query_row([seq], |r| r.get(0))?;
    stamps.record(db, PERSON, person, None, seq)?;
    Ok(db
        .prepare_cached("INSERT INTO person_jids (jid, person) VALUES (?1, ?2) RETURNING n")?
        .query_row(params![jid, person], |r| r.get(0))?)
}

const CHAT: &str = "chat";
const PERSON: &str = "person";
const MESSAGE: &str = "message";

/// Bumps chat `n`'s version, as what its row shows changed.
fn touch_chat(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<()> {
    let seq = stamps.seq(CHAT, Some(n));
    db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute(params![n, seq])?;
    stamps.record(db, CHAT, n, Some(n), seq)
}

/// Bumps the version of what only person `n`'s roster row shows, as that changed, leaving their
/// `v`, which message rows take. Their presence's events and their row's other events take seqs
/// apart, in either order in a transaction, so the version keeps the newer.
fn touch_roster(db: &Connection, stamps: &mut Stamps, n: i64) -> Result<()> {
    let seq = stamps.seq(PERSON, Some(n));
    db.prepare_cached("UPDATE people SET roster_v = max(roster_v, ?2) WHERE n = ?1")?.execute(params![n, seq])?;
    stamps.record(db, PERSON, n, None, seq)
}

/// What a chat's row held before an upsert, of what other rows show of it.
struct Shown {
    community: Option<String>,
    avatar: Option<String>,
    person: Option<i64>,
}

fn upsert_chat(db: &Connection, stamps: &mut Stamps, chat: &Chat) -> Result<bool> {
    let resolved;
    let chat = match chat_jid(db, &chat.id)? {
        id if id == chat.id => chat,
        id => {
            resolved = Chat { id, ..chat.clone() };
            &resolved
        }
    };
    let existing: Option<(i64, String, Shown)> = db
        .prepare_cached("SELECT n, kind, community, avatar, person FROM chats WHERE id = ?1")?
        .query_row([&chat.id], |r| {
            Ok((r.get(0)?, r.get(1)?, Shown { community: r.get(2)?, avatar: r.get(3)?, person: r.get(4)? }))
        })
        .optional()?;
    let was = existing.as_ref().map_or(kind_of(&chat.id), |(_, kind, _)| kind.as_str());
    let person = match chat.kind.unwrap_or(was) {
        "direct" if is_person(&chat.id) && (existing.is_none() || was != "direct") => {
            Some(person_of(db, stamps, &chat.id)?)
        }
        _ => None,
    };
    let ephemeral = newest_timer(db, chat, existing.as_ref().map(|(n, _, _)| *n))?;
    let seq = stamps.seq(CHAT, existing.as_ref().map(|(n, _, _)| *n));
    let changed: Option<(i64, Shown)> = db
        .prepare_cached(
            "INSERT INTO chats (id, kind, person, name, pinned, archived, muted_until, marked_unread,
                community, ephemeral, draft, v, avatar, avatar_id, ephemeral_at)
            VALUES (?1, coalesce(?2, ?13), ?3, ?4, coalesce(?5, 0), coalesce(?6, 0), coalesce(?7, 0), coalesce(?8, 0),
                nullif(?9, ''), coalesce(?10, 0), nullif(?11, ''), ?12, nullif(?14, ''), nullif(?15, ''), ?16)
            ON CONFLICT (id) DO UPDATE SET
                kind = coalesce(?2, kind), person = coalesce(?3, person), name = coalesce(?4, name),
                pinned = coalesce(?5, pinned), archived = coalesce(?6, archived),
                muted_until = coalesce(?7, muted_until), marked_unread = coalesce(?8, marked_unread),
                community = iif(?9 IS NULL, community, nullif(?9, '')), ephemeral = coalesce(?10, ephemeral),
                draft = iif(?11 IS NULL, draft, nullif(?11, '')), v = ?12,
                avatar = iif(?14 IS NULL, avatar, nullif(?14, '')), avatar_id = iif(?15 IS NULL, avatar_id, nullif(?15, ''))
            WHERE coalesce(?2, kind) IS NOT kind OR coalesce(?3, person) IS NOT person
                OR coalesce(?4, name) IS NOT name OR coalesce(?5, pinned) IS NOT pinned
                OR coalesce(?6, archived) IS NOT archived OR coalesce(?7, muted_until) IS NOT muted_until
                OR coalesce(?8, marked_unread) IS NOT marked_unread
                OR iif(?9 IS NULL, community, nullif(?9, '')) IS NOT community
                OR coalesce(?10, ephemeral) IS NOT ephemeral OR iif(?11 IS NULL, draft, nullif(?11, '')) IS NOT draft
                OR iif(?14 IS NULL, avatar, nullif(?14, '')) IS NOT avatar
                OR iif(?15 IS NULL, avatar_id, nullif(?15, '')) IS NOT avatar_id
            RETURNING n, community, avatar, person",
        )?
        .query_row(
            params![
                chat.id,
                chat.kind,
                person,
                chat.name,
                chat.pinned,
                chat.archived,
                chat.muted_until,
                chat.marked_unread,
                chat.community,
                ephemeral.map(|(seconds, _)| seconds),
                chat.draft,
                seq,
                kind_of(&chat.id),
                chat.avatar,
                chat.avatar_id,
                ephemeral.map(|(_, at)| at),
            ],
            |r| Ok((r.get(0)?, Shown { community: r.get(1)?, avatar: r.get(2)?, person: r.get(3)? })),
        )
        .optional()?;
    let Some((n, now)) = changed else { return Ok(false) };
    stamps.record(db, CHAT, n, Some(n), seq)?;
    // Marked unread or not is part of what settle writes, for it and the chats merging into it.
    if chat.marked_unread.is_some() {
        stamps.unsettle(n);
    }
    if let Some(person) = person {
        rehome(db, stamps, person)?;
    }
    let before = existing.map_or(Shown { community: None, avatar: None, person: None }, |(_, _, shown)| shown);
    // A community shows its groups' unread messages and newest message, so the one a group leaves
    // changes too; communities_follow has the one it's in now.
    if let Some(left) = before.community.filter(|left| now.community.as_ref() != Some(left)) {
        let community: Option<i64> =
            db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([left], |r| r.get(0)).optional()?;
        if let Some(community) = community {
            touch_chat(db, stamps, community)?;
        }
    }
    // Someone's roster row shows their one-to-one chat and its avatar.
    if let Some(person) = now.person.filter(|_| now.person != before.person || now.avatar != before.avatar) {
        touch_roster(db, stamps, person)?;
    }
    Ok(true)
}

/// Whether app state has said anything of index `key`: WhatsApp has sent a value for it, kept
/// whether it was applied or held back, or jaw has a patch for it on its way.
const SAID: &str = "SELECT EXISTS (SELECT 1 FROM app_state WHERE key = ?1)
    OR EXISTS (SELECT 1 FROM app_state_awaiting WHERE key = ?1)";

/// The JIDs of the person whose one-to-one chat is chat `?1`.
const THEIR_JIDS: &str =
    "SELECT j.jid FROM chats c JOIN person_jids j ON j.person = c.person WHERE c.id = ?1 AND c.kind = 'direct'";

/// The JIDs of the person whose one-to-one chat jaw keeps by `kept`, by any of which WhatsApp may
/// name the chat; none for any other chat.
pub(super) fn their_jids(db: &Connection, kept: &str) -> Result<Vec<String>> {
    Ok(db.prepare_cached(THEIR_JIDS)?.query_map([kept], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// History's `chat` less each setting app state has said anything of for it, under any of `jids`,
/// the JID jaw keeps the chat by, which jaw's own patches name, or, for a one-to-one chat, any other
/// JID of its person, by which app state may name it too.
fn unsaid(db: &Connection, chat: &Chat, jids: &[String]) -> Result<Chat> {
    let kept = chat_jid(db, &chat.id)?;
    let theirs = their_jids(db, &kept)?;
    let mut named: Vec<&str> = jids.iter().chain(&theirs).map(String::as_str).chain([kept.as_str()]).collect();
    named.sort_unstable();
    named.dedup();
    let said = |index: &str| -> Result<bool> {
        let mut said = db.prepare_cached(SAID)?;
        for &jid in &named {
            if said.query_row([serde_json::to_string(&[index, jid])?], |r| r.get(0))? {
                return Ok(true);
            }
        }
        Ok(false)
    };
    let mut chat = chat.clone();
    if chat.pinned.is_some() && said("pin_v1")? {
        chat.pinned = None;
    }
    if chat.archived.is_some() && said("archive")? {
        chat.archived = None;
    }
    if chat.muted_until.is_some() && said("mute")? {
        chat.muted_until = None;
    }
    if chat.marked_unread.is_some() && said("markChatAsRead")? {
        chat.marked_unread = None;
    }
    Ok(chat)
}

/// The timer a change to chat `n` sets, with when it was set: none when it says none, or when the
/// chat's was set later, as history's conversation can say a timer the chat has changed since. A
/// timer at least as new is taken, and its time kept here whether or not its value changes, so the
/// chat's version moves only when the timer does.
fn newest_timer(db: &Connection, chat: &Chat, n: Option<i64>) -> Result<Option<(u32, i64)>> {
    let Some(seconds) = chat.ephemeral else { return Ok(None) };
    let at = chat.ephemeral_at.unwrap_or_else(now_ms);
    let Some(n) = n else { return Ok(Some((seconds, at))) };
    let taken = db
        .prepare_cached("UPDATE chats SET ephemeral_at = ?2 WHERE n = ?1 AND ?2 >= coalesce(ephemeral_at, ?2)")?
        .execute(params![n, at])?;
    Ok((taken > 0).then_some((seconds, at)))
}

/// The person a JID belongs to, making one if needed.
fn person_of(db: &Connection, stamps: &mut Stamps, jid: &str) -> Result<i64> {
    let n = ensure_jid(db, stamps, jid)?;
    Ok(db.prepare_cached("SELECT person FROM person_jids WHERE n = ?1")?.query_row([n], |r| r.get(0))?)
}

fn upsert_person(db: &Connection, stamps: &mut Stamps, person: &Person) -> Result<bool> {
    let mut jids: Vec<String> = person.jids.iter().map(|jid| user_jid(jid)).collect();
    jids.sort();
    jids.dedup();
    if jids.is_empty() {
        bail!("a person needs at least one JID");
    }
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for jid in &jids {
        let found: Option<i64> = db
            .prepare_cached("SELECT person FROM person_jids WHERE jid = ?1")?
            .query_row([jid], |r| r.get(0))
            .optional()?;
        match found {
            Some(n) => known.push(n),
            None => unknown.push(jid),
        }
    }
    known.sort_unstable();
    known.dedup();
    let mut joined = !unknown.is_empty() || known.len() > 1;
    let keep = match known.first() {
        Some(&n) => n,
        None => {
            let seq = stamps.seq(PERSON, None);
            let n: i64 =
                db.prepare_cached("INSERT INTO people (v) VALUES (?1) RETURNING n")?.query_row([seq], |r| r.get(0))?;
            stamps.record(db, PERSON, n, None, seq)?;
            joined = true;
            n
        }
    };
    // Each JID is theirs before any join, so a sender of what a merge applies is found among them.
    for jid in unknown {
        db.prepare_cached("INSERT INTO person_jids (jid, person) VALUES (?1, ?2)")?.execute(params![jid, keep])?;
    }
    for &gone in known.iter().skip(1) {
        join(db, stamps, keep, gone)?;
    }
    if joined && !known.is_empty() {
        rehome(db, stamps, keep)?;
    }
    // What's sealed for an event between their JIDs may open now that jaw knows them.
    if joined {
        events::known_again(db, stamps, keep)?;
    }
    let seq = stamps.seq(PERSON, Some(keep));
    let changed: Option<i64> = db
        .prepare_cached(
            "UPDATE people SET name = coalesce(?2, name), push_name = coalesce(?3, push_name),
                verified_name = coalesce(?4, verified_name), v = ?5
            WHERE n = ?1 AND (?6 OR coalesce(?2, name) IS NOT name OR coalesce(?3, push_name) IS NOT push_name
                OR coalesce(?4, verified_name) IS NOT verified_name)
            RETURNING n",
        )?
        .query_row(params![keep, person.name, person.push_name, person.verified_name, seq, joined], |r| r.get(0))
        .optional()?;
    if changed.is_some() {
        stamps.record(db, PERSON, keep, None, seq)?;
    }
    Ok(changed.is_some())
}

/// Joins `gone` into `keep`: its JIDs, its chats and its folder places move, the fields `keep`
/// lacks are filled from it, and it's removed. Rows that name a JID aren't touched, which keeps a
/// join to a few rows however many messages they sent; the polls both voted in and the events both
/// answered take a new version, since their counts count the two as one from here on.
fn join(db: &Connection, stamps: &mut Stamps, keep: i64, gone: i64) -> Result<()> {
    polls::joining(db, stamps, keep, gone)?;
    events::joining(db, stamps, keep, gone)?;
    db.prepare_cached(
        "UPDATE people AS k SET name = coalesce(k.name, g.name), push_name = coalesce(k.push_name, g.push_name),
            verified_name = coalesce(k.verified_name, g.verified_name), about = coalesce(k.about, g.about),
            about_at = coalesce(k.about_at, g.about_at), avatar = coalesce(k.avatar, g.avatar),
            avatar_id = coalesce(k.avatar_id, g.avatar_id), last_seen = max(coalesce(k.last_seen, 0), coalesce(g.last_seen, 0))
        FROM (SELECT * FROM people WHERE n = ?2) AS g WHERE k.n = ?1",
    )?
    .execute(params![keep, gone])?;
    db.prepare_cached("UPDATE person_jids SET person = ?1 WHERE person = ?2")?.execute(params![keep, gone])?;
    let chats: Vec<i64> = db
        .prepare_cached("UPDATE chats SET person = ?1 WHERE person = ?2 RETURNING n")?
        .query_map(params![keep, gone], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for chat in chats {
        let seq = stamps.seq(CHAT, Some(chat));
        db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute(params![chat, seq])?;
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    let refiled = db
        .prepare_cached("UPDATE OR IGNORE folder_items SET person = ?1 WHERE person = ?2")?
        .execute(params![keep, gone])?
        + db.prepare_cached("DELETE FROM folder_items WHERE person = ?1")?.execute([gone])?;
    // Their chats show where they're filed, which may now be where the other was.
    if refiled > 0 {
        let chats: Vec<i64> = db
            .prepare_cached("SELECT n FROM chats WHERE person = ?1")?
            .query_map([keep], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for chat in chats {
            let seq = stamps.seq(CHAT, Some(chat));
            db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute(params![chat, seq])?;
            stamps.record(db, CHAT, chat, Some(chat), seq)?;
        }
    }
    db.prepare_cached("DELETE FROM people WHERE n = ?1")?.execute([gone])?;
    let seq = stamps.seq(PERSON, Some(gone));
    stamps.record(db, PERSON, gone, None, seq)?;
    merge_direct_chats(db, stamps, keep)
}

/// Starts merging a person's one-to-one chats into one, when joining their JIDs has given them
/// two: the one under their phone number is kept, or else the oldest.
fn merge_direct_chats(db: &Connection, stamps: &mut Stamps, person: i64) -> Result<()> {
    let chats: Vec<(i64, String)> = db
        .prepare_cached(
            "SELECT n, id FROM chats WHERE person = ?1 AND kind = 'direct'
                AND NOT EXISTS (SELECT 1 FROM chat_merges m WHERE m.gone = chats.n)
            ORDER BY n",
        )?
        .query_map([person], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    if chats.len() < 2 {
        return Ok(());
    }
    let keep = chats.iter().find(|(_, id)| id.ends_with("@s.whatsapp.net")).unwrap_or(&chats[0]).clone();
    for gone in chats.iter().filter(|(n, _)| *n != keep.0) {
        start_merge(db, stamps, &keep, gone)?;
    }
    Ok(())
}

/// Starts merging chat `gone` into `keep`, each a chat's n and JID. What's small moves now (its
/// tombstones, what waits for its messages, its folder places, its settings, the waits for jaw's
/// changes to them still on their way, and where it was read), and writes for it go to `keep` from
/// here on, as do the merges going into it; its messages follow a few at a time, and what's waiting
/// to be sent follows its message. Each setting of the chat kept is the newest held for it under
/// the two chats' JIDs, jaw's change still on its way counting at its time (chat_actions::settled),
/// and only one nothing is held for is the two chats' combined: pinned or muted if either is,
/// archived only if both are, marked if either is. What it kept for a message `keep` already has
/// is done to that message now, as it would be to one of its own as it moves: a deletion for you,
/// and what waited for it. Only a person's one-to-one chats merge, so sending's group tables
/// (recipients, settled, regroup) never take part.
fn start_merge(db: &Connection, stamps: &mut Stamps, keep: &(i64, String), gone: &(i64, String)) -> Result<()> {
    let ((keep, keep_id), (gone, gone_id)) = (keep, gone);
    let groups: i64 = db
        .prepare_cached("SELECT count(*) FROM chats WHERE n IN (?1, ?2) AND kind <> 'direct'")?
        .query_row([keep, gone], |r| r.get(0))?;
    if groups > 0 {
        bail!("only one-to-one chats merge, and {keep_id} or {gone_id} isn't one");
    }
    db.prepare_cached("INSERT OR REPLACE INTO chat_merges (gone, keep) VALUES (?1, ?2)")?.execute([gone, keep])?;
    db.prepare_cached("INSERT OR IGNORE INTO sync_state (key, value) VALUES (?1, '')")?
        .execute([merge_started(*gone)])?;
    let settled = chat_actions::settled(db, keep_id, gone_id)?;
    chat_actions::merging(db, gone_id, keep_id)?;
    repoint(db, stamps, (*gone, gone_id), (*keep, keep_id))?;
    carry(db, stamps, (*gone, gone_id), (*keep, keep_id))?;
    db.prepare_cached("UPDATE OR IGNORE folder_items SET chat = ?1 WHERE chat = ?2")?.execute([keep, gone])?;
    db.prepare_cached("DELETE FROM folder_items WHERE chat = ?1")?.execute([gone])?;
    // Its favourite slot comes too, unless the chat kept has one of its own.
    let slots: Vec<i64> = db
        .prepare_cached("SELECT slot FROM favourites WHERE chat = ?1")?
        .query_map([gone], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    db.prepare_cached("UPDATE OR IGNORE favourites SET chat = ?1 WHERE chat = ?2")?.execute([keep, gone])?;
    db.prepare_cached("DELETE FROM favourites WHERE chat = ?1")?.execute([gone])?;
    for slot in slots {
        let seq = stamps.seq(folders::FAVOURITE, Some(slot));
        stamps.record(db, folders::FAVOURITE, slot, None, seq)?;
    }
    // Each chat's opens decayed to the later of their last opens, so the older ones count as much
    // as they do now and no more.
    let opens = |n: i64| -> Result<(f64, Option<i64>)> {
        Ok(db
            .prepare_cached("SELECT frecency, opened_at FROM chats WHERE n = ?1")?
            .query_row([n], |r| Ok((r.get(0)?, r.get(1)?)))?)
    };
    let ((kept, kept_at), (went, went_at)) = (opens(*keep)?, opens(*gone)?);
    let latest = kept_at.max(went_at);
    let opened = latest.map_or(kept + went, |at| frecency(kept, kept_at, at) + frecency(went, went_at, at));
    let seq = stamps.seq(CHAT, Some(*keep));
    db.prepare_cached(
        "UPDATE chats AS k SET pinned = max(k.pinned, g.pinned), archived = min(k.archived, g.archived),
            muted_until = max(k.muted_until, g.muted_until), marked_unread = max(k.marked_unread, g.marked_unread),
            draft = coalesce(k.draft, g.draft), name = coalesce(k.name, g.name), avatar = coalesce(k.avatar, g.avatar),
            avatar_id = coalesce(k.avatar_id, g.avatar_id),
            ephemeral = iif((coalesce(g.ephemeral_at, -1), g.ephemeral) > (coalesce(k.ephemeral_at, -1), k.ephemeral),
                g.ephemeral, k.ephemeral),
            ephemeral_at = max(coalesce(k.ephemeral_at, g.ephemeral_at), coalesce(g.ephemeral_at, k.ephemeral_at)),
            frecency = ?4, opened_at = ?5, v = ?3
        FROM (SELECT * FROM chats WHERE n = ?2) AS g WHERE k.n = ?1",
    )?
    .execute(params![keep, gone, seq, opened, latest])?;
    stamps.record(db, CHAT, *keep, Some(*keep), seq)?;
    if let Some(settled) = settled {
        upsert_chat(db, stamps, &settled)?;
    }
    merge_reads(db, stamps, *keep, *gone)
}

/// Where sync_state keeps that this jawd started merging chat `gone` away, until the merge ends.
/// Once this jawd starts merging a chat, what's said of it goes to the chat it goes into and none
/// of its settings change, so the merge's last step merges none of them into the kept chat again,
/// which would undo what was done to that chat meanwhile (a draft sent, a pin taken off). A merge
/// an earlier jawd started has no such row: through a chain of merges, what was said of another of
/// the person's chats may have landed on its chat. A chain takes three of a person's chats, so
/// only a person with three JIDs or more has its settings merged again, and for them, what was
/// undone on the kept chat while the merge ran (a pin, a mute or an archive, the mark, a draft
/// sent, and a disappearing timer set before chats kept when theirs was set) comes back once.
fn merge_started(gone: i64) -> String {
    format!("merge.started.{gone}")
}

/// Moves what chat `gone` holds for messages it hasn't got into `keep`, each a chat's n and JID:
/// the tombstones of messages deleted for you, and what waits for its messages, in pending or set
/// aside, which `follow` moves. What's for a message `keep` already has is done to it now, since
/// that message won't arrive or move again to have it done: what waited first, then the deletions,
/// so nothing is left waiting for a message deleted for you. What's for one still in a chat merging
/// into `keep` is done as it moves.
fn carry(db: &Connection, stamps: &mut Stamps, gone: (i64, &str), keep: (i64, &str)) -> Result<()> {
    let deleted: Vec<String> = db
        .prepare_cached("UPDATE OR IGNORE tombstones SET chat = ?1 WHERE chat = ?2 RETURNING id")?
        .query_map([keep.0, gone.0], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    db.prepare_cached("DELETE FROM tombstones WHERE chat = ?1")?.execute([gone.0])?;
    follow(db, stamps, gone.1, keep)?;
    for id in &deleted {
        if let Some(n) = message_in(db, keep.0, id)? {
            delete_row(db, stamps, keep.0, n)?;
        }
    }
    Ok(())
}

/// The n of message `id` in chat `chat`, if it's there.
fn message_in(db: &Connection, chat: i64, id: &str) -> Result<Option<i64>> {
    Ok(db
        .prepare_cached("SELECT n FROM messages WHERE chat = ?1 AND id = ?2")?
        .query_row(params![chat, id], |r| r.get(0))
        .optional()?)
}

/// Points the merges going into chat `from`, which is going itself, at `to`, the chat it goes
/// into, each a chat's n and JID, so no chat is both kept and going, and every lookup that follows
/// one merge reaches the chat that stays. Each merging chat's read position is merged into `to`'s,
/// as if it had gone there from the start, and a failed step's quarantine row is named by where
/// its merge goes now.
fn repoint(db: &Connection, stamps: &mut Stamps, from: (i64, &str), to: (i64, &str)) -> Result<()> {
    let merging: Vec<i64> = db
        .prepare_cached("UPDATE chat_merges SET keep = ?1 WHERE keep = ?2 RETURNING gone")?
        .query_map([to.0, from.0], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for gone in merging {
        let gone_id: String =
            db.prepare_cached("SELECT id FROM chats WHERE n = ?1")?.query_row([gone], |r| r.get(0))?;
        db.prepare_cached("UPDATE OR REPLACE quarantine SET ref = ?1 WHERE source = 'merge' AND ref = ?2")?
            .execute([merge_reference(&gone_id, to.1), merge_reference(&gone_id, from.1)])?;
        merge_reads(db, stamps, to.0, gone)?;
    }
    Ok(())
}

/// How merge `gone` is named in quarantine, while it's under way.
fn merge_named(db: &Connection, gone: i64) -> Result<Option<String>> {
    Ok(db
        .prepare_cached(
            "SELECT g.id, k.id FROM chat_merges m JOIN chats g ON g.n = m.gone JOIN chats k ON k.n = m.keep
            WHERE m.gone = ?1",
        )?
        .query_row([gone], |r| Ok(merge_reference(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?)))
        .optional()?)
}

/// Has what waits under JID `from` for messages jaw hasn't seen there, in `pending` or set aside,
/// wait in chat `to` (its n and JID) instead, as what's addressed to `from` goes there now; and
/// does to the messages `to` already has what waited for them, as it would have been done had it
/// come to `to` in the first place.
fn follow(db: &Connection, stamps: &mut Stamps, from: &str, to: (i64, &str)) -> Result<()> {
    let (to, to_jid) = to;
    let mut waiting: Vec<String> = db
        .prepare_cached("UPDATE pending SET chat = ?1 WHERE chat = ?2 RETURNING target")?
        .query_map([to_jid, from], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    waiting.extend(polls::set_aside_moved(db, to_jid, from)?);
    waiting.extend(events::set_aside_moved(db, to_jid, from)?);
    waiting.sort_unstable();
    waiting.dedup();
    for id in waiting {
        let found: Option<(i64, String)> = db
            .prepare_cached("SELECT n, kind FROM messages WHERE chat = ?1 AND id = ?2")?
            .query_row(params![to, id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((n, kind)) = found {
            apply_pending(db, stamps, to, n, to_jid, &id, &kind)?;
        }
    }
    Ok(())
}

/// Has what waits under each JID of `person` that names no chat of its own follow it to the chat
/// it leads to now, the person's one-to-one chat, as joining their JIDs or making that chat
/// changes where it leads.
fn rehome(db: &Connection, stamps: &mut Stamps, person: i64) -> Result<()> {
    let chatless: Vec<String> = db
        .prepare_cached(
            "SELECT j.jid FROM person_jids j WHERE j.person = ?1 AND NOT EXISTS (SELECT 1 FROM chats c WHERE c.id = j.jid)",
        )?
        .query_map([person], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for jid in chatless {
        let to = chat_jid(db, &jid)?;
        if to == jid {
            continue;
        }
        let n: Option<i64> =
            db.prepare_cached("SELECT n FROM chats WHERE id = ?1")?.query_row([&to], |r| r.get(0)).optional()?;
        if let Some(n) = n {
            follow(db, stamps, &jid, (n, &to))?;
        }
    }
    Ok(())
}

/// The messages a merge step moves.
const MERGE_STEP: i64 = 200;

/// Moves up to MERGE_STEP messages of the first chat being merged into the chat it's merged into,
/// folding a message both have into one by the usual rules. Both chats then show their newest
/// message, so what watches either is told its messages moved, and once none are left the other
/// chat is removed. Says whether anything changed.
fn merge_step(db: &Connection, stamps: &mut Stamps, gone: Option<i64>) -> Result<bool> {
    let found: Option<(i64, i64)> = match gone {
        Some(gone) => db
            .prepare_cached("SELECT gone, keep FROM chat_merges WHERE gone = ?1")?
            .query_row([gone], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?,
        None => db
            .prepare_cached("SELECT gone, keep FROM chat_merges ORDER BY gone LIMIT 1")?
            .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?,
    };
    let Some((gone, keep)) = found else { return Ok(false) };
    // Where the chat going was read is merged into the kept chat's before each step moves its
    // messages, since what's owed is found among the messages still there: an earlier jawd let
    // messages reach a chat after its merge began, through a chain of merges.
    merge_reads(db, stamps, keep, gone)?;
    let moving: Vec<i64> = db
        .prepare_cached("SELECT n FROM messages WHERE chat = ?1 ORDER BY at, n LIMIT ?2")?
        .query_map(params![gone, MERGE_STEP], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for &n in &moving {
        move_merged(db, stamps, gone, keep, n)?;
    }
    show_newest(db, stamps, gone)?;
    show_newest(db, stamps, keep)?;
    // Oldest first, so the chat being merged keeps its newest until the last step.
    if moving.len() as i64 == MERGE_STEP {
        return Ok(true);
    }
    let (gone_jid, keep_jid): (String, String) = db
        .prepare_cached("SELECT g.id, k.id FROM chats g, chats k WHERE g.n = ?1 AND k.n = ?2")?
        .query_row([gone, keep], |r| Ok((r.get(0)?, r.get(1)?)))?;
    db.prepare_cached("DELETE FROM quarantine WHERE source = 'merge' AND ref = ?1")?
        .execute([merge_reference(&gone_jid, &keep_jid)])?;
    // Anything written to the gone chat by its n since the merge began, such as how far WhatsApp
    // has been told, is kept before the chat goes.
    merge_reads(db, stamps, keep, gone)?;
    // So are merges still going into it, which a jaw.db from before repoint can hold, and the
    // deletions and what waits that were sent there while they did.
    repoint(db, stamps, (gone, &gone_jid), (keep, &keep_jid))?;
    carry(db, stamps, (gone, &gone_jid), (keep, &keep_jid))?;
    let started = db.prepare_cached("DELETE FROM sync_state WHERE key = ?1")?.execute([merge_started(gone)])? > 0;
    if !started && could_chain(db, keep)? {
        merge_settings_again(db, stamps, (keep, &keep_jid), (gone, &gone_jid))?;
    }
    db.prepare_cached("DELETE FROM chat_merges WHERE gone = ?1")?.execute([gone])?;
    db.prepare_cached("DELETE FROM chats WHERE n = ?1")?.execute([gone])?;
    let seq = stamps.seq(CHAT, Some(gone));
    stamps.record(db, CHAT, gone, Some(gone), seq)?;
    Ok(true)
}

/// Whether chat `keep`'s person has three JIDs or more, as a chain of merges through chats of
/// theirs needs: with fewer, nothing said of another of their chats can have landed on a chat
/// merging into `keep`.
fn could_chain(db: &Connection, keep: i64) -> Result<bool> {
    let jids: i64 = db
        .prepare_cached("SELECT count(*) FROM person_jids WHERE person = (SELECT person FROM chats WHERE n = ?1)")?
        .query_row([keep], |r| r.get(0))?;
    Ok(jids >= 3)
}

/// Merges chat `gone`'s settings into `keep`'s again, by start_merge's rules, as a merge an earlier
/// jawd started for a person with three JIDs or more ends: a draft or a pin for another of the
/// person's chats may have landed on `gone` since, through a chain of merges, and a draft.set that
/// was answered is never lost. Its frecency, which start_merge added to `keep`'s, isn't added twice.
fn merge_settings_again(db: &Connection, stamps: &mut Stamps, keep: (i64, &str), gone: (i64, &str)) -> Result<()> {
    let settled = chat_actions::settled(db, keep.1, gone.1)?;
    let ((keep, _), (gone, _)) = (keep, gone);
    let seq = stamps.seq(CHAT, Some(keep));
    let changed = db
        .prepare_cached(
            "UPDATE chats AS k SET pinned = max(k.pinned, g.pinned), archived = min(k.archived, g.archived),
                muted_until = max(k.muted_until, g.muted_until), marked_unread = max(k.marked_unread, g.marked_unread),
                draft = coalesce(k.draft, g.draft), name = coalesce(k.name, g.name), avatar = coalesce(k.avatar, g.avatar),
                avatar_id = coalesce(k.avatar_id, g.avatar_id),
                ephemeral = iif((coalesce(g.ephemeral_at, -1), g.ephemeral) > (coalesce(k.ephemeral_at, -1), k.ephemeral),
                    g.ephemeral, k.ephemeral),
                ephemeral_at = max(coalesce(k.ephemeral_at, g.ephemeral_at), coalesce(g.ephemeral_at, k.ephemeral_at)),
                opened_at = max(coalesce(k.opened_at, g.opened_at), coalesce(g.opened_at, k.opened_at)), v = ?3
            FROM (SELECT * FROM chats WHERE n = ?2) AS g
            WHERE k.n = ?1 AND (max(k.pinned, g.pinned), min(k.archived, g.archived), max(k.muted_until, g.muted_until),
                max(k.marked_unread, g.marked_unread), coalesce(k.draft, g.draft), coalesce(k.name, g.name),
                coalesce(k.avatar, g.avatar), coalesce(k.avatar_id, g.avatar_id),
                iif((coalesce(g.ephemeral_at, -1), g.ephemeral) > (coalesce(k.ephemeral_at, -1), k.ephemeral),
                    g.ephemeral, k.ephemeral),
                max(coalesce(k.opened_at, g.opened_at), coalesce(g.opened_at, k.opened_at)))
                IS NOT (k.pinned, k.archived, k.muted_until, k.marked_unread, k.draft, k.name, k.avatar, k.avatar_id,
                k.ephemeral, k.opened_at)",
        )?
        .execute(params![keep, gone, seq])?;
    if changed > 0 {
        stamps.record(db, CHAT, keep, Some(keep), seq)?;
        // Marked unread or not is part of what settle writes, for it and the chats merging into it.
        stamps.unsettle(keep);
    }
    if let Some(settled) = settled {
        upsert_chat(db, stamps, &settled)?;
    }
    Ok(())
}

/// Whether jaw.db holds a chain of merges, which an earlier jawd made: a chat kept by one merge
/// that's being merged into another itself.
pub(super) fn chained(db: &Connection) -> Result<bool> {
    Ok(middle(db)?.is_some())
}

/// The first chat kept by one merge and being merged into another itself, and the chat it goes into.
fn middle(db: &Connection) -> Result<Option<(i64, i64)>> {
    Ok(db
        .prepare_cached(
            "SELECT m.gone, m.keep FROM chat_merges m
            WHERE m.gone > 0 AND EXISTS (SELECT 1 FROM chat_merges i WHERE i.keep = m.gone) ORDER BY m.gone LIMIT 1",
        )?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// Points every merge in a chain at the chat the chain ends in, before the writer takes any
/// change. An earlier jawd let a chat kept by one merge start merging into another, and until the
/// chat in the middle went, a write for the first chat's JID stopped at it, one merge on, as
/// everything that finds a chat through chat_merges follows one: it landed on a chat about to be
/// removed, and reading the chat that stays didn't read the first. Each merge going into a chat in
/// the middle goes into the chat that stays, as start_merge now leaves it, with where its chat was
/// read merged into that one's. Says how many chats were in the middle of a chain. Each hop along
/// a chain takes another merge, so a walk still going once it has taken as many as there are has
/// gone round in a circle, which no jawd makes, and jaw.db is refused, saying how to open it.
pub(super) fn flatten(db: &Connection, stamps: &mut Stamps) -> Result<usize> {
    let merges: usize =
        db.prepare_cached("SELECT count(*) FROM chat_merges")?.query_row([], |r| r.get::<_, i64>(0))?.try_into()?;
    let mut flattened = 0;
    while let Some((chat, next)) = middle(db)? {
        let mut top = next;
        for hop in 0.. {
            let Some(further) = goes_into(db, top)? else { break };
            if hop == merges {
                bail!(circle(db, top, merges)?);
            }
            top = further;
        }
        repoint(db, stamps, (chat, &chat_id(db, chat)?), (top, &chat_id(db, top)?))?;
        flattened += 1;
    }
    Ok(flattened)
}

/// The chat that chat `gone` is being merged into, if it's being merged.
fn goes_into(db: &Connection, gone: i64) -> Result<Option<i64>> {
    Ok(db
        .prepare_cached("SELECT gone, keep FROM chat_merges WHERE gone = ?1")?
        .query_row([gone], |r| r.get(1))
        .optional()?)
}

fn chat_id(db: &Connection, n: i64) -> Result<String> {
    Ok(db.prepare_cached("SELECT id FROM chats WHERE n = ?1")?.query_row([n], |r| r.get(0))?)
}

/// What to say of the merges that go round in a circle through chat `on`, of `merges` in all: each
/// by its chats' JIDs, and the one to delete for jaw.db to open, that of the chat a merge would
/// keep, a phone number's or else the oldest, so the others end in it.
fn circle(db: &Connection, on: i64, merges: usize) -> Result<String> {
    let mut round = vec![on];
    while round.len() < merges {
        match goes_into(db, round[round.len() - 1])? {
            Some(next) if next != on => round.push(next),
            _ => break,
        }
    }
    let ids = round.iter().map(|&n| chat_id(db, n)).collect::<Result<Vec<_>>>()?;
    let kept = (0..round.len()).min_by_key(|&i| (!ids[i].ends_with("@s.whatsapp.net"), round[i])).unwrap_or(0);
    if round.len() == 1 {
        return Ok(format!(
            "the merge of {} into itself goes round in a circle, which no jawd makes; deleting it \
             (DELETE FROM chat_merges WHERE gone = {on}) lets jaw.db open",
            ids[0]
        ));
    }
    let each: Vec<String> = (0..round.len()).map(|i| format!("{} into {}", ids[i], ids[(i + 1) % ids.len()])).collect();
    let (last, rest) = each.split_last().expect("a circle of two chats or more");
    Ok(format!(
        "the merges of {} and {last} go round in a circle, which no jawd makes; deleting the merge of {} \
         (DELETE FROM chat_merges WHERE gone = {}) lets jaw.db open, the others then ending in {}'s chat",
        rest.join(", "),
        each[kept],
        round[kept],
        ids[kept]
    ))
}

/// Has chat `chat` show its newest message, and sort by it, as its rows now are: for the two chats
/// of a merge, whose messages move between them.
fn show_newest(db: &Connection, stamps: &mut Stamps, chat: i64) -> Result<()> {
    let seq = stamps.seq(CHAT, Some(chat));
    db.prepare_cached(
        "UPDATE chats SET last_message = (SELECT n FROM messages WHERE chat = ?1 ORDER BY at DESC, n DESC LIMIT 1),
            last_at = coalesce((SELECT at FROM messages WHERE chat = ?1 ORDER BY at DESC, n DESC LIMIT 1), 0), v = ?2
        WHERE n = ?1",
    )?
    .execute(params![chat, seq])?;
    stamps.record(db, CHAT, chat, Some(chat), seq)
}

/// Moves message `n` of chat `gone`, being merged, into `keep`, as what's done to it there says:
/// one deleted for me there is deleted, one the kept chat has too is folded into that, and any
/// other moves; then what waited there for it is applied. Says where it is now, if anywhere.
fn move_merged(db: &Connection, stamps: &mut Stamps, gone: i64, keep: i64, n: i64) -> Result<Option<i64>> {
    let (id, kind): (String, String) = db
        .prepare_cached("SELECT id, kind FROM messages WHERE n = ?1")?
        .query_row([n], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if db.prepare_cached("SELECT 1 FROM tombstones WHERE chat = ?1 AND id = ?2")?.exists(params![keep, id])? {
        delete_row(db, stamps, gone, n)?;
        return Ok(None);
    }
    let kept: Option<i64> = db
        .prepare_cached("SELECT n FROM messages WHERE chat = ?1 AND id = ?2")?
        .query_row(params![keep, id], |r| r.get(0))
        .optional()?;
    let n = match kept {
        Some(kept) => {
            fold(db, stamps, keep, kept, n)?;
            kept
        }
        None => {
            let seq = stamps.seq(MESSAGE, Some(n));
            db.prepare_cached("UPDATE messages SET chat = ?2, v = ?3 WHERE n = ?1")?.execute(params![n, keep, seq])?;
            stamps.record(db, MESSAGE, n, Some(keep), seq)?;
            n
        }
    };
    stamps.unsettle(keep);
    stamps.unsettle(gone);
    let keep_jid: String = db.prepare_cached("SELECT id FROM chats WHERE n = ?1")?.query_row([keep], |r| r.get(0))?;
    apply_pending(db, stamps, keep, n, &keep_jid, &id, &kind)?;
    Ok(db.prepare_cached("SELECT 1 FROM messages WHERE n = ?1")?.exists([n])?.then_some(n))
}

/// Whether a chat is being merged into chat `keep`.
fn merging_into(db: &Connection, keep: i64) -> Result<bool> {
    Ok(db.prepare_cached("SELECT 1 FROM chat_merges WHERE keep = ?1")?.exists([keep])?)
}

/// Moves message `id` into chat `keep` now if it's still in a chat being merged into it, so what
/// names it finds it where it stays rather than where it's going. Says where it is now, if
/// anywhere.
fn moved_from_merging(db: &Connection, stamps: &mut Stamps, keep: i64, id: &str) -> Result<Option<i64>> {
    let found: Option<(i64, i64)> = db
        .prepare_cached(
            "SELECT m.chat, m.n FROM chat_merges c JOIN messages m ON m.chat = c.gone AND m.id = ?2 WHERE c.keep = ?1 LIMIT 1",
        )?
        .query_row(params![keep, id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((gone, n)) = found else { return Ok(None) };
    // Where the chat going was read is merged into the kept chat's before the message moves, as
    // before a step's, since what's owed for it is found only while it's still there.
    merge_reads(db, stamps, keep, gone)?;
    let moved = move_merged(db, stamps, gone, keep, n)?;
    show_newest(db, stamps, gone)?;
    show_newest(db, stamps, keep)?;
    Ok(moved)
}

/// The chat and n of message `id` in the chat `chat_jid` (already resolved by chat_jid), moving it
/// there first from a chat being merged into it.
fn located(db: &Connection, stamps: &mut Stamps, chat_jid: &str, id: &str) -> Result<Option<(i64, i64)>> {
    let found: Option<(i64, Option<i64>)> = db
        .prepare_cached(
            "SELECT c.n, m.n FROM chats c LEFT JOIN messages m ON m.chat = c.n AND m.id = ?2 WHERE c.id = ?1",
        )?
        .query_row(params![chat_jid, id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(match found {
        None => None,
        Some((chat, Some(n))) => Some((chat, n)),
        Some((chat, None)) => moved_from_merging(db, stamps, chat, id)?.map(|n| (chat, n)),
    })
}

/// Folds message `gone` into `kept`, the same message in the chat being kept: a later edit, a star,
/// the live envelope, a raw protobuf, its reactions, receipts, votes, answers and media go to the
/// kept one, a deletion for everyone sticks, and `gone` is removed.
fn fold(db: &Connection, stamps: &mut Stamps, chat: i64, kept: i64, gone: i64) -> Result<()> {
    let seq = stamps.seq(MESSAGE, Some(kept));
    // An event's edit says where and when as well as what it's called. A kept copy deleted for
    // everyone takes none of the other's words, nor its raw protobuf, which the deletion cleared.
    db.prepare_cached(
        "UPDATE messages AS k SET
            text = iif(NOT k.deleted AND g.edited_at > coalesce(k.edited_at, -1), g.text, k.text),
            spans = iif(NOT k.deleted AND g.edited_at > coalesce(k.edited_at, -1), g.spans, k.spans),
            summary = iif(NOT k.deleted AND g.edited_at > coalesce(k.edited_at, -1), g.summary, k.summary),
            detail = iif(k.kind = 'event' AND NOT k.deleted AND g.edited_at > coalesce(k.edited_at, -1), g.detail,
                k.detail),
            edited_at = max(coalesce(k.edited_at, g.edited_at), coalesce(g.edited_at, k.edited_at)),
            status = iif(coalesce(?4 ->> g.status, -1) > coalesce(?4 ->> k.status, -1), g.status, k.status),
            starred = max(k.starred, g.starred), mentions_me = max(k.mentions_me, g.mentions_me),
            raw = iif(k.deleted, k.raw, coalesce(k.raw, g.raw)),
            envelope = iif(k.deleted, k.envelope, coalesce(k.envelope, g.envelope)), v = ?3
        FROM (SELECT * FROM messages WHERE n = ?2) AS g WHERE k.n = ?1",
    )?
    .execute(params![kept, gone, seq, STATUS_RANKS])?;
    stamps.record(db, MESSAGE, kept, Some(chat), seq)?;
    db.prepare_cached(
        "INSERT INTO reactions (message, sender, emoji, at, mine) SELECT ?1, sender, emoji, at, mine FROM reactions
            WHERE message = ?2
        ON CONFLICT (message, sender) DO UPDATE SET emoji = excluded.emoji, at = excluded.at, mine = excluded.mine
            WHERE excluded.at > reactions.at",
    )?
    .execute([kept, gone])?;
    db.prepare_cached(
        "INSERT INTO receipts (message, recipient, delivered_at, read_at, played_at)
            SELECT ?1, recipient, delivered_at, read_at, played_at FROM receipts WHERE message = ?2
        ON CONFLICT (message, recipient) DO UPDATE SET delivered_at = coalesce(receipts.delivered_at, excluded.delivered_at),
            read_at = coalesce(receipts.read_at, excluded.read_at), played_at = coalesce(receipts.played_at, excluded.played_at)",
    )?
    .execute([kept, gone])?;
    // Each voter's vote in the copy goes to the kept one if it's newer than theirs there.
    db.prepare_cached(
        "INSERT OR IGNORE INTO poll_options (message, position, name, hash)
            SELECT ?1, position, name, hash FROM poll_options WHERE message = ?2",
    )?
    .execute([kept, gone])?;
    db.prepare_cached(
        "DELETE FROM poll_votes WHERE message = ?1 AND voter IN (SELECT g.voter FROM poll_voters g
            LEFT JOIN poll_voters k ON k.message = ?1 AND k.voter = g.voter
            WHERE g.message = ?2 AND (k.voter IS NULL OR (g.at, g.id) > (k.at, k.id)))",
    )?
    .execute([kept, gone])?;
    db.prepare_cached(
        "INSERT OR IGNORE INTO poll_votes (message, voter, position, at)
            SELECT ?1, v.voter, v.position, v.at FROM poll_votes v
            JOIN poll_voters g ON g.message = v.message AND g.voter = v.voter
            LEFT JOIN poll_voters k ON k.message = ?1 AND k.voter = v.voter
            WHERE v.message = ?2 AND (k.voter IS NULL OR (g.at, g.id) > (k.at, k.id))",
    )?
    .execute([kept, gone])?;
    db.prepare_cached(
        "INSERT INTO poll_voters (message, voter, at, id) SELECT ?1, voter, at, id FROM poll_voters WHERE message = ?2
        ON CONFLICT (message, voter) DO UPDATE SET at = excluded.at, id = excluded.id
            WHERE (excluded.at, excluded.id) > (poll_voters.at, poll_voters.id)",
    )?
    .execute([kept, gone])?;
    // Each answer to the copy, an event's, goes to the kept one if it's newer than its answerer's
    // there, before removing the copy takes its answers with it.
    db.prepare_cached(
        "INSERT INTO event_answers (message, who, answer, guests, at, id)
            SELECT ?1, who, answer, guests, at, id FROM event_answers WHERE message = ?2
        ON CONFLICT (message, who) DO UPDATE SET answer = excluded.answer, guests = excluded.guests,
            at = excluded.at, id = excluded.id
            WHERE (excluded.at, excluded.id) > (event_answers.at, event_answers.id)",
    )?
    .execute([kept, gone])?;
    media::fold(db, kept, gone)?;
    outbox::folded(db, stamps, kept, gone)?;
    let revoked: bool =
        db.prepare_cached("SELECT deleted FROM messages WHERE n = ?1")?.query_row([gone], |r| r.get(0))?;
    db.prepare_cached("DELETE FROM messages WHERE n = ?1")?.execute([gone])?;
    let seq = stamps.seq(MESSAGE, Some(gone));
    stamps.record(db, MESSAGE, gone, Some(chat), seq)?;
    if revoked {
        apply_amendment(db, stamps, chat, kept, Doing { what: &Amendment::Revoke, sender: None, mine: false, at: 0 })?;
    }
    // The send that went to the kept copy may be one nothing can send again.
    outbox::tidied(db, stamps, kept)
}

/// How many bits of a message's t count messages within one millisecond.
pub(super) const SAME_MS_BITS: u32 = 20;
/// The latest time a message's t can hold, in the year 2248.
pub(super) const LAST_MS: i64 = (1 << (63 - SAME_MS_BITS)) - 1;

/// A free t for a message sent at `at`: its millisecond, then the next count free in it.
fn allocate(db: &Connection, at: i64) -> Result<i64> {
    let first = at.clamp(0, LAST_MS) << SAME_MS_BITS;
    let last = first | ((1 << SAME_MS_BITS) - 1);
    let taken: Option<i64> = db
        .prepare_cached("SELECT t FROM messages WHERE t BETWEEN ?1 AND ?2 ORDER BY t DESC LIMIT 1")?
        .query_row(params![first, last], |r| r.get(0))
        .optional()?;
    match taken {
        None => Ok(first),
        Some(n) if n < last => Ok(n + 1),
        Some(_) => bail!("more than a million messages are stamped with the same millisecond, {at}"),
    }
}

/// What a message row already holds that a message arriving again mustn't undo: its deletion, its
/// edit and its star.
struct Kept {
    n: i64,
    t: i64,
    deleted: bool,
    edited_at: Option<i64>,
    starred: bool,
    text: Option<String>,
    spans: Option<String>,
    summary: Option<String>,
    status: Option<String>,
    detail: Option<String>,
}

/// status_rank as JSON, for SQL to rank statuses by with `->>`; a missing one ranks lowest.
const STATUS_RANKS: &str = r#"{"pending": 0, "failed": 1, "sent": 2, "delivered": 3, "read": 4, "played": 5}"#;

/// How far a message of yours has got: a status never goes back, and `failed` replaces only
/// `pending`, as a send that succeeds after failing replaces `failed`.
fn status_rank(status: Option<&str>) -> i8 {
    match status {
        None => -1,
        Some("pending") => 0,
        Some("failed") => 1,
        Some("sent") => 2,
        Some("delivered") => 3,
        Some("read") => 4,
        Some("played") => 5,
        Some(_) => -1,
    }
}

/// The status a message keeps when `incoming` arrives for it with `kept` already stored.
fn advanced<'a>(kept: Option<&'a str>, incoming: Option<&'a str>) -> Option<&'a str> {
    if status_rank(incoming) > status_rank(kept) { incoming } else { kept }
}

fn upsert_message(db: &Connection, stamps: &mut Stamps, message: &Message) -> Result<bool> {
    let chat_id = chat_jid(db, &message.chat)?;
    let chat = ensure_chat(db, stamps, &chat_id)?;
    if db.prepare_cached("SELECT 1 FROM tombstones WHERE chat = ?1 AND id = ?2")?.exists(params![chat, message.id])? {
        return Ok(false);
    }
    moved_from_merging(db, stamps, chat, &message.id)?;
    let sender = message.sender.as_deref().map(|jid| ensure_jid(db, stamps, jid)).transpose()?;
    let existing: Option<Kept> = db
        .prepare_cached(
            "SELECT n, t, deleted, edited_at, starred, text, spans, summary, status, detail FROM messages
            WHERE chat = ?1 AND id = ?2",
        )?
        .query_row(params![chat, message.id], |r| {
            Ok(Kept {
                n: r.get(0)?,
                t: r.get(1)?,
                deleted: r.get(2)?,
                edited_at: r.get(3)?,
                starred: r.get(4)?,
                text: r.get(5)?,
                spans: r.get(6)?,
                summary: r.get(7)?,
                status: r.get(8)?,
                detail: r.get(9)?,
            })
        })
        .optional()?;
    if existing.as_ref().is_some_and(|kept| kept.deleted) {
        return Ok(false);
    }
    // One whose time came before it did, as a catch-up can bring, is never shown: it's erased as
    // it would have been, leaving its tombstone, and what waited for it goes with it.
    if existing.is_none() && message.expires_at.is_some_and(|at| at <= now_ms()) {
        db.prepare_cached(TOMBSTONE)?.execute(params![chat, message.id, now_ms()])?;
        db.prepare_cached(PENDING_FOR)?.execute(params![chat_id, message.id])?;
        return Ok(false);
    }
    let seq = stamps.seq(MESSAGE, existing.as_ref().map(|kept| kept.n));
    let raw = message.raw.as_ref().map(|packed| packed.0.as_slice());
    let mut text = message.text.clone();
    let mut spans = message.spans.clone();
    let mut summary = (message.text.as_ref() != Some(&message.summary)).then(|| message.summary.clone());
    let mut edited_at = message.edited_at;
    let mut detail = message.detail.clone();
    let mut starred = message.starred;
    let mut status = message.status.map(str::to_string);
    if let Some(kept) = &existing {
        status = advanced(kept.status.as_deref(), status.as_deref()).map(str::to_string);
        // An edit wins by its time, and a copy without one is older than any.
        if kept.edited_at > message.edited_at {
            (text, spans, summary, edited_at) =
                (kept.text.clone(), kept.spans.clone(), kept.summary.clone(), kept.edited_at);
            // An event's edit says where and when as well as what it's called.
            if message.kind == "event" {
                detail = kept.detail.clone();
            }
        }
        // Stars come and go through app state, which a copy of the message doesn't know about.
        starred = kept.starred;
    }
    let (n, t) = match &existing {
        Some(kept) => (Some(kept.n), kept.t),
        None => {
            let t = allocate(db, message.at)?;
            // Its text, and its links' words, are indexed for search in the writer's idle
            // moments, live or history, since FTS5 does most of its work at commit and an ack or a
            // live write may be waiting on this one. Its n is chosen here so it can wait in
            // fts_backlog before the insert's triggers look; AUTOINCREMENT's sequence is past every
            // n ever used, so it's one no event has named.
            let linked = message.detail.as_deref().is_some_and(|detail| detail.contains("\"link\":{"));
            let n = if text.is_some() || linked {
                let n: i64 = db
                    .prepare_cached(
                        "SELECT coalesce((SELECT seq FROM sqlite_sequence WHERE name = 'messages'), 0) + 1",
                    )?
                    .query_row([], |r| r.get(0))?;
                db.prepare_cached("INSERT INTO fts_backlog (n) VALUES (?1)")?.execute([n])?;
                Some(n)
            } else {
                None
            };
            (n, t)
        }
    };
    let envelope = message.envelope.as_ref().map(|packed| packed.0.as_slice());
    let fields = params![
        message.at,
        sender,
        message.mine,
        message.kind,
        status,
        text,
        spans,
        summary,
        message.reply,
        detail,
        message.forwarded,
        edited_at,
        message.deleted,
        message.expires_at,
        starred,
        message.view_once,
        raw,
        seq,
        n,
        message.mentions_me,
        envelope,
        chat,
        message.id,
        t,
    ];
    // A live copy keeps the raw protobuf a history copy brought, which may say more, and its
    // envelope is kept whatever comes after.
    let changed: Option<i64> = if existing.is_some() {
        db.prepare_cached(
            "UPDATE messages SET at = ?1, sender = ?2, mine = ?3, kind = ?4, status = ?5, text = ?6, spans = ?7,
                summary = ?8, reply = ?9, detail = ?10, forwarded = ?11, edited_at = ?12, deleted = ?13,
                expires_at = iif(?14 < coalesce(expires_at, ?14 + 1), ?14, expires_at), starred = ?15,
                view_once = max(view_once, ?16),
                raw = iif(?17 IS NULL OR ?21 IS NOT NULL AND raw IS NOT NULL, raw, ?17), v = ?18,
                mentions_me = ?20, envelope = coalesce(envelope, ?21)
            WHERE n = ?19 AND (at IS NOT ?1 OR sender IS NOT ?2 OR mine IS NOT ?3 OR kind IS NOT ?4
                OR status IS NOT ?5 OR text IS NOT ?6 OR spans IS NOT ?7 OR summary IS NOT ?8 OR reply IS NOT ?9
                OR detail IS NOT ?10 OR forwarded IS NOT ?11 OR edited_at IS NOT ?12 OR deleted IS NOT ?13
                OR iif(?14 < coalesce(expires_at, ?14 + 1), ?14, expires_at) IS NOT expires_at OR starred IS NOT ?15
                OR max(view_once, ?16) IS NOT view_once
                OR iif(?17 IS NULL OR ?21 IS NOT NULL AND raw IS NOT NULL, raw, ?17) IS NOT raw
                OR mentions_me IS NOT ?20 OR coalesce(envelope, ?21) IS NOT envelope)
            RETURNING n",
        )?
        .query_row(&fields[..21], |r| r.get(0))
        .optional()?
    } else {
        Some(
            db.prepare_cached(
                "INSERT INTO messages (at, sender, mine, kind, status, text, spans, summary, reply, detail, forwarded,
                    edited_at, deleted, expires_at, starred, view_once, raw, v, n, mentions_me, envelope, chat, id, t)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                    ?22, ?23, ?24)
                RETURNING n",
            )?
            .query_row(fields, |r| r.get(0))?,
        )
    };
    // A copy that arrives live is given to jawd at once, for its thumbnail and perhaps its download;
    // one from history waits until it's shown or asked for, and a view-once one is never kept.
    let wanted = (message.live && !message.view_once).then(now_ms);
    let Some(n) = changed else {
        return match (&message.media, &existing) {
            (Some(copy), Some(kept)) => media::upsert(db, stamps, kept.n, chat, "sd", copy, wanted),
            _ => Ok(false),
        };
    };
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    stamps.expires(message.expires_at);
    if !message.poll.is_empty() {
        polls::options(db, n, &message.poll)?;
    }
    if let Some(copy) = &message.media {
        media::upsert(db, stamps, n, chat, "sd", copy, wanted)?;
    }
    // The chat's newest message sets its sort; older history never moves it.
    let seq = stamps.seq(CHAT, Some(chat));
    let moved = db
        .prepare_cached(
            "UPDATE chats SET last_at = max(last_at, ?2), last_message = ?3, v = ?4
            WHERE n = ?1 AND (last_message IS NULL OR last_message = ?3
                OR ?5 > (SELECT t FROM messages WHERE n = last_message))",
        )?
        .execute(params![chat, message.at.max(0), n, seq, t])?;
    if moved > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    let moved = stamps.unsettle(chat);
    let place = Some((message.at, n));
    match (message.mine, message.sent) {
        (true, Sent::Here) => moved.here = moved.here.max(place),
        (true, Sent::Elsewhere) => {
            moved.mine = moved.mine.max(place);
            moved.mine_live |= message.live;
            moved.mine_by_another |= user_jid(&message.chat) != chat_id;
        }
        (true, Sent::Forwarded) => {}
        (false, _) if !message.live => moved.history = moved.history.max(place),
        (false, _) => {}
    }
    if existing.is_none() {
        apply_pending(db, stamps, chat, n, &chat_id, &message.id, message.kind)?;
        // Opened on another of your devices before it came, past the receipts that wait for it.
        if message.view_once && !message.mine && expiry::opened_aside(db, &chat_id, &message.id)? {
            expiry::opened_elsewhere(db, stamps, n)?;
        }
    }
    Ok(true)
}

/// A message erased, or never shown, as a deletion for you leaves it: by its chat and ID, when.
const TOMBSTONE: &str = "INSERT OR IGNORE INTO tombstones (chat, id, at) VALUES (?1, ?2, ?3)";
/// What waits in `pending` for message ?2 in the chat whose JID is ?1.
const PENDING_FOR: &str = "DELETE FROM pending WHERE chat = ?1 AND target = ?2";

/// Applies what was waiting in `pending` for a message of kind `kind` that has just arrived, or has
/// just moved into `chat` with a merge; for a poll, the votes set aside for it come back to wait
/// there first. What waits for an event, which its secret opens one by one, is taken a few at a
/// time, the rest in steps (`events::arrived`).
pub(super) fn apply_pending(
    db: &Connection,
    stamps: &mut Stamps,
    chat: i64,
    n: i64,
    chat_jid: &str,
    id: &str,
    kind: &str,
) -> Result<()> {
    match kind {
        "poll" => polls::set_aside_back(db, chat_jid, id)?,
        "event" => return events::arrived(db, stamps, (chat, n, chat_jid, id)),
        _ => {}
    }
    let waiting: Vec<Waiting> = db
        .prepare_cached(
            "SELECT n, sender, mine, at, effect, raw FROM pending WHERE chat = ?1 AND target = ?2 ORDER BY at, n",
        )?
        .query_map(params![chat_jid, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (pending, sender, mine, at, effect, raw) in waiting {
        let Ok(what) = serde_json::from_str::<Amendment>(&effect) else { continue };
        let (what, at) = match what {
            Amendment::Keep { kind } if sealed_for_events(&kind) => {
                let sealed = events::Sealed { raw: &raw, sender: sender.as_deref(), mine, at };
                match events::opened(db, n, &sealed)? {
                    events::Opening::Opened(what, at) => (what, at),
                    events::Opening::Gone => {
                        db.prepare_cached("DELETE FROM pending WHERE n = ?1")?.execute([pending])?;
                        continue;
                    }
                    events::Opening::Sealed => continue,
                }
            }
            Amendment::Keep { .. } => continue,
            what => (what, at),
        };
        let gone = matches!(what, Amendment::DeleteForMe);
        let applied = apply_amendment(db, stamps, chat, n, Doing { what: &what, sender: sender.as_deref(), mine, at })?;
        db.prepare_cached("DELETE FROM pending WHERE n = ?1")?.execute([pending])?;
        // What waits for it after its deletion, or its erasure, has nothing left to change.
        if gone || (applied && matches!(what, Amendment::Opened)) {
            break;
        }
    }
    Ok(())
}

/// A row of `pending`: its n, who did what it says, whether it's yours, when, what it does and its
/// raw protobuf.
type Waiting = (i64, Option<String>, bool, i64, String, Vec<u8>);

/// Whether what's kept as `kind` may be sealed with an event's secret: an answer, or an edit.
fn sealed_for_events(kind: &str) -> bool {
    kind == "rsvp" || kind == "secret"
}

/// The chat and n of the message an amendment names, if jaw has it, wherever a merge has it.
fn target(db: &Connection, stamps: &mut Stamps, amend: &Amend) -> Result<Option<(i64, i64)>> {
    located(db, stamps, &chat_jid(db, &amend.chat)?, &amend.target)
}

/// Whether the message an amendment names was deleted for me, so nothing more is done to it.
fn tombstoned(db: &Connection, chat: &str, id: &str) -> Result<bool> {
    Ok(db
        .prepare_cached("SELECT 1 FROM tombstones t JOIN chats c ON c.n = t.chat WHERE c.id = ?1 AND t.id = ?2")?
        .exists(params![chat, id])?)
}

fn amend_message(db: &Connection, stamps: &mut Stamps, amend: &Amend) -> Result<bool> {
    let chat = chat_jid(db, &amend.chat)?;
    if tombstoned(db, &chat, &amend.target)? {
        // A receipt for an action of yours, which has an ID of its own and no row, says WhatsApp has it.
        if matches!(amend.what, Amendment::Status { .. }) {
            outbox::receipt_for_action(db, stamps, &amend.target)?;
        }
        return Ok(false);
    }
    let found = target(db, stamps, amend)?;
    if found.is_none() && amend.what == Amendment::DeleteForMe {
        // A message deleted for me before jaw has it never comes.
        let n = ensure_chat(db, stamps, &chat)?;
        db.prepare_cached("INSERT OR IGNORE INTO tombstones (chat, id, at) VALUES (?1, ?2, ?3)")?.execute(params![
            n,
            amend.target,
            amend.at
        ])?;
        return Ok(false);
    }
    let raw = amend.raw.as_ref().map_or(&[][..], |packed| packed.0.as_slice());
    // An answer or an edit sealed with its event's secret opens once jaw has the event; one for a
    // message deleted for everyone has nothing left to change, as a vote in a deleted poll hasn't.
    if let (Amendment::Keep { kind }, Some((chat, n))) = (&amend.what, found)
        && sealed_for_events(kind)
    {
        let sealed = events::Sealed { raw, sender: amend.sender.as_deref(), mine: amend.mine, at: amend.at };
        match events::opened(db, n, &sealed)? {
            events::Opening::Opened(what, at) => {
                let doing = Doing { what: &what, sender: amend.sender.as_deref(), mine: amend.mine, at };
                return apply_amendment(db, stamps, chat, n, doing);
            }
            events::Opening::Gone => return Ok(false),
            events::Opening::Sealed => events::unopened(db, n)?,
        }
    }
    match found {
        Some((chat, n)) if !matches!(amend.what, Amendment::Keep { .. }) => apply_amendment(
            db,
            stamps,
            chat,
            n,
            Doing { what: &amend.what, sender: amend.sender.as_deref(), mine: amend.mine, at: amend.at },
        ),
        _ => {
            // What's kept for a later jawd never applies, so a copy of it arriving again waits once.
            if matches!(amend.what, Amendment::Keep { .. }) && kinds::kept_already(db, &chat, amend, raw)? {
                return Ok(false);
            }
            db.prepare_cached(
                "INSERT INTO pending (chat, target, kind, at, raw, sender, mine, effect) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(params![
                chat,
                amend.target,
                amend.what.kind(),
                amend.at,
                raw,
                amend.sender.as_deref().map(user_jid),
                amend.mine,
                serde_json::to_string(&amend.what)?
            ])?;
            match amend.what.kind() {
                "vote" => polls::bounded(db)?,
                kind if events::WAITING_KINDS.contains(&kind) => events::bounded(db, kind)?,
                "opened" => expiry::opened_bounded(db)?,
                _ => {}
            }
            Ok(false)
        }
    }
}

/// An amendment being applied, and who did it when.
struct Doing<'a> {
    what: &'a Amendment,
    sender: Option<&'a str>,
    mine: bool,
    at: i64,
}

/// Does an amendment to message `n` in chat `chat`, saying whether it changed anything.
fn apply_amendment(db: &Connection, stamps: &mut Stamps, chat: i64, n: i64, doing: Doing) -> Result<bool> {
    let Doing { what, sender, mine, at } = doing;
    // Someone new who reacts is made first, since making them takes the next seq.
    let someone = match (what, sender) {
        (
            Amendment::React { .. } | Amendment::Status { .. } | Amendment::Vote { .. } | Amendment::Answer { .. },
            Some(sender),
        ) => Some(ensure_jid(db, stamps, sender)?),
        (
            Amendment::React { .. } | Amendment::Status { .. } | Amendment::Vote { .. } | Amendment::Answer { .. },
            None,
        ) => return Ok(false),
        _ => None,
    };
    if matches!(what, Amendment::DeleteForMe) {
        return delete_row(db, stamps, chat, n);
    }
    if matches!(what, Amendment::Opened) {
        return expiry::opened_elsewhere(db, stamps, n);
    }
    let seq = stamps.seq(MESSAGE, Some(n));
    let changed = match what {
        Amendment::React { emoji } => {
            let sender = someone.expect("a reaction's sender is made above");
            let changed = if emoji.is_empty() {
                db.prepare_cached("DELETE FROM reactions WHERE message = ?1 AND sender = ?2 AND at <= ?3")?
                    .execute([n, sender, at])?
            } else {
                db.prepare_cached(
                    "INSERT INTO reactions (message, sender, emoji, at, mine) VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT (message, sender) DO UPDATE SET emoji = excluded.emoji, at = excluded.at, mine = excluded.mine
                    WHERE excluded.at >= reactions.at AND excluded.emoji IS NOT reactions.emoji",
                )?
                .execute(params![n, sender, emoji, at, mine])?
            };
            if changed > 0 {
                db.prepare_cached("UPDATE messages SET v = ?2 WHERE n = ?1")?.execute([n, seq])?;
            }
            changed
        }
        Amendment::Edit { text, spans, summary } => db
            .prepare_cached(
                "UPDATE messages SET text = ?2, spans = ?3, summary = iif(?4 IS ?2, NULL, ?4), edited_at = ?5, v = ?6
                WHERE n = ?1 AND deleted = 0 AND (edited_at IS NULL OR edited_at <= ?5)",
            )?
            .execute(params![n, text, spans, summary, at, seq])?,
        Amendment::Revoke => {
            outbox::deleted_for_everyone(db, stamps, n)?;
            media::revoked(db, n)?;
            db.prepare_cached(
                "UPDATE messages SET deleted = 1, text = NULL, spans = NULL,
                    summary = iif(mine, 'You deleted this message', 'This message was deleted'),
                    reply = NULL, detail = NULL, raw = NULL, envelope = NULL, v = ?2
                WHERE n = ?1 AND deleted = 0",
            )?
            .execute([n, seq])?
        }
        Amendment::Star { starred } => db
            .prepare_cached("UPDATE messages SET starred = ?2, v = ?3 WHERE n = ?1 AND starred IS NOT ?2")?
            .execute(params![n, starred, seq])?,
        Amendment::Status { status } => {
            let recipient = someone.expect("a receipt's recipient is made above");
            let column = match status.as_str() {
                "read" => "read_at",
                "played" => "played_at",
                _ => "delivered_at",
            };
            db.prepare_cached(&format!(
                "INSERT INTO receipts (message, recipient, {column}) VALUES (?1, ?2, ?3)
                ON CONFLICT (message, recipient) DO UPDATE SET {column} = coalesce({column}, excluded.{column})"
            ))?
            .execute(params![n, recipient, at])?;
            // In a group, as on the phone, a message is delivered once every recipient has it and
            // read once every one has read it; each recipient's own times stay in receipts. Any
            // receipt says the server has it, so nothing of it is left to send.
            let kept: Option<(Option<String>, bool, String)> = db
                .prepare_cached(
                    "SELECT m.status, c.kind = 'direct', m.kind FROM messages m JOIN chats c ON c.n = m.chat
                    WHERE m.n = ?1 AND m.mine",
                )?
                .query_row([n], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .optional()?;
            if kept.is_some() {
                outbox::received(db, stamps, n)?;
            }
            let reached = match &kept {
                Some((_, true, _)) => Some(status.as_str()),
                Some((kept, false, kind)) if outbox::at_end(kind, kept.as_deref()) => None,
                Some((kept, false, _)) => match outbox::group_reached(db, n, chat, kept.as_deref())? {
                    None => Some(status.as_str()),
                    Some(None) => Some("sent"),
                    Some(Some(reached)) => Some(reached),
                },
                None => None,
            };
            match (kept, reached) {
                (Some((kept, _, kind)), Some(reached))
                    if advanced(kept.as_deref(), Some(reached)) != kept.as_deref() =>
                {
                    outbox::settled(db, n, &kind, reached)?;
                    db.prepare_cached("UPDATE messages SET status = ?2, v = ?3 WHERE n = ?1")?
                        .execute(params![n, reached, seq])?
                }
                _ => 0,
            }
        }
        Amendment::Vote { options, id } => {
            let voter = someone.expect("a vote's voter is made above");
            let poll: bool = db
                .prepare_cached("SELECT kind = 'poll' AND NOT deleted FROM messages WHERE n = ?1")?
                .query_row([n], |r| r.get(0))?;
            let changed = if poll { polls::vote(db, stamps, n, voter, options, id, at)? } else { 0 };
            if changed > 0 {
                db.prepare_cached("UPDATE messages SET v = ?2 WHERE n = ?1")?.execute([n, seq])?;
            }
            changed
        }
        Amendment::Hd { copy } => {
            let deleted: bool =
                db.prepare_cached("SELECT deleted FROM messages WHERE n = ?1")?.query_row([n], |r| r.get(0))?;
            usize::from(!deleted && media::upsert(db, stamps, n, chat, "hd", copy, None)?)
        }
        Amendment::Answer { answer, guests, id } => {
            let who = someone.expect("an answer's answerer is made above");
            let event: bool = db
                .prepare_cached("SELECT kind = 'event' AND NOT deleted FROM messages WHERE n = ?1")?
                .query_row([n], |r| r.get(0))?;
            let changed = if event { events::answer(db, n, who, (answer, *guests), id, at)? } else { 0 };
            if changed > 0 {
                db.prepare_cached("UPDATE messages SET v = ?2 WHERE n = ?1")?.execute([n, seq])?;
            }
            changed
        }
        // Only a newer edit replaces what an event shows, so one arriving again changes nothing.
        Amendment::EventEdit { text, spans, summary, detail } => db
            .prepare_cached(
                "UPDATE messages SET text = ?2, spans = ?3, summary = iif(?4 IS ?2, NULL, ?4), detail = ?5,
                    edited_at = ?6, v = ?7
                WHERE n = ?1 AND kind = 'event' AND NOT deleted AND (edited_at IS NULL OR edited_at < ?6)",
            )?
            .execute(params![n, text, spans, summary, detail, at, seq])?,
        Amendment::DeleteForMe | Amendment::Opened | Amendment::Keep { .. } => 0,
    };
    if changed == 0 {
        return Ok(false);
    }
    stamps.record(db, MESSAGE, n, Some(chat), seq)?;
    // The chat list shows its newest message, so the chat changes with it.
    let seq = stamps.seq(CHAT, Some(chat));
    if db.prepare_cached("UPDATE chats SET v = ?3 WHERE n = ?1 AND last_message = ?2")?.execute([chat, n, seq])? > 0 {
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(true)
}

fn set_members(db: &Connection, stamps: &mut Stamps, members: &Members) -> Result<bool> {
    let chat = ensure_chat(db, stamps, &members.chat)?;
    let mut had: HashMap<i64, String> = db
        .prepare_cached("SELECT member, role FROM group_members WHERE chat = ?1")?
        .query_map([chat], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut changed = false;
    let mut kept = Vec::new();
    for (jid, role) in &members.set {
        let member = ensure_jid(db, stamps, jid)?;
        kept.push(member);
        if had.remove(&member).as_deref() != Some(*role) {
            db.prepare_cached("INSERT OR REPLACE INTO group_members (chat, member, role) VALUES (?1, ?2, ?3)")?
                .execute(params![chat, member, role])?;
            changed = true;
        }
    }
    let mut gone: Vec<i64> = if members.all { had.into_keys().collect() } else { Vec::new() };
    for jid in &members.remove {
        let found: Option<i64> = db
            .prepare_cached("SELECT n FROM person_jids WHERE jid = ?1")?
            .query_row([user_jid(jid)], |r| r.get(0))
            .optional()?;
        gone.extend(found.filter(|member| !kept.contains(member)));
    }
    let mut left = Vec::new();
    for member in gone {
        if db.prepare_cached("DELETE FROM group_members WHERE chat = ?1 AND member = ?2")?.execute([chat, member])? > 0
        {
            left.push(member);
        }
    }
    changed |= !left.is_empty();
    outbox::members_left(db, chat, &left)?;
    if changed {
        let seq = stamps.seq(CHAT, Some(chat));
        db.prepare_cached("UPDATE chats SET v = ?2 WHERE n = ?1")?.execute([chat, seq])?;
        stamps.record(db, CHAT, chat, Some(chat), seq)?;
    }
    Ok(changed)
}

/// How long the feed keeps events.
pub(super) const EVENTS_KEPT: Duration = Duration::from_secs(7 * 24 * 3600);

/// Removes up to `limit` events older than the feed keeps, always leaving the newest, whose seq
/// the next change follows, and the actions kept for a late receipt past their time. Says how
/// many events went.
pub(super) fn prune(db: &Connection, limit: usize) -> Result<usize> {
    outbox::prune_kept(db)?;
    let before = now_ms() - EVENTS_KEPT.as_millis() as i64;
    Ok(db
        .prepare_cached(
            "DELETE FROM events WHERE seq IN (
            SELECT seq FROM events WHERE at < ?1 AND seq < (SELECT max(seq) FROM events) ORDER BY seq LIMIT ?2)",
        )?
        .execute(params![before, limit as i64])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_is_dropped_from_a_persons_jid_but_not_from_others() {
        assert_eq!(user_jid("351910000001:12@s.whatsapp.net"), "351910000001@s.whatsapp.net");
        assert_eq!(user_jid("9988776655:3@lid"), "9988776655@lid");
        assert_eq!(user_jid("351910000001:7@hosted"), "351910000001@hosted");
        assert_eq!(user_jid("9988776655:7@hosted.lid"), "9988776655@hosted.lid");
        assert_eq!(user_jid("120363000000000001@g.us"), "120363000000000001@g.us");
    }

    #[test]
    fn a_deletion_for_you_through_app_state_may_write_to_fts_itself() {
        let chat = "351910000002@s.whatsapp.net";
        let index: Vec<String> = ["deleteMessageForMe", chat, "m1", "0", "0"].map(String::from).into();
        let state = |changes| Change::AppState { index: index.clone(), value: Vec::new(), seq: 1, at: 1, changes };
        let deletion = Amend {
            chat: chat.into(),
            target: "m1".into(),
            sender: None,
            mine: false,
            at: 1,
            what: Amendment::DeleteForMe,
            raw: None,
        };
        assert!(touches_fts(&[state(vec![Change::Amend(deletion)])]));
        assert!(!touches_fts(&[state(vec![Change::Chat(Chat { id: chat.into(), ..Chat::default() })])]));
    }

    #[test]
    fn a_packed_protobuf_unpacks_to_what_it_was() {
        let raw: Vec<u8> = (0..2_000u32).map(|n| (n % 7) as u8).collect();
        let packed = Packed::new(&raw);
        assert!(packed.0.len() < raw.len() / 4, "{} bytes packed from {}", packed.0.len(), raw.len());
        assert_eq!(unpack(&packed.0).unwrap(), raw);
    }
}
