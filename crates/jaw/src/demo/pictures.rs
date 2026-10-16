//! The made-up account's pictures, painted rather than photographed: a lake, a sunset, a room,
//! a beach, a pool, a portrait, a city at night, colour bars and a heart sticker. Each is written
//! once to a temp folder at 768 and 256 pixels on its long side, with a tiny thumbnail, the way
//! jawd keeps a message's previews.

use crate::data::{Media, Previews};
use image::{ImageFormat, Rgba, RgbaImage, imageops};
use std::f32::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A picture: its name, the size it was taken at, and how it's painted.
struct Painting {
    name: &'static str,
    w: u32,
    h: u32,
    paint: fn(f32, f32) -> [u8; 4],
}

const PAINTINGS: [Painting; 9] = [
    Painting { name: "lake", w: 4032, h: 3024, paint: lake },
    Painting { name: "sunset", w: 3024, h: 2016, paint: sunset },
    Painting { name: "room", w: 3024, h: 4032, paint: room },
    Painting { name: "beach", w: 4000, h: 2250, paint: beach },
    Painting { name: "pool", w: 1920, h: 1080, paint: pool },
    Painting { name: "portrait", w: 1536, h: 2048, paint: portrait },
    Painting { name: "city", w: 2250, h: 4000, paint: city },
    Painting { name: "bars", w: 1920, h: 1080, paint: bars },
    Painting { name: "heart", w: 512, h: 512, paint: heart },
];

/// The pictures' folder, painted the first time it's asked for.
pub fn folder() -> PathBuf {
    static READY: OnceLock<PathBuf> = OnceLock::new();
    READY
        .get_or_init(|| {
            let dir = std::env::temp_dir().join("jaw-demo-pictures-v1");
            if !dir.join("ready").exists() {
                let _ = paint_all(&dir);
            }
            if !dir.join("files").join("ready").exists() {
                let _ = write_files(&dir.join("files"));
            }
            dir
        })
        .clone()
}

/// The files the made-up account's people share, small but real, so they open in their apps.
const FILES: &[&str] = &[
    "orcamento-obras-cozinha.pdf",
    "bilhetes-comboio.pdf",
    "topo-sintra.pdf",
    "seguro-escalada-2026.pdf",
    "rota-sintra-cascais.gpx",
];

/// A file's path in the folder, as a row's `path` gives it.
pub fn file(name: &str) -> Option<String> {
    FILES.contains(&name).then(|| format!("files/{name}"))
}

/// A one-page PDF saying `title`.
fn pdf(title: &str) -> Vec<u8> {
    let text = format!("BT /F1 24 Tf 72 720 Td ({title}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for at in offsets {
        out.extend(format!("{at:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    out
}

fn write_files(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for name in FILES {
        let bytes = if name.ends_with(".pdf") {
            pdf(name.trim_end_matches(".pdf"))
        } else {
            b"<?xml version=\"1.0\"?>\n<gpx version=\"1.1\" creator=\"jaw\"><trk><name>Sintra to Cascais</name><trkseg><trkpt lat=\"38.80\" lon=\"-9.39\"/><trkpt lat=\"38.70\" lon=\"-9.42\"/></trkseg></trk></gpx>\n".to_vec()
        };
        let partial = dir.join(format!(".{name}.{}", std::process::id()));
        std::fs::write(&partial, bytes)?;
        std::fs::rename(partial, dir.join(name))?;
    }
    std::fs::write(dir.join("ready"), b"")
}

/// A painted picture's media, as jawd sends it: previews relative to the folder.
pub fn media(name: &str) -> Media {
    let Some(p) = PAINTINGS.iter().find(|p| p.name == name) else { return Media::default() };
    let ext = extension(p);
    Media {
        w: p.w,
        h: p.h,
        previews: Previews { small: Some(format!("{name}-256.{ext}")), large: Some(format!("{name}-768.{ext}")) },
        thumbnail: Some(format!("{name}-thumb.{ext}")),
        ..Media::default()
    }
}

fn extension(p: &Painting) -> &'static str {
    if p.name == "heart" { "png" } else { "jpg" }
}

fn paint_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for p in &PAINTINGS {
        let long = 768.0 / p.w.max(p.h) as f32;
        let (w, h) = ((p.w as f32 * long).round() as u32, (p.h as f32 * long).round() as u32);
        let large = RgbaImage::from_fn(w, h, |x, y| Rgba((p.paint)(x as f32 / w as f32, y as f32 / h as f32)));
        let shrunk = |side: f32| {
            let k = side / 768.0;
            let (sw, sh) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
            imageops::resize(&large, sw, sh, imageops::FilterType::Triangle)
        };
        let ext = extension(p);
        save(dir, &format!("{}-768.{ext}", p.name), &large)?;
        save(dir, &format!("{}-256.{ext}", p.name), &shrunk(256.0))?;
        save(dir, &format!("{}-thumb.{ext}", p.name), &shrunk(32.0))?;
    }
    std::fs::write(dir.join("ready"), b"")
}

/// Writes a picture whole or not at all, so a reader never finds half of one.
fn save(dir: &Path, name: &str, img: &RgbaImage) -> std::io::Result<()> {
    let to = dir.join(name);
    let partial = dir.join(format!(".{name}.{}", std::process::id()));
    let written = if name.ends_with(".png") {
        img.save_with_format(&partial, ImageFormat::Png)
    } else {
        image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save_with_format(&partial, ImageFormat::Jpeg)
    };
    written.map_err(std::io::Error::other)?;
    std::fs::rename(partial, to)
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let c = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t) as u8;
    [c(0), c(1), c(2), 255]
}

