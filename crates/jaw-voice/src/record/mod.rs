//! Recording: the microphone's samples go from the device's thread into a bounded ring, and the
//! audio thread takes them, brings them to 16 kHz, measures them and encodes them as they come.

mod encode;
mod file;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::Sender;
use rtrb::{Consumer, RingBuffer};

use crate::device::{Capture, Input, Microphone};
use crate::resample::Resampler;
use crate::waveform::Envelope;
use crate::{Counters, Error, Event, RECORDING_LIMIT, Recorded, RecordingEnd};
use encode::{OggOpusWriter, RATE};
use file::PartFile;

pub use file::part_path;

/// The ring holds two seconds of the microphone, far more than the audio thread ever lags.
const RING_SECONDS: usize = 2;

pub(crate) struct Recorder {
    microphone: Box<dyn Microphone>,
    current: Option<Recording>,
}

struct Recording {
    /// Dropped first when finishing, so the microphone stops before the ring's last samples are taken.
    input: Option<Box<dyn Input>>,
    ring: Consumer<f32>,
    low: Arc<AtomicBool>,
    /// Samples the callback couldn't fit in the ring.
    lost: Arc<AtomicU64>,
    path: PathBuf,
    rate: u32,
    limit: u64,
    resample: Option<Resampler>,
    writer: OggOpusWriter<PartFile>,
    envelope: Envelope,
    taken: Vec<f32>,
    converted: Vec<f32>,
    squares: f64,
    measured: u64,
    captured: u64,
    heard: bool,
    warned: bool,
}

impl Recorder {
    pub(crate) fn new(microphone: Box<dyn Microphone>) -> Recorder {
        Recorder { microphone, current: None }
    }

    pub(crate) fn active(&self) -> bool {
        self.current.is_some()
    }

    pub(crate) fn start(
        &mut self,
        path: PathBuf,
        limit: Duration,
        wake: &Sender<()>,
        counters: &Arc<Counters>,
    ) -> Result<(), Error> {
        if self.current.is_some() {
            return Err(Error::Recording("jaw is already recording; stop or cancel that first".into()));
        }
        // The file first, so a name that's taken never lights the microphone.
        let writer = OggOpusWriter::new(PartFile::create(&path)?, serial())?;
        let mut input = self.microphone.open()?;
        let rate = input.rate();
        let capacity = rate as usize * RING_SECONDS;
        let (mut producer, ring) = RingBuffer::new(capacity);
        let low = Arc::new(AtomicBool::new(false));
        let lost = Arc::new(AtomicU64::new(0));
        let capture: Capture = {
            let (low, lost, wake, counters) = (Arc::clone(&low), Arc::clone(&lost), wake.clone(), Arc::clone(counters));
            Box::new(move |samples: &[f32]| {
                let (_, rest) = producer.push_partial_slice(samples);
                if !rest.is_empty() {
                    lost.fetch_add(rest.len() as u64, Ordering::Relaxed);
                    counters.overruns.fetch_add(rest.len() as u64, Ordering::Relaxed);
                }
                if capacity - producer.slots() > capacity / 4 && !low.swap(true, Ordering::AcqRel) {
                    let _ = wake.try_send(());
                }
            })
        };
        input.start(capture)?;
        self.current = Some(Recording {
            input: Some(input),
            ring,
            low,
            lost,
            path,
            rate,
            limit: (limit.min(RECORDING_LIMIT).as_secs_f64() * f64::from(RATE)) as u64,
            resample: (rate != RATE).then(|| Resampler::new(rate, RATE)),
            writer,
            envelope: Envelope::new(RATE),
            taken: Vec::new(),
            converted: Vec::new(),
            squares: 0.0,
            measured: 0,
            captured: 0,
            heard: false,
            warned: false,
        });
        Ok(())
    }

    /// Takes what the microphone has given; the event when the cap or a failure ends the recording.
    pub(crate) fn drain(&mut self) -> Option<Event> {
        let recording = self.current.as_mut()?;
        match recording.take() {
            Ok(false) => None,
            Ok(true) => Some(self.current.take()?.finish(RecordingEnd::Capped)),
            Err(error) => {
                self.current = None;
                Some(Event::RecordingFailed { error })
            }
        }
    }

