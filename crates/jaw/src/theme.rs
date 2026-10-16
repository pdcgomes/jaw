//! The look: ooze's, in Tokyo Night, falling back to the nearest of the 256 standard colours where
//! the terminal has no 24-bit colour. A theme is a palette of a few hex values; drawing asks
//! `Theme` for what a colour is for, never for a hue, so another theme is another palette.

use ratatui::style::Color;

/// A theme's colours as hex values: the ground it draws on, its greys, and eight accents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub surface: u32,
    pub selection: u32,
    pub fg: u32,
    pub text: u32,
    pub muted: u32,
    pub comment: u32,
    pub gutter: u32,
    pub blue: u32,
    pub cyan: u32,
    pub teal: u32,
    pub purple: u32,
    pub green: u32,
    pub yellow: u32,
    pub orange: u32,
    pub red: u32,
}

/// Omarchy's default, and ooze's and hauntop's.
pub const TOKYO_NIGHT: Palette = Palette {
    bg: 0x1a1b26,
    surface: 0x24283b,
    selection: 0x283457,
    fg: 0xc0caf5,
    text: 0xa9b1d6,
    muted: 0x737aa2,
    comment: 0x565f89,
    gutter: 0x3b4261,
    blue: 0x7aa2f7,
    cyan: 0x7dcfff,
    teal: 0x73daca,
    purple: 0xbb9af7,
    green: 0x9ece6a,
    yellow: 0xe0af68,
    orange: 0xff9e64,
    red: 0xf7768e,
};

/// Each colour by what it's for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The ground, a raised surface (overlays, monospace), and the selected row.
    pub bg: Color,
    pub surface: Color,
    pub selection: Color,
    /// Text that matters, ordinary text, secondary text, quieter still, and borders at rest.
    pub fg: Color,
    pub text: Color,
    pub muted: Color,
    pub comment: Color,
    pub gutter: Color,
    /// The name's pill in the header.
    pub brand: Color,
    /// The border of what has the keys, and the keys in the footer.
    pub focus: Color,
    pub link: Color,
    /// Where a search matched what it found.
    pub mark: Color,
    /// An unread count; a mention of you, and its count.
    pub unread: Color,
    pub mention: Color,
    /// Messages that came below while you read further up.
    pub arrived: Color,
    /// Someone typing, or online.
    pub online: Color,
    /// Ticks before they're read, and once they are.
    pub unseen: Color,
    pub read: Color,
    /// Something that needs you: a header in trouble, a message not sent.
    pub warn: Color,
    /// A passing notice in the footer.
    pub notice: Color,
    /// How far catching up has got.
    pub progress: Color,
    /// Group senders' names, by the index jawd gives each member.
    senders: [Color; 8],
    /// Whether colours are 24-bit, for the creature's own.
    pub truecolor: bool,
}

impl Theme {
    pub fn new(p: &Palette, truecolor: bool) -> Theme {
        let c = |h: u32| color(h, truecolor);
        Theme {
            bg: c(p.bg),
            surface: c(p.surface),
            selection: c(p.selection),
            fg: c(p.fg),
            text: c(p.text),
            muted: c(p.muted),
            comment: c(p.comment),
            gutter: c(p.gutter),
            brand: c(p.teal),
            focus: c(p.blue),
            link: c(p.cyan),
            mark: c(p.fg),
            unread: c(p.fg),
            mention: c(p.orange),
            arrived: c(p.orange),
            online: c(p.green),
            unseen: c(p.comment),
            read: c(p.cyan),
            warn: c(p.orange),
            notice: c(p.yellow),
            progress: c(p.blue),
            senders: [p.red, p.yellow, p.blue, p.purple, p.cyan, p.green, p.orange, p.teal].map(c),
            truecolor,
        }
    }

    pub fn with(truecolor: bool) -> Theme {
        Theme::new(&TOKYO_NIGHT, truecolor)
    }

    pub fn sender(&self, colour: u8) -> Color {
        self.senders[colour as usize % self.senders.len()]
    }
}

pub fn color(h: u32, truecolor: bool) -> Color {
    let (r, g, b) = ((h >> 16) as u8, (h >> 8) as u8, h as u8);
    if truecolor { Color::Rgb(r, g, b) } else { Color::Indexed(nearest_256(r, g, b)) }
}

/// Whether the terminal takes 24-bit colour; otherwise the theme falls back to the nearest of the
/// 256 standard colours.
pub fn truecolor() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default().to_lowercase();
    let ct = var("COLORTERM");
    if ct.contains("truecolor") || ct.contains("24bit") {
        return true;
    }
    let program = var("TERM_PROGRAM");
    if ["iterm.app", "ghostty", "wezterm", "vscode", "hyper", "tabby", "rio", "warpterminal"]
        .contains(&program.as_str())
    {
        return true;
    }
    let term = var("TERM");
    if ["kitty", "alacritty", "ghostty", "wezterm", "foot"].iter().any(|t| term.contains(t)) {
        return true;
    }
    // Terminal gained 24-bit colour in macOS 26.
    program == "apple_terminal" && macos_major() >= 26
}

#[cfg(target_os = "macos")]
fn macos_major() -> u32 {
    let name = c"kern.osproductversion";
    let mut buf = [0u8; 32];
    let mut len = buf.len();
    // SAFETY: the buffer and its length are ours, and sysctlbyname writes at most `len` bytes.
    let rc = unsafe {
        libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, &mut len, std::ptr::null_mut(), 0)
    };
    if rc != 0 {
        return 0;
    }
    let s = String::from_utf8_lossy(&buf[..len.saturating_sub(1)]).into_owned();
    s.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
fn macos_major() -> u32 {
    0
}

/// The closest colour in xterm's 6×6×6 cube or its grey ramp.
pub fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let level = |v: u8| LEVELS.iter().enumerate().min_by_key(|(_, l)| (**l as i32 - v as i32).abs()).unwrap().0;
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = 16 + 36 * ri + 6 * gi + bi;
    let cube_rgb = (LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let grey_i = ((avg as i32 - 8).max(0) / 10).min(23) as u8;
    let grey = 8 + 10 * grey_i;
    let dist = |(a, b2, c): (u8, u8, u8)| {
        let d = |x: u8, y: u8| (x as i32 - y as i32).pow(2);
        d(a, r) + d(b2, g) + d(c, b)
    };
    if dist((grey, grey, grey)) < dist(cube_rgb) { 232 + grey_i } else { cube as u8 }
}

pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_fall_back_to_their_nearest_of_the_256() {
        assert_eq!(nearest_256(0, 0, 0), 16);
        assert_eq!(nearest_256(255, 255, 255), 231);
        assert_eq!(nearest_256(0x1a, 0x1b, 0x26), 234, "Tokyo Night's background is a near-black grey");
        assert_eq!(nearest_256(0x7a, 0xa2, 0xf7), 111, "and its blue a soft blue");
    }
}