fn lake(u: f32, v: f32) -> [u8; 4] {
    let ridge = 0.42 - 0.12 * (u * 9.0).sin().abs() - 0.06 * (u * 23.0 + 1.0).sin();
    let sun = ((u - 0.72).powi(2) + (v - 0.18).powi(2)).sqrt();
    if v < 0.55 {
        if sun < 0.06 {
            return [255, 236, 190, 255];
        }
        if v > ridge {
            return mix([58, 84, 70], [32, 52, 44], (v - ridge) / 0.2);
        }
        return mix([120, 170, 220], [214, 228, 240], v / 0.5);
    }
    let ripple = ((v * 140.0).sin() * 0.5 + 0.5) * 0.15;
    let mut c = mix([70, 110, 150], [22, 48, 72], (v - 0.55) / 0.45 + ripple);
    if (u - 0.72).abs() < 0.04 * (1.0 - (v - 0.55)) && ripple > 0.08 {
        c = [230, 214, 170, 255];
    }
    c
}

fn sunset(u: f32, v: f32) -> [u8; 4] {
    let sun = ((u - 0.5).powi(2) + (v - 0.6).powi(2) * 2.25).sqrt();
    if v < 0.65 {
        if sun < 0.09 {
            return [255, 210, 120, 255];
        }
        let glow = (1.0 - sun * 2.0).max(0.0);
        let sky = mix([62, 40, 98], [242, 122, 72], v / 0.65);
        return mix([sky[0], sky[1], sky[2]], [255, 180, 110], glow * 0.6);
    }
    let streak = (1.0 - (u - 0.5).abs() * 8.0).max(0.0) * ((v * 200.0).sin() * 0.5 + 0.5);
    mix([28, 30, 64], [240, 150, 90], streak * 0.8)
}

fn room(u: f32, v: f32) -> [u8; 4] {
    if v > 0.72 {
        let plank = ((u * 10.0 + (v * 3.0).floor() * 0.5).fract() < 0.03) as u8 as f32;
        return mix([142, 98, 62], [96, 62, 38], plank + (v - 0.72));
    }
    if (0.52..0.86).contains(&u) && (0.14..0.5).contains(&v) {
        let frame = (u - 0.69).abs() < 0.01 || (v - 0.32).abs() < 0.01;
        if frame {
            return [240, 240, 236, 255];
        }
        return mix([110, 172, 230], [190, 220, 245], (v - 0.14) / 0.36);
    }
    if (0.08..0.46).contains(&u) && (0.52..0.72).contains(&v) {
        return mix([170, 64, 60], [120, 40, 40], (v - 0.52) / 0.2);
    }
    mix([236, 222, 196], [206, 190, 160], v + u * 0.2)
}

fn beach(u: f32, v: f32) -> [u8; 4] {
    let shore = 0.62 + 0.08 * (u * 2.0 * PI).sin() + 0.1 * u;
    if v < 0.42 {
        return mix([90, 160, 225], [200, 230, 250], v / 0.42);
    }
    if v < shore {
        let foam = (shore - v) < 0.015;
        if foam {
            return [245, 248, 250, 255];
        }
        return mix([30, 120, 170], [60, 180, 190], (v - 0.42) / 0.3);
    }
    mix([236, 214, 168], [214, 186, 130], (v - shore) * 3.0)
}

