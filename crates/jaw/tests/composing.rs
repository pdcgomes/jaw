//! What's composed beyond text, end to end: places and events, from a real jawd on a folder of its
//! own with the demo engine, as `jaw snapshot --keys` prints its frames through the real socket.
//! No jawd here can run the Go engine: each runs the demo engine, and `JAW_ENGINE` names nothing.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jaw_proto::AccountState;

/// How long a test waits for jawd to be as it expects, however busy the machine.
const PATIENCE: Duration = Duration::from_secs(60);
const CLIMBING: &str = "120363012345678901@g.us";
const ANA: &str = "351912345678@s.whatsapp.net";

/// jawd, built as this test was, beside jaw.
fn jawd_binary() -> &'static Path {
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

/// jaw or jawd with jaw's folder `home`, no notifications, and no jaw-engine to run.
fn command(program: &Path, home: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env("JAW_HOME", home)
        .env("JAW_NOTIFY", "off")
        .env("JAW_ENGINE", home.join("no-jaw-engine"))
        .env_remove("JAWD")
        .stdin(Stdio::null());
    command
}

/// Waits until `done` holds, or fails saying `what` it waited for.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "waited {PATIENCE:?} for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A connection to the jawd on `home`, as a script would make one.
fn connect(home: &Path) -> Option<(jaw_client::Connection, jaw_proto::Welcome)> {
    jaw_client::Connection::open(&jaw_proto::local::socket(home), "jaw-test", "0").ok()
}

/// Whether jawd on `home` answers, linked, with the demo's script played: its group named last.
fn ready(home: &Path) -> bool {
    let Some((mut connection, welcome)) = connect(home) else { return false };
    let watch = serde_json::json!({ "folder": "all", "around": "top", "before": 0, "after": 10 });
    let played =
        |rows: &Vec<serde_json::Value>| rows.len() >= 2 && rows.iter().any(|row| row["name"] == "Lisbon Climbing");
    welcome.account.state == AccountState::Linked
        && connection.call("chats.watch", watch).is_ok_and(|v| v["rows"].as_array().is_some_and(played))
}

/// A jawd on a folder of its own with the demo engine, ended when dropped.
struct Jawd {
    child: Child,
    home: tempfile::TempDir,
}

impl Jawd {
    fn demo() -> Jawd {
        let home = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(home.path().join("jawd.log")).unwrap();
        let mut jawd = command(jawd_binary(), home.path());
        jawd.args(["--engine", "demo"]).stdout(log.try_clone().unwrap()).stderr(log);
        let jawd = Jawd { child: jawd.spawn().expect("jawd starts"), home };
        until("jawd to serve the demo's chats", || ready(jawd.home.path()));
        jawd
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    /// One frame of jaw on this jawd after `keys`, as `jaw snapshot` prints it.
    fn snapshot(&self, keys: &str) -> String {
        let mut jaw = command(Path::new(env!("CARGO_BIN_EXE_jaw")), self.home());
        jaw.args(["snapshot", "--width", "200", "--height", "44", "--keys", keys]);
        let out = jaw.output().expect("jaw runs");
        assert!(out.status.success(), "jaw snapshot failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).expect("a frame is text")
    }

    /// Chat `chat`'s row for message `id`, once `done` holds of it.
    fn row_once(&self, chat: &str, id: &str, done: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let mut found = serde_json::Value::Null;
        until(&format!("{id} as wanted"), || {
            let Some((mut connection, _)) = connect(self.home()) else { return false };
            let watch = serde_json::json!({ "chat": chat, "around": "newest", "before": 40 });
            let Ok(window) = connection.call("messages.watch", watch) else { return false };
            let row = window["rows"].as_array().and_then(|rows| rows.iter().find(|row| row["id"] == id).cloned());
            match row {
                Some(row) if done(&row) => {
                    found = row;
                    true
                }
                _ => false,
            }
        });
        found
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The frame's lines that hold `text`.
fn lines_with<'a>(frame: &'a str, text: &str) -> Vec<&'a str> {
    frame.lines().filter(|line| line.contains(text)).collect()
}

#[test]
fn the_demos_event_shows_when_and_where_it_is_and_whos_going_and_its_pin_its_place() {
    let jawd = Jawd::demo();
    jawd.row_once(CLIMBING, "3EB0DEMO00000009", |row| row["event"]["going"] == 1 && row["event"]["maybe"] == 1);
    // Ana's chat, the newest, opens first; the climbing group is the next.
    let pin = jawd.snapshot("");
    assert_eq!(lines_with(&pin, "⌖ Miradouro da Graça").len(), 1, "{pin}");
    assert_eq!(lines_with(&pin, "Calçada da Graça, 1100-265 Lisboa").len(), 1, "{pin}");
    let frame = jawd.snapshot("<a-down>");
    eprintln!("{frame}");
    for shown in [
        "◷ Sintra",
        "Bring the rope",
        "⌖ Cabo da Roca, Estrada do Cabo da Roca, Colares",
        "https://www.openstreetmap.org/?mlat=38.7803&mlon=-9.4989",
        "1 going · 1 maybe · you said maybe",
    ] {
        assert_eq!(lines_with(&frame, shown).len(), 1, "{shown:?} once:\n{frame}");
    }
    assert_eq!(lines_with(&frame, "Sat ").len(), 1, "when it starts:\n{frame}");
}

#[test]
fn an_event_sent_through_the_socket_shows_once_a_phone_answers_it() {
    let jawd = Jawd::demo();
    let (mut connection, _) = connect(jawd.home()).expect("jawd answers");
    let planned = serde_json::json!({
        "chat": CLIMBING, "name": "Bouldering", "description": "Chalk is on me", "start": 1_892_000_000_000_i64,
        "place": "Lisboa Climbing Spot", "key": "jaw-e2e-1",
    });
    let sent = connection.call("send.event", planned).expect("send.event answers");
    let id = sent["message"].as_str().expect("its ID").to_string();
    jawd.row_once(CLIMBING, &id, |row| row["event"]["going"] == 1 && row["status"] == "read");
    // The group, newest now, opens first.
    let frame = jawd.snapshot("");
    eprintln!("{frame}");
    for shown in ["◷ Bouldering", "Chalk is on me", "⌖ Lisboa Climbing Spot", "1 going"] {
        assert!(!lines_with(&frame, shown).is_empty(), "{shown:?}:\n{frame}");
    }
}

#[test]
fn a_pin_sent_through_the_socket_shows_its_name_its_address_and_its_map() {
    let jawd = Jawd::demo();
    let (mut connection, _) = connect(jawd.home()).expect("jawd answers");
    let pin = serde_json::json!({
        "chat": ANA, "lat": 38.713_9, "lon": -9.133_4, "name": "  Castelo de São Jorge ",
        "address": "R. de Santa Cruz do Castelo, Lisboa", "key": "jaw-e2e-2",
    });
    let sent = connection.call("send.location", pin).expect("send.location answers");
    let id = sent["message"].as_str().expect("its ID").to_string();
    let row = jawd.row_once(ANA, &id, |row| row["status"] == "read");
    assert_eq!(row["location"]["name"], "Castelo de São Jorge", "trimmed: {row}");
    // Ana's chat, newest now, opens first.
    let frame = jawd.snapshot("");
    eprintln!("{frame}");
    for shown in [
        "⌖ Castelo de São Jorge",
        "R. de Santa Cruz do Castelo, Lisboa",
        "https://www.openstreetmap.org/?mlat=38.7139&mlon=-9.1334",
    ] {
        assert_eq!(lines_with(&frame, shown).len(), 1, "{shown:?} once:\n{frame}");
    }
}
