//! Voice, end to end: voice messages that arrive through the demo engine, downloaded into the cache
//! by media and played on jaw-voice's fake speaker, with the pushes every connection is sent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jaw_voice::fake::{FakeMicrophone, FakeSpeaker, MicrophoneProbe, SpeakerProbe};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::engine::demo::{self, DemoEngine, World};
use crate::engine::pb;
use crate::engine::supervisor::{Backoff, Supervisor};
use crate::send::{Makers, Timing};
use crate::store::{Change, Chat, Lane, Store};
use crate::work::Priority;

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
/// Four seconds of someone speaking, as a phone records a voice message: Ogg Opus at 16 kHz.
const SPEECH: &[u8] = include_bytes!("../../../jaw-voice/tests/fixtures/speech-16k.ogg");
/// How long a test waits for anything.
const PATIENCE: Duration = Duration::from_secs(60);
/// How often typing is said again while it goes on, and how long after it was last said it stops,
/// shortened so a test sees both.
const TYPING_AGAIN: Duration = Duration::from_millis(200);
const TYPING_QUIET: Duration = Duration::from_millis(500);

/// jawd's store, sync, media and voice on a folder of their own, with the demo engine on a world
/// of its own and jaw-voice's fake devices.
struct Rig {
    home: PathBuf,
    store: Store,
    world: Arc<World>,
    media: Media,
    voice: Voice,
    pushes: broadcast::Sender<Push>,
    /// The free space media takes the disk to have.
    free: Arc<std::sync::atomic::AtomicU64>,
}

impl Rig {
    async fn on(home: &Path, speaker: FakeSpeaker) -> Rig {
        let (microphone, _) = FakeMicrophone::new(48_000, Vec::new(), 1.0);
        Rig::with(home, speaker, microphone, super::Config::default()).await
    }

    async fn with(home: &Path, speaker: FakeSpeaker, microphone: FakeMicrophone, config: super::Config) -> Rig {
        let world: Arc<World> = Arc::default();
        let store = Store::open(home).unwrap();
        crate::sync::running(&store).await.unwrap();
        let engine = DemoEngine::sharing(vec![demo::connected()], world.clone());
        let (events, incoming) = mpsc::channel(1024);
        let supervisor = Supervisor::spawn(Arc::new(engine), events, Backoff::default());
        let live: crate::sync::Shared = Arc::new(std::sync::Mutex::new(crate::sync::starting(&store, true)));
        let (pushes, _) = broadcast::channel(4096);
        let mut connected = pushes.subscribe();
        let free = Arc::new(std::sync::atomic::AtomicU64::new(100_000_000_000));
        let media_config = crate::media::Config {
            keep_free: 0,
            send_keep_free: 0,
            free: Some(free.clone()),
            ..crate::media::Config::default()
        };
        let media =
            Media::start(store.clone(), Some(supervisor.clone()), home, &live, &pushes, media_config).await.unwrap();
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
            Timing {
                retry_first: Duration::from_millis(5),
                retry_most: Duration::from_millis(20),
                typing_again: TYPING_AGAIN,
                typing_quiet: TYPING_QUIET,
                ..Timing::default()
            },
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
        tokio::time::timeout(PATIENCE, online).await.expect("the demo engine never connected");
        for chat in [ANA, RUI] {
            store.apply(Lane::Live, vec![Change::Chat(Chat { id: chat.into(), ..Chat::default() })]).await.unwrap();
        }
        let voice =
            Voice::start(home, store.clone(), media.clone(), sending, pushes.clone(), speaker, microphone, config)
                .unwrap();
        Rig { home: home.to_path_buf(), store, world, media, voice, pushes, free }
    }

