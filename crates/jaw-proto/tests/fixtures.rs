//! Every file in protocol/fixtures/ reads into its type and writes back the same, so the wire
//! format only changes on purpose. A file is named `<method>.request.json`, `<method>.result.json`,
//! `<method>.push.json` or `error.<what>.json`, or with the name of another example after its kind,
//! as `layout.set.request.book.json`; one this test has no type for fails it.

use std::path::{Path, PathBuf};

use jaw_proto::find::{ChatOpened, Find, Found, Kind};
use jaw_proto::layout::{Book, LayoutGet, LayoutSet, Saved};
use jaw_proto::library::{LibraryPush, LibraryShown, LibraryWatch, LibraryWatched};
use jaw_proto::media::{Job, MediaCancel, MediaFetch, MediaSave, Saved as MediaSaved, SendMedia, SentMedia};
use jaw_proto::send::{
    ChatInfo, ChatInfoResult, ChatStart, ChatStarted, Delete, DraftSet, Edit, Forward, Forwarded, Info, MessageInfo,
    PollVote, React, Read, SendContact, SendEvent, SendLocation, SendPoll, SendRetry, SendText, Sent, Typing,
};
use jaw_proto::settings::{Messages, Settings, SettingsKept, SettingsSet};
use jaw_proto::sync::{HistoryMore, Notice};
use jaw_proto::themes::Themes;
use jaw_proto::windows::{
    ChatRow, ChatsPush, ChatsWatch, ChatsWatched, FolderRow, FoldersPush, FoldersWatch, FoldersWatched, MessageRow,
    MessagesPush, MessagesWatch, MessagesWatched, PresencePush, RosterRow, RosterWatch, TypingPush, Watch, WatchMove,
};
use jaw_proto::{Connection, Focus, FromJawd, Hello, Outcome, Push, Reply, Request, Status, Welcome};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// protocol/fixtures/, where every fixture is.
const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../protocol/fixtures");

