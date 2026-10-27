//! Others' presence and what they say of themselves. jawd follows the people the windows show,
//! open one-to-one chats' first, then favourites, then rosters' in their order, and only while an
//! interface has focus, which is also the only time WhatsApp sends presence: at most 20 new people
//! a minute and 200 on each connection to WhatsApp, since subscribing to everyone is what presence
//! trackers do and WhatsApp polices it. What WhatsApp says is written to people's rows, which
//! rosters sort by; typing fades on its own if its stop never comes. About texts are asked of
//! WhatsApp 50 people at a time, each at most once a day, while it can be reached.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jaw_proto::{Connection, ConnectionState, Push};
use rusqlite::params;
use tokio::sync::{broadcast, mpsc, watch};

use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::feed::Feed;
use crate::store::folders::{Presence as Said, Step, TYPING_FADES_MS};
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// The most people followed on one connection to WhatsApp, and newly a minute.
pub const MOST: usize = 200;
pub const PER_MINUTE: usize = 20;

/// Whether someone's looking: an interface's focus, and the connection to WhatsApp as the
/// `connection` pushes tell. `online` says how it is now, read once the pushes are subscribed to
/// and again whenever some were missed, so no change of it goes unseen.
pub struct Looking {
    pub focused: watch::Receiver<bool>,
    pub pushes: broadcast::Receiver<Push>,
    pub online: Box<dyn Fn() -> bool + Send + Sync>,
}

/// Whom presence is followed within: at most `most` on a connection, `per_minute` newly in each
/// `minute`; and the longest about texts wait to be asked for when none is due, `about_idle`.
/// Tests make the times shorter.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub most: usize,
    pub per_minute: usize,
    pub minute: Duration,
    pub about_idle: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { most: MOST, per_minute: PER_MINUTE, minute: Duration::from_secs(60), about_idle: ABOUT_IDLE }
    }
}
/// The most people one UserInfo asks about, and how often about texts are asked again.
const ABOUT_AT_ONCE: usize = 50;
const ABOUT_KEPT_MS: i64 = 24 * 3600 * 1000;
/// How long between two asks while some are due, and the longest wait when none is: people who
/// come later, a new contact or a first sync's, are asked about within it.
const ABOUT_PACE: Duration = Duration::from_secs(10);
const ABOUT_IDLE: Duration = Duration::from_secs(60);
/// What WhatsApp said, waiting to be written; past it, the oldest is dropped, as presence is.
const HEARD_MOST: usize = 1_024;

/// What WhatsApp said of someone, to write.
pub enum Heard {
    Presence(pb::PresenceUpdate),
    Typing(pb::Typing),
}

#[derive(Default)]
struct Stats {
    /// Followed on this connection, wanted now, and those wanted that aren't for the limits.
    subscribed: usize,
    wanted: usize,
    waiting: usize,
    /// Presence and typing heard, and dropped for coming faster than they could be written.
    heard: u64,
    dropped: u64,
    /// About texts asked for, and those due now.
    asked: u64,
}

#[derive(Clone)]
pub struct Following {
    heard: mpsc::Sender<Heard>,
    hearing: Arc<Mutex<Option<mpsc::Receiver<Heard>>>>,
    stats: Arc<Mutex<Stats>>,
}

impl Default for Following {
    fn default() -> Self {
        let (heard, hearing) = mpsc::channel(HEARD_MOST);
        Following { heard, hearing: Arc::new(Mutex::new(Some(hearing))), stats: Arc::default() }
    }
}

impl Following {
    /// Starts writing what's heard, and following presence for the windows `feed` serves while
    /// `focused` says an interface is in front, through `engine` once WhatsApp can be reached, as
    /// `pushes` tell.
    pub fn start(&self, store: Store, engine: Option<Supervisor>, feed: Feed, looking: Looking, limits: Limits) {
        let Looking { focused, pushes, online } = looking;
        let Some(hearing) = self.hearing.lock().expect("presence heard").take() else { return };
        let (connected, connection) = watch::channel(online());
        tokio::spawn(write_heard(store.clone(), hearing));
        tokio::spawn(connection_of(pushes, connected, online));
        if let Some(engine) = engine {
            let stats = self.stats.clone();
            let following =
                follow(store.clone(), engine.clone(), feed, focused, connection.clone(), stats.clone(), limits);
            tokio::spawn(following);
            tokio::spawn(about(store, engine, connection, stats, limits.about_idle));
        }
    }

