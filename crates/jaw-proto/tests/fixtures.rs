//! Every file in protocol/fixtures/ reads into its type and writes back the same, so the wire
//! format only changes on purpose. A file is named `<method>.request.json`, `<method>.result.json`,
//! `<method>.push.json` or `error.<what>.json`; one this test has no type for fails it.

use std::path::{Path, PathBuf};

use jaw_proto::find::{ChatOpened, Find, Found};
use jaw_proto::send::{
    ChatInfo, ChatInfoResult, ChatStart, ChatStarted, Delete, DraftSet, Edit, Forward, Forwarded, Info, MessageInfo,
    React, Read, SendRetry, SendText, Sent, Typing,
};
use jaw_proto::sync::{HistoryMore, Notice};
use jaw_proto::windows::{
    ChatsPush, ChatsWatch, ChatsWatched, MessagesPush, MessagesWatch, MessagesWatched, Watch, WatchMove,
};
use jaw_proto::{Connection, Focus, FromJawd, Hello, Outcome, Push, Reply, Request, Status, Welcome};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn fixtures() -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures");
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("can't read {}: {e}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

/// `value` read as `T` and written back, which must give `value` again.
fn same_as<T: DeserializeOwned + Serialize>(name: &str, value: &Value) -> T {
    let typed: T = serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{name} doesn't read: {e}"));
    let back = serde_json::to_value(&typed).unwrap();
    assert_eq!(&back, value, "{name} doesn't write back the same");
    typed
}

fn params<T: DeserializeOwned + Serialize>(name: &str, value: &Value) {
    same_as::<T>(name, value);
}

#[test]
fn every_fixture_reads_and_writes_back_the_same() {
    let files = fixtures();
    assert!(!files.is_empty(), "protocol/fixtures/ has no fixtures");
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{name} isn't JSON: {e}"));
        let parts: Vec<&str> = name.trim_end_matches(".json").rsplitn(2, '.').collect();
        let (kind, method) = match parts.as_slice() {
            [kind, method] => (*kind, *method),
            _ => panic!("{name} isn't named <method>.<kind>.json"),
        };
        if method == "error" {
            let reply: Reply = same_as(&name, &value);
            assert!(matches!(reply.outcome, Outcome::Error(_)), "{name} isn't an error");
            from_jawd(&name, value, false);
            continue;
        }
        match kind {
            "request" => {
                let request: Request = same_as(&name, &value);
                assert_eq!(request.method, method, "{name} is for {}", request.method);
                match method {
                    "hello" => params::<Hello>(&name, &request.params),
                    "status" => assert!(request.params.is_null(), "{name} has params"),
                    "find" => params::<Find>(&name, &request.params),
                    "chat.opened" => params::<ChatOpened>(&name, &request.params),
                    "chats.watch" => params::<ChatsWatch>(&name, &request.params),
                    "messages.watch" => params::<MessagesWatch>(&name, &request.params),
                    "watch.move" => params::<WatchMove>(&name, &request.params),
                    "unwatch" => params::<Watch>(&name, &request.params),
                    "history.more" => params::<HistoryMore>(&name, &request.params),
                    "focus" => params::<Focus>(&name, &request.params),
                    "send.text" => params::<SendText>(&name, &request.params),
                    "send.retry" => params::<SendRetry>(&name, &request.params),
                    "chat.start" => params::<ChatStart>(&name, &request.params),
                    "draft.set" => params::<DraftSet>(&name, &request.params),
                    "read" => params::<Read>(&name, &request.params),
                    "typing" => params::<Typing>(&name, &request.params),
                    "react" => params::<React>(&name, &request.params),
                    "edit" => params::<Edit>(&name, &request.params),
                    "delete" => params::<Delete>(&name, &request.params),
                    "forward" => params::<Forward>(&name, &request.params),
                    "message.info" => params::<MessageInfo>(&name, &request.params),
                    "chat.info" => params::<ChatInfo>(&name, &request.params),
                    _ => panic!("no type for {name}'s params"),
                }
            }
            "result" => {
                let reply: Reply = same_as(&name, &value);
                let Outcome::Result(result) = &reply.outcome else { panic!("{name} isn't a result") };
                match method {
                    "hello" => params::<Welcome>(&name, result),
                    "status" => params::<Status>(&name, result),
                    "find" => params::<Found>(&name, result),
                    "chat.opened" | "unwatch" | "history.more" | "focus" | "send.retry" | "draft.set" | "read"
                    | "typing" | "react" | "edit" | "delete" => {
                        assert_eq!(result, &serde_json::json!({}), "{name} has a result")
                    }
                    "send.text" => params::<Sent>(&name, result),
                    "chat.start" => params::<ChatStarted>(&name, result),
                    "forward" => params::<Forwarded>(&name, result),
                    "message.info" => params::<Info>(&name, result),
                    "chat.info" => params::<ChatInfoResult>(&name, result),
                    "chats.watch" => params::<ChatsWatched>(&name, result),
                    // A move's result is its window's; this fixture moves a messages window.
                    "messages.watch" | "watch.move" => params::<MessagesWatched>(&name, result),
                    _ => panic!("no type for {name}'s result"),
                }
            }
            "push" => {
                let push: Push = same_as(&name, &value);
                assert_eq!(push.method, method, "{name} is for {}", push.method);
                match method {
                    "connection" => params::<Connection>(&name, &push.params),
                    "chats" => params::<ChatsPush>(&name, &push.params),
                    "messages" => params::<MessagesPush>(&name, &push.params),
                    "resync" => params::<Watch>(&name, &push.params),
                    "notice" => params::<Notice>(&name, &push.params),
                    _ => panic!("no type for {name}'s params"),
                }
            }
            _ => panic!("{name}: a fixture is a request, a result, a push or an error"),
        }
        if kind != "request" {
            from_jawd(&name, value, kind == "push");
        }
    }
}

/// A line from jawd tells a reply from a push by its `id`.
fn from_jawd(name: &str, value: Value, push: bool) {
    let line: FromJawd = serde_json::from_value(value).unwrap_or_else(|e| panic!("{name} isn't a line from jawd: {e}"));
    assert_eq!(matches!(line, FromJawd::Push(_)), push, "{name} reads as the wrong kind of line from jawd");
}