fn pool(u: f32, v: f32) -> [u8; 4] {
    let lane = ((v * 6.0).fract() - 0.5).abs() < 0.02;
    if lane {
        return if (u * 20.0).fract() < 0.5 { [220, 60, 60, 255] } else { [240, 240, 240, 255] };
    }
    let caustic = ((u * 31.0 + (v * 17.0).sin() * 2.0).sin() * (v * 29.0 + (u * 13.0).sin() * 2.0).sin()).abs();
    let swimmer = ((u - 0.4).powi(2) * 4.0 + (v - 0.58).powi(2) * 30.0).sqrt() < 0.12;
    if swimmer {
        return [236, 176, 140, 255];
    }
    mix([20, 120, 190], [150, 220, 240], caustic.powi(3))
}

fn portrait(u: f32, v: f32) -> [u8; 4] {
    let (x, y) = (u - 0.5, v - 0.44);
    let face = (x * x / 0.05 + y * y / 0.07) < 1.0;
    let hair = (x * x / 0.07 + (y + 0.05).powi(2) / 0.09) < 1.0 && (v < 0.36 || x.abs() > 0.19);
    let eye = ((x.abs() - 0.08).powi(2) + (y + 0.03).powi(2)).sqrt() < 0.022;
    let shoulders = v > 0.78 && x.abs() < 0.42 - (1.0 - v) * 0.4;
    if eye {
        return [40, 30, 30, 255];
    }
    if hair {
        return mix([60, 38, 28], [34, 22, 18], v);
    }
    if face {
        return mix([238, 196, 164], [214, 166, 132], (x + 0.2) * 2.0);
    }
    if shoulders {
        return mix([46, 96, 120], [30, 64, 84], v);
    }
    mix([92, 160, 156], [40, 92, 100], v)
}

fn city(u: f32, v: f32) -> [u8; 4] {
    let column = (u * 9.0).floor();
    let top = 0.35 + 0.35 * ((column * 12.9898).sin() * 43758.547).fract();
    if v > top {
        let lit = ((u * 90.0).floor() * 7.3 + (v * 120.0).floor() * 3.1).sin() > 0.55;
        let window = (u * 90.0).fract() < 0.5 && (v * 120.0).fract() < 0.5;
        if window && lit {
            return [250, 210, 120, 255];
        }
        return mix([28, 30, 48], [16, 18, 30], column / 9.0);
    }
    let star = ((u * 400.0).floor() * 12.9 + (v * 400.0).floor() * 78.2).sin() > 0.997;
    if star {
        return [240, 240, 255, 255];
    }
    mix([14, 18, 46], [64, 44, 96], v / top)
}

fn bars(u: f32, v: f32) -> [u8; 4] {
    const BARS: [[u8; 3]; 7] =
        [[192, 192, 192], [192, 192, 0], [0, 192, 192], [0, 192, 0], [192, 0, 192], [192, 0, 0], [0, 0, 192]];
    if v < 0.7 {
        let [r, g, b] = BARS[((u * 7.0) as usize).min(6)];
        return [r, g, b, 255];
    }
    if v < 0.78 {
        let [r, g, b] = BARS[6 - ((u * 7.0) as usize).min(6)];
        return [r / 2, g / 2, b / 2, 255];
    }
    let grey = (u * 255.0) as u8;
    [grey, grey, grey, 255]
}

fn heart(u: f32, v: f32) -> [u8; 4] {
    let (x, y) = ((u - 0.5) * 2.6, (0.45 - v) * 2.6);
    let f = |x: f32, y: f32| (x * x + y * y - 1.0).powi(3) - x * x * y.powi(3);
    if f(x, y) <= 0.0 {
        return mix([236, 72, 96], [180, 30, 60], v + x * 0.2);
    }
    if f(x * 0.9, y * 0.9 + 0.02) <= 0.0 {
        return [255, 255, 255, 255];
    }
    [0, 0, 0, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_is_written_at_both_sizes_with_a_thumbnail() {
        let dir = folder();
        for p in &PAINTINGS {
            let m = media(p.name);
            for path in [m.previews.small, m.previews.large, m.thumbnail].into_iter().flatten() {
                let img = image::open(dir.join(&path)).expect("the preview is a picture");
                let long = img.width().max(img.height());
                assert!(
                    [256, 768, 32].contains(&long),
                    "{path} is {} × {}, not one of the preview sizes",
                    img.width(),
                    img.height()
                );
            }
        }
    }

    #[test]
    fn the_sticker_is_transparent_around_the_heart() {
        let img = image::open(folder().join(media("heart").previews.large.unwrap())).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(2, 2)[3], 0);
        assert_eq!(img.get_pixel(img.width() / 2, img.height() / 2)[3], 255);
    }
}
