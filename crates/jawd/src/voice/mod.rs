//! Voice messages, played and recorded on jawd's machine (see protocol/README.md's "Voice"). jawd
//! owns jaw-voice's audio thread, started with it and holding no device while nothing plays or
//! records, and one task, the voice task, is all that drives it: every `voice.*` request and every
//! event of the thread's goes through it in order, so one thing plays or records at a time and its
//! state is in one place. A voice message plays from its file in the cache, which media wants
//! first, at interactive priority, when it isn't there. A recording goes to a name of its own in the
//! recordings folder (`folder`), WhatsApp told you're recording while it lasts, is listened to from
//! its file once it's stopped, and is sent through the outbox as a voice message of yours, as
//! sending's `send_voice` writes one. The thread's events become the `playback` and `recording`
//! pushes, to every connection. A recording lasts while jawd runs and is the connection's that
//! asked for it: it's thrown away when that connection ends, or jawd stops, whether it's still
//! going or stopped; and what a connection plays stops when it ends, unless another has played
//! something since.

mod folder;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jaw_proto::voice::{
    Playback, PlaybackState, Recording as RecordingPush, RecordingState, VoicePlay, VoiceRecord, VoiceSeek, VoiceSend,
    VoiceSent, VoiceSpeed, VoiceStatus,
};
use jaw_proto::{Error, Push, code};
use jaw_voice::{Event, Microphone, Recorded, RecordingEnd, Speaker, Speed};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::media::{Media, VoiceCopy};
use crate::send::{Sending, VoiceNote};
use crate::store::{Store, read};
use crate::work::Priority;

/// The requests waiting for the voice task.
const ASKS: usize = 64;

/// What jawd may change about voice; the defaults are the protocol's.
#[derive(Clone, Debug)]
pub struct Config {
    /// The longest recording, 15 minutes, which jaw-voice never passes.
    pub limit: Duration,
    /// How often WhatsApp is told again that you're recording, within typing's quiet spell.
    pub recording_again: Duration,
    /// The free space a recording needs to start: room for it, the copy kept to send it, and
    /// what jaw keeps free besides.
    pub room: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config { limit: jaw_voice::RECORDING_LIMIT, recording_again: Duration::from_secs(5), room: 64_000_000 }
    }
}

/// Where a request's answer goes.
type Answer = oneshot::Sender<Result<Value, Error>>;

/// What a connection asked the voice task, and where its answer goes.
enum Ask {
    Call {
        method: String,
        params: Value,
        connection: u64,
        answer: Answer,
    },
    /// A connection ended, and what it played and recorded with it.
    Ended(u64),
    /// The file a play waited for is in the cache, or can't be: `play` names the play.
    Downloaded {
        play: u64,
        file: Result<PathBuf, Error>,
    },
    /// The recording being sent is on disk, synced, as a message of yours, or couldn't be.
    Sent {
        sent: Result<VoiceSent, Error>,
        answer: Answer,
    },
    /// jawd is stopping: nothing plays or records after the answer.
    Stop(oneshot::Sender<()>),
}

/// Voice, running.
#[derive(Clone)]
pub struct Voice {
    inner: Arc<Inner>,
}

struct Inner {
    asks: mpsc::Sender<Ask>,
    audio: Arc<jaw_voice::Voice>,
    folder: Arc<folder::Folder>,
    /// What `status` says, as the voice task last left it.
    status: Mutex<VoiceStatus>,
}

