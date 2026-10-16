//! The engine jawd reaches WhatsApp through, behind one trait, so engines can be swapped and
//! compared: `GoEngine` runs jaw-engine on whatsmeow, `DemoEngine` is a made-up account, and a
//! whatsapp-rust engine comes in M7. proto/engine/engine.proto is the link they all speak.

pub mod demo;
pub mod frame;
pub mod go;
pub mod supervisor;

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};
pub use wa_proto::jaw::engine as pb;

/// The link's version, which both sides' hello must carry.
pub const PROTOCOL: u32 = pb::Version::Current as u32;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// An event from an engine. One WhatsApp needs stored first carries an `Ack`, to send once it's
/// committed; until then the engine waits, and WhatsApp hasn't heard that it arrived.
pub struct Incoming {
    pub event: pb::Event,
    pub ack: Option<Ack>,
}

/// jawd's word that it has stored an event. It can be sent from any thread, such as the database
/// writer's, and costs nothing if the engine has gone.
pub struct Ack {
    seq: u64,
    tx: mpsc::UnboundedSender<Outgoing>,
}

impl Ack {
    pub fn send(self) {
        let _ = self.tx.send(Outgoing::Ack(self.seq));
    }
}

/// What jawd sends an engine.
pub enum Outgoing {
    Command { kind: pb::command::Kind, reply: oneshot::Sender<Result<pb::reply::Kind, EngineError>> },
    Ack(u64),
}

/// Commands to one running engine.
#[derive(Clone)]
pub struct Commands {
    tx: mpsc::UnboundedSender<Outgoing>,
}

impl Commands {
    /// Commands whose other end a test engine serves.
    #[cfg(test)]
    pub fn for_tests() -> (Commands, mpsc::UnboundedReceiver<Outgoing>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Commands { tx }, rx)
    }

    pub async fn call(&self, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
        let (reply, answer) = oneshot::channel();
        self.tx.send(Outgoing::Command { kind, reply }).map_err(|_| EngineError::Gone)?;
        answer.await.map_err(|_| EngineError::Gone)?
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    /// The engine went before it answered.
    Gone,
    /// No engine is running right now, and why.
    Unavailable(String),
    /// The engine tried and couldn't; `retry` says whether trying again later might work.
    Failed { message: String, retry: bool },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Gone => write!(f, "the engine went before it answered"),
            EngineError::Unavailable(why) => write!(f, "the engine isn't running: {why}"),
            EngineError::Failed { message, .. } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// One running engine: what it said in its hello, its commands, and a future that resolves, with
/// why, when it stops.
pub struct Instance {
    pub hello: pb::Hello,
    pub commands: Commands,
    pub exited: BoxFuture<'static, String>,
}

pub trait Engine: Send + Sync + 'static {
    /// The engine's name in `status`: go, rust or demo.
    fn kind(&self) -> &'static str;

    /// Starts an instance, which sends its events to `events` and resolves once it has said hello.
    /// `events` is bounded, so an engine waits when jawd falls behind.
    fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, anyhow::Result<Instance>>;
}

type Waiter = oneshot::Sender<Result<pb::reply::Kind, EngineError>>;

/// The replies jawd waits for, by command id, shared by an engine's writer and its reader. Once the
/// engine has gone it takes no more, so nobody waits for a reply that can't come.
#[derive(Clone)]
struct Pending {
    waiting: Arc<Mutex<Option<HashMap<u64, Waiter>>>>,
}

impl Default for Pending {
    fn default() -> Self {
        Pending { waiting: Arc::new(Mutex::new(Some(HashMap::new()))) }
    }
}

impl Pending {
    fn add(&self, id: u64, reply: Waiter) {
        match self.waiting.lock().expect("pending replies").as_mut() {
            Some(waiting) => {
                waiting.insert(id, reply);
            }
            None => {
                let _ = reply.send(Err(EngineError::Gone));
            }
        }
    }

    fn answer(&self, id: u64, reply: pb::Reply) {
        let waiter = self.waiting.lock().expect("pending replies").as_mut().and_then(|waiting| waiting.remove(&id));
        let Some(waiter) = waiter else { return };
        let outcome = match reply.kind {
            Some(pb::reply::Kind::Failure(failure)) => {
                Err(EngineError::Failed { message: failure.message, retry: failure.retry })
            }
            Some(kind) => Ok(kind),
            None => Err(EngineError::Failed { message: "the engine replied with nothing".into(), retry: false }),
        };
        let _ = waiter.send(outcome);
    }

    /// The engine has gone: everyone still waiting hears so, and so will anyone who asks.
    fn fail_all(&self) {
        let waiting = self.waiting.lock().expect("pending replies").take();
        for (_, waiter) in waiting.into_iter().flatten() {
            let _ = waiter.send(Err(EngineError::Gone));
        }
    }
}
