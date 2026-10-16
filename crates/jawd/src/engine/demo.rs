//! `DemoEngine`: a made-up account inside jawd, for `jawd --engine demo`, tests and the load
//! harness. It sends the same events a real engine does, waits for each message's ack as whatsmeow
//! would, and answers ping and bench. What it's sent goes to a made-up WhatsApp that outlives any
//! one engine, as the real one does: it answers with the server's time, the recipients' phones
//! send delivered and then read, and a message sent again with the same ID is the same message.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use prost::Message as _;
use tokio::sync::{Notify, mpsc, oneshot};
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::{Ack, BoxFuture, Commands, Engine, EngineError, Incoming, Instance, Outgoing, PROTOCOL, pb};

const ME: &str = "351910000001@s.whatsapp.net";

/// Two made-up members of every group the demo sends to, besides you.
const MEMBERS: [&str; 2] = ["351913333333@s.whatsapp.net", "351914444444@s.whatsapp.net"];

pub struct DemoEngine {
    script: Vec<pb::Event>,
    world: Arc<World>,
}

/// How long the made-up recipients take: their phone has a message `delivered` after it's sent,
/// and they've read it `read` after that.
#[derive(Clone, Copy, Debug)]
pub struct Replies {
    pub delivered: Duration,
    pub read: Duration,
}

impl Default for Replies {
    fn default() -> Self {
        Replies { delivered: Duration::from_millis(400), read: Duration::from_millis(1_500) }
    }
}

/// The made-up WhatsApp the demo engine talks to: what it was asked, which messages it has, and how
/// it behaves, kept across the engines that share it.
#[derive(Default)]
pub struct World {
    replies: Mutex<Replies>,
    asked: Mutex<Vec<pb::command::Kind>>,
    /// Each message the server has, by ID, with its time.
    has: Mutex<HashMap<String, i64>>,
    /// Messages a recipient has been given, by ID; a message sent twice is given once.
    given: Mutex<HashSet<String>>,
    /// How many sends to fail next, as a connection that's down does.
    failing: Mutex<u32>,
    /// Whether the server takes sends without answering, as a connection that drops does.
    holding: AtomicBool,
    /// How many sends the server has had at once, at most.
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    /// Whether sends wait to be answered until it opens.
    gated: AtomicBool,
    opened: Notify,
    asked_now: Notify,
}

impl World {
    pub fn replying(&self, replies: Replies) {
        *self.replies.lock().expect("the demo world") = replies;
    }

    /// Fails the next `n` sends with a connection that's down.
    #[cfg(test)]
    pub fn fail_next(&self, n: u32) {
        *self.failing.lock().expect("the demo world") = n;
    }

    /// Takes sends without answering them, until it's told otherwise.
    #[cfg(test)]
    pub fn hold(&self, holding: bool) {
        self.holding.store(holding, Ordering::SeqCst);
    }

    /// Holds sends unanswered while `gated`, answering them all once it opens.
    #[cfg(test)]
    pub fn gate(&self, gated: bool) {
        self.gated.store(gated, Ordering::SeqCst);
        if !gated {
            self.opened.notify_waiters();
        }
    }

    /// The most sends it has had at once.
    #[cfg(test)]
    pub fn most_at_once(&self) -> usize {
        self.most_in_flight.load(Ordering::SeqCst)
    }

    /// Everything the engine was asked, pings and benches apart, in order.
    #[cfg(test)]
    pub fn asked(&self) -> Vec<pb::command::Kind> {
        self.asked.lock().expect("the demo world").clone()
    }

    /// The messages it was asked to send, in order, each time it was asked.
    #[cfg(test)]
    pub fn sends(&self) -> Vec<pb::Send> {
        self.asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::Send(send) => Some(send),
                _ => None,
            })
            .collect()
    }

    /// How many messages the recipients have been given.
    #[cfg(test)]
    pub fn given(&self) -> usize {
        self.given.lock().expect("the demo world").len()
    }

    /// Waits until `enough` holds of what the engine was asked.
    #[cfg(test)]
    pub async fn until(&self, enough: impl Fn(&[pb::command::Kind]) -> bool) {
        loop {
            let notified = self.asked_now.notified();
            if enough(&self.asked.lock().expect("the demo world")) {
                return;
            }
            notified.await;
        }
    }

    fn ask(&self, kind: pb::command::Kind) {
        self.asked.lock().expect("the demo world").push(kind);
        self.asked_now.notify_waiters();
    }
}

