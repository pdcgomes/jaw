//! What to send for a file, in SD and HD, from its probe alone: one rule for every kind. A copy
//! that already fits its quality's limits in WhatsApp's format goes as it is, or with only its
//! metadata stripped; anything else is encoded again within the limits.

use crate::limits::{HD, Limits, SD, VOICE_BIT_RATE};
use crate::probe::{Format, Kind, Probe};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Sd,
    Hd,
}

impl Quality {
    pub fn limits(self) -> &'static Limits {
        match self {
            Quality::Sd => &SD,
            Quality::Hd => &HD,
        }
    }
}

/// The pool a copy is made in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Nothing encoded: the file as it is, or rewritten without its metadata, which costs about
    /// what reading it does.
    Copy,
    /// Decoded and encoded in this process: the image pool.
    Image,
    /// Encoded by an ffmpeg child: the transcode pool.
    Transcode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// The source file, sent as it is.
    Untouched,
    /// The same picture or streams in WhatsApp's container, without metadata.
    Strip,
    /// A JPEG at most `long_edge` pixels on its longest side.
    Photo { long_edge: u32, quality: u8 },
    /// H.264 in MP4 at `width` by `height`, with AAC sound at `audio_bit_rate` or none.
    Video { width: u32, height: u32, bit_rate: u64, max_bit_rate: u64, frame_rate: f64, audio_bit_rate: Option<u64> },
    /// AAC in MP4.
    Audio { bit_rate: u64 },
    /// Opus in Ogg, in one channel.
    Voice { bit_rate: u64 },
}

/// One copy to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub quality: Quality,
    pub kind: Kind,
    pub action: Action,
    pub mime: String,
    /// The extension for the copy's file.
    pub extension: &'static str,
    /// The copy's width and height as shown.
    pub dimensions: Option<(u32, u32)>,
    /// Whether the copy plays as a GIF, looping without sound: WhatsApp's `gifPlayback`.
    pub gif_playback: bool,
}

impl Plan {
    pub fn lane(&self) -> Lane {
        match self.action {
            Action::Untouched | Action::Strip => Lane::Copy,
            Action::Photo { .. } => Lane::Image,
            Action::Video { .. } | Action::Audio { .. } | Action::Voice { .. } => Lane::Transcode,
        }
    }
}

/// The copies to send: SD always, and HD as its linked second copy when the source has more to
/// give than SD keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct Plans {
    pub sd: Plan,
    pub hd: Option<Plan>,
}

pub fn plan(probe: &Probe) -> Plans {
    let shown = probe.display_dimensions();
    match probe.kind {
        Kind::Photo => {
            let beyond_sd = shown.is_none_or(|(w, h)| w.max(h) > SD.photo_long_edge);
            Plans { sd: photo(probe, Quality::Sd), hd: beyond_sd.then(|| photo(probe, Quality::Hd)) }
        }
        Kind::Video => {
            let beyond_sd = shown.is_none_or(|(w, h)| fit_box(w, h, &SD) < 1.0);
            Plans { sd: video(probe, Quality::Sd), hd: beyond_sd.then(|| video(probe, Quality::Hd)) }
        }
        Kind::Gif => {
            let (width, height) = video_size(shown, &SD);
            let action = Action::Video {
                width,
                height,
                bit_rate: SD.video_bit_rate,
                max_bit_rate: SD.video_max_bit_rate,
                frame_rate: SD.frame_rate,
                audio_bit_rate: None,
            };
            let sd = Plan {
                gif_playback: true,
                ..copy(probe, Quality::Sd, action, "video/mp4", "mp4", Some((width, height)))
            };
            Plans { sd, hd: None }
        }
        Kind::Audio => {
            let aac = matches!(probe.format, Format::M4a | Format::Mp4 | Format::Aac)
                && probe.audio_codec.as_deref() == Some("aac")
                && probe.bit_rate.is_none_or(|b| b <= SD.music_bit_rate * 2);
            let action = if aac { Action::Strip } else { Action::Audio { bit_rate: SD.music_bit_rate } };
            Plans { sd: copy(probe, Quality::Sd, action, "audio/mp4", "m4a", None), hd: None }
        }
        Kind::Voice => {
            let opus = probe.format == Format::Ogg && probe.audio_codec.as_deref() == Some("opus");
            let action = if opus { Action::Untouched } else { Action::Voice { bit_rate: VOICE_BIT_RATE } };
            Plans { sd: copy(probe, Quality::Sd, action, "audio/ogg; codecs=opus", "ogg", None), hd: None }
        }
        Kind::Sticker | Kind::AnimatedSticker | Kind::Document => {
            let extension = if probe.format == Format::Webp { "webp" } else { "" };
            Plans { sd: copy(probe, Quality::Sd, Action::Untouched, &probe.mime, extension, shown), hd: None }
        }
    }
}

