//! Voice, end to end: voice messages that arrive through the demo engine, downloaded into the cache
//! by media and played on jaw-voice's fake speaker, with the pushes every connection is sent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jaw_voice::fake::{FakeMicrophone, FakeSpeaker, SpeakerProbe};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, World};
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::media::Config;
use crate::store::{Change, Chat, Lane, Store};
use crate::work::Priority;

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
/// Four seconds of someone speaking, as a phone records a voice message: Ogg Opus at 16 kHz.
const SPEECH: &[u8] = include_bytes!("../../../jaw-voice/tests/fixtures/speech-16k.ogg");
/// How long a test waits for anything.
const PATIENCE: Duration = Duration::from_secs(60);

/// jawd's store, sync, media and voice on a folder of their own, with the demo engine on a world
/// of its own and jaw-voice's fake devices.
struct Rig {
    home: PathBuf,
    store: Store,
    world: Arc<World>,
    media: Media,
    voice: Voice,
    pushes: broadcast::Sender<Push>,
}

impl Rig {
    async fn on(home: &Path, speaker: FakeSpeaker) -> Rig {
        let world: Arc<World> = Arc::default();
        let store = Store::open(home).unwrap();
        crate::sync::running(&store).await.unwrap();
        let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let (pushes, _) = broadcast::channel(4096);
        let mut connected = pushes.subscribe();
        let config = Config { keep_free: 0, send_keep_free: 0, ..Config::default() };
        let media = Media::start(store.clone(), Some(supervisor.clone()), home, &live, &pushes, config).await.unwrap();
        let sync = crate::sync::run(
            incoming,
            store.clone(),
            live,
            pushes.clone(),
            Some(supervisor),
            home.into(),
            Default::default(),
            crate::feed::Typing::default(),
        );
        tokio::spawn(sync);
        let online = async {
            while let Ok(push) = connected.recv().await {
                if push.method == "connection" && push.params["state"] == "online" {
                    return;
                }
            }
        };
        tokio::time::timeout(PATIENCE, online).await.expect("the demo engine never connected");
        for chat in [ANA, RUI] {
            store.apply(Lane::Live, vec![Change::Chat(Chat { id: chat.into(), ..Chat::default() })]).await.unwrap();
        }
        let (microphone, _) = FakeMicrophone::new(48_000, Vec::new(), 1.0);
        let voice = Voice::start(media.clone(), pushes.clone(), speaker, microphone);
        Rig { home: home.to_path_buf(), store, world, media, voice, pushes }
    }

