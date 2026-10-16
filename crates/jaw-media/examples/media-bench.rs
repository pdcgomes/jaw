//! Measures the pipeline against the design's budgets on made-up media:
//!
//!     cargo run --release -p jaw-media --example media-bench [-- FOLDER] [--photos]
//!
//! It makes its inputs in FOLDER (a folder in the system's temporary one by default, kept for the
//! next run): a 12-megapixel JPEG and HEIC (macOS's `sips` makes the HEIC), one-minute 1080p H.264
//! and 4K HEVC videos with noise so they cost what footage does, and a GIF. Then it times each step
//! as jawd would run it, five times for photos (the median shown), once for videos.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use jaw_media::{Action, Cancel, Config, Destination, Job, Media, Plan, Prepared, Probe, VideoEncoder, plan};

fn main() {
    let photos_only = std::env::args().any(|a| a == "--photos");
    let folder = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("jaw-media-measure"));
    std::fs::create_dir_all(&folder).unwrap();
    let media = Media::new(Config::default());
    let status = media.status();
    println!("ffmpeg: {}", status.ffmpeg.as_deref().unwrap_or("missing"));
    println!("video encoder: {:?}; threads per ffmpeg: {}", status.video_encoder, media.config().threads);
    let inputs = Inputs::make(&media, &folder);
    let out = folder.join("out");
    std::fs::create_dir_all(&out).unwrap();

    println!("\n{:<52} {:>10}", "photos (median of 5)", "time");
    if inputs.photos.iter().all(|(name, _)| !name.contains("HEIC")) {
        println!("HEIC: skipped, sips isn't here to make one");
    }
    for (name, source) in &inputs.photos {
        let probe = media.probe(source).unwrap();
        let name = format!("{name}, {:.1} MB", probe.size as f64 / 1e6);
        let plans = plan(&probe);
        let hd = plans.hd.clone().unwrap();
        let sizes = |sent: &[Prepared]| {
            sent.iter().map(|p| format!("{:?} {} KB", p.quality, p.size / 1000)).collect::<Vec<_>>().join(", ")
        };
        let (time, sent) = median(|| both(&media, source, &probe, &plans.sd, &hd, &out));
        println!(
            "{:<52} {:>7.0} ms   {:?}, {}",
            format!("{name} to SD and HD, as planned"),
            ms(time),
            hd.action,
            sizes(&sent)
        );
        let encoded =
            Plan { action: Action::Photo { long_edge: 4096, quality: jaw_media::HD.photo_quality }, ..hd.clone() };
        let (time, sent) = median(|| both(&media, source, &probe, &plans.sd, &encoded, &out));
        println!("{:<52} {:>7.0} ms   {}", format!("{name} to SD and HD, HD encoded again"), ms(time), sizes(&sent));
        let (time, _) = median(|| {
            clear(&out, &["preview.256.jpg", "preview.768.jpg"]);
            media.previews(source, &probe, &out.join("preview"), &Cancel::new()).unwrap()
        });
        println!("{:<52} {:>7.0} ms", format!("{name} to previews at 256 and 768"), ms(time));
    }
    println!("in-process peak memory so far (the whole process): {} MB", peak_rss() / 1_000_000);
    if photos_only {
        return;
    }

    println!("\n{:<44} {:>9} {:>9} {:>10} {:>9}  output", "videos", "time", "CPU", "memory", "speed");
    let encoders: &[VideoEncoder] = if cfg!(target_os = "macos") {
        &[VideoEncoder::VideoToolbox, VideoEncoder::X264]
    } else {
        &[VideoEncoder::X264]
    };
    for (name, source) in [("1-minute 1080p H.264", &inputs.hd_video), ("1-minute 4K HEVC", &inputs.uhd_video)] {
        let probe = media.probe(source).unwrap();
        let plans = plan(&probe);
        for &encoder in encoders {
            if encoder == VideoEncoder::X264 && source == &inputs.uhd_video {
                continue;
            }
            let media = Media::new(Config { video_encoder: Some(encoder), ..Config::default() });
            for plan in [Some(&plans.sd), plans.hd.as_ref()].into_iter().flatten() {
                let name_of_copy = format!("video-{:?}.mp4", plan.quality);
                clear(&out, &[&name_of_copy]);
                let to = Destination::new(out.join(name_of_copy));
                let started = Instant::now();
                let sent =
                    media.prepare(source, &probe, &[(plan, &to)], &mut Job::new(Cancel::new())).unwrap().remove(0);
                video_row(&format!("{name}, {:?}, {:?}", plan.quality, encoder), started.elapsed(), &sent, &probe);
            }
        }
    }
    let probe = media.probe(&inputs.gif).unwrap();
    let plans = plan(&probe);
    clear(&out, &["gif.mp4"]);
    let started = Instant::now();
    let to = Destination::new(out.join("gif.mp4"));
    let sent = media.prepare(&inputs.gif, &probe, &[(&plans.sd, &to)], &mut Job::new(Cancel::new())).unwrap().remove(0);
    video_row(&format!("10-second GIF, {} KB, to MP4", probe.size / 1000), started.elapsed(), &sent, &probe);
}

/// Removes the last run's copies, since a copy never replaces a file.
fn clear(out: &Path, names: &[&str]) {
    for name in names {
        let _ = std::fs::remove_file(out.join(name));
    }
}

