//! `Failing`: the demo engine failing as a real one can, so what jawd and its interfaces do when
//! something fails can be tried, and tested, through the real socket: `jawd --engine demo --fail
//! crash`, say. It sits between the demo engine and jawd, passing the engine's events and acks to
//! jawd and jawd's commands to the engine, and starts well; each SIGUSR1 jawd gets turns its
//! failure on, and the next turns it off again:
//!
//! - `crash`: the engine stops as a jaw-engine that crashed does, and the next one jawd starts
//!   says hello only once the failure is off.
//! - `offline`: WhatsApp can't be reached: the connection drops, nothing arrives, and what needs
//!   WhatsApp fails as it does with no connection, until it comes back.
//! - `phone-offline`: the phone answers nothing jawd asks it. Once it's back it answers what
//!   waited: asked for older history, it sends a few older messages in that chat, as it does when
//!   the failure is off.
//! - `logout`: the phone logs jaw out. Linking again shows a made-up QR code, and once the failure
//!   is off the phone links the same account again.
//! - `outdated`: WhatsApp wants a newer version, and refuses this one until the failure is off.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use prost::Message as _;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, watch};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_history_sync::conversation::EndOfHistoryTransferType;
use wa_proto::wa_web_protobufs_history_sync::history_sync::HistorySyncType;
use wa_proto::wa_web_protobufs_history_sync::{Conversation, HistorySync, HistorySyncMsg};
use wa_proto::wa_web_protobufs_web::WebMessageInfo;

use super::demo::{self, DemoEngine, ME, World};
use super::{BoxFuture, Commands, Engine, EngineError, Incoming, Instance, Outgoing, pb};

/// How the demo engine fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Failure {
    /// The engine crashes, and starts again once the failure is off
    Crash,
    /// WhatsApp can't be reached
    Offline,
    /// The phone is offline, so what jawd asks it waits
    PhoneOffline,
    /// The phone logs jaw out, and links it again once the failure is off
    Logout,
    /// WhatsApp wants a newer version of jaw
    Outdated,
}

impl Failure {
    pub fn name(self) -> &'static str {
        match self {
            Failure::Crash => "crash",
            Failure::Offline => "offline",
            Failure::PhoneOffline => "phone-offline",
            Failure::Logout => "logout",
            Failure::Outdated => "outdated",
        }
    }

    /// Whether the failure leaves the engine without WhatsApp, so nothing arrives meanwhile.
    fn cuts_whatsapp(self) -> bool {
        matches!(self, Failure::Offline | Failure::Logout | Failure::Outdated)
    }
}

/// Why WhatsApp logged jaw out, as jaw-engine says when the phone removes the device.
pub const LOGGED_OUT: &str = "logged out from another device";
/// How many older messages the phone sends each time it's asked for older history.
const OLDER: i64 = 3;
/// A member of the demo's groups but you, whose messages the phone's older history has there.
const MEMBER: &str = "351913333333@s.whatsapp.net";
/// How long a made-up QR code lasts, as WhatsApp's first one does.
const QR_LASTS_MS: i64 = 60_000;

pub struct Failing {
    demo: DemoEngine,
    failure: Failure,
    shared: Arc<Shared>,
}

struct Shared {
    /// Whether the failure is on.
    on: watch::Sender<bool>,
    /// jawd's end of the link, which every engine started sends to.
    jawd: Mutex<Option<mpsc::Sender<Incoming>>>,
    /// Cuts the running engine off, as a crash does.
    crash: Mutex<Option<Arc<watch::Sender<bool>>>>,
    /// The requests for older history the phone hasn't answered, and the answers it has sent, by
    /// the session they name, until jawd fetches them.
    unanswered: Mutex<Vec<pb::HistoryRequest>>,
    answered: Mutex<HashMap<String, (u64, pb::HistoryRequest)>>,
    /// Numbers the phone's answers and the made-up QR codes.
    made: AtomicU64,
    listening: Once,
}

