//! The latest durations a pool measured, for its p50 and p99 in `status`.

use std::collections::VecDeque;
use std::time::Duration;

const KEPT: usize = 1024;

#[derive(Default)]
pub struct Samples {
    recent: VecDeque<u64>,
}

impl Samples {
    pub fn add(&mut self, took: Duration) {
        if self.recent.len() == KEPT {
            self.recent.pop_front();
        }
        self.recent.push_back(took.as_micros().min(u64::MAX as u128) as u64);
    }

    /// p50 and p99 of the latest, in microseconds; none before the first.
    pub fn percentiles(&self) -> Option<jaw_proto::Percentiles> {
        if self.recent.is_empty() {
            return None;
        }
        let mut sorted: Vec<u64> = self.recent.iter().copied().collect();
        sorted.sort_unstable();
        let at = |q: usize| sorted[(sorted.len() * q / 100).min(sorted.len() - 1)];
        Some(jaw_proto::Percentiles { p50_us: at(50), p99_us: at(99) })
    }

    /// The longest of the latest, in microseconds; none before the first.
    #[cfg(test)]
    pub fn most(&self) -> Option<u64> {
        self.recent.iter().copied().max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_the_latest() {
        let mut samples = Samples::default();
        assert_eq!(samples.percentiles(), None);
        for us in 1..=100 {
            samples.add(Duration::from_micros(us));
        }
        assert_eq!(samples.percentiles(), Some(jaw_proto::Percentiles { p50_us: 51, p99_us: 100 }));
        for _ in 0..KEPT {
            samples.add(Duration::from_micros(7));
        }
        assert_eq!(samples.percentiles(), Some(jaw_proto::Percentiles { p50_us: 7, p99_us: 7 }));
    }
}
