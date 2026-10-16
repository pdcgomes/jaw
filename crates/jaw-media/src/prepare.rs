//! Making the copies a plan asks for. Photos are decoded once in this process for every copy and
//! the thumbnail; videos, GIFs and audio stream through ffmpeg from path to path, with progress
//! and cancel. Each file is written under `.part` and renamed into place once whole and synced.

use std::borrow::Cow;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::DynamicImage;

use crate::error::{Error, Result};
use crate::file::{Claim, Part};
use crate::job::{Job, Progress};
use crate::limits::{THUMBNAIL_LONG_EDGE, THUMBNAIL_QUALITY};
use crate::plan::{Action, Plan, Quality};
use crate::preview::{self, Preview};
use crate::probe::{Kind, Probe};
use crate::run::{Stdout, run};
use crate::tools::VideoEncoder;
use crate::{Media, jpeg, raster};

/// Where a copy goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub file: PathBuf,
    /// Where a video's poster goes: its frame at the copy's size, as a JPEG, for jawd's previews of
    /// what it sent. None makes no poster file, though the thumbnail is made from it all the same.
    pub poster: Option<PathBuf>,
}

impl Destination {
    pub fn new(file: impl Into<PathBuf>) -> Destination {
        Destination { file: file.into(), poster: None }
    }

    pub fn with_poster(file: impl Into<PathBuf>, poster: impl Into<PathBuf>) -> Destination {
        Destination { file: file.into(), poster: Some(poster.into()) }
    }
}

/// A copy, ready to upload.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub quality: Quality,
    /// The file to upload: the source itself when it goes untouched, the destination otherwise.
    pub path: PathBuf,
    pub untouched: bool,
    pub mime: String,
    /// Bytes.
    pub size: u64,
    /// Width and height as shown.
    pub dimensions: Option<(u32, u32)>,
    pub duration: Option<Duration>,
    /// The JPEG thumbnail the message carries, THUMBNAIL_LONG_EDGE pixels on its longest side.
    pub thumbnail: Option<Vec<u8>>,
    /// A video's or GIF's poster, if the destination asked for one.
    pub poster: Option<Preview>,
    pub gif_playback: bool,
    /// The peak memory of the ffmpeg that made it, in bytes.
    pub peak_memory: Option<u64>,
    /// The processor time that ffmpeg used.
    pub cpu: Option<Duration>,
}

pub(crate) fn prepare(
    media: &Media,
    source: &Path,
    probe: &Probe,
    copies: &[(&Plan, &Destination)],
    job: &mut Job,
) -> Result<Vec<Prepared>> {
    let names = copies.iter().flat_map(|(_, to)| std::iter::once(to.file.as_path()).chain(to.poster.as_deref()));
    let _claim = Claim::take(names)?;
    if probe.kind == Kind::Photo {
        return photos(media, source, probe, copies, job);
    }
    let mut done = Vec::new();
    for (i, (plan, to)) in copies.iter().enumerate() {
        match one(media, source, probe, plan, to, i, job) {
            Ok(prepared) => done.push(prepared),
            Err(e) => {
                undo(&done);
                return Err(e);
            }
        }
    }
    Ok(done)
}

/// Removes the files a call made before it failed, so a failed or cancelled call leaves nothing:
/// each name is this call's own, since a name that was taken is never written.
fn undo(made: &[Prepared]) {
    for prepared in made {
        if !prepared.untouched {
            let _ = std::fs::remove_file(&prepared.path);
        }
        if let Some(poster) = &prepared.poster {
            let _ = std::fs::remove_file(&poster.path);
        }
    }
}

fn base(plan: &Plan, path: PathBuf, size: u64, probe: &Probe) -> Prepared {
    Prepared {
        quality: plan.quality,
        path,
        untouched: plan.action == Action::Untouched,
        mime: plan.mime.clone(),
        size,
        dimensions: plan.dimensions,
        duration: probe.duration,
        thumbnail: None,
        poster: None,
        gif_playback: plan.gif_playback,
        peak_memory: None,
        cpu: None,
    }
}

