//! jawd stops on SIGTERM within its bound however its engine is doing: missing, so the supervisor
//! keeps trying to start it again, or never answering, so a start is waiting for its hello; and
//! however it was started, even with SIGTERM blocked, as jaw's terminal starts it; and `jawd demo`
//! stops on one while its phases still run. Stopped while it starts, it takes its engine with it
//! however late its runtime's threads run at the end. However it stops, on SIGTERM or Ctrl-C, on a
//! second one at once, or failing to start, its engine has gone by the time it has, ended in order
//! when there's time, and until then it holds jaw.lock, so no other jawd starts beside it. Whichever
//! way a test ends, it leaves neither jawd nor an engine of its own running.

use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use prost::Message as _;
use wa_proto::jaw::engine as pb;

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

/// A fake jaw-engine, a shell script that first writes down its PID, so the test can tell whether
/// it outlived jawd. Should it still be running when the test ends, however the test ends, it's
/// killed.
struct Fake {
    program: PathBuf,
    pid: PathBuf,
    /// What `ps` shows it running, which tells it from a process that has taken its PID since.
    runs: String,
}

impl Fake {
    /// An engine that never says hello and never reads what jawd writes to it: a sleep no other
    /// process runs.
    fn silent(home: &Path) -> Fake {
        let sleep = sleep();
        Fake::new(home, &format!("exec {sleep}"), Some(sleep))
    }

    /// An engine that never says hello, never reads what jawd writes to it and ignores SIGTERM, so
    /// ended in order it runs on through both pauses, until SIGKILL.
    fn stubborn(home: &Path) -> Fake {
        let sleep = sleep();
        Fake::new(home, &format!("trap '' TERM\nexec {sleep}"), Some(sleep))
    }

    /// An engine that says hello, and then never answers or reads what jawd writes to it.
    fn mute(home: &Path) -> Fake {
        let sleep = sleep();
        Fake::new(home, &format!("{}\nexec {sleep}", hello(home)), Some(sleep))
    }

    /// An engine that runs `then` in its shell.
    fn shell(home: &Path, then: &str) -> Fake {
        Fake::new(home, then, None)
    }

