//! Sending: what you write, read and type, on its way to WhatsApp. `send.text` makes the message's
//! ID up front, as whatsmeow does, and writes the message (pending) and its outbox row in one
//! synced commit before it answers, so the row every window shows is the real message and nothing
//! sent is lost; each chat's sends then go one after another through the engine with that ID,
//! waiting while WhatsApp can't be reached and failing only when it refuses one (`outbox`).
//! Reading moves the chat's read position and owes WhatsApp read receipts, sent at most every so
//! often per chat (`reads`); typing is said at most every so often and stopped after a quiet spell
//! (`typing`); you're online only while an interface has focus (`presence`); and `chat.start`
//! finds or makes a chat by number or person (`start`). A reply quotes its message and mentions
//! name their people as WhatsApp writes them (`quote`); reactions, edits and deletions go as
//! messages of their own through the outbox (`actions`); `forward` sends messages again as new
//! ones of yours (`forward`); `message.info` gives each recipient's receipts, and `chat.info` a
//! chat's members (`info`). Text with a URL goes with its page's preview when the page answers in
//! time (`link`), and the next message to its chat waits for it, so none overtakes another.

mod actions;
pub mod bench;
mod forward;
mod info;
pub mod link;
mod outbox;
mod poll;
mod presence;
mod quote;
mod reads;
mod regroup;
mod start;
mod typing;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jaw_proto::send::{
    ChatInfo, ChatStart, Delete, DraftSet, Edit, Forward, MessageInfo, PollVote, React, Read, SendPoll, SendRetry,
    SendText, Typing,
};
use jaw_proto::{Error, Push, code};
use rusqlite::Connection;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Mutex, Notify, OwnedMutexGuard, broadcast, watch};

use crate::engine::supervisor::Supervisor;
use crate::engine::{EngineError, pb};
use crate::store::{Applied, Change, Chat, Store, read};
use crate::sync::Shared;
use crate::work::{Priority, Report, Serial};

/// How long sending waits for things, which tests shorten.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// The pause after a send fails on the way, at first and at most; it doubles each time.
    pub retry_first: Duration,
    pub retry_most: Duration,
    /// How long a message WhatsApp refused, when it may already have gone, waits for a receipt.
    pub unconfirmed_for: Duration,
    /// The shortest time between two of a chat's read receipts.
    pub read_every: Duration,
    /// How often typing is said again while it goes on, and how long after the interface last
    /// said so it's stopped.
    pub typing_again: Duration,
    pub typing_quiet: Duration,
    /// How long an interface must have had focus before you're online, and none before you're away.
    pub online_after: Duration,
    pub away_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            retry_first: Duration::from_secs(1),
            retry_most: Duration::from_secs(60),
            unconfirmed_for: Duration::from_secs(24 * 3600),
            read_every: Duration::from_secs(2),
            typing_again: Duration::from_secs(10),
            typing_quiet: Duration::from_secs(10),
            online_after: Duration::from_millis(500),
            away_after: Duration::from_secs(30),
        }
    }
}

/// How many chats send at once.
const CHATS_AT_ONCE: usize = 16;

#[derive(Clone)]
pub struct Sending {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    engine: Option<Supervisor>,
    sends: Arc<Serial>,
    timing: Timing,
    /// Enqueues one at a time, so a chat's sends are queued in the order they were committed.
    enqueue: Mutex<()>,
    /// Each chat's place for a message being made, held from its request to its enqueue, so one
    /// that waits for its link preview isn't overtaken by the next sent there.
    composing: std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>,
    previews: link::Previews,
    /// Woken each time the connection to WhatsApp comes back, for whatever waits to try again.
    online: Notify,
    /// Whether WhatsApp can be reached now; sends wait for it.
    connected: watch::Sender<bool>,
    /// Actions waiting for their message to reach WhatsApp.
    held: outbox::Held,
    reads: reads::Reads,
    typing: typing::Typists,
    /// What every interface is told, for saying WhatsApp refused an action.
    pushes: broadcast::Sender<Push>,
    /// Stops the next attempt that withdraws an action, between reading its row and withdrawing it:
    /// it notifies the first when it stops, and goes on once the second is notified.
    #[cfg(test)]
    pausing: Pause,
    /// Stops the next action on a message, between finding the message and its step, as `pausing`
    /// does.
    #[cfg(test)]
    acting: Pause,
}

