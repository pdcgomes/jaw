//! `jaw settings`, the binary as the owner runs it, on a `jawd --engine demo` of its own: jawd is
//! built beside this test, runs on a temporary folder that each child is pointed at with
//! `JAW_HOME`, and is killed through its own child when the test is done. A jawd older or newer
//! than this jaw is stood in for by one that only says hello. Everything the test waits for has a
//! deadline, so a jawd or a `jaw` that never answers fails it rather than holding the gate.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use jaw_proto::settings::TERMINAL;

/// How long a fresh jawd has to answer hello, making its jaw.db on a loaded machine.
const JAWD_ANSWERS: Duration = Duration::from_secs(60);

/// How long one `jaw settings` has to finish.
const JAW_FINISHES: Duration = Duration::from_secs(30);

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
    /// The asking runs on a thread of its own: jawd binds jaw.sock before it serves, so one stuck
    /// starting takes the connection and never answers it.
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
        let (answered, answers) = channel();
        let asking = socket.clone();
        std::thread::spawn(move || {
            while Instant::now() < end {
                let hello = jaw_client::Connection::open(&asking, TERMINAL, "0");
                if hello.is_ok_and(|(_, hello)| hello.settings.is_some()) {
                    let _ = answered.send(());
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        if answers.recv_timeout(end.saturating_duration_since(Instant::now())).is_err() {
            let log = std::fs::read_to_string(jawd.home.path().join("jawd.log")).unwrap_or_default();
            panic!("jawd didn't answer on {} within {JAWD_ANSWERS:?}:\n{log}", socket.display());
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

/// A jawd as far as `jaw settings` asks one, on a temporary folder: it answers hello with the
/// protocol's fixture carrying `settings`, none for a jawd from before them, and every other request
/// as a method it doesn't know, as such a jawd does `settings.set`.
fn saying_hello(settings: Option<serde_json::Value>) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(jaw_proto::local::socket(home.path())).unwrap();
    let fixture = format!("{}/../../protocol/fixtures/hello.result.json", env!("CARGO_MANIFEST_DIR"));
    let mut hello: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    match settings {
        Some(settings) => hello["result"]["settings"] = settings,
        None => drop(hello["result"].as_object_mut().unwrap().remove("settings")),
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().map_while(Result::ok) {
            let mut out = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let asked: serde_json::Value = serde_json::from_str(&line).unwrap();
                let (id, method) = (&asked["id"], asked["method"].as_str().unwrap_or_default());
                let answer = match method {
                    "hello" => serde_json::json!({ "id": id, "result": hello["result"] }),
                    _ => {
                        serde_json::json!({ "id": id, "error": { "code": -32601, "message": format!("jawd doesn't know {method}") } })
                    }
                };
                if writeln!(out, "{answer}").is_err() {
                    break;
                }
            }
        }
    });
    home
}

/// `jaw settings` with `args` on the jawd in `home`: how it exited, and what it printed on stdout
/// and on stderr. One still running after `JAW_FINISHES` is killed, through its own child, and
/// fails the test with what it printed.
fn jaw_settings(home: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let (mut out, mut err) = (tempfile::tempfile().unwrap(), tempfile::tempfile().unwrap());
    let mut jaw = Command::new(env!("CARGO_BIN_EXE_jaw"))
        .arg("settings")
        .args(args)
        .env("JAW_HOME", home)
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
    let Some(exited) = exited else {
        panic!("jaw settings {} hadn't finished after {JAW_FINISHES:?}, so it was killed:\n{out}{err}", args.join(" "));
    };
    (exited.code(), out, err)
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
    assert_eq!(exit, Some(2), "a setting named twice is a usage error too, refused before jaw connects: {err}");
    assert!(err.starts_with("error: theme is asked for twice; give each setting once\n"), "{err}");
    assert!(err.contains("Usage: jaw settings [NAME=VALUE]..."), "{err}");

    drop(jawd);
    assert!(!home.exists(), "the temporary folder is removed");
}

#[test]
fn jaw_settings_on_a_jawd_from_before_settings_says_it_keeps_none_and_exits_with_1() {
    let home = saying_hello(None);
    for args in [&[][..], &["theme=nord"]] {
        let (exit, out, err) = jaw_settings(home.path(), args);
        let said = "Error: this jawd is older and keeps no settings; restart it\n";
        assert_eq!((exit, out.as_str(), err.as_str()), (Some(1), "", said), "jaw settings {}", args.join(" "));
    }
}

#[test]
fn jaw_settings_prints_a_theme_this_jaw_doesnt_know_by_its_id_alone() {
    let home = saying_hello(Some(serde_json::json!({ "theme": "rose-pine-moon", "messages": "bubbles" })));
    let (exit, out, err) = jaw_settings(home.path(), &[]);
    assert_eq!(exit, Some(0), "{err}");
    assert!(out.starts_with("theme     rose-pine-moon (not one this jaw knows)\n"), "{out}");
}
