//! Media in, end to end: messages from the demo engine through the converter and the store, their
//! files downloaded into the cache by the demo's media server, and their rows as interfaces read
//! them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use prost::Message as _;
use rusqlite::Connection;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use wa_proto::wa_common::MessageKey;
use wa_proto::wa_web_protobufs_e2e as e2e;

use super::*;
use crate::engine::demo::{self, DemoEngine, Phone, Picture, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::store::{Change, Lane, Message, read};

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";

/// jawd's media, store and sync on a folder of their own, with the demo engine.
struct Rig {
    home: tempfile::TempDir,
    store: Store,
    world: Arc<World>,
    media: Media,
}

fn quick() -> Config {
    Config {
        phone_wait: Duration::from_millis(400),
        retry_first: Duration::from_millis(50),
        retry_most: Duration::from_millis(200),
        keep_free: 0,
        ..Config::default()
    }
}

async fn rig(config: Config) -> Rig {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    crate::sync::running(&store).await.unwrap();
    rig_on(home, store, config).await
}

async fn rig_on(home: tempfile::TempDir, store: Store, config: Config) -> Rig {
    let engine = DemoEngine::with(vec![demo::connected()]);
    let world = engine.world();
    let (events, incoming) = mpsc::channel(1024);
    let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
    let live: crate::sync::Shared = Arc::default();
    let (pushes, _) = broadcast::channel(256);
    let pipeline = jaw_media::Media::new(jaw_media::Config {
        previews: jaw_media::Durability::Derived,
        ..jaw_media::Config::default()
    });
    let media =
        Media::start_with(store.clone(), Some(supervisor.clone()), home.path(), &live, &pushes, config, pipeline)
            .await
            .unwrap();
    let sync = crate::sync::run(
        incoming,
        store.clone(),
        live,
        pushes,
        Some(supervisor),
        home.path().to_path_buf(),
        Default::default(),
    );
    tokio::spawn(sync);
    Rig { home, store, world, media }
}

impl Rig {
    /// Message `id` in `chat`, as messages.watch sends it.
    async fn row(&self, chat: &'static str, id: &str) -> Value {
        let id = id.to_string();
        let row = self
            .store
            .reads()
            .query(Priority::Interactive, move |db| Ok(read::message(db, chat, &id)?.map(|(row, _)| row)))
            .await
            .unwrap();
        row.map_or(Value::Null, |row| serde_json::to_value(crate::feed::message_row(row)).unwrap())
    }

    /// Waits until message `id`'s row says what `done` wants, and returns it.
    async fn until(&self, chat: &'static str, id: &str, what: &str, done: impl Fn(&Value) -> bool) -> Value {
        let mut committed = self.store.committed();
        let waited = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                let row = self.row(chat, id).await;
                if done(&row) {
                    return row;
                }
                let _ = tokio::time::timeout(Duration::from_millis(100), committed.changed()).await;
            }
        });
        match waited.await {
            Ok(row) => row,
            Err(_) => panic!("{id} never came to {what}; it is {}", self.row(chat, id).await),
        }
    }

    async fn query<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> T {
        self.store.reads().query(Priority::Interactive, query).await.unwrap()
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, jaw_proto::Error> {
        self.media.call(method, params).await
    }

    fn file(&self, relative: &Value) -> std::path::PathBuf {
        self.home.path().join(relative.as_str().expect("a path"))
    }

    async fn photo(&self, picture: &Picture, chat: &str, sender: &str, id: &str) {
        let at = now_ms();
        assert!(self.world.deliver(self.world.photo(picture, chat, sender, id, at, "")).await.is_some());
    }
}

fn local(row: &Value) -> bool {
    row["media"]["state"] == "local"
}

/// Half downloaded, where the demo's download stops while downloads are gated.
fn half(row: &Value) -> bool {
    row["media"]["progress"].as_f64().is_some_and(|p| (p - 0.5).abs() < 0.01)
}

