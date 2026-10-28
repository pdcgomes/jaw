//! `DemoEngine`: a made-up account inside jawd, for `jawd --engine demo`, `jawd demo`, tests and the
//! load harness. It sends the same events a real engine does, waits for each message's ack as
//! whatsmeow would, and answers ping and bench. What it's sent goes to a made-up WhatsApp that
//! outlives any one engine, as the real one does: it answers with the server's time, the
//! recipients' phones send delivered and then read, and a message sent again with the same ID is
//! the same message. A reaction, an edit or a deletion for everyone is a message too, which the
//! recipients' phones say they have but never read; the server refuses an edit once WhatsApp's
//! window for it has passed; and app state sent for your other devices is taken, kept with the
//! account's own as the latest for its index in its collection, and sent back, as whatsmeow fetches
//! a collection again after each patch, unless a test has it refused or its echo lost, or holds it
//! at a gate; a full fetch sends what it keeps in the collections named, or in every collection,
//! unless a test has fetches fail. Given an `Account`, it answers for its contacts and groups, and
//! writes each history blob its phone offers to the spool as jaw-engine does; events can also be
//! delivered from outside its script, several waiting for their acks at once. Its people type and
//! record as WhatsApp's chat presence says it, sent over the link as the `Typing` event jaw-engine
//! sends, in order with the rest.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use prost::Message as _;
use sha2::Digest as _;
use tokio::sync::{Notify, mpsc, oneshot};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_e2e::event_response_message::EventResponseType;

use super::pb::chat_presence::State as Said;
use super::{Ack, BoxFuture, Commands, Engine, EngineError, Incoming, Instance, Outgoing, PROTOCOL, pb};
use crate::convert::secret::{self, Between};
use crate::media::drive;
use crate::store::polls::option_hash;

mod pictures;

pub use pictures::Picture;

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
    /// Whether its people type and record once its script has played, as `new`'s do.
    typists: bool,
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
    /// How many app-state patches to refuse next, as WhatsApp refuses one it won't take.
    refusing_patches: Mutex<u32>,
    /// Whether app-state patches are taken and kept but never answered, as when the connection
    /// drops once the server has a patch.
    holding_patches: AtomicBool,
    /// How many app-state patches to take and keep next but fail as one to try again, as the engine
    /// fails a patch whose echo doesn't come back in time.
    losing_echoes: Mutex<u32>,
    /// Whether app-state patches wait to be answered until their gate opens.
    patches_gated: AtomicBool,
    /// Whether full fetches of app state fail as ones to try again, as when WhatsApp can't be
    /// reached.
    failing_fetches: AtomicBool,
    /// The app state the server keeps for every device: the account's, as its phone left it, and
    /// each patch it has taken since.
    app_state: Mutex<Held>,
    /// The running engine's half of the link, for events delivered from outside its script.
    link: Mutex<Option<WeakLink>>,
    /// Whose presence jawd follows, in the order it asked.
    subscribed: Mutex<Vec<String>>,
    /// The media server: each file by its direct path, as it decrypts.
    files: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    /// Direct paths the server no longer has a file for.
    expired: Mutex<HashSet<String>>,
    /// How the phone answers when asked to send a file again.
    phone: Mutex<Phone>,
    /// Whether downloads wait, partly written, until it opens.
    downloads_gated: AtomicBool,
    downloads_opened: Notify,
    /// The downloads running, by id, to cancel.
    running: Mutex<HashMap<String, Arc<Notify>>>,
    /// Numbers the partial files downloads write.
    parts: AtomicU64,
    /// Held while a downloaded file is sent to the drive and put in place, one at a time, as
    /// jaw-engine does.
    finishing: Mutex<()>,
    /// Each photo's direct path, by its message's ID, for the phone to send it again.
    photos: Mutex<HashMap<String, String>>,
    /// How many asks of the phone to fail next, and whether trying again could work.
    asks_failing: Mutex<(u32, bool)>,
    /// What a test does as the phone is asked, before it answers.
    #[cfg(test)]
    when_asked: Mutex<Option<WhenAsked>>,
    /// How often one of the people followed comes or goes, when not every `COMINGS`.
    comings: Mutex<Option<Duration>>,
    /// Whether uploads wait, half sent, until it opens.
    uploads_gated: AtomicBool,
    uploads_opened: Notify,
    /// How many uploads to fail next as a connection that's down does, and to refuse for good.
    failing_uploads: Mutex<u32>,
    refusing_uploads: Mutex<u32>,
}

/// What a test does as the made-up phone is asked to send a file again.
#[cfg(test)]
type WhenAsked = Box<dyn Fn(&pb::MediaRetry) + Send + Sync>;

/// The most of a file the media server keeps to be downloaded again; bigger ones it only counts.
const SERVED_MOST: usize = 32_000_000;

/// What the demo's upload read of a file: its hash, the hash of it as it went, its length, and the
/// file itself when the media server keeps it.
type Encrypted = (Vec<u8>, Vec<u8>, u64, Option<Vec<u8>>);

/// How the made-up phone answers when asked to send a file again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phone {
    /// It uploads the file again, a moment later, to a new direct path.
    #[default]
    Uploads,
    /// It no longer has the file.
    #[cfg(test)]
    Lost,
    /// It never answers, as when it's off.
    #[cfg(test)]
    Silent,
}

/// App state as WhatsApp's server keeps it: the latest mutation for each index in each collection,
/// in the order each index was first written.
#[derive(Default)]
struct Held {
    mutations: Vec<(String, pb::AppState)>,
    /// Where each collection's index is in `mutations`.
    at: HashMap<(String, Vec<String>), usize>,
}

impl World {
    /// Says `sender` is typing (`Composing`) or recording in `chat`, or has stopped, as WhatsApp
    /// says it, through the engine running now, which jawd takes only while WhatsApp would say it:
    /// while it's been told you're online. Nothing goes while no engine runs.
    pub async fn typing(&self, chat: &str, sender: &str, state: pb::chat_presence::State) {
        let events = self.link.lock().expect("the demo world").as_ref().and_then(|link| link.events.upgrade());
        if let Some(events) = events {
            let _ = events.send(Incoming { event: typing(chat, sender, state), ack: None }).await;
        }
    }

    /// Has one of the people jawd follows come or go every `every`, rather than every `COMINGS`.
    pub fn come_and_go_every(&self, every: Duration) {
        *self.comings.lock().expect("the demo world") = Some(every);
    }

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

    /// The full fetches of app state played so far.
    pub fn fetched(&self) -> Vec<Fetched> {
        self.fetched.lock().expect("the demo world").clone()
    }

    /// Has the media server hold `data` at `direct_path`.
    pub fn serve(&self, direct_path: &str, data: Arc<Vec<u8>>) {
        self.files.lock().expect("the demo world").insert(direct_path.to_string(), data);
    }