impl DemoEngine {
    /// The made-up account: connected, then a few messages arriving.
    pub fn new() -> DemoEngine {
        let now = now_ms();
        let ana = "351912345678@s.whatsapp.net";
        let climbing = "120363012345678901@g.us";
        DemoEngine::with(vec![
            connected(),
            text(ana, ana, "3EB0DEMO00000001", now - 120_000, "did you get the photos from Sunday?"),
            text(climbing, "351913333333@s.whatsapp.net", "3EB0DEMO00000002", now - 60_000, "Saturday at 9?"),
            text(ana, ana, "3EB0DEMO00000003", now - 5_000, "the one at the lake"),
        ])
    }

    pub fn with(script: Vec<pb::Event>) -> DemoEngine {
        DemoEngine { script, world: Arc::default() }
    }

    /// An engine on `world`, as an engine started again talks to the same WhatsApp.
    #[cfg(test)]
    pub fn sharing(script: Vec<pb::Event>, world: Arc<World>) -> DemoEngine {
        DemoEngine { script, world }
    }

    pub fn world(&self) -> Arc<World> {
        self.world.clone()
    }
}

/// The account connected, with its push name, as the demo's script starts.
pub fn connected() -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Connection(pb::Connection {
            state: pb::connection::State::Connected as i32,
            jid: ME.into(),
            push_name: "Pedro".into(),
            ..Default::default()
        })),
    }
}

impl Default for DemoEngine {
    fn default() -> Self {
        DemoEngine::new()
    }
}

/// A text message from `sender` in `chat`.
pub fn text(chat: &str, sender: &str, id: &str, at_ms: i64, body: &str) -> pb::Event {
    let raw = e2e::Message { conversation: Some(body.into()), ..Default::default() };
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Message(pb::Message {
            info: Some(pb::MessageInfo {
                chat: chat.into(),
                sender: sender.into(),
                group: chat.ends_with("@g.us"),
                id: id.into(),
                r#type: "text".into(),
                timestamp_ms: at_ms,
                ..Default::default()
            }),
            raw: raw.encode_to_vec(),
            ..Default::default()
        })),
    }
}

/// `recipient`'s phone saying it has, or has read, a message sent to `send.chat`.
fn receipt(send: &pb::Send, recipient: &str, kind: &str) -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Receipt(pb::Receipt {
            chat: send.chat.clone(),
            sender: recipient.into(),
            group: send.chat.ends_with("@g.us"),
            ids: vec![send.id.clone()],
            timestamp_ms: now_ms(),
            r#type: kind.into(),
            ..Default::default()
        })),
    }
}

/// Every number is on the made-up WhatsApp but those starting 999, each known by a LID as well.
fn on_whatsapp(asked: &pb::OnWhatsApp) -> pb::OnWhatsAppList {
    let users = asked
        .phones
        .iter()
        .map(|phone| {
            let is_in = !phone.starts_with("999");
            pb::OnWhatsAppUser {
                query: phone.clone(),
                is_in,
                jid: if is_in { format!("1000{phone}@lid") } else { String::new() },
                phone: if is_in { format!("{phone}@s.whatsapp.net") } else { String::new() },
                verified_name: String::new(),
            }
        })
        .collect();
    pb::OnWhatsAppList { users }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

impl Engine for DemoEngine {
    fn kind(&self) -> &'static str {
        "demo"
    }

    fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, Result<Instance>> {
        let script = self.script.clone();
        let world = self.world.clone();
        Box::pin(async move {
            let (tx, mut outgoing) = mpsc::unbounded_channel();
            let commands = Commands { tx };
            let sender = Sender {
                events,
                weak: commands.tx.downgrade(),
                seq: Arc::new(AtomicU64::new(0)),
                waiting: Arc::default(),
                world,
            };
            let playing = sender.clone();
            tokio::spawn(async move {
                for event in script {
                    if !playing.send(event).await {
                        break;
                    }
                }
            });
            let serving = tokio::spawn(async move {
                while let Some(out) = outgoing.recv().await {
                    match out {
                        Outgoing::Ack(seq) => sender.acked(seq),
                        Outgoing::Command { kind, reply } => {
                            let sender = sender.clone();
                            tokio::spawn(async move {
                                let _ = reply.send(sender.answer(kind).await);
                            });
                        }
                    }
                }
            });
            let exited: BoxFuture<'static, String> = Box::pin(async move {
                let _ = serving.await;
                "the demo engine stopped".to_string()
            });
            let hello =
                pb::Hello { protocol: PROTOCOL, program: "jawd's demo engine".into(), whatsmeow: String::new() };
            Ok(Instance { hello, commands, exited })
        })
    }
}

