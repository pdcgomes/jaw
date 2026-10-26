//! The loop. Input has a thread of its own (crossterm: keys, mouse, paste, focus), and jawd's
//! updates come from another; both arrive on one channel into the main thread, which sleeps until
//! something comes or the view's next deadline. Each turn handles every input first, applies the
//! updates waiting for at most 8 ms, and draws once if anything changed: at once after input or
//! after what the screen was waiting for (jawd's hello, a window's rows where it was asked to be),
//! and otherwise at most 60 times a second however fast updates come. Nothing changing, nothing is
//! drawn and the thread doesn't wake but for the clock's minute.

use crate::data::Update;
use crate::view::{Effect, View};
use anyhow::Result;
use ratatui::backend::{Backend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event as Input, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier};
use ratatui::{Terminal, crossterm};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// The shortest time between two frames drawn for updates alone.
pub const FRAME: Duration = Duration::from_micros(16_667);
/// How long a turn spends applying updates before it draws.
pub const APPLY: Duration = Duration::from_millis(8);

pub enum Event {
    /// From the terminal, and when it came.
    Input(Input, Instant),
    Update(Update),
    /// jawd has sent something, which the view drains.
    Jawd,
    /// A worker has made a picture ready.
    Picture(Box<crate::graphics::Done>),
    /// A signal to stop, as q would.
    Stop,
}

/// What the loop measured: how long each keypress took to be drawn, and the frames.
#[derive(Debug, Default)]
pub struct Probe {
    pub keys: Vec<Duration>,
    pub frames: u64,
    /// When the first frame with the open chat's messages was drawn.
    pub first: Option<Instant>,
    pub last: Option<Instant>,
    /// How long each chat a key opened took to show its messages, from the key.
    pub opens: Vec<Duration>,
    /// Whether the loop ends once the first frame with messages is drawn.
    pub until_first: bool,
    /// The frames drawn from this moment on.
    pub from: Option<Instant>,
    pub frames_from: u64,
}

/// Runs the view until it quits or the channel closes.
pub fn drive<B: Backend>(
    view: &mut View,
    rx: &Receiver<Event>,
    terminal: &mut Terminal<B>,
    probe: &mut Probe,
    effect: &mut dyn FnMut(Effect),
) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let mut waiting: VecDeque<Update> = VecDeque::new();
    let mut keys: Vec<Instant> = vec![];
    let mut last_frame: Option<Instant> = None;
    // The chat the last key opened, and when, until its messages are drawn.
    let mut opening: Option<(String, Instant)> = None;
    loop {
        let now = Instant::now();
        let deadline = if !waiting.is_empty() {
            Some(now)
        } else {
            let wake = Duration::from_millis((view.next_wake() - view.now()).max(0) as u64);
            let mut at = now + wake;
            if view.dirty {
                at = at.min(last_frame.map_or(now, |t| t + FRAME));
            }
            Some(at)
        };
        let first = match deadline {
            Some(at) if at <= now => rx.try_recv().ok(),
            Some(at) => match rx.recv_timeout(at - now) {
                Ok(e) => Some(e),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            },
            None => rx.recv().ok(),
        };
        let take = |e: Event,
                    view: &mut View,
                    waiting: &mut VecDeque<Update>,
                    keys: &mut Vec<Instant>,
                    opening: &mut Option<(String, Instant)>| match e {
            Event::Input(input, at) => {
                let key = matches!(input, Input::Key(_) | Input::Paste(_));
                let was = view.opened().map(str::to_string);
                view.on_input(input);
                if key {
                    keys.push(at);
                    if let Some(now) = view.opened().filter(|now| was.as_deref() != Some(*now)) {
                        *opening = Some((now.to_string(), at));
                    }
                }
            }
            Event::Update(u) => waiting.push_back(u),
            Event::Jawd => waiting.extend(view.drain_jawd()),
            Event::Picture(done) => view.picture(*done),
            Event::Stop => view.quit = true,
        };
        if let Some(e) = first {
            take(e, view, &mut waiting, &mut keys, &mut opening);
        }
        for _ in 0..10_000 {
            match rx.try_recv() {
                Ok(e) => take(e, view, &mut waiting, &mut keys, &mut opening),
                Err(_) => break,
            }
        }
        let start = Instant::now();
        while keys.is_empty() && !view.quit && start.elapsed() < APPLY {
            let Some(mut batch) = waiting.pop_front() else { break };
            while let Some(next) = waiting.pop_front() {
                if let Some(next) = batch.absorb(next) {
                    waiting.push_front(next);
                    break;
                }
            }
            view.apply(batch);
            // Input first: a key that came meanwhile is handled now and drawn next.
            while let Ok(e) = rx.try_recv() {
                take(e, view, &mut waiting, &mut keys, &mut opening);
            }
        }
        for e in view.take_effects() {
            effect(e);
        }
        if view.quit {
            return Ok(());
        }
        view.tick();
        let due = last_frame.is_none_or(|t| t.elapsed() >= FRAME);
        if view.dirty && (!keys.is_empty() || view.awaited || due) {
            view.dirty = false;
            view.awaited = false;
            let sent = view.take_writes();
            if !sent.is_empty() {
                effect(Effect::Write(sent));
            }
            terminal.draw(|f| view.draw(f))?;
            let drawn = view.take_writes();
            if !drawn.is_empty() {
                effect(Effect::Write(drawn));
            }
            let drawn = Instant::now();
            last_frame = Some(drawn);
            probe.frames += 1;
            probe.frames_from += u64::from(probe.from.is_some_and(|from| drawn >= from));
            probe.last = Some(drawn);
            if probe.first.is_none() && view.showing_messages() {
                probe.first = Some(drawn);
            }
            probe.keys.extend(keys.drain(..).map(|at| drawn - at));
            if let Some((chat, at)) = opening.take() {
                if view.opened() == Some(chat.as_str()) && view.showing_messages() {
                    probe.opens.push(drawn - at);
                } else {
                    opening = Some((chat, at));
                }
            }
            if probe.until_first && probe.first.is_some() {
                return Ok(());
            }
        }
    }
}

