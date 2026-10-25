//! The flood: the made-up account sending messages as fast as asked, half into the open chat,
//! while keys are pressed as someone would, through the same loop the terminal runs, drawn into a
//! terminal in memory. It measures how long each keypress takes to be drawn, how long each chat a
//! key opens takes to show its messages, how soon the first frame with a conversation comes after
//! the process started, how much memory the process has taken, and, with nothing pressed, how
//! often it woke and drew. It can run on a jawd of its own (`jawd bench feed`) or one already
//! running, such as `jawd demo`.

use crate::demo::{Clock, Demo, State};
use crate::graphics::{Images, Protocol};
use crate::run::{self, Event, Probe};
use crate::view::{Effect, View};
use anyhow::Result;
use jaw_proto::{Connection, ConnectionState};
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
    /// What the keys do.
    pub keys: Keys,
    /// Whether the flood comes from a real jawd, `jawd bench feed`'s made-up account on a folder
    /// of its own, rather than from jaw's.
    pub jawd: bool,
    /// A jawd already running to draw, rather than `jawd bench feed`, which floods it or not.
    pub socket: Option<std::path::PathBuf>,
    /// Whether it ends at the first frame with a conversation, which times jaw's start.
    pub until_first: bool,
    /// What the terminal draws pictures with; the cells are 8 by 16 pixels.
    pub pictures: Protocol,
    /// Whether a voice message plays all along, its playhead pushed ten times a second.
    pub voice: bool,
    /// Whether the library's grid of photos across all chats is open, the flood's photos
    /// coming into it.
    pub library: bool,
}

/// What the keys do while the flood goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Keys {
    /// Type, edit, step into the messages and back, send, and move between chats.
    Typing,
    /// Find things in the palette, which searches at each key.
    Palette,
    /// Move down the chat list and back up, opening each chat.
    Walk,
    /// Nothing at all, counting how often jaw wakes and draws.
    None,
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
    /// How long each chat a key opened took to show its messages, shortest first.
    pub opens: Vec<Duration>,
    pub frames: u64,
    /// Messages the flood sent, when the bench knows them.
    pub messages: Option<u64>,
    pub seconds: f64,
    /// From starting to the first frame with the open chat's messages.
    pub first_frame: Duration,
    /// The same from when the process started, as the kernel says.
    pub launched: Option<Duration>,
    /// The process's largest resident size, in bytes.
    pub peak_bytes: u64,
    /// What was written to the terminal beside the frames: pictures sent and drawn.
    pub written: u64,
    /// Where the voice message playing had got by the end, as its pushes said.
    pub voice_at: Option<f64>,
    /// After the warm-up: how long it lasted, the frames drawn, and how often the process woke.
    pub measured: f64,
    pub frames_measured: u64,
    pub wakeups: u64,
}

