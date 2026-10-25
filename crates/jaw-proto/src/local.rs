//! Where jaw keeps its files, so jawd and every interface find the same ones: `JAW_HOME` if it's
//! set, otherwise `~/Library/Application Support/jaw` on macOS and `$XDG_DATA_HOME/jaw` (or
//! `~/.local/share/jaw`) elsewhere.

use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    if let Some(home) = std::env::var_os("JAW_HOME") {
        return PathBuf::from(home);
    }
    platform()
}

/// Where jaw keeps its files when `JAW_HOME` doesn't say: the folder that holds your account.
pub fn platform() -> PathBuf {
    let user = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    if cfg!(target_os = "macos") {
        return user.join("Library/Application Support/jaw");
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(data) if !data.is_empty() => PathBuf::from(data).join("jaw"),
        _ => user.join(".local/share/jaw"),
    }
}

/// The socket interfaces talk to jawd on.
pub fn socket(home: &Path) -> PathBuf {
    home.join("jaw.sock")
}
