//! The client against a jawd made up line by line, for what a real one won't do on cue: answer
//! out of order, stop reading, or push 2,000 times a second.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use jaw_proto::find::Find;
use jaw_proto::windows::Around;
use serde_json::{Value, json};

use super::*;
use crate::window::{Chats, Messages};

/// A socket in a folder of its own that the test answers on.
struct Fake {
    _home: tempfile::TempDir,
    socket: PathBuf,
    listener: UnixListener,
}

impl Fake {
    fn new() -> Fake {
        let home = tempfile::tempdir().unwrap();
        let socket = home.path().join("jaw.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        Fake { _home: home, socket, listener }
    }

    fn accept(&self) -> Peer {
        let (stream, _) = self.listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Peer { reader: BufReader::new(stream.try_clone().unwrap()), writer: stream }
    }

    fn config(&self) -> Config {
        Config::new(&self.socket, "test", "0")
    }
}

/// One connection from the client, as jawd sees it.
struct Peer {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Peer {
    fn request(&mut self) -> Value {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
    }

    fn send(&mut self, line: Value) {
        self.writer.write_all(format!("{line}\n").as_bytes()).unwrap();
    }

    fn answer(&mut self, request: &Value, result: Value) {
        self.send(json!({ "id": request["id"], "result": result }));
    }

    /// Reads hello and answers it.
    fn greet(&mut self) {
        let hello = self.request();
        assert_eq!(hello["method"], "hello", "hello comes first: {hello}");
        let welcome = json!({"protocol": jaw_proto::PROTOCOL, "jawd": "0", "root": "/tmp/jaw", "seq": 0, "account": {"state": "linked"}, "connection": {"state": "online"}});
        self.answer(&hello, welcome);
    }
}

fn message(n: u64) -> Value {
    json!({"id": format!("m{n:05}"), "v": n, "sort": format!("{n:013}~{n:012}"), "chat": "c", "at": n, "mine": false,
        "kind": "text", "spans": [], "reply": null, "forwarded": 0, "reactions": [], "status": null, "edited": false,
        "deleted": false, "expires_at": null, "view_once": false, "starred": false})
}

fn chat(id: &str, v: u64) -> Value {
    json!({"id": id, "v": v, "sort": format!("0~{v:013}"), "kind": "direct", "name": id, "avatar": null, "last": null,
        "unread": 0, "mentions": 0, "pinned": false, "muted_until": null, "archived": false, "marked_unread": false,
        "draft": null, "typing": [], "community": null})
}

fn watched(watch: &str, seq: u64, rows: Vec<Value>) -> Value {
    json!({"watch": watch, "seq": seq, "total": rows.len(), "older": false, "newer": 0, "rows": rows})
}

/// Drains until `found` says it has what it wants, or a few seconds pass.
fn drain_until(client: &Client, mut found: impl FnMut(Frame<ChatRow, MessageRow>) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        client.wait(Duration::from_millis(50));
        if found(client.drain()) {
            return;
        }
    }
    panic!("what the test waited for didn't come");
}

fn connected(client: &Client) {
    drain_until(client, |frame| frame.events.iter().any(|e| matches!(e, Event::Connected(_))));
}

#[test]
fn two_thousand_pushes_a_second_become_at_most_sixty_applies_a_second() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let window = client.watch_messages("c", Place::newest(100));
    let watch = peer.request();
    assert_eq!(watch["method"], "messages.watch");
    peer.answer(&watch, watched("w1", 0, vec![]));
    let pushing = std::thread::spawn(move || {
        let start = Instant::now();
        for n in 1..=2_000u64 {
            let mut push = json!({"watch": "w1", "seq": n, "upsert": [message(n)], "newer": 0});
            if n > 100 {
                push["remove"] = json!([format!("m{:05}", n - 100)]);
            }
            peer.send(json!({"method": "messages", "params": push}));
            if n % 10 == 0 {
                let due = start + Duration::from_micros(n * 500);
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
            }
        }
        (peer, start.elapsed())
    });
    let mut model: Messages<MessageRow> = Messages::new("c", Place::newest(100));
    let mut applies = Vec::new();
    let mut pushes = 0;
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until && model.newest().is_none_or(|m| m.id != "m02000") {
        if !client.wait(Duration::from_millis(100)) {
            continue;
        }
        let frame = client.drain();
        for (w, update) in frame.messages {
            assert_eq!(w, window);
            model.apply(update);
            applies.push(Instant::now());
        }
        pushes += frame.read.len();
    }
    let (_peer, took) = pushing.join().unwrap();
    assert!(took < Duration::from_millis(1_500), "the pushes took {took:?}");
    let busiest = (0..applies.len())
        .map(|i| applies[i..].iter().take_while(|&&t| t < applies[i] + Duration::from_secs(1)).count())
        .max()
        .unwrap_or(0);
    assert!(busiest <= 61, "{busiest} applies in one second, for {pushes} pushes");
    assert!(applies.len() < pushes / 10, "{} applies for {pushes} pushes", applies.len());
    let ids: Vec<&str> = model.rows().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.len(), 100);
    assert_eq!((ids[0], ids[99]), ("m01901", "m02000"), "the window ends as the last push left it");
}

