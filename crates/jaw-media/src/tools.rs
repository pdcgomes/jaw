use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::error::{Error, Result};

/// Where ffmpeg and ffprobe are looked for after `PATH`, which a LaunchAgent's is too short to have.
const PLACES: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

/// An H.264 encoder ffmpeg has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoEncoder {
    /// Apple's, on the media engine where the Mac has one; macOS only.
    VideoToolbox,
    /// x264, in software, everywhere else.
    X264,
}

impl VideoEncoder {
    pub fn name(self) -> &'static str {
        match self {
            VideoEncoder::VideoToolbox => "h264_videotoolbox",
            VideoEncoder::X264 => "libx264",
        }
    }
}

/// The ffmpeg and ffprobe this machine has, and what they can do.
pub struct Tools {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
    version: Option<String>,
    encoders: Vec<String>,
    videotoolbox: OnceLock<std::result::Result<(), String>>,
}

/// What the pipeline can do here, for `jawd status`.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    /// ffmpeg's path and version, if it's installed.
    pub ffmpeg: Option<String>,
    /// The encoder videos will go through.
    pub video_encoder: Option<VideoEncoder>,
    /// What can't be done here, each with what's missing.
    pub missing: Vec<String>,
}

impl Tools {
    /// ffmpeg and ffprobe from `PATH` or where Homebrew and Linux distributions put them.
    pub fn find() -> Tools {
        Tools::at(locate("ffmpeg"), locate("ffprobe"))
    }

