//! JPEG's markers, read without decoding: the size, the EXIF orientation, whether there's metadata
//! to strip; and the same picture written again without it, its compressed data untouched.

use std::io::Write;

/// What a JPEG's headers say.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    /// EXIF's orientation, 1 to 8.
    pub orientation: u8,
    /// Whether it carries EXIF, XMP, IPTC, comments, extra pictures or anything else to strip.
    pub metadata: bool,
}

const SOI: u8 = 0xD8;
const EOI: u8 = 0xD9;
const SOS: u8 = 0xDA;
const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const APP2: u8 = 0xE2;
const APP14: u8 = 0xEE;

/// Reads the headers up to the first scan.
pub(crate) fn header(data: &[u8]) -> Option<Header> {
    let mut header = Header { orientation: 1, ..Header::default() };
    for (marker, body) in Segments::new(data)? {
        match marker {
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) && body.len() >= 5 => {
                header.height = u16::from_be_bytes([body[1], body[2]]) as u32;
                header.width = u16::from_be_bytes([body[3], body[4]]) as u32;
            }
            APP1 if body.starts_with(b"Exif\0\0") => {
                header.orientation = orientation(&body[6..]).unwrap_or(1);
                header.metadata |= body != &exif_orientation(header.orientation)[4..];
            }
            SOS => break,
            _ if !kept(marker, body) => header.metadata = true,
            _ => {}
        }
    }
    (header.width > 0 && header.height > 0).then_some(header)
}

/// Whether a segment is part of the picture rather than metadata about it: JFIF, the ICC profile
/// its colours need, Adobe's colour transform, and every segment that isn't an APPn or comment.
fn kept(marker: u8, body: &[u8]) -> bool {
    match marker {
        APP0 => body.starts_with(b"JFIF\0"),
        APP2 => body.starts_with(b"ICC_PROFILE\0"),
        APP14 => body.starts_with(b"Adobe"),
        0xE1..=0xEF | 0xFE => false,
        _ => true,
    }
}

/// Writes `data` again with its metadata stripped and nothing after its end, keeping only the
/// orientation, in an EXIF segment of its own, when it isn't upright.
pub(crate) fn strip(data: &[u8], orientation: u8, out: &mut impl Write) -> std::io::Result<()> {
    out.write_all(&[0xFF, SOI])?;
    let mut at = 2;
    let mut wrote_orientation = orientation == 1;
    while at + 4 <= data.len() {
        if data[at] != 0xFF {
            return Err(broken());
        }
        let marker = data[at + 1];
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if marker == EOI {
            break;
        }
        let length = u16::from_be_bytes([data[at + 2], data[at + 3]]) as usize;
        let end = at + 2 + length;
        if length < 2 || end > data.len() {
            return Err(broken());
        }
        if !wrote_orientation && !matches!(marker, APP0) {
            out.write_all(&exif_orientation(orientation))?;
            wrote_orientation = true;
        }
        if kept(marker, &data[at + 4..end]) {
            out.write_all(&data[at..end])?;
        }
        at = end;
        if marker == SOS {
            // The entropy-coded data runs to the next marker that isn't a stuffed byte or a restart.
            let scan = at;
            while at + 1 < data.len() && !(data[at] == 0xFF && !matches!(data[at + 1], 0x00 | 0xD0..=0xD7 | 0xFF)) {
                at += 1;
            }
            out.write_all(&data[scan..at])?;
        }
    }
    out.write_all(&[0xFF, EOI])
}

fn broken() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "the JPEG's markers are broken")
}

/// An APP1 EXIF segment holding only an orientation.
fn exif_orientation(orientation: u8) -> [u8; 36] {
    let mut s = [0u8; 36];
    s[..4].copy_from_slice(&[0xFF, APP1, 0, 34]);
    s[4..10].copy_from_slice(b"Exif\0\0");
    s[10..18].copy_from_slice(b"MM\0\x2A\0\0\0\x08");
    s[18..20].copy_from_slice(&1u16.to_be_bytes());
    s[20..32].copy_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0]);
    s
}

