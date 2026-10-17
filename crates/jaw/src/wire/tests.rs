//! jaw on a jawd: a scripted one that answers with the protocol's own fixtures, which the gate
//! runs, and a real `jawd --engine demo` on a folder of its own when `JAWD` names its binary.

use super::*;
use crate::data::Tick;
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

/// What the scripted jawd keeps between requests.
#[derive(Default)]
struct Kept {
    /// The methods it turned down, in order.
    refused: Vec<String>,
    /// Every request, in order.
    asked: Vec<Value>,
    /// Each message it took, by key.
    keyed: HashMap<String, String>,
    /// Whether to take the next message and drop the connection before answering, as a jawd
    /// killed between its commit and its answer does.
    swallow: bool,
}

/// A jawd that answers hello, the windows and find with the protocol's fixtures, takes messages
/// as jawd does, with their keys, turns down what jawd doesn't serve yet, and can go away and come
/// back on the same socket.
struct Scripted {
    socket: PathBuf,
    tell: std::sync::mpsc::Sender<Told>,
    kept: Arc<Mutex<Kept>>,
    _dir: tempfile::TempDir,
}

impl Scripted {
    fn refused(&self) -> Vec<String> {
        self.kept.lock().unwrap().refused.clone()
    }

    /// The params of every request for `method`, in order.
    fn asked(&self, method: &str) -> Vec<Value> {
        let kept = self.kept.lock().unwrap();
        kept.asked.iter().filter(|r| r["method"] == method).map(|r| r["params"].clone()).collect()
    }
}

/// Its answer to one line, or none when it drops the connection instead.
fn answer(line: &str, kept: &Mutex<Kept>) -> Option<Value> {
    let request: Value = serde_json::from_str(line).unwrap();
    let mut kept = kept.lock().unwrap();
    kept.asked.push(request.clone());
    let (id, params) = (request["id"].clone(), &request["params"]);
    let method = request["method"].as_str().unwrap_or_default().to_string();
    let error = |code: i32, message: &str, data: Value| {
        Some(json!({ "id": id, "error": { "code": code, "message": message, "data": data } }))
    };
    let result = match method.as_str() {
        "hello" => fixture("hello.result")["result"].clone(),
        "chats.watch" => fixture("chats.watch.result")["result"].clone(),
        "messages.watch" | "watch.move" => fixture("messages.watch.result")["result"].clone(),
        "find" if params["query"] == "ar" => fixture("find.result")["result"].clone(),
        "find" => json!({ "results": [] }),
        "send.text" if params["text"] == "one too many" => {
            let data = json!({ "limit": "outbox", "waiting": 1000, "failed": 3 });
            return error(code::LIMIT, "1000 messages of yours are waiting to send, 3 of them failed", data);
        }
        "send.text" => {
            let key = params["key"].as_str().unwrap_or_default().to_string();
            let n = kept.keyed.len();
            let message = kept.keyed.entry(key).or_insert_with(|| format!("3EB0{n:018X}")).clone();
            if std::mem::take(&mut kept.swallow) {
                return None;
            }
            let mut sent = fixture("send.text.result")["result"].clone();
            sent["message"] = json!(message);
            sent["row"]["id"] = json!(message);
            sent["row"]["spans"] = json!([{ "t": "text", "s": params["text"] }]);
            sent
        }
        "send.retry" if params["message"] == "3EB0GONE" => {
            return error(code::NOT_FOUND, "jawd has nothing of yours waiting to send as 3EB0GONE", Value::Null);
        }
        "chat.start" if params["phone"].as_str().is_some_and(|p| p.starts_with("+999")) => {
            return error(code::NOT_FOUND, "+999 123 456 isn't on WhatsApp", Value::Null);
        }
        "chat.start" => fixture("chat.start.result")["result"].clone(),
        "unwatch" | "send.retry" | "draft.set" | "read" | "typing" | "focus" | "chat.opened" => json!({}),
        _ => {
            kept.refused.push(method.clone());
            return error(code::UNKNOWN_METHOD, &format!("jawd doesn't know {method}"), Value::Null);
        }
    };
    Some(json!({ "id": id, "result": result }))
}

