//! The pipeline end to end on files made here: ffmpeg's test patterns and tones, pictures drawn in
//! the test, and two small checked-in fixtures ffmpeg can't make (a HEIC and an animated WebP).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::{DynamicImage, GenericImageView, Rgb, RgbImage, Rgba, RgbaImage};
use jaw_media::{
    Action, Cancel, Config, Destination, DocumentType, Error, Format, HD, Job, Kind, Lane, Media, Orientation,
    PreviewFormat, Previews, Probe, Progress, Quality, SD, VideoEncoder, instant_preview, plan, previews_lane,
};
use tempfile::TempDir;

fn media() -> Media {
    media_with(Config::default())
}

fn media_with(config: Config) -> Media {
    let media = Media::new(config);
    assert!(
        media.tools().has_ffmpeg(),
        "these tests need ffmpeg and ffprobe: brew install ffmpeg, or ffmpeg 7.1 on Linux"
    );
    media
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// Runs ffmpeg to make a test file.
fn make(args: &[&str], to: &Path) -> PathBuf {
    let out = Command::new(media().tools().ffmpeg_path().unwrap())
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(to)
        .output()
        .expect("ffmpeg runs");
    assert!(out.status.success(), "ffmpeg couldn't make {}: {}", to.display(), String::from_utf8_lossy(&out.stderr));
    to.to_path_buf()
}

/// A 1080p video with a tone, `seconds` long.
fn video(dir: &TempDir, name: &str, size: &str, seconds: u32, extra: &[&str]) -> PathBuf {
    let source = format!("testsrc2=s={size}:r=30:d={seconds}");
    let tone = format!("sine=f=440:d={seconds}");
    let mut args = vec!["-f", "lavfi", "-i", &source, "-f", "lavfi", "-i", &tone];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"]);
    args.extend_from_slice(&["-metadata", "title=at home", "-metadata", "location=+38.7+009.1/"]);
    make(&args, &dir.path().join(name))
}

/// A JPEG of `width` by `height`, left half red and right half blue, with EXIF saying `orientation`
/// and naming the camera, or no EXIF for orientation 0.
fn jpeg(dir: &TempDir, name: &str, width: u32, height: u32, orientation: u8, noise: bool) -> PathBuf {
    let mut seed = 1u32;
    let picture = RgbImage::from_fn(width, height, |x, _| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let n = if noise { (seed >> 24) as u8 / 4 } else { 0 };
        if x < width / 2 { Rgb([220, 20 + n, 20]) } else { Rgb([20, 20 + n, 220]) }
    });
    let mut data = Vec::new();
    jpeg_encoder::Encoder::new(&mut data, 92)
        .encode(picture.as_raw(), width as u16, height as u16, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    let body: Vec<u8> = [
        &b"Exif\0\0MM\0\x2A\0\0\0\x08\0\x02"[..],
        &[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0],
        &[0x01, 0x0F, 0, 2, 0, 0, 0, 4, b'j', b'a', b'w', 0],
        &[0, 0, 0, 0],
    ]
    .concat();
    let exif = [&[0xFF, 0xE1][..], &((body.len() + 2) as u16).to_be_bytes(), &body].concat();
    let path = dir.path().join(name);
    let exif: &[u8] = if orientation == 0 { &[] } else { &exif };
    std::fs::write(&path, [&data[..2], exif, &data[2..]].concat()).unwrap();
    path
}

fn probe_of(path: &Path) -> Probe {
    media().probe(path).unwrap_or_else(|e| panic!("{e}"))
}

fn ffprobe(path: &Path) -> serde_json::Value {
    let out = Command::new(media().tools().ffprobe_path().unwrap())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

fn prepare(
    media: &Media,
    source: &Path,
    probe: &Probe,
    plan: &jaw_media::Plan,
    to: &Destination,
) -> jaw_media::Prepared {
    media
        .prepare(source, probe, &[(plan, to)], &mut Job::new(Cancel::new()))
        .unwrap_or_else(|e| panic!("{e}"))
        .remove(0)
}

#[test]
fn each_kind_is_told_from_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let photo = jpeg(&dir, "photo.jpg", 400, 200, 6, false);
    let png = dir.path().join("cutout.png");
    RgbaImage::from_pixel(300, 100, Rgba([0, 0, 0, 0])).save(&png).unwrap();
    let sticker = dir.path().join("sticker.webp");
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(512, 512, Rgba([255, 0, 0, 128]))).save(&sticker).unwrap();
    let gif = make(&["-f", "lavfi", "-i", "testsrc2=s=160x120:r=10:d=2"], &dir.path().join("loop.gif"));
    let film = video(&dir, "film.mp4", "640x360", 2, &[]);
    let song =
        make(&["-f", "lavfi", "-i", "sine=f=440:d=3", "-c:a", "aac", "-b:a", "128k"], &dir.path().join("song.m4a"));
    let voice =
        make(&["-f", "lavfi", "-i", "sine=f=300:d=2", "-ac", "1", "-c:a", "libopus"], &dir.path().join("voice.ogg"));
    let pdf = dir.path().join("tickets.pdf");
    std::fs::write(&pdf, b"%PDF-1.4\n%%EOF\n").unwrap();

    let p = probe_of(&photo);
    assert_eq!(
        (p.kind, p.dimensions, p.orientation, p.display_dimensions()),
        (Kind::Photo, Some((400, 200)), Orientation::Rotate90, Some((200, 400)))
    );
    assert!(p.metadata);
    let p = probe_of(&png);
    assert_eq!((p.kind, p.alpha, p.mime.as_str()), (Kind::Photo, true, "image/png"));
    let p = probe_of(&fixture("photo.heic"));
    assert_eq!(
        (p.kind, p.format, p.dimensions, p.mime.as_str()),
        (Kind::Photo, Format::Heif, Some((1024, 640)), "image/heic")
    );
    let p = probe_of(&sticker);
    assert_eq!((p.kind, p.alpha, p.animated), (Kind::Sticker, true, false));
    let p = probe_of(&fixture("animated-sticker.webp"));
    assert_eq!((p.kind, p.frames, p.duration), (Kind::AnimatedSticker, Some(3), Some(Duration::from_millis(500))));
    let p = probe_of(&gif);
    assert_eq!(
        (p.kind, p.frames, p.dimensions, p.duration),
        (Kind::Gif, Some(20), Some((160, 120)), Some(Duration::from_secs(2)))
    );
    let p = probe_of(&film);
    assert_eq!(
        (p.kind, p.dimensions, p.video_codec.as_deref(), p.audio_codec.as_deref()),
        (Kind::Video, Some((640, 360)), Some("h264"), Some("aac"))
    );
    assert!((p.frame_rate.unwrap() - 30.0).abs() < 0.01 && p.bit_rate.is_some() && p.metadata);
    assert!((p.duration.unwrap().as_secs_f64() - 2.0).abs() < 0.1);
    let p = probe_of(&song);
    assert_eq!((p.kind, p.format, p.mime.as_str()), (Kind::Audio, Format::M4a, "audio/mp4"));
    let p = probe_of(&voice);
    assert_eq!((p.kind, p.channels, p.mime.as_str()), (Kind::Voice, Some(1), "audio/ogg; codecs=opus"));
    let p = probe_of(&pdf);
    assert_eq!((p.kind, p.mime.as_str()), (Kind::Document, "application/pdf"));
}

