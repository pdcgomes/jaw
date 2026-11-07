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
use crate::engine::pb::media_retried::Result as PhoneSaid;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::store::copies::{Before, COPIES, before_media, hd_child};
use crate::store::{Change, Lane, Message, read};

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";

/// How long a test's own connection to jaw.db waits for the write lock beside a running jawd,
/// whose writer, on a busy 2-core machine, can keep it more than 5 s.
const SIDE_WAITS: Duration = Duration::from_secs(60);

/// jawd's media, store and sync on a folder of their own, with the demo engine.
struct Rig {
    home: Arc<tempfile::TempDir>,
    store: Store,
    world: Arc<World>,
    media: Media,
    supervisor: Supervisor,
    live: crate::sync::Shared,
    pushes: broadcast::Sender<jaw_proto::Push>,
}

fn quick() -> Config {
    Config {
        phone_wait: Duration::from_millis(400),
        retry_first: Duration::from_millis(50),
        retry_most: Duration::from_millis(200),
        keep_free: 0,
        settle_every: Duration::from_secs(3600),
        ..Config::default()
    }
}

async fn rig(config: Config) -> Rig {
    rig_in(Arc::new(tempfile::tempdir().unwrap()), config).await
}

/// A rig on jaw's folder `home`, as a jawd started on it.
async fn rig_in(home: Arc<tempfile::TempDir>, config: Config) -> Rig {
    let store = Store::open(home.path()).unwrap();
    crate::sync::running(&store).await.unwrap();
    rig_on(home, store, config).await
}

/// Runs `jawd` on a runtime of its own, then ends it with everything it started, as a jawd that
/// stops, so another can start on the same folder.
fn as_a_jawd<T>(jawd: impl std::future::Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let out = runtime.block_on(jawd);
    runtime.shutdown_timeout(Duration::from_secs(5));
    out
}

async fn rig_on(home: Arc<tempfile::TempDir>, store: Store, config: Config) -> Rig {
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
        live.clone(),
        pushes.clone(),
        Some(supervisor.clone()),
        home.path().to_path_buf(),
        Default::default(),
        crate::feed::Typing::default(),
    );
    tokio::spawn(sync);
    Rig { home, store, world, media, supervisor, live, pushes }
}

impl Rig {
    /// jawd's socket in the rig's folder, served as jawd serves it, with windows and media.
    fn serve(&self) -> std::path::PathBuf {
        let feed = crate::feed::Feed::start(self.store.clone(), Arc::new(|| false), crate::feed::Timing::default());
        let socket = jaw_proto::local::socket(self.home.path());
        let listener = crate::sock::listen(&socket).unwrap();
        let state = crate::state::State::new(
            self.home.path(),
            Ok(self.supervisor.clone()),
            self.media.lanes(),
            self.live.clone(),
            Some(self.store.clone()),
            self.pushes.clone(),
        )
        .unwrap()
        .with_feed(feed)
        .with_media(self.media.clone());
        tokio::spawn(crate::sock::serve(Arc::new(state), listener));
        socket
    }

    /// jawd's socket in the rig's folder, served with sending as well, so view-once messages open.
    async fn serve_sending(&self) -> std::path::PathBuf {
        let feed = crate::feed::Feed::start(self.store.clone(), Arc::new(|| false), crate::feed::Timing::default());
        let attention = Arc::new(crate::attention::Attention::default());
        let sending = crate::send::Sending::start(
            self.store.clone(),
            Some(self.supervisor.clone()),
            self.live.clone(),
            &self.pushes,
            attention.focus(),
            crate::send::Timing::default(),
            crate::send::Makers { previews: crate::send::link::Previews::off(), media: Some(self.media.clone()) },
        )
        .await
        .unwrap();
        let socket = jaw_proto::local::socket(self.home.path());
        let listener = crate::sock::listen(&socket).unwrap();
        let state = crate::state::State::new(
            self.home.path(),
            Ok(self.supervisor.clone()),
            self.media.lanes(),
            self.live.clone(),
            Some(self.store.clone()),
            self.pushes.clone(),
        )
        .unwrap()
        .with_feed(feed)
        .with_attention(attention)
        .with_sending(sending)
        .with_media(self.media.clone())
        .with_expiry(crate::expiry::start(self.store.clone()));
        tokio::spawn(crate::sock::serve(Arc::new(state), listener));
        socket
    }

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

    /// How many copies are wanted.
    async fn wanted(&self) -> i64 {
        self.query(|db| Ok(db.query_row("SELECT count(*) FROM media WHERE wanted IS NOT NULL", [], |r| r.get(0))?))
            .await
    }