    /// Takes what WhatsApp said, never waiting: presence that comes faster than it can be written
    /// is dropped, as WhatsApp's own clients drop it.
    pub fn heard(&self, heard: Heard) {
        let mut stats = self.stats.lock().expect("presence stats");
        stats.heard += 1;
        if self.heard.try_send(heard).is_err() {
            stats.dropped += 1;
        }
    }

    /// What `jawd status` says of it.
    pub fn status(&self) -> jaw_proto::PresenceStatus {
        let stats = self.stats.lock().expect("presence stats");
        jaw_proto::PresenceStatus {
            followed: stats.subscribed as u64,
            most: MOST as u64,
            per_minute: PER_MINUTE as u64,
            wanted: stats.wanted as u64,
            waiting: stats.waiting as u64,
            heard: stats.heard,
            dropped: stats.dropped,
            about_asked: stats.asked,
        }
    }
}

fn now_ms() -> i64 {
    crate::feed::now_ms()
}

/// Says when WhatsApp can be reached, from the `connection` pushes, and from how it is now when
/// some of them were missed.
async fn connection_of(
    mut pushes: broadcast::Receiver<Push>,
    connected: watch::Sender<bool>,
    online: Box<dyn Fn() -> bool + Send + Sync>,
) {
    loop {
        let now = match pushes.recv().await {
            Ok(push) if push.method == "connection" => serde_json::from_value::<Connection>(push.params)
                .is_ok_and(|c| matches!(c.state, ConnectionState::Online | ConnectionState::CatchingUp)),
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => online(),
            Err(broadcast::error::RecvError::Closed) => return,
        };
        connected.send_if_modified(|was| std::mem::replace(was, now) != now);
    }
}

/// Whether a typing event is someone's in their own one-to-one chat, which the roster shows.
fn in_their_chat(typing: &pb::Typing) -> bool {
    let user = |jid: &str| jid.split(['@', ':', '.']).next().unwrap_or_default().to_string();
    !typing.chat.ends_with("@g.us") && user(&typing.chat) == user(&typing.sender)
}

/// Writes what WhatsApp says as it comes, what's waiting in one transaction, and lets typing fade
/// once TYPING_FADES_MS pass without it being said again.
async fn write_heard(store: Store, mut hearing: mpsc::Receiver<Heard>) {
    let mut fading: HashMap<String, Instant> = HashMap::new();
    let fade = Duration::from_millis(TYPING_FADES_MS as u64);
    loop {
        let next = fading.values().min().copied();
        let first = tokio::select! {
            heard = hearing.recv() => match heard {
                Some(heard) => Some(heard),
                None => return,
            },
            _ = async { tokio::time::sleep_until(next.unwrap_or_else(Instant::now).into()).await }, if next.is_some() => None,
        };
        let mut changes = Vec::new();
        let now = Instant::now();
        fading.retain(|jid, at| {
            if *at <= now {
                changes.push(Change::Folders(Step::Presence { jid: jid.clone(), state: Said::Paused }));
                false
            } else {
                true
            }
        });
        let mut waiting: Vec<Heard> = first.into_iter().collect();
        while waiting.len() < HEARD_MOST {
            match hearing.try_recv() {
                Ok(heard) => waiting.push(heard),
                Err(_) => break,
            }
        }
        for heard in waiting {
            match heard {
                Heard::Presence(p) => {
                    let state = if p.unavailable {
                        Said::Away((p.last_seen_ms > 0).then_some(p.last_seen_ms))
                    } else {
                        Said::Online
                    };
                    if p.unavailable {
                        fading.remove(&p.jid);
                    }
                    changes.push(Change::Folders(Step::Presence { jid: p.jid, state }));
                }
                Heard::Typing(t) if in_their_chat(&t) => {
                    let state = match t.state() {
                        pb::chat_presence::State::Composing => Said::Typing,
                        pb::chat_presence::State::Recording => Said::Recording,
                        _ => Said::Paused,
                    };
                    match state {
                        Said::Paused => fading.remove(&t.sender),
                        _ => fading.insert(t.sender.clone(), Instant::now() + fade),
                    };
                    changes.push(Change::Folders(Step::Presence { jid: t.sender, state }));
                }
                Heard::Typing(_) => {}
            }
        }
        if !changes.is_empty()
            && let Err(e) = store.apply(Lane::Live, changes).await
        {
            eprintln!("jawd: can't write who's around: {e:#}");
        }
    }
}

