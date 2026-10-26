use serde_json::{Value, json};

use super::*;
use crate::feed::tests::{Bed, Model, brisk};
use crate::store::{Amend, Amendment, Change, Chat, Lane, Message, Person};

const ANA: &str = "351910000001@s.whatsapp.net";
const RUI: &str = "351910000002@s.whatsapp.net";
const RUI_LID: &str = "100000000002@lid";
const CLIMBING: &str = "120363000000000001@g.us";
const FAMILIA: &str = "120363000000000002@g.us";

fn people_and_chats() -> Vec<Change> {
    vec![
        Change::Person(Person { jids: vec![ANA.into()], name: Some("Ana".into()), ..Person::default() }),
        Change::Person(Person {
            jids: vec![RUI.into(), RUI_LID.into()],
            name: Some("Rui".into()),
            ..Person::default()
        }),
        Change::Chat(Chat { id: ANA.into(), ..Chat::default() }),
        Change::Chat(Chat { id: CLIMBING.into(), name: Some("Lisbon Climbing".into()), ..Chat::default() }),
        Change::Chat(Chat { id: FAMILIA.into(), name: Some("Família Gomes".into()), ..Chat::default() }),
    ]
}

/// A message as the converter makes one of `kind`, with a caption, from `sender` (yours when None).
fn item(chat: &str, id: &str, at: i64, sender: Option<&str>, kind: &'static str, caption: Option<&str>) -> Message {
    let (detail, text, summary) = match kind {
        "document" => {
            let name = format!("{id}.pdf");
            let text = caption.map_or(name.clone(), |caption| format!("{caption}\n{name}"));
            (
                json!({"media": {"mime": "application/pdf", "bytes": 2_400_000, "name": name, "pages": 3}}),
                Some(text),
                name,
            )
        }
        "audio" => {
            (json!({"media": {"mime": "audio/mpeg", "bytes": 3_000_000, "seconds": 182}}), None, "Audio, 3:02".into())
        }
        "voice" => {
            (json!({"media": {"mime": "audio/ogg; codecs=opus", "seconds": 14}}), None, "Voice message, 0:14".into())
        }
        "text" => (Value::Null, caption.map(str::to_string), caption.unwrap_or_default().to_string()),
        _ => (
            json!({"media": {"mime": "image/jpeg", "w": 1600, "h": 1200, "bytes": 200_000}}),
            caption.map(str::to_string),
            caption.unwrap_or("Photo").to_string(),
        ),
    };
    Message {
        chat: chat.into(),
        id: id.into(),
        at,
        sender: sender.map(Into::into),
        mine: sender.is_none(),
        status: sender.is_none().then_some("sent"),
        kind,
        text,
        summary,
        detail: (!detail.is_null()).then(|| detail.to_string()),
        live: true,
        ..Message::default()
    }
}

fn ids(result: &Value) -> Vec<&str> {
    result["rows"].as_array().unwrap().iter().map(|row| row["id"].as_str().unwrap()).collect()
}

fn row<'a>(result: &'a Value, id: &str) -> &'a Value {
    result["rows"].as_array().unwrap().iter().find(|row| row["id"] == id).unwrap_or_else(|| panic!("no {id}: {result}"))
}