/// The orientation tag from EXIF's TIFF structure.
fn orientation(tiff: &[u8]) -> Option<u8> {
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| {
        let b: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if big { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) })
    };
    let u32_at = |at: usize| {
        let b: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    };
    let ifd = u32_at(4)? as usize;
    let entries = u16_at(ifd)? as usize;
    (0..entries.min(256)).find_map(|i| {
        let entry = ifd + 2 + i * 12;
        (u16_at(entry)? == 0x0112).then(|| u16_at(entry + 8)).flatten().filter(|o| (1..=8).contains(o)).map(|o| o as u8)
    })
}

/// The marker segments before the first scan, each with its body after the length.
struct Segments<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Segments<'a> {
    fn new(data: &'a [u8]) -> Option<Segments<'a>> {
        data.starts_with(&[0xFF, SOI]).then_some(Segments { data, at: 2 })
    }
}

impl<'a> Iterator for Segments<'a> {
    type Item = (u8, &'a [u8]);

    fn next(&mut self) -> Option<(u8, &'a [u8])> {
        let data = self.data;
        while data.get(self.at) == Some(&0xFF) && data.get(self.at + 1) == Some(&0xFF) {
            self.at += 1;
        }
        if data.get(self.at) != Some(&0xFF) {
            return None;
        }
        let marker = *data.get(self.at + 1)?;
        let length = u16::from_be_bytes([*data.get(self.at + 2)?, *data.get(self.at + 3)?]) as usize;
        let body = data.get(self.at + 4..self.at + 2 + length.max(2))?;
        self.at += 2 + length;
        Some((marker, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let pixels = vec![128u8; (width * height * 3) as usize];
        jpeg_encoder::Encoder::new(&mut out, 80)
            .encode(&pixels, width as u16, height as u16, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        out
    }

    /// `data` with `segment` inserted after its start, and `trailer` after its end.
    fn with(data: &[u8], segment: &[u8], trailer: &[u8]) -> Vec<u8> {
        [&data[..2], segment, &data[2..], trailer].concat()
    }

    fn app(marker: u8, body: &[u8]) -> Vec<u8> {
        [&[0xFF, marker][..], &((body.len() + 2) as u16).to_be_bytes(), body].concat()
    }

    #[test]
    fn the_size_and_orientation_are_read_from_the_headers() {
        let plain = jpeg(40, 30);
        assert_eq!(header(&plain), Some(Header { width: 40, height: 30, orientation: 1, metadata: false }));
        let turned = with(&plain, &exif_orientation(6), &[]);
        assert_eq!(header(&turned), Some(Header { width: 40, height: 30, orientation: 6, metadata: false }));
        let tagged = with(&plain, &app(APP1, b"Exif\0\0MM\0\x2A\0\0\0\x08\0\0 a place"), &[]);
        assert_eq!(header(&tagged).map(|h| (h.orientation, h.metadata)), Some((1, true)));
    }

    #[test]
    fn stripping_drops_metadata_and_trailers_and_keeps_the_picture_and_orientation() {
        let plain = jpeg(40, 30);
        let gps = app(APP1, b"Exif\0\0MM\0\x2A\0\0\0\x08\0\0 a place");
        let xmp = app(APP1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>");
        let comment = app(0xFE, b"taken at home");
        let dirty = with(&plain, &[gps, xmp, comment].concat(), b"a second picture");
        let mut clean = Vec::new();
        strip(&dirty, 1, &mut clean).unwrap();
        assert_eq!(clean, plain);
        let mut upright_later = Vec::new();
        strip(&dirty, 8, &mut upright_later).unwrap();
        assert_eq!(header(&upright_later).map(|h| (h.orientation, h.metadata)), Some((8, false)));
        let decoded = image::load_from_memory(&upright_later).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (40, 30));
    }
}
