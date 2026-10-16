//! A connection's windows. It keeps, for each window, the hash of every row its interface holds;
//! changes from the feed mark the windows they touch, and a flush reads each marked window again
//! and sends only the rows whose hash changed and the ids that left. What it keeps is bounded by
//! its windows however far behind its interface is, and the lines it sends go into a bounded
//! queue it never waits on: when the queue is full the windows stay marked, and once the queue
//! has had no room for `Timing::behind` they're dropped and the interface is told to watch again.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hasher;
use std::sync::Arc;
use std::time::Instant;

use jaw_proto::windows::{
    Around, ChatRow, ChatsPush, ChatsWatch, ChatsWatched, MessageRow, MessagesPush, MessagesWatch, MessagesWatched,
    Watch, WatchMove,
};
use jaw_proto::{Error, Push, code};
use rusqlite::Connection;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc;

use super::window::{self, ChatAnchor, ChatSpec, MessageAnchor, MessageSpec, Rows, Snapshot, Spec};
use super::{CatchingUp, Changes, Timing};
use crate::store::{Store, read};
use crate::work::Priority;

/// The most rows a window keeps on either side of its anchor.
const MOST_ROWS: u32 = 1_000;

/// What a client waits for.
pub enum Ready {
    Changes(Result<Arc<Changes>, RecvError>),
    Flush,
    /// The queue to the interface has room again.
    Room,
    /// The queue has had no room for too long.
    Behind,
}

pub struct Client {
    store: Store,
    changes: Option<broadcast::Receiver<Arc<Changes>>>,
    catching_up: CatchingUp,
    timing: Timing,
    out: mpsc::Sender<Vec<u8>>,
    /// The seq hello said the interface last saw, for the watches it sends again.
    since: Option<u64>,
    windows: BTreeMap<String, Window>,
    next: u64,
    /// When the first and the latest changes waiting to be sent came.
    first: Option<Instant>,
    last: Option<Instant>,
    /// When the last batch went, so the next waits at least `Timing::burst`.
    flushed: Option<Instant>,
    /// Since when a flush has waited for room in the queue.
    blocked: Option<Instant>,
    /// Watches dropped for falling behind, owed a `resync`.
    resync: Vec<String>,
}

struct Window {
    spec: Spec,
    state: State,
    dirty: bool,
    /// When a chat list last took its order from a fresh query, and whether it owes one.
    reordered: Instant,
    reorder: bool,
}

/// What a window's interface holds, and what jawd needs to know which changes touch it.
#[derive(Default)]
struct State {
    held: HashMap<String, Held>,
    total: u64,
    older: bool,
    newer: u64,
    /// A messages window's chat, by n, once it exists.
    chat: Option<i64>,
    senders: HashSet<i64>,
}

/// A row as its interface holds it.
struct Held {
    /// Of the row as sent; 0 for a row the interface says it has, which nothing matches.
    hash: u64,
    sort: String,
    n: i64,
}

/// What an interface needs to go from what it holds to a snapshot.
struct Update {
    seq: u64,
    total: u64,
    older: bool,
    newer: u64,
    upsert: Upsert,
    remove: Vec<String>,
}

enum Upsert {
    Chats(Vec<ChatRow>),
    Messages(Vec<MessageRow>),
}

trait Row: Serialize {
    fn id(&self) -> &str;
    fn sort(&self) -> &str;
    fn v(&self) -> u64;
}

impl Row for ChatRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.sort
    }
    fn v(&self) -> u64 {
        self.v
    }
}

impl Row for MessageRow {
    fn id(&self) -> &str {
        &self.id
    }
    fn sort(&self) -> &str {
        &self.sort
    }
    fn v(&self) -> u64 {
        self.v
    }
}

fn hash(row: &impl Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(&serde_json::to_vec(row).expect("jawd's own types serialize"));
    hasher.finish()
}

