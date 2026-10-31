//! Others' typing end to end, as the owner sees it: a `jawd --engine demo` of its own, whose made-up
//! people type and record, on a temporary folder each child is pointed at with `JAW_HOME`, and
//! `jaw snapshot --keys`, the binary, drawing what jawd sends through the real socket. An interface
//! in front keeps WhatsApp told you're online, since only then does it say who's typing. Everything
//! the test waits for has a deadline, so a jawd or a `jaw` that never answers fails it.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jaw_proto::Status;
use serde_json::{Value, json};

/// How long a fresh jawd has to answer hello with the demo's script played, on a loaded machine.
const JAWD_ANSWERS: Duration = Duration::from_secs(60);
/// How long one `jaw snapshot` has to finish.
const JAW_FINISHES: Duration = Duration::from_secs(60);
/// How long jawd has to hear the demo's people type once an interface is in front; they say it
/// every 4 s.
const HEARD: Duration = Duration::from_secs(60);

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
    /// Starts one and returns once it answers hello linked, its script played: the group named.
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
        let end = Instant::now() + JAWD_ANSWERS;
        let played =
            |rows: &Value| rows.as_array().is_some_and(|rows| rows.iter().any(|r| r["name"] == "Lisbon Climbing"));
        loop {
            let opened = jaw_client::Connection::open(&socket, "jaw-test", "0");
            if let Ok((mut connection, _)) = opened {
                let watch = json!({"folder": "all", "around": "top", "before": 0, "after": 10});
                if connection.call("chats.watch", watch).is_ok_and(|list| played(&list["rows"])) {
                    return jawd;
                }
            }
            if Instant::now() > end {
                let log = std::fs::read_to_string(jawd.home.path().join("jawd.log")).unwrap_or_default();
                panic!("jawd didn't play its script on {} within {JAWD_ANSWERS:?}:\n{log}", socket.display());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `jaw snapshot` on the jawd in `home` after `keys`, 140 columns by 34 rows, its status line on.
/// One still running after `JAW_FINISHES` is killed, through its own child, and fails the test.
fn snapshot(home: &Path, keys: &str) -> String {
    let out = tempfile::tempfile().unwrap();
    let mut jaw = Command::new(env!("CARGO_BIN_EXE_jaw"))
        .args(["snapshot", "--width", "140", "--height", "34", "--keys", keys])
        .env("JAW_HOME", home)
        .env("JAW_EXPERIMENTS", "status_line")
        .stdout(out.try_clone().unwrap())
        .stderr(Stdio::null())
        .spawn()
        .expect("jaw runs");
    let end = Instant::now() + JAW_FINISHES;
    let exited = loop {
        match jaw.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = jaw.kill();
                let _ = jaw.wait();
                panic!("jaw snapshot --keys {keys:?} hadn't finished after {JAW_FINISHES:?}, so it was killed");
            }
        }
    };
    let mut out = out;
    let mut screen = String::new();
    std::io::Seek::rewind(&mut out).unwrap();
    std::io::Read::read_to_string(&mut out, &mut screen).unwrap();
    assert!(exited.success(), "jaw snapshot --keys {keys:?} exited with {exited}:\n{screen}");
    screen
}

/// The row of `screen` a line containing `text` is on.
fn row_of(screen: &str, text: &str) -> usize {
    screen.lines().position(|line| line.contains(text)).unwrap_or_else(|| panic!("no {text:?} in\n{screen}"))
}

/// The row just above the message box, the last of the conversation's.
fn above_the_box(screen: &str) -> &str {
    let the_box = screen.lines().enumerate().filter(|(_, l)| l.contains("╭──")).map(|(i, _)| i).last().unwrap();
    screen.lines().nth(the_box - 1).unwrap()
}