    /// A rig whose speaker takes audio `pace` times as fast as a real one, and what it was given.
    async fn paced(home: &Path, pace: f64) -> (Rig, SpeakerProbe) {
        let (speaker, probe) = FakeSpeaker::new(48_000, pace);
        (Rig::on(home, speaker).await, probe)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, jaw_proto::Error> {
        self.voice.call(method, params, 1).await
    }

    /// Message `id`'s row, as windows show it.
    async fn row(&self, chat: &str, id: &str) -> Value {
        let (chat, id) = (chat.to_string(), id.to_string());
        let row = self
            .store
            .reads()
            .query(Priority::Interactive, move |db| Ok(crate::store::read::message(db, &chat, &id)?.map(|(r, _)| r)))
            .await
            .unwrap();
        row.map_or(Value::Null, |row| serde_json::to_value(crate::feed::message_row(row)).unwrap())
    }

    /// Waits until message `id`'s row says what `done` wants, and returns it.
    async fn until(&self, chat: &str, id: &str, what: &str, done: impl Fn(&Value) -> bool) -> Value {
        let waited = tokio::time::timeout(PATIENCE, async {
            loop {
                let row = self.row(chat, id).await;
                if done(&row) {
                    return row;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        match waited.await {
            Ok(row) => row,
            Err(_) => panic!("{id} never came to {what}; it is {:?}", self.row(chat, id).await),
        }
    }

    /// Ana's voice message `id`, of `data`, arrived and taken in by jawd.
    async fn arrives(&self, id: &str, data: &[u8]) {
        let event = self.world.voice(Arc::new(data.to_vec()), ANA, ANA, id, now_ms(), 4);
        assert!(self.world.deliver(event).await.is_some(), "jawd took {id} in");
        self.until(ANA, id, "a row", |row| !row.is_null()).await;
    }

    /// Ana's voice message `id`, arrived and downloaded into the cache.
    async fn cached(&self, id: &str, data: &[u8]) -> Value {
        self.arrives(id, data).await;
        self.until(ANA, id, "in the cache", |row| row["media"]["state"] == "local").await
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

/// Reads pushes until a `playback` one that `done` wants, and returns every `playback` push read.
async fn playbacks(pushes: &mut broadcast::Receiver<Push>, what: &str, done: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut seen = Vec::new();
    let waited = tokio::time::timeout(PATIENCE, async {
        loop {
            match pushes.recv().await {
                Ok(push) if push.method == "playback" => {
                    let done = done(&push.params);
                    seen.push(push.params);
                    if done {
                        return;
                    }
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    });
    assert!(waited.await.is_ok(), "no playback push came to {what}; these came: {seen:?}");
    seen
}

fn state(push: &Value) -> &str {
    push["state"].as_str().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voice_message_in_the_cache_plays_with_its_playhead_and_ends_at_its_length() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::paced(home.path(), 2.0).await;
    let row = rig.cached("3EB0VOICE1", SPEECH).await;
    assert_eq!(row["voice"]["seconds"], 4, "its row says how long it is");
    let mut pushes = rig.pushes.subscribe();
    assert_eq!(rig.call("voice.play", json!({ "message": "3EB0VOICE1", "speed": 1 })).await.unwrap(), json!({}));
    let seen = playbacks(&mut pushes, "its end", |p| state(p) == "ended").await;
    let playing: Vec<&Value> = seen.iter().filter(|p| state(p) == "playing").collect();
    assert!(playing.len() >= 3, "its playhead is pushed as it plays: {seen:?}");
    assert!(seen.iter().all(|p| p["message"] == "3EB0VOICE1" && p["speed"] == 1.0), "{seen:?}");
    let ats: Vec<f64> = playing.iter().map(|p| p["at"].as_f64().unwrap()).collect();
    assert!(ats.windows(2).all(|w| w[0] <= w[1]) && ats[0] < 0.5, "from the start, forwards: {ats:?}");
    let end = seen.last().unwrap();
    let seconds = end["seconds"].as_f64().unwrap();
    assert!((4.2..4.3).contains(&seconds), "its length, 4.27 s, from its file: {end}");
    assert_eq!(end["at"], end["seconds"], "it ends at its length");
    assert!(end.get("recording").is_none(), "and it's no recording");
    assert!(heard.sound() > 0, "it was heard");
    assert_eq!(rig.voice.status().playing, None, "nothing plays once it's ended");
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_then_play_without_at_carries_on_and_seek_and_speed_move_the_playhead() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 1.0).await;
    rig.cached("3EB0VOICE1", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1", "at": 1.0, "speed": 1 })).await.unwrap();
    let started = playbacks(&mut pushes, "playing", |p| state(p) == "playing").await;
    assert!(started.last().unwrap()["at"].as_f64().unwrap() >= 1.0, "from where it was asked: {started:?}");
    let status = rig.voice.status();
    assert_eq!((status.playing.as_deref(), status.state.as_deref()), (Some("3EB0VOICE1"), Some("playing")));

    rig.call("voice.pause", json!({})).await.unwrap();
    let paused = playbacks(&mut pushes, "paused", |p| state(p) == "paused").await;
    let at = paused.last().unwrap()["at"].as_f64().unwrap();
    assert_eq!(rig.voice.status().state.as_deref(), Some("paused"));
    rig.call("voice.play", json!({ "message": "3EB0VOICE1", "speed": 1 })).await.unwrap();
    let resumed = playbacks(&mut pushes, "playing again", |p| state(p) == "playing").await;
    let again = resumed.last().unwrap()["at"].as_f64().unwrap();
    assert!((at..at + 0.5).contains(&again), "it carries on where it paused, at {at}: {again}");

    rig.call("voice.seek", json!({ "at": 3.0 })).await.unwrap();
    playbacks(&mut pushes, "the seek", |p| state(p) == "playing" && (3.0..3.5).contains(&p["at"].as_f64().unwrap()))
        .await;
    rig.call("voice.speed", json!({ "speed": 2 })).await.unwrap();
    playbacks(&mut pushes, "the speed", |p| p["speed"] == 2.0).await;
    let ended = playbacks(&mut pushes, "its end", |p| state(p) == "ended").await;
    assert_eq!(ended.last().unwrap()["speed"], 2.0);

    // A seek past the end ends it.
    rig.call("voice.play", json!({ "message": "3EB0VOICE1", "speed": 1 })).await.unwrap();
    playbacks(&mut pushes, "playing from the start", |p| state(p) == "playing").await;
    rig.call("voice.seek", json!({ "at": 60.0 })).await.unwrap();
    playbacks(&mut pushes, "its end", |p| state(p) == "ended").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn one_not_in_the_cache_is_downloaded_first_then_played() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 4.0).await;
    rig.world.gate_downloads(true);
    rig.arrives("3EB0VOICE1", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1", "at": 0.5, "speed": 1.5 })).await.unwrap();
    let asked: bool = rig
        .store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row(
                "SELECT asked FROM media WHERE message = (SELECT n FROM messages WHERE id = '3EB0VOICE1')",
                [],
                |r| r.get(0),
            )?)
        })
        .await
        .unwrap();
    assert!(asked, "its file is asked for, ahead of what downloads by itself");
    assert_eq!(rig.voice.status().state.as_deref(), Some("waiting"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(pushes.try_recv().is_err(), "nothing plays before its file is in");
    rig.world.gate_downloads(false);
    let seen = playbacks(&mut pushes, "playing", |p| state(p) == "playing").await;
    let first = &seen[0];
    assert_eq!((&first["message"], &first["speed"]), (&json!("3EB0VOICE1"), &json!(1.5)));
    assert!(first["at"].as_f64().unwrap() >= 0.5, "from where it was asked: {first}");
    playbacks(&mut pushes, "its end", |p| state(p) == "ended").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_play_waiting_for_a_download_that_stops_is_pushed_failed_with_why() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 4.0).await;
    rig.world.gate_downloads(true);
    rig.arrives("3EB0VOICE1", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await.unwrap();
    rig.media.call("media.cancel", json!({ "message": "3EB0VOICE1" })).await.unwrap();
    let seen = playbacks(&mut pushes, "failed", |p| state(p) == "failed").await;
    let failed = seen.last().unwrap();
    assert_eq!(failed["message"], "3EB0VOICE1");
    assert!(failed["error"].as_str().unwrap().contains("cancelled"), "{failed}");
    assert_eq!(rig.voice.status().playing, None);
    rig.world.gate_downloads(false);
}

#[tokio::test(flavor = "multi_thread")]
async fn one_message_plays_at_a_time_a_new_play_stopping_the_last_or_one_waiting_for_its_file() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 1.0).await;
    rig.cached("3EB0VOICE1", SPEECH).await;
    rig.cached("3EB0VOICE2", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await.unwrap();
    playbacks(&mut pushes, "playing", |p| state(p) == "playing").await;
    rig.call("voice.play", json!({ "message": "3EB0VOICE2" })).await.unwrap();
    let seen = playbacks(&mut pushes, "the second", |p| p["message"] == "3EB0VOICE2").await;
    assert!(seen.iter().any(|p| p["message"] == "3EB0VOICE1" && state(p) == "stopped"), "the first stops: {seen:?}");

    // A play waiting for its file gives way to a newer one, and is pushed stopped.
    rig.world.gate_downloads(true);
    rig.arrives("3EB0VOICE3", SPEECH).await;
    rig.call("voice.play", json!({ "message": "3EB0VOICE3" })).await.unwrap();
    playbacks(&mut pushes, "the second stopped", |p| p["message"] == "3EB0VOICE2" && state(p) == "stopped").await;
    rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await.unwrap();
    let seen =
        playbacks(&mut pushes, "the first again", |p| p["message"] == "3EB0VOICE1" && state(p) == "playing").await;
    assert!(seen.iter().any(|p| p["message"] == "3EB0VOICE3" && state(p) == "stopped"), "the wait stops: {seen:?}");
    rig.world.gate_downloads(false);
    rig.until(ANA, "3EB0VOICE3", "in the cache", |row| row["media"]["state"] == "local").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let status = rig.voice.status();
    assert_eq!(status.playing.as_deref(), Some("3EB0VOICE1"), "and nothing it waited for plays once it's in");
    rig.call("voice.pause", json!({})).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn no_audio_output_or_a_file_that_isnt_a_voice_message_is_pushed_failed_with_why() {
    let home = tempfile::tempdir().unwrap();
    let rig = Rig::on(home.path(), FakeSpeaker::unavailable()).await;
    rig.cached("3EB0VOICE1", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await.unwrap();
    let seen = playbacks(&mut pushes, "failed", |p| state(p) == "failed").await;
    assert!(seen.last().unwrap()["error"].as_str().unwrap().contains("no audio output"), "{seen:?}");

    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 4.0).await;
    rig.cached("3EB0NOTVOICE", b"not an Ogg file, whatever its message says").await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0NOTVOICE" })).await.unwrap();
    let seen = playbacks(&mut pushes, "failed", |p| state(p) == "failed").await;
    let failed = seen.last().unwrap();
    assert_eq!(failed["message"], "3EB0NOTVOICE");
    assert!(failed["error"].as_str().unwrap().contains("isn't a voice message"), "{failed}");
    assert_eq!(rig.voice.status().playing, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn what_cant_be_played_is_refused_as_the_protocol_says() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 4.0).await;
    rig.cached("3EB0VOICE1", SPEECH).await;
    let refused = |answer: Result<Value, jaw_proto::Error>| answer.expect_err("refused");
    let missing = refused(rig.call("voice.play", json!({ "message": "3EB0NOSUCH" })).await);
    assert_eq!(missing.code, code::NOT_FOUND, "{missing:?}");
    for (params, param) in [
        (json!({ "message": "3EB0VOICE1", "speed": 3 }), "speed"),
        (json!({ "message": "3EB0VOICE1", "at": -1 }), "at"),
        (json!({}), "message"),
    ] {
        let e = refused(rig.call("voice.play", params).await);
        assert_eq!((e.code, e.data.as_ref().map(|d| d["param"].clone())), (code::INVALID_PARAMS, Some(json!(param))));
    }
    let e = refused(rig.call("voice.speed", json!({ "speed": 1.25 })).await);
    assert_eq!(e.code, code::INVALID_PARAMS);
    let e = refused(rig.call("voice.seek", json!({ "at": "soon" })).await);
    assert_eq!(e.code, code::INVALID_PARAMS);

    // A text message isn't a voice message.
    let text = demo::text(ANA, ANA, "3EB0TEXT", now_ms(), "olá");
    rig.world.deliver(text).await.unwrap();
    rig.until(ANA, "3EB0TEXT", "a row", |row| !row.is_null()).await;
    let e = refused(rig.call("voice.play", json!({ "message": "3EB0TEXT" })).await);
    assert_eq!(e.code, code::NOT_ALLOWED, "{e:?}");

    // The same ID in two chats needs its chat.
    let event = rig.world.voice(Arc::new(SPEECH.to_vec()), RUI, RUI, "3EB0VOICE1", now_ms(), 4);
    rig.world.deliver(event).await.unwrap();
    rig.until(RUI, "3EB0VOICE1", "in the cache", |row| row["media"]["state"] == "local").await;
    let e = refused(rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await);
    assert_eq!((e.code, e.data.map(|d| d["param"].clone())), (code::INVALID_PARAMS, Some(json!("chat"))));
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.play", json!({ "message": "3EB0VOICE1", "chat": RUI })).await.unwrap();
    playbacks(&mut pushes, "Rui's playing", |p| state(p) == "playing").await;
    rig.call("voice.pause", json!({})).await.unwrap();

    // Deleted, or to view once, it has no file to play.
    for (column, id) in [("deleted", "3EB0GONE"), ("view_once", "3EB0ONCE")] {
        rig.cached(id, SPEECH).await;
        let db = rusqlite::Connection::open(rig.home.join("jaw.db")).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        db.execute(&format!("UPDATE messages SET {column} = 1 WHERE id = ?1"), [id]).unwrap();
        let e = refused(rig.call("voice.play", json!({ "message": id })).await);
        assert_eq!(e.code, code::NOT_ALLOWED, "{column}: {e:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_connection_plays_stops_when_it_ends_unless_another_has_played_since() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::paced(home.path(), 1.0).await;
    rig.cached("3EB0VOICE1", SPEECH).await;
    let mut pushes = rig.pushes.subscribe();
    rig.voice.call("voice.play", json!({ "message": "3EB0VOICE1" }), 7).await.unwrap();
    playbacks(&mut pushes, "playing", |p| state(p) == "playing").await;
    rig.voice.ended(8);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.voice.status().playing.as_deref(), Some("3EB0VOICE1"), "another connection's end changes nothing");
    rig.voice.ended(7);
    playbacks(&mut pushes, "stopped", |p| state(p) == "stopped").await;
    assert_eq!(rig.voice.status().playing, None);
}