/// Sends every terminal event into the channel, from a thread of its own.
fn spawn_input(tx: Sender<Event>) {
    std::thread::Builder::new()
        .name("jaw-input".into())
        .spawn(move || {
            while let Ok(e) = event::read() {
                if tx.send(Event::Input(e, Instant::now())).is_err() {
                    return;
                }
            }
        })
        .expect("the input thread");
}

/// Has a stop, or the terminal going away, end the view the way Ctrl-C does, so the terminal is
/// put back. The signals wait on a thread of their own, blocked everywhere else, so nothing polls.
fn catch_signals(tx: Sender<Event>) {
    // SAFETY: the set is ours and initialised by sigemptyset before use; blocking signals in this
    // thread before others start means every thread inherits the mask, so only sigwait sees them.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGHUP);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        std::thread::Builder::new()
            .name("jaw-signals".into())
            .spawn(move || {
                let mut signal = 0;
                if libc::sigwait(&set, &mut signal) == 0 {
                    let _ = tx.send(Event::Stop);
                }
            })
            .expect("the signals thread");
    }
}

/// Text onto the clipboard through the terminal, as OSC 52.
fn copy(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

/// Opens a link or a file as the system would on a click, without waiting for it.
fn open(target: &str) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(opener)
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Runs the view in the terminal until Ctrl-C, on whatever `start` puts behind it. `start` gets
/// the channel's sender for its updates.
pub fn run(start: impl FnOnce(Sender<Event>) -> View) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    catch_signals(tx.clone());
    let mut view = start(tx.clone());
    let mut terminal = ratatui::init();
    let (protocol, cell) = crate::graphics::detect();
    view.images = crate::graphics::Images::start(protocol, cell, tx.clone());
    let enhanced = matches!(crossterm::terminal::supports_keyboard_enhancement(), Ok(true));
    let mut out = std::io::stdout();
    let _ = crossterm::execute!(out, event::EnableMouseCapture, event::EnableBracketedPaste, event::EnableFocusChange);
    if enhanced {
        let flags = event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;
        let _ = crossterm::execute!(out, event::PushKeyboardEnhancementFlags(flags));
    }
    spawn_input(tx);
    let result = drive(&mut view, &rx, &mut terminal, &mut Probe::default(), &mut |e| match e {
        Effect::Copy(text) => copy(&text),
        Effect::Open(target) => open(&target),
        Effect::Write(bytes) => {
            let mut out = std::io::stdout();
            let _ = out.write_all(&bytes);
            let _ = out.flush();
        }
    });
    if enhanced {
        let _ = crossterm::execute!(out, event::PopKeyboardEnhancementFlags);
    }
    let _ =
        crossterm::execute!(out, event::DisableFocusChange, event::DisableBracketedPaste, event::DisableMouseCapture);
    ratatui::restore();
    result
}

