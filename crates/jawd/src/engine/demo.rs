//! `DemoEngine`: a made-up account inside jawd, for `jawd --engine demo`, `jawd demo`, tests and the
//! load harness. It sends the same events a real engine does, waits for each message's ack as
//! whatsmeow would, and answers ping and bench. What it's sent goes to a made-up WhatsApp that
//! outlives any one engine, as the real one does: it answers with the server's time, the
//! recipients' phones send delivered and then read, and a message sent again with the same ID is
//! the same message. A reaction, an edit or a deletion for everyone is a message too, which the
//! recipients' phones say they have but never read; the server refuses an edit once WhatsApp's
//! window for it has passed; and app state sent for your other devices is taken. Given an
//! `Account`, it answers for its contacts and groups, and writes each history blob its phone offers
//! to the spool as jaw-engine does; events can also be delivered from outside its script, several
//! waiting for their acks at once.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use prost::Message as _;
use tokio::sync::{Notify, mpsc, oneshot};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::{Ack, BoxFuture, Commands, Engine, EngineError, Incoming, Instance, Outgoing, PROTOCOL, pb};
use crate::store::polls::option_hash;

pub const ME: &str = "351910000001@s.whatsapp.net";

/// Two made-up members of every group the demo sends to, besides you.
const MEMBERS: [&str; 2] = ["351913333333@s.whatsapp.net", "351914444444@s.whatsapp.net"];
/// How long after sending a message WhatsApp's server takes an edit of it, as whatsmeow says.
const EDIT_WINDOW_MS: i64 = 20 * 60 * 1000;

pub struct DemoEngine {
    script: Vec<pb::Event>,
    world: Arc<World>,
    /// How long it takes before its script plays, as an engine reading its session.db does.
    pause: Duration,
}

/// A made-up account the demo engine answers for: who's in its contacts and groups, the phone
/// numbers of their LIDs, and the history blobs its phone offers, each made when it's asked for.
pub trait Account: Send + Sync + 'static {
    fn contacts(&self) -> pb::ContactList;
    fn groups(&self) -> pb::GroupList;
    /// The members of a group but you, whose phones answer what you send there.
    fn members(&self, group: &str) -> Vec<String>;
    fn lid_pairs(&self, lids: &[String]) -> pb::LidPairs;
    /// The blob a history notification names, or None if the phone never offered one by that name.
    fn history(&self, notification: &e2e::HistorySyncNotification) -> Option<Blob>;
    /// Every mutation of the phone's app state, in the order whatsmeow hands them over as it
    /// fetches every collection again in full; none unless the phone has some.
    fn app_state(&self) -> Vec<pb::Event> {
        Vec::new()
    }
}

/// A full fetch of the phone's app state as the demo engine played it: how long it took, and how
/// long jawd took to ack each mutation.
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    pub took: Duration,
    pub acks: Vec<Duration>,
}

/// A blob of history: a HistorySync encoded as the phone sends it, and what jaw-engine says of one
/// it has written.
pub struct Blob {
    pub data: Vec<u8>,
    /// WAWebProtobufsHistorySync.HistorySync.HistorySyncType.
    pub kind: i32,
    pub chunk_order: u32,
    pub progress: u32,
    pub conversations: u32,
}

/// How long the made-up recipients take: their phone has a message `delivered` after it's sent,
/// and they've read it `read` after that.
#[derive(Clone, Copy, Debug)]
pub struct Replies {
    pub delivered: Duration,
    pub read: Duration,
}

impl Default for Replies {
    fn default() -> Self {
        Replies { delivered: Duration::from_millis(400), read: Duration::from_millis(1_500) }
    }
}

/// The made-up WhatsApp the demo engine talks to: what it was asked, which messages it has, and how
/// it behaves, kept across the engines that share it.
#[derive(Default)]
pub struct World {
    replies: Mutex<Replies>,
    asked: Mutex<Vec<pb::command::Kind>>,
    /// Each message the server has, by ID, with its time.
    has: Mutex<HashMap<String, i64>>,
    /// Messages a recipient has been given, by ID; a message sent twice is given once.
    given: Mutex<HashSet<String>>,
    /// How many sends to fail next, as a connection that's down does.
    failing: Mutex<u32>,
    /// How many sends to refuse next, as WhatsApp refuses one to a number that isn't on it.
    refusing: Mutex<u32>,
    /// Chats whose every send fails as a connection that's down does.
    failing_chats: Mutex<HashSet<String>>,
    /// Whether the server takes sends without answering, as a connection that drops does.
    holding: AtomicBool,
    /// How many sends the server has had at once, at most.
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    /// Whether sends wait to be answered until it opens.
    gated: AtomicBool,
    opened: Notify,
    asked_now: Notify,
    /// The account the engine answers for, besides its script.
    account: Option<Arc<dyn Account>>,
    /// The full fetches of its app state the engine has played.
    fetched: Mutex<Vec<Fetched>>,
    /// The running engine's half of the link, for events delivered from outside its script.
    link: Mutex<Option<WeakLink>>,
}

impl World {
    /// Hands `event` to jawd through the engine running now, as WhatsApp would send it, and says how
    /// long jawd took to ack it, or to take it if it needs no ack; None if no engine is running or
    /// jawd has gone. Several can wait for their acks at once.
    pub async fn deliver(self: &Arc<World>, event: pb::Event) -> Option<Duration> {
        let link = self.link.lock().expect("the demo world").clone()?;
        let sender = Sender {
            events: link.events.upgrade()?,
            weak: link.weak,
            seq: link.seq,
            waiting: link.waiting,
            world: self.clone(),
        };
        let began = Instant::now();
        sender.send(event).await.then(|| began.elapsed())
    }

    pub fn replying(&self, replies: Replies) {
        *self.replies.lock().expect("the demo world") = replies;
    }