/// The caller waits for what it asked for, so an answer wakes it at once, even just after a drain,
/// while a push waits for its frame and comes with the next answer if that's sooner.
#[test]
fn an_answer_wakes_the_caller_at_once_and_a_push_waits_for_its_frame() {
    let fake = Fake::new();
    let client: Client = Client::start(Config { frame: Duration::from_secs(10), ..fake.config() }, || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let window = client.watch_messages("c", Place::newest(3));
    let watch = peer.request();
    peer.answer(&watch, watched("w1", 3, (1..=3).map(message).collect()));
    assert!(client.wait(Duration::from_secs(5)), "a watch's answer wakes it at once, a drain ago or not");
    let frame = client.drain();
    assert_eq!(frame.messages.iter().map(|(w, _)| *w).collect::<Vec<_>>(), [window]);
    let push = json!({"watch": "w1", "seq": 4, "upsert": [message(4)], "remove": ["m00001"]});
    peer.send(json!({"method": "messages", "params": push}));
    assert!(!client.wait(Duration::from_millis(300)), "a push waits for its frame");
    let id = client.request("status", Value::Null).unwrap();
    let asked = peer.request();
    peer.answer(&asked, json!({}));
    assert!(client.wait(Duration::from_secs(5)), "a request's answer wakes it at once");
    let frame = client.drain();
    assert!(frame.events.iter().any(|e| matches!(e, Event::Reply { id: replied, .. } if *replied == id)));
    assert_eq!(frame.messages.len(), 1, "the push comes with it");
}

#[test]
fn only_the_latest_finds_answer_comes() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let typed: Vec<u64> = ["a", "an", "ana"]
        .iter()
        .map(|q| client.find(Find { query: q.to_string(), ..Find::default() }).unwrap())
        .collect();
    let finds: Vec<Value> = (0..3).map(|_| peer.request()).collect();
    assert_eq!(finds.iter().map(|f| f["params"]["query"].as_str().unwrap()).collect::<Vec<_>>(), ["a", "an", "ana"]);
    peer.answer(&finds[2], json!({"results": [], "counts": {"chat": 1}}));
    peer.send(json!({"id": finds[1]["id"], "error": {"code": 7, "message": "a newer find replaced this one"}}));
    peer.answer(&finds[0], json!({"results": [], "counts": {"chat": 9}}));
    let status = client.request("status", Value::Null).unwrap();
    let asked = peer.request();
    peer.answer(&asked, json!({}));
    let mut found = Vec::new();
    drain_until(&client, |frame| {
        for event in frame.events {
            match event {
                Event::Found { id, query, outcome } => found.push((id, query, outcome.map(|f| f.counts))),
                Event::Reply { id, .. } if id == status => return true,
                _ => {}
            }
        }
        false
    });
    assert_eq!(found.len(), 1, "{found:?}");
    let (id, query, counts) = &found[0];
    assert_eq!((*id, query.as_str()), (typed[2], "ana"));
    assert_eq!(counts.as_ref().unwrap()[&jaw_proto::find::Kind::Chat], 1);
}