/// The rows of `rows` an interface holding `held` lacks or holds otherwise, leaving out those it
/// holds that haven't changed since `since`; the ids it holds that aren't in `rows`; and what it
/// holds afterwards.
fn changed<R: Row>(
    held: &HashMap<String, Held>,
    rows: Vec<(i64, R)>,
    since: Option<i64>,
) -> (Vec<R>, Vec<String>, HashMap<String, Held>) {
    let mut now = HashMap::with_capacity(rows.len());
    let mut upsert = Vec::new();
    for (n, row) in rows {
        let hash = hash(&row);
        let same = held
            .get(row.id())
            .is_some_and(|held| held.hash == hash || since.is_some_and(|since| row.v() as i64 <= since));
        now.insert(row.id().to_string(), Held { hash, sort: row.sort().to_string(), n });
        if !same {
            upsert.push(row);
        }
    }
    let mut remove: Vec<String> = held.keys().filter(|id| !now.contains_key(*id)).cloned().collect();
    remove.sort();
    (upsert, remove, now)
}

/// What an interface holding `held` needs to hold `snapshot`, and what it holds then.
fn settle(held: &HashMap<String, Held>, snapshot: Snapshot, since: Option<i64>) -> (Update, State) {
    let (upsert, remove, held) = match snapshot.rows {
        Rows::Chats(rows) => {
            let (rows, remove, held) = changed(held, rows, since);
            (Upsert::Chats(rows), remove, held)
        }
        Rows::Messages(rows) => {
            let (rows, remove, held) = changed(held, rows, since);
            (Upsert::Messages(rows), remove, held)
        }
    };
    let update = Update {
        seq: snapshot.seq.max(0) as u64,
        total: snapshot.total,
        older: snapshot.older,
        newer: snapshot.newer,
        upsert,
        remove,
    };
    let state = State {
        held,
        total: snapshot.total,
        older: snapshot.older,
        newer: snapshot.newer,
        chat: snapshot.chat,
        senders: snapshot.senders,
    };
    (update, state)
}

/// `update` as a watch's or a move's result.
fn result(watch: String, update: Update) -> Value {
    let Update { seq, total, older, newer, upsert, remove } = update;
    let value = match upsert {
        Upsert::Chats(rows) => serde_json::to_value(ChatsWatched { watch, seq, total, rows, remove }),
        Upsert::Messages(rows) => {
            serde_json::to_value(MessagesWatched { watch, seq, total, older, newer, rows, remove })
        }
    };
    value.expect("jawd's own types serialize")
}

/// `update` as a push to an interface holding `was`, unless nothing in it changed.
fn push(watch: String, update: Update, was: &State) -> Option<Vec<u8>> {
    let Update { seq, total, older, newer, upsert, remove } = update;
    let total = (total != was.total).then_some(total);
    match upsert {
        Upsert::Chats(upsert) => {
            if upsert.is_empty() && remove.is_empty() && total.is_none() {
                return None;
            }
            Some(line("chats", ChatsPush { watch, seq, total, upsert, remove }))
        }
        Upsert::Messages(upsert) => {
            let older = (older != was.older).then_some(older);
            let newer = (newer != was.newer).then_some(newer);
            if upsert.is_empty() && remove.is_empty() && total.is_none() && older.is_none() && newer.is_none() {
                return None;
            }
            Some(line("messages", MessagesPush { watch, seq, total, older, newer, upsert, remove }))
        }
    }
}

fn line(method: &str, params: impl Serialize) -> Vec<u8> {
    let params = serde_json::to_value(params).expect("jawd's own types serialize");
    let mut line = serde_json::to_vec(&Push { method: method.into(), params }).expect("jawd's own types serialize");
    line.push(b'\n');
    line
}

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn sides(before: u32, after: u32) -> Result<(usize, usize), Error> {
    if before > MOST_ROWS || after > MOST_ROWS {
        return Err(Error::new(
            code::INVALID_PARAMS,
            format!("a window keeps at most {MOST_ROWS} rows on each side of its anchor"),
        ));
    }
    Ok((before as usize, after as usize))
}

fn archived(folder: &str) -> Result<bool, Error> {
    match folder {
        "all" => Ok(false),
        "archived" => Ok(true),
        _ => Err(Error::new(
            code::INVALID_PARAMS,
            format!("jawd can watch the all and archived folders so far, not {folder}"),
        )),
    }
}

fn unknown(watch: &str) -> Error {
    Error::new(code::NOT_FOUND, format!("this connection has no watch {watch}"))
}

async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// Waits for room in the queue; a queue whose interface has gone never has any.
async fn room(out: Option<mpsc::Sender<Vec<u8>>>) {
    if let Some(out) = out
        && out.reserve_owned().await.is_ok()
    {
        return;
    }
    std::future::pending().await
}

