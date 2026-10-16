//! jaw on jawd: `Jawd` over jaw-client's `Client`. The client's threads read and write the
//! socket, merging each window's updates as they come, and wake the loop at most once a frame; the
//! loop drains them here into the interface's `Update`s. What jawd turns down is said where the
//! user acted, quietly: a message that wasn't sent goes back to the box, an action says why in the
//! footer, and what goes on in the background (drafts, receipts, typing) says nothing.

use crate::data::{Chat, Counts, Found, FoundKind, Jawd, Message, Place, Typing, Update};
use crate::run::Event;
use jaw_client::{Client, Config, Event as From, Failure, WindowId};
use jaw_proto::find::{Find, Kind};
use jaw_proto::{AccountState, LinkStart, code};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// How long jaw waits for a jawd it started to answer before it says it can't reach it.
const PATIENCE: Duration = Duration::from_secs(15);

/// What a request was, for what to say if jawd turns it down.
enum Asked {
    Send {
        chat: String,
        text: String,
    },
    /// Something the user did, named as the footer says it: "muting".
    Did(&'static str),
    StartChat,
    /// Playing or recording a voice message, which the interface stops showing if it's refused.
    Voice,
    /// Background work that says nothing either way.
    Quiet,
}

/// The jawd jaw started, while it starts.
struct Starting {
    child: Child,
    since: Instant,
    log: PathBuf,
}

pub struct Wire {
    client: Client<Chat, Message>,
    tx: Sender<Event>,
    list: Option<WindowId>,
    open: Option<(WindowId, String)>,
    asked: HashMap<u64, Asked>,
    /// The client's id for each `find`, to the interface's.
    finds: HashMap<u64, u64>,
    starting: Option<Starting>,
}

/// What to say about a request jawd turned down: one it doesn't serve yet comes later.
fn why_not(failure: &Failure, what: &str) -> String {
    match failure {
        Failure::Jawd(e) if e.code == code::UNKNOWN_METHOD => format!("{what} comes with the next step"),
        other => other.to_string(),
    }
}

/// Where jawd is: beside jaw, as `make install` and cargo put them, or on the PATH.
pub fn jawd_binary() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join("jawd")));
    beside.filter(|p| p.exists()).unwrap_or_else(|| PathBuf::from("jawd"))
}

/// Starts jawd on `home`, on its own, so it goes on when jaw quits; its output goes to a log
/// beside its socket.
fn start_jawd(home: &Path) -> Result<Starting, String> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(home)
        .map_err(|e| format!("can't make {}: {e}", home.display()))?;
    let log = home.join("jawd.log");
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| format!("can't write {}: {e}", log.display()))?;
    let child = Command::new(jawd_binary())
        .env("JAW_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(out)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("can't start jawd: {e}; make build builds it"))?;
    Ok(Starting { child, since: Instant::now(), log })
}

/// The end of a log, for why a jawd that stopped as it started did.
fn tail(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    text.lines().rev().find(|l| !l.trim().is_empty()).map(|l| format!(": {}", l.trim())).unwrap_or_default()
}

fn kind(k: FoundKind) -> Kind {
    match k {
        FoundKind::Chat => Kind::Chat,
        FoundKind::Person => Kind::Person,
        FoundKind::Folder => Kind::Folder,
        FoundKind::Message => Kind::Message,
        FoundKind::Link => Kind::Link,
        FoundKind::Media => Kind::Media,
        FoundKind::File => Kind::File,
    }
}

impl Wire {
    /// Connects to jawd on `socket`, in the background; what it sends wakes the loop on `tx`.
    pub fn connect(socket: PathBuf, tx: Sender<Event>) -> Wire {
        let wake = tx.clone();
        let config = Config { focus: true, ..Config::new(socket, "jaw", env!("CARGO_PKG_VERSION")) };
        let client = Client::start(config, move || {
            let _ = wake.send(Event::Jawd);
        });
        Wire { client, tx, list: None, open: None, asked: HashMap::new(), finds: HashMap::new(), starting: None }
    }

    /// Connects to jawd on `home`, starting it first if nothing answers there.
    pub fn start(home: &Path, tx: Sender<Event>) -> Wire {
        let socket = jaw_proto::local::socket(home);
        let starting = (UnixStream::connect(&socket).is_err()).then(|| start_jawd(home));
        let mut wire = Wire::connect(socket, tx.clone());
        match starting {
            Some(Ok(s)) => {
                wire.starting = Some(s);
                let _ = tx.send(Event::Update(Update::Starting { failed: None }));
            }
            Some(Err(why)) => {
                let _ = tx.send(Event::Update(Update::Starting { failed: Some(why) }));
            }
            None => {}
        }
        wire
    }

    fn say(&self, u: Update) {
        let _ = self.tx.send(Event::Update(u));
    }