/// Where a test stops jawd the next time it gets there, with what it notifies as it stops and
/// what it waits for to go on.
#[cfg(test)]
type Pause = std::sync::Mutex<Option<(Arc<Notify>, Arc<Notify>)>>;

/// Stops at `pause`, if a test asked to.
#[cfg(test)]
async fn paused(pause: &Pause) {
    let pausing = pause.lock().expect("the test's pause").take();
    if let Some((stopped, go)) = pausing {
        stopped.notify_one();
        go.notified().await;
    }
}

impl Sending {
    /// Starts sending: what the outbox held is queued again in each chat's order, read receipts still
    /// owed are sent, and presence follows `focused`, whether any interface is in front, and the
    /// connection, which `live` says now and `pushes` as it changes. Text goes with the link
    /// previews `previews` makes.
    pub async fn start(
        store: Store,
        engine: Option<Supervisor>,
        live: Shared,
        pushes: &broadcast::Sender<Push>,
        focused: watch::Receiver<bool>,
        timing: Timing,
        previews: link::Previews,
    ) -> anyhow::Result<Sending> {
        let (notices, pushes) = (pushes.clone(), pushes.subscribe());
        let online = presence::connected(&live.lock().expect("live state").connection);
        let inner = Arc::new(Inner {
            store,
            engine,
            sends: Serial::new("sends", CHATS_AT_ONCE),
            timing,
            enqueue: Mutex::new(()),
            composing: std::sync::Mutex::default(),
            previews,
            online: Notify::new(),
            connected: watch::channel(online).0,
            held: outbox::Held::default(),
            reads: reads::Reads::default(),
            typing: typing::Typists::new(),
            pushes: notices,
            #[cfg(test)]
            pausing: Pause::default(),
            #[cfg(test)]
            acting: Pause::default(),
        });
        typing::start(&inner);
        regroup::start(&inner);
        outbox::release(&inner);
        tokio::spawn(presence::follow(inner.clone(), pushes, focused, online));
        outbox::resume(&inner).await?;
        reads::resume(&inner).await?;
        Ok(Sending { inner })
    }

    /// The sends' lane, for `status`.
    pub fn lane(&self) -> Arc<dyn Report> {
        self.inner.sends.clone()
    }

    /// Stops the next attempt that withdraws an action, between reading its row and withdrawing
    /// it: it notifies the first when it stops, and goes on once the second is notified.
    #[cfg(test)]
    fn pause_next_withdrawal(&self) -> (Arc<Notify>, Arc<Notify>) {
        let pause = (Arc::default(), Arc::default());
        *self.inner.pausing.lock().expect("the test's pause") = Some(pause.clone());
        pause
    }

    /// Stops the next reaction, edit or deletion for everyone between finding its message and its
    /// step: it notifies the first when it stops, and goes on once the second is notified.
    #[cfg(test)]
    fn pause_next_action(&self) -> (Arc<Notify>, Arc<Notify>) {
        let pause = (Arc::default(), Arc::default());
        *self.inner.acting.lock().expect("the test's pause") = Some(pause.clone());
        pause
    }

    /// Answers one of the sending methods: `send.text`, `send.retry`, `chat.start`, `draft.set`,
    /// `read`, `typing`, `chat.info` and the actions on a message.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let inner = &self.inner;
        match method {
            "send.text" => result(outbox::send_text(inner, params_of::<SendText>(method, params)?).await?),
            "send.poll" => result(poll::send_poll(inner, params_of::<SendPoll>(method, params)?).await?),
            "poll.vote" => poll::vote(inner, params_of::<PollVote>(method, params)?).await,
            "send.retry" => outbox::retry(inner, params_of::<SendRetry>(method, params)?).await,
            "chat.start" => result(start::start(inner, params_of::<ChatStart>(method, params)?).await?),
            "draft.set" => draft(inner, params_of::<DraftSet>(method, params)?).await,
            "read" => reads::read(inner, params_of::<Read>(method, params)?).await,
            "typing" => typing::typing(inner, params_of::<Typing>(method, params)?),
            "chat.info" => result(info::chat_info(inner, params_of::<ChatInfo>(method, params)?).await?),
            "react" => actions::react(inner, params_of::<React>(method, params)?).await,
            "edit" => actions::edit(inner, params_of::<Edit>(method, params)?).await,
            "delete" => actions::delete(inner, params_of::<Delete>(method, params)?).await,
            "forward" => result(forward::forward(inner, params_of::<Forward>(method, params)?).await?),
            "message.info" => result(info::message_info(inner, params_of::<MessageInfo>(method, params)?).await?),
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        }
    }

    /// The methods `call` answers.
    pub const METHODS: &[&str] = &[
        "send.text",
        "send.poll",
        "poll.vote",
        "send.retry",
        "chat.start",
        "draft.set",
        "read",
        "typing",
        "chat.info",
        "react",
        "edit",
        "delete",
        "forward",
        "message.info",
    ];
}

