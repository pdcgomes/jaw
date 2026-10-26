//! Media in: `media.fetch`, `media.save` and `media.cancel`. How a message's file is, and its
//! previews, come in the message's row as its `media` (see protocol/README.md).

use serde::{Deserialize, Serialize};

/// Which copy of a message's file: the standard one, or the HD copy WhatsApp's dual upload sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    #[default]
    Sd,
    Hd,
}

/// `media.fetch`'s params: download a message's file now, ahead of what downloads by itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFetch {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

/// `media.save`'s params: copy a message's file to `to`, a folder or a file's path, downloading it
/// first if it isn't yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSave {
    pub message: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

/// `media.save`'s result: where the file was saved, whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub path: String,
}

/// `media.cancel`'s params: stop downloading a message's file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCancel {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

/// What `status` says of media: the cache and what's waiting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaStatus {
    /// The cache's files and every byte of them on disk, previews included, and its limit.
    pub cache_bytes: u64,
    pub cache_files: u64,
    pub cache_limit: u64,
    /// Videos and documents download by themselves up to this size.
    pub auto_bytes: u64,
    /// Copies waiting for jawd to draw or download them, and those downloading now.
    pub wanted: u64,
    pub downloading: u64,
    /// Copies whose file the phone was asked to send again, waiting for its answer.
    pub waiting_for_phone: u64,
    /// Files no message names any more, waiting to go.
    pub unnamed: u64,
    /// Since jawd started: partial files a stopped jawd left, files no row named, files dropped from
    /// the cache to keep it to its size, and previews found broken and made again.
    pub partial_removed: u64,
    pub orphans_removed: u64,
    pub evicted: u64,
    pub remade: u64,
    /// Why background downloads are paused, while they are: the disk is nearly full.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<String>,
    /// Preview and thumbnail writes not yet made durable, which settle every few seconds.
    #[serde(default)]
    pub unsettled: u64,
    /// Whether the start's check of the previews a power cut may have broken is still going, and
    /// how many files it hasn't passed yet; nothing settles until it's done.
    #[serde(default)]
    pub checking: bool,
    #[serde(default)]
    pub suspect: u64,
    /// Whether media files are flushed to the drive in full, since jaw's folder isn't all on the
    /// Mac's own drive, where a barrier would order them.
    #[serde(default)]
    pub full_flush: bool,
}