impl Failing {
    pub fn new(demo: DemoEngine, failure: Failure) -> Failing {
        let shared = Shared {
            on: watch::channel(false).0,
            jawd: Mutex::default(),
            crash: Mutex::default(),
            unanswered: Mutex::default(),
            answered: Mutex::default(),
            made: AtomicU64::new(0),
            listening: Once::new(),
        };
        Failing { demo, failure, shared: Arc::new(shared) }
    }

    /// Turns the failure on or off, as SIGUSR1 does.
    #[cfg(test)]
    pub async fn turn(&self, on: bool) {
        turn(&self.shared, &self.demo.world(), self.failure, on).await;
    }

    /// Has each SIGUSR1 jawd gets turn the failure on or off, from the first start on.
    fn listen(&self) {
        let (shared, world, failure) = (self.shared.clone(), self.demo.world(), self.failure);
        self.shared.listening.call_once(move || {
            let mut usr1 = match signal(SignalKind::user_defined1()) {
                Ok(usr1) => usr1,
                Err(e) => {
                    eprintln!("jawd: can't listen for SIGUSR1, so the demo engine can't be made to fail: {e}");
                    return;
                }
            };
            eprintln!(
                "jawd: the demo engine fails ({}) at each SIGUSR1, and stops failing at the next",
                failure.name()
            );
            tokio::spawn(async move {
                while usr1.recv().await.is_some() {
                    let on = !*shared.on.borrow();
                    eprintln!(
                        "jawd: the demo engine's failure ({}) is {}",
                        failure.name(),
                        if on { "on" } else { "off" }
                    );
                    turn(&shared, &world, failure, on).await;
                }
            });
        });
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// The demo account's connection in `state`, with WhatsApp's `reason`.
fn connection(state: pb::connection::State, reason: &str) -> pb::Event {
    let mut event = demo::connected();
    if let Some(pb::event::Kind::Connection(connection)) = &mut event.kind {
        connection.state = state as i32;
        connection.reason = reason.into();
    }
    event
}

impl Shared {
    /// Tells jawd something as the engine running now would, needing no ack.
    async fn tell(&self, event: pb::Event) {
        let jawd = self.jawd.lock().expect("the failing engine").clone();
        if let Some(jawd) = jawd {
            let _ = jawd.send(Incoming { event, ack: None }).await;
        }
    }
}

/// Turns the failure on or off. A connection that comes back is said before what waited for it
/// is let go, as WhatsApp sends nothing before the connection is up.
async fn turn(shared: &Arc<Shared>, world: &Arc<World>, failure: Failure, on: bool) {
    if !on && failure.cuts_whatsapp() {
        react(shared, world, failure, on).await;
        shared.on.send_replace(on);
    } else {
        shared.on.send_replace(on);
        react(shared, world, failure, on).await;
    }
}

/// What the failure does as it's turned on or off.
async fn react(shared: &Arc<Shared>, world: &Arc<World>, failure: Failure, on: bool) {
    use pb::connection::State;
    match (failure, on) {
        (Failure::Crash, true) => {
            if let Some(cut) = shared.crash.lock().expect("the failing engine").as_ref() {
                cut.send_replace(true);
            }
        }
        // The engine waiting to start says hello once the failure is off.
        (Failure::Crash, false) => {}
        (Failure::Offline, true) => shared.tell(connection(State::Disconnected, "")).await,
        (Failure::Outdated, true) => shared.tell(connection(State::ClientOutdated, "")).await,
        (Failure::Logout, true) => shared.tell(connection(State::LoggedOut, LOGGED_OUT)).await,
        (Failure::Offline | Failure::Outdated, false) => {
            shared.tell(connection(State::Connecting, "")).await;
            shared.tell(demo::connected()).await;
        }
        // The phone scans the QR code and links the same account again.
        (Failure::Logout, false) => {
            let linked = pb::Linked { jid: ME.into(), push_name: "Pedro".into(), ..Default::default() };
            let link = pb::Link { step: Some(pb::link::Step::Done(linked)) };
            shared.tell(pb::Event { seq: 0, kind: Some(pb::event::Kind::Link(link)) }).await;
            shared.tell(demo::connected()).await;
        }
        (Failure::PhoneOffline, true) => {}
        (Failure::PhoneOffline, false) => {
            tokio::spawn(answer_phone(shared.clone(), world.clone()));
        }
    }
}

impl Engine for Failing {
    fn kind(&self) -> &'static str {
        self.demo.kind()
    }

    fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, Result<Instance>> {
        Box::pin(async move {
            self.listen();
            *self.shared.jawd.lock().expect("the failing engine") = Some(events.clone());
            if self.failure == Failure::Crash {
                let _ = self.shared.on.subscribe().wait_for(|on| !*on).await;
            }
            let (proxy, from_demo) = mpsc::channel(1);
            let running = self.demo.start(proxy).await?;
            let cut = Arc::new(watch::channel(false).0);
            *self.shared.crash.lock().expect("the failing engine") = Some(cut.clone());
            let forwarding =
                tokio::spawn(forward(from_demo, events, cut.subscribe(), self.shared.on.subscribe(), self.failure));
            let (tx, asked) = mpsc::unbounded_channel();
            tokio::spawn(serve(asked, running.commands, self.shared.clone(), self.demo.world(), self.failure));
            let mut crashing = cut.subscribe();
            let demo_exited = running.exited;
            let exited: BoxFuture<'static, String> = Box::pin(async move {
                let why = tokio::select! {
                    why = demo_exited => why,
                    _ = crashing.wait_for(|cut| *cut) => "the demo engine crashed, as SIGUSR1 asked".to_string(),
                };
                // Whatever it handed over reaches jawd before jawd hears it stopped.
                cut.send_replace(true);
                let _ = forwarding.await;
                why
            });
            Ok(Instance { hello: running.hello, commands: Commands { tx }, exited })
        })
    }
}

