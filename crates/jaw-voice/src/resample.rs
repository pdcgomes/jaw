//! A polyphase resampler for a fixed ratio: up by L, a Kaiser-windowed sinc low-pass, down by M,
//! computing only the samples kept. It brings decoded audio to an output device's rate when that
//! isn't one libopus decodes to, and a microphone's rate to the 16 kHz voice messages are made at.

pub(crate) struct Resampler {
    up: u64,
    down: u64,
    taps: usize,
    /// The filter split into `up` phases of `taps` coefficients each.
    bank: Vec<f32>,
    /// Where the filter's centre falls, so output and input line up with no delay.
    centre: u64,
    /// Input from absolute index `start`, which begins `taps` zeros before the first sample.
    buf: Vec<f32>,
    start: i64,
    /// The next output sample, and how many input samples have come.
    n: u64,
    fed: u64,
}

impl Resampler {
    pub(crate) fn new(from: u32, to: u32) -> Resampler {
        let g = gcd(from, to);
        let (up, down) = (u64::from(to / g), u64::from(from / g));
        // Going down needs a longer filter for the same transition at the lower rate.
        let taps = (24.0 * (down as f64 / up as f64).max(1.0)).ceil() as usize;
        let len = up as usize * taps;
        let cutoff = 0.46 / up.max(down) as f64;
        let mid = (len - 1) as f64 / 2.0;
        let beta = 7.0;
        let norm = bessel_i0(beta);
        let h: Vec<f64> = (0..len)
            .map(|j| {
                let x = j as f64 - mid;
                let sinc = if x == 0.0 {
                    2.0 * cutoff
                } else {
                    (2.0 * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
                };
                let r = x / (mid + 1.0);
                sinc * bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / norm
            })
            .collect();
        let mut bank = vec![0.0; len];
        for phase in 0..up as usize {
            let sum: f64 = (0..taps).map(|k| h[phase + k * up as usize]).sum();
            for k in 0..taps {
                bank[phase * taps + k] = (h[phase + k * up as usize] / sum) as f32;
            }
        }
        Resampler {
            up,
            down,
            taps,
            bank,
            centre: mid.round() as u64,
            buf: vec![0.0; taps],
            start: -(taps as i64),
            n: 0,
            fed: 0,
        }
    }

    pub(crate) fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(input);
        self.fed += input.len() as u64;
        self.run(out, u64::MAX);
    }

    /// Lets out what the filter still holds, so the output is as long as the input in time.
    pub(crate) fn finish(&mut self, out: &mut Vec<f32>) {
        let total = (self.fed * self.up).div_ceil(self.down);
        self.buf.extend(std::iter::repeat_n(0.0, self.taps));
        self.run(out, total);
    }

    fn run(&mut self, out: &mut Vec<f32>, limit: u64) {
        let end = self.start + self.buf.len() as i64;
        while self.n < limit {
            let t = self.n * self.down + self.centre;
            let base = (t / self.up) as i64;
            if base >= end {
                break;
            }
            let phase = (t % self.up) as usize;
            let i = (base - self.start) as usize;
            let coeffs = &self.bank[phase * self.taps..][..self.taps];
            let window = &self.buf[i + 1 - self.taps..=i];
            out.push(window.iter().rev().zip(coeffs).map(|(x, c)| x * c).sum());
            self.n += 1;
        }
        let next = ((self.n * self.down + self.centre) / self.up) as i64;
        let drop = (next + 1 - self.taps as i64 - self.start).clamp(0, self.buf.len() as i64) as usize;
        self.buf.drain(..drop);
        self.start += drop as i64;
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > sum * 1e-12 {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        k += 1.0;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{frequency, rms, sine};

    fn resample(from: u32, to: u32, input: &[f32], chunk: usize) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for c in input.chunks(chunk) {
            r.process(c, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn resampling_keeps_length_pitch_and_loudness_between_common_rates() {
        for (from, to) in
            [(48_000, 44_100), (44_100, 48_000), (48_000, 16_000), (44_100, 16_000), (16_000, 48_000), (24_000, 22_050)]
        {
            let input = sine(440.0, 0.5, from, 1.0);
            let out = resample(from, to, &input, 479);
            assert_eq!(out.len(), to as usize, "{from} to {to}");
            let body = &out[to as usize / 10..to as usize * 9 / 10];
            let hz = frequency(body, to);
            assert!((hz - 440.0).abs() < 1.0, "{from} to {to} sounds at {hz} Hz");
            assert!((rms(body) - 0.354).abs() < 0.01, "{from} to {to} has an RMS of {}", rms(body));
        }
    }

    #[test]
    fn resampling_lines_output_up_with_input_in_time() {
        // A click at 0.5 s lands at 0.5 s after resampling, give or take a sample.
        let mut input = vec![0.0; 48_000];
        input[24_000] = 1.0;
        let out = resample(48_000, 44_100, &input, 1_000);
        let peak = out.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert!(peak.abs_diff(22_050) <= 1, "the click is at {peak}");
    }

    #[test]
    fn going_down_removes_what_the_lower_rate_cant_carry() {
        // 12 kHz can't exist at 16 kHz; it mustn't fold back as a tone at 4 kHz.
        let out = resample(48_000, 16_000, &sine(12_000.0, 0.5, 48_000, 1.0), 960);
        assert!(rms(&out[1_600..14_400]) < 0.001, "{} is left", rms(&out[1_600..14_400]));
    }
}
