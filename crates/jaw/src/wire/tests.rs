//! jaw on a jawd: a scripted one that answers with the protocol's own fixtures, which the gate
//! runs, and a real `jawd --engine demo` on a folder of its own when `JAWD` names its binary.

use super::*;
use crate::run;
use crate::theme::Theme;
use crate::view::View;
use chrono::FixedOffset;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

/// Friday 16 October 2026, 14:02 UTC, the day the fixtures' messages were sent.
const NOW: i64 = 1_792_159_320_000;

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../protocol/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// What the scripted jawd is told to do.
enum Told {
    Go,
    Come,
}

/// A jawd that answers hello, the windows and find with the protocol's fixtures, turns down what
/// jawd doesn't serve yet, and can go away and come back on the same socket.
struct Scripted {
    socket: PathBuf,
    tell: std::sync::mpsc::Sender<Told>,
    /// The methods it turned down, in order.
    refused: Arc<Mutex<Vec<String>>>,
    _dir: tempfile::TempDir,
}

fn answer(line: &str, refused: &Mutex<Vec<String>>) -> Value {
    let request: Value = serde_json::from_str(line).unwrap();
    let id = request["id"].clone();
    let method = request["method"].as_str().unwrap_or_default().to_string();
    let result = match method.as_str() {
        "hello" => fixture("hello.result")["result"].clone(),
        "chats.watch" => fixture("chats.watch.result")["result"].clone(),
        "messages.watch" | "watch.move" => fixture("messages.watch.result")["result"].clone(),
        "find" if request["params"]["query"] == "ar" => fixture("find.result")["result"].clone(),
        "find" => json!({ "results": [] }),
        "unwatch" => json!({}),
        _ => {
            refused.lock().unwrap().push(method.clone());
            return json!({ "id": id, "error": { "code": code::UNKNOWN_METHOD, "message": format!("jawd doesn't know {method}") } });
        }
    };
    json!({ "id": id, "result": result })
}

fn scripted() -> Scripted {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("jaw.sock");
    let (tell, told) = channel::<Told>();
    let refused = Arc::new(Mutex::new(vec![]));
    let (path, refusing) = (socket.clone(), refused.clone());
    std::thread::spawn(move || {
        loop {
            let listener = UnixListener::bind(&path).unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut served = vec![];
            let gone = loop {
                if let Ok((conn, _)) = listener.accept() {
                    conn.set_nonblocking(false).unwrap();
                    let (mut out, refusing) = (conn.try_clone().unwrap(), refusing.clone());
                    served.push(conn.try_clone().unwrap());
                    std::thread::spawn(move || {
                        for line in BufReader::new(conn).lines() {
                            let Ok(line) = line else { return };
                            let mut reply = serde_json::to_vec(&answer(&line, &refusing)).unwrap();
                            reply.push(b'\n');
                            if out.write_all(&reply).is_err() {
                                return;
                            }
                        }
                    });
                }
                match told.recv_timeout(Duration::from_millis(5)) {
                    Ok(Told::Go) => break true,
                    Ok(Told::Come) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break false,
                }
            };
            for conn in served {
                let _ = conn.shutdown(std::net::Shutdown::Both);
            }
            drop(listener);
            let _ = std::fs::remove_file(&path);
            if !gone || !matches!(told.recv(), Ok(Told::Come)) {
                return;
            }
        }
    });
    Scripted { socket, tell, refused, _dir: dir }
}

fn view_on(socket: PathBuf, width: u16, height: u16) -> (View, Receiver<crate::run::Event>) {
    let (tx, rx) = channel();
    let mut v = View::new(Box::new(Wire::connect(socket, tx)));
    v.clock = Some(NOW);
    v.tz = FixedOffset::east_opt(0).unwrap();
    v.theme = Theme::with(true);
    v.no_color = false;
    v.resize(width, height);
    run::settle_live(&mut v, &rx, Duration::from_secs(5));
    (v, rx)
}

fn frame(v: &mut View) -> String {
    let (w, h) = v.size().unwrap();
    run::snapshot(v, w, h)
}

/// Waits for the screen to say something, for at most a few seconds.
fn until(v: &mut View, rx: &Receiver<crate::run::Event>, said: &str) -> String {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        run::settle_live(v, rx, Duration::from_millis(300));
        let screen = frame(v);
        if screen.contains(said) || Instant::now() > end {
            return screen;
        }
    }
}