fn copy(
    probe: &Probe,
    quality: Quality,
    action: Action,
    mime: &str,
    extension: &'static str,
    dimensions: Option<(u32, u32)>,
) -> Plan {
    Plan { quality, kind: probe.kind, action, mime: mime.to_string(), extension, dimensions, gif_playback: false }
}

fn photo(probe: &Probe, quality: Quality) -> Plan {
    let limits = quality.limits();
    let shown = probe.display_dimensions();
    let fits = probe.format == Format::Jpeg
        && shown.is_some_and(|(w, h)| {
            w.max(h) <= limits.photo_long_edge
                && probe.size as f64 * 8.0 <= limits.photo_bits_per_pixel * w as f64 * h as f64
        });
    if fits {
        let action = if probe.metadata { Action::Strip } else { Action::Untouched };
        return copy(probe, quality, action, "image/jpeg", "jpg", shown);
    }
    let long_edge = shown.map_or(limits.photo_long_edge, |(w, h)| w.max(h).min(limits.photo_long_edge));
    let dimensions = shown.map(|(w, h)| crate::raster::scaled(w, h, long_edge));
    copy(probe, quality, Action::Photo { long_edge, quality: limits.photo_quality }, "image/jpeg", "jpg", dimensions)
}

fn video(probe: &Probe, quality: Quality) -> Plan {
    let limits = quality.limits();
    let shown = probe.display_dimensions();
    let audio_bit_rate = probe.audio_codec.is_some().then_some(limits.audio_bit_rate);
    let fits = probe.format.is_iso_media()
        && probe.video_codec.as_deref() == Some("h264")
        && matches!(probe.pixel_format.as_deref(), Some("yuv420p" | "yuvj420p"))
        && matches!(probe.audio_codec.as_deref(), None | Some("aac"))
        && shown.is_some_and(|(w, h)| fit_box(w, h, limits) >= 1.0)
        && probe.frame_rate.is_none_or(|f| f <= limits.frame_rate + 0.5)
        && probe.bit_rate.is_some_and(|b| b <= limits.video_max_bit_rate + audio_bit_rate.unwrap_or(0));
    if fits {
        return copy(probe, quality, Action::Strip, "video/mp4", "mp4", shown);
    }
    let (width, height) = video_size(shown, limits);
    let action = Action::Video {
        width,
        height,
        bit_rate: limits.video_bit_rate,
        max_bit_rate: limits.video_max_bit_rate,
        frame_rate: limits.frame_rate,
        audio_bit_rate,
    };
    copy(probe, quality, action, "video/mp4", "mp4", Some((width, height)))
}

/// How much a picture of `width` by `height` must be scaled by to fit the limits' box; 1 or more
/// if it fits already.
fn fit_box(width: u32, height: u32, limits: &Limits) -> f64 {
    let (long, short) = (width.max(height) as f64, width.min(height).max(1) as f64);
    (limits.video_long_edge as f64 / long).min(limits.video_short_edge as f64 / short)
}