    /// The full fetches of the account's app state played so far.
    pub fn fetched(&self) -> Vec<Fetched> {
        self.fetched.lock().expect("the demo world").clone()
    }

    /// Fails the next `n` sends with a connection that's down.
    #[cfg(test)]
    pub fn fail_next(&self, n: u32) {
        *self.failing.lock().expect("the demo world") = n;
    }

    /// Fails every send to `chats`, from now on.
    #[cfg(test)]
    pub fn fail_chats(&self, chats: &[String]) {
        self.failing_chats.lock().expect("the demo world").extend(chats.iter().cloned());
    }

    /// Lets sends to every chat `fail_chats` named go again.
    #[cfg(test)]
    pub fn stop_failing_chats(&self) {
        self.failing_chats.lock().expect("the demo world").clear();
    }

    /// Refuses the next `n` sends for good.
    #[cfg(test)]
    pub fn refuse_next(&self, n: u32) {
        *self.refusing.lock().expect("the demo world") = n;
    }

    /// Has the server keep message `id` as sent at `at`, as one sent long ago.
    #[cfg(test)]
    pub fn sent_at(&self, id: &str, at: i64) {
        self.has.lock().expect("the demo world").insert(id.to_string(), at);
    }

    /// Takes sends without answering them, until it's told otherwise.
    #[cfg(test)]
    pub fn hold(&self, holding: bool) {
        self.holding.store(holding, Ordering::SeqCst);
    }

    /// Holds sends unanswered while `gated`, answering them all once it opens.
    #[cfg(test)]
    pub fn gate(&self, gated: bool) {
        self.gated.store(gated, Ordering::SeqCst);
        if !gated {
            self.opened.notify_waiters();
        }
    }

    /// The most sends it has had at once.
    #[cfg(test)]
    pub fn most_at_once(&self) -> usize {
        self.most_in_flight.load(Ordering::SeqCst)
    }

    /// Everything the engine was asked, pings and benches apart, in order.
    #[cfg(test)]
    pub fn asked(&self) -> Vec<pb::command::Kind> {
        self.asked.lock().expect("the demo world").clone()
    }

    /// The messages it was asked to send, in order, each time it was asked.
    #[cfg(test)]
    pub fn sends(&self) -> Vec<pb::Send> {
        self.asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::Send(send) => Some(send),
                _ => None,
            })
            .collect()
    }

    /// The votes it was asked to send, in order, each time it was asked.
    #[cfg(test)]
    pub fn votes(&self) -> Vec<pb::PollVote> {
        self.asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::PollVote(vote) => Some(vote),
                _ => None,
            })
            .collect()
    }

    /// How many messages the recipients have been given.
    #[cfg(test)]
    pub fn given(&self) -> usize {
        self.given.lock().expect("the demo world").len()
    }

    /// Waits until `enough` holds of what the engine was asked.
    #[cfg(test)]
    pub async fn until(&self, enough: impl Fn(&[pb::command::Kind]) -> bool) {
        let waited = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let notified = self.asked_now.notified();
                if enough(&self.asked.lock().expect("the demo world")) {
                    return;
                }
                notified.await;
            }
        });
        if waited.await.is_err() {
            panic!("the demo engine wasn't asked what was waited for; it was asked {:?}", self.asked());
        }
    }

    fn ask(&self, kind: pb::command::Kind) {
        self.asked.lock().expect("the demo world").push(kind);
        self.asked_now.notify_waiters();
    }
}

impl DemoEngine {
    /// The made-up account: connected, then a few messages arriving.
    pub fn new() -> DemoEngine {
        let now = now_ms();
        let ana = "351912345678@s.whatsapp.net";
        let climbing = "120363012345678901@g.us";
        let rui = "351913333333@s.whatsapp.net";
        DemoEngine::with(vec![
            connected(),
            text(ana, ana, "3EB0DEMO00000001", now - 120_000, "did you get the photos from Sunday?"),
            text(climbing, rui, "3EB0DEMO00000002", now - 60_000, "Saturday at 9?"),
            text(ana, ana, "3EB0DEMO00000003", now - 5_000, "the one at the lake"),
            // You and the two whose phones answer what you send there, so a mention of one lands;
            // told once the messages are in, so storing them holds none of the messages back.
            group(climbing, "Lisbon Climbing", &[ME, MEMBERS[0], MEMBERS[1]]),
            // Earlier than the rest, so each chat's newest stays as it was.
            link(
                ana,
                ana,
                "3EB0DEMO00000004",
                now - 600_000,
                &Preview {
                    text: "a previsão para domingo https://www.ipma.pt/pt/otempo/prev.localidade.hora/",
                    url: "https://www.ipma.pt/pt/otempo/prev.localidade.hora/",
                    title: "Previsão para Sintra · IPMA",
                    description: "Céu pouco nublado, vento fraco. Máxima de 22 °C.",
                },
            ),
            poll(
                climbing,
                rui,
                "3EB0DEMO00000005",
                now - 300_000,
                "Onde jantamos sexta?",
                &["Ramen", "Tasca do Chico", "Pizza"],
                1,
            ),
            vote(
                climbing,
                "351914444444@s.whatsapp.net",
                "3EB0DEMO00000006",
                now - 280_000,
                ("3EB0DEMO00000005", rui),
                &["Ramen"],
            ),
            vote(climbing, rui, "3EB0DEMO00000007", now - 270_000, ("3EB0DEMO00000005", rui), &["Tasca do Chico"]),
        ])
    }

    /// The made-up account's engine finding no device in its session.db, as after the account was
    /// logged out while jawd was off: two seconds after it starts, so an interface can connect to a
    /// jawd that says it's linked first, it says the account isn't linked, and nothing more.
    pub fn unlinked() -> DemoEngine {
        let unlinked = pb::Connection { state: pb::connection::State::Unlinked as i32, ..Default::default() };
        let script = vec![pb::Event { seq: 0, kind: Some(pb::event::Kind::Connection(unlinked)) }];
        DemoEngine { pause: Duration::from_secs(2), ..DemoEngine::with(script) }
    }

