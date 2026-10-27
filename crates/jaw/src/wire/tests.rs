//! jaw on a jawd: a scripted one that answers with the protocol's own fixtures, which the gate
//! runs, and a real `jawd --engine demo` on a folder of its own when `JAWD` names its binary.

use super::*;
use crate::data::Tick;
use crate::demo::{Clock, Demo, State};
use crate::run;
use crate::theme::Theme;
use crate::view::View;
use crate::view::tests::shown;
use chrono::FixedOffset;
use jaw_proto::settings::{Messages, Settings};
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
    /// Each action it did, by key, with its answer.
    acted: HashMap<String, Value>,
    /// Whether to take the next message, action, layout or settings change and drop the connection
    /// before answering, as a jawd killed between its commit and its answer does.
    swallow: bool,
    /// The panes' layout it keeps for jaw, and its version.
    layout: Option<Value>,
    layout_v: u64,
    /// The terminal's settings, which hello brings to a connection saying hello as the terminal
    /// and `settings.set` changes; none, as a jawd from before settings keeps, whose hello brings
    /// none.
    settings: Option<Value>,
    /// A change another interface makes after a connection's hello, kept before the next
    /// `settings.set`'s own, so its answer brings a setting that connection's hello didn't.
    meanwhile: Option<Value>,
}

/// A jawd that answers hello, the windows and find with the protocol's fixtures, takes messages
/// and actions as jawd does, with their keys, keeps the terminal's settings and pushes a change
/// once it has answered for it, turns down what jawd doesn't serve yet, and can go away and come
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

/// Its answer to one line, then the push that follows it if any, or none when it drops the
/// connection instead.
fn answer(line: &str, kept: &Mutex<Kept>) -> Option<Vec<Value>> {
    let request: Value = serde_json::from_str(line).unwrap();
    let mut kept = kept.lock().unwrap();
    kept.asked.push(request.clone());
    let (id, params) = (request["id"].clone(), &request["params"]);
    let method = request["method"].as_str().unwrap_or_default().to_string();
    let error = |code: i32, message: &str, data: Value| {
        Some(vec![json!({ "id": id, "error": { "code": code, "message": message, "data": data } })])
    };
    let mut push = None;
    let result = match method.as_str() {
        "hello" => {
            let mut hello = fixture("hello.result")["result"].clone();
            hello.as_object_mut().unwrap().remove("settings");
            if let Some(settings) = kept.settings.clone().filter(|_| params["client"] == TERMINAL) {
                hello["settings"] = settings;
            }
            hello
        }
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
        "edit" if params["text"] == "too late" => {
            return error(code::NOT_ALLOWED, "a message can be edited for 15 minutes after it's sent", Value::Null);
        }
        "react" | "edit" | "delete" | "forward" => {
            let key = params["key"].as_str().unwrap_or_default().to_string();
            let n = kept.acted.len();
            let done = if method == "forward" { json!({ "messages": [format!("3EB0F{n:017X}")] }) } else { json!({}) };
            let answer = kept.acted.entry(key).or_insert(done).clone();
            if std::mem::take(&mut kept.swallow) {
                return None;
            }
            answer
        }
        "message.info" => json!({
            "message": params["message"],
            "sent": 1_792_145_077_000_i64,
            "recipients": [{ "person": "person:12", "name": "Ana", "delivered": 1_792_145_076_000_i64, "read": null, "played": null }],
        }),
        "send.retry" if params["message"] == "3EB0GONE" => {
            return error(code::NOT_FOUND, "jawd has nothing of yours waiting to send as 3EB0GONE", Value::Null);
        }
        "chat.start" if params["phone"].as_str().is_some_and(|p| p.starts_with("+999")) => {
            return error(code::NOT_FOUND, "+999 123 456 isn't on WhatsApp", Value::Null);
        }
        "chat.start" => fixture("chat.start.result")["result"].clone(),
        "layout.get" => json!({ "interface": "jaw", "v": kept.layout_v, "layout": kept.layout }),
        "layout.set" => {
            let v = params["v"].as_u64().unwrap_or_default();
            if v > kept.layout_v {
                (kept.layout, kept.layout_v) = (Some(params["layout"].clone()), v);
            }
            let now = kept.layout_v == v && kept.layout.as_ref() == Some(&params["layout"]);
            if std::mem::take(&mut kept.swallow) {
                return None;
            }
            json!({ "interface": "jaw", "v": kept.layout_v, "layout": kept.layout, "kept": now })
        }
        "settings.set" if params["theme"] == "solarized" => {
            let data = json!({ "param": "theme", "allowed": ["gruvbox", "nord"] });
            return error(code::INVALID_PARAMS, "theme takes one of gruvbox, nord", data);
        }
        "settings.set" if params["theme"] == "garbled" => json!({}),
        "settings.set" if kept.settings.is_some() => {
            let was = kept.settings.clone().unwrap_or_default();
            let mut settings = was.clone();
            let meanwhile = kept.meanwhile.take().unwrap_or_default();
            for change in [&meanwhile, params] {
                for name in ["theme", "messages"] {
                    if let Some(value) = change.get(name) {
                        settings[name] = value.clone();
                    }
                }
            }
            if settings != was {
                push = Some(json!({ "method": "settings", "params": settings }));
            }
            kept.settings = Some(settings.clone());
            if std::mem::take(&mut kept.swallow) {
                return None;
            }
            json!({ "settings": settings })
        }
        "unwatch" | "send.retry" | "draft.set" | "read" | "typing" | "focus" | "chat.opened" => json!({}),
        _ => {
            kept.refused.push(method.clone());
            return error(code::UNKNOWN_METHOD, &format!("jawd doesn't know {method}"), Value::Null);
        }
    };
    Some([json!({ "id": id, "result": result })].into_iter().chain(push).collect())
}

