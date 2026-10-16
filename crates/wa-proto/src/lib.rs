//! WhatsApp's protobufs, as whatsmeow ships them, and the frames jawd and jaw-engine exchange.
//!
//! Each protobuf package is a module named after it: `jaw::engine` for the link,
//! `wa_web_protobufs_e2e` for WhatsApp's `Message`, `wa_web_protobufs_history_sync` for history,
//! and the packages they import.

#[allow(clippy::all, clippy::pedantic, rustdoc::all)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/protos.rs"));
}

pub use generated::*;

const VERSION: &str = include_str!("../../../proto/whatsapp/VERSION");

/// The whatsmeow version proto/whatsapp/ was copied from, which jaw-engine must be built with.
pub fn whatsmeow() -> &'static str {
    VERSION.trim().rsplit(' ').next().unwrap_or_default()
}