/// Hands jawd what the demo engine sends, acks and all, until the engine is cut off; while the
/// failure leaves it without WhatsApp, nothing arrives until it's back.
async fn forward(
    mut from_demo: mpsc::Receiver<Incoming>,
    to_jawd: mpsc::Sender<Incoming>,
    mut cut: watch::Receiver<bool>,
    mut on: watch::Receiver<bool>,
    failure: Failure,
) {
    loop {
        let incoming = tokio::select! {
            biased;
            _ = cut.wait_for(|cut| *cut) => return,
            incoming = from_demo.recv() => match incoming {
                Some(incoming) => incoming,
                None => return,
            },
        };
        if failure.cuts_whatsapp() {
            tokio::select! {
                biased;
                _ = cut.wait_for(|cut| *cut) => return,
                _ = on.wait_for(|on| !*on) => {}
            }
        }
        if to_jawd.send(incoming).await.is_err() {
            return;
        }
    }
}

/// Answers jawd's commands, each on a task of its own as the demo engine does.
async fn serve(
    mut asked: mpsc::UnboundedReceiver<Outgoing>,
    demo: Commands,
    shared: Arc<Shared>,
    world: Arc<World>,
    failure: Failure,
) {
    while let Some(out) = asked.recv().await {
        // jawd acks an event through the ack that came with it, never through the commands.
        let Outgoing::Command { kind, reply } = out else { continue };
        let (demo, shared, world) = (demo.clone(), shared.clone(), world.clone());
        tokio::spawn(async move {
            let _ = reply.send(answer(kind, &demo, &shared, &world, failure).await);
        });
    }
}

/// Whether a command needs WhatsApp's servers, as all do but those the engine answers itself.
fn needs_whatsapp(kind: &pb::command::Kind) -> bool {
    use pb::command::Kind;
    !matches!(kind, Kind::Ping(_) | Kind::LinkCancel(_) | Kind::BenchRun(_) | Kind::Contacts(_) | Kind::LidLookup(_))
}

