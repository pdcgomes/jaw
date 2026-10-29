//! jawd stops on SIGTERM within its bound however its engine is doing: missing, so the supervisor
//! keeps trying to start it again, or never answering, so a start is waiting for its hello; and
//! however it was started, even with SIGTERM blocked, as jaw's terminal starts it; and `jawd demo`
//! stops on one while its phases still run.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// How long jawd has to stop, as its own bound says.
const BOUND: Duration = Duration::from_secs(3);

/// jawd on `home` with `engine` as its jaw-engine, once it listens.
fn jawd(home: &Path, engine: &Path) -> Child {
    jawd_started(home, engine, |_| {})
}

/// jawd as `jawd` starts it, with `starting` first doing to its command what a parent would.
fn jawd_started(home: &Path, engine: &Path, starting: impl FnOnce(&mut Command)) -> Child {
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
    let child = command.spawn().expect("jawd starts");
    let socket = home.join("jaw.sock");
    let ready = Instant::now() + Duration::from_secs(30);
    while !socket.exists() {
        assert!(Instant::now() < ready, "jawd never listened on {}", socket.display());
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

/// Sends SIGTERM, and says how jawd ended and how long it took.
fn terminate(child: &mut Child) -> (ExitStatus, Duration) {
    // SAFETY: kill only sends a signal to the child this test started.
    unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    let sent = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return (status, sent.elapsed());
        }
        if sent.elapsed() > BOUND * 2 {
            let _ = child.kill();
            panic!("jawd was still running {:?} after SIGTERM", sent.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn log(home: &Path) -> String {
    std::fs::read_to_string(home.join("jawd.log")).unwrap()
}

#[test]
fn jawd_whose_engine_is_missing_stops_on_sigterm_within_its_bound() {
    let home = tempfile::tempdir().unwrap();
    let mut child = jawd(home.path(), &home.path().join("no-such-engine"));
    // Long enough for the supervisor to have failed to start it and be waiting to try again.
    std::thread::sleep(Duration::from_millis(1_500));
    let (status, took) = terminate(&mut child);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("starting it again") && log.contains("jawd: stopped in"), "{log}");
    assert!(!home.path().join("jaw.sock").exists(), "the socket is removed");
}

#[test]
fn jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it() {
    let home = tempfile::tempdir().unwrap();
    // A sleep no other process has, so the test can tell whether it outlived jawd.
    let marker = format!("{}.{}", 7_000 + std::process::id() % 1_000, std::process::id());
    let engine = home.path().join("silent-engine");
    std::fs::write(&engine, format!("#!/bin/sh\nexec sleep {marker}\n")).unwrap();
    std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = jawd(home.path(), &engine);
    let running = || Command::new("pgrep").args(["-f", &format!("sleep {marker}")]).output().unwrap().status.success();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !running() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(running(), "the engine started and is waiting to say hello");
    let (status, took) = terminate(&mut child);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    let gone = Instant::now() + Duration::from_secs(1);
    while running() && Instant::now() < gone {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!running(), "the engine outlived jawd");
}

#[test]
fn jawd_started_with_sigterm_blocked_as_jaw_starts_it_still_stops_on_sigterm_within_its_bound() {
    let home = tempfile::tempdir().unwrap();
    let mut child = jawd_started(home.path(), &home.path().join("no-such-engine"), |command| {
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
    let socket = home.path().join("jaw.sock");
    let ready = Instant::now() + Duration::from_secs(30);
    while jaw_client::Connection::open(&socket, "stopping test", "0").is_err() {
        assert!(Instant::now() < ready, "jawd never answered hello:\n{}", log(home.path()));
        std::thread::sleep(Duration::from_millis(20));
    }
    let (status, took) = terminate(&mut child);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM, so it stops"), "{log}");
}

#[test]
fn jawd_demo_stops_on_one_sigterm_while_its_phases_still_run() {
    let home = tempfile::tempdir().unwrap();
    let out = std::fs::File::create(home.path().join("jawd.log")).unwrap();
    // A small account, whose history goes in at once, and then ten minutes before the next phase.
    let mut child = Command::new(env!("CARGO_BIN_EXE_jawd"))
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
    // Its phases start once it says what jaw would show.
    let ready = Instant::now() + Duration::from_secs(30);
    while !log(home.path()).contains("jaw shows them") {
        assert!(Instant::now() < ready, "jawd demo never served:\n{}", log(home.path()));
        std::thread::sleep(Duration::from_millis(20));
    }
    let (status, took) = terminate(&mut child);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd demo: SIGTERM, so it stops") && log.contains("jawd demo: stopped in"), "{log}");
    assert!(!log.contains("serving until Ctrl-C"), "the one signal stops it, with no second to wait for:\n{log}");
}
