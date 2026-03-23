//! Neural feature sequence sliding for video-verified sync.
//!
//! Uses ISC (Image Similarity Challenge) features to find the correct
//! frame offset between two video sources by sliding a sequence of
//! feature vectors from one source across the other.
//!
//! 1:1 port of `video_verified/neural_matcher.py`.
//! Uses tch-rs for PyTorch operations instead of Python's torch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;
use crate::subtitles::frame_utils::video_reader::VideoReader;

use super::isc_model::{create_isc_model, preprocess_for_isc, IscModel};

/// Calculate video-verified offset using ISC neural feature sequence sliding.
///
/// Same interface as calculate_video_verified_offset() so they can be swapped.
pub fn calculate_neural_verified_offset(
    source_video: &str,
    target_video: &str,
    total_delay_ms: f64,
    global_shift_ms: f64,
    settings: Option<&AppSettings>,
    runner: &CommandRunner,
    temp_dir: Option<PathBuf>,
    video_duration_ms: Option<f64>,
    debug_output_dir: Option<PathBuf>,
    source_key: &str,
) -> (Option<f64>, HashMap<String, serde_json::Value>) {
    let default_settings = AppSettings::default();
    let settings = settings.unwrap_or(&default_settings);

    let log = |msg: &str| {
        runner.log_message(msg);
    };

    let pure_correlation_ms = total_delay_ms - global_shift_ms;

    log("[NeuralVerified] === Neural Feature Matching ===");
    log(&format!(
        "[NeuralVerified] Source: {}",
        Path::new(source_video).file_name().unwrap_or_default().to_string_lossy()
    ));
    log(&format!(
        "[NeuralVerified] Target: {}",
        Path::new(target_video).file_name().unwrap_or_default().to_string_lossy()
    ));
    log(&format!(
        "[NeuralVerified] Pure correlation (audio): {:+.3}ms",
        pure_correlation_ms
    ));

    // Get neural-specific settings
    let window_sec = settings.neural_window_seconds as f64;
    let slide_range_sec = settings.neural_slide_range_seconds as f64;
    let num_positions = settings.neural_num_positions as usize;
    let batch_size = settings.neural_batch_size as usize;

    log(&format!("[NeuralVerified] Model: ISC ft_v107 (256-dim)"));
    log(&format!(
        "[NeuralVerified] Window: {}s, Slide: +/-{}s, Positions: {}, Batch: {}",
        window_sec, slide_range_sec, num_positions, batch_size
    ));

    // Check tch-rs availability
    if !tch::Cuda::is_available() {
        log("[NeuralVerified] CUDA not available, will use CPU (slower)");
    }

    let device = if tch::Cuda::is_available() {
        tch::Device::Cuda(0)
    } else {
        tch::Device::Cpu
    };

    // Open video readers
    let src_reader = VideoReader::new(source_video, runner, temp_dir.clone());
    let tgt_reader = VideoReader::new(target_video, runner, temp_dir.clone());

    let src_fps = src_reader.fps.unwrap_or(23.976);
    let tgt_fps = tgt_reader.fps.unwrap_or(23.976);
    let src_frame_dur_ms = 1000.0 / src_fps;
    let src_total_frames = src_reader.get_frame_count();
    let tgt_total_frames = tgt_reader.get_frame_count();

    log(&format!(
        "[NeuralVerified] Source: {}f @ {:.3}fps",
        src_total_frames, src_fps
    ));
    log(&format!(
        "[NeuralVerified] Target: {}f @ {:.3}fps",
        tgt_total_frames, tgt_fps
    ));

    // Check FPS compatibility
    let fps_ratio = src_fps.max(tgt_fps) / src_fps.min(tgt_fps);
    if fps_ratio > 1.01 {
        log(&format!(
            "[NeuralVerified] WARNING: FPS mismatch ({:.3} vs {:.3})",
            src_fps, tgt_fps
        ));
        log("[NeuralVerified] Cross-fps matching not yet supported");
        log("[NeuralVerified] Falling back to audio correlation");

        let mut details = HashMap::new();
        details.insert("reason".to_string(), serde_json::json!("fallback-cross-fps"));
        details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("video_offset_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("final_offset_ms".to_string(), serde_json::json!(total_delay_ms));
        return (Some(total_delay_ms), details);
    }

    // Determine duration
    let src_dur_ms = video_duration_ms.unwrap_or_else(|| {
        src_total_frames as f64 / src_fps * 1000.0
    });

    // Load ISC model
    let t_model_start = Instant::now();
    let model = match create_isc_model(
        if tch::Cuda::is_available() { "cuda" } else { "cpu" },
        None,
        Some(&|msg: &str| runner.log_message(msg)),
    ) {
        Ok(m) => m,
        Err(e) => {
            log(&format!("[NeuralVerified] Failed to load ISC model: {}", e));
            let mut details = HashMap::new();
            details.insert("reason".to_string(), serde_json::json!("fallback-model-failed"));
            details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
            details.insert("video_offset_ms".to_string(), serde_json::json!(pure_correlation_ms));
            details.insert("final_offset_ms".to_string(), serde_json::json!(total_delay_ms));
            details.insert("error".to_string(), serde_json::json!(e.to_string()));
            return (Some(total_delay_ms), details);
        }
    };
    let t_model = t_model_start.elapsed();
    log(&format!(
        "[NeuralVerified] Model load time: {:.1}s",
        t_model.as_secs_f64()
    ));

    // Calculate frame counts for window and slide
    let src_n_frames = (window_sec * src_fps) as i64;
    let slide_pad = (slide_range_sec * tgt_fps) as i64;

    log(&format!(
        "[NeuralVerified] Source window: {} frames ({}s)",
        src_n_frames, window_sec
    ));
    log(&format!(
        "[NeuralVerified] Slide range: +/-{} frames (+/-{}s)",
        slide_pad, slide_range_sec
    ));

    // Select test positions (evenly across 10%-90%)
    let positions_pct: Vec<f64> = (0..num_positions)
        .map(|i| 10.0 + 80.0 * (i as f64 + 0.5) / num_positions as f64)
        .collect();

    log("[NeuralVerified] -----------------------------------------");
    log(&format!(
        "[NeuralVerified] Testing {} positions",
        num_positions
    ));

    // Run sliding at each position
    let mut results: Vec<HashMap<String, serde_json::Value>> = Vec::new();
    let t_total_start = Instant::now();

    for (i, &pct) in positions_pct.iter().enumerate() {
        let t_pos_start = Instant::now();

        // Source frame range
        let src_start = (src_total_frames as f64 * pct / 100.0) as i64;
        let src_end = (src_start + src_n_frames).min(src_total_frames);
        let src_frames: Vec<i64> = (src_start..src_end).collect();

        // Target frame range (padded for sliding)
        let tgt_center = src_start;
        let tgt_window_start = (tgt_center - slide_pad).max(0);
        let tgt_window_end = (tgt_center + src_n_frames + slide_pad).min(tgt_total_frames);
        let tgt_frames: Vec<i64> = (tgt_window_start..tgt_window_end).collect();

        if tgt_frames.len() <= src_frames.len() {
            log(&format!(
                "[NeuralVerified]   [{}/{}] {:.0}% -- SKIPPED (edge)",
                i + 1, num_positions, pct
            ));
            continue;
        }

        // Extract features
        let src_feats = extract_features_batch(
            &src_reader, &src_frames, &model, device, batch_size,
        );
        let tgt_feats = extract_features_batch(
            &tgt_reader, &tgt_frames, &model, device, batch_size,
        );

        // Slide and score
        let (scores, match_counts) = slide_and_score(&src_feats, &tgt_feats);

        if scores.is_empty() {
            continue;
        }

        let best_pos = scores
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        let offset_frames = (tgt_window_start + best_pos as i64) - src_start;
        let offset_ms = offset_frames as f64 * src_frame_dur_ms;
        let gradient = compute_gradient(&scores, best_pos);
        let dt = t_pos_start.elapsed().as_secs_f64();

        let mut result = HashMap::new();
        result.insert("position_pct".to_string(), serde_json::json!(pct));
        result.insert("src_start".to_string(), serde_json::json!(src_start));
        result.insert("offset_frames".to_string(), serde_json::json!(offset_frames));
        result.insert("offset_ms".to_string(), serde_json::json!(offset_ms));
        result.insert("score".to_string(), serde_json::json!(scores[best_pos]));
        result.insert("matches".to_string(), serde_json::json!(match_counts[best_pos]));
        result.insert("total".to_string(), serde_json::json!(src_frames.len()));
        result.insert("gradient".to_string(), serde_json::json!(gradient));
        result.insert("time_s".to_string(), serde_json::json!(dt));

        log(&format!(
            "[NeuralVerified]   [{}/{}] {:.0}% @{}f -> offset={:+}f ({:+.1}ms) score={:.4} match={}/{} ({:.1}s)",
            i + 1, num_positions, pct, src_start, offset_frames, offset_ms,
            scores[best_pos], match_counts[best_pos], src_frames.len(), dt
        ));

        results.push(result);
    }

    let dt_total = t_total_start.elapsed().as_secs_f64();

    if results.is_empty() {
        log("[NeuralVerified] No valid positions - falling back to audio correlation");
        let mut details = HashMap::new();
        details.insert("reason".to_string(), serde_json::json!("fallback-no-valid-positions"));
        details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("video_offset_ms".to_string(), serde_json::json!(pure_correlation_ms));
        details.insert("final_offset_ms".to_string(), serde_json::json!(total_delay_ms));
        return (Some(total_delay_ms), details);
    }

    // Consensus
    let offsets_f: Vec<i64> = results
        .iter()
        .filter_map(|r| r.get("offset_frames").and_then(|v| v.as_i64()))
        .collect();
    let scores_list: Vec<f64> = results
        .iter()
        .filter_map(|r| r.get("score").and_then(|v| v.as_f64()))
        .collect();

    // Find most common offset (consensus)
    let mut offset_counts: HashMap<i64, usize> = HashMap::new();
    for &off in &offsets_f {
        *offset_counts.entry(off).or_insert(0) += 1;
    }
    let (&consensus_frames, &consensus_count) = offset_counts
        .iter()
        .max_by_key(|(_, &count)| count)
        .unwrap_or((&0, &0));

    let consensus_ms = consensus_frames as f64 * src_frame_dur_ms;

    // Confidence assessment
    let consensus_ratio = consensus_count as f64 / results.len() as f64;
    let mean_score: f64 = scores_list.iter().sum::<f64>() / scores_list.len() as f64;

    let confidence = if consensus_ratio >= 0.9 && mean_score >= 0.98 {
        "HIGH"
    } else if consensus_ratio >= 0.7 && mean_score >= 0.95 {
        "MEDIUM"
    } else {
        "LOW"
    };

    log("[NeuralVerified] =======================================");
    log("[NeuralVerified] RESULTS SUMMARY");
    log("[NeuralVerified] =======================================");
    log(&format!(
        "[NeuralVerified] Consensus: {:+}f = {:+.1}ms ({}/{} positions)",
        consensus_frames, consensus_ms, consensus_count, results.len()
    ));
    log(&format!(
        "[NeuralVerified] Mean score: {:.4}, Confidence: {}",
        mean_score, confidence
    ));
    log(&format!(
        "[NeuralVerified] Total time: {:.1}s ({:.1}s/position)",
        dt_total,
        dt_total / results.len() as f64
    ));

    // Calculate final offset
    let video_offset_ms = consensus_ms;
    let final_offset_ms = video_offset_ms + global_shift_ms;

    log(&format!(
        "[NeuralVerified] Video-verified offset: {:+.3}ms (neural)",
        video_offset_ms
    ));
    log(&format!(
        "[NeuralVerified] + Global shift: {:+.3}ms",
        global_shift_ms
    ));
    log(&format!(
        "[NeuralVerified] = Final offset: {:+.3}ms",
        final_offset_ms
    ));
    log("[NeuralVerified] =======================================");

    let mut details = HashMap::new();
    details.insert("reason".to_string(), serde_json::json!("neural-matched"));
    details.insert("audio_correlation_ms".to_string(), serde_json::json!(pure_correlation_ms));
    details.insert("video_offset_ms".to_string(), serde_json::json!(video_offset_ms));
    details.insert("frame_offset".to_string(), serde_json::json!(consensus_frames));
    details.insert("final_offset_ms".to_string(), serde_json::json!(final_offset_ms));
    details.insert("confidence".to_string(), serde_json::json!(confidence));
    details.insert("consensus_count".to_string(), serde_json::json!(consensus_count));
    details.insert("num_positions".to_string(), serde_json::json!(results.len()));
    details.insert("mean_score".to_string(), serde_json::json!(mean_score));
    details.insert("source_fps".to_string(), serde_json::json!(src_fps));
    details.insert("target_fps".to_string(), serde_json::json!(tgt_fps));
    details.insert("total_time_s".to_string(), serde_json::json!(dt_total));
    details.insert("per_position_results".to_string(), serde_json::json!(results));

    (Some(final_offset_ms), details)
}

// ---- Internal helpers ----

/// Extract ISC features for a list of frame numbers using batch processing.
fn extract_features_batch(
    reader: &VideoReader,
    frame_nums: &[i64],
    model: &IscModel,
    device: tch::Device,
    batch_size: usize,
) -> Vec<Vec<f32>> {
    let mut all_feats: Vec<Vec<f32>> = Vec::new();
    let mut batch_tensors: Vec<tch::Tensor> = Vec::new();

    let _guard = tch::no_grad_guard();

    for (i, &fn_num) in frame_nums.iter().enumerate() {
        if let Some(frame) = reader.get_frame_at_index(fn_num) {
            // Convert grayscale frame to RGB tensor [1, 3, H, W] in [0, 1]
            let gray_data: Vec<f32> = frame.data.iter().map(|&p| p as f32 / 255.0).collect();
            let h = frame.height as i64;
            let w = frame.width as i64;

            // Replicate grayscale to 3 channels
            let gray_tensor = tch::Tensor::from_slice(&gray_data).reshape(&[1, 1, h, w]);
            let rgb_tensor = gray_tensor.repeat(&[1, 3, 1, 1]).to_device(device);

            // Preprocess for ISC (resize to 512x512, normalize)
            let preprocessed = preprocess_for_isc(&rgb_tensor, device);
            batch_tensors.push(preprocessed.squeeze_dim(0));
        }

        let is_last = i == frame_nums.len() - 1;
        if batch_tensors.len() == batch_size || (is_last && !batch_tensors.is_empty()) {
            let batch = tch::Tensor::stack(&batch_tensors, 0).to_device(device);
            let feats = model.forward(&batch);
            let feats_cpu = feats.to_device(tch::Device::Cpu);

            // Extract features as Vec<Vec<f32>>
            let batch_count = feats_cpu.size()[0] as usize;
            let feat_dim = feats_cpu.size()[1] as usize;
            for b in 0..batch_count {
                let feat_tensor = feats_cpu.get(b as i64);
                let feat_vec: Vec<f32> = Vec::<f32>::try_from(feat_tensor).unwrap_or_default();
                all_feats.push(feat_vec);
            }

            batch_tensors.clear();
        }
    }

    all_feats
}

/// Slide source features across target and compute cosine similarity.
///
/// Returns (scores, match_counts).
fn slide_and_score(
    src_feats: &[Vec<f32>],
    tgt_feats: &[Vec<f32>],
) -> (Vec<f64>, Vec<i64>) {
    let s = src_feats.len();
    let t = tgt_feats.len();
    if t <= s {
        return (Vec::new(), Vec::new());
    }
    let max_slides = t - s + 1;

    // L2 normalize
    let normalize = |v: &[f32]| -> Vec<f32> {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt() + 1e-8;
        v.iter().map(|x| x / norm).collect()
    };

    let src_norm: Vec<Vec<f32>> = src_feats.iter().map(|f| normalize(f)).collect();
    let tgt_norm: Vec<Vec<f32>> = tgt_feats.iter().map(|f| normalize(f)).collect();

    let mut scores = vec![0.0f64; max_slides];
    let mut match_counts = vec![0i64; max_slides];

    for p in 0..max_slides {
        let mut sum_sim = 0.0f64;
        let mut matches = 0i64;

        for i in 0..s {
            let sim: f64 = src_norm[i]
                .iter()
                .zip(tgt_norm[p + i].iter())
                .map(|(&a, &b)| a as f64 * b as f64)
                .sum();
            sum_sim += sim;
            if sim > 0.5 {
                matches += 1;
            }
        }

        scores[p] = sum_sim / s as f64;
        match_counts[p] = matches;
    }

    (scores, match_counts)
}

/// Compute average score drop-off per frame from peak.
fn compute_gradient(scores: &[f64], best_pos: usize) -> f64 {
    if scores.len() < 3 {
        return 0.0;
    }

    let peak_score = scores[best_pos];
    let mut gradients: Vec<f64> = Vec::new();

    for delta in 1..=5 {
        for &sign in &[-1i64, 1] {
            let pos = best_pos as i64 + sign * delta as i64;
            if pos >= 0 && (pos as usize) < scores.len() {
                let drop = peak_score - scores[pos as usize];
                gradients.push(drop / delta as f64);
            }
        }
    }

    if gradients.is_empty() {
        0.0
    } else {
        gradients.iter().sum::<f64>() / gradients.len() as f64
    }
}
