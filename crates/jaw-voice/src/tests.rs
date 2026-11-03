//! The audio thread end to end, through fake devices: nothing here plays a sound or opens a
//! microphone.

use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;

use crate::fake::{FakeMicrophone, FakeSpeaker};
use crate::test_support::{fixture, frequency, rms, sine};
use crate::{Event, PlaybackState, RecordingEnd, Speed, TICK, Voice, VoiceFile, analyse, part_path};

const PATIENCE: Duration = Duration::from_secs(20);

/// Every event until one matches, failing after a while.
fn until(events: &Receiver<Event>, mut wanted: impl FnMut(&Event) -> bool) -> Vec<Event> {
    let deadline = Instant::now() + PATIENCE;
    let mut seen = Vec::new();
    loop {
        let event = events.recv_deadline(deadline).unwrap_or_else(|_| panic!("waited in vain; saw {seen:#?}"));
        let done = wanted(&event);
        seen.push(event);
        if done {
            return seen;
        }
    }
}

fn playback(e: &Event) -> Option<(PlaybackState, f64, Speed)> {
    match e {
        Event::Playback { state, at, speed, .. } => Some((*state, at.as_secs_f64(), *speed)),
        _ => None,
    }
}

fn is(state: PlaybackState) -> impl FnMut(&Event) -> bool {
    move |e| playback(e).is_some_and(|(s, ..)| s == state)
}

fn no_microphone() -> FakeMicrophone {
    FakeMicrophone::unavailable()
}

#[test]
fn a_message_plays_to_its_end_reporting_the_playhead_ten_times_a_second() {
    // Other processes' load can only stretch a play and thin its reports, so each is judged at its
    // best of three plays, every play checked whole otherwise.
    let plays: Vec<(f64, usize)> = (0..3).map(|_| a_message_played_to_its_end()).collect();
    let played = plays.iter().map(|&(played, _)| played).min_by(f64::total_cmp).unwrap();
    let playing = plays.iter().map(|&(_, playing)| playing).max().unwrap();
    assert!(played < 2.6, "two seconds took {played:.2} s from the first sound at the quickest of {plays:.2?}");
    assert!(playing >= 16, "{playing} reports at the most of {plays:.2?}");
}

