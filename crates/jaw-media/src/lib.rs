//! jaw's media pipeline, on files: what a file is ([`Media::probe`]), the previews interfaces show
//! ([`Media::previews`], [`instant_preview`]), and the SD and HD copies jaw sends ([`plan`],
//! [`Media::prepare`]). Paths in, paths out: no database and no network, so jawd decides where
//! files live, which rows point to them and when they're uploaded.
//!
//! Everything here blocks the thread that calls it, and each call says which of jawd's pools it
//! belongs in ([`Lane`]): photos and previews are decoded and encoded in this process, on the
//! image pool; videos, GIFs and audio are encoded by an ffmpeg child at a lower OS priority, on the
//! transcode pool, streaming between paths so a 2 GB video never sits in memory. A job is
//! cancelled through its [`Cancel`], which kills its ffmpeg at once, and a file appears under its
//! final name only once it's whole and synced, or, for a preview its caller takes as derived,
//! whole ([`Durability`]).

mod error;
mod file;
mod gif;
mod job;
mod jpeg;
mod limits;
mod plan;
mod prepare;
mod preview;
mod probe;
mod raster;
mod run;
mod tools;
mod webp;

pub use error::{Error, Result};
pub use file::{Durability, remove_partial};
pub use job::{Cancel, Job, Progress};
pub use limits::{HD, Limits, SD, THUMBNAIL_LONG_EDGE, THUMBNAIL_QUALITY};
pub use plan::{Action, Lane, Plan, Plans, Quality, plan};
pub use prepare::{Destination, Prepared};
pub use preview::{
    DocumentType, LARGE, Preview, PreviewFormat, Previews, SMALL, document_type, instant_preview, preview_name,
    previews_lane, whole_preview,
};
pub use probe::{Format, Kind, Orientation, Probe};
pub use tools::{Status, Tools, VideoEncoder};

use std::path::Path;
use std::sync::Arc;

/// What jawd needs to change about how media is made; the defaults suit a laptop.
#[derive(Clone, Debug)]
pub struct Config {
    /// How much lower than jawd's own the priority of ffmpeg children is, as `nice` counts.
    pub nice: i32,
    /// The threads each ffmpeg child may use for decoding, filtering and software encoding; two
    /// at most with VideoToolbox, which goes no faster with more.
    pub threads: usize,
    /// The video encoder to use, or None for the best one this machine has.
    pub video_encoder: Option<VideoEncoder>,
    /// How `previews` puts its files in place: synced, or derived, for a caller that checks each
    /// with `whole_preview` before pointing to it and makes it again when it isn't.
    pub previews: Durability,
}

impl Default for Config {
    fn default() -> Config {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Config { nice: 10, threads: (cores / 2).clamp(2, 8), video_encoder: None, previews: Durability::Synced }
    }
}

/// The pipeline: the tools it found and how it uses them. Cheap to clone and shared by every pool.
#[derive(Clone)]
pub struct Media {
    tools: Arc<Tools>,
    config: Config,
}

impl Media {
    /// The pipeline with ffmpeg and ffprobe found where they're usually installed. Finding them
    /// runs each once, so call it off the realtime lane.
    pub fn new(config: Config) -> Media {
        Media::with_tools(Tools::find(), config)
    }

    pub fn with_tools(tools: Tools, config: Config) -> Media {
        Media { tools: Arc::new(tools), config }
    }

    pub fn tools(&self) -> &Tools {
        &self.tools
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// What the pipeline can and can't do on this machine, for `jawd status`.
    pub fn status(&self) -> Status {
        self.tools.status(self.config.video_encoder)
    }

    /// What `path` is. Photos, GIFs and stickers are read from their headers in this process in
    /// well under a millisecond; everything else asks ffprobe.
    pub fn probe(&self, path: &Path) -> Result<Probe> {
        probe::probe(&self.tools, path)
    }

    /// Writes `source`'s display-ready previews, at most SMALL and LARGE pixels on their longest
    /// side, beside `stem` (`<stem>.256.jpg` and `<stem>.768.jpg`, or `.png` for images with
    /// transparency), as `Config::previews` says; a document that has no cheap first page gets an
    /// icon instead.
    pub fn previews(&self, source: &Path, probe: &Probe, stem: &Path, cancel: &Cancel) -> Result<Previews> {
        preview::previews(self, source, probe, stem, cancel)
    }

    /// Makes each copy `copies` asks for, from `source`, in order: a photo is decoded once for all
    /// of them. Each copy's file, and the video's poster, appear under their names only once whole.
    pub fn prepare(
        &self,
        source: &Path,
        probe: &Probe,
        copies: &[(&Plan, &Destination)],
        job: &mut Job,
    ) -> Result<Vec<Prepared>> {
        prepare::prepare(self, source, probe, copies, job)
    }
}
