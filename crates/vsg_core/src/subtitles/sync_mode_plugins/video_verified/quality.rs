//! Frame quality measurement and sequence verification for video-verified sync.
//!
//! 1:1 port of `video_verified/quality.py`.

use std::collections::HashMap;
use std::path::Path;

use crate::subtitles::frame_utils::frame_hashing::{compare_frames_multi, MultiMetricResult};
use crate::subtitles::frame_utils::video_reader::{VideoFrame, VideoReader};

use super::offset::get_vfr_frame_for_time;

/// Result of verifying a sequence of consecutive frames.
#[derive(Debug, Clone)]
pub struct SequenceResult {
    /// Frames matching on primary metric
    pub matched: i64,
    /// Average primary metric distance
    pub avg_distance: f64,
    /// Per-frame primary metric distances
    pub distances: Vec<i32>,
    /// Frames actually compared
    pub total_tested: i64,
    /// Frames with phash hamming distance = 0
    pub phash_exact: i64,
    /// Per-frame phash distances
    pub phash_distances: Vec<i32>,
    /// Per-frame SSIM distances
    pub ssim_distances: Vec<f64>,
    /// Per-frame raw MSE values
    pub mse_values: Vec<f64>,
}

/// Normalize two frames to the same resolution for robust comparison.
pub fn normalize_frame_pair(
    source_frame: &VideoFrame,
    target_frame: &VideoFrame,
) -> (VideoFrame, VideoFrame) {
    if source_frame.dimensions() == target_frame.dimensions() {
        return (source_frame.clone(), target_frame.clone());
    }

    // Standard comparison size
    let target_size = (320u32, 240u32);
    let source_norm = source_frame.resize(target_size.0, target_size.1);
    let target_norm = target_frame.resize(target_size.0, target_size.1);
    (source_norm, target_norm)
}