#[test]
fn requests_are_refused_in_words_when_jawd_stops_reading() {
    let fake = Fake::new();
    let mut config = fake.config();
    config.queue = 4;
    let client: Client = Client::start(config, || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let big = "x".repeat(64 << 10);
    let refused = (0..2_000).find_map(|_| client.request("draft.set", json!({"chat": "c", "text": big})).err());
    assert_eq!(refused, Some(Failure::Full(4)));
    assert_eq!(refused.unwrap().to_string(), "jawd has stopped reading what jaw sends: 4 requests are waiting for it");
    drop(peer);
}

#[test]
fn a_request_in_flight_when_jawd_goes_fails_and_says_so() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let id = client.request("status", Value::Null).unwrap();
    assert_eq!(peer.request()["id"], id);
    drop(peer);
    let mut events = Vec::new();
    drain_until(&client, |frame| {
        events.extend(frame.events);
        events.iter().any(|e| matches!(e, Event::Disconnected { .. }))
    });
    assert_eq!(events[0], Event::Reply { id, outcome: Err(Failure::Lost) });
    assert!(
        matches!(&events[1], Event::Disconnected { why, retry_in } if why == "jawd closed the connection" && *retry_in == FIRST_RETRY)
    );
    assert_eq!(client.request("status", Value::Null), Err(Failure::Unreachable));
    assert!(Failure::Lost.to_string().contains("may not have been done"));
}

#[test]
fn a_jawd_not_started_yet_is_tried_again_less_often_until_it_answers() {
    let home = tempfile::tempdir().unwrap();
    let socket = home.path().join("jaw.sock");
    let client: Client = Client::start(Config::new(&socket, "test", "0"), || {});
    let mut waits = Vec::new();
    drain_until(&client, |frame| {
        for event in frame.events {
            if let Event::Disconnected { why, retry_in } = event {
                assert!(why.starts_with("jawd isn't running: nothing answers on"), "{why}");
                waits.push(retry_in.as_millis());
            }
        }
        waits.len() >= 4
    });
    assert_eq!(waits[..4], [100, 200, 400, 800]);
    assert_eq!(client.request("status", Value::Null), Err(Failure::Unreachable));
    let listener = UnixListener::bind(&socket).unwrap();
    let fake = Fake { _home: home, socket, listener };
    fake.accept().greet();
    connected(&client);
    assert_eq!(client.stats().connects, 1);
}

#[test]
fn moves_asked_while_one_is_in_flight_send_only_the_latest() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let window = client.watch_messages("c", Place::newest(3));
    let watch = peer.request();
    peer.answer(&watch, watched("w1", 3, (1..=3).map(message).collect()));
    client.move_window(window, Place::around("m00002", 1, 1));
    let first = peer.request();
    assert_eq!((first["method"].as_str(), first["params"]["around"].as_str()), (Some("watch.move"), Some("m00002")));
    client.move_window(window, Place::around("m00001", 1, 1));
    client.move_window(window, Place::around("m00003", 1, 1));
    let moved =
        json!({"watch": "w1", "seq": 3, "total": 3, "older": true, "newer": 1, "rows": [], "remove": ["m00002"]});
    peer.answer(&first, moved.clone());
    let latest = peer.request();
    assert_eq!(latest["params"]["around"], "m00003", "the move in between was never sent");
    peer.answer(
        &latest,
        json!({"watch": "w1", "seq": 3, "total": 3, "older": true, "newer": 0, "rows": [], "remove": []}),
    );
    let mut model: Messages<MessageRow> = Messages::new("c", Place::newest(3));
    drain_until(&client, |frame| {
        for (_, update) in frame.messages {
            model.apply(update);
        }
        model.place.around == Around::Row("m00003".into())
    });
    assert_eq!(model.rows().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["m00001", "m00003"]);
    assert_eq!((model.older, model.newer, model.arrived()), (true, 0, 0));
}