/// One play of the test above, checked whole but for how long it took from its first sound and how
/// many reports came, which it says.
fn a_message_played_to_its_end() -> (f64, usize) {
    let (speaker, heard) = FakeSpeaker::new(48_000, 1.0);
    let (voice, events) = Voice::start(speaker, no_microphone());
    let started = Instant::now();
    voice.play("m1", fixture("tone-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    let seen = until(&events, is(PlaybackState::Ended));
    let ended = Instant::now();
    let took = ended.duration_since(started).as_secs_f64();
    // Timed from the first sound, on the speaker's own thread, since load can hold up the start:
    // the command reaching the audio thread, opening the file and the speaker.
    let played = ended.duration_since(heard.first_sound().expect("it was heard")).as_secs_f64();

    let ats: Vec<f64> = seen.iter().filter_map(playback).map(|(_, at, _)| at).collect();
    assert!(ats.windows(2).all(|w| w[0] <= w[1]), "the playhead only moves forward: {ats:?}");
    assert_eq!(ats.last(), Some(&2.0));
    let playing = seen.iter().filter_map(playback).filter(|(s, ..)| *s == PlaybackState::Playing).count();
    // The first report, then one a tick at most, however late load makes the ticks.
    assert!(playing as f64 <= 1.0 + took / TICK.as_secs_f64(), "{playing} reports in {took:.2} s");
    assert!(took >= 1.95, "two seconds took {took:.2} s");
    let sound = heard.sound() as f64 / 48_000.0;
    assert!((sound - 2.0).abs() < 0.02, "{sound} s were heard");
    assert!(matches!(&seen[..], [Event::Playback { key, .. }, ..] if key == "m1"));
    (played, playing)
}

#[test]
fn faster_speeds_play_in_proportionally_less_time_at_the_same_pitch() {
    for (speed, seconds) in [(Speed::OneAndAHalf, 2.0 / 1.5), (Speed::Double, 1.0)] {
        let (speaker, heard) = FakeSpeaker::new(48_000, 10.0);
        let speaker = speaker.keeping();
        let (voice, events) = Voice::start(speaker, no_microphone());
        voice.play("m1", fixture("tone-16k.ogg"), Duration::ZERO, speed).unwrap();
        until(&events, is(PlaybackState::Ended));
        let sound = heard.sound() as f64 / 48_000.0;
        assert!((sound - seconds).abs() < 0.03, "{speed:?} played for {sound} s");
        let kept: Vec<f32> = heard.kept().into_iter().filter(|&x| x != 0.0).collect();
        let hz = frequency(&kept[4_800..kept.len() - 4_800], 48_000);
        assert!((hz - 440.0).abs() < 3.0, "{speed:?} sounds at {hz} Hz");
    }
}

#[test]
fn seeking_and_changing_speed_carry_on_from_the_position() {
    let (speaker, _) = FakeSpeaker::new(48_000, 4.0);
    let (voice, events) = Voice::start(speaker, no_microphone());
    voice.play("m1", fixture("speech-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    until(&events, |e| playback(e).is_some_and(|(_, at, _)| at >= 0.5));

    voice.seek(Duration::from_secs(3)).unwrap();
    let seen = until(&events, |e| playback(e).is_some_and(|(_, at, _)| (3.0..3.05).contains(&at)));
    assert!(seen.iter().filter_map(playback).all(|(s, ..)| s == PlaybackState::Playing));
    let after = until(&events, |e| playback(e).is_some());
    let at = playback(&after[after.len() - 1]).unwrap().1;
    assert!((3.0..3.6).contains(&at), "after the seek it's at {at}");

    voice.pause().unwrap();
    let paused = until(&events, is(PlaybackState::Paused));
    let p = playback(paused.last().unwrap()).unwrap().1;
    voice.set_speed(Speed::Double).unwrap();
    let changed = until(&events, is(PlaybackState::Paused));
    assert_eq!(playback(changed.last().unwrap()).unwrap(), (PlaybackState::Paused, p, Speed::Double));

    voice.resume().unwrap();
    let resumed = until(&events, is(PlaybackState::Playing));
    assert_eq!(playback(resumed.last().unwrap()).unwrap(), (PlaybackState::Playing, p, Speed::Double));
    let rest = until(&events, is(PlaybackState::Ended));
    let ats: Vec<f64> = rest.iter().filter_map(playback).map(|(_, at, _)| at).collect();
    assert!(ats.windows(2).all(|w| w[0] <= w[1]), "no jumps back after resuming at {p}: {ats:?}");

    voice.play("m1", fixture("speech-16k.ogg"), Duration::from_secs(1), Speed::Normal).unwrap();
    let before = until(&events, |e| playback(e).is_some_and(|(_, at, _)| at >= 1.3));
    let last = playback(before.last().unwrap()).unwrap().1;
    voice.set_speed(Speed::OneAndAHalf).unwrap();
    let changed = until(&events, |e| playback(e).is_some_and(|(_, _, s)| s == Speed::OneAndAHalf));
    let at = playback(changed.last().unwrap()).unwrap().1;
    // The fake speaker runs at four times real time, so a tick is 0.4 s of the message.
    assert!((last..last + 0.5).contains(&at), "the speed changed at {at}, after {last}");
}

#[test]
fn a_new_play_stops_the_last_so_one_message_plays_at_a_time() {
    let (speaker, heard) = FakeSpeaker::new(48_000, 1.0);
    let (voice, events) = Voice::start(speaker, no_microphone());
    voice.play("first", fixture("speech-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    until(&events, is(PlaybackState::Playing));
    voice.play("second", fixture("tone-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    let seen = until(&events, |e| matches!(e, Event::Playback { key, .. } if key == "second"));
    let stopped =
        seen.iter().find(|e| matches!(e, Event::Playback { key, state: PlaybackState::Stopped, .. } if key == "first"));
    assert!(stopped.is_some(), "the first is stopped: {seen:#?}");
    assert_eq!(heard.opens(), 2);
    voice.stop().unwrap();
    until(&events, is(PlaybackState::Stopped));
}

#[test]
fn an_output_that_cant_be_opened_or_a_file_that_isnt_voice_is_reported() {
    let (voice, events) = Voice::start(FakeSpeaker::unavailable(), no_microphone());
    voice.play("m1", fixture("tone-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    let seen = until(&events, |e| matches!(e, Event::PlaybackFailed { .. }));
    assert!(
        matches!(seen.last(), Some(Event::PlaybackFailed { key, error }) if key == "m1" && error.to_string().contains("no audio output"))
    );

    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (voice, events) = Voice::start(speaker, no_microphone());
    voice.play("m2", fixture("../../src/lib.rs"), Duration::ZERO, Speed::Normal).unwrap();
    let seen = until(&events, |e| matches!(e, Event::PlaybackFailed { .. }));
    assert!(
        matches!(seen.last(), Some(Event::PlaybackFailed { error, .. }) if error.to_string().contains("isn't a voice message"))
    );
}

#[test]
fn an_output_at_a_rate_libopus_cant_decode_to_gets_the_message_resampled() {
    let (speaker, heard) = FakeSpeaker::new(44_100, 10.0);
    let (voice, events) = Voice::start(speaker.keeping(), no_microphone());
    voice.play("m1", fixture("tone-48k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    until(&events, is(PlaybackState::Ended));
    let kept: Vec<f32> = heard.kept().into_iter().filter(|&x| x != 0.0).collect();
    assert!((kept.len() as f64 / 44_100.0 - 2.0).abs() < 0.01, "{} samples", kept.len());
    let body = &kept[4_410..kept.len() - 4_410];
    assert!((frequency(body, 44_100) - 440.0).abs() < 1.0);
    assert!((rms(body) - 0.354).abs() < 0.03);
}

fn recorder(samples: Vec<f32>, pace: f64) -> (Voice, Receiver<Event>, crate::fake::MicrophoneProbe) {
    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (mic, probe) = FakeMicrophone::new(48_000, samples, pace);
    let (voice, events) = Voice::start(speaker, mic);
    (voice, events, probe)
}

fn recorded(events: &Receiver<Event>) -> crate::Recorded {
    let end =
        |e: &Event| matches!(e, Event::Recorded(_) | Event::RecordingFailed { .. } | Event::RecordingRefused { .. });
    match until(events, end).pop() {
        Some(Event::Recorded(r)) => r,
        other => panic!("the recording failed: {other:?}"),
    }
}

fn wait(probe: &crate::fake::MicrophoneProbe) {
    let deadline = Instant::now() + PATIENCE;
    while !probe.finished() {
        assert!(Instant::now() < deadline, "the microphone never finished");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_recording_round_trips_through_encoding_and_decoding_as_a_whatsapp_voice_note() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.ogg");
    let (voice, events, probe) = recorder(sine(440.0, 0.5, 48_000, 2.0), 20.0);
    voice.record(&path, crate::RECORDING_LIMIT).unwrap();
    wait(&probe);
    // The thread takes what's in the ring at its next wake; stopping takes the rest.
    voice.stop_recording().unwrap();
    let r = recorded(&events);
    assert_eq!((r.path.as_path(), r.end.clone()), (path.as_path(), RecordingEnd::Stopped));
    assert_eq!(r.duration, Duration::from_secs(2));
    assert!(!part_path(&path).exists());

    // 16 kHz mono Opus in Ogg, as the phone apps send.
    let bytes = std::fs::read(&path).unwrap();
    let head = &bytes[28..47];
    assert_eq!(&head[..8], b"OpusHead");
    assert_eq!((head[9], u32::from_le_bytes(head[12..16].try_into().unwrap())), (1, 16_000));
    let kbits = bytes.len() as f64 * 8.0 / 2.0 / 1_000.0;
    assert!((14.0..34.0).contains(&kbits), "{kbits} kbit/s");

    let file = VoiceFile::open(&path).unwrap();
    assert_eq!(file.duration(), Duration::from_secs(2));
    let pcm = file.pcm(48_000).unwrap();
    assert_eq!(pcm.len(), 96_000);
    let body = &pcm[4_800..91_200];
    assert!((frequency(body, 48_000) - 440.0).abs() < 1.0, "{} Hz", frequency(body, 48_000));
    assert!((rms(body) - 0.354).abs() < 0.03, "RMS {}", rms(body));

    let w = analyse(&path).unwrap();
    assert_eq!(w.duration, r.duration);
    for (a, b) in w.points.iter().zip(&r.waveform) {
        assert!(a.abs_diff(*b) <= 5, "the recording's waveform {:?} and the file's {:?}", r.waveform, w.points);
    }
    assert!(r.waveform[2..62].iter().all(|&p| p > 85), "a steady tone is level: {:?}", r.waveform);
}

#[test]
fn levels_come_ten_times_a_second_while_recording() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 2.0), 1.0);
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    let seen =
        until(&events, |e| matches!(e, Event::Recording { elapsed, .. } if *elapsed >= Duration::from_millis(1_000)));
    let levels: Vec<u8> =
        seen.iter().filter_map(|e| if let Event::Recording { level, .. } = e { Some(*level) } else { None }).collect();
    assert!((8..=13).contains(&levels.len()), "{} levels in a second", levels.len());
    // A tone at half of full scale is about -9 dBFS.
    assert!(levels[2..].iter().all(|&l| (75..=90).contains(&l)), "{levels:?}");
    voice.cancel_recording().unwrap();
    until(&events, |e| matches!(e, Event::RecordingCancelled));
}

#[test]
fn a_cancelled_recording_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, probe) = recorder(sine(440.0, 0.5, 48_000, 2.0), 4.0);
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    until(&events, |e| matches!(e, Event::Recording { .. }));
    assert!(part_path(&dir.path().join("note.ogg")).exists());
    voice.cancel_recording().unwrap();
    until(&events, |e| matches!(e, Event::RecordingCancelled));
    assert!(probe.delivered() > 0);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_recording_stops_at_its_limit_with_exactly_that_much() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 3.0), 20.0);
    voice.record(dir.path().join("note.ogg"), Duration::from_secs(1)).unwrap();
    let r = recorded(&events);
    assert_eq!((r.end, r.duration), (RecordingEnd::Capped, Duration::from_secs(1)));
    assert_eq!(VoiceFile::open(&r.path).unwrap().duration(), Duration::from_secs(1));
}

#[test]
fn a_microphone_giving_only_silence_is_flagged_and_still_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, probe) = recorder(vec![0.0; 96_000], 10.0);
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    until(&events, |e| matches!(e, Event::RecordingSilent));
    wait(&probe);
    voice.stop_recording().unwrap();
    let r = recorded(&events);
    assert_eq!(r.waveform, [0; 64]);
}

#[test]
fn a_recording_with_nothing_in_it_fails_and_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, _) = recorder(Vec::new(), 1.0);
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    voice.stop_recording().unwrap();
    let seen = until(&events, |e| matches!(e, Event::RecordingFailed { .. } | Event::Recorded(_)));
    assert!(
        matches!(seen.last(), Some(Event::RecordingFailed { error }) if error.to_string() == "nothing was recorded")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn a_microphone_that_cant_be_opened_refuses_the_recording_and_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (voice, events) = Voice::start(speaker, FakeMicrophone::unavailable());
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    let seen = until(&events, |e| matches!(e, Event::RecordingRefused { .. }));
    assert!(
        matches!(seen.last(), Some(Event::RecordingRefused { error }) if error.to_string().contains("no microphone"))
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn recording_stops_playback_first() {
    let dir = tempfile::tempdir().unwrap();
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 1.0), 1.0);
    voice.play("m1", fixture("speech-16k.ogg"), Duration::ZERO, Speed::Normal).unwrap();
    until(&events, is(PlaybackState::Playing));
    voice.record(dir.path().join("note.ogg"), crate::RECORDING_LIMIT).unwrap();
    until(&events, is(PlaybackState::Stopped));
    until(&events, |e| matches!(e, Event::Recording { .. }));
}

/// Retries a command while the audio thread's queue is full.
fn insist(command: impl Fn() -> Result<(), crate::Error>) {
    let deadline = Instant::now() + PATIENCE;
    while let Err(e) = command() {
        assert!(Instant::now() < deadline, "{e}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Waits until the audio thread has taken every command, so all but the last are handled.
fn taken(voice: &Voice) {
    let commands = voice.commands.as_ref().expect("the audio thread is running");
    let deadline = Instant::now() + PATIENCE;
    while !commands.is_empty() {
        assert!(Instant::now() < deadline, "the audio thread never took its commands");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_recordings_end_is_held_for_a_caller_that_falls_behind_and_never_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (path, second) = (dir.path().join("note.ogg"), dir.path().join("second.ogg"));
    let (voice, events, probe) = recorder(sine(440.0, 0.5, 48_000, 2.0), 20.0);
    voice.record(&path, crate::RECORDING_LIMIT).unwrap();
    wait(&probe);
    // Nothing drains the events while playback fills the channel past full.
    for _ in 0..crate::EVENTS + 50 {
        insist(|| voice.play("m", fixture("missing.ogg"), Duration::ZERO, Speed::Normal));
    }
    insist(|| voice.stop_recording());
    insist(|| voice.record(&second, crate::RECORDING_LIMIT));
    insist(|| voice.play("last", fixture("missing.ogg"), Duration::ZERO, Speed::Normal));
    // Nothing is read until the thread has met them all, however long load makes the end's syncs:
    // an end that finds room isn't held.
    taken(&voice);
    assert!(voice.stats().dropped_events > 0);

    let refused = |e: &Event| matches!(e, Event::RecordingRefused { error } if error.to_string().contains("hasn't been read yet"));
    let last = |e: &Event| matches!(e, Event::PlaybackFailed { key, .. } if key == "last");
    let (mut ended, mut refusals, mut lasts) = (0, 0, 0);
    let seen = until(&events, |e| {
        ended += usize::from(matches!(e, Event::Recorded(_)));
        refusals += usize::from(refused(e));
        lasts += usize::from(last(e));
        ended > 0 && refusals > 0 && lasts > 0
    });
    let recorded_at = seen.iter().position(|e| matches!(e, Event::Recorded(_))).unwrap();
    assert!(recorded_at < seen.iter().position(refused).unwrap(), "the refusal comes after the end it waited for");
    assert_eq!((ended, refusals, lasts), (1, 1, 1));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "the second recording didn't start");
    assert!(path.exists());

    // Once the end is read, recording works again.
    voice.record(&second, crate::RECORDING_LIMIT).unwrap();
    until(&events, |e| matches!(e, Event::Recording { .. }));
    voice.cancel_recording().unwrap();
    until(&events, |e| matches!(e, Event::RecordingCancelled));
}

#[test]
fn refusals_held_while_the_caller_is_behind_never_push_out_a_recordings_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.ogg");
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 60.0), 1.0);
    voice.record(&path, crate::RECORDING_LIMIT).unwrap();
    until(&events, |e| matches!(e, Event::Recording { .. }));
    for _ in 0..crate::EVENTS + 10 {
        insist(|| voice.play("m", fixture("missing.ogg"), Duration::ZERO, Speed::Normal));
    }
    // Refused while recording, then the recording ends, then refused while its end waits.
    insist(|| voice.record(dir.path().join("b.ogg"), crate::RECORDING_LIMIT));
    insist(|| voice.stop_recording());
    insist(|| voice.record(dir.path().join("c.ogg"), crate::RECORDING_LIMIT));
    std::thread::sleep(Duration::from_millis(300));

    let (mut ended, mut refusals) = (0, 0);
    let seen = until(&events, |e| {
        ended += usize::from(matches!(e, Event::Recorded(_)));
        refusals += usize::from(matches!(e, Event::RecordingRefused { .. }));
        ended > 0 && refusals > 0
    });
    assert_eq!(ended, 1);
    assert!(!seen.iter().any(|e| matches!(e, Event::RecordingFailed { .. })), "a refusal isn't a failure");
    assert!(path.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "neither refused recording started");
}

#[test]
fn a_microphone_that_fails_midway_keeps_what_came_before() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.ogg");
    let (speaker, _) = FakeSpeaker::new(48_000, 1.0);
    let (mic, _) = FakeMicrophone::new(48_000, sine(440.0, 0.5, 48_000, 2.0), 10.0);
    let (voice, events) = Voice::start(speaker, mic.failing_after(48_000));
    voice.record(&path, crate::RECORDING_LIMIT).unwrap();
    let r = recorded(&events);
    assert_eq!(r.end, RecordingEnd::DeviceFailed("the microphone was unplugged".into()));
    assert_eq!(r.duration, Duration::from_secs(1));
    assert_eq!(VoiceFile::open(&path).unwrap().duration(), Duration::from_secs(1));
    assert!(!part_path(&path).exists());
}

/// Set for the child process of the test below, naming the file it records to.
const DISK_FULL: &str = "JAW_VOICE_RECORD_ONTO_A_FULL_DISK";

#[test]
fn a_full_disk_fails_the_recording_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let status = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::records_onto_a_full_disk", "--ignored"])
        .env(DISK_FULL, dir.path().join("note.ogg"))
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "the child's recording didn't fail cleanly");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
#[ignore = "the child process of a_full_disk_fails_the_recording_and_leaves_nothing"]
fn records_onto_a_full_disk() {
    let Some(path) = std::env::var_os(DISK_FULL) else {
        return;
    };
    // Files this process writes stop at 4 KiB, as a full disk stops them; the signal that limit
    // raises is ignored, so the write fails instead.
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
        let limit = libc::rlimit { rlim_cur: 4_096, rlim_max: libc::RLIM_INFINITY };
        assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
    }
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 10.0), 20.0);
    voice.record(Path::new(&path), crate::RECORDING_LIMIT).unwrap();
    let seen = until(&events, |e| matches!(e, Event::RecordingFailed { .. } | Event::Recorded(_)));
    let error = match seen.last() {
        Some(Event::RecordingFailed { error }) => error.to_string(),
        other => panic!("the recording should have failed: {other:?}"),
    };
    assert!(error.contains("couldn't write the recording"), "{error}");
    assert!(!Path::new(&path).exists() && !part_path(Path::new(&path)).exists());
}