/// Verify that a sequence of consecutive frames match between source and target.
pub fn verify_frame_sequence(
    source_start_idx: i64,
    target_start_idx: i64,
    sequence_length: i32,
    source_reader: &VideoReader,
    target_reader: &VideoReader,
    hash_algorithm: &str,
    hash_size: usize,
    hash_threshold: i32,
    comparison_method: &str,
    ssim_threshold: Option<i32>,
    ivtc_tolerance: i32,
    use_global_ssim: bool,
) -> SequenceResult {
    let eff_ssim_thresh = ssim_threshold.unwrap_or(10);
    let eff_mse_thresh = 5;

    let mut matched: i64 = 0;
    let mut distances: Vec<i32> = Vec::new();
    let mut phash_exact: i64 = 0;
    let mut phash_distances: Vec<i32> = Vec::new();
    let mut ssim_distances: Vec<f64> = Vec::new();
    let mut mse_values: Vec<f64> = Vec::new();
    let mut total_tested: i64 = 0;

    for i in 0..sequence_length {
        let source_idx = source_start_idx + i as i64;

        // For IVTC content, try +/-tolerance around the expected target frame
        let mut target_candidates = vec![target_start_idx + i as i64];
        if ivtc_tolerance > 0 {
            for delta in 1..=ivtc_tolerance {
                target_candidates.push(target_start_idx + i as i64 + delta as i64);
                target_candidates.push(target_start_idx + i as i64 - delta as i64);
            }
        }

        let mut best_multi: Option<MultiMetricResult> = None;
        let mut best_primary_dist = f64::INFINITY;
        let mut best_match = false;

        for target_idx in &target_candidates {
            if *target_idx < 0 {
                continue;
            }

            let source_frame = match source_reader.get_frame_at_index(source_idx) {
                Some(f) => f,
                None => continue,
            };
            let target_frame = match target_reader.get_frame_at_index(*target_idx) {
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

            let (primary_dist, is_match) = match comparison_method {
                "ssim" => (multi.ssim_distance, multi.ssim_match),
                "mse" => (multi.mse_distance, multi.mse_match),
                _ => (multi.phash_distance as f64, multi.phash_match),
            };

            if primary_dist < best_primary_dist {
                best_primary_dist = primary_dist;
                best_match = is_match;
                best_multi = Some(multi);
            }
        }

        if let Some(multi) = best_multi {
            total_tested += 1;
            distances.push(best_primary_dist as i32);
            phash_distances.push(multi.phash_distance);
            ssim_distances.push(multi.ssim_distance);
            mse_values.push(multi.mse_value);

            if multi.phash_distance == 0 {
                phash_exact += 1;
            }
            if best_match {
                matched += 1;
            }
        }
    }

    let avg_dist = if !distances.is_empty() {
        distances.iter().map(|&d| d as f64).sum::<f64>() / distances.len() as f64
    } else {
        f64::INFINITY
    };

    SequenceResult {
        matched,
        avg_distance: avg_dist,
        distances,
        total_tested,
        phash_exact,
        phash_distances,
        ssim_distances,
        mse_values,
    }
}

/// Measure quality of a candidate frame offset using multi-metric sequence verification.
pub fn measure_frame_offset_quality(
    frame_offset: i64,
    checkpoint_times: &[f64],
    source_reader: &VideoReader,
    target_reader: &VideoReader,
    fps: f64,
    source_frame_duration_ms: f64,
    target_frame_duration_ms: f64,
    _window_radius: i32,
    hash_algorithm: &str,
    hash_size: usize,
    hash_threshold: i32,
    comparison_method: &str,
    log: &dyn Fn(&str),
    sequence_verify_length: i32,
    ssim_threshold: Option<i32>,
    ivtc_tolerance: i32,
    use_global_ssim: bool,
) -> HashMap<String, serde_json::Value> {
    let eff_ssim_thresh = ssim_threshold.unwrap_or(10);
    let eff_mse_thresh = 5;

    let mut total_score: f64 = 0.0;
    let mut matched_count: i64 = 0;
    let mut sequence_verified_count: i64 = 0;
    let mut distances: Vec<f64> = Vec::new();
    let mut mse_values: Vec<f64> = Vec::new();
    let mut match_details: Vec<serde_json::Value> = Vec::new();
    let mut per_checkpoint_summary: Vec<serde_json::Value> = Vec::new();

    let mut total_frames_tested: i64 = 0;
    let mut total_frames_matched: i64 = 0;
    let mut total_phash_exact: i64 = 0;

    for &checkpoint_ms in checkpoint_times {
        // Source frame at checkpoint time
        let source_frame_idx = source_reader
            .get_frame_index_for_time(checkpoint_ms)
            .unwrap_or((checkpoint_ms / source_frame_duration_ms) as i64);

        // Target frame with this offset (TIME-based)
        let offset_time_ms = frame_offset as f64 * source_frame_duration_ms;
        let target_time_ms = checkpoint_ms + offset_time_ms;

        let target_frame_idx = target_reader
            .get_frame_index_for_time(target_time_ms)
            .unwrap_or((target_time_ms / target_frame_duration_ms) as i64);

        if target_frame_idx < 0 {
            continue;
        }

        // Get frames
        let source_frame = match source_reader.get_frame_at_index(source_frame_idx) {
            Some(f) => f,
            None => continue,
        };
        let target_frame = match target_reader.get_frame_at_index(target_frame_idx) {
            Some(f) => f,
            None => continue,
        };

        let (source_norm, target_norm) = normalize_frame_pair(&source_frame, &target_frame);

        let initial_multi = compare_frames_multi(
            &source_norm,
            &target_norm,
            hash_algorithm,
            hash_size,
            hash_threshold,
            eff_ssim_thresh,
            eff_mse_thresh,
            use_global_ssim,
        );

        let (initial_distance, initial_match) = match comparison_method {
            "ssim" => (initial_multi.ssim_distance, initial_multi.ssim_match),
            "mse" => (initial_multi.mse_distance, initial_multi.mse_match),
            _ => (initial_multi.phash_distance as f64, initial_multi.phash_match),
        };

        distances.push(initial_distance);
        mse_values.push(initial_multi.mse_value);

        // Sequence verification
        let seq_result = verify_frame_sequence(
            source_frame_idx,
            target_frame_idx,
            sequence_verify_length,
            source_reader,
            target_reader,
            hash_algorithm,
            hash_size,
            hash_threshold,
            comparison_method,
            ssim_threshold,
            ivtc_tolerance,
            use_global_ssim,
        );

        let min_sequence_matches = (sequence_verify_length as f64 * 0.7) as i64;
        let sequence_verified = seq_result.matched >= min_sequence_matches;

        total_frames_tested += seq_result.total_tested;
        total_frames_matched += seq_result.matched;
        total_phash_exact += seq_result.phash_exact;

        let cp_avg_ssim = if !seq_result.ssim_distances.is_empty() {
            seq_result.ssim_distances.iter().sum::<f64>() / seq_result.ssim_distances.len() as f64
        } else {
            f64::INFINITY
        };
        let cp_avg_mse = if !seq_result.mse_values.is_empty() {
            seq_result.mse_values.iter().sum::<f64>() / seq_result.mse_values.len() as f64
        } else {
            f64::INFINITY
        };

        per_checkpoint_summary.push(serde_json::json!({
            "checkpoint_ms": checkpoint_ms,
            "source_frame": source_frame_idx,
            "target_frame": target_frame_idx,
            "seq_matched": seq_result.matched,
            "seq_total": seq_result.total_tested,
            "phash_exact": seq_result.phash_exact,
            "avg_ssim_dist": cp_avg_ssim,
            "avg_mse": cp_avg_mse,
            "verified": sequence_verified,
        }));

        match_details.push(serde_json::json!({
            "source_frame": source_frame_idx,
            "target_frame": target_frame_idx,
            "distance": initial_distance,
            "is_match": initial_match,
            "sequence_matched": seq_result.matched,
            "sequence_length": sequence_verify_length,
            "sequence_verified": sequence_verified,
            "sequence_avg_dist": seq_result.avg_distance,
            "phash_distance": initial_multi.phash_distance,
            "ssim_distance": initial_multi.ssim_distance,
            "mse_value": initial_multi.mse_value,
        }));

        if sequence_verified {
            sequence_verified_count += 1;
            matched_count += 1;
            let seq_ratio = seq_result.matched as f64 / sequence_verify_length as f64;
            total_score += 2.0 * seq_ratio;
        } else if initial_match {
            matched_count += 1;
            total_score += 0.3;
        } else {
            total_score += (0.1 - initial_distance / (hash_threshold as f64 * 4.0)).max(0.0);
        }
    }

    let avg_distance = if !distances.is_empty() {
        distances.iter().sum::<f64>() / distances.len() as f64
    } else {
        f64::INFINITY
    };
    let avg_mse = if !mse_values.is_empty() {
        mse_values.iter().sum::<f64>() / mse_values.len() as f64
    } else {
        f64::INFINITY
    };

    let mut result = HashMap::new();
    result.insert("score".to_string(), serde_json::json!(total_score));
    result.insert("matched".to_string(), serde_json::json!(matched_count));
    result.insert("sequence_verified".to_string(), serde_json::json!(sequence_verified_count));
    result.insert("avg_distance".to_string(), serde_json::json!(avg_distance));
    result.insert("avg_mse".to_string(), serde_json::json!(avg_mse));
    result.insert("match_details".to_string(), serde_json::json!(match_details));
    result.insert("total_frames_tested".to_string(), serde_json::json!(total_frames_tested));
    result.insert("total_frames_matched".to_string(), serde_json::json!(total_frames_matched));
    result.insert("phash_exact_matches".to_string(), serde_json::json!(total_phash_exact));
    result.insert("per_checkpoint_summary".to_string(), serde_json::json!(per_checkpoint_summary));

    result
}