    fn new(home: &Path, then: &str, runs: Option<String>) -> Fake {
        let program = home.join("fake-engine");
        let pid = home.join("engine.pid");
        std::fs::write(&program, format!("#!/bin/sh\necho $$ > '{}'\n{then}\n", pid.display())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let runs = runs.unwrap_or_else(|| program.display().to_string());
        Fake { program, pid, runs }
    }

    /// Its PID while it runs: the one it wrote down, while that process still runs it.
    fn running(&self) -> Option<i32> {
        let pid: i32 = std::fs::read_to_string(&self.pid).ok()?.trim().parse().ok()?;
        let ps = Command::new("ps").args(["-ww", "-o", "command=", "-p", &pid.to_string()]).output().ok()?;
        String::from_utf8_lossy(&ps.stdout).contains(&self.runs).then_some(pid)
    }

    /// Waits until it runs.
    fn started(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.running().is_none() {
            assert!(Instant::now() < deadline, "the engine never started");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        if let Some(pid) = self.running() {
            // SAFETY: kill only sends a signal, to the engine this test made, still running it.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
}

/// A sleep no other process runs.
fn sleep() -> String {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let id = std::process::id();
    format!("sleep {}.{id}{}", 7_000 + id % 1_000, MADE.fetch_add(1, Ordering::Relaxed))
}

/// A line of shell that says the hello jawd takes from its engine.
fn hello(home: &Path) -> String {
    let hello = pb::Hello {
        protocol: pb::Version::Current as u32,
        program: "a fake jaw-engine".into(),
        whatsmeow: wa_proto::whatsmeow().into(),
    };
    let data = pb::Frame { id: 0, body: Some(pb::frame::Body::Hello(hello)) }.encode_to_vec();
    let mut frame = (data.len() as u32).to_be_bytes().to_vec();
    frame.extend(data);
    let path = home.join("hello");
    std::fs::write(&path, frame).unwrap();
    format!("cat '{}'", path.display())
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
    stop_with(jawd, libc::SIGTERM)
}

/// Sends `signal`, and says how jawd ended and how long it took.
fn stop_with(jawd: &mut Jawd, signal: i32) -> (ExitStatus, Duration) {
    // SAFETY: kill only sends a signal to the jawd this test started.
    unsafe { libc::kill(jawd.0.id() as i32, signal) };
    exits(jawd, Instant::now())
}

/// Waits for jawd to exit, once told to stop at `told`, and says how it ended and how long after.
fn exits(jawd: &mut Jawd, told: Instant) -> (ExitStatus, Duration) {
    loop {
        if let Some(status) = jawd.0.try_wait().unwrap() {
            return (status, told.elapsed());
        }
        assert!(told.elapsed() < BOUND * 2, "jawd was still running {:?} after it was told to stop", told.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether jaw.lock in `home` is free, as a second jawd would find it; taken, it's let go at once.
fn jaw_lock_free(home: &Path) -> bool {
    let file = std::fs::OpenOptions::new().write(true).open(home.join("jaw.lock")).unwrap();
    // SAFETY: flock only locks the file opened here, which outlives the call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return true;
    }
    let e = std::io::Error::last_os_error();
    assert_eq!(e.raw_os_error(), Some(libc::EWOULDBLOCK), "{e}");
    false
}

fn log(home: &Path) -> String {
    std::fs::read_to_string(home.join("jawd.log")).unwrap()
}

/// Waits until jawd's log says `said`.
fn says(home: &Path, said: &str) {
    let ready = Instant::now() + Duration::from_secs(30);
    while !log(home).contains(said) {
        assert!(Instant::now() < ready, "jawd never said {said:?}:\n{}", log(home));
        std::thread::sleep(Duration::from_millis(20));
    }
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

/// Waits until jawd says its engine is running, once the engine has said hello.
fn engine_running(home: &Path) {
    let socket = home.join("jaw.sock");
    let ready = Instant::now() + Duration::from_secs(30);
    loop {
        let running = jaw_client::Connection::open(&socket, "stopping test", "0")
            .and_then(|(mut connection, _)| connection.call("status", serde_json::Value::Null))
            .is_ok_and(|status| status["engine"]["state"] == "running");
        if running {
            return;
        }
        assert!(Instant::now() < ready, "jawd's engine never ran:\n{}", log(home));
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn jawd_whose_engine_is_missing_stops_on_sigterm_within_its_bound() {
    let home = tempfile::tempdir().unwrap();
    let mut jawd = jawd(home.path(), &home.path().join("no-such-engine"));
    // Started, however long that takes, and its supervisor waiting to try the engine again.
    answering(home.path());
    says(home.path(), "starting it again");
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("starting it again") && log.contains("jawd: stopped in"), "{log}");
    assert!(!home.path().join("jaw.sock").exists(), "the socket is removed");
}

/// jawd with an engine that never says hello, stopped with `signal` once the engine runs: it stops
/// within its bound, and its engine has gone by the time it has.
fn takes_its_silent_engine_with_it(signal: i32, said: &str) {
    let home = tempfile::tempdir().unwrap();
    let engine = Fake::silent(home.path());
    let mut jawd = jawd(home.path(), &engine.program);
    engine.started();
    let (status, took) = stop_with(&mut jawd, signal);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains(said), "{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    assert!(log.contains("jaw-engine didn't exit once its standard input closed, so SIGTERM ended it"), "{log}");
}

#[test]
fn jawd_whose_engine_never_answers_stops_on_sigterm_and_takes_its_engine_with_it() {
    takes_its_silent_engine_with_it(libc::SIGTERM, "jawd: SIGTERM");
}

#[test]
fn jawd_whose_engine_never_answers_stops_on_ctrl_c_and_takes_its_engine_with_it() {
    takes_its_silent_engine_with_it(libc::SIGINT, "jawd: Ctrl-C");
}

#[test]
fn jawd_stopped_while_it_starts_takes_its_engine_with_it_however_late_its_runtime_ends() {
    let home = tempfile::tempdir().unwrap();
    let engine = Fake::silent(home.path());
    // Its start held once the engine is started, so SIGTERM comes while it starts, and its
    // runtime's threads kept busy as it shuts down, as a Mac under load can keep them.
    let mut jawd = jawd_started(home.path(), &engine.program, |command| {
        command.args(["--hold-start", "--busy-at-exit", "1"]);
    });
    engine.started();
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM while starting, so it stops"), "{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    assert!(log.contains("jawd: jaw-engine didn't exit once its standard input closed, so SIGTERM ended it"), "{log}");
}

#[test]
fn a_second_sigterm_while_jawd_ends_its_engine_in_order_kills_it_at_once() {
    let home = tempfile::tempdir().unwrap();
    let engine = Fake::stubborn(home.path());
    let mut jawd = jawd_started(home.path(), &engine.program, |command| {
        command.arg("--hold-start");
    });
    engine.started();
    // SAFETY: kill only sends a signal to the jawd this test started.
    unsafe { libc::kill(jawd.0.id() as i32, libc::SIGTERM) };
    let first = Instant::now();
    // Stopped while it starts, it ends its engine in order, whose first pause is a second long.
    says(home.path(), "jawd: SIGTERM while starting, so it stops");
    std::thread::sleep(Duration::from_millis(100));
    let (status, _) = terminate(&mut jawd);
    let took = first.elapsed();
    let log = log(home.path());
    // Before the end in order's two pauses, a second and half a second, would have killed it.
    assert!(status.code() == Some(1) && took < Duration::from_millis(1500), "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM again, so it stops at once"), "{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    assert!(log.contains("jawd: jaw-engine was killed at once, with SIGKILL"), "{log}");
}

#[test]
fn jawd_stopped_while_it_starts_holds_jaw_lock_until_its_engine_has_gone() {
    let home = tempfile::tempdir().unwrap();
    let engine = Fake::stubborn(home.path());
    let mut jawd = jawd_started(home.path(), &engine.program, |command| {
        command.arg("--hold-start");
    });
    engine.started();
    // SAFETY: kill only sends a signal to the jawd this test started.
    unsafe { libc::kill(jawd.0.id() as i32, libc::SIGTERM) };
    let told = Instant::now();
    let mut held = 0;
    loop {
        // The lock first, so one found free while the engine still runs after was free as it ran.
        let free = jaw_lock_free(home.path());
        if engine.running().is_none() {
            break;
        }
        assert!(
            !free,
            "jaw.lock was free {:?} after SIGTERM, while the engine still ran:\n{}",
            told.elapsed(),
            log(home.path())
        );
        held += 1;
        assert!(told.elapsed() < BOUND * 2, "the engine was still running {:?} after SIGTERM", told.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
    let (status, took) = exits(&mut jawd, told);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(held > 0, "the engine had gone before jaw.lock was looked at:\n{log}");
    assert!(log.contains("jawd: SIGTERM while starting, so it stops"), "{log}");
    assert!(log.contains("jawd: jaw-engine didn't exit on SIGTERM either, so SIGKILL ended it"), "{log}");
    assert!(jaw_lock_free(home.path()), "jaw.lock went with jawd");
}

#[test]
fn jawd_ends_its_engine_in_order_and_waits_until_it_has_exited_before_it_does() {
    let home = tempfile::tempdir().unwrap();
    let said = home.path().join("said");
    // It never says hello, and once its standard input closes, as jaw-engine exits then, it takes
    // a moment to exit, saying so first.
    let engine = Fake::shell(home.path(), &format!("cat > /dev/null\nsleep 0.1\necho ended > '{}'", said.display()));
    let mut jawd = jawd(home.path(), &engine.program);
    answering(home.path());
    engine.started();
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(said.is_file(), "jawd exited before its engine had:\n{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    assert!(log.contains("jawd: jaw-engine exited once its standard input closed (exit status: 0)"), "{log}");
}

#[test]
fn a_second_sigterm_stops_jawd_at_once_and_its_engine_with_it() {
    let home = tempfile::tempdir().unwrap();
    // Its engine said hello, and never answers, so the stop waits for it to disconnect.
    let engine = Fake::mute(home.path());
    let mut jawd = jawd(home.path(), &engine.program);
    engine_running(home.path());
    // SAFETY: kill only sends a signal to the jawd this test started.
    unsafe { libc::kill(jawd.0.id() as i32, libc::SIGTERM) };
    says(home.path(), "jawd: SIGTERM, so it stops");
    std::thread::sleep(Duration::from_millis(200));
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    // Well before the stop in order would have ended, which waits for the engine to disconnect.
    assert!(status.code() == Some(1) && took < Duration::from_secs(2), "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd: SIGTERM again, so it stops at once"), "{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    assert!(log.contains("jawd: jaw-engine was killed at once, with SIGKILL"), "{log}");
}

#[test]
fn jawd_that_fails_to_start_takes_its_engine_with_it() {
    let home = tempfile::tempdir().unwrap();
    // Where media keeps its files, a file, so media's start fails, after the engine's has begun.
    std::fs::write(home.path().join("media"), "").unwrap();
    let engine = Fake::silent(home.path());
    let mut jawd = jawd(home.path(), &engine.program);
    let deadline = Instant::now() + BOUND * 2;
    let mut held = 0;
    let status = loop {
        // The lock first, so one found free while the engine still runs after was free as it ran.
        let free = jaw_lock_free(home.path());
        if let Some(status) = jawd.0.try_wait().unwrap() {
            break status;
        }
        if engine.running().is_some() {
            assert!(!free, "jaw.lock was free while the engine still ran:\n{}", log(home.path()));
            held += 1;
        }
        assert!(Instant::now() < deadline, "jawd was still running:\n{}", log(home.path()));
        std::thread::sleep(Duration::from_millis(10));
    };
    let log = log(home.path());
    assert_eq!(status.code(), Some(1), "{log}");
    assert!(log.lines().last().is_some_and(|last| last.starts_with("jawd: ")), "it says why last:\n{log}");
    assert!(engine.running().is_none(), "the engine outlived jawd:\n{log}");
    if engine.pid.exists() {
        assert!(held > 0, "the engine ran, but jaw.lock was never looked at while it did:\n{log}");
        assert!(
            log.contains("jawd: jaw-engine didn't exit once its standard input closed, so SIGTERM ended it"),
            "{log}"
        );
    }
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
    says(home.path(), "jaw shows them");
    let (status, took) = terminate(&mut jawd);
    let log = log(home.path());
    assert!(status.success() && took < BOUND, "{status} after {took:?}:\n{log}");
    assert!(log.contains("jawd demo: SIGTERM, so it stops") && log.contains("jawd demo: stopped in"), "{log}");
    assert!(!log.contains("serving until Ctrl-C"), "the one signal stops it, with no second to wait for:\n{log}");
}