#[test]
fn a_photos_previews_are_upright_and_never_larger_than_asked_or_than_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let photo = jpeg(&dir, "turned.jpg", 2000, 1000, 6, false);
    let probe = probe_of(&photo);
    assert_eq!(previews_lane(&probe), Lane::Image);
    let Previews::Pictures { small, large } =
        media().previews(&photo, &probe, &dir.path().join("p"), &Cancel::new()).unwrap()
    else {
        panic!("a photo has pictures")
    };
    assert_eq!((large.width, large.height, small.width, small.height), (384, 768, 128, 256));
    assert_eq!((large.format, large.path.file_name().unwrap().to_str()), (PreviewFormat::Jpeg, Some("p.768.jpg")));
    let shown = image::open(&large.path).unwrap();
    let (top, bottom) = (shown.get_pixel(192, 100).0, shown.get_pixel(192, 668).0);
    assert!(top[0] > 150 && top[2] < 100, "turned clockwise, the left half is on top: {top:?}");
    assert!(bottom[2] > 150 && bottom[0] < 100, "and the right half below: {bottom:?}");
    assert!(large.size < 60_000 && small.size < 15_000, "{} and {} bytes", large.size, small.size);

    let tiny = jpeg(&dir, "tiny.jpg", 100, 80, 1, false);
    let Previews::Pictures { small, large } =
        media().previews(&tiny, &probe_of(&tiny), &dir.path().join("t"), &Cancel::new()).unwrap()
    else {
        panic!("a photo has pictures")
    };
    assert_eq!((large.width, large.height, small.width, small.height), (100, 80, 100, 80));
}

