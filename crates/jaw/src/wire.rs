//! jaw on jawd: `Jawd` over jaw-client's `Client`. The client's threads read and write the
//! socket, merging each window's updates as they come, and wake the loop at most once a frame; the
//! loop drains them here into the interface's `Update`s. What jawd turns down is said where the
//! user acted, quietly: a message that wasn't sent goes back to the box, an action says why in the
//! footer, and what goes on in the background (drafts, receipts, typing) says nothing. A message
//! whose answer a dropped connection lost is asked for again with its key once jawd is back, and
//! so are the drafts and read positions jawd hadn't answered for.

use crate::data::{Chat, Counts, Found, FoundKind, Jawd, LibraryAsk, Message, MessageInfo, Place, Typing, Update};
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
use std::time::{Duration, Instant, SystemTime};

/// How long jaw waits for a jawd it started to answer before it says it can't reach it.
const PATIENCE: Duration = Duration::from_secs(15);
/// How long after a message was first asked for jaw asks for it again with its key. jawd keeps a
/// key for a day, so past that a second ask could make a second message.
const KEY_KEPT: Duration = Duration::from_secs(20 * 60 * 60);

/// A message asked for with `send.text`, kept until jawd answers for it.
struct Sending {
    chat: String,
    text: String,
    key: String,
    params: Value,
    /// When it was first asked for, by the wall clock, which jawd's day of keys is counted by and
    /// which goes on while the computer sleeps.
    first: SystemTime,
}

