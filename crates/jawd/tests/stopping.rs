//! jawd stops on SIGTERM within its bound however its engine is doing: missing, so the supervisor
//! keeps trying to start it again, or never answering, so a start is waiting for its hello.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// How long jawd has to stop, as its own bound says.
const BOUND: Duration = Duration::from_secs(3);

/// jawd on `home` with `engine` as its jaw-engine, once it listens.
fn jawd(home: &Path, engine: &Path) -> Child {
    let log = std::fs::File::create(home.join("jawd.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_jawd"))
        .env("JAW_HOME", home)
        .env("JAW_ENGINE", engine)
        .env("JAW_NOTIFY", "off")
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .expect("jawd starts");
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
