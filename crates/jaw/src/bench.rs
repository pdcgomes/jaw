//! The flood: the made-up account sending messages as fast as asked, half into the open chat,
//! while keys are pressed as someone would, through the same loop the terminal runs, drawn into a
//! terminal in memory. It measures how long each keypress takes to be drawn, how soon the first
//! frame with a conversation comes, and how much memory the process has taken.

use crate::demo::{Clock, Demo, State};
use crate::run::{self, Event, Probe};
use crate::view::View;
use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event as Input, KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

pub struct Flood {
    pub width: u16,
    pub height: u16,
    /// Messages a second.
    pub rate: u32,
    pub seconds: f64,
    /// A key this often.
    pub every: Duration,
}

pub struct Report {
    /// How long each key took to be drawn, shortest first.
    pub keys: Vec<Duration>,
    pub frames: u64,
    pub messages: u64,
    pub seconds: f64,
    /// From starting to the first frame with the open chat's messages.
    pub first_frame: Duration,
    /// The process's largest resident size, in bytes.
    pub peak_bytes: u64,
}

impl Report {
    pub fn at(&self, q: f64) -> Duration {
        if self.keys.is_empty() {
            return Duration::ZERO;
        }
        self.keys[((self.keys.len() - 1) as f64 * q).round() as usize]
    }

    pub fn describe(&self) -> String {
        let ms = |d: Duration| format!("{:.2} ms", d.as_secs_f64() * 1e3);
        format!(
            "keys       {} drawn: p50 {}, p99 {}, slowest {}\nmessages   {} in {:.1} s, {:.0} a second\nframes     {}, {:.0} a second\nfirst      the first frame with a conversation in {}\nmemory     {:.1} MB at most",
            self.keys.len(),
            ms(self.at(0.5)),
            ms(self.at(0.99)),
            ms(self.keys.last().copied().unwrap_or_default()),
            self.messages,
            self.seconds,
            self.messages as f64 / self.seconds,
            self.frames,
            self.frames as f64 / self.seconds,
            ms(self.first_frame),
            self.peak_bytes as f64 / 1e6,
        )
    }
}

/// What someone does while the messages pour in: types, edits, steps into the messages and
/// back, sends, and moves between chats.
fn script() -> Vec<KeyEvent> {
    let mut keys = vec![];
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    for c in "vens jantar logo?".chars() {
        keys.push(key(KeyCode::Char(c)));
    }
    keys.extend([
        key(KeyCode::Backspace),
        key(KeyCode::Left),
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
    ]);
    keys.push(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    keys.extend([key(KeyCode::Up), key(KeyCode::Up), key(KeyCode::Up), key(KeyCode::Down), key(KeyCode::Esc)]);
    for c in "ok".chars() {
        keys.push(key(KeyCode::Char(c)));
    }
    keys.push(key(KeyCode::Enter));
    keys.push(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    keys.extend([key(KeyCode::PageUp), key(KeyCode::PageDown)]);
    keys.push(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
    keys
}

pub fn peak_bytes() -> u64 {
    // SAFETY: getrusage fills the struct it's given, which is ours and zeroed.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let max = usage.ru_maxrss.max(0) as u64;
    if cfg!(target_os = "macos") { max } else { max * 1024 }
}

pub fn flood(f: &Flood) -> Result<Report> {
    let start = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let demo = Demo::start(tx.clone(), Clock::Wall, State::Online, false);
    let mut view = View::new(Box::new(demo.clone()));
    view.resize(f.width, f.height);
    let mut terminal = Terminal::new(TestBackend::new(f.width, f.height))?;
    let mut probe = Probe::default();
    let flooding = demo.flood(f.rate, f.seconds);
    let (every, seconds) = (f.every, f.seconds);
    let presser = std::thread::spawn(move || {
        let keys = script();
        let begun = Instant::now();
        let end = begun + Duration::from_secs_f64(seconds);
        let mut i = 0;
        loop {
            let next = begun + every * (i as u32 + 1);
            if next >= end {
                break;
            }
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            if tx.send(Event::Input(Input::Key(keys[i % keys.len()]), Instant::now())).is_err() {
                return;
            }
            i += 1;
        }
        std::thread::sleep(end.saturating_duration_since(Instant::now()));
        let _ = tx.send(Event::Stop);
    });
    run::drive(&mut view, &rx, &mut terminal, &mut probe, &mut |_| {})?;
    let messages = flooding.join().unwrap_or(0);
    let _ = presser.join();
    let mut keys = probe.keys;
    keys.sort();
    Ok(Report {
        keys,
        frames: probe.frames,
        messages,
        seconds: start.elapsed().as_secs_f64(),
        first_frame: probe.first.map_or(Duration::ZERO, |t| t - start),
        peak_bytes: peak_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The floods take turns, so neither measures the other.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// The design's budget: a keypress drawn within 16 ms at p99 while 2,000 messages a second
    /// arrive.
    #[test]
    fn a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report =
            flood(&Flood { width: 140, height: 40, rate: 2_000, seconds: 4.0, every: Duration::from_millis(12) })
                .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
        assert!(report.messages as f64 >= 2_000.0 * 3.5, "{}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn a_flood_alone_is_drawn_at_most_60_times_a_second() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report =
            flood(&Flood { width: 100, height: 30, rate: 2_000, seconds: 2.0, every: Duration::from_secs(60) })
                .unwrap();
        assert!(report.keys.is_empty());
        assert!(report.frames as f64 / report.seconds <= 61.0, "{}", report.describe());
        assert!(report.frames as f64 / report.seconds >= 20.0, "it keeps drawing what arrives: {}", report.describe());
    }
}
