//! What jawd does with the engine's events. Everything WhatsApp won't send again (messages, app
//! state, push names, groups, pictures) is kept in the inbox and acked once it's committed; the
//! converter (`drain`) turns inbox rows into chats, messages and people as soon as they land, and
//! deletes each in the transaction its rows go in. History is fetched a blob at a time and
//! converted in the background lane (`fetch`). On connecting, contacts, groups, app state never
//! synced and the LID map are brought in (`refresh`), and avatars follow (`avatars`). Linking, the
//! connection and catching up go out to interfaces as pushes.

mod avatars;
mod drain;
mod fetch;
mod refresh;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use jaw_proto::sync::{Notice, Tone};
use jaw_proto::{Account, AccountState, CatchUp, Connection, ConnectionState, HistoryProgress, LinkStep, Me, Push};
use prost::Message as _;
use tokio::sync::{broadcast, mpsc};

pub use self::avatars::Avatars;
use crate::convert;
use crate::engine::supervisor::Supervisor;
use crate::engine::{EngineError, Incoming, pb};
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// The account's and the connection's state, as interfaces see them.
pub struct Live {
    pub account: Account,
    pub connection: Connection,
    pub catch_up: Option<CatchUp>,
    catching_up: Option<Instant>,
    pub history: Option<HistoryProgress>,
    /// The account's own JIDs, for mentions of it and its own reactions.
    me: convert::Me,
    /// The chats messages arrived in while catching up, for the summary at the end.
    caught_chats: HashSet<String>,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            account: Account { state: AccountState::Unlinked, me: None },
            connection: Connection::new(ConnectionState::Offline),
            catch_up: None,
            catching_up: None,
            history: None,
            me: convert::Me::default(),
            caught_chats: HashSet::new(),
        }
    }
}

impl Live {
    /// Whether jawd is catching up after time offline, between WhatsApp saying what's waiting and
    /// saying it has all come. Meanwhile rows update in place, and the chat list's order is held.
    pub fn catching_up(&self) -> bool {
        self.catching_up.is_some()
    }
}

pub type Shared = Arc<Mutex<Live>>;

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

fn push(pushes: &broadcast::Sender<Push>, method: &str, params: impl serde::Serialize) {
    let params = serde_json::to_value(params).expect("jawd's own types serialize");
    let _ = pushes.send(Push { method: method.into(), params });
}

/// How many messages pass between two `connection` pushes saying how far a catch-up has got.
const PROGRESS_EVERY: u64 = 100;

/// Where sync keeps the account's JIDs between runs.
const ME: &str = "me";
/// Where sync keeps whether jawd last stopped cleanly: "1" from a clean shutdown until the next
/// start, "0" while it runs.
const CLEAN: &str = "clean";

/// Makes changes in the live lane without holding up the caller, saying if they fail.
fn apply_soon(store: &Store, changes: Vec<Change>, what: &'static str) {
    let store = store.clone();
    tokio::spawn(async move {
        if let Err(e) = store.apply(Lane::Live, changes).await {
            eprintln!("jawd: can't store {what}: {e:#}");
        }
    });
}

