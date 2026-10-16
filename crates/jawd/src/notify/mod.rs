//! Notifications for new messages, from jawd as ooze's come from `ooze watch`, so they arrive with
//! no window open. A task follows the store's committed events and gathers the chats they touch;
//! once the feed has been quiet for a moment, it reads those chats and notifies for each with a
//! new message from someone else since jawd started. A burst in a chat is one notification; what
//! comes in that chat soon after waits out a hold and goes as one more; and more chats than a few
//! at once, as when a catch-up ends, are one summary. Nothing goes while catching up. Muted and
//! archived chats, your own messages, history and a chat an interface in front is reading stay
//! quiet.

mod desktop;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use jaw_proto::Push;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use crate::attention::Attention;
use crate::feed::CatchingUp;
use crate::store::Store;
use crate::store::read::{self, ChatRow, Last};
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
    };
    tokio::spawn(follow.run(committed, pushes))
}

struct Follow {
    store: Store,
    attention: Arc<Attention>,
    catching_up: CatchingUp,
    to: Arc<dyn Notifier>,
    timing: Timing,
    /// When jawd started; older messages stay quiet.
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
}

impl Follow {
    async fn run(mut self, mut committed: watch::Receiver<i64>, pushes: broadcast::Receiver<Push>) {
        self.seen = *committed.borrow_and_update();
        let mut pushes = Some(pushes);
        loop {
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
                // A `connection` push comes when a catch-up ends, and the loop looks again.
                push = next(&mut pushes) => {
                    if let Err(broadcast::error::RecvError::Closed) = push {
                        pushes = None;
                    }
                }
                _ = sleep_until(wake) => {
                    let upto = *committed.borrow();
                    self.flush(upto).await;
                }
            }
        }
    }

    /// When there's next something to do, if there is.
    fn wake(&self) -> Option<Instant> {
        let changed = self.changed.map(|(first, last)| (first + self.timing.most).min(last + self.timing.quiet));
        let ready = match (self.catching_up)() {
            true => None,
            false => self.pending.values().min().copied(),
        };
        match (changed, ready) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    async fn flush(&mut self, upto: i64) {
        if self.changed.take().is_some() {
            self.take_in(upto).await;
        }
        if (self.catching_up)() {
            return;
        }
        let now = Instant::now();
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
        new.sort_by_key(|row| std::cmp::Reverse(row.last.as_ref().map_or(0, |last| last.at)));
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

    /// Reads the events after what's been taken in, up to `upto`, and marks the chats they touch.
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
            for event in events.iter().filter(|event| event.kind == "chat") {
                let ready = self.held.get(&event.key).copied().filter(|until| *until > now).unwrap_or(now);
                self.pending.entry(event.key).or_insert(ready);
            }
        }
    }

    /// Whether `row`'s newest message is one to notify about: new since it was last looked at and
    /// since jawd started, someone else's, unread, in a chat that isn't muted, archived or being
    /// read in front.
    fn new_message(&mut self, row: &ChatRow, now: i64) -> bool {
        let Some(last) = &row.last else { return false };
        if last.at < self.since || self.handled.get(&row.n).is_some_and(|&handled| last.at <= handled) {
            return false;
        }
        self.handled.insert(row.n, last.at);
        !last.mine
            && row.unread > 0
            && !row.archived
            && !row.muted_until.is_some_and(|until| until > now)
            && matches!(row.kind.as_str(), "direct" | "group" | "announcement")
            && !self.attention.reading(&row.id)
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