#[test]
fn reconnecting_watches_again_with_the_rows_held_and_ends_as_a_fresh_watch() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let list = client.watch_chats("all", Place::top(10));
    let open = client.watch_messages("c", Place::newest(10));
    let (a, b) = (peer.request(), peer.request());
    peer.answer(&a, json!({"watch": "w1", "seq": 5, "total": 2, "rows": [chat("ana", 4), chat("rui", 5)]}));
    peer.answer(&b, watched("w2", 5, (1..=3).map(message).collect()));
    peer.send(json!({"method": "messages", "params": {"watch": "w2", "seq": 7, "upsert": [message(7)], "remove": ["m00001"]}}));
    let mut chats: Chats<ChatRow> = Chats::new("all", Place::top(10));
    let mut messages: Messages<MessageRow> = Messages::new("c", Place::newest(10));
    let apply = |frame: Frame<ChatRow, MessageRow>, chats: &mut Chats<ChatRow>, messages: &mut Messages<MessageRow>| {
        for (w, update) in frame.chats {
            assert_eq!(w, list);
            chats.apply(update);
        }
        for (w, update) in frame.messages {
            assert_eq!(w, open);
            messages.apply(update);
        }
    };
    drain_until(&client, |frame| {
        apply(frame, &mut chats, &mut messages);
        messages.seq == 7
    });
    drop(peer);
    let mut peer = fake.accept();
    peer.greet();
    let (a, b) = (peer.request(), peer.request());
    assert_eq!(a["method"], "chats.watch");
    assert_eq!(a["params"]["have"], json!(["ana", "rui"]));
    assert_eq!(a["params"].get("since"), None, "a chat list comes whole");
    assert_eq!(b["method"], "messages.watch");
    assert_eq!(b["params"]["have"], json!(["m00002", "m00003", "m00007"]));
    assert_eq!(b["params"]["since"], 7);
    peer.answer(&a, json!({"watch": "w9", "seq": 9, "total": 1, "rows": [chat("rui", 8)], "remove": ["ana"]}));
    peer.answer(&b, json!({"watch": "w10", "seq": 9, "total": 3, "older": false, "newer": 0, "rows": [message(8)], "remove": ["m00002"]}));
    peer.send(json!({"method": "resync", "params": {"watch": "w10"}}));
    let again = peer.request();
    assert_eq!(again["params"]["have"], json!(["m00003", "m00007", "m00008"]), "a resync watches again too");
    assert_eq!(again["params"]["since"], 9);
    peer.answer(
        &again,
        json!({"watch": "w11", "seq": 10, "total": 3, "older": false, "newer": 0, "rows": [], "remove": []}),
    );
    drain_until(&client, |frame| {
        apply(frame, &mut chats, &mut messages);
        messages.seq == 10
    });
    assert_eq!(chats.rows().map(|c| (c.id.as_str(), c.v)).collect::<Vec<_>>(), [("rui", 8)]);
    assert_eq!(messages.rows().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["m00003", "m00007", "m00008"]);
    assert_eq!((chats.total, messages.total), (1, 3));
    assert_eq!(client.stats().resyncs, 1);
    assert!(messages.place.around == Around::Newest && messages.following());
}

