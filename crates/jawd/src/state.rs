//! What jawd knows about itself, for `hello` and `status`.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jaw_proto::{Account, Connection, Engine, Missing, PROTOCOL, Push, Status, Stored, Welcome};
use tokio::sync::broadcast;

use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::store::Store;
use crate::sync::Shared;
use crate::work::Report;

pub struct State {
    pub home: PathBuf,
    /// The user who owns jaw's folder, the only one jawd talks to.
    pub owner: u32,
    engine: Result<Supervisor, (&'static str, String)>,
    missing: Vec<Missing>,
    pools: Vec<Arc<dyn Report>>,
    live: Shared,
    store: Option<Store>,
    /// What every greeted connection is sent: linking, the connection's state.
    pub pushes: broadcast::Sender<Push>,
}

impl State {
    /// `engine` is the supervisor of the engine jawd runs, or the engine's kind and why it can't run;
    /// `pools` are the work pools `status` reports; `live` is what sync knows of the account.
    pub fn new(
        home: &Path,
        engine: Result<Supervisor, (&'static str, String)>,
        pools: Vec<Arc<dyn Report>>,
        live: Shared,
        store: Option<Store>,
        pushes: broadcast::Sender<Push>,
    ) -> Result<State> {
        let owner = std::fs::metadata(home).with_context(|| format!("can't read {}", home.display()))?.uid();
        Ok(State { home: home.to_path_buf(), owner, engine, missing: missing(), pools, live, store, pushes })
    }

    pub fn engine(&self) -> Result<&Supervisor, &str> {
        self.engine.as_ref().map_err(|(_, why)| why.as_str())
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
        let engine = match &self.engine {
            Ok(supervisor) => {
                let status = supervisor.status();
                Engine {
                    kind: status.kind.into(),
                    state: status.state.name().into(),
                    version: status.version,
                    restarts: status.restarts,
                    last_exit: status.last_exit,
                    answer_us: None,
                }
            }
            Err((kind, why)) => Engine {
                kind: (*kind).into(),
                state: "missing".into(),
                version: None,
                restarts: 0,
                last_exit: Some(why.clone()),
                answer_us: None,
            },
        };
        let (account, connection, catch_up, history) = {
            let live = self.live.lock().expect("live state");
            (live.account.clone(), live.connection.clone(), live.catch_up.clone(), live.history.clone())
        };
        Status {
            jawd: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL,
            root: self.home.display().to_string(),
            account,
            connection,
            engine,
            missing: self.missing.clone(),
            pools: self.pools.iter().map(|pool| pool.report()).collect(),
            catch_up,
            history,
            stored: self.store.as_ref().and_then(|store| {
                Some(Stored { inbox: store.count("inbox").ok()?, history: store.count("history_raw").ok()? })
            }),
        }
    }

    /// How long the engine takes to answer a ping, in microseconds, if it answers within half a second.
    pub async fn ping(&self) -> Option<u64> {
        let supervisor = self.engine().ok()?;
        let started = Instant::now();
        let ping = supervisor.call(pb::command::Kind::Ping(pb::Ping {}));
        match tokio::time::timeout(Duration::from_millis(500), ping).await {
            Ok(Ok(pb::reply::Kind::Pong(_))) => Some(started.elapsed().as_micros() as u64),
            _ => None,
        }
    }

    fn account(&self) -> Account {
        self.live.lock().expect("live state").account.clone()
    }

    fn connection(&self) -> Connection {
        self.live.lock().expect("live state").connection.clone()
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
