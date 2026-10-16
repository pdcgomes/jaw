//! jaw's voice messages: Ogg Opus played and recorded on one audio thread this crate owns, and
//! WhatsApp's 64-point waveform.
//!
//! [`Voice::start`] spawns the thread and returns a handle and a bounded channel of [`Event`]s for
//! the caller to drain: where playback is and how loud the recording is, ten times a second, and
//! each change of state. The thread sleeps on its channels while nothing plays or records, and
//! holds no audio device then.

mod device;
mod error;
pub mod fake;
mod oggopus;
mod play;
mod record;
mod resample;
mod stretch;
mod system;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod waveform;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TrySendError, select};

pub use device::{Capture, Input, Microphone, Output, Render, Speaker};
pub use error::Error;
pub use oggopus::VoiceFile;
pub use record::part_path;
pub use system::{SystemMicrophone, SystemSpeaker};
pub use waveform::{POINTS, Waveform, analyse};

use play::Player;
use record::Recorder;

/// How often the playhead and the recording's level are reported.
pub const TICK: Duration = Duration::from_millis(100);

/// The most events waiting for the caller. Progress is dropped once half are waiting; changes of
/// state that find it full are held until it has room, and never dropped.
pub const EVENTS: usize = 256;

/// The most commands waiting for the audio thread.
pub const COMMANDS: usize = 64;

/// The longest a recording can be; about 2.7 MB at 24 kbit/s.
pub const RECORDING_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Playback speed. Faster speeds keep the voice's pitch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Speed {
    #[default]
    Normal,
    OneAndAHalf,
    Double,
}

impl Speed {
    pub fn factor(self) -> f64 {
        match self {
            Speed::Normal => 1.0,
            Speed::OneAndAHalf => 1.5,
            Speed::Double => 2.0,
        }
    }

    /// The speed for 1, 1.5 or 2, as the protocol sends it.
    pub fn from_factor(factor: f64) -> Option<Speed> {
        [Speed::Normal, Speed::OneAndAHalf, Speed::Double].into_iter().find(|s| s.factor() == factor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    /// It played to the end.
    Ended,
    /// It was stopped, or another message or a recording took over.
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingEnd {
    Stopped,
    /// It reached the length limit.
    Capped,
    /// The microphone stopped working; what came before is kept.
    DeviceFailed(String),
}

/// A finished recording, synced and in place under its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub path: PathBuf,
    pub duration: Duration,
    pub waveform: [u8; POINTS],
    pub end: RecordingEnd,
    /// Microphone audio lost because the audio thread fell behind; nothing, normally.
    pub gaps: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Ten times a second while playing, and at each change of state. `key` is the caller's name
    /// for what's playing, such as the message's ID.
    Playback {
        key: String,
        state: PlaybackState,
        at: Duration,
        length: Duration,
        speed: Speed,
    },
    /// Nothing plays after this.
    PlaybackFailed {
        key: String,
        error: Error,
    },
    /// Ten times a second while recording; `level` is 0 to 100.
    Recording {
        elapsed: Duration,
        level: u8,
    },
    /// A second has passed with nothing but digital silence, as macOS gives when it denies the
    /// microphone. Recording carries on.
    RecordingSilent,
    Recorded(Recorded),
    /// Nothing was kept.
    RecordingCancelled,
    /// The recording ended and nothing was kept.
    RecordingFailed {
        error: Error,
    },
    /// A recording asked for didn't start, and why; a recording already going carries on.
    RecordingRefused {
        error: Error,
    },
}

/// Counts for `jawd status`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Times the output found nothing to play while a message was playing.
    pub underruns: u64,
    /// Microphone samples lost because the ring was full.
    pub overruns: u64,
    /// Progress dropped, or playback states superseded while held, because the caller wasn't
    /// draining events.
    pub dropped_events: u64,
    /// Damaged packets in played messages, concealed rather than ending them.
    pub concealed: u64,
}

#[derive(Default)]
pub(crate) struct Counters {
    underruns: AtomicU64,
    overruns: AtomicU64,
    dropped: AtomicU64,
    concealed: AtomicU64,
}

enum Command {
    Play { key: String, path: PathBuf, at: Duration, speed: Speed },
    Pause,
    Resume,
    Seek(Duration),
    Speed(Speed),
    Stop,
    Record { path: PathBuf, limit: Duration },
    StopRecording,
    CancelRecording,
}

/// The handle to the audio thread. Dropping it stops playback, cancels a recording and ends the
/// thread.
pub struct Voice {
    commands: Option<Sender<Command>>,
    thread: Option<JoinHandle<()>>,
    counters: Arc<Counters>,
}

impl Voice {
    /// Starts the audio thread with the system's default output and input.
    pub fn system() -> (Voice, Receiver<Event>) {
        Voice::start(SystemSpeaker, SystemMicrophone)
    }

