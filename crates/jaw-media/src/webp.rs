//! A WebP's chunks walked without decoding: its canvas, transparency, animation and length.

use std::io::{BufReader, Read};

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub alpha: bool,
    pub frames: u32,
    pub milliseconds: u64,
    /// Whether it carries EXIF or XMP.
    pub metadata: bool,
}

pub(crate) fn header(input: impl Read) -> Option<Header> {
    let mut r = BufReader::new(input);
    let mut riff = [0u8; 12];
    r.read_exact(&mut riff).ok()?;
    if &riff[..4] != b"RIFF" || &riff[8..] != b"WEBP" {
        return None;
    }
    let mut header = Header::default();
    let mut animated = false;
    loop {
        let mut chunk = [0u8; 8];
        if r.read_exact(&mut chunk).is_err() {
            break;
        }
        let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
        let padded = size + (size & 1);
        let wanted = match &chunk[..4] {
            b"VP8X" | b"VP8 " | b"VP8L" => padded.min(30),
            b"ANMF" => 16,
            _ => 0,
        };
        let mut head = vec![0u8; wanted as usize];
        r.read_exact(&mut head).ok()?;
        match &chunk[..4] {
            b"VP8X" if head.len() >= 10 => {
                header.alpha = head[0] & 0x10 != 0;
                animated = head[0] & 0x02 != 0;
                header.width = u24(&head[4..7]) + 1;
                header.height = u24(&head[7..10]) + 1;
            }
            b"VP8 " if head.len() >= 10 && header.width == 0 => {
                header.width = (u16::from_le_bytes([head[6], head[7]]) & 0x3FFF) as u32;
                header.height = (u16::from_le_bytes([head[8], head[9]]) & 0x3FFF) as u32;
                header.frames = 1;
            }
            b"VP8L" if head.len() >= 5 && head[0] == 0x2F && header.width == 0 => {
                let bits = u32::from_le_bytes([head[1], head[2], head[3], head[4]]);
                header.width = (bits & 0x3FFF) + 1;
                header.height = ((bits >> 14) & 0x3FFF) + 1;
                header.alpha = bits >> 28 & 1 == 1;
                header.frames = 1;
            }
            b"ANMF" if head.len() >= 16 => {
                header.frames += 1;
                header.milliseconds += u24(&head[12..15]) as u64;
            }
            b"EXIF" | b"XMP " => header.metadata = true,
            _ => {}
        }
        let rest = padded - wanted;
        if std::io::copy(&mut (&mut r).take(rest), &mut std::io::sink()).ok()? != rest {
            break;
        }
    }
    if !animated && header.frames == 0 {
        header.frames = 1;
    }
    (header.width > 0 && header.height > 0).then_some(header)
}

fn u24(b: &[u8]) -> u32 {
    b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16
}
