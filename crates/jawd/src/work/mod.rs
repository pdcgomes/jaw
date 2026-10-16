//! The work lanes. Realtime work (incoming messages, receipts, typing, sends, socket I/O) runs on
//! its own tasks and never queues behind anything here. Interactive work (what an interface shows
//! or sends right now) and background work (history, prefetch, avatars, eviction) share a pool per
//! resource, interactive first, so a big history sync can't hold up the photo you just opened.

mod pool;
mod samples;
mod serial;

pub use pool::{Pool, Priority};
pub use samples::Samples;
pub use serial::Serial;

/// What a pool reports for `status`.
pub trait Report: Send + Sync {
    fn report(&self) -> jaw_proto::Pool;
}
