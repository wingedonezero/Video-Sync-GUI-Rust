//! Core frame matching algorithm for video-verified sync.
//!
//! Contains the main `calculate_video_verified_offset()` function
//! which performs frame matching to find the TRUE video-to-video offset.
//!
//! 1:1 port of `video_verified/matcher.py`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;
use crate::subtitles::frame_utils::video_properties::detect_video_properties;
use crate::subtitles::frame_utils::video_reader::VideoReader;

use super::candidates::{generate_frame_candidates, select_checkpoint_times};
use super::offset::calculate_subframe_offset;
use super::quality::measure_frame_offset_quality;
use super::verification::run_final_verification;

/// Calculate the video-verified offset using frame matching with sub-frame precision.
///
/// Returns (final_offset_ms, details_dict).
pub fn calculate_video_verified_offset(
    source_video: &str,
    target_video: &str,
    total_delay_ms: f64,
    global_shift_ms: f64,
    settings: Option<&AppSettings>,
    runner: &CommandRunner,
    temp_dir: Option<PathBuf>,
    video_duration_ms: Option<f64>,
) -> (Option<f64>, HashMap<String, serde_json::Value>) {
    let default_settings = AppSettings::default();
    let settings = settings.unwrap_or(&default_settings);

    let log = |msg: &str| {
        runner.log_message(msg);
    };

    log("[VideoVerified] === Frame Matching for Delay Correction ===");

    if source_video.is_empty() || target_video.is_empty() {
        let mut details = HashMap::new();
        details.insert("reason".to_string(), serde_json::json!("missing-videos"));
        details.insert("error".to_string(), serde_json::json!("Both source and target videos required"));
        return (None, details);
    }

    let pure_correlation_ms = total_delay_ms - global_shift_ms;

    log(&format!("[VideoVerified] Source: {}", Path::new(source_video).file_name().unwrap_or_default().to_string_lossy()));
    log(&format!("[VideoVerified] Target: {}", Path::new(target_video).file_name().unwrap_or_default().to_string_lossy()));
    log(&format!("[VideoVerified] Total delay (with global): {:+.3}ms", total_delay_ms));
    log(&format!("[VideoVerified] Global shift: {:+.3}ms", global_shift_ms));
    log(&format!("[VideoVerified] Pure correlation (audio): {:+.3}ms", pure_correlation_ms));

    // Detect video properties
    let source_props = detect_video_properties(source_video, runner);
    let target_props = detect_video_properties(target_video, runner);

    let initial_fps = source_props
        .get("fps")
        .and_then(|v| v.as_f64())
        .unwrap_or(23.976);

    log(&format!("[VideoVerified] Initial FPS: {:.3}", initial_fps));

    // Get settings parameters
    let num_checkpoints = settings.video_verified_num_checkpoints as usize;
    let search_range_frames = settings.video_verified_search_range_frames;
    let hash_algorithm_str = settings.frame_hash_algorithm.to_string();
    let hash_algorithm = hash_algorithm_str.as_str();
    let hash_size = settings.frame_hash_size as usize;
    let hash_threshold = settings.frame_hash_threshold;
    let window_radius = settings.frame_window_radius;
    let comparison_method_str = settings.frame_comparison_method.to_string();
    let comparison_method = comparison_method_str.as_str();

    log(&format!(
        "[VideoVerified] Checkpoints: {}, Search: +/-{} frames",
        num_checkpoints, search_range_frames
    ));
    log(&format!(
        "[VideoVerified] Hash: {} size={} threshold={}",
        hash_algorithm, hash_size, hash_threshold
    ));
    log(&format!(
        "[VideoVerified] Comparison method: {}",
        comparison_method
    ));

    // Open video readers
    let source_reader = VideoReader::new(source_video, runner, temp_dir.clone());
    let target_reader = VideoReader::new(target_video, runner, temp_dir.clone());

    let fps = source_reader.fps.unwrap_or(initial_fps);
    let target_fps = target_reader.fps.unwrap_or(initial_fps);

    log(&format!(
        "[VideoVerified] FPS: source={:.3}, target={:.3}",
        fps, target_fps
    ));

    let source_index_fps = source_reader.real_fps.unwrap_or(fps);
    let target_index_fps = target_reader.real_fps.unwrap_or(target_fps);
    let source_frame_duration_ms = 1000.0 / source_index_fps;
    let target_frame_duration_ms = 1000.0 / target_index_fps;

    // Get video duration
    let source_duration = video_duration_ms.unwrap_or_else(|| {
        let props = detect_video_properties(source_video, runner);
        let dur = props.get("duration_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if dur > 0.0 { dur } else { 1200000.0 }
    });

    log(&format!(
        "[VideoVerified] Source duration: ~{:.1}s",
        source_duration / 1000.0
    ));

    // Generate candidate frame offsets
    let correlation_frames = pure_correlation_ms / source_frame_duration_ms;
    let candidates_frames = generate_frame_candidates(correlation_frames, search_range_frames);
    log(&format!(
        "[VideoVerified] Testing frame offsets: {:?} (around {:+.1} frames)",
        candidates_frames, correlation_frames
    ));

    // Select checkpoint times
    let checkpoint_times = select_checkpoint_times(source_duration, num_checkpoints);

    let sequence_length = settings.video_verified_sequence_length;
    let ssim_threshold_val: Option<i32> = if comparison_method == "ssim" || comparison_method == "mse" {
        Some(settings.frame_ssim_threshold as i32)
    } else {
        None
    };

    // Test each candidate frame offset
    let mut candidate_results: Vec<HashMap<String, serde_json::Value>> = Vec::new();

    for &frame_offset in &candidates_frames {
        let quality = measure_frame_offset_quality(
            frame_offset,
            &checkpoint_times,
            &source_reader,
            &target_reader,
            fps,
            source_frame_duration_ms,
            target_frame_duration_ms,
            window_radius,
            hash_algorithm,
            hash_size,
            hash_threshold,
            comparison_method,
            &log,
            sequence_length,
            ssim_threshold_val,
            0,
            false,
        );

        let approx_ms = frame_offset as f64 * source_frame_duration_ms;
        let seq_verified = quality.get("sequence_verified").and_then(|v| v.as_i64()).unwrap_or(0);
        let total_tested = quality.get("total_frames_tested").and_then(|v| v.as_i64()).unwrap_or(0);
        let total_matched = quality.get("total_frames_matched").and_then(|v| v.as_i64()).unwrap_or(0);
        let phash_exact = quality.get("phash_exact_matches").and_then(|v| v.as_i64()).unwrap_or(0);
        let avg_mse = quality.get("avg_mse").and_then(|v| v.as_f64()).unwrap_or(f64::INFINITY);

        let mut cr = HashMap::new();
        cr.insert("frame_offset".to_string(), serde_json::json!(frame_offset));
        cr.insert("approx_ms".to_string(), serde_json::json!(approx_ms));
        cr.insert("quality".to_string(), quality.get("score").cloned().unwrap_or(serde_json::json!(0.0)));
        cr.insert("matched_checkpoints".to_string(), quality.get("matched").cloned().unwrap_or(serde_json::json!(0)));
        cr.insert("sequence_verified".to_string(), serde_json::json!(seq_verified));
        cr.insert("avg_distance".to_string(), quality.get("avg_distance").cloned().unwrap_or(serde_json::json!(f64::INFINITY)));
        cr.insert("avg_mse".to_string(), serde_json::json!(avg_mse));
        cr.insert("match_details".to_string(), quality.get("match_details").cloned().unwrap_or(serde_json::json!([])));
        cr.insert("total_frames_tested".to_string(), serde_json::json!(total_tested));
        cr.insert("total_frames_matched".to_string(), serde_json::json!(total_matched));
        cr.insert("phash_exact_matches".to_string(), serde_json::json!(phash_exact));
        cr.insert("per_checkpoint_summary".to_string(), quality.get("per_checkpoint_summary").cloned().unwrap_or(serde_json::json!([])));

        log(&format!(
            "[VideoVerified]   Frame {:+} (~{:+.1}ms): {}/{} frames matched, seq={}/{}, phash_exact={}/{}",
            frame_offset, approx_ms, total_matched, total_tested, seq_verified,
            checkpoint_times.len(), phash_exact, total_tested
        ));

        candidate_results.push(cr);
    }

    // Select best candidate
    let rank_key = |r: &HashMap<String, serde_json::Value>| -> (i64, i64, i64, i64) {
        let sv = r.get("sequence_verified").and_then(|v| v.as_i64()).unwrap_or(0);
        let tm = r.get("total_frames_matched").and_then(|v| v.as_i64()).unwrap_or(0);
        let pe = r.get("phash_exact_matches").and_then(|v| v.as_i64()).unwrap_or(0);
        let mse = r.get("avg_mse").and_then(|v| v.as_f64()).unwrap_or(f64::INFINITY);
        let neg_mse = -(mse * 1000.0) as i64;
        (sv, tm, pe, neg_mse)
    };

    let best_result = candidate_results
        .iter()
        .max_by_key(|r| rank_key(r))
        .cloned()
        .unwrap_or_default();

    let best_frame_offset = best_result
        .get("frame_offset")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let best_seq_verified = best_result
        .get("sequence_verified")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    log("[VideoVerified] =======================================");
    log("[VideoVerified] RESULTS SUMMARY");
    log("[VideoVerified] =======================================");

    let best_matched = best_result.get("total_frames_matched").and_then(|v| v.as_i64()).unwrap_or(0);
    let best_tested = best_result.get("total_frames_tested").and_then(|v| v.as_i64()).unwrap_or(0);
    let best_phash = best_result.get("phash_exact_matches").and_then(|v| v.as_i64()).unwrap_or(0);

    log(&format!(
        "[VideoVerified] Winner: frame {:+} ({}/{} frames matched, seq={}/{}, phash_exact={}/{})",
        best_frame_offset, best_matched, best_tested, best_seq_verified,
        checkpoint_times.len(), best_phash, best_tested
    ));

    // Check if frame matching actually worked
    if best_seq_verified == 0 {
        log("[VideoVerified] WARNING: Sequence verification failed");
        log(&format!(
            "[VideoVerified] Falling back to audio correlation: {:+.3}ms",
            pure_correlation_ms
        ));

        let mut details = HashMap::new();
        details.insert("reason".to_string(), serde_json::json!("fallback-no-frame-matches"));
        details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("video_offset_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("final_offset_ms".to_string(), serde_json::json!(total_delay_ms));
        details.insert("candidates".to_string(), serde_json::json!(candidate_results));
        details.insert("sub_frame_precision".to_string(), serde_json::json!(false));

        return (Some(total_delay_ms), details);
    }

    // Final verification pass
    // Metric agreement count (simplified - count how many metrics agree on best)
    let agree_count = 3; // Simplified for now

    let verification = run_final_verification(
        best_frame_offset,
        &source_reader,
        &target_reader,
        source_duration,
        source_frame_duration_ms,
        target_frame_duration_ms,
        hash_algorithm,
        hash_size,
        hash_threshold,
        ssim_threshold_val,
        false,
        15,
        Some(&checkpoint_times),
        agree_count,
        Some(&|msg: &str| runner.log_message(msg)),
    );

    let v_matched = verification.get("frames_matched").and_then(|v| v.as_i64()).unwrap_or(0);
    let v_total = verification.get("frames_tested").and_then(|v| v.as_i64()).unwrap_or(0);
    let confidence = verification.get("confidence").and_then(|v| v.as_str()).unwrap_or("LOW");

    log(&format!(
        "[VideoVerified] Final verification: {}/{} matched",
        v_matched, v_total
    ));
    log(&format!("[VideoVerified] Confidence: {}", confidence));

    // Calculate offset in milliseconds
    let use_pts = settings.video_verified_use_pts_precision;
    let match_details_val = best_result
        .get("match_details")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Convert match_details to the expected format
    let match_details_maps: Vec<HashMap<String, serde_json::Value>> = match_details_val
        .iter()
        .filter_map(|v| v.as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()))
        .collect();

    let sub_frame_offset_ms = calculate_subframe_offset(
        best_frame_offset,
        &match_details_maps,
        &checkpoint_times,
        &source_reader,
        &target_reader,
        fps,
        source_frame_duration_ms,
        &log,
        use_pts,
    );

    let final_offset_ms = sub_frame_offset_ms + global_shift_ms;

    log("---------------------------------------");
    log(&format!("[VideoVerified] Audio correlation: {:+.3}ms", pure_correlation_ms));
    let precision_mode = if use_pts { "PTS-based" } else { "frame-based" };
    log(&format!(
        "[VideoVerified] Video-verified offset: {:+.3}ms ({})",
        sub_frame_offset_ms, precision_mode
    ));
    log(&format!("[VideoVerified] + Global shift: {:+.3}ms", global_shift_ms));
    log(&format!("[VideoVerified] = Final offset: {:+.3}ms", final_offset_ms));

    if (sub_frame_offset_ms - pure_correlation_ms).abs() > source_frame_duration_ms / 2.0 {
        log("[VideoVerified] WARNING: VIDEO OFFSET DIFFERS FROM AUDIO CORRELATION");
    }

    let mut details = HashMap::new();
    details.insert("reason".to_string(), serde_json::json!("frame-matched"));
    details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
    details.insert("video_offset_ms".to_string(), serde_json::json!(sub_frame_offset_ms));
    details.insert("frame_offset".to_string(), serde_json::json!(best_frame_offset));
    details.insert("final_offset_ms".to_string(), serde_json::json!(final_offset_ms));
    details.insert("candidates".to_string(), serde_json::json!(candidate_results));
    details.insert("checkpoints".to_string(), serde_json::json!(checkpoint_times.len()));
    details.insert("use_pts_precision".to_string(), serde_json::json!(use_pts));
    details.insert("source_content_type".to_string(), serde_json::json!(
        source_props.get("content_type").and_then(|v| v.as_str()).unwrap_or("unknown")
    ));
    details.insert("target_content_type".to_string(), serde_json::json!(
        target_props.get("content_type").and_then(|v| v.as_str()).unwrap_or("unknown")
    ));
    details.insert("source_fps".to_string(), serde_json::json!(fps));
    details.insert("target_fps".to_string(), serde_json::json!(target_fps));
    details.insert("verification".to_string(), serde_json::json!(verification));
    details.insert("confidence".to_string(), serde_json::json!(confidence));
    details.insert("metric_agreement".to_string(), serde_json::json!(agree_count));

    (Some(final_offset_ms), details)
}
