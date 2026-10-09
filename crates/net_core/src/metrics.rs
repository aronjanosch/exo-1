//! Statistics of the replay matrix: mean and percentile.
pub fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}
pub fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>() / v.len().max(1) as f64).sqrt()
}
/// Sorts `v`. Index = floor(n * fraction), clamped.
pub fn percentile(v: &mut [f64], fraction: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 * fraction) as usize).min(v.len() - 1)]
}
