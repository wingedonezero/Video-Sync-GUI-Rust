//! QA check — 1:1 port of `vsg_core/correction/stepping/qa_check.py`.
//!
//! Post-correction quality assurance.
//!
//! Runs a fresh dense correlation between the corrected audio and the
//! reference to verify that the delay is now uniform at the anchor value.
//!
//! Uses the same dense sliding-window methodology as the main analysis
//! step for consistency.

use std::collections::HashMap;

use crate::analysis::correlation::decode::{decode_audio, get_audio_stream_info, normalize_lang, DEFAULT_SR};
use crate::analysis::correlation::dense::run_dense_correlation;
use crate::analysis::correlation::filtering::{apply_bandpass, apply_lowpass};
use crate::analysis::correlation::gpu_backend::cleanup_gpu;
use crate::analysis::correlation::run::resolve_method;
use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

/// Verify corrected audio matches reference at `base_delay_ms` — `verify_correction`
///
/// Uses dense sliding-window correlation (same as the main analysis)
/// to produce hundreds of delay estimates, then checks that the median
/// is near base_delay_ms and the variance is low.
///
/// Returns `(passed, metadata_dict)`.
#[allow(clippy::too_many_arguments)]
pub fn verify_correction(
    corrected_path: &str,
    ref_file_path: &str,
    base_delay_ms: i32,
    settings: &AppSettings,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    log: &dyn Fn(&str),
    skip_mode: bool,
) -> (bool, HashMap<String, serde_json::Value>) {
    log("  [QA] Running dense correlation on corrected audio...");

    let qa_threshold = settings.stepping_qa_threshold;
    let qa_min_pct = settings.stepping_qa_min_accepted_pct;

    // --- 1. Select audio streams ---
    let ref_lang = normalize_lang(Some(&settings.analysis_lang_source1));

    let (idx_ref, _) =
        get_audio_stream_info(ref_file_path, ref_lang.as_deref(), runner, tool_paths);
    let (idx_tgt, _) =
        get_audio_stream_info(corrected_path, None, runner, tool_paths);

    let (idx_ref, idx_tgt) = match (idx_ref, idx_tgt) {
        (Some(r), Some(t)) => (r, t),
        _ => {
            log("  [QA] FAILED: Could not locate audio streams");
            let mut meta = HashMap::new();
            meta.insert(
                "reason".to_string(),
                serde_json::json!("no_audio_streams"),
            );
            return (false, meta);
        }
    };

    // --- 2. Decode ---
    let use_soxr = settings.use_soxr;
    let ref_pcm = match decode_audio(ref_file_path, idx_ref, DEFAULT_SR, use_soxr, runner, tool_paths) {
        Ok(pcm) => pcm,
        Err(e) => {
            log(&format!("  [QA] FAILED: decode error: {e}"));
            let mut meta = HashMap::new();
            meta.insert("reason".to_string(), serde_json::json!("decode_error"));
            meta.insert("error".to_string(), serde_json::json!(e));
            return (false, meta);
        }
    };

    let tgt_pcm = match decode_audio(corrected_path, idx_tgt, DEFAULT_SR, use_soxr, runner, tool_paths) {
        Ok(pcm) => pcm,
        Err(e) => {
            log(&format!("  [QA] FAILED: decode error: {e}"));
            let mut meta = HashMap::new();
            meta.insert("reason".to_string(), serde_json::json!("decode_error"));
            meta.insert("error".to_string(), serde_json::json!(e));
            return (false, meta);
        }
    };

    // --- 3. Apply filtering (same as main analysis) ---
    let filtering_method = &settings.filtering_method;
    let sr_i32 = DEFAULT_SR as i32;
    let (ref_pcm, tgt_pcm) = match filtering_method.to_string().as_str() {
        "Dialogue Band-Pass Filter" => {
            let ref_filtered = apply_bandpass(
                &ref_pcm,
                sr_i32,
                settings.filter_bandpass_lowcut_hz,
                settings.filter_bandpass_highcut_hz,
                settings.filter_bandpass_order,
                Some(log),
            );
            let tgt_filtered = apply_bandpass(
                &tgt_pcm,
                sr_i32,
                settings.filter_bandpass_lowcut_hz,
                settings.filter_bandpass_highcut_hz,
                settings.filter_bandpass_order,
                Some(log),
            );
            (ref_filtered, tgt_filtered)
        }
        "Low-Pass Filter" => {
            let cutoff = settings.audio_bandlimit_hz;
            if cutoff > 0 {
                let taps = settings.filter_lowpass_taps;
                let ref_filtered =
                    apply_lowpass(&ref_pcm, sr_i32, cutoff, taps, Some(log));
                let tgt_filtered =
                    apply_lowpass(&tgt_pcm, sr_i32, cutoff, taps, Some(log));
                (ref_filtered, tgt_filtered)
            } else {
                (ref_pcm, tgt_pcm)
            }
        }
        _ => (ref_pcm, tgt_pcm),
    };

    // --- 4. Run dense correlation ---
    let method = resolve_method(settings, false);

    let results = run_dense_correlation(
        &ref_pcm,
        &tgt_pcm,
        DEFAULT_SR,
        method.as_ref(),
        settings.dense_window_s,
        settings.dense_hop_s,
        qa_threshold,
        settings.dense_silence_threshold_db,
        settings.dense_outlier_threshold_ms,
        settings.scan_start_percentage,
        settings.scan_end_percentage.min(100.0),
        Some(log),
        settings.detection_dbscan_epsilon_ms,
        settings.detection_dbscan_min_samples_pct,
    );

    // Release GPU resources
    cleanup_gpu();

    // --- 5. Evaluate results ---
    let accepted: Vec<_> = results.iter().filter(|r| r.accepted).collect();
    let total_windows = results.len();
    let qa_min = (total_windows as f64 * qa_min_pct / 100.0) as usize;
    let qa_min = qa_min.max(10);

    if accepted.len() < qa_min {
        let actual_pct = if total_windows > 0 {
            accepted.len() as f64 / total_windows as f64 * 100.0
        } else {
            0.0
        };
        log(&format!(
            "  [QA] FAILED: Not enough confident windows ({}/{} = {:.1}%, need {:.0}%)",
            accepted.len(),
            total_windows,
            actual_pct,
            qa_min_pct,
        ));
        let mut meta = HashMap::new();
        meta.insert(
            "reason".to_string(),
            serde_json::json!("insufficient_accepted"),
        );
        meta.insert("count".to_string(), serde_json::json!(accepted.len()));
        meta.insert("total".to_string(), serde_json::json!(total_windows));
        meta.insert("pct".to_string(), serde_json::json!(actual_pct));
        meta.insert(
            "required_pct".to_string(),
            serde_json::json!(qa_min_pct),
        );
        return (false, meta);
    }

    let mut delays: Vec<f64> = accepted
        .iter()
        .map(|r| r.delay_ms as f64)
        .collect();
    delays.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median_delay = if delays.is_empty() {
        0.0
    } else if delays.len() % 2 == 1 {
        delays[delays.len() / 2]
    } else {
        (delays[delays.len() / 2 - 1] + delays[delays.len() / 2]) / 2.0
    };

    let mean = delays.iter().sum::<f64>() / delays.len() as f64;
    let variance = delays.iter().map(|&d| (d - mean).powi(2)).sum::<f64>()
        / delays.len() as f64;
    let std_dev = variance.sqrt();

    // Median check -- corrected audio should have uniform delay at base value
    let tol = if skip_mode { 100.0 } else { 20.0 };
    if (median_delay - base_delay_ms as f64).abs() > tol {
        log(&format!(
            "  [QA] FAILED: Median delay {median_delay:.1}ms != base {base_delay_ms}ms (tolerance +/-{tol}ms)"
        ));
        let mut meta = HashMap::new();
        meta.insert(
            "reason".to_string(),
            serde_json::json!("median_mismatch"),
        );
        meta.insert("median".to_string(), serde_json::json!(median_delay));
        meta.insert("base".to_string(), serde_json::json!(base_delay_ms));
        return (false, meta);
    }

    // Stability check -- delay should be uniform (low variance)
    let std_limit = if skip_mode { 500.0 } else { 15.0 };
    if std_dev > std_limit {
        log(&format!("  [QA] FAILED: Unstable (std = {std_dev:.1}ms)"));
        let mut meta = HashMap::new();
        meta.insert("reason".to_string(), serde_json::json!("unstable"));
        meta.insert("std_dev".to_string(), serde_json::json!(std_dev));
        return (false, meta);
    }

    log(&format!(
        "  [QA] PASSED - median={median_delay:.1}ms, std={std_dev:.1}ms, {} windows",
        accepted.len()
    ));

    let mut meta = HashMap::new();
    meta.insert(
        "median_delay".to_string(),
        serde_json::json!(median_delay),
    );
    meta.insert("std_dev".to_string(), serde_json::json!(std_dev));
    meta.insert(
        "accepted_count".to_string(),
        serde_json::json!(accepted.len()),
    );

    (true, meta)
}
