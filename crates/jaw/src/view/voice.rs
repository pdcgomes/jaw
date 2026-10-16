//! Voice messages in the view: what's playing as jawd says it, or as a key just asked so the line
//! answers at once; Enter plays and pauses, ←→ seek and x changes the speed; one ending plays the
//! next one you haven't heard, as the phone does. Ctrl-R records from the box, which becomes a
//! strip of levels: Enter sends it, Ctrl-R stops it to listen first, and Esc throws it away.

use super::View;
use crate::data::{Kind, Message, PlayState, Playback, RecordState, Recorded};
use crate::layout::Play;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::collections::VecDeque;

/// The speeds x steps through.
const SPEEDS: [f64; 3] = [1.0, 1.5, 2.0];
/// How far ← and → move the playhead.
const SEEK: f64 = 5.0;
/// How long a push about another message is taken as one sent before a key asked for this one.
const SETTLING: i64 = 1_000;
/// The most levels the strip keeps, more than any box is wide.
const LEVELS: usize = 400;

/// A voice message being recorded, or stopped to listen to before it's sent.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    pub stopped: bool,
    pub seconds: f64,
    /// The levels as they came, the newest last.
    pub levels: VecDeque<u8>,
    pub waveform: Vec<u8>,
    /// The microphone has given only silence.
    pub silent: bool,
}

impl View {
    /// The playhead on `id`'s line, while it's playing or paused.
    pub(super) fn play_of(&self, id: &str) -> Option<Play> {
        let p = self.playing.as_ref().filter(|p| p.message.as_deref() == Some(id))?;
        matches!(p.state, PlayState::Playing | PlayState::Paused).then(|| Play {
            at: (p.at.max(0.0) * 10.0) as u32,
            paused: p.state == PlayState::Paused,
            speed: (p.speed * 10.0).round() as u8,
        })
    }

    /// The recording's playback, while it's being listened to.
    pub(super) fn listening(&self) -> Option<&Playback> {
        self.playing.as_ref().filter(|p| p.message.is_none())
    }

    pub(super) fn on_playback(&mut self, p: Playback) {
        let mine = self.playing.as_ref().is_none_or(|c| c.message == p.message);
        let asked_another =
            self.voice_asked.as_ref().is_some_and(|(m, at)| *m != p.message && self.now() - at < SETTLING);
        match p.state {
            PlayState::Playing | PlayState::Paused if !asked_another => self.playing = Some(p),
            PlayState::Ended if mine => {
                self.playing = None;
                if let Some(id) = &p.message {
                    self.play_next(id);
                }
            }
            PlayState::Stopped if mine => self.playing = None,
            PlayState::Failed if mine => {
                self.playing = None;
                self.say(p.error.unwrap_or_else(|| "it couldn't be played".into()));
            }
            _ => {}
        }
    }

    pub(super) fn on_recorded(&mut self, r: Recorded) {
        let Some(rec) = self.recorder.as_mut() else { return };
        match r.state {
            RecordState::Recording => {
                rec.seconds = r.seconds;
                if let Some(level) = r.level {
                    rec.levels.push_back(level);
                    if rec.levels.len() > LEVELS {
                        rec.levels.pop_front();
                    }
                    rec.silent &= level == 0;
                }
            }
            RecordState::Silent => rec.silent = true,
            RecordState::Stopped => {
                rec.stopped = true;
                rec.seconds = r.seconds;
                rec.waveform = r.waveform;
                match r.end.as_deref() {
                    Some("limit") => self.say("recordings stop at 15 minutes"),
                    Some("interrupted") => {
                        self.say(r.reason.unwrap_or_else(|| "the microphone went away".into()));
                    }
                    _ => {}
                }
            }
            RecordState::Cancelled => self.recorder = None,
            RecordState::Failed => {
                self.recorder = None;
                self.say(r.error.unwrap_or_else(|| "it couldn't record".into()));
            }
        }
    }

    /// jawd turned voice down: nothing plays or records after all.
    pub(super) fn voice_refused(&mut self, why: String) {
        self.playing = None;
        self.recorder = None;
        self.voice_asked = None;
        self.say(why);
    }

    fn play(&mut self, m: &Message, at: Option<f64>) {
        let seconds = m.voice.as_ref().map_or(0.0, |v| v.seconds);
        let resumed = self.playing.as_ref().filter(|p| p.message.as_deref() == Some(&m.id)).map(|p| p.at);
        self.jawd.voice_play(Some(&m.id), at, self.speed);
        self.playing = Some(Playback {
            message: Some(m.id.clone()),
            state: PlayState::Playing,
            at: at.or(resumed).unwrap_or(0.0),
            seconds,
            speed: self.speed,
            error: None,
        });
        self.voice_asked = Some((Some(m.id.clone()), self.now()));
    }

