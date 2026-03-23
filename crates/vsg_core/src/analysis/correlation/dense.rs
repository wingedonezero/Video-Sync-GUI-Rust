//! Dense sliding window correlation — 1:1 port of `correlation/dense.py`.

use std::time::Instant;

use super::registry::CorrelationMethod;
use crate::analysis::types::ChunkResult;

/// RMS energy in dB for a sample chunk — `_rms_db`
fn rms_db(samples: &[f32]) -> f64 {
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    if rms < 1e-12 {
        -120.0
    } else {
        20.0 * rms.log10()
    }
}

/// Run dense sliding window correlation over the full file — `run_dense_correlation`
#[allow(clippy::too_many_arguments)]
pub fn run_dense_correlation(
    ref_pcm: &[f32],
    tgt_pcm: &[f32],
    sr: i64,
    method: &dyn CorrelationMethod,
    window_s: f64,
    hop_s: f64,
    min_match: f64,
    silence_threshold_db: f64,
    _outlier_threshold_ms: f64,
    start_pct: f64,
    end_pct: f64,
    log: Option<&dyn Fn(&str)>,
    _dbscan_epsilon_ms: f64,
    _dbscan_min_samples_pct: f64,
) -> Vec<ChunkResult> {
    let noop = |_: &str| {};
    let log = log.unwrap_or(&noop);

    let window_samples = (window_s * sr as f64).round() as usize;
    let hop_samples = (hop_s * sr as f64).round() as usize;
    let min_len = ref_pcm.len().min(tgt_pcm.len());
    let duration_s = min_len as f64 / sr as f64;

    // Apply scan range
    let scan_start = (duration_s * (start_pct / 100.0) * sr as f64).round() as usize;
    let scan_end = (duration_s * (end_pct / 100.0) * sr as f64)
        .round()
        .min(min_len as f64) as usize;

    let total_positions = if scan_end > scan_start + window_samples {
        (scan_end - scan_start - window_samples) / hop_samples + 1
    } else {
        0
    };

    log(&format!("[Dense Correlation] {}", method.name()));
    log(&format!(
        "  Window: {window_s}s, Hop: {hop_s}s, Range: {start_pct:.0}%-{end_pct:.0}% \
         ({:.1}s - {:.1}s)",
        scan_start as f64 / sr as f64,
        scan_end as f64 / sr as f64
    ));
    log(&format!("  Total windows: {total_positions}"));

    let mut results: Vec<ChunkResult> = Vec::new();
    let mut silence_count = 0usize;

    let t0 = Instant::now();
    let mut last_report = t0;

    let mut pos = scan_start;
    let mut window_idx = 0usize;

    while pos + window_samples <= scan_end {
        let center_s = (pos as f64 + window_samples as f64 / 2.0) / sr as f64;

        let ref_win = &ref_pcm[pos..pos + window_samples];
        let tgt_win = &tgt_pcm[pos..pos + window_samples];

        let ref_db = rms_db(ref_win);
        let tgt_db = rms_db(tgt_win);

        if ref_db < silence_threshold_db || tgt_db < silence_threshold_db {
            silence_count += 1;
        } else {
            let (raw_ms, confidence) = method.find_delay(ref_win, tgt_win, sr);
            let accepted = confidence >= min_match;

            results.push(ChunkResult {
                delay_ms: raw_ms.round() as i32,
                raw_delay_ms: raw_ms,
                match_pct: confidence,
                start_s: center_s,
                accepted,
            });
        }

        pos += hop_samples;
        window_idx += 1;

        // Progress reporting every 5 seconds
        let now = Instant::now();
        if now.duration_since(last_report).as_secs_f64() > 5.0 {
            let done = window_idx;
            let pct = if total_positions > 0 {
                done as f64 / total_positions as f64 * 100.0
            } else {
                100.0
            };
            let elapsed = now.duration_since(t0).as_secs_f64();
            let rate = if elapsed > 0.0 {
                done as f64 / elapsed
            } else {
                0.0
            };
            let eta = if rate > 0.0 {
                (total_positions - done) as f64 / rate
            } else {
                0.0
            };
            log(&format!(
                "  [{pct:5.1}%] {done}/{total_positions} ({rate:.0}/s, ETA {eta:.0}s)"
            ));
            last_report = now;
        }
    }

    let elapsed = t0.elapsed().as_secs_f64();
    let active_count = results.len();

    log(&format!(
        "  Done: {active_count} active + {silence_count} silence = {} windows in {elapsed:.1}s \
         ({:.0} windows/s)",
        active_count + silence_count,
        (active_count + silence_count) as f64 / elapsed.max(0.001)
    ));

    results
}
