//! Measures the crate on a five-minute voice note, through fake devices so nothing is heard and
//! no microphone opens:
//!
//!     cargo run --release -p jaw-voice --example measure
//!
//! The note is the speech fixture said over and over, recorded through the crate's own path.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::bail;
use jaw_voice::fake::{FakeMicrophone, FakeSpeaker};
use jaw_voice::{Event, PlaybackState, RECORDING_LIMIT, Speed, Voice, VoiceFile, analyse};

/// Counts the bytes the process holds, so memory is measured rather than guessed.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const MINUTES: f64 = 5.0;

fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let note = dir.path().join("five-minutes.ogg");
    let speech = speech(MINUTES * 60.0)?;

    // A hundred times real time is well within what the audio thread keeps up with.
    let (voice, _) = record(&note, speech.clone(), 100.0)?;
    let file = VoiceFile::open(&note)?;
    let size = std::fs::metadata(&note)?.len();
    println!(
        "the note: {:.2} s, {:.1} kbit/s, {} microphone samples lost",
        file.duration().as_secs_f64(),
        size as f64 * 8.0 / file.duration().as_secs_f64() / 1e3,
        voice.stats().overruns
    );
    drop(voice);

    for rate in [48_000, 16_000] {
        let started = Instant::now();
        let pcm = file.pcm(rate)?;
        let took = started.elapsed().as_secs_f64();
        println!(
            "decoding at {} kHz: {:.0}× real time ({:.0} ms for {} samples)",
            rate / 1000,
            MINUTES * 60.0 / took,
            took * 1e3,
            pcm.len()
        );
    }

    let mut times: Vec<f64> = (0..5)
        .map(|_| {
            let started = Instant::now();
            analyse(&note).map(|_| started.elapsed().as_secs_f64())
        })
        .collect::<Result<_, _>>()?;
    times.sort_by(f64::total_cmp);
    println!("waveform: {:.1} ms per minute of audio (median of 5 on the five-minute note)", times[2] * 1e3 / MINUTES);

    let mut firsts = Vec::new();
    for _ in 0..20 {
        let (speaker, heard) = FakeSpeaker::new(48_000, 1.0);
        let (voice, events) = Voice::start(speaker, FakeMicrophone::unavailable());
        let asked = Instant::now();
        voice.play("m", &note, Duration::from_secs(120), Speed::Normal)?;
        wait(&events, |e| matches!(e, Event::Playback { state: PlaybackState::Playing, .. }))?;
        while heard.first_sound().is_none() {
            std::thread::sleep(Duration::from_micros(100));
        }
        firsts.push((heard.first_sound().unwrap() - asked).as_secs_f64() * 1e3);
    }
    firsts.sort_by(f64::total_cmp);
    println!(
        "play to first sound written to the device: {:.2} ms median, {:.2} ms at most (20 plays from 2:00, fake device)",
        firsts[10], firsts[19]
    );

    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (voice, events) = Voice::start(speaker, FakeMicrophone::unavailable());
    let cpu = Cpu::start();
    voice.play("m", &note, Duration::ZERO, Speed::Normal)?;
    std::thread::sleep(Duration::from_secs(2));
    voice.set_speed(Speed::Double)?;
    std::thread::sleep(Duration::from_secs(2));
    voice.seek(Duration::from_secs(240))?;
    std::thread::sleep(Duration::from_secs(2));
    let (playing, wall) = cpu.stop();
    let (peak, steady) = (PEAK.load(Ordering::Relaxed) - base, LIVE.load(Ordering::Relaxed) - base);
    voice.stop()?;
    drop(events);
    let mb = |b: usize| b as f64 / 1_048_576.0;
    println!(
        "memory while playing the five-minute note: {:.2} MB held, {:.2} MB at most while opening it; the note itself {:.2} MB",
        mb(steady),
        mb(peak),
        mb(file.size())
    );
    println!("CPU while playing (1× then 2×, fake device): {:.2}% of a core over {wall:.0} s", playing * 100.0);
    drop(voice);

    let cpu = Cpu::start();
    let (voice, events) = record(&dir.path().join("live.ogg"), speech[..48_000 * 10].to_vec(), 1.0)?;
    let (recording, wall) = cpu.stop();
    println!(
        "CPU while recording from a 48 kHz microphone (fake, in real time): {:.2}% of a core over {wall:.0} s",
        recording * 100.0
    );
    drop((voice, events));
    Ok(())
}

/// The speech fixture at 48 kHz, said over and over for `seconds`.
fn speech(seconds: f64) -> anyhow::Result<Vec<f32>> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech-16k.ogg");
    let once = VoiceFile::open(&fixture)?.pcm(48_000)?;
    Ok(once.iter().copied().cycle().take((seconds * 48_000.0) as usize).collect())
}

/// Records `samples` through a fake microphone at `pace` to `path`, until all are in.
fn record(path: &PathBuf, samples: Vec<f32>, pace: f64) -> anyhow::Result<(Voice, crossbeam_channel::Receiver<Event>)> {
    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (mic, probe) = FakeMicrophone::new(48_000, samples, pace);
    let (voice, events) = Voice::start(speaker, mic);
    voice.record(path, RECORDING_LIMIT)?;
    while !probe.finished() {
        std::thread::sleep(Duration::from_millis(1));
    }
    voice.stop_recording()?;
    wait(&events, |e| matches!(e, Event::Recorded(_)))?;
    Ok((voice, events))
}

fn wait(events: &crossbeam_channel::Receiver<Event>, wanted: impl Fn(&Event) -> bool) -> anyhow::Result<()> {
    loop {
        match events.recv_timeout(Duration::from_secs(30))? {
            Event::PlaybackFailed { error, .. }
            | Event::RecordingFailed { error }
            | Event::RecordingRefused { error } => {
                bail!(error)
            }
            e if wanted(&e) => return Ok(()),
            _ => {}
        }
    }
}

/// The process's CPU time against the wall clock.
struct Cpu {
    cpu: f64,
    wall: Instant,
}

impl Cpu {
    fn start() -> Cpu {
        Cpu { cpu: cpu_seconds(), wall: Instant::now() }
    }

    fn stop(self) -> (f64, f64) {
        let wall = self.wall.elapsed().as_secs_f64();
        ((cpu_seconds() - self.cpu) / wall, wall)
    }
}

fn cpu_seconds() -> f64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}
