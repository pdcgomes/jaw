//! Others' typing in chats, as WhatsApp says it while you're online, kept in jawd's memory only: it's
//! never written to jaw.db and moves no row's `v` and no `seq`, so it never reaches the writer, and
//! jawd starts with nobody typing. What the engine says (`heard`) and the messages that end it
//! (`message`) are taken without waiting into a bounded intake, in the order they came; what comes
//! while it's full is dropped and counted, and a message dropped so ends everyone's typing, since
//! whose it ended can't be known. One task takes them: typing fades `Timing::fade` after it was last
//! said, ends at once on `paused` and with a message from them in that chat, and ends for everyone
//! while WhatsApp hears you're away or can't be reached (`hearing`), since nothing more would come.
//! Whoever types is named as a message from them is, read once while they type. Each chat's typing
//! is told to connections at most once each `Timing::pace`: the first change after a quiet spell at
//! once, then the newest of those that follow, the rest merged into it; each connection's windows
//! then send their interface a `typing` push for the chats they show (`client`). While typing comes
//! fast the task takes it at most once each `BATCH`, so a busy group costs a round of its work for
//! many changes, and the chats told in one round reach connections as one.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jaw_proto::TypingStats;
use jaw_proto::windows::{Typist, TypistState};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::{Notify, watch};

use crate::convert;
use crate::engine::pb::chat_presence::State as Said;
use crate::store::{Store, read};
use crate::work::Priority;

/// How long typing lasts once it was last said, and the shortest time between two changes of a
/// chat's typing told to connections. Tests make them shorter.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub fade: Duration,
    pub pace: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing { fade: Duration::from_secs(25), pace: Duration::from_millis(50) }
    }
}

/// The most of the engine's typing, and of the messages that end it, waiting to be taken.
pub const INTAKE_MOST: usize = 1_024;
/// How many rounds of changes told a connection can fall behind by before it looks again at every
/// chat it shows.
const CHANGES_KEPT: usize = 1_024;
/// The shortest time between two of the task's rounds while typing keeps coming; a change after a
/// quiet spell is taken at once.
const BATCH: Duration = Duration::from_millis(5);
/// How often the chats and people read for typing that has ended are let go of.
const FORGET_EVERY: Duration = Duration::from_secs(1);

/// What waits in the intake, in the order it came.
enum Came {
    Said {
        chat: String,
        sender: String,
        state: Said,
    },
    /// A message in `chat` (without a device) from `senders`: whom it came from, and the other JID
    /// WhatsApp gave them by, empty when it gave none.
    Message {
        chat: String,
        senders: [String; 2],
    },
}

#[derive(Default)]
struct Intake {
    came: VecDeque<Came>,
    /// The chats where someone may be typing, by every JID WhatsApp may name them by, without
    /// devices: those typing came in since the task last looked, and those it has someone typing
    /// in. A message anywhere else isn't taken.
    watching: HashSet<String>,
    /// Whether a message was dropped, so whose typing it ended isn't known.
    lost: bool,
}

/// Who's typing in each chat: fed by the engine and by the messages sync takes, and told to the
/// feed's connections.
#[derive(Clone)]
pub struct Typing {
    inner: Arc<Inner>,
}

struct Inner {
    timing: Timing,
    intake: Mutex<Intake>,
    woken: Notify,
    /// What connections are told: each chat's typists, oldest first, for the chats where someone is,
    /// by the chat's id as its row has it.
    told: Mutex<HashMap<String, Arc<[Typist]>>>,
    /// The chats whose typing was told, each round's together.
    changes: broadcast::Sender<Arc<[Arc<str>]>>,
    heard: AtomicU64,
    dropped: AtomicU64,
    sent: AtomicU64,
    merged: AtomicU64,
}