/// Every photo copy from one decode of the source, each encoded on a thread of its own so an HD
/// copy's encode doesn't wait behind the SD one, and the thumbnail from the smallest.
fn photos(
    media: &Media,
    source: &Path,
    probe: &Probe,
    copies: &[(&Plan, &Destination)],
    job: &mut Job,
) -> Result<Vec<Prepared>> {
    let longest = copies
        .iter()
        .filter_map(|(plan, _)| match plan.action {
            Action::Photo { long_edge, .. } => Some(long_edge),
            _ => None,
        })
        .max()
        .unwrap_or(THUMBNAIL_LONG_EDGE);
    let picture = raster::decode(media, source, probe, longest, None, &job.cancel)?;
    let cancel = &job.cancel;
    let picture = &picture;
    let made: Vec<Result<(Prepared, Option<DynamicImage>)>> = std::thread::scope(|scope| {
        let workers: Vec<_> = copies
            .iter()
            .map(|(plan, to)| scope.spawn(move || photo(source, probe, picture, plan, to, cancel)))
            .collect();
        workers.into_iter().map(|w| w.join().expect("a photo's encoder doesn't panic")).collect()
    });
    let mut done = Vec::new();
    let mut failed = None;
    let mut smallest: Option<DynamicImage> = None;
    for made in made {
        match made {
            Ok((prepared, scaled)) => {
                if let Some(scaled) = scaled.filter(|s| smallest.as_ref().is_none_or(|small| small.width() > s.width()))
                {
                    smallest = Some(scaled);
                }
                done.push(prepared);
            }
            Err(e) => failed = failed.or(Some(e)),
        }
    }
    let thumbnail = match failed.map_or_else(|| thumbnail(smallest.as_ref().unwrap_or(picture)), Err) {
        Ok(thumbnail) => thumbnail,
        Err(e) => {
            undo(&done);
            return Err(e);
        }
    };
    for (i, prepared) in done.iter().enumerate() {
        job.report(Progress { copy: i, done: 1.0, bytes: prepared.size });
    }
    for prepared in &mut done {
        prepared.thumbnail = Some(thumbnail.clone());
    }
    Ok(done)
}

/// One photo copy, and the picture it was encoded from when that was scaled from the source's.
fn photo(
    source: &Path,
    probe: &Probe,
    picture: &DynamicImage,
    plan: &Plan,
    to: &Destination,
    cancel: &crate::job::Cancel,
) -> Result<(Prepared, Option<DynamicImage>)> {
    cancel.check()?;
    let (mut prepared, scaled) = match plan.action {
        Action::Untouched => {
            let size = std::fs::metadata(source).map_err(Error::io(source))?.len();
            (base(plan, source.to_path_buf(), size, probe), None)
        }
        Action::Strip => {
            let data = std::fs::read(source).map_err(Error::io(source))?;
            let orientation = probe.orientation.to_exif();
            let size =
                Part::create(&to.file)?.write(|out| jpeg::strip(&data, orientation, out).map_err(Error::io(source)))?;
            (base(plan, to.file.clone(), size, probe), None)
        }
        Action::Photo { long_edge, quality } => {
            let scaled = fit_ref(picture, long_edge);
            let size = Part::create(&to.file)?
                .write(|out| raster::jpeg(&scaled, quality, out).map_err(Error::io(&to.file)))?;
            let mut prepared = base(plan, to.file.clone(), size, probe);
            prepared.dimensions = Some((scaled.width(), scaled.height()));
            let scaled = match scaled {
                Cow::Owned(scaled) => Some(scaled),
                Cow::Borrowed(_) => None,
            };
            (prepared, scaled)
        }
        _ => return Err(Error::unreadable(source, "a photo's plan can only make a photo")),
    };
    prepared.duration = None;
    Ok((prepared, scaled))
}