/// Set for the child process of the test below, naming the file it records to.
const CHILD: &str = "JAW_VOICE_RECORD_UNTIL_KILLED";

#[test]
fn a_killed_recording_leaves_no_half_file_under_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.ogg");
    let mut child = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::records_until_killed", "--ignored", "--nocapture"])
        .env(CHILD, &path)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let part = part_path(&path);
    let deadline = Instant::now() + PATIENCE;
    // Killed once it has written audio pages, past the headers.
    while std::fs::metadata(&part).map_or(0, |m| m.len()) < 8_192 {
        assert!(Instant::now() < deadline, "the child never wrote its recording");
        std::thread::sleep(Duration::from_millis(20));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!path.exists(), "a killed recording mustn't leave anything under its name");
    assert!(part.exists());
}

#[test]
#[ignore = "the child process of a_killed_recording_leaves_no_half_file_under_its_name"]
fn records_until_killed() {
    let Some(path) = std::env::var_os(CHILD) else {
        return;
    };
    let (voice, events, _) = recorder(sine(440.0, 0.5, 48_000, 120.0), 8.0);
    voice.record(Path::new(&path), crate::RECORDING_LIMIT).unwrap();
    for e in events {
        assert!(!matches!(e, Event::RecordingFailed { .. } | Event::RecordingRefused { .. }), "{e:?}");
    }
}