#[test]
fn heic_stickers_gifs_and_videos_get_previews_and_documents_an_icon() {
    let dir = tempfile::tempdir().unwrap();
    let media = media();
    let preview = |path: &Path, name: &str| {
        let probe = probe_of(path);
        match media.previews(path, &probe, &dir.path().join(name), &Cancel::new()).unwrap_or_else(|e| panic!("{e}")) {
            Previews::Pictures { small, large } => (small, large),
            Previews::Icon(icon) => panic!("{name} got an icon: {icon:?}"),
        }
    };
    let (small, large) = preview(&fixture("photo.heic"), "heic");
    assert_eq!((large.width, large.height, small.width, small.height), (768, 480, 256, 160));

    let (small, large) = preview(&fixture("animated-sticker.webp"), "sticker");
    assert_eq!((large.width, large.format, small.width), (512, PreviewFormat::Png, 256));
    let first = image::open(&large.path).unwrap();
    assert_eq!(first.get_pixel(10, 10).0[3], 0, "a sticker keeps its transparency");
    assert!(first.get_pixel(256, 256).0[0] > 200, "and shows its first frame, the red one");

    let gif = make(&["-f", "lavfi", "-i", "testsrc2=s=320x240:r=10:d=1"], &dir.path().join("loop.gif"));
    let (_, large) = preview(&gif, "gif");
    assert_eq!((large.width, large.height), (320, 240));

    let portrait = video(&dir, "portrait.mp4", "1280x720", 2, &[]);
    let turned = dir.path().join("turned.mp4");
    make(&["-display_rotation", "90", "-i", portrait.to_str().unwrap(), "-c", "copy"], &turned);
    let probe = probe_of(&turned);
    assert_eq!((probe.orientation, probe.display_dimensions()), (Orientation::Rotate270, Some((720, 1280))));
    let (small, large) = preview(&turned, "video");
    assert_eq!((large.width, large.height, small.width, small.height), (432, 768, 144, 256));

    let pdf = dir.path().join("tickets.pdf");
    std::fs::write(&pdf, b"%PDF-1.4\n%%EOF\n").unwrap();
    let probe = probe_of(&pdf);
    assert_eq!(previews_lane(&probe), Lane::Copy);
    assert_eq!(
        media.previews(&pdf, &probe, &dir.path().join("pdf"), &Cancel::new()).unwrap(),
        Previews::Icon(DocumentType::Pdf)
    );
}

#[test]
fn a_messages_thumbnail_is_scaled_up_to_stand_in_for_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    let mut thumbnail = Vec::new();
    jpeg_encoder::Encoder::new(&mut thumbnail, 50)
        .encode(&RgbImage::from_pixel(32, 24, Rgb([10, 200, 10])).into_raw(), 32, 24, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    let preview = instant_preview(&thumbnail, &dir.path().join("m")).unwrap();
    assert_eq!((preview.width, preview.height, preview.format), (256, 192, PreviewFormat::Jpeg));
    assert!(image::open(&preview.path).unwrap().get_pixel(128, 96).0[1] > 150);
}

