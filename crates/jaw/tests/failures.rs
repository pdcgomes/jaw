//! What jaw shows when something fails, end to end, as the design's "When something fails" lists
//! it: a real jawd on a folder of its own, its demo engine failing as `--fail` says from the first
//! SIGUSR1 it gets until the next, and jaw's frames as `jaw snapshot --keys` prints them through
//! the real socket. No jawd here can run the Go engine: each runs the demo engine, and `JAW_ENGINE`
//! names nothing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jaw_proto::{AccountState, ConnectionState, Status};

/// How long a test waits for jawd to be as it expects, however busy the machine.
const PATIENCE: Duration = Duration::from_secs(60);

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

/// jaw or jawd with the environment every run here has: jaw's folder `home`, no notifications,
/// and no jaw-engine to run.
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

/// One frame of jaw on `home`, `width` by `height`, after `keys`, as `jaw snapshot` prints it with
/// NO_COLOR set, so a photo is its one line whether or not the shell running the tests sets it.
fn snapshot_with(home: &Path, keys: &str, (width, height): (u16, u16), more: &[&str], jawd: Option<&Path>) -> String {
    let (width, height) = (width.to_string(), height.to_string());
    let mut jaw = command(Path::new(env!("CARGO_BIN_EXE_jaw")), home);
    jaw.args(["snapshot", "--width", &width, "--height", &height, "--keys", keys]).args(more).env("NO_COLOR", "1");
    if let Some(jawd) = jawd {
        jaw.env("JAWD", jawd);
    }
    let out = jaw.output().expect("jaw runs");
    assert!(out.status.success(), "jaw snapshot failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("a frame is text")
}

fn header(frame: &str) -> &str {
    frame.lines().next().unwrap_or_default()
}

/// What `jaw` with `args` on `home` prints, given `input`, and whether it succeeded; ended if it
/// takes longer than PATIENCE.
fn jaw(home: &Path, args: &[&str], input: &str) -> (bool, String) {
    let mut jaw = command(Path::new(env!("CARGO_BIN_EXE_jaw")), home);
    jaw.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = jaw.spawn().expect("jaw runs");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let deadline = Instant::now() + PATIENCE;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("jaw {args:?} didn't end within {PATIENCE:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output().unwrap();
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), said)
}

/// The frame's lines that hold `text`.
fn lines_with<'a>(frame: &'a str, text: &str) -> Vec<&'a str> {
    frame.lines().filter(|line| line.contains(text)).collect()
}