    /// Waits until the cleaner has made a whole pass that found nothing more to do, so whatever it
    /// was doing before is done, its files removed.
    async fn until_cleaner_idle(&self) {
        let before = self.media.inner.tidied.load(std::sync::atomic::Ordering::SeqCst);
        self.media.inner.clean.notify_one();
        let tidied = tokio::time::timeout(Duration::from_secs(60), async {
            while self.media.inner.tidied.load(std::sync::atomic::Ordering::SeqCst) == before {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(tidied.await.is_ok(), "the cleaner never finished a pass");
    }

    /// Waits until no copy is wanted, every job having settled what it took.
    async fn until_nothing_wanted(&self, what: &str) {
        let mut committed = self.store.committed();
        let waited = tokio::time::timeout(Duration::from_secs(60), async {
            while self.wanted().await > 0 {
                let _ = tokio::time::timeout(Duration::from_millis(100), committed.changed()).await;
            }
        });
        if waited.await.is_err() {
            panic!("{what}: {} copies are still wanted", self.wanted().await);
        }
    }

    /// Photo `id` in Ana's chat, as history brings it: nothing wants it until a window shows it or
    /// an interface asks for it.
    async fn photo_from_history(&self, picture: &Picture, id: &str) {
        let event = self.world.photo(picture, ANA, ANA, id, now_ms() - 86_400_000, "");
        let Some(pb::event::Kind::Message(message)) = event.kind else { unreachable!() };
        let web = crate::convert::web_message(&message).unwrap();
        let me = crate::convert::Me::new([demo::ME.to_string()]);
        self.store.apply(Lane::Background, crate::convert::message(&web, &me, false)).await.unwrap();
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
    // Its settle comes after its previews, each a write of the background lane.
    rig.until_nothing_wanted("nothing more to do").await;
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
    // Reading its row before its thumbnail was drawn wanted it again, as a window does, and the job
    // that settles that want can end after the phone's silence has left it expired, which it keeps.
    rig.until_nothing_wanted("nothing waits for good").await;
    let row = rig.row(ANA, "3EB0SILENT").await;
    assert_eq!(row["media"]["state"], "expired", "{row}");

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
async fn a_copy_waiting_for_the_phone_still_waits_after_a_job_that_had_nothing_to_fetch() {
    let rig = rig(Config { phone_wait: Duration::from_secs(60), ..quick() }).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    rig.photo(&lake, ANA, ANA, "3EB0ASKED").await;
    rig.until(ANA, "3EB0ASKED", "waiting for the phone", |row| row["media"]["state"] == "waiting_for_phone").await;
    // A window shows it lacking something, as one does before its thumbnail is drawn, so it's
    // wanted again, and its job has no file to fetch.
    let n: i64 = rig.query(|db| Ok(db.query_row("SELECT message FROM media", [], |r| r.get(0))?)).await;
    let again = Change::Media(Step::Want { message: n, variant: "sd", at: now_ms(), asked: false });
    rig.store.apply(Lane::Live, vec![again]).await.unwrap();
    rig.media.inner.schedule.wake();
    let mut done = false;
    for _ in 0..500 {
        let wanted: Option<i64> = rig
            .query(move |db| Ok(db.query_row("SELECT wanted FROM media WHERE message = ?1", [n], |r| r.get(0))?))
            .await;
        if wanted.is_none() {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(done, "the job never settled it");
    let row = rig.row(ANA, "3EB0ASKED").await;
    assert_eq!(row["media"]["state"], "waiting_for_phone", "it waits for the phone's answer, or its silence: {row}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phones_silence_noted_while_a_job_with_nothing_to_fetch_runs_is_kept() {
    let rig = rig(Config { phone_wait: Duration::from_secs(600), ..quick() }).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    rig.photo(&lake, ANA, ANA, "3EB0QUIET").await;
    rig.until(ANA, "3EB0QUIET", "waiting for the phone, with its thumbnail", |row| {
        row["media"]["state"] == "waiting_for_phone" && row["media"]["thumbnail"].is_string()
    })
    .await;
    rig.until_nothing_wanted("the job that asked the phone is done").await;
    // Its thumbnail forgotten, so the next job draws it, and every image worker held, so that job
    // waits there.
    let (n, retried, thumbnail): (i64, i64, String) = rig
        .query(|db| {
            Ok(db.query_row(
                "SELECT md.message, md.retried_at, f.thumbnail FROM media md JOIN media_files f USING (sha256)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .await;
    let forget =
        Step::Forget { sha256: lake.sha256.clone(), made: crate::store::media::Made::Thumbnail, path: thumbnail };
    rig.store.apply(Lane::Live, vec![Change::Media(forget)]).await.unwrap();
    let (open, opened) = tokio::sync::watch::channel(false);
    for worker in 0..rig.media.inner.images.report().workers {
        let mut opened = opened.clone();
        let held = async move {
            let _ = opened.wait_for(|open| *open).await;
            previews::Drawn::Nothing
        };
        let _ = rig.media.inner.images.submit(format!("held:{worker}"), Priority::Interactive, held).await;
    }
    // A window shows it, as one does a copy lacking its thumbnail, and a job takes it while it waits
    // for the phone: it has no file to fetch.
    let again = Change::Media(Step::Want { message: n, variant: "sd", at: now_ms(), asked: false });
    rig.store.apply(Lane::Live, vec![again]).await.unwrap();
    rig.media.inner.schedule.wake();
    let images = || rig.media.inner.images.report();
    for _ in 0..500 {
        if images().queued_background > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(images().queued_background, 1, "the job's thumbnail waits for a worker");
    // The phone's silence is noted meanwhile, then the job ends.
    let silent = Step::PhoneSilent { message: n, variant: "sd", before: retried };
    rig.store.apply(Lane::Live, vec![Change::Media(silent)]).await.unwrap();
    open.send(true).unwrap();
    rig.until_nothing_wanted("the job settled what it took").await;
    let row = rig.row(ANA, "3EB0QUIET").await;
    assert_eq!(row["media"]["state"], "expired", "the job left it as the phone's silence did: {row}");
    assert!(row["media"]["error"].as_str().unwrap().contains("didn't answer"), "{row}");
    assert!(row["media"]["thumbnail"].is_string(), "{row}");
    assert_eq!(rig.media.status().waiting_for_phone, 0, "nothing waits for an answer no one will wake for");
}

#[tokio::test(flavor = "multi_thread")]
async fn history_gone_from_the_server_waits_to_be_asked_for_before_the_phone_is_asked() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.photo_from_history(&lake, "3EB0OLD").await;
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
    // Room for every file until the test lets go of the files it holds as jobs would, so no pass of
    // the cleaner begins while they're held, which would step past them and drop newer ones.
    let limit = Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX));
    let rig = rig(Config { cache_bytes: 120_000, limit: Some(limit.clone()), ..quick() }).await;
    // Rows read here want nothing, as if no window showed them, so no want lands after its file has
    // gone and downloads it again.
    rig.store.files().watch(mpsc::channel(1).0);
    let pictures: Vec<Picture> = (0..4).map(|n| Picture::lake(400 + n * 40, 300 + n * 30)).collect();
    // Held until the counter of synced commits is read, and the cache then given its size.
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
    rig.until_nothing_wanted("every job settled").await;
    assert_eq!(rig.media.status().evicted, 0, "nothing is dropped while the cache has room");
    let synced = rig.store.storage().synced_commits;
    drop(held);
    limit.store(120_000, std::sync::atomic::Ordering::Relaxed);
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
    // A drop removes its files after its commit, and a pass may drop twice: counted once it's done.
    rig.until_cleaner_idle().await;
    let kept_files: i64 = rig.query(|db| Ok(db.query_row("SELECT count(*) FROM media_files", [], |r| r.get(0))?)).await;
    let on_disk = std::fs::read_dir(rig.home.path().join(FILES)).unwrap().count() as i64;
    assert_eq!(kept_files, on_disk, "every file dropped is gone from the disk, and none kept is");
    let second: (String, bool) = rig
        .query(|db| {
            Ok(db.query_row(
                "SELECT md.state, f.path IS NOT NULL FROM messages m JOIN media md ON md.message = m.n
                LEFT JOIN media_files f ON f.sha256 = md.sha256 WHERE m.id = '3EB0CACHE1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .await;
    assert_eq!(second, ("remote".to_string(), false), "the next oldest went");
    let first = rig.row(ANA, "3EB0CACHE0").await;
    assert!(local(&first), "the starred one stays, though it was used longest ago: {first}");
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
async fn the_cache_keeps_to_its_size_past_the_starred_files_it_cant_drop() {
    let rig = rig(Config { cache_bytes: 1_000_000, ..quick() }).await;
    let db = Connection::open(rig.home.path().join("jaw.db")).unwrap();
    db.busy_timeout(SIDE_WAITS).unwrap();
    db.execute("INSERT INTO chats (id, kind, v) VALUES (?1, 'direct', 1)", [ANA]).unwrap();
    // Four starred photos used longest ago, 1.2 MB between them, more than the cache holds; then
    // ten others of 50 kB.
    for n in 0..14u8 {
        let (starred, bytes) = if n < 4 { (true, 300_000) } else { (false, 50_000) };
        let sha256 = vec![n + 1; 32];
        let path = format!("{FILES}/{}.jpg", to_hex(&sha256));
        db.execute(
            "INSERT INTO messages (chat, id, at, t, kind, starred, v)
            VALUES ((SELECT n FROM chats WHERE id = ?1), ?2, ?3, ?3, 'image', ?4, 1)",
            rusqlite::params![ANA, format!("3EB0KEPT{n}"), i64::from(n), starred],
        )
        .unwrap();
        db.execute(
            "INSERT INTO media (message, variant, keys, sha256) VALUES (last_insert_rowid(), 'sd', 'image', ?1)",
            [&sha256],
        )
        .unwrap();
        db.execute(
            "INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![sha256, path, bytes as i64, i64::from(n)],
        )
        .unwrap();
        // After its row, as the sweep jawd starts with removes a file no row names.
        std::fs::write(rig.home.path().join(&path), vec![0u8; bytes]).unwrap();
    }
    // The evictions are counted in memory and the cache's size read from jaw.db, so one status can
    // have the first without the second; it waits for both, or for more evictions than it may make.
    let end = (10, 4, 1_200_000);
    let mut status = rig.media.status();
    for _ in 0..3_000 {
        rig.media.inner.clean.notify_one();
        status = rig.media.status();
        if (status.evicted, status.cache_files, status.cache_bytes) == end || status.evicted > 10 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        (status.evicted, status.cache_files, status.cache_bytes),
        end,
        "every file it may drop goes, though the starred ones it can't come first and fill it alone"
    );
    assert_eq!(std::fs::read_dir(rig.home.path().join(FILES)).unwrap().count(), 4);
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
    holder.busy_timeout(SIDE_WAITS).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let dropping = {
        let (inner, sha256) = (rig.media.inner.clone(), lake.sha256.clone());
        tokio::spawn(async move { cache::drop_files(&inner, vec![sha256], true).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(file.exists(), "the file stays while the commit that stops naming it can't be made");
    let synced = rig.store.storage().synced_commits;
    holder.execute_batch("COMMIT").unwrap();
    assert_eq!(dropping.await.unwrap().unwrap().0, 1);
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

#[test]
fn a_preview_left_empty_by_a_power_cut_is_made_again_at_the_next_start() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let drawn = |row: &Value| previewed(row) && row["media"]["thumbnail"].is_string();
    // A jawd draws a photo's previews and thumbnail, and stops before they're sent to the drive.
    let (small, thumbnail) = as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0CUT").await;
        let row = rig.until(ANA, "3EB0CUT", "its previews", drawn).await;
        (rig.file(&row["media"]["previews"]["256"]), rig.file(&row["media"]["thumbnail"]))
    });
    // As a power cut leaves files put in place without waiting for the disk.
    std::fs::write(&small, b"").unwrap();
    let cut = std::fs::read(&thumbnail).unwrap();
    std::fs::write(&thumbnail, &cut[..cut.len() / 2]).unwrap();
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        let row = rig.until(ANA, "3EB0CUT", "made again", drawn).await;
        assert!(jaw_media::whole_preview(&rig.file(&row["media"]["previews"]["256"])));
        assert!(jaw_media::whole_preview(&rig.file(&row["media"]["thumbnail"])));
        assert_eq!(rig.media.status().remade, 2, "the empty preview and the thumbnail cut short");
        assert_eq!(rig.world.downloads(), 0, "made from the file in the cache, not downloaded again");
    });
}

#[test]
fn previews_settled_before_a_stop_are_shown_at_the_next_start_without_being_checked() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let drawn = |row: &Value| previewed(row) && row["media"]["thumbnail"].is_string();
    let small = as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0SETTLED").await;
        let row = rig.until(ANA, "3EB0SETTLED", "its previews", drawn).await;
        until_checked(&rig).await;
        rig.media.settle().await.unwrap();
        let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        assert_eq!((settled, made), (2, 2), "the previews and the thumbnail, numbered 1 and 2, are settled");
        rig.file(&row["media"]["previews"]["256"])
    });
    // Only a power cut before they settled could break them; this one was broken by hand, which only
    // the weekly look finds, so the next start shows it as it is, never reading it on the way.
    std::fs::write(&small, b"").unwrap();
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        assert!(drawn(&rig.row(ANA, "3EB0SETTLED").await), "shown at once");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(rig.media.status().remade, 0, "and never checked");
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn the_read_path_shows_previews_a_power_cut_may_have_broken_only_once_the_check_has_passed_them() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let db = Connection::open(home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    db.execute("INSERT INTO chats (id, kind, v) VALUES (?1, 'direct', 1)", [ANA]).unwrap();
    // Three files, the second of which a start found suspect; none is on disk, as the read path
    // never looks.
    for n in [1u8, 2, 3] {
        let (sha256, made) = (vec![n; 32], i64::from(n));
        db.execute(
            "INSERT INTO messages (chat, id, at, t, kind, detail, v)
            VALUES ((SELECT n FROM chats WHERE id = ?1), ?2, ?3, ?3, 'image', '{\"media\":{}}', 1)",
            rusqlite::params![ANA, format!("3EB0FILE{n}"), made],
        )
        .unwrap();
        db.execute(
            "INSERT INTO media (message, variant, keys, sha256) VALUES (last_insert_rowid(), 'sd', 'image', ?1)",
            [&sha256],
        )
        .unwrap();
        db.execute(
            "INSERT INTO media_files (sha256, path, small, large, thumbnail, used_at, made)
            VALUES (?1, 'media/files/f.jpg', 'media/previews/s.jpg', 'media/previews/l.jpg', 'media/previews/t.jpg', 1, ?2)",
            rusqlite::params![sha256, made],
        )
        .unwrap();
    }
    store.files().suspect([vec![2u8; 32]]);
    let shown = |id: &'static str| {
        let store = store.clone();
        async move {
            let row = store
                .reads()
                .query(Priority::Interactive, move |db| Ok(read::message(db, ANA, id)?.map(|(row, _)| row)))
                .await
                .unwrap()
                .unwrap();
            let row = serde_json::to_value(crate::feed::message_row(row)).unwrap();
            (row["media"]["previews"].is_object(), row["media"]["thumbnail"].is_string())
        }
    };
    assert_eq!(shown("3EB0FILE1").await, (true, true), "not suspect");
    assert_eq!(shown("3EB0FILE2").await, (false, false), "not yet checked");
    assert_eq!(shown("3EB0FILE3").await, (true, true), "not suspect either");
    store.files().passed(&[vec![2u8; 32]]);
    assert_eq!(shown("3EB0FILE2").await, (true, true), "once the check has passed it");
}

/// Waits until the start's check has passed every file it found suspect.
async fn until_checked(rig: &Rig) {
    for _ in 0..1_000 {
        if !rig.media.status().checking {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the start's check never finished: {:?}", rig.media.status());
}

/// What a drive is asked, with how far jaw.db in `home` says previews are settled at each call.
type Asked = Arc<std::sync::Mutex<Vec<(drive::Flush, std::path::PathBuf, i64)>>>;

/// A stand-in for the drive that records what it's asked, and does nothing.
fn recording(home: &Path) -> (drive::Drive, Asked) {
    let asked = Asked::default();
    let (recorded, db, path) = (asked.clone(), std::sync::Mutex::new(None::<Connection>), home.join("jaw.db"));
    let drive = drive::Drive::stand_in(move |flush, at| {
        let mut db = db.lock().unwrap();
        let db = db.get_or_insert_with(|| Connection::open(&path).unwrap());
        let settled = db.query_row("SELECT settled FROM media_made", [], |r| r.get(0)).unwrap();
        recorded.lock().unwrap().push((flush, at.to_path_buf(), settled));
        Ok(())
    });
    (drive, asked)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settle_sends_each_preview_to_the_drive_and_fences_them_before_it_notes_them_settled() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let (drive, asked) = recording(home.path());
    let rig = rig_in(home.clone(), Config { drive: Some(drive), ..quick() }).await;
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0FENCED").await;
    let row = rig
        .until(ANA, "3EB0FENCED", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string())
        .await;
    until_checked(&rig).await;
    rig.media.settle().await.unwrap();
    let asked = asked.lock().unwrap().clone();
    let folder = home.path().join(PREVIEWS);
    let pushed: Vec<&std::path::PathBuf> =
        asked.iter().filter(|(flush, ..)| *flush == drive::Flush::Push).map(|(_, path, _)| path).collect();
    for file in [&row["media"]["previews"]["256"], &row["media"]["previews"]["768"], &row["media"]["thumbnail"]] {
        assert!(pushed.contains(&&rig.file(file)), "{file} was sent to the drive: {asked:?}");
    }
    assert!(pushed.contains(&&folder), "and its folder: {asked:?}");
    let barriers: Vec<_> = asked.iter().filter(|(flush, ..)| *flush == drive::Flush::Barrier).collect();
    assert_eq!(barriers.len(), 1, "one barrier: {asked:?}");
    assert_eq!(
        asked.last().map(|(flush, path, _)| (*flush, path)),
        Some((drive::Flush::Barrier, &folder)),
        "after every push"
    );
    assert!(asked.iter().all(|(.., settled)| *settled == 0), "nothing noted settled before the barrier: {asked:?}");
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (2, 2), "noted once fenced");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settle_stops_at_a_preview_it_cant_send_to_the_drive_and_passes_over_one_gone_since() {
    for (error, settles) in [(std::io::ErrorKind::PermissionDenied, false), (std::io::ErrorKind::NotFound, true)] {
        let failing = Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
        let drive = {
            let failing = failing.clone();
            drive::Drive::stand_in(move |_, path| match failing.lock().unwrap().as_deref() {
                Some(failing) if failing == path => Err(error.into()),
                _ => Ok(()),
            })
        };
        let rig = rig(Config { drive: Some(drive), ..quick() }).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0UNSENT").await;
        let row = rig
            .until(ANA, "3EB0UNSENT", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string())
            .await;
        until_checked(&rig).await;
        *failing.lock().unwrap() = Some(rig.file(&row["media"]["previews"]["256"]));
        let settled = rig.media.settle().await;
        let (through, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        assert_eq!(
            (settled.is_ok(), through == made),
            (settles, settles),
            "{error:?}: {settled:?}, settled through {through} of {made}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn off_the_macs_own_drive_downloads_are_flushed_in_full_and_status_says_so() {
    for (barriers, full) in [(true, false), (false, true)] {
        let rig = rig(Config { barriers: Some(barriers), ..quick() }).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0FLUSHED").await;
        rig.until(ANA, "3EB0FLUSHED", "downloaded", local).await;
        let asked: Vec<bool> = rig
            .world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::MediaDownload(download) => Some(download.full_flush),
                _ => None,
            })
            .collect();
        assert_eq!(asked, [full], "with barriers holding: {barriers}");
        assert_eq!(rig.media.status().full_flush, full);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stops_settle_gives_up_at_its_bound_and_notes_nothing_after() {
    let (release, held) = std::sync::mpsc::channel::<()>();
    let held = std::sync::Mutex::new(held);
    // A drive that takes its time over the barrier, as one under heavy I/O does.
    let drive = drive::Drive::stand_in(move |flush, _| {
        if flush == drive::Flush::Barrier {
            let _ = held.lock().unwrap().recv_timeout(Duration::from_secs(10));
        }
        Ok(())
    });
    let rig = rig(Config { drive: Some(drive), ..quick() }).await;
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0BOUND").await;
    rig.until(ANA, "3EB0BOUND", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    until_checked(&rig).await;
    let began = std::time::Instant::now();
    let settled = rig.media.settle_within(Duration::from_millis(200)).await;
    assert!(settled.is_err() && began.elapsed() < Duration::from_secs(2), "{settled:?} after {:?}", began.elapsed());
    release.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (0, 2), "a settle given up notes nothing, even once the drive is done");
}

/// The settles the settle's task has started.
fn settles(rig: &Rig) -> u64 {
    rig.media.inner.settles.load(std::sync::atomic::Ordering::SeqCst)
}

/// Waits until every preview and thumbnail named is settled, failing past `bound`.
async fn until_settled(rig: &Rig, bound: Duration) {
    let began = std::time::Instant::now();
    loop {
        let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        if made > 0 && settled == made {
            return;
        }
        assert!(began.elapsed() < bound, "settled through {settled} of {made} after {:?}", began.elapsed());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_settle_wakes_only_once_previews_are_named_and_settles_them_within_settle_every() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let (drive, asked) = recording(home.path());
    let every = Duration::from_millis(100);
    let rig = rig_in(home.clone(), Config { drive: Some(drive), settle_every: every, ..quick() }).await;
    until_checked(&rig).await;
    tokio::time::sleep(every * 10).await;
    assert_eq!(settles(&rig), 0, "nothing was named, so the settle never woke");
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0NAMED").await;
    rig.until(ANA, "3EB0NAMED", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    until_settled(&rig, every + Duration::from_secs(5)).await;
    let barriers = asked.lock().unwrap().iter().filter(|(flush, ..)| *flush == drive::Flush::Barrier).count();
    assert!(barriers >= 1, "sent to the drive and fenced, with nothing but the naming to wake it");
    // One named while a settle ran is left for the next, settle_every later.
    tokio::time::sleep(every * 5).await;
    let woken = settles(&rig);
    tokio::time::sleep(every * 10).await;
    assert_eq!(settles(&rig), woken, "once they're settled, it doesn't wake again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thumbnail_named_while_its_file_downloads_is_settled_and_so_are_its_previews_named_later() {
    let every = Duration::from_millis(100);
    let rig = rig(Config { drive: Some(drive::Drive::stand_in(|_, _| Ok(()))), settle_every: every, ..quick() }).await;
    until_checked(&rig).await;
    rig.world.gate_downloads(true);
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0ALONE").await;
    rig.until(ANA, "3EB0ALONE", "its thumbnail", |row| row["media"]["thumbnail"].is_string()).await;
    until_settled(&rig, every + Duration::from_secs(5)).await;
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (1, 1), "the thumbnail alone, its file still downloading");
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0ALONE", "its previews", previewed).await;
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!(made, 2, "then its previews ({settled} settled)");
    until_settled(&rig, every + Duration::from_secs(5)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn previews_named_while_a_settle_is_under_way_are_settled_by_the_next_with_nothing_more_named() {
    let order = std::sync::atomic::Ordering::SeqCst;
    let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (release, held) = std::sync::mpsc::channel::<()>();
    let held = std::sync::Mutex::new(held);
    let drive = {
        let entered = entered.clone();
        // Only a settle sends to the drive. The first push waits until the test lets it go, 30 s at
        // most, so a failing test can't hang the binary.
        drive::Drive::stand_in(move |_, _| {
            if !entered.swap(true, order) {
                let _ = held.lock().unwrap().recv_timeout(Duration::from_secs(30));
            }
            Ok(())
        })
    };
    let every = Duration::from_millis(100);
    let rig = rig(Config { drive: Some(drive), settle_every: every, ..quick() }).await;
    until_checked(&rig).await;
    rig.world.gate_downloads(true);
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0DURING").await;
    rig.until(ANA, "3EB0DURING", "its thumbnail", |row| row["media"]["thumbnail"].is_string()).await;
    let began = std::time::Instant::now();
    while !entered.load(order) {
        assert!(began.elapsed() < Duration::from_secs(10), "the thumbnail's settle never reached the drive");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0DURING", "its previews", previewed).await;
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (0, 2), "the previews named while the thumbnail's settle is held");
    release.send(()).unwrap();
    until_settled(&rig, every * 10 + Duration::from_secs(5)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settle_that_fails_is_tried_again_every_settle_every_until_it_goes_through() {
    // More failures than the namings of a photo's previews and thumbnail could wake it for.
    let failing = Arc::new(std::sync::atomic::AtomicU32::new(3));
    let drive = {
        let failing = failing.clone();
        // Only a settle sends to the drive, one at a time.
        drive::Drive::stand_in(move |_, _| {
            let order = std::sync::atomic::Ordering::SeqCst;
            if failing.load(order) == 0 {
                return Ok(());
            }
            failing.fetch_sub(1, order);
            Err(std::io::ErrorKind::PermissionDenied.into())
        })
    };
    let every = Duration::from_millis(100);
    let rig = rig(Config { drive: Some(drive), settle_every: every, ..quick() }).await;
    until_checked(&rig).await;
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0AGAIN").await;
    rig.until(ANA, "3EB0AGAIN", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    until_settled(&rig, every * 10 + Duration::from_secs(5)).await;
    assert_eq!(failing.load(std::sync::atomic::Ordering::SeqCst), 0, "three settles failed before one went through");
    assert!(settles(&rig) >= 4, "each tried again with nothing more named: {}", settles(&rig));
}

#[tokio::test(flavor = "multi_thread")]
async fn previews_named_are_settled_by_a_clean_stops_settle_however_long_the_settle_would_wait() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let (drive, asked) = recording(home.path());
    let rig = rig_in(home.clone(), Config { drive: Some(drive), ..quick() }).await;
    until_checked(&rig).await;
    rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0STOP").await;
    rig.until(ANA, "3EB0STOP", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (0, 2), "the settle waits settle_every, an hour here, after they're named");
    // As jawd's stop asks within its bound, SETTLING.
    rig.media.settle_within(Duration::from_millis(500)).await.unwrap();
    let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
    assert_eq!((settled, made), (2, 2), "the stop's settle takes them");
    let barriers = asked.lock().unwrap().iter().filter(|(flush, ..)| *flush == drive::Flush::Barrier).count();
    assert_eq!(barriers, 1, "behind one barrier");
}

#[test]
fn previews_a_start_found_unsettled_are_settled_once_the_check_has_passed_them() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    // A jawd draws a photo's previews and thumbnail, and stops before they're settled.
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0LEFT").await;
        rig.until(ANA, "3EB0LEFT", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    });
    as_a_jawd(async {
        let every = Duration::from_millis(100);
        let rig = rig_in(home.clone(), Config { settle_every: every, ..quick() }).await;
        until_checked(&rig).await;
        until_settled(&rig, every + Duration::from_secs(5)).await;
        assert_eq!(rig.media.status().remade, 0, "whole, so passed as they were, and nothing named since");
    });
}

/// A photo whose previews and thumbnail a jawd on `home` drew and never settled, and the path of
/// its 256 px preview, emptied as a power cut leaves it.
fn drawn_and_cut(home: &Arc<tempfile::TempDir>, id: &'static str) -> (std::path::PathBuf, Vec<u8>) {
    let lake = Picture::lake(640, 480);
    let small = as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&lake, ANA, ANA, id).await;
        let row =
            rig.until(ANA, id, "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
        rig.file(&row["media"]["previews"]["256"])
    });
    std::fs::write(&small, b"").unwrap();
    (small, lake.sha256)
}

fn drawn_whole(home: &Path) -> impl Fn(&Value) -> bool {
    let home = home.to_path_buf();
    move |row: &Value| {
        previewed(row)
            && row["media"]["thumbnail"].is_string()
            && row["media"]["previews"]["256"].as_str().is_some_and(|small| jaw_media::whole_preview(&home.join(small)))
    }
}

#[test]
fn a_suspect_preview_whose_file_a_job_holds_stays_unshown_and_unsettled_until_its_mended() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let (_, sha256) = drawn_and_cut(&home, "3EB0HELD");
    as_a_jawd(async {
        let rig = rig_in(home.clone(), Config { check_after: Duration::from_millis(300), ..quick() }).await;
        // A job holds the file as the check comes to it, as a download started again would.
        let claim = rig.media.inner.claims.share(&sha256).await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        let row = rig.row(ANA, "3EB0HELD").await;
        assert!(row["media"]["previews"].is_null() && row["media"]["thumbnail"].is_null(), "shown unmended: {row}");
        let status = rig.media.status();
        assert!(status.checking && status.suspect == 1 && status.remade == 0, "{status:?}");
        rig.media.settle().await.unwrap();
        let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        assert_eq!((settled, made), (0, 2), "nothing settles while it's unmended");
        drop(claim);
        rig.until(ANA, "3EB0HELD", "mended once its file is free", drawn_whole(home.path())).await;
        until_checked(&rig).await;
        let status = rig.media.status();
        assert!(status.suspect == 0 && status.remade == 1, "{status:?}");
        rig.media.settle().await.unwrap();
        let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        assert!(settled == made && made > 2, "settled once mended and drawn again: {settled} of {made}");
    });
}

#[test]
fn a_clean_stop_before_the_starts_check_settles_nothing_the_check_hasnt_passed() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    drawn_and_cut(&home, "3EB0STOPPED");
    as_a_jawd(async {
        let rig = rig_in(home.clone(), Config { check_after: Duration::from_secs(3600), ..quick() }).await;
        rig.media.settle().await.unwrap();
        let (settled, made) = rig.store.pair("SELECT settled, made FROM media_made").unwrap();
        assert_eq!((settled, made), (0, 2), "the stop settled what the check hadn't passed");
    });
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.until(ANA, "3EB0STOPPED", "mended at the next start", drawn_whole(home.path())).await;
        assert_eq!(rig.media.status().remade, 1);
    });
}

/// The row of message `id` anywhere in `line`, as a reply or a push carries it.
fn row_in<'a>(line: &'a Value, id: &str) -> Option<&'a Value> {
    match line {
        Value::Object(map) if map.get("id").and_then(Value::as_str) == Some(id) && map.contains_key("media") => {
            Some(line)
        }
        Value::Object(map) => map.values().find_map(|value| row_in(value, id)),
        Value::Array(values) => values.iter().find_map(|value| row_in(value, id)),
        _ => None,
    }
}

#[test]
fn windows_shown_a_photo_before_the_check_passed_it_are_told_once_it_has() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let lake = Picture::lake(640, 480);
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&lake, ANA, ANA, "3EB0TOLD").await;
        rig.until(ANA, "3EB0TOLD", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    });
    as_a_jawd(async {
        let rig = rig_in(home.clone(), Config { check_after: Duration::from_millis(1_500), ..quick() }).await;
        let mut interface = Interface::open(&rig.serve()).await;
        let hello = json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" });
        interface.call(1, "hello", hello).await;
        let watched =
            interface.call(2, "messages.watch", json!({ "chat": ANA, "around": "newest", "before": 20 })).await;
        let shown = row_in(&watched, "3EB0TOLD").expect("the photo in the window");
        assert!(shown["media"]["previews"].is_null(), "shown before the check passed it: {shown}");
        let told = interface
            .until(Duration::from_secs(10), |line| {
                row_in(line, "3EB0TOLD").is_some_and(|row| row["media"]["previews"].is_object())
            })
            .await;
        assert!(told.is_some(), "the window was never told the check passed it");
        assert_eq!(rig.media.status().remade, 0, "whole, so passed as it was");
    });
}

#[test]
fn the_weekly_look_mends_a_settled_preview_broken_since() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let small = as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&Picture::lake(640, 480), ANA, ANA, "3EB0WEEKLY").await;
        let row = rig
            .until(ANA, "3EB0WEEKLY", "its previews", |row| previewed(row) && row["media"]["thumbnail"].is_string())
            .await;
        until_checked(&rig).await;
        rig.media.settle().await.unwrap();
        rig.file(&row["media"]["previews"]["256"])
    });
    // Broken by a disk error, or a hand from outside, after it settled.
    std::fs::write(&small, b"").unwrap();
    as_a_jawd(async {
        let rig = rig_in(home.clone(), Config { look_every: Duration::from_millis(300), ..quick() }).await;
        rig.until(ANA, "3EB0WEEKLY", "mended by the look", drawn_whole(home.path())).await;
        assert_eq!(rig.media.status().remade, 1);
    });
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
    let rig = rig_on(Arc::new(home), store, quick()).await;
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
async fn copies_nobody_asked_for_are_done_a_few_at_a_time_and_one_asked_for_never_waits_for_them() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    for n in 0..40 {
        let photo = rig.world.photo(&Picture::lake(320 + n, 240), RUI, RUI, &format!("3EB0MANY{n}"), now_ms(), "");
        rig.world.deliver(without_thumbnail(photo)).await.unwrap();
    }
    let mut most = 0;
    for _ in 0..50 {
        most = most.max(rig.media.inner.schedule.running());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(most, schedule::UNASKED_AT_ONCE, "the photos' jobs at once, their downloads held");
    let status = rig.media.status();
    assert_eq!((status.unasked, status.unasked_most), (16, 16), "status says the cap is full");
    let asked = Picture::lake(640, 480);
    rig.photo_from_history(&asked, "3EB0WANTED").await;
    rig.call("media.fetch", json!({ "message": "3EB0WANTED" })).await.unwrap();
    rig.until(ANA, "3EB0WANTED", "downloading, ahead of the photos' jobs", |row| {
        row["media"]["state"] == "downloading"
    })
    .await;
    assert_eq!(rig.media.inner.schedule.running(), schedule::UNASKED_AT_ONCE + 1);
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0WANTED", "its previews", previewed).await;
    for n in 0..40 {
        rig.until(RUI, &format!("3EB0MANY{n}"), "its previews", previewed).await;
    }
}

/// A rig whose background work waits for the checkpointer past a few hundred frames in the WAL.
async fn rig_with_small_wal() -> Rig {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let store = Store::open_with_wal(home.path(), 256, 512).unwrap();
    crate::sync::running(&store).await.unwrap();
    rig_on(home, store, quick()).await
}

impl Rig {
    /// Holds background work until the reader returned is dropped: it holds the WAL from here, so
    /// the checkpointer can't copy what comes after, and enough comes after that background work
    /// waits for it.
    async fn hold_background(&self) -> Connection {
        let reader = Connection::open(self.home.path().join("jaw.db")).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        let padding = (0..1_000).map(|n| Change::Remember { key: format!("padding {n}"), value: "x".repeat(4_000) });
        self.store.apply(Lane::Live, padding.collect()).await.unwrap();
        let frames = self.store.storage().wal_frames;
        assert!(frames > 512, "the WAL holds {frames} frames");
        reader
    }

    /// Message `id`'s copy's want, in Ana's chat.
    async fn want_of(&self, id: &'static str) -> Option<i64> {
        self.query(move |db| {
            Ok(db.query_row(
                "SELECT md.wanted FROM media md JOIN messages m ON m.n = md.message WHERE m.id = ?1",
                [id],
                |r| r.get(0),
            )?)
        })
        .await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn what_windows_show_and_what_downloads_by_itself_wait_for_the_wals_bound_and_what_an_interface_asks_for_doesnt()
{
    let rig = rig_with_small_wal().await;
    let (shown, asked, arriving) = (Picture::lake(640, 480), Picture::lake(320, 240), Picture::lake(400, 300));
    rig.photo_from_history(&shown, "3EB0SHOWN").await;
    rig.photo_from_history(&asked, "3EB0ASKED").await;
    let reader = rig.hold_background().await;

    // A window shows one, and a photo arrives; then an interface asks for another, which is
    // downloaded, its progress shown, and drawn meanwhile.
    assert_eq!(rig.row(ANA, "3EB0SHOWN").await["media"]["state"], "remote");
    rig.photo(&arriving, ANA, ANA, "3EB0ARRIVING").await;
    rig.world.gate_downloads(true);
    rig.call("media.fetch", json!({ "message": "3EB0ASKED" })).await.unwrap();
    rig.until(ANA, "3EB0ASKED", "half downloaded while background work waits", half).await;
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0ASKED", "downloaded and drawn while background work waits", previewed).await;
    let took = rig.world.deliver(demo::text(ANA, ANA, "3EB0TEXT", now_ms(), "hi")).await.unwrap();
    assert!(took < Duration::from_secs(1), "a message waited {took:?}");
    assert_eq!(rig.want_of("3EB0SHOWN").await, None, "what the window showed waits with the rest of the background");
    assert!(!local(&rig.row(ANA, "3EB0SHOWN").await));
    let arrived = rig.row(ANA, "3EB0ARRIVING").await;
    assert!(arrived["media"]["thumbnail"].is_null() && !local(&arrived), "the photo's job waits too: {arrived}");

    // Once the reader lets go, the WAL starts again, and what waited is done.
    drop(reader);
    rig.until(ANA, "3EB0SHOWN", "downloaded and drawn once background work goes on", previewed).await;
    rig.until(ANA, "3EB0ARRIVING", "downloaded and drawn once background work goes on", previewed).await;
    assert!(rig.store.storage().background_waits > 0, "background work waited for the WAL");
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_done_while_background_work_waits_leave_the_downloads_lane_free_for_what_an_interface_asks_for() {
    let rig = rig_with_small_wal().await;
    let asked = Picture::lake(320, 240);
    rig.photo_from_history(&asked, "3EB0ASKED").await;
    rig.world.gate_downloads(true);
    // A window shows as many photos as there are download workers, and their downloads begin.
    let workers = rig.media.lanes()[0].report().workers;
    let shown: Vec<String> = (0..workers).map(|n| format!("3EB0SHOWN{n}")).collect();
    for (n, id) in shown.iter().enumerate() {
        rig.photo_from_history(&Picture::lake(400 + n as u32, 300), id).await;
        assert_eq!(rig.row(ANA, id).await["media"]["state"], "remote");
    }
    for id in &shown {
        rig.until(ANA, id, "half downloaded", half).await;
    }
    // They end while background work waits, and naming their files waits with it, out of the lane.
    let reader = rig.hold_background().await;
    rig.world.gate_downloads(false);
    rig.call("media.fetch", json!({ "message": "3EB0ASKED" })).await.unwrap();
    rig.until(ANA, "3EB0ASKED", "downloaded and drawn while background work waits", previewed).await;
    for id in &shown {
        let row = rig.row(ANA, id).await;
        assert!(!local(&row), "its file waits to be named with the rest of the background work: {row}");
    }
    for _ in 0..500 {
        if rig.media.status().naming == u64::from(workers) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(rig.media.status().naming, u64::from(workers), "status says how many files wait to be named");
    drop(reader);
    for id in &shown {
        rig.until(ANA, id, "downloaded and drawn once background work goes on", previewed).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_nobody_waits_on_never_holds_up_the_progress_of_a_file_an_interface_asks_for() {
    let rig = rig_with_small_wal().await;
    let (shown, asked) = (Picture::lake(640, 480), Picture::lake(320, 240));
    rig.photo_from_history(&shown, "3EB0SHOWN").await;
    rig.photo_from_history(&asked, "3EB0ASKED").await;
    rig.world.gate_downloads(true);
    assert_eq!(rig.row(ANA, "3EB0SHOWN").await["media"]["state"], "remote");
    rig.until(ANA, "3EB0SHOWN", "half downloaded", half).await;
    let reader = rig.hold_background().await;
    // The window's download says it has got further, past the least time between two of its
    // progress's writes, and that write waits for the background lane.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let progress = rig.live.lock().unwrap().media_progress.clone().unwrap();
    let size = shown.data.len() as u64;
    progress.send(pb::MediaProgress { id: to_hex(&shown.sha256), bytes: size * 3 / 4, size }).await.unwrap();
    // An interface asks for the other, whose download's progress shows at once all the same.
    rig.call("media.fetch", json!({ "message": "3EB0ASKED" })).await.unwrap();
    rig.until(ANA, "3EB0ASKED", "half downloaded while background work waits", half).await;
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0ASKED", "downloaded and drawn while background work waits", previewed).await;
    let before = rig.media.status();
    assert!(before.progress_waiting >= 1, "status says the window's progress waits: {before:?}");
    // More downloads nobody waits on say how far they've got than the queue holds: the rest is
    // passed over, and status says so.
    for n in 0..70u8 {
        let id = to_hex(&[n; 32]);
        progress.send(pb::MediaProgress { id, bytes: 1, size: 2 }).await.unwrap();
    }
    let said = |s: &MediaStatus| s.progress_waiting + s.progress_passed;
    let mut status = rig.media.status();
    for _ in 0..500 {
        status = rig.media.status();
        if said(&status) == said(&before) + 70 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(said(&status), said(&before) + 70, "{status:?}");
    assert!(status.progress_waiting <= status.progress_most + 1, "{status:?}");
    assert!(status.progress_passed >= before.progress_passed + 5, "{status:?}");
    drop(reader);
    rig.until(ANA, "3EB0SHOWN", "downloaded and drawn once background work goes on", previewed).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_a_window_began_downloading_is_named_at_once_for_an_interface_asking_while_background_work_waits() {
    let rig = rig_with_small_wal().await;
    let lake = Picture::lake(640, 480);
    rig.photo_from_history(&lake, "3EB0WINDOW").await;
    rig.photo_from_history(&lake, "3EB0SAME").await;
    rig.world.gate_downloads(true);
    // A window shows one, and its file begins downloading in the background.
    assert_eq!(rig.row(ANA, "3EB0WINDOW").await["media"]["state"], "remote");
    rig.until(ANA, "3EB0WINDOW", "half downloaded", half).await;
    let reader = rig.hold_background().await;
    // An interface asks for the other message with the same file, which joins that download.
    rig.call("media.fetch", json!({ "message": "3EB0SAME" })).await.unwrap();
    rig.until(ANA, "3EB0SAME", "downloading", |row| row["media"]["state"] == "downloading").await;
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0SAME", "downloaded and drawn while background work waits", previewed).await;
    assert_eq!(rig.world.downloads(), 1, "one download for both");
    assert!(rig.want_of("3EB0WINDOW").await.is_some(), "the window's copy's job waits to settle");
    drop(reader);
    rig.until_nothing_wanted("both settled once background work goes on").await;
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
    assert_eq!(names, ["downloads", "images", "transcodes", "uploads"]);
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
    let settled = Step::Settled { message: n, variant: "sd", wanted: taken, state: Some("remote"), error: None };
    store.apply(Lane::Live, vec![Change::Media(settled)]).await.unwrap();
    let (_, wanted, asked) = read(&store).await;
    assert_eq!((wanted, asked), (Some(taken + 1), true), "still wanted, as asked");
    let done = Step::Settled { message: n, variant: "sd", wanted: taken + 1, state: Some("remote"), error: None };
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
    let settled = Step::Settled { message: n, variant: "sd", wanted: taken + 5, state: Some("remote"), error: None };
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
    db.execute_batch(
        "INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (randomblob(32), 'media/files/b.jpg', 7, 1)",
    )
    .unwrap();
    assert_eq!(state(&db), (7, 1, 1), "a file's row written when no copy names it waits to go too");
    drop(store);
}

/// `event`, a photo, without the thumbnail its message carries, as a photo whose thumbnail hasn't
/// been drawn yet is to the cache.
fn without_thumbnail(mut event: pb::Event) -> pb::Event {
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let mut raw = e2e::Message::decode(message.raw.as_slice()).unwrap();
        if let Some(image) = raw.image_message.as_mut() {
            image.jpeg_thumbnail = None;
        }
        message.raw = raw.encode_to_vec();
    }
    event
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_deleted_while_its_file_downloads_leaves_nothing_of_it_in_the_cache() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let (revoked, deleted) = (Picture::lake(640, 480), Picture::lake(320, 240));
    for (picture, id) in [(&revoked, "3EB0REVOKED"), (&deleted, "3EB0DELETED")] {
        rig.world.deliver(without_thumbnail(rig.world.photo(picture, ANA, ANA, id, now_ms(), ""))).await.unwrap();
        rig.until(ANA, id, "half downloaded", half).await;
    }
    let revoke = Change::Amend(crate::store::Amend {
        chat: ANA.into(),
        target: "3EB0REVOKED".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: crate::store::Amendment::Revoke,
        raw: None,
    });
    let deleted_for_me = Change::DeleteMessage { chat: ANA.into(), id: "3EB0DELETED".into() };
    rig.store.apply(Lane::Live, vec![revoke, deleted_for_me]).await.unwrap();
    rig.world.gate_downloads(false);
    let count = |folder: &str| std::fs::read_dir(rig.home.path().join(folder)).unwrap().count() as i64;
    // Their partial files stay in the cache's folder until each download ends and puts its file
    // in place, so the folder is empty only once both have, and their files have gone.
    let mut left = (0, 0, 0, 0, 0);
    for _ in 0..500 {
        rig.media.inner.clean.notify_one();
        let rows: (i64, i64, i64) = rig
            .query(|db| {
                Ok(db.query_row(
                    "SELECT (SELECT count(*) FROM media), (SELECT count(*) FROM media_files),
                        (SELECT count(*) FROM media_drop)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?)
            })
            .await;
        left = (rows.0, rows.1, rows.2, count(FILES), count(PREVIEWS));
        if left == (0, 0, 0, 0, 0) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        left,
        (0, 0, 0, 0, 0),
        "copies, files' rows, files waiting to go, files and previews: nothing of either is kept"
    );
    assert_eq!(rig.media.status().cache_bytes, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_an_interface_asks_of_media_reaches_the_disk_before_it_is_answered() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let (asked, saved) = (Picture::lake(640, 480), Picture::lake(320, 240));
    rig.photo(&asked, ANA, ANA, "3EB0ASKED").await;
    rig.photo(&saved, ANA, ANA, "3EB0SAVED").await;
    for id in ["3EB0ASKED", "3EB0SAVED"] {
        rig.until(ANA, id, "half downloaded", half).await;
    }
    let synced = || rig.store.storage().synced_commits;
    let before = synced();
    rig.call("media.fetch", json!({ "message": "3EB0ASKED" })).await.unwrap();
    assert!(synced() > before, "media.fetch was answered before what it asked reached the disk");
    let before = synced();
    rig.call("media.cancel", json!({ "message": "3EB0ASKED" })).await.unwrap();
    assert!(synced() > before, "media.cancel was answered before it reached the disk");
    // media.save of a file not here yet keeps that it's wanted on the disk while it waits for it.
    let to = rig.home.path().join("saved");
    std::fs::create_dir(&to).unwrap();
    let before = synced();
    let saving = {
        let (media, to) = (rig.media.clone(), to.display().to_string());
        tokio::spawn(async move { media.call("media.save", json!({ "message": "3EB0SAVED", "to": to })).await })
    };
    for _ in 0..250 {
        if synced() > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(synced() > before, "media.save's want didn't reach the disk while it waited for the file");
    assert!(!saving.is_finished(), "it waits for the file");
    rig.world.gate_downloads(false);
    let saved = tokio::time::timeout(Duration::from_secs(30), saving).await.unwrap().unwrap().unwrap();
    assert!(saved["path"].as_str().is_some_and(|path| path.starts_with(&to.display().to_string())), "{saved}");
}

/// One interface's connection to jawd's socket, reading what jawd sends it.
struct Interface {
    write: tokio::net::unix::OwnedWriteHalf,
    lines: tokio::io::Lines<tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>>,
}

impl Interface {
    async fn open(socket: &Path) -> Interface {
        let (read, write) = tokio::net::UnixStream::connect(socket).await.unwrap().into_split();
        Interface { write, lines: tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(read)) }
    }

    async fn send(&mut self, line: Value) {
        tokio::io::AsyncWriteExt::write_all(&mut self.write, format!("{line}\n").as_bytes()).await.unwrap();
    }

    /// The first line jawd sends that `wanted` takes, within `within`; None if none comes.
    async fn until(&mut self, within: Duration, wanted: impl Fn(&Value) -> bool) -> Option<Value> {
        tokio::time::timeout(within, async {
            while let Ok(Some(line)) = self.lines.next_line().await {
                let line: Value = serde_json::from_str(&line).unwrap();
                if wanted(&line) {
                    return Some(line);
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    /// Sends request `id` and waits for its reply.
    async fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({ "id": id, "method": method, "params": params })).await;
        self.until(Duration::from_secs(10), |line| line["id"] == id).await.expect("a reply")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_waiting_for_its_file_holds_up_nothing_else_on_its_connection() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let lake = Picture::lake(640, 480);
    rig.photo(&lake, ANA, ANA, "3EB0SLOW").await;
    rig.until(ANA, "3EB0SLOW", "half downloaded", half).await;
    let to = rig.home.path().join("saved");
    std::fs::create_dir(&to).unwrap();
    let mut interface = Interface::open(&rig.serve()).await;
    let hello = json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" });
    interface.call(1, "hello", hello).await;
    interface.call(2, "messages.watch", json!({ "chat": ANA, "around": "newest", "before": 20 })).await;
    interface.send(json!({ "id": 3, "method": "media.save", "params": { "message": "3EB0SLOW", "to": to } })).await;
    // While the save waits for its file, the connection's other requests are answered...
    interface.send(json!({ "id": 4, "method": "status" })).await;
    let answered = interface.until(Duration::from_secs(5), |line| line["id"] == 4 || line["id"] == 3).await;
    assert_eq!(answered.as_ref().map(|line| line["id"].clone()), Some(json!(4)), "status came first: {answered:?}");
    // ...and its windows are told what arrives.
    rig.world.deliver(demo::text(ANA, ANA, "3EB0WHILE", now_ms(), "meanwhile")).await.unwrap();
    let pushed = interface
        .until(Duration::from_secs(5), |line| {
            line["params"]["upsert"].as_array().is_some_and(|rows| rows.iter().any(|row| row["id"] == "3EB0WHILE"))
        })
        .await;
    assert!(pushed.is_some(), "the window wasn't told of the message while the save waited");
    rig.world.gate_downloads(false);
    let saved = interface.until(Duration::from_secs(30), |line| line["id"] == 3).await.expect("the save's reply");
    assert!(
        saved["result"]["path"].as_str().is_some_and(|path| path.starts_with(&to.display().to_string())),
        "{saved}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_wait_while_the_disk_is_nearly_full_and_go_on_once_there_is_room() {
    let free = Arc::new(std::sync::atomic::AtomicU64::new(100_000_000));
    let rig = rig(Config { keep_free: 512_000_000, free: Some(free.clone()), ..quick() }).await;
    let (asked, waiting) = (Picture::lake(640, 480), Picture::lake(320, 240));
    rig.photo(&waiting, ANA, ANA, "3EB0WAITING").await;
    let mut status = rig.media.status();
    for _ in 0..250 {
        status = rig.media.status();
        if status.paused.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let paused = status.paused.expect("background downloads say why they wait");
    assert!(paused.contains("100 MB free") && paused.contains("512 MB"), "{paused}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.world.downloads(), 0, "nothing downloads in the background meanwhile");
    // What an interface asks for keeps only 64 MB free, so it comes.
    rig.photo(&asked, ANA, ANA, "3EB0ROOM").await;
    rig.until(ANA, "3EB0ROOM", "converted", |row| !row.is_null()).await;
    rig.call("media.fetch", json!({ "message": "3EB0ROOM" })).await.unwrap();
    rig.until(ANA, "3EB0ROOM", "downloaded once asked", local).await;
    assert!(!local(&rig.row(ANA, "3EB0WAITING").await), "the other still waits");
    free.store(10_000_000_000, std::sync::atomic::Ordering::Relaxed);
    rig.until(ANA, "3EB0WAITING", "downloaded once there's room", local).await;
    assert_eq!(rig.media.status().paused, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn below_the_disks_floor_no_download_starts_one_asked_for_included_and_both_go_once_there_is_room() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let config = crate::disk::Config { ballast: 0, ..Default::default() };
    let (disk, fake) = crate::disk::Disk::faked(home.path(), config, 10_000_000_000, 500_000_000_000);
    let store = Store::open_watched(home.path(), disk.clone()).unwrap();
    crate::sync::running(&store).await.unwrap();
    let rig = rig_on(home, store, quick()).await;
    fake.free.store(1_000_000_000, std::sync::atomic::Ordering::Relaxed);
    assert!(!disk.room());
    let (asked, waiting) = (Picture::lake(640, 480), Picture::lake(320, 240));
    rig.photo(&waiting, ANA, ANA, "3EB0WAITING").await;
    rig.photo(&asked, ANA, ANA, "3EB0ASKED").await;
    rig.until(ANA, "3EB0ASKED", "converted", |row| !row.is_null()).await;
    rig.call("media.fetch", json!({ "message": "3EB0ASKED" })).await.unwrap();
    tokio::time::sleep(crate::disk::POLL * 3).await;
    assert_eq!(rig.world.downloads(), 0, "nothing downloads, what an interface asked for included");
    let paused = rig.media.status().paused.expect("downloads say why they wait");
    assert!(paused.starts_with("jaw's disk has 1.0 GB free"), "{paused}");
    fake.free.store(10_000_000_000, std::sync::atomic::Ordering::Relaxed);
    rig.until(ANA, "3EB0ASKED", "downloaded once there's room, without being asked again", local).await;
    rig.until(ANA, "3EB0WAITING", "and the one nobody asked for", local).await;
    assert_eq!(rig.media.status().paused, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_asks_the_phone_again_for_what_a_stopped_jawd_left_waiting_for_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    crate::sync::running(&store).await.unwrap();
    // A photo gone from the server, whose phone a stopped jawd's engine asked for it: that engine
    // kept the key to read the answer with, and is gone with it.
    let lake = Picture::lake(640, 480);
    let event = World::default().photo(&lake, ANA, ANA, "3EB0ASKEDBEFORE", now_ms(), "");
    let Some(pb::event::Kind::Message(message)) = event.kind else { unreachable!() };
    let web = crate::convert::web_message(&message).unwrap();
    let me = crate::convert::Me::new([demo::ME.to_string()]);
    store.apply(Lane::Live, crate::convert::message(&web, &me, true)).await.unwrap();
    let (n, wanted): (i64, i64) = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT message, wanted FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
        })
        .await
        .unwrap();
    // Its job downloaded it, found it gone and noted the ask before asking.
    let downloading = Step::Downloading { message: n, variant: "sd" };
    let asked = Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() };
    store.apply(Lane::Live, vec![Change::Media(downloading), Change::Media(asked)]).await.unwrap();
    // The phone's answer would be waited for for ten minutes; a start asks again at once.
    let rig = rig_on(Arc::new(home), store, Config { phone_wait: Duration::from_secs(600), ..quick() }).await;
    let _ = rig.world.photo(&lake, ANA, ANA, "3EB0ASKEDBEFORE", now_ms(), "");
    rig.world.expire(&lake.direct_path);
    let row = rig.until(ANA, "3EB0ASKEDBEFORE", "downloaded from the phone's new upload", local).await;
    assert_eq!(std::fs::read(rig.file(&row["media"]["path"])).unwrap(), *lake.data);
    let asked_again = rig.world.asked().iter().filter(|kind| matches!(kind, pb::command::Kind::MediaRetry(_))).count();
    assert_eq!(asked_again, 1, "the phone was asked again");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_hd_copy_never_attaches_to_a_message_deleted_for_everyone() {
    let rig = rig(quick()).await;
    let (sd, hd) = (Picture::lake(640, 480), Picture::lake(1600, 1200));
    rig.photo(&sd, ANA, ANA, "3EB0SDGONE").await;
    rig.until(ANA, "3EB0SDGONE", "its previews", previewed).await;
    let revoke = Change::Amend(crate::store::Amend {
        chat: ANA.into(),
        target: "3EB0SDGONE".into(),
        sender: Some(ANA.into()),
        mine: false,
        at: now_ms(),
        what: crate::store::Amendment::Revoke,
        raw: None,
    });
    rig.store.apply(Lane::Live, vec![revoke]).await.unwrap();
    rig.world.deliver(hd_copy(&rig.world, &hd, "3EB0SDGONE", "3EB0HDLATE")).await.unwrap();
    // Converted in order, so once a message after it is in, the HD copy has been taken in too.
    rig.world.deliver(demo::text(ANA, ANA, "3EB0AFTERHD", now_ms(), "after")).await.unwrap();
    rig.until(ANA, "3EB0AFTERHD", "converted", |row| !row.is_null()).await;
    let hd_copies: i64 =
        rig.query(|db| Ok(db.query_row("SELECT count(*) FROM media WHERE variant = 'hd'", [], |r| r.get(0))?)).await;
    assert_eq!(hd_copies, 0, "no HD copy for a deleted message");
    let row = rig.row(ANA, "3EB0SDGONE").await;
    assert!(row["deleted"] == json!(true) && row["media"].get("hd").is_none(), "{row}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_media_row_from_before_migration_20_is_kept() {
    let mut db = Connection::open_in_memory().unwrap();
    for migration in &crate::store::schema_for_tests()[..19] {
        db.execute_batch(migration).unwrap();
    }
    db.pragma_update(None, "user_version", 19).unwrap();
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

/// Holds every one of jaw.db's readers until the sender is dropped.
fn hold_readers(rig: &Rig) -> crossbeam_channel::Sender<()> {
    let (gate, waiting) = crossbeam_channel::bounded::<()>(0);
    for _ in 0..8 {
        let (waiting, reads) = (waiting.clone(), rig.store.reads().clone());
        tokio::spawn(async move {
            reads
                .query(Priority::Interactive, move |_| {
                    let _ = waiting.recv();
                    Ok(())
                })
                .await
        });
    }
    gate
}

impl Rig {
    /// As hold_background, again: padding of its own, so it writes new frames.
    async fn hold_background_again(&self, round: &str) -> Connection {
        let reader = Connection::open(self.home.path().join("jaw.db")).unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let _: i64 = reader.query_row("SELECT count(*) FROM messages", [], |r| r.get(0)).unwrap();
        let padding = (0..1_000).map(|n| Change::Remember {
            key: format!("padding {round} {n}"),
            value: round.repeat(4_000 / round.len()),
        });
        self.store.apply(Lane::Live, padding.collect()).await.unwrap();
        reader
    }
}

/// The audit's A1: an interface asks for the very photo a window showed, whose job, taken in the
/// background, is downloading it; what an interface asks for is written live.
#[tokio::test(flavor = "multi_thread")]
async fn a_photo_asked_for_while_its_windows_job_runs_is_named_while_background_work_waits() {
    let rig = rig_with_small_wal().await;
    let lake = Picture::lake(640, 480);
    rig.photo_from_history(&lake, "3EB0SHOWN").await;
    rig.world.gate_downloads(true);
    assert_eq!(rig.row(ANA, "3EB0SHOWN").await["media"]["state"], "remote");
    rig.until(ANA, "3EB0SHOWN", "half downloaded", half).await;
    let reader = rig.hold_background().await;
    rig.call("media.fetch", json!({ "message": "3EB0SHOWN" })).await.unwrap();
    rig.world.gate_downloads(false);
    let began = std::time::Instant::now();
    let shown = tokio::time::timeout(
        Duration::from_secs(10),
        rig.until(ANA, "3EB0SHOWN", "downloaded and drawn while background work waits", previewed),
    )
    .await;
    let waited = began.elapsed();
    let row = rig.row(ANA, "3EB0SHOWN").await;
    drop(reader);
    rig.until(ANA, "3EB0SHOWN", "drawn once background work goes on", previewed).await;
    assert!(shown.is_ok(), "asked for, it waited {waited:?} for background work: {row}");
    assert_eq!(rig.world.downloads(), 1, "the asked job joined the window's download");
    rig.until_nothing_wanted("both jobs settled").await;
}

/// The audit's A2: the phone's answer and the note of the ask it answers, applied in either order,
/// leave the copy as the phone said.
#[tokio::test(flavor = "multi_thread")]
async fn a_phones_answer_committed_before_the_ask_it_answers_still_settles_the_copy() {
    for answer_first in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let message = Message {
            chat: ANA.into(),
            id: "3EB0LOST".into(),
            at: 1,
            sender: Some(ANA.into()),
            kind: "image",
            summary: "Photo".into(),
            live: true,
            media: Some(Box::new(crate::store::media::MediaCopy { sha256: Some(vec![4; 32]), ..Default::default() })),
            ..Message::default()
        };
        store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
        let (n, wanted): (i64, i64) = store
            .reads()
            .query(Priority::Interactive, |db| {
                Ok(db.query_row("SELECT message, wanted FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
            })
            .await
            .unwrap();
        let downloading = Step::Downloading { message: n, variant: "sd" };
        store.apply(Lane::Background, vec![Change::Media(downloading)]).await.unwrap();
        let answer = Step::Answered {
            chat: ANA.into(),
            id: "3EB0LOST".into(),
            answer: crate::store::media::Answer::NotFound,
            at: now_ms(),
        };
        let asked = Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() };
        if answer_first {
            store.apply(Lane::Live, vec![Change::Media(answer)]).await.unwrap();
            store.apply(Lane::Background, vec![Change::Media(asked)]).await.unwrap();
        } else {
            store.apply(Lane::Background, vec![Change::Media(asked)]).await.unwrap();
            store.apply(Lane::Live, vec![Change::Media(answer)]).await.unwrap();
        }
        let (state, error): (String, Option<String>) = store
            .reads()
            .query(Priority::Interactive, |db| {
                Ok(db.query_row("SELECT state, error FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
            })
            .await
            .unwrap();
        assert_eq!(state, "gone", "answer first {answer_first}: {error:?}");
    }
}

/// The audit's A2b: the same in a running jawd: a live photo the server no longer has, and the
/// phone, asked, says it has no file either, while background work waits behind the WAL's bound.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_that_says_it_has_no_file_before_its_ask_is_noted_leaves_the_copy_gone() {
    let rig = rig_with_small_wal().await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Lost);
    let first = rig.hold_background().await;
    let photo = without_thumbnail(rig.world.photo(&lake, ANA, ANA, "3EB0LOST", now_ms(), ""));
    rig.world.deliver(photo).await.unwrap();
    for _ in 0..500 {
        if rig.media.inner.schedule.running() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(rig.media.inner.schedule.running(), 1, "its job runs, held at its first write");
    let readers = hold_readers(&rig);
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(first);
    let db = Connection::open(rig.home.path().join("jaw.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let state = |db: &Connection| -> (String, Option<String>) {
        db.query_row("SELECT state, error FROM media", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
    };
    for _ in 0..500 {
        if state(&db).0 == "downloading" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(state(&db).0, "downloading");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let second = rig.hold_background_again("second").await;
    drop(readers);
    for _ in 0..500 {
        if rig.world.asked().iter().any(|k| matches!(k, pb::command::Kind::MediaRetry(_))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(rig.world.asked().iter().any(|k| matches!(k, pb::command::Kind::MediaRetry(_))), "the phone was asked");
    tokio::time::sleep(Duration::from_millis(1_000)).await;
    let before = state(&db);
    drop(second);
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let after = state(&db);
    assert_eq!(after.0, "gone", "while background work waited: {before:?}; then {after:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_phone_is_asked_only_once_its_ask_is_noted() {
    let rig = rig(Config { phone_wait: Duration::from_secs(600), ..quick() }).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    let path = rig.home.path().join("jaw.db");
    let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let noting = seen.clone();
    rig.world.when_asked(move |_| {
        let db = Connection::open(&path).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        let state: String = db.query_row("SELECT state FROM media", [], |r| r.get(0)).unwrap();
        noting.lock().unwrap().push(state);
    });
    rig.photo(&lake, ANA, ANA, "3EB0NOTED").await;
    let asked = tokio::time::timeout(Duration::from_secs(60), async {
        while seen.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(asked.await.is_ok(), "the phone was never asked");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, ["waiting_for_phone"], "as the phone was asked, jaw.db already said so");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_that_cant_be_reached_is_asked_again_and_one_that_cant_be_asked_says_why() {
    let rig = rig(quick()).await;
    let reached = Picture::lake(640, 480);
    rig.world.expire(&reached.direct_path);
    rig.world.fail_asks(1, true);
    rig.photo(&reached, ANA, ANA, "3EB0AGAIN").await;
    rig.until(ANA, "3EB0AGAIN", "downloaded from the phone's upload once asked again", previewed).await;
    let asks = rig.world.asked().iter().filter(|k| matches!(k, pb::command::Kind::MediaRetry(_))).count();
    assert_eq!(asks, 2, "the ask that couldn't reach the phone, then the one that did");

    let refused = Picture::lake(320, 240);
    rig.world.expire(&refused.direct_path);
    rig.world.fail_asks(1, false);
    rig.photo(&refused, ANA, ANA, "3EB0REFUSED").await;
    let row = rig.until(ANA, "3EB0REFUSED", "expired", |row| row["media"]["state"] == "expired").await;
    let error = row["media"]["error"].as_str().unwrap();
    assert!(error.contains("couldn't ask your phone") && error.contains("can't be reached"), "{row}");
    rig.until_nothing_wanted("nothing waits for good").await;
    assert_eq!(rig.media.status().waiting_for_phone, 0);
}

/// Photo `id` from Ana, which the server answers 404 for and whose ask can't reach the phone, once
/// its job has ended and it waits to be tried again.
async fn waiting_to_be_tried_again(rig: &Rig, id: &str) {
    // What sync brings in as the engine connects is in before the photo comes, so it can't wake the
    // scheduler after the look.
    let quiet = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.world.fetched().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut committed = rig.store.committed();
        while let Ok(Ok(())) = tokio::time::timeout(Duration::from_millis(500), committed.changed()).await {}
    });
    assert!(quiet.await.is_ok(), "jaw.db never went quiet once the app state was fetched");
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.fail_asks(1, true);
    rig.photo(&lake, ANA, ANA, id).await;
    let waiting = |row: &Value| row["media"]["error"] == "the server answered 404";
    rig.until(ANA, id, "waiting to be tried again", waiting).await;
    let ended = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.media.inner.schedule.running() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(ended.await.is_ok(), "the job whose ask couldn't reach the phone never ended");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_whose_wait_ends_while_the_scheduler_looks_is_tried_again_with_nothing_else_to_wake_it() {
    let wait = Duration::from_secs(1);
    let rig = rig(Config { retry_first: wait, retry_most: wait, ..quick() }).await;
    waiting_to_be_tried_again(&rig, "3EB0DUE").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Begun well within its wait and ended past it, with nothing to wake the scheduler after it.
    rig.media.look_slowly(wait * 2);
    rig.until(ANA, "3EB0DUE", "downloaded from the phone's upload once tried again", previewed).await;
    let asks = rig.world.asked().iter().filter(|k| matches!(k, pb::command::Kind::MediaRetry(_))).count();
    assert_eq!(asks, 2, "the ask that couldn't reach the phone, then the one that did");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_that_ends_while_the_scheduler_looks_brings_one_more_look_not_a_spin() {
    use std::sync::atomic::Ordering::SeqCst;
    let wait = Duration::from_secs(1);
    let rig = rig(Config { retry_first: wait, retry_most: wait, ..quick() }).await;
    let schedule = &rig.media.inner.schedule;
    waiting_to_be_tried_again(&rig, "3EB0DUE").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // No look can take the photo, nor write anything that would wake the scheduler again.
    schedule.refuse_reads.store(true, SeqCst);
    rig.media.look_slowly(wait * 2);
    let began = tokio::time::timeout(Duration::from_secs(10), async {
        while !schedule.next_look_pause.lock().unwrap().is_zero() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(began.await.is_ok(), "the slow look never began");
    // The slow look counted itself before it took its pause.
    let slow = schedule.looks.load(SeqCst);
    // Past the slow look's end, and long enough for a spin to make thousands of looks.
    tokio::time::sleep(wait * 2 + Duration::from_secs(2)).await;
    let looks = schedule.looks.load(SeqCst) - slow;
    schedule.refuse_reads.store(false, SeqCst);
    // One, the look its wait brought, whose read failed as well; none if the slow look began after
    // the wait ended; one more if a wake that came as it began waited until it was over. A spin
    // makes thousands.
    assert!(looks <= 2, "{looks} looks after the slow one, whose wait ended during it");
    assert!(!previewed(&rig.row(ANA, "3EB0DUE").await), "nothing was read, so nothing was done");
    schedule.wake();
    rig.until(ANA, "3EB0DUE", "downloaded once reads work again and a wake comes", previewed).await;
}

/// The audit's A3: a photo that arrives while a window shows history is done ahead of what the
/// window shows after it.
#[tokio::test(flavor = "multi_thread")]
async fn a_photo_arriving_is_done_ahead_of_what_windows_show_after_it() {
    let rig = rig(quick()).await;
    let shown = |n: u32| format!("3EB0PAGE{n}");
    for n in 0..48u32 {
        rig.photo_from_history(&Picture::lake(300 + n, 200), &shown(n)).await;
    }
    rig.world.gate_downloads(true);
    for n in 0..16 {
        rig.row(ANA, &shown(n)).await;
    }
    for _ in 0..500 {
        if rig.media.inner.schedule.running() >= schedule::UNASKED_AT_ONCE {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    rig.photo(&Picture::lake(640, 480), RUI, RUI, "3EB0ARRIVED").await;
    for n in 16..48 {
        rig.row(ANA, &shown(n)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    rig.world.gate_downloads(false);
    rig.until(RUI, "3EB0ARRIVED", "its thumbnail drawn", |row| row["media"]["thumbnail"].is_string()).await;
    let mut later_done = 0;
    for n in 16..48 {
        if local(&rig.row(ANA, &shown(n)).await) {
            later_done += 1;
        }
    }
    // Taken first, it runs beside at most UNASKED_AT_ONCE - 1 of them, which on a machine with few
    // cores may end before it; taken after them, at least UNASKED_AT_ONCE would have started first.
    assert!(later_done < schedule::UNASKED_AT_ONCE, "{later_done} of the 32 shown after it came first");
}

#[tokio::test(flavor = "multi_thread")]
async fn copies_are_taken_asked_for_oldest_first_then_arrived_oldest_first_then_the_rest_newest_first() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let photo = |id: &str, n: u8, live: bool| {
        Change::Message(Message {
            chat: ANA.into(),
            id: id.into(),
            at: 1_000 + i64::from(n),
            sender: Some(ANA.into()),
            kind: "image",
            summary: "Photo".into(),
            live,
            media: Some(Box::new(crate::store::media::MediaCopy { sha256: Some(vec![n; 32]), ..Default::default() })),
            ..Message::default()
        })
    };
    for (n, id) in ["3EB0W1", "3EB0W2", "3EB0W3", "3EB0A1", "3EB0A2"].iter().enumerate() {
        store.apply(Lane::Live, vec![photo(id, n as u8, false)]).await.unwrap();
    }
    for (n, id) in ["3EB0L1", "3EB0L2"].iter().enumerate() {
        store.apply(Lane::Live, vec![photo(id, 10 + n as u8, true)]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let n_of = |id: &'static str| {
        let reads = store.reads().clone();
        async move {
            reads
                .query(Priority::Interactive, move |db| {
                    Ok(db.query_row("SELECT n FROM messages WHERE id = ?1", [id], |r| r.get::<_, i64>(0))?)
                })
                .await
                .unwrap()
        }
    };
    // Windows' wants and an interface's asks, at times out of order with the arrivals.
    let wants = [("3EB0W1", 100, false), ("3EB0A2", 250, true), ("3EB0W3", 300, false), ("3EB0A1", 150, true)];
    for (id, at, asked) in wants.into_iter().chain([("3EB0W2", 200, false), ("3EB0L1", 400, false)]) {
        let want = Step::Want { message: n_of(id).await, variant: "sd", at, asked };
        store.apply(Lane::Live, vec![Change::Media(want)]).await.unwrap();
    }
    let ids = |pass: schedule::Pass| {
        let reads = store.reads().clone();
        async move {
            reads
                .query(Priority::Interactive, move |db| {
                    let rows = schedule::read(db, pass, 10)?;
                    let mut ids = Vec::new();
                    for row in rows {
                        let n = row.map_err(|(n, ..)| anyhow::anyhow!("copy {n} has no hash"))?.message;
                        ids.push(db.query_row("SELECT id FROM messages WHERE n = ?1", [n], |r| r.get::<_, String>(0))?);
                    }
                    Ok(ids)
                })
                .await
                .unwrap()
        }
    };
    assert_eq!(ids(schedule::Pass::Asked).await, ["3EB0A1", "3EB0A2"], "asked for, oldest first");
    assert_eq!(ids(schedule::Pass::Arrived).await, ["3EB0L1", "3EB0L2"], "arrived live, oldest first");
    assert_eq!(ids(schedule::Pass::Shown).await, ["3EB0W3", "3EB0W2", "3EB0W1"], "the rest, newest first");
}

#[test]
fn the_starts_check_reads_a_page_again_that_it_couldnt_and_shows_nothing_of_it_meanwhile() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let lake = Picture::lake(640, 480);
    as_a_jawd(async {
        let rig = rig_in(home.clone(), quick()).await;
        rig.photo(&lake, ANA, ANA, "3EB0LOOK").await;
        rig.until(ANA, "3EB0LOOK", "drawn", |row| previewed(row) && row["media"]["thumbnail"].is_string()).await;
    });
    let failing = Arc::new(std::sync::atomic::AtomicU32::new(3));
    as_a_jawd(async {
        let rig = rig_in(home.clone(), Config { looks_failing: Some(failing.clone()), ..quick() }).await;
        for _ in 0..1_000 {
            if failing.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(failing.load(std::sync::atomic::Ordering::SeqCst), 0, "the check looked at the page three times");
        let status = rig.media.status();
        assert!(status.checking && status.suspect == 1, "the page it couldn't read is still to check: {status:?}");
        let row = rig.row(ANA, "3EB0LOOK").await;
        assert!(row["media"]["previews"].is_null() && row["media"]["thumbnail"].is_null(), "shown unread: {row}");
        rig.until(ANA, "3EB0LOOK", "shown once a look reads it", drawn_whole(home.path())).await;
        until_checked(&rig).await;
        assert_eq!(rig.media.status().suspect, 0);
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_that_panics_gives_its_place_back() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(512, 384);
    schedule::PANICKING.lock().unwrap().push(lake.sha256.clone());
    rig.photo(&lake, ANA, ANA, "3EB0PANIC").await;
    rig.until(ANA, "3EB0PANIC", "done by a job after the one that panicked", previewed).await;
    rig.until_nothing_wanted("its want settled").await;
    assert!(!schedule::PANICKING.lock().unwrap().contains(&lake.sha256), "its first job panicked");
    // The job that settled it gives its place back as it returns, a moment after its commit.
    let freed = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.media.inner.schedule.running() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(freed.await.is_ok(), "a place is held: {:?}", rig.media.status());
    assert_eq!(rig.media.status().unasked, 0, "no place is held");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_no_copy_names_past_a_page_that_jobs_hold_still_go() {
    let rig = rig(quick()).await;
    let db = Connection::open(rig.home.path().join("jaw.db")).unwrap();
    db.busy_timeout(SIDE_WAITS).unwrap();
    // A page of files no copy names, every one held by a job, and one more past them that none holds.
    let mut held = Vec::new();
    let mut files = Vec::new();
    for n in (0..64u8).chain([200]) {
        let sha256 = vec![n; 32];
        let path = format!("{FILES}/{}.bin", to_hex(&sha256));
        // Its row first: the sweep jawd starts with, which may still be going, removes a file no
        // row names.
        db.execute(
            "INSERT INTO media_files (sha256, path, bytes, used_at) VALUES (?1, ?2, 16, 1)",
            rusqlite::params![sha256, path],
        )
        .unwrap();
        std::fs::write(rig.home.path().join(&path), b"named by nothing").unwrap();
        files.push(rig.home.path().join(&path));
        if n < 64 {
            held.push(rig.media.inner.claims.share(&sha256).await);
        }
    }
    let rows = |db: &Connection| -> i64 { db.query_row("SELECT count(*) FROM media_files", [], |r| r.get(0)).unwrap() };
    rig.media.inner.clean.notify_one();
    for _ in 0..500 {
        if rows(&db) == 64 && !files[64].exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(rows(&db), 64, "the one past the page went, and those held stayed");
    assert!(!files[64].exists() && files[..64].iter().all(|f| f.exists()));
    drop(held);
    rig.media.inner.clean.notify_one();
    for _ in 0..500 {
        if rows(&db) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    rig.until_cleaner_idle().await;
    assert_eq!(rows(&db), 0, "and they go once let go");
    assert!(files.iter().all(|f| !f.exists()));
}

/// The audit's K7: media's start makes its folders while link previews' thumbnails make
/// media/links, and media/ with it, at the same moment.
#[test]
fn preparing_the_cache_beside_a_link_thumbnail_never_fails() {
    let mut failed = Vec::new();
    for _ in 0..300 {
        let home = tempfile::tempdir().unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let (waiting, links) = (barrier.clone(), home.path().join("media/links"));
        let other = std::thread::spawn(move || {
            waiting.wait();
            std::fs::DirBuilder::new().recursive(true).create(&links).unwrap();
        });
        barrier.wait();
        // What's raced is the making of folders; the drive's flushes would only slow each start.
        if let Err(e) = super::cache::prepare_syncing(home.path(), &|_| Ok(())) {
            failed.push(format!("{e:#}"));
        }
        other.join().unwrap();
    }
    assert!(failed.is_empty(), "{} of 300 starts failed: {:?}", failed.len(), failed.first());
}

/// A media/ link previews' thumbnails made, which they sync only inside media/links, is synced in
/// jaw's folder once the cache's folders are made in it, so they can't go with it in a power cut;
/// a start that makes nothing syncs nothing.
#[test]
fn the_cache_made_in_a_media_folder_another_made_syncs_that_folder_in_jaws() {
    let home = tempfile::tempdir().unwrap();
    std::fs::DirBuilder::new().recursive(true).create(home.path().join("media/links")).unwrap();
    let synced = std::sync::Mutex::new(Vec::new());
    let record = |folder: &Path| {
        synced.lock().unwrap().push(folder.to_path_buf());
        Ok(())
    };
    super::cache::prepare_syncing(home.path(), &record).unwrap();
    let first = std::mem::take(&mut *synced.lock().unwrap());
    for folder in [home.path().to_path_buf(), home.path().join("media"), home.path().join(FILES)] {
        assert!(first.contains(&folder), "{folder:?} synced: {first:?}");
    }
    super::cache::prepare_syncing(home.path(), &record).unwrap();
    assert_eq!(*synced.lock().unwrap(), Vec::<std::path::PathBuf>::new(), "nothing synced again");
}

/// The audit's K4: the owner opens a photo the phone was asked for and hasn't answered yet; the
/// first ask's "no file" then comes while the second job downloads. The copy ends as the phone said,
/// without the phone being asked again.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_to_an_earlier_ask_while_a_later_job_downloads_is_kept() {
    let rig = rig(Config { phone_wait: Duration::from_secs(600), downloads: 1, ..quick() }).await;
    let lake = Picture::lake(640, 480);
    rig.world.expire(&lake.direct_path);
    rig.world.phone(Phone::Silent);
    rig.photo(&lake, ANA, ANA, "3EB0LATE").await;
    rig.until(ANA, "3EB0LATE", "waiting for the phone", |row| row["media"]["state"] == "waiting_for_phone").await;
    // The one download worker is held by another photo's download, so the second job's download
    // waits in the lane, its copy downloading, as the first ask's answer comes.
    rig.world.gate_downloads(true);
    let blocker = Picture::lake(512, 384);
    rig.photo(&blocker, RUI, RUI, "3EB0BLOCKER").await;
    rig.until(RUI, "3EB0BLOCKER", "half downloaded", half).await;
    rig.call("media.fetch", json!({ "message": "3EB0LATE" })).await.unwrap();
    rig.until(ANA, "3EB0LATE", "downloading again", |row| row["media"]["state"] == "downloading").await;
    rig.world.deliver(answer("3EB0LATE", PhoneSaid::NotFound, "")).await.unwrap();
    rig.converted("3EB0AFTER3").await;
    let between = rig.row(ANA, "3EB0LATE").await;
    rig.world.gate_downloads(false);
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let after = rig.row(ANA, "3EB0LATE").await;
    let asks = rig.world.asked().iter().filter(|k| matches!(k, pb::command::Kind::MediaRetry(_))).count();
    assert_eq!(after["media"]["state"], "gone", "between {}; after {}", between["media"], after["media"]);
    assert_eq!(asks, 1, "the phone asked once");
}

/// The audit's K1: the phone's answer that it has no file, acked into the inbox before a stop and not
/// yet converted, meets the next start, which wants the copy again; the phone is off by then.
/// `ahead` inbox rows wait before the answer; `media_first` starts media before sync, as the rig
/// does, rather than after, as main.rs does. Says how the copy ended and how often the phone was
/// asked after the start.
fn answer_left_in_the_inbox(ahead: usize, media_first: bool) -> (String, Option<String>, usize) {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let lake = Picture::lake(640, 480);
    as_a_jawd(async {
        let store = Store::open(home.path()).unwrap();
        crate::sync::running(&store).await.unwrap();
        let event = World::default().photo(&lake, ANA, ANA, "3EB0LEFT", now_ms(), "");
        let Some(pb::event::Kind::Message(message)) = event.kind else { unreachable!() };
        let web = crate::convert::web_message(&message).unwrap();
        let me = crate::convert::Me::new([demo::ME.to_string()]);
        store.apply(Lane::Live, crate::convert::message(&web, &me, true)).await.unwrap();
        let (n, wanted): (i64, i64) = store
            .reads()
            .query(Priority::Interactive, |db| {
                Ok(db.query_row("SELECT message, wanted FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
            })
            .await
            .unwrap();
        let downloading = Step::Downloading { message: n, variant: "sd" };
        let asked = Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() };
        store.apply(Lane::Live, vec![Change::Media(downloading), Change::Media(asked)]).await.unwrap();
        for k in 0..ahead {
            let text = demo::text(ANA, ANA, &format!("3EB0AHEAD{k:05}"), now_ms(), "ahead of the answer");
            let Some(pb::event::Kind::Message(m)) = text.kind else { unreachable!() };
            store.inbox("message", now_ms(), m.encode_to_vec(), None);
        }
        let answer = pb::MediaRetried {
            id: "3EB0LEFT".into(),
            chat: ANA.into(),
            result: PhoneSaid::NotFound as i32,
            timestamp_ms: now_ms(),
            ..Default::default()
        };
        store.inbox("media_retry", now_ms(), answer.encode_to_vec(), None);
        for _ in 0..4_000 {
            if store.count("inbox").unwrap() as usize == ahead + 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(store.count("inbox").unwrap() as usize, ahead + 1, "the answer waits in the inbox");
    });
    as_a_jawd(async {
        let store = Store::open(home.path()).unwrap();
        crate::sync::running(&store).await.unwrap();
        let engine = DemoEngine::with(vec![demo::connected()]);
        let world = engine.world();
        world.phone(Phone::Silent);
        let _ = world.photo(&lake, ANA, ANA, "3EB0LEFT", now_ms(), "");
        world.expire(&lake.direct_path);
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let live: crate::sync::Shared = Arc::default();
        let (pushes, _) = broadcast::channel(256);
        let pipeline = jaw_media::Media::new(jaw_media::Config {
            previews: jaw_media::Durability::Derived,
            ..jaw_media::Config::default()
        });
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live.clone(),
            pushes.clone(),
            Some(supervisor.clone()),
            home.path().to_path_buf(),
            Default::default(),
            crate::feed::Typing::default(),
        );
        let config = Config { phone_wait: Duration::from_secs(60), ..quick() };
        let _media = if media_first {
            let media = Media::start_with(
                store.clone(),
                Some(supervisor.clone()),
                home.path(),
                &live,
                &pushes,
                config,
                pipeline,
            )
            .await
            .unwrap();
            tokio::spawn(sync);
            media
        } else {
            tokio::spawn(sync);
            Media::start_with(store.clone(), Some(supervisor.clone()), home.path(), &live, &pushes, config, pipeline)
                .await
                .unwrap()
        };
        let read = || {
            let reads = store.reads().clone();
            async move {
                reads
                    .query(Priority::Interactive, |db| {
                        Ok(db.query_row("SELECT state, error FROM media", [], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                        })?)
                    })
                    .await
                    .unwrap()
            }
        };
        let mut last = read().await;
        for _ in 0..1_500 {
            last = read().await;
            if !matches!(last.0.as_str(), "remote" | "downloading") && store.count("inbox").unwrap() == 0 {
                tokio::time::sleep(Duration::from_millis(1_500)).await;
                last = read().await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let asks = world.asked().iter().filter(|k| matches!(k, pb::command::Kind::MediaRetry(_))).count();
        (last.0, last.1, asks)
    })
}

#[test]
fn an_answer_left_in_the_inbox_at_a_stop_is_kept_when_media_starts_first() {
    let (state, error, asks) = answer_left_in_the_inbox(0, true);
    assert_eq!(state, "gone", "{error:?}, asked again {asks} times");
}

#[test]
fn an_answer_left_in_the_inbox_at_a_stop_is_kept_when_sync_starts_first() {
    let (state, error, asks) = answer_left_in_the_inbox(0, false);
    assert_eq!(state, "gone", "{error:?}, asked again {asks} times");
}

#[test]
fn an_answer_left_behind_a_backlog_at_a_stop_is_kept_when_sync_starts_first() {
    let (state, error, asks) = answer_left_in_the_inbox(2_000, false);
    assert_eq!(state, "gone", "{error:?}, asked again {asks} times");
}

/// An answer from the phone, as the engine hands it over.
fn answer(id: &str, result: PhoneSaid, direct_path: &str) -> pb::Event {
    pb::Event {
        seq: 0,
        kind: Some(pb::event::Kind::MediaRetried(pb::MediaRetried {
            id: id.into(),
            chat: ANA.into(),
            result: result as i32,
            direct_path: direct_path.into(),
            timestamp_ms: now_ms(),
            ..Default::default()
        })),
    }
}

impl Rig {
    /// Waits until everything delivered before now is converted: a text after it is in.
    async fn converted(&self, after: &str) {
        self.world.deliver(demo::text(ANA, ANA, after, now_ms(), "after")).await.unwrap();
        self.until(ANA, after, "converted", |row| !row.is_null()).await;
    }
}

/// The audit's K3: answers delivered twice, and an old one after a newer: each copy stays as the
/// phone first said, nothing downloads twice, and nothing's version moves.
#[tokio::test(flavor = "multi_thread")]
async fn answers_twice_or_late_change_nothing() {
    let rig = rig(Config { phone_wait: Duration::from_secs(600), ..quick() }).await;
    let lost = Picture::lake(640, 480);
    rig.world.expire(&lost.direct_path);
    rig.world.phone(Phone::Lost);
    rig.photo(&lost, ANA, ANA, "3EB0LOSTTWICE").await;
    rig.until(ANA, "3EB0LOSTTWICE", "gone", |row| {
        row["media"]["state"] == "gone" && row["media"]["thumbnail"].is_string()
    })
    .await;
    rig.until_nothing_wanted("settled").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let row = rig.row(ANA, "3EB0LOSTTWICE").await;
    rig.world.deliver(answer("3EB0LOSTTWICE", PhoneSaid::NotFound, "")).await.unwrap();
    rig.world.deliver(answer("3EB0LOSTTWICE", PhoneSaid::GeneralError, "")).await.unwrap();
    rig.converted("3EB0AFTER1").await;
    let again = rig.row(ANA, "3EB0LOSTTWICE").await;
    assert_eq!((&again["media"], &again["v"]), (&row["media"], &row["v"]), "a second answer changes nothing");

    rig.world.phone(Phone::Uploads);
    let up = Picture::lake(320, 240);
    rig.world.expire(&up.direct_path);
    rig.photo(&up, ANA, ANA, "3EB0UPTWICE").await;
    rig.until(ANA, "3EB0UPTWICE", "downloaded", previewed).await;
    rig.until_nothing_wanted("settled").await;
    let row = rig.row(ANA, "3EB0UPTWICE").await;
    let downloads = rig.world.downloads();
    rig.world.deliver(answer("3EB0UPTWICE", PhoneSaid::Success, "/v/t62/again-and-again")).await.unwrap();
    rig.world.deliver(answer("3EB0UPTWICE", PhoneSaid::NotFound, "")).await.unwrap();
    rig.converted("3EB0AFTER2").await;
    rig.until_nothing_wanted("nothing wanted again").await;
    let again = rig.row(ANA, "3EB0UPTWICE").await;
    assert_eq!(rig.world.downloads(), downloads, "no second download");
    assert_eq!((&again["media"], &again["v"]), (&row["media"], &row["v"]), "a late answer changes nothing");
}

/// The audit's K5: arrivals, history windows show and an interface's asks, all at once, some
/// sharing a file: every file downloads once, and every copy is done.
#[tokio::test(flavor = "multi_thread")]
async fn every_file_downloads_once_whatever_wants_it() {
    let rig = rig(quick()).await;
    rig.world.gate_downloads(true);
    let history: Vec<Picture> = (0..12).map(|n| Picture::lake(300 + n, 200)).collect();
    for (k, picture) in history.iter().enumerate() {
        rig.photo_from_history(picture, &format!("3EB0HIST{k}A")).await;
        rig.photo_from_history(picture, &format!("3EB0HIST{k}B")).await;
    }
    for k in 0..12 {
        rig.row(ANA, &format!("3EB0HIST{k}A")).await;
        rig.row(ANA, &format!("3EB0HIST{k}B")).await;
    }
    let arriving: Vec<Picture> = (0..10).map(|n| Picture::lake(500 + n, 400)).collect();
    for (k, picture) in arriving.iter().enumerate() {
        rig.photo(picture, RUI, RUI, &format!("3EB0LIVE{k}")).await;
    }
    for (k, picture) in history.iter().take(10).enumerate() {
        rig.photo(picture, RUI, RUI, &format!("3EB0SAME{k}")).await;
    }
    for k in [0, 3, 7] {
        rig.call("media.fetch", json!({ "message": format!("3EB0HIST{k}A") })).await.unwrap();
    }
    for k in [1, 4] {
        rig.call("media.fetch", json!({ "message": format!("3EB0LIVE{k}") })).await.unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    rig.world.gate_downloads(false);
    rig.until_nothing_wanted("everything done").await;
    let mut not_local = Vec::new();
    for k in 0..12 {
        for side in ["A", "B"] {
            let id = format!("3EB0HIST{k}{side}");
            if !local(&rig.row(ANA, &id).await) {
                not_local.push(id);
            }
        }
    }
    for k in 0..10 {
        for id in [format!("3EB0LIVE{k}"), format!("3EB0SAME{k}")] {
            if !local(&rig.row(RUI, &id).await) {
                not_local.push(id);
            }
        }
    }
    assert!(not_local.is_empty(), "{not_local:?}");
    assert_eq!(rig.world.downloads(), 22, "each of the 22 files once");
}

/// A photo at the store, as it arrived live, with the copy its message brings; its n and its want.
async fn stored_photo(store: &Store, id: &str, sha256: u8) -> (i64, i64) {
    let message = Message {
        chat: ANA.into(),
        id: id.into(),
        at: 1,
        sender: Some(ANA.into()),
        kind: "image",
        summary: "Photo".into(),
        live: true,
        media: Some(Box::new(crate::store::media::MediaCopy { sha256: Some(vec![sha256; 32]), ..Default::default() })),
        ..Message::default()
    };
    store.apply(Lane::Live, vec![Change::Message(message)]).await.unwrap();
    let id = id.to_string();
    store
        .reads()
        .query(Priority::Interactive, move |db| {
            let mine = "SELECT message, wanted FROM media WHERE message = (SELECT n FROM messages WHERE id = ?1)";
            Ok(db.query_row(mine, [id], |r| Ok((r.get(0)?, r.get(1)?)))?)
        })
        .await
        .unwrap()
}

/// The audit's K1b, K1 at the store: a start wants again a copy waiting for the phone, whose "no
/// file" answer waits in the inbox, converted after: the copy ends as the phone said, wanted by
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_converted_after_a_start_wanted_its_copy_again_still_settles_it() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (n, wanted) = stored_photo(&store, "3EB0RESTART", 5).await;
    let steps = [
        Step::Downloading { message: n, variant: "sd" },
        Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() },
        Step::Restarted { at: now_ms() },
        Step::Answered {
            chat: ANA.into(),
            id: "3EB0RESTART".into(),
            answer: crate::store::media::Answer::NotFound,
            at: now_ms(),
        },
    ];
    for step in steps {
        store.apply(Lane::Live, vec![Change::Media(step)]).await.unwrap();
    }
    let (state, wanted): (String, Option<i64>) = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT state, wanted FROM media", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
        })
        .await
        .unwrap();
    assert_eq!((state.as_str(), wanted), ("gone", None), "the phone's answer, converted after the start");
}

/// The phone uploads a file again, and a "no file" to an earlier ask is converted after that answer,
/// before the upload is downloaded: an upload answers every ask before it, so the copy is still
/// downloaded from where the phone uploaded it.
#[tokio::test(flavor = "multi_thread")]
async fn a_late_no_file_after_the_phone_uploaded_the_file_leaves_it_to_be_downloaded() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (n, wanted) = stored_photo(&store, "3EB0UPLOADED", 6).await;
    let answered = |answer| Step::Answered { chat: ANA.into(), id: "3EB0UPLOADED".into(), answer, at: now_ms() };
    let steps = [
        Step::Downloading { message: n, variant: "sd" },
        Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() },
        answered(crate::store::media::Answer::Uploaded("/v/t62/uploaded-again".into())),
        answered(crate::store::media::Answer::NotFound),
    ];
    for step in steps {
        store.apply(Lane::Live, vec![Change::Media(step)]).await.unwrap();
    }
    let (state, wanted, path): (String, Option<i64>, Option<String>) = store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT state, wanted, direct_path FROM media", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?)
        })
        .await
        .unwrap();
    assert_eq!((state.as_str(), wanted.is_some(), path.as_deref()), ("remote", true, Some("/v/t62/uploaded-again")));
}

/// The phone is asked for a photo's file, which another message with the same file downloads
/// meanwhile, and the owner opens the photo: the phone's "no file" then changes nothing, as an
/// upload would, so the copy shows its file with no error, still wanted until its job settles it.
#[tokio::test(flavor = "multi_thread")]
async fn a_no_file_for_a_copy_whose_file_is_already_in_place_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path()).unwrap();
    let (n, wanted) = stored_photo(&store, "3EB0ASKEDFOR", 7).await;
    stored_photo(&store, "3EB0SAMEFILE", 7).await;
    let steps = [
        Step::Downloading { message: n, variant: "sd" },
        Step::AskedPhone { message: n, variant: "sd", wanted, at: now_ms() },
        Step::Placed { sha256: vec![7; 32], path: "media/files/same.jpg".into(), bytes: 1_000, at: now_ms() },
        Step::Want { message: n, variant: "sd", at: now_ms(), asked: true },
    ];
    for step in steps {
        store.apply(Lane::Live, vec![Change::Media(step)]).await.unwrap();
    }
    let copy = || async {
        let copy: (String, Option<i64>, Option<String>, i64) = store
            .reads()
            .query(Priority::Interactive, move |db| {
                Ok(db.query_row(
                    "SELECT md.state, md.wanted, md.error, m.v FROM media md JOIN messages m ON m.n = md.message
                        WHERE md.message = ?1 AND md.variant = 'sd'",
                    [n],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?)
            })
            .await
            .unwrap();
        copy
    };
    let opened = copy().await;
    assert_eq!((opened.0.as_str(), opened.1.is_some(), opened.2.as_deref()), ("remote", true, None));
    let answer = crate::store::media::Answer::NotFound;
    let answered = Step::Answered { chat: ANA.into(), id: "3EB0ASKEDFOR".into(), answer, at: now_ms() };
    store.apply(Lane::Live, vec![Change::Media(answered)]).await.unwrap();
    assert_eq!(copy().await, opened, "the phone's late answer changes nothing");
}

/// The audit's K8: a photo jaw.db already held before migration 20, as an earlier jawd converted it:
/// its message and raw protobuf, and no copy, since no jawd before this one wrote one. Shown in a
/// window and asked for before the copies backfill has reached it, it's fetched, and the backfill,
/// reaching it, has nothing left to give it.
#[tokio::test(flavor = "multi_thread")]
async fn a_photo_stored_before_media_in_gets_a_copy_to_show_and_fetch() {
    let rig = rig(quick()).await;
    let lake = Picture::lake(640, 480);
    rig.photo_from_history(&lake, "3EB0BEFORE").await;
    let db = Connection::open(rig.home.path().join("jaw.db")).unwrap();
    db.busy_timeout(SIDE_WAITS).unwrap();
    db.execute("DELETE FROM media WHERE message = (SELECT n FROM messages WHERE id = '3EB0BEFORE')", []).unwrap();
    // The row migration 20 leaves on a jaw.db holding it, which this one, made empty, didn't get;
    // nothing takes its steps here until the test does.
    db.execute(
        "INSERT INTO backfills (name, after, upto) SELECT 'copies', 0, seq FROM sqlite_sequence WHERE name = 'messages'",
        [],
    )
    .unwrap();
    let row = rig.row(ANA, "3EB0BEFORE").await;
    assert!(row["media"].get("state").is_none(), "with no copy, it shows only what its message says: {row}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let asked = rig.call("media.fetch", json!({ "message": "3EB0BEFORE" })).await;
    assert!(asked.is_ok(), "media.fetch before the backfill reached it: {asked:?}");
    let row = rig.until(ANA, "3EB0BEFORE", "fetched and drawn", previewed).await;
    assert_eq!(std::fs::read(rig.file(&row["media"]["path"])).unwrap(), *lake.data);
    rig.until_nothing_wanted("settled").await;
    let settled = rig.row(ANA, "3EB0BEFORE").await;
    while rig.store.backfill_left(COPIES).unwrap() > 0 {
        rig.store.apply(Lane::Background, vec![Change::Backfill(COPIES)]).await.unwrap();
    }
    let copies: i64 = db.query_row("SELECT count(*) FROM media", [], |r| r.get(0)).unwrap();
    assert_eq!(copies, 1, "its copy, once");
    assert_eq!(rig.row(ANA, "3EB0BEFORE").await, settled, "the backfill changes nothing it shows");
}

/// Message `id` from Ana, a day old, as `raw`, as jawd keeps it.
fn kept_from_ana(id: &str, raw: &e2e::Message) -> wa_proto::wa_web_protobufs_web::WebMessageInfo {
    let Some(pb::event::Kind::Message(mut message)) = demo::text(ANA, ANA, id, now_ms() - 86_400_000, "").kind else {
        unreachable!("a text is a message")
    };
    message.raw = raw.encode_to_vec();
    crate::convert::web_message(&message).unwrap()
}

/// An event's message, as its raw protobuf says it.
fn raw_of(event: &pb::Event) -> e2e::Message {
    let Some(pb::event::Kind::Message(message)) = &event.kind else { unreachable!("a message") };
    e2e::Message::decode(message.raw.as_slice()).unwrap()
}

/// A file of `bytes` made of `seed`, its SHA-256 and where WhatsApp's server keeps it.
fn file_of(seed: u8, bytes: usize) -> (Arc<Vec<u8>>, Vec<u8>, String) {
    let data: Vec<u8> = (0..bytes).map(|n| (n as u8).wrapping_mul(31).wrapping_add(seed)).collect();
    let sha256 = <sha2::Sha256 as sha2::Digest>::digest(&data).to_vec();
    (Arc::new(data), sha256, format!("/v/t62.7119-24/old-{seed}.enc"))
}

/// A jaw.db made by a jawd before media in, at version 16, 17 or 18, holding a photo, a voice message, a
/// video, a document and a sticker, a photo to view once, one deleted, and the HD copy of the photo
/// kept as a message of its own, none with a copy of its file: once jawd starts on it, its backfill
/// gives each its copy, and each is fetched, but for those deleted or to view once, whose files jaw
/// doesn't keep.
#[tokio::test(flavor = "multi_thread")]
async fn files_stored_before_media_in_get_their_copies_at_the_upgrade_and_are_fetched() {
    let maker = World::default();
    let (photo, hd) = (Picture::lake(640, 480), Picture::lake(1600, 1200));
    let photo_raw = raw_of(&maker.photo(&photo, ANA, ANA, "3EB0OLDPHOTO", now_ms(), ""));
    let hd_raw = raw_of(&maker.photo(&hd, ANA, ANA, "3EB0OLDHD", now_ms(), ""));
    let gone_raw = raw_of(&maker.photo(&Picture::lake(320, 240), ANA, ANA, "3EB0OLDGONE", now_ms(), ""));
    let (voice, _, _) = file_of(1, 6_000);
    let voice_raw = raw_of(&maker.voice(voice.clone(), ANA, ANA, "3EB0OLDVOICE", now_ms(), 3));
    let voice_path = voice_raw.audio_message.as_ref().unwrap().direct_path.clone().unwrap();
    let mut files = vec![("3EB0OLDPHOTO", photo.data.clone()), ("3EB0OLDHD", hd.data.clone()), ("3EB0OLDVOICE", voice)];
    let mut serve = vec![(photo.direct_path.clone(), photo.data.clone()), (hd.direct_path.clone(), hd.data.clone())];
    serve.push((voice_path, files[2].1.clone()));
    let mut kept = |id: &'static str, seed: u8, raw: &dyn Fn(Vec<u8>, String) -> e2e::Message| {
        let (data, sha256, path) = file_of(seed, 30_000);
        serve.push((path.clone(), data.clone()));
        files.push((id, data));
        raw(sha256, path)
    };
    let keyed =
        |sha256: Vec<u8>| (Some(sha256.clone()), Some(sha256.iter().rev().copied().collect()), Some(vec![7u8; 32]));
    let video = kept("3EB0OLDVIDEO", 2, &|sha256, path| {
        let (file_sha256, file_enc_sha256, media_key) = keyed(sha256);
        let video = e2e::VideoMessage {
            mimetype: Some("video/mp4".into()),
            file_sha256,
            file_enc_sha256,
            media_key,
            direct_path: Some(path),
            file_length: Some(30_000),
            seconds: Some(2),
            ..Default::default()
        };
        e2e::Message { video_message: Some(Box::new(video)), ..Default::default() }
    });
    let document = kept("3EB0OLDDOC", 3, &|sha256, path| {
        let (file_sha256, file_enc_sha256, media_key) = keyed(sha256);
        let document = e2e::DocumentMessage {
            mimetype: Some("application/pdf".into()),
            file_sha256,
            file_enc_sha256,
            media_key,
            direct_path: Some(path),
            file_length: Some(30_000),
            file_name: Some("menu.pdf".into()),
            ..Default::default()
        };
        e2e::Message { document_message: Some(Box::new(document)), ..Default::default() }
    });
    let sticker = kept("3EB0OLDSTICKER", 4, &|sha256, path| {
        let (file_sha256, file_enc_sha256, media_key) = keyed(sha256);
        let sticker = e2e::StickerMessage {
            mimetype: Some("image/webp".into()),
            file_sha256,
            file_enc_sha256,
            media_key,
            direct_path: Some(path),
            file_length: Some(30_000),
            ..Default::default()
        };
        e2e::Message { sticker_message: Some(Box::new(sticker)), ..Default::default() }
    });
    let once = e2e::Message {
        view_once_message_v2: Some(Box::new(e2e::FutureProofMessage { message: Some(Box::new(photo_raw.clone())) })),
        ..Default::default()
    };
    let befores = || {
        let hd = Before {
            info: kept_from_ana("3EB0OLDHD", &hd_child(ANA, "3EB0OLDPHOTO", hd_raw.clone())),
            shown_as: Some(kept_from_ana("3EB0OLDHD", &hd_raw)),
            deleted: false,
        };
        vec![
            Before::kept(kept_from_ana("3EB0OLDPHOTO", &photo_raw)),
            Before::kept(kept_from_ana("3EB0OLDVOICE", &voice_raw)),
            Before::kept(kept_from_ana("3EB0OLDVIDEO", &video)),
            Before::kept(kept_from_ana("3EB0OLDDOC", &document)),
            Before::kept(kept_from_ana("3EB0OLDSTICKER", &sticker)),
            Before::kept(kept_from_ana("3EB0OLDONCE", &once)),
            Before { deleted: true, ..Before::kept(kept_from_ana("3EB0OLDGONE", &gone_raw)) },
            hd,
        ]
    };
    for version in [16, 17, 18] {
        let home = Arc::new(tempfile::tempdir().unwrap());
        before_media(home.path(), version, &befores());
        let rig = rig_in(home.clone(), quick()).await;
        // WhatsApp's server keeps the files, as it does for history not long gone.
        for (path, data) in &serve {
            rig.world.serve(path, data.clone());
        }
        let backfilled = tokio::time::timeout(Duration::from_secs(60), async {
            while rig.store.backfill_left(COPIES).unwrap() > 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        assert!(backfilled.await.is_ok(), "from version {version}, the backfill never ended");
        for (id, data) in &files {
            rig.call("media.fetch", json!({ "message": id })).await.unwrap();
            let row = rig.until(ANA, id, "fetched", local).await;
            assert_eq!(std::fs::read(rig.file(&row["media"]["path"])).unwrap(), **data, "from version {version}: {id}");
        }
        // The HD copy kept as a photo of its own is its photo's HD copy too.
        rig.call("media.fetch", json!({ "message": "3EB0OLDPHOTO", "variant": "hd" })).await.unwrap();
        let row = rig.until(ANA, "3EB0OLDPHOTO", "its HD copy", |row| row["media"]["hd_path"].is_string()).await;
        assert_eq!(std::fs::read(rig.file(&row["media"]["hd_path"])).unwrap(), *hd.data, "from version {version}");
        for id in ["3EB0OLDONCE", "3EB0OLDGONE"] {
            let refused = rig.call("media.fetch", json!({ "message": id })).await.unwrap_err();
            assert_eq!(refused.code, jaw_proto::code::NOT_ALLOWED, "from version {version}: {id}");
        }
        let copied: Vec<String> = rig
            .query(|db| {
                Ok(db
                    .prepare(
                        "SELECT m.id || ' ' || md.variant FROM media md JOIN messages m ON m.n = md.message
                        ORDER BY m.n, md.variant",
                    )?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .await;
        let expected = [
            "3EB0OLDPHOTO hd",
            "3EB0OLDPHOTO sd",
            "3EB0OLDVOICE sd",
            "3EB0OLDVIDEO sd",
            "3EB0OLDDOC sd",
            "3EB0OLDSTICKER sd",
            "3EB0OLDONCE sd",
            "3EB0OLDHD sd",
        ];
        assert_eq!(copied, expected, "from version {version}: a copy each but the deleted photo's");
    }
}

/// The group in the jaw.db `made_by_main` gives.
const MADE_GROUP: &str = "120363000000000042@g.us";

/// Files as WhatsApp's server holds them, by where.
type Served = Vec<(String, Arc<Vec<u8>>)>;

/// A jaw.db as jawd's own sync left it at f917587, before jawd kept media, from what an engine
/// handed over live and history kept raw, with a message of every kind that brings a file, and the
/// files WhatsApp's server held for them, by where it held them: `tests/fixtures/media-in/`, whose
/// `made-at-f917587.rs` has the scene.
fn made_by_main() -> (Arc<tempfile::TempDir>, Served) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media-in");
    let home = Arc::new(tempfile::tempdir().unwrap());
    let db = zstd::decode_all(std::fs::File::open(fixtures.join("made-by-main.db.zst")).unwrap()).unwrap();
    std::fs::write(home.path().join("jaw.db"), db).unwrap();
    let served = zstd::decode_all(std::fs::File::open(fixtures.join("served.json.zst")).unwrap()).unwrap();
    let served: std::collections::BTreeMap<String, String> = serde_json::from_slice(&served).unwrap();
    let files = served.into_iter().map(|(path, hex)| (path, Arc::new(crate::store::media::from_hex(&hex).unwrap())));
    (home, files.collect())
}

/// The audit's I7 on a jaw.db main's jawd made: once jawd starts on it, every message it holds
/// that brings a file has its copy, as one arriving now would, but those deleted; the HD copy kept
/// as a photo of its own is that photo's HD copy too, and one whose photo never came waits for it;
/// and each is fetched as WhatsApp's server keeps it, but the one to view once.
#[tokio::test(flavor = "multi_thread")]
async fn a_jaw_db_main_made_gets_the_copy_of_every_file_it_holds_and_each_is_fetched() {
    let (home, served) = made_by_main();
    let before: (i64, i64) = Connection::open(home.path().join("jaw.db"))
        .unwrap()
        .query_row("SELECT (SELECT user_version FROM pragma_user_version), (SELECT count(*) FROM media)", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(before, (17, 0), "main's jaw.db, at version 17, with no copy");
    let rig = rig_in(home.clone(), quick()).await;
    for (path, data) in &served {
        rig.world.serve(path, data.clone());
    }
    let backfilled = tokio::time::timeout(Duration::from_secs(60), async {
        while rig.store.backfill_left(COPIES).unwrap() > 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    assert!(backfilled.await.is_ok(), "the backfill never ended");
    let copied: Vec<(String, String, String, String)> = rig
        .query(|db| {
            Ok(db
                .prepare(
                    "SELECT c.id, m.id, md.variant, hex(md.sha256) FROM media md JOIN messages m ON m.n = md.message
                    JOIN chats c ON c.n = m.chat ORDER BY m.id, md.variant",
                )?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    let waiting: Vec<String> = rig
        .query(|db| {
            Ok(db
                .prepare("SELECT target || ' ' || kind FROM pending ORDER BY n")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .await;
    let mut expected: Vec<&str> = vec![
        "3EB0FXAUDIO sd",
        "3EB0FXDOC sd",
        "3EB0FXDOCCAP sd",
        "3EB0FXEPHEMERAL sd",
        "3EB0FXGIF sd",
        "3EB0FXGROUP sd",
        "3EB0FXHD sd",
        "3EB0FXLIVE sd",
        "3EB0FXMINE sd",
        "3EB0FXONCE sd",
        "3EB0FXORPHAN sd",
        "3EB0FXPHOTO hd",
        "3EB0FXPHOTO sd",
        "3EB0FXPTV sd",
        "3EB0FXSTICKER sd",
        "3EB0FXVIDEO sd",
        "3EB0FXVOICE sd",
    ];
    expected.sort_unstable();
    let named: Vec<String> = copied.iter().map(|(_, id, variant, _)| format!("{id} {variant}")).collect();
    assert_eq!(named, expected, "a copy for every file, but the one deleted for everyone's and the text's");
    assert_eq!(waiting, ["3EB0FXNOPARENT hd"], "the HD copy of a photo jaw.db never had waits for it");
    assert_eq!(rig.store.count("quarantine").unwrap(), 0);
    let file_of = |path: &Value| std::fs::read(rig.file(path)).unwrap();
    let hashed =
        |data: &[u8]| crate::store::media::to_hex(&<sha2::Sha256 as sha2::Digest>::digest(data)).to_uppercase();
    for (chat, id, variant, sha256) in copied.iter().filter(|(_, id, variant, _)| id != "3EB0FXONCE" && variant == "sd")
    {
        let chat = if chat == MADE_GROUP { MADE_GROUP } else { ANA };
        rig.call("media.fetch", json!({ "message": id })).await.unwrap();
        let row = rig.until(chat, id, "fetched", local).await;
        let data = file_of(&row["media"]["path"]);
        assert_eq!(&hashed(&data), sha256, "{id} {variant}: the file its copy names");
        assert!(served.iter().any(|(_, served)| **served == data), "{id}: a file WhatsApp's server held");
    }
    rig.call("media.fetch", json!({ "message": "3EB0FXPHOTO", "variant": "hd" })).await.unwrap();
    let row = rig.until(ANA, "3EB0FXPHOTO", "its HD copy", |row| row["media"]["hd_path"].is_string()).await;
    let hd = file_of(&row["media"]["hd_path"]);
    assert_eq!(hd, file_of(&rig.row(ANA, "3EB0FXHD").await["media"]["path"]), "the HD row's file");
    for (id, code) in [
        ("3EB0FXONCE", jaw_proto::code::NOT_ALLOWED),
        ("3EB0FXREVOKED", jaw_proto::code::NOT_ALLOWED),
        ("3EB0FXDELME", jaw_proto::code::NOT_FOUND),
        ("3EB0FXTEXT", jaw_proto::code::NOT_FOUND),
    ] {
        assert_eq!(rig.call("media.fetch", json!({ "message": id })).await.unwrap_err().code, code, "{id}");
    }
}

/// A view-once photo from Ana, `id`, with a caption.
async fn view_once_photo(rig: &Rig, picture: &Picture, id: &str) {
    let mut event = rig.world.photo(picture, ANA, ANA, id, now_ms(), "where we met");
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let photo = e2e::Message::decode(message.raw.as_slice()).unwrap();
        let once = e2e::FutureProofMessage { message: Some(Box::new(photo)) };
        message.raw = e2e::Message { view_once_message_v2: Some(Box::new(once)), ..Default::default() }.encode_to_vec();
    }
    rig.world.deliver(event).await.unwrap();
    rig.until(ANA, id, "converted", |row| !row.is_null()).await;
}

/// Waits until message `id` in Ana's chat has gone, and its files with it.
async fn gone_with_its_files(rig: &Rig, id: &str) {
    rig.until(ANA, id, "erased", Value::is_null).await;
    rig.media.inner.clean.notify_one();
    let count = |folder: &str| std::fs::read_dir(rig.home.path().join(folder)).map_or(0, |entries| entries.count());
    for _ in 0..500 {
        if count(FILES) == 0 && count(PREVIEWS) == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("its files stayed: {} in files, {} in previews", count(FILES), count(PREVIEWS));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_photo_opens_once_named_only_in_the_answer_and_goes_with_its_files_once_closed() {
    let rig = rig(quick()).await;
    view_once_photo(&rig, &Picture::lake(640, 480), "3EB0ONCE").await;
    let row = rig.row(ANA, "3EB0ONCE").await;
    assert_eq!((&row["view_once"], &row["spans"]), (&json!(true), &json!([])), "no caption shows: {row}");
    let mut ui = Interface::open(&rig.serve_sending().await).await;
    let hello = json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" });
    ui.call(1, "hello", hello.clone()).await;
    let opened = ui.call(2, "view_once.open", json!({ "message": "3EB0ONCE" })).await;
    let answer = &opened["result"];
    assert_eq!(answer["kind"], json!("image"), "{opened}");
    assert_eq!(answer["spans"], json!([{ "t": "text", "s": "where we met" }]), "its caption, once it's open");
    assert!(rig.file(&answer["path"]).is_file() && rig.file(&answer["media"]["previews"]["768"]).is_file(), "{answer}");
    assert!(answer["until"].as_i64().is_some_and(|until| until > now_ms() + 50_000), "a minute to show it: {answer}");
    let row = rig.row(ANA, "3EB0ONCE").await;
    assert!(row["media"]["path"].is_null() && row["media"]["previews"].is_null(), "no window gets its file: {row}");
    assert_eq!(
        ui.call(3, "view_once.open", json!({ "message": "3EB0ONCE" })).await["result"],
        *answer,
        "the same, again"
    );
    let mut other = Interface::open(&jaw_proto::local::socket(rig.home.path())).await;
    other.call(1, "hello", hello).await;
    let refused = other.call(2, "view_once.open", json!({ "message": "3EB0ONCE" })).await;
    assert_eq!(refused["error"]["code"], json!(jaw_proto::code::NOT_ALLOWED), "open in one interface at a time");
    let played = |asked: &pb::command::Kind| matches!(asked, pb::command::Kind::MarkRead(mark) if mark.r#type == "played" && mark.ids == ["3EB0ONCE"]);
    for _ in 0..500 {
        if rig.world.asked().iter().any(played) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(rig.world.asked().iter().any(played), "WhatsApp is told it was played");
    let status = ui.call(10, "status", Value::Null).await;
    let said = &status["result"]["disappearing"];
    assert_eq!((&said["open"], &said["open_most"], &said["erased"]), (&json!(1), &json!(16), &json!(0)), "{status}");
    assert_eq!(ui.call(4, "view_once.close", json!({ "message": "3EB0ONCE" })).await["result"], json!({}));
    gone_with_its_files(&rig, "3EB0ONCE").await;
    let status = ui.call(11, "status", Value::Null).await;
    let said = &status["result"]["disappearing"];
    assert_eq!((&said["open"], &said["erased"]), (&json!(0), &json!(1)), "opened ones count as erased: {status}");
    let again = ui.call(5, "view_once.open", json!({ "message": "3EB0ONCE" })).await;
    assert_eq!(again["error"]["code"], json!(jaw_proto::code::NOT_FOUND), "it's gone: {again}");
    assert_eq!(ui.call(6, "view_once.close", json!({ "message": "3EB0ONCE" })).await["result"], json!({}), "done");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_photo_isnt_downloaded_until_its_opened_as_a_photo_beside_it_is() {
    let rig = rig(quick()).await;
    view_once_photo(&rig, &Picture::lake(320, 240), "3EB0ONCE").await;
    rig.photo(&Picture::lake(200, 150), ANA, ANA, "3EB0PLAIN").await;
    rig.until(ANA, "3EB0PLAIN", "downloaded", local).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let files = std::fs::read_dir(rig.home.path().join(FILES)).unwrap().count();
    assert_eq!(files, 1, "the ordinary photo's file alone");
    assert!(!local(&rig.row(ANA, "3EB0ONCE").await), "the view-once photo stays where WhatsApp has it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disappearing_photos_file_and_previews_go_from_jaws_folder_as_it_does() {
    let rig = rig(quick()).await;
    let mut event = rig.world.photo(&Picture::lake(320, 240), ANA, ANA, "3EB0GOES", now_ms(), "gone tomorrow");
    if let Some(pb::event::Kind::Message(message)) = event.kind.as_mut() {
        let mut photo = e2e::Message::decode(message.raw.as_slice()).unwrap();
        if let Some(image) = photo.image_message.as_mut() {
            image.context_info = Some(Box::new(e2e::ContextInfo { expiration: Some(86_400), ..Default::default() }));
        }
        message.raw = photo.encode_to_vec();
    }
    rig.world.deliver(event).await.unwrap();
    let row = rig
        .until(ANA, "3EB0GOES", "downloaded with its previews", |row| {
            local(row) && row["media"]["previews"]["768"].is_string()
        })
        .await;
    let expires_at = row["expires_at"].as_i64().expect("it disappears");
    assert!(rig.file(&row["media"]["path"]).is_file() && rig.file(&row["media"]["previews"]["768"]).is_file());
    let batch = crate::expiry::erase_due(&rig.store, expires_at, &[]).await.unwrap();
    assert_eq!(batch.erased, 1);
    gone_with_its_files(&rig, "3EB0GOES").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_photo_goes_once_the_interface_that_opened_it_goes() {
    let rig = rig(quick()).await;
    view_once_photo(&rig, &Picture::lake(320, 240), "3EB0LEFT").await;
    let socket = rig.serve_sending().await;
    let until = {
        let mut ui = Interface::open(&socket).await;
        ui.call(1, "hello", json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" })).await;
        let opened = ui.call(2, "view_once.open", json!({ "message": "3EB0LEFT" })).await;
        assert!(opened["result"]["path"].is_string(), "{opened}");
        opened["result"]["until"].as_i64().expect("opened")
    };
    gone_with_its_files(&rig, "3EB0LEFT").await;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    assert!(now < until, "it went at its time, a minute after it was opened, not as its interface went");
}

/// How many view-once messages `status` says are open, asked as call `id`.
async fn open_now(ui: &mut Interface, id: u64) -> Value {
    ui.call(id, "status", Value::Null).await["result"]["disappearing"]["open"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_view_once_photo_erased_or_gone_while_open_is_open_no_more_and_closing_it_then_says_done() {
    let rig = rig(quick()).await;
    view_once_photo(&rig, &Picture::lake(320, 240), "3EB0TIMED").await;
    view_once_photo(&rig, &Picture::lake(320, 240), "3EB0VANISH").await;
    let mut ui = Interface::open(&rig.serve_sending().await).await;
    ui.call(1, "hello", json!({ "protocol": jaw_proto::PROTOCOL, "client": "test", "version": "0" })).await;
    let opened = ui.call(2, "view_once.open", json!({ "message": "3EB0TIMED" })).await;
    let until = opened["result"]["until"].as_i64().expect("opened");
    assert_eq!(open_now(&mut ui, 3).await, json!(1));
    // The timer gets there first, at `until`, as it often does before the interface closes it.
    assert_eq!(crate::expiry::erase_due(&rig.store, until, &[]).await.unwrap().erased, 1);
    assert_eq!(open_now(&mut ui, 4).await, json!(0), "its erasure ends its opening");
    assert_eq!(ui.call(5, "view_once.close", json!({ "message": "3EB0TIMED" })).await["result"], json!({}));
    let opened = ui.call(6, "view_once.open", json!({ "message": "3EB0VANISH" })).await;
    assert!(opened["result"]["path"].is_string(), "{opened}");
    {
        // Gone with nothing said of it, as one erased just before `view_once.close` looks.
        let db = rusqlite::Connection::open(rig.home.path().join("jaw.db")).unwrap();
        db.busy_timeout(SIDE_WAITS).unwrap();
        db.pragma_update(None, "foreign_keys", true).unwrap();
        db.execute("DELETE FROM messages WHERE id = '3EB0VANISH'", []).unwrap();
    }
    assert_eq!(open_now(&mut ui, 7).await, json!(1));
    assert_eq!(ui.call(8, "view_once.close", json!({ "message": "3EB0VANISH" })).await["result"], json!({}));
    assert_eq!(open_now(&mut ui, 9).await, json!(0), "closing one that's gone ends its opening");
}
