//! Records a voice message from the default microphone until Enter, as jawd will.
//!
//!     cargo run -p jaw-voice --example record -- <file.ogg> [most seconds]
//!
//! macOS asks once for the terminal you run it from to use the microphone.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use crossbeam_channel::select;
use jaw_voice::{Event, RECORDING_LIMIT, Voice};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().context("usage: record <file.ogg> [most seconds]")?);
    let limit = args.next().map_or(Ok(RECORDING_LIMIT), |s| s.parse().map(Duration::from_secs))?;

    let (enter_tx, enter) = crossbeam_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        let _ = enter_tx.send(());
    });

    let (voice, events) = Voice::system();
    voice.record(&path, limit)?;
    println!("recording to {}; Enter stops", path.display());
    loop {
        select! {
            recv(enter) -> _ => voice.stop_recording()?,
            recv(events) -> event => match event? {
                Event::Recording { elapsed, level } => {
                    let bar = "█".repeat(usize::from(level) / 4);
                    print!("\r● {}:{:02} {bar:<25}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);
                    std::io::stdout().flush()?;
                }
                Event::RecordingSilent => {
                    eprintln!("\nthe microphone gives only silence; if macOS asked, allow it in System Settings › Privacy & Security › Microphone");
                }
                Event::Recorded(r) => {
                    println!("\n{:.2} s ({:?}), {} bytes", r.duration.as_secs_f64(), r.end, std::fs::metadata(&r.path)?.len());
                    println!("waveform {:?}", r.waveform);
                    println!("play it: cargo run -p jaw-voice --example play -- {}", r.path.display());
                    return Ok(());
                }
                Event::RecordingFailed { error } | Event::RecordingRefused { error } => bail!(error),
                _ => {}
            }
        }
    }
}
