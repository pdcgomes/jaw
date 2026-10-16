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
    /// How long the flood runs before the keys begin, so they're measured once it's steady.
    pub warm: Duration,
    /// Whether the keys are pressed in the palette, which searches at each one.
    pub palette: bool,
    /// Whether the flood comes from a real jawd, `jawd bench feed`'s made-up account on a folder
    /// of its own, rather than from jaw's.
    pub jawd: bool,
}

/// `jawd bench feed` writing its flood, for as long as the bench needs it.
struct Feeding {
    child: std::process::Child,
}

impl Feeding {
    /// Starts it and waits for its socket, which comes once its made-up history is written.
    fn start(rate: u32, seconds: f64) -> Result<(Feeding, std::path::PathBuf)> {
        let child = std::process::Command::new(crate::wire::jawd_binary())
            .args(["bench", "feed", "--rate", &rate.to_string(), "--seconds", &(seconds.ceil() as u32 + 2).to_string()])
            .args(["--chats", "40"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("can't start jawd bench feed: {e}; cargo build -p jawd builds it"))?;
        let socket = std::env::temp_dir().join(format!("jaw-bench-feed-{}", child.id())).join("jaw.sock");
        let feeding = Feeding { child };
        let end = Instant::now() + Duration::from_secs(120);
        while std::os::unix::net::UnixStream::connect(&socket).is_err() {
            anyhow::ensure!(Instant::now() < end, "jawd bench feed didn't open {}", socket.display());
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok((feeding, socket))
    }
}

impl Drop for Feeding {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

/// What someone does in the palette: types words to find, edits them, moves through what's found
/// and narrows it, never closing it.
fn palette_script() -> Vec<KeyEvent> {
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    let mut keys = vec![];
    for word in ["jantar", "vinho", "ana", "ar"] {
        keys.extend(word.chars().map(|c| key(KeyCode::Char(c))));
        keys.extend([
            key(KeyCode::Down),
            key(KeyCode::Down),
            key(KeyCode::Up),
            key(KeyCode::Tab),
            key(KeyCode::BackTab),
        ]);
        keys.push(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    }
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
    let (tx, rx) = std::sync::mpsc::channel();
    let length = f.warm.as_secs_f64() + f.seconds;
    let (mut view, flooding, feeding) = if f.jawd {
        let (feeding, socket) = Feeding::start(f.rate, length)?;
        (View::new(Box::new(crate::wire::Wire::connect(socket, tx.clone()))), None, Some(feeding))
    } else {
        let demo = Demo::start(tx.clone(), Clock::Wall, State::Online, false);
        demo.answer_on_a_thread();
        let flooding = demo.flood(f.rate, length);
        (View::new(Box::new(demo)), Some(flooding), None)
    };
    let start = Instant::now();
    view.resize(f.width, f.height);
    let mut terminal = Terminal::new(TestBackend::new(f.width, f.height))?;
    let mut probe = Probe::default();
    let (every, seconds, warm, palette) = (f.every, f.seconds, f.warm, f.palette);
    let presser = std::thread::spawn(move || {
        let keys = if palette { palette_script() } else { script() };
        std::thread::sleep(warm);
        if palette {
            let open = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
            if tx.send(Event::Input(Input::Key(open), Instant::now())).is_err() {
                return;
            }
        }
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
    let messages = match flooding {
        Some(flooding) => flooding.join().unwrap_or(0),
        None => (f.rate as f64 * length) as u64,
    };
    drop(feeding);
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
        let report = flood(&Flood {
            width: 140,
            height: 40,
            rate: 2_000,
            seconds: 4.0,
            every: Duration::from_millis(12),
            warm: Duration::from_secs(1),
            palette: false,
            jawd: false,
        })
        .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
        assert!(report.messages as f64 >= 2_000.0 * 4.5, "{}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn with_the_palette_open_a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let flood = Flood {
            width: 140,
            height: 40,
            rate: 2_000,
            seconds: 4.0,
            every: Duration::from_millis(12),
            warm: Duration::from_secs(1),
            palette: true,
            jawd: false,
        };
        let report = super::flood(&flood).unwrap();
        eprintln!("{}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn a_flood_alone_is_drawn_at_most_60_times_a_second() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report = flood(&Flood {
            width: 100,
            height: 30,
            rate: 2_000,
            seconds: 2.0,
            every: Duration::from_secs(60),
            warm: Duration::ZERO,
            palette: false,
            jawd: false,
        })
        .unwrap();
        assert!(report.keys.is_empty());
        assert!(report.frames as f64 / report.seconds <= 61.0, "{}", report.describe());
        assert!(report.frames as f64 / report.seconds >= 20.0, "it keeps drawing what arrives: {}", report.describe());
    }
}