/// The `q`th quantile of `sorted`.
fn at(sorted: &[Duration], q: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

impl Report {
    pub fn at(&self, q: f64) -> Duration {
        at(&self.keys, q)
    }

    pub fn describe(&self) -> String {
        let ms = |d: Duration| format!("{:.2} ms", d.as_secs_f64() * 1e3);
        let messages = match self.messages {
            Some(n) => format!("{n} in {:.1} s, {:.0} a second", self.seconds, n as f64 / self.seconds),
            None => format!("as many as jawd sent in {:.1} s", self.seconds),
        };
        let launched = self.launched.map_or(String::new(), |d| format!(", {} after the process started", ms(d)));
        let mut lines = vec![
            format!(
                "keys       {} drawn: p50 {}, p99 {}, slowest {}",
                self.keys.len(),
                ms(self.at(0.5)),
                ms(self.at(0.99)),
                ms(self.keys.last().copied().unwrap_or_default())
            ),
            format!("messages   {messages}"),
            format!("frames     {}, {:.0} a second", self.frames, self.frames as f64 / self.seconds),
            format!("first      the first frame with a conversation in {}{launched}", ms(self.first_frame)),
            format!("memory     {:.1} MB at most", self.peak_bytes as f64 / 1e6),
            format!("pictures   {:.1} MB written to the terminal", self.written as f64 / 1e6),
        ];
        if !self.opens.is_empty() {
            lines.insert(
                1,
                format!(
                    "opens      {} chats opened by a key and drawn: p50 {}, p99 {}, slowest {}",
                    self.opens.len(),
                    ms(at(&self.opens, 0.5)),
                    ms(at(&self.opens, 0.99)),
                    ms(self.opens.last().copied().unwrap_or_default())
                ),
            );
        }
        if self.measured > 0.0 && self.keys.is_empty() {
            lines.push(format!(
                "quiet      {} frames and {} wake-ups in {:.0} s with nothing pressed",
                self.frames_measured, self.wakeups, self.measured
            ));
        }
        if let Some(at) = self.voice_at {
            lines.push(format!("voice      playing, at {}", crate::fmt::length(at)));
        }
        lines.join("\n")
    }

    /// The report as `make perf` reads it, times in milliseconds.
    pub fn json(&self) -> serde_json::Value {
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let summary = |sorted: &[Duration]| {
            serde_json::json!({
                "n": sorted.len(),
                "p50": ms(at(sorted, 0.5)),
                "p99": ms(at(sorted, 0.99)),
                "max": ms(sorted.last().copied().unwrap_or_default()),
            })
        };
        serde_json::json!({
            "keys": summary(&self.keys),
            "opens": summary(&self.opens),
            "frames": self.frames,
            "seconds": self.seconds,
            "messages": self.messages,
            "first_frame_ms": ms(self.first_frame),
            "launched_ms": self.launched.map(ms),
            "peak_mb": self.peak_bytes as f64 / 1e6,
            "measured_seconds": self.measured,
            "frames_measured": self.frames_measured,
            "wakeups": self.wakeups,
            "wakeups_per_minute": if self.measured > 0.0 { self.wakeups as f64 * 60.0 / self.measured } else { 0.0 },
        })
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

/// Down the chat list and back up again, opening each chat on the way.
fn walk_script() -> Vec<KeyEvent> {
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
    let up = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
    std::iter::repeat_n(down, 30).chain(std::iter::repeat_n(up, 30)).collect()
}

pub fn flood(f: &Flood) -> Result<Report> {
    let (start, age) = (Instant::now(), jaw_client::usage::age());
    let (tx, rx) = std::sync::mpsc::channel();
    let length = f.warm.as_secs_f64() + f.seconds;
    let (mut view, flooding, feeding) = if let (true, Some(socket)) = (f.jawd, &f.socket) {
        (View::new(Box::new(crate::wire::Wire::connect(socket.clone(), tx.clone()))), None, None)
    } else if f.jawd {
        let (feeding, socket) = Feeding::start(f.rate, length)?;
        (View::new(Box::new(crate::wire::Wire::connect(socket, tx.clone()))), None, Some(feeding))
    } else {
        let demo = Demo::start(tx.clone(), Clock::Wall, State::Online, false);
        demo.answer_on_a_thread();
        if f.voice {
            demo.keep_voice_time();
            demo.play_newest_voice("ana");
        }
        let flooding = demo.flood(f.rate, length);
        (View::new(Box::new(demo)), Some(flooding), None)
    };
    view.images = Images::start(f.pictures, (8, 16), tx.clone());
    view.no_color = false;
    if f.library {
        view.open_library(None, Some(crate::data::LibraryKind::Photo), "");
    }
    view.resize(f.width, f.height);
    if feeding.is_some() {
        // `jawd bench feed` has no engine, so no account: once it has said hello, the screen is
        // taken past linking as an account's is once its connection is up.
        while !view.answered() {
            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Event::Jawd) => {
                    for u in view.drain_jawd() {
                        view.apply(u);
                    }
                }
                Ok(Event::Update(u)) => view.apply(u),
                Ok(_) => {}
                Err(_) => anyhow::bail!("jawd bench feed didn't say hello"),
            }
        }
        view.apply(crate::data::Update::Connection(Connection::new(ConnectionState::Online)));
    }
    let mut terminal = Terminal::new(TestBackend::new(f.width, f.height))?;
    let mut probe = Probe { until_first: f.until_first, from: Some(start + f.warm), ..Probe::default() };
    let (every, seconds, warm, keys) = (f.every, f.seconds, f.warm, f.keys);
    let until_first = f.until_first;
    let presser = std::thread::spawn(move || {
        let script = match keys {
            Keys::Typing => script(),
            Keys::Palette => palette_script(),
            Keys::Walk => walk_script(),
            Keys::None => Vec::new(),
        };
        if until_first {
            return 0;
        }
        std::thread::sleep(warm);
        let quiet = jaw_client::usage::now().wakeups;
        if keys == Keys::Palette {
            let open = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
            if tx.send(Event::Input(Input::Key(open), Instant::now())).is_err() {
                return 0;
            }
        }
        let begun = Instant::now();
        let end = begun + Duration::from_secs_f64(seconds);
        let mut i = 0;
        while !script.is_empty() {
            let next = begun + every * (i as u32 + 1);
            if next >= end {
                break;
            }
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            if tx.send(Event::Input(Input::Key(script[i % script.len()]), Instant::now())).is_err() {
                return 0;
            }
            i += 1;
        }
        std::thread::sleep(end.saturating_duration_since(Instant::now()));
        let woke = jaw_client::usage::now().wakeups - quiet;
        let _ = tx.send(Event::Stop);
        woke
    });
    let mut written = 0u64;
    run::drive(&mut view, &rx, &mut terminal, &mut probe, &mut |e| {
        if let Effect::Write(bytes) = e {
            written += bytes.len() as u64;
        }
    })?;
    let messages = match (flooding, &feeding) {
        (Some(flooding), _) => Some(flooding.join().unwrap_or(0)),
        (None, Some(_)) => Some((f.rate as f64 * length) as u64),
        (None, None) => None,
    };
    drop(feeding);
    let wakeups = presser.join().unwrap_or_default();
    let (mut keys, mut opens) = (probe.keys, probe.opens);
    keys.sort();
    opens.sort();
    let first_frame = probe.first.map_or(Duration::ZERO, |t| t - start);
    Ok(Report {
        keys,
        opens,
        frames: probe.frames,
        messages,
        seconds: start.elapsed().as_secs_f64(),
        first_frame,
        launched: age.filter(|_| probe.first.is_some()).map(|age| age + first_frame),
        measured: if until_first { 0.0 } else { seconds },
        frames_measured: probe.frames_from,
        wakeups,
        peak_bytes: peak_bytes(),
        written,
        voice_at: view.playing.as_ref().map(|p| p.at),
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
            keys: Keys::Typing,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: false,
            library: false,
        })
        .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
        assert!(report.messages.unwrap_or_default() as f64 >= 2_000.0 * 4.5, "{}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn with_kittys_pictures_a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report = flood(&Flood {
            width: 140,
            height: 40,
            rate: 2_000,
            seconds: 4.0,
            every: Duration::from_millis(12),
            warm: Duration::from_secs(1),
            keys: Keys::Typing,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Kitty,
            voice: false,
            library: false,
        })
        .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.written > 0, "the pictures were sent: {}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn with_a_voice_message_playing_a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report = flood(&Flood {
            width: 140,
            height: 40,
            rate: 2_000,
            seconds: 4.0,
            every: Duration::from_millis(12),
            warm: Duration::from_secs(1),
            keys: Keys::Typing,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: true,
            library: false,
        })
        .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.voice_at.is_some_and(|at| at >= 3.0), "it played all along: {}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
        assert!(report.at(0.99) <= Duration::from_millis(16), "{}", report.describe());
    }

    #[test]
    fn with_the_librarys_photos_showing_a_keypress_is_drawn_within_16_ms_at_p99_while_2000_messages_a_second_arrive() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report = flood(&Flood {
            width: 140,
            height: 40,
            rate: 2_000,
            seconds: 4.0,
            every: Duration::from_millis(12),
            warm: Duration::from_secs(1),
            keys: Keys::Typing,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: false,
            library: true,
        })
        .unwrap();
        eprintln!("{}", report.describe());
        assert!(report.keys.len() >= 200, "{}", report.describe());
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
            keys: Keys::Palette,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: false,
            library: false,
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
            keys: Keys::None,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: false,
            library: false,
        })
        .unwrap();
        assert!(report.keys.is_empty());
        assert!(report.frames as f64 / report.seconds <= 61.0, "{}", report.describe());
        assert!(report.frames as f64 / report.seconds >= 20.0, "it keeps drawing what arrives: {}", report.describe());
    }

    #[test]
    fn walking_down_the_chat_list_times_each_chat_from_its_key_to_its_messages_drawn() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let report = flood(&Flood {
            width: 140,
            height: 40,
            rate: 0,
            seconds: 2.0,
            every: Duration::from_millis(100),
            warm: Duration::from_millis(300),
            keys: Keys::Walk,
            jawd: false,
            socket: None,
            until_first: false,
            pictures: Protocol::Blocks,
            voice: false,
            library: false,
        })
        .unwrap();
        assert!(report.opens.len() >= 5, "{}", report.describe());
        assert!(report.opens.iter().all(|open| *open < Duration::from_secs(1)), "{}", report.describe());
        assert!(report.launched.is_some_and(|launched| launched >= report.first_frame), "{}", report.describe());
    }
}