async fn recv(changes: &mut Option<broadcast::Receiver<Arc<Changes>>>) -> Result<Arc<Changes>, RecvError> {
    match changes {
        Some(changes) => changes.recv().await,
        None => std::future::pending().await,
    }
}

impl Window {
    fn touched_by(&self, changes: &Changes) -> bool {
        match (&self.spec, self.state.chat) {
            (Spec::Chats(_), _) => !changes.chats.is_empty() || !changes.people.is_empty(),
            (Spec::Messages(_), Some(chat)) => {
                changes.messages.contains(&chat)
                    || changes.chats.contains(&chat)
                    || self.state.senders.iter().any(|sender| changes.people.contains(sender))
            }
            (Spec::Messages(_), None) => !changes.chats.is_empty(),
        }
    }

    /// Whether it may take a new order now: always, except a chat list while catching up, which
    /// reorders once each `hold`.
    fn may_reorder(&self, now: Instant, holding: bool, timing: &Timing) -> bool {
        !holding || !matches!(self.spec, Spec::Chats(_)) || now >= self.reordered + timing.hold
    }

    /// When it owes a reorder it may take.
    fn reorder_at(&self, holding: bool, timing: &Timing) -> Option<Instant> {
        match self.reorder {
            true if holding => Some(self.reordered + timing.hold),
            true => Some(Instant::now()),
            false => None,
        }
    }

    /// Takes what its interface now holds; `reordered` says when, if it took a new order.
    fn keep(&mut self, state: State, reordered: Option<Instant>) {
        self.state = state;
        self.dirty = false;
        match reordered {
            Some(at) => {
                self.reordered = at;
                self.reorder = false;
            }
            None => self.reorder = true,
        }
    }
}

impl Client {
    pub(super) fn new(
        store: Store,
        changes: broadcast::Receiver<Arc<Changes>>,
        catching_up: CatchingUp,
        timing: Timing,
        out: mpsc::Sender<Vec<u8>>,
    ) -> Client {
        Client {
            store,
            changes: Some(changes),
            catching_up,
            timing,
            out,
            since: None,
            windows: BTreeMap::new(),
            next: 1,
            first: None,
            last: None,
            flushed: None,
            blocked: None,
            resync: Vec::new(),
        }
    }

    /// Takes hello's `since`, the default for the watches this connection sends again.
    pub fn hello(&mut self, since: Option<u64>) {
        self.since = since;
    }