impl Inner {
    fn count(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

impl Default for Typing {
    fn default() -> Self {
        Typing::new(Timing::default())
    }
}

impl Typing {
    pub fn new(timing: Timing) -> Typing {
        Typing::keeping(timing, CHANGES_KEPT)
    }

    /// The same, keeping `kept` rounds of changes told for a connection that falls behind, which
    /// tests make few.
    pub(super) fn keeping(timing: Timing, kept: usize) -> Typing {
        let inner = Inner {
            timing,
            intake: Mutex::default(),
            woken: Notify::new(),
            told: Mutex::default(),
            changes: broadcast::channel(kept).0,
            heard: AtomicU64::default(),
            dropped: AtomicU64::default(),
            sent: AtomicU64::default(),
            merged: AtomicU64::default(),
        };
        Typing { inner: Arc::new(inner) }
    }

    /// The engine's word that `sender` is typing (`Composing`) or recording in `chat`, or has
    /// stopped (`Paused`), as WhatsApp's chat presence says it. It never waits: while the intake is
    /// full it's dropped, and counted.
    pub fn heard(&self, chat: &str, sender: &str, state: Said) {
        let inner = &self.inner;
        Inner::count(&inner.heard);
        let mut intake = inner.intake.lock().expect("typing's intake");
        if intake.came.len() >= INTAKE_MOST {
            Inner::count(&inner.dropped);
            return;
        }
        intake.watching.insert(convert::user_jid(chat));
        intake.came.push_back(Came::Said { chat: chat.into(), sender: sender.into(), state });
        drop(intake);
        inner.woken.notify_one();
    }

    /// Takes word of a message from `sender`, known also as `alt` (empty when WhatsApp didn't say),
    /// in `chat`, which ends their typing there. Where nobody may be typing, it costs a look under a
    /// lock, and nothing at all while nobody types anywhere.
    pub fn message(&self, chat: &str, sender: &str, alt: &str) {
        let inner = &self.inner;
        let mut intake = inner.intake.lock().expect("typing's intake");
        if intake.watching.is_empty() {
            return;
        }
        let chat = convert::user_jid(chat);
        if !intake.watching.contains(&chat) {
            return;
        }
        if intake.came.len() >= INTAKE_MOST {
            intake.lost = true;
            return;
        }
        intake.came.push_back(Came::Message { chat, senders: [sender.into(), alt.into()] });
        drop(intake);
        inner.woken.notify_one();
    }

    /// Starts taking what the intake holds: `me` names your own devices, whose typing isn't shown,
    /// and `hearing` says whether WhatsApp says who's typing now: it has been told you're online, and
    /// can be reached.
    pub fn start(&self, store: Store, me: watch::Receiver<convert::Me>, hearing: watch::Receiver<bool>) {
        tokio::spawn(run(self.inner.clone(), store, me, hearing));
    }

    /// A connection's view of who's typing.
    pub fn watching(&self) -> Watching {
        Watching { inner: self.inner.clone(), changes: self.inner.changes.subscribe() }
    }

    pub fn stats(&self) -> TypingStats {
        let inner = &self.inner;
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        TypingStats {
            chats: inner.told.lock().expect("who's typing").len() as u64,
            heard: load(&inner.heard),
            dropped: load(&inner.dropped),
            waiting: inner.intake.lock().expect("typing's intake").came.len() as u64,
            waiting_most: INTAKE_MOST as u64,
            sent: load(&inner.sent),
            merged: load(&inner.merged),
        }
    }
}

/// A connection's view of who's typing: the chats whose typing was told since, and each chat's now.
pub struct Watching {
    inner: Arc<Inner>,
    changes: broadcast::Receiver<Arc<[Arc<str>]>>,
}

impl Watching {
    /// The next chats whose typing was told together, or how many rounds of them this fell behind
    /// by. Cancelling it loses nothing.
    pub async fn changed(&mut self) -> Result<Arc<[Arc<str>]>, RecvError> {
        match self.changes.recv().await {
            // Its sender lives as long as this does.
            Err(RecvError::Closed) => std::future::pending().await,
            other => other,
        }
    }

    /// Who's typing in `chat` now, as connections are told it; None when nobody is.
    pub fn now(&self, chat: &str) -> Option<Arc<[Typist]>> {
        self.inner.told.lock().expect("who's typing").get(chat).cloned()
    }

    /// A `typing` push went to the connection's interface.
    pub fn sent(&self) {
        Inner::count(&self.inner.sent);
    }

    /// A change waiting for the connection's interface gave way to a newer one.
    pub fn merged(&self) {
        Inner::count(&self.inner.merged);
    }
}

/// A person as their typing shows them.
struct Who {
    n: i64,
    /// `person:` and `n`, as the protocol names them.
    id: String,
    name: String,
    /// Every JID they're known by, without devices.
    jids: Vec<String>,
}

/// Someone typing in a chat.
struct Typer {
    who: Arc<Who>,
    state: TypistState,
    /// When it fades, unless it's said again first.
    until: Instant,
}

#[derive(Default)]
struct Chat {
    /// Oldest first.
    typers: Vec<Typer>,
    /// Every JID WhatsApp may name it by, without devices: those its typing came with, its own,
    /// and in a one-to-one chat its person's.
    jids: HashSet<String>,
    /// Whether a change waits to be told, and when its typing was last told.
    owed: bool,
    told_at: Option<Instant>,
}

impl Chat {
    fn owe(&mut self, inner: &Inner) {
        if std::mem::replace(&mut self.owed, true) {
            Inner::count(&inner.merged);
        }
    }

    /// Ends the typing of whoever `ends` says, owing the change if anyone's did, and says whether.
    fn end(&mut self, inner: &Inner, ends: impl Fn(&Typer) -> bool) -> bool {
        let before = self.typers.len();
        self.typers.retain(|typer| !ends(typer));
        let ended = self.typers.len() != before;
        if ended {
            self.owe(inner);
        }
        ended
    }
}

/// What the task keeps: each chat's typists, by the chat's id; and chats' ids and people by the
/// JIDs WhatsApp gives, read once while someone types there or as them, None when jaw has no such
/// chat or person.
#[derive(Default)]
struct Typists {
    chats: HashMap<String, Chat>,
    chat_ids: HashMap<String, Option<String>>,
    people: HashMap<String, Option<Arc<Who>>>,
    /// Whether someone started or stopped typing since the chats and people read were last pruned.
    moved: bool,
}

/// What typing a chat presence says: typing, recording, or none, for a pause.
fn typist_state(state: Said) -> Option<TypistState> {
    match state {
        Said::Composing => Some(TypistState::Typing),
        Said::Recording => Some(TypistState::Recording),
        Said::Paused | Said::Unspecified => None,
    }
}

impl Typists {
    /// Reads, in one go, the chats and people the typing in `came` names that aren't known yet. A
    /// read that fails leaves them out until they're said again.
    async fn name(&mut self, store: &Store, me: &convert::Me, came: &VecDeque<Came>) {
        let (mut chats, mut people) = (HashSet::new(), HashSet::new());
        for came in came {
            let Came::Said { chat, sender, state } = came else { continue };
            if typist_state(*state).is_none() || me.is(sender) {
                continue;
            }
            let (chat, sender) = (convert::user_jid(chat), convert::user_jid(sender));
            if !self.chat_ids.contains_key(&chat) {
                chats.insert(chat);
            }
            if !self.people.contains_key(&sender) {
                people.insert(sender);
            }
        }
        if chats.is_empty() && people.is_empty() {
            return;
        }
        let named = store
            .reads()
            .query(Priority::Interactive, move |db| {
                let chats = chats
                    .into_iter()
                    .map(|jid| Ok((read::chat_named(db, &jid)?, jid)))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let people = people
                    .into_iter()
                    .map(|jid| Ok((read::typist(db, &jid)?, jid)))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok((chats, people))
            })
            .await;
        match named {
            Ok((chats, people)) => {
                self.chat_ids.extend(chats.into_iter().map(|(id, jid)| (jid, id)));
                let people = people.into_iter().map(|(who, jid)| {
                    (jid, who.map(|(n, name, jids)| Arc::new(Who { n, id: format!("person:{n}"), name, jids })))
                });
                self.people.extend(people);
            }
            Err(e) => eprintln!("jawd: can't name who's typing, so they show once they say it again: {e:#}"),
        }
    }

    /// The chat WhatsApp names by `jid`, without a device, if someone types there or did lately.
    fn chat(&mut self, jid: &str) -> Option<&mut Chat> {
        let id = match self.chat_ids.get(jid) {
            Some(Some(id)) => id.clone(),
            _ => self.chats.iter().find(|(_, chat)| chat.jids.contains(jid)).map(|(id, _)| id.clone())?,
        };
        self.chats.get_mut(&id)
    }

    /// `sender` typing, recording or stopping in `chat`, as WhatsApp said it `now`.
    fn said(&mut self, chat: &str, sender: &str, state: Said, now: Instant, inner: &Inner) {
        let (chat, sender) = (convert::user_jid(chat), convert::user_jid(sender));
        let Some(state) = typist_state(state) else {
            // A pause ends their typing there, whichever of their JIDs it comes by.
            let person = self.people.get(&sender).cloned().flatten().map(|who| who.n);
            let ends = |typer: &Typer| person == Some(typer.who.n) || typer.who.jids.contains(&sender);
            self.moved |= self.chat(&chat).is_some_and(|chat| chat.end(inner, ends));
            return;
        };
        let Some(Some(id)) = self.chat_ids.get(&chat).cloned() else { return };
        let Some(Some(who)) = self.people.get(&sender).cloned() else { return };
        let typists = self.chats.entry(id.clone()).or_default();
        if convert::is_person(&id) {
            typists.jids.extend(who.jids.iter().cloned());
        }
        typists.jids.insert(chat);
        typists.jids.insert(id);
        let until = now + inner.timing.fade;
        match typists.typers.iter_mut().find(|typer| typer.who.n == who.n) {
            Some(typer) => {
                typer.until = until;
                if std::mem::replace(&mut typer.state, state) != state {
                    typists.owe(inner);
                }
            }
            None => {
                typists.typers.push(Typer { who, state, until });
                typists.owe(inner);
                self.moved = true;
            }
        }
    }

    /// A message from `senders` in `chat` ends their typing there, by the JIDs it names them by or
    /// the person they were read as.
    fn messaged(&mut self, chat: &str, senders: &[String; 2], inner: &Inner) {
        let senders: Vec<String> =
            senders.iter().filter(|jid| !jid.is_empty()).map(|jid| convert::user_jid(jid)).collect();
        let people: Vec<i64> =
            senders.iter().filter_map(|jid| self.people.get(jid).cloned().flatten()).map(|who| who.n).collect();
        let ends =
            |typer: &Typer| people.contains(&typer.who.n) || typer.who.jids.iter().any(|jid| senders.contains(jid));
        self.moved |= self.chat(chat).is_some_and(|chat| chat.end(inner, ends));
    }

    /// Everyone's typing ends, as when WhatsApp hears you're away or can't be reached.
    fn end_all(&mut self, inner: &Inner) {
        for chat in self.chats.values_mut() {
            self.moved |= chat.end(inner, |_| true);
        }
    }

    fn fade(&mut self, now: Instant, inner: &Inner) {
        for chat in self.chats.values_mut() {
            self.moved |= chat.end(inner, |typer| typer.until <= now);
        }
    }

    /// Tells connections each chat's typing that's owed and may change now. One that changed and
    /// changed back meanwhile tells nothing, and counts as merged.
    fn tell(&mut self, now: Instant, inner: &Inner) {
        let pace = inner.timing.pace;
        let mut told = inner.told.lock().expect("who's typing");
        let mut changed = Vec::new();
        for (id, chat) in &mut self.chats {
            if !chat.owed || chat.told_at.is_some_and(|at| now < at + pace) {
                continue;
            }
            chat.owed = false;
            let who: Vec<Typist> = chat
                .typers
                .iter()
                .map(|typer| Typist { id: typer.who.id.clone(), name: typer.who.name.clone(), state: typer.state })
                .collect();
            if told.get(id).map_or(who.is_empty(), |was| **was == *who) {
                Inner::count(&inner.merged);
                continue;
            }
            chat.told_at = Some(now);
            if who.is_empty() {
                told.remove(id);
            } else {
                told.insert(id.clone(), who.into());
            }
            changed.push(id.clone());
        }
        drop(told);
        if !changed.is_empty() {
            let _ = inner.changes.send(changed.into_iter().map(Arc::from).collect());
        }
        self.chats
            .retain(|_, chat| !chat.typers.is_empty() || chat.owed || chat.told_at.is_some_and(|at| now < at + pace));
    }

    /// Forgets the chats and people read for typing that has ended.
    fn forget(&mut self) {
        let typing: HashSet<&str> = self
            .chats
            .values()
            .flat_map(|chat| chat.typers.iter())
            .flat_map(|typer| typer.who.jids.iter().map(String::as_str))
            .collect();
        self.people.retain(|jid, _| typing.contains(jid.as_str()));
        let chats = &self.chats;
        let typed_in =
            |id: &Option<String>| id.as_ref().and_then(|id| chats.get(id)).is_some_and(|chat| !chat.typers.is_empty());
        self.chat_ids.retain(|_, id| typed_in(id));
    }

    /// The JIDs of the chats where someone types, for the messages that end it.
    fn watched(&self) -> HashSet<String> {
        self.chats.values().filter(|chat| !chat.typers.is_empty()).flat_map(|chat| chat.jids.iter().cloned()).collect()
    }

    /// When the next typist fades, or the next change owed may be told.
    fn due(&self, pace: Duration) -> Option<Instant> {
        let fades = self.chats.values().flat_map(|chat| chat.typers.iter().map(|typer| typer.until));
        let owed = self
            .chats
            .values()
            .filter(|chat| chat.owed)
            .map(|chat| chat.told_at.map_or_else(Instant::now, |at| at + pace));
        fades.chain(owed).min()
    }
}

async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at.into()).await,
        None => std::future::pending().await,
    }
}

