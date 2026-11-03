//! jawd stops on SIGTERM within its bound however its engine is doing: missing, so the supervisor
//! keeps trying to start it again, or never answering, so a start is waiting for its hello; and
//! however it was started, even with SIGTERM blocked, as jaw's terminal starts it; and `jawd demo`
//! stops on one while its phases still run. Stopped while it starts, it takes its engine with it
//! however late its runtime's threads run at the end. Whichever way a test ends, it leaves neither
//! jawd nor an engine of its own running.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// How long jawd has to stop, as its own bound says.
const BOUND: Duration = Duration::from_secs(3);

/// A jawd the test started, killed should the test end while it runs.
struct Jawd(Child);

impl Drop for Jawd {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// An engine that never says hello and never reads what jawd writes to it: a sleep no other
/// process runs, which first writes down its PID, so the test can tell whether it outlived jawd.
/// Should it still be running when the test ends, however the test ends, it's killed.
struct Silent {
    program: PathBuf,
    pid: PathBuf,
    command: String,
}

impl Silent {
    fn new(home: &Path) -> Silent {
        static MADE: AtomicU32 = AtomicU32::new(0);
        let id = std::process::id();
        let command = format!("sleep {}.{id}{}", 7_000 + id % 1_000, MADE.fetch_add(1, Ordering::Relaxed));
        let program = home.join("silent-engine");
        let pid = home.join("engine.pid");
        std::fs::write(&program, format!("#!/bin/sh\necho $$ > '{}'\nexec {command}\n", pid.display())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Silent { program, pid, command }
    }

    /// Its PID while it runs: the one it wrote down, while that process still runs its sleep.
    fn running(&self) -> Option<i32> {
        let pid: i32 = std::fs::read_to_string(&self.pid).ok()?.trim().parse().ok()?;
        let ps = Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]).output().ok()?;
        (String::from_utf8_lossy(&ps.stdout).trim() == self.command).then_some(pid)
    }
}