#[test]
fn a_file_that_already_fits_goes_untouched_or_only_stripped() {
    let dir = tempfile::tempdir().unwrap();
    let media = media();

    let small = jpeg(&dir, "small.jpg", 1200, 900, 0, false);
    let probe = probe_of(&small);
    let plans = plan(&probe);
    assert_eq!((plans.sd.action, plans.hd.is_none()), (Action::Untouched, true));
    let sent = prepare(&media, &small, &probe, &plans.sd, &Destination::new(dir.path().join("sd.jpg")));
    assert_eq!((sent.path.as_path(), sent.untouched), (small.as_path(), true));
    assert!(!dir.path().join("sd.jpg").exists());
    let thumbnail = image::load_from_memory(sent.thumbnail.as_ref().unwrap()).unwrap();
    assert_eq!((thumbnail.width(), thumbnail.height()), (100, 75));

    let tagged = jpeg(&dir, "tagged.jpg", 1200, 900, 8, false);
    let probe = probe_of(&tagged);
    let plans = plan(&probe);
    assert_eq!(plans.sd.action, Action::Strip);
    let sent = prepare(&media, &tagged, &probe, &plans.sd, &Destination::new(dir.path().join("stripped.jpg")));
    let stripped = std::fs::read(&sent.path).unwrap();
    assert!(!stripped.windows(3).any(|w| w == b"jaw"), "the EXIF's other tags are gone");
    assert_eq!(probe_of(&sent.path).orientation, Orientation::Rotate270, "the orientation stays");

    let film = video(&dir, "film.mp4", "848x480", 2, &["-b:v", "800k"]);
    let probe = probe_of(&film);
    let plans = plan(&probe);
    assert_eq!((plans.sd.action, plans.sd.lane(), plans.hd.is_none()), (Action::Strip, Lane::Copy, true));
    let to = Destination::with_poster(dir.path().join("film-sd.mp4"), dir.path().join("film-sd.jpg"));
    let sent = prepare(&media, &film, &probe, &plans.sd, &to);
    let out = ffprobe(&sent.path);
    let tags = out["format"]["tags"].as_object().cloned().unwrap_or_default();
    assert!(!tags.contains_key("title") && !tags.contains_key("location"), "metadata stripped: {tags:?}");
    let before = ffprobe(&film);
    for (i, stream) in out["streams"].as_array().unwrap().iter().enumerate() {
        assert_eq!(stream["codec_name"], before["streams"][i]["codec_name"]);
        assert_eq!(stream["nb_frames"], before["streams"][i]["nb_frames"], "the streams are copied as they were");
    }
    assert_eq!(sent.poster.as_ref().map(|p| (p.width, p.height)), Some((848, 480)));
    assert!(sent.thumbnail.is_some() && (sent.duration.unwrap().as_secs_f64() - 2.0).abs() < 0.1);
}

#[test]
fn a_turned_video_keeps_its_orientation_when_stripped() {
    let dir = tempfile::tempdir().unwrap();
    let film = video(&dir, "film.mp4", "848x480", 1, &["-b:v", "600k"]);
    let turned =
        make(&["-display_rotation", "-90", "-i", film.to_str().unwrap(), "-c", "copy"], &dir.path().join("turned.mp4"));
    let probe = probe_of(&turned);
    let plans = plan(&probe);
    assert_eq!(plans.sd.action, Action::Strip);
    let sent = prepare(&media(), &turned, &probe, &plans.sd, &Destination::new(dir.path().join("sent.mp4")));
    let after = probe_of(&sent.path);
    assert_eq!((after.orientation, after.display_dimensions()), (Orientation::Rotate90, Some((480, 848))));
}

#[test]
fn a_big_photo_is_made_into_sd_and_hd_copies_within_their_limits() {
    let dir = tempfile::tempdir().unwrap();
    let media = media();
    let photo = jpeg(&dir, "big.jpg", 4000, 3000, 6, true);
    let probe = probe_of(&photo);
    let plans = plan(&probe);
    let hd = plans.hd.as_ref().unwrap();
    assert!(matches!(plans.sd.action, Action::Photo { long_edge: 1600, .. }));
    let (sd_to, hd_to) = (Destination::new(dir.path().join("sd.jpg")), Destination::new(dir.path().join("hd.jpg")));
    let sent =
        media.prepare(&photo, &probe, &[(&plans.sd, &sd_to), (hd, &hd_to)], &mut Job::new(Cancel::new())).unwrap();
    for (copy, limits, quality) in [(&sent[0], SD, Quality::Sd), (&sent[1], HD, Quality::Hd)] {
        let out = probe_of(&copy.path);
        let (w, h) = out.display_dimensions().unwrap();
        assert_eq!(copy.quality, quality);
        assert!(w.max(h) <= limits.photo_long_edge && h > w, "{quality:?} is upright within its limit: {w}x{h}");
        assert_eq!((out.metadata, Some((w, h))), (false, copy.dimensions));
    }
    assert!(sent[0].size < sent[1].size && sent[1].size <= probe.size);

    let heic = probe_of(&fixture("photo.heic"));
    let plans = plan(&heic);
    let sent =
        prepare(&media, &fixture("photo.heic"), &heic, &plans.sd, &Destination::new(dir.path().join("heic.jpg")));
    assert_eq!(
        (probe_of(&sent.path).format, sent.dimensions, sent.mime.as_str()),
        (Format::Jpeg, Some((1024, 640)), "image/jpeg")
    );
}

