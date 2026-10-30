//! Media out, end to end: files sent with `send.media` through the store, kept, made with jaw-media
//! and uploaded by the demo engine, and sent through the outbox, with their rows as interfaces read
//! them, the files left in jaw's folder, and what the demo's WhatsApp was given.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use rusqlite::Connection;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use wa_proto::wa_web_protobufs_e2e as e2e;
use wa_proto::wa_web_protobufs_e2e::message_association::AssociationType;

use super::super::*;
use super::{OUTGOING, UPLOADS};
use crate::engine::demo::{self, DemoEngine, Picture, Replies, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::send::{Makers, Sending, Timing};
use crate::store::{Amend, Amendment, Chat, Lane, read};

const ANA: &str = "351912345678@s.whatsapp.net";

/// jawd's media, sending, store and sync on a folder of their own, with the demo engine on `world`.
struct Rig {
    home: PathBuf,
    store: Store,
    world: Arc<World>,
    media: Media,
    sending: Sending,
    pushes: broadcast::Sender<Push>,
}

fn quick() -> Config {
    Config {
        retry_first: Duration::from_millis(50),
        retry_most: Duration::from_millis(200),
        keep_free: 0,
        send_keep_free: 0,
        ..Config::default()
    }
}

fn timing() -> Timing {
    Timing { retry_first: Duration::from_millis(5), retry_most: Duration::from_millis(20), ..Timing::default() }
}

fn world() -> Arc<World> {
    let world: Arc<World> = Arc::default();
    world.replying(Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
    world
}

impl Rig {
    async fn on(home: &Path, world: Arc<World>) -> Rig {
        Rig::with(home, world, quick()).await
    }

    async fn with(home: &Path, world: Arc<World>, config: Config) -> Rig {
        let store = Store::open(home).unwrap();
        crate::sync::running(&store).await.unwrap();
        let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let (pushes, _) = broadcast::channel(4096);
        let mut connected = pushes.subscribe();
        let pipeline = jaw_media::Media::new(jaw_media::Config {
            previews: jaw_media::Durability::Derived,
            ..jaw_media::Config::default()
        });
        let media = Media::start_with(store.clone(), Some(supervisor.clone()), home, &live, &pushes, config, pipeline)
            .await
            .unwrap();
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live.clone(),
            pushes.clone(),
            Some(supervisor.clone()),
            home.into(),
            Default::default(),
            crate::feed::Typing::default(),
        );
        tokio::spawn(sync);
        let attention = Arc::new(crate::attention::Attention::default());
        let sending = Sending::start(
            store.clone(),
            Some(supervisor),
            live,
            &pushes,
            attention.focus(),
            timing(),
            Makers { previews: crate::send::link::Previews::off(), media: Some(media.clone()) },
        )
        .await
        .unwrap();
        let online = async {
            while let Ok(push) = connected.recv().await {
                if push.method == "connection" && push.params["state"] == "online" {
                    return;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), online).await.expect("the demo engine never connected");
        store.apply(Lane::Live, vec![Change::Chat(Chat { id: ANA.into(), ..Chat::default() })]).await.unwrap();
        Rig { home: home.to_path_buf(), store, world, media, sending, pushes }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, jaw_proto::Error> {
        self.sending.call(method, params).await
    }

    async fn row(&self, id: &str) -> Value {
        let id = id.to_string();
        let row = self
            .store
            .reads()
            .query(Priority::Interactive, move |db| Ok(read::message(db, ANA, &id)?.map(|(row, _)| row)))
            .await
            .unwrap();
        row.map_or(Value::Null, |row| serde_json::to_value(crate::feed::message_row(row)).unwrap())
    }

    /// Waits until message `id`'s row says what `done` wants, and returns it.
    async fn until(&self, id: &str, what: &str, done: impl Fn(&Value) -> bool) -> Value {
        let waited = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let row = self.row(id).await;
                if done(&row) {
                    return row;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        match waited.await {
            Ok(row) => row,
            Err(_) => {
                let row = tokio::time::timeout(Duration::from_secs(5), self.row(id)).await;
                panic!("{id} never came to {what}; it is {row:?}")
            }
        }
    }

    async fn query<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> T {
        self.store.reads().query(Priority::Interactive, query).await.unwrap()
    }

    /// The files in one of jaw's folders, by name.
    fn files(&self, folder: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.home.join(folder))
            .map(|entries| entries.filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned())).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// What the demo's WhatsApp was sent as message `id`, each time it was sent.
    fn sent(&self, id: &str) -> Vec<e2e::Message> {
        self.world
            .sends()
            .into_iter()
            .filter(|s| s.id == id)
            .map(|s| e2e::Message::decode(s.raw.as_slice()).unwrap())
            .collect()
    }
}

/// A photo of yours outside jaw's folder, as you pick one to send.
fn yours(dir: &Path, name: &str, picture: &Picture) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, picture.data.as_slice()).unwrap();
    path
}

fn sent(row: &Value) -> bool {
    matches!(row["status"].as_str(), Some("sent" | "delivered" | "read"))
}

/// The image a message carries, out of the wrappers its kind goes in.
fn image(message: &e2e::Message) -> &e2e::ImageMessage {
    message
        .image_message
        .as_deref()
        .or_else(|| message.associated_child_message.as_ref()?.message.as_ref()?.image_message.as_deref())
        .expect("a photo")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_photo_is_kept_made_uploaded_and_sent_with_what_its_message_needs_and_each_step_shows() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let mut pushes = rig.pushes.subscribe();
    let lake = Picture::lake(1200, 900);
    let path = yours(picked.path(), "lake.jpg", &lake);
    rig.world.gate_uploads(true);
    let answer = rig
        .call("send.media", json!({ "chat": ANA, "files": [{ "path": path, "caption": "the lake" }] }))
        .await
        .unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    assert!(answer.get("album").is_none(), "one photo is no album");
    let row = &answer["rows"][0];
    assert_eq!((&row["kind"], &row["status"], &row["mine"]), (&json!("image"), &json!("pending"), &json!(true)));
    assert_eq!((&row["media"]["w"], &row["media"]["h"]), (&json!(1200), &json!(900)), "its shape from the start");
    assert_eq!(row["spans"][0]["s"], "the lake");
    // Made, and in the cache by its content, while its upload waits.
    let made = rig.until(&id, "made", |row| row["media"]["state"] == "local").await;
    assert_eq!(made["status"], "pending", "it goes once it's uploaded");
    let file = std::fs::read(rig.home.join(made["media"]["path"].as_str().unwrap())).unwrap();
    let hash = to_hex(&<sha2::Sha256 as sha2::Digest>::digest(&file));
    assert_eq!(made["media"]["path"], format!("media/files/{hash}.jpg"));
    assert_eq!(file, *lake.data, "a photo that fits SD goes untouched");
    rig.world.gate_uploads(false);
    rig.until(&id, "sent, with its previews", |row| sent(row) && row["media"]["previews"]["256"].is_string()).await;
    let uploads = rig.world.uploads();
    assert_eq!(uploads.len(), 1);
    assert_eq!(
        (uploads[0].file_sha256.clone(), uploads[0].kind),
        (hex::decode(&hash), pb::media_download::Kind::Image as i32)
    );
    let [message] = rig.sent(&id).try_into().unwrap();
    let photo = image(&message);
    assert_eq!(photo.caption.as_deref(), Some("the lake"));
    assert_eq!((photo.width, photo.height, photo.file_length), (Some(1200), Some(900), Some(lake.data.len() as u64)));
    assert_eq!(photo.file_sha256.as_deref().map(to_hex), Some(hash.clone()));
    assert!(
        photo.url.is_some() && photo.direct_path.is_some() && photo.media_key.as_ref().is_some_and(|k| k.len() == 32)
    );
    assert!(photo.jpeg_thumbnail.as_ref().is_some_and(|t| t.starts_with(&[0xFF, 0xD8])), "it carries its thumbnail");
    assert!(photo.media_key_timestamp.is_some_and(|t| t > 0));
    // What it sent is what jaw keeps of it, and its file can be had again from WhatsApp.
    let kept = rig
        .query({
            let id = id.clone();
            move |db| {
                let raw: Vec<u8> = db.query_row("SELECT raw FROM messages WHERE id = ?1", [&id], |r| r.get(0))?;
                crate::store::unpack(&raw)
            }
        })
        .await;
    let kept = wa_proto::wa_web_protobufs_web::WebMessageInfo::decode(kept.as_slice()).unwrap().message.unwrap();
    assert_eq!(kept, message, "the row's raw protobuf is what was sent");
    assert_eq!(*rig.world.file_at(photo.direct_path.as_deref().unwrap()).unwrap(), *lake.data);
    // jaw's copy of the photo you picked is gone once the copy sent is made, and nothing's half written.
    for _ in 0..200 {
        if rig.files("media/outgoing").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(rig.files("media/outgoing").is_empty(), "left {:?}", rig.files("media/outgoing"));
    assert!(rig.files("media/uploads").is_empty(), "left {:?}", rig.files("media/uploads"));
    let outgoing: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM outgoing", [], |r| r.get(0))?)).await;
    assert_eq!(outgoing, 0, "nothing more to make or upload once it's gone");
    // How far it got was pushed: made, then uploaded.
    let mut kinds = Vec::new();
    while let Ok(push) = pushes.try_recv() {
        if push.method == "job" && push.params["message"] == id.as_str() {
            kinds.push((push.params["kind"].as_str().unwrap().to_string(), push.params["progress"].as_f64().unwrap()));
        }
    }
    assert!(kinds.contains(&("transcode".into(), 1.0)) && kinds.contains(&("upload".into(), 1.0)), "{kinds:?}");
    let first_upload = kinds.iter().position(|(k, _)| k == "upload").unwrap();
    assert!(kinds[first_upload..].iter().all(|(k, _)| k == "upload"), "made before it's uploaded: {kinds:?}");
}

mod hex {
    pub fn decode(hex: &str) -> Vec<u8> {
        crate::store::media::from_hex(hex).unwrap()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_file_you_picked_can_change_or_go_once_send_media_has_answered() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let lake = Picture::lake(1000, 750);
    let path = yours(picked.path(), "lake.jpg", &lake);
    rig.media.hold_making(true);
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    // Written over where it is, then gone, before a copy is made from it.
    std::fs::OpenOptions::new().write(true).open(&path).unwrap().write_all(b"not a photo any more").unwrap();
    std::fs::remove_file(&path).unwrap();
    rig.media.hold_making(false);
    rig.media.send_now();
    rig.until(&id, "sent", sent).await;
    let [message] = rig.sent(&id).try_into().unwrap();
    assert_eq!(image(&message).file_sha256.as_deref(), Some(lake.sha256.as_slice()), "what you picked, as it was");
}

use std::io::Write as _;

#[tokio::test(flavor = "multi_thread")]
async fn hd_goes_as_whatsapps_dual_upload_the_sd_copy_first_then_the_hd_copy_naming_it() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let big = Picture::lake(2400, 1800);
    let path = yours(picked.path(), "big.jpg", &big);
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let row = rig
        .until(&id, "sent with its HD copy", |row| {
            sent(row) && row["media"]["hd"] == json!(true) && row["media"]["hd_path"].is_string()
        })
        .await;
    let hd = loop {
        if let Some(hd) = rig.world.sends().into_iter().find(|s| s.id != id) {
            break hd;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let order: Vec<String> = rig.world.sends().into_iter().map(|s| s.id).collect();
    assert_eq!(order, [id.clone(), hd.id.clone()], "the SD message goes first, then its HD copy");
    let sd = image(&rig.sent(&id)[0]).clone();
    assert_eq!((sd.width, sd.height), (Some(1600), Some(1200)), "SD is shrunk");
    let child = e2e::Message::decode(hd.raw.as_slice()).unwrap();
    let inner = child.associated_child_message.as_ref().and_then(|c| c.message.as_deref()).expect("a child message");
    let association = inner.message_context_info.as_ref().and_then(|c| c.message_association.as_ref()).unwrap();
    assert_eq!(association.association_type, Some(AssociationType::HdImageDualUpload as i32));
    assert_eq!(association.parent_message_key.as_ref().and_then(|k| k.id.as_deref()), Some(id.as_str()));
    let full = image(&child);
    assert_eq!((full.width, full.height), (Some(2400), Some(1800)), "HD keeps the photo's size");
    assert!(full.url.is_some() && full.jpeg_thumbnail.is_some());
    assert_eq!(rig.world.uploads().len(), 2);
    let hd_file = std::fs::read(rig.home.join(row["media"]["hd_path"].as_str().unwrap())).unwrap();
    assert_eq!(hd_file, *big.data, "a photo that fits HD goes untouched as its HD copy");
    assert!(rig.row(&hd.id).await.is_null(), "the HD copy is no message of its own");
}

#[tokio::test(flavor = "multi_thread")]
async fn several_photos_go_as_an_album_in_the_order_given_with_the_reply_on_the_first() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let asked = rig.call("send.text", json!({ "chat": ANA, "text": "send me the photos" })).await.unwrap();
    let question = asked["message"].as_str().unwrap().to_string();
    let files: Vec<Value> = (0..3)
        .map(|n| json!({ "path": yours(picked.path(), &format!("p{n}.jpg"), &Picture::numbered(640 + n, 480, n)), "caption": format!("photo {n}") }))
        .collect();
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": files, "reply": question })).await.unwrap();
    let album = answer["album"].as_str().expect("an album").to_string();
    let ids: Vec<String> =
        answer["messages"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 3);
    for id in &ids {
        rig.until(id, "sent", sent).await;
    }
    let sends: Vec<String> = rig.world.sends().into_iter().map(|s| s.id).filter(|s| *s != question).collect();
    assert_eq!(sends, [vec![album.clone()], ids.clone()].concat(), "the album first, then each in the order given");
    let parent =
        e2e::Message::decode(rig.world.sends().iter().find(|s| s.id == album).unwrap().raw.as_slice()).unwrap();
    assert_eq!(
        parent.album_message.as_ref().map(|a| (a.expected_image_count, a.expected_video_count)),
        Some((Some(3), Some(0)))
    );
    for (n, id) in ids.iter().enumerate() {
        let [message] = rig.sent(id).try_into().unwrap();
        let association = message.message_context_info.as_ref().and_then(|c| c.message_association.as_ref()).unwrap();
        assert_eq!(association.association_type, Some(AssociationType::MediaAlbum as i32));
        assert_eq!(association.parent_message_key.as_ref().and_then(|k| k.id.as_deref()), Some(album.as_str()));
        let photo = image(&message);
        assert_eq!(photo.caption, Some(format!("photo {n}")));
        let quoted = photo.context_info.as_ref().and_then(|c| c.stanza_id.as_deref());
        assert_eq!(quoted, (n == 0).then_some(question.as_str()), "the reply goes with the first");
    }
    let row = rig.row(&ids[0]).await;
    assert_eq!(row["reply"]["id"], question.as_str());
    assert_eq!(rig.row(&album).await["kind"], "album");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_cant_be_sent_is_refused_and_nothing_of_it_is_kept() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let good = yours(picked.path(), "good.jpg", &Picture::lake(320, 240));
    let text = picked.path().join("notes.txt");
    std::fs::write(&text, "not a photo").unwrap();
    let huge = picked.path().join("huge.mov");
    std::fs::File::create(&huge).unwrap().set_len(super::FILE_MOST + 1).unwrap();
    for (files, code, what) in [
        (json!([{ "path": "relative/a.jpg" }]), jaw_proto::code::INVALID_PARAMS, "a path that isn't whole"),
        (
            json!([{ "path": good }, { "path": picked.path().join("gone.jpg") }]),
            jaw_proto::code::INVALID_PARAMS,
            "a file that isn't there",
        ),
        (json!([{ "path": good }, { "path": picked.path() }]), jaw_proto::code::INVALID_PARAMS, "a folder"),
        (
            json!([{ "path": good }, { "path": text, "kind": "image" }]),
            jaw_proto::code::INVALID_PARAMS,
            "text as a photo",
        ),
        (json!([{ "path": huge }]), jaw_proto::code::LIMIT, "a file over WhatsApp's 2 GB"),
        (json!([]), jaw_proto::code::INVALID_PARAMS, "nothing"),
    ] {
        let refused = rig.call("send.media", json!({ "chat": ANA, "files": files })).await.unwrap_err();
        assert_eq!(refused.code, code, "{what}: {refused:?}");
        assert!(rig.files("media/outgoing").is_empty(), "{what} left {:?}", rig.files("media/outgoing"));
    }
    let too_many: Vec<Value> = (0..31).map(|_| json!({ "path": good })).collect();
    let refused = rig.call("send.media", json!({ "chat": ANA, "files": too_many })).await.unwrap_err();
    assert_eq!((refused.code, refused.data.unwrap()["limit"].clone()), (jaw_proto::code::LIMIT, json!("files")));
    let nowhere =
        rig.call("send.media", json!({ "chat": "351919999999@s.whatsapp.net", "files": [{ "path": good }] })).await;
    assert_eq!(nowhere.unwrap_err().code, jaw_proto::code::NOT_FOUND);
    assert!(rig.files("media/outgoing").is_empty());
    let messages: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?)).await;
    assert_eq!(messages, 0, "and nothing was written");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_key_answers_with_the_same_messages_and_sends_nothing_new() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let files: Vec<Value> = (0..2)
        .map(|n| json!({ "path": yours(picked.path(), &format!("k{n}.jpg"), &Picture::numbered(400, 300, n)) }))
        .collect();
    let ask = json!({ "chat": ANA, "files": files, "key": "jaw-keyed-photos" });
    let first = rig.call("send.media", ask.clone()).await.unwrap();
    let again = rig.call("send.media", ask.clone()).await.unwrap();
    assert_eq!((&first["messages"], &first["album"]), (&again["messages"], &again["album"]));
    for id in first["messages"].as_array().unwrap() {
        rig.until(id.as_str().unwrap(), "sent", sent).await;
    }
    let after = rig.call("send.media", ask).await.unwrap();
    assert_eq!(after["messages"], first["messages"]);
    assert_eq!(rig.world.uploads().len(), 2, "each file uploaded once");
    assert_eq!(rig.world.sends().len(), 3, "the album and its two photos, once each");
    let messages: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM messages", [], |r| r.get(0))?)).await;
    assert_eq!(messages, 3);
    for _ in 0..200 {
        if rig.files("media/outgoing").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        rig.files("media/outgoing").is_empty(),
        "the copies kept for the repeats went: {:?}",
        rig.files("media/outgoing")
    );
}

fn failed(row: &Value) -> bool {
    row["status"] == "failed"
}

/// Whether ffmpeg is here, for the tests of what only it makes.
fn has_ffmpeg() -> bool {
    jaw_media::Tools::find().has_ffmpeg()
}

/// A file ffmpeg makes from `input`, as a phone's camera or someone's computer would.
fn made_by_ffmpeg(dir: &Path, name: &str, input: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let ran = std::process::Command::new(jaw_media::Tools::find().ffmpeg_path().unwrap())
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
        .args(input)
        .arg(&path)
        .output()
        .unwrap();
    assert!(ran.status.success(), "ffmpeg: {}", String::from_utf8_lossy(&ran.stderr));
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_cancelled_while_its_copy_is_made_or_uploaded_fails_until_retried_and_leaves_nothing_half_done() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    // Cancelled before its copy is made.
    rig.media.hold_making(true);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let first = answer["messages"][0].as_str().unwrap().to_string();
    rig.media.call("media.cancel", json!({ "message": first })).await.unwrap();
    let row = rig.until(&first, "failed", failed).await;
    assert_eq!(row["media"].get("error"), None, "no copy was made for its media row to say why: {row}");
    rig.media.hold_making(false);
    // Cancelled half uploaded.
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "b.jpg", &Picture::lake(800, 600));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let second = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&second, "made", |row| row["media"]["state"] == "local").await;
    loop {
        if rig.files("media/uploads").iter().any(|f| f.ends_with(".part")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    rig.media.call("media.cancel", json!({ "message": second })).await.unwrap();
    let row = rig.until(&second, "failed", failed).await;
    assert_eq!(row["media"]["error"], "you cancelled it");
    for _ in 0..500 {
        if rig.files("media/uploads").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        rig.files("media/uploads").is_empty(),
        "the cancelled upload's partial file went: {:?}",
        rig.files("media/uploads")
    );
    assert!(rig.world.sends().is_empty(), "nothing cancelled went");
    // Both go once they're retried, the first made from jaw's copy of your file, kept for it.
    rig.world.gate_uploads(false);
    for id in [&first, &second] {
        rig.call("send.retry", json!({ "message": id })).await.unwrap();
        rig.until(id, "sent after a retry", sent).await;
        assert_eq!(rig.sent(id).len(), 1, "sent once");
    }
    let unsent: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?)).await;
    assert_eq!(unsent, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_cancelled_after_its_copy_is_made_or_uploaded_but_before_jaw_db_is_told_stays_failed() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let state = |id: &str| {
        let id = id.to_string();
        rig.query(move |db| {
            Ok(db.query_row(
                "SELECT g.state FROM outgoing g JOIN outbox o ON o.n = g.outbox WHERE o.id = ?1",
                [&id],
                |r| r.get::<_, String>(0),
            )?)
        })
    };
    for (n, at) in ["made", "uploaded"].into_iter().enumerate() {
        rig.media.hold_keeping(at == "made");
        rig.world.gate_uploads(at == "uploaded");
        let path = yours(picked.path(), &format!("{at}.jpg"), &Picture::lake(640 + 40 * n as u32, 480));
        let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
        let id = answer["messages"][0].as_str().unwrap().to_string();
        if at == "uploaded" {
            rig.until(&id, "its copy made", |row| row["media"]["state"] == "local").await;
            rig.media.hold_keeping(true);
            rig.world.gate_uploads(false);
        }
        let held = tokio::time::timeout(Duration::from_secs(30), async {
            while rig.media.hold_keeping(true) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        held.await.unwrap_or_else(|_| panic!("its copy was never {at}"));
        rig.media.call("media.cancel", json!({ "message": id })).await.unwrap();
        rig.until(&id, "failed", failed).await;
        rig.media.hold_keeping(false);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(state(&id).await, "failed", "{at} after it was cancelled, it stays failed");
        assert!(failed(&rig.row(&id).await));
        assert_eq!(rig.world.uploads().len(), n, "nothing more is uploaded");
        assert!(rig.sent(&id).is_empty(), "nor sent");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_that_fails_on_the_way_is_tried_again_and_one_refused_fails_until_retried() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let mut pushes = rig.pushes.subscribe();
    rig.world.fail_uploads(2);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&id, "sent after two failed uploads", sent).await;
    assert_eq!(rig.world.uploads().len(), 3);
    rig.world.refuse_uploads(1);
    let path = yours(picked.path(), "b.jpg", &Picture::lake(700, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let refused = answer["messages"][0].as_str().unwrap().to_string();
    let row = rig.until(&refused, "failed", failed).await;
    assert!(row["media"]["error"].as_str().unwrap().contains("refused the file with 413"), "{row}");
    let notice = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let push = pushes.recv().await.unwrap();
            if push.method == "notice" {
                return push.params["text"].as_str().unwrap().to_string();
            }
        }
    })
    .await
    .unwrap();
    assert!(notice.starts_with("jaw couldn't send a photo: "), "{notice}");
    rig.call("send.retry", json!({ "message": refused })).await.unwrap();
    rig.until(&refused, "sent once retried", sent).await;
    assert_eq!(rig.world.uploads().len(), 5, "uploaded again from where it got to, not made again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_jaw_db_cant_say_is_made_or_uploaded_fails_its_message_rather_than_going_round_again() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let refuse = |state: &str| {
        db.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS refused;
             CREATE TRIGGER refused BEFORE UPDATE OF state ON outgoing WHEN NEW.state = '{state}'
             BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;"
        ))
        .unwrap();
    };
    let quarantined = || async {
        rig.query(|db| Ok(db.query_row("SELECT count(*) FROM quarantine", [], |r| r.get::<_, i64>(0))?)).await
    };
    // Made, and jaw.db can't say so.
    refuse("uploading");
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let made = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&made, "failed", failed).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(rig.world.uploads().is_empty(), "nothing is uploaded of a copy jaw.db doesn't name");
    assert_eq!(quarantined().await, 1, "what failed is kept");
    // Uploaded, and jaw.db can't say so.
    refuse("ready");
    let path = yours(picked.path(), "b.jpg", &Picture::lake(700, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let uploaded = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&uploaded, "failed", failed).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.world.uploads().len(), 1, "uploaded once, not again and again");
    assert!(rig.sent(&uploaded).is_empty(), "a message whose upload jaw.db doesn't have isn't sent");
    assert_eq!(quarantined().await, 2);
    // Failed, and jaw.db can't say so either: it's tried again until it can.
    refuse("uploading");
    db.execute_batch(
        "CREATE TRIGGER unfailed BEFORE UPDATE OF state ON outbox WHEN NEW.state = 'failed'
         BEGIN SELECT RAISE(ABORT, 'refused by the test'); END;",
    )
    .unwrap();
    let mut pushes = rig.pushes.subscribe();
    let path = yours(picked.path(), "c.jpg", &Picture::lake(760, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let unfailed = answer["messages"][0].as_str().unwrap().to_string();
    let noticed = async {
        loop {
            if pushes.recv().await.unwrap().method == "notice" {
                return;
            }
        }
    };
    let early = tokio::time::timeout(Duration::from_millis(800), noticed).await;
    assert!(early.is_err(), "nobody is told it failed before jaw.db says so");
    assert_eq!(rig.row(&unfailed).await["status"], "pending");
    db.execute_batch("DROP TRIGGER unfailed").unwrap();
    rig.until(&unfailed, "failed once jaw.db can say so", failed).await;
    db.execute_batch("DROP TRIGGER refused").unwrap();
    for id in [&made, &uploaded, &unfailed] {
        rig.call("send.retry", json!({ "message": id })).await.unwrap();
        rig.until(id, "sent once retried", sent).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_waiting_for_its_file_keeps_its_place_in_its_chat_and_messages_after_it_wait() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let photo = answer["messages"][0].as_str().unwrap().to_string();
    let text = rig.call("send.text", json!({ "chat": ANA, "text": "that's the lake" })).await.unwrap();
    let text = text["message"].as_str().unwrap().to_string();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(rig.world.sends().is_empty(), "the text waits behind the photo");
    rig.world.gate_uploads(false);
    rig.until(&text, "sent", sent).await;
    let order: Vec<String> = rig.world.sends().into_iter().map(|s| s.id).collect();
    assert_eq!(order, [photo, text], "in the order they were sent");
}

#[tokio::test(flavor = "multi_thread")]
async fn videos_gifs_audio_and_documents_go_as_their_kinds_with_what_each_needs() {
    if !has_ffmpeg() {
        eprintln!("ffmpeg isn't here, so videos, GIFs and audio aren't tried");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let video = made_by_ffmpeg(
        picked.path(),
        "clip.mov",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=1280x720:r=30:d=2",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
    );
    let gif = made_by_ffmpeg(picked.path(), "loop.gif", &["-f", "lavfi", "-i", "testsrc2=s=320x240:r=10:d=1"]);
    let song = made_by_ffmpeg(picked.path(), "song.wav", &["-f", "lavfi", "-i", "sine=f=330:d=2"]);
    let rent = picked.path().join("rent.pdf");
    std::fs::write(&rent, b"%PDF-1.7\n% not much of a document\n").unwrap();
    let files = json!([
        { "path": video, "caption": "the clip" },
        { "path": gif },
        { "path": song },
        { "path": rent, "caption": "for March" },
    ]);
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": files, "quality": "hd" })).await.unwrap();
    assert!(answer.get("album").is_none(), "one video is no album");
    let ids: Vec<String> =
        answer["messages"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let kinds: Vec<Value> = answer["rows"].as_array().unwrap().iter().map(|r| r["kind"].clone()).collect();
    assert_eq!(kinds, [json!("video"), json!("gif"), json!("audio"), json!("document")]);
    for id in &ids {
        rig.until(id, "sent", sent).await;
    }
    let [clip] = rig.sent(&ids[0]).try_into().unwrap();
    let clip = clip.video_message.unwrap();
    assert_eq!((clip.width, clip.height, clip.seconds, clip.gif_playback), (Some(848), Some(476), Some(2), None));
    assert_eq!((clip.mimetype.as_deref(), clip.caption.as_deref()), (Some("video/mp4"), Some("the clip")));
    assert!(clip.jpeg_thumbnail.is_some() && clip.url.is_some(), "a video carries its poster's thumbnail");
    // Its HD copy, which needed only its metadata stripped, went after it, naming it.
    let hd = loop {
        if let Some(hd) = rig.world.sends().into_iter().find(|s| !ids.contains(&s.id)) {
            break e2e::Message::decode(hd.raw.as_slice()).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let child = hd.associated_child_message.unwrap().message.unwrap();
    let association = child.message_context_info.unwrap().message_association.unwrap();
    assert_eq!(association.association_type, Some(AssociationType::HdVideoDualUpload as i32));
    assert_eq!(child.video_message.as_ref().map(|v| (v.width, v.height)), Some((Some(1280), Some(720))));
    let [loops] = rig.sent(&ids[1]).try_into().unwrap();
    let loops = loops.video_message.unwrap();
    assert_eq!(
        (loops.gif_playback, loops.mimetype.as_deref()),
        (Some(true), Some("video/mp4")),
        "a GIF goes as a looping MP4"
    );
    let [audio] = rig.sent(&ids[2]).try_into().unwrap();
    let audio = audio.audio_message.unwrap();
    assert_eq!((audio.ptt, audio.mimetype.as_deref(), audio.seconds), (Some(false), Some("audio/mp4"), Some(2)));
    let [document] = rig.sent(&ids[3]).try_into().unwrap();
    let document = document.document_with_caption_message.unwrap().message.unwrap().document_message.unwrap();
    assert_eq!(
        (document.file_name.as_deref(), document.caption.as_deref(), document.mimetype.as_deref()),
        (Some("rent.pdf"), Some("for March"), Some("application/pdf"))
    );
    assert_eq!(document.file_length, Some(std::fs::metadata(&rent).unwrap().len()), "a document goes as it is");
    let lanes: Vec<(String, u64)> = rig.media.lanes().iter().map(|l| l.report()).map(|r| (r.name, r.done)).collect();
    let done = |name: &str| lanes.iter().find(|(n, _)| n == name).map_or(0, |(_, d)| *d);
    assert!(done("transcodes") >= 2 && done("uploads") >= 6, "made and uploaded on their lanes: {lanes:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_is_made_only_with_room_for_it_on_the_disk() {
    if !has_ffmpeg() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::with(home.path(), world(), Config { send_keep_free: u64::MAX / 4, ..quick() }).await;
    let song = made_by_ffmpeg(picked.path(), "song.wav", &["-f", "lavfi", "-i", "sine=f=330:d=1"]);
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": song }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let row = rig.until(&id, "failed", failed).await;
    assert!(row["media"]["error"].is_null(), "no copy was made: {row}");
    let error: String = rig.query(|db| Ok(db.query_row("SELECT error FROM outbox", [], |r| r.get(0))?)).await;
    assert!(error.starts_with("the disk has "), "{error}");
    assert_eq!(rig.files("media/outgoing").len(), 1, "jaw's copy of your file stays for a retry, and nothing else");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_send_deleted_for_you_takes_its_rows_and_jaws_copy_of_your_file_with_it() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.media.hold_making(true);
    let big = yours(picked.path(), "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": big }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    rig.media.call("media.cancel", json!({ "message": id })).await.unwrap();
    rig.until(&id, "failed", failed).await;
    rig.media.hold_making(false);
    let rows = |db: &Connection| -> anyhow::Result<(i64, i64)> {
        Ok((
            db.query_row("SELECT count(*) FROM outbox WHERE kind <> 'app_state'", [], |r| r.get(0))?,
            db.query_row("SELECT count(*) FROM outgoing", [], |r| r.get(0))?,
        ))
    };
    assert_eq!(rig.query(rows).await, (2, 2), "the message and its HD copy, each with its copy to make");
    assert_eq!(rig.files("media/outgoing").len(), 1);
    rig.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    assert_eq!(rig.query(rows).await, (0, 0), "nothing is left to send, nor its HD copy");
    for _ in 0..300 {
        if rig.files("media/outgoing").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        rig.files("media/outgoing").is_empty(),
        "and jaw's copy of your file went: {:?}",
        rig.files("media/outgoing")
    );
    let retried = rig.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(retried.code, jaw_proto::code::NOT_FOUND);
    assert!(rig.world.sends().is_empty() && rig.world.uploads().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_on_its_way_stays_in_a_full_cache_even_once_its_message_is_deleted_for_you() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let wait = Duration::from_secs(2);
    let rig =
        Rig::with(home.path(), world(), Config { cache_bytes: 1, retry_first: wait, retry_most: wait, ..quick() })
            .await;
    // Its first upload fails, so it waits to be uploaded again with no job holding its file.
    rig.world.fail_uploads(1);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&id, "its copy made", |row| row["media"]["state"] == "local").await;
    let failed_once = tokio::time::timeout(Duration::from_secs(10), async {
        while rig.world.uploads().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    failed_once.await.expect("an upload was tried");
    tokio::time::sleep(Duration::from_millis(100)).await;
    // As on the phone, deleting it for you doesn't stop it going, so nothing but the copy on its way
    // names its file.
    rig.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    let path: String = rig
        .query(|db| {
            Ok(db.query_row("SELECT f.path FROM outgoing g JOIN media_files f ON f.sha256 = g.sha256", [], |r| {
                r.get(0)
            })?)
        })
        .await;
    let file = home.path().join(path);
    for _ in 0..5 {
        rig.media.inner.clean.notify_one();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(rig.world.uploads().len(), 1, "still waiting to be uploaded again");
    assert!(file.exists(), "the cache is over its size, and the copy on its way stays: {:?}", rig.media.status());
    let went = tokio::time::timeout(Duration::from_secs(30), async {
        while rig.sent(&id).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    went.await.expect("it went");
    for _ in 0..300 {
        rig.media.inner.clean.notify_one();
        if !file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!file.exists(), "once it has gone, nothing names its file, and the cache drops it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_of_your_file_goes_only_once_a_synced_commit_has_stopped_naming_it() {
    let home = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let name = format!("{OUTGOING}/3EB0UNNEEDED.jpg");
    let file = home.path().join(&name);
    std::fs::write(&file, b"a copy of yours nothing needs any more").unwrap();
    let holder = Connection::open(home.path().join("jaw.db")).unwrap();
    holder.execute("INSERT INTO outgoing_drop (path) VALUES (?1)", [&name]).unwrap();
    // jaw.db's write lock, held from outside, so nothing can be committed until it's let go.
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let dropping = {
        let inner = rig.media.inner.clone();
        tokio::spawn(async move { super::drop_some(&inner).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(file.exists(), "the file stays while the commit that stops naming it can't be made");
    let synced = rig.store.storage().synced_commits;
    holder.execute_batch("COMMIT").unwrap();
    dropping.await.unwrap().unwrap();
    assert!(!file.exists(), "and goes once it's made");
    assert!(rig.store.storage().synced_commits > synced, "in a synced commit");
    let left: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM outgoing_drop", [], |r| r.get(0))?)).await;
    assert_eq!(left, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copys_upload_is_kept_in_a_synced_commit_before_its_message_goes() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "up.jpg", &Picture::lake(900, 600));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    // Half uploaded, and jaw's copy of your file dropped in a synced commit of its own, so the
    // upload's is the one commit left before the message goes.
    for _ in 0..1_000 {
        if partial(home.path(), UPLOADS) && rig.files(OUTGOING).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(partial(home.path(), UPLOADS), "half uploaded");
    assert!(rig.files(OUTGOING).is_empty(), "jaw's copy dropped: {:?}", rig.files(OUTGOING));
    assert!(rig.world.sends().is_empty(), "the message waits for its upload");
    // Taken and never answered, so no receipt comes to be kept meanwhile.
    rig.world.hold(true);
    let synced = rig.store.storage().synced_commits;
    rig.world.gate_uploads(false);
    rig.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(s) if s.id == id))).await;
    assert!(rig.store.storage().synced_commits > synced, "the message went before its upload reached the disk");
}

#[tokio::test(flavor = "multi_thread")]
async fn send_media_and_its_cancel_reach_the_disk_before_they_are_answered() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    // Nothing is made meanwhile, so the synced commits counted are the requests' own.
    rig.media.hold_making(true);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let synced = || rig.store.storage().synced_commits;
    let before = synced();
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    assert!(synced() > before, "send.media was answered before its messages reached the disk");
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let before = synced();
    rig.media.call("media.cancel", json!({ "message": id })).await.unwrap();
    assert!(synced() > before, "media.cancel was answered before it reached the disk");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_of_your_file_no_row_names_goes_at_the_start() {
    let home = tempfile::tempdir().unwrap();
    // As a stop between keeping a file and the commit naming it leaves one.
    std::fs::create_dir_all(home.path().join(OUTGOING)).unwrap();
    let left = home.path().join(OUTGOING).join("3EB0STOPPED.jpg");
    std::fs::write(&left, b"a copy of yours no message names").unwrap();
    let rig = Rig::on(home.path(), world()).await;
    assert!(!left.exists(), "the start removes it before anything runs");
    assert_eq!(rig.media.status().orphans_removed, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_send_deleted_for_everyone_from_your_phone_takes_its_hd_copy_with_it() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.refuse_next(1);
    let path = yours(picked.path(), "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&id, "refused", failed).await;
    let revoked = Amend {
        chat: ANA.into(),
        target: id.clone(),
        sender: None,
        mine: true,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    };
    rig.store.apply(Lane::Live, vec![Change::Amend(revoked)]).await.unwrap();
    let rows = |db: &Connection| -> anyhow::Result<(i64, i64)> {
        Ok((
            db.query_row("SELECT count(*) FROM outbox WHERE kind NOT IN ('app_state', 'revoke')", [], |r| r.get(0))?,
            db.query_row("SELECT count(*) FROM outgoing", [], |r| r.get(0))?,
        ))
    };
    assert_eq!(rig.query(rows).await, (0, 0), "nothing of it is left to send, nor its HD copy");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sends: Vec<String> = rig.world.sends().into_iter().map(|s| s.id).collect();
    assert_eq!(sends, [id], "its HD copy never goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hd_copy_waits_while_whatsapp_has_refused_its_message_and_goes_after_it_once_sent_again() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.refuse_next(1);
    let path = yours(picked.path(), "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    rig.until(&id, "refused", failed).await;
    let hd_ready = |db: &Connection| -> anyhow::Result<bool> {
        Ok(db.query_row("SELECT count(*) FROM outgoing WHERE variant = 'hd' AND state = 'ready'", [], |r| {
            r.get::<_, i64>(0)
        })? == 1)
    };
    for _ in 0..1_000 {
        if rig.query(hd_ready).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(rig.query(hd_ready).await, "its HD copy made and uploaded meanwhile");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let sends = || rig.world.sends().into_iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(sends(), std::slice::from_ref(&id), "the HD copy doesn't go while its message hasn't");
    rig.call("send.retry", json!({ "message": id })).await.unwrap();
    let went = tokio::time::timeout(Duration::from_secs(30), async {
        while sends().len() < 3 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    went.await.expect("the message and its HD copy went");
    let order = sends();
    assert_eq!(order[..2], [id.clone(), id.clone()], "refused, then sent again: {order:?}");
    assert_ne!(order[2], id, "then its HD copy");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_whose_message_was_deleted_for_you_leaves_the_cache_once_it_has_gone() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let made = rig.until(&id, "its copy made", |row| row["media"]["state"] == "local").await;
    let file = home.path().join(made["media"]["path"].as_str().unwrap());
    // Deleted for you on its way, it goes all the same, and only the copy on its way names its file.
    rig.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    rig.world.gate_uploads(false);
    let went = tokio::time::timeout(Duration::from_secs(30), async {
        while rig.sent(&id).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    went.await.expect("it went");
    for _ in 0..500 {
        rig.media.inner.clean.notify_one();
        if !file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!file.exists(), "once it has gone, nothing names its file, which goes with the cache far from full");
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_the_lanes_and_whats_on_its_way() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.media.hold_making(true);
    let path = yours(picked.path(), "a.jpg", &Picture::lake(640, 480));
    let answer = rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }] })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let status = rig.media.status();
    let size = std::fs::metadata(&path).unwrap().len();
    assert_eq!(
        (status.preparing, status.uploading, status.unsent, status.kept_files, status.kept_bytes),
        (1, 0, 0, 1, size)
    );
    let names: Vec<String> = rig.media.lanes().iter().map(|lane| lane.report().name).collect();
    assert_eq!(names, ["downloads", "images", "transcodes", "uploads"]);
    rig.media.call("media.cancel", json!({ "message": id })).await.unwrap();
    rig.until(&id, "failed", failed).await;
    let status = rig.media.status();
    assert_eq!((status.preparing, status.unsent, status.kept_files), (0, 1, 1));
}

const SCENE_HOME: &str = "JAW_MEDIA_OUT_TEST_HOME";
const SCENE: &str = "JAW_MEDIA_OUT_TEST_SCENE";
const SCENE_PICKED: &str = "JAW_MEDIA_OUT_TEST_PICKED";

/// Runs `scene` of jawd's on `home` in a process of its own, this test binary again, and kills it
/// with SIGKILL the moment it says it has got to where the scene stops, as a crash or a power cut
/// stops jawd mid-step. Returns what it said there.
async fn killed(home: &Path, picked: &Path, scene: &str) -> String {
    use std::io::{BufRead, BufReader};
    let (home, picked, scene) = (home.to_path_buf(), picked.to_path_buf(), scene.to_string());
    tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["media::outgoing::tests::a_scene_of_media_out_to_be_killed", "--exact", "--ignored", "--nocapture"])
            .env(SCENE_HOME, &home)
            .env(SCENE_PICKED, &picked)
            .env(SCENE, &scene)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as libc::pid_t;
        let watchdog = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(90));
            // SAFETY: kill only sends a signal to the child this test started.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        });
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let said = lines.map_while(Result::ok).find_map(|line| line.strip_prefix("AT ").map(str::to_string));
        child.kill().unwrap();
        child.wait().unwrap();
        drop(watchdog);
        said.unwrap_or_else(|| panic!("the scene {scene} never got to where it stops"))
    })
    .await
    .unwrap()
}

fn at(what: &str) {
    println!("AT {what}");
    std::io::stdout().flush().unwrap();
}

/// Whether any of jaw's media folders holds a partial file, as a job killed halfway leaves one.
fn partial(home: &Path, folder: &str) -> bool {
    std::fs::read_dir(home.join(folder))
        .map(|entries| entries.filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().ends_with(".part")))
        .unwrap_or(false)
}

/// The scenes `killed` runs, each in a process of its own; run on its own, it does nothing.
#[test]
#[ignore = "run by the kill tests, in a process of their own"]
fn a_scene_of_media_out_to_be_killed() {
    let (Ok(home), Ok(picked), Ok(scene)) =
        (std::env::var(SCENE_HOME), std::env::var(SCENE_PICKED), std::env::var(SCENE))
    else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
    runtime.block_on(async move {
        let (home, picked) = (Path::new(&home), Path::new(&picked));
        let world = world();
        let rig = Rig::on(home, world).await;
        let send = |files: Value| rig.call("send.media", json!({ "chat": ANA, "files": files }));
        match scene.as_str() {
            "kept" => {
                rig.media.hold_making(true);
                let path = yours(picked, "kept.jpg", &Picture::lake(900, 600));
                let answer = send(json!([{ "path": path }])).await.unwrap();
                at(answer["messages"][0].as_str().unwrap());
            }
            "making" => {
                let video = made_by_ffmpeg(
                    picked,
                    "long.mov",
                    &["-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30:d=20", "-c:v", "libx264", "-preset", "ultrafast"],
                );
                let answer = send(json!([{ "path": video }])).await.unwrap();
                loop {
                    if partial(home, OUTGOING) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                at(answer["messages"][0].as_str().unwrap());
            }
            "uploading" => {
                rig.world.gate_uploads(true);
                let path = yours(picked, "up.jpg", &Picture::lake(900, 600));
                let answer = send(json!([{ "path": path }])).await.unwrap();
                loop {
                    if partial(home, UPLOADS) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                at(answer["messages"][0].as_str().unwrap());
            }
            "sending" => {
                rig.world.hold(true);
                let path = yours(picked, "sent.jpg", &Picture::lake(900, 600));
                let answer = send(json!([{ "path": path }])).await.unwrap();
                rig.world.until(|asked| asked.iter().any(|k| matches!(k, pb::command::Kind::Send(_)))).await;
                at(answer["messages"][0].as_str().unwrap());
            }
            _ => {}
        }
        std::future::pending::<()>().await;
    });
}

/// After a kill: nothing half written is left once jawd starts again, and the message goes, once,
/// with the ID it was given and what its file is.
async fn after_the_kill(home: &Path, id: &str, made_again: bool) -> Rig {
    let rig = Rig::on(home, world()).await;
    assert!(!partial(home, OUTGOING) && !partial(home, UPLOADS), "a start removes what a kill left half written");
    rig.until(id, "sent after the restart", sent).await;
    let sends = rig.world.sends();
    assert_eq!(sends.iter().filter(|s| s.id == id).count(), 1, "sent once, with its own ID: {sends:?}");
    let [message] = rig.sent(id).try_into().unwrap();
    let raw = message.encode_to_vec();
    assert!(!raw.is_empty());
    assert_eq!(rig.world.uploads().is_empty(), !made_again, "uploaded again only if it hadn't been");
    for _ in 0..300 {
        if rig.files(OUTGOING).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(rig.files(OUTGOING).is_empty(), "jaw's copy of your file went once it was sent: {:?}", rig.files(OUTGOING));
    rig
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kill_once_send_media_has_answered_makes_and_sends_it_from_jaws_copy_at_the_next_start() {
    let (home, picked) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = killed(home.path(), picked.path(), "kept").await;
    // The photo you picked goes; jaw's copy of it doesn't.
    for entry in std::fs::read_dir(picked.path()).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let rig = after_the_kill(home.path(), &id, true).await;
    assert_eq!(rig.world.uploads().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_refused_once_its_copy_is_ready_at_the_next_start_fails_since_it_cant_have_gone_before() {
    let (home, picked) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = killed(home.path(), picked.path(), "kept").await;
    let world = world();
    world.refuse_next(1);
    let rig = Rig::on(home.path(), world).await;
    // Queued before the kill, it waited for its copy in this run, so nothing sent it before.
    let row = rig.until(&id, "failed, not waiting to hear whether it went", failed).await;
    assert_eq!(rig.world.uploads().len(), 1, "{row}");
    assert_eq!(rig.world.sends().iter().filter(|s| s.id == id).count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kill_while_a_copy_is_made_leaves_a_partial_file_that_the_next_start_removes_and_makes_again() {
    if !has_ffmpeg() {
        return;
    }
    let (home, picked) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = killed(home.path(), picked.path(), "making").await;
    assert!(partial(home.path(), OUTGOING), "the kill came while ffmpeg wrote the copy");
    let rig = after_the_kill(home.path(), &id, true).await;
    assert!(rig.media.status().partial_removed >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kill_mid_upload_leaves_the_engines_partial_file_that_the_next_start_removes_and_uploads_again() {
    let (home, picked) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = killed(home.path(), picked.path(), "uploading").await;
    assert!(partial(home.path(), UPLOADS), "the kill came mid-upload");
    let rig = after_the_kill(home.path(), &id, true).await;
    assert!(rig.media.status().partial_removed >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kill_once_its_file_is_uploaded_sends_the_same_message_again_and_uploads_nothing() {
    let (home, picked) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let id = killed(home.path(), picked.path(), "sending").await;
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    let raw: Vec<u8> = db.query_row("SELECT raw FROM outbox WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
    drop(db);
    let rig = after_the_kill(home.path(), &id, false).await;
    let sent = rig.world.sends().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!(sent.raw, raw, "what went before the kill, its keys and all, goes again");
    let image = image(&e2e::Message::decode(raw.as_slice()).unwrap()).clone();
    assert!(image.direct_path.is_some() && image.media_key.is_some(), "it was uploaded before the kill");
}

/// A photo sent in HD, both copies made and held half uploaded, then the SD copy marked uploaded in
/// jaw.db from outside, so its message goes while its HD copy is still on its way.
async fn sd_gone_and_hd_on_its_way(rig: &Rig, picked: &Path) -> String {
    rig.world.gate_uploads(true);
    let path = yours(picked, "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let both = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.world.uploads().len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    both.await.expect("both copies made and on their way up");
    let db = Connection::open(rig.home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let marked =
        db.execute("UPDATE outgoing SET state = 'ready' WHERE variant = 'sd' AND state = 'uploading'", []).unwrap();
    assert_eq!(marked, 1);
    rig.media.inner.sending.changed();
    rig.until(&id, "sent", sent).await;
    id
}

/// What the demo's WhatsApp was sent, in order, each as a message, its HD copy or a deletion.
fn kinds_sent(rig: &Rig) -> Vec<(String, &'static str)> {
    rig.world
        .sends()
        .into_iter()
        .map(|s| {
            let m = e2e::Message::decode(s.raw.as_slice()).unwrap();
            let kind = if m.protocol_message.is_some() {
                "revoke"
            } else if m.associated_child_message.is_some() {
                "hd"
            } else {
                "message"
            };
            (s.id, kind)
        })
        .collect()
}

/// Message `id` of yours deleted for everyone from your phone.
fn revoked_from_your_phone(id: &str) -> Change {
    Change::Amend(Amend {
        chat: ANA.into(),
        target: id.into(),
        sender: None,
        mine: true,
        at: now_ms(),
        what: Amendment::Revoke,
        raw: None,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hd_copy_never_goes_once_its_sent_message_is_deleted_for_everyone_from_your_phone() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let id = sd_gone_and_hd_on_its_way(&rig, picked.path()).await;
    rig.store.apply(Lane::Live, vec![revoked_from_your_phone(&id)]).await.unwrap();
    let hd: i64 =
        rig.query(|db| Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'hd'", [], |r| r.get(0))?)).await;
    eprintln!("AUDIT-A: HD rows left once the message is deleted for everyone: {hd}");
    rig.world.gate_uploads(false);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let kinds = kinds_sent(&rig);
    eprintln!("AUDIT-A: sent {kinds:?}");
    assert_eq!(kinds.len(), 1, "the HD copy of a message deleted for everyone went: {kinds:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn send_retry_never_sends_the_failed_hd_copy_of_a_message_deleted_for_everyone() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let both = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.world.uploads().len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    both.await.expect("both copies made and on their way up");
    // The HD copy's upload refused for good, as Unsent leaves it, and the SD copy's done.
    let db = Connection::open(rig.home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute_batch(
        "UPDATE outgoing SET state = 'failed', error = 'refused in the audit' WHERE variant = 'hd';
         UPDATE outbox SET state = 'failed', error = 'refused in the audit' WHERE kind = 'hd';
         UPDATE outgoing SET state = 'ready' WHERE variant = 'sd';",
    )
    .unwrap();
    rig.media.inner.sending.changed();
    rig.until(&id, "sent", sent).await;
    rig.store.apply(Lane::Live, vec![revoked_from_your_phone(&id)]).await.unwrap();
    let hd: i64 =
        rig.query(|db| Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'hd'", [], |r| r.get(0))?)).await;
    eprintln!("AUDIT-B: failed HD rows left once the message is deleted for everyone: {hd}");
    rig.world.gate_uploads(false);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let retried = rig.call("send.retry", json!({ "message": id })).await;
    eprintln!("AUDIT-B: send.retry answered {retried:?}");
    tokio::time::sleep(Duration::from_secs(3)).await;
    let kinds = kinds_sent(&rig);
    eprintln!("AUDIT-B: sent {kinds:?}");
    assert_eq!(kinds.len(), 1, "send.retry sent the HD copy of a message deleted for everyone: {kinds:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn your_deletion_for_everyone_takes_the_hd_copy_on_its_way_which_never_goes() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let id = sd_gone_and_hd_on_its_way(&rig, picked.path()).await;
    let deleted = rig.call("delete", json!({ "message": id, "for": "everyone" })).await;
    eprintln!("AUDIT-C: delete answered {deleted:?}");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = kinds_sent(&rig);
    eprintln!("AUDIT-C: sent while the HD copy waits: {before:?}");
    rig.world.gate_uploads(false);
    let went = tokio::time::timeout(Duration::from_secs(20), async {
        while kinds_sent(&rig).len() < 3 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    let _ = went.await;
    let order = kinds_sent(&rig);
    eprintln!("AUDIT-C: sent in order {order:?}");
    let kinds: Vec<&str> = order.iter().map(|(_, k)| *k).collect();
    assert_eq!(kinds, ["message", "revoke"], "the deletion waited behind the HD copy, which went first");
}

#[tokio::test(flavor = "multi_thread")]
async fn your_deletion_for_everyone_goes_at_once_rather_than_wait_behind_the_hd_copy_it_takes() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let id = sd_gone_and_hd_on_its_way(&rig, picked.path()).await;
    // The HD copy's upload stays held, so its row, waiting for it, keeps the front of the chat's queue.
    rig.call("delete", json!({ "message": id, "for": "everyone" })).await.unwrap();
    let went = tokio::time::timeout(Duration::from_secs(10), async {
        while !kinds_sent(&rig).iter().any(|(_, kind)| *kind == "revoke") {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    went.await.expect("the deletion went while the HD copy it took was still held half uploaded");
    let kinds: Vec<&str> = kinds_sent(&rig).iter().map(|(_, kind)| *kind).collect();
    assert_eq!(kinds, ["message", "revoke"]);
    rig.world.gate_uploads(false);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hd_copy_whose_message_is_deleted_for_everyone_is_withdrawn_whatever_left_its_row() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let id = sd_gone_and_hd_on_its_way(&rig, picked.path()).await;
    // Deleted for everyone by something that leaves its HD copy's row, as nothing in jaw.db does now.
    let db = Connection::open(rig.home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("UPDATE messages SET deleted = 1 WHERE id = ?1", [&id]).unwrap();
    rig.world.gate_uploads(false);
    let gone = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let hd: i64 = rig
                .query(|db| Ok(db.query_row("SELECT count(*) FROM outbox WHERE kind = 'hd'", [], |r| r.get(0))?))
                .await;
            if hd == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    gone.await.expect("its row went once its copy was uploaded");
    assert_eq!(kinds_sent(&rig), [(id, "message")], "the HD copy of a message deleted for everyone was sent");
}

#[tokio::test(flavor = "multi_thread")]
async fn send_retry_refuses_the_failed_hd_copy_of_a_message_deleted_for_everyone() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    let id = sd_gone_and_hd_on_its_way(&rig, picked.path()).await;
    let db = Connection::open(rig.home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    // Its HD copy refused, and the message deleted for everyone by something that leaves the HD
    // copy's row, as nothing in jaw.db does now.
    db.execute_batch(&format!(
        "UPDATE outgoing SET state = 'failed', error = 'refused by the test' WHERE variant = 'hd';
         UPDATE outbox SET state = 'failed', error = 'refused by the test' WHERE kind = 'hd';
         UPDATE messages SET deleted = 1 WHERE id = '{id}';"
    ))
    .unwrap();
    rig.world.gate_uploads(false);
    let refused = rig.call("send.retry", json!({ "message": id })).await.unwrap_err();
    assert_eq!(refused.code, jaw_proto::code::NOT_ALLOWED, "{refused:?}");
    let state: String =
        rig.query(|db| Ok(db.query_row("SELECT state FROM outbox WHERE kind = 'hd'", [], |r| r.get(0))?)).await;
    assert_eq!(state, "failed", "nor is it queued again");
    assert_eq!(kinds_sent(&rig), [(id, "message")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_hd_copy_whose_sent_message_is_deleted_for_you_leaves_nothing() {
    let home = tempfile::tempdir().unwrap();
    let picked = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), world()).await;
    rig.world.gate_uploads(true);
    let path = yours(picked.path(), "big.jpg", &Picture::lake(2400, 1800));
    let answer =
        rig.call("send.media", json!({ "chat": ANA, "files": [{ "path": path }], "quality": "hd" })).await.unwrap();
    let id = answer["messages"][0].as_str().unwrap().to_string();
    let both = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.world.uploads().len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    both.await.expect("both copies made and on their way up");
    let db = Connection::open(rig.home.join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute_batch(
        "UPDATE outgoing SET state = 'failed', error = 'refused in the audit' WHERE variant = 'hd';
         UPDATE outbox SET state = 'failed', error = 'refused in the audit' WHERE kind = 'hd';
         UPDATE outgoing SET state = 'ready' WHERE variant = 'sd';",
    )
    .unwrap();
    rig.media.inner.sending.changed();
    rig.until(&id, "sent", sent).await;
    rig.world.gate_uploads(false);
    rig.call("delete", json!({ "message": id, "for": "me" })).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let left = rig
        .query(|db| {
            Ok((
                db.query_row("SELECT count(*) FROM outbox WHERE kind = 'hd'", [], |r| r.get::<_, i64>(0))?,
                db.query_row("SELECT count(*) FROM outgoing", [], |r| r.get::<_, i64>(0))?,
                db.query_row(
                    "SELECT count(*) FROM outbox WHERE NOT (state = 'failed' AND kind IN ('reaction', 'edit', 'revoke', 'vote'))",
                    [],
                    |r| r.get::<_, i64>(0),
                )?,
            ))
        })
        .await;
    let retried = rig.call("send.retry", json!({ "message": id })).await;
    eprintln!(
        "AUDIT-E: HD rows, outgoing rows and rows counted toward the outbox's bound left: {left:?}; send.retry {retried:?}"
    );
    assert_eq!(left, (0, 0, 0), "a failed HD copy of a message deleted for you stays, out of send.retry's reach");
}
