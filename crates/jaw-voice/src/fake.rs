//! Devices for tests: a speaker that takes what's rendered at a chosen pace and counts it, and a
//! microphone that delivers given samples at a pace. Neither touches the system's audio, so tests
//! and CI never play a sound, open a microphone or meet a permission prompt.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::Error;
use crate::device::{Capture, Input, Microphone, Output, Render, Speaker};

/// Blocks of 10 ms, as a real device's callbacks come.
const BLOCK: Duration = Duration::from_millis(10);

pub struct FakeSpeaker {
    rate: u32,
    pace: f64,
    state: Arc<Played>,
    available: bool,
}

/// What a fake speaker was given, read from a test.
#[derive(Clone)]
pub struct SpeakerProbe(Arc<Played>);

#[derive(Default)]
struct Played {
    opens: AtomicU64,
    frames: AtomicU64,
    sound: AtomicU64,
    keep: AtomicBool,
    kept: Mutex<Vec<f32>>,
    first_sound: Mutex<Option<Instant>>,
}

impl FakeSpeaker {
    /// A speaker at `rate` that takes audio `pace` times as fast as a real one.
    pub fn new(rate: u32, pace: f64) -> (FakeSpeaker, SpeakerProbe) {
        let state = Arc::new(Played::default());
        (FakeSpeaker { rate, pace, state: Arc::clone(&state), available: true }, SpeakerProbe(state))
    }

    /// Keeps every sample it's given, for tests that look at them.
    pub fn keeping(self) -> FakeSpeaker {
        self.state.keep.store(true, Ordering::Relaxed);
        self
    }

    /// A speaker that can't be opened, as when no output is connected.
    pub fn unavailable() -> FakeSpeaker {
        FakeSpeaker { rate: 48_000, pace: 1.0, state: Arc::default(), available: false }
    }
}

impl SpeakerProbe {
    pub fn opens(&self) -> u64 {
        self.0.opens.load(Ordering::Relaxed)
    }

    /// Frames rendered, silence included.
    pub fn frames(&self) -> u64 {
        self.0.frames.load(Ordering::Relaxed)
    }

    /// Frames that weren't silence: what was heard.
    pub fn sound(&self) -> u64 {
        self.0.sound.load(Ordering::Relaxed)
    }

    pub fn kept(&self) -> Vec<f32> {
        self.0.kept.lock().unwrap().clone()
    }

    /// When the first sound was written to the device.
    pub fn first_sound(&self) -> Option<Instant> {
        *self.0.first_sound.lock().unwrap()
    }
}

impl Speaker for FakeSpeaker {
    fn open(&mut self) -> Result<Box<dyn Output>, Error> {
        if !self.available {
            return Err(Error::Device("there's no audio output; connect speakers or headphones".into()));
        }
        self.state.opens.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(FakeOutput { rate: self.rate, pace: self.pace, state: Arc::clone(&self.state), runner: None }))
    }
}

struct FakeOutput {
    rate: u32,
    pace: f64,
    state: Arc<Played>,
    runner: Option<Runner>,
}

impl Output for FakeOutput {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn start(&mut self, mut render: Render) -> Result<(), Error> {
        let state = Arc::clone(&self.state);
        let mut buf = vec![0.0; (self.rate / 100) as usize];
        self.runner = Some(Runner::spawn(self.pace, move || {
            buf.fill(0.0);
            render(&mut buf);
            let sound = buf.iter().filter(|&&x| x != 0.0).count() as u64;
            if sound > 0 {
                state.first_sound.lock().unwrap().get_or_insert_with(Instant::now);
            }
            state.frames.fetch_add(buf.len() as u64, Ordering::Relaxed);
            state.sound.fetch_add(sound, Ordering::Relaxed);
            if state.keep.load(Ordering::Relaxed) {
                state.kept.lock().unwrap().extend_from_slice(&buf);
            }
            true
        }));
        Ok(())
    }

    fn fault(&self) -> Option<String> {
        None
    }
}