    pub fn with(script: Vec<pb::Event>) -> DemoEngine {
        DemoEngine { script, world: Arc::default(), pause: Duration::ZERO }
    }

    /// The made-up `account`, which plays `script` and answers what jawd asks about it.
    pub fn for_account(script: Vec<pb::Event>, account: Arc<dyn Account>) -> DemoEngine {
        let world = Arc::new(World { account: Some(account), ..World::default() });
        DemoEngine { script, world, pause: Duration::ZERO }
    }

    /// An engine on `world`, as an engine started again talks to the same WhatsApp.
    #[cfg(test)]
    pub fn sharing(script: Vec<pb::Event>, world: Arc<World>) -> DemoEngine {
        DemoEngine { script, world, pause: Duration::ZERO }
    }

    pub fn world(&self) -> Arc<World> {
        self.world.clone()
    }
}

/// The account connected, with its push name, as the demo's script starts.
pub fn connected() -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Connection(pb::Connection {
            state: pb::connection::State::Connected as i32,
            jid: ME.into(),
            push_name: "Pedro".into(),
            ..Default::default()
        })),
    }
}

impl Default for DemoEngine {
    fn default() -> Self {
        DemoEngine::new()
    }
}

/// Group `jid`'s name and who's in it, as WhatsApp tells of a change to a group.
pub fn group(jid: &str, name: &str, members: &[&str]) -> pb::Event {
    let change = pb::GroupChange {
        jid: jid.into(),
        timestamp_ms: now_ms(),
        name: Some(name.into()),
        join: members.iter().map(|member| member.to_string()).collect(),
        ..Default::default()
    };
    pb::Event { seq: 0, kind: Some(pb::event::Kind::GroupChange(change)) }
}

/// A text message from `sender` in `chat`.
pub fn text(chat: &str, sender: &str, id: &str, at_ms: i64, body: &str) -> pb::Event {
    let raw = e2e::Message { conversation: Some(body.into()), ..Default::default() };
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Message(pb::Message {
            info: Some(pb::MessageInfo {
                chat: chat.into(),
                sender: sender.into(),
                group: chat.ends_with("@g.us"),
                id: id.into(),
                r#type: "text".into(),
                timestamp_ms: at_ms,
                ..Default::default()
            }),
            raw: raw.encode_to_vec(),
            ..Default::default()
        })),
    }
}

/// A link preview as a phone sends one with its text.
pub struct Preview<'a> {
    pub text: &'a str,
    pub url: &'a str,
    pub title: &'a str,
    pub description: &'a str,
}

/// A message from `sender` in `chat` with a link `preview`, and its small thumbnail, as the phones
/// send one.
pub fn link(chat: &str, sender: &str, id: &str, at_ms: i64, preview: &Preview) -> pb::Event {
    let thumbnail = image::RgbImage::from_fn(96, 96, |x, y| image::Rgb([30 + x as u8, 90 + y as u8, 180]));
    let mut jpeg = Vec::new();
    let encoded = thumbnail.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 60));
    let raw = e2e::Message {
        extended_text_message: Some(Box::new(e2e::ExtendedTextMessage {
            text: Some(preview.text.into()),
            matched_text: Some(preview.url.into()),
            title: Some(preview.title.into()),
            description: Some(preview.description.into()),
            preview_type: Some(e2e::extended_text_message::PreviewType::None as i32),
            jpeg_thumbnail: encoded.is_ok().then_some(jpeg),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut event = text(chat, sender, id, at_ms, "");
    if let Some(pb::event::Kind::Message(message)) = &mut event.kind {
        message.raw = raw.encode_to_vec();
    }
    event
}

/// A poll from `sender` in `chat`, asking `question` with `options`, of which a vote may choose
/// `max`, with a secret of its own, as the phones send one.
pub fn poll(chat: &str, sender: &str, id: &str, at_ms: i64, question: &str, options: &[&str], max: u32) -> pb::Event {
    let creation = e2e::PollCreationMessage {
        name: Some(question.into()),
        options: options
            .iter()
            .map(|name| e2e::poll_creation_message::Option {
                option_name: Some(name.to_string()),
                ..Default::default()
            })
            .collect(),
        selectable_options_count: Some(if max as usize >= options.len() { 0 } else { max }),
        ..Default::default()
    };
    let raw = e2e::Message {
        poll_creation_message_v3: Some(Box::new(creation)),
        message_context_info: Some(e2e::MessageContextInfo { message_secret: Some(vec![7; 32]), ..Default::default() }),
        ..Default::default()
    };
    let mut event = text(chat, sender, id, at_ms, "");
    if let Some(pb::event::Kind::Message(message)) = &mut event.kind {
        message.raw = raw.encode_to_vec();
    }
    event
}

/// `voter`'s vote, as message `id`, in the poll `poll` (its ID and who sent it), choosing `options`
/// by name: encrypted, as WhatsApp brings it, with what the engine decrypted beside it.
pub fn vote(chat: &str, voter: &str, id: &str, at_ms: i64, poll: (&str, &str), options: &[&str]) -> pb::Event {
    let raw = e2e::Message {
        poll_update_message: Some(e2e::PollUpdateMessage {
            poll_creation_message_key: Some(MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(false),
                id: Some(poll.0.into()),
                participant: chat.ends_with("@g.us").then(|| poll.1.into()),
            }),
            vote: Some(e2e::PollEncValue { enc_payload: Some(vec![0; 16]), enc_iv: Some(vec![0; 12]) }),
            sender_timestamp_ms: Some(at_ms),
            ..Default::default()
        }),
        ..Default::default()
    };
    let chosen = e2e::PollVoteMessage { selected_options: options.iter().map(|name| option_hash(name)).collect() };
    let mut event = text(chat, voter, id, at_ms, "");
    if let Some(pb::event::Kind::Message(message)) = &mut event.kind {
        message.raw = raw.encode_to_vec();
        message.decrypted = chosen.encode_to_vec();
    }
    event
}

/// `recipient`'s phone saying it has, or has read, a message sent to `send.chat`.
fn receipt(send: &pb::Send, recipient: &str, kind: &str) -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::Receipt(pb::Receipt {
            chat: send.chat.clone(),
            sender: recipient.into(),
            group: send.chat.ends_with("@g.us"),
            ids: vec![send.id.clone()],
            timestamp_ms: now_ms(),
            r#type: kind.into(),
            ..Default::default()
        })),
    }
}

