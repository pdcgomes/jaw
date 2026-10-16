//! Pictures as pixels, upright: decoded in this process where the `image` crate reads the format,
//! by ffmpeg where it doesn't; scaled with `fast_image_resize`; encoded as JPEG, or as lossless
//! PNG where there's transparency to keep.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader, RgbImage, RgbaImage};

use crate::Media;
use crate::error::{Error, Result};
use crate::job::Cancel;
use crate::probe::{Format, Probe};
use crate::run::{Stdout, run};

/// The most pixels decoded in this process; a bigger picture is decoded and scaled down by
/// ffmpeg, so a job here holds at most about four bytes a pixel of this, twice over.
pub(crate) const IN_PROCESS_PIXELS: u64 = 25_000_000;
/// The longest side ffmpeg gives a picture back at, whatever it was asked for.
const FFMPEG_LONGEST: u32 = 8192;

/// Decodes `path`'s picture, upright, scaled down to at most `long_edge` pixels on its longest
/// side. A video's picture is its frame at `at` seconds.
pub(crate) fn decode(
    media: &Media,
    path: &Path,
    probe: &Probe,
    long_edge: u32,
    at: Option<f64>,
    cancel: &Cancel,
) -> Result<DynamicImage> {
    cancel.check()?;
    let pixels = probe.dimensions.map_or(0, |(w, h)| w as u64 * h as u64);
    let image = if probe.decodes_here() && at.is_none() && pixels <= IN_PROCESS_PIXELS {
        decode_here(path, probe)?
    } else if probe.decodes_here() && !media.tools().has_ffmpeg() {
        return Err(Error::TooLarge {
            path: path.into(),
            why: format!("it has {pixels} pixels, and pictures over {IN_PROCESS_PIXELS} need ffmpeg"),
        });
    } else {
        if probe.format == Format::Heif && !media.tools().reads_heic() {
            return Err(Error::Missing { tool: "ffmpeg 7.1 or newer", needed_for: "to read HEIC photos".into() });
        }
        decode_with_ffmpeg(media, path, probe, long_edge.min(FFMPEG_LONGEST), at, cancel)?
    };
    cancel.check()?;
    Ok(fit(image, long_edge))
}

fn decode_here(path: &Path, probe: &Probe) -> Result<DynamicImage> {
    let reader = ImageReader::open(path).map_err(Error::io(path))?.with_guessed_format().map_err(Error::io(path))?;
    let mut decoder = reader.into_decoder().map_err(|e| Error::unreadable(path, e))?;
    let orientation = decoder.orientation().unwrap_or(probe.orientation);
    let image = DynamicImage::from_decoder(decoder).map_err(|e| Error::unreadable(path, e))?;
    let mut image = if image.color().has_alpha() && probe.alpha {
        DynamicImage::ImageRgba8(image.into_rgba8())
    } else {
        DynamicImage::ImageRgb8(image.into_rgb8())
    };
    image.apply_orientation(orientation);
    Ok(image)
}

fn decode_with_ffmpeg(
    media: &Media,
    path: &Path,
    probe: &Probe,
    long_edge: u32,
    at: Option<f64>,
    cancel: &Cancel,
) -> Result<DynamicImage> {
    let ffmpeg = media.tools().ffmpeg("to read this kind of picture")?;
    let input = probe.picture.as_deref().unwrap_or("0:v:0");
    let pixel_format = if probe.alpha { "rgba" } else { "rgb24" };
    let threads = media.config().threads.to_string();
    let mut args: Vec<OsString> = Vec::new();
    for arg in ["-nostdin", "-hide_banner", "-loglevel", "error", "-threads", &threads] {
        args.push(arg.into());
    }
    if let Some(at) = at {
        args.extend(["-ss".into(), format!("{at:.3}").into()]);
    }
    args.extend(["-i".into(), path.into(), "-filter_complex".into()]);
    args.push(
        format!(
            "[{input}]scale=w='min(iw,{long_edge})':h='min(ih,{long_edge})':force_original_aspect_ratio=decrease:\
             flags=bicubic,format={pixel_format}[out]"
        )
        .into(),
    );
    for arg in ["-map", "[out]", "-frames:v", "1", "-c:v", "pam", "-f", "image2pipe", "-"] {
        args.push(arg.into());
    }
    let limit = (long_edge as usize).pow(2) * 4 + 1024;
    let ran = run("ffmpeg", ffmpeg, &args, media.config().nice, path, Stdout::Capture { limit }, cancel)?;
    pam(ran.stdout).ok_or_else(|| Error::unreadable(path, "ffmpeg gave back no picture"))
}

/// A picture from a PAM file, which is a short text header and the pixels.
fn pam(mut data: Vec<u8>) -> Option<DynamicImage> {
    const END: &[u8] = b"ENDHDR\n";
    let end = data[..data.len().min(256)].windows(END.len()).position(|w| w == END)?;
    let header = std::str::from_utf8(&data[..end]).ok()?.to_string();
    let field =
        |name: &str| header.lines().find_map(|l| l.strip_prefix(name)).and_then(|v| v.trim().parse::<u32>().ok());
    let (width, height, depth) = (field("WIDTH ")?, field("HEIGHT ")?, field("DEPTH ")?);
    data.drain(..end + END.len());
    let pixels = data;
    match depth {
        3 => RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8),
        4 => RgbaImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgba8),
        _ => None,
    }
}

/// The size a picture of `width` by `height` is scaled to so its longest side is `long_edge`.
pub(crate) fn scaled(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let longest = width.max(height).max(1) as f64;
    let factor = long_edge as f64 / longest;
    (((width as f64 * factor).round() as u32).max(1), ((height as f64 * factor).round() as u32).max(1))
}

/// The picture scaled down so its longest side is at most `long_edge`; never up.
pub(crate) fn fit(image: DynamicImage, long_edge: u32) -> DynamicImage {
    if image.width().max(image.height()) <= long_edge {
        return image;
    }
    let (w, h) = scaled(image.width(), image.height(), long_edge);
    resize(&image, w, h)
}

/// The picture at exactly `width` by `height`.
pub(crate) fn resize(image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    let mut out = DynamicImage::new(width, height, image.color());
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
    Resizer::new().resize(image, &mut out, &options).expect("the same pixel layout on both sides");
    out
}

/// Encodes the picture as a baseline JPEG at `quality` with its colour at half resolution (4:2:0,
/// as cameras' and WhatsApp's are), flattened onto white if it has transparency. The encoder's own
/// default from quality 90 up is full-resolution colour, which costs 75% more time and bytes.
pub(crate) fn jpeg(image: &DynamicImage, quality: u8, out: &mut impl Write) -> std::io::Result<()> {
    let flattened;
    let rgb = match image {
        DynamicImage::ImageRgb8(rgb) => rgb,
        other => {
            flattened = on_white(other);
            &flattened
        }
    };
    let (width, height) = (rgb.width() as u16, rgb.height() as u16);
    let mut encoder = jpeg_encoder::Encoder::new(out, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    encoder.encode(rgb.as_raw(), width, height, jpeg_encoder::ColorType::Rgb).map_err(std::io::Error::other)
}

fn on_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let over = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

/// Encodes the picture as a PNG, which keeps its transparency.
pub(crate) fn png(image: &DynamicImage, out: &mut impl Write) -> std::io::Result<()> {
    let rgba = image.to_rgba8();
    image::codecs::png::PngEncoder::new(out)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        .map_err(std::io::Error::other)
}