pub struct FakeMicrophone {
    rate: u32,
    pace: f64,
    samples: Arc<Vec<f32>>,
    state: Arc<Heard>,
    available: bool,
    fails_after: Option<usize>,
}

/// How far a fake microphone has got, read from a test.
#[derive(Clone)]
pub struct MicrophoneProbe(Arc<Heard>);

#[derive(Default)]
struct Heard {
    delivered: AtomicU64,
    finished: AtomicBool,
}

impl FakeMicrophone {
    /// A microphone at `rate` that delivers `samples`, `pace` times as fast as a real one, then
    /// nothing more.
    pub fn new(rate: u32, samples: Vec<f32>, pace: f64) -> (FakeMicrophone, MicrophoneProbe) {
        let state = Arc::new(Heard::default());
        let samples = Arc::new(samples);
        let mic = FakeMicrophone { rate, pace, samples, state: Arc::clone(&state), available: true, fails_after: None };
        (mic, MicrophoneProbe(state))
    }

    /// A microphone that can't be opened, as when none is connected.
    pub fn unavailable() -> FakeMicrophone {
        let (mut mic, _) = FakeMicrophone::new(48_000, Vec::new(), 1.0);
        mic.available = false;
        mic
    }

    /// Stops working after delivering `samples`, as when it's unplugged.
    pub fn failing_after(mut self, samples: usize) -> FakeMicrophone {
        self.fails_after = Some(samples);
        self
    }
}

impl MicrophoneProbe {
    pub fn delivered(&self) -> u64 {
        self.0.delivered.load(Ordering::Relaxed)
    }

    /// Whether every sample has been delivered.
    pub fn finished(&self) -> bool {
        self.0.finished.load(Ordering::Relaxed)
    }
}

impl Microphone for FakeMicrophone {
    fn open(&mut self) -> Result<Box<dyn Input>, Error> {
        if !self.available {
            return Err(Error::Device("there's no microphone; connect one to record".into()));
        }
        let (rate, pace) = (self.rate, self.pace);
        Ok(Box::new(FakeInput {
            rate,
            pace,
            samples: Arc::clone(&self.samples),
            state: Arc::clone(&self.state),
            fails_after: self.fails_after,
            runner: None,
        }))
    }
}

struct FakeInput {
    rate: u32,
    pace: f64,
    samples: Arc<Vec<f32>>,
    state: Arc<Heard>,
    fails_after: Option<usize>,
    runner: Option<Runner>,
}

impl Input for FakeInput {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn start(&mut self, mut capture: Capture) -> Result<(), Error> {
        let (samples, state) = (Arc::clone(&self.samples), Arc::clone(&self.state));
        let block = (self.rate / 100) as usize;
        let end = self.fails_after.unwrap_or(usize::MAX).min(samples.len());
        let mut at = 0;
        self.runner = Some(Runner::spawn(self.pace, move || {
            let chunk = &samples[at..(at + block).min(end)];
            capture(chunk);
            at += chunk.len();
            state.delivered.store(at as u64, Ordering::Relaxed);
            let more = at < end;
            state.finished.store(!more, Ordering::Relaxed);
            more
        }));
        Ok(())
    }

    fn fault(&self) -> Option<String> {
        let failed = self.fails_after.is_some_and(|n| self.state.delivered.load(Ordering::Relaxed) >= n as u64);
        failed.then(|| "the microphone was unplugged".to_string())
    }
}

/// A thread calling `step` once a block, `pace` times as fast as real time, until `step` returns
/// false or the runner is dropped.
struct Runner {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Runner {
    fn spawn(pace: f64, mut step: impl FnMut() -> bool + Send + 'static) -> Runner {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let period = BLOCK.div_f64(pace);
        let thread = std::thread::spawn(move || {
            let mut next = Instant::now();
            while !stopped.load(Ordering::Relaxed) && step() {
                next += period;
                if let Some(wait) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
            }
        });
        Runner { stop, thread: Some(thread) }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