/// Applies every update waiting, as the loop would between keys.
pub fn settle(view: &mut View, rx: &Receiver<Event>) {
    for _ in 0..4 {
        while let Ok(e) = rx.try_recv() {
            match e {
                Event::Update(u) => view.apply(u),
                Event::Jawd => {
                    for u in view.drain_jawd() {
                        view.apply(u);
                    }
                }
                Event::Picture(done) => view.picture(*done),
                _ => {}
            }
        }
        frame(view, 140, 40);
    }
}

/// Applies what a jawd sends until it has been quiet a moment with something on screen, or for at
/// most `most`, drawing between so the windows the screen needs are asked for.
pub fn settle_live(view: &mut View, rx: &Receiver<Event>, most: Duration) {
    let end = Instant::now() + most;
    let quiet = Duration::from_millis(200);
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        match rx.recv_timeout(quiet.min(end - now)) {
            Ok(Event::Update(u)) => view.apply(u),
            Ok(Event::Jawd) => {
                for u in view.drain_jawd() {
                    view.apply(u);
                }
            }
            Ok(Event::Picture(done)) => view.picture(*done),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) if view.showing_messages() || view.answered() => break,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        frame(view, 140, 40);
    }
    frame(view, 140, 40);
}

/// Presses keys against a jawd, waiting for what each brings.
pub fn press_live(view: &mut View, rx: &Receiver<Event>, spec: &str) -> Result<()> {
    for k in keys(spec)? {
        view.on_input(Input::Key(k));
        settle_live(view, rx, Duration::from_secs(2));
    }
    Ok(())
}

fn frame(view: &mut View, width: u16, height: u16) -> Buffer {
    let (w, h) = view.size().unwrap_or((width, height));
    let mut term = Terminal::new(TestBackend::new(w, h)).expect("a test backend");
    term.draw(|f| view.draw(f)).expect("a frame");
    term.backend().buffer().clone()
}

