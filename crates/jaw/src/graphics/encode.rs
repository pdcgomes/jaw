//! A preview turned into what the terminal draws: read, scaled to the cells it fills, and encoded
//! as half-blocks and, for a terminal with graphics, as its protocol has it.

use super::{Block, Encoded, Key, Protocol, Rgb, kitty};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

/// Pixels `w` by `h`, three bytes each, or four where the picture has alpha.
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub alpha: bool,
    pub bytes: Vec<u8>,
}

impl Pixels {
    fn kind(&self) -> PixelType {
        if self.alpha { PixelType::U8x4 } else { PixelType::U8x3 }
    }

    /// The pixel at `x`, `y`, `None` where it's mostly transparent.
    fn at(&self, x: u32, y: u32) -> Option<Rgb> {
        let n = if self.alpha { 4 } else { 3 };
        let i = (y * self.w + x) as usize * n;
        let p = &self.bytes[i..i + n];
        (!self.alpha || p[3] >= 128).then_some([p[0], p[1], p[2]])
    }
}

fn read(path: &std::path::Path) -> Result<Pixels, String> {
    let decoded = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (decoded.width(), decoded.height());
    Ok(if decoded.color().has_alpha() {
        Pixels { w, h, alpha: true, bytes: decoded.into_rgba8().into_raw() }
    } else {
        Pixels { w, h, alpha: false, bytes: decoded.into_rgb8().into_raw() }
    })
}

/// The picture scaled to fill `w` by `h`, its middle kept where its shape isn't theirs, as for an
/// album's tiles.
pub fn scale(src: &Pixels, w: u32, h: u32) -> Result<Pixels, String> {
    let from = ImageRef::new(src.w, src.h, &src.bytes, src.kind()).map_err(|e| e.to_string())?;
    let mut to = Image::new(w.max(1), h.max(1), src.kind());
    let options =
        ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)).fit_into_destination(None);
    Resizer::new().resize(&from, &mut to, &options).map_err(|e| e.to_string())?;
    Ok(Pixels { w: to.width(), h: to.height(), alpha: src.alpha, bytes: to.into_vec() })
}

/// Two pixels a cell, a cell a column and two pixels a row.
pub fn blocks(src: &Pixels, cols: u16, rows: u16) -> Result<Vec<Block>, String> {
    let small = scale(src, cols as u32, rows as u32 * 2)?;
    let at = |x: u32, y: u32| small.at(x, y);
    let mut out = Vec::with_capacity(cols as usize * rows as usize);
    for row in 0..rows as u32 {
        for col in 0..cols as u32 {
            out.push((at(col, row * 2), at(col, row * 2 + 1)));
        }
    }
    Ok(out)
}

fn png(p: &Pixels) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        &p.bytes,
        p.w,
        p.h,
        if p.alpha { image::ExtendedColorType::Rgba8 } else { image::ExtendedColorType::Rgb8 },
    )
    .map_err(|e| e.to_string())?;
    Ok(out)
}

/// iTerm2's inline picture, filling `cols` by `rows` cells.
pub fn iterm(png: &[u8], cols: u16, rows: u16) -> Vec<u8> {
    let data = crate::run::base64(png);
    let mut out =
        format!("\x1b]1337;File=inline=1;size={};width={cols};height={rows};preserveAspectRatio=0:", png.len())
            .into_bytes();
    out.extend(data.as_bytes());
    out.push(0x07);
    out
}

/// A sixel picture: six rows of pixels a band, its colours the 216 of a 6×6×6 cube, transparent
/// pixels left as they are.
pub fn sixel(p: &Pixels) -> Vec<u8> {
    let level = |v: u8| ((v as u16 * 5 + 127) / 255) as usize;
    let index =
        |x: u32, y: u32| -> Option<usize> { p.at(x, y).map(|[r, g, b]| level(r) * 36 + level(g) * 6 + level(b)) };
    let mut out = format!("\x1bP0;1;0q\"1;1;{};{}", p.w, p.h).into_bytes();
    let mut used = [false; 216];
    for y in 0..p.h {
        for x in 0..p.w {
            if let Some(c) = index(x, y) {
                used[c] = true;
            }
        }
    }
    for (c, _) in used.iter().enumerate().filter(|(_, u)| **u) {
        let (r, g, b) = (c / 36, c / 6 % 6, c % 6);
        out.extend(format!("#{c};2;{};{};{}", r * 20, g * 20, b * 20).as_bytes());
    }
    for band in (0..p.h).step_by(6) {
        let mut first = true;
        for (c, _) in used.iter().enumerate().filter(|(_, u)| **u) {
            let bits: Vec<u8> = (0..p.w)
                .map(|x| {
                    (0..6).fold(0u8, |acc, dy| {
                        let y = band + dy;
                        if y < p.h && index(x, y) == Some(c) { acc | 1 << dy } else { acc }
                    })
                })
                .collect();
            if bits.iter().all(|b| *b == 0) {
                continue;
            }
            if !first {
                out.push(b'$');
            }
            first = false;
            out.extend(format!("#{c}").as_bytes());
            let mut i = 0;
            while i < bits.len() {
                let run = bits[i..].iter().take_while(|b| **b == bits[i]).count();
                let ch = 63 + bits[i];
                if run > 3 {
                    out.extend(format!("!{run}").as_bytes());
                    out.push(ch);
                } else {
                    out.extend(std::iter::repeat_n(ch, run));
                }
                i += run;
            }
        }
        out.push(b'-');
    }
    out.extend(b"\x1b\\");
    out
}

/// What a job makes: half-blocks always, and the protocol's own where the terminal has one.
pub(super) fn encode(
    key: &Key,
    protocol: Protocol,
    cell: (u16, u16),
    kitty_id: Option<u32>,
) -> Result<Encoded, String> {
    let src = read(&key.path)?;
    let blocks = blocks(&src, key.cols, key.rows)?;
    let (pw, ph) = (key.cols as u32 * cell.0 as u32, key.rows as u32 * cell.1 as u32);
    let (escape, kitty) = match protocol {
        Protocol::Kitty => {
            let id = kitty_id.unwrap_or(1);
            (Some(kitty::send(id, &png(&scale(&src, pw, ph)?)?, key.cols, key.rows)), Some(id))
        }
        Protocol::Iterm => (Some(iterm(&png(&scale(&src, pw, ph)?)?, key.cols, key.rows)), None),
        Protocol::Sixel => (Some(sixel(&scale(&src, pw, ph)?)), None),
        Protocol::Blocks | Protocol::None => (None, None),
    };
    Ok(Encoded { blocks, escape, kitty })
}
