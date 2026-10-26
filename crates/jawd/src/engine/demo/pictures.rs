//! The demo's photos: pictures painted here and encoded as a phone's camera would, each with the
//! thumbnail its message carries, as the made-up media server holds them.

use std::sync::{Arc, OnceLock};

use image::codecs::jpeg::JpegEncoder;
use image::{Rgb, RgbImage};
use sha2::{Digest, Sha256};

/// A photo the made-up server holds: its JPEG, its hash, where the server has it, and its thumbnail.
#[derive(Clone, Debug)]
pub struct Picture {
    pub data: Arc<Vec<u8>>,
    pub sha256: Vec<u8>,
    pub direct_path: String,
    pub thumbnail: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Picture {
    /// A lake under hills at `width` by `height`, smooth, as a phone's photo of it compressed.
    pub fn lake(width: u32, height: u32) -> Picture {
        static LAKES: OnceLock<std::sync::Mutex<Vec<Picture>>> = OnceLock::new();
        let lakes = LAKES.get_or_init(Default::default);
        if let Some(lake) = lakes.lock().expect("lakes").iter().find(|p| (p.width, p.height) == (width, height)) {
            return lake.clone();
        }
        let lake = Picture::painted(width, height, 0);
        lakes.lock().expect("lakes").push(lake.clone());
        lake
    }

    /// The lake with a camera's noise in it, so it costs to decode what a phone's photo does (a
    /// 12-megapixel one is about 4 MB); each `n` gives another file.
    pub fn numbered(width: u32, height: u32, n: u32) -> Picture {
        Picture::painted_with(width, height, 24, n)
    }

    fn painted(width: u32, height: u32, noise: u32) -> Picture {
        Picture::painted_with(width, height, noise, 0)
    }

    fn painted_with(width: u32, height: u32, noise: u32, n: u32) -> Picture {
        let mut seed = 0x2545_F491_u32 ^ n.wrapping_mul(0x9E37_79B9);
        let mut grain = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            if noise == 0 { 0 } else { (seed % (2 * noise + 1)) as i32 - noise as i32 }
        };
        let picture = RgbImage::from_fn(width, height, |x, y| {
            let (fx, fy) = (x as f32 / width as f32, y as f32 / height as f32);
            let hills = 0.45 + 0.08 * (fx * 9.0).sin() + 0.04 * (fx * 23.0).cos();
            let base = if fy < hills - 0.1 {
                [(120.0 + 100.0 * fy) as i32, (170.0 + 60.0 * fy) as i32, 235]
            } else if fy < hills {
                [70, (110.0 + 40.0 * fx) as i32, 60]
            } else {
                let ripple = 12.0 * ((fy * 140.0).sin() * (fx * 30.0).cos());
                [(30.0 + ripple) as i32, (90.0 + 50.0 * fy + ripple) as i32, (150.0 + 40.0 * fy) as i32]
            };
            let g = grain();
            Rgb(base.map(|c| (c + g).clamp(0, 255) as u8))
        });
        let data = jpeg(&picture, 88);
        let long = width.max(height) as f32;
        let thumb = image::imageops::thumbnail(
            &picture,
            (width as f32 * 100.0 / long) as u32,
            (height as f32 * 100.0 / long) as u32,
        );
        let sha256 = Sha256::digest(&data).to_vec();
        let direct_path = format!("/v/demo/{}.enc?ccb=11-4", hex(&sha256[..8]));
        Picture { data: Arc::new(data), sha256, direct_path, thumbnail: jpeg(&thumb, 60), width, height }
    }
}

fn jpeg(picture: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(picture).expect("a painted picture encodes");
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