async fn answer(
    kind: pb::command::Kind,
    demo: &Commands,
    shared: &Arc<Shared>,
    world: &Arc<World>,
    failure: Failure,
) -> Result<pb::reply::Kind, EngineError> {
    use pb::command::Kind;
    let on = *shared.on.borrow();
    let done = Ok(pb::reply::Kind::Done(pb::Done {}));
    match kind {
        Kind::LinkStart(_) if failure == Failure::Logout && on => {
            shared.tell(qr(shared)).await;
            done
        }
        kind if on && failure.cuts_whatsapp() && needs_whatsapp(&kind) => match failure {
            Failure::Logout => Err(EngineError::Failed { message: "not logged in".into(), retry: false }),
            _ => Err(EngineError::Failed { message: "websocket not connected".into(), retry: true }),
        },
        // The request goes to the phone; the phone answers it at once if it's online.
        Kind::HistoryRequest(request) if failure == Failure::PhoneOffline => {
            shared.unanswered.lock().expect("the failing engine").push(request);
            if !on {
                tokio::spawn(answer_phone(shared.clone(), world.clone()));
            }
            done
        }
        Kind::HistoryDownload(download) => {
            let asked = e2e::HistorySyncNotification::decode(download.notification.as_slice())
                .ok()
                .and_then(|notification| notification.peer_data_request_session_id)
                .and_then(|session| shared.answered.lock().expect("the failing engine").remove(&session));
            match asked {
                Some((n, request)) => written(download, n, request).await,
                None => demo.call(Kind::HistoryDownload(download)).await,
            }
        }
        kind => demo.call(kind).await,
    }
}

/// A made-up QR code to link with, which no phone can link.
fn qr(shared: &Shared) -> pb::Event {
    let n = shared.made.fetch_add(1, Ordering::Relaxed);
    let qr = pb::QrCode { code: format!("jaw-demo-{n}: a made-up QR code"), expires_ms: now_ms() + QR_LASTS_MS };
    pb::Event { seq: 0, kind: Some(pb::event::Kind::Link(pb::Link { step: Some(pb::link::Step::Qr(qr)) })) }
}

/// The phone, online, answers each request for older history that waited for it as a phone does:
/// with a history notification, whose blob jawd then fetches.
async fn answer_phone(shared: Arc<Shared>, world: Arc<World>) {
    let waiting = std::mem::take(&mut *shared.unanswered.lock().expect("the failing engine"));
    for request in waiting {
        let n = shared.made.fetch_add(1, Ordering::Relaxed);
        let session = format!("jaw-demo-{n}");
        shared.answered.lock().expect("the failing engine").insert(session.clone(), (n, request));
        world.deliver(notification(n, &session)).await;
    }
}

/// The phone's message saying older history is ready, as it sends one to its own account.
fn notification(n: u64, session: &str) -> pb::Event {
    let notification = e2e::HistorySyncNotification {
        sync_type: Some(e2e::HistorySyncType::OnDemand as i32),
        chunk_order: Some(1),
        progress: Some(100),
        peer_data_request_session_id: Some(session.into()),
        ..Default::default()
    };
    let protocol = e2e::ProtocolMessage {
        r#type: Some(e2e::protocol_message::Type::HistorySyncNotification as i32),
        history_sync_notification: Some(notification),
        ..Default::default()
    };
    let raw = e2e::Message { protocol_message: Some(Box::new(protocol)), ..Default::default() };
    let info = pb::MessageInfo {
        chat: ME.into(),
        sender: ME.into(),
        from_me: true,
        id: format!("3EB0DEMOPHONE{n:06}"),
        r#type: "protocol".into(),
        timestamp_ms: now_ms(),
        ..Default::default()
    };
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Message(pb::Message {
            info: Some(info),
            raw: raw.encode_to_vec(),
            ..Default::default()
        })),
    }
}