/// Who's typing, as the client keeps it: each answer says it for every chat of its window, the one
/// both a chat list and a conversation show included; a push changes it for a chat a window shows
/// and nothing for one none does; a chat is forgotten as it leaves every window; and all of it once
/// the connection goes.
#[test]
fn typing_is_kept_for_the_chats_the_windows_show_as_answers_and_pushes_say_and_forgotten_with_the_connection() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let who = |name: &str, state: &str| json!([{"id": format!("person:{}", name.len()), "name": name, "state": state}]);
    let typing = |frame: &Frame<ChatRow, MessageRow>| -> Option<Vec<(String, String)>> {
        frame.typing.as_ref().map(|t| t.iter().map(|(chat, who)| (chat.clone(), who[0].name.clone())).collect())
    };
    let held = |pairs: &[(&str, &str)]| -> Option<Vec<(String, String)>> {
        Some(pairs.iter().map(|(chat, name)| (chat.to_string(), name.to_string())).collect())
    };

    client.watch_chats("all", Place::top(10));
    let watch = peer.request();
    let mut list = watched("w1", 1, vec![chat("a", 1), chat("b", 1)]);
    list["typing"] = json!({"a": who("Ana", "typing"), "b": who("Rui", "recording")});
    peer.answer(&watch, list);
    let mut last = None;
    drain_until(&client, |frame| {
        last = typing(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("a", "Ana"), ("b", "Rui")]));

    // The conversation of a chat the list shows answers with its typing, which stays.
    let conversation = client.watch_messages("a", Place::newest(10));
    let watch = peer.request();
    let mut messages = watched("w2", 1, vec![message(1)]);
    messages["typing"] = json!({"a": who("Ana", "typing")});
    peer.answer(&watch, messages);
    drain_until(&client, |frame| !frame.messages.is_empty());
    let status = client.request("status", Value::Null).unwrap();
    let asked = peer.request();
    peer.answer(&asked, json!({}));
    drain_until(&client, |frame| {
        assert_eq!(frame.typing, None, "what the conversation's answer said changed nothing");
        frame.events.iter().any(|e| matches!(e, Event::Reply { id, .. } if *id == status))
    });

    // A push ends Rui's; one for a chat no window shows is let go of.
    peer.send(json!({"method": "typing", "params": {"chat": "b", "who": []}}));
    peer.send(json!({"method": "typing", "params": {"chat": "z", "who": who("Zé", "typing")}}));
    drain_until(&client, |frame| {
        last = typing(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("a", "Ana")]));

    // The list lets go of Ana's chat, which the conversation still shows; then it's unwatched.
    let push = json!({"watch": "w1", "seq": 2, "remove": ["a"]});
    peer.send(json!({"method": "chats", "params": push}));
    drain_until(&client, |frame| !frame.chats.is_empty());
    client.unwatch(conversation);
    drain_until(&client, |frame| {
        last = typing(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[]), "forgotten once no window shows it");

    // Gone with the connection.
    peer.send(json!({"method": "typing", "params": {"chat": "b", "who": who("Rui", "typing")}}));
    drain_until(&client, |frame| {
        last = typing(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("b", "Rui")]));
    drop(peer);
    drain_until(&client, |frame| {
        last = typing(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[]), "nothing kept once the connection ends");
}

/// A one-to-one chat's person's presence, as the client keeps it: each conversation's answer says
/// it for its chat, a push changes it for a chat a conversation shows and nothing for one none
/// does, a move's answer says it again, and it's forgotten as the conversation is unwatched and when
/// the connection goes.
#[test]
fn presence_is_kept_for_the_conversations_shown_as_answers_and_pushes_say_and_forgotten_with_them() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let states = |frame: &Frame<ChatRow, MessageRow>| -> Option<Vec<(String, String, bool)>> {
        let presence = frame.presence.as_ref()?;
        Some(presence.iter().map(|(chat, p)| (chat.clone(), p.presence.state.clone(), p.presence.known)).collect())
    };
    let held = |states: &[(&str, &str, bool)]| -> Option<Vec<(String, String, bool)>> {
        Some(states.iter().map(|(chat, state, known)| (chat.to_string(), state.to_string(), *known)).collect())
    };

    let conversation = client.watch_messages("a", Place::newest(10));
    let watch = peer.request();
    let mut messages = watched("w1", 1, vec![message(1)]);
    messages["presence"] = json!({"person": "person:3", "state": "online", "known": true});
    peer.answer(&watch, messages);
    let mut last = None;
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("a", "online", true)]));

    // A push changes it; one for a chat no conversation shows is let go of.
    let seen = json!({"chat": "a", "person": "person:3", "state": "seen", "at": 1792000000000i64, "known": true});
    peer.send(json!({"method": "presence", "params": seen}));
    let elsewhere = json!({"chat": "z", "person": "person:9", "state": "online", "known": true});
    peer.send(json!({"method": "presence", "params": elsewhere}));
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("a", "seen", true)]));
    let at = client.drain();
    assert_eq!(at.presence, None, "nothing more came");

    // Moved, the conversation's answer says it again, faint now.
    client.move_window(conversation, Place::newest(20));
    let moving = peer.request();
    assert_eq!(moving["method"], "watch.move");
    let mut moved = watched("w1", 2, vec![message(1)]);
    moved["presence"] = json!({"person": "person:3", "state": "seen", "at": 1792000000000i64, "known": false});
    peer.answer(&moving, moved);
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("a", "seen", false)]));

    // Unwatched, it's forgotten; and another conversation's goes with the connection.
    client.unwatch(conversation);
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[]), "forgotten once its conversation is unwatched");
    let _ = peer.request();
    client.watch_messages("b", Place::newest(10));
    let watch = peer.request();
    let mut messages = watched("w2", 3, vec![message(1)]);
    messages["presence"] = json!({"person": "person:4", "state": "hidden", "known": true});
    peer.answer(&watch, messages);
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[("b", "hidden", true)]));
    drop(peer);
    drain_until(&client, |frame| {
        last = states(&frame);
        last.is_some()
    });
    assert_eq!(last, held(&[]), "nothing kept once the connection ends");
}

