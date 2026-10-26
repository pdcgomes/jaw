//! The look: ooze's, in one of Omarchy's themes, falling back to the nearest of the 256 standard
//! colours where the terminal has no 24-bit colour. A theme is Omarchy's palette mapped onto what
//! each colour is for; drawing asks `Theme` for a role, never for a hue, and each role is kept
//! readable against the ground whatever the palette.

use jaw_proto::themes::{Colors, Mode, Rgb};
use ratatui::style::Color;

/// Each role's colour as hex, from a theme's palette, before the terminal's colours are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roles {
    pub bg: u32,
    pub surface: u32,
    pub selection: u32,
    pub fg: u32,
    pub text: u32,
    pub muted: u32,
    pub comment: u32,
    pub gutter: u32,
    pub accent: u32,
    pub link: u32,
    pub mention: u32,
    pub online: u32,
    pub notice: u32,
    pub senders: [u32; 8],
}

impl Roles {
    /// Omarchy's keys mapped onto jaw's roles. A grey outside its band of contrast with the ground
    /// is moved toward the ground or the text until it's in, keeping its hue; the selection is
    /// then held apart from the surface too, which overlays draw it on; and every colour reads at
    /// least 3:1.
    pub fn of(c: &Colors) -> Roles {
        let bg = c.background.0;
        let (fg, text) = if contrast(c.bright_foreground.0, bg) >= contrast(c.foreground.0, bg) {
            (c.bright_foreground.0, c.foreground.0)
        } else {
            (c.foreground.0, c.bright_foreground.0)
        };
        let held = |colour: u32, low: f64, high: f64, target: f64| fit(colour, bg, fg, low, high, target);
        let comment = held(c.dark_foreground.0, 2.5, 4.5, 3.0);
        let hue = |colour: Rgb| held(colour.0, 3.0, f64::INFINITY, 3.0);
        let surface = held(c.lighter_background.0, 1.05, f64::INFINITY, 1.1);
        let selection = held(c.selection.0, 1.15, f64::INFINITY, 1.25);
        Roles {
            bg,
            surface,
            selection: fit(selection, surface, fg, 1.15, f64::INFINITY, 1.15),
            fg,
            text,
            muted: mix(comment, text, 0.5),
            comment,
            gutter: held(c.muted.0, 1.4, 2.6, 1.8),
            accent: c.accent.0,
            link: hue(c.cyan),
            mention: hue(c.orange),
            online: hue(c.green),
            notice: hue(c.yellow),
            senders: [c.red, c.yellow, c.blue, c.magenta, c.cyan, c.green, c.orange, c.bright_cyan].map(hue),
        }
    }
}