impl Voice {
    /// Prepares the recordings folder in `home`, then starts the audio thread on `speaker` and
    /// `microphone`, and the voice task, which finds voice messages' files through `media`, the
    /// chats recordings are for in `store`, tells WhatsApp you're recording through `sending`, and
    /// pushes `playback` and `recording` to `pushes`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        home: &Path,
        store: Store,
        media: Media,
        sending: Sending,
        pushes: broadcast::Sender<Push>,
        speaker: impl Speaker + 'static,
        microphone: impl Microphone + 'static,
        config: Config,
    ) -> anyhow::Result<Voice> {
        let folder = Arc::new(folder::Folder::prepare(home)?);
        if folder.removed_at_start > 0 {
            eprintln!("jawd: removed {} recordings a stopped jawd left", folder.removed_at_start);
        }
        if !folder.hard_links {
            eprintln!("jawd: the recordings folder takes no hard links, so jawd can't record");
        }
        let (audio, events) = jaw_voice::Voice::start(speaker, microphone);
        let audio = Arc::new(audio);
        let (asks, asked) = mpsc::channel(ASKS);
        let inner = Arc::new(Inner {
            asks: asks.clone(),
            audio: audio.clone(),
            folder: folder.clone(),
            status: Mutex::new(VoiceStatus {
                hard_links: folder.hard_links,
                removed_at_start: folder.removed_at_start,
                ..VoiceStatus::default()
            }),
        });
        let (heard, hearing) = mpsc::channel(jaw_voice::EVENTS);
        // The thread's events come on a channel of its own, which a thread of ours drains into the
        // task's, so the task waits on one runtime's channels alone.
        std::thread::Builder::new()
            .name("jawd-voice-events".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    if heard.blocking_send(event).is_err() {
                        return;
                    }
                }
            })
            .expect("the voice events' thread starts");
        let task = Task {
            audio,
            store,
            media,
            sending,
            pushes,
            asks,
            config,
            folder,
            playing: None,
            recording: None,
            plays: 0,
            say_again: None,
            status: Arc::downgrade(&inner),
        };
        tokio::spawn(task.run(asked, hearing));
        Ok(Voice { inner })
    }

    /// Answers one of the voice methods, for the connection numbered `connection`.
    pub async fn call(&self, method: &str, params: Value, connection: u64) -> Result<Value, Error> {
        let (answer, answered) = oneshot::channel();
        let ask = Ask::Call { method: method.to_string(), params, connection, answer };
        if self.inner.asks.send(ask).await.is_err() {
            return Err(Error::new(code::INTERNAL, "jawd's voice has stopped"));
        }
        answered.await.unwrap_or_else(|_| Err(Error::new(code::INTERNAL, "jawd's voice has stopped")))
    }

    /// The methods `call` answers.
    pub const METHODS: &[&str] = &[
        "voice.play",
        "voice.pause",
        "voice.seek",
        "voice.speed",
        "voice.record",
        "voice.stop",
        "voice.send",
        "voice.cancel",
    ];

    /// Connection `connection` has ended: what it plays stops, and its recording is thrown away.
    pub fn ended(&self, connection: u64) {
        let _ = self.inner.asks.try_send(Ask::Ended(connection));
    }

    /// Stops what plays and throws away the recording, going or stopped, as jawd stops.
    pub async fn stop(&self) {
        let (stopped, stopping) = oneshot::channel();
        if self.inner.asks.send(Ask::Stop(stopped)).await.is_ok() {
            let _ = stopping.await;
        }
    }

    pub fn status(&self) -> VoiceStatus {
        let mut status = self.inner.status.lock().expect("voice status").clone();
        let stats = self.inner.audio.stats();
        (status.underruns, status.overruns, status.dropped_events, status.concealed) =
            (stats.underruns, stats.overruns, stats.dropped_events, stats.concealed);
        (status.files, status.bytes) = self.inner.folder.counts();
        status
    }
}

/// What plays: a voice message, by its ID, or the stopped recording.
#[derive(Clone, Debug, PartialEq, Eq)]
enum What {
    Message(String),
    Recording,
}

/// The audio thread's name for play `play` of `what`: numbered, so the events of a play that ended
/// are never taken for the next one's of the same message.
fn key(play: u64, what: &What) -> String {
    match what {
        What::Message(id) => format!("{play} message:{id}"),
        What::Recording => format!("{play} recording"),
    }
}

/// The play and what plays that a key names.
fn from_key(key: &str) -> Option<(u64, What)> {
    let (play, what) = key.split_once(' ')?;
    let what = match what.strip_prefix("message:") {
        Some(id) => What::Message(id.to_string()),
        None if what == "recording" => What::Recording,
        None => return None,
    };
    Some((play.parse().ok()?, what))
}

/// How what plays is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Now {
    /// Waiting for its file, to play from `at` at `speed` once it's in.
    Waiting,
    /// Paused while it waited for its file, at `at`; it plays from there when asked again.
    Held,
    Playing,
    Paused,
}

struct Playing {
    what: What,
    /// Names this play: its wait for its file, and its key on the audio thread.
    play: u64,
    /// The connection that asked for it last.
    connection: u64,
    now: Now,
    at: f64,
    seconds: f64,
    speed: Speed,
}

/// How the recording is.
enum Rec {
    /// Asked for: `voice.record` is answered once the thread has started it, with its first level,
    /// or refused it.
    Starting(Answer),
    Going,
    /// Asked to stop: these are answered once it's synced and in place, or thrown away.
    Stopping(Vec<Answer>),
    /// Asked to be thrown away while it went or stopped: these are answered once it is.
    Cancelling(Vec<Answer>),
    /// Synced and in place under its name, to listen to, send or throw away.
    Stopped(Recorded),
    /// Being written as a message of yours, which it stays stopped for if it can't be.
    Sending(Recorded),
}

struct Recording {
    chat: String,
    /// The ID made for the message it may become, which names its file.
    id: String,
    path: PathBuf,
    /// The connection that asked for it.
    connection: u64,
    now: Rec,
    seconds: f64,
    /// Whether WhatsApp has been told you're recording.
    said: bool,
    /// A `voice.send` that came while it went, with its key, sent once it's stopped.
    send: Option<(Option<String>, Answer)>,
}

struct Task {
    audio: Arc<jaw_voice::Voice>,
    store: Store,
    media: Media,
    sending: Sending,
    pushes: broadcast::Sender<Push>,
    /// For the waits a play starts, which say when its file is in.
    asks: mpsc::Sender<Ask>,
    config: Config,
    folder: Arc<folder::Folder>,
    playing: Option<Playing>,
    recording: Option<Recording>,
    /// Numbers each play, so a later one leaves an earlier waiting for nothing, and the thread's
    /// events of each are told apart.
    plays: u64,
    /// When WhatsApp is next told again that you're recording.
    say_again: Option<Instant>,
    status: std::sync::Weak<Inner>,
}