fn scripted() -> Scripted {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("jaw.sock");
    let (tell, told) = channel::<Told>();
    let kept = Arc::new(Mutex::new(Kept::default()));
    let (path, keeping) = (socket.clone(), kept.clone());
    std::thread::spawn(move || {
        loop {
            let listener = UnixListener::bind(&path).unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut served = vec![];
            let gone = loop {
                if let Ok((conn, _)) = listener.accept() {
                    conn.set_nonblocking(false).unwrap();
                    let (mut out, keeping) = (conn.try_clone().unwrap(), keeping.clone());
                    served.push(conn.try_clone().unwrap());
                    std::thread::spawn(move || {
                        for line in BufReader::new(conn).lines() {
                            let Ok(line) = line else { return };
                            let Some(reply) = answer(&line, &keeping) else {
                                let _ = out.shutdown(std::net::Shutdown::Both);
                                return;
                            };
                            let mut reply = serde_json::to_vec(&reply).unwrap();
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
    Scripted { socket, tell, kept, _dir: dir }
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
    assert!(
        screen.contains("▶ ▁▁▁▁▂▂▂▂▄▄▄▄") && screen.contains("  0:14"),
        "a voice message's waveform, from its row:\n{screen}"
    );
}

/// Waits until `enough` holds, settling the view meanwhile, for at most a few seconds.
fn until_true(v: &mut View, rx: &Receiver<crate::run::Event>, mut enough: impl FnMut(&mut View) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if enough(v) {
            return true;
        }
        if Instant::now() > end {
            return false;
        }
        run::settle_live(v, rx, Duration::from_millis(100));
    }
}

const ANA: &str = "351912345678@s.whatsapp.net";

#[test]
fn each_message_goes_with_a_key_of_its_own_and_what_jawd_does_not_serve_yet_is_said_quietly() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "olá\n").unwrap();
    run::press_live(&mut v, &rx, "até já\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("send.text").len() == 2));
    let sent = jawd.asked("send.text");
    let keys: Vec<&str> = sent.iter().map(|p| p["key"].as_str().unwrap()).collect();
    assert!(keys.iter().all(|k| k.starts_with("jaw-") && k.len() == 36), "{keys:?}");
    assert_ne!(keys[0], keys[1], "a key for each message");
    assert_eq!((sent[0]["chat"].as_str(), sent[0]["text"].as_str()), (Some(ANA), Some("olá")));
    let screen = frame(&mut v);
    assert!(!screen.contains("not sent") && !screen.contains("next step"), "{screen}");
    let sending = ["send.text", "typing", "draft.set", "read", "focus"];
    assert!(!jawd.refused().iter().any(|m| sending.contains(&m.as_str())), "{:?}", jawd.refused());
    run::press_live(&mut v, &rx, "<c-k>pin this\n").unwrap();
    let screen = until(&mut v, &rx, "pinning");
    assert!(screen.contains("pinning comes with the next step"), "{screen}");
}

#[test]
fn a_message_whose_answer_the_connection_lost_is_asked_for_again_with_its_key_and_sent_once() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    jawd.kept.lock().unwrap().swallow = true;
    run::press_live(&mut v, &rx, "a caminho\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("send.text").len() == 2), "{:?}", jawd.asked("send.text"));
    let screen = until(&mut v, &rx, "jaw  online");
    let sent = jawd.asked("send.text");
    assert_eq!(sent[0], sent[1], "asked again as it was, with its key");
    assert_eq!(jawd.kept.lock().unwrap().keyed.len(), 1, "one message");
    assert!(!screen.contains("not sent") && !screen.contains("│ a caminho"), "nothing goes back to the box:\n{screen}");
    assert!(!screen.contains("asking jawd again"), "jawd answered for it:\n{screen}");
    run::press_live(&mut v, &rx, "outra\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("send.text").len() == 3));
    assert_ne!(jawd.asked("send.text")[2]["key"], sent[0]["key"], "the next message has a key of its own");
}

#[test]
fn a_message_lost_longer_ago_than_jawd_keeps_keys_goes_back_to_its_box_rather_than_being_asked_again() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    let params = json!({ "chat": ANA, "text": "a caminho", "key": "jaw-old" });
    for (key, ago) in [("jaw-old", KEY_KEPT + Duration::from_secs(60)), ("jaw-new", Duration::from_secs(60))] {
        let mut params = params.clone();
        params["key"] = json!(key);
        let first = SystemTime::now() - ago;
        let (chat, text, key) = (ANA.to_string(), "a caminho".to_string(), key.to_string());
        wire.again.push(Sending { chat, text, key, params, first });
    }
    jawd.tell.send(Told::Go).unwrap();
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Unreachable { .. })));
    jawd.tell.send(Told::Come).unwrap();
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Sent { .. })));
    let keys: Vec<Value> = jawd.asked("send.text").iter().map(|p| p["key"].clone()).collect();
    assert_eq!(keys, [json!("jaw-new")], "only the one jawd still knows the key of is asked again");
    let back = said.iter().find_map(|u| match u {
        Update::Unsent { key, why, .. } => Some((key.as_str(), why.as_str())),
        _ => None,
    });
    assert!(back.is_some_and(|(key, why)| key == "jaw-old" && why.contains("look in the chat")), "{said:?}");
}

