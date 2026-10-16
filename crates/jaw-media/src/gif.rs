//! A GIF's blocks walked without decoding a frame: its size, frames, length and transparency.

use std::io::{BufReader, Read};

#[derive(Debug, PartialEq)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// The frames' delays added up, in hundredths of a second, as browsers play them.
    pub centiseconds: u64,
    pub alpha: bool,
}

/// Browsers play a delay under 2 hundredths as 10, and so does ffmpeg.
const SHORTEST: u64 = 2;
const PLAYED_AS: u64 = 10;

pub(crate) fn header(input: impl Read) -> Option<Header> {
    let mut r = BufReader::new(input);
    let mut screen = [0u8; 13];
    r.read_exact(&mut screen).ok()?;
    if !screen.starts_with(b"GIF8") {
        return None;
    }
    let mut header = Header {
        width: u16::from_le_bytes([screen[6], screen[7]]) as u32,
        height: u16::from_le_bytes([screen[8], screen[9]]) as u32,
        frames: 0,
        centiseconds: 0,
        alpha: false,
    };
    skip_table(&mut r, screen[10])?;
    let mut delay = 0;
    loop {
        match byte(&mut r)? {
            0x21 => {
                let label = byte(&mut r)?;
                let first = sub_block(&mut r)?;
                if label == 0xF9 && first.len() >= 4 {
                    header.alpha |= first[0] & 1 == 1;
                    delay = u16::from_le_bytes([first[1], first[2]]) as u64;
                }
                if !first.is_empty() {
                    skip_sub_blocks(&mut r)?;
                }
            }
            0x2C => {
                let mut descriptor = [0u8; 9];
                r.read_exact(&mut descriptor).ok()?;
                skip_table(&mut r, descriptor[8])?;
                byte(&mut r)?;
                skip_sub_blocks(&mut r)?;
                header.frames += 1;
                header.centiseconds += if delay < SHORTEST { PLAYED_AS } else { delay };
                delay = 0;
            }
            0x3B => break,
            _ => return None,
        }
    }
    (header.frames > 0).then_some(header)
}

fn byte(r: &mut impl Read) -> Option<u8> {
    let mut b = [0u8];
    r.read_exact(&mut b).ok()?;
    Some(b[0])
}

fn skip_table(r: &mut impl Read, flags: u8) -> Option<()> {
    if flags & 0x80 != 0 {
        let size = 3u64 << ((flags & 7) + 1);
        (std::io::copy(&mut r.take(size), &mut std::io::sink()).ok()? == size).then_some(())
    } else {
        Some(())
    }
}

/// One sub-block's data; empty at the terminator.
fn sub_block(r: &mut impl Read) -> Option<Vec<u8>> {
    let mut data = vec![0u8; byte(r)? as usize];
    r.read_exact(&mut data).ok()?;
    Some(data)
}

fn skip_sub_blocks(r: &mut impl Read) -> Option<()> {
    loop {
        let size = byte(r)? as u64;
        if size == 0 {
            return Some(());
        }
        if std::io::copy(&mut r.take(size), &mut std::io::sink()).ok()? != size {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba, RgbaImage};

    #[test]
    fn frames_length_and_transparency_are_read_without_decoding() {
        let mut gif = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut gif);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            for shade in [0u8, 120, 240] {
                let frame = RgbaImage::from_pixel(30, 20, Rgba([shade, 0, 0, if shade == 0 { 0 } else { 255 }]));
                encoder.encode_frame(Frame::from_parts(frame, 0, 0, Delay::from_numer_denom_ms(200, 1))).unwrap();
            }
        }
        let header = header(gif.as_slice()).unwrap();
        assert_eq!(header, Header { width: 30, height: 20, frames: 3, centiseconds: 60, alpha: true });
    }
}