    fn ask(&mut self, method: &str, params: Value, asked: Asked) {
        match self.client.request(method, params) {
            Ok(id) => {
                self.asked.insert(id, asked);
            }
            Err(failure) => {
                if let Some(u) = self.refused(asked, &failure) {
                    self.say(u);
                }
            }
        }
    }

    /// What the interface hears of a request jawd didn't do.
    fn refused(&self, asked: Asked, failure: &Failure) -> Option<Update> {
        match asked {
            Asked::Send { chat, text } => Some(Update::Unsent { chat, text, why: why_not(failure, "sending") }),
            Asked::Did(what) => Some(Update::Notice(why_not(failure, what))),
            Asked::StartChat => Some(Update::Notice(why_not(failure, "starting a chat"))),
            Asked::Voice => Some(Update::VoiceRefused(why_not(failure, "voice"))),
            Asked::Quiet => None,
        }
    }

    /// The client couldn't reach jawd: while the jawd jaw started starts, that's starting.
    fn unreachable(&mut self, why: String, retry_in: Duration, out: &mut Vec<Update>) {
        if let Some(s) = self.starting.as_mut() {
            match s.child.try_wait() {
                Ok(Some(status)) => {
                    let failed = format!("jawd stopped as it started ({status}){}", tail(&s.log));
                    self.starting = None;
                    out.push(Update::Starting { failed: Some(failed) });
                    return;
                }
                _ if s.since.elapsed() < PATIENCE => {
                    out.push(Update::Starting { failed: None });
                    return;
                }
                _ => self.starting = None,
            }
        }
        out.push(Update::Unreachable { why, retry_in: retry_in.as_millis() as i64 });
    }

    fn event(&mut self, e: From, out: &mut Vec<Update>) {
        match e {
            From::Connected(w) => {
                self.starting = None;
                let mut me = vec![];
                if let Some(m) = w.account.me {
                    me.push(m.id);
                    let digits: String = m.phone.chars().filter(char::is_ascii_digit).collect();
                    if !digits.is_empty() {
                        me.push(format!("{digits}@s.whatsapp.net"));
                    }
                }
                out.push(Update::Hello {
                    root: w.root,
                    linked: w.account.state == AccountState::Linked,
                    me,
                    connection: w.connection,
                });
            }
            From::Disconnected { why, retry_in } => self.unreachable(why, retry_in, out),
            From::Reply { id, outcome } => {
                let Some(asked) = self.asked.remove(&id) else { return };
                match (asked, outcome) {
                    (Asked::StartChat, Ok(result)) => {
                        if let Ok(chat) = serde_json::from_value::<Chat>(result["chat"].clone()) {
                            out.push(Update::ChatStarted { chat });
                        }
                    }
                    (asked, Err(failure)) => out.extend(self.refused(asked, &failure)),
                    _ => {}
                }
            }
            From::Found { id, outcome, .. } => {
                let Some(view) = self.finds.remove(&id) else { return };
                self.finds.retain(|asked, _| *asked > id);
                out.push(match outcome {
                    Ok(found) => {
                        let results = found
                            .results
                            .iter()
                            .filter_map(|hit| serde_json::to_value(hit).ok())
                            .filter_map(|v| serde_json::from_value::<Found>(v).ok())
                            .collect();
                        let counts = serde_json::to_value(&found.counts)
                            .ok()
                            .and_then(|v| serde_json::from_value::<Counts>(v).ok())
                            .unwrap_or_default();
                        Update::Found { id: view, results, counts }
                    }
                    Err(Failure::Jawd(e)) if e.code == code::CANCELLED => {
                        Update::FindFailed { id: view, cancelled: true, why: e.message }
                    }
                    Err(failure) => Update::FindFailed { id: view, cancelled: false, why: failure.to_string() },
                });
            }
            From::Connection(c) => out.push(Update::Connection(c)),
            From::Link(step) => out.push(Update::Link(step)),
            From::Push(push) if push.method == "notice" => {
                if let Some(text) = push.params["text"].as_str() {
                    out.push(Update::Notice(text.to_string()));
                }
            }
            From::Push(push) if push.method == "playback" => {
                out.extend(serde_json::from_value(push.params).ok().map(Update::Playback));
            }
            From::Push(push) if push.method == "recording" => {
                out.extend(serde_json::from_value(push.params).ok().map(Update::Recorded));
            }
            // A window that couldn't move stays where it was, which the screen already shows.
            From::Window { .. } | From::Push(_) => {}
        }
    }
}

impl Jawd for Wire {
    fn watch_chats(&mut self, place: Place) {
        match self.list {
            Some(w) => self.client.move_window(w, place),
            None => self.list = Some(self.client.watch_chats("all", place)),
        }
    }