#[test]
fn a_message_past_the_outbox_limit_stays_in_the_box_with_its_key_and_says_why() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "one too many\n").unwrap();
    let screen = until(&mut v, &rx, "not sent");
    assert!(screen.contains("not sent: 1000 wait to send, 3 failed; it can go once some have"), "{screen}");
    assert!(screen.contains("│ one too many"), "the text is kept:\n{screen}");
    run::press_live(&mut v, &rx, "\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("send.text").len() == 2));
    let sent = jawd.asked("send.text");
    assert_eq!(sent[0]["key"], sent[1]["key"], "the same message, under the key it was composed with");
}

#[test]
fn a_chat_is_started_by_number_and_one_not_on_whatsapp_says_so() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<c-k>+351 913 333 333").unwrap();
    run::press_live(&mut v, &rx, "\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| !jawd.asked("chat.start").is_empty()));
    assert_eq!(jawd.asked("chat.start")[0], json!({ "phone": "+351 913 333 333" }));
    let screen = until(&mut v, &rx, "+351913333333");
    assert!(screen.lines().nth(1).unwrap().contains("+351913333333"), "it's open:\n{screen}");
    run::press_live(&mut v, &rx, "<c-k>+999 123 456 789").unwrap();
    run::press_live(&mut v, &rx, "\n").unwrap();
    let screen = until(&mut v, &rx, "isn't on WhatsApp");
    assert!(screen.contains("+999 123 456 isn't on WhatsApp"), "jawd's reason:\n{screen}");
    assert!(!screen.contains("next step"), "{screen}");
}

/// Drives a `Wire` on its own, gathering what it says.
fn drive(
    wire: &mut Wire,
    rx: &Receiver<crate::run::Event>,
    said: &mut Vec<Update>,
    enough: impl Fn(&[Update]) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(10);
    while !enough(said) && Instant::now() < end {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(crate::run::Event::Jawd) => said.extend(wire.drain()),
            Ok(crate::run::Event::Update(u)) => said.push(u),
            _ => {}
        }
    }
}

