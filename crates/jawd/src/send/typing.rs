//! Typing, as chat presence. An interface says typing as you type; jawd tells WhatsApp when it
//! starts or changes, and again only every `Timing::typing_again` while it goes on, so recipients
//! keep seeing it however often it's said. Paused goes at once; typing that nobody has said again
//! for `Timing::typing_quiet` is stopped. They go to the engine in the order they were said, and
//! never hold up the interface: the reply comes before WhatsApp hears.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use jaw_proto::send::{Typing, TypingState};
use jaw_proto::{Error, code};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{Inner, engine};
use crate::engine::pb;

/// Chat presences waiting for the engine; past that, typing is best left unsaid.
const WAITING_MOST: usize = 64;

pub(super) struct Typists {
    chats: std::sync::Mutex<HashMap<String, Typist>>,
    out: mpsc::Sender<pb::ChatPresence>,
    queued: std::sync::Mutex<Option<mpsc::Receiver<pb::ChatPresence>>>,
}

/// What WhatsApp was last told of typing in a chat, when, and which of the interface's word of it
/// was the latest.
struct Typist {
    state: TypingState,
    said: Instant,
    word: u64,
}

impl Typists {
    pub(super) fn new() -> Typists {
        let (out, queued) = mpsc::channel(WAITING_MOST);
        Typists { chats: Default::default(), out, queued: std::sync::Mutex::new(Some(queued)) }
    }
}

/// Starts telling the engine, one chat presence after another.
pub(super) fn start(inner: &Arc<Inner>) {
    let Some(mut queued) = inner.typing.queued.lock().expect("typing").take() else { return };
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        while let Some(presence) = queued.recv().await {
            let Some(inner) = weak.upgrade() else { return };
            if let Err(e) = engine(&inner, pb::command::Kind::ChatPresence(presence)).await {
                eprintln!("jawd: can't say you're typing: {e}");
            }
        }
    });
}

fn state(state: TypingState) -> pb::chat_presence::State {
    match state {
        TypingState::Composing => pb::chat_presence::State::Composing,
        TypingState::Recording => pb::chat_presence::State::Recording,
        TypingState::Paused => pb::chat_presence::State::Paused,
    }
}

fn say(inner: &Inner, chat: &str, typing: TypingState) {
    let presence = pb::ChatPresence { chat: chat.to_string(), state: state(typing) as i32 };
    if inner.typing.out.try_send(presence).is_err() {
        eprintln!("jawd: typing in {chat} went unsaid, since the engine is behind");
    }
}

pub(super) fn typing(inner: &Arc<Inner>, typing: Typing) -> Result<Value, Error> {
    if !typing.chat.contains('@') {
        return Err(Error::new(code::INVALID_PARAMS, format!("{} isn't a chat", typing.chat)));
    }
    let chat = typing.chat;
    let now = Instant::now();
    let word = {
        let mut chats = inner.typing.chats.lock().expect("typing");
        if typing.state == TypingState::Paused {
            if chats.remove(&chat).is_some() {
                say(inner, &chat, TypingState::Paused);
            }
            return Ok(Value::Object(Default::default()));
        }
        let typist = chats.entry(chat.clone()).or_insert(Typist { state: typing.state, said: now, word: 0 });
        if typist.word == 0 || typist.state != typing.state || typist.said.elapsed() >= inner.timing.typing_again {
            say(inner, &chat, typing.state);
            typist.said = now;
        }
        typist.state = typing.state;
        typist.word += 1;
        typist.word
    };
    let quiet = inner.timing.typing_quiet;
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        tokio::time::sleep(quiet).await;
        let Some(inner) = weak.upgrade() else { return };
        let mut chats = inner.typing.chats.lock().expect("typing");
        if chats.get(&chat).is_some_and(|typist| typist.word == word) {
            chats.remove(&chat);
            say(&inner, &chat, TypingState::Paused);
        }
    });
    Ok(Value::Object(Default::default()))
}
