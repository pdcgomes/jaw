//! `jaw demo`: a made-up account, for trying jaw without one, for snapshots and for the tests. It
//! plays jawd's part as the protocol describes it: it answers watches with windows of rows,
//! pushes upserts as messages arrive and their ticks move, and follows the newest only while the
//! window does. Live, a thread of its own has people type and write; a flood sends messages as
//! fast as asked.

use crate::data::{
    Around, Chat, ChatKind, Jawd, Kind, Last, Message, Quote, Reaction, Sender, Span, SpanKind, Tick, Typing, Update,
};
use crate::fmt::{DAY, MINUTE};
use crate::run::Event;
use jaw_proto::{Connection, ConnectionState, LinkStep};
use std::collections::HashMap;
use std::sync::mpsc::Sender as Channel;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// The account's own person.
pub const ME: &str = "person:0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum State {
    /// Linked and online.
    Online,
    /// Not linked yet: the QR code, or a code typed on the phone.
    Linking,
    /// jaw starting jawd.
    Starting,
    CatchingUp,
    PhoneOffline,
    Offline,
    Reconnecting,
    LoggedOut,
    UpdateNeeded,
    Banned,
    /// jawd out of reach.
    Unreachable,
    /// Linked, with no chats yet.
    New,
}

#[derive(Clone, Copy, Debug)]
pub enum Clock {
    Fixed(i64),
    Wall,
}

impl Clock {
    fn now(self) -> i64 {
        match self {
            Clock::Fixed(at) => at,
            Clock::Wall => chrono::Utc::now().timestamp_millis(),
        }
    }
}

struct Thread {
    chat: Chat,
    messages: Vec<Message>,
}

/// The open chat's window, as jawd keeps it for an interface.
struct Open {
    chat: String,
    following: bool,
    before: usize,
    /// The first message in the window, and one past the last.
    lo: usize,
    hi: usize,
    newer: u32,
}

enum Later {
    Tick { chat: String, id: String, tick: Tick },
    Beat(usize),
    Qr,
    Linked,
    CatchUp,
    Retry,
    Started,
}

pub struct Account {
    tx: Channel<Event>,
    clock: Clock,
    state: State,
    threads: Vec<Thread>,
    index: HashMap<String, usize>,
    open: Option<Open>,
    seq: u64,
    connection: Connection,
    later: Vec<(i64, Later)>,
    live: bool,
    gone: bool,
    qr: u32,
}

struct Shared {
    account: Mutex<Account>,
    wake: Condvar,
}

/// The made-up account as the interface's jawd.
#[derive(Clone)]
pub struct Demo {
    shared: Arc<Shared>,
}

impl Demo {
    /// The account in `state`, its first updates already on `tx`. Live, its people keep writing.
    pub fn start(tx: Channel<Event>, clock: Clock, state: State, live: bool) -> Demo {
        let now = clock.now();
        let threads = if state == State::New { vec![] } else { made_up(now) };
        let index = threads.iter().enumerate().map(|(i, t)| (t.chat.id.clone(), i)).collect();
        let seq = threads.iter().map(|t| t.messages.len() as u64).sum();
        let connection = match state {
            State::CatchingUp => {
                Connection { done: Some(1_204), total: Some(5_000), ..Connection::new(ConnectionState::CatchingUp) }
            }
            State::PhoneOffline => Connection::new(ConnectionState::PhoneOffline),
            State::Offline => Connection::new(ConnectionState::Offline),
            State::Reconnecting => Connection::new(ConnectionState::EngineRestarting),
            State::LoggedOut => Connection::new(ConnectionState::LoggedOut),
            State::UpdateNeeded => Connection::new(ConnectionState::UpdateNeeded),
            State::Banned => Connection { until: Some(now + 2 * DAY), ..Connection::new(ConnectionState::Banned) },
            State::Linking => Connection::new(ConnectionState::Connecting),
            _ => Connection::new(ConnectionState::Online),
        };
        let account = Account {
            tx,
            clock,
            state,
            threads,
            index,
            open: None,
            seq,
            connection,
            later: vec![],
            live,
            gone: false,
            qr: 0,
        };
        let demo = Demo { shared: Arc::new(Shared { account: Mutex::new(account), wake: Condvar::new() }) };
        {
            let mut a = demo.lock();
            match state {
                State::Starting => {
                    a.send(Update::Starting { failed: None });
                    a.after(2_500, Later::Started);
                }
                State::Unreachable => a.unreachable(),
                _ => a.hello(),
            }
            match state {
                State::CatchingUp => a.after(400, Later::CatchUp),
                State::Online => a.after(3_000, Later::Beat(0)),
                _ => {}
            }
        }
        if live {
            let shared = demo.shared.clone();
            std::thread::Builder::new()
                .name("jaw-demo".into())
                .spawn(move || live_thread(&shared))
                .expect("the demo's thread");
        }
        demo
    }

