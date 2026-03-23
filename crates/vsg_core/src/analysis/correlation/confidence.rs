//! Peak confidence normalization — 1:1 port of `correlation/confidence.py`.
//!
//! Shared by multiple correlation methods to convert raw peaks to 0-100 score.
//! Note: This is the CPU/numpy version used as a fallback.
//! GPU methods use gpu_correlation::scc_confidence/psr_confidence instead.

/// Normalize peak confidence — `normalize_peak_confidence`
///
/// Uses three normalization strategies:
/// 1. peak / median (prominence over noise floor)
/// 2. peak / second_best (uniqueness of the match)
/// 3. peak / local_stddev (signal-to-noise ratio)
pub fn normalize_peak_confidence(correlation_array: &[f64], peak_idx: usize) -> f64 {
    let abs_corr: Vec<f64> = correlation_array.iter().map(|v| v.abs()).collect();
    let peak_value = abs_corr[peak_idx];

    // Metric 1: Noise floor using median
    let mut sorted = abs_corr.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let noise_floor_median = sorted[sorted.len() / 2];
    let prominence_ratio = peak_value / (noise_floor_median + 1e-9);

    // Metric 2: Second-best peak (excluding immediate neighbors)
    let neighbor_range = (abs_corr.len() / 100).max(1);
    let start_mask = peak_idx.saturating_sub(neighbor_range);
    let end_mask = (peak_idx + neighbor_range + 1).min(abs_corr.len());

    let second_best = abs_corr
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < start_mask || *i >= end_mask)
        .map(|(_, &v)| v)
        .fold(0.0f64, f64::max);
    let second_best = if second_best > 0.0 {
        second_best
    } else {
        noise_floor_median
    };
    let uniqueness_ratio = peak_value / (second_best + 1e-9);

    // Metric 3: SNR using 90th percentile background
    let threshold_90_idx = (sorted.len() as f64 * 0.9) as usize;
    let threshold_90 = sorted[threshold_90_idx.min(sorted.len() - 1)];
    let background: Vec<f64> = abs_corr.iter().filter(|&&v| v < threshold_90).copied().collect();
    let bg_stddev = if background.len() > 10 {
        let mean = background.iter().sum::<f64>() / background.len() as f64;
        let variance =
            background.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / background.len() as f64;
        variance.sqrt()
    } else {
        1e-9
    };
    let snr_ratio = peak_value / (bg_stddev + 1e-9);

    // Combine metrics
    let confidence = (prominence_ratio * 5.0) + (uniqueness_ratio * 8.0) + (snr_ratio * 1.5);
    let confidence = confidence / 3.0;

    confidence.clamp(0.0, 100.0)
}
