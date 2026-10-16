//! What jaw sends at each quality, in one table. Each number says where it came from. None has
//! been measured against what today's phone apps send yet: they're to be measured before they're
//! relied on (the design's "SD and HD sizes are measured from what the phone app sends"), and
//! those marked as estimates are the first to check.
//!
//! HD goes as WhatsApp's dual upload: the SD copy first, then the HD copy linked to it, so a phone
//! that wants less data shows the SD one. A file within SD's limits has no HD copy, as on phones,
//! which offer HD only for what SD would shrink.

/// One quality's limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// A photo's longest side, in pixels.
    pub photo_long_edge: u32,
    /// The JPEG quality a photo is encoded at, from 1 to 100.
    pub photo_quality: u8,
    /// The most a JPEG may spend a pixel, in bits, and still go untouched: a photo already
    /// compressed about as hard as this quality's isn't compressed again.
    pub photo_bits_per_pixel: f64,
    /// A video's longest and shortest sides, in pixels, once it's upright.
    pub video_long_edge: u32,
    pub video_short_edge: u32,
    /// A video's average bit rate and its peak, in bits a second; a file over the peak, sound
    /// included, is encoded again.
    pub video_bit_rate: u64,
    pub video_max_bit_rate: u64,
    /// The most frames a second a video keeps.
    pub frame_rate: f64,
    /// The bit rate of a video's sound, AAC, in bits a second.
    pub audio_bit_rate: u64,
    /// The bit rate of an audio file encoded as AAC, in bits a second.
    pub music_bit_rate: u64,
}

/// Standard quality, WhatsApp's default.
pub const SD: Limits = Limits {
    // 1,600 pixels: commonly reported for standard photos from WhatsApp's phone apps (a 4:3
    // camera's photo arrives as 1600 by 1200).
    photo_long_edge: 1600,
    // 80: mautrix-whatsapp's bridge encodes the JPEGs it sends at 80 (pkg/msgconv/from-matrix.go);
    // WhatsApp doesn't publish its own. An estimate.
    photo_quality: 80,
    // jaw's own: a quality-80 JPEG of a phone photo spends about 1 to 2 bits a pixel.
    photo_bits_per_pixel: 2.0,
    // 480p: commonly reported for standard videos since before HD video came in 2023.
    video_long_edge: 848,
    video_short_edge: 480,
    // 1.2 Mbit/s, peaking at 1.8: an estimate for 480p H.264.
    video_bit_rate: 1_200_000,
    video_max_bit_rate: 1_800_000,
    // 30: an estimate; phones' 60 fps videos are taken to arrive at 30.
    frame_rate: 30.0,
    // 96 kbit/s AAC-LC: an estimate.
    audio_bit_rate: 96_000,
    // 128 kbit/s: an estimate; audio files that are AAC already go as they are.
    music_bit_rate: 128_000,
};

/// HD, the dual upload's second copy.
pub const HD: Limits = Limits {
    // 4,096 pixels: WABetaInfo (2023) says HD "preserves image dimensions, but light compression
    // is still applied"; 4096 keeps a 12-megapixel phone photo (4032 by 3024) whole and caps
    // larger ones. The cap is an estimate.
    photo_long_edge: 4096,
    // 90: WABetaInfo describes HD's compression as "lighter, keeping about 80% of the original
    // quality"; 90 is jaw's reading of that. An estimate.
    photo_quality: 90,
    // jaw's own: a phone camera's JPEG spends about 2 to 4 bits a pixel, so most go untouched.
    photo_bits_per_pixel: 4.0,
    // 720p: reported as HD video's size when it came in August 2023.
    video_long_edge: 1280,
    video_short_edge: 720,
    // 3 Mbit/s, peaking at 4.5: an estimate for 720p H.264.
    video_bit_rate: 3_000_000,
    video_max_bit_rate: 4_500_000,
    frame_rate: 30.0,
    // 128 kbit/s AAC-LC: an estimate.
    audio_bit_rate: 128_000,
    music_bit_rate: 128_000,
};

/// The JPEG thumbnail every photo and video message carries, shown before anything downloads.
/// Baileys, a WhatsApp Web library, sends 32 pixels wide (src/Utils/messages-media.ts); 100 on the
/// longest side is an estimate of what phones send.
pub const THUMBNAIL_LONG_EDGE: u32 = 100;
pub const THUMBNAIL_QUALITY: u8 = 60;

/// A voice message's bit rate when one has to be encoded again: Opus in one channel. An estimate.
pub(crate) const VOICE_BIT_RATE: u64 = 32_000;
