//! Notifications for new messages, from jawd as ooze's come from `ooze watch`, so they arrive with
//! no window open. A task follows the store's committed events and gathers the chats they touch;
//! once the feed has been quiet for a moment, it reads those chats and notifies for each with a
//! new message from someone else since jawd started. A burst in a chat is one notification; what
//! comes in that chat soon after waits out a hold and goes as one more; and more chats than a few
//! at once are one summary. Nothing goes while catching up: when a catch-up ends, the chats whose
//! newest message it brought are one notification, however long ago those were sent, so what
//! came while jawd was off is one summary once it has caught up after starting. Muted and archived
//! chats, your own messages, history and a chat an interface in front is reading stay quiet.

mod desktop;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use jaw_proto::Push;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use crate::attention::Attention;
use crate::feed::CatchingUp;
use crate::store::Store;
use crate::store::read::{self, Around, ChatRow, Last};
use crate::work::Priority;

/// One notification: the chat, how many are unread when there's more than one, and the newest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub subtitle: Option<String>,
    pub body: String,
}

pub trait Notifier: Send + Sync {
    /// Where notifications go, as `status` says it.
    fn name(&self) -> &str;
    /// Shows `notice`. It can take a while, so it's called on a blocking thread.
    fn notify(&self, notice: &Notice) -> Result<()>;
}

/// The desktop's notifier, unless `off` or `JAW_NOTIFY=off` turns notifications off; otherwise
/// why there's none, as `status` says it.
pub fn chosen(off: bool) -> Result<Box<dyn Notifier>, String> {
    if off {
        return Err("off, as --no-notify asks".into());
    }
    if let Some(value) = std::env::var_os("JAW_NOTIFY")
        && matches!(value.to_string_lossy().to_lowercase().as_str(), "off" | "0" | "no" | "false")
    {
        return Err("off, as JAW_NOTIFY asks".into());
    }
    desktop::find()
}

/// How notifications are paced.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// How long the feed is quiet before the chats it touched are looked at.
    pub quiet: Duration,
    /// The longest a change waits during a burst.
    pub most: Duration,
    /// How long after a chat's notification the next for it waits, gathering what comes meanwhile.
    pub hold: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { quiet: Duration::from_millis(300), most: Duration::from_secs(1), hold: Duration::from_secs(10) }
    }
}

/// More chats than this at once are one summary.
const AT_ONCE: usize = 3;
/// The most characters of a message a notification shows.
const PREVIEW: usize = 120;
/// The most events one read takes.
const EVENTS_AT_ONCE: usize = 4096;

/// Starts notifying through `to` for what's committed from now on. `pushes` wakes it when a
/// catch-up ends.
pub fn start(
    store: Store,
    attention: Arc<Attention>,
    catching_up: CatchingUp,
    pushes: broadcast::Receiver<Push>,
    to: Arc<dyn Notifier>,
    timing: Timing,
) -> JoinHandle<()> {
    let committed = store.committed();
    let follow = Follow {
        store,
        attention,
        catching_up,
        to,
        timing,
        since: now_ms(),
        seen: 0,
        changed: None,
        pending: HashMap::new(),
        held: HashMap::new(),
        handled: HashMap::new(),
        caught: None,
    };
    tokio::spawn(follow.run(committed, pushes))
}

struct Follow {
    store: Store,
    attention: Arc<Attention>,
    catching_up: CatchingUp,
    to: Arc<dyn Notifier>,
    timing: Timing,
    /// When jawd started; older messages stay quiet, unless they come in a catch-up.
    since: i64,
    /// The feed's seq taken in.
    seen: i64,
    /// When the first and the latest commit not taken in yet came.
    changed: Option<(Instant, Instant)>,
    /// Chats to look at, by n, each with when it may be.
    pending: HashMap<i64, Instant>,
    /// Chats just notified for, by n, until their hold ends.
    held: HashMap<i64, Instant>,
    /// Each chat's newest message looked at, by its time, so nothing is notified twice.
    handled: HashMap<i64, i64>,
    /// What's come during a catch-up, while one is under way or settling.
    caught: Option<Caught>,
}

/// The messages a catch-up brought, for the one notification when it ends.
#[derive(Default)]
struct Caught {
    /// Each chat's messages written meanwhile, by n.
    chats: HashMap<i64, HashSet<i64>>,
    /// When it ended, if it has; what's committed for a moment after still belongs to it, since
    /// its last messages are converted after WhatsApp says it's done.
    ended: Option<Instant>,
}