#[test]
fn drafts_and_read_positions_jawd_missed_are_asked_for_again_once_it_is_back_the_newest_only() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    jawd.tell.send(Told::Go).unwrap();
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Unreachable { .. })));
    wire.draft(ANA, "até");
    wire.read(ANA, "3EB0A1B2C3D4E5F6");
    wire.draft(ANA, "até já");
    jawd.tell.send(Told::Come).unwrap();
    drive(&mut wire, &rx, &mut said, |_| !jawd.asked("draft.set").is_empty() && !jawd.asked("read").is_empty());
    assert_eq!(jawd.asked("draft.set"), [json!({ "chat": ANA, "text": "até já" })], "the newest draft, once");
    assert_eq!(jawd.asked("read"), [json!({ "chat": ANA, "up_to": "3EB0A1B2C3D4E5F6" })]);
    wire.retry("3EB0GONE");
    wire.retry("3EB0C431F2A9B8E7D61A2B");
    drive(&mut wire, &rx, &mut said, |_| jawd.asked("send.retry").len() == 2);
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Notice(_))));
    let notices: Vec<&Update> = said.iter().filter(|u| matches!(u, Update::Notice(_))).collect();
    assert_eq!(
        notices,
        [&Update::Notice("not sent again: jawd has nothing of yours waiting to send as 3EB0GONE".into())],
        "only the retry jawd turned down says so"
    );
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
        let log =
            std::fs::OpenOptions::new().create(true).append(true).open(self.home.path().join("jawd.log")).unwrap();
        let child = Command::new(&self.bin)
            .args(["--engine", "demo"])
            .env("JAW_HOME", self.home.path())
            .env("JAW_NOTIFY", "off")
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap();
        self.child = Some(child);
        let socket = jaw_proto::local::socket(self.home.path());
        let end = Instant::now() + Duration::from_secs(20);
        while !Real::ready(&socket) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Whether jawd answers, linked, with the demo's two chats in its list.
    fn ready(socket: &Path) -> bool {
        let Ok((mut connection, welcome)) = jaw_client::Connection::open(socket, "jaw-test", "0") else { return false };
        let watch = json!({ "folder": "all", "around": "top", "before": 0, "after": 10 });
        welcome.account.state == AccountState::Linked
            && connection
                .call("chats.watch", watch)
                .is_ok_and(|v| v["rows"].as_array().is_some_and(|rows| rows.len() >= 2))
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

/// A line that went through the relay, and which way.
#[derive(Clone, Debug)]
struct Passed {
    from_jawd: bool,
    line: Value,
}

/// A relay between jaw and a real jawd that keeps every line that passes, and can swallow
/// jawd's answer to the next `send.text`: it says so on `swallowed`, and once the test has killed
/// jawd and says so on `cut`, drops jaw's connection, as a jawd killed between its commit and its
/// answer does. jaw reconnecting is relayed to whichever jawd is on the socket then.
struct Relay {
    socket: PathBuf,
    passed: Arc<Mutex<Vec<Passed>>>,
    swallow: Arc<std::sync::atomic::AtomicBool>,
    /// While set, jaw's connections are turned away, as while jawd isn't there.
    hold: Arc<std::sync::atomic::AtomicBool>,
    swallowed: Receiver<()>,
    cut: std::sync::mpsc::Sender<()>,
    _dir: tempfile::TempDir,
}

impl Relay {
    fn to(jawd: &Real) -> Relay {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("relay.sock");
        let upstream = jaw_proto::local::socket(jawd.home.path());
        let listener = UnixListener::bind(&socket).unwrap();
        let passed = Arc::new(Mutex::new(vec![]));
        let swallow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hold = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (swallowed_tx, swallowed) = channel();
        let (cut, cutting) = channel::<()>();
        let cutting = Arc::new(Mutex::new(cutting));
        let (keep, swallowing, holding) = (passed.clone(), swallow.clone(), hold.clone());
        std::thread::spawn(move || {
            for jaw in listener.incoming() {
                let Ok(jaw) = jaw else { return };
                if holding.load(std::sync::atomic::Ordering::SeqCst) {
                    continue;
                }
                let Ok(jawd) = UnixStream::connect(&upstream) else { continue };
                let sends = Arc::new(Mutex::new(std::collections::HashSet::new()));
                let (up, keeping, asked) = (jawd.try_clone().unwrap(), keep.clone(), sends.clone());
                let reading = jaw.try_clone().unwrap();
                std::thread::spawn(move || {
                    let mut up = up;
                    for line in BufReader::new(reading).lines() {
                        let Ok(line) = line else { break };
                        let v: Value = serde_json::from_str(&line).unwrap();
                        if v["method"] == "send.text" {
                            asked.lock().unwrap().insert(v["id"].as_u64().unwrap());
                        }
                        keeping.lock().unwrap().push(Passed { from_jawd: false, line: v });
                        if up.write_all(format!("{line}\n").as_bytes()).is_err() {
                            break;
                        }
                    }
                    let _ = up.shutdown(std::net::Shutdown::Both);
                });
                let (keeping, swallowing, told, cutting) =
                    (keep.clone(), swallowing.clone(), swallowed_tx.clone(), cutting.clone());
                std::thread::spawn(move || {
                    let mut down = jaw;
                    for line in BufReader::new(jawd.try_clone().unwrap()).lines() {
                        let Ok(line) = line else { break };
                        let v: Value = serde_json::from_str(&line).unwrap();
                        let answers_a_send = v["id"].as_u64().is_some_and(|id| sends.lock().unwrap().contains(&id));
                        keeping.lock().unwrap().push(Passed { from_jawd: true, line: v });
                        if answers_a_send && swallowing.swap(false, std::sync::atomic::Ordering::SeqCst) {
                            let _ = told.send(());
                            let _ = cutting.lock().unwrap().recv();
                            break;
                        }
                        if down.write_all(format!("{line}\n").as_bytes()).is_err() {
                            break;
                        }
                    }
                    let _ = down.shutdown(std::net::Shutdown::Both);
                    let _ = jawd.shutdown(std::net::Shutdown::Both);
                });
            }
        });
        Relay { socket, passed, swallow, hold, swallowed, cut, _dir: dir }
    }

    /// Starts `jawd` again, turning jaw away until it's linked and has its chats.
    fn restart(&self, jawd: &mut Real) {
        self.hold.store(true, std::sync::atomic::Ordering::SeqCst);
        jawd.start();
        self.hold.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn passed(&self) -> Vec<Passed> {
        self.passed.lock().unwrap().clone()
    }

    /// The params of each request jaw made for `method`, with jawd's answer if it came.
    fn asked(&self, method: &str) -> Vec<(Value, Option<Value>)> {
        let passed = self.passed();
        let answer = |id: &Value| passed.iter().find(|p| p.from_jawd && p.line["id"] == *id).map(|p| p.line.clone());
        passed
            .iter()
            .filter(|p| !p.from_jawd && p.line["method"] == method)
            .map(|p| (p.line["params"].clone(), answer(&p.line["id"])))
            .collect()
    }

    /// Each status jawd sent for message `id`, in answers, watches and pushes, in order.
    fn statuses(&self, id: &str) -> Vec<String> {
        let mut seen = vec![];
        for p in self.passed().iter().filter(|p| p.from_jawd) {
            let v = &p.line;
            let rows = [&v["result"]["row"], &v["result"]["rows"], &v["params"]["upsert"]];
            for row in rows.iter().flat_map(|r| r.as_array().cloned().unwrap_or_else(|| vec![(*r).clone()])) {
                if row["id"] == id
                    && let Some(status) = row["status"].as_str()
                    && seen.last().is_none_or(|s: &String| s != status)
                {
                    seen.push(status.to_string());
                }
            }
        }
        seen
    }
}

/// Where a status is on the way from pending to read.
fn rank(status: &str) -> usize {
    ["pending", "sent", "delivered", "read", "played"].iter().position(|s| *s == status).unwrap_or(usize::MAX)
}

#[test]
fn on_a_real_jawd_a_message_goes_pending_then_sent_delivered_and_read_and_what_sending_asks_is_answered() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "a caminho").unwrap();
    v.clock = Some(NOW + 2_000);
    v.tick();
    v.on_input(ratatui::crossterm::event::Event::FocusLost);
    v.on_input(ratatui::crossterm::event::Event::FocusGained);
    run::press_live(&mut v, &rx, "\n").unwrap();
    let read =
        |v: &mut View| crate::view::tests::mine_saying(v, "a caminho").iter().any(|m| m.status == Some(Tick::Read));
    assert!(until_true(&mut v, &rx, read), "{}", frame(&mut v));
    let screen = frame(&mut v);
    eprintln!("{screen}");
    assert!(screen.contains("a caminho") && screen.contains("✓✓"), "{screen}");

    let sends = relay.asked("send.text");
    assert_eq!(sends.len(), 1, "{sends:?}");
    let (params, answer) = &sends[0];
    assert!(params["key"].as_str().is_some_and(|k| k.starts_with("jaw-")), "{params}");
    let answer = answer.as_ref().expect("an answer");
    let id = answer["result"]["message"].as_str().expect("send.text's message").to_string();
    assert!(id.starts_with("3EB0") && id.len() == 22, "{answer}");
    assert_eq!(answer["result"]["row"]["id"], id.as_str(), "send.text's row: {answer}");
    assert_eq!(answer["result"]["row"]["status"], "pending", "{answer}");
    let statuses = relay.statuses(&id);
    eprintln!("{statuses:?}");
    assert_eq!(statuses.first().map(String::as_str), Some("pending"), "{statuses:?}");
    assert!(statuses.windows(2).all(|w| rank(&w[0]) < rank(&w[1])), "only ever forward: {statuses:?}");
    for status in ["delivered", "read"] {
        assert!(statuses.iter().any(|s| s == status), "{status} came: {statuses:?}");
    }
    assert_eq!(crate::view::tests::mine_saying(&v, "a caminho").len(), 1);

    for method in ["typing", "focus", "draft.set", "read", "send.text"] {
        let asked = relay.asked(method);
        assert!(!asked.is_empty(), "jaw asked {method}");
        for (params, answer) in asked {
            let answer = answer.unwrap_or_else(|| panic!("{method} {params} was answered"));
            assert!(answer.get("result").is_some(), "{method} {params}: {answer}");
        }
    }
    assert!(relay.asked("typing").iter().any(|(p, _)| p["state"] == "composing"));
    assert!(relay.asked("focus").iter().any(|(p, _)| p["focused"] == false));
}