    /// Answers `chats.watch`, `messages.watch`, `watch.move` or `unwatch`.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        match method {
            "chats.watch" => {
                let watch: ChatsWatch = params_of(method, params)?;
                let archived = archived(&watch.folder)?;
                let spec = self.chat_spec(archived, watch.around, watch.before, watch.after).await?;
                self.watch(Spec::Chats(spec), watch.since, watch.have).await
            }
            "messages.watch" => {
                let watch: MessagesWatch = params_of(method, params)?;
                let spec = self.message_spec(watch.chat, watch.around, watch.before, watch.after).await?;
                self.watch(Spec::Messages(spec), watch.since, watch.have).await
            }
            "watch.move" => {
                let moving: WatchMove = params_of(method, params)?;
                let spec = match self.windows.get(&moving.watch).map(|window| &window.spec) {
                    Some(Spec::Chats(chats)) => {
                        let archived = chats.archived;
                        Spec::Chats(self.chat_spec(archived, moving.around, moving.before, moving.after).await?)
                    }
                    Some(Spec::Messages(messages)) => {
                        let chat = messages.chat.clone();
                        Spec::Messages(self.message_spec(chat, moving.around, moving.before, moving.after).await?)
                    }
                    None => return Err(unknown(&moving.watch)),
                };
                self.relocate(moving.watch, spec).await
            }
            "unwatch" => {
                let Watch { watch } = params_of(method, params)?;
                self.windows.remove(&watch).ok_or_else(|| unknown(&watch))?;
                Ok(Value::Object(Default::default()))
            }
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        }
    }

    async fn read<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, Error> {
        self.store
            .reads()
            .query(Priority::Interactive, query)
            .await
            .map_err(|e| Error::new(code::INTERNAL, format!("jawd couldn't read the window: {e:#}")))
    }

    async fn chat_spec(&self, archived: bool, around: Around, before: u32, after: u32) -> Result<ChatSpec, Error> {
        let (before, after) = sides(before, after)?;
        let anchor = match around {
            Around::Top => None,
            Around::Row(id) => {
                let chat = id.clone();
                match self.read(move |db| read::chat_place(db, &chat)).await? {
                    Some((_, cursor, _)) => Some(ChatAnchor { id, cursor }),
                    None => return Err(Error::new(code::NOT_FOUND, format!("jawd has no chat {id}"))),
                }
            }
            Around::Newest | Around::Unread => {
                return Err(Error::new(code::INVALID_PARAMS, "a chat list is anchored at top or on a chat's id"));
            }
        };
        Ok(ChatSpec { archived, anchor, before, after })
    }

    async fn message_spec(&self, chat: String, around: Around, before: u32, after: u32) -> Result<MessageSpec, Error> {
        let (before, after) = sides(before, after)?;
        let anchor = match around {
            Around::Newest => None,
            Around::Unread => {
                let of = chat.clone();
                self.read(move |db| read::first_unread(db, &of)).await?.map(|(id, cursor)| MessageAnchor { id, cursor })
            }
            Around::Row(id) => {
                let (of, message) = (chat.clone(), id.clone());
                match self.read(move |db| read::message_place(db, &of, &message)).await? {
                    Some(cursor) => Some(MessageAnchor { id, cursor }),
                    None => return Err(Error::new(code::NOT_FOUND, format!("jawd has no message {id} in {chat}"))),
                }
            }
            Around::Top => {
                return Err(Error::new(
                    code::INVALID_PARAMS,
                    "a chat's messages are anchored at newest, unread or a message's id",
                ));
            }
        };
        Ok(MessageSpec { chat, anchor, before, after })
    }

    async fn snapshot(&self, spec: Spec, only: Option<Vec<i64>>) -> Result<(Snapshot, Spec), Error> {
        self.read(move |db| window::query(db, spec, only)).await
    }

    async fn watch(&mut self, spec: Spec, since: Option<u64>, have: Vec<String>) -> Result<Value, Error> {
        let (snapshot, spec) = self.snapshot(spec, None).await?;
        // Rows' versions are seqs, so a since past the feed's last is from a jaw.db since replaced.
        let since = since
            .or(self.since)
            .and_then(|since| i64::try_from(since).ok())
            .filter(|&since| since >= snapshot.oldest - 1 && since <= snapshot.seq);
        let have = have.into_iter().map(|id| (id, Held { hash: 0, sort: String::new(), n: 0 })).collect();
        let (update, state) = settle(&have, snapshot, since);
        let watch = format!("w{}", self.next);
        self.next += 1;
        let mut window =
            Window { spec, state: State::default(), dirty: false, reordered: Instant::now(), reorder: false };
        window.keep(state, Some(Instant::now()));
        self.windows.insert(watch.clone(), window);
        Ok(result(watch, update))
    }

    async fn relocate(&mut self, watch: String, spec: Spec) -> Result<Value, Error> {
        let (snapshot, spec) = self.snapshot(spec, None).await?;
        let window = self.windows.get_mut(&watch).ok_or_else(|| unknown(&watch))?;
        window.spec = spec;
        let (update, state) = settle(&window.state.held, snapshot, None);
        window.keep(state, Some(Instant::now()));
        Ok(result(watch, update))
    }

    /// Waits for a change from the feed, a flush falling due, room for a blocked flush, or a
    /// blocked one waiting too long. Cancelling it loses nothing.
    pub async fn ready(&mut self) -> Ready {
        let flush_at = if self.blocked.is_none() { self.flush_at() } else { None };
        let behind_at = self.blocked.filter(|_| !self.windows.is_empty()).map(|since| since + self.timing.behind);
        let out = self.blocked.map(|_| self.out.clone());
        tokio::select! {
            biased;
            _ = until(flush_at) => Ready::Flush,
            _ = room(out) => Ready::Room,
            _ = until(behind_at) => Ready::Behind,
            changes = recv(&mut self.changes) => Ready::Changes(changes),
        }
    }

    pub async fn handle(&mut self, ready: Ready) {
        match ready {
            Ready::Changes(Ok(changes)) => self.mark(&changes),
            Ready::Changes(Err(RecvError::Lagged(_))) => {
                for window in self.windows.values_mut() {
                    window.dirty = true;
                }
                self.waiting(Instant::now());
            }
            Ready::Changes(Err(RecvError::Closed)) => self.changes = None,
            Ready::Flush | Ready::Room => self.flush().await,
            Ready::Behind => {
                if self.blocked.is_some_and(|since| since.elapsed() >= self.timing.behind) {
                    self.resync.extend(std::mem::take(&mut self.windows).into_keys());
                    self.first = None;
                    self.last = None;
                }
            }
        }
    }

    fn mark(&mut self, changes: &Changes) {
        let mut touched = false;
        for window in self.windows.values_mut() {
            if window.touched_by(changes) {
                window.dirty = true;
                touched = true;
            }
        }
        if touched {
            self.waiting(changes.at);
        }
    }

    /// Notes a change waiting since `at`.
    fn waiting(&mut self, at: Instant) {
        self.first = Some(self.first.map_or(at, |first| first.min(at)));
        self.last = Some(self.last.map_or(at, |last| last.max(at)));
    }

    fn flush_at(&self) -> Option<Instant> {
        if !self.resync.is_empty() {
            return Some(Instant::now());
        }
        let holding = (self.catching_up)();
        let mut at = None;
        if let (Some(first), Some(last)) = (self.first, self.last)
            && self.windows.values().any(|window| window.dirty)
        {
            let due = (last + self.timing.quiet).min(first + self.timing.burst);
            at = Some(self.flushed.map_or(due, |flushed| due.max(flushed + self.timing.burst)));
        }
        for window in self.windows.values() {
            if let Some(reorder) = window.reorder_at(holding, &self.timing) {
                at = Some(at.map_or(reorder, |at: Instant| at.min(reorder)));
            }
        }
        at
    }

    /// Sends what each due window's interface lacks, until the queue is full.
    async fn flush(&mut self) {
        while let Some(watch) = self.resync.first() {
            if self.out.try_send(line("resync", Watch { watch: watch.clone() })).is_err() {
                self.blocked.get_or_insert_with(Instant::now);
                return;
            }
            self.resync.remove(0);
        }
        let holding = (self.catching_up)();
        let now = Instant::now();
        self.flushed = Some(now);
        let due: Vec<String> = self
            .windows
            .iter()
            .filter(|(_, window)| window.dirty || (window.reorder && window.may_reorder(now, holding, &self.timing)))
            .map(|(watch, _)| watch.clone())
            .collect();
        for watch in due {
            if self.out.capacity() == 0 {
                self.blocked.get_or_insert_with(Instant::now);
                return;
            }
            let Some(window) = self.windows.get(&watch) else { continue };
            // A chat list held while catching up reads its chats where they are and keeps their order.
            let only = (!window.may_reorder(now, holding, &self.timing))
                .then(|| window.state.held.values().map(|held| held.n).collect());
            let reordered = only.is_none().then_some(now);
            let snapshot = self.snapshot(window.spec.clone(), only).await;
            let Some(window) = self.windows.get_mut(&watch) else { continue };
            let (mut snapshot, spec) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    eprintln!("jawd: can't bring {watch} up to date: {}", e.message);
                    window.dirty = false;
                    continue;
                }
            };
            window.spec = spec;
            if reordered.is_none()
                && let Rows::Chats(rows) = &mut snapshot.rows
            {
                for (_, row) in rows.iter_mut() {
                    if let Some(held) = window.state.held.get(&row.id) {
                        row.sort.clone_from(&held.sort);
                    }
                }
            }
            let (update, state) = settle(&window.state.held, snapshot, None);
            match push(watch, update, &window.state) {
                None => window.keep(state, reordered),
                Some(line) => match self.out.try_send(line) {
                    Ok(()) => window.keep(state, reordered),
                    Err(_) => {
                        self.blocked.get_or_insert_with(Instant::now);
                        return;
                    }
                },
            }
        }
        self.first = None;
        self.last = None;
        self.blocked = None;
    }

    /// Whether it has updates or resyncs it hasn't sent.
    #[cfg(test)]
    pub fn pending(&self) -> bool {
        !self.resync.is_empty() || self.windows.values().any(|window| window.dirty || window.reorder)
    }
}