/// Draws a frame at a size, the size later frames between keys keep.
pub fn snapshot(view: &mut View, width: u16, height: u16) -> String {
    view.resize(width, height);
    let buf = frame(view, width, height);
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut line = String::new();
        for x in 0..buf.area.width {
            line.push_str(buf[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// One frame in 24-bit colour, as escape codes, for pictures of it.
pub fn snapshot_ansi(view: &mut View, width: u16, height: u16) -> String {
    view.resize(width, height);
    let buf = frame(view, width, height);
    let code = |c: Color, fg: bool| match c {
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", if fg { 38 } else { 48 }),
        Color::Indexed(i) => format!("{};5;{i}", if fg { 38 } else { 48 }),
        _ => (if fg { "39" } else { "49" }).to_string(),
    };
    let mut out = String::new();
    for y in 0..buf.area.height {
        let mut last: Option<(Color, Color, Modifier)> = None;
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let now = (cell.fg, cell.bg, cell.modifier);
            if last != Some(now) {
                let bold = if cell.modifier.contains(Modifier::BOLD) { ";1" } else { ";22" };
                out.push_str(&format!("\x1b[0;{};{}{bold}m", code(cell.fg, true), code(cell.bg, false)));
                last = Some(now);
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

/// Keys as `--keys` writes them: text types itself, `\n` is Enter and `\t` Tab, and a key's name
/// in angle brackets is that key, with `a-`, `c-` or `s-` before it for Alt, Ctrl or Shift:
/// `<up>`, `<esc>`, `<a-down>`, `<a-enter>`, `<c-u>`, `<pgup>`. `<lt>` is a `<`.
pub fn keys(spec: &str) -> Result<Vec<KeyEvent>> {
    let mut out = vec![];
    let mut rest = spec.replace("\\n", "\n").replace("\\t", "\t");
    while !rest.is_empty() {
        if rest.starts_with('<')
            && let Some(end) = rest.find('>')
        {
            let name = rest[1..end].to_lowercase();
            rest = rest[end + 1..].to_string();
            let mut mods = KeyModifiers::NONE;
            let mut name = name.as_str();
            loop {
                let (m, after) = match name.split_once('-') {
                    Some(("a", after)) => (KeyModifiers::ALT, after),
                    Some(("c", after)) => (KeyModifiers::CONTROL, after),
                    Some(("s", after)) => (KeyModifiers::SHIFT, after),
                    _ => break,
                };
                mods |= m;
                name = after;
            }
            let code = match name {
                "up" => KeyCode::Up,
                "down" => KeyCode::Down,
                "left" => KeyCode::Left,
                "right" => KeyCode::Right,
                "enter" | "cr" => KeyCode::Enter,
                "esc" => KeyCode::Esc,
                "tab" => KeyCode::Tab,
                "bs" | "backspace" => KeyCode::Backspace,
                "del" | "delete" => KeyCode::Delete,
                "home" => KeyCode::Home,
                "end" => KeyCode::End,
                "pgup" => KeyCode::PageUp,
                "pgdn" => KeyCode::PageDown,
                "f1" => KeyCode::F(1),
                "lt" => KeyCode::Char('<'),
                "space" => KeyCode::Char(' '),
                one if one.chars().count() == 1 => KeyCode::Char(one.chars().next().unwrap_or(' ')),
                other => anyhow::bail!("jaw doesn't know the key <{other}>"),
            };
            out.push(KeyEvent::new(code, mods));
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        rest = rest[c.len_utf8()..].to_string();
        out.push(KeyEvent::new(
            match c {
                '\n' => KeyCode::Enter,
                '\t' => KeyCode::Tab,
                c => KeyCode::Char(c),
            },
            KeyModifiers::NONE,
        ));
    }
    Ok(out)
}

/// Presses keys one after another, everything they bring applied and drawn between them.
pub fn press(view: &mut View, rx: &Receiver<Event>, spec: &str) -> Result<()> {
    for k in keys(spec)? {
        view.on_input(Input::Key(k));
        settle(view, rx);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{Clock, Demo, State};

    #[test]
    fn keys_are_text_and_names_in_angle_brackets() {
        let k = keys("hi<a-up><c-u>\\n<lt>").unwrap();
        assert_eq!(k[0], KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(k[2], KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert_eq!(k[3], KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(k[4].code, KeyCode::Enter);
        assert_eq!(k[5].code, KeyCode::Char('<'));
        assert!(keys("<nope>").is_err());
    }

    #[test]
    fn base64_as_the_clipboard_takes_it() {
        assert_eq!(base64(b"ok"), "b2s=");
        assert_eq!(base64("até já".as_bytes()), "YXTDqSBqw6E=");
    }

    /// With nothing arriving, the loop draws its first frame and then nothing, and its next wake
    /// is the clock's minute.
    #[test]
    fn nothing_changing_nothing_is_drawn() {
        let (tx, rx) = std::sync::mpsc::channel();
        let demo = Demo::start(tx.clone(), Clock::Wall, State::Online, false);
        let mut view = View::new(Box::new(demo));
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let mut probe = Probe::default();
        let start = Instant::now();
        let stop = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            let _ = tx.send(Event::Stop);
        });
        drive(&mut view, &rx, &mut terminal, &mut probe, &mut |_| {}).unwrap();
        stop.join().unwrap();
        assert!(probe.first.is_some(), "the conversation was drawn");
        let quiet = start.elapsed() - (probe.last.unwrap() - start);
        let minute = |t: i64| t / crate::fmt::MINUTE;
        let crossed = minute(view.now()) != minute(view.now() - start.elapsed().as_millis() as i64);
        assert!(
            crossed || quiet >= Duration::from_millis(400),
            "no frame in the last {quiet:?} of {:?}",
            start.elapsed()
        );
        assert!(probe.frames <= 6, "a few frames while the account arrives, then none: {}", probe.frames);
        assert!(view.next_wake() - view.now() <= 60_000, "the next wake is the clock's minute");
    }
}
