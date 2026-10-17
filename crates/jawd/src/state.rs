//! What jawd knows about itself, for `hello` and `status`.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use jaw_proto::{Account, Connection, Engine, Interfaces, Missing, PROTOCOL, Push, Status, Stored, Welcome};
use tokio::sync::{broadcast, watch};

use crate::attention::Attention;
use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::feed::Feed;
use crate::find::Finder;
use crate::send::Sending;
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
    /// The palette's search, once its index has loaded.
    pub finder: OnceLock<Finder>,
    feed: Option<Feed>,
    attention: Arc<Attention>,
    notifications: String,
    /// Whether jawd is stopping, so connections end and take no more work.
    stopping: watch::Sender<bool>,
    sending: Option<Sending>,
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
        Ok(State {
            home: home.to_path_buf(),
            owner,
            engine,
            missing: missing(),
            pools,
            live,
            store,
            pushes,
            finder: OnceLock::new(),
            feed: None,
            attention: Arc::default(),
            notifications: "off".into(),
            stopping: watch::channel(false).0,
            sending: None,
        })
    }

    /// What each interface is looking at: whether one is in front, and which chats they read.
    pub fn attention(&self) -> &Arc<Attention> {
        &self.attention
    }

    /// Keeps what each interface is looking at in `attention`, which others follow too.
    pub fn with_attention(mut self, attention: Arc<Attention>) -> State {
        self.attention = attention;
        self
    }

    /// Whether any interface is in front.
    pub fn focused(&self) -> bool {
        self.attention.focused()
    }

    /// Says in `status` where notifications go, or why they don't.
    pub fn with_notifications(mut self, notifications: String) -> State {
        self.notifications = notifications;
        self
    }

    /// Ends every connection, and refuses what they'd still send.
    pub fn stop(&self) {
        self.stopping.send_replace(true);
    }

    /// Waits until jawd is stopping.
    pub async fn stopping(&self) {
        let mut stopping = self.stopping.subscribe();
        let _ = stopping.wait_for(|stopping| *stopping).await;
    }

    /// Sends, reads and types through `sending`.
    pub fn with_sending(mut self, sending: Sending) -> State {
        self.sending = Some(sending);
        self
    }

    pub fn sending(&self) -> Option<&Sending> {
        self.sending.as_ref()
    }

    /// Serves windows from `feed`.
    pub fn with_feed(mut self, feed: Feed) -> State {
        self.feed = Some(feed);
        self
    }

    pub fn feed(&self) -> Option<&Feed> {
        self.feed.as_ref()
    }

    /// The account's and the connection's state, which sync keeps.
    pub fn live(&self) -> &Shared {
        &self.live
    }

    pub fn store(&self) -> Option<&Store> {
        self.store.as_ref()
    }

    pub fn engine(&self) -> Result<&Supervisor, &str> {
        self.engine.as_ref().map_err(|(_, why)| why.as_str())
    }

    pub fn welcome(&self) -> Welcome {
        Welcome {
            protocol: PROTOCOL,
            jawd: env!("CARGO_PKG_VERSION").into(),
            root: self.home.display().to_string(),
            seq: self.feed.as_ref().map_or(0, Feed::seq),
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
                Some(Stored {
                    inbox: store.count("inbox").ok()?,
                    history: store.count("history_raw").ok()?,
                    chats: store.count("chats").ok()?,
                    messages: store.count("messages").ok()?,
                    people: store.count("people").ok()?,
                    unindexed: store.count("fts_backlog").ok()?,
                    quarantined: store.count("quarantine").ok()?,
                    merging: store.count("chat_merges").ok()?,
                    merges_stuck: self.live.lock().expect("live state").merges_stuck,
                    outbox: store.count("outbox").ok()?,
                    unconfirmed: store.count("outbox WHERE state = 'unconfirmed'").ok()?,
                    recipients: store.count("recipients").ok()?,
                    regroup: store.count("regroup").ok()?,
                    settled: store.count("settled").ok()?,
                    send_keys: store.count("send_keys").ok()?,
                })
            }),
            interfaces: Some(Interfaces {
                connected: self.attention.connected() as u32,
                focused: self.focused(),
                notifications: self.notifications.clone(),
            }),
            storage: self.store.as_ref().map(Store::storage),
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