fn previewed(row: &Value) -> bool {
    local(row) && row["media"]["previews"]["256"].is_string() && row["media"]["previews"]["768"].is_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_photo_arriving_shows_its_thumbnail_then_its_file_and_previews_each_whole_once_named() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let lake = Picture::lake(1200, 900);
    rig.photo(&lake, ANA, ANA, "3EB0PHOTO1").await;
    let row = rig.until(ANA, "3EB0PHOTO1", "its thumbnail", |row| row["media"]["thumbnail"].is_string()).await;
    assert_eq!((&row["kind"], &row["media"]["w"], &row["media"]["h"]), (&json!("image"), &json!(1200), &json!(900)));
    assert!(row["media"]["previews"].is_null() && row["media"]["path"].is_null(), "nothing more yet: {row}");
    let thumbnail = image::open(rig.file(&row["media"]["thumbnail"])).unwrap();
    assert_eq!((thumbnail.width(), thumbnail.height()), (256, 192), "the thumbnail drawn at 256 pixels");
    let row = rig.until(ANA, "3EB0PHOTO1", "half downloaded", half).await;
    assert_eq!(row["media"]["state"], "downloading", "half way, as the demo's download stops there: {row}");

    rig.world.gate_downloads(false);
    let row = rig.until(ANA, "3EB0PHOTO1", "its previews", previewed).await;
    let file = std::fs::read(rig.file(&row["media"]["path"])).unwrap();
    assert_eq!(file, *lake.data, "the file is the photo, whole");
    let name = Path::new(row["media"]["path"].as_str().unwrap()).file_name().unwrap().to_str().unwrap().to_string();
    assert_eq!(name, format!("{}.jpg", to_hex(&lake.sha256)), "named by its content");
    let (small, large) = (
        image::open(rig.file(&row["media"]["previews"]["256"])).unwrap(),
        image::open(rig.file(&row["media"]["previews"]["768"])).unwrap(),
    );
    assert_eq!((small.width(), small.height(), large.width(), large.height()), (256, 192, 768, 576));
    assert!(row["media"].get("progress").is_none(), "{row}");
    let wanted: i64 = rig
        .query(|db| Ok(db.query_row("SELECT count(*) FROM media WHERE wanted IS NOT NULL", [], |r| r.get(0))?))
        .await;
    assert_eq!(wanted, 0, "nothing more to do");
    let status = rig.media.status();
    let on_disk = std::fs::metadata(rig.file(&row["media"]["path"])).unwrap().len()
        + [&row["media"]["previews"]["256"], &row["media"]["previews"]["768"], &row["media"]["thumbnail"]]
            .iter()
            .map(|p| std::fs::metadata(rig.file(p)).unwrap().len())
            .sum::<u64>();
    assert_eq!((status.cache_files, status.cache_bytes), (1, on_disk), "the cache counts every byte of it");
    assert_eq!(rig.world.downloads(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_video_or_document_downloads_by_itself_only_below_the_size_set_and_when_asked() {
    let rig = rig(Config { auto_bytes: 50_000, ..quick() }).await;
    let big = Picture::numbered(800, 600, 0);
    assert!(big.data.len() > 50_000);
    let small = Picture::lake(320, 240);
    assert!(small.data.len() < 50_000);
    for (id, picture) in [("3EB0BIG", &big), ("3EB0SMALL", &small)] {
        let mut event = rig.world.photo(picture, ANA, ANA, id, now_ms(), "");
        // Sent as a document, which downloads by itself only below the size set.
        if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
            let photo = e2e::Message::decode(message.raw.as_slice()).unwrap().image_message.unwrap();
            let document = e2e::DocumentMessage {
                mimetype: photo.mimetype,
                file_sha256: photo.file_sha256,
                file_length: photo.file_length,
                media_key: photo.media_key,
                file_enc_sha256: photo.file_enc_sha256,
                direct_path: photo.direct_path,
                file_name: Some(format!("{id}.jpg")),
                ..Default::default()
            };
            message.raw =
                e2e::Message { document_message: Some(Box::new(document)), ..Default::default() }.encode_to_vec();
        }
        rig.world.deliver(event).await.unwrap();
    }
    rig.until(ANA, "3EB0SMALL", "downloaded", local).await;
    let mut settled = false;
    for _ in 0..100 {
        let left: bool = rig
            .query(|db| {
                Ok(db.query_row(
                    "SELECT md.wanted IS NULL FROM media md JOIN messages m ON m.n = md.message WHERE m.id = '3EB0BIG'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .await;
        if left {
            settled = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(settled, "the big one is looked at and left");
    let row = rig.row(ANA, "3EB0BIG").await;
    assert_eq!((&row["kind"], &row["media"]["state"]), (&json!("document"), &json!("remote")), "{row}");
    assert_eq!(rig.world.downloads(), 1, "only the small one downloaded by itself");

    rig.call("media.fetch", json!({ "message": "3EB0BIG" })).await.unwrap();
    rig.until(ANA, "3EB0BIG", "downloaded once asked", local).await;
    assert_eq!(rig.world.downloads(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_messages_with_one_file_download_it_once_and_both_show_it() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0ONE").await;
    rig.until(ANA, "3EB0ONE", "downloading", |row| row["media"]["state"] == "downloading").await;
    rig.photo(&lake, RUI, RUI, "3EB0TWO").await;
    rig.until(RUI, "3EB0TWO", "downloading", |row| row["media"]["state"] == "downloading").await;
    rig.world.gate_downloads(false);
    let one = rig.until(ANA, "3EB0ONE", "its previews", previewed).await;
    let two = rig.until(RUI, "3EB0TWO", "its previews", previewed).await;
    assert_eq!(one["media"]["path"], two["media"]["path"]);
    assert_eq!(one["media"]["previews"], two["media"]["previews"]);
    assert_eq!(rig.world.downloads(), 1, "one download for both");
    assert_eq!(rig.media.status().cache_files, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn media_save_copies_the_file_whole_and_never_over_a_file_there() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0SAVE").await;
    rig.until(ANA, "3EB0SAVE", "downloaded", local).await;
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("3EB0SAVE.jpg"), b"someone else's").unwrap();
    let saved = rig.call("media.save", json!({ "message": "3EB0SAVE", "to": out.path() })).await.unwrap();
    let path = saved["path"].as_str().unwrap();
    assert_eq!(Path::new(path).file_name().unwrap(), "3EB0SAVE (1).jpg", "a name of its own beside the one taken");
    assert_eq!(std::fs::read(path).unwrap(), *lake.data);
    assert_eq!(std::fs::read(out.path().join("3EB0SAVE.jpg")).unwrap(), b"someone else's", "left as it was");
    let named = out.path().join("lake.jpg");
    let saved = rig.call("media.save", json!({ "message": "3EB0SAVE", "to": named })).await.unwrap();
    assert_eq!(saved["path"], json!(named.display().to_string()));
    let names: Vec<String> =
        std::fs::read_dir(out.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(names.len(), 3, "no partial file is left: {names:?}");
    let refused = rig.call("media.save", json!({ "message": "3EB0SAVE", "to": "relative/place" })).await.unwrap_err();
    assert_eq!(refused.code, jaw_proto::code::INVALID_PARAMS);
    let missing = rig.call("media.save", json!({ "message": "3EB0NONE", "to": out.path() })).await.unwrap_err();
    assert_eq!(missing.code, jaw_proto::code::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn media_save_downloads_a_file_first_when_it_isnt_yet() {
    let rig = rig(Config { auto_bytes: 0, ..quick() }).await;
    let lake = Picture::lake(640, 480);
    let mut event = rig.world.photo(&lake, ANA, ANA, "3EB0LATER", now_ms(), "");
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let photo = e2e::Message::decode(message.raw.as_slice()).unwrap().image_message.unwrap();
        let video = e2e::VideoMessage {
            mimetype: Some("video/mp4".into()),
            file_sha256: photo.file_sha256,
            file_length: photo.file_length,
            media_key: photo.media_key,
            file_enc_sha256: photo.file_enc_sha256,
            direct_path: photo.direct_path,
            ..Default::default()
        };
        message.raw = e2e::Message { video_message: Some(Box::new(video)), ..Default::default() }.encode_to_vec();
    }
    rig.world.deliver(event).await.unwrap();
    rig.until(ANA, "3EB0LATER", "settled", |row| row["media"]["state"] == "remote").await;
    let out = tempfile::tempdir().unwrap();
    let saved = rig.call("media.save", json!({ "message": "3EB0LATER", "to": out.path() })).await.unwrap();
    assert_eq!(std::fs::read(saved["path"].as_str().unwrap()).unwrap(), *lake.data);
    assert!(local(&rig.row(ANA, "3EB0LATER").await));
}

#[tokio::test(flavor = "multi_thread")]
async fn media_cancel_stops_a_download_and_leaves_nothing() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0STOP").await;
    rig.until(ANA, "3EB0STOP", "half downloaded", half).await;
    let files = rig.home.path().join(FILES);
    assert_eq!(std::fs::read_dir(&files).unwrap().count(), 1, "its partial file");
    rig.call("media.cancel", json!({ "message": "3EB0STOP" })).await.unwrap();
    let row = rig.until(ANA, "3EB0STOP", "remote", |row| row["media"]["state"] == "remote").await;
    assert!(row["media"]["path"].is_null() && row["media"].get("progress").is_none(), "{row}");
    for _ in 0..100 {
        if std::fs::read_dir(&files).unwrap().count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(std::fs::read_dir(&files).unwrap().count(), 0, "the partial file went with it");
    rig.world.gate_downloads(false);
    let n: i64 = rig.query(|db| Ok(db.query_row("SELECT message FROM media", [], |r| r.get(0))?)).await;
    // A window showing it again wants it, as the read path does.
    let shown = Change::Media(Step::Want { message: n, variant: "sd", at: now_ms(), asked: false });
    rig.store.apply(Lane::Live, vec![shown]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!local(&rig.row(ANA, "3EB0STOP").await), "nothing downloads it again by itself");
    assert_eq!(rig.world.downloads(), 1);
    rig.call("media.fetch", json!({ "message": "3EB0STOP" })).await.unwrap();
    rig.until(ANA, "3EB0STOP", "downloaded once asked", local).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_gone_from_the_server_is_asked_of_the_phone_and_downloaded_from_its_answer() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.photo(&lake, ANA, ANA, "3EB0GONE").await;
    let row = rig.until(ANA, "3EB0GONE", "downloaded from the phone's upload", previewed).await;
    assert_eq!(std::fs::read(rig.file(&row["media"]["path"])).unwrap(), *lake.data);
    let asked: Vec<pb::MediaRetry> = rig
        .world
        .asked()
        .into_iter()
        .filter_map(|kind| match kind {
            pb::command::Kind::MediaRetry(retry) => Some(retry),
            _ => None,
        })
        .collect();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].chat.as_str(), asked[0].id.as_str(), asked[0].from_me), (ANA, "3EB0GONE", false));
    assert_eq!(rig.world.downloads(), 2, "the first from where it was, the second from where the phone put it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_that_never_answers_leaves_the_file_expired_saying_why_and_asking_again_tries_again() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    rig.photo(&lake, ANA, ANA, "3EB0SILENT").await;
    rig.until(ANA, "3EB0SILENT", "waiting for the phone", |row| row["media"]["state"] == "waiting_for_phone").await;
    let row = rig.until(ANA, "3EB0SILENT", "expired", |row| row["media"]["state"] == "expired").await;
    assert!(row["media"]["error"].as_str().unwrap().contains("didn't answer"), "{row}");
    let waiting: i64 = rig
        .query(|db| Ok(db.query_row("SELECT count(*) FROM media WHERE wanted IS NOT NULL", [], |r| r.get(0))?))
        .await;
    assert_eq!(waiting, 0, "and nothing waits for good");

    rig.world.phone(Phone::Uploads);
    rig.call("media.fetch", json!({ "message": "3EB0SILENT" })).await.unwrap();
    rig.until(ANA, "3EB0SILENT", "downloaded once asked again", local).await;

    let lost = Picture::lake(320, 240);
    rig.world.expire(&lost.direct_path);
    rig.world.phone(Phone::Lost);
    rig.photo(&lost, ANA, ANA, "3EB0LOST").await;
    let row = rig.until(ANA, "3EB0LOST", "gone", |row| row["media"]["state"] == "gone").await;
    assert!(row["media"]["error"].as_str().unwrap().contains("no longer has"), "{row}");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_gone_from_the_server_waits_to_be_asked_for_before_the_phone_is_asked() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    let event = rig.world.photo(&lake, ANA, ANA, "3EB0OLD", now_ms() - 86_400_000, "");
    let Some(pb::event::Kind::Message(message)) = event.kind else { unreachable!() };
    let web = crate::convert::web_message(&message).unwrap();
    let me = crate::convert::Me::new([demo::ME.to_string()]);
    rig.store.apply(Lane::Background, crate::convert::message(&web, &me, false)).await.unwrap();
    let row = rig.row(ANA, "3EB0OLD").await;
    assert_eq!(row["media"]["state"], "remote", "history waits until it's shown: {row}");
    // A window showing it.
    let row = rig.until(ANA, "3EB0OLD", "expired", |row| row["media"]["state"] == "expired").await;
    assert!(row["media"]["thumbnail"].is_string() || row["media"]["error"].is_string(), "{row}");
    assert!(row["media"]["error"].as_str().unwrap().contains("ask for it"), "{row}");
    assert!(!rig.world.asked().iter().any(|k| matches!(k, pb::command::Kind::MediaRetry(_))), "the phone isn't asked");
    rig.call("media.fetch", json!({ "message": "3EB0OLD" })).await.unwrap();
    rig.until(ANA, "3EB0OLD", "downloaded once asked", local).await;
    assert!(rig.world.asked().iter().any(|k| matches!(k, pb::command::Kind::MediaRetry(_))), "asked, it is");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cache_drops_what_was_used_longest_ago_but_never_a_starred_or_unsent_file_and_only_after_a_synced_commit() {
    let rig = rig(Config { cache_bytes: 120_000, ..quick() }).await;
    let pictures: Vec<Picture> = (0..4).map(|n| Picture::lake(400 + n * 40, 300 + n * 30)).collect();
    // Held until the counter of synced commits is read, so nothing can be dropped before.
    let mut held = Vec::new();
    for picture in &pictures[1..] {
        held.push(rig.media.inner.claims.share(&picture.sha256).await);
    }
    for (n, picture) in pictures.iter().enumerate() {
        let id = format!("3EB0CACHE{n}");
        rig.photo(picture, ANA, ANA, &id).await;
        rig.until(ANA, &id, "its previews", previewed).await;
        if n == 0 {
            let star = Change::Amend(crate::store::Amend {
                chat: ANA.into(),
                target: id.clone(),
                sender: None,
                mine: true,
                at: now_ms(),
                what: crate::store::Amendment::Star { starred: true },
                raw: None,
            });
            rig.store.apply(Lane::Live, vec![star]).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(rig.media.status().evicted, 0, "nothing is dropped while a job may name it");
    let synced = rig.store.storage().synced_commits;
    drop(held);
    rig.media.inner.clean.notify_one();
    let mut status = rig.media.status();
    for _ in 0..500 {
        status = rig.media.status();
        if status.cache_bytes <= 120_000 && status.evicted >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(status.cache_bytes <= 120_000 && status.evicted >= 1, "{status:?}");
    assert!(rig.store.storage().synced_commits > synced, "dropped in a synced commit");
    let first = rig.row(ANA, "3EB0CACHE0").await;
    assert!(local(&first), "the starred one stays, though it was used longest ago: {first}");
    let second = rig.row(ANA, "3EB0CACHE1").await;
    assert_eq!(second["media"]["state"], "remote", "the next oldest went: {second}");
    let kept_files: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM media_files", [], |r| r.get(0))?)).await;
    let on_disk = std::fs::read_dir(rig.home.path().join(FILES)).unwrap().count() as i64;
    assert_eq!(kept_files, on_disk, "every file dropped is gone from the disk, and none kept is");
    let (bytes, files): (i64, i64) = rig
        .query(|db| {
            Ok(db.query_row(
                "SELECT (SELECT bytes FROM media_cache) - (SELECT sum(stored) FROM media_files), (SELECT files FROM media_cache)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .await;
    assert_eq!((bytes, files), (0, kept_files), "the cache's size is its files' as the triggers keep it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_goes_only_once_a_synced_commit_has_stopped_naming_it() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0DROP").await;
    let row = rig.until(ANA, "3EB0DROP", "its previews", previewed).await;
    let file = rig.file(&row["media"]["path"]);
    // jaw.db's write lock, held from outside, so nothing can be committed until it's let go.
    let holder = Connection::open(rig.home.path().join("jaw.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let dropping = {
        let (inner, sha256) = (rig.media.inner.clone(), lake.sha256.clone());
        tokio::spawn(async move { cache::drop_files(&inner, vec![sha256], true).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(file.exists(), "the file stays while the commit that stops naming it can't be made");
    let synced = rig.store.storage().synced_commits;
    holder.execute_batch("COMMIT").unwrap();
    assert_eq!(dropping.await.unwrap().unwrap(), 1);
    assert!(!file.exists());
    assert!(rig.store.storage().synced_commits > synced);
    let row = rig.row(ANA, "3EB0DROP").await;
    assert_eq!(row["media"]["state"], "remote");
    assert!(row["media"]["previews"].is_null() && row["media"]["thumbnail"].is_null(), "{row}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_deleted_takes_its_file_from_the_cache_unless_another_names_it() {
    let rig = rig(quick()).await;
    let (shared, alone) = (Picture::lake(640, 480), Picture::lake(320, 240));
    rig.photo(&shared, ANA, ANA, "3EB0SHARED1").await;
    rig.photo(&shared, RUI, RUI, "3EB0SHARED2").await;
    rig.photo(&alone, ANA, ANA, "3EB0ALONE").await;
    for (chat, id) in [(ANA, "3EB0SHARED1"), (RUI, "3EB0SHARED2"), (ANA, "3EB0ALONE")] {
        rig.until(chat, id, "its previews", previewed).await;
    }
    let deleted = |chat: &str, id: &str| Change::DeleteMessage { chat: chat.into(), id: id.into() };
    let revoked = Change::Amend(crate::store::Amend {
        chat: ANA.into(),
        target: "3EB0ALONE".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: crate::store::Amendment::Revoke,
        raw: None,
    });
    rig.store.apply(Lane::Live, vec![deleted(ANA, "3EB0SHARED1"), revoked]).await.unwrap();
    rig.media.inner.clean.notify_one();
    let files = rig.home.path().join(FILES);
    let count = |folder: &str| std::fs::read_dir(rig.home.path().join(folder)).unwrap().count();
    for _ in 0..200 {
        if count(FILES) == 1 && count(PREVIEWS) == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let left: Vec<String> =
        std::fs::read_dir(&files).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(left, [format!("{}.jpg", to_hex(&shared.sha256))], "the one Rui's message still names stays");
    assert!(local(&rig.row(RUI, "3EB0SHARED2").await));
    let unnamed: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM media_drop", [], |r| r.get(0))?)).await;
    assert_eq!(unnamed, 0);
    let previews: Vec<String> = std::fs::read_dir(rig.home.path().join(PREVIEWS))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let hash = to_hex(&shared.sha256);
    assert!(
        previews.len() == 3 && previews.iter().all(|p| p.starts_with(&hash)),
        "and only its previews and thumbnail: {previews:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_preview_left_empty_by_a_power_cut_reads_as_missing_and_is_made_again() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0CUT").await;
    let row = rig.until(ANA, "3EB0CUT", "its previews", previewed).await;
    let (small, thumbnail) = (rig.file(&row["media"]["previews"]["256"]), rig.file(&row["media"]["thumbnail"]));
    // As a power cut leaves files put in place without waiting for the disk.
    std::fs::write(&small, b"").unwrap();
    let cut = std::fs::read(&thumbnail).unwrap();
    std::fs::write(&thumbnail, &cut[..cut.len() / 2]).unwrap();
    rig.store.files().forget_whole();
    let row = rig.row(ANA, "3EB0CUT").await;
    assert!(row["media"]["previews"].is_null(), "an empty preview reads as missing: {row}");
    assert!(row["media"]["thumbnail"].is_null(), "and a thumbnail cut short: {row}");
    let row =
        rig.until(ANA, "3EB0CUT", "made again", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    assert!(jaw_media::whole_preview(&rig.file(&row["media"]["previews"]["256"])));
    assert!(jaw_media::whole_preview(&rig.file(&row["media"]["thumbnail"])));
    assert_eq!(rig.media.status().remade, 2);
    assert_eq!(rig.world.downloads(), 1, "made from the file in the cache, not downloaded again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_removes_partial_files_and_files_no_row_names_and_takes_up_what_a_stopped_jawd_left() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    crate::sync::running(&store).await.unwrap();
    std::fs::create_dir_all(home.path().join(FILES)).unwrap();
    std::fs::create_dir_all(home.path().join(PREVIEWS)).unwrap();
    let stray = format!("{}/{}.jpg", FILES, "ab".repeat(32));
    let partial = format!("{}/{}.jpg.123-4.part", FILES, "cd".repeat(32));
    std::fs::write(home.path().join(&stray), b"left by a crash").unwrap();
    std::fs::write(home.path().join(&partial), b"half").unwrap();
    // A photo a stopped jawd left downloading: its row says so, and it's wanted.
    let lake = Picture::lake(640, 480);
    let world_for_event = World::default();
    let event = world_for_event.photo(&lake, ANA, ANA, "3EB0LEFT", now_ms(), "");
    let Some(pb::event::Kind::Message(message)) = event.kind else { unreachable!() };
    let web = crate::convert::web_message(&message).unwrap();
    let me = crate::convert::Me::new([demo::ME.to_string()]);
    store.apply(Lane::Live, crate::convert::message(&web, &me, true)).await.unwrap();
    let n: i64 = store
        .reads()
        .query(Priority::Interactive, |db| Ok(db.query_row("SELECT message FROM media", [], |r| r.get(0))?))
        .await
        .unwrap();
    store.apply(Lane::Live, vec![Change::Media(Step::Downloading { message: n, variant: "sd" })]).await.unwrap();
    let rig = rig_on(home, store, quick()).await;
    rig.world.serve(&lake.direct_path, lake.data.clone());
    assert!(!rig.home.path().join(&partial).exists(), "the partial file is removed before anything starts");
    assert_eq!(rig.media.status().partial_removed, 1);
    let row = rig.until(ANA, "3EB0LEFT", "downloaded", previewed).await;
    assert!(rig.file(&row["media"]["path"]).exists());
    for _ in 0..100 {
        if !rig.home.path().join(&stray).exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!rig.home.path().join(&stray).exists(), "a file no row names is swept");
    assert_eq!(rig.media.status().orphans_removed, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_photo_is_never_kept_and_a_deleted_ones_file_cant_be_fetched() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    let mut event = rig.world.photo(&lake, ANA, ANA, "3EB0ONCE", now_ms(), "");
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let photo = e2e::Message::decode(message.raw.as_slice()).unwrap();
        let once = e2e::FutureProofMessage { message: Some(Box::new(photo)) };
        message.raw = e2e::Message { view_once_message: Some(Box::new(once)), ..Default::default() }.encode_to_vec();
    }
    rig.world.deliver(event).await.unwrap();
    let row = rig.until(ANA, "3EB0ONCE", "converted", |row| !row.is_null()).await;
    assert_eq!(row["view_once"], json!(true));
    let refused = rig.call("media.fetch", json!({ "message": "3EB0ONCE" })).await.unwrap_err();
    assert_eq!(refused.code, jaw_proto::code::NOT_ALLOWED);
    // Wanted however it came to be, its job settles it without downloading or drawing anything.
    let n: i64 = rig.query(|db| Ok(db.query_row("SELECT message FROM media", [], |r| r.get(0))?)).await;
    let wanted = Change::Media(Step::Want { message: n, variant: "sd", at: now_ms(), asked: true });
    rig.store.apply(Lane::Live, vec![wanted]).await.unwrap();
    for _ in 0..500 {
        let settled: bool =
            rig.query(|db| Ok(db.query_row("SELECT wanted IS NULL FROM media", [], |r| r.get(0))?)).await;
        if settled {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.world.downloads(), 0);
    assert!(rig.row(ANA, "3EB0ONCE").await["media"]["thumbnail"].is_null());
    let none = rig.call("media.fetch", json!({ "message": "3EB0NONE" })).await.unwrap_err();
    assert_eq!(none.code, jaw_proto::code::NOT_FOUND);
    let hd = rig.call("media.fetch", json!({ "message": "3EB0ONCE", "variant": "hd" })).await.unwrap_err();
    assert_eq!(hd.code, jaw_proto::code::NOT_ALLOWED);
}

/// The HD copy of message `parent`, as WhatsApp's dual upload sends it: a message of its own,
/// wrapped, naming its SD message.
fn hd_copy(world: &World, picture: &Picture, parent: &str, id: &str) -> pb::Event {
    let mut event = world.photo(picture, ANA, ANA, id, now_ms(), "");
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let mut inner = e2e::Message::decode(message.raw.as_slice()).unwrap();
        inner.message_context_info = Some(e2e::MessageContextInfo {
            message_association: Some(e2e::MessageAssociation {
                association_type: Some(e2e::message_association::AssociationType::HdImageDualUpload as i32),
                parent_message_key: Some(MessageKey {
                    remote_jid: Some(ANA.into()),
                    from_me: Some(false),
                    id: Some(parent.into()),
                    participant: None,
                }),
                message_index: None,
            }),
            ..Default::default()
        });
        let wrapped = e2e::FutureProofMessage { message: Some(Box::new(inner)) };
        message.raw =
            e2e::Message { associated_child_message: Some(Box::new(wrapped)), ..Default::default() }.encode_to_vec();
    }
    event
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hd_copy_attaches_to_its_message_whichever_comes_first_and_downloads_when_asked() {
    let rig = rig(quick()).await;
    let (sd, hd) = (Picture::lake(640, 480), Picture::lake(1600, 1200));
    rig.photo(&sd, ANA, ANA, "3EB0SD1").await;
    rig.world.deliver(hd_copy(&rig.world, &hd, "3EB0SD1", "3EB0HD1")).await.unwrap();
    // And the other way around: the HD copy first, waiting for its message.
    rig.world.deliver(hd_copy(&rig.world, &hd, "3EB0SD2", "3EB0HD2")).await.unwrap();
    rig.photo(&sd, ANA, ANA, "3EB0SD2").await;
    for id in ["3EB0SD1", "3EB0SD2"] {
        let row = rig.until(ANA, id, "its HD copy", |row| row["media"]["hd"] == json!(true) && previewed(row)).await;
        assert!(row["media"].get("hd_path").is_none(), "an HD copy downloads only when asked: {row}");
    }
    for id in ["3EB0HD1", "3EB0HD2"] {
        assert!(rig.row(ANA, id).await.is_null(), "{id} shows as no message of its own");
    }
    rig.call("media.fetch", json!({ "message": "3EB0SD1", "variant": "hd" })).await.unwrap();
    let row = rig.until(ANA, "3EB0SD1", "its HD copy downloaded", |row| row["media"]["hd_path"].is_string()).await;
    assert_eq!(std::fs::read(rig.file(&row["media"]["hd_path"])).unwrap(), *hd.data);
    assert!(local(&rig.row(ANA, "3EB0SD2").await), "the other shows the SD copy it had");
    assert_eq!(rig.row(ANA, "3EB0SD2").await["media"]["hd_path"], row["media"]["hd_path"], "and the same HD file");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flood_of_photos_never_holds_up_a_message() {
    let rig = rig(quick()).await;
    let photos: Vec<Picture> = (0..40).map(|n| Picture::lake(800 + n, 600)).collect();
    let flood = {
        let world = rig.world.clone();
        tokio::spawn(async move {
            for (n, picture) in photos.iter().enumerate() {
                world.deliver(world.photo(picture, RUI, RUI, &format!("3EB0FLOOD{n}"), now_ms(), "")).await;
            }
        })
    };
    let mut acks = Vec::new();
    for n in 0..20 {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let took = rig.world.deliver(demo::text(ANA, ANA, &format!("3EB0TEXT{n}"), now_ms(), "hi")).await.unwrap();
        acks.push(took);
    }
    flood.await.unwrap();
    acks.sort();
    let worst = acks[acks.len() - 1];
    eprintln!("a text's ack during a flood of photos: p50 {:?}, worst {worst:?}", acks[acks.len() / 2]);
    assert!(worst < Duration::from_millis(500), "a message waited {worst:?} behind photos");
    for n in 0..40 {
        rig.until(RUI, &format!("3EB0FLOOD{n}"), "its previews", previewed).await;
    }
    let lanes = rig.media.lanes();
    let downloads = lanes[0].report();
    assert_eq!((downloads.name.as_str(), downloads.workers, downloads.done), ("downloads", 6, 40));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_the_cache_and_what_waits() {
    let rig = rig(Config { phone_wait: Duration::from_secs(600), ..quick() }).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    rig.photo(&lake, ANA, ANA, "3EB0WAIT").await;
    // Its thumbnail is drawn while its file is looked for.
    rig.until(ANA, "3EB0WAIT", "waiting for the phone, with its thumbnail", |row| {
        row["media"]["state"] == "waiting_for_phone" && row["media"]["thumbnail"].is_string()
    })
    .await;
    let status = rig.media.status();
    assert_eq!(
        (status.waiting_for_phone, status.cache_limit, status.auto_bytes),
        (1, quick().cache_bytes, quick().auto_bytes)
    );
    assert_eq!(status.cache_files, 1, "its thumbnail is in the cache");
    let names: Vec<String> = rig.media.lanes().iter().map(|lane| lane.report().name).collect();
    assert_eq!(names, ["downloads", "images"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_arriving_again_changes_nothing_twice_and_fills_in_what_it_lacked() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let copy = crate::store::media::MediaCopy {
        keys: crate::store::media::Keys::Image,
        mime: Some("image/jpeg".into()),
        sha256: Some(vec![7; 32]),
        thumbnail: true,
        ..Default::default()
    };
    let message = |copy: crate::store::media::MediaCopy, live: bool| Message {
        chat: ANA.into(),
        id: "3EB0AGAIN".into(),
        at: 1_792_000_000_000,
        sender: Some(ANA.into()),
        kind: "image",
        summary: "Photo".into(),
        detail: Some(json!({ "media": { "mime": "image/jpeg" } }).to_string()),
        live,
        media: Some(Box::new(copy)),
        ..Message::default()
    };
    store.apply(Lane::Live, vec![Change::Message(message(copy.clone(), true))]).await.unwrap();
    let seq = *store.committed().borrow();
    let again = store.apply(Lane::Live, vec![Change::Message(message(copy.clone(), true))]).await.unwrap();
    assert_eq!((again.changed, *store.committed().borrow()), (0, seq), "the same copy again changes nothing");
    let fuller = crate::store::media::MediaCopy {
        direct_path: Some("/v/again".into()),
        media_key: Some(vec![1; 32]),
        sha256: Some(vec![9; 32]),
        ..copy.clone()
    };
    let filled = store.apply(Lane::Background, vec![Change::Message(message(fuller.clone(), false))]).await.unwrap();
    assert_eq!(filled.changed, 1, "a copy that says more fills in what was missing");
    let row: (Option<String>, Vec<u8>, Option<i64>) = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT direct_path, sha256, wanted FROM media", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?)
        })
        .await
        .unwrap();
    assert_eq!(
        (row.0.as_deref(), row.1, row.2.is_some()),
        (Some("/v/again"), vec![7; 32], true),
        "its hash stays, as it was wanted"
    );
    let seq = *store.committed().borrow();
    let replay = store.apply(Lane::Background, vec![Change::Message(message(fuller, false))]).await.unwrap();
    assert_eq!((replay.changed, *store.committed().borrow()), (0, seq));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_settling_never_drops_a_want_that_came_after_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let message = Message {
        chat: ANA.into(),
        id: "3EB0RACE".into(),
        at: 1,
        sender: Some(ANA.into()),
        kind: "image",
        summary: "Photo".into(),
        live: true,
        media: Some(Box::new(crate::store::media::MediaCopy { sha256: Some(vec![3; 32]), ..Default::default() })),
        ..Message::default()
    };
    store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    let read = |store: &Store| {
        let store = store.clone();
        async move {
            store
                .reads()
                .query(Priority::Interactive, |db| {
                    Ok(db.query_row("SELECT message, wanted, asked FROM media", [], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, bool>(2)?))
                    })?)
                })
                .await
                .unwrap()
        }
    };
    let (n, taken, _) = read(&store).await;
    let taken = taken.unwrap();
    // An interface asks while the job that took it runs, within the same millisecond.
    let asked = Step::Want { message: n, variant: "sd", at: taken, asked: true };
    store.apply(Lane::Live, vec![Change::Media(asked)]).await.unwrap();
    let settled = Step::Settled { message: n, variant: "sd", wanted: taken, state: "remote", error: None };
    store.apply(Lane::Live, vec![Change::Media(settled)]).await.unwrap();
    let (_, wanted, asked) = read(&store).await;
    assert_eq!((wanted, asked), (Some(taken + 1), true), "still wanted, as asked");
    let done = Step::Settled { message: n, variant: "sd", wanted: taken + 1, state: "remote", error: None };
    store.apply(Lane::Live, vec![Change::Media(done)]).await.unwrap();
    assert_eq!(read(&store).await, (n, None, false), "the job that took the asked want settles it");

    // The phone's answer comes while a job that took the copy runs, within the same millisecond.
    let shown = Step::Want { message: n, variant: "sd", at: taken + 5, asked: false };
    store.apply(Lane::Live, vec![Change::Media(shown)]).await.unwrap();
    let uploaded = Step::Answered {
        chat: ANA.into(),
        id: "3EB0RACE".into(),
        answer: crate::store::media::Answer::Uploaded("/v/t62.7118-24/again.enc".into()),
        at: taken + 5,
    };
    store.apply(Lane::Live, vec![Change::Media(uploaded)]).await.unwrap();
    let settled = Step::Settled { message: n, variant: "sd", wanted: taken + 5, state: "remote", error: None };
    store.apply(Lane::Live, vec![Change::Media(settled)]).await.unwrap();
    let (_, wanted, asked) = read(&store).await;
    assert_eq!((wanted, asked), (Some(taken + 6), true), "still wanted, to be downloaded from where the phone put it");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_caches_size_and_its_unnamed_files_are_kept_by_its_triggers() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let state = |db: &Connection| -> (i64, i64, i64) {
        db.query_row("SELECT bytes, files, (SELECT count(*) FROM media_drop) FROM media_cache", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
    };
    db.execute_batch(
        "INSERT INTO chats (id, kind, v) VALUES ('c', 'direct', 1);
        INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'a', 1, 1, 'image', 1), (1, 'b', 2, 2, 'image', 1);
        INSERT INTO media (message, variant, sha256) VALUES (1, 'sd', zeroblob(32)), (2, 'sd', zeroblob(32));
        INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (zeroblob(32), 'media/files/a.jpg', 100, 1);
        UPDATE media_files SET small = 's', large = 'l', previews_bytes = 30, thumbnail = 't', thumbnail_bytes = 5;",
    )
    .unwrap();
    assert_eq!(state(&db), (135, 1, 0));
    db.execute_batch("DELETE FROM messages WHERE n = 1").unwrap();
    assert_eq!(state(&db), (135, 1, 0), "another copy still names it");
    db.execute_batch("DELETE FROM messages WHERE n = 2").unwrap();
    assert_eq!(state(&db), (135, 1, 1), "no copy names it: it waits to go");
    db.execute_batch(
        "INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'c', 3, 3, 'image', 1);
        INSERT INTO media (message, variant, sha256) VALUES (3, 'sd', zeroblob(32));",
    )
    .unwrap();
    assert_eq!(state(&db), (135, 1, 0), "named again, it doesn't");
    db.execute_batch("UPDATE media_files SET previews_bytes = NULL; DELETE FROM media_files").unwrap();
    assert_eq!(state(&db), (0, 0, 0));
    drop(store);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_media_row_from_before_migration_11_is_kept() {
    let mut db = Connection::open_in_memory().unwrap();
    for migration in &crate::store::schema_for_tests()[..10] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 10).unwrap();
    db.execute_batch(
        "INSERT INTO chats (id, kind, v) VALUES ('c', 'direct', 1);
        INSERT INTO messages (chat, id, at, t, kind, v) VALUES (1, 'a', 1, 1, 'image', 1);
        INSERT INTO media (message, variant, mime, bytes, sha256, path, preview_small, state, opened_at)
            VALUES (1, 'sd', 'image/jpeg', 42, zeroblob(32), 'media/files/a.jpg', 'media/previews/a.256.jpg', 'here', 7);",
    )
    .unwrap();
    crate::store::migrate_for_tests(&mut db).unwrap();
    let kept: (String, i64, String, String, String, i64, i64, i64) = db
        .query_row(
            "SELECT md.mime, md.bytes, md.state, f.path, f.small, f.used_at, c.bytes, c.files
            FROM media md JOIN media_files f ON f.sha256 = md.sha256, media_cache c",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .unwrap();
    assert_eq!(
        kept,
        (
            "image/jpeg".into(),
            42,
            "remote".into(),
            "media/files/a.jpg".into(),
            "media/previews/a.256.jpg".into(),
            7,
            42,
            1
        )
    );
}
