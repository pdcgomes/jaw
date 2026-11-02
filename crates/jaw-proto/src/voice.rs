//! Voice messages: `voice.play`, `voice.pause`, `voice.seek` and `voice.speed`, `voice.record`,
//! `voice.stop`, `voice.send` and `voice.cancel`, with the `playback` and `recording` pushes and what
//! `status` says of voice (see protocol/README.md's "Voice"). One thing plays or records at once, on
//! jawd's machine.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::windows::MessageRow;

/// `voice.play`'s params: a voice message, by its ID and optionally its chat, or the stopped
/// recording, from `at` seconds (where it paused, or 0, when left out) at `speed` (1, 1.5 or 2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoicePlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    /// The stopped recording, to listen to before it's sent, in place of `message`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recording: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

/// `voice.seek`'s params: where to move the playhead of what's playing or paused, in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceSeek {
    pub at: f64,
}

/// `voice.speed`'s params: 1, 1.5 or 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceSpeed {
    pub speed: f64,
}

/// `voice.record`'s params: the chat the recording is for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceRecord {
    pub chat: String,
}

/// `voice.send`'s params. `key` is as `send.text`'s: asking again with it within a day answers
/// with the same message. `at` and `repeat` would send it later, which jawd refuses until it
/// schedules messages.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceSend {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Value>,
}

/// `voice.send`'s result, once the message is on disk, synced, with its outbox row: its ID and its
/// row as `messages.watch` sends it, pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoiceSent {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<MessageRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    Playing,
    Paused,
    /// It played to its end; `at` is its length.
    Ended,
    /// Another message or a recording took over, or it was stopped.
    Stopped,
    /// Nothing plays; `error` says why.
    Failed,
}

/// The `playback` push: what plays or is paused and where its playhead is, ten times a second
/// while it plays and at each change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    /// The voice message, or none for the recording, which `recording` says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recording: bool,
    pub state: PlaybackState,
    pub at: f64,
    /// Its length.
    pub seconds: f64,
    pub speed: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordingState {
    Recording,
    /// A second of nothing but digital silence, as macOS gives when it hasn't allowed jawd the
    /// microphone; it carries on recording.
    Silent,
    Stopped,
    Cancelled,
    Failed,
}

/// The `recording` push: how loud, ten times a second, while recording, and each change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub state: RecordingState,
    pub seconds: f64,
    /// While recording: 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Once stopped: its 64 points, each 0 to 100, as WhatsApp sends them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waveform: Vec<u8>,
    /// Once stopped: `stopped`, `limit` or `interrupted`, with `reason` for the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What `status` says of voice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceStatus {
    /// What plays or is paused: a voice message's ID, `recording`, or none.
    #[serde(default)]
    pub playing: Option<String>,
    /// How it is: `waiting` for its file, `playing` or `paused`.
    #[serde(default)]
    pub state: Option<String>,
    /// The recording: `recording`, `stopped` or `sending`, or none.
    #[serde(default)]
    pub recording: Option<String>,
    /// How long it is so far.
    #[serde(default)]
    pub seconds: f64,
    /// The files in the recordings folder, at most one recording's, and their bytes.
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub bytes: u64,
    /// What a stopped jawd left there, removed as this one started.
    #[serde(default)]
    pub removed_at_start: u64,
    /// Whether the folder takes hard links, which putting a recording in place needs.
    #[serde(default)]
    pub hard_links: bool,
    /// The last recording's microphone audio lost because the audio thread fell behind, and
    /// whether it heard nothing but digital silence.
    #[serde(default)]
    pub gaps_ms: u64,
    #[serde(default)]
    pub silent: bool,
    /// Since jawd started: times the output found nothing to play, microphone samples lost, events
    /// dropped for a caller behind, and damaged packets concealed.
    #[serde(default)]
    pub underruns: u64,
    #[serde(default)]
    pub overruns: u64,
    #[serde(default)]
    pub dropped_events: u64,
    #[serde(default)]
    pub concealed: u64,
}
