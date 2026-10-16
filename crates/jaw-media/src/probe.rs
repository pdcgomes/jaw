//! What a file is: its magic bytes first, then its headers for photos, GIFs and stickers, which
//! answer in microseconds, and ffprobe's JSON for everything else.

use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

pub use image::metadata::Orientation;

use crate::error::{Error, Result};
use crate::job::Cancel;
use crate::run::{Stdout, run};
use crate::tools::Tools;
use crate::{gif, jpeg, webp};

/// What a file is for, as WhatsApp sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Photo,
    Video,
    /// An animation without sound, which WhatsApp sends as an MP4 that loops.
    Gif,
    Sticker,
    AnimatedSticker,
    /// Music and other recordings, played as a file.
    Audio,
    /// A voice message: Opus in Ogg, in one channel.
    Voice,
    Document,
}

/// The file's format, from its magic bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Jpeg,
    Png,
    Gif,
    Webp,
    Heif,
    Avif,
    Tiff,
    Bmp,
    Mp4,
    Mov,
    ThreeGp,
    Matroska,
    Avi,
    Ogg,
    Mp3,
    Aac,
    M4a,
    Wav,
    Flac,
    Amr,
    Pdf,
    Zip,
    Other,
}

impl Format {
    /// Whether the file is an ISO media file, as MP4, MOV, 3GP, M4A and HEIF are.
    pub fn is_iso_media(self) -> bool {
        matches!(self, Format::Mp4 | Format::Mov | Format::ThreeGp | Format::M4a | Format::Heif | Format::Avif)
    }
}

/// What a file is.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    /// What it's for. A guess from the file alone; jawd sets it from the message when it has one,
    /// so a sticker or voice message is one whatever its file looks like.
    pub kind: Kind,
    pub format: Format,
    pub mime: String,
    /// Bytes.
    pub size: u64,
    /// Width and height as stored, before `orientation` turns it upright.
    pub dimensions: Option<(u32, u32)>,
    pub orientation: Orientation,
    pub duration: Option<Duration>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub pixel_format: Option<String>,
    /// Frames a second.
    pub frame_rate: Option<f64>,
    /// Bits a second, for the whole file.
    pub bit_rate: Option<u64>,
    /// Frames, for an animation.
    pub frames: Option<u32>,
    pub animated: bool,
    pub alpha: bool,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    /// Whether it carries metadata that sending strips (EXIF, XMP, tags, places, cover art).
    pub metadata: bool,
    /// The stream ffmpeg reads the picture from: a HEIF's tile grid or a video's first real stream.
    pub(crate) picture: Option<String>,
}

impl Probe {
    pub(crate) fn new(kind: Kind, format: Format, mime: &str, size: u64) -> Probe {
        Probe {
            kind,
            format,
            mime: mime.to_string(),
            size,
            dimensions: None,
            orientation: Orientation::NoTransforms,
            duration: None,
            video_codec: None,
            audio_codec: None,
            pixel_format: None,
            frame_rate: None,
            bit_rate: None,
            frames: None,
            animated: false,
            alpha: false,
            channels: None,
            sample_rate: None,
            metadata: false,
            picture: None,
        }
    }

    /// Width and height as shown, once upright.
    pub fn display_dimensions(&self) -> Option<(u32, u32)> {
        let (w, h) = self.dimensions?;
        Some(if swaps(self.orientation) { (h, w) } else { (w, h) })
    }

    pub fn long_edge(&self) -> Option<u32> {
        self.dimensions.map(|(w, h)| w.max(h))
    }

    /// Whether the in-process decoders read this file's pictures.
    pub(crate) fn decodes_here(&self) -> bool {
        matches!(self.format, Format::Jpeg | Format::Png | Format::Gif | Format::Webp)
    }
}