fn transcodes_a_1080p_video_within_sd_and_hd(encoder: VideoEncoder) {
    let dir = tempfile::tempdir().unwrap();
    let media = media_with(Config { video_encoder: Some(encoder), ..Config::default() });
    let film = video(&dir, "film.mp4", "1920x1080", 3, &["-r", "60"]);
    let probe = probe_of(&film);
    let plans = plan(&probe);
    for (plan, limits) in [(&plans.sd, SD), (plans.hd.as_ref().unwrap(), HD)] {
        let reports = Arc::new(Mutex::new(Vec::<Progress>::new()));
        let seen = reports.clone();
        let mut job = Job::new(Cancel::new()).on_progress(move |p| seen.lock().unwrap().push(p));
        let to = Destination::new(dir.path().join(format!("{:?}.mp4", plan.quality)));
        let sent = media.prepare(&film, &probe, &[(plan, &to)], &mut job).unwrap_or_else(|e| panic!("{e}")).remove(0);
        let out = probe_of(&sent.path);
        let (w, h) = out.dimensions.unwrap();
        assert_eq!(
            (out.video_codec.as_deref(), out.audio_codec.as_deref(), out.format),
            (Some("h264"), Some("aac"), Format::Mp4)
        );
        assert!(w <= limits.video_long_edge && h <= limits.video_short_edge, "{:?}: {w}x{h}", plan.quality);
        assert!(out.frame_rate.unwrap() <= limits.frame_rate + 0.5, "{:?}", out.frame_rate);
        assert!(out.bit_rate.unwrap() <= limits.video_max_bit_rate + limits.audio_bit_rate * 2, "{:?}", out.bit_rate);
        assert!(!out.metadata, "{:?} keeps no metadata", plan.quality);
        let reports = reports.lock().unwrap();
        assert_eq!(reports.last().map(|p| p.done), Some(1.0));
        assert!(reports.windows(2).all(|w| w[0].done <= w[1].done && w[0].bytes <= w[1].bytes));
        assert!(sent.peak_memory.unwrap() > 0);
    }
}

#[test]
fn a_1080p_video_is_made_into_sd_and_hd_with_x264() {
    transcodes_a_1080p_video_within_sd_and_hd(VideoEncoder::X264);
}

#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "VideoToolbox is only on macOS")]
fn a_1080p_video_is_made_into_sd_and_hd_with_videotoolbox() {
    if let Err(why) = media().tools().videotoolbox() {
        panic!("VideoToolbox should work on this Mac: {why}");
    }
    transcodes_a_1080p_video_within_sd_and_hd(VideoEncoder::VideoToolbox);
}

#[test]
fn a_gif_becomes_a_silent_mp4_that_loops() {
    let dir = tempfile::tempdir().unwrap();
    let gif = make(&["-f", "lavfi", "-i", "testsrc2=s=300x200:r=10:d=3"], &dir.path().join("loop.gif"));
    let probe = probe_of(&gif);
    let plans = plan(&probe);
    assert!(plans.sd.gif_playback && plans.hd.is_none());
    let sent = prepare(&media(), &gif, &probe, &plans.sd, &Destination::new(dir.path().join("loop.mp4")));
    let out = probe_of(&sent.path);
    assert_eq!((out.kind, out.video_codec.as_deref(), out.audio_codec.as_deref()), (Kind::Video, Some("h264"), None));
    assert_eq!((out.dimensions, sent.gif_playback, sent.mime.as_str()), (Some((300, 200)), true, "video/mp4"));
    assert!((out.duration.unwrap().as_secs_f64() - 3.0).abs() < 0.2, "{:?}", out.duration);
}

#[test]
fn audio_becomes_aac_and_voice_stays_opus() {
    let dir = tempfile::tempdir().unwrap();
    let wav =
        make(&["-f", "lavfi", "-i", "sine=f=440:d=2", "-metadata", "artist=someone"], &dir.path().join("song.wav"));
    let probe = probe_of(&wav);
    let plans = plan(&probe);
    assert_eq!(plans.sd.action, Action::Audio { bit_rate: SD.music_bit_rate });
    let sent = prepare(&media(), &wav, &probe, &plans.sd, &Destination::new(dir.path().join("song.m4a")));
    let out = probe_of(&sent.path);
    assert_eq!(
        (out.kind, out.format, out.audio_codec.as_deref(), out.metadata),
        (Kind::Audio, Format::M4a, Some("aac"), false)
    );

    let voice =
        make(&["-f", "lavfi", "-i", "sine=f=300:d=2", "-ac", "1", "-c:a", "libopus"], &dir.path().join("v.ogg"));
    let plans = plan(&probe_of(&voice));
    assert_eq!(plans.sd.action, Action::Untouched);
}