    /// The next voice message after `id` in the chat that you haven't heard, if one came.
    fn play_next(&mut self, id: &str) {
        let Some(w) = self.messages.as_ref() else { return };
        let Some(mut at) = w.get(id).cloned() else { return };
        while let Some(next) = w.after(&at.sort).cloned() {
            if next.kind == Kind::Voice && !next.mine && next.voice.as_ref().is_some_and(|v| !v.played) {
                self.play(&next, Some(0.0));
                return;
            }
            at = next;
        }
    }

    /// Enter on a voice message: plays it, pauses it, or carries on where it paused.
    pub(super) fn toggle_voice(&mut self, m: &Message) {
        match self.play_of(&m.id) {
            Some(p) if !p.paused => {
                self.jawd.voice_pause();
                if let Some(p) = self.playing.as_mut() {
                    p.state = PlayState::Paused;
                }
            }
            Some(_) => self.play(m, None),
            None => self.play(m, Some(0.0)),
        }
    }

    /// ← or → on the voice message playing or paused; says whether it was that one.
    pub(super) fn seek_voice(&mut self, m: &Message, by: f64) -> bool {
        if self.play_of(&m.id).is_none() {
            return false;
        }
        let Some(p) = self.playing.as_mut() else { return false };
        p.at = (p.at + by * SEEK).clamp(0.0, p.seconds.max(0.0));
        let at = p.at;
        self.jawd.voice_seek(at);
        true
    }

    /// x: the next speed, for what's playing and what plays next.
    pub(super) fn next_speed(&mut self, m: &Message) {
        let i = SPEEDS.iter().position(|s| (*s - self.speed).abs() < 0.01).unwrap_or(0);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
        let speed = self.speed;
        if self.play_of(&m.id).is_some() || self.listening().is_some() {
            self.jawd.voice_speed(speed);
            if let Some(p) = self.playing.as_mut() {
                p.speed = speed;
            }
        } else {
            self.say(format!("voice messages play at {}×", crate::fmt::speed(speed)));
        }
    }

    /// Ctrl-R from the box: records for the open chat.
    pub(super) fn record(&mut self) {
        let Some(chat) = self.open.clone() else { return };
        if self.recorder.is_some() {
            return;
        }
        self.jawd.voice_record(&chat);
        self.playing = None;
        self.recorder = Some(Recorder::default());
    }

    /// Keys while the box is a recording: everything else waits until it's sent or thrown away.
    pub(super) fn on_recorder_key(&mut self, k: KeyEvent, ctrl: bool) {
        let Some(rec) = self.recorder.as_mut() else { return };
        match k.code {
            KeyCode::Enter => {
                self.jawd.voice_send();
                self.recorder = None;
                if self.listening().is_some() {
                    self.playing = None;
                }
            }
            KeyCode::Esc => {
                self.jawd.voice_cancel();
                self.recorder = None;
                if self.listening().is_some() {
                    self.playing = None;
                }
            }
            KeyCode::Char('r') if ctrl && !rec.stopped => {
                rec.stopped = true;
                let seconds = rec.seconds;
                self.jawd.voice_stop();
                self.listen(Some(0.0), seconds);
            }
            KeyCode::Char('r') if ctrl => {
                let seconds = rec.seconds;
                match self.listening().map(|p| p.state) {
                    Some(PlayState::Playing) => {
                        self.jawd.voice_pause();
                        if let Some(p) = self.playing.as_mut() {
                            p.state = PlayState::Paused;
                        }
                    }
                    Some(PlayState::Paused) => self.listen(None, seconds),
                    _ => self.listen(Some(0.0), seconds),
                }
            }
            _ => {}
        }
    }

    fn listen(&mut self, at: Option<f64>, seconds: f64) {
        let from = at.or(self.listening().map(|p| p.at)).unwrap_or(0.0);
        self.jawd.voice_play(None, at, self.speed);
        self.playing = Some(Playback {
            message: None,
            state: PlayState::Playing,
            at: from,
            seconds,
            speed: self.speed,
            error: None,
        });
        self.voice_asked = Some((None, self.now()));
    }

    /// The footer's keys while recording.
    pub(super) fn recorder_hints(&self) -> Vec<(&'static str, &'static str, bool)> {
        let stopped = self.recorder.as_ref().is_some_and(|r| r.stopped);
        let listen = match (stopped, self.listening().map(|p| p.state)) {
            (false, _) => "stops to listen",
            (true, Some(PlayState::Playing)) => "pauses",
            (true, _) => "listens",
        };
        vec![("⏎", "sends", true), ("ctrl-r", listen, true), ("esc", "throws it away", true)]
    }

    /// The footer's keys on a selected voice message.
    pub(super) fn voice_hints(&self, m: &Message) -> Vec<(&'static str, &'static str, bool)> {
        let play = self.play_of(&m.id);
        let mut hints = vec![("⏎", if play.is_some_and(|p| !p.paused) { "pause" } else { "play" }, true)];
        if play.is_some() {
            hints.push(("←→", "seek", true));
        }
        hints.push(("x", "speed", true));
        hints.push(("esc", "back to typing", true));
        hints
    }
}