impl Follow {
    async fn run(mut self, mut committed: watch::Receiver<i64>, pushes: broadcast::Receiver<Push>) {
        self.seen = *committed.borrow_and_update();
        let mut pushes = Some(pushes);
        loop {
            self.observe();
            let wake = self.wake();
            tokio::select! {
                changed = committed.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let now = Instant::now();
                    let first = self.changed.map_or(now, |(first, _)| first);
                    self.changed = Some((first, now));
                }
                // A `connection` push comes when a catch-up starts and ends. One that says it
                // started counts even when it's read after it ended, so no catch-up goes unseen.
                push = next(&mut pushes) => match push {
                    Ok(push) if push.method == "connection" && push.params["state"] == "catching_up" => {
                        self.caught.get_or_insert_default();
                    }
                    Err(broadcast::error::RecvError::Closed) => pushes = None,
                    _ => {}
                },
                _ = sleep_until(wake) => {
                    let upto = *committed.borrow();
                    self.flush(upto).await;
                }
            }
        }
    }

    /// Notes a catch-up starting, or ending.
    fn observe(&mut self) {
        match ((self.catching_up)(), self.caught.as_mut()) {
            (true, None) => self.caught = Some(Caught::default()),
            (true, Some(caught)) => caught.ended = None,
            (false, Some(caught)) if caught.ended.is_none() => caught.ended = Some(Instant::now()),
            _ => {}
        }
    }

    /// When there's next something to do, if there is.
    fn wake(&self) -> Option<Instant> {
        let changed = self.changed.map(|(first, last)| (first + self.timing.most).min(last + self.timing.quiet));
        let ready = match (self.catching_up)() {
            true => None,
            false => self.pending.values().min().copied(),
        };
        let caught = self.caught.as_ref().and_then(|caught| caught.ended).map(|ended| ended + self.timing.most);
        [changed, ready, caught].into_iter().flatten().min()
    }

    async fn flush(&mut self, upto: i64) {
        if self.changed.take().is_some() {
            self.take_in(upto).await;
        }
        let now = Instant::now();
        if let Some(caught) = self.caught.take_if(|c| c.ended.is_some_and(|ended| now >= ended + self.timing.most)) {
            self.caught_up(caught).await;
        }
        if (self.catching_up)() {
            return;
        }
        self.held.retain(|_, until| *until > now);
        let ready: Vec<i64> = self.pending.iter().filter(|(_, at)| **at <= now).map(|(&n, _)| n).collect();
        if ready.is_empty() {
            return;
        }
        for n in &ready {
            self.pending.remove(n);
        }
        let rows = match self.store.reads().query(Priority::Background, move |db| read::chats_by_n(db, &ready)).await {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("jawd: can't read the chats to notify about: {e:#}");
                return;
            }
        };
        let at = now_ms();
        let mut new: Vec<ChatRow> = rows.into_iter().filter(|row| self.new_message(row, at)).collect();
        newest_first(&mut new);
        for row in &new {
            self.held.insert(row.n, now + self.timing.hold);
        }
        if new.len() > AT_ONCE {
            self.deliver(summary(&new));
        } else {
            for row in &new {
                self.deliver(notice(row));
            }
        }
    }

    /// Reads the events after what's been taken in, up to `upto`: the chats they touch wait to be
    /// looked at, or, while a catch-up is under way or settling, their messages wait for its end.
    async fn take_in(&mut self, upto: i64) {
        while self.seen < upto {
            let since = self.seen;
            let events =
                self.store.reads().query(Priority::Background, move |db| read::events(db, since, EVENTS_AT_ONCE));
            let events = match events.await {
                Ok(events) => events,
                Err(e) => {
                    eprintln!("jawd: notifications can't read what changed: {e:#}");
                    return;
                }
            };
            let Some(newest) = events.last() else { return };
            self.seen = newest.seq;
            let now = Instant::now();
            for event in &events {
                match (event.kind.as_str(), event.chat, self.caught.as_mut()) {
                    ("message", Some(chat), Some(catching)) => {
                        catching.chats.entry(chat).or_default().insert(event.key);
                    }
                    ("chat", _, None) => {
                        let ready = self.held.get(&event.key).copied().filter(|until| *until > now).unwrap_or(now);
                        self.pending.entry(event.key).or_insert(ready);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The one notification for a catch-up: for the chats whose newest message it brought, that
    /// chat's notification if there's one, or a summary of them all, however long ago they were
    /// sent.
    async fn caught_up(&mut self, caught: Caught) {
        if caught.chats.is_empty() {
            return;
        }
        let chats: Vec<i64> = caught.chats.keys().copied().collect();
        let rows = self.store.reads().query(Priority::Background, move |db| {
            let mut rows = Vec::new();
            for row in read::chats_by_n(db, &chats)? {
                let newest = read::messages(db, &row.id, Around::Newest, 1)?.pop().map(|message| message.n);
                rows.push((row, newest));
            }
            Ok(rows)
        });
        let rows = match rows.await {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("jawd: can't read the chats a catch-up brought messages to: {e:#}");
                return;
            }
        };
        let (at, now) = (now_ms(), Instant::now());
        let mut new = Vec::new();
        let mut messages = 0;
        for (row, newest) in rows {
            let Some(last) = &row.last else { continue };
            self.handled.insert(row.n, last.at);
            let brought = &caught.chats[&row.n];
            if newest.is_some_and(|n| brought.contains(&n)) && !self.quiet(&row, at) {
                messages += (row.unread.max(0) as usize).min(brought.len());
                self.held.insert(row.n, now + self.timing.hold);
                new.push(row);
            }
        }
        newest_first(&mut new);
        match new.as_slice() {
            [] => {}
            [row] => self.deliver(notice(row)),
            rows => self.deliver(caught_summary(rows, messages)),
        }
    }

    /// Whether `row`'s newest message is one to notify about: new since it was last looked at and
    /// since jawd started, and not kept quiet.
    fn new_message(&mut self, row: &ChatRow, now: i64) -> bool {
        let Some(last) = &row.last else { return false };
        if last.at < self.since || self.handled.get(&row.n).is_some_and(|&handled| last.at <= handled) {
            return false;
        }
        self.handled.insert(row.n, last.at);
        !self.quiet(row, now)
    }

    /// Whether a chat stays quiet: its newest message is yours or read, or it's muted, archived, a
    /// kind that doesn't notify, or being read in front.
    fn quiet(&self, row: &ChatRow, now: i64) -> bool {
        row.last.as_ref().is_none_or(|last| last.mine)
            || row.unread <= 0
            || row.archived
            || row.muted_until.is_some_and(|until| until > now)
            || !matches!(row.kind.as_str(), "direct" | "group" | "announcement")
            || self.attention.reading(&row.id)
    }

    fn deliver(&self, notice: Notice) {
        let to = self.to.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = to.notify(&notice) {
                eprintln!("jawd: a notification didn't show: {e:#}");
            }
        });
    }
}

fn newest_first(rows: &mut [ChatRow]) {
    rows.sort_by_key(|row| std::cmp::Reverse(row.last.as_ref().map_or(0, |last| last.at)));
}

async fn next(pushes: &mut Option<broadcast::Receiver<Push>>) -> Result<Push, broadcast::error::RecvError> {
    match pushes {
        Some(pushes) => pushes.recv().await,
        None => std::future::pending().await,
    }
}

async fn sleep_until(wake: Option<Instant>) {
    match wake {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

/// A chat's notification: its name, how many are unread when it's more than one, and its newest
/// message, with who wrote it in a group.
fn notice(row: &ChatRow) -> Notice {
    let text = row.last.as_ref().map(preview).unwrap_or_default();
    let from = row.last.as_ref().and_then(|last| last.from.as_deref()).filter(|_| row.kind != "direct");
    Notice {
        title: row.name.clone(),
        subtitle: (row.unread > 1).then(|| format!("{} unread", row.unread)),
        body: match from {
            Some(from) => format!("{from}: {text}"),
            None => text,
        },
    }
}

/// One notification for several chats, the newest first.
fn summary(rows: &[ChatRow]) -> Notice {
    let unread: i64 = rows.iter().map(|row| row.unread).sum();
    let names: Vec<&str> = rows.iter().take(AT_ONCE).map(|row| row.name.as_str()).collect();
    let body = match rows.len() - names.len() {
        0 => names.join(", "),
        more => format!("{} and {more} more", names.join(", ")),
    };
    Notice { title: "jaw".into(), subtitle: Some(format!("{unread} unread in {} chats", rows.len())), body }
}

/// The one notification for a catch-up that brought messages to several chats, the newest first.
fn caught_summary(rows: &[ChatRow], messages: usize) -> Notice {
    let new = match messages {
        1 => "1 new message".to_string(),
        n => format!("{n} new messages"),
    };
    Notice { subtitle: Some(format!("{new} in {} chats", rows.len())), ..summary(rows) }
}

/// A message on one line, at most PREVIEW characters, or its kind's word when it has no text.
fn preview(last: &Last) -> String {
    let line: String = last.summary.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let line = line.trim();
    if line.is_empty() {
        return kind_word(&last.kind).into();
    }
    match line.char_indices().nth(PREVIEW) {
        Some((cut, _)) => format!("{}…", line[..cut].trim_end()),
        None => line.into(),
    }
}

fn kind_word(kind: &str) -> &'static str {
    match kind {
        "image" => "Photo",
        "video" => "Video",
        "gif" => "GIF",
        "video_note" => "Video message",
        "voice" => "Voice message",
        "audio" => "Audio",
        "document" => "Document",
        "sticker" => "Sticker",
        "location" | "live_location" => "Location",
        "contact" => "Contact",
        "poll" => "Poll",
        "event" => "Event",
        _ => "Message",
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}