fn both(media: &Media, source: &Path, probe: &Probe, sd: &Plan, hd: &Plan, out: &Path) -> Vec<Prepared> {
    clear(out, &["sd.jpg", "hd.jpg"]);
    let (sd_to, hd_to) = (Destination::new(out.join("sd.jpg")), Destination::new(out.join("hd.jpg")));
    media.prepare(source, probe, &[(sd, &sd_to), (hd, &hd_to)], &mut Job::new(Cancel::new())).unwrap()
}

fn video_row(name: &str, took: Duration, sent: &Prepared, probe: &Probe) {
    let length = probe.duration.map_or(0.0, |d| d.as_secs_f64());
    let (w, h) = sent.dimensions.unwrap_or_default();
    println!(
        "{:<44} {:>7.1} s {:>7.1} s {:>7} MB {:>8.1}x  {w}x{h}, {:.1} Mbit/s",
        name,
        took.as_secs_f64(),
        sent.cpu.unwrap_or_default().as_secs_f64(),
        sent.peak_memory.unwrap_or_default() / 1_000_000,
        length / took.as_secs_f64(),
        sent.size as f64 * 8.0 / length.max(0.001) / 1e6,
    );
}

fn median<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut last = f();
    let mut times = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        last = f();
        times.push(started.elapsed());
    }
    times.sort();
    (times[2], last)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn peak_rss() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    usage.ru_maxrss as u64 * if cfg!(target_os = "macos") { 1 } else { 1024 }
}

struct Inputs {
    photos: Vec<(String, PathBuf)>,
    hd_video: PathBuf,
    uhd_video: PathBuf,
    gif: PathBuf,
}

impl Inputs {
    fn make(media: &Media, folder: &Path) -> Inputs {
        let ffmpeg = media.tools().ffmpeg_path().expect("measuring needs ffmpeg").to_path_buf();
        let run = |args: &[&str], to: &Path| {
            if to.exists() {
                return;
            }
            println!("making {}", to.display());
            let status = Command::new(&ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
                .args(args)
                .arg(to)
                .status()
                .unwrap();
            assert!(status.success(), "ffmpeg couldn't make {}", to.display());
        };
        // Two 12-megapixel pictures: textured like a photo with a little sensor noise, and covered
        // in noise, which costs a JPEG codec more than any photo does.
        let pictures = [
            (
                "photo-like",
                "perlin=s=4032x3024:octaves=6:persistence=0.6:random_mode=ken,format=gray,\
                 geq=r='p(X,Y)*0.9+30*sin(X/97)':g='p(X,Y)*0.7+40':b='p(X,Y)*0.5+60*cos(Y/131)',noise=alls=4:allf=t",
            ),
            ("noisy", "mandelbrot=s=4032x3024:end_pts=1,noise=alls=12:allf=t"),
        ];
        let mut photos = Vec::new();
        for (name, source) in pictures {
            let png = folder.join(format!("{name}.png"));
            run(&["-f", "lavfi", "-i", source, "-frames:v", "1"], &png);
            let jpeg = folder.join(format!("{name}.jpg"));
            if !jpeg.exists() {
                let picture = image::open(&png).unwrap().into_rgb8();
                let mut data = Vec::new();
                let mut encoder = jpeg_encoder::Encoder::new(&mut data, 92);
                encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
                encoder.encode(picture.as_raw(), 4032, 3024, jpeg_encoder::ColorType::Rgb).unwrap();
                std::fs::write(&jpeg, data).unwrap();
            }
            photos.push((format!("12 MP {name} JPEG"), jpeg));
            let heic = folder.join(format!("{name}.heic"));
            if !heic.exists() && cfg!(target_os = "macos") {
                Command::new("sips").args(["-s", "format", "heic"]).arg(&png).arg("--out").arg(&heic).output().unwrap();
            }
            if heic.exists() {
                photos.push((format!("12 MP {name} HEIC"), heic));
            }
        }
        let noisy = |size: &str| format!("testsrc2=s={size}:r=30:d=60,noise=alls=10:allf=t+u");
        let (h264, hevc): (&[&str], &[&str]) = if cfg!(target_os = "macos") {
            (
                &["-c:v", "h264_videotoolbox", "-b:v", "16M"],
                &["-c:v", "hevc_videotoolbox", "-b:v", "40M", "-tag:v", "hvc1"],
            )
        } else {
            (
                &["-c:v", "libx264", "-preset", "ultrafast", "-b:v", "16M"],
                &["-c:v", "libx264", "-preset", "ultrafast", "-b:v", "40M"],
            )
        };
        let tone = ["-f", "lavfi", "-i", "sine=f=440:d=60", "-c:a", "aac", "-b:a", "192k"];
        let hd_video = folder.join("1080p.mp4");
        run(
            &[&["-f", "lavfi", "-i", &noisy("1920x1080")][..], &tone, h264, &["-pix_fmt", "yuv420p"]].concat(),
            &hd_video,
        );
        let uhd_video = folder.join("4k.mov");
        run(
            &[&["-f", "lavfi", "-i", &noisy("3840x2160")][..], &tone, hevc, &["-pix_fmt", "yuv420p"]].concat(),
            &uhd_video,
        );
        let gif = folder.join("loop.gif");
        run(&["-f", "lavfi", "-i", "testsrc2=s=480x270:r=15:d=10"], &gif);
        Inputs { photos, hd_video, uhd_video, gif }
    }
}