/// The picture at most `long_edge` pixels on its longest side, borrowed when it already is.
fn fit_ref(image: &DynamicImage, long_edge: u32) -> Cow<'_, DynamicImage> {
    if image.width().max(image.height()) <= long_edge {
        Cow::Borrowed(image)
    } else {
        let (w, h) = raster::scaled(image.width(), image.height(), long_edge);
        Cow::Owned(raster::resize(image, w, h))
    }
}

fn thumbnail(image: &DynamicImage) -> Result<Vec<u8>> {
    let small = fit_ref(image, THUMBNAIL_LONG_EDGE);
    let mut out = Vec::new();
    raster::jpeg(&small, THUMBNAIL_QUALITY, &mut out)
        .map_err(|e| Error::Io { path: PathBuf::from("thumbnail"), error: e })?;
    Ok(out)
}

/// One copy of anything that isn't a photo.
fn one(
    media: &Media,
    source: &Path,
    probe: &Probe,
    plan: &Plan,
    to: &Destination,
    copy: usize,
    job: &mut Job,
) -> Result<Prepared> {
    job.cancel.check()?;
    let mut prepared = match plan.action {
        Action::Untouched => {
            let size = std::fs::metadata(source).map_err(Error::io(source))?.len();
            base(plan, source.to_path_buf(), size, probe)
        }
        _ => {
            let mut args = ffmpeg_args(media, source, probe, plan)?;
            let part = Part::create(&to.file)?;
            args.push(part.path().into());
            let ffmpeg = media.tools().ffmpeg("to send videos, GIFs and audio")?;
            let duration = probe.duration.map(|d| d.as_secs_f64());
            let cancel = job.cancel.clone();
            let mut report = |done: f32, bytes: u64| job.report(Progress { copy, done, bytes });
            let ran = run(
                "ffmpeg",
                ffmpeg,
                &args,
                media.config().nice,
                source,
                Stdout::Progress { duration, report: &mut report },
                &cancel,
            )?;
            let size = part.finish()?;
            let mut prepared = base(plan, to.file.clone(), size, probe);
            prepared.peak_memory = Some(ran.peak_memory);
            prepared.cpu = Some(ran.cpu);
            prepared
        }
    };
    if matches!(probe.kind, Kind::Video | Kind::Gif)
        && probe.dimensions.is_some()
        && let Err(e) = poster(media, source, probe, plan, to, &job.cancel, &mut prepared)
    {
        undo(&[prepared]);
        return Err(e);
    }
    job.report(Progress { copy, done: 1.0, bytes: prepared.size });
    Ok(prepared)
}

/// A video's or GIF's thumbnail, and its poster if the destination asks for one, from its frame
/// at the copy's size.
fn poster(
    media: &Media,
    source: &Path,
    probe: &Probe,
    plan: &Plan,
    to: &Destination,
    cancel: &crate::job::Cancel,
    prepared: &mut Prepared,
) -> Result<()> {
    let long_edge = plan.dimensions.map_or(crate::preview::LARGE, |(w, h)| w.max(h));
    let at = (probe.kind == Kind::Video).then(|| probe.duration.map_or(0.0, |d| (d.as_secs_f64() * 0.1).min(1.0)));
    let frame = raster::decode(media, source, probe, long_edge, at, cancel)?;
    prepared.thumbnail = Some(thumbnail(&frame)?);
    if let Some(poster) = &to.poster {
        prepared.poster = Some(preview::write_to(&frame, poster, false)?);
    }
    Ok(())
}

