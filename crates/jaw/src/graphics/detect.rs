//! What the terminal can draw: `JAW_GRAPHICS` if it says, then what the terminal's environment
//! says, then, where neither does and both ends are a terminal, a question the terminal answers
//! within a moment: kitty's graphics query and the attributes it reports, which say sixel.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// kitty's graphics protocol with Unicode placeholders: kitty and Ghostty.
    Kitty,
    /// iTerm2's inline images: iTerm2 and WezTerm.
    Iterm,
    Sixel,
    /// Half-blocks, two pixels a cell, which every terminal draws.
    Blocks,
    /// No pictures: their summaries, as before.
    None,
}

impl Protocol {
    /// A name as `JAW_GRAPHICS` and `--graphics` take it.
    pub fn parse(name: &str) -> Result<Protocol, String> {
        Protocol::named(name).ok_or_else(|| format!("{name} isn't kitty, iterm, sixel, blocks or none"))
    }

    fn named(name: &str) -> Option<Protocol> {
        Some(match name.trim().to_lowercase().as_str() {
            "kitty" => Protocol::Kitty,
            "iterm" | "iterm2" => Protocol::Iterm,
            "sixel" => Protocol::Sixel,
            "blocks" | "halfblocks" => Protocol::Blocks,
            "none" | "off" => Protocol::None,
            _ => return None,
        })
    }
}

/// What the environment says about the terminal, if it says enough.
pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Protocol> {
    if let Some(p) = var("JAW_GRAPHICS").as_deref().and_then(Protocol::named) {
        return Some(p);
    }
    let term = var("TERM").unwrap_or_default().to_lowercase();
    let program = var("TERM_PROGRAM").unwrap_or_default().to_lowercase();
    if var("TMUX").is_some()
        || term.starts_with("screen")
        || term == "dumb"
        || program == "apple_terminal"
        || program == "vscode"
    {
        return Some(Protocol::Blocks);
    }
    if var("KITTY_WINDOW_ID").is_some() || term.contains("kitty") || program == "ghostty" || term.contains("ghostty") {
        return Some(Protocol::Kitty);
    }
    if program == "iterm.app" || program == "wezterm" {
        return Some(Protocol::Iterm);
    }
    if term.contains("foot") || term.contains("mlterm") || term.contains("contour") {
        return Some(Protocol::Sixel);
    }
    None
}

/// What the terminal can draw, and its cells' size in pixels: asking it if the environment
/// doesn't say. It must run in raw mode, before anything else reads the terminal's input.
pub fn detect() -> (Protocol, (u16, u16)) {
    let cell = cell_size().unwrap_or((8, 16));
    if let Some(p) = from_env(|k| std::env::var(k).ok()) {
        return (p, cell);
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return (Protocol::Blocks, cell);
    }
    (ask().unwrap_or(Protocol::Blocks), cell)
}

/// The terminal's cells' size in pixels, where it says.
fn cell_size() -> Option<(u16, u16)> {
    // SAFETY: TIOCGWINSZ fills the winsize it's given, which is ours and zeroed.
    let ws = unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) != 0 {
            return None;
        }
        ws
    };
    (ws.ws_xpixel > 0 && ws.ws_ypixel > 0 && ws.ws_col > 0 && ws.ws_row > 0)
        .then(|| (ws.ws_xpixel / ws.ws_col, ws.ws_ypixel / ws.ws_row))
}

/// kitty's query for a picture it won't keep, then the attributes every terminal reports, so the
/// answer to the last says when to stop waiting.
fn ask() -> Option<Protocol> {
    let mut out = std::io::stdout();
    out.write_all(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c").ok()?;
    out.flush().ok()?;
    let answer = read_until_attributes(Duration::from_millis(150));
    Some(read_answer(&answer))
}

/// What a terminal's answers to the two questions say.
pub fn read_answer(answer: &[u8]) -> Protocol {
    let text = String::from_utf8_lossy(answer);
    if text.contains("\x1b_Gi=31;OK") {
        return Protocol::Kitty;
    }
    let attributes = text.split("\x1b[?").nth(1).and_then(|a| a.split('c').next()).unwrap_or("");
    if attributes.split(';').any(|a| a == "4") { Protocol::Sixel } else { Protocol::Blocks }
}

fn read_until_attributes(wait: Duration) -> Vec<u8> {
    let end = Instant::now() + wait;
    let mut got = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return got;
        }
        let mut fd = libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd of ours, for at most `left`.
        let ready = unsafe { libc::poll(&mut fd, 1, left.as_millis().max(1) as libc::c_int) };
        if ready <= 0 {
            return got;
        }
        let mut buf = [0u8; 256];
        // SAFETY: reads at most the buffer's length into it.
        let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            return got;
        }
        got.extend_from_slice(&buf[..n as usize]);
        let text = String::from_utf8_lossy(&got);
        if let Some(at) = text.find("\x1b[?")
            && text[at..].contains('c')
        {
            return got;
        }
    }
}