impl Task {
    async fn run(mut self, mut asked: mpsc::Receiver<Ask>, mut hearing: mpsc::Receiver<Event>) {
        loop {
            let again = self.say_again;
            tokio::select! {
                ask = asked.recv() => match ask {
                    Some(Ask::Stop(stopped)) => {
                        self.stop_playing();
                        self.throw_away(Vec::new()).await;
                        self.publish();
                        let _ = stopped.send(());
                    }
                    Some(ask) => self.ask(ask).await,
                    None => return,
                },
                event = hearing.recv() => match event {
                    Some(event) => self.heard(event).await,
                    None => return,
                },
                () = tokio::time::sleep_until(again.unwrap_or_else(Instant::now)), if again.is_some() => {
                    self.say("recording").await;
                }
            }
            self.publish();
        }
    }

    async fn ask(&mut self, ask: Ask) {
        match ask {
            Ask::Call { method, params, connection, answer } => self.call(&method, params, connection, answer).await,
            Ask::Ended(connection) => {
                if self.playing.as_ref().is_some_and(|p| p.connection == connection) {
                    self.stop_playing();
                }
                if self.recording.as_ref().is_some_and(|r| r.connection == connection) {
                    self.throw_away(Vec::new()).await;
                }
            }
            Ask::Downloaded { play, file } => self.downloaded(play, file),
            Ask::Sent { sent, answer } => self.sent(sent, answer),
            Ask::Stop(stopped) => {
                let _ = stopped.send(());
            }
        }
    }

    async fn call(&mut self, method: &str, params: Value, connection: u64, answer: Answer) {
        let answered = match method {
            "voice.record" => match params_of::<VoiceRecord>(method, params) {
                Ok(record) => return self.record(record.chat, connection, answer).await,
                Err(e) => Err(e),
            },
            "voice.stop" => return self.stop_recording(answer),
            "voice.send" => match params_of::<VoiceSend>(method, params) {
                Ok(send) => return self.send(send, answer).await,
                Err(e) => Err(e),
            },
            "voice.cancel" => return self.cancel(answer).await,
            "voice.play" => match params_of(method, params) {
                Ok(play) => self.play(play, connection).await,
                Err(e) => Err(e),
            },
            "voice.pause" => self.pause(),
            "voice.seek" => params_of::<VoiceSeek>(method, params).and_then(|seek| self.seek(seek.at)),
            "voice.speed" => {
                params_of::<VoiceSpeed>(method, params).and_then(|s| speed_of(s.speed)).and_then(|s| self.speed(s))
            }
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        };
        let _ = answer.send(answered);
    }

    async fn play(&mut self, play: VoicePlay, connection: u64) -> Result<Value, Error> {
        let speed = play.speed.map_or(Ok(Speed::Normal), speed_of)?;
        let at = play.at.map(seconds_of).transpose()?;
        let (what, path, seconds, copy) = match (play.message, play.recording) {
            (_, true) => {
                let Some(recording) = self.recording.as_ref() else {
                    return Err(Error::new(code::NOT_FOUND, "there's no recording to play"));
                };
                let Rec::Stopped(recorded) = &recording.now else {
                    return Err(Error::new(code::NOT_ALLOWED, "the recording isn't stopped to listen to"));
                };
                (What::Recording, Some(recording.path.clone()), recorded.duration.as_secs_f64(), None)
            }
            (Some(id), false) => {
                if self.recording.as_ref().is_some_and(|r| !matches!(r.now, Rec::Stopped(_) | Rec::Sending(_))) {
                    return Err(Error::new(code::NOT_ALLOWED, "jaw is recording; stop or send that first"));
                }
                let copy = self.media.voice_file(&id, play.chat.as_deref()).await?;
                (What::Message(id), copy.path.clone(), copy.seconds.unwrap_or(0.0), Some(copy))
            }
            (None, false) => return Err(bad_param("message", "voice.play names a voice message, or the recording")),
        };
        let same = self.playing.as_ref().filter(|p| p.what == what).map(|p| (p.now, p.at));
        // Without `at`, what's paused carries on where it paused; anything else starts at 0.
        let at = match same {
            Some((Now::Paused, _)) if at.is_none() => {
                try_audio(self.audio.set_speed(speed))?;
                try_audio(self.audio.resume())?;
                if let Some(playing) = self.playing.as_mut() {
                    playing.connection = connection;
                }
                return Ok(nothing());
            }
            Some((Now::Held, held)) if at.is_none() => held,
            _ => at.unwrap_or(0.0),
        };
        // One waiting for its file, or held before it, gives way: to another message, pushed
        // stopped, or to this one, which carries on from here.
        if same.is_some_and(|(now, _)| matches!(now, Now::Waiting | Now::Held)) {
            self.playing = None;
        } else {
            self.leave_waiting();
        }
        self.plays += 1;
        let play = self.plays;
        match (path, copy) {
            (Some(path), copy) => {
                try_audio(self.audio.play(key(play, &what), path, Duration::from_secs_f64(at), speed))?;
                self.playing = Some(Playing { what, play, connection, now: Now::Playing, at, seconds, speed });
                if let Some(sha256) = copy.and_then(|c| c.sha256) {
                    let media = self.media.clone();
                    tokio::spawn(async move { media.used(sha256).await });
                }
            }
            (None, Some(copy)) => {
                self.media.fetch_now(&copy).await?;
                // What the thread plays stops now, and pushes `stopped` itself.
                if self.playing.is_some() {
                    let _ = self.audio.stop();
                }
                self.playing = Some(Playing { what, play, connection, now: Now::Waiting, at, seconds, speed });
                self.wait_for(play, &copy);
            }
            (None, None) => return Err(Error::new(code::INTERNAL, "the recording has no file")),
        }
        Ok(nothing())
    }