    /// The tools at these paths, or none where a path is None.
    pub fn at(ffmpeg: Option<PathBuf>, ffprobe: Option<PathBuf>) -> Tools {
        let version = ffmpeg.as_deref().and_then(|ffmpeg| {
            let out = output(Command::new(ffmpeg).arg("-version"))?;
            out.lines().next()?.strip_prefix("ffmpeg version ")?.split_whitespace().next().map(str::to_string)
        });
        let encoders = ffmpeg
            .as_deref()
            .and_then(|ffmpeg| output(Command::new(ffmpeg).args(["-hide_banner", "-encoders"])))
            .map(|out| {
                out.lines()
                    .filter_map(|line| {
                        let mut words = line.split_whitespace();
                        let flags = words.next()?;
                        (flags.len() == 6 && flags != "------").then(|| words.next().map(str::to_string)).flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        Tools { ffmpeg, ffprobe, version, encoders, videotoolbox: OnceLock::new() }
    }

    /// No tools at all: photos, GIFs and stickers still work.
    pub fn none() -> Tools {
        Tools::at(None, None)
    }

    pub(crate) fn ffmpeg(&self, needed_for: &str) -> Result<&Path> {
        self.ffmpeg.as_deref().ok_or_else(|| Error::Missing { tool: "ffmpeg", needed_for: needed_for.to_string() })
    }

    pub(crate) fn ffprobe(&self, needed_for: &str) -> Result<&Path> {
        self.ffprobe.as_deref().ok_or_else(|| Error::Missing { tool: "ffprobe", needed_for: needed_for.to_string() })
    }

    pub fn ffmpeg_path(&self) -> Option<&Path> {
        self.ffmpeg.as_deref()
    }

    pub fn ffprobe_path(&self) -> Option<&Path> {
        self.ffprobe.as_deref()
    }

    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some() && self.ffprobe.is_some()
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.iter().any(|e| e == name)
    }

    /// Whether VideoToolbox can encode H.264 here, found by encoding a few frames the first time
    /// it's asked; why not, if it can't.
    pub fn videotoolbox(&self) -> std::result::Result<(), String> {
        self.videotoolbox
            .get_or_init(|| {
                if !cfg!(target_os = "macos") {
                    return Err("VideoToolbox is only on macOS".into());
                }
                let Some(ffmpeg) = self.ffmpeg.as_deref() else { return Err("ffmpeg isn't installed".into()) };
                if !self.has_encoder(VideoEncoder::VideoToolbox.name()) {
                    return Err("this ffmpeg was built without VideoToolbox".into());
                }
                let out = Command::new(ffmpeg)
                    .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
                    .args([
                        "testsrc2=s=256x144:d=0.2",
                        "-c:v",
                        "h264_videotoolbox",
                        "-allow_sw",
                        "1",
                        "-f",
                        "null",
                        "-",
                    ])
                    .stdin(Stdio::null())
                    .output()
                    .map_err(|e| format!("ffmpeg didn't start: {e}"))?;
                if out.status.success() {
                    Ok(())
                } else {
                    Err(format!("VideoToolbox refused a test encode: {}", String::from_utf8_lossy(&out.stderr).trim()))
                }
            })
            .clone()
    }

    /// The encoder videos go through: `wanted` if it works here, otherwise VideoToolbox where it
    /// works and x264 where it doesn't.
    pub fn video_encoder(&self, wanted: Option<VideoEncoder>) -> Result<VideoEncoder> {
        self.ffmpeg("to send videos and GIFs")?;
        let x264 = || {
            if self.has_encoder(VideoEncoder::X264.name()) {
                Ok(VideoEncoder::X264)
            } else {
                Err(Error::Missing { tool: "ffmpeg with x264", needed_for: "to send videos and GIFs".into() })
            }
        };
        match wanted {
            Some(VideoEncoder::X264) => x264(),
            Some(VideoEncoder::VideoToolbox) => {
                self.videotoolbox().map(|()| VideoEncoder::VideoToolbox).map_err(|why| Error::Missing {
                    tool: "VideoToolbox",
                    needed_for: format!("to send videos as configured ({why})"),
                })
            }
            None => self.videotoolbox().map(|()| VideoEncoder::VideoToolbox).or_else(|_| x264()),
        }
    }

    /// Whether this ffmpeg reads HEIC photos, which are grids of tiles it joins from 7.1 on.
    pub fn reads_heic(&self) -> bool {
        self.ffmpeg.is_some() && self.version.as_deref().is_none_or(|v| at_least(v, 7, 1))
    }

    pub fn status(&self, wanted: Option<VideoEncoder>) -> Status {
        let mut missing = Vec::new();
        if self.ffmpeg.is_none() || self.ffprobe.is_none() {
            missing.push(
                "ffmpeg: sending videos, GIFs, audio and HEIC photos, and video posters, need ffmpeg and ffprobe"
                    .into(),
            );
        } else if !self.reads_heic() {
            missing.push("ffmpeg 7.1 or newer: HEIC photos need it".into());
        }
        let video_encoder = self.video_encoder(wanted).ok();
        if self.ffmpeg.is_some() && video_encoder.is_none() {
            missing.push("ffmpeg with x264 or VideoToolbox: sending videos and GIFs needs one".into());
        }
        let ffmpeg = self
            .ffmpeg
            .as_ref()
            .map(|path| format!("{} ({})", path.display(), self.version.as_deref().unwrap_or("version unknown")));
        Status { ffmpeg, video_encoder, missing }
    }
}

fn locate(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(PLACES.iter().map(PathBuf::from))
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
}

fn output(command: &mut Command) -> Option<String> {
    let out = command.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether a release version such as "8.1.2" or "n7.1-3-g…" is at least major.minor; a build from
/// git without a release number counts as new enough.
fn at_least(version: &str, major: u32, minor: u32) -> bool {
    let mut parts = version.trim_start_matches('n').split(['.', '-']).map(|p| p.parse::<u32>());
    match (parts.next(), parts.next()) {
        (Some(Ok(a)), Some(Ok(b))) => (a, b) >= (major, minor),
        (Some(Ok(a)), _) => a > major,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_versions_are_compared_and_builds_from_git_count_as_new() {
        assert!(at_least("8.1.2", 7, 1));
        assert!(at_least("7.1", 7, 1));
        assert!(!at_least("6.1.1-3ubuntu5", 7, 1));
        assert!(at_least("n7.1.1-6-g48c0f071d4-20250405", 7, 1));
        assert!(at_least("N-119999-g1234abcd", 7, 1));
    }

    #[test]
    fn without_ffmpeg_the_status_says_what_needs_it() {
        let status = Tools::none().status(None);
        assert_eq!((status.ffmpeg, status.video_encoder), (None, None));
        assert!(status.missing[0].starts_with("ffmpeg: sending videos"), "{:?}", status.missing);
    }
}