/// A 20-second 720p video, long enough to stop halfway.
fn long_job(dir: &TempDir) -> (Media, PathBuf, Probe, jaw_media::Plan) {
    let media = media_with(Config { video_encoder: Some(VideoEncoder::X264), threads: 1, ..Config::default() });
    let film = video(dir, "long.mp4", "1920x1080", 20, &[]);
    let probe = probe_of(&film);
    let plan = plan(&probe).hd.unwrap();
    (media, film, probe, plan)
}

#[test]
fn a_cancelled_transcode_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (media, film, probe, plan) = long_job(&dir);
    let cancel = Cancel::new();
    let stop = cancel.clone();
    let mut job = Job::new(cancel).on_progress(move |p| {
        if p.done > 0.05 {
            stop.cancel();
        }
    });
    let out = dir.path().join("out");
    let to = Destination::new(out.join("hd.mp4"));
    let started = std::time::Instant::now();
    let result = media.prepare(&film, &probe, &[(&plan, &to)], &mut job);
    assert!(matches!(result, Err(Error::Cancelled)), "{:?}", result.err());
    assert!(started.elapsed() < Duration::from_secs(15));
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "neither the copy nor its partial file is left");
}

#[test]
fn a_job_cancelled_after_its_first_copy_leaves_neither_copy() {
    let dir = tempfile::tempdir().unwrap();
    let (media, film, probe, hd) = long_job(&dir);
    let sd = plan(&probe).sd;
    let cancel = Cancel::new();
    let stop = cancel.clone();
    let mut job = Job::new(cancel).on_progress(move |p| {
        if p.copy == 1 && p.done > 0.02 {
            stop.cancel();
        }
    });
    let out = dir.path().join("out");
    let (sd_to, hd_to) =
        (Destination::with_poster(out.join("sd.mp4"), out.join("sd.jpg")), Destination::new(out.join("hd.mp4")));
    let result = media.prepare(&film, &probe, &[(&sd, &sd_to), (&hd, &hd_to)], &mut job);
    assert!(matches!(result, Err(Error::Cancelled)), "{:?}", result.err());
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "the SD copy and its poster went too");
}

#[test]
fn a_name_another_call_is_writing_is_busy_and_never_reported_as_there() {
    let dir = tempfile::tempdir().unwrap();
    let (media, film, probe, hd) = long_job(&dir);
    let sd = plan(&probe).sd;
    let out = dir.path().join("out");
    let (sd_to, hd_to) = (Destination::new(out.join("sd.mp4")), Destination::new(out.join("hd.mp4")));
    let cancel = Cancel::new();
    let stop = cancel.clone();
    let second = Arc::new(Mutex::new(None));
    let seen = second.clone();
    let (other, other_film, other_probe, other_sd) = (media.clone(), film.clone(), probe.clone(), sd.clone());
    let mut job = Job::new(cancel).on_progress(move |p| {
        if p.copy == 1 && seen.lock().unwrap().is_none() {
            assert!(out_has(&other_film, "sd.mp4"), "the first call's SD copy is in place");
            let again = Destination::new(other_film.with_file_name("out").join("sd.mp4"));
            let result = other.prepare(&other_film, &other_probe, &[(&other_sd, &again)], &mut Job::new(Cancel::new()));
            *seen.lock().unwrap() = Some(result.map(|_| ()));
            stop.cancel();
        }
    });
    let result = media.prepare(&film, &probe, &[(&sd, &sd_to), (&hd, &hd_to)], &mut job);
    assert!(matches!(result, Err(Error::Cancelled)), "{:?}", result.err());
    let second = second.lock().unwrap().take().expect("the second call ran");
    assert!(matches!(second, Err(Error::Busy { .. })), "{second:?}");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "and once the first returns, the name holds nothing");
}

fn out_has(film: &Path, name: &str) -> bool {
    film.with_file_name("out").join(name).exists()
}

