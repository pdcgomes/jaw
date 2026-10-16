//! WhatsApp's waveform: 64 points from 0 to 100, each the mean loudness of a 64th of the message,
//! scaled so the loudest is 100, as the phone apps send in `AudioMessage.waveform`.

use std::path::Path;
use std::time::Duration;

use crate::Error;
use crate::oggopus::VoiceFile;

pub const POINTS: usize = 64;

/// A voice message's waveform and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waveform {
    pub points: [u8; POINTS],
    pub duration: Duration,
}

/// Reads a voice message's waveform and length from its file, for messages that come without them.
pub fn analyse(path: &Path) -> Result<Waveform, Error> {
    const RATE: u32 = 16_000;
    let file = VoiceFile::open(path)?;
    let mut decode = file.decode(RATE, Duration::ZERO)?;
    let mut envelope = Envelope::new(RATE);
    let mut pcm = Vec::with_capacity(960);
    while decode.next(&mut pcm) {
        envelope.push(&pcm);
        pcm.clear();
    }
    Ok(Waveform { points: envelope.points(), duration: file.duration() })
}

/// Mean loudness per 10 ms, enough to fold into 64 points whatever the length.
pub(crate) struct Envelope {
    chunk: usize,
    means: Vec<f32>,
    sum: f32,
    count: usize,
}

impl Envelope {
    pub(crate) fn new(rate: u32) -> Envelope {
        Envelope { chunk: (rate / 100) as usize, means: Vec::new(), sum: 0.0, count: 0 }
    }

    pub(crate) fn push(&mut self, samples: &[f32]) {
        for x in samples {
            self.sum += x.abs();
            self.count += 1;
            if self.count == self.chunk {
                self.means.push(self.sum / self.chunk as f32);
                self.sum = 0.0;
                self.count = 0;
            }
        }
    }

    pub(crate) fn points(&self) -> [u8; POINTS] {
        let mut means = self.means.clone();
        if self.count > 0 {
            means.push(self.sum / self.count as f32);
        }
        let mut points = [0; POINTS];
        let n = means.len();
        if n == 0 {
            return points;
        }
        let values: Vec<f32> = (0..POINTS)
            .map(|b| {
                let lo = (b * n / POINTS).min(n - 1);
                let hi = ((b + 1) * n / POINTS).max(lo + 1);
                means[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
            })
            .collect();
        let loudest = values.iter().copied().fold(0.0, f32::max);
        if loudest > 0.0 {
            for (p, v) in points.iter_mut().zip(&values) {
                *p = (100.0 * v / loudest).floor() as u8;
            }
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn the_waveform_follows_the_message_from_silence_to_sound_and_back() {
        // One second of silence, a second of tone, a second of silence.
        let w = analyse(&fixture("step-16k.ogg")).unwrap();
        assert_eq!(w.duration, Duration::from_secs(3));
        let p = w.points;
        assert!(p.iter().all(|&v| v <= 100));
        assert_eq!(*p.iter().max().unwrap(), 100);
        assert!(p[..20].iter().all(|&v| v < 5), "the first third is quiet: {p:?}");
        assert!(p[23..41].iter().all(|&v| v > 85), "the middle third is loud: {p:?}");
        assert!(p[44..].iter().all(|&v| v < 5), "the last third is quiet: {p:?}");
    }

    #[test]
    fn speech_gives_a_waveform_that_rises_and_falls() {
        let w = analyse(&fixture("speech-16k.ogg")).unwrap();
        let p = w.points;
        assert_eq!(*p.iter().max().unwrap(), 100);
        let quiet = p.iter().filter(|&&v| v < 30).count();
        let loud = p.iter().filter(|&&v| v > 60).count();
        assert!(quiet >= 4 && loud >= 8, "speech has pauses and syllables: {p:?}");
    }

    #[test]
    fn silence_and_a_message_too_short_to_fill_the_points_still_give_64() {
        let mut silent = Envelope::new(16_000);
        silent.push(&[0.0; 16_000]);
        assert_eq!(silent.points(), [0; POINTS]);

        let mut short = Envelope::new(16_000);
        short.push(&[0.0; 160]);
        short.push(&[0.5; 160]);
        let p = short.points();
        assert_eq!((p[0], p[63]), (0, 100));
    }
}