    /// Has the voice task told when `copy`'s file is in the cache, for play `play`.
    fn wait_for(&self, play: u64, copy: &VoiceCopy) {
        let (media, asks, message) = (self.media.clone(), self.asks.clone(), copy.message);
        tokio::spawn(async move {
            let file = media.downloaded(message).await;
            let _ = asks.send(Ask::Downloaded { play, file }).await;
        });
    }

    fn downloaded(&mut self, play: u64, file: Result<PathBuf, Error>) {
        let Some(playing) = self.playing.as_ref().filter(|p| p.play == play && p.now == Now::Waiting) else { return };
        let (what, at, speed) = (playing.what.clone(), playing.at, playing.speed);
        let path = file.map(|path| (path, key(play, &what)));
        let started =
            path.and_then(|(path, key)| try_audio(self.audio.play(key, path, Duration::from_secs_f64(at), speed)));
        match started {
            Ok(()) => {
                if let Some(playing) = self.playing.as_mut() {
                    playing.now = Now::Playing;
                }
            }
            Err(error) => {
                let failed = self.push_of(&what, PlaybackState::Failed, Some(error.message));
                self.playing = None;
                self.push(failed);
            }
        }
    }

    fn pause(&mut self) -> Result<Value, Error> {
        match self.playing.as_ref().map(|p| p.now) {
            Some(Now::Playing) => try_audio(self.audio.pause())?,
            Some(Now::Waiting) => {
                if let Some(playing) = self.playing.as_mut() {
                    playing.now = Now::Held;
                }
                if let Some(push) = self.playing.as_ref().map(|p| self.push_of(&p.what, PlaybackState::Paused, None)) {
                    self.push(push);
                }
            }
            _ => {}
        }
        Ok(nothing())
    }

    fn seek(&mut self, at: f64) -> Result<Value, Error> {
        let at = seconds_of(at)?;
        match self.playing.as_mut() {
            Some(playing) if matches!(playing.now, Now::Playing | Now::Paused) => {
                try_audio(self.audio.seek(Duration::from_secs_f64(at)))?
            }
            Some(playing) => playing.at = at,
            None => {}
        }
        Ok(nothing())
    }

    fn speed(&mut self, speed: Speed) -> Result<Value, Error> {
        match self.playing.as_mut() {
            Some(playing) if matches!(playing.now, Now::Playing | Now::Paused) => {
                try_audio(self.audio.set_speed(speed))?
            }
            Some(playing) => playing.speed = speed,
            None => {}
        }
        Ok(nothing())
    }

    /// Stops what plays: the thread's, which it pushes `stopped` for itself, or one waiting for its
    /// file, which is pushed here.
    fn stop_playing(&mut self) {
        match self.playing.as_ref().map(|p| p.now) {
            Some(Now::Playing | Now::Paused) => {
                let _ = self.audio.stop();
            }
            Some(Now::Waiting | Now::Held) => self.leave_waiting(),
            None => {}
        }
    }

    /// A play waiting for its file, or held before it, is over: it's pushed `stopped`.
    fn leave_waiting(&mut self) {
        if let Some(playing) = self.playing.as_ref().filter(|p| matches!(p.now, Now::Waiting | Now::Held)) {
            let stopped = self.push_of(&playing.what, PlaybackState::Stopped, None);
            self.playing = None;
            self.push(stopped);
        }
    }