/// WCAG's relative luminance of a colour.
fn luminance(c: u32) -> f64 {
    let channel = |shift: u32| {
        let v = f64::from((c >> shift) & 0xff) / 255.0;
        if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// WCAG's contrast between two colours, from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `a` moved `t` of the way to `b`, channel by channel.
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (f64::from((a >> shift) & 0xff), f64::from((b >> shift) & 0xff));
        ((x + (y - x) * t).round() as u32).min(255) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// `colour` as it is when its contrast with `ground` is from `low` to `high`, and otherwise moved
/// toward `fg` or `ground` until its contrast is `target`.
fn fit(colour: u32, ground: u32, fg: u32, low: f64, high: f64, target: f64) -> u32 {
    let now = contrast(colour, ground);
    if (low..=high).contains(&now) {
        return colour;
    }
    let end = if now < low { fg } else { ground };
    let (mut near, mut far) = (0.0, 1.0);
    for _ in 0..30 {
        let t = (near + far) / 2.0;
        let reached = contrast(mix(colour, end, t), ground);
        if (now < low && reached < target) || (now > high && reached > target) {
            near = t;
        } else {
            far = t;
        }
    }
    mix(colour, end, far)
}

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
    /// Whether the theme is dark text on a light ground.
    pub light: bool,
}

impl Theme {
    fn new(r: &Roles, mode: Mode, truecolor: bool) -> Theme {
        let c = |h: u32| color(h, truecolor);
        Theme {
            bg: c(r.bg),
            surface: c(r.surface),
            selection: c(r.selection),
            fg: c(r.fg),
            text: c(r.text),
            muted: c(r.muted),
            comment: c(r.comment),
            gutter: c(r.gutter),
            brand: c(r.accent),
            focus: c(r.accent),
            link: c(r.link),
            mark: c(r.fg),
            unread: c(r.fg),
            mention: c(r.mention),
            arrived: c(r.mention),
            online: c(r.online),
            unseen: c(r.comment),
            read: c(r.link),
            warn: c(r.mention),
            notice: c(r.notice),
            progress: c(r.accent),
            senders: r.senders.map(c),
            truecolor,
            light: mode == Mode::Light,
        }
    }

    /// The theme `id` names, Tokyo Night where it names none jaw knows.
    pub fn named(id: &str, truecolor: bool) -> Theme {
        let theme = jaw_proto::themes::or_default(id);
        Theme::new(&Roles::of(&theme.colors), theme.mode, truecolor)
    }

    pub fn with(truecolor: bool) -> Theme {
        Theme::named(jaw_proto::themes::DEFAULT, truecolor)
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

    #[test]
    fn every_theme_keeps_its_greys_in_their_bands_and_in_order() {
        for theme in jaw_proto::themes::all() {
            let r = Roles::of(&theme.colors);
            let c = |x: u32| contrast(x, r.bg);
            let id = &theme.id;
            assert!((1.4..=2.61).contains(&c(r.gutter)), "{id}: borders {:.2}", c(r.gutter));
            assert!((2.5..=4.51).contains(&c(r.comment)), "{id}: dim text {:.2}", c(r.comment));
            assert!(c(r.gutter) < c(r.comment) && c(r.comment) < c(r.muted), "{id}: borders, dim, secondary");
            assert!(c(r.muted) < c(r.text) && c(r.text) <= c(r.fg), "{id}: secondary, text, text that matters");
            assert!(c(r.text) >= 4.5, "{id}: text {:.2}", c(r.text));
            assert!(c(r.surface) >= 1.05 && c(r.selection) >= 1.15, "{id}: surface and selection stand out");
            let on_surface = contrast(r.selection, r.surface);
            assert!(on_surface >= 1.15, "{id}: the selection on the surface {on_surface:.2}, as overlays draw it");
            assert!(contrast(r.fg, r.selection) >= 4.5, "{id}: text on the selection");
        }
    }

    #[test]
    fn every_colour_reads_at_least_three_to_one() {
        for theme in jaw_proto::themes::all() {
            let r = Roles::of(&theme.colors);
            let colours = [
                ("accent", r.accent),
                ("link", r.link),
                ("mention", r.mention),
                ("online", r.online),
                ("notice", r.notice),
            ];
            for (what, colour) in colours.into_iter().chain(r.senders.iter().map(|s| ("sender", *s))) {
                assert!(contrast(colour, r.bg) >= 3.0, "{}: {what} {:.2}", theme.id, contrast(colour, r.bg));
            }
        }
    }

    /// What jaw moves in each theme at Omarchy's 4c1b4cb, as the design lists it.
    const MOVED: [(&[&str], &[&str]); 6] = [
        (&[], &["catppuccin", "flexoki-light", "gruvbox", "hackerman", "lumon", "matte-black", "rose-pine"]),
        (
            &["dim text raised"],
            &["catppuccin-latte", "everforest", "last-horizon", "miasma", "nord", "solitude", "white"],
        ),
        (&["borders lowered", "dim text lowered"], &["ethereal", "osaka-jade", "retro-82", "starship"]),
        (&["borders lowered"], &["miasma", "ristretto", "vantablack", "white"]),
        (&["surface raised"], &["last-horizon", "lupine", "solitude"]),
        (&["selection raised"], &["kanagawa", "tokyo-night", "vantablack", "white"]),
    ];

    #[test]
    fn each_theme_moves_the_greys_the_design_lists_and_each_colour_moved_lands_within_a_step_of_its_target() {
        for theme in jaw_proto::themes::all() {
            let (c, r, id) = (&theme.colors, Roles::of(&theme.colors), theme.id.as_str());
            let mut moved = vec![];
            for (what, was, now, ground, target) in [
                ("surface", c.lighter_background.0, r.surface, r.bg, 1.1),
                ("selection", c.selection.0, r.selection, r.surface, 1.15),
                ("dim text", c.dark_foreground.0, r.comment, r.bg, 3.0),
                ("borders", c.muted.0, r.gutter, r.bg, 1.8),
            ] {
                if now == was {
                    continue;
                }
                let (from, to) = (contrast(was, ground), contrast(now, ground));
                moved.push(format!("{what} {}", if to > from { "raised" } else { "lowered" }));
                let step = if to > from { target..=target + 0.05 } else { target - 0.05..=target };
                assert!(step.contains(&to), "{id}: {what} moved to {to:.3}, not within a step of {target}");
            }
            moved.sort();
            let mut listed: Vec<String> = MOVED
                .iter()
                .filter(|(_, ids)| ids.contains(&id))
                .flat_map(|(what, _)| what.iter().map(|w| w.to_string()))
                .collect();
            listed.sort();
            assert!(MOVED.iter().any(|(_, ids)| ids.contains(&id)), "{id} isn't in the design's list");
            assert_eq!(moved, listed, "{id}");
            let senders = [c.red, c.yellow, c.blue, c.magenta, c.cyan, c.green, c.orange, c.bright_cyan];
            for (was, now) in [(c.cyan, r.link), (c.orange, r.mention), (c.green, r.online), (c.yellow, r.notice)]
                .into_iter()
                .chain(senders.into_iter().zip(r.senders))
            {
                let to = contrast(now, r.bg);
                assert!(now == was.0 || (3.0..=3.05).contains(&to), "{id}: #{:06x} raised to {to:.3}, not 3", was.0);
            }
        }
    }

    #[test]
    fn a_grey_outside_its_band_is_moved_toward_the_ground_or_the_text_keeping_its_hue() {
        let theme = |id: &str| jaw_proto::themes::get(id).unwrap().colors;
        let on_line =
            |from: u32, to: u32, colour: u32| (0..=10_000).any(|i| mix(from, to, f64::from(i) / 10_000.0) == colour);
        let solitude = theme("solitude");
        let r = Roles::of(&solitude);
        let (grey, raised) = (solitude.dark_foreground.0, r.comment);
        assert!(contrast(raised, r.bg) > contrast(grey, r.bg), "Solitude's dim text is too dim, so it's raised");
        assert_ne!(r.fg, r.text, "Solitude's text that matters isn't its ordinary text");
        assert!(on_line(grey, r.fg, raised), "on the line from its own grey to the text that matters");
        assert!(!on_line(grey, r.text, raised), "not on the line to ordinary text");
        let osaka = theme("osaka-jade");
        let (grey, ground, lowered) = (osaka.dark_foreground.0, osaka.background.0, Roles::of(&osaka).comment);
        assert!(contrast(lowered, ground) < contrast(grey, ground), "Osaka Jade's dim text stands out too much");
        assert!(on_line(grey, ground, lowered), "so it's lowered on the line from its own grey to the ground");
    }

    #[test]
    fn tokyo_night_is_omarchys_and_an_unknown_theme_is_tokyo_night() {
        let r = Roles::of(&jaw_proto::themes::get("tokyo-night").unwrap().colors);
        assert_eq!(
            (r.bg, r.surface, r.selection, r.fg, r.text, r.muted, r.comment, r.gutter),
            (0x1a1b26, 0x24283b, 0x2d3247, 0xc0caf5, 0xa9b1d6, 0x8088b0, 0x565f89, 0x414868)
        );
        assert_eq!(
            (r.accent, r.link, r.mention, r.senders[3], r.senders[7]),
            (0x7aa2f7, 0x449dab, 0xeb927b, 0xad8ee6, 0x0db9d7)
        );
        assert_eq!(Theme::named("no-such-theme", true), Theme::with(true));
        assert!(!Theme::with(true).light && Theme::named("catppuccin-latte", true).light);
    }
}