#[test]
fn jaw_draws_what_jawd_sends_through_the_client() {
    let jawd = scripted();
    let (mut v, _rx) = view_on(jawd.socket.clone(), 140, 30);
    let screen = frame(&mut v);
    assert!(screen.lines().next().unwrap().contains("jaw  online"), "{screen}");
    assert!(screen.contains("Ana Ribeiro") && screen.contains("Lisbon Climbing"), "{screen}");
    assert!(screen.contains("did you get the photos from Sunday?") && screen.contains("❤ 2"), "{screen}");
    assert!(screen.contains("┃ Ana did you get the photos from Sunday?"), "a reply's quote, from its row:\n{screen}");
    assert!(screen.contains("▶ voice message"), "a kind with no summary from jawd yet has jaw's:\n{screen}");
}

#[test]
fn what_jawd_does_not_serve_yet_is_said_quietly_where_it_was_asked() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "olá\n").unwrap();
    let screen = until(&mut v, &rx, "not sent");
    assert!(screen.contains("not sent: sending comes with the next step"), "on the box it went back to:\n{screen}");
    assert!(screen.contains("│ olá"), "{screen}");
    let refused = jawd.refused.lock().unwrap().clone();
    assert!(refused.contains(&"send.text".to_string()), "{refused:?}");
    run::press_live(&mut v, &rx, "<c-k>pin this\n").unwrap();
    let screen = until(&mut v, &rx, "pinning");
    assert!(screen.contains("pinning comes with the next step"), "{screen}");
}

#[test]
fn find_goes_through_the_client_and_its_answer_to_the_palette() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 34);
    run::press_live(&mut v, &rx, "<c-k>ar").unwrap();
    let screen = until(&mut v, &rx, "Artur Reis");
    assert!(screen.contains("Artur Reis") && screen.contains("Archived") && screen.contains("Arrábida"), "{screen}");
    assert!(screen.contains("messages 214"), "jawd's counts on the strip:\n{screen}");
}

#[test]
fn when_jawd_goes_the_screen_keeps_what_it_had_and_comes_back_without_a_jump() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 100, 30);
    let before = frame(&mut v);
    jawd.tell.send(Told::Go).unwrap();
    let gone = until(&mut v, &rx, "can't reach jawd");
    assert!(gone.lines().next().unwrap().contains("can't reach jawd · trying again in"), "{gone}");
    assert_eq!(
        gone.lines().skip(1).collect::<Vec<_>>(),
        before.lines().skip(1).collect::<Vec<_>>(),
        "the screen keeps what it had"
    );
    jawd.tell.send(Told::Come).unwrap();
    let back = until(&mut v, &rx, "jaw  online");
    assert_eq!(back, before, "and comes back as it was");
}

/// A real jawd on a folder of its own, with its demo engine.
struct Real {
    bin: PathBuf,
    home: tempfile::TempDir,
    child: Option<Child>,
}

impl Real {
    fn start(&mut self) {
        let child = Command::new(&self.bin)
            .args(["--engine", "demo"])
            .env("JAW_HOME", self.home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.child = Some(child);
        let socket = jaw_proto::local::socket(self.home.path());
        let end = Instant::now() + Duration::from_secs(20);
        while UnixStream::connect(&socket).is_err() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Real {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A real jawd when `JAWD` names its binary; the gate has none built, so these say so and pass.
fn real() -> Option<Real> {
    let bin = PathBuf::from(std::env::var_os("JAWD")?);
    let mut real = Real { bin, home: tempfile::tempdir().unwrap(), child: None };
    real.start();
    Some(real)
}

#[test]
fn on_a_real_jawd_jaw_draws_at_140_100_and_80_columns_and_keeps_its_screen_through_a_restart() {
    let Some(mut jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let socket = jaw_proto::local::socket(jawd.home.path());
    for (w, h) in [(140, 24), (100, 24), (80, 24)] {
        let (mut v, rx) = view_on(socket.clone(), w, h);
        if w == 80 {
            run::press_live(&mut v, &rx, "\n").unwrap();
        }
        let screen = frame(&mut v);
        eprintln!("{screen}");
        assert!(screen.contains("the one at the lake"), "{w}:\n{screen}");
    }
    let (mut v, rx) = view_on(socket.clone(), 100, 24);
    run::press_live(&mut v, &rx, "olá\n").unwrap();
    let screen = until(&mut v, &rx, "not sent");
    assert!(screen.contains("not sent: sending comes with the next step"), "{screen}");
    run::press_live(&mut v, &rx, "<c-u>").unwrap();
    let before = frame(&mut v);
    jawd.stop();
    let gone = until(&mut v, &rx, "can't reach jawd");
    eprintln!("{gone}");
    assert_eq!(gone.lines().skip(1).collect::<Vec<_>>(), before.lines().skip(1).collect::<Vec<_>>());
    jawd.start();
    let back = until(&mut v, &rx, "jaw  online");
    eprintln!("{back}");
    assert_eq!(
        back.lines().skip(1).collect::<Vec<_>>(),
        before.lines().skip(1).collect::<Vec<_>>(),
        "back without a jump"
    );
}
