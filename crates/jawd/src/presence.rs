//! Others' presence and what they say of themselves. jawd follows the people the windows show,
//! open one-to-one chats' first, then favourites, then rosters' in their order, and only while an
//! interface has focus, which is also the only time WhatsApp sends presence: at most 20 new people
//! a minute and 200 on each connection to WhatsApp, since subscribing to everyone is what presence
//! trackers do and WhatsApp polices it; a keepalive timeout keeps the connection, and whom it
//! follows. Whether they're online, and when they were last seen, is written to people's rows,
//! which rosters sort by and conversations read; who's typing never is. About texts are asked of
//! WhatsApp 50 people at a time, each at most once a day, while it can be reached.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jaw_proto::{Connection, ConnectionState, Push};
use rusqlite::params;
use tokio::sync::{broadcast, mpsc, watch};

use crate::engine::pb;
use crate::engine::supervisor::Supervisor;
use crate::feed::Feed;
use crate::store::folders::{Presence as Said, Step};
use crate::store::{Change, Lane, Store};
use crate::work::Priority;

/// The most people followed on one connection to WhatsApp, and newly a minute.
pub const MOST: usize = 200;
pub const PER_MINUTE: usize = 20;

/// Whether someone's looking: an interface's focus, and the connection to WhatsApp as the
/// `connection` pushes tell. `reach` says how it is now: whether WhatsApp can be reached, and how
/// many connections to it have ended, read together, since sync changes both at once and counts a
/// connection's end before a push says it; it's read once the pushes are subscribed to, again
/// whenever some were missed, and each time whom to follow is worked out, so no change of it goes
/// unseen. `connections` changes as connections end, as sync counts them: whom presence follows is
/// counted for each, and a keepalive timeout, which leaves WhatsApp unreachable on the same
/// connection, ends none.
pub struct Looking {
    pub focused: watch::Receiver<bool>,
    pub pushes: broadcast::Receiver<Push>,
    pub reach: Arc<dyn Fn() -> (bool, u64) + Send + Sync>,
    pub connections: watch::Receiver<u64>,
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

#[derive(Default)]
struct Stats {
    /// Followed on this connection, wanted now, and those wanted that aren't for the limits.
    subscribed: usize,
    wanted: usize,
    waiting: usize,
    /// Presence heard, and dropped for coming faster than it could be written.
    heard: u64,
    dropped: u64,
    /// About texts asked for, and those due now.
    asked: u64,
}

#[derive(Clone)]
pub struct Following {
    heard: mpsc::Sender<pb::PresenceUpdate>,
    hearing: Arc<Mutex<Option<mpsc::Receiver<pb::PresenceUpdate>>>>,
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
        let Looking { focused, pushes, reach, connections } = looking;
        let Some(hearing) = self.hearing.lock().expect("presence heard").take() else { return };
        let (connected, connection) = watch::channel(reach().0);
        tokio::spawn(write_heard(store.clone(), hearing));
        tokio::spawn(connection_of(pushes, connected, reach.clone()));
        if let Some(engine) = engine {
            let stats = self.stats.clone();
            let reach = Reach { connected: connection.clone(), connections, now: reach };
            let following = follow(store.clone(), engine.clone(), feed, focused, reach, stats.clone(), limits);
            tokio::spawn(following);
            tokio::spawn(about(store, engine, connection, stats, limits.about_idle));
        }
    }