async fn run(inner: Arc<Inner>, store: Store, me: watch::Receiver<convert::Me>, mut hearing: watch::Receiver<bool>) {
    let mut typists = Typists::default();
    let mut following = true;
    let mut round: Option<Instant> = None;
    let mut forgot = Instant::now();
    loop {
        // While it keeps coming, what comes waits for the next round with whatever comes meanwhile.
        if let Some(next) = round.map(|at| at + BATCH).filter(|next| *next > Instant::now()) {
            tokio::time::sleep_until(next.into()).await;
        }
        let due = typists.due(inner.timing.pace);
        tokio::select! {
            _ = inner.woken.notified() => {}
            changed = hearing.changed(), if following => following = changed.is_ok(),
            _ = until(due) => {}
        }
        round = Some(Instant::now());
        // As it is now, whatever woke this: what was said after you went away isn't taken. With
        // nothing left to say whether WhatsApp hears you, it doesn't.
        let on = following && *hearing.borrow_and_update();
        // A message dropped while the intake was full came after all it held then, which is all
        // that's taken now: nothing more was taken until this took it.
        let (came, lost) = {
            let mut intake = inner.intake.lock().expect("typing's intake");
            (std::mem::take(&mut intake.came), std::mem::take(&mut intake.lost))
        };
        if on && !came.is_empty() {
            let me = me.borrow().clone();
            typists.name(&store, &me, &came).await;
            let now = Instant::now();
            for came in &came {
                match came {
                    Came::Said { sender, .. } if me.is(sender) => {}
                    Came::Said { chat, sender, state } => typists.said(chat, sender, *state, now, &inner),
                    Came::Message { chat, senders } => typists.messaged(chat, senders, &inner),
                }
            }
        }
        if !on || lost {
            typists.end_all(&inner);
        }
        let now = Instant::now();
        typists.fade(now, &inner);
        // What's read for typing that has ended is let go of now and then rather than as it ends,
        // which costs a look at everyone typing, so someone who stops and starts again soon is
        // read once.
        if forgot.elapsed() >= FORGET_EVERY {
            typists.forget();
            forgot = Instant::now();
        }
        // The chats messages are looked for in are those of the typing about to be told, so a
        // message that comes once a connection has heard of it is taken. They change only as
        // someone starts or stops; between, typing said since only adds to them, which `heard`
        // does as it comes.
        if std::mem::take(&mut typists.moved) {
            let mut intake = inner.intake.lock().expect("typing's intake");
            let waiting: Vec<String> = intake
                .came
                .iter()
                .filter_map(|came| match came {
                    Came::Said { chat, .. } => Some(convert::user_jid(chat)),
                    Came::Message { .. } => None,
                })
                .collect();
            intake.watching = typists.watched();
            intake.watching.extend(waiting);
        }
        typists.tell(now, &inner);
    }
}

