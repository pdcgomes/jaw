//! Playback: one message at a time, decoded on the audio thread a little ahead into a lock-free
//! ring the device's callback drains. Seeking or changing speed decodes again from the position;
//! pausing closes the device, and resuming decodes from where it paused.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crossbeam_channel::Sender;
use rtrb::{Producer, RingBuffer};

use crate::device::{Output, Render, Speaker};
use crate::oggopus::{Decode, OPUS_RATES, VoiceFile};
use crate::resample::Resampler;
use crate::stretch::Stretch;
use crate::{Counters, Error, Event, PlaybackState, Speed};

/// The ring holds half a second; the audio thread tops it up when it's half empty.
const AHEAD: Duration = Duration::from_millis(500);
/// Sound starts once this much is decoded.
const FIRST: Duration = Duration::from_millis(60);

pub(crate) struct Player {
    speaker: Box<dyn Speaker>,
    current: Option<Current>,
}

struct Current {
    key: String,
    file: Arc<VoiceFile>,
    speed: Speed,
    state: State,
}

enum State {
    Playing(Box<Stream>),
    Paused(Duration),
}

impl Player {
    pub(crate) fn new(speaker: Box<dyn Speaker>) -> Player {
        Player { speaker, current: None }
    }

    pub(crate) fn playing(&self) -> bool {
        matches!(self.current, Some(Current { state: State::Playing(_), .. }))
    }

    pub(crate) fn play(
        &mut self,
        key: String,
        path: &Path,
        at: Duration,
        speed: Speed,
        wake: &Sender<()>,
        counters: &Arc<Counters>,
    ) -> Vec<Event> {
        let mut events: Vec<Event> = self.stop().into_iter().collect();
        let started = VoiceFile::open(path).and_then(|file| {
            let at = at.min(file.duration());
            let stream = Stream::start(self.speaker.as_mut(), &file, at, speed, wake, counters)?;
            Ok(Current { key: key.clone(), file, speed, state: State::Playing(Box::new(stream)) })
        });
        match started {
            Ok(current) => {
                events.push(current.event(PlaybackState::Playing));
                self.current = Some(current);
            }
            Err(error) => events.push(Event::PlaybackFailed { key, error }),
        }
        events
    }

    pub(crate) fn pause(&mut self) -> Option<Event> {
        let current = self.current.as_mut()?;
        if let State::Playing(stream) = &current.state {
            current.state = State::Paused(stream.position(current.speed, current.file.duration()));
        }
        Some(current.event(PlaybackState::Paused))
    }

    pub(crate) fn resume(&mut self, wake: &Sender<()>, counters: &Arc<Counters>) -> Option<Event> {
        let current = self.current.as_mut()?;
        let State::Paused(at) = current.state else {
            return Some(current.event(PlaybackState::Playing));
        };
        match Stream::start(self.speaker.as_mut(), &current.file, at, current.speed, wake, counters) {
            Ok(stream) => {
                current.state = State::Playing(Box::new(stream));
                Some(current.event(PlaybackState::Playing))
            }
            Err(error) => {
                let key = self.current.take()?.key;
                Some(Event::PlaybackFailed { key, error })
            }
        }
    }

    pub(crate) fn seek(&mut self, at: Duration) -> Option<Event> {
        let current = self.current.as_mut()?;
        let at = at.min(current.file.duration());
        current.restart(at, current.speed)
    }

    pub(crate) fn set_speed(&mut self, speed: Speed) -> Option<Event> {
        let current = self.current.as_mut()?;
        let at = current.position();
        current.restart(at, speed)
    }

    pub(crate) fn stop(&mut self) -> Option<Event> {
        let current = self.current.take()?;
        Some(current.event(PlaybackState::Stopped))
    }

    /// Tops the ring up, and reports the end or a device that stopped.
    pub(crate) fn refill(&mut self) -> Option<Event> {
        let current = self.current.as_mut()?;
        let State::Playing(stream) = &mut current.state else {
            return None;
        };
        if let Some(fault) = stream.output.fault() {
            let key = self.current.take()?.key;
            return Some(Event::PlaybackFailed {
                key,
                error: Error::Device(format!("the audio output stopped: {fault}")),
            });
        }
        stream.refill();
        if stream.ended() {
            return Some(self.current.take()?.event(PlaybackState::Ended));
        }
        None
    }