    /// A rig whose microphone delivers `samples` at 48 kHz, `pace` times as fast as a real one,
    /// recording for at most `limit`, and how far the microphone has got.
    async fn recording(home: &Path, samples: Vec<f32>, pace: f64, limit: Duration) -> (Rig, MicrophoneProbe) {
        let (microphone, heard) = FakeMicrophone::new(48_000, samples, pace);
        let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
        let config = super::Config { limit, ..super::Config::default() };
        (Rig::with(home, speaker, microphone, config).await, heard)
    }

    /// The files in the recordings folder, by name.
    fn recordings(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.home.join(folder::RECORDINGS))
            .map(|entries| entries.filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned())).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// What the demo's WhatsApp was told of typing and recording, by chat and state, in order.
    fn said(&self) -> Vec<(String, i32)> {
        self.world
            .asked()
            .into_iter()
            .filter_map(|kind| match kind {
                pb::command::Kind::ChatPresence(p) => Some((p.chat, p.state)),
                _ => None,
            })
            .collect()
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

/// `seconds` of a 440 Hz tone at 48 kHz, as a microphone hears someone humming.
fn tone(seconds: f64) -> Vec<f32> {
    (0..(seconds * 48_000.0) as usize)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5)
        .collect()
}

/// Reads pushes until a `recording` one that `done` wants, and returns every `recording` push read.
async fn recordings(pushes: &mut broadcast::Receiver<Push>, what: &str, done: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut seen = Vec::new();
    let waited = tokio::time::timeout(PATIENCE, async {
        loop {
            match pushes.recv().await {
                Ok(push) if push.method == "recording" => {
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
    assert!(waited.await.is_ok(), "no recording push came to {what}; these came: {seen:?}");
    seen
}

/// Waits until the fake microphone has given everything it has.
async fn heard_all(heard: &MicrophoneProbe) {
    let waited = tokio::time::timeout(PATIENCE, async {
        while !heard.finished() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(waited.await.is_ok(), "the microphone never finished");
}

const RECORDING_SAID: i32 = pb::chat_presence::State::Recording as i32;
const PAUSED_SAID: i32 = pb::chat_presence::State::Paused as i32;

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_pushes_its_levels_then_stopped_with_its_waveform_its_file_whole_before_stop_answers() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(2.0), 2.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    assert_eq!(rig.call("voice.record", json!({ "chat": ANA })).await.unwrap(), json!({}));
    let status = rig.voice.status();
    assert_eq!(status.recording.as_deref(), Some("recording"), "it's started once it's answered");
    let levels = recordings(&mut pushes, "a level", |p| p["level"].as_u64().is_some_and(|l| l > 0)).await;
    assert!(levels.iter().all(|p| p["state"] == "recording"), "{levels:?}");
    heard_all(&heard).await;
    assert_eq!(rig.call("voice.stop", json!({})).await.unwrap(), json!({}));
    // Answered once it's whole under its name, which is the ID made for its message.
    let names = rig.recordings();
    let [name] = names.as_slice() else { panic!("one recording, whole: {names:?}") };
    assert!(name.starts_with("3EB0") && name.ends_with(".ogg"), "{name}");
    let file = jaw_voice::VoiceFile::open(&rig.home.join(folder::RECORDINGS).join(name)).unwrap();
    assert!((1.8..2.2).contains(&file.duration().as_secs_f64()), "{:?}", file.duration());
    let seen = recordings(&mut pushes, "stopped", |p| p["state"] == "stopped").await;
    let stopped = seen.last().unwrap();
    assert_eq!(stopped["end"], "stopped");
    assert_eq!(stopped["waveform"].as_array().map(Vec::len), Some(64), "{stopped}");
    assert!((1.8..2.2).contains(&stopped["seconds"].as_f64().unwrap()), "{stopped}");
    let status = rig.voice.status();
    assert_eq!((status.recording.as_deref(), status.files), (Some("stopped"), 1));
    assert!(status.hard_links && !status.silent);
    // Stopped already, it stays as it is.
    assert_eq!(rig.call("voice.stop", json!({})).await.unwrap(), json!({}));
    assert_eq!(rig.recordings(), names);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stopped_recording_plays_to_listen_to_and_cancel_throws_it_away() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let e = rig.call("voice.play", json!({ "recording": true })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "it's listened to once it's stopped: {e:?}");
    let e = rig.call("voice.play", json!({ "message": "3EB0VOICE1" })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "and nothing else plays while it records: {e:?}");
    heard_all(&heard).await;
    rig.call("voice.stop", json!({})).await.unwrap();
    rig.call("voice.play", json!({ "recording": true, "speed": 1 })).await.unwrap();
    let seen = playbacks(&mut pushes, "its end", |p| p["state"] == "ended").await;
    assert!(seen.iter().all(|p| p["recording"] == true && p.get("message").is_none()), "{seen:?}");
    assert_eq!(rig.recordings().len(), 1, "it's kept, to send");

    rig.call("voice.play", json!({ "recording": true })).await.unwrap();
    playbacks(&mut pushes, "playing", |p| p["state"] == "playing").await;
    assert_eq!(rig.call("voice.cancel", json!({})).await.unwrap(), json!({}));
    assert!(rig.recordings().is_empty(), "thrown away, its file goes: {:?}", rig.recordings());
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    playbacks(&mut pushes, "listening stopped", |p| p["state"] == "stopped" && p["recording"] == true).await;
    let status = rig.voice.status();
    assert_eq!((status.recording, status.playing, status.files), (None, None, 0));
    let e = rig.call("voice.play", json!({ "recording": true })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_FOUND, "there's none to listen to now");
    let e = rig.call("voice.stop", json!({})).await.unwrap_err();
    assert_eq!(e.code, code::NOT_FOUND, "nor to stop");
    assert_eq!(rig.call("voice.cancel", json!({})).await.unwrap(), json!({}), "and throwing it away again is done");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_thrown_away_while_it_goes_leaves_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::recording(home.path(), tone(30.0), 1.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    recordings(&mut pushes, "a level", |p| p["state"] == "recording").await;
    rig.call("voice.cancel", json!({})).await.unwrap();
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    assert!(rig.recordings().is_empty(), "nothing left: {:?}", rig.recordings());
    assert_eq!(rig.voice.status().recording, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_microphone_failing_midway_ends_interrupted_keeping_what_came_before() {
    let home = tempfile::tempdir().unwrap();
    let (microphone, _) = FakeMicrophone::new(48_000, tone(3.0), 2.0);
    let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
    let rig = Rig::with(home.path(), speaker, microphone.failing_after(48_000), super::Config::default()).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let seen = recordings(&mut pushes, "stopped", |p| p["state"] == "stopped").await;
    let stopped = seen.last().unwrap();
    assert_eq!(stopped["end"], "interrupted");
    assert!(stopped["reason"].as_str().unwrap().contains("unplugged"), "{stopped}");
    assert!((0.8..1.2).contains(&stopped["seconds"].as_f64().unwrap()), "what came before is kept: {stopped}");
    assert_eq!(rig.recordings().len(), 1);
    assert_eq!(rig.voice.status().recording.as_deref(), Some("stopped"));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_microphone_is_refused_and_leaves_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
    let rig = Rig::with(home.path(), speaker, FakeMicrophone::unavailable(), super::Config::default()).await;
    let e = rig.call("voice.record", json!({ "chat": ANA })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED);
    assert!(e.message.contains("no microphone"), "{e:?}");
    assert!(rig.recordings().is_empty());
    assert_eq!(rig.voice.status().recording, None);
    assert!(rig.said().is_empty(), "WhatsApp isn't told you're recording: {:?}", rig.said());
}

#[tokio::test(flavor = "multi_thread")]
async fn digital_silence_is_pushed_silent_and_status_keeps_it() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), vec![0.0; 48_000 * 2], 1.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    recordings(&mut pushes, "silent", |p| p["state"] == "silent").await;
    assert!(rig.voice.status().silent, "status says it heard nothing");
    heard_all(&heard).await;
    rig.call("voice.stop", json!({})).await.unwrap();
    assert!(rig.voice.status().silent, "and still does once it's stopped");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_stops_itself_at_its_limit() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::recording(home.path(), tone(5.0), 2.0, Duration::from_secs(1)).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let seen = recordings(&mut pushes, "stopped", |p| p["state"] == "stopped").await;
    let stopped = seen.last().unwrap();
    assert_eq!(stopped["end"], "limit", "{stopped}");
    assert!((0.9..1.1).contains(&stopped["seconds"].as_f64().unwrap()), "{stopped}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_recording_a_missing_chat_and_a_full_disk_are_refused() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let e = rig.call("voice.record", json!({ "chat": RUI })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "already recording: {e:?}");
    heard_all(&heard).await;
    rig.call("voice.stop", json!({})).await.unwrap();
    let e = rig.call("voice.record", json!({ "chat": ANA })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "one stopped waits to be sent or thrown away: {e:?}");
    rig.call("voice.cancel", json!({})).await.unwrap();

    let e = rig.call("voice.record", json!({ "chat": "351990000000@s.whatsapp.net" })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_FOUND, "{e:?}");
    rig.free.store(10_000_000, std::sync::atomic::Ordering::Relaxed);
    let e = rig.call("voice.record", json!({ "chat": ANA })).await.unwrap_err();
    assert_eq!((e.code, e.data.map(|d| d["limit"].clone())), (code::LIMIT, Some(json!("disk"))));
    assert!(rig.recordings().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_empties_the_recordings_folder_of_what_a_stopped_jawd_left() {
    let home = tempfile::tempdir().unwrap();
    let folder = home.path().join(folder::RECORDINGS);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("3EB0LEFT.ogg"), SPEECH).unwrap();
    std::fs::write(jaw_voice::part_path(&folder.join("3EB0HALF.ogg")), b"OggS").unwrap();
    let (rig, _) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    assert!(rig.recordings().is_empty(), "{:?}", rig.recordings());
    let status = rig.voice.status();
    assert_eq!((status.removed_at_start, status.files), (2, 0));
    let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&folder).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o700, "only you can read it");
}

#[tokio::test(flavor = "multi_thread")]
async fn whatsapp_is_told_youre_recording_while_it_lasts_and_no_longer_once_it_ends() {
    let home = tempfile::tempdir().unwrap();
    let (microphone, heard) = FakeMicrophone::new(48_000, tone(2.0), 1.0);
    let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
    let config = super::Config { recording_again: Duration::from_millis(100), ..super::Config::default() };
    let rig = Rig::with(home.path(), speaker, microphone, config).await;
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    heard_all(&heard).await;
    // Said again as typing is, and never stopped by typing's quiet spell while it lasts.
    let said = rig.said();
    assert!(said.len() >= 2 && said.iter().all(|s| *s == (ANA.to_string(), RECORDING_SAID)), "{said:?}");
    rig.call("voice.stop", json!({})).await.unwrap();
    let waited = tokio::time::timeout(PATIENCE, async {
        while rig.said().last() != Some(&(ANA.to_string(), PAUSED_SAID)) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    assert!(waited.await.is_ok(), "stopped is said once it ends: {:?}", rig.said());
    let ended = rig.said().len();
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(rig.said().len(), ended, "and nothing more: {:?}", rig.said());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_ending_throws_its_recording_away() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.voice.call("voice.record", json!({ "chat": ANA }), 7).await.unwrap();
    heard_all(&heard).await;
    rig.voice.call("voice.stop", json!({}), 7).await.unwrap();
    rig.voice.ended(8);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.recordings().len(), 1, "another connection's end changes nothing");
    rig.voice.ended(7);
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    assert!(rig.recordings().is_empty());
    assert_eq!(rig.voice.status().recording, None);
}