#[cfg(test)]
mod tests {
    use super::Said::{Composing, Paused, Recording};
    use super::*;
    use crate::store::{Change, Chat, Lane, Person};
    use jaw_proto::windows::TypistState::{Recording as RECORDING, Typing as TYPING};

    const ANA: &str = "351912345678@s.whatsapp.net";
    const ANA_LID: &str = "100000000001@lid";
    const RUI: &str = "351913333333@s.whatsapp.net";
    const ZE: &str = "351914444444@s.whatsapp.net";
    const ME: &str = "351910000001@s.whatsapp.net";
    const GROUP: &str = "120363012345678901@g.us";
    /// Long enough that nothing fades during a test that isn't about fading.
    const LONG: Duration = Duration::from_secs(120);
    /// The most a change takes to be told on a loaded machine.
    const WAIT: Duration = Duration::from_secs(15);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Ana, by her number and her LID, in her chat and in Lisbon Climbing with Rui and Zé; and you,
    /// whose typing isn't shown. WhatsApp hears you, and connections are told as `seen` is.
    struct Bed {
        _home: tempfile::TempDir,
        store: Store,
        typing: Typing,
        seen: Watching,
        hearing: watch::Sender<bool>,
        me: watch::Sender<convert::Me>,
    }

    impl Bed {
        async fn new(timing: Timing) -> Bed {
            let bed = Bed::idle(timing).await;
            bed.start();
            bed
        }