#[test]
fn typing_comes_through_the_socket_to_the_chat_list_the_header_the_status_line_and_after_the_newest_message() {
    let jawd = Jawd::demo();
    let socket = jaw_proto::local::socket(jawd.home.path());
    let (mut looking, _) = jaw_client::Connection::open(&socket, "jaw-test", "0").unwrap();
    looking.call("focus", json!({"focused": true})).unwrap();
    // Ana types in her chat and Rui records in Lisbon Climbing once WhatsApp hears you.
    let end = Instant::now() + HEARD;
    loop {
        let status: Status = serde_json::from_value(looking.call("status", Value::Null).unwrap()).unwrap();
        if status.typing.is_some_and(|typing| typing.chats >= 2) {
            break;
        }
        assert!(Instant::now() < end, "jawd never heard the demo's people type: {:?}", status.typing);
        std::thread::sleep(Duration::from_millis(100));
    }

    // Ana's chat, the newest, opens first.
    let screen = snapshot(jawd.home.path(), "");
    assert!(screen.contains("╭ +351912345678 · typing… ─"), "the header:\n{screen}");
    let ana = row_of(&screen, "│ ● +351912345678");
    assert!(screen.lines().nth(ana + 1).unwrap().contains("│   typing…"), "her row in the chat list:\n{screen}");
    let climbing = row_of(&screen, "│ ● Lisbon Climbing");
    let group = screen.lines().nth(climbing + 1).unwrap();
    assert!(group.contains("+351913333333") && group.contains('…'), "the group's row:\n{screen}");
    let status = screen.lines().rev().nth(1).unwrap();
    assert!(status.contains("+351912345678") && status.contains("typing…"), "the status line:\n{screen}");
    // Her roster row says it too, from her chat's typing, which jawd tells the roster.
    let roster =
        |l: &&str| l.starts_with("│ ● +3519123456") && l.split("││").next().is_some_and(|s| s.contains("typing…"));
    assert!(screen.lines().any(|l| roster(&l)), "her roster row:\n{screen}");
    let bubble = above_the_box(&screen);
    assert!(bubble.contains("│        • • • "), "after the newest message, where hers start:\n{screen}");
    assert!(
        screen
            .lines()
            .nth(screen.lines().position(|l| l == bubble).unwrap() - 1)
            .unwrap()
            .contains("the one at the lake")
    );

    // In the group Rui records, Zé typing now and then beside him.
    let screen = snapshot(jawd.home.path(), "<a-down>");
    let header = screen.lines().nth(1).unwrap();
    assert!(header.contains("╭ Lisbon Climbing · +351913333333"), "the group's header:\n{screen}");
    assert!(header.contains("recording…") || header.contains("typing…"), "{header}");
    let bubbles: Vec<&str> = screen.lines().filter(|l| l.contains("• • •")).collect();
    assert!(bubbles.iter().any(|l| l.contains("│ +351913333333  • • • ")), "his, after his name:\n{screen}");
    assert!(above_the_box(&screen).contains("• • •"), "after the newest message:\n{screen}");
}

/// A line-by-line connection with a deadline on each read, for waiting on pushes.
struct Line {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
}

impl Line {
    fn open(socket: &Path) -> Line {
        let writer = UnixStream::connect(socket).unwrap();
        writer.set_read_timeout(Some(HEARD)).unwrap();
        let mut line = Line { reader: BufReader::new(writer.try_clone().unwrap()), writer, next: 1 };
        let hello = json!({"protocol": jaw_proto::PROTOCOL, "client": "jaw-test", "version": "0", "focus": true});
        line.call("hello", hello);
        line
    }

    /// Sends a request and returns its result, passing over the pushes that come first.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.writer, "{}", json!({"id": id, "method": method, "params": params})).unwrap();
        loop {
            let line = self.read();
            if line["id"] == id {
                return line["result"].clone();
            }
        }
    }

    fn read(&mut self) -> Value {
        let mut text = String::new();
        let read = self.reader.read_line(&mut text);
        assert!(read.is_ok_and(|n| n > 0), "jawd sent nothing within {HEARD:?}");
        serde_json::from_str(&text).unwrap()
    }

    /// The next `presence` push, within `HEARD`.
    fn presence(&mut self) -> Value {
        loop {
            let line = self.read();
            if line["method"] == "presence" {
                return line["params"].clone();
            }
        }
    }
}

#[test]
fn presence_comes_through_the_socket_to_a_conversation_as_its_person_comes_and_goes() {
    const ANA: &str = "351912345678@s.whatsapp.net";
    let jawd = Jawd::demo();
    let mut line = Line::open(&jaw_proto::local::socket(jawd.home.path()));
    // Her conversation open, with an interface in front, jawd follows her: its answer says how she
    // is, and pushes follow as the demo's people come and go, every few seconds.
    let answer = line.call("messages.watch", json!({"chat": ANA, "around": "newest", "before": 10}));
    let presence = &answer["presence"];
    assert!(presence["person"].as_str().is_some_and(|p| p.starts_with("person:")), "{answer}");
    assert!(presence["known"].is_boolean(), "{answer}");
    let group =
        line.call("messages.watch", json!({"chat": "120363012345678901@g.us", "around": "newest", "before": 5}));
    assert_eq!(group.get("presence"), None, "a group's conversation has none: {group}");
    let mut came = Vec::new();
    while came.len() < 3 {
        let pushed = line.presence();
        assert_eq!((&pushed["chat"], &pushed["person"]), (&json!(ANA), &presence["person"]), "{pushed}");
        let state = pushed["state"].as_str().unwrap().to_string();
        assert!(["online", "seen", "hidden"].contains(&state.as_str()), "{pushed}");
        assert_eq!(pushed.get("at").is_some(), state == "seen", "when she was seen, only once she's gone: {pushed}");
        came.push((state, pushed["known"].as_bool().unwrap()));
    }
    assert!(came.iter().any(|(_, known)| *known), "followed while her conversation is open: {came:?}");
    assert!(came.windows(2).all(|pair| pair[0] != pair[1]), "each push a change: {came:?}");
}