    /// `voice.record`: answered once the thread has started it, or with why not.
    async fn record(&mut self, chat: String, connection: u64, answer: Answer) {
        if let Some(recording) = &self.recording {
            let why = match recording.now {
                Rec::Stopped(_) => "there's a recording stopped already; send it or throw it away first",
                _ => "jaw is already recording; stop or cancel that first",
            };
            let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, why)));
            return;
        }
        let looked = chat.clone();
        let known = self.store.reads().query(Priority::Interactive, move |db| read::chat_n(db, &looked)).await;
        let refused = match known {
            Err(e) => Some(Error::new(code::INTERNAL, format!("jawd couldn't read jaw.db: {e:#}"))),
            Ok(None) => {
                Some(Error::new(code::NOT_FOUND, format!("jawd has no chat {chat}; chat.start finds or makes one")))
            }
            Ok(Some(_)) if !self.folder.hard_links => Some(Error::new(
                code::NOT_ALLOWED,
                "jaw can't record here: jaw's folder takes no hard links, which putting a recording in place needs",
            )),
            Ok(Some(_)) => self.media.free_space().filter(|&free| free < self.config.room).map(|free| {
                let mut error = Error::new(
                    code::LIMIT,
                    format!(
                        "the disk has {} MB free, and a recording needs {} MB",
                        free / 1_000_000,
                        self.config.room / 1_000_000
                    ),
                );
                error.data = Some(json!({ "limit": "disk" }));
                error
            }),
        };
        if let Some(error) = refused {
            let _ = answer.send(Err(error));
            return;
        }
        let id = match crate::send::new_id() {
            Ok(id) => id,
            Err(e) => {
                let _ = answer.send(Err(Error::new(code::INTERNAL, format!("jawd couldn't make a message ID: {e}"))));
                return;
            }
        };
        self.stop_playing();
        let path = self.folder.path_for(&id);
        if let Err(error) = try_audio(self.audio.record(path.clone(), self.config.limit)) {
            let _ = answer.send(Err(error));
            return;
        }
        let now = Rec::Starting(answer);
        self.recording = Some(Recording { chat, id, path, connection, now, seconds: 0.0, said: false, send: None });
    }

    /// `voice.send`: the stopped recording, or the one going once it's stopped, written as a voice
    /// message of yours to the chat it was recorded in.
    async fn send(&mut self, send: VoiceSend, answer: Answer) {
        if send.at.is_some() || send.repeat.is_some() {
            let param = if send.at.is_some() { "at" } else { "repeat" };
            let why = "jawd doesn't send voice messages later yet; send it now, or throw it away";
            let _ = answer.send(Err(bad_param(param, why)));
            return;
        }
        let Some(recording) = self.recording.as_mut() else {
            // Asked again with its key once it's gone, as when its answer was lost, it's the
            // message it became.
            let sent = match send.key.as_deref() {
                Some(key) => self.sending.voice_sent(key).await,
                None => Ok(None),
            };
            let _ = answer.send(match sent {
                Ok(Some(sent)) => Ok(serde_json::to_value(sent).expect("jawd's own types serialize")),
                Ok(None) => Err(Error::new(code::NOT_FOUND, "there's no recording to send")),
                Err(e) => Err(e),
            });
            return;
        };
        let busy = |why: &str| Err(Error::new(code::NOT_ALLOWED, why));
        match &mut recording.now {
            Rec::Going => match try_audio(self.audio.stop_recording()) {
                Ok(()) => {
                    recording.now = Rec::Stopping(Vec::new());
                    recording.send = Some((send.key, answer));
                }
                Err(error) => {
                    let _ = answer.send(Err(error));
                }
            },
            Rec::Stopping(_) if recording.send.is_none() => recording.send = Some((send.key, answer)),
            Rec::Stopped(_) => self.start_sending(send.key, answer),
            Rec::Stopping(_) | Rec::Sending(_) => {
                let _ = answer.send(busy("the recording is being sent already"));
            }
            Rec::Starting(_) | Rec::Cancelling(_) => {
                let _ = answer.send(busy("the recording is starting or being thrown away; try again in a moment"));
            }
        }
    }

    /// Hands the stopped recording to sending, which writes it on a task of its own while the
    /// thread's events go on.
    fn start_sending(&mut self, key: Option<String>, answer: Answer) {
        let Some(recording) = self.recording.as_mut() else { return };
        let made = match std::mem::replace(&mut recording.now, Rec::Cancelling(Vec::new())) {
            Rec::Stopped(made) => made,
            now => {
                recording.now = now;
                let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, "the recording isn't stopped")));
                return;
            }
        };
        let note = VoiceNote {
            chat: recording.chat.clone(),
            id: recording.id.clone(),
            path: recording.path.clone(),
            seconds: made.duration.as_secs_f64(),
            waveform: made.waveform.to_vec(),
            key,
        };
        recording.now = Rec::Sending(made);
        let (sending, asks) = (self.sending.clone(), self.asks.clone());
        tokio::spawn(async move {
            let sent = sending.send_voice(note).await;
            let _ = asks.send(Ask::Sent { sent, answer }).await;
        });
    }

    /// The recording being sent is a message of yours, its file going now that the outbox's copy
    /// of it is synced; or it couldn't be, and it's stopped again, as it was.
    fn sent(&mut self, sent: Result<VoiceSent, Error>, answer: Answer) {
        match sent {
            Ok(sent) => {
                if self.playing.as_ref().is_some_and(|p| p.what == What::Recording) {
                    self.stop_playing();
                }
                if let Some(recording) = self.recording.take() {
                    self.folder.remove(&recording.path);
                }
                let _ = answer.send(Ok(serde_json::to_value(sent).expect("jawd's own types serialize")));
            }
            Err(error) => {
                if let Some(recording) = self.recording.as_mut()
                    && let Rec::Sending(made) = std::mem::replace(&mut recording.now, Rec::Cancelling(Vec::new()))
                {
                    recording.now = Rec::Stopped(made);
                }
                let _ = answer.send(Err(error));
            }
        }
    }

    /// `voice.stop`: answered once the recording is synced and in place under its name.
    fn stop_recording(&mut self, answer: Answer) {
        let Some(recording) = self.recording.as_mut() else {
            let _ = answer.send(Err(Error::new(code::NOT_FOUND, "jaw isn't recording")));
            return;
        };
        match &mut recording.now {
            Rec::Going => match try_audio(self.audio.stop_recording()) {
                Ok(()) => recording.now = Rec::Stopping(vec![answer]),
                Err(error) => {
                    let _ = answer.send(Err(error));
                }
            },
            Rec::Stopping(waiting) => waiting.push(answer),
            Rec::Stopped(_) | Rec::Sending(_) => {
                let _ = answer.send(Ok(nothing()));
            }
            Rec::Starting(_) | Rec::Cancelling(_) => {
                let why = "the recording is starting or being thrown away; try again in a moment";
                let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, why)));
            }
        }
    }

    /// `voice.cancel`: the recording thrown away, going or stopped, answered once it is; with none,
    /// there's nothing to throw away.
    async fn cancel(&mut self, answer: Answer) {
        match self.recording.as_ref().map(|r| &r.now) {
            None => {
                let _ = answer.send(Ok(nothing()));
            }
            Some(Rec::Starting(_)) => {
                let why = "the recording is starting; try again in a moment";
                let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, why)));
            }
            Some(Rec::Sending(_)) => {
                let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, "the recording is being sent")));
            }
            Some(_) => self.throw_away(vec![answer]).await,
        }
    }

    /// Throws the recording away, going or stopped, answering `answers` once it's gone.
    async fn throw_away(&mut self, mut answers: Vec<Answer>) {
        let Some(recording) = self.recording.as_mut() else {
            for answer in answers {
                let _ = answer.send(Ok(nothing()));
            }
            return;
        };
        if let Some((_, send)) = recording.send.take() {
            let _ = send.send(Err(Error::new(code::NOT_FOUND, "the recording was thrown away")));
        }
        let now = std::mem::replace(&mut recording.now, Rec::Cancelling(Vec::new()));
        match now {
            // Being written as a message, it's past throwing away: it goes, and its file with it.
            Rec::Sending(made) => {
                recording.now = Rec::Sending(made);
                for answer in answers {
                    let _ = answer.send(Err(Error::new(code::NOT_ALLOWED, "the recording is being sent")));
                }
            }
            Rec::Going | Rec::Starting(_) => {
                if let Rec::Starting(answer) = now {
                    let _ = answer.send(Err(Error::new(code::NOT_FOUND, "the recording was thrown away")));
                }
                let _ = self.audio.cancel_recording();
                recording.now = Rec::Cancelling(answers);
            }
            // Stopping, it ends in place, and is removed once it has.
            Rec::Stopping(stopping) => {
                for answer in stopping {
                    let _ = answer.send(Err(Error::new(code::NOT_FOUND, "the recording was thrown away")));
                }
                recording.now = Rec::Cancelling(answers);
            }
            Rec::Cancelling(mut waiting) => {
                waiting.append(&mut answers);
                recording.now = Rec::Cancelling(waiting);
            }
            Rec::Stopped(_) => {
                if self.playing.as_ref().is_some_and(|p| p.what == What::Recording) {
                    self.stop_playing();
                }
                let recording = self.recording.take().expect("a recording");
                self.folder.remove(&recording.path);
                self.recorded(RecordingState::Cancelled, 0.0);
                for answer in answers {
                    let _ = answer.send(Ok(nothing()));
                }
            }
        }
    }

    /// Tells WhatsApp you're recording in the recording's chat, or that you've stopped.
    async fn say(&mut self, state: &str) {
        let Some(recording) = self.recording.as_mut() else {
            self.say_again = None;
            return;
        };
        let recording_now = state == "recording";
        if !recording_now && !recording.said {
            return;
        }
        recording.said = recording_now;
        let chat = recording.chat.clone();
        self.say_again = recording_now.then(|| Instant::now() + self.config.recording_again);
        if let Err(e) = self.sending.call("typing", json!({ "chat": chat, "state": state })).await {
            eprintln!("jawd: can't say you're recording in {chat}: {}", e.message);
        }
    }

    /// What the audio thread said.
    async fn heard(&mut self, event: Event) {
        match event {
            Event::Playback { key, state, at, length, speed } => {
                let Some((play, what)) = from_key(&key) else { return };
                let state = match state {
                    jaw_voice::PlaybackState::Playing => PlaybackState::Playing,
                    jaw_voice::PlaybackState::Paused => PlaybackState::Paused,
                    jaw_voice::PlaybackState::Ended => PlaybackState::Ended,
                    jaw_voice::PlaybackState::Stopped => PlaybackState::Stopped,
                };
                let (at, seconds) = (at.as_secs_f64(), length.as_secs_f64());
                if let Some(playing) = self.playing.as_mut().filter(|p| p.play == play) {
                    (playing.at, playing.seconds, playing.speed) = (at, seconds, speed);
                    match state {
                        PlaybackState::Playing => playing.now = Now::Playing,
                        PlaybackState::Paused => playing.now = Now::Paused,
                        _ => self.playing = None,
                    }
                }
                self.push(Playback { state, at, seconds, speed: speed.factor(), error: None, ..named(&what) });
            }
            Event::PlaybackFailed { key, error } => {
                let Some((play, what)) = from_key(&key) else { return };
                let mut failed =
                    Playback { state: PlaybackState::Failed, error: Some(error.to_string()), ..named(&what) };
                if let Some(playing) = self.playing.as_ref().filter(|p| p.play == play) {
                    (failed.at, failed.seconds, failed.speed) = (playing.at, playing.seconds, playing.speed.factor());
                    self.playing = None;
                }
                self.push(failed);
            }
            Event::Recording { elapsed, level } => {
                let Some(recording) = self.recording.as_mut() else { return };
                recording.seconds = elapsed.as_secs_f64();
                let seconds = recording.seconds;
                if matches!(recording.now, Rec::Starting(_))
                    && let Rec::Starting(answer) = std::mem::replace(&mut recording.now, Rec::Going)
                {
                    let _ = answer.send(Ok(nothing()));
                    self.say("recording").await;
                }
                if matches!(self.recording.as_ref().map(|r| &r.now), Some(Rec::Going | Rec::Stopping(_))) {
                    let level = Some(level);
                    self.push_recording(RecordingPush { level, ..recorded(RecordingState::Recording, seconds) });
                }
            }
            Event::RecordingSilent => {
                let Some(seconds) = self.recording.as_ref().map(|r| r.seconds) else { return };
                if let Some(inner) = self.status.upgrade() {
                    inner.status.lock().expect("voice status").silent = true;
                }
                self.recorded(RecordingState::Silent, seconds);
            }
            Event::Recorded(made) => self.made(made).await,
            Event::RecordingCancelled => {
                let Some(mut recording) = self.recording.take() else { return };
                if let Some((_, send)) = recording.send.take() {
                    let _ = send.send(Err(Error::new(code::NOT_FOUND, "the recording was thrown away")));
                }
                self.say_ended(&recording).await;
                self.folder.remove(&recording.path);
                self.recorded(RecordingState::Cancelled, recording.seconds);
                answer_all(recording.now, Ok(nothing()));
            }
            Event::RecordingFailed { error } => {
                let Some(mut recording) = self.recording.take() else { return };
                if let Some((_, send)) = recording.send.take() {
                    let _ = send.send(Err(Error::new(code::NOT_ALLOWED, error.to_string())));
                }
                self.say_ended(&recording).await;
                self.folder.remove(&recording.path);
                let failed = RecordingPush { error: Some(error.to_string()), ..recorded(RecordingState::Failed, 0.0) };
                self.push_recording(failed);
                answer_all(recording.now, Err(Error::new(code::NOT_ALLOWED, error.to_string())));
            }
            // A recording refused never started: one waiting to start is told why, and one thrown
            // away meanwhile is gone.
            Event::RecordingRefused { error } => match self.recording.as_ref().map(|r| &r.now) {
                Some(Rec::Starting(_)) => {
                    let recording = self.recording.take().expect("a recording");
                    answer_all(recording.now, Err(Error::new(code::NOT_ALLOWED, error.to_string())));
                }
                Some(Rec::Cancelling(_)) => {
                    let recording = self.recording.take().expect("a recording");
                    self.recorded(RecordingState::Cancelled, 0.0);
                    answer_all(recording.now, Ok(nothing()));
                }
                _ => {}
            },
        }
    }

    /// The recording is synced and in place: it's stopped, or, asked meanwhile to be thrown away,
    /// removed.
    async fn made(&mut self, made: Recorded) {
        let Some(recording) = self.recording.as_mut() else {
            self.folder.remove(&made.path);
            return;
        };
        if made.path != recording.path {
            self.folder.remove(&made.path);
            return;
        }
        if let Some(inner) = self.status.upgrade() {
            inner.status.lock().expect("voice status").gaps_ms = made.gaps.as_millis() as u64;
        }
        let seconds = made.duration.as_secs_f64();
        recording.seconds = seconds;
        let (end, reason) = match &made.end {
            RecordingEnd::Stopped => ("stopped", None),
            RecordingEnd::Capped => ("limit", None),
            RecordingEnd::DeviceFailed(why) => ("interrupted", Some(why.clone())),
        };
        let push = RecordingPush {
            waveform: made.waveform.to_vec(),
            end: Some(end.into()),
            reason,
            ..recorded(RecordingState::Stopped, seconds)
        };
        match std::mem::replace(&mut recording.now, Rec::Stopped(made)) {
            Rec::Cancelling(answers) => {
                let recording = self.recording.take().expect("a recording");
                self.say_ended(&recording).await;
                self.folder.remove(&recording.path);
                self.recorded(RecordingState::Cancelled, seconds);
                for answer in answers {
                    let _ = answer.send(Ok(nothing()));
                }
            }
            now => {
                self.say("paused").await;
                self.push_recording(push);
                answer_all(now, Ok(nothing()));
                if let Some((key, answer)) = self.recording.as_mut().and_then(|r| r.send.take()) {
                    self.start_sending(key, answer);
                }
            }
        }
    }

    /// WhatsApp is told you've stopped recording in `recording`'s chat, if it was told you were.
    async fn say_ended(&mut self, recording: &Recording) {
        self.say_again = None;
        if recording.said
            && let Err(e) = self.sending.call("typing", json!({ "chat": recording.chat, "state": "paused" })).await
        {
            eprintln!("jawd: can't say you've stopped recording in {}: {}", recording.chat, e.message);
        }
    }

    /// Pushes the recording's `state` at `seconds`, with nothing more to say.
    fn recorded(&self, state: RecordingState, seconds: f64) {
        self.push_recording(recorded(state, seconds));
    }

    fn push_recording(&self, recording: RecordingPush) {
        push(&self.pushes, "recording", &recording);
    }

    /// The `playback` push for `what` in `state`, where it plays or waits now.
    fn push_of(&self, what: &What, state: PlaybackState, error: Option<String>) -> Playback {
        let playing = self.playing.as_ref().filter(|p| p.what == *what);
        let (at, seconds, speed) = playing.map_or((0.0, 0.0, 1.0), |p| (p.at, p.seconds, p.speed.factor()));
        Playback { state, at, seconds, speed, error, ..named(what) }
    }

    fn push(&self, playback: Playback) {
        push(&self.pushes, "playback", &playback);
    }

    /// Leaves `status` as the task has things now.
    fn publish(&self) {
        let Some(inner) = self.status.upgrade() else { return };
        let mut status = inner.status.lock().expect("voice status");
        status.playing = self.playing.as_ref().map(|p| match &p.what {
            What::Message(id) => id.clone(),
            What::Recording => "recording".into(),
        });
        status.state = self.playing.as_ref().map(|p| {
            match p.now {
                Now::Waiting => "waiting",
                Now::Held | Now::Paused => "paused",
                Now::Playing => "playing",
            }
            .to_string()
        });
        status.recording = self.recording.as_ref().map(|r| {
            match r.now {
                Rec::Starting(_) | Rec::Going | Rec::Stopping(_) | Rec::Cancelling(_) => "recording",
                Rec::Stopped(_) => "stopped",
                Rec::Sending(_) => "sending",
            }
            .to_string()
        });
        status.seconds = self.recording.as_ref().map_or(0.0, |r| r.seconds);
        if matches!(self.recording.as_ref().map(|r| &r.now), Some(Rec::Starting(_))) {
            status.silent = false;
        }
    }
}