/// What a request was, for what to say if jawd turns it down.
enum Asked {
    Send(Box<Sending>),
    /// `send.retry` for a failed message.
    Retry,
    /// A chat's draft or read position: `draft.set` or `read`, asked again once jawd is back if the
    /// connection dropped first, unless something newer was asked for the chat meanwhile.
    Owed(&'static str, String, Value),
    /// Something the user did, named as the footer says it: "muting".
    Did(&'static str),
    StartChat,
    /// Playing or recording a voice message, which the interface stops showing if it's refused.
    Voice,
    /// A message's receipts, for the sheet waiting for them.
    Info(String),
    /// A chat's members, for completing `@` names; refused, the interface uses who it's seen.
    Members(String),
    /// Forwarding to this many chats.
    Forward(usize),
    /// The folder tree's window.
    Folders,
    /// A folder's roster window.
    Roster(String),
    /// A folder being made.
    FolderMade,
    /// The library's window, for the ask the interface numbered so.
    Library(u64),
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
    /// The folder the chat list's window shows.
    list_folder: String,
    /// The folder tree's watch, once asked for, and its ID once jawd answers.
    folders: Option<Option<String>>,
    /// The roster's folder, and its watch's ID once jawd answers.
    roster: Option<(String, Option<String>)>,
    /// What the library asked, and its watch's ID once jawd answers.
    library: Option<(LibraryAsk, Option<String>)>,
    open: Option<(WindowId, String)>,
    asked: HashMap<u64, Asked>,
    /// Messages whose answer the connection lost, to ask for again with their keys once jawd is
    /// back.
    again: Vec<Sending>,
    /// The newest `draft.set` and `read` asked for each chat that jawd hasn't answered.
    owed: HashMap<(&'static str, String), Value>,
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

/// Why jawd didn't take a message, for its box: the outbox's limit in a few words, or jawd's own.
fn why_unsent(failure: &Failure) -> String {
    match failure {
        Failure::Jawd(e) if e.code == code::LIMIT => {
            let data = e.data.as_ref();
            let n = |field: &str| data.and_then(|d| d[field].as_u64());
            match (data.and_then(|d| d["limit"].as_str()), n("waiting"), n("failed")) {
                (Some("outbox"), Some(waiting), failed) => {
                    let failed = failed.filter(|f| *f > 0).map_or(String::new(), |f| format!(", {f} failed"));
                    format!("{waiting} wait to send{failed}; it can go once some have")
                }
                _ => e.message.clone(),
            }
        }
        other => other.to_string(),
    }
}

/// A library window's answer or push, as the interface takes it.
fn library(asked: u64, v: &Value, whole: bool) -> Update {
    let rows = if whole { &v["rows"] } else { &v["upsert"] };
    let window = crate::data::LibraryWindow {
        upsert: serde_json::from_value(rows.clone()).unwrap_or_default(),
        remove: serde_json::from_value(v["remove"].clone()).unwrap_or_default(),
        whole,
        total: v["total"].as_u64().map(|n| n as u32),
        older: v["older"].as_bool(),
        counts: serde_json::from_value(v["counts"].clone()).ok(),
    };
    Update::Library { asked, window }
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
        Wire {
            client,
            tx,
            list: None,
            list_folder: "all".into(),
            folders: None,
            roster: None,
            library: None,
            open: None,
            asked: HashMap::new(),
            again: vec![],
            owed: HashMap::new(),
            finds: HashMap::new(),
            starting: None,
        }
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

    /// Asks for a chat's draft or read position, which is owed until jawd answers for it.
    fn owe(&mut self, method: &'static str, chat: &str, params: Value) {
        self.owed.insert((method, chat.to_string()), params.clone());
        self.ask(method, params.clone(), Asked::Owed(method, chat.to_string(), params));
    }

    /// Settles what was owed for a chat, unless something newer has been asked for it since.
    fn settle(&mut self, method: &'static str, chat: String, params: &Value) {
        let key = (method, chat);
        if self.owed.get(&key) == Some(params) {
            self.owed.remove(&key);
        }
    }

    /// jawd is back: what the connection lost is asked for again, messages with their keys, so
    /// jawd answers with any it already has rather than making another.
    fn ask_again(&mut self, out: &mut Vec<Update>) {
        for sending in std::mem::take(&mut self.again) {
            if sending.first.elapsed().unwrap_or_default() > KEY_KEPT {
                let why = "jawd was away too long to tell whether it went, so look in the chat before sending it again";
                let Sending { chat, text, key, .. } = sending;
                out.push(Update::Unsent { chat, key, text, why: why.into() });
            } else {
                let params = sending.params.clone();
                self.ask("send.text", params, Asked::Send(Box::new(sending)));
            }
        }
        let owed: Vec<_> = self.owed.iter().map(|((m, c), p)| (*m, c.clone(), p.clone())).collect();
        for (method, chat, params) in owed {
            self.ask(method, params.clone(), Asked::Owed(method, chat, params));
        }
    }

    /// What the interface hears of a request jawd didn't do.
    fn refused(&mut self, asked: Asked, failure: &Failure) -> Option<Update> {
        match asked {
            Asked::Send(sending) if matches!(failure, Failure::Lost) => {
                let (chat, key) = (sending.chat.clone(), sending.key.clone());
                self.again.push(*sending);
                Some(Update::Unsure { chat, key })
            }
            Asked::Send(sending) => {
                let Sending { chat, text, key, .. } = *sending;
                Some(Update::Unsent { chat, key, text, why: why_unsent(failure) })
            }
            Asked::Retry => Some(Update::Notice(format!("not sent again: {failure}"))),
            Asked::Owed(method, chat, params) => match failure {
                Failure::Jawd(e) => {
                    self.settle(method, chat, &params);
                    (method == "draft.set").then(|| Update::Notice(format!("the draft wasn't kept: {}", e.message)))
                }
                _ => None,
            },
            Asked::Did(what) => Some(Update::Notice(why_not(failure, what))),
            Asked::StartChat => Some(Update::NotStarted(failure.to_string())),
            Asked::Voice => Some(Update::VoiceRefused(why_not(failure, "voice"))),
            Asked::Info(message) => Some(Update::Info { message, info: Err(why_not(failure, "a message's info")) }),
            Asked::Members(chat) => Some(Update::Members { chat, members: vec![] }),
            Asked::Forward(_) => Some(Update::Notice(why_not(failure, "forwarding"))),
            Asked::Folders | Asked::Roster(_) => None,
            Asked::Library(_) => Some(Update::LibraryRefused(why_not(failure, "the library"))),
            Asked::FolderMade => Some(Update::Notice(why_not(failure, "making folders"))),
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
                if self.folders.take().is_some() {
                    self.watch_folders();
                }
                if let Some((folder, _)) = self.roster.take() {
                    self.watch_roster(&folder);
                }
                if let Some((ask, _)) = self.library.take() {
                    self.watch_library(&ask);
                }
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
                self.ask_again(out);
            }
            From::Disconnected { why, retry_in } => self.unreachable(why, retry_in, out),
            From::Reply { id, outcome } => {
                let Some(asked) = self.asked.remove(&id) else { return };
                match (asked, outcome) {
                    (Asked::Send(sending), Ok(result)) => {
                        let message = result["message"].as_str().unwrap_or_default().to_string();
                        out.push(Update::Sent { key: sending.key, message });
                    }
                    (Asked::Owed(method, chat, params), Ok(_)) => self.settle(method, chat, &params),
                    (Asked::StartChat, Ok(result)) => {
                        if let Ok(chat) = serde_json::from_value::<Chat>(result["chat"].clone()) {
                            out.push(Update::ChatStarted { chat: Box::new(chat) });
                        }
                    }
                    (Asked::Info(message), Ok(result)) => {
                        let info = serde_json::from_value::<MessageInfo>(result).map_err(|e| e.to_string());
                        out.push(Update::Info { message, info });
                    }
                    (Asked::Members(chat), Ok(result)) => {
                        let members = serde_json::from_value(result["members"].clone()).unwrap_or_default();
                        out.push(Update::Members { chat, members });
                    }
                    (Asked::Folders, Ok(result)) => {
                        self.folders = Some(result["watch"].as_str().map(String::from));
                        let upsert = serde_json::from_value(result["rows"].clone()).unwrap_or_default();
                        out.push(Update::Folders { upsert, remove: vec![], whole: true });
                    }
                    (Asked::Roster(folder), Ok(result)) => {
                        if self.roster.as_ref().is_some_and(|(f, _)| *f == folder) {
                            self.roster = Some((folder.clone(), result["watch"].as_str().map(String::from)));
                            let upsert = serde_json::from_value(result["rows"].clone()).unwrap_or_default();
                            out.push(Update::Roster { folder, upsert, remove: vec![], whole: true });
                        }
                    }
                    (Asked::Library(asked), Ok(result)) => {
                        if let Some((_, watch)) = self.library.as_mut().filter(|(a, _)| a.id == asked) {
                            *watch = result["watch"].as_str().map(String::from);
                            out.push(library(asked, &result, true));
                        }
                    }
                    (Asked::FolderMade, Ok(result)) => {
                        if let Ok(folder) = serde_json::from_value(result["folder"].clone()) {
                            out.push(Update::FolderMade(Box::new(folder)));
                        }
                    }
                    (Asked::Forward(n), Ok(_)) => out.push(Update::Notice(if n == 1 {
                        "forwarded".into()
                    } else {
                        format!("forwarded to {n} chats")
                    })),
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
            From::Push(push) if push.method == "library" => {
                if let Some((ask, Some(w))) = &self.library
                    && push.params["watch"].as_str() == Some(w.as_str())
                {
                    out.push(library(ask.id, &push.params, false));
                }
            }
            From::Push(push) if push.method == "folders" => {
                let upsert = serde_json::from_value(push.params["upsert"].clone()).unwrap_or_default();
                let remove = serde_json::from_value(push.params["remove"].clone()).unwrap_or_default();
                out.push(Update::Folders { upsert, remove, whole: false });
            }
            From::Push(push) if push.method == "roster" => {
                let watch = push.params["watch"].as_str();
                if let Some((folder, Some(w))) = &self.roster
                    && Some(w.as_str()) == watch
                {
                    let upsert = serde_json::from_value(push.params["upsert"].clone()).unwrap_or_default();
                    let remove = serde_json::from_value(push.params["remove"].clone()).unwrap_or_default();
                    out.push(Update::Roster { folder: folder.clone(), upsert, remove, whole: false });
                }
            }
            From::Window { window, failure } if Some(window) == self.list && self.list_folder != "all" => {
                let folder = std::mem::replace(&mut self.list_folder, "all".into());
                self.client.unwatch(window);
                self.list = Some(self.client.watch_chats("all", Place::top(100)));
                out.push(Update::FolderRefused { folder, why: why_not(&failure, "folders") });
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
    fn watch_chats(&mut self, folder: &str, place: Place) {
        match self.list {
            Some(w) if self.list_folder == folder => self.client.move_window(w, place),
            other => {
                if let Some(w) = other {
                    self.client.unwatch(w);
                }
                self.list_folder = folder.to_string();
                self.list = Some(self.client.watch_chats(folder, place));
            }
        }
    }

    fn watch_library(&mut self, ask: &LibraryAsk) {
        self.unwatch_library();
        let mut params = serde_json::to_value(ask).unwrap_or(Value::Null);
        if ask.kind.is_none() {
            params["kind"] = json!("all");
        }
        self.library = Some((ask.clone(), None));
        self.ask("library.watch", params, Asked::Library(ask.id));
    }

    fn unwatch_library(&mut self) {
        if let Some((_, Some(w))) = self.library.take() {
            self.ask("unwatch", json!({ "watch": w }), Asked::Quiet);
        }
    }

    fn watch_folders(&mut self) {
        self.folders = Some(None);
        self.ask("folders.watch", json!({}), Asked::Folders);
    }

    fn watch_roster(&mut self, folder: &str) {
        if let Some((_, Some(w))) = self.roster.take() {
            self.ask("unwatch", json!({ "watch": w }), Asked::Quiet);
        }
        self.roster = Some((folder.to_string(), None));
        let params = json!({ "folder": folder, "around": "top", "before": 0, "after": 200, "offline": true });
        self.ask("roster.watch", params, Asked::Roster(folder.to_string()));
    }

    fn folder_create(&mut self, name: &str, parent: Option<&str>) {
        self.ask("folder.create", json!({ "name": name, "parent": parent }), Asked::FolderMade);
    }

    fn folder_rename(&mut self, folder: &str, name: &str) {
        self.ask("folder.rename", json!({ "folder": folder, "name": name }), Asked::Did("renaming folders"));
    }

    fn folder_move(&mut self, folder: &str, parent: Option<&str>) {
        self.ask("folder.move", json!({ "folder": folder, "parent": parent }), Asked::Did("moving folders"));
    }

    fn folder_delete(&mut self, folder: &str) {
        self.ask("folder.delete", json!({ "folder": folder }), Asked::Did("deleting folders"));
    }

    fn folder_add(&mut self, folder: &str, item: &str) {
        self.ask("folder.add", json!({ "folder": folder, "item": item }), Asked::Did("filing into folders"));
    }

    fn folder_remove(&mut self, folder: &str, item: &str) {
        self.ask("folder.remove", json!({ "folder": folder, "item": item }), Asked::Did("filing into folders"));
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

    fn send_text(&mut self, chat: &str, text: &str, key: &str, reply: Option<&str>, mentions: &[String]) {
        let mut params = json!({ "chat": chat, "text": text, "key": key });
        if let Some(reply) = reply {
            params["reply"] = json!(reply);
        }
        if !mentions.is_empty() {
            params["mentions"] = json!(mentions);
        }
        let sending = Sending {
            chat: chat.to_string(),
            text: text.to_string(),
            key: key.to_string(),
            params: params.clone(),
            first: SystemTime::now(),
        };
        self.ask("send.text", params, Asked::Send(Box::new(sending)));
    }

    fn react(&mut self, message: &str, emoji: &str) {
        self.ask("react", json!({ "message": message, "emoji": emoji }), Asked::Did("reacting"));
    }

    fn edit(&mut self, message: &str, text: &str) {
        self.ask("edit", json!({ "message": message, "text": text }), Asked::Did("editing"));
    }

    fn delete(&mut self, message: &str, everyone: bool) {
        let to = if everyone { "everyone" } else { "me" };
        self.ask("delete", json!({ "message": message, "for": to }), Asked::Did("deleting"));
    }

    fn forward(&mut self, messages: &[String], to: &[String]) {
        self.ask("forward", json!({ "messages": messages, "to": to }), Asked::Forward(to.len()));
    }

    fn message_info(&mut self, message: &str) {
        self.ask("message.info", json!({ "message": message }), Asked::Info(message.to_string()));
    }

    fn chat_info(&mut self, chat: &str) {
        self.ask("chat.info", json!({ "chat": chat }), Asked::Members(chat.to_string()));
    }

    fn draft(&mut self, chat: &str, text: &str) {
        self.owe("draft.set", chat, json!({ "chat": chat, "text": text }));
    }

    fn read(&mut self, chat: &str, up_to: &str) {
        self.owe("read", chat, json!({ "chat": chat, "up_to": up_to }));
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

    fn start_chat_by_phone(&mut self, phone: &str) {
        self.ask("chat.start", json!({ "phone": phone }), Asked::StartChat);
    }

    fn retry(&mut self, message: &str) {
        self.ask("send.retry", json!({ "message": message }), Asked::Retry);
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