        /// The same, nothing taken from the intake until `start`.
        async fn idle(timing: Timing) -> Bed {
            let home = tempfile::tempdir().unwrap();
            let store = Store::open(home.path()).unwrap();
            let person = |jids: &[&str], name: Option<&str>, push: &str| {
                Change::Person(Person {
                    jids: jids.iter().map(|jid| jid.to_string()).collect(),
                    name: name.map(Into::into),
                    push_name: Some(push.into()),
                    ..Person::default()
                })
            };
            let changes = vec![
                person(&[ANA, ANA_LID], Some("Ana Ribeiro"), "Ana"),
                person(&[RUI], None, "Rui"),
                person(&[ZE], None, "Zé"),
                person(&[ME], None, "Pedro"),
                Change::Chat(Chat { id: ANA.into(), ..Chat::default() }),
                Change::Chat(Chat {
                    id: GROUP.into(),
                    kind: Some("group"),
                    name: Some("Lisbon Climbing".into()),
                    ..Chat::default()
                }),
            ];
            store.apply(Lane::Live, changes).await.unwrap();
            let typing = Typing::new(timing);
            let seen = typing.watching();
            let hearing = watch::channel(true).0;
            let me = watch::channel(convert::Me::new([ME.to_string()])).0;
            Bed { _home: home, store, typing, seen, hearing, me }
        }