/// A day of everything the library holds, and things it doesn't.
async fn shared(bed: &Bed) {
    bed.apply(people_and_chats()).await;
    bed.apply(vec![
        Change::Message(item(CLIMBING, "p1", 1_000, Some(RUI), "image", Some("na praia\ncom a Inês"))),
        Change::Message(item(CLIMBING, "v1", 2_000, Some(RUI_LID), "video", None)),
        Change::Message(item(CLIMBING, "g1", 3_000, None, "gif", None)),
        Change::Message(item(ANA, "n1", 4_000, Some(ANA), "video_note", None)),
        Change::Message(item(ANA, "s1", 5_000, Some(ANA), "voice", None)),
        Change::Message(item(FAMILIA, "d1", 6_000, None, "document", Some("o orçamento"))),
        Change::Message(item(FAMILIA, "a1", 7_000, Some(ANA), "audio", None)),
        Change::Message(item(FAMILIA, "k1", 8_000, Some(ANA), "sticker", None)),
        Change::Message(item(FAMILIA, "t1", 9_000, Some(ANA), "text", Some("olá, vamos à praia"))),
        Change::Message(item(FAMILIA, "p2", 10_000, Some(ANA), "image", None)),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_holds_a_kinds_items_newest_first_with_what_each_needs_and_every_kinds_counts() {
    let bed = Bed::new(brisk());
    shared(&bed).await;
    let mut w = bed.watcher(64);
    let watch = |kind: &str, more: Value| {
        let mut params = json!({"kind": kind, "around": "top", "after": 10});
        params.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        params
    };
    let photos = w.call("library.watch", watch("photo", json!({}))).await.unwrap();
    assert_eq!(ids(&photos), ["p2", "p1"], "newest first, the sticker and the text no items");
    assert_eq!((photos["total"].as_u64(), photos["older"].as_bool()), (Some(2), Some(false)));
    assert_eq!(photos["counts"], json!({"photo": 2, "video": 3, "voice": 1, "file": 2}), "every kind's, links unknown");
    let p1 = row(&photos, "p1");
    assert_eq!((p1["kind"].as_str(), p1["message"].as_str()), (Some("photo"), Some("p1")));
    assert_eq!((p1["chat"].as_str(), p1["chat_name"].as_str()), (Some(CLIMBING), Some("Lisbon Climbing")));
    assert_eq!((p1["from"]["name"].as_str(), p1["mine"].as_bool()), (Some("Rui"), Some(false)));
    assert_eq!(p1["caption"], json!([{"t": "text", "s": "na praia com a Inês"}]), "on one line");
    assert_eq!((p1["media"]["w"].as_u64(), p1["media"]["h"].as_u64()), (Some(1600), Some(1200)));
    assert_eq!(row(&photos, "p2")["caption"], json!([]), "no caption");
    assert!(p1["sort"].as_str() < row(&photos, "p2")["sort"].as_str(), "read from the largest sort down");

    let videos = w.call("library.watch", watch("video", json!({}))).await.unwrap();
    assert_eq!(ids(&videos), ["n1", "g1", "v1"], "GIFs and video messages too");
    assert!(row(&videos, "g1").get("from").is_none() && row(&videos, "g1")["mine"] == true, "yours");
    assert_eq!(row(&videos, "v1")["from"]["name"], "Rui", "his LID's message is his");

    let files = w.call("library.watch", watch("file", json!({}))).await.unwrap();
    assert_eq!(ids(&files), ["a1", "d1"], "documents and audio files");
    assert_eq!(row(&files, "d1")["file"], json!({"name": "d1.pdf", "type": "PDF", "bytes": 2_400_000, "pages": 3}));
    assert_eq!(row(&files, "d1")["caption"], json!([{"t": "text", "s": "o orçamento"}]), "the caption, not the name");
    assert_eq!(row(&files, "a1")["file"], json!({"name": "Audio, 3:02", "type": "MP3", "bytes": 3_000_000}));
    let voice = w.call("library.watch", watch("voice", json!({}))).await.unwrap();
    assert_eq!(row(&voice, "s1")["voice"], json!({"seconds": 14.0}));

    let all = w.call("library.watch", watch("all", json!({"after": 1}))).await.unwrap();
    assert_eq!(ids(&all), ["p2", "n1", "s1", "a1"], "the newest of each kind, unpaged");
    assert_eq!((all["total"].as_u64(), all["older"].as_bool()), (Some(8), Some(false)));

    let narrowed = |more| watch("photo", more);
    let since = w.call("library.watch", narrowed(json!({"since": 5_000}))).await.unwrap();
    assert_eq!((ids(&since), since["counts"]["video"].as_u64()), (vec!["p2"], Some(0)));
    let until = w.call("library.watch", narrowed(json!({"until": 1_000}))).await.unwrap();
    assert_eq!(ids(&until), ["p1"], "both ends included");
    let rui = w.call("library.watch", watch("video", json!({"person": "person:2"}))).await.unwrap();
    assert_eq!((ids(&rui), rui["counts"]["photo"].as_u64()), (vec!["v1"], Some(1)), "by either of his JIDs");
    let mine = w.call("library.watch", watch("file", json!({"person": "me"}))).await.unwrap();
    assert_eq!((ids(&mine), mine["counts"]["video"].as_u64()), (vec!["d1"], Some(1)));
    let familia = w.call("library.watch", watch("file", json!({"chat": FAMILIA}))).await.unwrap();
    assert_eq!(
        (ids(&familia), familia["counts"].clone()),
        (vec!["a1", "d1"], json!({"photo": 1, "video": 0, "voice": 0, "file": 2}))
    );
    let hers = w.call("library.watch", watch("file", json!({"chat": FAMILIA, "person": "person:1"}))).await.unwrap();
    assert_eq!(ids(&hers), ["a1"], "one person's items in one chat");

    let praia = w.call("library.watch", narrowed(json!({"text": "PRAIA"}))).await.unwrap();
    assert_eq!(ids(&praia), ["p1"], "by its caption, never the text message");
    assert_eq!(
        row(&praia, "p1")["caption"],
        json!([{"t": "text", "s": "na "}, {"t": "mark", "s": "praia"}, {"t": "text", "s": " com a Inês"}])
    );
    let orcamento = w.call("library.watch", watch("file", json!({"text": "orcamento"}))).await.unwrap();
    assert_eq!(ids(&orcamento), ["d1"], "accent-blind");
    let named = w.call("library.watch", watch("file", json!({"text": "d1"}))).await.unwrap();
    assert_eq!((ids(&named), named["counts"]["file"].as_u64()), (vec!["d1"], Some(1)), "by a file's name");
}

#[tokio::test(flavor = "multi_thread")]
async fn library_watch_says_what_it_cant_do() {
    let bed = Bed::new(brisk());
    shared(&bed).await;
    let mut w = bed.watcher(64);
    let code = |result: Result<Value, jaw_proto::Error>| {
        let e = result.unwrap_err();
        (e.code, e.data.as_ref().and_then(|d| d["param"].as_str()).map(str::to_string))
    };
    let asked = |params: Value| params;
    let nope = |code: i32| (code, None);
    let param = |code: i32, p: &str| (code, Some(p.to_string()));
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "chat": "x@g.us", "around": "top"}))).await),
        nope(3)
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "person": "person:99", "around": "top"}))).await),
        nope(3)
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "person": "bob", "around": "top"}))).await),
        param(-32602, "person")
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "since": 2, "until": 1, "around": "top"}))).await),
        param(-32602, "since")
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "around": "newest"}))).await),
        param(-32602, "around")
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "all", "around": "p1"}))).await),
        param(-32602, "around")
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "around": "v1"}))).await),
        nope(3),
        "a video's not a photo"
    );
    assert_eq!(
        code(w.call("library.watch", asked(json!({"kind": "photo", "around": "top", "after": 1_001}))).await).0,
        -32602
    );
    assert_eq!(code(w.call("library.watch", asked(json!({"kind": "pictures", "around": "top"}))).await).0, -32602);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_links_window_is_empty_and_says_links_come_later() {
    let bed = Bed::new(brisk());
    shared(&bed).await;
    let mut w = bed.watcher(64);
    let links = w.call("library.watch", json!({"kind": "link", "around": "top", "after": 10})).await.unwrap();
    assert!(ids(&links).is_empty() && links["total"] == 0 && links["older"] == false);
    assert_eq!(links["note"].as_str(), Some(NO_LINKS));
    assert!(links["counts"].get("link").is_none(), "not a count of none: {}", links["counts"]);
    let all = w.call("library.watch", json!({"kind": "all", "around": "top", "after": 3})).await.unwrap();
    assert_eq!(all["note"].as_str(), Some(NO_LINKS));
    let photos = w.call("library.watch", json!({"kind": "photo", "around": "top", "after": 3})).await.unwrap();
    assert!(photos.get("note").is_none());
}