impl Drop for Silent {
    fn drop(&mut self) {
        if let Some(pid) = self.running() {
            // SAFETY: kill only sends a signal, to the engine this test made, still running its sleep.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// jawd on `home` with `engine` as its jaw-engine, once it listens.
fn jawd(home: &Path, engine: &Path) -> Jawd {
    jawd_started(home, engine, |_| {})
}

/// jawd as `jawd` starts it, with `starting` first doing to its command what a parent would.
fn jawd_started(home: &Path, engine: &Path, starting: impl FnOnce(&mut Command)) -> Jawd {
    let log = std::fs::File::create(home.join("jawd.log")).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jawd"));
    command
        .env("JAW_HOME", home)
        .env("JAW_ENGINE", engine)
        .env("JAW_NOTIFY", "off")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    starting(&mut command);
    let jawd = Jawd(command.spawn().expect("jawd starts"));
    let socket = home.join("jaw.sock");
    let ready = Instant::now() + Duration::from_secs(30);
    while !socket.exists() {
        assert!(Instant::now() < ready, "jawd never listened on {}", socket.display());
        std::thread::sleep(Duration::from_millis(20));
    }
    jawd
}

/// Sends SIGTERM, and says how jawd ended and how long it took.
fn terminate(jawd: &mut Jawd) -> (ExitStatus, Duration) {
    // SAFETY: kill only sends a signal to the jawd this test started.
    unsafe { libc::kill(jawd.0.id() as i32, libc::SIGTERM) };
    let sent = Instant::now();
    loop {
        if let Some(status) = jawd.0.try_wait().unwrap() {
            return (status, sent.elapsed());
        }
        assert!(sent.elapsed() < BOUND * 2, "jawd was still running {:?} after SIGTERM", sent.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn log(home: &Path) -> String {
    std::fs::read_to_string(home.join("jawd.log")).unwrap()
}

/// Waits until jawd answers hello, which it does only once its start is over and it serves.
fn answering(home: &Path) {
    let socket = home.join("jaw.sock");
    let ready = Instant::now() + Duration::from_secs(30);
    while jaw_client::Connection::open(&socket, "stopping test", "0").is_err() {
        assert!(Instant::now() < ready, "jawd never answered hello:\n{}", log(home));
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn jawd_whose_engine_is_missing_stops_on_sigterm_within_its_bound() {
    let home = tempfile::tempdir().unwrap();
    let mut jawd = jawd(home.path(), &home.path().join("no-such-engine"));
    // Started, however long that takes, and its supervisor waiting to try the engine again.
    answering(home.path());
    let ready = Instant::now() + Duration::from_secs(30);
    while !log(home.path()).contains("starting it again") {
        assert!(Instant::now() < ready, "jawd never tried its engine again:\n{}", log(home.path()));
        std::thread::sleep(Duration::from_millis(20));
    }
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("starting it again") && log.contains("jawd: stopped in"), "{log}");
    assert!(!home.path().join("jaw.sock").exists(), "the socket is removed");
}

#[test]
fn jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it() {
    let home = tempfile::tempdir().unwrap();
    let engine = Silent::new(home.path());
    let mut jawd = jawd(home.path(), &engine.program);
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.running().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(engine.running().is_some(), "the engine started and is waiting to say hello");
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    let gone = Instant::now() + Duration::from_secs(1);
    while engine.running().is_some() && Instant::now() < gone {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
}

#[test]
fn jawd_stopped_while_it_starts_takes_its_engine_with_it_however_late_its_runtime_ends() {
    let home = tempfile::tempdir().unwrap();
    let engine = Silent::new(home.path());
    // Its start held once the engine is started, so SIGTERM comes while it starts, and its
    // runtime's threads kept busy as it shuts down, as a Mac under load can keep them.
    let mut jawd = jawd_started(home.path(), &engine.program, |command| {
        command.args(["--hold-start", "--busy-at-exit", "1"]);
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.running().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(engine.running().is_some(), "the engine started and is waiting to say hello");
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM while starting, so it stops"), "{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
}

#[test]
fn jawd_started_with_sigterm_blocked_as_jaw_starts_it_still_stops_on_sigterm_within_its_bound() {
    let home = tempfile::tempdir().unwrap();
    let mut jawd = jawd_started(home.path(), &home.path().join("no-such-engine"), |command| {
        // SAFETY: between fork and exec the child only sets its signal mask, which is
        // async-signal-safe.
        unsafe {
            command.pre_exec(|| {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, libc::SIGTERM);
                libc::sigaddset(&mut set, libc::SIGHUP);
                libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
                Ok(())
            });
        }
    });
    // Once hello is answered jawd is serving, where a `kill` after jaw says to stop it comes.
    answering(home.path());
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM, so it stops"), "{log}");
}

#[test]
fn jawd_demo_stops_on_one_sigterm_while_its_phases_still_run() {
    let home = tempfile::tempdir().unwrap();
    let out = std::fs::File::create(home.path().join("jawd.log")).unwrap();
    // A small account, whose history goes in at once, and then ten minutes before the next phase.
    let child = Command::new(env!("CARGO_BIN_EXE_jawd"))
        .env("JAW_HOME", home.path())
        .env("JAW_ENGINE", home.path().join("no-such-engine"))
        .env("JAW_NOTIFY", "off")
        .arg("demo")
        .arg("--folder")
        .arg(home.path().join("demo"))
        .args(["--chats", "3", "--people", "3", "--messages", "30", "--blob", "10", "--after", "600"])
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .expect("jawd demo starts");
    let mut jawd = Jawd(child);
    // Its phases start once it says what jaw would show.
    let ready = Instant::now() + Duration::from_secs(30);
    while !log(home.path()).contains("jaw shows them") {
        assert!(Instant::now() < ready, "jawd demo never served:\n{}", log(home.path()));
        std::thread::sleep(Duration::from_millis(20));
    }
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd demo: SIGTERM, so it stops") && log.contains("jawd demo: stopped in"), "{log}");
    assert!(!log.contains("serving until Ctrl-C"), "the one signal stops it, with no second to wait for:\n{log}");
}