/// A roster watched through `watch_roster` shows its people's one-to-one chats for typing: its
/// answer says who's typing in them, a push keeps typing for a chat whose person a roster push just
/// brought in, and a chat is forgotten as its person leaves the roster; its answer and pushes still
/// reach the caller.
#[test]
fn a_rosters_people_own_chats_are_shown_for_typing_from_its_answer_and_its_pushes_as_they_come() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let who = |name: &str| json!([{"id": "person:1", "name": name, "state": "typing"}]);
    let typing = |frame: &Frame<ChatRow, MessageRow>| -> Option<Vec<String>> {
        frame.typing.as_ref().map(|t| t.keys().cloned().collect())
    };
    let person = |n: u64, chat: Option<&str>| {
        json!({"id": format!("person:{n}"), "v": n, "sort": format!("2~{n}"), "name": format!("P{n}"), "phone": null,
            "about": null, "avatar": null, "presence": {"state": "hidden", "known": false}, "chat": chat, "folders": []})
    };

    let asked = client.watch_roster(json!({"folder": "all", "around": "top", "after": 10})).unwrap();
    let watch = peer.request();
    assert_eq!(watch["method"], "roster.watch");
    let mut roster = json!({"watch": "w1", "seq": 1, "total": 2, "rows": [person(1, Some("a")), person(2, None)]});
    roster["typing"] = json!({"a": who("Ana")});
    peer.answer(&watch, roster);
    let mut last = None;
    let mut answered = false;
    drain_until(&client, |frame| {
        answered |= frame.events.iter().any(|e| matches!(e, Event::Reply { id, .. } if *id == asked));
        last = typing(&frame).or(last.take());
        answered && last.is_some()
    });
    assert_eq!(last, Some(vec!["a".to_string()]), "the answer's typing, for the chat its person has");

    // Someone comes into the roster, and a typing push for their chat follows at once.
    let came = json!({"watch": "w1", "seq": 2, "upsert": [person(3, Some("c"))]});
    peer.send(json!({"method": "roster", "params": came}));
    peer.send(json!({"method": "typing", "params": {"chat": "c", "who": who("Cora")}}));
    let mut pushed = false;
    drain_until(&client, |frame| {
        pushed |= frame.events.iter().any(|e| matches!(e, Event::Push(push) if push.method == "roster"));
        last = typing(&frame).or(last.take());
        pushed && last.as_ref().is_some_and(|chats| chats.len() == 2)
    });
    assert_eq!(last, Some(vec!["a".to_string(), "c".to_string()]));

    // Ana leaves the roster, so her chat's typing is forgotten.
    let left = json!({"watch": "w1", "seq": 3, "remove": ["person:1"]});
    peer.send(json!({"method": "roster", "params": left}));
    drain_until(&client, |frame| {
        last = typing(&frame).or(last.take());
        last.as_ref().is_some_and(|chats| chats.len() == 1)
    });
    assert_eq!(last, Some(vec!["c".to_string()]));
}

#[test]
fn a_chat_lists_open_goes_with_its_watch_each_move_and_a_reconnect_and_a_move_without_it_keeps_it() {
    let fake = Fake::new();
    let client: Client = Client::start(fake.config(), || {});
    let mut peer = fake.accept();
    peer.greet();
    connected(&client);
    let open = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    let list = client.watch_chats("all", Place::top(10).open(open(&["vela", "bairro", "vela"])));
    let watch = peer.request();
    assert_eq!((watch["method"].as_str(), &watch["params"]["open"]), (Some("chats.watch"), &json!(["bairro", "vela"])));
    peer.answer(&watch, json!({"watch": "w1", "seq": 5, "total": 1, "rows": [chat("ana", 4)]}));
    let chat_window = client.watch_messages("c", Place::newest(3));
    let messages = peer.request();
    peer.answer(&messages, watched("w2", 5, (1..=3).map(message).collect()));
    client.move_window(chat_window, Place::around("m00002", 1, 1));
    let moved = peer.request();
    assert_eq!(moved["params"].get("open"), None, "a chat's window has none: {moved}");
    let answer = json!({"watch": "w2", "seq": 5, "total": 3, "older": true, "newer": 1, "rows": [], "remove": []});
    peer.answer(&moved, answer);

    let moves = [
        (Place::top(10).open(open(&["bairro"])), json!(["bairro"])),
        (Place::top(10).open(Vec::new()), json!([])),
        (Place::top(20), json!([])),
    ];
    for (place, sent) in moves {
        let asked = place.open.clone();
        client.move_window(list, place);
        let moving = peer.request();
        assert_eq!(moving["method"], "watch.move");
        assert_eq!(moving["params"]["open"], sent, "a move asking {asked:?}");
        peer.answer(&moving, json!({"watch": "w1", "seq": 5, "total": 1, "rows": [], "remove": []}));
    }
    drop(peer);
    let mut peer = fake.accept();
    peer.greet();
    let watches = [peer.request(), peer.request()];
    let again = watches.iter().find(|w| w["method"] == "chats.watch").expect("the chat list watched again");
    assert_eq!(again["params"]["open"], json!([]), "watched again with the communities it had open: {again}");
}