#[test]
fn on_a_real_jawd_killed_between_sending_and_answering_jaw_asks_again_with_its_key_and_there_is_one_message() {
    let Some(mut jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    relay.swallow.store(true, std::sync::atomic::Ordering::SeqCst);
    run::press_live(&mut v, &rx, "chego às nove\n").unwrap();
    relay.swallowed.recv_timeout(Duration::from_secs(10)).expect("jawd answered the send");
    jawd.stop();
    relay.cut.send(()).unwrap();
    let gone = until(&mut v, &rx, "can't reach jawd");
    assert!(gone.contains("asking jawd again once it's back"), "{gone}");
    assert!(!gone.contains("│ chego às nove"), "{gone}");
    relay.restart(&mut jawd);
    // The demo's made-up WhatsApp goes with the jawd killed, so a message it had by then hears no
    // receipts after; one it hadn't is sent again with the same ID and goes on to read.
    let sent = |v: &mut View| {
        crate::view::tests::mine_saying(v, "chego às nove")
            .iter()
            .any(|m| matches!(m.status, Some(Tick::Sent | Tick::Delivered | Tick::Read)))
    };
    assert!(until_true(&mut v, &rx, sent), "{}", frame(&mut v));
    std::thread::sleep(Duration::from_secs(2));
    run::settle_live(&mut v, &rx, Duration::from_secs(1));
    let screen = frame(&mut v);
    eprintln!("{screen}");
    assert_eq!(crate::view::tests::mine_saying(&v, "chego às nove").len(), 1, "one message, not two:\n{screen}");
    assert!(!screen.contains("not sent") && !screen.contains("│ chego às nove"), "{screen}");
    assert!(!screen.contains("asking jawd again"), "{screen}");

    let sends = relay.asked("send.text");
    assert_eq!(sends.len(), 2, "asked again once jawd was back: {sends:?}");
    assert_eq!(sends[0].0, sends[1].0, "with the same key");
    let message = |answer: &Option<Value>| answer.as_ref().unwrap()["result"]["message"].clone();
    assert_eq!(message(&sends[0].1), message(&sends[1].1), "and jawd answered with the message it had");
    let messages = |v: &Value| v["result"]["rows"].as_array().cloned().unwrap_or_default();
    let watched: Vec<Value> = relay
        .passed()
        .iter()
        .filter(|p| p.from_jawd)
        .flat_map(|p| messages(&p.line))
        .filter(|r| r["mine"] == true && r["spans"][0]["s"] == "chego às nove")
        .map(|r| r["id"].clone())
        .collect();
    assert!(watched.iter().all(|id| *id == message(&sends[0].1)), "jawd's windows hold that one only: {watched:?}");
}

#[test]
fn on_a_real_jawd_a_chat_starts_by_number_and_reading_marks_it_read() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    let unread = |relay: &Relay, chat: &str| {
        relay
            .passed()
            .iter()
            .filter(|p| p.from_jawd)
            .flat_map(|p| {
                let v = &p.line;
                [&v["result"]["rows"], &v["params"]["upsert"]]
                    .iter()
                    .flat_map(|r| r.as_array().cloned().unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .filter(|r| r["id"] == chat && r.get("unread").is_some())
            .map(|r| r["unread"].as_u64().unwrap())
            .next_back()
    };
    const CLIMBING: &str = "120363012345678901@g.us";
    assert!(until_true(&mut v, &rx, |_| unread(&relay, ANA) == Some(0)), "Ana's chat, open, is read");
    let reads = relay.asked("read");
    assert_eq!(reads[0].0, json!({ "chat": ANA, "up_to": "3EB0DEMO00000003" }), "{reads:?}");
    run::press_live(&mut v, &rx, "<a-down>").unwrap();
    v.clock = Some(NOW + 5_000);
    v.tick();
    assert!(until_true(&mut v, &rx, |_| unread(&relay, CLIMBING) == Some(0)), "{:?}", relay.asked("read"));
    assert!(relay.asked("read").iter().any(|(p, a)| *p == json!({ "chat": CLIMBING, "up_to": "3EB0DEMO00000002" })
        && a.as_ref().is_some_and(|a| a.get("result").is_some())));

    run::press_live(&mut v, &rx, "<c-k>+351 913 777 777").unwrap();
    run::press_live(&mut v, &rx, "\n").unwrap();
    let screen = until(&mut v, &rx, "+351913777777");
    eprintln!("{screen}");
    assert!(screen.lines().nth(1).unwrap().contains("+351913777777"), "it's open:\n{screen}");
    let started = relay.asked("chat.start");
    let chat = started[0].1.as_ref().unwrap()["result"]["chat"].clone();
    assert_eq!(chat["id"], "351913777777@s.whatsapp.net", "{chat}");
    run::press_live(&mut v, &rx, "olá\n").unwrap();
    let said = |v: &mut View| !crate::view::tests::mine_saying(v, "olá").is_empty();
    assert!(until_true(&mut v, &rx, said), "{}", frame(&mut v));
    run::press_live(&mut v, &rx, "<c-k>+999 123 456 789").unwrap();
    run::press_live(&mut v, &rx, "\n").unwrap();
    let screen = until(&mut v, &rx, "WhatsApp");
    eprintln!("{screen}");
    let refused = relay.asked("chat.start");
    let why = refused[1].1.as_ref().unwrap()["error"]["message"].as_str().unwrap().to_string();
    assert!(screen.contains(&why[..why.len().min(40)]), "jawd's reason, {why:?}:\n{screen}");
}

#[test]
fn on_a_real_jawd_a_draft_typed_in_one_jaw_is_in_the_other_and_sending_it_clears_it_in_both() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let socket = jaw_proto::local::socket(jawd.home.path());
    let (mut one, rx1) = view_on(socket.clone(), 140, 30);
    let (mut two, rx2) = view_on(socket.clone(), 140, 30);
    run::press_live(&mut two, &rx2, "<a-down>").unwrap();
    run::press_live(&mut one, &rx1, "até logo").unwrap();
    run::press_live(&mut one, &rx1, "<a-down>").unwrap();
    let screen = until(&mut two, &rx2, "draft: até logo");
    assert!(screen.contains("draft: até logo"), "the other jaw's list:\n{screen}");
    run::press_live(&mut two, &rx2, "<a-up>").unwrap();
    let screen = until(&mut two, &rx2, "│ até logo");
    assert!(screen.contains("│ até logo"), "in its box:\n{screen}");
    run::press_live(&mut two, &rx2, "\n").unwrap();
    let said = |v: &mut View| !crate::view::tests::mine_saying(v, "até logo").is_empty();
    assert!(until_true(&mut two, &rx2, said), "{}", frame(&mut two));
    let cleared = |v: &mut View| {
        let screen = frame(v);
        !screen.contains("draft: até logo")
    };
    assert!(until_true(&mut one, &rx1, cleared), "{}", frame(&mut one));
    run::press_live(&mut one, &rx1, "<a-up>").unwrap();
    let said = |v: &mut View| crate::view::tests::mine_saying(v, "até logo").len() == 1;
    assert!(until_true(&mut one, &rx1, said), "{}", frame(&mut one));
    let screen = frame(&mut one);
    assert!(!screen.contains("│ até logo"), "sent, so no longer a draft:\n{screen}");
}

#[test]
fn on_a_real_jawd_retrying_a_message_jawd_no_longer_holds_says_why_and_sends_nothing() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jaw_proto::local::socket(jawd.home.path()), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    wire.send_text(ANA, "vou já", "jaw-retry-test", None, &[]);
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Sent { .. })));
    let Some(Update::Sent { message, .. }) = said.iter().find(|u| matches!(u, Update::Sent { .. })).cloned() else {
        panic!("{said:?}")
    };
    wire.retry(&message);
    std::thread::sleep(Duration::from_secs(3));
    wire.retry(&message);
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Notice(_))));
    let notices: Vec<&Update> = said.iter().filter(|u| matches!(u, Update::Notice(_))).collect();
    eprintln!("{notices:?}");
    assert!(
        !notices.is_empty()
            && notices.iter().all(
                |n| matches!(n, Update::Notice(t) if t.starts_with("not sent again: ") && !t.contains("next step"))
            ),
        "{notices:?}"
    );
    wire.send_text(ANA, "vou já", "jaw-retry-test", None, &[]);
    drive(&mut wire, &rx, &mut said, |s| s.iter().filter(|u| matches!(u, Update::Sent { .. })).count() == 2);
    let sent: Vec<&Update> = said.iter().filter(|u| matches!(u, Update::Sent { .. })).collect();
    assert_eq!(sent[0], sent[1], "the same key is the same message: {sent:?}");
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
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 100, 24);
    run::press_live(&mut v, &rx, "olá\n").unwrap();
    let read = |v: &mut View| crate::view::tests::mine_saying(v, "olá").iter().any(|m| m.status == Some(Tick::Read));
    assert!(until_true(&mut v, &rx, read), "{}", frame(&mut v));
    let before = frame(&mut v);
    jawd.stop();
    let gone = until(&mut v, &rx, "can't reach jawd");
    eprintln!("{gone}");
    assert_eq!(gone.lines().skip(1).collect::<Vec<_>>(), before.lines().skip(1).collect::<Vec<_>>());
    relay.restart(&mut jawd);
    let back = until(&mut v, &rx, "jaw  online");
    eprintln!("{back}");
    assert_eq!(untimed(&back), untimed(&before), "back without a jump");
}

