//! What sync adds to the protocol: `history.more`, which asks the phone for older messages in a
//! chat, and the `notice` push, which says something once, such as what arrived while jaw was
//! offline.

use serde::{Deserialize, Serialize};

/// `history.more`'s params. Its result is empty once the phone has been asked; the messages arrive
/// as upserts when it answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryMore {
    pub chat: String,
}

/// The `notice` push: something an interface should say once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Info,
    Warning,
    Error,
}
