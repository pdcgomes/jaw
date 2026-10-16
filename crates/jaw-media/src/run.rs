//! Running ffmpeg and ffprobe: at a lower OS priority, killed at once by a job's cancel, with
//! ffmpeg's progress read as it goes and its peak memory measured when it exits.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{ChildStdout, Command, Stdio};
use std::time::Duration;

use crate::error::{Error, Result};
use crate::job::{Cancel, kill};

/// What a child prints on its standard output.
pub(crate) enum Stdout<'a> {
    /// ffmpeg's `-progress pipe:1` lines: the fraction done, against `duration` in seconds, and
    /// the bytes written so far go to `report`.
    Progress { duration: Option<f64>, report: &'a mut dyn FnMut(f32, u64) },
    /// Data the caller wants, at most `limit` bytes of it.
    Capture { limit: usize },
}

pub(crate) struct Ran {
    pub stdout: Vec<u8>,
    /// The child's peak resident memory, in bytes.
    pub peak_memory: u64,
    /// The processor time it used, its own and the system's on its behalf.
    pub cpu: Duration,
}

/// How much of what a failing child said is kept for its error.
const STDERR_KEPT: usize = 4096;

/// Runs `program` with `args`, working on `path`, at `nice` below jawd's priority.
pub(crate) fn run(
    tool: &'static str,
    program: &Path,
    args: &[OsString],
    nice: i32,
    path: &Path,
    stdout: Stdout,
    cancel: &Cancel,
) -> Result<Ran> {
    cancel.check()?;
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // nice(2) is async-signal-safe and changes only the child, between fork and exec.
    unsafe {
        command.pre_exec(move || {
            libc::nice(nice);
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| Error::Tool { tool, path: path.into(), why: e.to_string() })?;
    let pid = child.id() as libc::pid_t;
    cancel.hold(pid);
    let (mut out, mut err) = (child.stdout.take().expect("stdout"), child.stderr.take().expect("stderr"));
    let (captured, said, over) = std::thread::scope(|scope| {
        let said = scope.spawn(move || {
            let mut tail = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > STDERR_KEPT * 2 {
                    tail.drain(..tail.len() - STDERR_KEPT);
                }
            }
            tail
        });
        let (captured, over) = match stdout {
            Stdout::Progress { duration, report } => {
                progress(&mut out, duration, report);
                (Vec::new(), None)
            }
            Stdout::Capture { limit } => capture(&mut out, limit, pid),
        };
        drop(out);
        (captured, said.join().unwrap_or_default(), over)
    });
    cancel.release();
    let (status, peak_memory, cpu) = reap(pid).map_err(Error::io(path))?;
    drop(child);
    cancel.check()?;
    if let Some(limit) = over {
        return Err(Error::TooLarge { path: path.into(), why: format!("{tool} gave more than {limit} bytes") });
    }
    if status != 0 {
        let said = String::from_utf8_lossy(&said);
        let said = said.trim();
        let why = if said.is_empty() { format!("it stopped with status {status}") } else { said.to_string() };
        return Err(Error::Tool { tool, path: path.into(), why });
    }
    Ok(Ran { stdout: captured, peak_memory, cpu })
}

fn progress(out: &mut ChildStdout, duration: Option<f64>, report: &mut dyn FnMut(f32, u64)) {
    let (mut at, mut bytes) = (0.0f64, 0u64);
    for line in BufReader::new(out).lines() {
        let Ok(line) = line else { break };
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "out_time_us" => at = value.parse::<f64>().map(|us| us / 1e6).unwrap_or(at),
            "total_size" => bytes = value.parse().unwrap_or(bytes),
            "progress" => {
                let done = match (value, duration) {
                    ("end", _) => 1.0,
                    (_, Some(d)) if d > 0.0 => (at / d).clamp(0.0, 1.0),
                    _ => 0.0,
                };
                report(done as f32, bytes);
            }
            _ => {}
        }
    }
}

/// Reads everything the child prints, killing it once it's printed more than `limit` bytes.
fn capture(out: &mut ChildStdout, limit: usize, pid: libc::pid_t) -> (Vec<u8>, Option<usize>) {
    // Reserved, not touched: only what's written is resident.
    let mut captured = Vec::with_capacity(limit);
    let mut buf = vec![0u8; 64 * 1024];
    while let Ok(n) = out.read(&mut buf) {
        if n == 0 {
            break;
        }
        if captured.len() + n > limit {
            kill(pid);
            return (Vec::new(), Some(limit));
        }
        captured.extend_from_slice(&buf[..n]);
    }
    (captured, None)
}

/// Waits for the child and returns its exit status (128 and the signal if a signal ended it), its
/// peak resident memory in bytes and the processor time it used.
fn reap(pid: libc::pid_t) -> std::io::Result<(i32, u64, Duration)> {
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        // The child is this process's own, not yet reaped, so wait4 is the one to reap it.
        if unsafe { libc::wait4(pid, &mut status, 0, &mut usage) } == pid {
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) };
    // macOS counts ru_maxrss in bytes, Linux in kilobytes.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    let time =
        |t: libc::timeval| Duration::from_secs(t.tv_sec.max(0) as u64) + Duration::from_micros(t.tv_usec.max(0) as u64);
    Ok((code, usage.ru_maxrss.max(0) as u64 * unit, time(usage.ru_utime) + time(usage.ru_stime)))
}
