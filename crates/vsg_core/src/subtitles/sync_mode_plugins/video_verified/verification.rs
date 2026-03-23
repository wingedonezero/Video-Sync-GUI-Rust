//! Final verification pass for video-verified sync.
//!
//! After the search phase picks a winning frame offset, runs an independent
//! verification using NEW checkpoint times and multi-metric comparison.
//!
//! 1:1 port of `video_verified/verification.py`.

use std::collections::HashMap;

use crate::subtitles::frame_utils::frame_hashing::compare_frames_multi;
use crate::subtitles::frame_utils::video_reader::VideoReader;

use super::quality::normalize_frame_pair;

/// Run a final verification pass on the winning frame offset.
///
/// Uses independent checkpoint times (different from search phase) and
/// multi-metric comparison to cross-validate the result.
pub fn run_final_verification(
    best_frame_offset: i64,
    source_reader: &VideoReader,
    target_reader: &VideoReader,
    source_duration: f64,
    source_frame_duration_ms: f64,
    target_frame_duration_ms: f64,
    hash_algorithm: &str,
    hash_size: usize,
    hash_threshold: i32,
    ssim_threshold: Option<i32>,
    use_global_ssim: bool,
    num_verification_points: usize,
    _checkpoint_times_used: Option<&[f64]>,
    metric_agreement: i32,
    log: Option<&dyn Fn(&str)>,
) -> HashMap<String, serde_json::Value> {
    let log_fn = |msg: &str| {
        if let Some(f) = log {
            f(msg);
        }
    };

    let eff_ssim_thresh = ssim_threshold.unwrap_or(10);
    let eff_mse_thresh = 5;

    // Generate verification checkpoint times (different from search checkpoints)
    let margin_pct = 5.0;
    let start_pct = margin_pct;
    let end_pct = 100.0 - margin_pct;
    let span_pct = end_pct - start_pct;

    let mut verify_times = Vec::new();
    for i in 0..num_verification_points {
        let pos = start_pct + span_pct * (i as f64 + 0.3) / num_verification_points as f64;
        let time_ms = source_duration * pos / 100.0;
        verify_times.push(time_ms);
    }

    let mut results = Vec::new();
    let mut phash_matched: i64 = 0;
    let mut ssim_matched: i64 = 0;
    let mut mse_matched: i64 = 0;
    let mut phash_exact: i64 = 0;
    let mut total_tested: i64 = 0;

    for vt_ms in &verify_times {
        let source_idx = source_reader
            .get_frame_index_for_time(*vt_ms)
            .unwrap_or((*vt_ms / source_frame_duration_ms) as i64);

        let offset_time_ms = best_frame_offset as f64 * source_frame_duration_ms;
        let target_time_ms = vt_ms + offset_time_ms;

        let target_idx = target_reader
            .get_frame_index_for_time(target_time_ms)
            .unwrap_or((target_time_ms / target_frame_duration_ms) as i64);

        if target_idx < 0 {
            continue;
        }

        let source_frame = match source_reader.get_frame_at_index(source_idx) {
            Some(f) => f,
            None => continue,
        };
        let target_frame = match target_reader.get_frame_at_index(target_idx) {
            Some(f) => f,
            None => continue,
        };

        let (source_norm, target_norm) = normalize_frame_pair(&source_frame, &target_frame);

        let multi = compare_frames_multi(
            &source_norm,
            &target_norm,
            hash_algorithm,
            hash_size,
            hash_threshold,
            eff_ssim_thresh,
            eff_mse_thresh,
            use_global_ssim,
        );

        total_tested += 1;
        if multi.phash_match {
            phash_matched += 1;
        }
        if multi.phash_distance == 0 {
            phash_exact += 1;
        }
        if multi.ssim_match {
            ssim_matched += 1;
        }
        if multi.mse_match {
            mse_matched += 1;
        }

        results.push(serde_json::json!({
            "time_ms": vt_ms,
            "source_idx": source_idx,
            "target_idx": target_idx,
            "phash_distance": multi.phash_distance,
            "ssim_distance": multi.ssim_distance,
            "mse_value": multi.mse_value,
            "phash_match": multi.phash_match,
            "ssim_match": multi.ssim_match,
            "mse_match": multi.mse_match,
        }));
    }

    // Compute match rates
    if total_tested == 0 {
        let mut result = HashMap::new();
        result.insert("confidence".to_string(), serde_json::json!("LOW"));
        result.insert("frames_matched".to_string(), serde_json::json!(0));
        result.insert("frames_tested".to_string(), serde_json::json!(0));
        result.insert("match_rate".to_string(), serde_json::json!(0.0));
        result.insert("phash_exact".to_string(), serde_json::json!(0));
        result.insert("phash_matched".to_string(), serde_json::json!(0));
        result.insert("ssim_matched".to_string(), serde_json::json!(0));
        result.insert("mse_matched".to_string(), serde_json::json!(0));
        result.insert("metric_agreement".to_string(), serde_json::json!(metric_agreement));
        result.insert("results".to_string(), serde_json::json!([]));
        return result;
    }

    // "frames_matched" = matched on SSIM
    let frames_matched = ssim_matched;
    let match_rate = frames_matched as f64 / total_tested as f64;

    let confidence = if match_rate >= 0.90 && metric_agreement == 3 {
        "HIGH"
    } else if match_rate >= 0.70 && metric_agreement >= 2 {
        "MEDIUM"
    } else {
        "LOW"
    };

    let mut result = HashMap::new();
    result.insert("confidence".to_string(), serde_json::json!(confidence));
    result.insert("frames_matched".to_string(), serde_json::json!(frames_matched));
    result.insert("frames_tested".to_string(), serde_json::json!(total_tested));
    result.insert("match_rate".to_string(), serde_json::json!(match_rate));
    result.insert("phash_exact".to_string(), serde_json::json!(phash_exact));
    result.insert("phash_matched".to_string(), serde_json::json!(phash_matched));
    result.insert("ssim_matched".to_string(), serde_json::json!(ssim_matched));
    result.insert("mse_matched".to_string(), serde_json::json!(mse_matched));
    result.insert("metric_agreement".to_string(), serde_json::json!(metric_agreement));
    result.insert("results".to_string(), serde_json::json!(results));

    result
}