#[test]
fn previews_that_fail_halfway_leave_neither() {
    let dir = tempfile::tempdir().unwrap();
    let photo = jpeg(&dir, "photo.jpg", 2000, 1000, 1, false);
    std::fs::write(dir.path().join("p.256.jpg"), b"already there").unwrap();
    let result = media().previews(&photo, &probe_of(&photo), &dir.path().join("p"), &Cancel::new());
    assert!(matches!(result, Err(Error::Exists { .. })), "{result:?}");
    assert!(!dir.path().join("p.768.jpg").exists());
    assert_eq!(std::fs::read(dir.path().join("p.256.jpg")).unwrap(), b"already there");
}

/// Set for the child process of the test below, naming the folder of its inputs.
const DISK_FULL: &str = "JAW_MEDIA_WRITE_ONTO_A_FULL_DISK";

#[test]
fn a_full_disk_fails_each_kind_of_job_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    jpeg(&dir, "photo.jpg", 2000, 1500, 6, true);
    video(&dir, "film.mp4", "640x360", 2, &[]);
    // The child's own output goes to pipes: the file-size limit it sets would also stop its writes to
    // a log the test runner gave it, once that log is past the limit.
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writes_onto_a_full_disk", "--ignored", "--nocapture"])
        .env(DISK_FULL, dir.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&child.stdout), String::from_utf8_lossy(&child.stderr));
    assert!(
        child.status.success() && stdout.contains("1 passed"),
        "the child's jobs didn't fail cleanly ({}):\n{stdout}\n{stderr}",
        child.status
    );
    assert_eq!(std::fs::read_dir(dir.path().join("out")).unwrap().count(), 0);
}

#[test]
#[ignore = "the child process of a_full_disk_fails_each_kind_of_job_and_leaves_nothing"]
fn writes_onto_a_full_disk() {
    let Some(dir) = std::env::var_os(DISK_FULL).map(PathBuf::from) else {
        return;
    };
    let media = media();
    let out = dir.join("out");
    std::fs::create_dir(&out).unwrap();
    // Files this process and its ffmpeg write stop at 20 KB, as a full disk stops them; the signal
    // that limit raises is ignored, so the write fails instead.
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
        let limit = libc::rlimit { rlim_cur: 20_000, rlim_max: libc::RLIM_INFINITY };
        assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
    }
    let photo = dir.join("photo.jpg");
    let probe = probe_of(&photo);
    let plans = plan(&probe);
    let (sd, hd) = (Destination::new(out.join("sd.jpg")), Destination::new(out.join("hd.jpg")));
    let copies = [(&plans.sd, &sd), (plans.hd.as_ref().unwrap(), &hd)];
    let photo_copies = media.prepare(&photo, &probe, &copies, &mut Job::new(Cancel::new()));
    assert!(matches!(photo_copies, Err(Error::Io { .. })), "{:?}", photo_copies.map(|_| ()));
    let previews = media.previews(&photo, &probe, &out.join("p"), &Cancel::new());
    assert!(matches!(previews, Err(Error::Io { .. })), "{:?}", previews.map(|_| ()));
    let film = dir.join("film.mp4");
    let probe = probe_of(&film);
    let mut transcode = plan(&probe).sd;
    transcode.action = Action::Video {
        width: 640,
        height: 360,
        bit_rate: 2_000_000,
        max_bit_rate: 3_000_000,
        frame_rate: 30.0,
        audio_bit_rate: Some(96_000),
    };
    let to = Destination::new(out.join("film.mp4"));
    let transcoded = media.prepare(&film, &probe, &[(&transcode, &to)], &mut Job::new(Cancel::new()));
    assert!(matches!(transcoded, Err(Error::Tool { .. })), "{:?}", transcoded.map(|_| ()));
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}

#[test]
fn a_transcode_whose_ffmpeg_is_killed_leaves_no_half_file() {
    let dir = tempfile::tempdir().unwrap();
    let (media, film, probe, plan) = long_job(&dir);
    let out = dir.path().join("out");
    let this_job = out.display().to_string();
    let mut killed = false;
    let mut job = Job::new(Cancel::new()).on_progress(move |p| {
        if p.done > 0.05 && !killed {
            killed = true;
            Command::new("pkill").args(["-KILL", "-f", &this_job]).status().unwrap();
        }
    });
    let result = media.prepare(&film, &probe, &[(&plan, &Destination::new(out.join("hd.mp4")))], &mut job);
    match result {
        Err(Error::Tool { why, .. }) => assert!(why.contains("137"), "{why}"),
        other => panic!("a killed ffmpeg is an error: {:?}", other.map(|_| ())),
    }
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "neither the copy nor its partial file is left");
}