    /// Where playback is, ten times a second while it plays.
    pub(crate) fn tick(&self) -> Option<Event> {
        let current = self.current.as_ref().filter(|c| matches!(c.state, State::Playing(_)))?;
        Some(current.event(PlaybackState::Playing))
    }
}

impl Current {
    fn position(&self) -> Duration {
        match &self.state {
            State::Playing(stream) => stream.position(self.speed, self.file.duration()),
            State::Paused(at) => *at,
        }
    }

    fn restart(&mut self, at: Duration, speed: Speed) -> Option<Event> {
        self.speed = speed;
        match &mut self.state {
            State::Paused(_) => {
                self.state = State::Paused(at);
                Some(self.event(PlaybackState::Paused))
            }
            State::Playing(stream) => match stream.restart(&self.file, at, speed) {
                Ok(()) => Some(self.event(PlaybackState::Playing)),
                Err(error) => {
                    self.state = State::Paused(at);
                    Some(Event::PlaybackFailed { key: self.key.clone(), error })
                }
            },
        }
    }

    fn event(&self, state: PlaybackState) -> Event {
        let length = self.file.duration();
        let at = if state == PlaybackState::Ended { length } else { self.position() };
        Event::Playback { key: self.key.clone(), state, at, length, speed: self.speed }
    }
}

/// What the callback and the audio thread share.
#[derive(Default)]
struct Shared {
    /// Samples the device has taken, counted by the callback alone.
    popped: AtomicU64,
    /// Samples before this are stale after a seek, and the callback drops them unplayed.
    discard: AtomicU64,
    /// Set by the callback when the ring is half empty, so it wakes the audio thread once.
    low: AtomicBool,
    /// Everything has been decoded, so an empty ring is the end rather than an underrun.
    drained: AtomicBool,
}

struct Stream {
    output: Box<dyn Output>,
    producer: Producer<f32>,
    shared: Arc<Shared>,
    rate: u32,
    source: Source,
    pending: Vec<f32>,
    sent: usize,
    pushed: u64,
    /// The segment since the last start, seek or change of speed: where it began in the message
    /// and how many samples had been pushed by then.
    from: Duration,
    from_sample: u64,
    exhausted: bool,
}

impl Stream {
    fn start(
        speaker: &mut dyn Speaker,
        file: &Arc<VoiceFile>,
        at: Duration,
        speed: Speed,
        wake: &Sender<()>,
        counters: &Arc<Counters>,
    ) -> Result<Stream, Error> {
        let output = speaker.open()?;
        let rate = output.rate();
        let capacity = (f64::from(rate) * AHEAD.as_secs_f64()) as usize;
        let (producer, mut consumer) = RingBuffer::<f32>::new(capacity);
        let shared = Arc::new(Shared::default());
        let mut stream = Stream {
            output,
            producer,
            shared: Arc::clone(&shared),
            rate,
            source: Source::new(file, rate, at, speed, counters)?,
            pending: Vec::new(),
            sent: 0,
            pushed: 0,
            from: at,
            from_sample: 0,
            exhausted: false,
        };
        stream.fill((f64::from(rate) * FIRST.as_secs_f64()) as usize);

        let (wake, counters) = (wake.clone(), Arc::clone(counters));
        let render: Render = Box::new(move |out: &mut [f32]| {
            let mut popped = shared.popped.load(Ordering::Relaxed);
            let discard = shared.discard.load(Ordering::Acquire);
            if popped < discard {
                let n = ((discard - popped) as usize).min(consumer.slots());
                if let Ok(stale) = consumer.read_chunk(n) {
                    stale.commit_all();
                }
                popped += n as u64;
            }
            let n = if popped >= discard {
                let (heard, _) = consumer.pop_partial_slice(out);
                heard.len()
            } else {
                0
            };
            out[n..].fill(0.0);
            popped += n as u64;
            shared.popped.store(popped, Ordering::Release);
            if n < out.len() && !shared.drained.load(Ordering::Acquire) {
                counters.underruns.fetch_add(1, Ordering::Relaxed);
            }
            if consumer.slots() < capacity / 2 && !shared.low.swap(true, Ordering::AcqRel) {
                let _ = wake.try_send(());
            }
        });
        stream.output.start(render)?;
        Ok(stream)
    }

