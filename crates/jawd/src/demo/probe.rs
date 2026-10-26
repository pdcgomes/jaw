//! The demo's own interface on the socket, which measures jawd as an interface sees it: a watcher
//! on the chat list and the open chat's newest messages, noting when each message it was told to
//! expect reaches it; and an asker that opens chats, reads pages at any depth, finds messages and
//! opens and pages the library, timing each answer. The watcher waits on its socket without a timeout, so an idle jawd stays
//! idle.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::usage;

/// The chat list window's rows, as jaw watches it.
const CHAT_ROWS: u32 = 60;
/// A page of messages.
pub const PAGE: u32 = 100;

/// What the watcher was told to expect, and what reached it.
#[derive(Default)]
pub struct Expected {
    /// Messages on their way, by ID, with when the engine handed each over.
    waiting: Mutex<HashMap<String, Instant>>,
    /// Each that came: from its hand-over, the row's version, and when it came.
    came: Mutex<Vec<(Duration, i64, Instant)>>,
    pushes: AtomicU64,
    lists: AtomicU64,
}

impl Expected {
    pub fn expect(&self, id: String, handed: Instant) {
        self.waiting.lock().expect("expected messages").insert(id, handed);
    }

    /// What came since the last take, and how many expected never did, forgetting both.
    pub fn take(&self) -> (Vec<(Duration, i64, Instant)>, usize) {
        let came = std::mem::take(&mut *self.came.lock().expect("arrived messages"));
        let missing = std::mem::take(&mut *self.waiting.lock().expect("expected messages")).len();
        (came, missing)
    }

    /// Pushes of any kind so far, and the chat list's.
    pub fn pushes(&self) -> (u64, u64) {
        (self.pushes.load(Ordering::Relaxed), self.lists.load(Ordering::Relaxed))
    }
}

/// A connection that says hello and asks and waits, dropping pushes.
struct Line {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
}

impl Line {
    fn open(socket: &Path) -> Result<Line> {
        let stream = UnixStream::connect(socket).with_context(|| format!("can't reach {}", socket.display()))?;
        let mut line = Line { reader: BufReader::new(stream.try_clone()?), writer: stream, next: 1 };
        // In front, as jaw is while it shows a chat, so WhatsApp is told you're online.
        let hello = json!({"protocol": jaw_proto::PROTOCOL, "client": "jawd demo", "version": jaw_version::VERSION, "focus": true});
        line.call("hello", hello)?;
        Ok(line)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        self.writer.write_all(format!("{}\n", json!({"id": id, "method": method, "params": params})).as_bytes())?;
        let mut text = String::new();
        loop {
            text.clear();
            if self.reader.read_line(&mut text)? == 0 {
                bail!("jawd closed the connection");
            }
            let line: Value = serde_json::from_str(&text)?;
            if line["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(error) = line.get("error") {
                bail!("jawd refused {method}: {error}");
            }
            return Ok(line["result"].clone());
        }
    }
}

/// The watcher: its socket, to end it, and its thread, which says the CPU it spent.
pub struct Watcher {
    stream: UnixStream,
    thread: std::thread::JoinHandle<Duration>,
}

impl Watcher {
    /// Watches the chat list's top and `chat`'s newest messages, as jaw does when it opens.
    pub fn start(socket: &Path, chat: &str, expected: Arc<Expected>) -> Result<Watcher> {
        let mut line = Line::open(socket)?;
        line.call("chats.watch", json!({"folder": "all", "around": "top", "after": CHAT_ROWS}))?;
        line.call("messages.watch", json!({"chat": chat, "around": "newest", "before": PAGE}))?;
        let stream = line.writer.try_clone()?;
        let thread = std::thread::Builder::new().name("jawd-demo-watcher".into()).spawn(move || {
            let cpu = usage::thread_cpu();
            let mut text = String::new();
            while matches!(line.reader.read_line(&mut text), Ok(n) if n > 0) {
                let read = Instant::now();
                if let Ok(push) = serde_json::from_str::<Value>(&text) {
                    seen(&push, read, &expected);
                }
                text.clear();
            }
            usage::thread_cpu() - cpu
        })?;
        Ok(Watcher { stream, thread })
    }

    /// Ends the watcher, and says the CPU its thread spent.
    pub fn stop(self) -> Duration {
        let _ = self.stream.shutdown(Shutdown::Both);
        self.thread.join().unwrap_or_default()
    }
}

fn seen(push: &Value, read: Instant, expected: &Expected) {
    expected.pushes.fetch_add(1, Ordering::Relaxed);
    match push["method"].as_str() {
        Some("chats") => {
            expected.lists.fetch_add(1, Ordering::Relaxed);
        }
        Some("messages") => {
            let Some(rows) = push["params"]["upsert"].as_array() else { return };
            let mut waiting = expected.waiting.lock().expect("expected messages");
            let came: Vec<_> = rows
                .iter()
                .filter_map(|row| {
                    let handed = waiting.remove(row["id"].as_str()?)?;
                    Some((read - handed, row["v"].as_i64().unwrap_or_default(), read))
                })
                .collect();
            drop(waiting);
            expected.came.lock().expect("arrived messages").extend(came);
        }
        _ => {}
    }
}

/// The asker: chats opened, pages read and searches made, each timed from asking to its answer.
pub struct Asker {
    line: Line,
}

impl Asker {
    pub fn open(socket: &Path) -> Result<Asker> {
        Ok(Asker { line: Line::open(socket)? })
    }

    /// Opens `chat` at its newest page, as an interface does, and lets it go again; a chat whose
    /// history hasn't come yet opens empty.
    pub fn open_chat(&mut self, chat: &str) -> Result<Duration> {
        let (took, _) = self.page(json!({"chat": chat, "around": "newest", "before": PAGE}))?;
        Ok(took)
    }

    /// The page of messages before `id` in `chat`, wherever it is.
    pub fn page_before(&mut self, chat: &str, id: &str) -> Result<Duration> {
        let (took, rows) = self.page(json!({"chat": chat, "around": id, "before": PAGE, "after": 0}))?;
        if rows == 0 {
            bail!("the page before {id} in {chat} has nothing in it");
        }
        Ok(took)
    }

    fn page(&mut self, params: Value) -> Result<(Duration, usize)> {
        let asked = Instant::now();
        let window = self.line.call("messages.watch", params)?;
        let took = asked.elapsed();
        self.line.call("unwatch", json!({"watch": window["watch"]}))?;
        Ok((took, window["rows"].as_array().map_or(0, Vec::len)))
    }

    /// The library window `params` asks for, watched and let go as jaw's library does, with what
    /// it answered.
    pub fn library(&mut self, params: Value) -> Result<(Duration, Value)> {
        let asked = Instant::now();
        let window = self.line.call("library.watch", params)?;
        let took = asked.elapsed();
        self.line.call("unwatch", json!({"watch": window["watch"]}))?;
        Ok((took, window))
    }

    /// Names the library's items on screen, as jaw does when what it draws changes.
    pub fn shown(&mut self, items: &[&str]) -> Result<()> {
        self.line.call("library.shown", json!({"items": items}))?;
        Ok(())
    }

    /// The palette's find for `query`, messages only, as jaw asks for them; says how many matched,
    /// up to the 1,000 find counts.
    pub fn find(&mut self, query: &str) -> Result<(Duration, u64)> {
        let asked = Instant::now();
        let found = self.line.call("find", json!({"query": query, "kinds": ["message"]}))?;
        Ok((asked.elapsed(), found["counts"]["message"].as_u64().unwrap_or_default()))
    }
}
