//! Contact cards end to end: `jaw snapshot --keys`, the binary as the owner runs it, on a `jawd
//! --engine demo` of its own, whose made-up account brings Ana's dentist's card and two climbers'
//! in one message and takes yours. jawd is built beside this test, runs on a temporary folder that
//! each child is pointed at with `JAW_HOME`, and is killed through its own child when the test is
//! done. Everything the test waits for has a deadline, so a jawd or a `jaw` that never answers
//! fails it rather than holding the gate.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long a fresh jawd has to answer and play its made-up account, on a loaded machine.
const JAWD_PLAYS: Duration = Duration::from_secs(60);
/// How long one `jaw snapshot` has to finish.
const JAW_FINISHES: Duration = Duration::from_secs(60);
const ANA: &str = "351912345678@s.whatsapp.net";

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

/// A `jawd --engine demo` on a temporary folder, killed when it's dropped, and the folder removed.
struct Jawd {
    child: Child,
    home: tempfile::TempDir,
}

impl Jawd {
    /// Starts one and returns once its made-up account is played: its two chats in the list, the
    /// group named, as the demo's last event names it once its messages are in.
    fn demo() -> Jawd {
        let home = tempfile::tempdir().unwrap();
        let log = File::create(home.path().join("jawd.log")).unwrap();
        let child = Command::new(jawd())
            .args(["--engine", "demo"])
            .env("JAW_HOME", home.path())
            .env("JAW_NOTIFY", "off")
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("jawd starts");
        let jawd = Jawd { child, home };
        let socket = jaw_proto::local::socket(jawd.home.path());
        let end = Instant::now() + JAWD_PLAYS;
        let (played, plays) = channel();
        let asking = socket.clone();
        std::thread::spawn(move || {
            while Instant::now() < end {
                let watch = json!({ "folder": "all", "around": "top", "before": 0, "after": 10 });
                let named =
                    |rows: &Value| rows.as_array().is_some_and(|r| r.iter().any(|c| c["name"] == "Lisbon Climbing"));
                if let Ok((mut connection, _)) = jaw_client::Connection::open(&asking, "jaw-test", "0")
                    && connection.call("chats.watch", watch).is_ok_and(|v| named(&v["rows"]))
                {
                    let _ = played.send(());
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        if plays.recv_timeout(end.saturating_duration_since(Instant::now())).is_err() {
            let log = std::fs::read_to_string(jawd.home.path().join("jawd.log")).unwrap_or_default();
            panic!("jawd didn't play its account on {} within {JAWD_PLAYS:?}:\n{log}", socket.display());
        }
        jawd
    }

    /// One frame of `jaw snapshot` on this jawd after `keys`, as text. One still running after
    /// `JAW_FINISHES` is killed, through its own child, and fails the test with what it printed.
    fn snapshot(&self, keys: &str) -> String {
        let (mut out, mut err) = (tempfile::tempfile().unwrap(), tempfile::tempfile().unwrap());
        let mut jaw = Command::new(env!("CARGO_BIN_EXE_jaw"))
            .args(["snapshot", "--width", "140", "--height", "40", "--keys", keys])
            .env("JAW_HOME", self.home.path())
            .stdout(out.try_clone().unwrap())
            .stderr(err.try_clone().unwrap())
            .spawn()
            .expect("jaw runs");
        let end = Instant::now() + JAW_FINISHES;
        let exited = loop {
            match jaw.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
                _ => {
                    let _ = jaw.kill();
                    let _ = jaw.wait();
                    break None;
                }
            }
        };
        let printed = |file: &mut File| {
            let mut said = String::new();
            file.rewind().unwrap();
            file.read_to_string(&mut said).unwrap();
            said
        };
        let (out, err) = (printed(&mut out), printed(&mut err));
        match exited {
            Some(status) if status.success() => out,
            Some(status) => panic!("jaw snapshot --keys {keys:?} exited with {status}:\n{out}{err}"),
            None => panic!("jaw snapshot --keys {keys:?} hadn't finished after {JAW_FINISHES:?}:\n{out}{err}"),
        }
    }

    /// Ana's chat as a window shows it, once `holds` says so of its rows, for 20 s at most.
    fn ana_once(&self, what: &str, holds: impl Fn(&[Value]) -> bool) -> Vec<Value> {
        let socket = jaw_proto::local::socket(self.home.path());
        let (mut connection, _) = jaw_client::Connection::open(&socket, "jaw-test", "0").unwrap();
        let watch = json!({ "chat": ANA, "around": "newest", "before": 40 });
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            let rows = connection.call("messages.watch", watch.clone()).unwrap()["rows"].as_array().cloned().unwrap();
            if holds(&rows) {
                return rows;
            }
            assert!(Instant::now() < end, "never: {what}:\n{rows:#?}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn on_a_demo_jawd_cards_show_their_names_and_numbers_one_goes_from_the_palette_and_enter_starts_a_chat() {
    let jawd = Jawd::demo();

    let screen = jawd.snapshot("");
    let at = screen.lines().position(|l| l.contains("◉ Dr.ª Marta Lopes")).unwrap_or_else(|| panic!("{screen}"));
    assert!(
        screen.lines().nth(at + 1).is_some_and(|l| l.contains("+351 21 345 6789, 21 000 00 00")),
        "Ana's card, its name and its numbers:\n{screen}"
    );
    let screen = jawd.snapshot("<a-down>");
    for line in ["◉ 2 contacts", "Inês Gomes · +351 913 002 210", "Tiago Martins · +351 916 666 666"] {
        assert!(screen.contains(line), "the climbers' two cards in one message, {line}:\n{screen}");
    }

    // Rui's card, from the palette's people, found by his number, into Ana's chat, the first.
    let screen = jawd.snapshot("<c-k>send a contact\n913\n");
    assert!(screen.contains("sending +351913333333's card"), "{screen}");
    let mine = |rows: &[Value]| {
        rows.iter().find(|r| r["mine"] == true && r["kind"] == "contact" && r["status"] == "read").cloned()
    };
    let rows = jawd.ana_once("your card read by Ana's phone", |rows| mine(rows).is_some());
    let card = mine(&rows).unwrap();
    assert_eq!(card["contact"]["name"], "+351913333333");
    let vcard = card["contact"]["vcard"].as_str().unwrap();
    assert!(vcard.starts_with("BEGIN:VCARD\r\nVERSION:3.0\r\n") && vcard.contains("waid=351913333333"), "{vcard}");
    let screen = jawd.snapshot("");
    let at = screen.lines().position(|l| l.contains("◉ +351913333333")).unwrap_or_else(|| panic!("{screen}"));
    assert!(screen.lines().nth(at).is_some_and(|l| l.contains("✓✓")), "yours, read:\n{screen}");

    // Enter on Ana's card, found by its name, asks which of its numbers to chat with, the one on
    // WhatsApp first, and starts a chat with Dr.ª Marta by it.
    let screen = jawd.snapshot("<c-k>lopes\n\n");
    let numbers = ["start a chat · Dr.ª Marta Lopes", "▸ Dr.ª Marta Lopes", "+351213456789", "21 000 00 00"];
    assert!(numbers.iter().all(|said| screen.contains(said)), "{screen}");
    let screen = jawd.snapshot("<c-k>lopes\n\n\n");
    assert!(screen.contains("╭ +351213456789 "), "her chat is open:\n{screen}");
}