/// The longest text a message or a draft can have, in characters, as WhatsApp allows.
const TEXT_MOST: usize = 65_536;

/// Takes the places of `chats` for messages being made, in the order of their JIDs so two takers
/// never wait for each other, once the messages made there before have been enqueued; held until
/// they're dropped.
async fn composing(inner: &Inner, chats: &[&str]) -> Vec<OwnedMutexGuard<()>> {
    let mut chats = chats.to_vec();
    chats.sort_unstable();
    chats.dedup();
    let places: Vec<Arc<Mutex<()>>> = {
        let mut composing = inner.composing.lock().expect("chats being written to");
        // A place nobody holds or waits for is let go of, so they're as many as the chats in use.
        composing.retain(|_, place| Arc::strong_count(place) > 1);
        chats.iter().map(|chat| composing.entry(chat.to_string()).or_default().clone()).collect()
    };
    let mut held = Vec::with_capacity(places.len());
    for place in places {
        held.push(place.lock_owned().await);
    }
    held
}

async fn draft(inner: &Inner, draft: DraftSet) -> Result<Value, Error> {
    if draft.text.chars().count() > TEXT_MOST {
        return Err(Error::new(code::LIMIT, format!("a draft can be {TEXT_MOST} characters at most")));
    }
    let chat = draft.chat.clone();
    if query(inner, move |db| read::chat_n(db, &chat)).await?.is_none() {
        return Err(no_chat(&draft.chat));
    }
    let change = Change::Chat(Chat { id: draft.chat, draft: Some(draft.text), ..Chat::default() });
    durably(inner, vec![change], "keep the draft").await?;
    Ok(Value::Object(Default::default()))
}

fn result(value: impl Serialize) -> Result<Value, Error> {
    Ok(serde_json::to_value(value).expect("jawd's own types serialize"))
}

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn no_chat(chat: &str) -> Error {
    Error::new(code::NOT_FOUND, format!("jawd has no chat {chat}; chat.start finds or makes one"))
}

fn internal(what: &str, e: anyhow::Error) -> Error {
    Error::new(code::INTERNAL, format!("jawd couldn't {what}: {e:#}"))
}

/// Runs a query on the read pool, ahead of background ones.
async fn query<T: Send + 'static>(
    inner: &Inner,
    query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Error> {
    inner.store.reads().query(Priority::Interactive, query).await.map_err(|e| internal("read jaw.db", e))
}

/// Makes `changes` in one transaction synced to the disk before it returns, as everything an
/// interface is told is done must be; `what` says what for, if it fails.
async fn durably(inner: &Inner, changes: Vec<Change>, what: &str) -> Result<Applied, Error> {
    let applied = inner.store.apply_durable(changes).await.map_err(|e| internal(what, e))?;
    if applied.quarantined > 0 {
        return Err(Error::new(code::INTERNAL, format!("jawd couldn't {what}; what failed is kept in quarantine")));
    }
    Ok(applied)
}

/// Asks the engine running now, if there is one.
async fn engine(inner: &Inner, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
    match &inner.engine {
        Some(engine) => engine.call(kind).await,
        None => Err(EngineError::Unavailable("jawd runs no engine".into())),
    }
}

/// Whether what the engine said means trying again later might work.
fn retryable(e: &EngineError) -> bool {
    match e {
        EngineError::Gone | EngineError::Unavailable(_) => true,
        EngineError::Failed { retry, .. } => *retry,
    }
}

/// The protocol's error for something the engine couldn't do.
fn engine_error(e: EngineError) -> Error {
    match e {
        EngineError::Failed { message, retry: false } => Error::new(code::NOT_ALLOWED, message),
        e => {
            let mut error = Error::new(code::ENGINE_UNAVAILABLE, e.to_string());
            error.data = Some(serde_json::json!({ "retry_ms": 1_000 }));
            error
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}