/// The JID to follow someone by: their phone number's, or their LID.
fn jid_of(db: &rusqlite::Connection, person: i64) -> anyhow::Result<Option<String>> {
    let jids: Vec<String> = db
        .prepare_cached("SELECT jid FROM person_jids WHERE person = ?1")?
        .query_map([person], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(jids.iter().find(|jid| jid.ends_with("@s.whatsapp.net")).or(jids.first()).cloned())
}

/// The people favourite slots hold: in favourite folders of yours, or the person of a favourite chat.
fn favoured(db: &rusqlite::Connection) -> anyhow::Result<HashSet<i64>> {
    let people: HashSet<i64> = db
        .prepare_cached(
            "SELECT p.person FROM favourites s CROSS JOIN folder_people p ON p.folder = s.folder
            UNION ALL SELECT c.person FROM favourites s CROSS JOIN chats c ON c.n = s.chat WHERE c.person IS NOT NULL",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(people)
}

/// Follows the people the windows show while someone's looking and WhatsApp can be reached,
/// within MOST on each connection and PER_MINUTE newly: open one-to-one chats' people first, then
/// favourites, then rosters' people in their order.
async fn follow(
    store: Store,
    engine: Supervisor,
    feed: Feed,
    mut focused: watch::Receiver<bool>,
    mut connected: watch::Receiver<bool>,
    stats: Arc<Mutex<Stats>>,
    limits: Limits,
) {
    let mut shown = feed.shown();
    let mut subscribed: HashSet<i64> = HashSet::new();
    let mut minute: VecDeque<Instant> = VecDeque::new();
    let mut written: Option<Vec<i64>> = None;
    let mut refused: HashSet<i64> = HashSet::new();
    let mut was_connected = *connected.borrow();
    loop {
        let (looking, online) = (*focused.borrow_and_update(), *connected.borrow_and_update());
        shown.borrow_and_update();
        if online != was_connected {
            // A connection is new to WhatsApp: whom it follows starts again, and nothing's known.
            subscribed.clear();
            refused.clear();
            written = None;
            if let Err(e) = store.apply(Lane::Live, vec![Change::Folders(Step::Unknown)]).await {
                eprintln!("jawd: can't forget who was around: {e:#}");
            }
            was_connected = online;
        }
        let (open, listed) = if looking && online { feed.showing() } else { Default::default() };
        let favoured: HashSet<i64> = store.reads().query(Priority::Background, favoured).await.unwrap_or_default();
        let mut ordered = open;
        ordered.extend(listed.iter().copied().filter(|n| favoured.contains(n)));
        ordered.extend(listed.iter().copied().filter(|n| !favoured.contains(n)));
        let now = Instant::now();
        while minute.front().is_some_and(|at| now.duration_since(*at) >= limits.minute) {
            minute.pop_front();
        }
        for &person in &ordered {
            if subscribed.contains(&person) || refused.contains(&person) {
                continue;
            }
            if subscribed.len() >= limits.most || minute.len() >= limits.per_minute {
                break;
            }
            let jid = match store.reads().query(Priority::Background, move |db| jid_of(db, person)).await {
                Ok(Some(jid)) => jid,
                _ => {
                    refused.insert(person);
                    continue;
                }
            };
            match engine.call(pb::command::Kind::SubscribePresence(pb::SubscribePresence { jid: jid.clone() })).await {
                Ok(_) => {
                    subscribed.insert(person);
                    minute.push_back(Instant::now());
                }
                Err(e) => {
                    eprintln!("jawd: can't follow {jid}'s presence: {e}");
                    refused.insert(person);
                }
            }
        }
        let followed: Vec<i64> = if looking && online {
            ordered.iter().copied().filter(|n| subscribed.contains(n)).collect()
        } else {
            Vec::new()
        };
        let mut sorted = followed.clone();
        sorted.sort_unstable();
        if written.as_ref() != Some(&sorted) {
            match store.apply(Lane::Live, vec![Change::Folders(Step::Following { people: sorted.clone() })]).await {
                Ok(_) => written = Some(sorted),
                Err(e) => eprintln!("jawd: can't write whom it follows: {e:#}"),
            }
        }
        let waiting = ordered.iter().filter(|n| !subscribed.contains(n) && !refused.contains(n)).count();
        {
            let mut stats = stats.lock().expect("presence stats");
            stats.subscribed = subscribed.len();
            stats.wanted = ordered.len();
            stats.waiting = waiting;
        }
        // Someone waits for the minute to pass; or nothing does until a window, focus or the
        // connection changes.
        let until = (waiting > 0 && subscribed.len() < limits.most)
            .then(|| minute.front().map(|at| *at + limits.minute))
            .flatten();
        tokio::select! {
            changed = shown.changed() => if changed.is_err() { return },
            changed = focused.changed() => if changed.is_err() { return },
            changed = connected.changed() => if changed.is_err() { return },
            _ = async { tokio::time::sleep_until(until.unwrap_or_else(Instant::now).into()).await }, if until.is_some() => {}
        }
    }
}

/// Asks WhatsApp what the roster's people say of themselves, the longest unasked first, 50 at a
/// time, each at most once a day, while WhatsApp can be reached.
async fn about(
    store: Store,
    engine: Supervisor,
    mut connected: watch::Receiver<bool>,
    stats: Arc<Mutex<Stats>>,
    idle: Duration,
) {
    let idle_ms = idle.as_millis().max(1) as i64;
    loop {
        if !*connected.borrow_and_update() {
            if connected.changed().await.is_err() {
                return;
            }
            continue;
        }
        let before = now_ms() - ABOUT_KEPT_MS;
        let due = store
            .reads()
            .query(Priority::Background, move |db| {
                let people: Vec<(i64, Option<i64>)> = db
                    .prepare_cached(
                        "SELECT n, about_at FROM people WHERE (name IS NOT NULL OR direct_chats > 0)
                            AND (about_at IS NULL OR about_at < ?1)
                        ORDER BY about_at LIMIT ?2",
                    )?
                    .query_map(params![before, ABOUT_AT_ONCE as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                let next: Option<i64> = db
                    .prepare_cached(
                        "SELECT min(about_at) FROM people WHERE (name IS NOT NULL OR direct_chats > 0) AND about_at >= ?1",
                    )?
                    .query_row([before], |r| r.get(0))?;
                let mut jids = Vec::new();
                for (n, _) in &people {
                    jids.extend(jid_of(db, *n)?);
                }
                Ok((jids, next))
            })
            .await;
        let wait = match due {
            Ok((jids, _)) if !jids.is_empty() => {
                let at = now_ms();
                match engine.call(pb::command::Kind::UserInfo(pb::UserInfo { jids: jids.clone() })).await {
                    Ok(pb::reply::Kind::UserInfoList(list)) => {
                        stats.lock().expect("presence stats").asked += 1;
                        let about = list.users.into_iter().map(|user| (user.jid, user.about)).collect();
                        let step = Step::About { asked: jids, about, at };
                        if let Err(e) = store.apply(Lane::Background, vec![Change::Folders(step)]).await {
                            eprintln!("jawd: can't keep what people say of themselves: {e:#}");
                        }
                        ABOUT_PACE
                    }
                    Ok(other) => {
                        eprintln!("jawd: the engine answered who people are with {other:?}");
                        idle
                    }
                    Err(e) => {
                        eprintln!("jawd: can't ask WhatsApp what people say of themselves: {e}");
                        ABOUT_PACE * 6
                    }
                }
            }
            Ok((_, Some(next))) => {
                Duration::from_millis((next + ABOUT_KEPT_MS - now_ms()).clamp(1_000.min(idle_ms), idle_ms) as u64)
            }
            Ok(_) => idle,
            Err(e) => {
                eprintln!("jawd: can't read whose about text is due: {e:#}");
                ABOUT_PACE * 6
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            changed = connected.changed() => if changed.is_err() { return },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::engine::demo::DemoEngine;
    use crate::engine::supervisor::Backoff;
    use crate::store::Person;
    use crate::store::folders::{Done, Item, Outcome, Target};

    const AT: i64 = 1_792_000_000_000;

    fn jid(n: usize) -> String {
        format!("3519200{n:05}@s.whatsapp.net")
    }

    /// How many people the engine has been asked to follow, and whom first.
    fn asked(world: &crate::engine::demo::World) -> Vec<String> {
        world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::SubscribePresence(s) => Some(s.jid),
                _ => None,
            })
            .collect()
    }

    async fn until(what: &str, mut done: impl FnMut() -> bool) {
        let began = Instant::now();
        while !done() {
            assert!(began.elapsed() < Duration::from_secs(20), "{what} didn't happen");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn the_connection_is_read_again_when_its_pushes_were_missed() {
        let (pushes, heard) = broadcast::channel(1);
        let (connected, mut connection) = watch::channel(false);
        let online = Arc::new(AtomicBool::new(false));
        let now = online.clone();
        tokio::spawn(connection_of(heard, connected, Box::new(move || now.load(Ordering::Relaxed))));
        // Two pushes into room for one, before it reads: the connection push is missed.
        online.store(true, Ordering::Relaxed);
        pushes.send(Push { method: "connection".into(), params: serde_json::json!({"state": "online"}) }).unwrap();
        pushes
            .send(Push { method: "notice".into(), params: serde_json::json!({"text": "x", "tone": "info"}) })
            .unwrap();
        let seen = tokio::time::timeout(Duration::from_secs(5), connection.wait_for(|&up| up)).await;
        assert!(seen.is_ok(), "a missed push is made up for by how the connection is now");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn someone_who_comes_once_nobody_is_due_is_asked_about_within_the_idle_wait() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let engine = DemoEngine::with(vec![crate::engine::demo::connected()]);
        let world = engine.world();
        let (events, _incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let feed = Feed::start(store.clone(), Arc::new(|| false), crate::feed::Timing::default());
        let (_focus, focused) = watch::channel(false);
        let (pushes, _) = broadcast::channel(16);
        let limits = Limits { about_idle: Duration::from_millis(200), ..Limits::default() };
        let looking = Looking { focused, pushes: pushes.subscribe(), online: Box::new(|| true) };
        Following::default().start(store.clone(), Some(supervisor), feed, looking, limits);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let ana = Person { jids: vec![jid(1)], name: Some("Ana".into()), ..Person::default() };
        store.apply(Lane::Live, vec![Change::Person(ana)]).await.unwrap();
        let asked_about = |world: &crate::engine::demo::World| {
            world
                .asked()
                .into_iter()
                .any(|kind| matches!(kind, pb::command::Kind::UserInfo(u) if u.jids.contains(&jid(1))))
        };
        until("Ana asked about", || asked_about(&world)).await;
        let began = Instant::now();
        loop {
            let about = store
                .reads()
                .query(crate::work::Priority::Interactive, |db| {
                    Ok(db.query_row(
                        "SELECT p.about FROM people p JOIN person_jids j ON j.person = p.n WHERE j.jid = ?1",
                        [jid(1)],
                        |r| r.get::<_, Option<String>>(0),
                    )?)
                })
                .await
                .unwrap();
            if about.is_some() {
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(20), "her about text was never kept");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn presence_is_followed_within_its_limits_favourites_first_and_only_while_someone_looks() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let people: Vec<Change> = (0..260)
            .map(|n| {
                Change::Person(Person { jids: vec![jid(n)], name: Some(format!("Person {n:03}")), ..Person::default() })
            })
            .collect();
        store.apply(Lane::Live, people).await.unwrap();
        // The last person is a favourite's, by their chat, so they come first however far down.
        store
            .apply(
                Lane::Live,
                vec![Change::Message(crate::store::Message {
                    chat: jid(259),
                    id: "m".into(),
                    at: AT,
                    sender: Some(jid(259)),
                    kind: "text",
                    summary: "x".into(),
                    ..Default::default()
                })],
            )
            .await
            .unwrap();
        let done = Done::default();
        let favourite = Step::Favourite { slot: 1, target: Some(Target::Chat(jid(259))), done: done.clone() };
        store.apply_durable(vec![Change::Folders(favourite)]).await.unwrap();
        let engine = DemoEngine::with(vec![crate::engine::demo::connected()]);
        let world = engine.world();
        let (events, _incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let feed = Feed::start(store.clone(), Arc::new(|| false), crate::feed::Timing::default());
        let (out, _lines) = mpsc::channel(64);
        let mut client = feed.client(out);
        client
            .call("roster.watch", serde_json::json!({"folder": "all", "around": "top", "after": 1000}))
            .await
            .unwrap();
        let (focus, focused) = watch::channel(false);
        let (pushes, _) = broadcast::channel(16);
        let limits = Limits { most: 200, per_minute: 20, minute: Duration::from_millis(400), about_idle: ABOUT_IDLE };
        let following = Following::default();
        let looking = Looking { focused, pushes: pushes.subscribe(), online: Box::new(|| true) };
        following.start(store.clone(), Some(supervisor), feed.clone(), looking, limits);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(asked(&world).is_empty(), "nobody's followed while nobody looks");
        focus.send_replace(true);
        until("the first minute's twenty", || asked(&world).len() >= 20).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(asked(&world).len(), 20, "twenty in the first minute");
        assert_eq!(asked(&world)[0], jid(259), "the favourite's first");
        until("two hundred", || asked(&world).len() >= 200).await;
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        assert_eq!(asked(&world).len(), 200, "never more than two hundred on a connection");
        let status = following.status();
        assert_eq!((status.followed, status.wanted, status.waiting), (200, 260, 60));
        let followed = || {
            store.reads().query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT count(*) FROM people WHERE followed", [], |r| r.get::<_, i64>(0))?)
            })
        };
        assert_eq!(followed().await.unwrap(), 200, "each followed shows as known");
        focus.send_replace(false);
        let stopped = AtomicBool::new(false);
        let began = Instant::now();
        while !stopped.load(Ordering::Relaxed) && began.elapsed() < Duration::from_secs(10) {
            if followed().await.unwrap() == 0 {
                stopped.store(true, Ordering::Relaxed);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(stopped.load(Ordering::Relaxed), "nobody's known once nobody looks");
        drop(client);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_open_one_to_one_chats_person_is_followed_first_though_favourites_would_fill_the_connection() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        // More favourites than a connection follows, in a folder on Alt-1, and Rui, who isn't one
        // and whose chat is open: behind them, he'd never be followed.
        let rui = jid(MOST + 10);
        let people: Vec<Change> = (0..=MOST + 10)
            .map(|n| {
                Change::Person(Person { jids: vec![jid(n)], name: Some(format!("Person {n:03}")), ..Person::default() })
            })
            .collect();
        store.apply(Lane::Live, people).await.unwrap();
        let said = crate::store::Message {
            chat: rui.clone(),
            id: "m".into(),
            at: AT,
            sender: Some(rui.clone()),
            kind: "text",
            summary: "x".into(),
            ..Default::default()
        };
        store.apply(Lane::Live, vec![Change::Message(said)]).await.unwrap();
        let done = Done::default();
        let close = Step::Create { name: "Close".into(), parent: None, rule: None, done: done.clone() };
        store.apply_durable(vec![Change::Folders(close)]).await.unwrap();
        let Some(Outcome::Done(Some(close))) = done.take() else { panic!("the folder wasn't made") };
        let others: Vec<i64> = store
            .reads()
            .query(crate::work::Priority::Interactive, {
                let rui = rui.clone();
                move |db| {
                    Ok(db
                        .prepare("SELECT person FROM person_jids WHERE jid <> ?1")?
                        .query_map([rui], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?)
                }
            })
            .await
            .unwrap();
        let filed = others
            .into_iter()
            .map(|n| Change::Folders(Step::File { folder: close, item: Item::Person(n), done: Done::default() }))
            .collect();
        store.apply_durable(filed).await.unwrap();
        let slot = Step::Favourite { slot: 1, target: Some(Target::Folder(close)), done: Done::default() };
        store.apply_durable(vec![Change::Folders(slot)]).await.unwrap();
        let engine = DemoEngine::with(vec![crate::engine::demo::connected()]);
        let world = engine.world();
        let (events, _incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let feed = Feed::start(store.clone(), Arc::new(|| false), crate::feed::Timing::default());
        let (out, _lines) = mpsc::channel(64);
        let mut client = feed.client(out);
        client
            .call("roster.watch", serde_json::json!({"folder": "all", "around": "top", "after": 1000}))
            .await
            .unwrap();
        client
            .call("messages.watch", serde_json::json!({"chat": rui, "around": "newest", "before": 10}))
            .await
            .unwrap();
        let (focus, focused) = watch::channel(false);
        let (pushes, _) = broadcast::channel(16);
        let limits = Limits { minute: Duration::from_millis(400), ..Limits::default() };
        let looking = Looking { focused, pushes: pushes.subscribe(), online: Box::new(|| true) };
        Following::default().start(store.clone(), Some(supervisor), feed.clone(), looking, limits);
        focus.send_replace(true);
        until("the first minute's", || asked(&world).len() >= PER_MINUTE).await;
        let asked = asked(&world);
        assert_eq!(asked[0], rui, "the open chat's person first: {:?}", &asked[..3]);
        drop(client);
    }
}
