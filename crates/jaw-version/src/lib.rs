//! The version jaw, jawd and jaw-engine all say: git describe's for the commit they're built from,
//! as scripts/version gives it, so jaw can tell when the jawd it talks to is from another build.

/// Such as `0.1.0` at the tag v0.1.0, `0.1.0-3-g4d2c1e9` three commits after it, or the commit's
/// hash before the first tag.
pub const VERSION: &str = env!("JAW_VERSION");
