//! The protocol jaw's interfaces speak to jawd: one JSON object per line on a Unix socket, shaped
//! like JSON-RPC 2.0 without its `jsonrpc` member. protocol/README.md specifies it, and
//! protocol/fixtures/ pins it: each fixture reads into these types and writes back the same.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod find;
pub mod layout;
pub mod local;
pub mod send;
pub mod settings;
pub mod sync;
pub mod themes;
pub mod windows;

/// The version this crate speaks. jawd serves one, and refuses a hello with another.
pub const PROTOCOL: u32 = 1;

/// A request from an interface. Its reply carries the same `id`, which the interface picks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// jawd's answer to a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Value),
    Error(Error),
}

/// Something jawd sends unasked: a window's updates, the connection's state, a notice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Push {
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// A line from jawd: a reply has an `id`, a push doesn't.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FromJawd {
    Reply(Reply),
    Push(Push),
}

/// Why a request failed. `message` is a sentence an interface can show as it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Error {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Error { code, message: message.into(), data: None }
    }
}

/// Error codes: JSON-RPC's own, then jaw's.
pub mod code {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const UNKNOWN_METHOD: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    pub const NOT_LINKED: i32 = 1;
    pub const ENGINE_UNAVAILABLE: i32 = 2;
    pub const NOT_FOUND: i32 = 3;
    pub const NOT_ALLOWED: i32 = 4;
    pub const LIMIT: i32 = 5;
    pub const VERSION: i32 = 6;
    pub const CANCELLED: i32 = 7;
}

/// `hello`'s params: the first request on every connection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub client: String,
    pub version: String,
    #[serde(default)]
    pub focus: bool,
    /// The last `seq` this client saw, when it's reconnecting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

/// `focus`'s params: whether the interface is in front now, as a terminal reports focus in and out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    pub focused: bool,
}

/// `hello`'s result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub protocol: u32,
    /// jawd's version.
    pub jawd: String,
    /// jaw's folder; every path jawd sends is relative to it.
    pub root: String,
    /// The newest `seq` in jawd's feed.
    pub seq: u64,
    pub account: Account,
    pub connection: Connection,
    /// The settings of the connection's own domain, the one its `client` names, each with the
    /// value in effect; absent for a client jawd keeps no settings for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<settings::Settings>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub state: AccountState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub me: Option<Me>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Unlinked,
    Linking,
    Linked,
}

/// The account's own person.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub id: String,
    pub name: String,
    pub phone: String,
}

/// The connection to WhatsApp, as the `connection` push and `hello` report it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub state: ConnectionState,
    /// While catching up: how many of `total` have arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// While banned: until when, in milliseconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
    /// Why, when WhatsApp said: a failed connection's cause, a logout's reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Connection {
    pub fn new(state: ConnectionState) -> Self {
        Connection { state, done: None, total: None, until: None, reason: None }
    }
}

/// `link.start`'s params: by QR code, or by a code typed on the phone when `phone` is given.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkStart {
    /// In international form, digits only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

/// The `link` push, as linking goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkStep {
    /// A QR code to scan, until `expires` (milliseconds since the epoch); the next follows.
    Qr {
        code: String,
        expires: i64,
    },
    /// The code to type on the phone.
    Code {
        code: String,
    },
    Done {
        phone: String,
    },
    Failed {
        reason: String,
    },
}

/// A catch-up after time offline: what WhatsApp said was waiting, what has been stored so far,
/// and how long it took once it's done.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatchUp {
    pub total: u64,
    pub messages: u64,
    pub receipts: u64,
    pub stored: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
}

/// History downloaded from the phone since jawd started.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryProgress {
    pub blobs: u64,
    pub conversations: u64,
    pub messages: u64,
    pub bytes: u64,
    /// Time spent downloading and decoding, and storing, in seconds.
    pub download_seconds: f64,
    pub store_seconds: f64,
    /// The phone's own measure of how far it has got, in percent.
    pub progress: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connecting,
    Online,
    CatchingUp,
    PhoneOffline,
    Offline,
    LoggedOut,
    UpdateNeeded,
    Banned,
    EngineRestarting,
}

/// `status`'s result: what `jawd status` and `jaw status` print.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub jawd: String,
    pub protocol: u32,
    pub root: String,
    pub account: Account,
    pub connection: Connection,
    /// The engine jawd runs, and its version once it has said hello.
    pub engine: Engine,
    /// What jawd looked for and didn't find, with what it's for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<Missing>,
    /// Each work pool: what it's running and holding, and how long its jobs wait and run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pools: Vec<Pool>,
    /// The latest catch-up after time offline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catch_up: Option<CatchUp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryProgress>,
    /// What jaw.db holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored: Option<Stored>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interfaces: Option<Interfaces>,
    /// The windows interfaces watch, and the reads jawd keeps for them to share.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Windows>,
    /// jaw.db's WAL and commits, and why it can't be written if it can't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<Storage>,
    /// The layouts kept for interfaces, and the limits on them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layouts: Option<Layouts>,
    /// Each domain's settings in effect, by the domain's name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, serde_json::Map<String, Value>>,
    /// Each setting whose value in jaw.db this jawd doesn't take.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings_ignored: Vec<settings::Ignored>,
}

/// The layouts jawd keeps for interfaces (`layout.set`), and the most it keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layouts {
    /// Interfaces with a layout kept, and the most that can have one.
    pub kept: u64,
    pub most: u64,
    /// The biggest layout kept, in bytes, and the most one can take.
    pub largest_bytes: u64,
    pub bytes_most: u64,
    /// Layouts refused since jawd started for being past one of those limits.
    pub refused: u64,
}

