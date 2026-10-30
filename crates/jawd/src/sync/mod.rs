//! What jawd does with the engine's events. Everything WhatsApp won't send again (messages, app
//! state, push names, groups, pictures) is kept in the inbox and acked once it's committed; the
//! converter (`drain`) turns inbox rows into chats, messages and people as soon as they land, and
//! deletes each in the transaction its rows go in. History is fetched a blob at a time and
//! converted in the background lane (`fetch`). On connecting, contacts, groups, app state never
//! synced and the LID map are brought in (`refresh`), and avatars follow (`avatars`). Linking, the
//! connection and catching up go out to interfaces as pushes.

mod avatars;
pub(crate) mod backfill;
mod drain;
mod fetch;
mod merge;
mod refresh;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jaw_proto::sync::{Notice, Tone};
use jaw_proto::{Account, AccountState, CatchUp, Connection, ConnectionState, HistoryProgress, LinkStep, Me, Push};
use prost::Message as _;
use tokio::sync::{broadcast, mpsc, watch};

pub use self::avatars::Avatars;
pub use self::merge::Merges;
use crate::convert;
use crate::engine::supervisor::{EngineStatus, Supervisor};
use crate::engine::{EngineError, Incoming, pb};
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// The account's and the connection's state, as interfaces see them.
pub struct Live {
    pub account: Account,
    pub connection: Connection,
    pub catch_up: Option<CatchUp>,
    catching_up: Option<Instant>,
    /// Whether the connection running now has said Connected, which whatsmeow says once a
    /// connection, and only once the IQs after its handshake are answered, so after the
    /// connection's own keepalive timeout at times.
    said_connected: bool,
    pub history: Option<HistoryProgress>,
    /// The account's own JIDs, for mentions of it and its own reactions, once jaw.db keeps them;
    /// nothing is converted until then (`stored_me`).
    me: watch::Sender<convert::Me>,
    /// The chats messages arrived in while catching up, for the summary at the end.
    caught_chats: HashSet<String>,
    /// Merges whose steps kept failing, left until jawd starts again.
    pub merges_stuck: u64,
    /// When jawd first asked the phone something it's known to answer and hasn't yet, older
    /// history so far. WhatsApp never says the phone is offline, so a question unanswered for
    /// `phone_patience` is how jawd finds out.
    phone: watch::Sender<Option<Instant>>,
    /// The chats whose history the phone has said, in this run, it keeps more of than it has sent,
    /// so a question about one has an answer to come.
    more_on_phone: HashSet<String>,
    pub phone_patience: Duration,
    pub phone_give_up: Duration,
    /// Where the engine's word on how far each download has got goes, once media has started.
    pub media_progress: Option<mpsc::Sender<pb::MediaProgress>>,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            account: Account { state: AccountState::Unlinked, me: None },
            connection: Connection::new(ConnectionState::Offline),
            catch_up: None,
            catching_up: None,
            said_connected: false,
            history: None,
            me: watch::Sender::new(convert::Me::default()),
            caught_chats: HashSet::new(),
            merges_stuck: 0,
            phone: watch::Sender::new(None),
            more_on_phone: HashSet::new(),
            phone_patience: PHONE_PATIENCE,
            phone_give_up: PHONE_GIVE_UP,
            media_progress: None,
        }
    }
}

/// How long the phone has to answer before jawd says it's offline, counted while the connection is
/// online, and afresh each time it comes back, since the answer may be waiting for it: an online
/// phone answers a request for older history in a few seconds, one woken by a push in a few more.
pub const PHONE_PATIENCE: Duration = Duration::from_secs(20);
/// How long after asking jawd gives a question up, so one the phone never answers (a request lost
/// on the way, say) can't have jawd say it's offline for ever.
pub const PHONE_GIVE_UP: Duration = Duration::from_secs(120);

