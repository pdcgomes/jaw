//! The client against a real jawd. `jawd bench feed` serves a made-up account while it writes
//! 2,000 messages a second: the client falls behind on it until jawd tells it to watch again, and
//! it's killed mid-stream and its jaw.db served again by `jawd --engine demo`, as a jawd that
//! crashed and started again would. Each test's jawd has its own folder and socket.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jaw_client::{Chats, Client, Config, Event, Failure, Frame, Messages, Place, WindowId};
use jaw_proto::find::{Find, Kind};
use jaw_proto::windows::{ChatRow, MessageRow};
use serde_json::Value;

/// `jawd bench feed`'s open chat, where one message in four goes.
const OPEN: &str = "351900000000@s.whatsapp.net";

/// jawd, built as this test was, beside it.
fn jawd() -> &'static Path {
    static JAWD: OnceLock<PathBuf> = OnceLock::new();
    JAWD.get_or_init(|| {
        let exe = std::env::current_exe().expect("the test's path");
        let profile = exe.parent().and_then(Path::parent).expect("the test sits in target/<profile>/deps");
        let mut build = Command::new(env!("CARGO"));
        build.args(["build", "--quiet", "-p", "jawd", "--bin", "jawd"]).current_dir(env!("CARGO_MANIFEST_DIR"));
        if profile.file_name().is_some_and(|name| name == "release") {
            build.arg("--release");
        }
        assert!(build.status().expect("cargo runs").success(), "jawd didn't build");
        profile.join("jawd")
    })
}

/// A jawd running for a test, killed when it's dropped.
struct Jawd {
    child: Child,
    /// Its folder, with jaw.db and jaw.sock.
    home: PathBuf,
}

impl Jawd {
    /// `jawd bench feed` writing for `seconds`, its folder in `scratch`; returns once it serves.
    fn bench(scratch: &Path, seconds: u32) -> Jawd {
        let child = Command::new(jawd())
            .args(["bench", "feed", "--rate", "2000", "--seconds", &seconds.to_string(), "--chats", "20"])
            .env("TMPDIR", scratch)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("jawd bench feed starts");
        let home = scratch.join(format!("jaw-bench-feed-{}", child.id()));
        let jawd = Jawd { child, home };
        let ready = Instant::now() + Duration::from_secs(120);
        while !jawd.socket().exists() {
            assert!(Instant::now() < ready, "jawd bench feed never served on {}", jawd.socket().display());
            std::thread::sleep(Duration::from_millis(20));
        }
        jawd
    }

    /// `jawd --engine demo` on `home`.
    fn demo(home: &Path) -> Jawd {
        let child = Command::new(jawd())
            .args(["--engine", "demo"])
            .env("JAW_HOME", home)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("jawd starts");
        Jawd { child, home: home.to_path_buf() }
    }

    fn socket(&self) -> PathBuf {
        jaw_proto::local::socket(&self.home)
    }

