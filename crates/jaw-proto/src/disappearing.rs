//! Disappearing and view-once messages: `chat.disappearing`, `view_once.open` and
//! `view_once.close`, and what `status` says of them. A chat's timer comes in its row as
//! `ephemeral`, and a message's in its row as `expires_at` and `view_once` (see protocol/README.md).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The timers the phones offer, in seconds: off, 24 hours, 7 days and 90 days.
pub const TIMERS: [u32; 4] = [0, 86_400, 604_800, 7_776_000];

/// `chat.disappearing`'s params: the chat's timer, one of TIMERS.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatDisappearing {
    pub chat: String,
    pub seconds: u32,
}

/// `view_once.open`'s and `view_once.close`'s params: someone else's view-once message, named as
/// the actions on a message name one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewOnce {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

/// `view_once.open`'s result: the message's file once it's in place, its caption and its media as
/// a row's are, and when jawd erases it whatever the interface does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opened {
    pub message: String,
    pub kind: String,
    /// The file, relative to hello's `root`.
    pub path: String,
    /// Its caption, parsed as a row's text is.
    pub spans: Value,
    pub media: Map<String, Value>,
    /// In milliseconds since the epoch.
    pub until: i64,
}

/// What `status` says of disappearing and view-once messages: the messages with a time to go and
/// when the next does, those erased since jawd started, and the view-once messages open now.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisappearingStatus {
    pub expiring: u64,
    pub next: Option<i64>,
    pub erased: u64,
    pub open: u64,
    pub open_most: u64,
}
