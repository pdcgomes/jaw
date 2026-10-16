//! What jawd knows about itself, for `hello` and `status`.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jaw_proto::{Account, AccountState, Connection, ConnectionState, Engine, Missing, PROTOCOL, Status, Welcome};

pub struct State {
    pub home: PathBuf,
    /// The user who owns jaw's folder, the only one jawd talks to.
    pub owner: u32,
    missing: Vec<Missing>,
}

impl State {
    pub fn new(home: &Path) -> Result<State> {
        let owner = std::fs::metadata(home).with_context(|| format!("can't read {}", home.display()))?.uid();
        Ok(State { home: home.to_path_buf(), owner, missing: missing() })
    }

    pub fn welcome(&self) -> Welcome {
        Welcome {
            protocol: PROTOCOL,
            jawd: env!("CARGO_PKG_VERSION").into(),
            root: self.home.display().to_string(),
            seq: 0,
            account: self.account(),
            connection: self.connection(),
        }
    }

    pub fn status(&self) -> Status {
        Status {
            jawd: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL,
            root: self.home.display().to_string(),
            account: self.account(),
            connection: self.connection(),
            engine: Engine { kind: "go".into(), state: "stopped".into(), version: None },
            missing: self.missing.clone(),
        }
    }

    fn account(&self) -> Account {
        Account { state: AccountState::Unlinked, me: None }
    }

    fn connection(&self) -> Connection {
        Connection::new(ConnectionState::Offline)
    }
}

/// The programs jawd uses but doesn't carry, looked for on PATH and where Homebrew puts them, since
/// a LaunchAgent's PATH doesn't include it.
fn missing() -> Vec<Missing> {
    let wanted = [
        ("ffmpeg", "sending videos, GIFs and audio files, and video posters"),
        ("ffprobe", "reading what a file to send is"),
    ];
    let mut folders: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    folders.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    wanted
        .into_iter()
        .filter(|(program, _)| !folders.iter().any(|folder| folder.join(program).is_file()))
        .map(|(program, needed_for)| Missing { what: program.into(), needed_for: needed_for.into() })
        .collect()
}