/// A few messages from before the oldest jaw has in the chat, an hour apart, as the phone sends
/// older history: from the other person in a one-to-one chat, from a member in a group; and that
/// it keeps more of the chat than it sent.
fn older(n: u64, request: &pb::HistoryRequest) -> HistorySync {
    let group = request.chat.ends_with("@g.us");
    let participant = group.then(|| MEMBER.to_string());
    let messages = (1..=OLDER)
        .map(|k| HistorySyncMsg {
            message: Some(WebMessageInfo {
                key: MessageKey {
                    remote_jid: Some(request.chat.clone()),
                    from_me: Some(false),
                    id: Some(format!("3EB0DEMOOLDER{n:04}{k}")),
                    participant: participant.clone(),
                },
                message: Some(e2e::Message {
                    conversation: Some(format!("an older message from the phone, {k} of {OLDER}")),
                    ..Default::default()
                }),
                message_timestamp: Some((request.oldest_ms / 1000 - (OLDER + 1 - k) * 3600).max(0) as u64),
                participant: participant.clone(),
                ..Default::default()
            }),
            msg_order_id: Some(k as u64),
        })
        .collect();
    let conversation = Conversation {
        id: request.chat.clone(),
        messages,
        end_of_history_transfer: Some(true),
        end_of_history_transfer_type: Some(
            EndOfHistoryTransferType::CompleteOnDemandSyncButMoreMsgRemainOnPrimary as i32,
        ),
        ..Default::default()
    };
    HistorySync {
        sync_type: HistorySyncType::OnDemand as i32,
        conversations: vec![conversation],
        chunk_order: Some(1),
        progress: Some(100),
        ..Default::default()
    }
}