/// Notes that jawd has asked the phone something it's known to answer, unless it's still waiting
/// for an earlier answer; says when, if this is the question it waits for now.
fn asked_phone(live: &Shared) -> Option<Instant> {
    let now = Instant::now();
    let noted = live.lock().expect("live state").phone.send_if_modified(|asked| {
        if asked.is_some() {
            return false;
        }
        *asked = Some(now);
        true
    });
    noted.then_some(now)
}

/// The phone is online if it sends anything: history, an answer to a peer's request, a message
/// sent from it or a receipt. It's the account's device 0, whose JID is the account's own with no
/// device, where another of your devices has one.
fn heard_from(live: &Shared, sender: &str) {
    let mut live = live.lock().expect("live state");
    let phone = live.me.borrow().jids.iter().any(|jid| jid == sender);
    if phone {
        live.forget_question();
    }
}

impl Live {
    /// Stops waiting for the phone's answer: it has come, or it's given up.
    fn forget_question(&mut self) {
        self.phone.send_if_modified(|asked| asked.take().is_some());
    }

    /// Forgets what the phone said and was asked: the device it would answer is gone.
    fn forget_phone(&mut self) {
        self.forget_question();
        self.more_on_phone.clear();
    }

    /// Notes what a history blob says of each chat's history the phone keeps: more of it than it
    /// sent, or none.
    fn phone_keeps(&mut self, remains: Vec<(String, bool)>) {
        for (chat, more) in remains {
            if more {
                self.more_on_phone.insert(chat);
            } else {
                self.more_on_phone.remove(&chat);
            }
        }
    }

    /// Whether jawd is catching up after time offline, between WhatsApp saying what's waiting and
    /// saying it has all come. Meanwhile rows update in place, and the chat list's order is held.
    pub fn catching_up(&self) -> bool {
        self.catching_up.is_some()
    }

    /// Ends the connection running now: a catch-up it cut short ends, as its end would but for the
    /// summary, so the chat list and notifications go on with what it brought, and what it didn't
    /// bring comes in the next connection's; and the next Connected is the next connection's.
    fn cut_short(&mut self) {
        self.said_connected = false;
        if let Some(started) = self.catching_up.take() {
            self.caught_chats.clear();
            if let Some(catch_up) = self.catch_up.as_mut() {
                catch_up.seconds = Some(started.elapsed().as_secs_f64());
            }
        }
    }