        fn start(&self) {
            self.typing.start(self.store.clone(), self.me.subscribe(), self.hearing.subscribe());
        }

        /// Who connections are told is typing in `chat`, by name and state, oldest first.
        fn told(&self, chat: &str) -> Vec<(String, TypistState)> {
            told(&self.seen, chat)
        }

        /// Waits until connections are told `want` is typing in `chat`, and says when they were.
        async fn until(&mut self, chat: &str, want: &[(&str, TypistState)]) -> Instant {
            self.until_within(chat, want, WAIT).await
        }

        async fn until_within(&mut self, chat: &str, want: &[(&str, TypistState)], within: Duration) -> Instant {
            let want: Vec<(String, TypistState)> =
                want.iter().map(|(name, state)| (name.to_string(), *state)).collect();
            let end = Instant::now() + within;
            loop {
                if self.told(chat) == want {
                    return Instant::now();
                }
                let waited = tokio::time::timeout_at(end.into(), self.seen.changed()).await;
                assert!(waited.is_ok(), "{chat} was never told {want:?}; it's told {:?}", self.told(chat));
            }
        }
    }

    fn told(seen: &Watching, chat: &str) -> Vec<(String, TypistState)> {
        seen.now(chat).map(|who| who.iter().map(|t| (t.name.clone(), t.state)).collect()).unwrap_or_default()
    }