fn fixtures() -> Vec<PathBuf> {
    let folder = Path::new(FOLDER);
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|e| panic!("can't read {}: {e}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

/// The fixture `name`, such as `status.result`, as JSON.
fn fixture(name: &str) -> Value {
    let path = Path::new(FOLDER).join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("can't read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} isn't JSON: {e}"))
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
        let stem = name.trim_end_matches(".json");
        let (stem, example) = match stem.rsplit_once('.') {
            Some((rest, example)) if [".request", ".result", ".push"].iter().any(|kind| rest.ends_with(kind)) => {
                (rest, Some(example))
            }
            _ => (stem, None),
        };
        let book = example == Some("book");
        // A chat list holding a community for its groups reads as any chat list.
        let community = example == Some("community")
            && matches!(stem, "chats.watch.request" | "chats.watch.result" | "chats.push" | "watch.move.request");
        assert!(example.is_none() || book || community, "{name}: this test has no type for the example {example:?}");
        let parts: Vec<&str> = stem.rsplitn(2, '.').collect();
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
                    "status" | "themes" => assert!(request.params.is_null(), "{name} has params"),
                    "settings.set" => params::<SettingsSet>(&name, &request.params),
                    "find" => params::<Find>(&name, &request.params),
                    "chat.opened" => params::<ChatOpened>(&name, &request.params),
                    "chats.watch" => params::<ChatsWatch>(&name, &request.params),
                    "messages.watch" => params::<MessagesWatch>(&name, &request.params),
                    "library.watch" => params::<LibraryWatch>(&name, &request.params),
                    "library.shown" => params::<LibraryShown>(&name, &request.params),
                    "watch.move" => params::<WatchMove>(&name, &request.params),
                    "unwatch" => params::<Watch>(&name, &request.params),
                    "history.more" => params::<HistoryMore>(&name, &request.params),
                    "focus" => params::<Focus>(&name, &request.params),
                    "send.text" => params::<SendText>(&name, &request.params),
                    "send.poll" => params::<SendPoll>(&name, &request.params),
                    "send.location" => params::<SendLocation>(&name, &request.params),
                    "send.event" => params::<SendEvent>(&name, &request.params),
                    "send.contact" => params::<SendContact>(&name, &request.params),
                    "poll.vote" => params::<PollVote>(&name, &request.params),
                    "send.retry" => params::<SendRetry>(&name, &request.params),
                    "send.media" => params::<SendMedia>(&name, &request.params),
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
                    "layout.get" => params::<LayoutGet>(&name, &request.params),
                    "layout.set" if book => params::<LayoutSet<Book>>(&name, &request.params),
                    "layout.set" => params::<LayoutSet>(&name, &request.params),
                    "folders.watch" => params::<FoldersWatch>(&name, &request.params),
                    "roster.watch" => params::<RosterWatch>(&name, &request.params),
                    "folder.create" | "folder.add" | "favourite.set" => {
                        assert!(request.params.is_object(), "{name}'s params")
                    }
                    "media.fetch" => params::<MediaFetch>(&name, &request.params),
                    "media.save" => params::<MediaSave>(&name, &request.params),
                    "media.cancel" => params::<MediaCancel>(&name, &request.params),
                    _ => panic!("no type for {name}'s params"),
                }
            }
            "result" => {
                let reply: Reply = same_as(&name, &value);
                let Outcome::Result(result) = &reply.outcome else { panic!("{name} isn't a result") };
                match method {
                    "hello" => params::<Welcome>(&name, result),
                    "status" => params::<Status>(&name, result),
                    "find" => {
                        // A chat's or a message's row is as its window sends it.
                        let found: Found = same_as(&name, result);
                        for hit in &found.results {
                            match hit.kind {
                                Kind::Chat => params::<ChatRow>(&name, &hit.row),
                                Kind::Message => params::<MessageRow>(&name, &hit.row),
                                _ => {}
                            }
                        }
                    }
                    "chat.opened" | "unwatch" | "history.more" | "focus" | "send.retry" | "draft.set" | "read"
                    | "typing" | "react" | "edit" | "delete" | "poll.vote" | "folder.add" | "favourite.set"
                    | "media.fetch" | "media.cancel" | "library.shown" => {
                        assert_eq!(result, &serde_json::json!({}), "{name} has a result")
                    }
                    "media.save" => params::<MediaSaved>(&name, result),
                    "send.text" | "send.poll" | "send.location" | "send.event" | "send.contact" => {
                        params::<Sent>(&name, result)
                    }
                    "send.media" => params::<SentMedia>(&name, result),
                    "chat.start" => params::<ChatStarted>(&name, result),
                    "forward" => params::<Forwarded>(&name, result),
                    "message.info" => params::<Info>(&name, result),
                    "chat.info" => params::<ChatInfoResult>(&name, result),
                    "layout.get" | "layout.set" if book => params::<Saved<Book>>(&name, result),
                    "layout.get" | "layout.set" => params::<Saved>(&name, result),
                    "chats.watch" => params::<ChatsWatched>(&name, result),
                    "folders.watch" => params::<FoldersWatched>(&name, result),
                    "roster.watch" => params::<ChatsWatched<RosterRow>>(&name, result),
                    "folder.create" => params::<FolderRow>(&name, &result["folder"]),
                    "library.watch" => params::<LibraryWatched>(&name, result),
                    // A move's result is its window's; this fixture moves a messages window.
                    "messages.watch" | "watch.move" => params::<MessagesWatched>(&name, result),
                    "settings.set" => params::<SettingsKept>(&name, result),
                    "themes" => params::<Themes>(&name, result),
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
                    "folders" => params::<FoldersPush>(&name, &push.params),
                    "roster" => params::<ChatsPush<RosterRow>>(&name, &push.params),
                    "library" => params::<LibraryPush>(&name, &push.params),
                    "resync" => params::<Watch>(&name, &push.params),
                    "notice" => params::<Notice>(&name, &push.params),
                    "layout" if book => params::<Saved<Book>>(&name, &push.params),
                    "layout" => params::<Saved>(&name, &push.params),
                    "settings" => params::<Settings>(&name, &push.params),
                    "typing" => params::<TypingPush>(&name, &push.params),
                    "presence" => params::<PresencePush>(&name, &push.params),
                    "job" => params::<Job>(&name, &push.params),
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

/// The themes fixture is the first two themes exactly as jaw-proto reads them from protocol/themes.
#[test]
fn the_themes_fixture_is_what_the_theme_files_say() {
    let themes: Themes = serde_json::from_value(fixture("themes.result")["result"].clone()).unwrap();
    assert_eq!(themes.themes, jaw_proto::themes::all()[..2]);
}

/// Hello's answer carries the terminal's settings, and status every domain's by its name.
#[test]
fn hello_carries_its_domains_settings_and_status_every_domains() {
    let hello: Welcome = serde_json::from_value(fixture("hello.result")["result"].clone()).unwrap();
    assert_eq!(hello.settings, Some(Settings::default()));
    let status: Status = serde_json::from_value(fixture("status.result")["result"].clone()).unwrap();
    let jaw: Settings = same_as("status.result's jaw domain", &Value::Object(status.settings["jaw"].clone()));
    assert_eq!(jaw, Settings { theme: "tokyo-night".into(), messages: Messages::Irc });
    assert_eq!(status.settings_ignored[0].domain, "jaw");
}