    /// Takes what WhatsApp said, never waiting: presence that comes faster than it can be written
    /// is dropped, as WhatsApp's own clients drop it.
    pub fn heard(&self, presence: pb::PresenceUpdate) {
        let mut stats = self.stats.lock().expect("presence stats");
        stats.heard += 1;
        if self.heard.try_send(presence).is_err() {
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

/// Whether WhatsApp can be reached while the connection is `state`. With the phone offline it can:
/// WhatsApp's servers send presence whether or not the phone answers.
pub fn reachable(state: ConnectionState) -> bool {
    use ConnectionState::{CatchingUp, Online, PhoneOffline};
    matches!(state, Online | CatchingUp | PhoneOffline)
}

/// Says when WhatsApp can be reached, from the `connection` pushes, and from how it is now when
/// some of them were missed.
async fn connection_of(
    mut pushes: broadcast::Receiver<Push>,
    connected: watch::Sender<bool>,
    reach: Arc<dyn Fn() -> (bool, u64) + Send + Sync>,
) {
    loop {
        let now = match pushes.recv().await {
            Ok(push) if push.method == "connection" => {
                serde_json::from_value::<Connection>(push.params).is_ok_and(|c| reachable(c.state))
            }
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => reach().0,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        connected.send_if_modified(|was| std::mem::replace(was, now) != now);
    }
}

/// Writes the presence WhatsApp says as it comes, what's waiting in one transaction.
async fn write_heard(store: Store, mut hearing: mpsc::Receiver<pb::PresenceUpdate>) {
    while let Some(first) = hearing.recv().await {
        let mut waiting = vec![first];
        while waiting.len() < HEARD_MOST {
            match hearing.try_recv() {
                Ok(heard) => waiting.push(heard),
                Err(_) => break,
            }
        }
        let changes = waiting
            .into_iter()
            .map(|p| {
                let state = if p.unavailable {
                    Said::Away((p.last_seen_ms > 0).then_some(p.last_seen_ms))
                } else {
                    Said::Online
                };
                Change::Folders(Step::Presence { jid: p.jid, state })
            })
            .collect();
        if let Err(e) = store.apply(Lane::Live, changes).await {
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

/// What changes as WhatsApp comes and goes, as the pushes say it and as connections end, and how it
/// is `now`: whether it can be reached, and how many connections to it have ended.
struct Reach {
    connected: watch::Receiver<bool>,
    connections: watch::Receiver<u64>,
    now: Arc<dyn Fn() -> (bool, u64) + Send + Sync>,
}

/// Follows the people the windows show while someone's looking and WhatsApp can be reached,
/// within MOST on each connection and PER_MINUTE newly: open one-to-one chats' people first, then
/// favourites, then rosters' people in their order. Whom a connection follows it follows until it
/// ends: while a keepalive timeout leaves WhatsApp unreachable on it, nobody's known, and once its
/// pings get through they're known again at once, never asked for again or counted twice.
async fn follow(
    store: Store,
    engine: Supervisor,
    feed: Feed,
    mut focused: watch::Receiver<bool>,
    reach: Reach,
    stats: Arc<Mutex<Stats>>,
    limits: Limits,
) {
    let Reach { mut connected, mut connections, now: reach } = reach;
    let mut shown = feed.shown();
    let mut subscribed: HashSet<i64> = HashSet::new();
    let mut minute: VecDeque<Instant> = VecDeque::new();
    let mut written: Option<Vec<i64>> = None;
    let mut refused: HashSet<i64> = HashSet::new();
    // The connection whom `subscribed` holds were followed on, by how many had ended before it.
    let (mut on, mut counting): (Option<u64>, bool) = (None, true);
    loop {
        let looking = *focused.borrow_and_update();
        connected.borrow_and_update();
        connections.borrow_and_update();
        shown.borrow_and_update();
        // Not the pushes' word: a connection's end is counted before its push says so, and the
        // pushes alone would show it reachable, its end taken for the next connection.
        let (online, ended) = reach();
        if online && on != Some(ended) {
            // A connection is new to WhatsApp: whom it follows starts again, and nothing's known.
            subscribed.clear();
            refused.clear();
            written = None;
            if let Err(e) = store.apply(Lane::Live, vec![Change::Folders(Step::Unknown)]).await {
                eprintln!("jawd: can't forget who was around: {e:#}");
            }
            on = Some(ended);
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
            // With nothing left to count connections, the one that's running is the last.
            changed = connections.changed(), if counting => counting = changed.is_ok(),
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
        tokio::spawn(connection_of(heard, connected, Arc::new(move || (now.load(Ordering::Relaxed), 0))));
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
        let looking = Looking {
            focused,
            pushes: pushes.subscribe(),
            reach: Arc::new(|| (true, 0)),
            connections: watch::channel(0).1,
        };
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
        let looking = Looking {
            focused,
            pushes: pushes.subscribe(),
            reach: Arc::new(|| (true, 0)),
            connections: watch::channel(0).1,
        };
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
        let looking = Looking {
            focused,
            pushes: pushes.subscribe(),
            reach: Arc::new(|| (true, 0)),
            connections: watch::channel(0).1,
        };
        Following::default().start(store.clone(), Some(supervisor), feed.clone(), looking, limits);
        focus.send_replace(true);
        until("the first minute's", || asked(&world).len() >= PER_MINUTE).await;
        let asked = asked(&world);
        assert_eq!(asked[0], rui, "the open chat's person first: {:?}", &asked[..3]);
        drop(client);
    }

    /// jawd following presence for a roster of `people`, on a connection to WhatsApp that the test
    /// says is reachable or not, through `connection` pushes, and ends by counting it in `ended`,
    /// as sync does; with someone looking all along. `reach` is whether it can be reached and how
    /// many connections have ended, changed together, as sync changes them under its lock.
    struct Followed {
        _home: tempfile::TempDir,
        store: Store,
        world: Arc<crate::engine::demo::World>,
        following: Following,
        pushes: broadcast::Sender<Push>,
        reach: Arc<Mutex<(bool, u64)>>,
        ended: watch::Sender<u64>,
        client: crate::feed::Client,
        _focus: watch::Sender<bool>,
    }

    impl Followed {
        async fn new(people: usize, roster: usize, limits: Limits) -> Followed {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let named = (0..people).map(|n| {
                Change::Person(Person { jids: vec![jid(n)], name: Some(format!("Person {n:03}")), ..Person::default() })
            });
            store.apply(Lane::Live, named.collect()).await.unwrap();
            let engine = DemoEngine::with(vec![crate::engine::demo::connected()]);
            let world = engine.world();
            let (events, _incoming) = mpsc::channel(1024);
            let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
            let feed = Feed::start(store.clone(), Arc::new(|| false), crate::feed::Timing::default());
            let (out, _lines) = mpsc::channel(64);
            let mut client = feed.client(out);
            let watched = serde_json::json!({"folder": "all", "around": "top", "after": roster});
            client.call("roster.watch", watched).await.unwrap();
            let (focus, focused) = watch::channel(true);
            let (pushes, _) = broadcast::channel(16);
            let (ended, connections) = watch::channel(0);
            let reach = Arc::new(Mutex::new((true, 0)));
            let now = reach.clone();
            let looking = Looking {
                focused,
                pushes: pushes.subscribe(),
                reach: Arc::new(move || *now.lock().unwrap()),
                connections,
            };
            let following = Following::default();
            following.start(store.clone(), Some(supervisor), feed, looking, limits);
            Followed { _home: home, store, world, following, pushes, reach, ended, client, _focus: focus }
        }

        /// WhatsApp can be reached now or can't, as the `connection` push says.
        fn say(&self, state: &str) {
            let state = serde_json::from_value(serde_json::json!(state)).expect("a connection's state");
            self.reach.lock().unwrap().0 = reachable(state);
            let _ = self.pushes.send(Push { method: "connection".into(), params: serde_json::json!({"state": state}) });
        }

        /// The connection ends, as sync's `cut_short` ends it: counted, and unreachable, at once,
        /// before any push says so.
        fn ends(&self) {
            let mut reach = self.reach.lock().unwrap();
            *reach = (false, reach.1 + 1);
            self.ended.send_replace(reach.1);
        }

        async fn followed(&self) -> i64 {
            let count = |db: &rusqlite::Connection| {
                Ok(db.query_row("SELECT count(*) FROM people WHERE followed", [], |r| r.get::<_, i64>(0))?)
            };
            self.store.reads().query(crate::work::Priority::Interactive, count).await.unwrap()
        }

        async fn until_followed(&self, n: i64, what: &str) {
            let began = Instant::now();
            while self.followed().await != n {
                assert!(began.elapsed() < Duration::from_secs(20), "{what} didn't happen");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        async fn around(&self) -> i64 {
            let count = |db: &rusqlite::Connection| {
                Ok(db.query_row("SELECT count(*) FROM people WHERE presence = 'online'", [], |r| r.get::<_, i64>(0))?)
            };
            self.store.reads().query(crate::work::Priority::Interactive, count).await.unwrap()
        }
    }

    /// A keepalive timeout leaves WhatsApp unreachable on the same connection: nobody's known while
    /// it lasts, each keeping the state last heard, and once its pings get through again those the
    /// connection followed are known again at once, none asked for again or counted twice; only a
    /// new connection starts again (the keepalive audit's O10).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_keepalive_timeout_and_its_restore_bring_back_whom_the_connection_followed_at_once_and_once() {
        // A minute longer than the test, so the first twenty are all its minute takes.
        let limits = Limits { minute: Duration::from_secs(600), ..Limits::default() };
        let followed = Followed::new(25, 1000, limits).await;
        until("the minute's twenty", || asked(&followed.world).len() >= 20).await;
        followed.until_followed(20, "the twenty followed").await;
        followed.following.heard(pb::PresenceUpdate { jid: jid(0), unavailable: false, last_seen_ms: 0 });
        let began = Instant::now();
        while followed.around().await != 1 {
            assert!(began.elapsed() < Duration::from_secs(20), "Person 000 never came online");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // The pings fail: nobody's known, and who was around shows as last heard.
        followed.say("connecting");
        followed.until_followed(0, "nobody known during the timeout").await;
        assert_eq!(followed.around().await, 1, "the last state heard stays, faint");
        until("the connection's twenty still followed on it, nobody wanted", || {
            let status = followed.following.status();
            (status.followed, status.wanted) == (20, 0)
        })
        .await;

        // They get through again: the same twenty are known at once, none asked for again.
        followed.say("online");
        followed.until_followed(20, "the twenty known again within the minute").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(asked(&followed.world).len(), 20, "none asked for again on the same connection");
        let status = followed.following.status();
        assert_eq!((status.followed, status.wanted, status.waiting), (20, 25, 5));
        assert_eq!(followed.around().await, 1);

        // The phone going offline leaves WhatsApp reachable, and everyone known.
        followed.say("phone_offline");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(followed.followed().await, 20, "the phone offline is no timeout");

        // A new connection starts again: who was around isn't any more, and the minute's twenty are
        // spent, so nobody's followed on it until the minute passes.
        followed.say("connecting");
        followed.ends();
        followed.say("online");
        let began = Instant::now();
        while followed.around().await != 0 {
            assert!(began.elapsed() < Duration::from_secs(20), "who was around was never forgotten");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        followed.until_followed(0, "nobody followed on the new connection").await;
        until("nobody counted followed on the new connection", || followed.following.status().followed == 0).await;
        assert_eq!(asked(&followed.world).len(), 20, "the minute's limit holds across connections");
        drop(followed.client);
    }

    /// However many keepalive timeouts a connection lives through, and wherever its windows have
    /// moved meanwhile, it follows at most `most` people; a new connection may follow as many again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_follows_at_most_its_most_across_keepalive_restores_however_its_windows_move() {
        let limits = Limits { most: 5, per_minute: 100, minute: Duration::from_millis(50), about_idle: ABOUT_IDLE };
        let mut followed = Followed::new(12, 6, limits).await;
        until("the connection's five", || asked(&followed.world).len() >= 5).await;
        followed.until_followed(5, "five followed").await;
        // The roster moves on to the rest, and the pings fail and get through three times.
        let rest: i64 = followed
            .store
            .reads()
            .query(crate::work::Priority::Interactive, |db| {
                Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [jid(6)], |r| r.get(0))?)
            })
            .await
            .unwrap();
        let moved = serde_json::json!({"watch": "w1", "around": format!("person:{rest}"), "before": 0, "after": 5});
        followed.client.call("watch.move", moved).await.unwrap();
        for _ in 0..3 {
            followed.say("connecting");
            followed.until_followed(0, "nobody known during the timeout").await;
            followed.say("online");
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        let asked_now: HashSet<String> = asked(&followed.world).into_iter().collect();
        assert_eq!(asked_now.len(), 5, "one connection follows at most five: {asked_now:?}");
        assert_eq!(followed.following.status().followed, 5);
        // A new connection follows the roster's five as it is now.
        followed.say("connecting");
        followed.ends();
        followed.say("online");
        until("the new connection's five", || asked(&followed.world).len() >= 10).await;
        followed.until_followed(5, "five followed on the new connection").await;
        let shown: HashSet<String> = (6..11).map(jid).collect();
        let latest: HashSet<String> = asked(&followed.world)[5..].iter().cloned().collect();
        assert_eq!(latest, shown, "the people the roster shows now");
    }

    /// sync counts a connection's end before its push says WhatsApp can't be reached, and
    /// whatsmeow reconnects by saying Connected with no Connecting before it, so that end is all
    /// that says the next connection is new: nobody's asked for on the connection that ended while
    /// the pushes still say it's reachable, and the next follows them again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_counted_ended_before_its_push_asks_for_nobody_and_the_next_follows_them_again() {
        let limits = Limits { per_minute: 100, ..Limits::default() };
        let followed = Followed::new(5, 1000, limits).await;
        until("the five", || asked(&followed.world).len() >= 5).await;
        followed.until_followed(5, "the five followed").await;

        // The connection ends, counted at once, its push still to come.
        followed.ends();
        followed.until_followed(0, "nobody known once the connection ended").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(asked(&followed.world).len(), 5, "nobody asked for on the connection that ended");
        assert_eq!(followed.followed().await, 0);

        // Its push, then the next connection's, with nothing counted between.
        followed.say("offline");
        followed.say("online");
        until("the five asked for on the next connection", || asked(&followed.world).len() >= 10).await;
        followed.until_followed(5, "the five followed on the next connection").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(asked(&followed.world).len(), 10, "each asked for once on the next connection");
    }
}
