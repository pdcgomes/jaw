//! The made-up account's voice, as jawd's player and recorder would answer: a playhead and levels
//! moved along ten times a second on a clock thread of its own, which runs only while something
//! plays or records, and only for a demo that keeps time; tests see each push as it's asked for.

use super::{Account, Demo, Later};
use crate::data::{Kind, Message, PlayState, Playback, RecordState, Recorded, Tick, Update, Voice};
use jaw_client::Update as Window;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// How often the playhead and the levels move.
const STEP: f64 = 0.1;

pub struct Player {
    /// The message, or none for the recording.
    message: Option<String>,
    at: f64,
    seconds: f64,
    speed: f64,
    paused: bool,
    /// When the playhead last moved.
    moved: Instant,
}

pub struct Recorder {
    chat: String,
    seconds: f64,
    levels: Vec<u8>,
    stopped: bool,
    moved: Instant,
}

/// A waveform that looks spoken: bursts of syllables with pauses between, from `seed`.
pub fn waveform(seed: u64) -> Vec<u8> {
    (0..64).map(|i| level(seed, i as f64 * 0.37)).collect()
}

fn level(seed: u64, t: f64) -> u8 {
    let s = seed as f64;
    let words = ((t * 1.3 + s).sin() * 0.5 + 0.5).powf(0.7);
    let syllables = ((t * 9.0 + s * 3.1).sin() * 0.5 + 0.5).powi(2);
    let mut n = seed.wrapping_add((t * 10.0) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    n ^= n >> 29;
    let grain = (n % 17) as f64;
    ((words * (20.0 + 70.0 * syllables)) + grain).min(100.0) as u8
}

/// A voice message's 64 points from a recording's levels.
fn squeeze(levels: &[u8]) -> Vec<u8> {
    if levels.is_empty() {
        return vec![0; 64];
    }
    (0..64)
        .map(|i| {
            let from = i * levels.len() / 64;
            let to = ((i + 1) * levels.len() / 64).clamp(from + 1, levels.len());
            levels[from.min(levels.len() - 1)..to].iter().copied().max().unwrap_or(0)
        })
        .collect()
}

impl Account {
    fn playback(&mut self, state: PlayState) {
        let Some(p) = &self.player else { return };
        let push =
            Playback { message: p.message.clone(), state, at: p.at, seconds: p.seconds, speed: p.speed, error: None };
        self.send(Update::Playback(push));
    }

    fn recorded(&mut self, state: RecordState, level: Option<u8>) {
        let Some(r) = &self.recorder else { return };
        let waveform = if state == RecordState::Stopped { squeeze(&r.levels) } else { vec![] };
        let end = (state == RecordState::Stopped).then(|| "stopped".to_string());
        let push = Recorded { state, seconds: r.seconds, level, waveform, end, reason: None, error: None };
        self.send(Update::Recorded(push));
    }

    /// Where a message is: its thread and place in it.
    fn find_message(&self, id: &str) -> Option<(usize, usize)> {
        self.threads.iter().enumerate().find_map(|(t, th)| th.messages.iter().position(|m| m.id == id).map(|i| (t, i)))
    }

    fn stop_playing(&mut self) {
        if self.player.is_some() {
            self.playback(PlayState::Stopped);
            self.player = None;
        }
    }

    pub(super) fn on_voice_play(&mut self, message: Option<String>, at: Option<f64>, speed: f64) {
        let seconds = match &message {
            Some(id) => {
                let Some((t, i)) = self.find_message(id) else { return };
                let m = &self.threads[t].messages[i];
                let seconds = m.voice.as_ref().map_or(0.0, |v| v.seconds);
                if !m.mine && m.voice.as_ref().is_some_and(|v| !v.played) {
                    self.played(t, i);
                }
                seconds
            }
            None => match &self.recorder {
                Some(r) if r.stopped => r.seconds,
                _ => return,
            },
        };
        let resumed = self.player.as_ref().filter(|p| p.message == message && p.paused).map(|p| p.at);
        if self.player.as_ref().is_some_and(|p| p.message != message) {
            self.stop_playing();
        }
        let at = at.or(resumed).unwrap_or(0.0).clamp(0.0, seconds);
        self.player = Some(Player { message, at, seconds, speed, paused: false, moved: Instant::now() });
        self.playback(PlayState::Playing);
    }

    /// Someone else's voice message, played for the first time: its played receipt goes, once.
    fn played(&mut self, t: usize, i: usize) {
        let m = &mut self.threads[t].messages[i];
        if let Some(v) = m.voice.as_mut() {
            v.played = true;
        }
        m.v += 1;
        let m = m.clone();
        let chat = self.threads[t].chat.id.clone();
        if self.open.as_ref().is_some_and(|o| o.chat == chat && (o.lo..o.hi).contains(&i)) {
            self.send(Update::Messages { chat, update: Window { upsert: vec![m], ..Window::default() } });
        }
    }

    pub(super) fn on_voice_pause(&mut self) {
        if let Some(p) = self.player.as_mut().filter(|p| !p.paused) {
            p.at = (p.at + p.moved.elapsed().as_secs_f64() * p.speed).min(p.seconds);
            p.paused = true;
            self.playback(PlayState::Paused);
        }
    }

    pub(super) fn on_voice_seek(&mut self, at: f64) {
        if let Some(p) = self.player.as_mut() {
            p.at = at.clamp(0.0, p.seconds);
            p.moved = Instant::now();
            let state = if p.paused { PlayState::Paused } else { PlayState::Playing };
            self.playback(state);
        }
    }

    pub(super) fn on_voice_speed(&mut self, speed: f64) {
        if let Some(p) = self.player.as_mut() {
            p.at += if p.paused { 0.0 } else { p.moved.elapsed().as_secs_f64() * p.speed };
            p.moved = Instant::now();
            p.speed = speed;
            let state = if p.paused { PlayState::Paused } else { PlayState::Playing };
            self.playback(state);
        }
    }

    pub(super) fn on_voice_record(&mut self, chat: &str) {
        if self.recorder.is_some() {
            return;
        }
        self.stop_playing();
        let moved = Instant::now();
        self.recorder = Some(Recorder { chat: chat.to_string(), seconds: 0.0, levels: vec![], stopped: false, moved });
        self.recorded(RecordState::Recording, Some(0));
    }

    pub(super) fn on_voice_stop(&mut self) {
        if let Some(r) = self.recorder.as_mut().filter(|r| !r.stopped) {
            r.stopped = true;
            self.recorded(RecordState::Stopped, None);
        }
    }

    pub(super) fn on_voice_send(&mut self) {
        self.on_voice_stop();
        if self.player.as_ref().is_some_and(|p| p.message.is_none()) {
            self.stop_playing();
        }
        let Some(r) = self.recorder.take() else { return };
        let Some(&t) = self.index.get(&r.chat) else { return };
        let seconds = r.seconds.max(1.0);
        let at = self.now();
        let mut m = self.message(at, None, vec![], Some(Tick::Pending));
        m.kind = Kind::Voice;
        m.summary = Some(format!("▶ voice message {}", crate::fmt::length(seconds)));
        m.voice = Some(Voice { seconds, waveform: squeeze(&r.levels), played: true });
        let id = m.id.clone();
        self.arrive(t, m);
        self.after(400, Later::Tick { chat: r.chat.clone(), id: id.clone(), tick: Tick::Sent });
        self.after(1_500, Later::Tick { chat: r.chat, id, tick: Tick::Delivered });
    }

    pub(super) fn on_voice_cancel(&mut self) {
        if self.player.as_ref().is_some_and(|p| p.message.is_none()) {
            self.stop_playing();
        }
        if self.recorder.is_some() {
            self.recorded(RecordState::Cancelled, None);
            self.recorder = None;
        }
    }

    /// Whether the clock has anything to move.
    fn voice_busy(&self) -> bool {
        self.player.as_ref().is_some_and(|p| !p.paused) || self.recorder.as_ref().is_some_and(|r| !r.stopped)
    }

    /// About a tenth of a second on: the playhead along, a level from the microphone.
    fn voice_step(&mut self) {
        if let Some(p) = self.player.as_mut().filter(|p| !p.paused) {
            p.at += p.moved.elapsed().as_secs_f64() * p.speed;
            p.moved = Instant::now();
            if p.at >= p.seconds {
                p.at = p.seconds;
                self.playback(PlayState::Ended);
                self.player = None;
            } else {
                self.playback(PlayState::Playing);
            }
        }
        if let Some(r) = self.recorder.as_mut().filter(|r| !r.stopped) {
            r.seconds += r.moved.elapsed().as_secs_f64();
            r.moved = Instant::now();
            let l = level(7, r.seconds * 3.0);
            r.levels.push(l);
            self.recorded(RecordState::Recording, Some(l));
        }
    }
}

impl Demo {
    /// Keeps time for voice: the playhead and the levels move ten times a second, as jawd's do.
    pub fn keep_voice_time(&self) {
        self.shared.clocked.store(true, Ordering::SeqCst);
    }

    /// Plays the newest voice message in `chat`, as if another interface had.
    pub fn play_newest_voice(&self, chat: &str) {
        let chat = chat.to_string();
        self.defer(move |a| {
            let Some(&t) = a.index.get(&chat) else { return };
            let newest = a.threads[t].messages.iter().rev().find(|m| m.voice.is_some()).map(|m| m.id.clone());
            if let Some(id) = newest {
                a.on_voice_play(Some(id), Some(0.0), 1.0);
            }
        });
        self.clock_voice();
    }

    /// Starts the clock thread if time is kept and it isn't running; it stops once nothing moves.
    pub(super) fn clock_voice(&self) {
        let shared = &self.shared;
        if !shared.clocked.load(Ordering::SeqCst) || shared.ticking.swap(true, Ordering::SeqCst) {
            return;
        }
        let demo = self.clone();
        std::thread::Builder::new()
            .name("jaw-demo-voice".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs_f64(STEP));
                    let mut a = demo.lock();
                    if a.gone {
                        return;
                    }
                    a.voice_step();
                    if a.voice_busy() {
                        continue;
                    }
                    drop(a);
                    demo.shared.ticking.store(false, Ordering::SeqCst);
                    // Something asked for between the last step and now would find the clock
                    // still running and not start another.
                    if !demo.lock().voice_busy() || demo.shared.ticking.swap(true, Ordering::SeqCst) {
                        return;
                    }
                }
            })
            .expect("the demo's voice clock");
    }
}

/// A voice message's row, as jawd's converter makes it.
pub fn voiced(m: &mut Message, seconds: f64, seed: u64, played: bool) {
    m.voice = Some(Voice { seconds, waveform: waveform(seed), played: played || m.mine });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_made_up_waveform_has_64_points_with_quiet_and_loud_in_it() {
        let w = waveform(3);
        assert_eq!(w.len(), 64);
        assert!(w.iter().all(|v| *v <= 100));
        assert!(w.iter().any(|v| *v < 25) && w.iter().any(|v| *v > 60), "{w:?}");
    }

    #[test]
    fn a_recordings_levels_squeeze_into_64_points() {
        let levels: Vec<u8> = (0..70).map(|i| (i % 100) as u8).collect();
        let w = squeeze(&levels);
        assert_eq!(w.len(), 64);
        assert_eq!(*w.last().unwrap(), 69);
        assert_eq!(squeeze(&[]), vec![0; 64]);
    }
}