/// A screen below its top bar with its times of day left out, since the demo engine started
/// again sends its made-up messages at times of its new start.
fn untimed(screen: &str) -> Vec<String> {
    screen
        .lines()
        .skip(1)
        .map(|line| {
            let mut c: Vec<char> = line.chars().collect();
            for i in 0..c.len().saturating_sub(4) {
                let d = |j: usize| c[i + j].is_ascii_digit();
                if d(0) && d(1) && c[i + 2] == ':' && d(3) && d(4) {
                    c.splice(i..i + 5, "hh:mm".chars());
                }
            }
            c.into_iter().collect()
        })
        .collect()
}

#[test]
fn recording_before_jawd_serves_voice_says_it_comes_with_the_next_step_and_the_box_comes_back() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<c-r>").unwrap();
    let screen = until(&mut v, &rx, "voice comes with the next step");
    assert!(!screen.contains("● recording"), "the box is a box again:\n{screen}");
    assert!(jawd.refused().contains(&"voice.record".to_string()));
}

#[test]
fn message_actions_before_jawd_serves_them_say_they_come_with_the_next_step() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<up>2").unwrap();
    until(&mut v, &rx, "reacting comes with the next step");
    run::press_live(&mut v, &rx, "i").unwrap();
    let screen = until(&mut v, &rx, "a message's info comes with the next step");
    assert!(screen.contains("╭ info "), "on the sheet that asked:\n{screen}");
    let refused = jawd.refused();
    assert!(refused.contains(&"react".to_string()) && refused.contains(&"message.info".to_string()), "{refused:?}");
}