/// Waits until `done` holds, or fails saying `what` it waited for.
fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !done() {
        assert!(Instant::now() < deadline, "waited {PATIENCE:?} for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// jawd's status on `home`, if it answers.
fn status_of(home: &Path) -> Option<Status> {
    let socket = jaw_proto::local::socket(home);
    let (mut connection, _) = jaw_client::Connection::open(&socket, "jaw-test", "0").ok()?;
    serde_json::from_value(connection.call("status", serde_json::Value::Null).ok()?).ok()
}

/// Whether jawd on `home` answers, linked, with the demo's chats in its list, the group named, as
/// the script's last event names it once its messages are in.
fn ready(home: &Path) -> bool {
    let socket = jaw_proto::local::socket(home);
    let Ok((mut connection, welcome)) = jaw_client::Connection::open(&socket, "jaw-test", "0") else { return false };
    let watch = serde_json::json!({ "folder": "all", "around": "top", "before": 0, "after": 10 });
    let played =
        |rows: &Vec<serde_json::Value>| rows.len() >= 2 && rows.iter().any(|row| row["name"] == "Lisbon Climbing");
    welcome.account.state == AccountState::Linked
        && connection.call("chats.watch", watch).is_ok_and(|v| v["rows"].as_array().is_some_and(played))
}

/// A jawd on a folder of its own with the demo engine and `args`, ended when dropped.
struct Jawd {
    child: Child,
    home: tempfile::TempDir,
}

impl Jawd {
    fn demo(args: &[&str]) -> Jawd {
        let home = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(home.path().join("jawd.log")).unwrap();
        let mut jawd = command(jawd_binary(), home.path());
        jawd.args(["--engine", "demo"]).args(args).stdout(log.try_clone().unwrap()).stderr(log);
        let jawd = Jawd { child: jawd.spawn().expect("jawd starts"), home };
        until("jawd to serve the demo's chats", || ready(jawd.home.path()));
        jawd
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    /// Turns the demo engine's failure on, or off again.
    fn toggle(&self) {
        // SAFETY: kill only sends a signal to the child this test started.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGUSR1) };
    }

    fn until(&self, what: &str, done: impl Fn(&Status) -> bool) {
        until(what, || status_of(self.home()).is_some_and(|status| done(&status)));
    }

    fn snapshot(&self, keys: &str) -> String {
        snapshot_with(self.home(), keys, (110, 24), &[], None)
    }

    /// What `jawd status` prints.
    fn status(&self) -> String {
        let out = command(jawd_binary(), self.home()).arg("status").output().expect("jawd status runs");
        assert!(out.status.success(), "jawd status failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.home().join("jawd.log")).unwrap_or_default()
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The jawd a jaw started, through a script that notes its process, ended when dropped.
struct Started {
    home: tempfile::TempDir,
    script: PathBuf,
}

impl Started {
    /// A script for jaw to start as its jawd: once `$JAW_HOME/go` exists, if `waits`, it becomes
    /// jawd with the demo engine.
    fn new(waits: bool) -> Started {
        let home = tempfile::tempdir().unwrap();
        let script = home.path().join("start-jawd");
        let wait = if waits { "while [ ! -e \"$JAW_HOME/go\" ]; do sleep 0.05; done\n" } else { "" };
        let body = format!(
            "#!/bin/sh\necho $$ > \"$JAW_HOME/jawd.pid\"\n{wait}exec '{}' --engine demo\n",
            jawd_binary().display()
        );
        std::fs::write(&script, body).unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        Started { home, script }
    }

    fn home(&self) -> &Path {
        self.home.path()
    }

    fn snapshot(&self, keys: &str) -> String {
        snapshot_with(self.home(), keys, (100, 16), &["--start"], Some(&self.script))
    }

    fn pid(&self) -> Option<i32> {
        std::fs::read_to_string(self.home().join("jawd.pid")).ok()?.trim().parse().ok()
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        let Some(pid) = self.pid() else { return };
        // SAFETY: the process is the one jaw started for this test, which its script named.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        let gone = Instant::now() + Duration::from_secs(10);
        // SAFETY: signal 0 only asks whether the process is still there.
        while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < gone {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn jaw_starts_jawd_says_so_while_it_starts_and_shows_the_chats_once_it_answers() {
    let jawd = Started::new(true);
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  starting jawd…"), "{frame}");
    assert!(frame.contains("Starting jawd, which keeps your messages and talks to WhatsApp."), "{frame}");
    assert!(frame.contains("If it can't start, this says why."), "{frame}");
    assert!(jawd.pid().is_some(), "jaw started the jawd it was told to");
    std::fs::write(jawd.home().join("go"), "").unwrap();
    until("the jawd jaw started to serve the demo's chats", || ready(jawd.home()));
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    assert!(frame.contains("Lisbon Climbing") && frame.contains("the one at the lake"), "{frame}");
}

#[test]
fn a_jawd_that_cant_start_says_why_and_jaw_keeps_saying_it() {
    let jawd = Started::new(false);
    std::fs::write(jawd.home().join("jaw.db"), "not a database, so jawd can't open it").unwrap();
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  jawd didn't start"), "{frame}");
    assert!(frame.contains("It didn't start: jawd stopped as it started (exit status: 1): can't set"), "{frame}");
    assert!(frame.contains("not a database"), "the whole of why, wrapped to the screen:\n{frame}");
    assert!(!frame.contains("can't reach jawd"), "jaw's tries to connect since don't hide why:\n{frame}");
}

#[test]
fn an_engine_that_crashes_shows_reconnecting_with_the_chats_kept_until_jawd_has_started_it_again() {
    let jawd = Jawd::demo(&["--fail", "crash"]);
    let before = jawd.snapshot("");
    assert!(header(&before).contains("jaw  online"), "{before}");
    jawd.toggle();
    jawd.until("the engine to crash", |s| {
        s.connection.state == ConnectionState::EngineRestarting && s.engine.restarts == 1
    });
    let during = jawd.snapshot("");
    assert!(header(&during).contains("jaw  reconnecting: the engine restarted"), "{during}");
    let body = |frame: &str| frame.lines().skip(1).map(str::to_string).collect::<Vec<_>>();
    assert_eq!(body(&during), body(&before), "jaw keeps everything it had");
    let said = jawd.status();
    assert!(said.contains("whatsapp  reconnecting while the engine restarts"), "{said}");
    assert!(said.contains("last stopped: the demo engine crashed"), "{said}");
    jawd.toggle();
    jawd.until("the engine to be back", |s| {
        s.connection.state == ConnectionState::Online && s.engine.state == "running"
    });
    let after = jawd.snapshot("");
    assert!(header(&after).contains("jaw  online"), "{after}");
    assert!(jawd.status().contains("started again 1 times"), "{}", jawd.log());
}

#[test]
fn with_whatsapp_out_of_reach_the_header_says_so_and_a_message_waits_with_its_clock_until_it_can_go() {
    let jawd = Jawd::demo(&["--fail", "offline"]);
    jawd.toggle();
    jawd.until("WhatsApp to be out of reach", |s| s.connection.state == ConnectionState::Offline);
    let frame = jawd.snapshot("on my way\n");
    assert!(header(&frame).contains("jaw  offline: what you send waits until it's back"), "{frame}");
    assert!(
        frame.contains("Lisbon Climbing") && frame.contains("the one at the lake"),
        "everything local works:\n{frame}"
    );
    let sent = lines_with(&frame, "on my way");
    assert!(!sent.is_empty() && sent.iter().all(|line| line.contains('◷')), "it waits with its clock:\n{frame}");
    let said = jawd.status();
    assert!(said.contains("whatsapp  offline") && said.contains("1 messages of yours to send"), "{said}");
    jawd.toggle();
    jawd.until("the message to go", |s| {
        s.connection.state == ConnectionState::Online && s.stored.is_some_and(|stored| stored.outbox == 0)
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    let gone = lines_with(&frame, "on my way");
    assert!(!gone.is_empty() && gone.iter().all(|line| line.contains('✓') && !line.contains('◷')), "{frame}");
}

/// The messages jawd on `home` finds with `text`.
fn found(home: &Path, text: &str) -> usize {
    let socket = jaw_proto::local::socket(home);
    let Ok((mut connection, _)) = jaw_client::Connection::open(&socket, "jaw-test", "0") else { return 0 };
    let find = serde_json::json!({ "query": text, "kinds": ["message"] });
    connection.call("find", find).ok().and_then(|v| v["results"].as_array().map(Vec::len)).unwrap_or(0)
}

#[test]
fn after_a_keepalive_times_out_the_header_says_connecting_and_a_message_sent_then_goes_once_the_pings_get_through() {
    let jawd = Jawd::demo(&["--fail", "keepalive"]);
    jawd.toggle();
    jawd.until("the keepalive to time out", |s| s.connection.state == ConnectionState::Connecting);
    let frame = jawd.snapshot("sent while the pings fail\n");
    assert!(header(&frame).contains("jaw  connecting"), "{frame}");
    let sent = lines_with(&frame, "sent while the pings fail");
    assert!(!sent.is_empty() && sent.iter().all(|line| line.contains('◷')), "it waits with its clock:\n{frame}");
    std::thread::sleep(Duration::from_secs(1));
    let waiting = status_of(jawd.home()).expect("jawd answers");
    let outbox = waiting.stored.map(|stored| stored.outbox);
    assert_eq!((waiting.connection.state, outbox), (ConnectionState::Connecting, Some(1)), "nothing goes meanwhile");
    jawd.toggle();
    jawd.until("the message to go", |s| {
        s.connection.state == ConnectionState::Online && s.stored.is_some_and(|stored| stored.outbox == 0)
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    let gone = lines_with(&frame, "sent while the pings fail");
    assert!(!gone.is_empty() && gone.iter().all(|line| line.contains('✓') && !line.contains('◷')), "{frame}");
    until("the message to be found", || found(jawd.home(), "pings fail") > 0);
    assert_eq!(found(jawd.home(), "pings fail"), 1, "it went once, as one message");
}

#[test]
fn with_jawd_itself_out_of_reach_the_mimic_sleeps() {
    let home = tempfile::tempdir().unwrap();
    let frame = snapshot_with(home.path(), "", (100, 24), &[], None);
    assert!(header(&frame).contains("jaw  can't reach jawd · trying again in"), "{frame}");
    assert!(frame.contains("jawd isn't answering.") && frame.contains('▀'), "the Mimic, asleep:\n{frame}");
    assert!(frame.contains("jaw tries again by itself"), "{frame}");
}

#[test]
fn with_the_phone_offline_older_history_waits_and_says_so_while_messages_still_go() {
    let jawd = Jawd::demo(&["--fail", "phone-offline", "--phone-patience", "1"]);
    let stored = |s: &Status| s.stored.map_or(0, |stored| stored.messages);
    let before = stored(&status_of(jawd.home()).expect("jawd answers"));
    // Online, the phone answers at once, and says it keeps more of the chat, so the next question
    // has an answer to come. Each frame is a new jaw's, from the box, so it presses <up> once more
    // each time until it's past the oldest message, however many the demo's first chat holds.
    let asking = |frame: &str| frame.contains("asking your phone for older messages");
    let (most, mut pressed) = (20, 1);
    let mut frame = jawd.snapshot("<up>");
    while !asking(&frame) && pressed < most {
        pressed += 1;
        frame = jawd.snapshot(&"<up>".repeat(pressed));
    }
    assert!(asking(&frame), "{pressed} presses of <up> never moved past the oldest message:\n{frame}");
    jawd.until("the phone's older messages", |s| {
        stored(s) >= before + 3 && s.history.as_ref().is_some_and(|history| history.blobs >= 1)
    });
    assert_eq!(status_of(jawd.home()).expect("jawd answers").connection.state, ConnectionState::Online);
    jawd.toggle();
    until("the phone to go offline", || jawd.log().contains("failure (phone-offline) is on"));
    let frame = jawd.snapshot(&"<up>".repeat(12));
    assert!(frame.contains("asking your phone for older messages"), "moving past the oldest message again:\n{frame}");
    jawd.until("jawd to say the phone is offline", |s| s.connection.state == ConnectionState::PhoneOffline);
    let frame = jawd.snapshot("still goes\n");
    assert!(header(&frame).contains("jaw  online · phone offline: photos and older history wait for it"), "{frame}");
    jawd.until("the message to go", |s| s.stored.is_some_and(|stored| stored.outbox == 0));
    assert!(jawd.status().contains("whatsapp  online, but the phone is offline"), "{}", jawd.status());
    jawd.toggle();
    jawd.until("the phone's older messages", |s| {
        s.connection.state == ConnectionState::Online && stored(s) >= before + 7
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    assert!(frame.contains("an older message from the phone, 3 of 3"), "{frame}");
    let sent = lines_with(&frame, "still goes");
    assert!(!sent.is_empty() && sent.iter().all(|line| line.contains('✓')), "{frame}");
}

#[test]
fn logged_out_from_the_phone_jaw_says_why_keeps_the_messages_and_links_again_from_a_qr_code() {
    let jawd = Jawd::demo(&["--fail", "logout"]);
    let stored = status_of(jawd.home()).and_then(|s| s.stored).expect("jawd says what it stores");
    jawd.toggle();
    jawd.until("the phone to log jaw out", |s| {
        s.connection.state == ConnectionState::LoggedOut && s.account.state == AccountState::Unlinked
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  logged out from another device"), "{frame}");
    assert!(frame.contains("Logged out from another device."), "{frame}");
    assert!(frame.contains("jawd has deleted the session and kept your messages."), "{frame}");
    assert!(frame.contains("⏎ link again"), "{frame}");
    let said = jawd.status();
    let kept = format!("stored    {} messages in {} chats", stored.messages, stored.chats);
    assert!(said.contains("account   not linked yet") && said.contains(&kept), "{said}");
    let linking = snapshot_with(jawd.home(), "\n", (110, 40), &[], None);
    assert!(linking.contains("link jaw to WhatsApp") && linking.contains("It changes in"), "{linking}");
    assert!(linking.contains("████"), "the QR code:\n{linking}");
    jawd.toggle();
    jawd.until("the phone to link jaw again", |s| {
        s.connection.state == ConnectionState::Online && s.account.state == AccountState::Linked
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    assert!(frame.contains("Lisbon Climbing") && frame.contains("the one at the lake"), "the messages kept:\n{frame}");
}

#[test]
fn logged_out_from_jaw_jaw_unlink_asks_first_keeps_the_messages_and_jaw_link_links_again() {
    let jawd = Jawd::demo(&[]);
    let stored = status_of(jawd.home()).and_then(|s| s.stored).expect("jawd says what it stores");
    let (ok, said) = jaw(jawd.home(), &["unlink"], "n\n");
    assert!(ok && said.contains("Log out? [y/N]") && said.ends_with("jaw is still linked.\n"), "{said}");
    assert_eq!(status_of(jawd.home()).unwrap().account.state, AccountState::Linked, "nothing logged out");
    let (ok, said) = jaw(jawd.home(), &["unlink"], "y\n");
    assert!(ok && said.contains("Logged out. jawd has deleted the session and kept your messages"), "{said}");
    jawd.until("jaw to be logged out", |s| {
        s.connection.state == ConnectionState::LoggedOut && s.account.state == AccountState::Unlinked
    });
    let now = status_of(jawd.home()).unwrap();
    assert_eq!(now.connection.reason.as_deref(), Some("logged out from jaw"));
    let kept = now.stored.expect("jawd says what it stores");
    assert_eq!((kept.messages, kept.chats), (stored.messages, stored.chats), "the messages kept");
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  logged out from jaw"), "{frame}");
    assert!(frame.contains("Logged out from jaw.") && frame.contains("kept your messages"), "{frame}");
    assert!(frame.contains("⏎ link again"), "{frame}");
    let (ok, said) = jaw(jawd.home(), &["unlink", "--yes"], "");
    assert!(ok && said == "jaw isn't linked to WhatsApp; jaw link links it\n", "{said}");
    let (ok, said) = jaw(jawd.home(), &["link"], "");
    assert!(
        ok && said.contains("████") && said.contains("Linked +351910000001."),
        "the made-up phone links it:\n{said}"
    );
    jawd.until("jaw to be linked again", |s| {
        s.connection.state == ConnectionState::Online && s.account.state == AccountState::Linked
    });
    let frame = jawd.snapshot("");
    assert!(header(&frame).contains("jaw  online"), "{frame}");
    assert!(frame.contains("Lisbon Climbing") && frame.contains("the one at the lake"), "the messages kept:\n{frame}");
    let (ok, said) = jaw(jawd.home(), &["link"], "");
    assert!(ok && said == "jaw is already linked to +351910000001; jaw unlink logs it out\n", "{said}");
}

/// jaw's words when the phone linked another account than the one whose messages the folder keeps.
const BELONGS: &str = "this folder belongs to +351910000001, whose messages jaw keeps in it, so only +351910000001 \
    links here: link it, or start jaw on a folder of its own for the other account, with JAW_HOME=<folder> jaw";

#[test]
fn after_a_logout_another_number_is_refused_saying_whose_folder_it_is_and_that_number_links_again() {
    let jawd = Jawd::demo(&[]);
    let stored = status_of(jawd.home()).and_then(|s| s.stored).expect("jawd says what it stores");
    let (ok, said) = jaw(jawd.home(), &["unlink", "--yes"], "");
    assert!(ok && said.contains("Logged out."), "{said}");
    jawd.until("jaw to be logged out", |s| s.account.state == AccountState::Unlinked);
    let (ok, said) = jaw(jawd.home(), &["link", "--phone", "351919999999"], "");
    assert!(
        !ok && said.contains("then type DEMO-") && said.contains(BELONGS),
        "the code typed on another's phone:\n{said}"
    );
    let now = status_of(jawd.home()).unwrap();
    assert_eq!(now.account.state, AccountState::Unlinked, "nothing of the other account linked");
    let kept = now.stored.expect("jawd says what it stores");
    assert_eq!((kept.messages, kept.chats), (stored.messages, stored.chats), "nor stored");
    let (ok, said) = jaw(jawd.home(), &["link"], "");
    assert!(ok && said.contains("Linked +351910000001."), "the made-up account's phone links it:\n{said}");
    jawd.until("jaw to be linked again", |s| {
        s.connection.state == ConnectionState::Online
            && s.account.me.as_ref().is_some_and(|me| me.phone == "+351910000001")
    });
}

#[test]
fn a_folder_that_keeps_no_accounts_messages_links_any_number() {
    let home = tempfile::tempdir().unwrap();
    let log = std::fs::File::create(home.path().join("jawd.log")).unwrap();
    let mut jawd = command(jawd_binary(), home.path());
    jawd.args(["--engine", "demo-unlinked"]).stdout(log.try_clone().unwrap()).stderr(log);
    let jawd = Jawd { child: jawd.spawn().expect("jawd starts"), home };
    jawd.until("the engine to run", |s| s.engine.state == "running" && s.account.state == AccountState::Unlinked);
    let (ok, said) = jaw(jawd.home(), &["link", "--phone", "351919999999"], "");
    assert!(ok && said.contains("then type DEMO-") && said.contains("Linked +351919999999."), "{said}");
    jawd.until("the other number to be linked", |s| {
        s.account.state == AccountState::Linked && s.account.me.as_ref().is_some_and(|me| me.phone == "+351919999999")
    });
}

#[test]
fn when_whatsapp_wants_a_newer_jaw_the_header_says_update_needed_and_reading_still_works() {
    let jawd = Jawd::demo(&["--fail", "outdated"]);
    jawd.toggle();
    jawd.until("WhatsApp to want a newer jaw", |s| s.connection.state == ConnectionState::UpdateNeeded);
    let frame = jawd.snapshot("hi\n");
    assert!(header(&frame).contains("jaw  update needed: WhatsApp wants a newer jaw · reading still works"), "{frame}");
    assert!(frame.contains("did you get the photos from Sunday?"), "reading still works:\n{frame}");
    let sent = lines_with(&frame, " hi ");
    assert!(!sent.is_empty() && sent.iter().all(|line| line.contains('◷')), "what's sent waits:\n{frame}");
    assert!(jawd.status().contains("whatsapp  WhatsApp wants a newer version of jaw"));
}

#[test]
fn with_no_ffmpeg_text_still_goes_and_comes_and_jawd_status_says_what_is_missing() {
    let jawd = Jawd::demo(&["--without", "ffmpeg", "--without", "ffprobe"]);
    let frame = jawd.snapshot("still works\n");
    assert!(frame.contains("the one at the lake"), "what arrived:\n{frame}");
    jawd.until("the message to go", |s| s.stored.is_some_and(|stored| stored.outbox == 0));
    let frame = jawd.snapshot("");
    let sent = lines_with(&frame, "still works");
    assert!(!sent.is_empty() && sent.iter().all(|line| line.contains('✓')), "{frame}");
    let said = jawd.status();
    assert!(said.contains("missing   ffmpeg, for sending videos, GIFs and audio files, and video posters"), "{said}");
    assert!(said.contains("missing   ffprobe, for reading what a file to send is"), "{said}");
}
