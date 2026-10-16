use std::path::PathBuf;

pub(crate) fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

pub(crate) fn sine(hz: f64, amplitude: f64, rate: u32, seconds: f64) -> Vec<f32> {
    let n = (f64::from(rate) * seconds).round() as usize;
    (0..n).map(|i| (amplitude * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(rate)).sin()) as f32).collect()
}

pub(crate) fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// The frequency of a steady tone, from the time between its first and last rising zero crossings.
pub(crate) fn frequency(x: &[f32], rate: u32) -> f64 {
    let crossings: Vec<f64> = x
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, w)| i as f64 + f64::from(-w[0] / (w[1] - w[0])))
        .collect();
    match (crossings.first(), crossings.last()) {
        (Some(a), Some(b)) if b > a => (crossings.len() - 1) as f64 * f64::from(rate) / (b - a),
        _ => 0.0,
    }
}