    #[test]
    fn typing_fades_25_s_after_it_was_last_said_is_told_at_most_every_50_ms_and_waits_1024_at_most() {
        let Timing { fade, pace } = Timing::default();
        assert_eq!((fade, pace, INTAKE_MOST), (Duration::from_secs(25), Duration::from_millis(50), 1_024));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn typing_is_told_with_whoever_types_named_as_their_messages_are_and_by_person() {
        let mut bed = Bed::new(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(GROUP, ANA_LID, Composing);
        bed.typing.heard(GROUP, "351913333333:12@s.whatsapp.net", Recording);
        bed.until(GROUP, &[("Ana Ribeiro", TYPING), ("Rui", RECORDING)]).await;
        let who = bed.seen.now(GROUP).unwrap();
        let ana: i64 = bed
            .store
            .reads()
            .query(Priority::Interactive, |db| {
                Ok(db.query_row("SELECT person FROM person_jids WHERE jid = ?1", [ANA], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert_eq!(who[0].id, format!("person:{ana}"), "whichever JID she types by, she's her person");
        // A LID's chat is the one with the same person under their number.
        bed.typing.heard(ANA_LID, ANA_LID, Composing);
        bed.until(ANA, &[("Ana Ribeiro", TYPING)]).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn typing_fades_once_it_has_gone_unsaid_for_its_fade_and_saying_it_again_keeps_it() {
        let fade = ms(2_000);
        let mut bed = Bed::new(Timing { fade, pace: ms(20) }).await;
        bed.typing.heard(ANA, ANA, Composing);
        bed.until(ANA, &[("Ana Ribeiro", TYPING)]).await;
        tokio::time::sleep(ms(500)).await;
        let said = Instant::now();
        bed.typing.heard(ANA, ANA, Composing);
        let faded = bed.until_within(ANA, &[], fade + WAIT).await;
        assert!(faded >= said + fade, "it faded {:?} after it was last said, sooner than {fade:?}", faded - said);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pause_ends_their_typing_at_once_and_only_theirs() {
        let mut bed = Bed::new(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(GROUP, RUI, Composing);
        bed.typing.heard(GROUP, ZE, Recording);
        bed.until(GROUP, &[("Rui", TYPING), ("Zé", RECORDING)]).await;
        bed.typing.heard(GROUP, RUI, Paused);
        bed.until(GROUP, &[("Zé", RECORDING)]).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_from_them_ends_their_typing_in_that_chat_alone_whichever_jid_it_names_them_by() {
        let mut bed = Bed::new(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(ANA, ANA, Composing);
        bed.typing.heard(GROUP, ANA, Composing);
        bed.typing.heard(GROUP, RUI, Recording);
        bed.until(ANA, &[("Ana Ribeiro", TYPING)]).await;
        bed.until(GROUP, &[("Ana Ribeiro", TYPING), ("Rui", RECORDING)]).await;
        // Hers in the group ends her typing there, and nobody else's, nor hers in her own chat.
        bed.typing.message(GROUP, "351912345678:3@s.whatsapp.net", "");
        bed.until(GROUP, &[("Rui", RECORDING)]).await;
        assert_eq!(bed.told(ANA), [("Ana Ribeiro".to_string(), TYPING)]);
        // Hers in her own chat, as WhatsApp addresses it by her LID, ends it there.
        bed.typing.message(ANA_LID, ANA_LID, ANA);
        bed.until(ANA, &[]).await;
        assert_eq!(bed.told(GROUP), [("Rui".to_string(), RECORDING)]);
        // Rui's by a LID jaw doesn't know him by, with the number he typed by beside it, ends his.
        bed.typing.message(GROUP, "100000000003@lid", RUI);
        bed.until(GROUP, &[]).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn everyones_typing_ends_while_whatsapp_hears_youre_away_and_what_comes_meanwhile_isnt_taken() {
        let mut bed = Bed::new(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(ANA, ANA, Composing);
        bed.typing.heard(GROUP, RUI, Recording);
        bed.until(ANA, &[("Ana Ribeiro", TYPING)]).await;
        bed.until(GROUP, &[("Rui", RECORDING)]).await;
        bed.hearing.send_replace(false);
        bed.until(ANA, &[]).await;
        bed.until(GROUP, &[]).await;
        // Said while you're away, as WhatsApp wouldn't, it's let go of once it's taken.
        bed.typing.heard(ANA, ANA, Composing);
        let end = Instant::now() + WAIT;
        while bed.typing.stats().waiting > 0 {
            assert!(Instant::now() < end, "the typing said while away was never taken");
            tokio::time::sleep(ms(5)).await;
        }
        bed.hearing.send_replace(true);
        bed.typing.heard(GROUP, ZE, Composing);
        bed.until(GROUP, &[("Zé", TYPING)]).await;
        assert_eq!(bed.told(ANA), [], "what was said while away doesn't come back");
        // And so does all of it once nothing says whether WhatsApp hears you.
        drop(std::mem::replace(&mut bed.hearing, watch::channel(true).0));
        bed.until(GROUP, &[]).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn your_own_devices_typing_and_typing_jaw_cant_place_arent_shown() {
        let mut bed = Bed::new(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(ANA, ME, Composing);
        bed.typing.heard(GROUP, "351910000001:4@s.whatsapp.net", Recording);
        // Someone jaw has no person for, and a chat it doesn't have.
        bed.typing.heard(GROUP, "351919999999@s.whatsapp.net", Composing);
        bed.typing.heard("120363000000000009@g.us", RUI, Composing);
        bed.typing.heard(GROUP, RUI, Composing);
        bed.until(GROUP, &[("Rui", TYPING)]).await;
        assert_eq!(bed.told(ANA), []);
        assert_eq!(bed.typing.stats().chats, 1, "{:?}", bed.typing.stats());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_chats_typing_is_told_at_most_once_a_pace_the_first_change_at_once_and_the_rest_merged() {
        let pace = ms(1_000);
        let mut bed = Bed::new(Timing { fade: LONG, pace }).await;
        let first = Instant::now();
        bed.typing.heard(GROUP, RUI, Composing);
        let told = bed.until(GROUP, &[("Rui", TYPING)]).await;
        assert!(told - first < pace / 2, "the first change after a quiet spell waited {:?}", told - first);
        // A burst in which each change differs from the one before, one every 20 ms for 3 s, in
        // whole rounds of four: Zé typing, Rui recording, Zé stopping, Rui typing again.
        let typing = bed.typing.clone();
        let burst = tokio::spawn(async move {
            let began = Instant::now();
            let mut n = 0u64;
            while began.elapsed() < ms(3_000) || !n.is_multiple_of(4) {
                let (who, state) = match n % 4 {
                    0 => (ZE, Composing),
                    1 => (RUI, Recording),
                    2 => (ZE, Paused),
                    _ => (RUI, Composing),
                };
                typing.heard(GROUP, who, state);
                n += 1;
                tokio::time::sleep(ms(20)).await;
            }
            (n, began)
        });
        // Every change told until a pace and a half has passed with none once the burst is over.
        let (mut tells, mut last) = (0u64, Instant::now());
        loop {
            match tokio::time::timeout(pace * 3 / 2, bed.seen.changed()).await {
                Ok(Ok(chats)) if chats.iter().all(|chat| &**chat == GROUP) => {
                    (tells, last) = (tells + 1, Instant::now())
                }
                Ok(other) => panic!("{other:?}"),
                Err(_) if burst.is_finished() => break,
                Err(_) => {}
            }
        }
        let (changes, began) = burst.await.unwrap();
        assert_eq!(bed.told(GROUP), [("Rui".to_string(), TYPING)], "it ends on the newest");
        let lasted = last.saturating_duration_since(began);
        let most = (lasted.as_millis() / pace.as_millis()) as u64 + 2;
        assert!(tells <= most, "{tells} changes told in {lasted:?}, at most {most} at one a {pace:?}");
        let stats = bed.typing.stats();
        assert_eq!((stats.heard, stats.dropped), (changes + 1, 0), "{stats:?}");
        assert!(stats.merged >= changes - most, "{changes} changes, {tells} told, {stats:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn what_comes_while_the_intake_is_full_is_dropped_and_counted_and_a_message_dropped_ends_everyones_typing() {
        let mut bed = Bed::idle(Timing { fade: LONG, pace: ms(20) }).await;
        bed.typing.heard(GROUP, RUI, Composing);
        for n in 1..INTAKE_MOST {
            bed.typing.heard(GROUP, ZE, if n % 2 == 0 { Composing } else { Recording });
        }
        let full = bed.typing.stats();
        assert_eq!((full.waiting, full.waiting_most, full.dropped), (INTAKE_MOST as u64, INTAKE_MOST as u64, 0));
        // Zé stopping comes too fast, and so does a message of Rui's, which would have ended his.
        bed.typing.heard(GROUP, ZE, Paused);
        bed.typing.message(GROUP, RUI, "");
        let stats = bed.typing.stats();
        assert_eq!((stats.heard, stats.dropped, stats.waiting), (INTAKE_MOST as u64 + 1, 1, INTAKE_MOST as u64));
        bed.start();
        let end = Instant::now() + WAIT;
        while bed.typing.stats().waiting > 0 {
            assert!(Instant::now() < end, "what the intake held was never taken");
            tokio::time::sleep(ms(5)).await;
        }
        // Once what was held is taken, nobody is left typing there, as nobody can say whose
        // typing the message ended. Ana's typing, taken after it, says it has been.
        bed.typing.heard(ANA, ANA, Composing);
        bed.until(ANA, &[("Ana Ribeiro", TYPING)]).await;
        assert_eq!(bed.told(GROUP), []);
    }
}