    pub fn start(speaker: impl Speaker + 'static, microphone: impl Microphone + 'static) -> (Voice, Receiver<Event>) {
        let (commands, inbox) = crossbeam_channel::bounded(COMMANDS);
        let (events, outbox) = crossbeam_channel::bounded(EVENTS);
        let counters = Arc::new(Counters::default());
        let (speaker, microphone): (Box<dyn Speaker>, Box<dyn Microphone>) = (Box::new(speaker), Box::new(microphone));
        let shared = Arc::clone(&counters);
        // Streams aren't Send on every platform, so they're made and kept on this thread alone.
        let thread = std::thread::Builder::new()
            .name("jaw-voice".into())
            .spawn(move || {
                let thread = AudioThread {
                    player: Player::new(speaker),
                    recorder: Recorder::new(microphone),
                    events,
                    held: Held::default(),
                    counters: shared,
                };
                thread.run(inbox)
            })
            .expect("the audio thread starts");
        (Voice { commands: Some(commands), thread: Some(thread), counters }, outbox)
    }

    /// Hands the audio thread a command; it never waits, and says so when the thread is behind.
    fn send(&self, command: Command) -> Result<(), Error> {
        let commands = self.commands.as_ref().ok_or_else(|| Error::Busy("the audio thread has stopped".into()))?;
        commands.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => Error::Busy("the audio thread is behind; try again in a moment".into()),
            TrySendError::Disconnected(_) => Error::Busy("the audio thread has stopped".into()),
        })
    }

    /// Plays a voice message from `at`, stopping whatever played before.
    pub fn play(
        &self,
        key: impl Into<String>,
        path: impl Into<PathBuf>,
        at: Duration,
        speed: Speed,
    ) -> Result<(), Error> {
        self.send(Command::Play { key: key.into(), path: path.into(), at, speed })
    }

    pub fn pause(&self) -> Result<(), Error> {
        self.send(Command::Pause)
    }

    /// Carries on from where it paused.
    pub fn resume(&self) -> Result<(), Error> {
        self.send(Command::Resume)
    }

    pub fn seek(&self, at: Duration) -> Result<(), Error> {
        self.send(Command::Seek(at))
    }

    pub fn set_speed(&self, speed: Speed) -> Result<(), Error> {
        self.send(Command::Speed(speed))
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.send(Command::Stop)
    }

    /// Records to `path`, at most `limit` and never more than [`RECORDING_LIMIT`]. `path` holds the
    /// whole file once [`Event::Recorded`] comes and never a part of one; until then the recording
    /// is at [`part_path`]. A path that's taken is refused, as is a new recording while the last
    /// one's end waits to be read. Playback stops first.
    pub fn record(&self, path: impl Into<PathBuf>, limit: Duration) -> Result<(), Error> {
        self.send(Command::Record { path: path.into(), limit })
    }

    /// Finishes the recording: [`Event::Recorded`] follows once it's synced and in place.
    pub fn stop_recording(&self) -> Result<(), Error> {
        self.send(Command::StopRecording)
    }

    /// Throws the recording away; nothing is kept.
    pub fn cancel_recording(&self) -> Result<(), Error> {
        self.send(Command::CancelRecording)
    }

    pub fn stats(&self) -> Stats {
        let c = &self.counters;
        Stats {
            underruns: c.underruns.load(Ordering::Relaxed),
            overruns: c.overruns.load(Ordering::Relaxed),
            dropped_events: c.dropped.load(Ordering::Relaxed),
            concealed: c.concealed.load(Ordering::Relaxed),
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.commands = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct AudioThread {
    player: Player,
    recorder: Recorder,
    events: Sender<Event>,
    held: Held,
    counters: Arc<Counters>,
}

/// Changes of state that found the caller behind, kept until the channel has room: the newest
/// playback state and the newest refusal, since each supersedes the last, and recordings' ends,
/// which are never dropped. No recording starts while an end is held, so there's at most one.
#[derive(Default)]
struct Held {
    playback: Option<Event>,
    ends: Vec<Event>,
    refusal: Option<Event>,
}

impl Held {
    fn is_empty(&self) -> bool {
        self.playback.is_none() && self.ends.is_empty() && self.refusal.is_none()
    }
}

impl AudioThread {
    fn run(mut self, commands: Receiver<Command>) {
        // The device callbacks wake the thread when the output runs low or the input fills.
        let (wake, woken) = crossbeam_channel::bounded::<()>(1);
        let mut tick: Option<Instant> = None;
        loop {
            let deadline = tick.map_or_else(crossbeam_channel::never, crossbeam_channel::at);
            select! {
                recv(commands) -> command => match command {
                    Ok(command) => self.handle(command, &wake),
                    Err(_) => break,
                },
                recv(woken) -> _ => {}
                recv(deadline) -> _ => {
                    if let Some(event) = self.player.tick() {
                        self.progress(event);
                    }
                    for event in self.recorder.tick() {
                        self.send(event);
                    }
                }
            }
            if let Some(event) = self.player.refill() {
                self.send(event);
            }
            if let Some(event) = self.recorder.drain() {
                self.send(event);
            }
            self.release();
            // Held events are offered again each tick until the caller makes room.
            let active = self.player.playing() || self.recorder.active() || !self.held.is_empty();
            let now = Instant::now();
            tick = match tick {
                _ if !active => None,
                Some(t) if t > now => Some(t),
                Some(t) => Some((t + TICK).max(now)),
                None => Some(now + TICK),
            };
        }
        self.player.stop();
        self.recorder.cancel();
    }

    fn handle(&mut self, command: Command, wake: &Sender<()>) {
        let counters = &self.counters;
        let events: Vec<Event> = match command {
            Command::Play { key, path, at, speed } => self.player.play(key, &path, at, speed, wake, counters),
            Command::Pause => self.player.pause().into_iter().collect(),
            Command::Resume => self.player.resume(wake, counters).into_iter().collect(),
            Command::Seek(at) => self.player.seek(at).into_iter().collect(),
            Command::Speed(speed) => self.player.set_speed(speed).into_iter().collect(),
            Command::Stop => self.player.stop().into_iter().collect(),
            Command::Record { .. } if !self.held.ends.is_empty() => {
                let why = "the last recording's end hasn't been read yet; read the events before recording again";
                vec![Event::RecordingRefused { error: Error::Recording(why.into()) }]
            }
            Command::Record { .. } if self.recorder.active() => {
                let why = "jaw is already recording; stop or cancel that first";
                vec![Event::RecordingRefused { error: Error::Recording(why.into()) }]
            }
            Command::Record { path, limit } => {
                let mut events: Vec<Event> = self.player.stop().into_iter().collect();
                if let Err(error) = self.recorder.start(path, limit, wake, counters) {
                    events.push(Event::RecordingRefused { error });
                }
                events
            }
            Command::StopRecording => self.recorder.stop().into_iter().collect(),
            Command::CancelRecording => self.recorder.cancel().into_iter().collect(),
        };
        for event in events {
            self.send(event);
        }
    }

    /// Progress is dropped when the caller has fallen behind or a change of state is waiting; the
    /// next tick says the same, newer.
    fn progress(&self, event: Event) {
        let room = self.held.is_empty() && self.events.len() < EVENTS / 2;
        if !room || matches!(self.events.try_send(event), Err(TrySendError::Full(_))) {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Sends a change of state, or holds it while the caller is behind, keeping its order behind
    /// whatever of its kind is already held.
    fn send(&mut self, event: Event) {
        match event {
            Event::Recording { .. } | Event::RecordingSilent => self.progress(event),
            Event::Playback { .. } | Event::PlaybackFailed { .. } => {
                let Some(event) = (if self.held.playback.is_none() { self.offer(event) } else { Some(event) }) else {
                    return;
                };
                if self.held.playback.replace(event).is_some() {
                    self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Event::RecordingRefused { .. } => {
                let Some(event) = (if self.held.refusal.is_none() { self.offer(event) } else { Some(event) }) else {
                    return;
                };
                if self.held.refusal.replace(event).is_some() {
                    self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Event::Recorded(_) | Event::RecordingCancelled | Event::RecordingFailed { .. } => {
                let Some(event) = (if self.held.ends.is_empty() { self.offer(event) } else { Some(event) }) else {
                    return;
                };
                self.held.ends.push(event);
            }
        }
    }

    /// The event back if the channel is full; gone if it was sent or no one is listening.
    fn offer(&self, event: Event) -> Option<Event> {
        match self.events.try_send(event) {
            Err(TrySendError::Full(event)) => Some(event),
            _ => None,
        }
    }

    fn release(&mut self) {
        if let Some(event) = self.held.playback.take() {
            self.held.playback = self.offer(event);
        }
        while !self.held.ends.is_empty() {
            let event = self.held.ends.remove(0);
            if let Some(event) = self.offer(event) {
                self.held.ends.insert(0, event);
                break;
            }
        }
        if let Some(event) = self.held.refusal.take() {
            self.held.refusal = self.offer(event);
        }
    }
}