/// Writes the older history the phone made for `request` where jawd asks, whole or not at all.
async fn written(
    download: pb::HistoryDownload,
    n: u64,
    request: pb::HistoryRequest,
) -> Result<pb::reply::Kind, EngineError> {
    let history = older(n, &request);
    let data = history.encode_to_vec();
    let path = PathBuf::from(&download.path);
    let bytes = data.len() as u64;
    tokio::task::spawn_blocking(move || demo::write_whole(&path, &data))
        .await
        .map_err(|e| EngineError::Failed { message: format!("writing the history stopped: {e}"), retry: true })?
        .map_err(|e| EngineError::Failed { message: format!("can't write the history: {e}"), retry: true })?;
    Ok(pb::reply::Kind::HistoryFile(pb::HistoryFile {
        path: download.path,
        r#type: history.sync_type,
        chunk_order: 1,
        progress: 100,
        bytes,
        conversations: 1,
    }))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wa_proto::wa_web_protobufs_history_sync::HistorySync;

    use super::*;
    use crate::engine::supervisor::{Backoff, State, Supervisor};

    const ANA: &str = "351912345678@s.whatsapp.net";

    /// The demo engine with nothing to play but the connection, failing as `failure` says.
    fn failing(failure: Failure) -> Arc<Failing> {
        Arc::new(Failing::new(DemoEngine::with(vec![demo::connected()]), failure))
    }

    /// The next event jawd is handed, acked if it needs one.
    async fn next(rx: &mut mpsc::Receiver<Incoming>) -> pb::event::Kind {
        let incoming = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("an event").unwrap();
        if let Some(ack) = incoming.ack {
            ack.send();
        }
        incoming.event.kind.expect("an event of a kind")
    }

    /// Whether nothing comes for a moment.
    async fn quiet(rx: &mut mpsc::Receiver<Incoming>) -> bool {
        tokio::time::timeout(Duration::from_millis(200), rx.recv()).await.is_err()
    }

    fn state(kind: &pb::event::Kind) -> pb::connection::State {
        match kind {
            pb::event::Kind::Connection(connection) => connection.state(),
            other => panic!("not a connection: {other:?}"),
        }
    }

    fn send(id: &str) -> pb::command::Kind {
        pb::command::Kind::Send(pb::Send { chat: ANA.into(), id: id.into(), raw: Vec::new() })
    }

    #[tokio::test]
    async fn a_crash_ends_the_engine_and_the_next_says_hello_only_once_the_failure_is_off() {
        let engine = failing(Failure::Crash);
        let (tx, mut rx) = mpsc::channel(8);
        let quick = Backoff { first: Duration::from_millis(5), most: Duration::from_millis(5), ..Backoff::default() };
        let supervisor = Supervisor::spawn(engine.clone(), tx, quick);
        assert_eq!(state(&next(&mut rx).await), pb::connection::State::Connected);
        engine.turn(true).await;
        let mut status = supervisor.watch();
        let restarting = status.wait_for(|s| s.restarts == 1 && s.state == State::Starting);
        let restarted = tokio::time::timeout(Duration::from_secs(10), restarting).await.unwrap().unwrap().clone();
        assert_eq!(restarted.last_exit.as_deref(), Some("the demo engine crashed, as SIGUSR1 asked"));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(supervisor.status().state, State::Starting, "the next waits to say hello");
        assert!(quiet(&mut rx).await, "and says nothing");
        engine.turn(false).await;
        assert_eq!(state(&next(&mut rx).await), pb::connection::State::Connected, "until the failure is off");
        assert_eq!((supervisor.status().state, supervisor.status().runs), (State::Running, 2));
    }

    #[tokio::test]
    async fn offline_and_outdated_hold_what_arrives_and_fail_what_needs_whatsapp_until_they_are_off() {
        use pb::connection::State as Said;
        for (failure, said) in [(Failure::Offline, Said::Disconnected), (Failure::Outdated, Said::ClientOutdated)] {
            let engine = failing(failure);
            let (tx, mut rx) = mpsc::channel(8);
            let instance = engine.start(tx).await.unwrap();
            assert_eq!(state(&next(&mut rx).await), Said::Connected);
            engine.turn(true).await;
            assert_eq!(state(&next(&mut rx).await), said, "{failure:?}");
            let refused = instance.commands.call(send("3EB0FAILING0001")).await;
            let expected = EngineError::Failed { message: "websocket not connected".into(), retry: true };
            assert_eq!(refused, Err(expected), "{failure:?}: a send fails as with no connection, to be tried again");
            let ping = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await;
            assert!(matches!(ping, Ok(pb::reply::Kind::Pong(_))), "{failure:?}: the engine itself still answers");
            let world = engine.demo.world();
            let arriving = tokio::spawn(async move { world.deliver(demo::text(ANA, ANA, "3EB0HELD", 0, "hi")).await });
            assert!(quiet(&mut rx).await, "{failure:?}: nothing arrives meanwhile");
            engine.turn(false).await;
            assert_eq!(state(&next(&mut rx).await), Said::Connecting, "{failure:?}");
            assert_eq!(state(&next(&mut rx).await), Said::Connected, "{failure:?}");
            assert!(matches!(next(&mut rx).await, pb::event::Kind::Message(_)), "{failure:?}: what waited comes");
            assert!(arriving.await.unwrap().is_some(), "{failure:?}: and was acked");
            let sent = instance.commands.call(send("3EB0FAILING0002")).await;
            assert!(matches!(sent, Ok(pb::reply::Kind::Sent(_))), "{failure:?}: sends go again: {sent:?}");
        }
    }

    #[tokio::test]
    async fn a_logout_from_the_phone_offers_a_made_up_qr_code_and_links_the_account_again_once_it_is_off() {
        let engine = failing(Failure::Logout);
        let (tx, mut rx) = mpsc::channel(8);
        let instance = engine.start(tx).await.unwrap();
        next(&mut rx).await;
        engine.turn(true).await;
        let pb::event::Kind::Connection(out) = next(&mut rx).await else { panic!("no logout") };
        assert_eq!((out.state(), out.reason.as_str()), (pb::connection::State::LoggedOut, LOGGED_OUT));
        let refused = instance.commands.call(send("3EB0FAILING0003")).await;
        assert_eq!(refused, Err(EngineError::Failed { message: "not logged in".into(), retry: false }));
        let start = pb::command::Kind::LinkStart(pb::LinkStart { phone: String::new() });
        assert!(matches!(instance.commands.call(start).await, Ok(pb::reply::Kind::Done(_))));
        let pb::event::Kind::Link(pb::Link { step: Some(pb::link::Step::Qr(qr)) }) = next(&mut rx).await else {
            panic!("no QR code")
        };
        assert!(qr.code.contains("made-up") && qr.expires_ms > now_ms(), "{qr:?}");
        engine.turn(false).await;
        let pb::event::Kind::Link(pb::Link { step: Some(pb::link::Step::Done(linked)) }) = next(&mut rx).await else {
            panic!("not linked")
        };
        assert_eq!(linked.jid, ME, "the same account");
        assert_eq!(state(&next(&mut rx).await), pb::connection::State::Connected);
    }

    fn history_request(oldest_ms: i64) -> pb::command::Kind {
        pb::command::Kind::HistoryRequest(pb::HistoryRequest {
            chat: ANA.into(),
            oldest_id: "3EB0DEMO00000001".into(),
            oldest_from_me: false,
            oldest_ms,
            count: 50,
        })
    }

    /// The history notification in the phone's answer.
    fn offered(kind: pb::event::Kind) -> e2e::HistorySyncNotification {
        let pb::event::Kind::Message(message) = kind else { panic!("not a message: {kind:?}") };
        let raw = e2e::Message::decode(message.raw.as_slice()).unwrap();
        raw.protocol_message.and_then(|p| p.history_sync_notification).expect("a history notification")
    }

    #[tokio::test]
    async fn the_phone_offline_answers_a_request_for_older_history_only_once_it_is_back() {
        let engine = failing(Failure::PhoneOffline);
        let (tx, mut rx) = mpsc::channel(8);
        let instance = engine.start(tx).await.unwrap();
        next(&mut rx).await;
        let oldest_ms = 1_792_000_000_000;
        assert!(matches!(instance.commands.call(history_request(oldest_ms)).await, Ok(pb::reply::Kind::Done(_))));
        let online = offered(next(&mut rx).await);
        assert_eq!(online.sync_type, Some(e2e::HistorySyncType::OnDemand as i32), "online, it answers at once");

        engine.turn(true).await;
        assert!(matches!(instance.commands.call(history_request(oldest_ms)).await, Ok(pb::reply::Kind::Done(_))));
        assert!(quiet(&mut rx).await, "offline, it doesn't answer");
        engine.turn(false).await;
        let notification = offered(next(&mut rx).await);
        let spool = tempfile::tempdir().unwrap();
        let path = spool.path().join("older.pb");
        let download = pb::command::Kind::HistoryDownload(pb::HistoryDownload {
            notification: notification.encode_to_vec(),
            path: path.display().to_string(),
        });
        let Ok(pb::reply::Kind::HistoryFile(file)) = instance.commands.call(download).await else { panic!("no blob") };
        let history = HistorySync::decode(std::fs::read(&path).unwrap().as_slice()).unwrap();
        assert_eq!((file.bytes, file.conversations), (std::fs::metadata(&path).unwrap().len(), 1));
        assert_eq!(history.sync_type, HistorySyncType::OnDemand as i32);
        let messages = &history.conversations[0].messages;
        assert_eq!((history.conversations[0].id.as_str(), messages.len()), (ANA, OLDER as usize));
        let at = |m: &HistorySyncMsg| m.message.as_ref().unwrap().message_timestamp.unwrap() as i64 * 1000;
        assert!(messages.iter().all(|m| at(m) < oldest_ms), "all from before the oldest jaw has");
        let again = pb::command::Kind::HistoryDownload(pb::HistoryDownload {
            notification: notification.encode_to_vec(),
            path: spool.path().join("again.pb").display().to_string(),
        });
        assert!(instance.commands.call(again).await.is_err(), "an answer is fetched once, as the demo has it");
    }
}
