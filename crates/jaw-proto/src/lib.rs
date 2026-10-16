//! The protocol jaw's interfaces speak to jawd: one JSON object per line on a Unix socket, shaped
//! like JSON-RPC 2.0 without its `jsonrpc` member. protocol/README.md specifies it, and
//! protocol/fixtures/ pins it: each fixture reads into these types and writes back the same.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod local;

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
    pub const NOT_LINKED: i32 = 1;
    pub const ENGINE_UNAVAILABLE: i32 = 2;
    pub const NOT_FOUND: i32 = 3;
    pub const NOT_ALLOWED: i32 = 4;
    pub const LIMIT: i32 = 5;
    pub const VERSION: i32 = 6;
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
}

impl Connection {
    pub fn new(state: ConnectionState) -> Self {
        Connection { state, done: None, total: None, until: None }
    }
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Engine {
    /// `go`, `rust` or `demo`.
    pub kind: String,
    /// `starting`, `running`, `restarting` or `stopped`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Missing {
    pub what: String,
    pub needed_for: String,
}