/// An action on another message, which the recipients' phones say they have but never read.
enum Action {
    /// An edit of the message with this ID.
    Edit(String),
    Other,
}

/// What a message to send is, if it's an action rather than a message to show.
fn action_of(raw: &[u8]) -> Option<Action> {
    let message = e2e::Message::decode(raw).ok()?;
    let edited =
        message.edited_message.as_ref().and_then(|e| e.message.as_ref()).and_then(|m| m.protocol_message.as_ref());
    if let Some(edit) = edited {
        return Some(Action::Edit(edit.key.as_ref()?.id.clone()?));
    }
    (message.reaction_message.is_some() || message.protocol_message.is_some() || message.poll_update_message.is_some())
        .then_some(Action::Other)
}

/// Every number is on the made-up WhatsApp but those starting 999, each known by a LID as well.
fn on_whatsapp(asked: &pb::OnWhatsApp) -> pb::OnWhatsAppList {
    let users = asked
        .phones
        .iter()
        .map(|phone| {
            let is_in = !phone.starts_with("999");
            pb::OnWhatsAppUser {
                query: phone.clone(),
                is_in,
                jid: if is_in { format!("1000{phone}@lid") } else { String::new() },
                phone: if is_in { format!("{phone}@s.whatsapp.net") } else { String::new() },
                verified_name: String::new(),
            }
        })
        .collect();
    pb::OnWhatsAppList { users }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Writes `data` to `path` as jaw-engine writes a history blob: through a .part file synced to the
/// disk before it's renamed into place, and the folder synced after, so after a power cut the file
/// is either whole or not there.
fn write_whole(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let folder = path.parent().filter(|f| !f.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&part)?;
        file.write_all(data)?;
        // F_FULLFSYNC on macOS, where fsync stops at the drive's cache.
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, path)?;
    std::fs::File::open(folder)?.sync_all()
}

impl Engine for DemoEngine {
    fn kind(&self) -> &'static str {
        "demo"
    }

    fn start(&self, events: mpsc::Sender<Incoming>) -> BoxFuture<'_, Result<Instance>> {
        let script = self.script.clone();
        let world = self.world.clone();
        let pause = self.pause;
        Box::pin(async move {
            let (tx, mut outgoing) = mpsc::unbounded_channel();
            let commands = Commands { tx };
            let sender = Sender {
                events,
                weak: commands.tx.downgrade(),
                seq: Arc::new(AtomicU64::new(0)),
                waiting: Arc::default(),
                world: world.clone(),
            };
            *world.link.lock().expect("the demo world") = Some(WeakLink {
                events: sender.events.downgrade(),
                weak: sender.weak.clone(),
                seq: sender.seq.clone(),
                waiting: sender.waiting.clone(),
            });
            let playing = sender.clone();
            tokio::spawn(async move {
                tokio::time::sleep(pause).await;
                for event in script {
                    if !playing.send(event).await {
                        break;
                    }
                }
            });
            let serving = tokio::spawn(async move {
                while let Some(out) = outgoing.recv().await {
                    match out {
                        Outgoing::Ack(seq) => sender.acked(seq),
                        Outgoing::Command { kind, reply } => {
                            let sender = sender.clone();
                            tokio::spawn(async move {
                                let _ = reply.send(sender.answer(kind).await);
                            });
                        }
                    }
                }
            });
            let exited: BoxFuture<'static, String> = Box::pin(async move {
                let _ = serving.await;
                "the demo engine stopped".to_string()
            });
            let hello =
                pb::Hello { protocol: PROTOCOL, program: "jawd's demo engine".into(), whatsmeow: String::new() };
            Ok(Instance { hello, commands, exited })
        })
    }
}

/// The demo's half of the link: it numbers what needs an ack and waits for it, as jaw-engine does.
#[derive(Clone)]
struct Sender {
    events: mpsc::Sender<Incoming>,
    weak: mpsc::WeakUnboundedSender<Outgoing>,
    seq: Arc<AtomicU64>,
    waiting: Arc<Mutex<HashMap<u64, oneshot::Sender<()>>>>,
    world: Arc<World>,
}

/// A running engine's half of the link as its world keeps it, holding neither jawd's end open nor
/// the world itself.
#[derive(Clone)]
struct WeakLink {
    events: mpsc::WeakSender<Incoming>,
    weak: mpsc::WeakUnboundedSender<Outgoing>,
    seq: Arc<AtomicU64>,
    waiting: Arc<Mutex<HashMap<u64, oneshot::Sender<()>>>>,
}

