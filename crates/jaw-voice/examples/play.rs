//! Plays a voice message through the default output, and says how long the first sound took.
//!
//!     cargo run -p jaw-voice --example play -- <file.ogg> [speed: 1, 1.5 or 2] [seconds to start at]

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use jaw_voice::{Error, Event, Output, PlaybackState, Render, Speaker, Speed, SystemMicrophone, SystemSpeaker, Voice};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().context("usage: play <file.ogg> [speed: 1, 1.5 or 2] [seconds to start at]")?);
    let speed = match args.next() {
        Some(s) => Speed::from_factor(s.parse()?).context("the speed is 1, 1.5 or 2")?,
        None => Speed::Normal,
    };
    let at = Duration::from_secs_f64(args.next().map_or(Ok(0.0), |s| s.parse())?);

    let waveform = jaw_voice::analyse(&path)?;
    println!("{}  {}", bars(&waveform.points), clock(waveform.duration));

    let first = Arc::new(Mutex::new(None));
    let (voice, events) = Voice::start(Timed { first: Arc::clone(&first) }, SystemMicrophone);
    let asked = Instant::now();
    voice.play("example", &path, at, speed)?;
    for event in events {
        match event {
            Event::Playback { state: PlaybackState::Playing, at, length, .. } => {
                print!("\r▶ {} / {}  ", clock(at), clock(length));
                std::io::stdout().flush()?;
            }
            Event::Playback { state: PlaybackState::Ended, .. } => break,
            Event::PlaybackFailed { error, .. } => bail!(error),
            _ => {}
        }
    }
    println!();
    if let Some(sound) = *first.lock().unwrap() {
        println!("the first sound reached the output {:.1} ms after play", (sound - asked).as_secs_f64() * 1e3);
    }
    Ok(())
}

/// The system's output, noting when the first sound is written to it.
struct Timed {
    first: Arc<Mutex<Option<Instant>>>,
}

impl Speaker for Timed {
    fn open(&mut self) -> Result<Box<dyn Output>, Error> {
        Ok(Box::new(TimedOutput { inner: SystemSpeaker.open()?, first: Arc::clone(&self.first) }))
    }
}

struct TimedOutput {
    inner: Box<dyn Output>,
    first: Arc<Mutex<Option<Instant>>>,
}

impl Output for TimedOutput {
    fn rate(&self) -> u32 {
        self.inner.rate()
    }

    fn start(&mut self, mut render: Render) -> Result<(), Error> {
        let (first, noted) = (Arc::clone(&self.first), AtomicBool::new(false));
        self.inner.start(Box::new(move |out: &mut [f32]| {
            render(out);
            if !noted.load(Ordering::Relaxed) && out.iter().any(|&x| x != 0.0) {
                noted.store(true, Ordering::Relaxed);
                *first.lock().unwrap() = Some(Instant::now());
            }
        }))
    }

    fn fault(&self) -> Option<String> {
        self.inner.fault()
    }
}

/// Two points to a cell, as jaw draws it.
fn bars(points: &[u8]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    points.chunks(2).map(|p| BARS[(usize::from(p[0].max(p[1])) * 7 / 100).min(7)]).collect()
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}