    /// Ten times a second: how long and how loud, a warning when the microphone gives only silence,
    /// or the end when the device stopped.
    pub(crate) fn tick(&mut self) -> Vec<Event> {
        let Some(r) = self.current.as_mut() else {
            return Vec::new();
        };
        if let Some(fault) = r.input.as_ref().and_then(|i| i.fault()) {
            let recording = self.current.take().expect("checked above");
            return vec![recording.finish(RecordingEnd::DeviceFailed(fault))];
        }
        let mut events = vec![Event::Recording {
            elapsed: Duration::from_secs_f64(r.writer.samples() as f64 / f64::from(RATE)),
            level: level(r.squares, r.measured),
        }];
        (r.squares, r.measured) = (0.0, 0);
        if !r.heard && !r.warned && r.captured >= u64::from(r.rate) {
            r.warned = true;
            events.push(Event::RecordingSilent);
        }
        events
    }

    pub(crate) fn stop(&mut self) -> Option<Event> {
        Some(self.current.take()?.finish(RecordingEnd::Stopped))
    }

    pub(crate) fn cancel(&mut self) -> Option<Event> {
        // Dropping the recording stops the microphone and removes its `.part`.
        self.current.take().map(|_| Event::RecordingCancelled)
    }
}

impl Recording {
    /// Takes everything in the ring; true once the cap is reached.
    fn take(&mut self) -> Result<bool, Error> {
        self.low.store(false, Ordering::Release);
        loop {
            let n = self.ring.slots();
            if n == 0 {
                return Ok(false);
            }
            let chunk = self.ring.read_chunk(n).expect("the slots were counted");
            let (a, b) = chunk.as_slices();
            self.taken.clear();
            self.taken.extend_from_slice(a);
            self.taken.extend_from_slice(b);
            chunk.commit_all();
            for &x in &self.taken {
                self.squares += f64::from(x * x);
                self.heard |= x != 0.0;
            }
            self.measured += self.taken.len() as u64;
            self.captured += self.taken.len() as u64;
            if self.convert(false)? {
                return Ok(true);
            }
        }
    }

    /// Brings what was taken to 16 kHz and encodes it, up to the cap; true once it's reached.
    fn convert(&mut self, last: bool) -> Result<bool, Error> {
        self.converted.clear();
        match &mut self.resample {
            Some(r) => {
                r.process(&self.taken, &mut self.converted);
                if last {
                    r.finish(&mut self.converted);
                }
            }
            None => self.converted.extend_from_slice(&self.taken),
        }
        let room = self.limit.saturating_sub(self.writer.samples()) as usize;
        let keep = &self.converted[..self.converted.len().min(room)];
        self.envelope.push(keep);
        self.writer.push(keep)?;
        Ok(self.writer.samples() >= self.limit)
    }

    fn finish(self, end: RecordingEnd) -> Event {
        match self.complete(end) {
            Ok(recorded) => Event::Recorded(recorded),
            Err(error) => Event::RecordingFailed { error },
        }
    }

    fn complete(mut self, end: RecordingEnd) -> Result<Recorded, Error> {
        self.input = None;
        if end != RecordingEnd::Capped && !self.take()? {
            self.taken.clear();
            self.convert(true)?;
        }
        let samples = self.writer.samples();
        if samples == 0 {
            return Err(Error::Recording("nothing was recorded".into()));
        }
        let waveform = self.envelope.points();
        self.writer.finish()?.finish()?;
        Ok(Recorded {
            path: self.path,
            duration: Duration::from_secs_f64(samples as f64 / f64::from(RATE)),
            waveform,
            end,
            gaps: Duration::from_secs_f64(self.lost.load(Ordering::Relaxed) as f64 / f64::from(self.rate)),
        })
    }
}

/// Loudness from 0 to 100: -50 dBFS and below is 0, full scale is 100.
fn level(squares: f64, n: u64) -> u8 {
    if n == 0 || squares <= 0.0 {
        return 0;
    }
    let db = 10.0 * (squares / n as f64).log10();
    ((db + 50.0) * 2.0).clamp(0.0, 100.0) as u8
}

fn serial() -> u32 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    nanos ^ std::process::id().rotate_left(16)
}