/// Whether an orientation turns the picture on its side.
pub(crate) fn swaps(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

/// How much of a file's start is read for its magic and headers: enough for a JPEG's EXIF.
const HEAD: usize = 128 * 1024;
/// The size of a WhatsApp sticker, which a WebP of exactly this size is taken to be.
const STICKER: u32 = 512;

pub(crate) fn probe(tools: &Tools, path: &Path) -> Result<Probe> {
    let mut file = File::open(path).map_err(Error::io(path))?;
    let size = file.metadata().map_err(Error::io(path))?.len();
    let mut head = Vec::with_capacity(HEAD.min(size as usize));
    (&mut file).take(HEAD as u64).read_to_end(&mut head).map_err(Error::io(path))?;
    let format = sniff(&head);
    match format {
        Format::Jpeg => {
            let h = jpeg::header(&head).ok_or_else(|| Error::unreadable(path, "its JPEG headers are broken"))?;
            let mut p = Probe::new(Kind::Photo, format, "image/jpeg", size);
            p.dimensions = Some((h.width, h.height));
            p.orientation = Orientation::from_exif(h.orientation).unwrap_or(Orientation::NoTransforms);
            p.metadata = h.metadata;
            Ok(p)
        }
        Format::Png => {
            let mut p = Probe::new(Kind::Photo, format, "image/png", size);
            let ihdr = head.get(16..26).ok_or_else(|| Error::unreadable(path, "its PNG header is cut short"))?;
            p.dimensions = Some((
                u32::from_be_bytes(ihdr[0..4].try_into().expect("4 bytes")),
                u32::from_be_bytes(ihdr[4..8].try_into().expect("4 bytes")),
            ));
            p.alpha = matches!(ihdr[9], 4 | 6) || head.windows(4).any(|w| w == b"tRNS");
            p.animated = head.windows(4).any(|w| w == b"acTL");
            p.metadata = ["eXIf", "tEXt", "iTXt", "zTXt"].iter().any(|c| head.windows(4).any(|w| w == c.as_bytes()));
            Ok(p)
        }
        Format::Gif => {
            let file = File::open(path).map_err(Error::io(path))?;
            let h = gif::header(file).ok_or_else(|| Error::unreadable(path, "its GIF blocks are broken"))?;
            let kind = if h.frames > 1 { Kind::Gif } else { Kind::Photo };
            let mut p = Probe::new(kind, format, "image/gif", size);
            p.dimensions = Some((h.width, h.height));
            p.frames = Some(h.frames);
            p.animated = h.frames > 1;
            p.alpha = h.alpha;
            p.duration = p.animated.then(|| Duration::from_millis(h.centiseconds * 10));
            Ok(p)
        }
        Format::Webp => {
            let file = File::open(path).map_err(Error::io(path))?;
            let h = webp::header(file).ok_or_else(|| Error::unreadable(path, "its WebP chunks are broken"))?;
            let animated = h.frames > 1;
            let kind = match (h.width == STICKER && h.height == STICKER, animated) {
                (true, true) => Kind::AnimatedSticker,
                (true, false) => Kind::Sticker,
                (false, true) => Kind::Gif,
                (false, false) => Kind::Photo,
            };
            let mut p = Probe::new(kind, format, "image/webp", size);
            p.dimensions = Some((h.width, h.height));
            p.frames = Some(h.frames);
            p.animated = animated;
            p.alpha = h.alpha;
            p.metadata = h.metadata;
            p.duration = animated.then(|| Duration::from_millis(h.milliseconds));
            Ok(p)
        }
        Format::Pdf | Format::Zip | Format::Other => Ok(document(path, format, size)),
        _ if !tools.has_ffmpeg() => {
            let mut p = document(path, format, size);
            p.kind = guessed_kind(format);
            Ok(p)
        }
        _ => ffprobe(tools, path, format, size),
    }
}

fn document(path: &Path, format: Format, size: u64) -> Probe {
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mime = match format {
        Format::Pdf => "application/pdf",
        _ => mime_by_extension(&extension),
    };
    Probe::new(Kind::Document, format, mime, size)
}

/// What a file is likely for from its format alone, when ffprobe can't say more.
fn guessed_kind(format: Format) -> Kind {
    match format {
        Format::Heif | Format::Avif | Format::Tiff | Format::Bmp => Kind::Photo,
        Format::Mp4 | Format::Mov | Format::ThreeGp | Format::Matroska | Format::Avi => Kind::Video,
        Format::Ogg | Format::Mp3 | Format::Aac | Format::M4a | Format::Wav | Format::Flac | Format::Amr => Kind::Audio,
        _ => Kind::Document,
    }
}

/// The format from the file's first bytes.
pub(crate) fn sniff(head: &[u8]) -> Format {
    let at = |range: std::ops::Range<usize>| head.get(range).unwrap_or_default();
    match head {
        [0xFF, 0xD8, 0xFF, ..] => Format::Jpeg,
        [0x89, b'P', b'N', b'G', ..] => Format::Png,
        [b'G', b'I', b'F', b'8', ..] => Format::Gif,
        [b'R', b'I', b'F', b'F', ..] => match at(8..12) {
            b"WEBP" => Format::Webp,
            b"WAVE" => Format::Wav,
            b"AVI " => Format::Avi,
            _ => Format::Other,
        },
        _ if at(4..8) == b"ftyp" => brand(head),
        [0x1A, 0x45, 0xDF, 0xA3, ..] => Format::Matroska,
        [b'O', b'g', b'g', b'S', ..] => Format::Ogg,
        [b'I', b'D', b'3', ..] => Format::Mp3,
        [0xFF, b, ..] if b & 0xF6 == 0xF0 => Format::Aac,
        [0xFF, b, ..] if b & 0xE0 == 0xE0 => Format::Mp3,
        [b'f', b'L', b'a', b'C', ..] => Format::Flac,
        [b'#', b'!', b'A', b'M', b'R', ..] => Format::Amr,
        [b'%', b'P', b'D', b'F', ..] => Format::Pdf,
        [b'P', b'K', 3, 4, ..] => Format::Zip,
        [b'I', b'I', 42, 0, ..] | [b'M', b'M', 0, 42, ..] => Format::Tiff,
        [b'B', b'M', ..] => Format::Bmp,
        _ => Format::Other,
    }
}

/// An ISO media file's format from its brands.
fn brand(head: &[u8]) -> Format {
    let size = head.get(0..4).map(|b| u32::from_be_bytes(b.try_into().expect("4 bytes")) as usize).unwrap_or(0);
    let box_ = head.get(8..size.clamp(12, head.len().max(12))).unwrap_or_default();
    let brands: Vec<&[u8]> = box_.chunks(4).enumerate().filter(|(i, _)| *i != 1).map(|(_, b)| b).collect();
    let has = |names: &[&[u8; 4]]| brands.iter().any(|b| names.iter().any(|n| b == n));
    if has(&[b"avif", b"avis"]) {
        Format::Avif
    } else if has(&[b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1"]) {
        Format::Heif
    } else if has(&[b"qt  "]) {
        Format::Mov
    } else if has(&[b"M4A ", b"M4B "]) {
        Format::M4a
    } else if brands.first().is_some_and(|b| b.starts_with(b"3g")) {
        Format::ThreeGp
    } else {
        Format::Mp4
    }
}

fn mime_by_extension(extension: &str) -> &'static str {
    match extension {
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "pages" => "application/vnd.apple.pages",
        "numbers" => "application/vnd.apple.numbers",
        "key" => "application/vnd.apple.keynote",
        "txt" | "md" | "log" => "text/plain",
        "csv" => "text/csv",
        "rtf" => "application/rtf",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "apk" => "application/vnd.android.package-archive",
        "vcf" => "text/vcard",
        "ics" => "text/calendar",
        _ => "application/octet-stream",
    }
}

fn ffprobe(tools: &Tools, path: &Path, format: Format, size: u64) -> Result<Probe> {
    let ffprobe = tools.ffprobe("to read videos, audio and HEIC photos")?;
    let mut args: Vec<OsString> =
        ["-v", "error", "-print_format", "json", "-show_format", "-show_streams"].iter().map(OsString::from).collect();
    if tools.reads_heic() {
        args.push("-show_stream_groups".into());
    }
    args.push(path.into());
    let ran = run("ffprobe", ffprobe, &args, 0, path, Stdout::Capture { limit: 16 << 20 }, &Cancel::new())?;
    let json: Value = serde_json::from_slice(&ran.stdout).map_err(|e| Error::unreadable(path, e))?;
    from_ffprobe(&json, format, size).ok_or_else(|| Error::unreadable(path, "ffprobe found nothing to show or play"))
}

/// A probe from ffprobe's JSON; None if it found no picture or sound.
pub(crate) fn from_ffprobe(json: &Value, format: Format, size: u64) -> Option<Probe> {
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let of = |kind: &str| {
        streams.iter().find(|s| s["codec_type"] == kind && s["disposition"]["attached_pic"].as_i64() != Some(1))
    };
    let (video, audio) = (of("video"), of("audio"));
    let fmt = &json["format"];
    let number = |v: &Value| v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64());
    let mut p = Probe::new(Kind::Document, format, "application/octet-stream", size);
    p.duration = number(&fmt["duration"]).filter(|d| d.is_finite() && *d > 0.0).map(Duration::from_secs_f64);
    p.bit_rate = number(&fmt["bit_rate"]).map(|b| b as u64);
    let tags =
        |v: &Value| v["tags"].as_object().is_some_and(|t| t.keys().any(|k| !TECHNICAL_TAGS.contains(&k.as_str())));
    p.metadata = tags(fmt)
        || streams.iter().any(|s| s["codec_type"] == "data" || s["disposition"]["attached_pic"].as_i64() == Some(1));
    let grid = json["stream_groups"]
        .as_array()
        .and_then(|groups| groups.iter().position(|g| g["type"] == "Tile Grid").map(|i| (i, &groups[i])));
    if let Some(a) = audio {
        p.audio_codec = a["codec_name"].as_str().map(str::to_string);
        p.channels = a["channels"].as_u64().map(|c| c as u32);
        p.sample_rate = number(&a["sample_rate"]).map(|r| r as u32);
    }
    if let Some((index, group)) = grid {
        let component = &group["components"][0];
        p.dimensions = Some((component["width"].as_u64()? as u32, component["height"].as_u64()? as u32));
        p.orientation = rotation(&component["side_data_list"])
            .or_else(|| video.and_then(|v| rotation(&v["side_data_list"])))
            .unwrap_or(Orientation::NoTransforms);
        p.video_codec = video.and_then(|v| v["codec_name"].as_str()).map(str::to_string);
        p.picture = Some(format!("0:g:{index}"));
        p.kind = Kind::Photo;
    } else if let Some(v) = video {
        p.dimensions = Some((v["width"].as_u64()? as u32, v["height"].as_u64()? as u32));
        p.orientation = rotation(&v["side_data_list"]).unwrap_or(Orientation::NoTransforms);
        p.video_codec = v["codec_name"].as_str().map(str::to_string);
        p.pixel_format = v["pix_fmt"].as_str().map(str::to_string);
        p.frame_rate = rate(&v["avg_frame_rate"]).or_else(|| rate(&v["r_frame_rate"]));
        p.frames = v["nb_frames"].as_str().and_then(|n| n.parse().ok());
        p.picture = Some(format!("0:{}", v["index"].as_u64()?));
        let still = matches!(format, Format::Heif | Format::Avif | Format::Tiff | Format::Bmp)
            || p.duration.is_none() && p.frames.is_none_or(|n| n <= 1);
        p.kind = if still { Kind::Photo } else { Kind::Video };
        if still && p.duration.is_some_and(|d| d.as_secs_f64() < 0.1) {
            p.duration = None;
        }
    } else if audio.is_some() {
        let voice = format == Format::Ogg && p.audio_codec.as_deref() == Some("opus") && p.channels == Some(1);
        p.kind = if voice { Kind::Voice } else { Kind::Audio };
    } else {
        return None;
    }
    p.mime = mime(&p).to_string();
    Some(p)
}