impl Sender {
    /// Sends an event; a message, a receipt, an app-state mutation or a bench event waits for its
    /// ack, as jaw-engine waits for each. False if jawd has gone.
    async fn send(&self, mut event: pb::Event) -> bool {
        let durable = matches!(
            event.kind,
            Some(
                pb::event::Kind::Message(_)
                    | pb::event::Kind::Receipt(_)
                    | pb::event::Kind::AppState(_)
                    | pb::event::Kind::Bench(_)
            )
        );
        if !durable {
            return self.events.send(Incoming { event, ack: None }).await.is_ok();
        }
        let Some(tx) = self.weak.upgrade() else { return false };
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let (acked, ack_came) = oneshot::channel();
        self.waiting.lock().expect("demo acks").insert(seq, acked);
        event.seq = seq;
        if self.events.send(Incoming { event, ack: Some(Ack { seq, tx }) }).await.is_err() {
            return false;
        }
        ack_came.await.is_ok()
    }

    fn acked(&self, seq: u64) {
        if let Some(waiter) = self.waiting.lock().expect("demo acks").remove(&seq) {
            let _ = waiter.send(());
        }
    }

    async fn answer(&self, kind: pb::command::Kind) -> Result<pb::reply::Kind, EngineError> {
        let done = Ok(pb::reply::Kind::Done(pb::Done {}));
        match kind {
            pb::command::Kind::Ping(_) => Ok(pb::reply::Kind::Pong(pb::Pong { at_ms: now_ms() })),
            pb::command::Kind::BenchRun(run) => self.bench(run).await,
            // The made-up account has no connection to end.
            pb::command::Kind::LinkCancel(_) => Ok(pb::reply::Kind::Done(pb::Done {})),
            pb::command::Kind::Send(send) => {
                self.world.ask(pb::command::Kind::Send(send.clone()));
                self.sent(send).await
            }
            // The made-up WhatsApp takes a vote as a message of its own, as the real one does once
            // jaw-engine has encrypted it.
            pb::command::Kind::PollVote(vote) => {
                self.world.ask(pb::command::Kind::PollVote(vote.clone()));
                let update = e2e::Message {
                    poll_update_message: Some(e2e::PollUpdateMessage {
                        poll_creation_message_key: Some(MessageKey {
                            remote_jid: Some(vote.chat.clone()),
                            from_me: Some(vote.poll_from_me),
                            id: Some(vote.poll_id.clone()),
                            participant: (!vote.poll_sender.is_empty()).then(|| vote.poll_sender.clone()),
                        }),
                        sender_timestamp_ms: Some(vote.at_ms),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                self.sent(pb::Send { chat: vote.chat, id: vote.id, raw: update.encode_to_vec() }).await
            }
            pb::command::Kind::OnWhatsapp(asked) => {
                self.world.ask(pb::command::Kind::OnWhatsapp(asked.clone()));
                Ok(pb::reply::Kind::OnWhatsappList(on_whatsapp(&asked)))
            }
            kind @ (pb::command::Kind::MarkRead(_)
            | pb::command::Kind::ChatPresence(_)
            | pb::command::Kind::Presence(_)
            | pb::command::Kind::AppStatePatch(_)) => {
                self.world.ask(kind);
                done
            }
            pb::command::Kind::Contacts(_) => Ok(pb::reply::Kind::ContactList(self.account()?.contacts())),
            pb::command::Kind::Groups(_) => Ok(pb::reply::Kind::GroupList(self.account()?.groups())),
            pb::command::Kind::LidLookup(asked) => {
                Ok(pb::reply::Kind::LidPairs(self.account()?.lid_pairs(&asked.lids)))
            }
            // Its app state, if it has any, comes a mutation at a time, each waiting for its ack, as
            // whatsmeow hands each over as it fetches every collection again in full.
            pb::command::Kind::AppStateResync(_) => {
                let account = self.account()?;
                let (began, mut acks) = (Instant::now(), Vec::new());
                for mutation in account.app_state() {
                    let sent = Instant::now();
                    if !self.send(mutation).await {
                        return Err(EngineError::Gone);
                    }
                    acks.push(sent.elapsed());
                }
                self.world.fetched.lock().expect("the demo world").push(Fetched { took: began.elapsed(), acks });
                done
            }
            // Nobody in the made-up phone has a picture, and its history has all been offered.
            pb::command::Kind::HistoryRequest(_) => self.account().and(done),
            pb::command::Kind::Avatar(_) => self.account().map(|_| {
                let none = pb::AvatarFile { state: pb::avatar_file::State::None as i32, ..Default::default() };
                pb::reply::Kind::AvatarFile(none)
            }),
            pb::command::Kind::HistoryDownload(download) => self.history(download).await,
            _ => Err(EngineError::Failed { message: "the demo engine doesn't do that".into(), retry: false }),
        }
    }

    fn account(&self) -> Result<Arc<dyn Account>, EngineError> {
        self.world
            .account
            .clone()
            .ok_or_else(|| EngineError::Failed { message: "the demo engine doesn't do that".into(), retry: false })
    }

    /// Makes the blob a history notification names and writes it to the path jawd chose, whole or
    /// not at all, as jaw-engine does once it has downloaded one.
    async fn history(&self, download: pb::HistoryDownload) -> Result<pb::reply::Kind, EngineError> {
        let account = self.account()?;
        let notification = e2e::HistorySyncNotification::decode(download.notification.as_slice()).map_err(|e| {
            EngineError::Failed { message: format!("that isn't a history notification: {e}"), retry: false }
        })?;
        let made = tokio::task::spawn_blocking(move || {
            let Some(blob) = account.history(&notification) else {
                return Err(EngineError::Failed { message: "the phone has no such history".into(), retry: false });
            };
            write_whole(Path::new(&download.path), &blob.data)
                .map_err(|e| EngineError::Failed { message: format!("can't write the history: {e}"), retry: true })?;
            Ok(pb::reply::Kind::HistoryFile(pb::HistoryFile {
                path: download.path,
                r#type: blob.kind,
                chunk_order: blob.chunk_order,
                progress: blob.progress,
                bytes: blob.data.len() as u64,
                conversations: blob.conversations,
            }))
        });
        made.await
            .map_err(|e| EngineError::Failed { message: format!("making the history stopped: {e}"), retry: true })?
    }

    /// The server takes a message, once whatever its ID; the recipients' phones then say they have
    /// it and, later, that they've read it.
    async fn sent(&self, send: pb::Send) -> Result<pb::reply::Kind, EngineError> {
        let world = &self.world;
        let next = |counter: &Mutex<u32>| {
            let mut left = counter.lock().expect("the demo world");
            let now = *left > 0;
            *left = left.saturating_sub(1);
            now
        };
        if next(&world.refusing) {
            return Err(EngineError::Failed {
                message: format!("no LID found for {} from server", send.chat),
                retry: false,
            });
        }
        if next(&world.failing) || world.failing_chats.lock().expect("the demo world").contains(&send.chat) {
            return Err(EngineError::Failed { message: "websocket not connected".into(), retry: true });
        }
        let action = action_of(&send.raw);
        if let Some(Action::Edit(of)) = &action {
            let edited = world.has.lock().expect("the demo world").get(of).copied();
            if edited.is_some_and(|at| now_ms() - at > EDIT_WINDOW_MS) {
                return Err(EngineError::Failed { message: "server returned error 406".into(), retry: false });
            }
        }
        let at = *world.has.lock().expect("the demo world").entry(send.id.clone()).or_insert_with(now_ms);
        if world.holding.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        let flying = world.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        world.most_in_flight.fetch_max(flying, Ordering::SeqCst);
        loop {
            let opened = world.opened.notified();
            if !world.gated.load(Ordering::SeqCst) {
                break;
            }
            opened.await;
        }
        world.in_flight.fetch_sub(1, Ordering::SeqCst);
        if world.given.lock().expect("the demo world").insert(send.id.clone()) {
            let (sender, replies) = (self.clone(), *world.replies.lock().expect("the demo world"));
            let send = send.clone();
            let recipients: Vec<String> = match (send.chat.ends_with("@g.us"), &world.account) {
                (true, Some(account)) => account.members(&send.chat),
                (true, None) => MEMBERS.iter().map(|member| member.to_string()).collect(),
                (false, _) => vec![send.chat.clone()],
            };
            tokio::spawn(async move {
                tokio::time::sleep(replies.delivered).await;
                for recipient in &recipients {
                    if !sender.send(receipt(&send, recipient, "")).await {
                        return;
                    }
                }
                if action.is_some() {
                    return;
                }
                for recipient in &recipients {
                    tokio::time::sleep(replies.read).await;
                    if !sender.send(receipt(&send, recipient, "read")).await {
                        return;
                    }
                }
            });
        }
        Ok(pb::reply::Kind::Sent(pb::Sent { id: send.id, at_ms: at, chat: send.chat }))
    }

    async fn bench(&self, run: pb::BenchRun) -> Result<pb::reply::Kind, EngineError> {
        let payload = vec![0u8; run.size as usize];
        let mut trips = Vec::with_capacity(run.count as usize);
        let started = Instant::now();
        for _ in 0..run.count {
            let sent = Instant::now();
            let event =
                pb::Event { seq: 0, kind: Some(pb::event::Kind::Bench(pb::Bench { payload: payload.clone() })) };
            if !self.send(event).await {
                return Err(EngineError::Gone);
            }
            trips.push(sent.elapsed().as_micros() as u64);
        }
        trips.sort_unstable();
        let at = |q: usize| trips.get(trips.len() * q / 100).or(trips.last()).copied().unwrap_or_default();
        Ok(pb::reply::Kind::BenchResult(pb::BenchResult {
            count: run.count,
            elapsed_us: started.elapsed().as_micros() as u64,
            p50_us: at(50),
            p99_us: at(99),
            max_us: trips.last().copied().unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use wa_proto::wa_web_protobufs_history_sync::HistorySync;

    use super::*;

    #[tokio::test]
    async fn the_demo_plays_its_account_and_waits_for_each_ack() {
        let (tx, mut rx) = mpsc::channel(8);
        let instance = DemoEngine::new().start(tx).await.unwrap();
        let connected = rx.recv().await.unwrap();
        assert!(matches!(connected.event.kind, Some(pb::event::Kind::Connection(_))));
        assert!(connected.ack.is_none());
        let first = rx.recv().await.unwrap();
        let Some(pb::event::Kind::Message(message)) = &first.event.kind else { panic!("{:?}", first.event) };
        let raw = e2e::Message::decode(message.raw.as_slice()).unwrap();
        assert_eq!(raw.conversation.as_deref(), Some("did you get the photos from Sunday?"));
        assert_eq!(first.event.seq, 1);
        let ack = first.ack.expect("a message needs an ack");
        let early = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
        assert!(early.is_err(), "the next message came before the first was acked");
        ack.send();
        let second = rx.recv().await.unwrap();
        assert_eq!(second.event.seq, 2);
        second.ack.expect("a message needs an ack").send();
        let third = rx.recv().await.unwrap();
        third.ack.unwrap().send();

        let pong = instance.commands.call(pb::command::Kind::Ping(pb::Ping {})).await.unwrap();
        assert!(matches!(pong, pb::reply::Kind::Pong(_)));
        let acking = tokio::spawn(async move {
            while let Some(incoming) = rx.recv().await {
                if let Some(ack) = incoming.ack {
                    ack.send();
                }
            }
        });
        let result =
            instance.commands.call(pb::command::Kind::BenchRun(pb::BenchRun { count: 500, size: 10 })).await.unwrap();
        let pb::reply::Kind::BenchResult(result) = result else { panic!("{result:?}") };
        assert_eq!(result.count, 500);
        let refused = instance.commands.call(pb::command::Kind::Logout(pb::Logout {})).await;
        assert!(matches!(refused, Err(EngineError::Failed { .. })));
        drop(instance.commands);
        assert_eq!(instance.exited.await, "the demo engine stopped");
        acking.abort();
    }

    const ANA: &str = "351912345678@s.whatsapp.net";
    const CLIMBING: &str = "120363012345678901@g.us";

    /// An account with one contact, one group and one blob of history, the second the phone offers.
    struct Tiny;

    impl Account for Tiny {
        fn contacts(&self) -> pb::ContactList {
            let ana = pb::Contact { jid: ANA.into(), full_name: "Ana Ribeiro".into(), ..Default::default() };
            pb::ContactList { contacts: vec![ana] }
        }

        fn groups(&self) -> pb::GroupList {
            let climbing = pb::GroupInfo { jid: CLIMBING.into(), name: "Lisbon Climbing".into(), ..Default::default() };
            pb::GroupList { groups: vec![climbing] }
        }

        fn members(&self, _: &str) -> Vec<String> {
            vec![ANA.into()]
        }

        fn lid_pairs(&self, lids: &[String]) -> pb::LidPairs {
            pb::LidPairs { pairs: lids.iter().map(|lid| pb::LidPair { lid: lid.clone(), phone: ANA.into() }).collect() }
        }

        fn history(&self, notification: &e2e::HistorySyncNotification) -> Option<Blob> {
            use wa_proto::wa_web_protobufs_history_sync::{Conversation, history_sync::HistorySyncType};
            (notification.chunk_order == Some(1)).then(|| {
                let history = HistorySync {
                    sync_type: HistorySyncType::Full as i32,
                    conversations: vec![Conversation { id: ANA.into(), ..Default::default() }],
                    ..Default::default()
                };
                Blob {
                    data: history.encode_to_vec(),
                    kind: history.sync_type,
                    chunk_order: 1,
                    progress: 100,
                    conversations: 1,
                }
            })
        }

        fn app_state(&self) -> Vec<pb::Event> {
            [["pin_v1", ANA], ["archive", CLIMBING]]
                .into_iter()
                .map(|index| pb::Event {
                    seq: 0,
                    kind: Some(pb::event::Kind::AppState(pb::AppState {
                        index: index.iter().map(|part| part.to_string()).collect(),
                        value: Vec::new(),
                    })),
                })
                .collect()
        }
    }

    #[tokio::test]
    async fn a_full_fetch_of_the_app_state_waits_for_each_mutations_ack_and_answers_once_all_are_in() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::for_account(Vec::new(), Arc::new(Tiny));
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        let answered = AtomicBool::new(false);
        let fetching = async {
            let names = vec!["regular".to_string()];
            let reply = instance.commands.call(pb::command::Kind::AppStateResync(pb::AppStateResync { names })).await;
            answered.store(true, Ordering::SeqCst);
            reply
        };
        let acking = async {
            for expected in ["pin_v1", "archive"] {
                let incoming = tokio::time::timeout(Duration::from_secs(10), rx.recv())
                    .await
                    .unwrap_or_else(|_| panic!("{expected} never came"))
                    .unwrap();
                let Some(pb::event::Kind::AppState(state)) = &incoming.event.kind else {
                    panic!("{:?}", incoming.event)
                };
                assert_eq!(state.index[0], expected);
                let ack = incoming.ack.expect("a mutation needs an ack, as jaw-engine waits for each");
                let early = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
                assert!(early.is_err(), "the next came before {expected} was acked");
                assert!(!answered.load(Ordering::SeqCst), "the fetch was answered before {expected} was acked");
                ack.send();
            }
        };
        let (reply, ()) = tokio::join!(fetching, acking);
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        let fetched = world.fetched();
        assert_eq!((fetched.len(), fetched[0].acks.len()), (1, 2), "one fetch of two mutations, each acked");
    }

    fn download(notification: e2e::HistorySyncNotification, path: &Path) -> pb::command::Kind {
        let path = path.display().to_string();
        pb::command::Kind::HistoryDownload(pb::HistoryDownload { notification: notification.encode_to_vec(), path })
    }

    #[tokio::test]
    async fn the_demo_says_who_is_in_its_group_once_its_messages_are_in_so_a_mention_of_one_lands() {
        let (tx, mut rx) = mpsc::channel(8);
        let _instance = DemoEngine::new().start(tx).await.unwrap();
        let mut played = Vec::new();
        let playing = async {
            while played.len() < 5 {
                let incoming = rx.recv().await.unwrap();
                if let Some(ack) = incoming.ack {
                    ack.send();
                }
                played.push(incoming.event.kind.unwrap());
            }
        };
        let whole = tokio::time::timeout(Duration::from_secs(10), playing).await;
        assert!(whole.is_ok(), "the script played its five events: {played:?}");
        let messages = played.iter().filter(|kind| matches!(kind, pb::event::Kind::Message(_))).count();
        assert_eq!(messages, 3, "its messages first: {played:?}");
        let Some(pb::event::Kind::GroupChange(group)) = played.last() else { panic!("{played:?}") };
        assert_eq!((group.jid.as_str(), group.name.as_deref()), ("120363012345678901@g.us", Some("Lisbon Climbing")));
        assert_eq!(group.join, [ME, MEMBERS[0], MEMBERS[1]], "you, and the two whose phones answer what you send");
    }

    #[tokio::test]
    async fn an_account_answers_for_its_people_and_writes_each_history_blob_whole_where_jawd_asks() {
        let (tx, _rx) = mpsc::channel(8);
        let instance = DemoEngine::for_account(Vec::new(), Arc::new(Tiny)).start(tx).await.unwrap();
        let call = |kind| instance.commands.call(kind);
        let Ok(pb::reply::Kind::ContactList(contacts)) = call(pb::command::Kind::Contacts(pb::Contacts {})).await
        else {
            panic!("no contacts")
        };
        assert_eq!(contacts.contacts[0].full_name, "Ana Ribeiro");
        let Ok(pb::reply::Kind::GroupList(groups)) = call(pb::command::Kind::Groups(pb::Groups {})).await else {
            panic!("no groups")
        };
        assert_eq!(groups.groups[0].name, "Lisbon Climbing");
        let asked = pb::LidLookup { lids: vec!["100000000001@lid".into()] };
        let Ok(pb::reply::Kind::LidPairs(pairs)) = call(pb::command::Kind::LidLookup(asked)).await else {
            panic!("no LIDs")
        };
        assert_eq!(pairs.pairs[0].phone, ANA);

        let spool = tempfile::tempdir().unwrap();
        let path = spool.path().join("spool").join("3EB0HISTORY.pb");
        let offered = e2e::HistorySyncNotification { chunk_order: Some(1), ..Default::default() };
        let Ok(pb::reply::Kind::HistoryFile(file)) = call(download(offered, &path)).await else {
            panic!("no history file")
        };
        let written = std::fs::read(&path).unwrap();
        assert_eq!((file.bytes, file.conversations, file.progress), (written.len() as u64, 1, 100));
        assert_eq!(HistorySync::decode(written.as_slice()).unwrap().conversations[0].id, ANA);
        let names: Vec<_> =
            std::fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["3EB0HISTORY.pb"], "no .part file is left beside it");

        let never = e2e::HistorySyncNotification { chunk_order: Some(2), ..Default::default() };
        let gone = call(download(never, &spool.path().join("never.pb"))).await;
        assert!(matches!(gone, Err(EngineError::Failed { retry: false, .. })), "{gone:?}");
        assert!(!spool.path().join("never.pb").exists());

        let (tx, _rx) = mpsc::channel(8);
        let plain = DemoEngine::with(Vec::new()).start(tx).await.unwrap();
        let offered = e2e::HistorySyncNotification { chunk_order: Some(1), ..Default::default() };
        let refused = plain.commands.call(download(offered, &spool.path().join("plain.pb"))).await;
        assert!(matches!(refused, Err(EngineError::Failed { retry: false, .. })), "without an account: {refused:?}");
    }

    #[tokio::test]
    async fn a_history_blob_is_renamed_into_place_whole_and_never_written_into_the_file_at_its_path() {
        let (tx, _rx) = mpsc::channel(8);
        let instance = DemoEngine::for_account(Vec::new(), Arc::new(Tiny)).start(tx).await.unwrap();
        let spool = tempfile::tempdir().unwrap();
        let path = spool.path().join("3EB0HISTORY.pb");
        std::fs::write(&path, b"a blob cut short").unwrap();
        // A second name for the file at the blob's path, which sees anything written into that file.
        let other = spool.path().join("other name");
        std::fs::hard_link(&path, &other).unwrap();
        let offered = e2e::HistorySyncNotification { chunk_order: Some(1), ..Default::default() };
        let reply = instance.commands.call(download(offered, &path)).await;
        assert!(matches!(reply, Ok(pb::reply::Kind::HistoryFile(_))), "{reply:?}");
        assert_eq!(
            std::fs::read(&other).unwrap(),
            b"a blob cut short",
            "the blob was written into the file at its path"
        );
        let written = HistorySync::decode(std::fs::read(&path).unwrap().as_slice()).unwrap();
        assert_eq!(written.conversations.len(), 1, "the path holds the whole blob");
        let mut names: Vec<_> = std::fs::read_dir(spool.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, ["3EB0HISTORY.pb", "other name"], "no .part file is left beside it");
    }

    #[tokio::test]
    async fn events_delivered_from_outside_the_script_each_wait_for_their_own_ack_several_at_once() {
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        assert!(world.deliver(text(ANA, ANA, "early", 0, "hi")).await.is_none(), "no engine is running yet");
        let (tx, mut rx) = mpsc::channel(64);
        let _instance = engine.start(tx).await.unwrap();
        let delivering: Vec<_> = (0..8)
            .map(|n| {
                let world = world.clone();
                tokio::spawn(async move { world.deliver(text(ANA, ANA, &format!("m{n}"), 0, "hi")).await })
            })
            .collect();
        let mut held = Vec::new();
        for _ in 0..8 {
            held.push(rx.recv().await.unwrap());
        }
        let mut seqs: Vec<u64> = held.iter().map(|incoming| incoming.event.seq).collect();
        seqs.sort_unstable();
        seqs.dedup();
        assert_eq!(seqs.len(), 8, "each has a seq of its own");
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(delivering.iter().all(|d| !d.is_finished()), "none is answered before its ack");

        let last = held.pop().unwrap();
        let Some(pb::event::Kind::Message(message)) = &last.event.kind else { panic!("{:?}", last.event) };
        let id = message.info.as_ref().unwrap().id.clone();
        last.ack.expect("a message needs an ack").send();
        let n: usize = id.trim_start_matches('m').parse().unwrap();
        let mut delivering: Vec<_> = delivering.into_iter().map(Some).collect();
        let answered = tokio::time::timeout(Duration::from_secs(5), delivering[n].take().unwrap()).await;
        assert!(answered.unwrap().unwrap().is_some(), "the one acked is answered");
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(delivering.iter().flatten().all(|d| !d.is_finished()), "the rest still wait for theirs");
        for incoming in held {
            incoming.ack.expect("a message needs an ack").send();
        }
        for d in delivering.into_iter().flatten() {
            assert!(d.await.unwrap().is_some());
        }

        assert!(world.deliver(connected()).await.is_some(), "a connection needs no ack");
        assert!(rx.recv().await.unwrap().ack.is_none());
        drop(rx);
        assert!(world.deliver(text(ANA, ANA, "late", 0, "hi")).await.is_none(), "jawd has gone");
    }
}