    /// A photo from `sender` in `chat`, `picture`, which the media server then holds and the phone
    /// can send again.
    pub fn photo(&self, picture: &Picture, chat: &str, sender: &str, id: &str, at_ms: i64, caption: &str) -> pb::Event {
        self.serve(&picture.direct_path, picture.data.clone());
        self.photos.lock().expect("the demo world").insert(id.to_string(), picture.direct_path.clone());
        let key: Vec<u8> = (0..32u8).map(|n| n ^ id.bytes().fold(0u8, |a, b| a.wrapping_add(b))).collect();
        let image = e2e::ImageMessage {
            mimetype: Some("image/jpeg".into()),
            caption: (!caption.is_empty()).then(|| caption.to_string()),
            file_sha256: Some(picture.sha256.clone()),
            file_length: Some(picture.data.len() as u64),
            width: Some(picture.width),
            height: Some(picture.height),
            media_key: Some(key),
            file_enc_sha256: Some(picture.sha256.iter().rev().copied().collect()),
            direct_path: Some(picture.direct_path.clone()),
            jpeg_thumbnail: Some(picture.thumbnail.clone()),
            ..Default::default()
        };
        let raw = e2e::Message { image_message: Some(Box::new(image)), ..Default::default() };
        let Some(pb::event::Kind::Message(mut message)) = text(chat, sender, id, at_ms, "").kind else {
            unreachable!("a text is a message")
        };
        message.raw = raw.encode_to_vec();
        if let Some(info) = message.info.as_mut() {
            info.r#type = "media".into();
            info.media_type = "image".into();
        }
        pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(message)) }
    }

    /// A document from `sender` in `chat`: `data`, named `name`, which the media server then holds.
    #[allow(clippy::too_many_arguments)]
    pub fn document(
        &self,
        chat: &str,
        sender: &str,
        id: &str,
        at_ms: i64,
        name: &str,
        mime: &str,
        data: &[u8],
        caption: &str,
    ) -> pb::Event {
        let (sha256, key, direct_path) = self.served(id, data);
        let document = e2e::DocumentMessage {
            mimetype: Some(mime.into()),
            file_name: Some(name.into()),
            title: Some(name.into()),
            caption: (!caption.is_empty()).then(|| caption.to_string()),
            file_sha256: Some(sha256.clone()),
            file_length: Some(data.len() as u64),
            media_key: Some(key),
            file_enc_sha256: Some(sha256.iter().rev().copied().collect()),
            direct_path: Some(direct_path),
            ..Default::default()
        };
        let raw = e2e::Message { document_message: Some(Box::new(document)), ..Default::default() };
        media(chat, sender, id, at_ms, raw, "document")
    }

    /// A voice message from `sender` in `chat`, `seconds` long, of `data`, an Ogg file, which the
    /// media server then holds.
    pub fn voice(&self, data: Arc<Vec<u8>>, chat: &str, sender: &str, id: &str, at_ms: i64, seconds: u32) -> pb::Event {
        let sha256 = <sha2::Sha256 as sha2::Digest>::digest(data.as_slice()).to_vec();
        let name: String = sha256.iter().take(8).map(|b| format!("{b:02x}")).collect();
        let direct_path = format!("/v/t62.7117-24/{name}.enc?ccb=11-4");
        self.serve(&direct_path, data.clone());
        let audio = e2e::AudioMessage {
            mimetype: Some("audio/ogg; codecs=opus".into()),
            file_sha256: Some(sha256.clone()),
            file_length: Some(data.len() as u64),
            seconds: Some(seconds),
            ptt: Some(true),
            media_key: Some((0..32u8).map(|n| n ^ 0x5a).collect()),
            file_enc_sha256: Some(sha256.iter().rev().copied().collect()),
            direct_path: Some(direct_path),
            ..Default::default()
        };
        let raw = e2e::Message { audio_message: Some(Box::new(audio)), ..Default::default() };
        let Some(pb::event::Kind::Message(mut message)) = text(chat, sender, id, at_ms, "").kind else {
            unreachable!("a text is a message")
        };
        message.raw = raw.encode_to_vec();
        if let Some(info) = message.info.as_mut() {
            info.r#type = "media".into();
            info.media_type = "ptt".into();
        }
        pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(message)) }
    }

    /// Has the media server hold `data` for message `id`: its hash, the key it's sent with, and
    /// where the server has it.
    fn served(&self, id: &str, data: &[u8]) -> (Vec<u8>, Vec<u8>, String) {
        let sha256 = sha2::Sha256::digest(data).to_vec();
        let direct_path = format!("/v/demo/{}.enc?ccb=11-4", pictures::hex(&sha256[..8]));
        self.serve(&direct_path, Arc::new(data.to_vec()));
        let key: Vec<u8> = (0..32u8).map(|n| n ^ id.bytes().fold(0u8, |a, b| a.wrapping_add(b))).collect();
        (sha256, key, direct_path)
    }

    /// Has the media server answer 404 for `direct_path`, as WhatsApp's does for old media.
    #[cfg(test)]
    pub fn expire(&self, direct_path: &str) {
        self.expired.lock().expect("the demo world").insert(direct_path.to_string());
    }

    /// Sets how the phone answers when asked to send a file again.
    #[cfg(test)]
    pub fn phone(&self, phone: Phone) {
        *self.phone.lock().expect("the demo world") = phone;
    }

    /// Fails the next `n` asks of the phone to send a file again, saying whether trying again could
    /// work.
    #[cfg(test)]
    pub fn fail_asks(&self, n: u32, retry: bool) {
        *self.asks_failing.lock().expect("the demo world") = (n, retry);
    }

    /// Does `when` each time the phone is asked to send a file again, before it answers.
    #[cfg(test)]
    pub fn when_asked(&self, when: impl Fn(&pb::MediaRetry) + Send + Sync + 'static) {
        *self.when_asked.lock().expect("the demo world") = Some(Box::new(when));
    }

    /// Holds downloads partly written while `gated`, letting them go on once it opens.
    #[cfg(test)]
    pub fn gate_downloads(&self, gated: bool) {
        self.downloads_gated.store(gated, Ordering::SeqCst);
        if !gated {
            self.downloads_opened.notify_waiters();
        }
    }

    /// Holds uploads half sent while `gated`, letting them go on once it opens.
    #[cfg(test)]
    pub fn gate_uploads(&self, gated: bool) {
        self.uploads_gated.store(gated, Ordering::SeqCst);
        if !gated {
            self.uploads_opened.notify_waiters();
        }
    }

    /// Fails the next `n` uploads as a connection that's down does, which may pass.
    #[cfg(test)]
    pub fn fail_uploads(&self, n: u32) {
        *self.failing_uploads.lock().expect("the demo world") = n;
    }

    /// Refuses the next `n` uploads for good, as a media server refuses a file it won't take.
    #[cfg(test)]
    pub fn refuse_uploads(&self, n: u32) {
        *self.refusing_uploads.lock().expect("the demo world") = n;
    }

    /// The uploads it was asked for, in order.
    #[cfg(test)]
    pub fn uploads(&self) -> Vec<pb::MediaUpload> {
        self.asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::MediaUpload(upload) => Some(upload),
                _ => None,
            })
            .collect()
    }

    /// The file the media server holds at `direct_path`, as it decrypts.
    #[cfg(test)]
    pub fn file_at(&self, direct_path: &str) -> Option<Arc<Vec<u8>>> {
        self.files.lock().expect("the demo world").get(direct_path).cloned()
    }

    /// How many downloads it was asked for.
    #[cfg(test)]
    pub fn downloads(&self) -> usize {
        self.asked().iter().filter(|kind| matches!(kind, pb::command::Kind::MediaDownload(_))).count()
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

    /// Refuses the next `n` app-state patches for good.
    #[cfg(test)]
    pub fn refuse_patches(&self, n: u32) {
        *self.refusing_patches.lock().expect("the demo world") = n;
    }

    /// Takes and keeps app-state patches but never answers them, as when the connection drops once
    /// the server has each, until it's told otherwise.
    #[cfg(test)]
    pub fn hold_patches(&self, holding: bool) {
        self.holding_patches.store(holding, Ordering::SeqCst);
    }

    /// Takes and keeps the next `n` app-state patches, but fails each as one to try again, as the
    /// engine fails a patch WhatsApp took whose echo didn't come back in time.
    #[cfg(test)]
    pub fn lose_echoes(&self, n: u32) {
        *self.losing_echoes.lock().expect("the demo world") = n;
    }

    /// Holds app-state patches unanswered while `gated`, as a server that hasn't answered yet,
    /// answering them all once it opens.
    #[cfg(test)]
    pub fn gate_patches(&self, gated: bool) {
        self.patches_gated.store(gated, Ordering::SeqCst);
        if !gated {
            self.opened.notify_waiters();
        }
    }

    /// Fails every full fetch of app state as one to try again, as when WhatsApp can't be reached,
    /// until it's told otherwise.
    #[cfg(test)]
    pub fn fail_fetches(&self, failing: bool) {
        self.failing_fetches.store(failing, Ordering::SeqCst);
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

    /// Keeps `state` as the latest for its index in `collection`.
    fn keep(&self, collection: &str, state: pb::AppState) {
        let mut held = self.app_state.lock().expect("the demo world");
        let held = &mut *held;
        match held.at.entry((collection.to_string(), state.index.clone())) {
            Entry::Occupied(at) => held.mutations[*at.get()].1 = state,
            Entry::Vacant(at) => {
                at.insert(held.mutations.len());
                held.mutations.push((collection.to_string(), state));
            }
        }
    }

    /// What the server keeps in collections `names`, or in every collection when it names none, as
    /// a full fetch brings it.
    fn kept_in(&self, names: &[String]) -> Vec<pb::Event> {
        let held = self.app_state.lock().expect("the demo world");
        held.mutations
            .iter()
            .filter(|(collection, _)| names.is_empty() || names.contains(collection))
            .map(|(_, state)| pb::Event { seq: 0, kind: Some(pb::event::Kind::AppState(state.clone())) })
            .collect()
    }
}

/// Four seconds of someone speaking, as a phone records a voice message: Ogg Opus at 16 kHz.
const SPEECH: &[u8] = include_bytes!("../../../jaw-voice/tests/fixtures/speech-16k.ogg");

/// The plan the made-up climbing group shares for its Saturday, as a file.
const SATURDAY: &[u8] = b"Sintra, Saturday\n\n09:00 meet at Rossio, the 09:11 train\n10:00 Peninha, the slabs first\n\
13:30 lunch in the village\n16:00 back down, the 17:40 train\n\nBring the 60 m rope and the small rack.\n";

/// The collection WhatsApp keeps a mutation in, by the kind its index starts with, as whatsmeow
/// groups its app-state indexes; any other kind in regular.
fn collection(index: &[String]) -> &'static str {
    match index.first().map(String::as_str) {
        Some("setting_pushName" | "setting_locale" | "setting_securityNotification") => "critical_block",
        Some("contact" | "lid_contact") => "critical_unblock_low",
        Some("star" | "mute" | "deleteMessageForMe" | "clearChat" | "deleteChat" | "userStatusMute") => "regular_high",
        Some("pin_v1" | "archive" | "markChatAsRead" | "setting_unarchiveChats") => "regular_low",
        _ => "regular",
    }
}

