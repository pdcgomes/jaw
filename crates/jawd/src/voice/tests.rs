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
use crate::store::expiry::tests::{
    bytes_gone_but_for, free_holding, pages_holding, settled, several_of_each, still_found,
};
use crate::store::{Amend, Amendment, Change, Chat, Lane, Store};
use crate::work::Priority;

const ANA: &str = "351912345678@s.whatsapp.net";
const RUI: &str = "351913333333@s.whatsapp.net";
/// Four seconds of someone speaking, as a phone records a voice message: Ogg Opus at 16 kHz.
const SPEECH: &[u8] = include_bytes!("../../../jaw-voice/tests/fixtures/speech-16k.ogg");
/// How long a test waits for anything.
const PATIENCE: Duration = Duration::from_secs(60);
/// How long a test's own connection to jaw.db waits for the write lock beside a running jawd,
/// whose writer, on a busy 2-core machine, can keep it more than 5 s. Its transactions take the
/// lock as they begin, before any read: a deferred one's first insert has met a snapshot a commit
/// of jawd's made stale (SQLITE_BUSY_SNAPSHOT), which no wait cures.
const SIDE_WAITS: Duration = Duration::from_secs(60);
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
    sending: Sending,
    /// The free space media takes the disk to have.
    free: Arc<std::sync::atomic::AtomicU64>,
}

impl Rig {
    async fn on(home: &Path, speaker: FakeSpeaker) -> Rig {
        let (microphone, _) = FakeMicrophone::new(48_000, Vec::new(), 1.0);
        Rig::with(home, speaker, microphone, super::Config::default()).await
    }

    async fn with(home: &Path, speaker: FakeSpeaker, microphone: FakeMicrophone, config: super::Config) -> Rig {
        let pipeline = jaw_media::Media::new(jaw_media::Config {
            previews: jaw_media::Durability::Derived,
            ..jaw_media::Config::default()
        });
        Rig::piped(home, speaker, microphone, config, pipeline).await
    }