/// Format tags that say nothing about who, where or when.
const TECHNICAL_TAGS: [&str; 5] = ["major_brand", "minor_version", "compatible_brands", "encoder", "handler_name"];

fn rate(v: &Value) -> Option<f64> {
    let (n, d) = v.as_str()?.split_once('/')?;
    let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
    (n > 0.0 && d > 0.0).then_some(n / d)
}

/// The orientation a display matrix asks for. ffprobe gives its rotation anticlockwise, so a
/// phone's portrait video says -90.
fn rotation(side_data: &Value) -> Option<Orientation> {
    let degrees = side_data.as_array()?.iter().find_map(|d| d["rotation"].as_f64())?;
    Some(match (degrees.round() as i64).rem_euclid(360) {
        270 => Orientation::Rotate90,
        180 => Orientation::Rotate180,
        90 => Orientation::Rotate270,
        _ => Orientation::NoTransforms,
    })
}

fn mime(p: &Probe) -> &'static str {
    match (p.kind, p.format) {
        (Kind::Photo, Format::Heif) => "image/heic",
        (Kind::Photo, Format::Avif) => "image/avif",
        (Kind::Photo, Format::Tiff) => "image/tiff",
        (Kind::Photo, Format::Bmp) => "image/bmp",
        (Kind::Video, Format::Mov) => "video/quicktime",
        (Kind::Video, Format::ThreeGp) => "video/3gpp",
        (Kind::Video, Format::Matroska) => "video/x-matroska",
        (Kind::Video, Format::Avi) => "video/x-msvideo",
        (Kind::Video, _) => "video/mp4",
        (Kind::Voice, _) => "audio/ogg; codecs=opus",
        (_, Format::Ogg) => "audio/ogg",
        (_, Format::Mp3) => "audio/mpeg",
        (_, Format::Aac) => "audio/aac",
        (_, Format::Wav) => "audio/wav",
        (_, Format::Flac) => "audio/flac",
        (_, Format::Amr) => "audio/amr",
        (_, Format::Matroska) => "audio/webm",
        (_, Format::Mp4 | Format::M4a | Format::Mov | Format::ThreeGp) => "audio/mp4",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_told_apart_by_their_first_bytes() {
        let ftyp = |brands: &[u8]| [&((8 + brands.len()) as u32).to_be_bytes()[..], b"ftyp", brands].concat();
        assert_eq!(sniff(&ftyp(b"heic\0\0\0\0mif1heic")), Format::Heif);
        assert_eq!(sniff(&ftyp(b"mif1\0\0\0\0mif1avif")), Format::Avif);
        assert_eq!(sniff(&ftyp(b"qt  \0\0\0\0qt  ")), Format::Mov);
        assert_eq!(sniff(&ftyp(b"isom\0\0\x02\0isomiso2avc1mp41")), Format::Mp4);
        assert_eq!(sniff(&ftyp(b"M4A \0\0\0\0M4A isom")), Format::M4a);
        assert_eq!(sniff(&ftyp(b"3gp4\0\0\0\0isom3gp4")), Format::ThreeGp);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Format::Webp);
        assert_eq!(sniff(b"OggS\0\x02"), Format::Ogg);
        assert_eq!(sniff(b"%PDF-1.7"), Format::Pdf);
        assert_eq!(sniff(&[0xFF, 0xF1, 0x50]), Format::Aac);
        assert_eq!(sniff(&[0xFF, 0xFB, 0x90]), Format::Mp3);
        assert_eq!(sniff(b"hello"), Format::Other);
    }

    #[test]
    fn a_phones_portrait_video_is_turned_upright() {
        let json = serde_json::json!({
            "streams": [{"index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                         "pix_fmt": "yuv420p", "avg_frame_rate": "30000/1001",
                         "side_data_list": [{"side_data_type": "Display Matrix", "rotation": -90}]},
                        {"index": 1, "codec_type": "audio", "codec_name": "aac", "channels": 2, "sample_rate": "44100"}],
            "format": {"duration": "12.5", "bit_rate": "8000000", "tags": {"major_brand": "qt  ",
                       "com.apple.quicktime.location.ISO6709": "+38.7+009.1/"}}
        });
        let p = from_ffprobe(&json, Format::Mov, 12_500_000).unwrap();
        assert_eq!(
            (p.kind, p.display_dimensions(), p.orientation),
            (Kind::Video, Some((1080, 1920)), Orientation::Rotate90)
        );
        assert_eq!((p.mime.as_str(), p.metadata, p.picture.as_deref()), ("video/quicktime", true, Some("0:0")));
        assert!((p.frame_rate.unwrap() - 29.97).abs() < 0.01);
    }
}