impl DemoEngine {
    /// The made-up account: connected, then a few messages arriving, a photo, a file and a voice
    /// message among them.
    pub fn new() -> DemoEngine {
        let now = now_ms();
        let ana = "351912345678@s.whatsapp.net";
        let climbing = "120363012345678901@g.us";
        let rui = "351913333333@s.whatsapp.net";
        let world = Arc::new(World::default());
        let lake = Picture::lake(1600, 1200);
        let script = vec![
            connected(),
            text(ana, ana, "3EB0DEMO00000001", now - 120_000, "did you get the photos from Sunday?"),
            text(climbing, rui, "3EB0DEMO00000002", now - 60_000, "Saturday at 9?"),
            world.photo(&lake, ana, ana, "3EB0DEMO00000008", now - 30_000, "Sunday, at the lake"),
            world.voice(Arc::new(SPEECH.to_vec()), ana, ana, "3EB0DEMO00000010", now - 20_000, 4),
            text(ana, ana, "3EB0DEMO00000003", now - 5_000, "the one at the lake"),
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
            event(
                climbing,
                rui,
                "3EB0DEMO00000009",
                now - 400_000,
                &Planned {
                    name: "Sintra",
                    description: "Bring the rope",
                    at: (saturday(now), saturday(now) + 6 * 3_600),
                    place: (38.780_3, -9.498_9, "Cabo da Roca", "Estrada do Cabo da Roca, Colares"),
                },
            ),
            answer(
                climbing,
                MEMBERS[1],
                "3EB0DEMO0000000A",
                now - 390_000,
                ("3EB0DEMO00000009", rui),
                EventResponseType::Going,
            ),
            // Yours, from your phone.
            answer(
                climbing,
                ME,
                "3EB0DEMO0000000B",
                now - 380_000,
                ("3EB0DEMO00000009", rui),
                EventResponseType::Maybe,
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
            location(
                ana,
                ana,
                "3EB0DEMO0000000C",
                now - 900_000,
                (38.716_25, -9.131_97),
                ("Miradouro da Graça", "Calçada da Graça, 1100-265 Lisboa"),
            ),
            world.document(
                climbing,
                rui,
                "3EB0DEMO0000000D",
                now - 250_000,
                "sintra-saturday.txt",
                "text/plain",
                SATURDAY,
                "the plan for Saturday",
            ),
            // Earlier than the rest too: Ana's dentist's card, and two climbers' as one message.
            contact(ana, ana, "3EB0DEMO0000000F", now - 900_000, &[("Dr.ª Marta Lopes", MARTA)]),
            contact(
                climbing,
                rui,
                "3EB0DEMO0000000E",
                now - 450_000,
                &[("Inês Gomes", INES), ("Tiago Martins", TIAGO)],
            ),
            // You and the two whose phones answer what you send there, so a mention of one lands;
            // told once the messages are in, so storing them holds none of the messages back, and
            // last, so an interface that sees the group named has the whole script.
            group(climbing, "Lisbon Climbing", &[ME, MEMBERS[0], MEMBERS[1]]),
        ];
        DemoEngine { script, world, pause: Duration::ZERO, typists: false }.typing()
    }

    /// The same, its people typing and recording once its script has played (`typists`).
    fn typing(self) -> DemoEngine {
        DemoEngine { typists: true, ..self }
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
        DemoEngine { script, world: Arc::default(), pause: Duration::ZERO, typists: false }
    }

    /// The made-up `account`, which plays `script` and answers what jawd asks about it; its WhatsApp
    /// keeps the app state its phone has.
    pub fn for_account(script: Vec<pb::Event>, account: Arc<dyn Account>) -> DemoEngine {
        let world = World { account: Some(account.clone()), ..World::default() };
        for event in account.app_state() {
            if let Some(pb::event::Kind::AppState(state)) = event.kind {
                world.keep(collection(&state.index), state);
            }
        }
        DemoEngine { script, world: Arc::new(world), pause: Duration::ZERO, typists: false }
    }

    /// An engine on `world`, as an engine started again talks to the same WhatsApp.
    #[cfg(test)]
    pub fn sharing(script: Vec<pb::Event>, world: Arc<World>) -> DemoEngine {
        DemoEngine { script, world, pause: Duration::ZERO, typists: false }
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

/// A message from `sender` in `chat` carrying the file `raw` describes, of `media_type`.
fn media(chat: &str, sender: &str, id: &str, at_ms: i64, raw: e2e::Message, media_type: &str) -> pb::Event {
    let Some(pb::event::Kind::Message(mut message)) = text(chat, sender, id, at_ms, "").kind else {
        unreachable!("a text is a message")
    };
    message.raw = raw.encode_to_vec();
    if let Some(info) = message.info.as_mut() {
        info.r#type = "media".into();
        info.media_type = media_type.into();
    }
    pb::Event { seq: 0, kind: Some(pb::event::Kind::Message(message)) }
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

/// A pin from `sender` in `chat` at `(lat, lon)`, with the place's name and address, as the phones
/// send one.
pub fn location(chat: &str, sender: &str, id: &str, at_ms: i64, at: (f64, f64), place: (&str, &str)) -> pb::Event {
    let raw = e2e::Message {
        location_message: Some(Box::new(e2e::LocationMessage {
            degrees_latitude: Some(at.0),
            degrees_longitude: Some(at.1),
            name: Some(place.0.into()),
            address: Some(place.1.into()),
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

/// The secret the demo's events go with, as the phones give each event one of its own.
const EVENT_SECRET: [u8; 32] = [9; 32];

/// 9:00 UTC on the first Saturday after `now_ms`'s day (UTC), a week on from a Saturday, in seconds
/// since the epoch.
fn saturday(now_ms: i64) -> i64 {
    let days = now_ms.div_euclid(86_400_000);
    // The epoch was a Thursday; Saturday is two days on.
    let ahead = match (2 - days.rem_euclid(7)).rem_euclid(7) {
        0 => 7,
        n => n,
    };
    (days + ahead) * 86_400 + 9 * 3_600
}

/// An event as someone plans one: its name, what it says, when it starts and ends (seconds since
/// the epoch) and where (a pin, with the place's name and address).
pub struct Planned<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub at: (i64, i64),
    pub place: (f64, f64, &'a str, &'a str),
}

/// An event from `sender` in `chat`, with a secret of its own, as the phones send one.
pub fn event(chat: &str, sender: &str, id: &str, at_ms: i64, planned: &Planned) -> pb::Event {
    let (lat, lon, name, address) = planned.place;
    let raw = e2e::Message {
        event_message: Some(Box::new(e2e::EventMessage {
            name: Some(planned.name.into()),
            description: Some(planned.description.into()),
            start_time: Some(planned.at.0),
            end_time: Some(planned.at.1),
            location: Some(Box::new(e2e::LocationMessage {
                degrees_latitude: Some(lat),
                degrees_longitude: Some(lon),
                name: Some(name.into()),
                address: Some(address.into()),
                ..Default::default()
            })),
            ..Default::default()
        })),
        message_context_info: Some(e2e::MessageContextInfo {
            message_secret: Some(EVENT_SECRET.to_vec()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut event = text(chat, sender, id, at_ms, "");
    if let Some(pb::event::Kind::Message(message)) = &mut event.kind {
        message.raw = raw.encode_to_vec();
    }
    event
}

/// `who`'s answer, as message `id`, to the event `event` (its ID and who sent it), sealed with
/// the demo's events' secret as the phones seal one.
pub fn answer(chat: &str, who: &str, id: &str, at_ms: i64, event: (&str, &str), said: EventResponseType) -> pb::Event {
    answer_sealed(chat, who, id, at_ms, (event.0, event.1, &EVENT_SECRET), said)
}

/// `who`'s answer, as message `id`, to the event `event` (its ID, who sent it and its secret).
fn answer_sealed(
    chat: &str,
    who: &str,
    id: &str,
    at_ms: i64,
    event: (&str, &str, &[u8]),
    said: EventResponseType,
) -> pb::Event {
    let (event_id, author, secret) = event;
    let plain =
        e2e::EventResponseMessage { response: Some(said as i32), timestamp_ms: Some(at_ms), ..Default::default() };
    let between = Between { id: event_id, author, who };
    let iv = [1; 12];
    let raw = e2e::Message {
        enc_event_response_message: Some(e2e::EncEventResponseMessage {
            event_creation_message_key: Some(MessageKey {
                remote_jid: Some(chat.into()),
                from_me: Some(author == ME),
                id: Some(event_id.into()),
                participant: (chat.ends_with("@g.us") && author != ME).then(|| author.into()),
            }),
            enc_payload: Some(secret::seal(secret, secret::EVENT_RESPONSE, between, iv, &plain.encode_to_vec())),
            enc_iv: Some(iv.to_vec()),
        }),
        ..Default::default()
    };
    let mut answer = text(chat, who, id, at_ms, "");
    if let Some(pb::event::Kind::Message(message)) = &mut answer.kind {
        message.raw = raw.encode_to_vec();
        if let Some(info) = message.info.as_mut() {
            info.from_me = who == ME;
        }
    }
    answer
}

/// What a recipient's phone answers to an event of yours, `send`, once it has read it: going.
fn answering(send: &pb::Send, who: &str) -> Option<pb::Event> {
    let sent = e2e::Message::decode(send.raw.as_slice()).ok()?;
    sent.event_message.as_ref()?;
    let secret = sent.message_context_info.as_ref()?.message_secret.as_deref()?;
    let id = format!("{}-going", send.id);
    Some(answer_sealed(&send.chat, who, &id, now_ms(), (&send.id, ME, secret), EventResponseType::Going))
}

/// The made-up account's contact cards, as WhatsApp's phones write them.
const MARTA: &str = "BEGIN:VCARD\nVERSION:3.0\nN:Lopes;Marta;;Dr.ª;\nFN:Dr.ª Marta Lopes\nORG:Clínica da Graça;\n\
    item1.TEL;waid=351213456789:+351 21 345 6789\nitem1.X-ABLabel:Mobile\nTEL;type=WORK:21 000 00 00\nEND:VCARD";
const INES: &str = "BEGIN:VCARD\nVERSION:3.0\nN:Gomes;Inês;;;\nFN:Inês Gomes\n\
    item1.TEL;waid=351913002210:+351 913 002 210\nitem1.X-ABLabel:Mobile\nEND:VCARD";
const TIAGO: &str = "BEGIN:VCARD\nVERSION:3.0\nN:Martins;Tiago;;;\nFN:Tiago Martins\n\
    item1.TEL;waid=351916666666:+351 916 666 666\nitem1.X-ABLabel:Mobile\nEND:VCARD";

/// Contact cards from `sender` in `chat`, each a name and a vCard, as the phones send them: one as
/// a contact message, several as one message of them all.
pub fn contact(chat: &str, sender: &str, id: &str, at_ms: i64, cards: &[(&str, &str)]) -> pb::Event {
    let mut cards: Vec<e2e::ContactMessage> = cards
        .iter()
        .map(|(name, vcard)| e2e::ContactMessage {
            display_name: Some(name.to_string()),
            vcard: Some(vcard.to_string()),
            ..Default::default()
        })
        .collect();
    let raw = match cards.len() {
        1 => e2e::Message { contact_message: Some(Box::new(cards.remove(0))), ..Default::default() },
        n => {
            let array = e2e::ContactsArrayMessage {
                display_name: Some(format!("{n} contacts")),
                contacts: cards,
                context_info: None,
            };
            e2e::Message { contacts_array_message: Some(Box::new(array)), ..Default::default() }
        }
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

/// What the made-up people say of themselves.
const ABOUT: [&str; 6] = ["", "Disponível", "a escalar, não ligar", "Lisboa ⇄ Porto", "at the studio", "🌊"];

/// A number of its own for each JID, the same every run.
fn seed(jid: &str) -> u64 {
    jid.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

/// `jid` around, or away and last seen a while ago.
fn presence(jid: &str, around: bool) -> pb::Event {
    let ago = 60_000 * (1 + seed(jid) % 600) as i64;
    let update = pb::PresenceUpdate {
        jid: jid.into(),
        unavailable: !around,
        last_seen_ms: if around { 0 } else { now_ms() - ago },
    };
    pb::Event { seq: 0, kind: Some(pb::event::Kind::Presence(update)) }
}

/// `sender` typing, recording or stopping in `chat`, as jaw-engine hands on WhatsApp's chat
/// presence.
pub fn typing(chat: &str, sender: &str, state: Said) -> pb::Event {
    let typing =
        pb::Typing { chat: chat.into(), sender: sender.into(), state: state as i32, sender_alt: String::new() };
    pb::Event { seq: 0, kind: Some(pb::event::Kind::Typing(typing)) }
}

/// How often one of the people followed comes or goes.
const COMINGS: Duration = Duration::from_secs(5);

/// The people jawd follows come and go, one every few seconds, each sometimes typing first, for as
/// long as jawd is there to hear it.
async fn comings_and_goings(sender: Sender) {
    let mut around: HashMap<String, bool> = HashMap::new();
    for turn in 0u64.. {
        let every = sender.world.comings.lock().expect("the demo world").unwrap_or(COMINGS);
        tokio::time::sleep(every).await;
        let subscribed = sender.world.subscribed.lock().expect("the demo world").clone();
        let Some(jid) = subscribed.get(turn as usize % subscribed.len().max(1)).cloned() else { continue };
        let now = !around.get(&jid).copied().unwrap_or(seed(&jid).is_multiple_of(3));
        around.insert(jid.clone(), now);
        if now
            && turn % 3 == 0
            && jid.ends_with("@s.whatsapp.net")
            && !sender.send(typing(&jid, &jid, Said::Composing)).await
        {
            return;
        }
        if !sender.send(presence(&jid, now)).await {
            return;
        }
    }
}

/// How often the made-up account's people say again that they're typing or recording, as the
/// phones do every few seconds while it goes on.
const TYPING_AGAIN: Duration = Duration::from_secs(4);

/// The made-up account's people type and record for as long as the engine runs: Ana in her chat,
/// Rui recording in Lisbon Climbing, each said again every few seconds, and the group's other
/// member typing there and stopping in turn. jawd hears it only while WhatsApp would say it.
async fn typing_while_running(sender: Sender) {
    let (ana, climbing) = ("351912345678@s.whatsapp.net", "120363012345678901@g.us");
    for turn in 0u64.. {
        let other = if turn % 2 == 0 { Said::Composing } else { Said::Paused };
        let said = [
            typing(ana, ana, Said::Composing),
            typing(climbing, MEMBERS[0], Said::Recording),
            typing(climbing, MEMBERS[1], other),
        ];
        // Until this engine is stopped, or jawd has gone.
        for event in said {
            if sender.weak.upgrade().is_none() || !sender.send(event).await {
                return;
            }
        }
        tokio::time::sleep(TYPING_AGAIN).await;
    }
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
pub(super) fn write_whole(path: &Path, data: &[u8]) -> std::io::Result<()> {
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
        let (pause, typists) = (self.pause, self.typists);
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
                        return;
                    }
                }
                if typists {
                    typing_while_running(playing).await;
                }
            });
            tokio::spawn(comings_and_goings(sender.clone()));
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
                    | pb::event::Kind::MediaRetried(_)
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
            | pb::command::Kind::Presence(_)) => {
                self.world.ask(kind);
                done
            }
            pb::command::Kind::SubscribePresence(asked) => {
                self.world.ask(pb::command::Kind::SubscribePresence(asked.clone()));
                let mut subscribed = self.world.subscribed.lock().expect("the demo world");
                if !subscribed.contains(&asked.jid) {
                    subscribed.push(asked.jid.clone());
                    let (sender, jid) = (self.clone(), asked.jid.clone());
                    tokio::spawn(async move { sender.send(presence(&jid, seed(&jid).is_multiple_of(3))).await });
                }
                Ok(pb::reply::Kind::Done(pb::Done {}))
            }
            pb::command::Kind::UserInfo(asked) => {
                self.world.ask(pb::command::Kind::UserInfo(asked.clone()));
                let users = asked
                    .jids
                    .iter()
                    .map(|jid| pb::User {
                        jid: jid.clone(),
                        about: ABOUT[seed(jid) as usize % ABOUT.len()].into(),
                        ..Default::default()
                    })
                    .collect();
                Ok(pb::reply::Kind::UserInfoList(pb::UserInfoList { users }))
            }
            pb::command::Kind::AppStatePatch(patch) => {
                self.world.ask(pb::command::Kind::AppStatePatch(patch.clone()));
                self.patched(patch).await
            }
            kind @ pb::command::Kind::Contacts(_) => {
                self.world.ask(kind);
                Ok(pb::reply::Kind::ContactList(self.account()?.contacts()))
            }
            kind @ pb::command::Kind::Groups(_) => {
                self.world.ask(kind);
                Ok(pb::reply::Kind::GroupList(self.account()?.groups()))
            }
            pb::command::Kind::LidLookup(asked) => {
                Ok(pb::reply::Kind::LidPairs(self.account()?.lid_pairs(&asked.lids)))
            }
            // What the server keeps in the collections named, or in every collection when it names
            // none, comes a mutation at a time, each waiting for its ack, as whatsmeow hands each
            // over as it fetches each collection again in full.
            pb::command::Kind::AppStateResync(resync) => {
                self.world.ask(pb::command::Kind::AppStateResync(resync.clone()));
                if self.world.failing_fetches.load(Ordering::SeqCst) {
                    return Err(EngineError::Failed { message: "websocket not connected".into(), retry: true });
                }
                let (began, mut acks) = (Instant::now(), Vec::new());
                for mutation in self.world.kept_in(&resync.names) {
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
            pb::command::Kind::MediaDownload(download) => {
                self.world.ask(pb::command::Kind::MediaDownload(download.clone()));
                self.download(download).await
            }
            pb::command::Kind::MediaUpload(upload) => {
                self.world.ask(pb::command::Kind::MediaUpload(upload.clone()));
                self.upload(upload).await
            }
            pb::command::Kind::MediaCancel(cancel) => {
                if let Some(stop) = self.world.running.lock().expect("the demo world").get(&cancel.id) {
                    stop.notify_one();
                }
                done
            }
            pb::command::Kind::MediaRetry(retry) => {
                self.world.ask(pb::command::Kind::MediaRetry(retry.clone()));
                #[cfg(test)]
                if let Some(when) = self.world.when_asked.lock().expect("the demo world").as_ref() {
                    when(&retry);
                }
                {
                    let mut failing = self.world.asks_failing.lock().expect("the demo world");
                    if failing.0 > 0 {
                        failing.0 -= 1;
                        let message = "the phone can't be reached just now".into();
                        return Err(EngineError::Failed { message, retry: failing.1 });
                    }
                }
                self.send_again(retry);
                done
            }
            _ => Err(EngineError::Failed { message: "the demo engine doesn't do that".into(), retry: false }),
        }
    }

    fn progress(&self, id: &str, bytes: u64, size: u64) {
        let progress = pb::MediaProgress { id: id.to_string(), bytes, size };
        let _ = self.events.try_send(Incoming {
            event: pb::Event { seq: 0, kind: Some(pb::event::Kind::MediaProgress(progress)) },
            ack: None,
        });
    }

    /// Writes the file a download names where jawd asked, as jaw-engine does once it has decrypted
    /// it: through a partial file, checked against its hash, sent to the drive ordered before
    /// whatever follows, and linked in under its name, never over a file there, one at a time; half
    /// way, it waits while downloads are gated, and can be cancelled.
    async fn download(&self, download: pb::MediaDownload) -> Result<pb::reply::Kind, EngineError> {
        use pb::media_file::State;
        let file = |state: State, bytes: u64, message: &str| {
            Ok(pb::reply::Kind::MediaFile(pb::MediaFile { state: state as i32, bytes, message: message.into() }))
        };
        let path = PathBuf::from(&download.path);
        if let Ok(there) = std::fs::symlink_metadata(&path) {
            return file(State::Exists, there.len(), "");
        }
        let world = &self.world;
        let data = {
            let expired = world.expired.lock().expect("the demo world").contains(&download.direct_path);
            world.files.lock().expect("the demo world").get(&download.direct_path).cloned().filter(|_| !expired)
        };
        let Some(data) = data else { return file(State::Gone, 0, "the server answered 404") };
        let stop = Arc::new(Notify::new());
        world.running.lock().expect("the demo world").insert(download.id.clone(), stop.clone());
        let part = PathBuf::from(format!(
            "{}.{}-{}.part",
            download.path,
            std::process::id(),
            world.parts.fetch_add(1, Ordering::Relaxed)
        ));
        let half = data.len() / 2;
        let first = {
            let (part, data) = (part.clone(), data.clone());
            tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
                let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&part)?;
                out.write_all(&data[..half])?;
                Ok(out)
            })
            .await
        };
        let out = match first {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                world.running.lock().expect("the demo world").remove(&download.id);
                return Err(EngineError::Failed { message: format!("can't write the file: {e}"), retry: true });
            }
            Err(e) => return Err(EngineError::Failed { message: format!("writing stopped: {e}"), retry: true }),
        };
        self.progress(&download.id, half as u64, download.size);
        let cancelled = loop {
            let opened = world.downloads_opened.notified();
            if !world.downloads_gated.load(Ordering::SeqCst) {
                break false;
            }
            tokio::select! {
                () = opened => {}
                () = stop.notified() => break true,
            }
        };
        world.running.lock().expect("the demo world").remove(&download.id);
        if cancelled {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return file(State::Cancelled, 0, "the download was cancelled");
        }
        let (sha256, total, full) = (download.file_sha256.clone(), data.len() as u64, download.full_flush);
        let shared = self.world.clone();
        let finished = tokio::task::spawn_blocking(move || -> std::io::Result<State> {
            let mut out = out;
            let placed = (|| {
                out.write_all(&data[half..])?;
                if <sha2::Sha256 as sha2::Digest>::digest(data.as_slice()).as_slice() != sha256.as_slice() {
                    return Ok(State::BadHash);
                }
                let _one = shared.finishing.lock().expect("the demo world");
                drive::barrier(&out, full)?;
                match std::fs::hard_link(&part, &path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(State::Exists),
                    Err(e) => return Err(e),
                }
                std::fs::remove_file(&part)?;
                drive::barrier(&std::fs::File::open(path.parent().unwrap_or(Path::new(".")))?, full)?;
                Ok(State::Placed)
            })();
            let _ = std::fs::remove_file(&part);
            placed
        })
        .await;
        match finished {
            Ok(Ok(State::Placed)) => {
                self.progress(&download.id, total, download.size);
                file(State::Placed, total, "")
            }
            Ok(Ok(State::BadHash)) => file(State::BadHash, 0, "the file doesn't match its message's hash"),
            Ok(Ok(state)) => file(state, total, ""),
            Ok(Err(e)) => Err(EngineError::Failed { message: format!("can't write the file: {e}"), retry: true }),
            Err(e) => Err(EngineError::Failed { message: format!("writing stopped: {e}"), retry: true }),
        }
    }

    /// Uploads the file jawd names as jaw-engine does: read as a stream and checked against its
    /// hash, encrypted into a partial file of its own beside the name jawd gave, which goes once it's
    /// sent; half way, it waits while uploads are gated, and can be cancelled. The media server then
    /// holds it, a small one to be downloaded again, under a direct path of its own.
    async fn upload(&self, upload: pb::MediaUpload) -> Result<pb::reply::Kind, EngineError> {
        use pb::media_uploaded::State;
        let answer = |state: State, message: &str| {
            Ok(pb::reply::Kind::MediaUploaded(pb::MediaUploaded {
                state: state as i32,
                message: message.into(),
                ..Default::default()
            }))
        };
        let world = &self.world;
        let next = |counter: &Mutex<u32>| {
            let mut left = counter.lock().expect("the demo world");
            let now = *left > 0;
            *left = left.saturating_sub(1);
            now
        };
        if next(&world.refusing_uploads) {
            return Err(EngineError::Failed {
                message: "WhatsApp's media server refused the file with 413".into(),
                retry: false,
            });
        }
        if next(&world.failing_uploads) {
            return Err(EngineError::Failed {
                message: "can't upload the file: websocket not connected".into(),
                retry: true,
            });
        }
        let part = PathBuf::from(format!(
            "{}.{}-{}.part",
            upload.temp,
            std::process::id(),
            world.parts.fetch_add(1, Ordering::Relaxed)
        ));
        let (path, kept) = (PathBuf::from(&upload.path), part.clone());
        let encrypted = tokio::task::spawn_blocking(move || -> std::io::Result<Encrypted> {
            use sha2::Digest;
            use std::io::Read;
            let mut file = std::fs::File::open(&path)?;
            let mut out = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&kept)?;
            let (mut plain, mut enc) = (sha2::Sha256::new(), sha2::Sha256::new());
            let mut served = Some(Vec::new());
            let (mut piece, mut length) = (vec![0u8; 64 * 1024], 0u64);
            loop {
                let n = file.read(&mut piece)?;
                if n == 0 {
                    break;
                }
                plain.update(&piece[..n]);
                length += n as u64;
                served = served.filter(|s: &Vec<u8>| s.len() + n <= SERVED_MOST).map(|mut s| {
                    s.extend_from_slice(&piece[..n]);
                    s
                });
                for b in &mut piece[..n] {
                    *b ^= 0x5a;
                }
                enc.update(&piece[..n]);
                out.write_all(&piece[..n])?;
            }
            Ok((plain.finalize().to_vec(), enc.finalize().to_vec(), length, served))
        })
        .await;
        let (sha256, enc_sha256, length, served) = match encrypted {
            Ok(Ok(done)) => done,
            Ok(Err(e)) => {
                let _ = std::fs::remove_file(&part);
                return Err(EngineError::Failed {
                    message: format!("can't read the file to upload: {e}"),
                    retry: true,
                });
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(EngineError::Failed { message: format!("uploading stopped: {e}"), retry: true });
            }
        };
        if sha256 != upload.file_sha256 {
            let _ = std::fs::remove_file(&part);
            return answer(State::Changed, "the file isn't the one jawd named: its SHA-256 doesn't match");
        }
        let stop = Arc::new(Notify::new());
        world.running.lock().expect("the demo world").insert(upload.id.clone(), stop.clone());
        let size = length.div_ceil(16) * 16 + 10;
        self.progress(&upload.id, size / 2, size);
        let cancelled = loop {
            let opened = world.uploads_opened.notified();
            if !world.uploads_gated.load(Ordering::SeqCst) {
                break false;
            }
            tokio::select! {
                () = opened => {}
                () = stop.notified() => break true,
            }
        };
        world.running.lock().expect("the demo world").remove(&upload.id);
        let _ = std::fs::remove_file(&part);
        if cancelled {
            return answer(State::Cancelled, "the upload was cancelled");
        }
        let hex: String = sha256.iter().map(|b| format!("{b:02x}")).collect();
        let direct_path = format!("/v/t62.7118-24/demo/{hex}.enc?ccb=11-4&oh=demo");
        if let Some(served) = served {
            world.serve(&direct_path, Arc::new(served));
        }
        self.progress(&upload.id, size, size);
        let media_key: Vec<u8> = <sha2::Sha256 as sha2::Digest>::digest([&sha256[..], b"key"].concat()).to_vec();
        Ok(pb::reply::Kind::MediaUploaded(pb::MediaUploaded {
            state: State::Uploaded as i32,
            url: format!("https://mmg.whatsapp.net{direct_path}"),
            direct_path,
            media_key,
            file_sha256: sha256,
            file_enc_sha256: enc_sha256,
            file_length: length,
            message: String::new(),
        }))
    }

    /// The phone, asked to send a message's file again, answers as the world says, a moment later:
    /// uploading it to a new direct path, saying it no longer has it, or not at all.
    fn send_again(&self, retry: pb::MediaRetry) {
        let (sender, phone) = (self.clone(), *self.world.phone.lock().expect("the demo world"));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let world = &sender.world;
            let was = world.photos.lock().expect("the demo world").get(&retry.id).cloned();
            let (result, direct_path) = match (phone, was) {
                #[cfg(test)]
                (Phone::Silent, _) => return,
                (Phone::Uploads, Some(was)) => {
                    let again = format!("{was}&again={}", world.parts.fetch_add(1, Ordering::Relaxed));
                    let data = world.files.lock().expect("the demo world").get(&was).cloned();
                    if let Some(data) = data {
                        world.serve(&again, data);
                    }
                    (pb::media_retried::Result::Success, again)
                }
                _ => (pb::media_retried::Result::NotFound, String::new()),
            };
            let answer = pb::MediaRetried {
                id: retry.id,
                chat: retry.chat,
                sender: retry.sender,
                from_me: retry.from_me,
                result: result as i32,
                direct_path,
                timestamp_ms: now_ms(),
            };
            sender.send(pb::Event { seq: 0, kind: Some(pb::event::Kind::MediaRetried(answer)) }).await;
        });
    }

    fn account(&self) -> Result<Arc<dyn Account>, EngineError> {
        self.world
            .account
            .clone()
            .ok_or_else(|| EngineError::Failed { message: "the demo engine doesn't do that".into(), retry: false })
    }

    /// WhatsApp takes an app-state patch, unless told to refuse it, and keeps it as the latest for
    /// its index in its collection; whatsmeow then fetches the collection again, so the patch comes
    /// back as an AppState event, acked before the patch is answered. A held patch is kept, and its
    /// answer lost; a patch whose echo is lost is kept, and failed as the engine fails one whose
    /// echo doesn't come back in time. A gated patch waits for its gate before any of this.
    async fn patched(&self, patch: pb::AppStatePatch) -> Result<pb::reply::Kind, EngineError> {
        let world = &self.world;
        loop {
            let opened = world.opened.notified();
            if !world.patches_gated.load(Ordering::SeqCst) {
                break;
            }
            opened.await;
        }
        {
            let mut refusing = world.refusing_patches.lock().expect("the demo world");
            if *refusing > 0 {
                *refusing -= 1;
                return Err(EngineError::Failed { message: "server returned error 400".into(), retry: false });
            }
        }
        let state = pb::AppState { index: patch.index, value: patch.value };
        world.keep(&patch.name, state.clone());
        {
            let mut losing = world.losing_echoes.lock().expect("the demo world");
            if *losing > 0 {
                *losing -= 1;
                let what = state.index.first().map_or("", String::as_str);
                let message = format!("WhatsApp took the {what} change, but its echo didn't come back within 30s");
                return Err(EngineError::Failed { message, retry: true });
            }
        }
        if world.holding_patches.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if !self.send(pb::Event { seq: 0, kind: Some(pb::event::Kind::AppState(state)) }).await {
            return Err(EngineError::Gone);
        }
        Ok(pb::reply::Kind::Done(pb::Done {}))
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
                if let Some(answer) = recipients.first().and_then(|who| answering(&send, who)) {
                    sender.send(answer).await;
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
            [["pin_v1", ANA], ["archive", CLIMBING], ["mute", ANA]]
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
            let names = vec!["regular_low".to_string()];
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
        let joined = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(fetching, acking) });
        let (reply, ()) = joined.await.expect("the fetch never answered");
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
        let demo = DemoEngine::new();
        let script = demo.script.len();
        let _instance = demo.start(tx).await.unwrap();
        let mut played = Vec::new();
        let playing = async {
            while played.len() < script {
                let incoming = rx.recv().await.unwrap();
                if let Some(ack) = incoming.ack {
                    ack.send();
                }
                played.push(incoming.event.kind.unwrap());
            }
        };
        let whole = tokio::time::timeout(Duration::from_secs(10), playing).await;
        assert!(whole.is_ok(), "the script played its {script} events: {played:?}");
        let messages = played.iter().filter(|kind| matches!(kind, pb::event::Kind::Message(_))).count();
        assert_eq!(
            messages,
            script - 2,
            "its messages first, the photo, the file and the voice message among them: {played:?}"
        );
        let Some(pb::event::Kind::GroupChange(group)) = played.last() else { panic!("named last: {played:?}") };
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

    #[test]
    fn the_demos_event_is_at_9_utc_on_the_first_saturday_after_today_a_week_on_from_a_saturday() {
        // Saturday 31 October 2026 at 9:00 UTC, and the Saturday after, in seconds since the epoch.
        const OCT_31: i64 = 1_793_437_200;
        const NOV_7: i64 = OCT_31 + 7 * 86_400;
        // `ms` milliseconds into the day `days` on from 31 October, UTC.
        let at = |days: i64, ms: i64| (OCT_31 - 9 * 3_600 + days * 86_400) * 1_000 + ms;
        let hours = |h: i64| h * 3_600_000;
        for (now, wanted, when) in [
            (at(-2, hours(12)), OCT_31, "Thursday at noon"),
            (at(-1, 0), OCT_31, "Friday at midnight"),
            (at(0, -1), OCT_31, "Friday, a millisecond before midnight"),
            (at(0, 0), NOV_7, "Saturday at midnight"),
            (at(0, hours(9) - 1), NOV_7, "Saturday, a millisecond before 9:00"),
            (at(0, hours(9)), NOV_7, "Saturday at 9:00"),
            (at(1, -1), NOV_7, "Saturday, a millisecond before midnight"),
            (at(1, 0), NOV_7, "Sunday at midnight"),
            (at(3, hours(12)), NOV_7, "Tuesday at noon"),
            (at(7, -1), NOV_7, "the next Friday, a millisecond before midnight"),
        ] {
            assert_eq!(saturday(now), wanted, "{when}");
        }
        // Every minute of three weeks: 9:00 UTC on a Saturday, never today and at most a week on.
        for minute in 0..21 * 24 * 60 {
            let now = at(-3, minute * 60_000);
            let starts = saturday(now);
            let ahead = starts.div_euclid(86_400) - now.div_euclid(86_400_000);
            assert_eq!((starts - OCT_31).rem_euclid(7 * 86_400), 0, "a Saturday at 9:00 from {now}");
            assert!((1..=7).contains(&ahead), "{ahead} days on from {now}");
        }
    }

    #[tokio::test]
    async fn an_app_state_patch_comes_back_as_app_state_before_its_answer_and_a_fetch_brings_back_what_was_taken() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        let pin = pb::AppStatePatch {
            name: "regular_low".into(),
            index: vec!["pin_v1".into(), ANA.into()],
            version: 5,
            value: vec![8, 1],
        };
        let answered = AtomicBool::new(false);
        let patching = async {
            let reply = instance.commands.call(pb::command::Kind::AppStatePatch(pin.clone())).await;
            answered.store(true, Ordering::SeqCst);
            reply
        };
        let acking = async {
            let incoming =
                tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("no echo came").unwrap();
            let Some(pb::event::Kind::AppState(echo)) = &incoming.event.kind else { panic!("{:?}", incoming.event) };
            assert_eq!((&echo.index, &echo.value), (&pin.index, &pin.value), "the patch as it was sent");
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(!answered.load(Ordering::SeqCst), "answered before its echo was acked");
            incoming.ack.expect("an echo waits for its ack, as jaw-engine's does").send();
        };
        let (reply, ()) = tokio::join!(patching, acking);
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        world.refuse_patches(1);
        let other = pb::AppStatePatch { index: vec!["pin_v1".into(), CLIMBING.into()], ..pin.clone() };
        let refusing = instance.commands.call(pb::command::Kind::AppStatePatch(other));
        let refused = tokio::time::timeout(Duration::from_secs(10), refusing).await.expect("the refusal never came");
        assert!(matches!(refused, Err(EngineError::Failed { retry: false, .. })), "{refused:?}");
        let fetch = |names: &[&str]| {
            let names = names.iter().map(|name| name.to_string()).collect();
            pb::command::Kind::AppStateResync(pb::AppStateResync { names })
        };
        let fetching = instance.commands.call(fetch(&["regular_low"]));
        let acking = async {
            let incoming =
                tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("nothing came back").unwrap();
            let Some(pb::event::Kind::AppState(state)) = &incoming.event.kind else { panic!("{:?}", incoming.event) };
            assert_eq!(state.index, pin.index, "what WhatsApp took, not what it refused");
            incoming.ack.expect("a mutation needs an ack").send();
        };
        let joined = tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(fetching, acking) });
        let (reply, ()) = joined.await.expect("the fetch of regular_low never answered");
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        let elsewhere = tokio::time::timeout(Duration::from_secs(10), instance.commands.call(fetch(&["regular_high"])))
            .await
            .expect("the fetch of regular_high never answered");
        assert!(matches!(elsewhere, Ok(pb::reply::Kind::Done(_))), "{elsewhere:?}");
        assert!(rx.try_recv().is_err(), "nothing was taken in regular_high");
        let fetches =
            world.asked().into_iter().filter(|kind| matches!(kind, pb::command::Kind::AppStateResync(_))).count();
        assert_eq!(fetches, 2, "each fetch is noted");
    }

    /// jaw's app-state patch of `index` in collection `name`.
    fn patch(name: &str, index: [&str; 2], value: &[u8]) -> pb::command::Kind {
        let index = index.iter().map(|part| part.to_string()).collect();
        pb::command::Kind::AppStatePatch(pb::AppStatePatch {
            name: name.into(),
            index,
            version: 2,
            value: value.to_vec(),
        })
    }

    /// A full fetch of collections `names`.
    fn resync(names: &[&str]) -> pb::command::Kind {
        let names = names.iter().map(|name| name.to_string()).collect();
        pb::command::Kind::AppStateResync(pb::AppStateResync { names })
    }

    /// A mutation as WhatsApp sends one back.
    fn state(index: [&str; 2], value: &[u8]) -> pb::AppState {
        pb::AppState { index: index.iter().map(|part| part.to_string()).collect(), value: value.to_vec() }
    }

    /// Asks the engine `kind`, acking each mutation that comes meanwhile, and says how it answered and
    /// what came, in order; it fails, rather than waits, if no answer comes within ten seconds.
    async fn answered(
        instance: &Instance,
        rx: &mut mpsc::Receiver<Incoming>,
        kind: pb::command::Kind,
    ) -> (Result<pb::reply::Kind, EngineError>, Vec<pb::AppState>) {
        let asked = format!("{kind:?}");
        let mut asking = std::pin::pin!(instance.commands.call(kind));
        let mut came = Vec::new();
        let answering = async {
            loop {
                tokio::select! {
                    reply = &mut asking => return reply,
                    Some(incoming) = rx.recv() => {
                        let Some(pb::event::Kind::AppState(state)) = &incoming.event.kind else {
                            panic!("{:?}", incoming.event)
                        };
                        came.push(state.clone());
                        incoming.ack.expect("a mutation needs an ack").send();
                    }
                }
            }
        };
        let reply = tokio::time::timeout(Duration::from_secs(10), answering)
            .await
            .unwrap_or_else(|_| panic!("{asked} was never answered"));
        (reply, came)
    }

    #[tokio::test]
    async fn a_fetch_sends_the_latest_whatsapp_holds_in_the_collections_named_and_in_every_one_with_none() {
        let (tx, mut rx) = mpsc::channel(8);
        let instance = DemoEngine::for_account(Vec::new(), Arc::new(Tiny)).start(tx).await.unwrap();
        let (reply, echo) = answered(&instance, &mut rx, patch("regular_low", ["archive", CLIMBING], &[8, 0])).await;
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        assert_eq!(echo, [state(["archive", CLIMBING], &[8, 0])]);
        let (reply, low) = answered(&instance, &mut rx, resync(&["regular_low"])).await;
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        assert_eq!(
            low,
            [state(["pin_v1", ANA], &[]), state(["archive", CLIMBING], &[8, 0])],
            "the account's pin and its archive as jaw patched it, once, and not its mute, in regular_high"
        );
        let (_, high) = answered(&instance, &mut rx, resync(&["regular_high"])).await;
        assert_eq!(high, [state(["mute", ANA], &[])]);
        let (_, every) = answered(&instance, &mut rx, resync(&[])).await;
        assert_eq!(
            every,
            [state(["pin_v1", ANA], &[]), state(["archive", CLIMBING], &[8, 0]), state(["mute", ANA], &[])],
            "no names, every collection"
        );
    }

    #[tokio::test]
    async fn a_held_patch_is_kept_without_an_answer_and_a_fetch_meanwhile_brings_it_back() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        world.hold_patches(true);
        let mut holding = std::pin::pin!(instance.commands.call(patch("regular_high", ["mute", ANA], &[8, 1])));
        let unanswered = tokio::time::timeout(Duration::from_millis(200), &mut holding).await;
        assert!(unanswered.is_err(), "a held patch was answered: {unanswered:?}");
        assert!(rx.try_recv().is_err(), "nor was it sent back");
        let (reply, fetched) = answered(&instance, &mut rx, resync(&["regular_high"])).await;
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        assert_eq!(fetched, [state(["mute", ANA], &[8, 1])], "WhatsApp took it before the answer was lost");
        let still = tokio::time::timeout(Duration::from_millis(50), &mut holding).await;
        assert!(still.is_err(), "and it's still unanswered: {still:?}");
        world.hold_patches(false);
        let (reply, echo) = answered(&instance, &mut rx, patch("regular_high", ["mute", ANA], &[8, 0])).await;
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "a patch once they're let go: {reply:?}");
        assert_eq!(echo, [state(["mute", ANA], &[8, 0])]);
    }

    #[tokio::test]
    async fn only_as_many_patches_as_a_test_says_are_refused_and_the_next_is_taken() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        world.refuse_patches(1);
        let (refused, nothing) = answered(&instance, &mut rx, patch("regular_low", ["pin_v1", ANA], &[8, 1])).await;
        assert!(matches!(refused, Err(EngineError::Failed { retry: false, .. })), "{refused:?}");
        assert!(nothing.is_empty(), "a refused patch isn't sent back: {nothing:?}");
        let (taken, echo) = answered(&instance, &mut rx, patch("regular_low", ["pin_v1", ANA], &[8, 1])).await;
        assert!(matches!(taken, Ok(pb::reply::Kind::Done(_))), "the next is taken: {taken:?}");
        assert_eq!(echo, [state(["pin_v1", ANA], &[8, 1])]);
    }

    #[tokio::test]
    async fn a_patch_whose_echo_is_lost_is_kept_and_failed_as_one_to_try_again_and_only_as_many_as_a_test_says() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        world.lose_echoes(1);
        let (failed, nothing) = answered(&instance, &mut rx, patch("regular_low", ["pin_v1", ANA], &[8, 1])).await;
        let lost = "WhatsApp took the pin_v1 change, but its echo didn't come back within 30s";
        assert!(matches!(&failed, Err(EngineError::Failed { message, retry: true }) if message == lost), "{failed:?}");
        assert!(nothing.is_empty(), "its echo never came: {nothing:?}");
        let (_, fetched) = answered(&instance, &mut rx, resync(&["regular_low"])).await;
        assert_eq!(fetched, [state(["pin_v1", ANA], &[8, 1])], "WhatsApp took it all the same");
        let (taken, echo) = answered(&instance, &mut rx, patch("regular_low", ["pin_v1", ANA], &[8, 0])).await;
        assert!(matches!(taken, Ok(pb::reply::Kind::Done(_))), "the next is echoed: {taken:?}");
        assert_eq!(echo, [state(["pin_v1", ANA], &[8, 0])]);
    }

    #[tokio::test]
    async fn a_gated_patch_waits_for_its_gate_and_fetches_told_to_fail_fail_as_ones_to_try_again() {
        let (tx, mut rx) = mpsc::channel(8);
        let engine = DemoEngine::with(Vec::new());
        let world = engine.world();
        let instance = engine.start(tx).await.unwrap();
        world.gate_patches(true);
        let mut gated = std::pin::pin!(instance.commands.call(patch("regular_low", ["pin_v1", ANA], &[8, 1])));
        let unanswered = tokio::time::timeout(Duration::from_millis(200), &mut gated).await;
        assert!(unanswered.is_err(), "a gated patch was answered: {unanswered:?}");
        assert!(rx.try_recv().is_err(), "nor was it sent back");
        world.gate_patches(false);
        let incoming = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("no echo came").unwrap();
        incoming.ack.expect("an echo waits for its ack, as jaw-engine's does").send();
        let reply =
            tokio::time::timeout(Duration::from_secs(10), gated).await.expect("unanswered once the gate opened");
        assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        world.fail_fetches(true);
        let (failed, nothing) = answered(&instance, &mut rx, resync(&["regular_low"])).await;
        assert!(matches!(failed, Err(EngineError::Failed { retry: true, .. })), "{failed:?}");
        assert!(nothing.is_empty(), "a failed fetch hands nothing over: {nothing:?}");
        world.fail_fetches(false);
        let (fetched, came) = answered(&instance, &mut rx, resync(&["regular_low"])).await;
        assert!(matches!(fetched, Ok(pb::reply::Kind::Done(_))), "{fetched:?}");
        assert_eq!(came, [state(["pin_v1", ANA], &[8, 1])]);
    }

    #[tokio::test]
    async fn an_index_patched_twice_comes_back_once_as_the_second() {
        let (tx, mut rx) = mpsc::channel(8);
        let instance = DemoEngine::with(Vec::new()).start(tx).await.unwrap();
        for value in [[8, 1], [8, 0]] {
            let (reply, _) = answered(&instance, &mut rx, patch("regular_low", ["pin_v1", ANA], &value)).await;
            assert!(matches!(reply, Ok(pb::reply::Kind::Done(_))), "{reply:?}");
        }
        let (_, fetched) = answered(&instance, &mut rx, resync(&["regular_low"])).await;
        assert_eq!(fetched, [state(["pin_v1", ANA], &[8, 0])]);
    }
}
