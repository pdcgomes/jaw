//! Voice messages, played on jawd's machine (see protocol/README.md's "Voice"). jawd owns
//! jaw-voice's audio thread, started with it and holding no device while nothing plays, and one
//! task, the voice task, is all that drives it: every `voice.*` request and every event of the
//! thread's goes through it in order, so one thing plays at a time and its state is in one place. A
//! voice message plays from its file in the cache, which media wants first, at interactive
//! priority, when it isn't there; the thread's events become the `playback` push, to every
//! connection. What a connection plays stops when it ends, unless another has played something
//! since.

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jaw_proto::voice::{Playback, PlaybackState, VoicePlay, VoiceSeek, VoiceSpeed, VoiceStatus};
use jaw_proto::{Error, Push, code};
use jaw_voice::{Event, Microphone, Speaker, Speed};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::media::{Media, VoiceCopy};

/// The requests waiting for the voice task.
const ASKS: usize = 64;

/// What a connection asked the voice task, and where its answer goes.
enum Ask {
    Call {
        method: String,
        params: Value,
        connection: u64,
        answer: oneshot::Sender<Result<Value, Error>>,
    },
    /// A connection ended, and what it played with it.
    Ended(u64),
    /// The file a play waited for is in the cache, or can't be: `play` names the play.
    Downloaded {
        play: u64,
        file: Result<PathBuf, Error>,
    },
    /// jawd is stopping: nothing plays after the answer.
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
    /// What `status` says, as the voice task last left it.
    status: Mutex<VoiceStatus>,
}