    /// The account's own JIDs as they're kept, empty until the engine has said which account is
    /// linked.
    pub fn me(&self) -> watch::Receiver<convert::Me> {
        self.me.subscribe()
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
/// Where sync keeps the linked account as interfaces see it (`Me` as JSON) between runs, so jawd
/// says it's linked before its engine connects: written when the engine connects, and emptied when
/// the account is logged out or the engine says it isn't linked.
///
/// `ME` and `ACCOUNT` are caches of what WhatsApp said through the engine, kept in sync_state,
/// which is jaw's own: a rebuild may copy them and a restore bring an old one back, and the
/// engine's first word replaces them, so nothing durable is decided from them.
const ACCOUNT: &str = "account";
/// Why an account jawd said was linked isn't any more, when its engine finds no device.
const NOT_LINKED: &str = "this device isn't linked any more";
/// Where sync keeps whether jawd last stopped cleanly: "1" from a clean shutdown until the next
/// start, "0" while it runs, written before the engine starts.
const CLEAN: &str = "clean";

/// Notes durably that jawd is running, before the engine starts, so a stop that isn't clean is
/// known at the next start; says so when the last one wasn't.
pub async fn running(store: &Store) -> anyhow::Result<()> {
    if matches!(store.remembered(CLEAN)?, Some(clean) if clean == "0") {
        eprintln!("jawd: the last run didn't stop cleanly");
    }
    store.apply_durable(vec![Change::Remember { key: CLEAN.into(), value: "0".into() }]).await?;
    Ok(())
}

/// The account and connection as jawd knows them at start, before the engine says anything: linked
/// and connecting (offline with no engine to connect) when an account was linked at the last
/// connection and not logged out since, and unlinked otherwise. The engine's first word corrects
/// it: unlinked when session.db has no device, logged out when WhatsApp says so.
pub fn starting(store: &Store, engine: bool) -> Live {
    let mut live = Live::default();
    if let Ok(Some(me)) = store.remembered(ME) {
        live.me.send_replace(convert::Me::new(me.split(' ').map(str::to_string)));
    }
    let kept = store.remembered(ACCOUNT).ok().flatten().filter(|json| !json.is_empty());
    let account = kept.and_then(|json| match serde_json::from_str::<Me>(&json) {
        Ok(me) => Some(me),
        Err(e) => {
            eprintln!("jawd: the account jaw.db keeps doesn't read ({e}), so it starts unlinked until the engine says");
            None
        }
    });
    if let Some(me) = account {
        live.account = Account { state: AccountState::Linked, me: Some(me) };
        live.connection = Connection::new(if engine { ConnectionState::Connecting } else { ConnectionState::Offline });
    }
    live
}

/// Ends the account after a logout jawd asked for, as WhatsApp logging it out would, since whatsmeow
/// says nothing after one: jaw.db forgets the account, synced, and a catch-up the logout cut short
/// ends. The `connection` push it returns tells interfaces it's logged out, so they offer to link
/// again; the caller sends it once the logout's answer has gone ahead of it.
pub async fn logged_out(store: &Store, live: &Shared, reason: &str) -> anyhow::Result<Push> {
    store.apply_durable(vec![Change::Remember { key: ACCOUNT.into(), value: String::new() }]).await?;
    let connection = {
        let mut live = live.lock().expect("live state");
        live.account = Account { state: AccountState::Unlinked, me: None };
        live.forget_phone();
        live.cut_short();
        live.connection = Connection::new(ConnectionState::LoggedOut);
        live.connection.reason = Some(reason.into());
        live.connection.clone()
    };
    let params = serde_json::to_value(connection).expect("jawd's own types serialize");
    Ok(Push { method: "connection".into(), params })
}

/// Keeps the account's JIDs in jaw.db without holding up the caller, trying again until they're
/// in, and only then gives them to what converts: the store reads them there for a group
/// message's ticks, so nothing may be converted with them before they're committed.
fn remember(store: &Store, live: &Shared, me: convert::Me) {
    let (store, live) = (store.clone(), live.clone());
    tokio::spawn(async move {
        let mut pause = REMEMBERING_FIRST;
        loop {
            let kept = vec![Change::Remember { key: ME.into(), value: me.jids.join(" ") }];
            let why = match store.apply(Lane::Live, kept).await {
                Ok(applied) if applied.quarantined == 0 => break,
                Ok(_) => "it was kept in quarantine".to_string(),
                Err(e) => format!("{e:#}"),
            };
            eprintln!("jawd: can't store the account's JIDs, so they're tried again in {pause:?}: {why}");
            tokio::time::sleep(pause).await;
            pause = (pause * 2).min(REMEMBERING_MOST);
        }
        live.lock().expect("live state").me.send_replace(me);
    });
}

/// How long storing the account's JIDs waits before trying again, at first and at most.
const REMEMBERING_FIRST: std::time::Duration = std::time::Duration::from_millis(10);
const REMEMBERING_MOST: std::time::Duration = std::time::Duration::from_secs(1);

/// The account's own JIDs once jaw.db keeps them, waiting until it does. Nothing is converted
/// without them: the account's own reactions are kept under its JID, mentions of it are found by
/// them, and the store reads them for a group message's ticks. So `what` stays where it is, whole
/// and in order, until the engine has said which account is linked.
async fn stored_me(live: &Shared, what: &str) -> convert::Me {
    let mut stored = live.lock().expect("live state").me.subscribe();
    if stored.borrow().jids.is_empty() {
        eprintln!(
            "jawd: {what} waits until the engine says which account is linked: converting it needs the account's JIDs"
        );
    }
    let me = stored.wait_for(|me| !me.jids.is_empty()).await.expect("live state keeps the account's JIDs");
    me.clone()
}

/// Handles the engine's events until it stops sending them. A message from someone ends their
/// typing in its chat, which `typing` keeps in memory, never in jaw.db.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    mut incoming: mpsc::Receiver<Incoming>,
    store: Store,
    live: Shared,
    pushes: broadcast::Sender<Push>,
    engine: Option<Supervisor>,
    home: PathBuf,
    following: crate::presence::Following,
    typing: crate::feed::Typing,
) {
    let (offered, offers) = mpsc::unbounded_channel();
    let avatars = Avatars::new();
    let merges = Merges::default();
    tokio::spawn(merge::run(store.clone(), merges.clone(), live.clone(), merge::Timing::default()));
    tokio::spawn(backfill::run(store.clone()));
    tokio::spawn(drain::inbox(store.clone(), live.clone(), offered, avatars.clone(), merges.clone()));
    tokio::spawn(drain::history_raw(store.clone(), live.clone()));
    if let Some(engine) = &engine {
        tokio::spawn(fetch::run(
            offers,
            store.clone(),
            engine.clone(),
            live.clone(),
            home.join("spool"),
            merges.clone(),
        ));
        tokio::spawn(avatars.clone().run(store.clone(), engine.clone(), home.clone()));
    }
    // whatsmeow moves an app-state collection's version on in session.db before it hands over what
    // the version carried, so an engine that stopped in between leaves changes jawd never got. Each
    // engine that connects (the first, and each one started again) fetches every collection again
    // in full, which changes nothing that's already right. The restarts count says which engine.
    let mut refreshed: Option<u32> = None;
    let mut linked_now = false;
    let mut status = engine.as_ref().map(Supervisor::watch);
    // The stops sync has said, as the supervisor counts them; it starts the next engine only once
    // sync has said the last, so nothing the next engine sends can come before it.
    let mut said = engine.as_ref().map_or(0, Supervisor::follow);
    let (mut phone, timing) = {
        let live = live.lock().expect("live state");
        (live.phone.subscribe(), PhoneTiming { patience: live.phone_patience, give_up: live.phone_give_up })
    };
    // Since when the connection has been online without a break, which the phone's patience
    // counts from.
    let mut online_since: Option<Instant> = None;
    loop {
        let online = matches!(
            live.lock().expect("live state").connection.state,
            ConnectionState::Online | ConnectionState::PhoneOffline
        );
        online_since = online.then(|| online_since.unwrap_or_else(Instant::now));
        // An engine's events are all in `incoming` before the supervisor counts its stop, and the
        // next engine's can't come before sync has said it, so taking events first keeps the order
        // they came in. What jawd says of the phone is said here too, in order with the engine's.
        let Incoming { event, ack } = tokio::select! {
            biased;
            incoming = incoming.recv() => match incoming {
                Some(incoming) => incoming,
                None => break,
            },
            stops = stopped(&mut status, said) => {
                restarting(&live, &pushes);
                said = stops;
                if let Some(engine) = &engine {
                    engine.taken_in(stops);
                }
                continue;
            }
            () = phone_due(&mut phone, &live, timing, online_since) => {
                phone_said(&live, &pushes, timing, online_since);
                continue;
            }
        };
        let kept = |kind: &'static str, payload: Vec<u8>, ack| store.inbox(kind, now_ms(), payload, ack);
        match event.kind {
            Some(pb::event::Kind::Message(message)) => {
                let info = message.info.clone().unwrap_or_default();
                heard_from(&live, &info.sender);
                if !info.from_me {
                    typing.message(&info.chat, &info.sender, &info.sender_alt);
                }
                caught(&live, &pushes, &info.chat);
                store.inbox("message", info.timestamp_ms, message.encode_to_vec(), ack);
            }
            Some(pb::event::Kind::AppState(state)) => kept("app_state", state.encode_to_vec(), ack),
            Some(pb::event::Kind::PushName(name)) => kept("push_name", name.encode_to_vec(), ack),
            Some(pb::event::Kind::Group(group)) => kept("group", group.encode_to_vec(), ack),
            Some(pb::event::Kind::GroupChange(change)) => kept("group_change", change.encode_to_vec(), ack),
            Some(pb::event::Kind::Picture(picture)) => kept("picture", picture.encode_to_vec(), ack),
            Some(pb::event::Kind::Bench(bench)) => kept("bench", bench.payload, ack),
            // Each engine fetches app state again in full, so jawd needn't keep the versions.
            Some(pb::event::Kind::AppStateSynced(_)) => {}
            Some(pb::event::Kind::Receipt(receipt)) => {
                heard_from(&live, &receipt.sender);
                kept("receipt", receipt.encode_to_vec(), ack)
            }
            // Fire and forget: what's around now, written as it comes and gone by the next start.
            Some(pb::event::Kind::Presence(presence)) => following.heard(presence),
            // Who's typing is never written to jaw.db: the presence and typing step (presence-typing)
            // keeps it in jawd's memory and pushes it as `typing`. Until it lands, it goes no further.
            Some(pb::event::Kind::Typing(_)) => {}
            Some(pb::event::Kind::MediaRetried(answer)) => kept("media_retry", answer.encode_to_vec(), ack),
            // Progress is said again a moment later, so one that finds no room is let go.
            Some(pb::event::Kind::MediaProgress(progress)) => {
                if let Some(media) = live.lock().expect("live state").media_progress.as_ref() {
                    let _ = media.try_send(progress);
                }
            }
            Some(pb::event::Kind::Connection(connection)) => {
                let (pushed, me, account) = {
                    let mut live = live.lock().expect("live state");
                    let was_linked = live.account.state == AccountState::Linked;
                    if !apply(&mut live, &connection) {
                        continue;
                    }
                    let connected =
                        connection.state() == pb::connection::State::Connected && !connection.jid.is_empty();
                    let me = connected.then(|| convert::Me::new([connection.jid.clone(), connection.lid.clone()]));
                    let account = match connection.state() {
                        // Unlinked before, the account is already empty.
                        pb::connection::State::Unlinked | pb::connection::State::LoggedOut => {
                            was_linked.then(String::new)
                        }
                        _ if connected => {
                            live.account.me.as_ref().map(|me| serde_json::to_string(me).expect("Me serializes"))
                        }
                        _ => None,
                    };
                    (live.connection.clone(), me, account)
                };
                push(&pushes, "connection", pushed);
                // Waited for, so a logout and a later connection are kept in the order they came. It
                // holds the loop for one live commit, a synced one for a logout, once a connection.
                // whatsmeow can dispatch Connected with the removed device's JID after LoggedOut,
                // which the engine drops: once an account is logged out, only what jawd must store
                // goes on from its client.
                if let Some(account) = account {
                    let forget = account.is_empty();
                    let change = vec![Change::Remember { key: ACCOUNT.into(), value: account }];
                    let kept =
                        if forget { store.apply_durable(change).await } else { store.apply(Lane::Live, change).await };
                    match kept {
                        Ok(kept) if kept.quarantined > 0 => {
                            eprintln!("jawd: the account went to quarantine, not stored")
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("jawd: can't store the account: {e:#}"),
                    }
                }
                if let Some(me) = me {
                    remember(&store, &live, me);
                    if let Some(engine) = &engine
                        && refreshed != Some(engine.status().restarts)
                    {
                        refreshed = Some(engine.status().restarts);
                        // An account linked in this run has its app state synced in full by whatsmeow.
                        let resync = !std::mem::take(&mut linked_now);
                        tokio::spawn(refresh::run(
                            store.clone(),
                            engine.clone(),
                            avatars.clone(),
                            merges.clone(),
                            resync,
                        ));
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

/// Waits until more engines have stopped than the `said` sync has said, and says how many have.
/// The supervisor counts them, so none goes unseen however long sync was held up; with no engine,
/// or once its supervisor has gone, that's never.
async fn stopped(status: &mut Option<watch::Receiver<EngineStatus>>, said: u32) -> u32 {
    if let Some(status) = status
        && let Ok(stops) = status.wait_for(|status| status.restarts > said).await.map(|status| status.restarts)
    {
        return stops;
    }
    std::future::pending().await
}

/// The engine has stopped and the next isn't running: interfaces hear jawd is reconnecting, so they
/// keep what they have and sends wait, and a catch-up it cut short ends, so the chat list and
/// notifications aren't held for it. What WhatsApp said of the account (logged out, a newer version
/// wanted, a ban) stands, since starting the engine again doesn't change it; the next engine's own
/// word replaces it if it should.
fn restarting(live: &Shared, pushes: &broadcast::Sender<Push>) {
    let connection = {
        let mut live = live.lock().expect("live state");
        live.cut_short();
        use ConnectionState::{Banned, LoggedOut, UpdateNeeded};
        if matches!(live.connection.state, LoggedOut | UpdateNeeded | Banned) {
            return;
        }
        live.connection = Connection::new(ConnectionState::EngineRestarting);
        live.connection.clone()
    };
    push(pushes, "connection", connection);
}

/// How long jawd waits for the phone: `patience` online before it says the phone is offline, and
/// `give_up` from asking before it stops waiting.
#[derive(Clone, Copy)]
struct PhoneTiming {
    patience: Duration,
    give_up: Duration,
}

/// Where a question asked `at` has waited online from: the later of then and the connection's last
/// coming online.
fn waited_from(at: Instant, online_since: Option<Instant>) -> Instant {
    online_since.map_or(at, |since| since.max(at))
}

/// When what jawd says of the phone may next change, if nothing else does first: a question's
/// patience used up online, or its giving up; or now, while jawd says the phone is offline and no
/// question waits.
fn phone_next(
    asked: Option<Instant>,
    state: ConnectionState,
    online_since: Option<Instant>,
    timing: PhoneTiming,
) -> Option<Instant> {
    match (asked, state) {
        (None, ConnectionState::PhoneOffline) => Some(Instant::now()),
        (None, _) => None,
        (Some(at), ConnectionState::Online) => {
            Some((waited_from(at, online_since) + timing.patience).min(at + timing.give_up))
        }
        (Some(at), _) => Some(at + timing.give_up),
    }
}

/// Waits until what jawd says of the phone should change. It's made again after each event, so the
/// connection's changes are seen.
async fn phone_due(
    phone: &mut watch::Receiver<Option<Instant>>,
    live: &Shared,
    timing: PhoneTiming,
    online_since: Option<Instant>,
) {
    loop {
        let asked = *phone.borrow_and_update();
        let state = live.lock().expect("live state").connection.state;
        let changed = match phone_next(asked, state, online_since, timing) {
            Some(due) if due <= Instant::now() => return,
            Some(due) => tokio::select! {
                changed = phone.changed() => changed,
                () = tokio::time::sleep_until(due.into()) => return,
            },
            None => phone.changed().await,
        };
        if changed.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// Says the phone is offline while a question has waited `patience` for it with the connection
/// online, and online again once it has answered or the question is given up. Arrivals and sends go
/// on either way.
fn phone_said(live: &Shared, pushes: &broadcast::Sender<Push>, timing: PhoneTiming, online_since: Option<Instant>) {
    let connection = {
        let mut live = live.lock().expect("live state");
        if live.phone.borrow().is_some_and(|at| at.elapsed() >= timing.give_up) {
            live.forget_question();
        }
        let asked = *live.phone.borrow();
        let waiting = asked.is_some_and(|at| waited_from(at, online_since).elapsed() >= timing.patience);
        let state = match (live.connection.state, waiting) {
            (ConnectionState::Online, true) => ConnectionState::PhoneOffline,
            (ConnectionState::PhoneOffline, false) => ConnectionState::Online,
            _ => return,
        };
        live.connection = Connection::new(state);
        live.connection.clone()
    };
    push(pushes, "connection", connection);
}

/// Ends the engine in the order that leaves nothing of it after jaw.db says the stop was clean:
/// whatsmeow disconnects, the supervisor ends the engine and starts it no more, and only then is
/// "1" noted, if the disconnect went through or no engine was running and none said hello since.
pub async fn stop(store: &Store, engine: Option<&Supervisor>) {
    let disconnected = match engine {
        Some(engine) => disconnect(engine).await,
        None => Some(0),
    };
    if let Some(engine) = engine {
        engine.stop().await;
    }
    noted_clean(store, engine, disconnected).await;
}

/// Asks the engine running now to disconnect. Some(runs) when it did, or when none was running,
/// with the supervisor's runs from before asking; None when it couldn't or didn't in time.
async fn disconnect(engine: &Supervisor) -> Option<u32> {
    let runs = engine.status().runs;
    let asked = engine.call(pb::command::Kind::LinkCancel(pb::LinkCancel {}));
    match tokio::time::timeout(DISCONNECTING, asked).await {
        Ok(Ok(_) | Err(EngineError::Unavailable(_))) => Some(runs),
        Ok(Err(e)) => {
            eprintln!("jawd: the engine couldn't disconnect: {e}");
            None
        }
        Err(_) => {
            eprintln!("jawd: the engine didn't disconnect in time");
            None
        }
    }
}

/// Notes the stop was clean once the engine has been ended, unless it didn't disconnect or another
/// engine said hello after it was asked to.
async fn noted_clean(store: &Store, engine: Option<&Supervisor>, disconnected: Option<u32>) {
    let Some(runs) = disconnected else { return };
    if let Some(engine) = engine {
        let status = engine.status();
        if status.state != crate::engine::supervisor::State::Stopped || status.runs != runs {
            eprintln!("jawd: an engine ran after it was asked to disconnect, so the stop isn't noted clean");
            return;
        }
    }
    if let Err(e) = store.apply_durable(vec![Change::Remember { key: CLEAN.into(), value: "1".into() }]).await {
        eprintln!("jawd: can't note a clean stop: {e:#}");
    }
}

/// How long the engine has to disconnect at a clean stop, inside jawd's 3 s to stop; if that runs
/// out first, the marker stays "0", and the next start says the last didn't stop cleanly.
const DISCONNECTING: std::time::Duration = std::time::Duration::from_secs(2);

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
        // While a keepalive timeout has the connection say connecting, the catch-up counts on, and
        // says how far it has got once the pings get through again.
        if live.connection.state != ConnectionState::CatchingUp {
            return;
        }
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

/// The account and connection as a Connection event says they are now; false when the event says
/// nothing jawd takes.
fn apply(live: &mut Live, connection: &pb::Connection) -> bool {
    use pb::connection::State;
    // Pings get through again only after the timeout that had jawd say connecting, so a restore
    // heard while it says anything else is older than the word that changed it.
    if connection.state() == State::KeepaliveRestored && live.connection.state != ConnectionState::Connecting {
        return false;
    }
    let mut reason = (!connection.reason.is_empty()).then(|| connection.reason.clone());
    let state = match connection.state() {
        State::Unspecified | State::Disconnected | State::StreamReplaced | State::ConnectFailed => {
            ConnectionState::Offline
        }
        // Told it was linked, an interface hears an engine with no device as the account logged
        // out, which it offers to link again.
        State::Unlinked if live.account.state == AccountState::Linked => {
            reason.get_or_insert_with(|| NOT_LINKED.into());
            ConnectionState::LoggedOut
        }
        State::Unlinked => ConnectionState::Offline,
        State::Connecting | State::KeepaliveTimeout => ConnectionState::Connecting,
        State::Connected | State::KeepaliveRestored => ConnectionState::Online,
        State::LoggedOut => ConnectionState::LoggedOut,
        State::ClientOutdated => ConnectionState::UpdateNeeded,
        State::TemporaryBan => ConnectionState::Banned,
    };
    if matches!(connection.state(), State::Unlinked | State::LoggedOut) {
        live.account = Account { state: AccountState::Unlinked, me: None };
        live.forget_phone();
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
    // A catch-up lasts as long as its connection: one cut short ends, so a connection after it (the
    // account linked again, or the same one back) isn't taken for part of it. A keepalive timeout,
    // the only word that leaves a catch-up connecting, doesn't end one, since whatsmeow keeps the
    // connection until its pings get through again; but it can reconnect instead, with no
    // Disconnected, so Connected after a timeout is a new connection once this one has said it.
    let reconnected = connection.state() == State::Connected
        && live.said_connected
        && live.connection.state == ConnectionState::Connecting;
    if reconnected
        || !matches!(connection.state(), State::Connected | State::KeepaliveTimeout | State::KeepaliveRestored)
    {
        live.cut_short();
    }
    if connection.state() == State::Connected {
        live.said_connected = true;
    }
    // Connected can come after WhatsApp said what's waiting, and the catch-up goes on; pings that get
    // through again, or the connection's own Connected after its timeout, put back the catch-up the
    // timeout hid.
    if live.catching_up() && state == ConnectionState::Online {
        if live.connection.state == ConnectionState::Connecting {
            let catch_up = live.catch_up.as_ref().map(|c| (c.messages, c.stored));
            live.connection = Connection::new(ConnectionState::CatchingUp);
            live.connection.total = catch_up.map(|(messages, _)| messages);
            live.connection.done = catch_up.map(|(_, stored)| stored);
        }
        return true;
    }
    live.connection = Connection::new(state);
    live.connection.reason = reason;
    if state == ConnectionState::Banned && connection.until_ms > 0 {
        live.connection.until = Some(connection.until_ms);
    }
    true
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

/// Asks the phone for the MORE messages before the oldest in `chat` that WhatsApp has: not one of
/// yours still to go, or failed, which the phone hasn't got. jawd waits for the answer only when
/// one is known to come, for a chat the phone has said it keeps more of, since whether it answers
/// at all once nothing older is left isn't known, and its silence then would look like it's
/// offline. The question is noted before it goes, so its answer can't come first, and dropped if
/// it couldn't go.
pub async fn history_more(store: &Store, engine: &Supervisor, live: &Shared, chat: String) -> Result<(), Refused> {
    let asked = chat.clone();
    let oldest = store
        .reads()
        .query(Priority::Interactive, move |db| {
            Ok(db
                .query_row(
                    "SELECT m.id, m.mine, m.at FROM chats c JOIN messages m ON m.chat = c.n WHERE c.id = ?1
                    AND NOT (m.mine AND coalesce(m.status, '') IN ('pending', 'failed')) ORDER BY m.at, m.n LIMIT 1",
                    [&asked],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, i64>(2)?)),
                )
                .ok())
        })
        .await
        .map_err(|e| Refused::NotFound(format!("can't read the chat: {e:#}")))?;
    let Some((id, mine, at)) = oldest else {
        return Err(Refused::NotFound(format!(
            "jaw has no message WhatsApp has in {chat} to ask for older ones before"
        )));
    };
    let known = live.lock().expect("live state").more_on_phone.contains(&chat);
    let noted = if known { asked_phone(live) } else { None };
    let request = pb::HistoryRequest { chat, oldest_id: id, oldest_from_me: mine, oldest_ms: at, count: MORE };
    if let Err(e) = engine.call(pb::command::Kind::HistoryRequest(request)).await {
        if let Some(noted) = noted {
            live.lock().expect("live state").phone.send_if_modified(|asked| asked.take_if(|at| *at == noted).is_some());
        }
        return Err(Refused::Engine(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