/// Handles the engine's events until it stops sending them.
pub async fn run(
    mut incoming: mpsc::Receiver<Incoming>,
    store: Store,
    live: Shared,
    pushes: broadcast::Sender<Push>,
    engine: Option<Supervisor>,
    home: PathBuf,
) {
    if let Ok(Some(me)) = store.remembered(ME) {
        live.lock().expect("live state").me = convert::Me::new(me.split(' ').map(str::to_string));
    }
    // whatsmeow moves an app-state collection's version on in session.db before jawd has stored
    // what it carried, so after a stop that wasn't clean every collection is fetched again in full.
    let unclean = !matches!(store.remembered(CLEAN), Ok(Some(clean)) if clean == "1");
    if let Err(e) = store.apply(Lane::Live, vec![Change::Remember { key: CLEAN.into(), value: "0".into() }]).await {
        eprintln!("jawd: can't note that jawd is running: {e:#}");
    }
    let (offered, offers) = mpsc::unbounded_channel();
    let avatars = Avatars::new();
    tokio::spawn(drain::inbox(store.clone(), live.clone(), offered, avatars.clone()));
    tokio::spawn(drain::history_raw(store.clone(), live.clone()));
    if let Some(engine) = &engine {
        tokio::spawn(fetch::run(offers, store.clone(), engine.clone(), live.clone(), home.join("spool")));
        tokio::spawn(avatars.clone().run(store.clone(), engine.clone(), home.clone()));
    }
    let mut refreshed = false;
    let mut linked_now = false;
    while let Some(Incoming { event, ack }) = incoming.recv().await {
        let kept = |kind: &'static str, payload: Vec<u8>, ack| store.inbox(kind, now_ms(), payload, ack);
        match event.kind {
            Some(pb::event::Kind::Message(message)) => {
                let info = message.info.clone().unwrap_or_default();
                caught(&live, &pushes, &info.chat);
                store.inbox("message", info.timestamp_ms, message.encode_to_vec(), ack);
            }
            Some(pb::event::Kind::AppState(state)) => kept("app_state", state.encode_to_vec(), ack),
            Some(pb::event::Kind::PushName(name)) => kept("push_name", name.encode_to_vec(), ack),
            Some(pb::event::Kind::Group(group)) => kept("group", group.encode_to_vec(), ack),
            Some(pb::event::Kind::GroupChange(change)) => kept("group_change", change.encode_to_vec(), ack),
            Some(pb::event::Kind::Picture(picture)) => kept("picture", picture.encode_to_vec(), ack),
            Some(pb::event::Kind::Bench(bench)) => kept("bench", bench.payload, ack),
            Some(pb::event::Kind::AppStateSynced(synced)) => {
                let key = format!("{}{}", refresh::APP_STATE, synced.name);
                apply_soon(
                    &store,
                    vec![Change::Remember { key, value: synced.version.to_string() }],
                    "app state's version",
                );
            }
            Some(pb::event::Kind::Receipt(receipt)) => kept("receipt", receipt.encode_to_vec(), ack),
            Some(pb::event::Kind::Connection(connection)) => {
                let (pushed, me) = {
                    let mut live = live.lock().expect("live state");
                    apply(&mut live, &connection);
                    let connected =
                        connection.state() == pb::connection::State::Connected && !connection.jid.is_empty();
                    let me = connected.then(|| {
                        live.me = convert::Me::new([connection.jid.clone(), connection.lid.clone()]);
                        live.me.jids.join(" ")
                    });
                    (live.connection.clone(), me)
                };
                push(&pushes, "connection", pushed);
                if let Some(me) = me {
                    apply_soon(&store, vec![Change::Remember { key: ME.into(), value: me }], "the account's JIDs");
                    if !refreshed && let Some(engine) = &engine {
                        refreshed = true;
                        let resync = if linked_now {
                            refresh::Resync::None
                        } else if unclean {
                            refresh::Resync::All
                        } else {
                            refresh::Resync::Missing
                        };
                        tokio::spawn(refresh::run(store.clone(), engine.clone(), avatars.clone(), resync));
                    }
                }
            }
            Some(pb::event::Kind::Link(link)) => {
                if let Some(step) = link_step(link) {
                    linked_now |= matches!(step, LinkStep::Done { .. });
                    push(&pushes, "link", step);
                }
            }
            Some(pb::event::Kind::OfflinePreview(preview)) => {
                let connection = {
                    let mut live = live.lock().expect("live state");
                    live.catch_up = Some(CatchUp {
                        total: preview.total.into(),
                        messages: preview.messages.into(),
                        receipts: preview.receipts.into(),
                        stored: 0,
                        seconds: None,
                    });
                    live.catching_up = Some(Instant::now());
                    live.caught_chats.clear();
                    live.connection.state = ConnectionState::CatchingUp;
                    live.connection.total = Some(preview.messages.into());
                    live.connection.done = Some(0);
                    live.connection.clone()
                };
                push(&pushes, "connection", connection);
            }
            Some(pb::event::Kind::OfflineDone(done)) => {
                let (connection, summary) = caught_up(&mut live.lock().expect("live state"), &done);
                push(&pushes, "connection", connection);
                if let Some(summary) = summary {
                    push(&pushes, "notice", Notice { text: summary, tone: Tone::Info });
                }
            }
            None => {}
        }
    }
}

/// Ends jawd's run cleanly: the engine stops taking events from WhatsApp, what it was handing over
/// is kept, and then jaw.db says the stop was clean, so the next start needn't fetch app state again.
pub async fn stopped(store: &Store, engine: Option<&Supervisor>) {
    if let Some(engine) = engine {
        let stop = engine.call(pb::command::Kind::LinkCancel(pb::LinkCancel {}));
        if tokio::time::timeout(STOPPING, stop).await.is_err() {
            eprintln!("jawd: the engine didn't stop in time, so the next start fetches app state again");
            return;
        }
        tokio::time::sleep(SETTLING).await;
    }
    if let Err(e) = store.apply_durable(vec![Change::Remember { key: CLEAN.into(), value: "1".into() }]).await {
        eprintln!("jawd: can't note a clean stop: {e:#}");
    }
}

/// How long the engine has to disconnect at a clean stop, and how long what it was handing over
/// has to be kept after.
const STOPPING: std::time::Duration = std::time::Duration::from_secs(2);
const SETTLING: std::time::Duration = std::time::Duration::from_millis(300);

/// Counts a message that arrived while catching up, and says how far it has got now and then.
fn caught(live: &Shared, pushes: &broadcast::Sender<Push>, chat: &str) {
    let progress = {
        let mut live = live.lock().expect("live state");
        if live.catching_up.is_none() {
            return;
        }
        live.caught_chats.insert(convert::user_jid(chat));
        let Some(catch_up) = live.catch_up.as_mut() else { return };
        catch_up.stored += 1;
        let stored = catch_up.stored;
        live.connection.done = Some(stored);
        (stored % PROGRESS_EVERY == 0).then(|| live.connection.clone())
    };
    if let Some(connection) = progress {
        push(pushes, "connection", connection);
    }
}