/// The demo's half of the link: it numbers what needs an ack and waits for it, as jaw-engine does.
#[derive(Clone)]
struct Sender {
    events: mpsc::Sender<Incoming>,
    weak: mpsc::WeakUnboundedSender<Outgoing>,
    seq: Arc<AtomicU64>,
    waiting: Arc<Mutex<HashMap<u64, oneshot::Sender<()>>>>,
    world: Arc<World>,
}

impl Sender {
    /// Sends an event; a message, a receipt or a bench event waits for its ack. False if jawd has
    /// gone.
    async fn send(&self, mut event: pb::Event) -> bool {
        let durable = matches!(
            event.kind,
            Some(pb::event::Kind::Message(_) | pb::event::Kind::Receipt(_) | pb::event::Kind::Bench(_))
        );
        if !durable {
            return self.events.send(Incoming { event, ack: None }).await.is_ok();
        }
        let Some(tx) = self.weak.upgrade() else { return false };
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let (acked, ack_came) = oneshot::channel();
        self.waiting.lock().expect("demo acks").insert(seq, acked);
        event.seq = seq;
        if self.events.send(Incoming { event, ack: Some(Ack { seq, tx }) }).await.is_err() {
            return false;
        }
        ack_came.await.is_ok()
    }

    fn acked(&self, seq: u64) {
        if let Some(waiter) = self.waiting.lock().expect("demo acks").remove(&seq) {
            let _ = waiter.send(());
        }
    }

    async fn answer(&self, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
        let done = Ok(pb::reply::Kind::Done(pb::Done {}));
        match kind {
            pb::command::Kind::Ping(_) => Ok(pb::reply::Kind::Pong(pb::Pong { at_ms: now_ms() })),
            pb::command::Kind::BenchRun(run) => self.bench(run).await,
            // The made-up account has no connection to end.
            pb::command::Kind::LinkCancel(_) => Ok(pb::reply::Kind::Done(pb::Done {})),
            pb::command::Kind::Send(send) => {
                self.world.ask(pb::command::Kind::Send(send.clone()));
                self.sent(send).await
            }
            pb::command::Kind::OnWhatsapp(asked) => {
                self.world.ask(pb::command::Kind::OnWhatsapp(asked.clone()));
                Ok(pb::reply::Kind::OnWhatsappList(on_whatsapp(&asked)))
            }
            kind @ (pb::command::Kind::MarkRead(_)
            | pb::command::Kind::ChatPresence(_)
            | pb::command::Kind::Presence(_)) => {
                self.world.ask(kind);
                done
            }
            _ => Err(EngineError::Failed { message: "the demo engine doesn't do that".into(), retry: false }),
        }
    }