    async fn piped(
        home: &Path,
        speaker: FakeSpeaker,
        microphone: FakeMicrophone,
        config: super::Config,
        pipeline: jaw_media::Media,
    ) -> Rig {
        let world: Arc<World> = Arc::default();
        world.replying(demo::Replies { delivered: Duration::from_millis(5), read: Duration::from_millis(5) });
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
            Media::start_with(store.clone(), Some(supervisor.clone()), home, &live, &pushes, media_config, pipeline)
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
        let voice = Voice::start(
            home,
            store.clone(),
            media.clone(),
            sending.clone(),
            pushes.clone(),
            speaker,
            microphone,
            config,
        )
        .unwrap();
        Rig { home: home.to_path_buf(), store, world, media, voice, pushes, sending, free }
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
    status_when(&rig, "nothing plays once it's ended", |s| s.playing.is_none()).await;
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
    let plays = |s: &VoiceStatus| (s.playing.as_deref(), s.state.as_deref()) == (Some("3EB0VOICE1"), Some("playing"));
    status_when(&rig, "it plays", plays).await;

    rig.call("voice.pause", json!({})).await.unwrap();
    let paused = playbacks(&mut pushes, "paused", |p| state(p) == "paused").await;
    let at = paused.last().unwrap()["at"].as_f64().unwrap();
    status_when(&rig, "it's paused", |s| s.state.as_deref() == Some("paused")).await;
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
    status_when(&rig, "it waits for its file", |s| s.state.as_deref() == Some("waiting")).await;
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
    status_when(&rig, "nothing plays", |s| s.playing.is_none()).await;
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
    status_when(&rig, "nothing plays", |s| s.playing.is_none()).await;
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
        db.busy_timeout(SIDE_WAITS).unwrap();
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
    status_when(&rig, "nothing plays", |s| s.playing.is_none()).await;
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

/// The voice status once `done` holds: the voice task leaves it as each step ends, after the
/// pushes and answers the step sends, so one read just after them may find the step before's.
async fn status_when(rig: &Rig, what: &str, done: impl Fn(&VoiceStatus) -> bool) -> VoiceStatus {
    let waited = tokio::time::timeout(PATIENCE, async {
        loop {
            let status = rig.voice.status();
            if done(&status) {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    waited.await.unwrap_or_else(|_| panic!("the status never said {what}: {:?}", rig.voice.status()))
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
    let kept = |s: &VoiceStatus| (s.recording.as_deref(), s.files) == (Some("stopped"), 1);
    let status = status_when(&rig, "it's stopped, its file kept", kept).await;
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
    let none = |s: &VoiceStatus| s.recording.is_none() && s.playing.is_none() && s.files == 0;
    status_when(&rig, "nothing records, plays or is kept", none).await;
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
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
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
    status_when(&rig, "it's stopped", |s| s.recording.as_deref() == Some("stopped")).await;
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
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
    assert!(rig.said().is_empty(), "WhatsApp isn't told you're recording: {:?}", rig.said());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_with_nothing_in_it_is_refused_and_leaves_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::recording(home.path(), Vec::new(), 1.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let e = rig.call("voice.send", json!({ "key": "jaw-voice-empty" })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "sending it stops it, with nothing in it: {e:?}");
    assert!(e.message.contains("nothing was recorded"), "{e:?}");
    let seen = recordings(&mut pushes, "failed", |p| p["state"] == "failed").await;
    assert!(seen.last().unwrap()["error"].as_str().unwrap().contains("nothing was recorded"), "{seen:?}");
    let kept: (i64, i64, i64) = rig
        .store
        .reads()
        .query(Priority::Interactive, |db| {
            let count = |table: &str| -> rusqlite::Result<i64> {
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            };
            Ok((count("messages")?, count("outbox")?, count("outgoing")?))
        })
        .await
        .unwrap();
    assert_eq!(kept, (0, 0, 0), "no message, outbox row or copy to make");
    assert!(rig.files("media/outgoing").is_empty() && rig.recordings().is_empty(), "and no file");
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
    assert!(rig.world.sends().is_empty());
    let e = rig.call("voice.send", json!({ "key": "jaw-voice-empty" })).await.unwrap_err();
    assert_eq!(e.code, code::NOT_FOUND, "its key names nothing, so there's nothing to send: {e:?}");

    // Stopped, it's refused the same.
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    let e = rig.call("voice.stop", json!({})).await.unwrap_err();
    assert_eq!(e.code, code::NOT_ALLOWED, "{e:?}");
    assert!(rig.recordings().is_empty(), "nothing left: {:?}", rig.recordings());
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
    let waited = tokio::time::timeout(PATIENCE, async {
        while rig.said().last() != Some(&(ANA.to_string(), PAUSED_SAID)) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    assert!(waited.await.is_ok(), "WhatsApp is told you've stopped: {:?}", rig.said());
}

#[tokio::test(flavor = "multi_thread")]
async fn digital_silence_is_pushed_silent_and_status_keeps_it() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), vec![0.0; 48_000 * 2], 1.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    recordings(&mut pushes, "silent", |p| p["state"] == "silent").await;
    status_when(&rig, "it heard nothing", |s| s.silent).await;
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
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_that_recorded_and_closes_its_socket_throws_its_recording_away() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    let socket = jaw_proto::local::socket(home.path());
    let listener = crate::sock::listen(&socket).unwrap();
    let live = Arc::new(std::sync::Mutex::new(crate::sync::starting(&rig.store, true)));
    let engine = Err(("demo", "the rig's sending has an engine of its own".to_string()));
    let state =
        crate::state::State::new(home.path(), engine, Vec::new(), live, Some(rig.store.clone()), rig.pushes.clone())
            .unwrap()
            .with_voice(rig.voice.clone());
    tokio::spawn(crate::sock::serve(Arc::new(state), listener));
    let mut pushes = rig.pushes.subscribe();
    let connection = tokio::task::spawn_blocking(move || {
        let (mut connection, _) = jaw_client::Connection::open(&socket, "test", "0").unwrap();
        connection.call("voice.record", json!({ "chat": ANA })).unwrap();
        connection
    })
    .await
    .unwrap();
    heard_all(&heard).await;
    let connection = tokio::task::spawn_blocking(move || {
        let mut connection = connection;
        connection.call("voice.stop", json!({})).unwrap();
        connection
    })
    .await
    .unwrap();
    assert_eq!(rig.recordings().len(), 1, "stopped, it waits to be sent");
    drop(connection);
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    assert!(rig.recordings().is_empty(), "{:?}", rig.recordings());
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;

    // One closed while it records stops the microphone there and then.
    let path = jaw_proto::local::socket(home.path());
    let connection = tokio::task::spawn_blocking(move || {
        let (mut connection, _) = jaw_client::Connection::open(&path, "test", "0").unwrap();
        connection.call("voice.record", json!({ "chat": ANA })).unwrap();
        connection
    })
    .await
    .unwrap();
    recordings(&mut pushes, "a level", |p| p["state"] == "recording").await;
    drop(connection);
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    assert!(rig.recordings().is_empty(), "nor its part: {:?}", rig.recordings());
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
    let waited = tokio::time::timeout(PATIENCE, async {
        while rig.said().last() != Some(&(ANA.to_string(), PAUSED_SAID)) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    assert!(waited.await.is_ok(), "WhatsApp is told you've stopped: {:?}", rig.said());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connection_ending_while_the_voice_tasks_requests_are_full_still_throws_its_recording_away() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.voice.call("voice.record", json!({ "chat": ANA }), 7).await.unwrap();
    heard_all(&heard).await;
    rig.voice.call("voice.stop", json!({}), 7).await.unwrap();
    // Held, the voice task takes one of the 72 requests that come at most, and its requests fill.
    rig.voice.hold(true);
    let waiting: Vec<_> = (0..ASKS + 8)
        .map(|_| {
            let voice = rig.voice.clone();
            tokio::spawn(async move { voice.call("voice.pause", json!({}), 8).await })
        })
        .collect();
    let full = tokio::time::timeout(PATIENCE, async {
        while rig.voice.inner.asks.capacity() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    assert!(full.await.is_ok(), "the voice task's requests never filled");
    rig.voice.ended(7);
    rig.voice.hold(false);
    for waited in waiting {
        waited.await.unwrap().unwrap();
    }
    recordings(&mut pushes, "cancelled", |p| p["state"] == "cancelled").await;
    assert!(rig.recordings().is_empty(), "{:?}", rig.recordings());
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn jawd_stopping_throws_a_stopped_recording_away_and_status_counts_it_meanwhile() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    let file = rig.recorded(&heard).await;
    let status = rig.voice.status();
    assert_eq!((status.files, status.bytes), (1, file.len() as u64), "status counts the recording");
    rig.voice.stop().await;
    assert!(rig.recordings().is_empty(), "jawd's stop throws it away: {:?}", rig.recordings());
    assert_eq!((rig.voice.status().files, rig.voice.status().bytes), (0, 0));
}

impl Rig {
    /// What the demo's WhatsApp was sent as message `id`, each time it was sent.
    fn sent(&self, id: &str) -> Vec<wa_proto::wa_web_protobufs_e2e::Message> {
        use prost::Message as _;
        self.world
            .sends()
            .into_iter()
            .filter(|s| s.id == id)
            .map(|s| wa_proto::wa_web_protobufs_e2e::Message::decode(s.raw.as_slice()).unwrap())
            .collect()
    }

    /// Records `seconds` of the fake microphone's tone in Ana's chat, and stops: the recording's
    /// file, as it was put in place.
    async fn recorded(&self, heard: &MicrophoneProbe) -> Vec<u8> {
        self.call("voice.record", json!({ "chat": ANA })).await.unwrap();
        heard_all(heard).await;
        self.call("voice.stop", json!({})).await.unwrap();
        let names = self.recordings();
        let [name] = names.as_slice() else { panic!("one recording: {names:?}") };
        std::fs::read(self.home.join(folder::RECORDINGS).join(name)).unwrap()
    }

    /// The files in one of jaw's folders, by name.
    fn files(&self, folder: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.home.join(folder))
            .map(|entries| entries.filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned())).collect())
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn sent(row: &Value) -> bool {
    matches!(row["status"].as_str(), Some("sent" | "delivered" | "read"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_goes_to_whatsapp_as_a_voice_note_with_its_waveform_length_and_file() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(2.0), 2.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.sending.call("draft.set", json!({ "chat": ANA, "text": "words typed meanwhile" })).await.unwrap();
    let file = rig.recorded(&heard).await;
    let stopped = recordings(&mut pushes, "stopped", |p| p["state"] == "stopped").await;
    let waveform = stopped.last().unwrap()["waveform"].clone();
    let answer = rig.call("voice.send", json!({ "key": "jaw-voice-1" })).await.unwrap();
    let id = answer["message"].as_str().unwrap().to_string();
    let row = &answer["row"];
    assert_eq!((&row["kind"], &row["status"], &row["mine"]), (&json!("voice"), &json!("pending"), &json!(true)));
    assert_eq!((&row["voice"]["seconds"], &row["voice"]["waveform"]), (&json!(2), &waveform), "{row}");
    assert!(rig.recordings().is_empty(), "the recording's own file goes once it's written: {:?}", rig.recordings());
    status_when(&rig, "nothing records", |s| s.recording.is_none()).await;

    let row = rig.until(ANA, &id, "sent", sent).await;
    assert_eq!(row["media"]["state"], "local", "its copy is in the cache: {row}");
    let [upload] = rig.world.uploads().try_into().unwrap();
    assert_eq!(upload.kind, pb::media_download::Kind::Audio as i32, "with WhatsApp's audio keys");
    let [message] = rig.sent(&id).try_into().unwrap();
    let audio = message.audio_message.as_deref().unwrap();
    assert_eq!(
        (audio.ptt, audio.mimetype.as_deref(), audio.seconds),
        (Some(true), Some("audio/ogg; codecs=opus"), Some(2))
    );
    let points: Vec<u8> = serde_json::from_value(waveform).unwrap();
    assert_eq!(audio.waveform.as_deref(), Some(points.as_slice()), "the recording's waveform");
    assert_eq!(audio.file_length, Some(file.len() as u64));
    assert!(audio.url.is_some() && audio.media_key.as_ref().is_some_and(|k| k.len() == 32));
    assert_eq!(*rig.world.file_at(audio.direct_path.as_deref().unwrap()).unwrap(), file, "it goes as it was recorded");
    assert!(audio.context_info.is_none(), "a chat without a timer gives it none");
    let draft: Option<String> = rig
        .store
        .reads()
        .query(Priority::Interactive, |db| {
            Ok(db.query_row("SELECT draft FROM chats WHERE id = ?1", [ANA], |r| r.get(0))?)
        })
        .await
        .unwrap();
    assert_eq!(draft.as_deref(), Some("words typed meanwhile"), "a voice message leaves the draft as it is");
    for _ in 0..200 {
        if rig.files("media/outgoing").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        rig.files("media/outgoing").is_empty(),
        "jaw's copy goes once its copy is made: {:?}",
        rig.files("media/outgoing")
    );

    // Asked again with its key, it's the same message, and nothing more goes.
    let again = rig.call("voice.send", json!({ "key": "jaw-voice-1" })).await.unwrap();
    assert_eq!(again["message"], id.as_str());
    assert_eq!(again["row"]["id"], id.as_str());
    assert_eq!(rig.sent(&id).len(), 1);
    let e = rig.call("voice.send", json!({})).await.unwrap_err();
    assert_eq!(e.code, code::NOT_FOUND, "without one, there's nothing to send: {e:?}");

    // The key given again with a new recording answers with the message it named, and the new
    // recording, which isn't sent, stays stopped.
    rig.recorded(&heard).await;
    let reused = rig.call("voice.send", json!({ "key": "jaw-voice-1" })).await.unwrap();
    assert_eq!(reused["message"], id.as_str());
    assert_eq!(rig.recordings().len(), 1, "the new recording is kept");
    status_when(&rig, "it's stopped", |s| s.recording.as_deref() == Some("stopped")).await;
    assert_eq!(rig.world.sends().len(), 1, "and nothing more goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn voice_send_answers_after_a_synced_commit() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    rig.media.hold_making(true);
    rig.recorded(&heard).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let synced = rig.store.storage().synced_commits;
    rig.call("voice.send", json!({ "key": "jaw-voice-synced" })).await.unwrap();
    let after = rig.store.storage().synced_commits;
    assert!(after > synced, "voice.send was answered before its message reached the disk: {synced} then {after}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_while_it_records_stops_it_first() {
    let home = tempfile::tempdir().unwrap();
    let (rig, _) = Rig::recording(home.path(), tone(30.0), 1.0, jaw_voice::RECORDING_LIMIT).await;
    let mut pushes = rig.pushes.subscribe();
    rig.call("voice.record", json!({ "chat": ANA })).await.unwrap();
    recordings(&mut pushes, "a second of it", |p| p["seconds"].as_f64().unwrap_or(0.0) >= 1.0).await;
    let answer = rig.call("voice.send", json!({})).await.unwrap();
    let id = answer["message"].as_str().unwrap().to_string();
    let stopped = recordings(&mut pushes, "stopped", |p| p["state"] == "stopped").await;
    assert_eq!(stopped.last().unwrap()["end"], "stopped");
    let row = rig.until(ANA, &id, "sent", sent).await;
    assert!(row["voice"]["seconds"].as_u64().is_some_and(|s| (1..30).contains(&s)), "{row}");
    assert!(rig.recordings().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_later_is_refused_until_jawd_schedules_and_the_recording_kept() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    rig.recorded(&heard).await;
    for (params, param) in [
        (json!({ "at": 1_893_456_000_000_i64 }), "at"),
        (json!({ "repeat": { "every": "day", "time": "09:00" } }), "repeat"),
    ] {
        let e = rig.call("voice.send", params).await.unwrap_err();
        assert_eq!((e.code, e.data.map(|d| d["param"].clone())), (code::INVALID_PARAMS, Some(json!(param))));
    }
    assert_eq!(rig.recordings().len(), 1, "kept, to send now or throw away");
    status_when(&rig, "it's stopped", |s| s.recording.as_deref() == Some("stopped")).await;
    assert!(rig.world.sends().is_empty());
    rig.call("voice.send", json!({})).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_outbox_refuses_it_and_keeps_the_recording() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    rig.recorded(&heard).await;
    {
        let mut db = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(SIDE_WAITS).unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
        for n in 0..1_000i64 {
            tx.execute(
                "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = ?3), ?1, ?2, ?2, 'text', 1, 'pending', 1)",
                rusqlite::params![format!("w{n}"), n, RUI],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), ?1, ?2, 'text', x'', 'failed', 0, 0)",
                rusqlite::params![RUI, format!("w{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let e = rig.call("voice.send", json!({})).await.unwrap_err();
    assert_eq!(e.code, code::LIMIT);
    assert_eq!(e.data.map(|d| d["limit"].clone()), Some(json!("outbox")));
    assert_eq!(rig.recordings().len(), 1, "the recording is kept as it was");
    status_when(&rig, "it's stopped", |s| s.recording.as_deref() == Some("stopped")).await;
    assert!(
        rig.files("media/outgoing").is_empty(),
        "and nothing of it kept to send: {:?}",
        rig.files("media/outgoing")
    );
}

/// What's left of message `id` and of anything to send: its rows, the outbox's, and jaw's copies.
async fn left_of(rig: &Rig, id: &str) -> (i64, i64, i64, Vec<String>) {
    let id = id.to_string();
    let (messages, outbox, outgoing) = rig
        .store
        .reads()
        .query(Priority::Interactive, move |db| {
            let messages: i64 = db.query_row("SELECT count(*) FROM messages WHERE id = ?1", [&id], |r| r.get(0))?;
            let outbox: i64 = db.query_row("SELECT count(*) FROM outbox", [], |r| r.get(0))?;
            let outgoing: i64 = db.query_row("SELECT count(*) FROM outgoing", [], |r| r.get(0))?;
            Ok((messages, outbox, outgoing))
        })
        .await
        .unwrap();
    (messages, outbox, outgoing, rig.files("media/outgoing"))
}

/// The ID the stopped recording's message would have: its file's name.
fn recording_id(rig: &Rig) -> String {
    let names = rig.recordings();
    let [name] = names.as_slice() else { panic!("one recording: {names:?}") };
    name.trim_end_matches(".ogg").to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn kept_bytes_past_their_bound_refuse_it_and_keep_the_recording() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    rig.recorded(&heard).await;
    {
        let db = rusqlite::Connection::open(home.path().join("jaw.db")).unwrap();
        db.busy_timeout(SIDE_WAITS).unwrap();
        db.execute_batch(&format!(
            "INSERT INTO messages (chat, id, at, t, kind, mine, status, v)
                VALUES ((SELECT n FROM chats WHERE id = '{RUI}'), 'big', 1, 1, 'audio', 1, 'failed', 1);
            INSERT INTO outbox (message, jid, id, kind, raw, state, next_at, created_at)
                VALUES (last_insert_rowid(), '{RUI}', 'big', 'audio', x'', 'failed', 0, 0);
            INSERT INTO outgoing (outbox, message, variant, kind, source, bytes, state, error)
                VALUES (last_insert_rowid(), (SELECT n FROM messages WHERE id = 'big'), 'sd', 'audio',
                    'media/outgoing/big.ogg', 8000000000, 'failed', 'the upload failed');"
        ))
        .unwrap();
    }
    let id = recording_id(&rig);
    let e = rig.call("voice.send", json!({})).await.unwrap_err();
    assert_eq!(e.code, code::LIMIT, "{e:?}");
    assert_eq!(e.data.as_ref().map(|d| d["limit"].clone()), Some(json!("media")), "{e:?}");
    assert_eq!(rig.recordings().len(), 1, "the recording is kept as it was");
    status_when(&rig, "it's stopped", |s| s.recording.as_deref() == Some("stopped")).await;
    let (messages, outbox, outgoing, copies) = left_of(&rig, &id).await;
    assert_eq!(
        (messages, outbox, outgoing, copies),
        (0, 1, 1, Vec::<String>::new()),
        "and nothing of it kept to send: the outbox holds the failed send alone"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disappearing_chats_timer_goes_with_it_and_it_goes_without_ffmpeg() {
    let home = tempfile::tempdir().unwrap();
    let (microphone, heard) = FakeMicrophone::new(48_000, tone(1.0), 4.0);
    let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
    let pipeline = jaw_media::Media::with_tools(
        jaw_media::Tools::none(),
        jaw_media::Config { previews: jaw_media::Durability::Derived, ..jaw_media::Config::default() },
    );
    let rig = Rig::piped(home.path(), speaker, microphone, super::Config::default(), pipeline).await;
    let timer = Chat { id: ANA.into(), ephemeral: Some(86_400), ..Chat::default() };
    rig.store.apply(Lane::Live, vec![Change::Chat(timer)]).await.unwrap();
    let file = rig.recorded(&heard).await;
    let id = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
    rig.until(ANA, &id, "sent", sent).await;
    let [message] = rig.sent(&id).try_into().unwrap();
    let audio = message.audio_message.as_deref().unwrap();
    assert_eq!(audio.context_info.as_ref().and_then(|c| c.expiration), Some(86_400), "the chat's timer");
    assert_eq!(*rig.world.file_at(audio.direct_path.as_deref().unwrap()).unwrap(), file, "as it was, with no ffmpeg");
}

/// How many chats are merging into others now.
async fn merging(rig: &Rig) -> i64 {
    rig.store
        .reads()
        .query(Priority::Interactive, |db| Ok(db.query_row("SELECT count(*) FROM chat_merges", [], |r| r.get(0))?))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recording_whose_chat_merges_goes_to_the_chat_kept_with_its_timer_before_the_fold_and_after() {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::recording(home.path(), tone(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
    // Nothing goes to WhatsApp until both are written, so nothing it answers moves the merge on.
    rig.media.hold_making(true);
    let lid = "100000000099@lid";
    let from_lid = crate::store::Message {
        chat: lid.into(),
        id: "3EB0LID1".into(),
        at: now_ms() - 60_000,
        sender: Some(lid.into()),
        kind: "text",
        text: Some("olá".into()),
        summary: "olá".into(),
        live: true,
        ..crate::store::Message::default()
    };
    let from_ana =
        crate::store::Message { chat: ANA.into(), id: "3EB0ANA1".into(), sender: Some(ANA.into()), ..from_lid.clone() };
    let timer = Chat { id: ANA.into(), ephemeral: Some(86_400), ..Chat::default() };
    let changes = vec![Change::Message(from_lid), Change::Message(from_ana), Change::Chat(timer)];
    rig.store.apply(Lane::Live, changes).await.unwrap();

    // Recorded in her chat under her LID, then her LID and her number are known to be hers: that
    // chat merges into the one under her number, which has a timer.
    rig.call("voice.record", json!({ "chat": lid })).await.unwrap();
    heard_all(&heard).await;
    rig.call("voice.stop", json!({})).await.unwrap();
    let hers =
        crate::store::Person { jids: vec![lid.into(), ANA.into()], name: Some("Ana".into()), ..Default::default() };
    rig.store.apply(Lane::Live, vec![Change::Person(hers)]).await.unwrap();
    assert_eq!(merging(&rig).await, 1, "the merge has begun");
    let during = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
    assert_eq!(rig.row(ANA, &during).await["kind"], "voice", "while it merges, it's in the chat kept");

    // Recorded there again, and sent once the merge has ended and the chat under her LID is gone.
    rig.call("voice.record", json!({ "chat": lid })).await.unwrap();
    heard_all(&heard).await;
    rig.call("voice.stop", json!({})).await.unwrap();
    let folded = tokio::time::timeout(PATIENCE, async {
        while merging(&rig).await > 0 {
            rig.store.apply(Lane::Background, vec![Change::MergeStep(None)]).await.unwrap();
        }
    });
    assert!(folded.await.is_ok(), "the merge never ended");
    let gone = rig.store.reads().query(Priority::Interactive, move |db| crate::store::read::chat_n(db, lid)).await;
    assert_eq!(gone.unwrap(), None, "the chat under her LID is gone");
    let after = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
    assert_eq!(rig.row(ANA, &after).await["kind"], "voice", "once it has ended, it's in the chat kept too");
    assert!(rig.recordings().is_empty(), "{:?}", rig.recordings());

    rig.media.hold_making(false);
    for id in [&during, &after] {
        rig.until(ANA, id, "sent", sent).await;
        let chats: Vec<String> = rig.world.sends().into_iter().filter(|s| s.id == *id).map(|s| s.chat).collect();
        assert_eq!(chats, [ANA], "{id} goes once, to her number");
        let [message] = rig.sent(id).try_into().unwrap();
        let audio = message.audio_message.as_deref().unwrap();
        let expiration = audio.context_info.as_ref().and_then(|c| c.expiration);
        assert_eq!((audio.ptt, expiration), (Some(true), Some(86_400)), "{id} carries the chat kept's timer");
    }
}

/// `seconds` of a tone at 48 kHz that swells as it goes, so a recording of it has a rising
/// waveform, whose bytes nothing else in jaw.db holds.
fn swell(seconds: f64) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    (0..n)
        .map(|i| {
            let loud = 0.05 + 0.9 * i as f32 / n as f32;
            (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * loud
        })
        .collect()
}

impl Rig {
    /// A rig recording a swelling tone, whose chat with Ana has a timer of `seconds`, beside thirty
    /// captioned photos and thirty messages with links in every search index.
    async fn timed(home: &Path, seconds: u32) -> (Rig, MicrophoneProbe) {
        let (rig, heard) = Rig::recording(home, swell(1.0), 4.0, jaw_voice::RECORDING_LIMIT).await;
        let timer = Chat { id: ANA.into(), ephemeral: Some(seconds), ephemeral_at: Some(1_000), ..Chat::default() };
        let mut changes = several_of_each();
        changes.push(Change::Chat(timer));
        rig.store.apply(Lane::Live, changes).await.unwrap();
        rig.store.indexed().await.unwrap();
        (rig, heard)
    }

    async fn query<T: Send + 'static>(
        &self,
        query: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T> + Send + 'static,
    ) -> T {
        self.store.reads().query(Priority::Interactive, query).await.unwrap()
    }

    /// Message `id`'s n, when it goes, and its detail, which says how long it is and its waveform.
    async fn timed_row(&self, id: &str) -> (i64, Option<i64>, String) {
        let id = id.to_string();
        let (n, expires_at, detail): (i64, Option<i64>, Option<String>) = self
            .query(move |db| {
                Ok(db.query_row("SELECT n, expires_at, detail FROM messages WHERE id = ?1", [&id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?)
            })
            .await;
        let detail = detail.expect("a voice message's row says how long it is");
        assert!(detail.contains("waveform"), "and gives its waveform: {detail}");
        (n, expires_at, detail)
    }

    /// The waveform message `id`'s outbox row sends.
    async fn waveform_going(&self, id: &str) -> Vec<u8> {
        let id = id.to_string();
        let raw: Vec<u8> =
            self.query(move |db| Ok(db.query_row("SELECT raw FROM outbox WHERE id = ?1", [&id], |r| r.get(0))?)).await;
        let goes: wa_proto::wa_web_protobufs_e2e::Message = prost::Message::decode(raw.as_slice()).unwrap();
        goes.audio_message.and_then(|audio| audio.waveform).expect("a voice message goes with its waveform")
    }

    /// Waits until a recording is on its way up, and held there.
    async fn uploading(&self) {
        let up = tokio::time::timeout(PATIENCE, async {
            while self.world.uploads().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(up.await.is_ok(), "its recording never went up");
    }

    /// Waits until the outbox holds nothing: every message has gone, or never will.
    async fn outbox_empty(&self, what: &str) {
        let left = || {
            self.query(|db| {
                Ok(db
                    .prepare("SELECT kind, state, error FROM outbox")?
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<rusqlite::Result<Vec<(String, String, Option<String>)>>>()?)
            })
        };
        let emptied = tokio::time::timeout(PATIENCE, async {
            while !left().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        assert!(emptied.await.is_ok(), "{what}: the outbox never emptied: {:?}", left().await);
    }

    /// The files in jaw's folder, wherever they are in it, that are `file`.
    fn copies_of(&self, file: &[u8]) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut folders = vec![self.home.clone()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                } else if std::fs::read(&path).is_ok_and(|bytes| bytes == file) {
                    found.push(path);
                }
            }
        }
        found
    }
}

/// A voice note of yours goes by the timer it went with, counted from when WhatsApp has it, as a
/// text does (round three's D1, the voice audit's O5): one sent while its chat had no timer, the
/// timer set to a day before WhatsApp answers, has no time to go, so the timer never erases it, as
/// WhatsApp and your phone keep it; one sent with a day's timer, the timer turned off before the
/// answer, goes a day after it, as WhatsApp erases it everywhere else, and then nothing of its row
/// is left. Among thirty captioned photos and thirty messages with links.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_of_yours_goes_by_the_timer_it_went_with_not_the_one_its_chat_has_once_the_server_has_it() {
    let day = 86_400_000;
    for (went_with, set_before_answer) in [(0u32, 86_400u32), (86_400, 0)] {
        let home = tempfile::tempdir().unwrap();
        let (rig, heard) = Rig::timed(home.path(), went_with).await;
        rig.world.gate(true);
        rig.recorded(&heard).await;
        let id = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
        rig.sending.call("chat.disappearing", json!({ "chat": ANA, "seconds": set_before_answer })).await.unwrap();
        let answered_from = now_ms();
        rig.world.gate(false);
        rig.until(ANA, &id, "sent", sent).await;
        let answered_by = now_ms();
        let [went] = rig.sent(&id).try_into().unwrap();
        let audio = went.audio_message.as_deref().unwrap();
        let carried = audio.context_info.as_ref().and_then(|c| c.expiration);
        assert_eq!(carried.unwrap_or(0), went_with, "it went with the timer its chat had when it was sent");
        let (n, expires_at, detail) = rig.timed_row(&id).await;
        if went_with == 0 {
            assert_eq!(expires_at, None, "it went without a timer, so it stays, whatever its chat's is now");
            let timer = Change::Expire { n, at: answered_by + day };
            let erased = rig.store.apply(Lane::Background, vec![timer]).await.unwrap();
            assert_eq!(erased.changed, 0, "a day after WhatsApp had it, the timer has nothing of it to erase");
            assert_eq!(rig.row(ANA, &id).await["kind"], "voice", "and it's still there");
            continue;
        }
        let at = expires_at.expect("it goes, though its chat's timer is off now");
        assert!(
            (answered_from + day..=answered_by + day).contains(&at),
            "it goes a day after WhatsApp had it: {at} not in {}..={}",
            answered_from + day,
            answered_by + day,
        );
        let waveform = audio.waveform.clone().unwrap();
        let before = free_holding(&rig.store, &rig.home, &waveform).await;
        let before_detail = free_holding(&rig.store, &rig.home, detail.as_bytes()).await;
        let erased = rig.store.apply(Lane::Background, vec![Change::Expire { n, at }]).await.unwrap();
        assert_eq!(erased.changed, 1, "at its time it's erased");
        settled(&rig.store).await;
        bytes_gone_but_for(&rig.home, &waveform, &before, "its waveform, once it's erased");
        bytes_gone_but_for(&rig.home, detail.as_bytes(), &before_detail, "its row's detail, once it's erased");
        still_found(&rig.store, "once it's erased").await;
    }
}

/// How a test takes a voice note of yours away while it's on its way.
#[derive(Clone, Copy)]
enum Away {
    /// Deleted for you from jaw, which, as on the phone, still sends it.
    ForYou,
    /// Deleted for everyone from jaw: shown deleted at once, its row changed once WhatsApp has the
    /// deletion, which goes after it.
    ForEveryone,
    /// Revoked on another of your devices, which changes its row at once, as round three's I7 test
    /// revokes a text.
    Revoked,
}

impl Rig {
    async fn take_away(&self, id: &str, away: Away) {
        let for_ = match away {
            Away::ForYou => "me",
            Away::ForEveryone => "everyone",
            Away::Revoked => {
                let revoke = Amend {
                    chat: ANA.into(),
                    target: id.into(),
                    sender: None,
                    mine: true,
                    at: now_ms(),
                    what: Amendment::Revoke,
                    raw: None,
                };
                let applied = self.store.apply(Lane::Live, vec![Change::Amend(revoke)]).await.unwrap();
                assert_eq!(applied.changed, 1, "the revoke changes its row");
                return;
            }
        };
        self.sending.call("delete", json!({ "chat": ANA, "message": id, "for": for_ })).await.unwrap();
    }

    /// Whether message `id`'s row is deleted for everyone, and whether it keeps its raw protobuf and
    /// its detail.
    async fn revoked_row(&self, id: &str) -> (bool, bool, bool) {
        let id = id.to_string();
        self.query(move |db| {
            Ok(db.query_row(
                "SELECT deleted, raw IS NOT NULL, detail IS NOT NULL FROM messages WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .await
    }
}

/// A voice note of yours still on its way, which goes with its chat's timer, is taken as an erasure
/// takes it, whatever takes it away while its recording uploads (round three's I7, with a voice
/// note): deleted for you, revoked on another of your devices, or deleted for everyone from jaw.
/// Once its row is changed, nothing of what it said is left in jaw.db but in the outbox row that
/// still sends it, and once WhatsApp has it, nothing anywhere, the WAL included, though the upload
/// on its way as it was taken away is done after; or, taken away while its copy was made and
/// refused by WhatsApp's media server, once nothing can send it. Among thirty captioned photos and
/// thirty messages with links.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_of_yours_on_its_way_with_a_timer_deleted_or_revoked_leaves_nothing_of_its_row() {
    let ways = [
        (Away::ForYou, "deleted for you"),
        (Away::Revoked, "revoked on another of your devices"),
        (Away::ForEveryone, "deleted for everyone"),
    ];
    for refused in [false, true] {
        for (away, way) in ways {
            let what =
                format!("{way} {}", if refused { "while its copy is made, then refused" } else { "as it uploads" });
            taken_away_leaves_nothing(away, refused, &what).await;
        }
    }
}

/// One case of the test above: a voice note taken away `away`, its copy refused if `refused`.
async fn taken_away_leaves_nothing(away: Away, refused: bool, what: &str) {
    let home = tempfile::tempdir().unwrap();
    let (rig, heard) = Rig::timed(home.path(), 86_400).await;
    if refused {
        rig.media.hold_making(true);
        rig.world.refuse_uploads(1);
    } else {
        rig.world.gate_uploads(true);
    }
    rig.recorded(&heard).await;
    let id = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
    if !refused {
        rig.uploading().await;
    }
    let (_, _, detail) = rig.timed_row(&id).await;
    let waveform = rig.waveform_going(&id).await;
    let before = free_holding(&rig.store, &rig.home, &waveform).await;
    let before_detail = free_holding(&rig.store, &rig.home, detail.as_bytes()).await;
    rig.take_away(&id, away).await;
    // Deleted for everyone from jaw, its row says what it did until WhatsApp has the deletion.
    if !matches!(away, Away::ForEveryone) {
        settled(&rig.store).await;
        let left: Vec<_> = pages_holding(&rig.home, &waveform)
            .into_iter()
            .filter(|(page, owner)| owner.as_deref() != Some("outbox") && !before.contains(page))
            .collect();
        assert!(left.is_empty(), "{what}: its waveform is left but in the outbox row that sends it: {left:?}");
        bytes_gone_but_for(&rig.home, detail.as_bytes(), &before_detail, &format!("{what}: its row's detail"));
    }
    rig.media.hold_making(false);
    rig.world.gate_uploads(false);
    rig.outbox_empty(what).await;
    assert_eq!(rig.sent(&id).len(), usize::from(!refused), "{what}: it goes once it's uploaded, all the same");
    settled(&rig.store).await;
    if !matches!(away, Away::ForYou) {
        assert_eq!(rig.revoked_row(&id).await, (true, false, false), "{what}: its row says nothing of it");
    }
    bytes_gone_but_for(&rig.home, &waveform, &before, &format!("{what}, once it's done with: its waveform"));
    let detail_left = format!("{what}, once it's done with: its row's detail");
    bytes_gone_but_for(&rig.home, detail.as_bytes(), &before_detail, &detail_left);
    still_found(&rig.store, "the others").await;
}

/// A voice note of yours erased while its recording is still on its way up, with its chat's timer
/// on, leaves nothing of the recording, as an erased photo's HD copy leaves none (round three's
/// L3): no file of it anywhere in jaw's folder, no copy's row and no failed send for send.retry to
/// find, whether it's deleted for you, revoked on another of your devices or deleted for everyone
/// from jaw, and whether its upload then goes or, taken away while its copy was made, WhatsApp's
/// media server refuses it.
#[tokio::test(flavor = "multi_thread")]
async fn an_erased_voice_note_of_yours_whose_recording_is_on_its_way_up_leaves_no_file_row_or_failed_send() {
    let ways = [
        (Away::ForYou, "deleted for you"),
        (Away::Revoked, "revoked on another of your devices"),
        (Away::ForEveryone, "deleted for everyone"),
    ];
    for refused in [false, true] {
        for (away, way) in ways {
            let what =
                format!("{way} {}", if refused { "while its copy is made, then refused" } else { "as it uploads" });
            let home = tempfile::tempdir().unwrap();
            let (rig, heard) = Rig::timed(home.path(), 86_400).await;
            if refused {
                rig.media.hold_making(true);
                rig.world.refuse_uploads(1);
            } else {
                rig.world.gate_uploads(true);
            }
            let file = rig.recorded(&heard).await;
            let id = rig.call("voice.send", json!({})).await.unwrap()["message"].as_str().unwrap().to_string();
            if !refused {
                rig.uploading().await;
            }
            rig.take_away(&id, away).await;
            rig.media.hold_making(false);
            rig.world.gate_uploads(false);
            rig.outbox_empty(&what).await;
            let cleaned = tokio::time::timeout(PATIENCE, async {
                while !rig.copies_of(&file).is_empty() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            });
            assert!(cleaned.await.is_ok(), "{what}: the recording is left in {:?}", rig.copies_of(&file));
            let left = rig
                .query(|db| {
                    let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0));
                    Ok((count("SELECT count(*) FROM outgoing")?, count("SELECT count(*) FROM outgoing_drop")?))
                })
                .await;
            assert_eq!(left, (0, 0), "{what}: no copy's row, and no file kept for one");
            let retried = rig.sending.call("send.retry", json!({ "message": id })).await;
            assert!(retried.is_err(), "{what}: send.retry finds nothing to send: {retried:?}");
            assert_eq!(rig.sent(&id).len(), usize::from(!refused), "{what}: it goes only once it's uploaded");
        }
    }
}
