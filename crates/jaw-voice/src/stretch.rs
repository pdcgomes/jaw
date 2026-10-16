//! Faster playback with the pitch kept: WSOLA (waveform similarity overlap-add). Frames of 25 ms
//! are taken from the input `speed` times further apart than they're laid down in the output, each
//! nudged by up to 10 ms to where it best continues the frame before, and cross-faded with a Hann
//! window. Speech's pitch periods stay whole, so voices sound as they did, only quicker.

pub(crate) struct Stretch {
    speed: f64,
    frame: usize,
    hop: usize,
    tolerance: u64,
    /// The similarity search looks at every `step`th sample, then refines around the best.
    step: usize,
    window: Vec<f32>,
    input: Vec<f32>,
    start: u64,
    /// Where the next frame would be taken from with no nudge, in input samples.
    next: f64,
    prev: Option<u64>,
    overlap: Vec<f32>,
    fed: u64,
    emitted: u64,
}

impl Stretch {
    pub(crate) fn new(rate: u32, speed: f64) -> Stretch {
        let frame = (rate as usize / 40) & !1;
        let hop = frame / 2;
        let window = (0..frame)
            .map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / frame as f64).cos()) as f32)
            .collect();
        Stretch {
            speed,
            frame,
            hop,
            tolerance: u64::from(rate / 100),
            step: (rate as usize / 12_000).max(1),
            window,
            input: Vec::new(),
            start: 0,
            next: 0.0,
            prev: None,
            overlap: vec![0.0; hop],
            fed: 0,
            emitted: 0,
        }
    }

    pub(crate) fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.input.extend_from_slice(input);
        self.fed += input.len() as u64;
        self.run(out);
    }

    /// Lets out the rest, so the output is the input's length divided by the speed.
    pub(crate) fn finish(&mut self, out: &mut Vec<f32>) {
        let total = (self.fed as f64 / self.speed).round() as u64;
        let pad = self.frame + 2 * self.tolerance as usize + (self.hop as f64 * self.speed) as usize;
        self.input.extend(std::iter::repeat_n(0.0, pad));
        let (before, emitted) = (out.len(), self.emitted);
        self.run(out);
        out.extend_from_slice(&self.overlap);
        let keep = total.saturating_sub(emitted).min((out.len() - before) as u64);
        out.truncate(before + keep as usize);
        self.emitted = emitted + keep;
    }

    fn run(&mut self, out: &mut Vec<f32>) {
        let end = self.start + self.input.len() as u64;
        loop {
            let nominal = self.next.round() as u64;
            let lo = nominal.saturating_sub(self.tolerance).max(self.start);
            let hi = nominal + self.tolerance;
            let target = self.prev.map(|p| p + self.hop as u64);
            let needed = (hi + self.frame as u64).max(target.map_or(0, |t| t + self.frame as u64));
            if needed > end {
                break;
            }
            let at = match target {
                Some(t) => self.best(lo, hi, t),
                None => nominal,
            };
            let x = &self.input[(at - self.start) as usize..][..self.frame];
            let (w, hop) = (&self.window, self.hop);
            let before = out.len();
            out.extend((0..hop).map(|n| self.overlap[n] + w[n] * x[n]));
            for n in 0..hop {
                self.overlap[n] = w[hop + n] * x[hop + n];
            }
            self.emitted += (out.len() - before) as u64;
            self.prev = Some(at);
            self.next += hop as f64 * self.speed;

            let keep = (self.next.round() as u64).saturating_sub(self.tolerance).min(at + hop as u64);
            let drop = keep.saturating_sub(self.start) as usize;
            if drop > 4 * self.frame {
                self.input.drain(..drop);
                self.start += drop as u64;
            }
        }
    }

    /// The frame start in `lo..=hi` most like the natural continuation at `target`.
    fn best(&self, lo: u64, hi: u64, target: u64) -> u64 {
        let score = |at: u64, stride: usize| {
            let a = &self.input[(at - self.start) as usize..][..self.frame];
            let b = &self.input[(target - self.start) as usize..][..self.frame];
            let (mut dot, mut energy) = (0.0f32, 1e-9f32);
            for i in (0..self.frame).step_by(stride) {
                dot += a[i] * b[i];
                energy += a[i] * a[i];
            }
            dot / energy.sqrt()
        };
        let pick = |range: &mut dyn Iterator<Item = u64>, stride: usize| {
            range.map(|at| (score(at, stride), at)).max_by(|a, b| a.0.total_cmp(&b.0)).map_or(lo, |(_, at)| at)
        };
        let coarse = pick(&mut (lo..=hi).step_by(self.step), self.step);
        let step = self.step as u64;
        pick(&mut (coarse.saturating_sub(step - 1).max(lo)..=(coarse + step - 1).min(hi)), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{frequency, rms, sine};

    fn stretch(rate: u32, speed: f64, input: &[f32]) -> Vec<f32> {
        let mut s = Stretch::new(rate, speed);
        let mut out = Vec::new();
        for c in input.chunks(997) {
            s.push(c, &mut out);
        }
        s.finish(&mut out);
        out
    }

    #[test]
    fn faster_playback_shortens_the_audio_and_keeps_its_pitch() {
        for rate in [16_000, 44_100, 48_000] {
            for speed in [1.5, 2.0] {
                let input = sine(220.0, 0.5, rate, 2.0);
                let out = stretch(rate, speed, &input);
                let expected = (input.len() as f64 / speed).round() as usize;
                assert_eq!(out.len(), expected, "{rate} at {speed}×");
                let body = &out[rate as usize / 10..out.len() - rate as usize / 10];
                let hz = frequency(body, rate);
                assert!((hz - 220.0).abs() < 2.0, "{rate} at {speed}× sounds at {hz} Hz");
                assert!((rms(body) - 0.354).abs() < 0.02, "{rate} at {speed}× has an RMS of {}", rms(body));
            }
        }
    }

    #[test]
    fn stretching_keeps_where_things_happen_in_proportion() {
        // A burst at 1.2 s comes out at 0.6 s at 2×.
        let rate = 16_000;
        let mut input = vec![0.0; 2 * rate as usize];
        let burst = sine(300.0, 0.5, rate, 0.1);
        input[19_200..19_200 + burst.len()].copy_from_slice(&burst);
        let out = stretch(rate, 2.0, &input);
        let first = out.iter().position(|x| x.abs() > 0.05).unwrap();
        assert!(first.abs_diff(9_600) < 400, "the burst starts at {first}");
    }
}
