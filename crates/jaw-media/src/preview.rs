//! Display-ready previews, so no interface decodes full-size media to show a list: every picture,
//! sticker, GIF and video at SMALL and LARGE pixels on its longest side, upright; the thumbnail a
//! message carries, scaled up to stand in until then; and an icon by type for what has no picture.
//!
//! Previews are JPEGs, which the `image` crate encodes and decodes fastest and every interface can
//! draw, except where there's transparency to keep (stickers, mostly), which go as PNG, as the
//! protocol says and as kitty's graphics take them without decoding.

use std::path::{Path, PathBuf};

use image::DynamicImage;

use crate::Media;
use crate::error::{Error, Result};
use crate::file::{Claim, Part};
use crate::job::Cancel;
use crate::plan::Lane;
use crate::probe::{Format, Kind, Probe};
use crate::raster;

/// The longest side of the preview a list shows, in pixels.
pub const SMALL: u32 = 256;
/// The longest side of the preview an open message shows, in pixels.
pub const LARGE: u32 = 768;
const QUALITY: u8 = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: PreviewFormat,
    /// Bytes on disk.
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Previews {
    /// The picture at both sizes; a picture smaller than LARGE is kept at its own size, never
    /// scaled up.
    Pictures { small: Preview, large: Preview },
    /// No picture to show: the interface draws this type's icon.
    Icon(DocumentType),
}

/// What a file without a picture is, for its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentType {
    Pdf,
    Text,
    Word,
    Spreadsheet,
    Presentation,
    Archive,
    Audio,
    Video,
    Image,
    Contact,
    Calendar,
    App,
    Other,
}

pub fn document_type(mime: &str) -> DocumentType {
    let mime = mime.split(';').next().unwrap_or_default().trim();
    match mime {
        "application/pdf" => DocumentType::Pdf,
        "text/vcard" | "text/x-vcard" => DocumentType::Contact,
        "text/calendar" => DocumentType::Calendar,
        "application/vnd.android.package-archive" => DocumentType::App,
        "application/zip"
        | "application/gzip"
        | "application/x-7z-compressed"
        | "application/vnd.rar"
        | "application/x-tar" => DocumentType::Archive,
        "application/rtf" | "application/json" => DocumentType::Text,
        _ if mime.contains("wordprocessing") || mime == "application/msword" || mime.ends_with(".text") => {
            DocumentType::Word
        }
        _ if mime.contains("spreadsheet") || mime.contains("excel") || mime.ends_with(".numbers") => {
            DocumentType::Spreadsheet
        }
        _ if mime.contains("presentation") || mime.contains("powerpoint") || mime.ends_with(".keynote") => {
            DocumentType::Presentation
        }
        _ if mime.ends_with(".pages") => DocumentType::Word,
        _ if mime.starts_with("text/") => DocumentType::Text,
        _ if mime.starts_with("audio/") => DocumentType::Audio,
        _ if mime.starts_with("video/") => DocumentType::Video,
        _ if mime.starts_with("image/") => DocumentType::Image,
        _ => DocumentType::Other,
    }
}

/// Whether a file has a picture to preview: anything visual, even when it was sent as a document.
fn has_picture(probe: &Probe) -> bool {
    let visual = matches!(
        probe.format,
        Format::Jpeg
            | Format::Png
            | Format::Gif
            | Format::Webp
            | Format::Heif
            | Format::Avif
            | Format::Tiff
            | Format::Bmp
    ) || probe.video_codec.is_some();
    visual && !matches!(probe.kind, Kind::Audio | Kind::Voice)
}

/// The pool a file's previews are made in: the image pool for anything with a picture, ffmpeg's
/// posters included, since each is one frame; none for an icon.
pub fn previews_lane(probe: &Probe) -> Lane {
    if has_picture(probe) { Lane::Image } else { Lane::Copy }
}

/// Where in a video its poster is taken from, in seconds: a moment in, past a fade from black.
fn poster_at(probe: &Probe) -> Option<f64> {
    (probe.kind == Kind::Video).then(|| probe.duration.map_or(0.0, |d| (d.as_secs_f64() * 0.1).min(1.0)))
}

pub(crate) fn previews(media: &Media, source: &Path, probe: &Probe, stem: &Path, cancel: &Cancel) -> Result<Previews> {
    if !has_picture(probe) {
        return Ok(Previews::Icon(document_type(&probe.mime)));
    }
    let names: Vec<PathBuf> =
        ["768", "256"].iter().flat_map(|label| [name(stem, label, false), name(stem, label, true)]).collect();
    let _claim = Claim::take(names.iter().map(PathBuf::as_path))?;
    let large = raster::decode(media, source, probe, LARGE, poster_at(probe), cancel)?;
    let small = raster::fit(large.clone(), SMALL);
    let alpha = large.color().has_alpha();
    let large = write(&large, stem, "768", alpha)?;
    // A failed or cancelled call leaves nothing, and the large preview's name is this call's own.
    match cancel.check().and_then(|()| write(&small, stem, "256", alpha)) {
        Ok(small) => Ok(Previews::Pictures { small, large }),
        Err(e) => {
            let _ = std::fs::remove_file(&large.path);
            Err(e)
        }
    }
}

/// A preview's name beside `stem`: `<stem>.<label>.jpg`, or `.png` when it keeps transparency.
fn name(stem: &Path, label: &str, alpha: bool) -> PathBuf {
    let mut name = stem.as_os_str().to_owned();
    name.push(format!(".{label}.{}", if alpha { "png" } else { "jpg" }));
    PathBuf::from(name)
}

fn write(image: &DynamicImage, stem: &Path, label: &str, alpha: bool) -> Result<Preview> {
    write_to(image, &name(stem, label, alpha), alpha)
}

/// Writes `image` to `path`, as PNG if `alpha`, as JPEG otherwise.
pub(crate) fn write_to(image: &DynamicImage, path: &Path, alpha: bool) -> Result<Preview> {
    let format = if alpha { PreviewFormat::Png } else { PreviewFormat::Jpeg };
    let path = path.to_path_buf();
    let part = Part::create(&path)?;
    let written = part.path().to_path_buf();
    let size = part.write(|out| {
        if alpha { raster::png(image, out) } else { raster::jpeg(image, QUALITY, out) }.map_err(Error::io(&written))
    })?;
    Ok(Preview { path, width: image.width(), height: image.height(), format, size })
}

/// Writes the thumbnail a message carries (a small JPEG), scaled up to SMALL and softened so its
/// blocks don't show, as `<stem>.instant.jpg`: what's shown until the file itself is downloaded.
pub fn instant_preview(thumbnail: &[u8], stem: &Path) -> Result<Preview> {
    let path = name(stem, "instant", false);
    let _claim = Claim::take([path.as_path()])?;
    let image = image::load_from_memory(thumbnail).map_err(|e| Error::unreadable(stem, e))?;
    let image = DynamicImage::ImageRgb8(image.into_rgb8());
    let (w, h) = (image.width(), image.height());
    let image = if w.max(h) < SMALL {
        let (sw, sh) = raster::scaled(w, h, SMALL);
        let sigma = sw as f32 / w as f32 / 2.0;
        raster::resize(&image, sw, sh).blur(sigma)
    } else {
        raster::fit(image, SMALL)
    };
    write_to(&image, &path, false)
}
