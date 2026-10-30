//! Media in: `media.fetch`, `media.save` and `media.cancel`; and out: `send.media`, with its `job`
//! pushes. How a message's file is, and its previews, come in the message's row as its `media` (see
//! protocol/README.md).

use serde::{Deserialize, Serialize};

use crate::windows::MessageRow;

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

/// `media.cancel`'s params: stop downloading a message's file, or, for one of yours on its way,
/// stop making and uploading it, which leaves it failed until `send.retry`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCancel {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

/// `send.media`'s params: files to send to `chat`, a message each, in the order given; several
/// photos and videos go as an album.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMedia {
    pub chat: String,
    pub files: Vec<Attachment>,
    /// `sd` by default; `hd` also sends an HD copy of each photo and video that has more to give.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Variant>,
    /// The ID of the message the first file answers, in the same chat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// As `send.text`'s: asking again with it within a day answers with the same messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// One file to send: a whole path or one starting `~/`, what it goes as (from what it is, when
/// not given), and its caption.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<AttachmentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// What a file goes as. A document goes as it is, whatever it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Image,
    Video,
    Gif,
    Audio,
    Document,
}

/// `send.media`'s result, once every message is on disk, synced, with its outbox row: their IDs,
/// a file's each in the order of `files`, the album's when they went as one, and their rows as
/// `messages.watch` sends them, pending.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SentMedia {
    pub messages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<MessageRow>,
}

/// The `job` push: how far making or moving a message's file has got.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Names the job, the same in every push about it.
    pub id: String,
    pub kind: JobKind,
    /// The message whose file it is, as rows name it: an HD copy's is its SD message's.
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
    /// From 0 to 1; 1 once it's done.
    pub progress: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobKind {
    Upload,
    Download,
    Transcode,
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
    /// Of the copies being done, those nobody asked for, and the most of them done at once.
    #[serde(default)]
    pub unasked: u64,
    #[serde(default)]
    pub unasked_most: u64,
    /// Files downloaded and waiting to be named, one for each copy being done at most.
    #[serde(default)]
    pub naming: u64,
    /// The progress of downloads nobody waits on that waits to be written, the most that may, and
    /// how much was passed over past that since jawd started, each download's next saying more.
    #[serde(default)]
    pub progress_waiting: u64,
    #[serde(default)]
    pub progress_most: u64,
    #[serde(default)]
    pub progress_passed: u64,
    /// Copies of your files on their way to WhatsApp: being made, uploading, and failed until
    /// `send.retry`.
    #[serde(default)]
    pub preparing: u64,
    #[serde(default)]
    pub uploading: u64,
    #[serde(default)]
    pub unsent: u64,
    /// jaw's own copies of the files you picked, kept until the copies sent are made from them, and
    /// their bytes; and those nothing needs any more, waiting to go.
    #[serde(default)]
    pub kept_files: u64,
    #[serde(default)]
    pub kept_bytes: u64,
    #[serde(default)]
    pub kept_unneeded: u64,
}