fn strings(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// ffmpeg's arguments for a copy, up to the output's path.
fn ffmpeg_args(media: &Media, source: &Path, probe: &Probe, plan: &Plan) -> Result<Vec<OsString>> {
    let encoder = match plan.action {
        Action::Video { .. } => Some(media.tools().video_encoder(media.config().video_encoder)?),
        _ => None,
    };
    // With VideoToolbox decoding and encoding, more than two threads only hold more frames: a 4K
    // video goes as fast in 205 MB as in 420 MB with eight.
    let threads = match encoder {
        Some(VideoEncoder::VideoToolbox) => media.config().threads.min(2),
        _ => media.config().threads,
    }
    .to_string();
    let mut args = strings(&["-nostdin", "-hide_banner", "-loglevel", "error", "-nostats", "-progress", "pipe:1"]);
    args.extend(strings(&["-y", "-threads", &threads, "-filter_threads", &threads]));
    let video = probe.picture.clone().unwrap_or_else(|| "0:v:0".into());
    if encoder == Some(VideoEncoder::VideoToolbox) && probe.kind == Kind::Video {
        args.extend(strings(&["-hwaccel", "videotoolbox"]));
    }
    args.extend(["-i".into(), source.into()]);
    args.extend(strings(&["-threads", &threads, "-map_metadata", "-1", "-map_chapters", "-1"]));
    match plan.action {
        Action::Strip if probe.kind == Kind::Video => {
            args.extend(strings(&[
                "-map",
                &video,
                "-map",
                "0:a:0?",
                "-c",
                "copy",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ]));
        }
        Action::Strip => {
            args.extend(strings(&["-vn", "-map", "0:a:0", "-c:a", "copy", "-movflags", "+faststart", "-f", "ipod"]));
        }
        Action::Video { width, height, bit_rate, max_bit_rate, frame_rate, audio_bit_rate } => {
            let filter = format!("scale={width}:{height}:flags=bicubic,format=yuv420p");
            args.extend(strings(&["-map", &video, "-vf", &filter, "-fpsmax", &frame_rate.to_string()]));
            let (rate, max, buffer) = (bit_rate.to_string(), max_bit_rate.to_string(), (max_bit_rate * 2).to_string());
            match encoder.expect("a video's encoder") {
                VideoEncoder::VideoToolbox => args.extend(strings(&[
                    "-c:v",
                    "h264_videotoolbox",
                    "-allow_sw",
                    "1",
                    "-profile:v",
                    "high",
                    "-b:v",
                    &rate,
                    "-maxrate",
                    &max,
                    "-bufsize",
                    &buffer,
                ])),
                VideoEncoder::X264 => args.extend(strings(&[
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-profile:v",
                    "high",
                    "-crf",
                    "23",
                    "-maxrate",
                    &max,
                    "-bufsize",
                    &buffer,
                ])),
            }
            match audio_bit_rate {
                Some(audio) => {
                    args.extend(strings(&["-map", "0:a:0?", "-c:a", "aac", "-b:a", &audio.to_string()]));
                    if probe.channels.is_some_and(|c| c > 2) {
                        args.extend(strings(&["-ac", "2"]));
                    }
                }
                None => args.push("-an".into()),
            }
            args.extend(strings(&["-movflags", "+faststart", "-f", "mp4"]));
        }
        Action::Audio { bit_rate } => {
            args.extend(strings(&["-vn", "-map", "0:a:0", "-c:a", "aac", "-b:a", &bit_rate.to_string()]));
            if probe.channels.is_some_and(|c| c > 2) {
                args.extend(strings(&["-ac", "2"]));
            }
            args.extend(strings(&["-movflags", "+faststart", "-f", "ipod"]));
        }
        Action::Voice { bit_rate } => {
            if !media.tools().has_encoder("libopus") {
                return Err(Error::Missing {
                    tool: "ffmpeg with libopus",
                    needed_for: "to send voice messages".into(),
                });
            }
            args.extend(strings(&["-vn", "-map", "0:a:0", "-c:a", "libopus", "-b:a", &bit_rate.to_string()]));
            args.extend(strings(&["-ac", "1", "-application", "voip", "-f", "ogg"]));
        }
        Action::Untouched | Action::Photo { .. } => {
            return Err(Error::unreadable(source, "this copy isn't one ffmpeg makes"));
        }
    }
    Ok(args)
}