    /// The server takes a message, once whatever its ID; the recipients' phones then say they have
    /// it and, later, that they've read it.
    async fn sent(&self, send: pb::Send) -> Result<pb::reply::Kind, EngineError> {
        let world = &self.world;
        let failing = {
            let mut failing = world.failing.lock().expect("the demo world");
            let fails = *failing > 0;
            *failing = failing.saturating_sub(1);
            fails
        };
        if failing {
            return Err(EngineError::Failed { message: "websocket not connected".into(), retry: true });
        }
        let at = *world.has.lock().expect("the demo world").entry(send.id.clone()).or_insert_with(now_ms);
        if world.holding.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        let flying = world.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        world.most_in_flight.fetch_max(flying, Ordering::SeqCst);
        loop {
            let opened = world.opened.notified();
            if !world.gated.load(Ordering::SeqCst) {
                break;
            }
            opened.await;
        }
        world.in_flight.fetch_sub(1, Ordering::SeqCst);
        if world.given.lock().expect("the demo world").insert(send.id.clone()) {
            let (sender, replies) = (self.clone(), *world.replies.lock().expect("the demo world"));
            let send = send.clone();
            tokio::spawn(async move {
                let group = send.chat.ends_with("@g.us");
                let recipients: Vec<&str> = if group { MEMBERS.to_vec() } else { vec![send.chat.as_str()] };
                tokio::time::sleep(replies.delivered).await;
                for recipient in &recipients {
                    if !sender.send(receipt(&send, recipient, "")).await {
                        return;
                    }
                }
                tokio::time::sleep(replies.read).await;
                sender.send(receipt(&send, recipients[0], "read")).await;
            });
        }
        Ok(pb::reply::Kind::Sent(pb::Sent { id: send.id, at_ms: at, chat: send.chat }))
    }

    async fn bench(&self, run: pb::BenchRun) -> Result<pb::reply::Kind, EngineError> {
        let payload = vec![0u8; run.size as usize];
        let mut trips = Vec::with_capacity(run.count as usize);
        let started = Instant::now();
        for _ in 0..run.count {
            let sent = Instant::now();
            let event =
                pb::Event { seq: 0, kind: Some(pb::event::Kind::Bench(pb::Bench { payload: payload.clone() })) };
            if !self.send(event).await {
                return Err(EngineError::Gone);
            }
            trips.push(sent.elapsed().as_micros() as u64);
        }
        trips.sort_unstable();
        let at = |q: usize| trips.get(trips.len() * q / 100).or(trips.last()).copied().unwrap_or_default();
        Ok(pb::reply::Kind::BenchResult(pb::BenchResult {
            count: run.count,
            elapsed_us: started.elapsed().as_micros() as u64,
            p50_us: at(50),
            p99_us: at(99),
            max_us: trips.last().copied().unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_demo_plays_its_account_and_waits_for_each_ack() {
        let (tx, mut rx) = mpsc::channel(8);
        let instance = DemoEngine::new().start(tx).await.unwrap();
        let connected = rx.recv().await.unwrap();
        assert!(matches!(connected.event.kind, Some(pb::event::Kind::Connection(_))));
        assert!(connected.ack.is_none());
        let first = rx.recv().await.unwrap();
        let Some(pb::event::Kind::Message(message)) = &first.event.kind else { panic!("{:?}", first.event) };
        let raw = e2e::Message::decode(message.raw.as_slice()).unwrap();
        assert_eq!(raw.conversation.as_deref(), Some("did you get the photos from Sunday?"));
        assert_eq!(first.event.seq, 1);
        let ack = first.ack.expect("a message needs an ack");
        let early = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(early.is_err(), "the next message came before the first was acked");
        ack.send();
        let second = rx.recv().await.unwrap();
        assert_eq!(second.event.seq, 2);
        second.ack.expect("a message needs an ack").send();
        let third = rx.recv().await.unwrap();
        third.ack.unwrap().send();

        let pong = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap();
        assert!(matches!(pong, pb::reply::Kind::Pong(_)));
        let acking = tokio::spawn(async move {
            while let Some(incoming) = rx.recv().await {
                if let Some(ack) = incoming.ack {
                    ack.send();
                }
            }
        });
        let result =
            instance.commands.call(pb::command::Kind::BenchRun(pb::BenchRun { count: 500, size: 10 })).await.unwrap();
        let pb::reply::Kind::BenchResult(result) = result else { panic!("{result:?}") };
        assert_eq!(result.count, 500);
        let refused = instance.commands.call(pb::command::Kind::Logout(pb::Logout {})).await;
        assert!(matches!(refused, Err(EngineError::Failed { .. })));
        drop(instance.commands);
        assert_eq!(instance.exited.await, "the demo engine stopped");
        acking.abort();
    }
}