/// Ends a catch-up: the connection is online, and the summary of what arrived, if anything did.
fn caught_up(live: &mut Live, done: &pb::OfflineDone) -> (Connection, Option<String>) {
    let took = live.catching_up.take().map(|started| started.elapsed().as_secs_f64());
    let chats = std::mem::take(&mut live.caught_chats).len();
    let mut summary = None;
    if let (Some(catch_up), Some(seconds)) = (live.catch_up.as_mut(), took) {
        catch_up.seconds = Some(seconds);
        eprintln!(
            "jawd: caught up: {} messages stored of {} waiting ({} things in all, {} said done) in {seconds:.2} s, {:.0} messages a second",
            catch_up.stored,
            catch_up.messages,
            catch_up.total,
            done.count,
            catch_up.stored as f64 / seconds.max(1e-6)
        );
        summary = match (catch_up.stored, chats) {
            (0, _) => None,
            (1, _) => Some("While you were away: 1 message".to_string()),
            (n, 1) => Some(format!("While you were away: {n} messages in 1 chat")),
            (n, chats) => Some(format!("While you were away: {n} messages in {chats} chats")),
        };
    }
    live.connection = Connection::new(ConnectionState::Online);
    (live.connection.clone(), summary)
}

/// The account and connection as a Connection event says they are now.
fn apply(live: &mut Live, connection: &pb::Connection) {
    use pb::connection::State;
    let reason = (!connection.reason.is_empty()).then(|| connection.reason.clone());
    let state = match connection.state() {
        State::Unspecified | State::Disconnected | State::StreamReplaced | State::ConnectFailed => {
            ConnectionState::Offline
        }
        State::Unlinked => ConnectionState::Offline,
        State::Connecting | State::KeepaliveTimeout => ConnectionState::Connecting,
        State::Connected => ConnectionState::Online,
        State::LoggedOut => ConnectionState::LoggedOut,
        State::ClientOutdated => ConnectionState::UpdateNeeded,
        State::TemporaryBan => ConnectionState::Banned,
    };
    if matches!(connection.state(), State::Unlinked | State::LoggedOut) {
        live.account = Account { state: AccountState::Unlinked, me: None };
    } else if !connection.jid.is_empty() {
        let phone = connection.jid.split(['@', ':', '.']).next().unwrap_or_default();
        // A name WhatsApp said in app state stands until the engine knows one too.
        let known = live.account.me.as_ref().map(|me| me.name.clone()).unwrap_or_default();
        let name = if connection.push_name.is_empty() { known } else { connection.push_name.clone() };
        live.account = Account {
            state: AccountState::Linked,
            me: Some(Me { id: connection.jid.clone(), name, phone: format!("+{phone}") }),
        };
    }
    if live.catching_up() && state == ConnectionState::Online {
        return;
    }
    live.connection = Connection::new(state);
    live.connection.reason = reason;
    if state == ConnectionState::Banned && connection.until_ms > 0 {
        live.connection.until = Some(connection.until_ms);
    }
}

fn link_step(link: pb::Link) -> Option<LinkStep> {
    Some(match link.step? {
        pb::link::Step::Qr(qr) => LinkStep::Qr { code: qr.code, expires: qr.expires_ms },
        pb::link::Step::Code(code) => LinkStep::Code { code },
        pb::link::Step::Done(done) => {
            let phone = done.jid.split(['@', ':', '.']).next().unwrap_or_default();
            LinkStep::Done { phone: format!("+{phone}") }
        }
        pb::link::Step::Failed(reason) => LinkStep::Failed { reason },
    })
}

/// How many messages `history.more` asks the phone for.
const MORE: u32 = 50;

/// Why `history.more` couldn't ask, as the protocol's error.
pub enum Refused {
    NotFound(String),
    Engine(EngineError),
}

/// Asks the phone for the MORE messages before the oldest jaw has in `chat`.
pub async fn history_more(store: &Store, engine: &Supervisor, chat: String) -> Result<(), Refused> {
    let asked = chat.clone();
    let oldest = store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(db
                .query_row(
                    "SELECT m.id, m.mine, m.at FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1
                    ORDER BY m.at, m.n LIMIT 1",
                    [&asked],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, i64>(2)?)),
                )
                .ok())
        })
        .await
        .map_err(|e| Refused::NotFound(format!("can't read the chat: {e:#}")))?;
    let Some((id, mine, at)) = oldest else {
        return Err(Refused::NotFound(format!("jaw has no messages in {chat} to ask for older ones before")));
    };
    let request = pb::HistoryRequest { chat, oldest_id: id, oldest_from_me: mine, oldest_ms: at, count: MORE };
    engine.call(pb::command::Kind::HistoryRequest(request)).await.map(|_| ()).map_err(Refused::Engine)
}

#[cfg(test)]
mod tests;