/// A video's size within the limits' box, never larger than it was, in even numbers as H.264's
/// 4:2:0 needs.
fn video_size(shown: Option<(u32, u32)>, limits: &Limits) -> (u32, u32) {
    let (w, h) = shown.unwrap_or((limits.video_long_edge, limits.video_short_edge));
    let factor = fit_box(w, h, limits).min(1.0);
    let even = |v: f64| (v.round() as u32 / 2 * 2).max(2);
    (even(w as f64 * factor), even(h as f64 * factor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::Orientation;

    fn probe(kind: Kind, format: Format, dimensions: (u32, u32), size: u64) -> Probe {
        let mut p = Probe::new(kind, format, "application/octet-stream", size);
        p.dimensions = Some(dimensions);
        p
    }

    #[test]
    fn a_small_jpeg_goes_untouched_and_gets_no_hd_copy() {
        let p = probe(Kind::Photo, Format::Jpeg, (1200, 900), 150_000);
        let plans = plan(&p);
        assert_eq!((plans.sd.action, plans.sd.lane(), plans.hd), (Action::Untouched, Lane::Copy, None));
    }

    #[test]
    fn a_phone_photo_is_shrunk_for_sd_and_only_stripped_for_hd() {
        let mut p = probe(Kind::Photo, Format::Jpeg, (4032, 3024), 3_500_000);
        p.metadata = true;
        p.orientation = Orientation::Rotate90;
        let plans = plan(&p);
        assert_eq!(plans.sd.action, Action::Photo { long_edge: 1600, quality: SD.photo_quality });
        assert_eq!(plans.sd.dimensions, Some((1200, 1600)));
        let hd = plans.hd.unwrap();
        assert_eq!((hd.action, hd.dimensions), (Action::Strip, Some((3024, 4032))));
    }

    #[test]
    fn a_heavy_jpeg_and_a_heic_are_encoded_again_for_hd() {
        let heavy = plan(&probe(Kind::Photo, Format::Jpeg, (4032, 3024), 9_000_000)).hd.unwrap();
        assert_eq!(heavy.action, Action::Photo { long_edge: 4032, quality: HD.photo_quality });
        let heic = plan(&probe(Kind::Photo, Format::Heif, (4032, 3024), 2_000_000)).hd.unwrap();
        assert_eq!((heic.action, heic.mime.as_str()), (Action::Photo { long_edge: 4032, quality: 90 }, "image/jpeg"));
    }

    #[test]
    fn a_video_that_fits_is_only_stripped_and_a_bigger_one_gets_both_copies() {
        let mut small = probe(Kind::Video, Format::Mp4, (848, 480), 1_000_000);
        small.video_codec = Some("h264".into());
        small.audio_codec = Some("aac".into());
        small.pixel_format = Some("yuv420p".into());
        small.frame_rate = Some(30.0);
        small.bit_rate = Some(1_000_000);
        let plans = plan(&small);
        assert_eq!((plans.sd.action, plans.hd), (Action::Strip, None));

        let mut phone = small.clone();
        phone.dimensions = Some((3840, 2160));
        phone.orientation = Orientation::Rotate90;
        phone.video_codec = Some("hevc".into());
        phone.frame_rate = Some(59.94);
        phone.bit_rate = Some(45_000_000);
        let plans = plan(&phone);
        assert_eq!(plans.sd.dimensions, Some((476, 848)));
        assert_eq!(plans.hd.as_ref().unwrap().dimensions, Some((720, 1280)));
        assert!(matches!(
            plans.hd.unwrap().action,
            Action::Video { frame_rate: 30.0, audio_bit_rate: Some(128_000), .. }
        ));
    }

    #[test]
    fn a_gif_becomes_a_silent_mp4_that_loops() {
        let plans = plan(&probe(Kind::Gif, Format::Gif, (481, 271), 2_000_000));
        assert_eq!(
            (plans.sd.mime.as_str(), plans.sd.gif_playback, plans.sd.dimensions),
            ("video/mp4", true, Some((480, 270)))
        );
        assert!(matches!(plans.sd.action, Action::Video { audio_bit_rate: None, .. }));
    }

    #[test]
    fn documents_stickers_and_voice_messages_go_as_they_are() {
        for (kind, format) in [(Kind::Document, Format::Pdf), (Kind::Sticker, Format::Webp), (Kind::Voice, Format::Ogg)]
        {
            let mut p = probe(kind, format, (512, 512), 10_000);
            p.audio_codec = Some("opus".into());
            assert_eq!(plan(&p).sd.action, Action::Untouched, "{kind:?}");
        }
    }
}