/// The interfaces connected to jawd, and where its notifications go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interfaces {
    pub connected: u32,
    /// Whether one of them is in front.
    pub focused: bool,
    /// Where notifications go (`Notification Center`, `notify-send`), or why they don't.
    pub notifications: String,
    /// The requests in every connection's lane, read and not yet answered, this `status` among
    /// them; the most a connection's lane holds, past which jawd reads nothing more from it until
    /// one is answered; and the connections whose lane is that full.
    #[serde(default)]
    pub in_lanes: u64,
    #[serde(default)]
    pub lane_most: u64,
    #[serde(default)]
    pub lanes_full: u64,
    /// The connections whose reply, or `layout` push, waits for room in the queue to their
    /// interface, one each at most, which jawd reads nothing from and sends nothing else meanwhile.
    #[serde(default)]
    pub replies_waiting: u64,
    /// The requests being answered on tasks of their own, finds and saves, and the most a
    /// connection can have, past which another is refused.
    #[serde(default)]
    pub apart: u64,
    #[serde(default)]
    pub apart_most: u64,
}

/// The windows interfaces watch, and the reads jawd keeps for them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Windows {
    /// Windows watched on every connection, how many of them differ, and the most a connection
    /// can watch at once.
    pub watched: u64,
    pub distinct: u64,
    pub most: u64,
    /// The bytes of the reads kept for windows watched alike to share, the most they may hold, and
    /// how many were let go to stay under it.
    pub kept_bytes: u64,
    pub kept_most: u64,
    pub let_go: u64,
    /// Window reads since jawd started, and the times a window took a read kept or under way
    /// instead.
    pub reads: u64,
    pub taken: u64,
    pub joined: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Storage {
    /// The WAL file's size.
    pub wal_bytes: u64,
    /// Frames in the WAL, and those of them not yet copied into jaw.db.
    pub wal_frames: u64,
    pub frames_waiting: u64,
    /// The most frames the WAL has held since jawd started.
    pub wal_peak_frames: u64,
    /// Commits synced to the disk before an ack or an answer went.
    pub synced_commits: u64,
    /// How often background work waited for the checkpointer, and for how long in all.
    pub background_waits: u64,
    pub background_waited_ms: u64,
    /// Live writes made in an inbox group's synced commit, and those commits that failed and were
    /// made again without their live writes.
    #[serde(default)]
    pub shared_writes: u64,
    #[serde(default)]
    pub shared_failed: u64,
    /// Why jaw.db can't be written, while it can't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trouble: Option<String>,
    /// The backups of jaw.db taken before upgrades, kept beside it for a while.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backups: Vec<Backup>,
}

/// A backup of jaw.db taken before an upgrade changed its rows, and until when it's kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    /// Its file's name, beside jaw.db.
    pub file: String,
    pub bytes: u64,
    pub taken_ms: i64,
    /// It goes at the first clean stop after this.
    pub kept_until_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    /// Events kept as they arrived and not converted yet, messages among them.
    pub inbox: u64,
    /// Messages from history waiting to be converted.
    pub history: u64,
    #[serde(default)]
    pub chats: u64,
    #[serde(default)]
    pub messages: u64,
    #[serde(default)]
    pub people: u64,
    /// Messages not yet indexed for search.
    #[serde(default)]
    pub unindexed: u64,
    /// Changes that failed to convert, kept with their errors.
    #[serde(default)]
    pub quarantined: u64,
    /// Chats being merged into another, the same person's under another JID.
    #[serde(default)]
    pub merging: u64,
    /// Of those, merges whose steps kept failing, left until jawd starts again.
    #[serde(default)]
    pub merges_stuck: u64,
    /// Messages and actions of yours waiting to send, failed and waiting for a retry, or kept for a
    /// late receipt.
    #[serde(default)]
    pub outbox: u64,
    /// Of those, the ones WhatsApp refused when they may already have gone, waiting for a receipt.
    #[serde(default)]
    pub unconfirmed: u64,
    /// Of those, actions that had no receipt in their day, kept 30 days, a thousand at most, so a
    /// receipt that comes later still does them.
    #[serde(default)]
    pub actions_kept: u64,
    /// Messages deleted for you and the IDs of your actions, about 40 bytes each, kept for as long
    /// as the account is linked so nothing brings them back.
    #[serde(default)]
    pub tombstones: u64,
    /// Who your group messages were sent to, kept until each has been read by all.
    #[serde(default)]
    pub recipients: u64,
    /// Members who left groups, whose messages from you there are still to be looked at again.
    #[serde(default)]
    pub regroup: u64,
    /// Group messages of yours read by all, whose recipients are still to be let go of.
    #[serde(default)]
    pub settled: u64,
    /// The keys interfaces gave the messages they sent, kept for a day.
    #[serde(default)]
    pub send_keys: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pool {
    pub name: String,
    pub workers: u32,
    pub running: u32,
    pub queued_interactive: u32,
    pub queued_background: u32,
    /// Jobs finished since jawd started.
    pub done: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_interactive: Option<Percentiles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_background: Option<Percentiles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Percentiles>,
}

/// Of the latest thousand or so, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50_us: u64,
    pub p99_us: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Engine {
    /// `go`, `rust` or `demo`.
    pub kind: String,
    /// `starting`, `running`, `restarting` or `missing`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// How many times it has been started again since jawd started.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub restarts: u32,
    /// Why it last stopped, or why it can't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit: Option<String>,
    /// How long it took to answer a ping just now, in microseconds; absent if it didn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_us: Option<u64>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Missing {
    pub what: String,
    pub needed_for: String,
}