    fn restart(&mut self, file: &Arc<VoiceFile>, at: Duration, speed: Speed) -> Result<(), Error> {
        self.source = Source::new(file, self.rate, at, speed, &self.source.counters)?;
        self.pending.clear();
        self.sent = 0;
        self.exhausted = false;
        self.shared.drained.store(false, Ordering::Release);
        self.from = at;
        self.from_sample = self.pushed;
        self.shared.discard.store(self.pushed, Ordering::Release);
        self.refill();
        Ok(())
    }

    fn refill(&mut self) {
        self.shared.low.store(false, Ordering::Release);
        self.fill(usize::MAX);
    }

    /// Pushes decoded samples until the ring is full, `limit` more are in, or the message ends.
    fn fill(&mut self, limit: usize) {
        let mut pushed = 0;
        while pushed < limit {
            if self.sent == self.pending.len() {
                self.pending.clear();
                self.sent = 0;
                if self.exhausted {
                    self.shared.drained.store(true, Ordering::Release);
                    return;
                }
                self.exhausted = !self.source.fill(&mut self.pending, 960);
                continue;
            }
            let room = self.producer.slots().min(limit - pushed);
            if room == 0 {
                return;
            }
            let end = (self.sent + room).min(self.pending.len());
            let (done, _) = self.producer.push_partial_slice(&self.pending[self.sent..end]);
            self.sent += done.len();
            pushed += done.len();
            self.pushed += done.len() as u64;
        }
    }

    fn position(&self, speed: Speed, length: Duration) -> Duration {
        let heard = self.shared.popped.load(Ordering::Acquire).saturating_sub(self.from_sample);
        let seconds = heard as f64 / f64::from(self.rate) * speed.factor();
        (self.from + Duration::from_secs_f64(seconds)).min(length)
    }

    fn ended(&self) -> bool {
        self.shared.drained.load(Ordering::Acquire) && self.shared.popped.load(Ordering::Acquire) >= self.pushed
    }
}

/// Decoding, then the time-stretch for faster speeds, then the device's rate.
struct Source {
    decode: Decode,
    stretch: Option<Stretch>,
    resample: Option<Resampler>,
    decoded: Vec<f32>,
    stretched: Vec<f32>,
    done: bool,
    counters: Arc<Counters>,
    concealed: u64,
}

impl Source {
    fn new(
        file: &Arc<VoiceFile>,
        rate: u32,
        at: Duration,
        speed: Speed,
        counters: &Arc<Counters>,
    ) -> Result<Source, Error> {
        // libopus decodes straight to the device's rate when it can, saving a resample.
        let decode_rate = if OPUS_RATES.contains(&rate) { rate } else { 48_000 };
        Ok(Source {
            decode: file.decode(decode_rate, at)?,
            stretch: (speed != Speed::Normal).then(|| Stretch::new(decode_rate, speed.factor())),
            resample: (decode_rate != rate).then(|| Resampler::new(decode_rate, rate)),
            decoded: Vec::new(),
            stretched: Vec::new(),
            done: false,
            counters: Arc::clone(counters),
            concealed: 0,
        })
    }

    /// Appends at least `want` samples to `out` unless the message ends; false once it has.
    fn fill(&mut self, out: &mut Vec<f32>, want: usize) -> bool {
        let start = out.len();
        while !self.done && out.len() - start < want {
            self.decoded.clear();
            let more = self.decode.next(&mut self.decoded);
            self.done = !more;
            let concealed = self.decode.concealed();
            if concealed > self.concealed {
                self.counters.concealed.fetch_add(concealed - self.concealed, Ordering::Relaxed);
                self.concealed = concealed;
            }
            let stage = match &mut self.stretch {
                Some(s) => {
                    self.stretched.clear();
                    if more {
                        s.push(&self.decoded, &mut self.stretched);
                    } else {
                        s.finish(&mut self.stretched);
                    }
                    &self.stretched
                }
                None => &self.decoded,
            };
            match &mut self.resample {
                Some(r) => {
                    r.process(stage, out);
                    if !more {
                        r.finish(out);
                    }
                }
                None => out.extend_from_slice(stage),
            }
        }
        !self.done
    }
}