fn revoke(chat: &str, id: &str) -> Change {
    Change::Amend(Amend {
        chat: chat.into(),
        target: id.into(),
        sender: Some(RUI.into()),
        mine: false,
        at: 99_000,
        what: Amendment::Revoke,
        raw: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_moves_around_an_item_and_keeps_its_place_when_the_item_goes() {
    let bed = Bed::new(brisk());
    bed.apply(people_and_chats()).await;
    let photos: Vec<Change> = (1..=20)
        .map(|k| Change::Message(item(CLIMBING, &format!("p{k:02}"), k * 1_000, Some(RUI), "image", None)))
        .collect();
    bed.apply(photos).await;
    let mut w = bed.watcher(64);
    let top = w.call("library.watch", json!({"kind": "photo", "around": "top", "after": 5})).await.unwrap();
    assert_eq!((ids(&top), top["older"].as_bool()), (vec!["p20", "p19", "p18", "p17", "p16"], Some(true)));
    let watch = top["watch"].as_str().unwrap().to_string();
    let moved = w.call("watch.move", json!({"watch": watch, "around": "p10", "before": 2, "after": 2})).await.unwrap();
    assert_eq!(w.windows[&watch].ids(), ["p12", "p11", "p10", "p09", "p08"].map(String::from));
    assert_eq!(moved["remove"].as_array().unwrap().len(), 5, "the rows that left: {moved}");
    let seq = bed.apply(vec![Change::DeleteMessage { chat: CLIMBING.into(), id: "p10".into() }]).await;
    w.settle(seq).await;
    assert_eq!(w.windows[&watch].ids(), ["p12", "p11", "p09", "p08"].map(String::from), "where it was");
    let seq = bed.apply(vec![revoke(CLIMBING, "p09")]).await;
    w.settle(seq).await;
    let model = &w.windows[&watch];
    assert_eq!(model.ids(), ["p12", "p11", "p08", "p07"].map(String::from), "deleted for everyone, it's no item");
    assert_eq!((model.total, model.counts["photo"].as_u64()), (18, Some(18)));
    assert!(model.older);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_that_comes_again_or_twice_changes_nothing_twice() {
    let bed = Bed::new(brisk());
    shared(&bed).await;
    let mut w = bed.watcher(64);
    let photos = w.call("library.watch", json!({"kind": "photo", "around": "top", "after": 10})).await.unwrap();
    let watch = photos["watch"].as_str().unwrap().to_string();
    let again = item(CLIMBING, "p1", 1_000, Some(RUI), "image", Some("na praia\ncom a Inês"));
    let history = Message { live: false, ..again.clone() };
    bed.apply(vec![Change::Message(again.clone()), Change::Message(again)]).await;
    let seq = bed.store.apply(Lane::Background, vec![Change::Message(history)]).await.unwrap().seq;
    bed.store.indexed().await.unwrap();
    w.settle(seq.max(bed.feed.seq() as i64)).await;
    let model = &w.windows[&watch];
    assert_eq!((model.ids(), model.total), (vec!["p2".to_string(), "p1".into()], 2));
    assert_eq!(model.counts, json!({"photo": 2, "video": 3, "voice": 1, "file": 2}));
    let praia = w.call("library.watch", json!({"kind": "photo", "text": "praia", "around": "top", "after": 5})).await;
    assert_eq!(ids(&praia.unwrap()), ["p1"], "once");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_found_by_its_words_comes_into_a_window_once_they_are_indexed() {
    let bed = Bed::new(brisk());
    shared(&bed).await;
    // Every message's words wait in fts_backlog to be indexed, live ones too.
    bed.store.indexed().await.unwrap();
    let mut w = bed.watcher(64);
    let watch = w.watch("library.watch", json!({"kind": "photo", "text": "praia", "around": "top", "after": 5})).await;
    assert_eq!(w.windows[&watch].ids(), ["p1"]);
    // History waits in fts_backlog for its words to be indexed, after it's written, in a step of its
    // own that nothing else in this window changes in.
    let old = Message { live: false, ..item(FAMILIA, "h1", 500, Some(ANA), "image", Some("a praia em 2019")) };
    bed.store.apply(Lane::Background, vec![Change::Message(old)]).await.unwrap();
    bed.store.indexed().await.unwrap();
    w.settle(bed.feed.seq() as i64).await;
    assert_eq!(w.windows[&watch].ids(), ["p1", "h1"], "found once its words are");
    same(
        &w.windows[&watch],
        &fresh(&bed, &json!({"kind": "photo", "text": "praia", "around": "top", "after": 5})).await,
        "indexed",
    );
}

/// The window `params` asks for, as a fresh watch on a connection of its own gives it.
async fn fresh(bed: &Bed, params: &Value) -> Model {
    let mut w = bed.watcher(64);
    let watch = w.watch("library.watch", params.clone()).await;
    w.windows.remove(&watch).unwrap()
}

fn same(held: &Model, fresh: &Model, what: &str) {
    assert_eq!(held.ids(), fresh.ids(), "{what}: its rows");
    assert_eq!(held.ordered(), fresh.ordered(), "{what}: what its rows say");
    assert_eq!((held.total, held.older), (fresh.total, fresh.older), "{what}: total and older");
    assert_eq!(held.counts, fresh.counts, "{what}: counts");
}

/// A xorshift generator, so a failing seed can be run again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, of: &'a [T]) -> &'a T {
        &of[self.below(of.len())]
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_match_fresh_watches_after_random_writes() {
    const KINDS: [&str; 9] = ["image", "image", "video", "gif", "voice", "document", "audio", "sticker", "text"];
    const SAID: [Option<&str>; 4] = [None, Some("na praia"), Some("o jantar"), Some("fotos da praia")];
    const CHATS: [&str; 3] = [ANA, CLIMBING, FAMILIA];
    const SENDERS: [Option<&str>; 4] = [None, Some(ANA), Some(RUI), Some(RUI_LID)];
    for seed in 1..=4u64 {
        let bed = Bed::new(brisk());
        bed.apply(people_and_chats()).await;
        let mut rng = Rng(0x2545_F491_4F6C_DD1D ^ seed);
        let mut made: Vec<(String, String)> = Vec::new();
        let mut at = 1_000;
        for k in 0..30 {
            at += 1_000;
            let chat = *rng.pick(&CHATS);
            let id = format!("s{k}");
            bed.apply(vec![Change::Message(item(chat, &id, at, *rng.pick(&SENDERS), "image", *rng.pick(&SAID)))]).await;
            // The anchor stays, so a fresh watch can be anchored on it too.
            if id != "s15" {
                made.push((chat.into(), id));
            }
        }
        let wants = [
            json!({"kind": "photo", "around": "top", "after": 5}),
            json!({"kind": "photo", "around": "s15", "before": 3, "after": 3}),
            json!({"kind": "video", "chat": CLIMBING, "around": "top", "after": 4}),
            json!({"kind": "file", "person": "person:1", "around": "top", "after": 4}),
            json!({"kind": "photo", "person": "person:2", "chat": FAMILIA, "around": "top", "after": 4}),
            json!({"kind": "all", "around": "top", "after": 2}),
            json!({"kind": "photo", "text": "praia", "around": "top", "after": 4}),
            json!({"kind": "voice", "person": "me", "around": "top", "after": 3}),
            json!({"kind": "photo", "since": 20_000, "until": 60_000, "around": "top", "after": 6}),
        ];
        let mut w = bed.watcher(256);
        let mut watched = Vec::new();
        for want in &wants {
            watched.push((w.watch("library.watch", want.clone()).await, want.clone()));
        }
        for round in 0..12 {
            let mut changes = Vec::new();
            let mut history = Vec::new();
            for _ in 0..1 + rng.below(6) {
                at += 1_000;
                match rng.below(10) {
                    0..=4 => {
                        let (chat, kind) = (*rng.pick(&CHATS), *rng.pick(&KINDS));
                        let id = format!("r{round}-{}", rng.next() % 100_000);
                        let message = item(chat, &id, at, *rng.pick(&SENDERS), kind, *rng.pick(&SAID));
                        made.push((chat.into(), id));
                        changes.push(Change::Message(message));
                    }
                    5 => {
                        let (chat, kind) = (*rng.pick(&CHATS), *rng.pick(&KINDS));
                        let id = format!("h{round}-{}", rng.next() % 100_000);
                        let old = Message {
                            live: false,
                            ..item(chat, &id, at - 500_000, *rng.pick(&SENDERS), kind, *rng.pick(&SAID))
                        };
                        made.push((chat.into(), id));
                        history.push(Change::Message(old));
                    }
                    6 if !made.is_empty() => {
                        let (chat, id) = made[rng.below(made.len())].clone();
                        changes.push(Change::DeleteMessage { chat, id });
                    }
                    7 if !made.is_empty() => {
                        let (chat, id) = made[rng.below(made.len())].clone();
                        changes.push(revoke(&chat, &id));
                    }
                    8 => {
                        let name = format!("Climbing {}", rng.below(100));
                        changes.push(Change::Chat(Chat { id: CLIMBING.into(), name: Some(name), ..Chat::default() }));
                    }
                    _ => {
                        let name = format!("Ana {}", rng.below(100));
                        changes.push(Change::Person(Person {
                            jids: vec![ANA.into()],
                            name: Some(name),
                            ..Person::default()
                        }));
                    }
                }
            }
            if !history.is_empty() {
                bed.store.apply(Lane::Background, history).await.unwrap();
            }
            bed.apply(changes).await;
            bed.store.indexed().await.unwrap();
            w.settle(bed.feed.seq() as i64).await;
            for (watch, want) in &watched {
                let held = &w.windows[watch];
                same(held, &fresh(&bed, want).await, &format!("seed {seed}, round {round}, {want}"));
            }
        }
    }
}

#[test]
fn a_file_says_what_it_is_by_its_extension_or_its_mime_type() {
    assert_eq!(extension("orcamento-obras.pdf").as_deref(), Some("PDF"));
    assert_eq!(extension("track.gpx").as_deref(), Some("GPX"));
    assert_eq!(extension("no extension"), None);
    assert_eq!(extension("odd.not an extension"), None);
    assert_eq!(mime_type("audio/mpeg").as_deref(), Some("MP3"));
    assert_eq!(mime_type("audio/ogg; codecs=opus").as_deref(), Some("OGG"));
    assert_eq!(mime_type("application/x-7z"), Some("7Z".into()));
    assert_eq!(mime_type("application/vnd.openxmlformats-officedocument.wordprocessingml.document"), None);
}