    fn watch_messages(&mut self, chat: &str, place: Place) {
        match self.open.take() {
            Some((w, open)) if open == chat => {
                self.client.move_window(w, place);
                self.open = Some((w, open));
            }
            other => {
                if let Some((w, _)) = other {
                    self.client.unwatch(w);
                }
                self.open = Some((self.client.watch_messages(chat, place), chat.to_string()));
            }
        }
    }

    fn opened(&mut self, chat: &str) {
        self.ask("chat.opened", json!({ "chat": chat }), Asked::Quiet);
    }

    fn send_text(&mut self, chat: &str, text: &str) {
        let asked = Asked::Send { chat: chat.to_string(), text: text.to_string() };
        self.ask("send.text", json!({ "chat": chat, "text": text }), asked);
    }

    fn draft(&mut self, chat: &str, text: &str) {
        self.ask("draft.set", json!({ "chat": chat, "text": text }), Asked::Quiet);
    }

    fn read(&mut self, chat: &str, up_to: &str) {
        self.ask("read", json!({ "chat": chat, "up_to": up_to }), Asked::Quiet);
    }

    fn typing(&mut self, chat: &str, state: Typing) {
        let state = match state {
            Typing::Composing => "composing",
            Typing::Paused => "paused",
        };
        self.ask("typing", json!({ "chat": chat, "state": state }), Asked::Quiet);
    }

    fn focus(&mut self, focused: bool) {
        if let Ok(id) = self.client.focus(focused) {
            self.asked.insert(id, Asked::Quiet);
        }
    }

    fn link_start(&mut self, phone: Option<String>) {
        let params = serde_json::to_value(LinkStart { phone }).unwrap_or(Value::Null);
        self.ask("link.start", params, Asked::Did("linking"));
    }

    fn find(&mut self, id: u64, query: &str, k: Option<FoundKind>, chat: Option<&str>, limit: usize) {
        let find = Find {
            query: query.to_string(),
            kinds: k.map(kind).into_iter().collect(),
            chat: chat.map(String::from),
            limit: Some(limit as u32),
        };
        match self.client.find(find) {
            Ok(asked) => {
                self.finds.insert(asked, id);
            }
            Err(failure) => self.say(Update::FindFailed { id, cancelled: false, why: failure.to_string() }),
        }
    }

    fn start_chat(&mut self, person: &str) {
        self.ask("chat.start", json!({ "person": person }), Asked::StartChat);
    }

    fn mute(&mut self, chat: &str, until: Option<i64>) {
        self.ask("chat.mute", json!({ "chat": chat, "until": until }), Asked::Did("muting"));
    }

    fn archive(&mut self, chat: &str, archived: bool) {
        self.ask("chat.archive", json!({ "chat": chat, "archived": archived }), Asked::Did("archiving"));
    }

    fn pin(&mut self, chat: &str, pinned: bool) {
        self.ask("chat.pin", json!({ "chat": chat, "pinned": pinned }), Asked::Did("pinning"));
    }

    fn mark_unread(&mut self, chat: &str, unread: bool) {
        let what = if unread { "marking a chat unread" } else { "marking a chat read" };
        self.ask("chat.mark_unread", json!({ "chat": chat, "unread": unread }), Asked::Did(what));
    }

    fn voice_play(&mut self, message: Option<&str>, at: Option<f64>, speed: f64) {
        let mut params = match message {
            Some(m) => json!({ "message": m, "speed": speed }),
            None => json!({ "recording": true, "speed": speed }),
        };
        if let Some(at) = at {
            params["at"] = json!(at);
        }
        self.ask("voice.play", params, Asked::Voice);
    }

    fn voice_pause(&mut self) {
        self.ask("voice.pause", json!({}), Asked::Voice);
    }

    fn voice_seek(&mut self, at: f64) {
        self.ask("voice.seek", json!({ "at": at }), Asked::Voice);
    }

    fn voice_speed(&mut self, speed: f64) {
        self.ask("voice.speed", json!({ "speed": speed }), Asked::Voice);
    }

    fn voice_record(&mut self, chat: &str) {
        self.ask("voice.record", json!({ "chat": chat }), Asked::Voice);
    }

    fn voice_stop(&mut self) {
        self.ask("voice.stop", json!({}), Asked::Voice);
    }

    fn voice_send(&mut self) {
        self.ask("voice.send", json!({}), Asked::Voice);
    }

    fn voice_cancel(&mut self) {
        self.ask("voice.cancel", json!({}), Asked::Quiet);
    }

    fn drain(&mut self) -> Vec<Update> {
        let frame = self.client.drain();
        let mut out = vec![];
        for e in frame.events {
            self.event(e, &mut out);
        }
        for (w, update) in frame.chats {
            if Some(w) == self.list {
                out.push(Update::Chats(update));
            }
        }
        for (w, update) in frame.messages {
            if let Some((open, chat)) = &self.open
                && *open == w
            {
                out.push(Update::Messages { chat: chat.clone(), update });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
