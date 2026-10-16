//! What's drawn rather than written: the Mimic in half-block pixel art, a pixel to a cell, and
//! QR codes in half-blocks, light on dark.

use crate::theme::{Theme, nearest_256};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// badcoffee.dev's Mimic, 45 by 26 pixels.
const MIMIC: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../creatures/mimic.png"));

/// Its eyes' rows: open, each is a ring on rows 2 to 4; shut, a line on row 4.
const EYES: std::ops::RangeInclusive<usize> = 2..=4;
/// Its lid, eyes and upper teeth, which drop this many pixels when it chomps.
const LID: usize = 10;
const CHOMP: usize = 3;

type Rgb = [u8; 3];

#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub w: usize,
    pub h: usize,
    px: Vec<Option<Rgb>>,
}

/// How the Mimic looks this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// jawd can't be reached: grey, its eyes shut.
    pub asleep: bool,
    /// Its mouth shut, half of a chomp.
    pub shut: bool,
}

impl Sprite {
    pub fn mimic() -> Sprite {
        Sprite::decode(MIMIC).unwrap_or(Sprite { w: 0, h: 0, px: vec![] })
    }

    fn decode(bytes: &[u8]) -> Option<Sprite> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let (w, h) = (info.width as usize, info.height as usize);
        let channels = info.color_type.samples();
        let px = buf[..info.buffer_size()]
            .chunks_exact(channels)
            .map(|p| match channels {
                4 => (p[3] >= 128).then_some([p[0], p[1], p[2]]),
                3 => Some([p[0], p[1], p[2]]),
                2 => (p[1] >= 128).then_some([p[0], p[0], p[0]]),
                _ => Some([p[0], p[0], p[0]]),
            })
            .collect();
        Some(Sprite { w, h, px })
    }

    fn at(&self, x: usize, y: usize) -> Option<Rgb> {
        if x < self.w && y < self.h { self.px[y * self.w + x] } else { None }
    }

    /// Terminal rows it takes.
    pub fn rows(&self) -> usize {
        self.h.div_ceil(2)
    }

    /// The pixel at (x, y) as it looks: its eyes shut while asleep, its lid dropped mid-chomp.
    fn looking(&self, look: Look, x: usize, y: usize) -> Option<Rgb> {
        let y = if look.shut {
            match y {
                0..CHOMP => return None,
                _ if y <= LID + CHOMP => y - CHOMP,
                _ => y,
            }
        } else {
            y
        };
        let p = self.at(x, y)?;
        if look.asleep && EYES.contains(&y) {
            let white = |p: Rgb| luma(p) > 190.0;
            let iris = self.at(x, *EYES.start()).is_some_and(white);
            let lid = self.at(x, EYES.start() - 1).unwrap_or(p);
            return Some(if iris && y == *EYES.end() {
                [0xfb, 0xf6, 0xe6]
            } else if white(p) {
                lid
            } else {
                p
            });
        }
        Some(p)
    }

    /// Two pixels to a cell: the top one as the glyph's colour and the bottom one as the cell's.
    pub fn lines(&self, look: Look, theme: &Theme) -> Vec<Line<'static>> {
        let truecolor = theme.truecolor;
        let shade = |p: Rgb| -> Color {
            let p = if look.asleep {
                let g = (luma(p) * 0.55) as u8;
                [g, g, g.saturating_add(8)]
            } else {
                p
            };
            if truecolor { Color::Rgb(p[0], p[1], p[2]) } else { Color::Indexed(nearest_256(p[0], p[1], p[2])) }
        };
        (0..self.rows())
            .map(|row| {
                let spans: Vec<Span> = (0..self.w)
                    .map(|x| match (self.looking(look, x, row * 2), self.looking(look, x, row * 2 + 1)) {
                        (None, None) => Span::styled(" ", Style::new().bg(theme.bg)),
                        (Some(a), None) => Span::styled("▀", Style::new().fg(shade(a)).bg(theme.bg)),
                        (None, Some(b)) => Span::styled("▄", Style::new().fg(shade(b)).bg(theme.bg)),
                        (Some(a), Some(b)) => Span::styled("▀", Style::new().fg(shade(a)).bg(shade(b))),
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
    }
}

fn luma(p: Rgb) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// A QR code in half-blocks, light on dark with its quiet zone, so a phone reads it from a dark
/// terminal: two modules to a cell, the top one as the glyph.
pub fn qr(code: &str) -> Vec<String> {
    let Ok(qr) = qrcode::QrCode::new(code.as_bytes()) else { return vec![] };
    let n = qr.width();
    let quiet = 2;
    let dark = |x: isize, y: isize| {
        x >= 0 && y >= 0 && (x as usize) < n && (y as usize) < n && qr[(x as usize, y as usize)] == qrcode::Color::Dark
    };
    let side = (n + 2 * quiet) as isize;
    (0..side)
        .step_by(2)
        .map(|y| {
            (0..side)
                .map(|x| {
                    let (x, y) = (x - quiet as isize, y - quiet as isize);
                    match (!dark(x, y), !dark(x, y + 1) && y + 1 < side - quiet as isize) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mimic_is_a_pixel_to_a_cell_and_two_pixels_to_a_row() {
        let s = Sprite::mimic();
        assert_eq!((s.w, s.h, s.rows()), (45, 26, 13), "mimic.png's grid");
        let lines = s.lines(Look::default(), &Theme::with(true));
        assert_eq!(lines.len(), 13);
        assert!(lines.iter().all(|l| l.width() == 45));
    }

    #[test]
    fn asleep_it_is_grey_and_its_eyes_are_shut() {
        let s = Sprite::mimic();
        let asleep = Look { asleep: true, shut: false };
        let white = |p: Option<Rgb>| p.is_some_and(|p| luma(p) > 190.0);
        assert!(white(s.looking(Look::default(), 12, 2)), "an open eye is white on top");
        assert!(!white(s.looking(asleep, 12, 2)) && !white(s.looking(asleep, 13, 3)));
        assert!((12..=14).chain(32..=34).all(|x| white(s.looking(asleep, x, 4))), "a shut eye is a line");
        let grey = |c: Color| matches!(c, Color::Rgb(r, g, b) if r == g && b >= g && b - g <= 8);
        for line in s.lines(asleep, &Theme::with(true)) {
            for span in line.spans.iter().filter(|s| s.content != " ") {
                assert!(span.style.fg.is_none_or(grey), "{:?}", span.style);
            }
        }
    }

    #[test]
    fn a_chomp_drops_its_lid_onto_its_lower_teeth() {
        let s = Sprite::mimic();
        let shut = Look { asleep: false, shut: true };
        assert_eq!(s.looking(shut, 12, 0), None, "the top of the lid has gone down");
        assert_eq!(s.looking(shut, 12, 8 + CHOMP), s.looking(Look::default(), 12, 8), "the upper teeth with it");
        assert_eq!(s.looking(shut, 13, 16), s.looking(Look::default(), 13, 16), "the lower teeth stay");
    }

    #[test]
    fn a_qr_code_is_light_on_dark_with_room_around_it() {
        let rows = qr("2@jaw-demo,made-up,linking-code");
        assert!(!rows.is_empty());
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
        assert!(rows[0].chars().all(|c| c == '█'), "the quiet zone is light: {}", rows[0]);
        assert!(rows.iter().any(|r| r.contains(' ')), "and the modules dark");
    }
}