    fn signal(&self, signal: i32) {
        // SAFETY: kill only sends a signal to the child this test started.
        unsafe { libc::kill(self.child.id() as i32, signal) };
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An interface's models for the chat list and the open chat, fed from its client's frames.
struct Interface {
    client: Client,
    list: WindowId,
    open: WindowId,
    chats: Chats<ChatRow>,
    messages: Messages<MessageRow>,
    events: Vec<Event>,
}

impl Interface {
    fn new(config: Config) -> Interface {
        let client: Client = Client::start(config, || {});
        let list = client.watch_chats("all", Place::top(60));
        let open = client.watch_messages(OPEN, Place::newest(100));
        Interface {
            client,
            list,
            open,
            chats: Chats::new("all", Place::top(60)),
            messages: Messages::new(OPEN, Place::newest(100)),
            events: Vec::new(),
        }
    }

    fn apply(&mut self, frame: Frame<ChatRow, MessageRow>) {
        for (window, update) in frame.chats {
            assert_eq!(window, self.list);
            self.chats.apply(update);
        }
        for (window, update) in frame.messages {
            assert_eq!(window, self.open);
            self.messages.apply(update);
        }
        self.events.extend(frame.events);
    }

    /// Applies what has come, after waiting a moment for it.
    fn step(&mut self) {
        self.client.wait(Duration::from_millis(20));
        let frame = self.client.drain();
        self.apply(frame);
    }

    /// Applies frames until `done` holds, for up to `within`.
    fn until(&mut self, within: Duration, what: &str, mut done: impl FnMut(&Interface) -> bool) {
        let until = Instant::now() + within;
        while !done(self) {
            assert!(Instant::now() < until, "{what} didn't happen within {within:?}; events: {:?}", self.events);
            self.step();
        }
    }

    fn loaded(&self) -> bool {
        self.chats.loaded && self.messages.loaded
    }

    fn disconnected(&self) -> bool {
        self.events.iter().any(|e| matches!(e, Event::Disconnected { .. }))
    }

    /// What it shows, to compare with another interface's.
    fn shown(&self) -> (Vec<ChatRow>, u64, Vec<MessageRow>, u64, bool, u64) {
        let m = &self.messages;
        (self.chats.rows().cloned().collect(), self.chats.total, m.rows().cloned().collect(), m.total, m.older, m.newer)
    }
}

#[test]
fn a_jawd_killed_mid_stream_and_started_again_ends_with_the_windows_a_fresh_watch_gives() {
    let scratch = tempfile::tempdir().unwrap();
    let bench = Jawd::bench(scratch.path(), 60);
    let config = Config::new(bench.socket(), "test", "0");
    let mut ui = Interface::new(config.clone());
    ui.until(Duration::from_secs(10), "the windows' first rows", Interface::loaded);
    let seq = ui.messages.seq;
    ui.until(Duration::from_secs(10), "messages arriving", |ui| ui.messages.seq > seq + 1_000);

    // Frozen, jawd can't answer; killed, its connection drops with the request still in flight.
    bench.signal(libc::SIGSTOP);
    let asked = ui.client.request("status", Value::Null).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let home = bench.home.clone();
    drop(bench);
    ui.until(Duration::from_secs(5), "the request failing", |ui| {
        ui.events.contains(&Event::Reply { id: asked, outcome: Err(Failure::Lost) }) && ui.disconnected()
    });
    let away = ui.shown();
    assert_eq!((away.0.len(), away.2.len()), (20, 100), "the models keep what they had while jawd is away");
    ui.until(Duration::from_secs(1), "saying jawd isn't running", |ui| {
        ui.events.iter().any(|e| matches!(e, Event::Disconnected { why, .. } if why.contains("jawd isn't running")))
    });

    let _jawd = Jawd::demo(&home);
    ui.until(Duration::from_secs(30), "connecting again", |ui| ui.client.stats().connects == 2);
    let mut fresh = Interface::new(config);
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        ui.step();
        fresh.step();
        if fresh.loaded() && ui.shown() == fresh.shown() {
            break;
        }
        assert!(
            Instant::now() < until,
            "the windows watched again never matched a fresh watch:\n{:?}\n{:?}",
            ui.shown(),
            fresh.shown()
        );
    }
    assert!(ui.messages.seq > seq, "jaw.db kept what was written before the kill");
}

#[test]
fn the_palette_keeps_only_the_answer_to_its_latest_find() {
    // The bench's jaw.db, served by a jawd that finds things in it.
    let scratch = tempfile::tempdir().unwrap();
    let home = Jawd::bench(scratch.path(), 30).home.clone();
    let jawd = Jawd::demo(&home);
    let mut ui = Interface::new(Config::new(jawd.socket(), "test", "0"));
    ui.until(Duration::from_secs(30), "the windows' first rows", Interface::loaded);
    let typed: Vec<u64> = ["s", "su", "sunday"]
        .iter()
        .map(|query| ui.client.find(Find { query: query.to_string(), ..Find::default() }).unwrap())
        .collect();
    let last = ui.client.request("status", Value::Null).unwrap();
    ui.until(Duration::from_secs(10), "the last find's answer", |ui| {
        ui.events.iter().any(|e| matches!(e, Event::Found { .. }))
            && ui.events.iter().any(|e| matches!(e, Event::Reply { id, .. } if *id == last))
    });
    let found: Vec<&Event> = ui.events.iter().filter(|e| matches!(e, Event::Found { .. })).collect();
    assert_eq!(found.len(), 1, "{found:?}");
    let Event::Found { id, query, outcome: Ok(found) } = found[0] else { panic!("{found:?}") };
    assert_eq!((*id, query.as_str()), (typed[2], "sunday"));
    assert_eq!(found.results[0].kind, Kind::Message, "the made-up account's messages ask about Sunday");
}

#[test]
fn an_interface_that_falls_behind_is_told_to_watch_again_and_ends_as_a_fresh_one() {
    let scratch = tempfile::tempdir().unwrap();
    let bench = Jawd::bench(scratch.path(), 10);
    let mut config = Config::new(bench.socket(), "test", "0");
    let mut fresh = Interface::new(config.clone());
    config.events = 1;
    let mut slow = Interface::new(config);
    for ui in [&mut slow, &mut fresh] {
        ui.until(Duration::from_secs(10), "the windows' first rows", Interface::loaded);
    }

    // With room for one event, the second reply stops the slow interface's reading, so jawd's
    // queue to it fills while messages pour in, and after 5 seconds jawd drops its windows.
    for _ in 0..3 {
        slow.client.request("status", Value::Null).unwrap();
    }
    std::thread::sleep(Duration::from_secs(9));
    slow.until(Duration::from_secs(10), "jawd's resync", |ui| ui.client.stats().resyncs == 2);

    // Once the bench has written its last message and gone, both hold what they last had.
    for ui in [&mut slow, &mut fresh] {
        ui.until(Duration::from_secs(60), "the bench ending", Interface::disconnected);
    }
    assert_eq!(slow.client.stats().connects, 1, "the slow interface stayed connected");
    assert_eq!(slow.shown(), fresh.shown(), "the windows watched again match the ones watched throughout");
    assert_eq!((slow.chats.len(), slow.messages.len()), (20, 100));
    drop(bench);
}