fn scripted() -> Scripted {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("jaw.sock");
    let (tell, told) = channel::<Told>();
    let settings = fixture("hello.result")["result"]["settings"].clone();
    let kept = Arc::new(Mutex::new(Kept { settings: Some(settings), ..Kept::default() }));
    let (path, keeping) = (socket.clone(), kept.clone());
    // Listening before it returns, so jaw never connects to a socket that isn't there yet.
    let mut first = Some(UnixListener::bind(&socket).unwrap());
    std::thread::spawn(move || {
        loop {
            let listener = first.take().unwrap_or_else(|| UnixListener::bind(&path).unwrap());
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
                            let Some(said) = answer(&line, &keeping) else {
                                let _ = out.shutdown(std::net::Shutdown::Both);
                                return;
                            };
                            for line in said {
                                let mut line = serde_json::to_vec(&line).unwrap();
                                line.push(b'\n');
                                if out.write_all(&line).is_err() {
                                    return;
                                }
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
    view_with(socket, width, height, "")
}

/// jaw on `socket` with the experiments `on`, as JAW_EXPERIMENTS sets them before it says hello.
fn view_with(socket: PathBuf, width: u16, height: u16, on: &str) -> (View, Receiver<crate::run::Event>) {
    let (tx, rx) = channel();
    let mut v = View::new(Box::new(Wire::connect(socket, tx)));
    v.experiments = crate::experiments::Experiments::parse(on);
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
fn the_scripted_jawd_listens_once_it_is_made() {
    for _ in 0..20 {
        let jawd = scripted();
        assert!(UnixStream::connect(&jawd.socket).is_ok(), "jaw would find no jawd on {}", jawd.socket.display());
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

/// After jawd's hello, the chat list and then the open chat's messages each come a round trip
/// after the one before, and each is drawn as it comes, through the client and the loop, rather
/// than a frame after the last: so the first frame with a conversation is drawn sooner than two
/// frames after the hello, which waiting a frame for each answer never was.
#[test]
fn the_first_frame_with_a_conversation_is_drawn_as_soon_as_jawds_answers_come() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut v = View::new(Box::new(Wire::connect(jawd.socket.clone(), tx.clone())));
    v.clock = Some(NOW);
    let mut greeted = false;
    while !greeted {
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(run::Event::Jawd)), "jawd's hello");
        for u in v.drain_jawd() {
            greeted |= matches!(u, Update::Hello { .. });
            v.apply(u);
        }
    }
    let hello = Instant::now();
    let stop = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let _ = tx.send(run::Event::Stop);
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    let mut probe = run::Probe::default();
    run::drive(&mut v, &rx, &mut terminal, &mut probe, &mut |_| {}).unwrap();
    stop.join().unwrap();
    let first = probe.first.expect("the conversation was drawn") - hello;
    assert!(first < 2 * run::FRAME, "the first frame with a conversation came {first:?} after jawd's hello");
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
const CLIMBING: &str = "120363012345678901@g.us";

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
        wire.again.push(Again::Send(Sending { chat, text, key, params, first }));
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

/// The terminal's settings through the client: a change jawd keeps comes back as its answer, then
/// as jawd's push; one it refuses says why; each connection's hello brings what jawd keeps then,
/// so the change once jaw is back; and a hello without settings, a jawd's from before them, is
/// the defaults.
#[test]
fn the_settings_come_in_each_hello_and_jawds_answer_and_push_and_one_it_refuses_says_why() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    let hello = |said: &[Update]| {
        said.iter().find_map(|u| if let Update::Hello { settings, .. } = u { Some(settings.clone()) } else { None })
    };
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    assert_eq!(hello(&said), Some(Settings::default()), "{said:?}");
    wire.set_settings(&SettingsSet { theme: Some("gruvbox".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Settings(_))));
    let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Bubbles };
    let came: Vec<&Update> =
        said.iter().filter(|u| matches!(u, Update::SettingsKept(_) | Update::Settings(_))).collect();
    let answered = [&Update::SettingsKept(gruvbox.clone()), &Update::Settings(gruvbox.clone())];
    assert_eq!(came, answered, "the answer, then jawd's push");
    wire.set_settings(&SettingsSet { theme: Some("solarized".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::SettingsRefused(_))));
    assert!(said.iter().any(|u| matches!(u, Update::SettingsRefused(why) if why.contains("theme takes one of"))));
    assert_eq!(jawd.asked("settings.set"), [json!({ "theme": "gruvbox" }), json!({ "theme": "solarized" })]);

    let again = |wire: &mut Wire, said: &mut Vec<Update>| {
        said.clear();
        jawd.tell.send(Told::Go).unwrap();
        drive(wire, &rx, said, |s| s.iter().any(|u| matches!(u, Update::Unreachable { .. })));
        jawd.tell.send(Told::Come).unwrap();
        drive(wire, &rx, said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    };
    again(&mut wire, &mut said);
    assert_eq!(hello(&said), Some(gruvbox), "back, the settings jawd keeps now: {said:?}");
    jawd.kept.lock().unwrap().settings = None;
    again(&mut wire, &mut said);
    assert_eq!(hello(&said), Some(Settings::default()), "a hello without settings is the defaults: {said:?}");
}

/// `jaw settings` on the scripted jawd: without changes it prints what hello brings, saying hello as
/// the terminal, and asks for no change; changes go in one `settings.set` and what it prints is
/// jawd's answer, every setting in it, one another interface changed since hello too; a change
/// jawd refuses fails with jawd's own message, an answer jaw can't read says it was jawd's answer
/// to `settings.set`, and with nothing on the socket it says jawd isn't running.
#[test]
fn jaw_settings_prints_what_hello_brings_or_jawds_answer_to_one_change_and_fails_with_jawds_message() {
    let jawd = scripted();
    let changes = crate::tests::changes;
    let said = crate::settings(&jawd.socket, &[]).unwrap();
    assert!(
        said.starts_with("theme     tokyo-night (Tokyo Night)\n") && said.contains("\nmessages  bubbles\n"),
        "{said}"
    );
    assert!(jawd.asked("settings.set").is_empty(), "nothing is changed without changes");

    let said = crate::settings(&jawd.socket, &changes(&["theme=gruvbox", "messages=irc"])).unwrap();
    assert_eq!(jawd.asked("settings.set"), [json!({ "theme": "gruvbox", "messages": "irc" })], "one settings.set");
    assert!(said.starts_with("theme     gruvbox (Gruvbox)\n") && said.contains("\nmessages  irc\n"), "{said}");
    jawd.kept.lock().unwrap().meanwhile = Some(json!({ "theme": "kanagawa" }));
    let said = crate::settings(&jawd.socket, &changes(&["messages=bubbles"])).unwrap();
    let kanagawa =
        |said: &str| said.starts_with("theme     kanagawa (Kanagawa)\n") && said.contains("\nmessages  bubbles\n");
    assert!(kanagawa(&said), "jawd's answer, with the theme another interface chose since hello: {said}");
    let said = crate::settings(&jawd.socket, &[]).unwrap();
    assert!(kanagawa(&said), "hello's: {said}");
    assert_eq!(jawd.asked("settings.set").len(), 2, "and reading them changes nothing");
    assert!(jawd.asked("hello").iter().all(|hello| hello["client"] == TERMINAL), "{:?}", jawd.asked("hello"));

    let refused = crate::settings(&jawd.socket, &changes(&["theme=solarized"])).unwrap_err();
    assert_eq!(refused.to_string(), "theme takes one of gruvbox, nord", "the scripted jawd's own list, not jaw's");
    let garbled = crate::settings(&jawd.socket, &changes(&["theme=garbled"])).unwrap_err();
    assert_eq!(format!("{garbled:#}"), "jawd's answer to settings.set: missing field `settings`");
    let away = crate::settings(&jawd.socket.with_file_name("nobody.sock"), &[]).unwrap_err();
    assert!(away.to_string().starts_with("jawd isn't running"), "{away}");
}

/// Whether an update answers a settings change.
fn answers_settings(u: &Update) -> bool {
    matches!(u, Update::SettingsKept(_) | Update::SettingsRefused(_) | Update::SettingsUnconfirmed(_))
}

/// The footer at 80 columns once `update` has come, on the made-up account.
fn footer_at_80(update: &Update) -> String {
    let (tx, rx) = channel();
    let mut v = View::new(Box::new(Demo::start(tx, Clock::Fixed(NOW), State::Online, false)));
    v.clock = Some(NOW);
    v.resize(80, 24);
    run::settle(&mut v, &rx);
    v.apply(update.clone());
    frame(&mut v).lines().last().unwrap_or_default().to_string()
}

/// An answer to `settings.set` that doesn't read as the settings jawd kept still answers the
/// change, as not confirmed, so the screen doesn't wait for good on an answer that has come.
#[test]
fn an_answer_to_a_settings_change_that_cant_be_read_says_so() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    wire.set_settings(&SettingsSet { theme: Some("garbled".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(answers_settings));
    let answered: Vec<&Update> = said.iter().filter(|u| answers_settings(u)).collect();
    assert_eq!(answered, [&Update::SettingsUnconfirmed("jaw couldn't read jawd's answer".into())], "{said:?}");
}

/// A settings change whose answer the connection lost may have been kept: it's said as not
/// confirmed, and jawd's hello once it's back brings what it kept. One asked while jawd is away is
/// never sent, so it's not kept. The footer says each whole at 80 columns.
#[test]
fn a_settings_change_whose_answer_is_lost_is_unconfirmed_and_one_never_sent_is_not_kept() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    said.clear();
    jawd.kept.lock().unwrap().swallow = true;
    wire.set_settings(&SettingsSet { theme: Some("gruvbox".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    let lost = "jawd went away; you'll see what it kept when it's back";
    let answered: Vec<&Update> = said.iter().filter(|u| answers_settings(u)).collect();
    assert_eq!(answered, [&Update::SettingsUnconfirmed(lost.into())], "{said:?}");
    assert_eq!(footer_at_80(answered[0]), format!(" not confirmed: {lost}"), "whole at 80 columns");
    let gruvbox = Settings { theme: "gruvbox".into(), messages: Messages::Bubbles };
    let hello = said.iter().find_map(|u| if let Update::Hello { settings, .. } = u { Some(settings) } else { None });
    assert_eq!(hello, Some(&gruvbox), "back, what jawd kept: {said:?}");

    said.clear();
    jawd.tell.send(Told::Go).unwrap();
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Unreachable { .. })));
    wire.set_settings(&SettingsSet { theme: Some("nord".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(answers_settings));
    let answered: Vec<&Update> = said.iter().filter(|u| answers_settings(u)).collect();
    let unsent = "jawd isn't connected; try again in a moment";
    assert_eq!(answered, [&Update::SettingsRefused(unsent.into())], "{said:?}");
    assert_eq!(footer_at_80(answered[0]), format!(" not kept: {unsent}"), "whole at 80 columns");
}

/// A jawd from before settings, whose hello brings none and which doesn't know `settings.set`, is
/// said to be older and to keep no settings, whole at 80 columns; and `jaw settings` on it says so
/// and fails, with changes or without, asking it for none, rather than printing the defaults.
#[test]
fn a_jawd_from_before_settings_is_said_to_keep_none_in_the_footer_and_by_jaw_settings() {
    let jawd = scripted();
    jawd.kept.lock().unwrap().settings = None;
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    wire.set_settings(&SettingsSet { theme: Some("nord".into()), messages: None });
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(answers_settings));
    let answered: Vec<&Update> = said.iter().filter(|u| answers_settings(u)).collect();
    assert_eq!(answered, [&Update::SettingsRefused(OLDER.into())], "{said:?}");
    assert_eq!(footer_at_80(answered[0]), format!(" not kept: {OLDER}"), "whole at 80 columns");
    for changes in [vec![], crate::tests::changes(&["theme=nord"])] {
        let refused = crate::settings(&jawd.socket, &changes).unwrap_err();
        assert_eq!(refused.to_string(), OLDER, "{changes:?}");
    }
    assert_eq!(jawd.asked("settings.set").len(), 1, "only the footer's change was asked");
}

/// A theme this jaw doesn't know, as a newer jawd may keep, is printed by `jaw settings` by its ID
/// alone, with no name made up for it.
#[test]
fn jaw_settings_prints_a_theme_this_jaw_doesnt_know_by_its_id_alone() {
    let jawd = scripted();
    jawd.kept.lock().unwrap().settings = Some(json!({ "theme": "rose-pine-moon", "messages": "irc" }));
    let said = crate::settings(&jawd.socket, &[]).unwrap();
    assert!(said.starts_with("theme     rose-pine-moon (not one this jaw knows)\n"), "{said}");
    assert!(said.contains("\nmessages  irc\n"), "{said}");
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
    /// The engine it runs: `demo`, or `demo-unlinked` for one whose device has gone.
    engine: &'static str,
}

impl Real {
    fn start(&mut self) {
        let log =
            std::fs::OpenOptions::new().create(true).append(true).open(self.home.path().join("jawd.log")).unwrap();
        let child = Command::new(&self.bin)
            .args(["--engine", self.engine])
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

    /// Whether jawd answers, linked, with the demo's script played: its two chats in its list, the
    /// group named, as the script's last event names it once its messages are in.
    fn ready(socket: &Path) -> bool {
        let Ok((mut connection, welcome)) = jaw_client::Connection::open(socket, "jaw-test", "0") else { return false };
        let watch = json!({ "folder": "all", "around": "top", "before": 0, "after": 10 });
        let played = |rows: &Vec<Value>| rows.len() >= 2 && rows.iter().any(|row| row["name"] == "Lisbon Climbing");
        welcome.account.state == AccountState::Linked
            && connection.call("chats.watch", watch).is_ok_and(|v| v["rows"].as_array().is_some_and(played))
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
    let mut real = Real { bin, home: tempfile::tempdir().unwrap(), child: None, engine: "demo" };
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
/// jawd's answer to the next request for a method, `send.text` or an action: it says so on
/// `swallowed`, and once the test has killed jawd and says so on `cut`, drops jaw's connection, as
/// a jawd killed between its commit and its answer does. jaw reconnecting is relayed to whichever
/// jawd is on the socket then.
struct Relay {
    socket: PathBuf,
    passed: Arc<Mutex<Vec<Passed>>>,
    /// The method whose next answer it swallows.
    swallow: Arc<Mutex<Option<&'static str>>>,
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
        let swallow = Arc::new(Mutex::new(None));
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
                let methods = Arc::new(Mutex::new(HashMap::new()));
                let (up, keeping, asked) = (jawd.try_clone().unwrap(), keep.clone(), methods.clone());
                let reading = jaw.try_clone().unwrap();
                std::thread::spawn(move || {
                    let mut up = up;
                    for line in BufReader::new(reading).lines() {
                        let Ok(line) = line else { break };
                        let v: Value = serde_json::from_str(&line).unwrap();
                        if let (Some(id), Some(method)) = (v["id"].as_u64(), v["method"].as_str()) {
                            asked.lock().unwrap().insert(id, method.to_string());
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
                        let method = v["id"].as_u64().and_then(|id| methods.lock().unwrap().get(&id).cloned());
                        keeping.lock().unwrap().push(Passed { from_jawd: true, line: v });
                        let swallowed = method.is_some_and(|method| {
                            let mut swallow = swallowing.lock().unwrap();
                            let this = *swallow == Some(method.as_str());
                            if this {
                                *swallow = None;
                            }
                            this
                        });
                        if swallowed {
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

    /// Swallows jawd's answer to the next request for `method`.
    fn swallow(&self, method: &'static str) {
        *self.swallow.lock().unwrap() = Some(method);
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
    relay.swallow("send.text");
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

/// The fixtures' messages in Ana's chat: hers, and a voice message of yours.
const HERS: &str = "3EB0A1B2C3D4E5F6";
const VOICE: &str = "3EB0D2E3F4A5B6C7";

/// Whether `key` is one jaw made: `jaw-` and 32 hex digits.
fn made(key: &Value) -> bool {
    key.as_str()
        .and_then(|k| k.strip_prefix("jaw-"))
        .is_some_and(|hex| hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

#[test]
fn each_action_names_its_chat_with_a_key_of_its_own_and_message_info_names_its_chat() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<up>2<up>1d\n").unwrap();
    run::press_live(&mut v, &rx, "<up>far").unwrap();
    until(&mut v, &rx, "Ana Ribeiro");
    run::press_live(&mut v, &rx, "\n").unwrap();
    until(&mut v, &rx, "forwarded");
    run::press_live(&mut v, &rx, "i").unwrap();
    let screen = until(&mut v, &rx, "10:04:37");
    assert!(screen.contains("╭ info ") && screen.contains("10:04:37"), "jawd's answer, on the sheet:\n{screen}");

    let reacted = jawd.asked("react");
    let what: Vec<(&str, &str)> =
        reacted.iter().map(|p| (p["message"].as_str().unwrap(), p["emoji"].as_str().unwrap())).collect();
    assert_eq!(what, [(VOICE, "❤️"), (HERS, "👍")], "{reacted:?}");
    let deleted = jawd.asked("delete");
    assert_eq!((&deleted[0]["message"], &deleted[0]["for"]), (&json!(HERS), &json!("me")), "{deleted:?}");
    let forwarded = jawd.asked("forward");
    assert_eq!((&forwarded[0]["messages"], &forwarded[0]["to"]), (&json!([VOICE]), &json!([ANA])), "{forwarded:?}");
    let acts: Vec<Value> = [reacted, deleted, forwarded].concat();
    assert!(acts.iter().all(|p| p["chat"] == ANA && made(&p["key"])), "each names its chat, with a key: {acts:?}");
    let keys: std::collections::HashSet<&str> = acts.iter().filter_map(|p| p["key"].as_str()).collect();
    assert_eq!(keys.len(), 4, "a key for each action: {acts:?}");
    assert_eq!(jawd.asked("message.info"), [json!({ "message": VOICE, "chat": ANA })], "info has its chat, and no key");
    let actions = ["react", "edit", "delete", "forward", "message.info"];
    assert!(!jawd.refused().iter().any(|m| actions.contains(&m.as_str())), "{:?}", jawd.refused());
}

#[test]
fn an_action_whose_answer_the_connection_lost_is_asked_for_again_with_its_key_and_done_once() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    jawd.kept.lock().unwrap().swallow = true;
    run::press_live(&mut v, &rx, "<up>2").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("react").len() == 2), "{:?}", jawd.asked("react"));
    let screen = until(&mut v, &rx, "jaw  online");
    let reacted = jawd.asked("react");
    assert_eq!(reacted[0], reacted[1], "asked again as it was, with its key");
    assert_eq!(jawd.kept.lock().unwrap().acted.len(), 1, "done once");
    assert!(!screen.contains("asking jawd again") && !screen.contains("may not have been done"), "{screen}");
    run::press_live(&mut v, &rx, "1").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("react").len() == 3));
    assert_ne!(jawd.asked("react")[2]["key"], reacted[0]["key"], "the next action has a key of its own");
}

#[test]
fn actions_lost_longer_ago_than_jawd_keeps_keys_say_to_look_in_the_chat_and_the_rest_go_again_in_order() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    let lost = |method: &'static str, key: &str, mut params: Value, what: &'static str, ago: Duration| {
        params["chat"] = json!(ANA);
        params["key"] = json!(key);
        let first = SystemTime::now() - ago;
        Again::Act(Acting { method, chat: ANA.into(), key: key.into(), params, what, done: None, first })
    };
    let (old, recent) = (KEY_KEPT + Duration::from_secs(60), Duration::from_secs(60));
    let forward = json!({ "messages": [HERS], "to": [CLIMBING] });
    wire.again.push(lost("forward", "jaw-old", forward, "forwarding", old));
    wire.again.push(lost("edit", "jaw-first", json!({ "message": VOICE, "text": "chego às 9" }), "editing", recent));
    wire.again.push(lost("edit", "jaw-then", json!({ "message": VOICE, "text": "chego às 10" }), "editing", recent));
    jawd.tell.send(Told::Go).unwrap();
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Unreachable { .. })));
    jawd.tell.send(Told::Come).unwrap();
    let answered = |s: &[Update]| s.iter().filter(|u| matches!(u, Update::Acted { said: None, .. })).count() == 2;
    drive(&mut wire, &rx, &mut said, answered);
    let edits: Vec<Value> = jawd.asked("edit").iter().map(|p| p["text"].clone()).collect();
    assert_eq!(edits, [json!("chego às 9"), json!("chego às 10")], "asked again in the order first asked");
    assert!(jawd.asked("forward").is_empty(), "the one jawd no longer knows the key of isn't asked again");
    let back = said.iter().find_map(|u| match u {
        Update::Acted { key, said: Some(said) } if key == "jaw-old" => Some(said.as_str()),
        _ => None,
    });
    assert!(back.is_some_and(|why| why.contains("look in the chat before forwarding again")), "{said:?}");
}

#[test]
fn an_action_jawd_turns_down_says_jawds_reason_and_a_forward_says_where_it_went() {
    let jawd = scripted();
    let (tx, rx) = channel();
    let mut wire = Wire::connect(jawd.socket.clone(), tx);
    let mut said = vec![];
    drive(&mut wire, &rx, &mut said, |s| s.iter().any(|u| matches!(u, Update::Hello { .. })));
    wire.edit(ANA, VOICE, "too late");
    wire.forward(ANA, &[HERS.into()], &[ANA.into(), CLIMBING.into()]);
    drive(&mut wire, &rx, &mut said, |s| s.iter().filter(|u| matches!(u, Update::Acted { .. })).count() == 2);
    let acted: Vec<Option<&str>> = said
        .iter()
        .filter_map(|u| match u {
            Update::Acted { said, .. } => Some(said.as_deref()),
            _ => None,
        })
        .collect();
    assert_eq!(acted, [Some("a message can be edited for 15 minutes after it's sent"), Some("forwarded to 2 chats")]);
}

/// The demo engine's message in its group, from a member jawd has no name for.
const RUIS: &str = "3EB0DEMO00000002";

fn text(m: &crate::data::Message) -> String {
    m.spans.iter().map(|s| s.s.as_str()).collect()
}

/// Whether jawd's answer to the latest `find` has chat `chat` among its results.
fn found(relay: &Relay, chat: &str) -> bool {
    relay.asked("find").last().and_then(|(_, answer)| answer.clone()).is_some_and(|a| {
        a["result"]["results"]
            .as_array()
            .is_some_and(|hits| hits.iter().any(|h| h["kind"] == "chat" && h["row"]["id"] == chat))
    })
}

/// Whether jawd has answered the `n`th request for `method`.
fn answered(relay: &Relay, method: &str, n: usize) -> bool {
    relay.asked(method).get(n).is_some_and(|(_, answer)| answer.is_some())
}

#[test]
fn on_a_real_jawd_each_message_action_goes_with_its_chat_and_a_key_of_its_own_and_is_done() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<a-down>").unwrap();
    assert!(until_true(&mut v, &rx, |v| shown(v, RUIS).is_some()), "{}", frame(&mut v));

    // A reply to Rui's message, mentioning who the @ offers.
    run::press_live(&mut v, &rx, "<up>r@+351").unwrap();
    run::press_live(&mut v, &rx, "<tab>sim, às 9\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| answered(&relay, "send.text", 0)), "{:?}", relay.asked("send.text"));
    let (asked, answer) = relay.asked("send.text").remove(0);
    let answer = answer.unwrap();
    let person = asked["mentions"][0].as_str().unwrap_or_else(|| panic!("a mention: {asked}")).to_string();
    assert_eq!((&asked["chat"], &asked["reply"]), (&json!(CLIMBING), &json!(RUIS)), "{asked}");
    let members: Vec<Value> = relay
        .asked("chat.info")
        .into_iter()
        .filter_map(|(_, answer)| answer?["result"]["members"].as_array().cloned())
        .flatten()
        .collect();
    let mine = if members.iter().any(|m| m["person"] == person.as_str()) {
        let row = &answer["result"]["row"];
        assert_eq!(row["reply"]["id"], RUIS, "{answer}");
        let mention = row["spans"].as_array().unwrap().iter().any(|s| s["t"] == "mention");
        assert!(mention, "the mention is marked: {answer}");
        answer["result"]["message"].as_str().unwrap().to_string()
    } else {
        // A jawd that doesn't know who's in the group, as one whose demo engine never says, refuses
        // the mention, and the reply goes back to its box with jawd's reason.
        assert_eq!(answer["error"]["data"]["param"], "mentions", "{answer}");
        let screen = until(&mut v, &rx, "not sent");
        eprintln!("{screen}");
        assert!(screen.contains("not sent") && screen.contains(" sim, às 9"), "back in its box:\n{screen}");
        run::press_live(&mut v, &rx, "<c-u><up>rsim, às 9\n").unwrap();
        assert!(until_true(&mut v, &rx, |_| answered(&relay, "send.text", 1)), "{:?}", relay.asked("send.text"));
        let answer = relay.asked("send.text").remove(1).1.unwrap();
        assert_eq!(answer["result"]["row"]["reply"]["id"], RUIS, "{answer}");
        answer["result"]["message"].as_str().unwrap().to_string()
    };
    // Edited and deleted only once it has gone, as jawd may require of an action on yours.
    let went = |v: &mut View| {
        shown(v, &mine).is_some_and(|m| matches!(m.status, Some(Tick::Sent | Tick::Delivered | Tick::Read)))
    };
    assert!(until_true(&mut v, &rx, went), "{}", frame(&mut v));

    run::press_live(&mut v, &rx, "<up><up>2").unwrap();
    let reacted = |v: &mut View| {
        shown(v, RUIS).is_some_and(|m| m.reactions.iter().any(|r| r.mine && r.emoji.replace('\u{fe0f}', "") == "❤"))
    };
    assert!(until_true(&mut v, &rx, reacted), "{}", frame(&mut v));

    run::press_live(&mut v, &rx, "<esc><up>e<c-u>sim, às 9h30\n").unwrap();
    let edited = |v: &mut View| shown(v, &mine).is_some_and(|m| m.edited && text(&m) == "sim, às 9h30");
    assert!(until_true(&mut v, &rx, edited), "{}", frame(&mut v));

    run::press_live(&mut v, &rx, "<up>i").unwrap();
    assert!(until_true(&mut v, &rx, |_| answered(&relay, "message.info", 0)), "{:?}", relay.asked("message.info"));
    let (asked, answer) = relay.asked("message.info").remove(0);
    assert_eq!(asked, json!({ "message": mine, "chat": CLIMBING }));
    let info = &answer.unwrap()["result"];
    assert!(info["sent"].is_i64() && !info["recipients"].as_array().unwrap().is_empty(), "{info}");
    let screen = until(&mut v, &rx, "delivered");
    eprintln!("{screen}");
    assert!(screen.contains("╭ info ") && screen.contains("delivered"), "{screen}");

    // Any key closes the info and leaves your reply selected, so Rui's message is one up from it.
    run::press_live(&mut v, &rx, "<esc><up>f912345678").unwrap();
    assert!(until_true(&mut v, &rx, |_| found(&relay, ANA)), "{:?}", relay.asked("find"));
    run::settle_live(&mut v, &rx, Duration::from_secs(1));
    run::press_live(&mut v, &rx, "\n").unwrap();
    assert!(until_true(&mut v, &rx, |_| answered(&relay, "forward", 0)), "{:?}", relay.asked("forward"));
    let (asked, answer) = relay.asked("forward").remove(0);
    assert_eq!((&asked["messages"], &asked["to"]), (&json!([RUIS]), &json!([ANA])), "{asked}");
    let copy = answer.unwrap()["result"]["messages"][0].as_str().expect("the forward's message").to_string();

    run::press_live(&mut v, &rx, "<esc><up>d<down>\n").unwrap();
    let gone = |v: &mut View| shown(v, &mine).is_some_and(|m| m.deleted);
    assert!(until_true(&mut v, &rx, gone), "deleted for everyone:\n{}", frame(&mut v));
    assert!(frame(&mut v).contains("You deleted this message"));
    run::press_live(&mut v, &rx, "<up><up>d\n").unwrap();
    assert!(until_true(&mut v, &rx, |v| shown(v, RUIS).is_none()), "deleted for you:\n{}", frame(&mut v));

    let acts: Vec<(Value, Option<Value>)> =
        ["react", "edit", "forward", "delete"].into_iter().flat_map(|method| relay.asked(method)).collect();
    assert_eq!(acts.len(), 5, "{acts:?}");
    for (params, answer) in &acts {
        assert!(params["chat"] == CLIMBING && made(&params["key"]), "each names its chat, with a key: {params}");
        assert!(answer.as_ref().is_some_and(|a| a.get("result").is_some()), "{params}: {answer:?}");
    }
    let keys: std::collections::HashSet<&str> = acts.iter().filter_map(|(p, _)| p["key"].as_str()).collect();
    assert_eq!(keys.len(), acts.len(), "a key for each action: {acts:?}");
    let deleted: Vec<(&Value, &Value)> = acts[3..].iter().map(|(p, _)| (&p["message"], &p["for"])).collect();
    assert_eq!(deleted, [(&json!(mine), &json!("everyone")), (&json!(RUIS), &json!("me"))]);

    run::press_live(&mut v, &rx, "<a-up>").unwrap();
    let forwarded =
        |v: &mut View| shown(v, &copy).is_some_and(|m| m.mine && m.forwarded == 1 && text(&m) == "Saturday at 9?");
    assert!(until_true(&mut v, &rx, forwarded), "the forward, in Ana's chat:\n{}", frame(&mut v));
    eprintln!("{}", frame(&mut v));
}

#[test]
fn on_a_real_jawd_killed_between_forwarding_and_answering_jaw_asks_again_with_its_key_and_there_is_one_copy() {
    let Some(mut jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    run::press_live(&mut v, &rx, "<a-down>").unwrap();
    assert!(until_true(&mut v, &rx, |v| shown(v, RUIS).is_some()), "{}", frame(&mut v));
    run::press_live(&mut v, &rx, "<up>f912345678").unwrap();
    assert!(until_true(&mut v, &rx, |_| found(&relay, ANA)), "{:?}", relay.asked("find"));
    run::settle_live(&mut v, &rx, Duration::from_secs(1));
    relay.swallow("forward");
    run::press_live(&mut v, &rx, "\n").unwrap();
    relay.swallowed.recv_timeout(Duration::from_secs(10)).expect("jawd answered the forward");
    jawd.stop();
    relay.cut.send(()).unwrap();
    let gone = until(&mut v, &rx, "can't reach jawd");
    assert!(gone.contains("asking jawd again once it's back"), "{gone}");
    relay.restart(&mut jawd);
    assert!(until_true(&mut v, &rx, |_| answered(&relay, "forward", 1)), "{:?}", relay.asked("forward"));
    let forwards = relay.asked("forward");
    assert_eq!(forwards.len(), 2, "asked again once jawd was back: {forwards:?}");
    assert_eq!(forwards[0].0, forwards[1].0, "with the same key");
    let copies = |answer: &Option<Value>| answer.as_ref().unwrap()["result"]["messages"].clone();
    assert_eq!(copies(&forwards[0].1), copies(&forwards[1].1), "and jawd answered with the forward it had");
    let screen = until(&mut v, &rx, "forwarded");
    assert!(!screen.contains("asking jawd again"), "{screen}");

    run::press_live(&mut v, &rx, "<a-up>").unwrap();
    let mine = |v: &View| crate::view::tests::mine_saying(v, "Saturday at 9?");
    assert!(until_true(&mut v, &rx, |v| !mine(v).is_empty()), "{}", frame(&mut v));
    std::thread::sleep(Duration::from_secs(2));
    run::settle_live(&mut v, &rx, Duration::from_secs(1));
    let screen = frame(&mut v);
    eprintln!("{screen}");
    let ids: Vec<Value> = mine(&v).iter().map(|m| json!(m.id)).collect();
    assert_eq!(json!(ids), copies(&forwards[0].1), "one copy, not two:\n{screen}");
}

#[test]
fn on_a_real_jawd_that_said_linked_and_whose_engine_then_finds_no_device_jaw_offers_to_link_again() {
    let Some(mut jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    // Linked once, so jawd keeps the account; then its engine finds no device in session.db.
    jawd.stop();
    jawd.engine = "demo-unlinked";
    jawd.start();
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 100, 30);
    let screen = until(&mut v, &rx, "link again");
    eprintln!("{screen}");
    assert!(screen.contains("link again") && screen.contains("logged out"), "{screen}");
    let header = screen.lines().next().unwrap();
    assert!(header.contains("logged out: this device isn't linked any more"), "jawd's reason:\n{screen}");
    assert!(screen.contains("Logged out: this device isn't linked any more.") && !screen.contains("Your phone"));
    let passed = relay.passed.lock().unwrap().clone();
    let from_jawd: Vec<&Value> = passed.iter().filter(|p| p.from_jawd).map(|p| &p.line).collect();
    let hello = from_jawd.iter().find(|line| line["result"]["account"].is_object()).expect("jawd's hello");
    assert_eq!(hello["result"]["account"]["state"], "linked", "jaw was told the account was linked first");
    let logged_out = from_jawd
        .iter()
        .find(|line| line["method"] == "connection" && line["params"]["state"] == "logged_out")
        .expect("then that it's logged out");
    assert_eq!(logged_out["params"]["reason"], "this device isn't linked any more");
}

/// The quick look's windows go through the client beside the open chat's: one `messages.watch` of
/// the newest few for each chat that needs you, their rows to the quick look, and an `unwatch` for
/// each as it closes.
#[test]
fn the_quick_looks_windows_are_watched_beside_the_open_chats_and_let_go_as_it_closes() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    v.experiments = crate::experiments::Experiments::parse("quick_look");
    let (watched, unwatched) = (jawd.asked("messages.watch").len(), jawd.asked("unwatch").len());
    run::press_live(&mut v, &rx, "<c-g>").unwrap();
    let looked: Vec<Value> = jawd.asked("messages.watch").into_iter().skip(watched).collect();
    assert!(
        looked.len() == 1
            && looked[0]["chat"] == CLIMBING
            && looked[0]["around"] == "newest"
            && looked[0]["before"] == 3,
        "Lisbon Climbing mentions you, and Ana's is open: {looked:?}"
    );
    let screen = until(&mut v, &rx, "Ana           did you get the photos");
    assert!(screen.contains("╭ quick look") && screen.contains("Ana           did you get the photos"), "{screen}");
    run::press_live(&mut v, &rx, "<esc>").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("unwatch").len() == unwatched + 1), "{:?}", jawd.asked("unwatch"));
    assert_eq!(jawd.asked("messages.watch").len(), watched + 1, "the open chat's window stays as it was");
}

#[test]
fn on_a_real_jawd_the_quick_looks_windows_are_watched_while_it_is_open_and_jawd_lets_them_go() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let socket = jaw_proto::local::socket(jawd.home.path());
    let watched = || windows_watched(&socket);
    let relay = Relay::to(&jawd);
    let (mut v, rx) = view_on(relay.socket.clone(), 140, 30);
    v.experiments = crate::experiments::Experiments::parse("quick_look");
    assert!(until_true(&mut v, &rx, |v| frame(v).contains("Lisbon Climbing")), "{}", frame(&mut v));
    let before = watched();
    run::press_live(&mut v, &rx, "<c-g>").unwrap();
    let screen = until(&mut v, &rx, "╭ quick look");
    eprintln!("{screen}");
    let looked: Vec<Value> =
        relay.asked("messages.watch").into_iter().map(|(p, _)| p).filter(|p| p["before"] == 3).collect();
    assert!(looked.len() == 1 && looked[0]["chat"] == CLIMBING, "{looked:?}");
    assert!(screen.contains("╭ quick look") && screen.contains("Lisbon Climbing"), "{screen}");
    assert!(until_true(&mut v, &rx, |_| watched() == before + 1), "jawd watches it beside the rest: {before}");
    run::press_live(&mut v, &rx, "<esc>").unwrap();
    assert!(until_true(&mut v, &rx, |_| watched() == before), "and lets it go: {} after {before}", watched());
    assert!(!frame(&mut v).contains("╭ quick look"));
}

/// The windows watched on every connection to the jawd on `socket`, as its status says.
fn windows_watched(socket: &Path) -> u64 {
    let (mut connection, _) = jaw_client::Connection::open(socket, "jaw-test", "0").unwrap();
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", Value::Null).unwrap()).unwrap();
    status.windows.map_or(0, |w| w.watched)
}

/// A split watches the new pane's chat in a window of its own, the first pane's window kept as it
/// was, and closing the pane lets its window go.
#[test]
fn a_split_watches_the_new_panes_chat_beside_the_open_one_and_closing_it_lets_it_go() {
    let jawd = scripted();
    let (mut v, rx) = view_on(jawd.socket.clone(), 140, 30);
    v.experiments = crate::experiments::Experiments::parse("panes");
    let (watched, unwatched) = (jawd.asked("messages.watch").len(), jawd.asked("unwatch").len());
    run::press_live(&mut v, &rx, "<c-p>%").unwrap();
    let new: Vec<Value> = jawd.asked("messages.watch").into_iter().skip(watched).collect();
    assert!(new.len() == 1 && new[0]["chat"] == CLIMBING, "the next chat, in the new pane: {new:?}");
    assert_eq!(jawd.asked("unwatch").len(), unwatched, "Ana's window kept for her pane");
    let screen = until(&mut v, &rx, "╭ 2  Lisbon Climbing");
    assert!(screen.contains("╭ 1 Ana Ribeiro") && screen.contains("╭ 2  Lisbon Climbing"), "{screen}");
    run::press_live(&mut v, &rx, "<c-p>x").unwrap();
    assert!(until_true(&mut v, &rx, |_| jawd.asked("unwatch").len() == unwatched + 1), "{:?}", jawd.asked("unwatch"));
    assert_eq!(jawd.asked("messages.watch").len(), watched + 1, "Ana's window as it was, nothing watched again");
}

#[test]
fn on_a_real_jawd_each_pane_has_a_window_and_closing_one_lets_jawd_forget_it() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let socket = jaw_proto::local::socket(jawd.home.path());
    let (mut v, rx) = view_on(socket.clone(), 140, 30);
    v.experiments = crate::experiments::Experiments::parse("panes");
    assert!(until_true(&mut v, &rx, |v| frame(v).contains("Lisbon Climbing")), "{}", frame(&mut v));
    let before = windows_watched(&socket);
    run::press_live(&mut v, &rx, "<c-p>%").unwrap();
    let screen = until(&mut v, &rx, "╭ 2 ");
    eprintln!("{screen}");
    assert!(screen.contains("╭ 1 ") && screen.contains("╭ 2 ") && screen.contains("the one at the lake"), "{screen}");
    assert!(until_true(&mut v, &rx, |_| windows_watched(&socket) == before + 1), "a window for the new pane: {before}");
    run::press_live(&mut v, &rx, "<c-p>x").unwrap();
    assert!(until_true(&mut v, &rx, |_| windows_watched(&socket) == before), "and jawd lets it go");
    assert!(!frame(&mut v).contains("╭ 2 "));
}

/// The view's clock moved past the time a layout waits before it's kept, a tick at a time.
fn layout_settles(v: &mut View, rx: &Receiver<crate::run::Event>) {
    for later in [3_000, 6_000] {
        v.clock = Some(NOW + later);
        v.tick();
        run::settle_live(v, rx, Duration::from_millis(100));
    }
}

/// With panes on, jaw opens with the layout jawd keeps for it, and keeps its own once it settles:
/// a layout whose answer the connection lost is asked for again as it was, its version too.
#[test]
fn with_panes_jaw_opens_with_the_layout_jawd_keeps_and_asks_again_for_one_whose_answer_was_lost() {
    let jawd = scripted();
    let two = json!({ "format": 1, "focused": 1, "tree": { "split": {
        "how": "across", "first": 600, "a": { "chat": ANA }, "b": { "chat": CLIMBING } } } });
    {
        let mut kept = jawd.kept.lock().unwrap();
        (kept.layout, kept.layout_v) = (Some(two), 7);
    }
    let (mut v, rx) = view_with(jawd.socket.clone(), 140, 30, "panes");
    assert!(until_true(&mut v, &rx, |v| v.opened() == Some(CLIMBING)), "{}", frame(&mut v));
    let screen = frame(&mut v);
    let titles = screen.lines().nth(1).unwrap_or_default();
    assert!(titles.contains("╭ 1 Ana Ribeiro") && titles.contains("╭ 2  Lisbon Climbing"), "{screen}");
    assert_eq!(jawd.asked("layout.get").len(), 1);
    jawd.kept.lock().unwrap().swallow = true;
    run::press_live(&mut v, &rx, "<c-p>x").unwrap();
    layout_settles(&mut v, &rx);
    assert!(until_true(&mut v, &rx, |_| jawd.asked("layout.set").len() == 2), "{:?}", jawd.asked("layout.set"));
    let sets = jawd.asked("layout.set");
    assert_eq!(sets[0], sets[1], "asked again as it was");
    assert_eq!(sets[0]["layout"], json!({ "format": 1, "focused": 0, "tree": { "chat": ANA } }), "Ana's pane, alone");
    assert!(sets[0]["v"].as_u64().unwrap() > 7, "newer than the one it replaces");
    assert_eq!(jawd.kept.lock().unwrap().layout, Some(sets[0]["layout"].clone()));
    assert_eq!(jawd.asked("layout.get").len(), 1, "asked for once, at start");
}

fn layouts_kept(socket: &Path) -> u64 {
    let (mut connection, _) = jaw_client::Connection::open(socket, "jaw-test", "0").unwrap();
    let status: jaw_proto::Status = serde_json::from_value(connection.call("status", Value::Null).unwrap()).unwrap();
    status.layouts.map_or(0, |l| l.kept)
}

/// On a real jawd, panes kept by one jaw open in the next.
#[test]
fn on_a_real_jawd_jaw_keeps_its_panes_and_the_next_jaw_opens_with_them() {
    let Some(jawd) = real() else {
        eprintln!("JAWD isn't set to a jawd binary, so this ran on no real jawd");
        return;
    };
    let socket = jaw_proto::local::socket(jawd.home.path());
    let (mut v, rx) = view_with(socket.clone(), 140, 30, "panes");
    assert!(until_true(&mut v, &rx, |v| frame(v).contains("Lisbon Climbing")), "{}", frame(&mut v));
    run::press_live(&mut v, &rx, "<c-p>%").unwrap();
    until(&mut v, &rx, "╭ 2 ");
    assert_eq!(layouts_kept(&socket), 0, "not kept at the key");
    layout_settles(&mut v, &rx);
    assert!(until_true(&mut v, &rx, |_| layouts_kept(&socket) == 1), "kept once it settled");
    let keys = v.opened().map(str::to_string);
    drop(v);
    let (mut next, rx) = view_with(socket, 140, 30, "panes");
    let screen = until(&mut next, &rx, "╭ 2 ");
    assert!(screen.contains("╭ 1 ") && screen.contains("╭ 2 "), "{screen}");
    assert_eq!(next.opened().map(str::to_string), keys, "the keys where they were");
}
