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
use crate::layout::Layouts;
use crate::media::Media;
use crate::send::Sending;
use crate::settings::Settings;
use crate::sock::{APART_MOST, LANE_MOST, Lanes};
use crate::store::{Store, contacts, copies, expiry, kinds, links};
use crate::sync::Shared;
use crate::voice::Voice;
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
    /// Every connection's lane and waiting reply, for `status`.
    pub lanes: Lanes,
    /// Whether jawd is stopping, so connections end and take no more work.
    stopping: watch::Sender<bool>,
    sending: Option<Sending>,
    layouts: Option<Layouts>,
    settings: Option<Arc<Settings>>,
    following: Option<(crate::presence::Following, Arc<crate::organise::Checks>)>,
    media: Option<Media>,
    voice: Option<Voice>,
    expiry: Option<Arc<crate::expiry::Expiry>>,
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
            missing: missing(&[]),
            pools,
            live,
            store,
            pushes,
            finder: OnceLock::new(),
            feed: None,
            attention: Arc::default(),
            notifications: "off".into(),
            lanes: Lanes::default(),
            stopping: watch::channel(false).0,
            sending: None,
            layouts: None,
            settings: None,
            following: None,
            media: None,
            voice: None,
            expiry: None,
        })
    }

    /// Says in `status` what the timer for disappearing messages has erased and waits for.
    pub fn with_expiry(mut self, expiry: Arc<crate::expiry::Expiry>) -> State {
        self.expiry = Some(expiry);
        self
    }

    /// Fetches, saves and keeps media through `media`.
    pub fn with_media(mut self, media: Media) -> State {
        self.media = Some(media);
        self
    }

    pub fn media(&self) -> Option<&Media> {
        self.media.as_ref()
    }

    /// Plays and records voice messages through `voice`.
    pub fn with_voice(mut self, voice: Voice) -> State {
        self.voice = Some(voice);
        self
    }

    pub fn voice(&self) -> Option<&Voice> {
        self.voice.as_ref()
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

    /// Takes the programs in `without` for missing, whether or not they're installed, as a machine
    /// without them would.
    pub fn without(mut self, without: &[String]) -> State {
        self.missing = missing(without);
        self
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

    /// Keeps interfaces' layouts with `layouts`.
    pub fn with_layouts(mut self, layouts: Layouts) -> State {
        self.layouts = Some(layouts);
        self
    }

    pub fn layouts(&self) -> Option<&Layouts> {
        self.layouts.as_ref()
    }

    /// Keeps each frontend's settings in `settings`.
    pub fn with_settings(mut self, settings: Arc<Settings>) -> State {
        self.settings = Some(settings);
        self
    }

    pub fn settings(&self) -> Option<&Settings> {
        self.settings.as_deref()
    }

    /// Whose presence it follows, and the folders' checks, for `status`.
    pub fn with_following(
        mut self,
        following: crate::presence::Following,
        checks: Arc<crate::organise::Checks>,
    ) -> State {
        self.following = Some((following, checks));
        self
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
            jawd: jaw_version::VERSION.into(),
            root: self.home.display().to_string(),
            seq: self.feed.as_ref().map_or(0, Feed::seq),
            account: self.account(),
            connection: self.connection(),
            settings: None,
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
            jawd: jaw_version::VERSION.into(),
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
                let (link_thumbnails, link_thumbnail_bytes) = store.thumbnails_kept().ok()?;
                let (spool_files, spool_bytes) = crate::sync::spooled(&self.home.join("spool"));
                Some(Stored {
                    inbox: store.count("inbox").ok()?,
                    history: store.count("history_raw").ok()?,
                    spool_files,
                    spool_bytes,
                    chats: store.count("chats").ok()?,
                    messages: store.count("messages").ok()?,
                    people: store.count("people").ok()?,
                    unindexed: store.count("fts_backlog").ok()?,
                    quarantined: store.count("quarantine").ok()?,
                    merging: store.count("chat_merges").ok()?,
                    merges_stuck: self.live.lock().expect("live state").merges_stuck,
                    outbox: store.count("outbox").ok()?,
                    unconfirmed: store.count("outbox WHERE state = 'unconfirmed'").ok()?,
                    actions_kept: store
                        .count("outbox WHERE state = 'failed' AND kind IN ('reaction', 'edit', 'revoke', 'vote')")
                        .ok()?,
                    awaiting: store.count("app_state_awaiting").ok()?,
                    tombstones: store.count("tombstones").ok()?,
                    recipients: store.count("recipients").ok()?,
                    regroup: store.count("regroup").ok()?,
                    settled: store.count("settled").ok()?,
                    send_keys: store.count("send_keys").ok()?,
                    links: store.count("links").ok()?,
                    links_backfill: store.backfill_left(links::LINKS).ok()?,
                    link_thumbnails,
                    link_thumbnail_bytes,
                    link_thumbnails_refused: store.thumbnails().refused(),
                    votes_waiting: store.count("pending WHERE kind = 'vote'").ok()?,
                    votes_set_aside: store.count("quarantine WHERE source = 'vote'").ok()?,
                    polls_backfill: store.backfill_left(links::POLLS).ok()?,
                    kinds_backfill: store.backfill_left(kinds::KINDS).ok()?,
                    copies_backfill: store.backfill_left(copies::COPIES).ok()?,
                    contacts_backfill: store.backfill_left(contacts::CONTACTS).ok()?,
                    voices_backfill: store.backfill_left(links::VOICES).ok()?,
                    once_backfill: store.backfill_left(expiry::VIEW_ONCE).ok()?,
                    encrypted_kept: store.count("pending WHERE kind IN ('secret', 'rsvp')").ok()?,
                    places_backfill: store.backfill_left(links::PLACES).ok()?,
                    events_backfill: store.backfill_left(links::EVENTS).ok()?,
                    answers_waiting: store.count("pending WHERE kind = 'answer'").ok()?,
                    answers_set_aside: store.count("quarantine WHERE source = 'answer'").ok()?,
                    edits_set_aside: store.count("quarantine WHERE source = 'edit'").ok()?,
                    events_opening: store.count("openings").ok()?,
                    events_unopened: store.count("unopened").ok()?,
                })
            }),
            interfaces: Some(Interfaces {
                connected: self.attention.connected() as u32,
                focused: self.focused(),
                notifications: self.notifications.clone(),
                in_lanes: self.lanes.requests(),
                lane_most: LANE_MOST as u64,
                lanes_full: self.lanes.full(),
                replies_waiting: self.lanes.waiting(),
                apart: self.lanes.apart(),
                apart_most: APART_MOST as u64,
            }),
            windows: self.feed.as_ref().map(Feed::windows),
            storage: self.store.as_ref().map(Store::storage),
            disk: self.store.as_ref().and_then(|store| store.disk().status()),
            layouts: self.layouts.as_ref().map(Layouts::status),
            typing: self.feed.as_ref().map(|feed| feed.typing().stats()),
            settings: self.settings.as_ref().map(|s| s.by_domain()).unwrap_or_default(),
            settings_ignored: self.settings.as_ref().map(|s| s.ignored()).unwrap_or_default(),
            presence: self.following.as_ref().map(|(following, _)| following.status()),
            folders: self.following.as_ref().map(|(_, checks)| checks.status()),
            media: self.media.as_ref().map(Media::status),
            voice: self.voice.as_ref().map(Voice::status),
            disappearing: self.store.as_ref().zip(self.expiry.as_ref()).map(|(store, expiry)| {
                let mut status = expiry.status(store);
                status.open = self.sending.as_ref().map_or(0, crate::send::Sending::open);
                status.open_most = crate::store::expiry::OPEN_MOST as u64;
                status
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
/// a LaunchAgent's PATH doesn't include it; those in `without` are missing however they're found.
fn missing(without: &[String]) -> Vec<Missing> {
    let wanted = [
        ("ffmpeg", "sending videos, GIFs and audio files, and video posters"),
        ("ffprobe", "reading what a file to send is"),
    ];
    let mut folders: Vec<PathBuf> =
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    folders.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    wanted
        .into_iter()
        .filter(|(program, _)| {
            without.iter().any(|w| w == program) || !folders.iter().any(|folder| folder.join(program).is_file())
        })
        .map(|(program, needed_for)| Missing { what: program.into(), needed_for: needed_for.into() })
        .collect()
}