impl Voice {
    /// Starts the audio thread on `speaker` and `microphone`, and the voice task, which finds files
    /// through `media` and pushes `playback` to `pushes`.
    pub fn start(
        media: Media,
        pushes: broadcast::Sender<Push>,
        speaker: impl Speaker + 'static,
        microphone: impl Microphone + 'static,
    ) -> Voice {
        let (audio, events) = jaw_voice::Voice::start(speaker, microphone);
        let audio = Arc::new(audio);
        let (asks, asked) = mpsc::channel(ASKS);
        let inner = Arc::new(Inner { asks: asks.clone(), audio: audio.clone(), status: Mutex::default() });
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
        let task = Task { audio, media, pushes, asks, playing: None, plays: 0, status: Arc::downgrade(&inner) };
        tokio::spawn(task.run(asked, hearing));
        Voice { inner }
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
    pub const METHODS: &[&str] = &["voice.play", "voice.pause", "voice.seek", "voice.speed"];

    /// Connection `connection` has ended: what it plays stops.
    pub fn ended(&self, connection: u64) {
        let _ = self.inner.asks.try_send(Ask::Ended(connection));
    }

    /// Stops what plays, as jawd stops.
    pub async fn stop(&self) {
        let (stopped, stopping) = oneshot::channel();
        if self.inner.asks.send(Ask::Stop(stopped)).await.is_ok() {
            let _ = stopping.await;
        }
    }

    pub fn status(&self) -> VoiceStatus {
        let mut status = self.inner.status.lock().expect("voice status").clone();
        let stats = self.inner.audio.stats();
        status.underruns = stats.underruns;
        status.overruns = stats.overruns;
        status.dropped_events = stats.dropped_events;
        status.concealed = stats.concealed;
        status
    }
}

/// What plays: a voice message, by its ID.
#[derive(Clone, Debug, PartialEq, Eq)]
enum What {
    Message(String),
}

/// The audio thread's name for play `play` of `what`: numbered, so the events of a play that ended
/// are never taken for the next one's of the same message.
fn key(play: u64, what: &What) -> String {
    match what {
        What::Message(id) => format!("{play} message:{id}"),
    }
}

/// The play and what plays that a key names.
fn from_key(key: &str) -> Option<(u64, What)> {
    let (play, what) = key.split_once(' ')?;
    Some((play.parse().ok()?, What::Message(what.strip_prefix("message:")?.to_string())))
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

struct Task {
    audio: Arc<jaw_voice::Voice>,
    media: Media,
    pushes: broadcast::Sender<Push>,
    /// For the waits a play starts, which say when its file is in.
    asks: mpsc::Sender<Ask>,
    playing: Option<Playing>,
    /// Numbers each play, so a later one leaves an earlier waiting for nothing, and the thread's
    /// events of each are told apart.
    plays: u64,
    status: std::sync::Weak<Inner>,
}

impl Task {
    async fn run(mut self, mut asked: mpsc::Receiver<Ask>, mut hearing: mpsc::Receiver<Event>) {
        loop {
            tokio::select! {
                ask = asked.recv() => match ask {
                    Some(Ask::Stop(stopped)) => {
                        self.stop_playing();
                        self.publish();
                        let _ = stopped.send(());
                    }
                    Some(ask) => self.ask(ask).await,
                    None => return,
                },
                event = hearing.recv() => match event {
                    Some(event) => self.heard(event),
                    None => return,
                },
            }
            self.publish();
        }
    }

    async fn ask(&mut self, ask: Ask) {
        match ask {
            Ask::Call { method, params, connection, answer } => {
                let answered = self.call(&method, params, connection).await;
                let _ = answer.send(answered);
            }
            Ask::Ended(connection) => {
                if self.playing.as_ref().is_some_and(|p| p.connection == connection) {
                    self.stop_playing();
                }
            }
            Ask::Downloaded { play, file } => self.downloaded(play, file),
            Ask::Stop(stopped) => {
                let _ = stopped.send(());
            }
        }
    }

    async fn call(&mut self, method: &str, params: Value, connection: u64) -> Result<Value, Error> {
        match method {
            "voice.play" => self.play(params_of(method, params)?, connection).await,
            "voice.pause" => self.pause(),
            "voice.seek" => self.seek(params_of::<VoiceSeek>(method, params)?.at),
            "voice.speed" => self.speed(speed_of(params_of::<VoiceSpeed>(method, params)?.speed)?),
            _ => Err(Error::new(code::UNKNOWN_METHOD, format!("jawd doesn't know {method}"))),
        }
    }

    async fn play(&mut self, play: VoicePlay, connection: u64) -> Result<Value, Error> {
        let speed = play.speed.map_or(Ok(Speed::Normal), speed_of)?;
        let at = play.at.map(seconds_of).transpose()?;
        let id = match (play.message, play.recording) {
            (_, true) => return Err(Error::new(code::NOT_FOUND, "there's no recording to play")),
            (Some(id), false) => id,
            (None, false) => return Err(bad_param("message", "voice.play names a voice message, or the recording")),
        };
        let what = What::Message(id.clone());
        let copy = self.media.voice_file(&id, play.chat.as_deref()).await?;
        let same = self.playing.as_ref().filter(|p| p.what == what).map(|p| (p.now, p.at));
        // Without `at`, a message paused carries on where it paused; any other starts at 0.
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
        let seconds = copy.seconds.unwrap_or(0.0);
        // One waiting for its file, or held before it, gives way: to another message, pushed
        // stopped, or to this one, which carries on from here.
        if same.is_some_and(|(now, _)| matches!(now, Now::Waiting | Now::Held)) {
            self.playing = None;
        } else {
            self.leave_waiting();
        }
        self.plays += 1;
        let play = self.plays;
        match copy.path.clone() {
            Some(path) => {
                try_audio(self.audio.play(key(play, &what), path, Duration::from_secs_f64(at), speed))?;
                self.playing = Some(Playing { what, play, connection, now: Now::Playing, at, seconds, speed });
                if let Some(sha256) = copy.sha256.clone() {
                    let media = self.media.clone();
                    tokio::spawn(async move { media.used(sha256).await });
                }
            }
            None => {
                self.media.fetch_now(&copy).await?;
                // What the thread plays stops now, and pushes `stopped` itself.
                if self.playing.is_some() {
                    let _ = self.audio.stop();
                }
                self.playing = Some(Playing { what, play, connection, now: Now::Waiting, at, seconds, speed });
                self.wait_for(play, &copy);
            }
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

    /// What the audio thread said.
    fn heard(&mut self, event: Event) {
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
            _ => {}
        }
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
        });
        status.state = self.playing.as_ref().map(|p| {
            match p.now {
                Now::Waiting => "waiting",
                Now::Held | Now::Paused => "paused",
                Now::Playing => "playing",
            }
            .to_string()
        });
    }
}

/// A `playback` push naming `what`, its other fields to fill in.
fn named(what: &What) -> Playback {
    let message = match what {
        What::Message(id) => Some(id.clone()),
    };
    Playback {
        message,
        recording: false,
        state: PlaybackState::Stopped,
        at: 0.0,
        seconds: 0.0,
        speed: 1.0,
        error: None,
    }
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
    error.data = Some(serde_json::json!({ "param": param }));
    error
}

fn speed_of(speed: f64) -> Result<Speed, Error> {
    Speed::from_factor(speed).ok_or_else(|| bad_param("speed", "a voice message plays at 1, 1.5 or 2 times its speed"))
}

/// A playhead's place, in seconds from the start.
fn seconds_of(at: f64) -> Result<f64, Error> {
    if at.is_finite() && at >= 0.0 && at < 86_400.0 {
        Ok(at)
    } else {
        Err(bad_param("at", "a playhead's place is a number of seconds from the start"))
    }
}

/// What the audio thread said to a command: a thread behind or gone is an error, never a success.
fn try_audio(said: Result<(), jaw_voice::Error>) -> Result<(), Error> {
    said.map_err(|e| Error::new(code::INTERNAL, e.to_string()))
}