    fn lock(&self) -> MutexGuard<'_, Account> {
        self.shared.account.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn wake(&self) {
        self.shared.wake.notify_all();
    }

    /// Sends `rate` messages a second for `seconds`, half into the open chat and half across the
    /// others, from a thread of its own.
    pub fn flood(&self, rate: u32, seconds: f64) -> std::thread::JoinHandle<u64> {
        let demo = self.clone();
        std::thread::Builder::new()
            .name("jaw-flood".into())
            .spawn(move || {
                let start = std::time::Instant::now();
                let mut sent = 0u64;
                let mut rng = 0x5eed_u64;
                while start.elapsed().as_secs_f64() < seconds {
                    let due = (start.elapsed().as_secs_f64() * rate as f64) as u64;
                    if sent < due {
                        let mut a = demo.lock();
                        if a.gone {
                            break;
                        }
                        while sent < due {
                            a.flooded(sent, &mut rng);
                            sent += 1;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                sent
            })
            .expect("the flood's thread")
    }
}

fn live_thread(shared: &Shared) {
    let mut a = shared.account.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let now = a.now();
        let due: Vec<Later> = {
            let (due, rest): (Vec<_>, Vec<_>) =
                std::mem::take(&mut a.later).into_iter().partition(|(at, _)| *at <= now);
            a.later = rest;
            due.into_iter().map(|(_, l)| l).collect()
        };
        for l in due {
            a.run(l);
        }
        if a.gone {
            return;
        }
        let next = a.later.iter().map(|(at, _)| *at).min();
        a = match next {
            Some(at) => {
                let wait = Duration::from_millis((at - a.now()).max(1) as u64);
                shared.wake.wait_timeout(a, wait).unwrap_or_else(|e| e.into_inner()).0
            }
            None => shared.wake.wait(a).unwrap_or_else(|e| e.into_inner()),
        };
    }
}

/// Something one of the made-up account's people says while it's live.
struct Beat {
    /// After the beat before, in milliseconds.
    after: i64,
    chat: &'static str,
    who: (&'static str, &'static str, u8),
    /// Whether they're seen typing first.
    types: bool,
    text: &'static str,
}

const BEATS: &[Beat] = &[
    Beat {
        after: 3_000, chat: "ana", who: ("person:1", "Ana", 0), types: true, text: "já agora, levas tu o *vinho*?"
    },
    Beat {
        after: 5_000,
        chat: "familia",
        who: ("person:3", "Rui", 1),
        types: false,
        text: "chego às 20h, guardem-me um lugar",
    },
    Beat {
        after: 6_000,
        chat: "climbing",
        who: ("person:20", "Marta", 2),
        types: false,
        text: "alguém para sábado às 9?",
    },
    Beat {
        after: 4_000,
        chat: "escola",
        who: ("person:30", "Prof. Helena", 4),
        types: false,
        text: "Lembrete: visita de estudo _amanhã_ às 9h",
    },
    Beat {
        after: 7_000,
        chat: "ana",
        who: ("person:1", "Ana", 0),
        types: true,
        text: "ou compro eu no caminho, diz-me",
    },
    Beat { after: 6_000, chat: "familia", who: ("person:4", "Inês", 2), types: false, text: "@Pedro trazes a avó?" },
    Beat {
        after: 9_000,
        chat: "climbing",
        who: ("person:21", "João", 3),
        types: true,
        text: "eu vou! levo a corda de 70",
    },
];

impl Account {
    fn now(&self) -> i64 {
        self.clock.now()
    }

    fn send(&mut self, u: Update) {
        if self.tx.send(Event::Update(u)).is_err() {
            self.gone = true;
        }
    }

    fn after(&mut self, ms: i64, l: Later) {
        if self.live {
            let at = self.now() + ms;
            self.later.push((at, l));
        }
    }

    fn hello(&mut self) {
        let linked = self.state != State::Linking;
        self.send(Update::Hello { linked, me: Some(ME.into()), connection: self.connection.clone() });
    }

    fn unreachable(&mut self) {
        let why = "jawd isn't running: nothing answers on jaw.sock".to_string();
        let retry_at = self.now() + 4_000;
        self.send(Update::Unreachable { why, retry_at });
        self.after(4_000, Later::Retry);
    }

    fn run(&mut self, l: Later) {
        match l {
            Later::Tick { chat, id, tick } => self.tick(&chat, &id, tick),
            Later::Beat(i) => self.beat(i),
            Later::Qr => {
                if matches!(self.state, State::Linking) {
                    self.qr += 1;
                    self.send_qr();
                }
            }
            Later::Linked => {
                self.state = State::Online;
                self.connection = Connection::new(ConnectionState::Online);
                self.send(Update::Link(LinkStep::Done { phone: "+351 912 000 000".into() }));
                self.send(Update::Connection(self.connection.clone()));
            }
            Later::CatchUp => {
                let (done, total) = (self.connection.done.unwrap_or(0) + 173, self.connection.total.unwrap_or(0));
                if done >= total {
                    self.connection = Connection::new(ConnectionState::Online);
                } else {
                    self.connection.done = Some(done);
                    self.after(400, Later::CatchUp);
                }
                self.send(Update::Connection(self.connection.clone()));
            }
            Later::Retry => self.unreachable(),
            Later::Started => {
                self.state = State::Online;
                self.hello();
                self.after(3_000, Later::Beat(0));
            }
        }
    }

    fn beat(&mut self, i: usize) {
        let beat = &BEATS[i % BEATS.len()];
        let next = BEATS[(i + 1) % BEATS.len()].after;
        let sender = Sender { id: beat.who.0.into(), name: beat.who.1.into(), colour: beat.who.2 };
        let Some(&t) = self.index.get(beat.chat) else { return };
        if beat.types && self.threads[t].chat.typing.is_empty() {
            self.set_typing(t, Some(sender.name.clone()));
            self.later.push((self.now() + 2_500, Later::Beat(i)));
            return;
        }
        self.set_typing(t, None);
        let at = self.now();
        let m = self.message(at, Some(sender), markup(beat.text), None);
        self.arrive(t, m);
        self.after(next, Later::Beat(i + 1));
    }

    fn set_typing(&mut self, t: usize, who: Option<String>) {
        let chat = &mut self.threads[t].chat;
        chat.typing = who.into_iter().collect();
        chat.v += 1;
        self.push_chat(t);
    }

    fn message(&mut self, at: i64, from: Option<Sender>, spans: Vec<Span>, status: Option<Tick>) -> Message {
        self.seq += 1;
        Message {
            id: format!("3EB0JAW{:09}", self.seq),
            v: 1,
            sort: format!("{at:013}~{:09}", self.seq),
            at,
            mine: from.is_none(),
            from,
            kind: Kind::Text,
            spans,
            reply: None,
            reactions: vec![],
            status,
            edited: false,
            deleted: false,
            summary: None,
            system: None,
        }
    }

    fn total(&self) -> usize {
        self.threads.len()
    }

    fn push_chat(&mut self, t: usize) {
        let row = self.threads[t].chat.clone();
        let total = self.total();
        self.send(Update::Chats { reset: false, upsert: vec![row], remove: vec![], total });
    }

    /// A message comes into chat `t`: its row moves up, and the open window takes it if it
    /// follows the newest, or counts it as newer if it doesn't.
    fn arrive(&mut self, t: usize, m: Message) {
        let thread = &mut self.threads[t];
        let mention = m.spans.iter().any(|s| s.t == SpanKind::Mention && s.person.as_deref() == Some(ME));
        thread.chat.last = Some(Last {
            from: m.from.as_ref().filter(|_| thread.chat.kind == ChatKind::Group).map(|f| f.name.clone()),
            summary: crate::fmt::one_line(&m.spans.iter().map(|s| s.s.as_str()).collect::<String>()),
            status: m.status,
            at: m.at,
        });
        thread.chat.sort = format!("{}~{:013}", u8::from(thread.chat.pinned), m.at);
        if !m.mine {
            thread.chat.unread += 1;
            thread.chat.mentions += u32::from(mention);
        } else {
            thread.chat.unread = 0;
            thread.chat.mentions = 0;
            thread.chat.draft = None;
        }
        thread.chat.v += 1;
        thread.messages.push(m.clone());
        let len = thread.messages.len();
        let chat = thread.chat.id.clone();
        self.push_chat(t);
        let Some(open) = self.open.as_mut().filter(|o| o.chat == chat) else { return };
        if open.following {
            open.hi = len;
            let mut remove = vec![];
            if open.hi - open.lo > open.before {
                remove.push(self.threads[t].messages[open.lo].id.clone());
                open.lo += 1;
            }
            let older = open.lo > 0;
            self.send(Update::Messages {
                chat,
                reset: false,
                upsert: vec![m],
                remove,
                older: Some(older),
                newer: None,
            });
        } else {
            open.newer += 1;
            let newer = open.newer;
            self.send(Update::Messages {
                chat,
                reset: false,
                upsert: vec![],
                remove: vec![],
                older: None,
                newer: Some(newer),
            });
        }
    }

    fn tick(&mut self, chat: &str, id: &str, tick: Tick) {
        let Some(&t) = self.index.get(chat) else { return };
        let thread = &mut self.threads[t];
        let Some(i) = thread.messages.iter().position(|m| m.id == id) else { return };
        let m = &mut thread.messages[i];
        m.status = Some(tick);
        m.v += 1;
        let m = m.clone();
        if i + 1 == thread.messages.len()
            && let Some(last) = thread.chat.last.as_mut()
        {
            last.status = Some(tick);
            thread.chat.v += 1;
            self.push_chat(t);
        }
        if let Some(open) = self.open.as_ref().filter(|o| o.chat == chat && (o.lo..o.hi).contains(&i)) {
            let chat = open.chat.clone();
            self.send(Update::Messages {
                chat,
                reset: false,
                upsert: vec![m],
                remove: vec![],
                older: None,
                newer: None,
            });
        }
    }

    /// A flood's `n`th message: every other one into the open chat.
    fn flooded(&mut self, n: u64, rng: &mut u64) {
        if self.threads.is_empty() {
            return;
        }
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let open = self.open.as_ref().and_then(|o| self.index.get(&o.chat).copied());
        let t = match open {
            Some(t) if n.is_multiple_of(2) => t,
            _ => (*rng >> 33) as usize % self.threads.len(),
        };
        let who = (*rng >> 20) as usize % NAMES.len();
        let sender = Sender { id: format!("person:{}", 100 + who), name: NAMES[who].into(), colour: who as u8 };
        let words: Vec<&str> =
            (0..3 + (*rng >> 40) as usize % 12).map(|k| WORDS[(*rng as usize >> (k % 16)) % WORDS.len()]).collect();
        let at = self.now();
        let m =
            self.message(at, Some(sender), vec![Span { t: SpanKind::Text, s: words.join(" "), person: None }], None);
        self.arrive(t, m);
    }

    fn send_qr(&mut self) {
        let code = format!("2@jaw-demo-{:04},made-up,not-a-real-code,{}", self.qr, "x".repeat(8));
        let expires = self.now() + 20_000;
        self.send(Update::Link(LinkStep::Qr { code, expires }));
        self.after(20_000, Later::Qr);
    }
}

impl Jawd for Demo {
    fn watch_chats(&mut self, _around: Option<&str>, _before: usize, _after: usize) {
        let mut a = self.lock();
        let rows = a.threads.iter().map(|t| t.chat.clone()).collect();
        let total = a.total();
        a.send(Update::Chats { reset: true, upsert: rows, remove: vec![], total });
    }

    fn watch_messages(&mut self, chat: &str, around: Around, before: usize, after: usize) {
        let mut a = self.lock();
        let Some(&t) = a.index.get(chat) else { return };
        let messages = &a.threads[t].messages;
        let len = messages.len();
        let (lo, hi, following) = match &around {
            Around::Newest => (len.saturating_sub(before), len, true),
            Around::Message(id) => match messages.iter().position(|m| &m.id == id) {
                Some(i) => (i.saturating_sub(before), (i + after + 1).min(len), false),
                None => (len.saturating_sub(before), len, true),
            },
        };
        let rows = messages[lo..hi].to_vec();
        a.open = Some(Open { chat: chat.into(), following, before, lo, hi, newer: 0 });
        a.send(Update::Messages {
            chat: chat.into(),
            reset: true,
            upsert: rows,
            remove: vec![],
            older: Some(lo > 0),
            newer: Some(0),
        });
    }

    fn send_text(&mut self, chat: &str, text: &str) {
        let mut a = self.lock();
        let Some(&t) = a.index.get(chat) else { return };
        let at = a.now();
        let m = a.message(at, None, markup(text), Some(Tick::Pending));
        let (chat, id) = (chat.to_string(), m.id.clone());
        a.arrive(t, m);
        if a.connection.state == ConnectionState::Offline {
            a.send(Update::Notice("offline: it goes when WhatsApp is back".into()));
            return;
        }
        a.after(400, Later::Tick { chat: chat.clone(), id: id.clone(), tick: Tick::Sent });
        a.after(1_200, Later::Tick { chat: chat.clone(), id: id.clone(), tick: Tick::Delivered });
        a.after(3_500, Later::Tick { chat, id, tick: Tick::Read });
        drop(a);
        self.wake();
    }

    fn draft(&mut self, chat: &str, text: &str) {
        let mut a = self.lock();
        let Some(&t) = a.index.get(chat) else { return };
        let c = &mut a.threads[t].chat;
        c.draft = (!text.is_empty()).then(|| text.to_string());
        c.v += 1;
        a.push_chat(t);
    }

    fn read(&mut self, chat: &str, _up_to: &str) {
        let mut a = self.lock();
        let Some(&t) = a.index.get(chat) else { return };
        let c = &mut a.threads[t].chat;
        if c.unread > 0 || c.mentions > 0 {
            c.unread = 0;
            c.mentions = 0;
            c.v += 1;
            a.push_chat(t);
        }
    }

    fn typing(&mut self, _chat: &str, _state: Typing) {}

    fn focus(&mut self, _focused: bool) {}

    fn link_start(&mut self, phone: Option<String>) {
        let mut a = self.lock();
        match phone {
            None => a.send_qr(),
            Some(_) => {
                a.send(Update::Link(LinkStep::Code { code: "K4PM-7XQ2".into() }));
                a.after(8_000, Later::Linked);
            }
        }
        drop(a);
        self.wake();
    }
}

/// WhatsApp's own formatting into spans, as jawd's converter gives them: *bold*, _italic_,
/// ~strike~ and `mono`, links, and @Pedro as a mention of you.
pub fn markup(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = vec![];
    let mut push = |t: SpanKind, s: &str, person: Option<&str>| {
        if s.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.t == t && t == SpanKind::Text => last.s.push_str(s),
            _ => out.push(Span { t, s: s.into(), person: person.map(String::from) }),
        }
    };
    let mut rest = text;
    while !rest.is_empty() {
        let next = rest.find(['*', '_', '~', '`', '@', 'h']).unwrap_or(rest.len());
        push(SpanKind::Text, &rest[..next], None);
        rest = &rest[next..];
        let Some(c) = rest.chars().next() else { break };
        if c == 'h' && (rest.starts_with("https://") || rest.starts_with("http://")) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            push(SpanKind::Link, &rest[..end], None);
            rest = &rest[end..];
            continue;
        }
        if c == '@' && rest.starts_with("@Pedro") {
            push(SpanKind::Mention, "@Pedro", Some(ME));
            rest = &rest["@Pedro".len()..];
            continue;
        }
        let kind = match c {
            '*' => SpanKind::Bold,
            '_' => SpanKind::Italic,
            '~' => SpanKind::Strike,
            '`' => SpanKind::Mono,
            _ => SpanKind::Text,
        };
        match rest[1..].find(c).filter(|_| kind != SpanKind::Text) {
            Some(end) if end > 0 => {
                push(kind, &rest[1..1 + end], None);
                rest = &rest[end + 2..];
            }
            _ => {
                push(SpanKind::Text, &rest[..c.len_utf8()], None);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

const NAMES: &[&str] = &["Ana", "Rui", "Inês", "Sofia", "Tiago", "Marta", "João", "Leonor"];
const WORDS: &[&str] = &[
    "sim", "não", "amanhã", "hoje", "jantar", "vinho", "praia", "comboio", "às", "nove", "quem", "vem", "leva", "o",
    "a", "bolo", "fotos", "ok", "boa", "ideia", "chego", "tarde", "cedo", "logo", "depois", "falamos", "obrigado",
];

/// A chat being written: messages added oldest first.
struct Writing {
    thread: Thread,
    now: i64,
    seq: u64,
}

fn person(id: &str, name: &str, colour: u8) -> Sender {
    Sender { id: id.into(), name: name.into(), colour }
}

impl Writing {
    fn new(id: &str, name: &str, kind: ChatKind, now: i64) -> Writing {
        let chat = Chat {
            id: id.into(),
            v: 1,
            sort: format!("0~{:013}", now - 2 * DAY),
            kind,
            name: name.into(),
            last: None,
            unread: 0,
            mentions: 0,
            pinned: false,
            muted_until: None,
            draft: None,
            typing: vec![],
        };
        Writing { thread: Thread { chat, messages: vec![] }, now, seq: 0 }
    }

    /// A message `ago` minutes before now.
    fn add(&mut self, ago: f64, from: Option<&Sender>, kind: Kind, text: &str) -> &mut Message {
        self.seq += 1;
        let at = self.now - (ago * MINUTE as f64) as i64;
        let id = format!("{}-{:05}", self.thread.chat.id.split('@').next().unwrap_or("chat"), self.seq);
        self.thread.messages.push(Message {
            id,
            v: 1,
            sort: format!("{at:013}~{:09}", self.seq),
            at,
            from: from.cloned(),
            mine: from.is_none(),
            kind,
            spans: markup(text),
            reply: None,
            reactions: vec![],
            status: from.is_none().then_some(Tick::Read),
            edited: false,
            deleted: false,
            summary: None,
            system: None,
        });
        self.thread.messages.last_mut().expect("just added")
    }

    fn theirs(&mut self, ago: f64, from: &Sender, text: &str) -> &mut Message {
        self.add(ago, Some(from), Kind::Text, text)
    }

    fn mine(&mut self, ago: f64, text: &str) -> &mut Message {
        self.add(ago, None, Kind::Text, text)
    }

    fn other(&mut self, ago: f64, from: Option<&Sender>, kind: Kind, summary: &str, caption: &str) -> &mut Message {
        let m = self.add(ago, from, kind, caption);
        m.summary = Some(summary.into());
        m
    }

    fn system(&mut self, ago: f64, kind: Kind, said: &str) {
        let m = self.add(ago, None, kind, "");
        m.mine = false;
        m.status = None;
        m.system = Some(said.into());
    }

    /// The chat's row from its newest message, with `unread` of theirs not yet seen.
    fn done(mut self, unread: u32, mentions: u32) -> Thread {
        let c = &mut self.thread.chat;
        if let Some(m) = self.thread.messages.last() {
            let summary = match m.kind {
                Kind::Text => crate::fmt::one_line(&m.spans.iter().map(|s| s.s.as_str()).collect::<String>()),
                Kind::System | Kind::Call => m.system.clone().unwrap_or_default(),
                _ => m.summary.clone().unwrap_or_default(),
            };
            let from = m.from.as_ref().filter(|_| c.kind == ChatKind::Group).map(|f| f.name.clone());
            c.last = Some(Last { from, summary, status: m.status.filter(|_| m.mine), at: m.at });
            c.sort = format!("{}~{:013}", u8::from(c.pinned), m.at);
        }
        c.unread = unread;
        c.mentions = mentions;
        self.thread
    }
}

/// The made-up account: a dozen chats around a Friday lunchtime in Lisbon.
fn made_up(now: i64) -> Vec<Thread> {
    let h = 60.0;
    let ana = person("person:1", "Ana", 0);
    let mae = person("person:2", "Mãe", 0);
    let rui = person("person:3", "Rui", 1);
    let ines = person("person:4", "Inês", 2);
    let sofia = person("person:5", "Sofia", 3);
    let mut threads = vec![];

    let mut w = Writing::new("ana", "Ana Ribeiro", ChatKind::Direct, now);
    w.thread.chat.pinned = true;
    w.thread.chat.typing = vec!["Ana".into()];
    w.theirs(14.0 * h + 52.0, &ana, "chegaste bem?");
    w.mine(14.0 * h + 50.0, "cheguei, boa noite");
    w.mine(4.0 * h + 50.0, "bom dia! dormiste bem?");
    w.theirs(4.0 * h + 47.0, &ana, "sim! a fazer café, e tu?");
    w.mine(4.0 * h + 46.0, "reunião às 10, falamos ao almoço");
    w.theirs(4.0, &ana, "did you get the photos from Sunday?");
    w.other(3.9, Some(&ana), Kind::Image, "▣ photo · 4032 × 3024", "").reactions =
        vec![Reaction { emoji: "❤".into(), n: 2, mine: true }];
    w.mine(3.0, "yes, the one at the lake");
    w.mine(2.9, "a Sofia tirou mais, vou pedir-lhe");
    w.other(1.0, Some(&ana), Kind::Voice, "▶ voice message 0:14", "");
    w.mine(0.9, "haha, ela vai adorar");
    w.theirs(0.0, &ana, "vens jantar *sexta*? a tua mãe pergunta");
    threads.push(w.done(1, 0));

    let mut w = Writing::new("mae", "Mãe", ChatKind::Direct, now);
    w.thread.chat.pinned = true;
    w.theirs(3.0 * h, &mae, "deixei-te sopa no frigorífico");
    w.mine(2.9 * h, "obrigado mãe ❤");
    w.theirs(40.0, &mae, "liga-me quando puderes");
    w.mine(22.0, "ok, ligo-te logo à noite");
    threads.push(w.done(0, 0));

    let helena = person("person:30", "Prof. Helena", 4);
    let mut w = Writing::new("escola", "Escola do Bairro", ChatKind::Group, now);
    for (i, said) in [
        "Bom dia! As inscrições para a natação abrem segunda.",
        "Os livros de fichas chegaram, podem levantar na secretaria.",
        "Lembrem-se das autorizações para a visita de estudo.",
        "A festa de Natal será a 18 de dezembro.",
        "3.º B: reunião na quinta às 18h",
    ]
    .iter()
    .enumerate()
    {
        w.theirs(107.0 + (4 - i) as f64 * 3.0 * h, &helena, said);
    }
    threads.push(w.done(9, 0));

    let mut w = Writing::new("familia", "Família Gomes", ChatKind::Group, now);
    let wed = 2.0 * 24.0 * h;
    w.theirs(wed + 5.0 * h, &mae, "Bom dia a todos! O *jantar de sexta* é às 20h, cá em casa.");
    w.theirs(wed + 5.0 * h - 1.0, &mae, "@Pedro trazes o _vinho_? e ~não te esqueças~ do pão");
    w.theirs(wed + 5.0 * h - 1.0, &mae, "a lista: `pão, vinho, queijo da serra`");
    w.theirs(wed + 5.0 * h - 8.0, &rui, "eu levo o vinho, o Pedro leva o pão").reply = Some(Quote {
        from: "Mãe".into(),
        colour: 0,
        summary: "@Pedro trazes o vinho? e não te esqueças do pão".into(),
    });
    w.theirs(wed + 4.0 * h + 31.0, &ines, "e para o verão? https://casadapraia.pt/ericeira");
    w.other(wed + 2.0 * h, Some(&ines), Kind::Image, "▣ 2 photos", "a vista do quarto");
    w.other(wed + 2.0 * h - 6.0, Some(&sofia), Kind::Sticker, "◇ sticker", "");
    w.other(wed + 1.0 * h + 42.0, Some(&rui), Kind::Video, "▶ video 0:42 · 18.4 MB", "o Tomás a nadar sozinho!");
    w.other(wed + 1.0 * h + 22.0, Some(&mae), Kind::Voice, "▶ voice message 0:31", "");
    w.other(wed + 57.0, Some(&rui), Kind::Document, "▣ orcamento-obras-cozinha.pdf · 2.4 MB · 3 pages", "");
    w.other(wed + 32.0, Some(&mae), Kind::Location, "⌖ Miradouro da Senhora do Monte · Lisbon", "");
    w.other(wed + 21.0, Some(&ines), Kind::Contact, "◉ Dr.ª Marta Lopes · +351 213 456 789", "");
    w.mine(3.0 * h, "eu levo o pão e a sobremesa");
    w.theirs(2.0 * h + 20.0, &mae, "boa! 😘");
    w.theirs(2.0 * h + 15.0, &ines, "@Pedro vens buscar a avó?");
    w.other(2.0 * h + 10.0, Some(&ines), Kind::Image, "▣ photo", "");
    threads.push(w.done(3, 1));

    let mut w = Writing::new("pedro-ana", "Pedro & Ana", ChatKind::Group, now);
    w.other(20.0 * h, Some(&ana), Kind::Document, "▣ bilhetes-comboio.pdf", "");
    threads.push(w.done(0, 0));

    let mut w = Writing::new("rui", "Rui Costa", ChatKind::Direct, now);
    w.theirs(21.0 * h, &rui, "sábado há futebol?");
    w.mine(20.9 * h, "há, às 10");
    w.thread.chat.draft = Some("levo o bolo?".into());
    threads.push(w.done(0, 0));

    let mut w = Writing::new("jantar", "Jantar de sexta", ChatKind::Group, now);
    let thu = 24.0 * h;
    let tiago = person("person:6", "Tiago", 4);
    w.system(thu - 4.0 * h - 40.0, Kind::System, "Rui added Sofia");
    w.other(thu - 4.0 * h - 20.0, Some(&rui), Kind::Poll, "☰ Onde jantamos sexta? · one choice · 6 voted", "");
    w.theirs(thu - 4.0 * h - 24.0, &ines, "Ramiro! há anos que não vou").reactions =
        vec![Reaction { emoji: "❤".into(), n: 3, mine: true }, Reaction { emoji: "☺".into(), n: 1, mine: false }];
    w.system(thu - 5.0 * h - 2.0, Kind::Call, "✆ Missed voice call from Rui");
    w.mine(thu - 5.0 * h - 5.0, "até já").status = Some(Tick::Pending);
    w.mine(thu - 5.0 * h - 5.0, "a sair de casa").status = Some(Tick::Sent);
    w.mine(thu - 5.0 * h - 6.0, "estou no metro").status = Some(Tick::Delivered);
    w.mine(thu - 5.0 * h - 31.0, "cheguei!");
    w.other(thu - 5.0 * h - 33.0, None, Kind::Voice, "▶ voice message 0:06", "").status = Some(Tick::Played);
    w.mine(thu - 5.0 * h - 34.0, "esqueci-me do pão").status = Some(Tick::Failed);
    w.theirs(thu - 5.0 * h - 40.0, &sofia, "afinal chego às 21h").edited = true;
    w.theirs(thu - 5.0 * h - 41.0, &sofia, "").deleted = true;
    w.mine(thu - 5.0 * h - 42.0, "").deleted = true;
    w.add(thu - 5.0 * h - 45.0, Some(&tiago), Kind::Unknown, "");
    w.theirs(thu - 5.0 * h - 50.0, &rui, "alguém tem o número do restaurante?");
    w.theirs(thu - 5.0 * h - 52.0, &rui, "liguei agora, mesa para 6 às 20h30");
    let mut sorted = std::mem::take(&mut w.thread.messages);
    sorted.sort_by_key(|m| m.at);
    w.thread.messages = sorted;
    threads.push(w.done(0, 0));

    let mut w = Writing::new("sofia", "Sofia Martins", ChatKind::Direct, now);
    w.other(4.0 * 24.0 * h, Some(&sofia), Kind::Voice, "▶ voice message 0:42", "");
    threads.push(w.done(0, 0));

    let mut w = Writing::new("avo", "Avó Lurdes", ChatKind::Direct, now);
    w.thread.chat.muted_until = Some(i64::MAX);
    w.theirs(5.0 * 24.0 * h, &person("person:7", "Avó", 5), "Beijinhos a todos");
    threads.push(w.done(1, 0));

    let mut w = Writing::new("climbing", "Lisbon Climbing", ChatKind::Group, now);
    let climbers: Vec<Sender> = ["Marta", "João", "Leonor", "Duarte", "Bea"]
        .iter()
        .enumerate()
        .map(|(i, n)| person(&format!("person:{}", 20 + i), n, i as u8))
        .collect();
    let mut rng = 7u64;
    let count = 1_500;
    for i in 0..count {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let ago = 3.0 * h + (count - i) as f64 * 55.0;
        let words: Vec<&str> =
            (0..2 + (rng >> 40) as usize % 14).map(|k| WORDS[((rng >> (k % 24)) as usize) % WORDS.len()]).collect();
        let text = words.join(" ");
        if (rng >> 13).is_multiple_of(5) {
            w.mine(ago, &text);
        } else {
            w.theirs(ago, &climbers[(rng >> 50) as usize % climbers.len()], &text);
        }
    }
    w.theirs(3.0 * h, &climbers[0], "Saturday at 9?");
    threads.push(w.done(14, 0));

    let mut w = Writing::new("mariana", "Mariana Lopes", ChatKind::Direct, now);
    w.thread.chat.sort = format!("0~{:013}", now - 26 * h as i64 * MINUTE);
    threads.push(w.done(0, 0));

    let mut w = Writing::new("tiago", "Tiago Fonseca", ChatKind::Direct, now);
    w.mine(9.0 * 24.0 * h + 5.0, "obrigado pela boleia!");
    w.theirs(9.0 * 24.0 * h, &tiago, "de nada, quando quiseres");
    threads.push(w.done(0, 0));

    let mut w = Writing::new("ginasio", "Ginásio do Bairro", ChatKind::Group, now);
    w.thread.chat.muted_until = Some(i64::MAX);
    w.theirs(6.0 * h, &person("person:40", "Recepção", 6), "Aulas de sábado canceladas esta semana");
    threads.push(w.done(14, 0));

    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_becomes_spans_as_jawd_gives_them() {
        let spans = markup("@Pedro trazes o _vinho_? e ~não~ do *pão* `já` https://x.pt ok");
        let kinds: Vec<(SpanKind, &str)> = spans.iter().map(|s| (s.t, s.s.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (SpanKind::Mention, "@Pedro"),
                (SpanKind::Text, " trazes o "),
                (SpanKind::Italic, "vinho"),
                (SpanKind::Text, "? e "),
                (SpanKind::Strike, "não"),
                (SpanKind::Text, " do "),
                (SpanKind::Bold, "pão"),
                (SpanKind::Text, " "),
                (SpanKind::Mono, "já"),
                (SpanKind::Text, " "),
                (SpanKind::Link, "https://x.pt"),
                (SpanKind::Text, " ok"),
            ]
        );
        assert_eq!(markup("a_b c")[0].s, "a_b c", "a lone marker is only text");
    }
}
