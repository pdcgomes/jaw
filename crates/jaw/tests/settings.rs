//! `jaw settings`, the binary as the owner runs it, on a `jawd --engine demo` of its own: jawd is
//! built beside this test, runs on a temporary folder that each child is pointed at with
//! `JAW_HOME`, and is killed through its own child when the test is done.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use jaw_proto::settings::TERMINAL;

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
    /// Its folder, with jaw.db, jaw.sock and what jawd says on stderr in jawd.log.
    home: tempfile::TempDir,
}

impl Jawd {
    /// Starts one and returns once it answers hello with the terminal's settings, its jaw.db open.
    fn demo() -> Jawd {
        let home = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(home.path().join("jawd.log")).unwrap();
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
        let answers =
            || jaw_client::Connection::open(&socket, TERMINAL, "0").is_ok_and(|(_, hello)| hello.settings.is_some());
        let end = Instant::now() + Duration::from_secs(60);
        while !answers() {
            let log = std::fs::read_to_string(jawd.home.path().join("jawd.log")).unwrap_or_default();
            assert!(Instant::now() < end, "jawd never answered on {}:\n{log}", socket.display());
            std::thread::sleep(Duration::from_millis(50));
        }
        jawd
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Jawd {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `jaw settings` with `args` on the jawd in `home`: how it exited, and what it printed on stdout
/// and on stderr.
fn jaw_settings(home: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let ran = Command::new(env!("CARGO_BIN_EXE_jaw"))
        .arg("settings")
        .args(args)
        .env("JAW_HOME", home)
        .output()
        .expect("jaw runs");
    (ran.status.code(), String::from_utf8(ran.stdout).unwrap(), String::from_utf8(ran.stderr).unwrap())
}

#[test]
fn jaw_settings_prints_and_changes_them_on_a_demo_jawd_and_exits_with_1_when_jawd_refuses_or_isnt_there() {
    let mut jawd = Jawd::demo();
    let home = jawd.home.path().to_path_buf();
    let (exit, defaults, _) = jaw_settings(&home, &[]);
    assert_eq!(exit, Some(0), "{defaults}");
    assert!(defaults.starts_with("theme     tokyo-night (Tokyo Night)\n"), "{defaults}");
    assert!(defaults.contains("\nmessages  bubbles\n"), "{defaults}");

    let (exit, changed, _) = jaw_settings(&home, &["theme=nord", "messages=irc"]);
    assert_eq!(exit, Some(0), "{changed}");
    assert!(changed.starts_with("theme     nord (Nord)\n") && changed.contains("\nmessages  irc\n"), "{changed}");
    assert_eq!(jaw_settings(&home, &[]), (Some(0), changed, String::new()), "read back through hello");

    let (exit, out, err) = jaw_settings(&home, &["theme=solarized"]);
    let themes: Vec<&str> = jaw_proto::themes::all().iter().map(|t| t.id.as_str()).collect();
    let refused = format!("Error: theme takes one of {}", themes.join(", "));
    assert_eq!((exit, out.as_str(), err.lines().next()), (Some(1), "", Some(refused.as_str())), "jawd's message");

    jawd.stop();
    let (exit, out, err) = jaw_settings(&home, &[]);
    assert_eq!((exit, out.as_str()), (Some(1), ""), "{err}");
    assert!(err.starts_with("Error: jawd isn't running: nothing answers on"), "{err}");
    let (exit, _, err) = jaw_settings(&home, &["gruvbox"]);
    assert_eq!(exit, Some(2), "a change that isn't a pair is refused before jaw connects: {err}");
    assert!(err.contains("invalid value 'gruvbox' for '[NAME=VALUE]...': it isn't a name=value pair"), "{err}");
    let (exit, _, err) = jaw_settings(&home, &["theme=nord", "theme=gruvbox"]);
    assert_eq!((exit, err.lines().next()), (Some(1), Some("Error: theme is asked for twice; give each setting once")));

    drop(jawd);
    assert!(!home.exists(), "the temporary folder is removed");
}