/// Answers whoever waits on a recording that's `now`, with `answered`.
fn answer_all(now: Rec, answered: Result<Value, Error>) {
    let answers = match now {
        Rec::Starting(answer) => vec![answer],
        Rec::Stopping(answers) | Rec::Cancelling(answers) => answers,
        Rec::Going | Rec::Stopped(_) | Rec::Sending(_) => Vec::new(),
    };
    for answer in answers {
        let _ = answer.send(answered.clone());
    }
}

/// A `recording` push in `state` at `seconds`, its other fields to fill in.
fn recorded(state: RecordingState, seconds: f64) -> RecordingPush {
    RecordingPush { state, seconds, level: None, waveform: Vec::new(), end: None, reason: None, error: None }
}

/// A `playback` push naming `what`, its other fields to fill in.
fn named(what: &What) -> Playback {
    let (message, recording) = match what {
        What::Message(id) => (Some(id.clone()), false),
        What::Recording => (None, true),
    };
    Playback { message, recording, state: PlaybackState::Stopped, at: 0.0, seconds: 0.0, speed: 1.0, error: None }
}

fn push(pushes: &broadcast::Sender<Push>, method: &str, params: &impl Serialize) {
    let params = serde_json::to_value(params).expect("jawd's own types serialize");
    let _ = pushes.send(Push { method: method.into(), params });
}

fn nothing() -> Value {
    Value::Object(Default::default())
}

fn params_of<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, Error> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| Error::new(code::INVALID_PARAMS, format!("{method}'s params: {e}")))
}

fn bad_param(param: &str, why: &str) -> Error {
    let mut error = Error::new(code::INVALID_PARAMS, why);
    error.data = Some(json!({ "param": param }));
    error
}

fn speed_of(speed: f64) -> Result<Speed, Error> {
    Speed::from_factor(speed).ok_or_else(|| bad_param("speed", "a voice message plays at 1, 1.5 or 2 times its speed"))
}

/// A playhead's place, in seconds from the start.
fn seconds_of(at: f64) -> Result<f64, Error> {
    if at.is_finite() && (0.0..86_400.0).contains(&at) {
        Ok(at)
    } else {
        Err(bad_param("at", "a playhead's place is a number of seconds from the start"))
    }
}

/// What the audio thread said to a command: a thread behind or gone is an error, never a success.
fn try_audio(said: Result<(), jaw_voice::Error>) -> Result<(), Error> {
    said.map_err(|e| Error::new(code::INTERNAL, e.to_string()))
}
