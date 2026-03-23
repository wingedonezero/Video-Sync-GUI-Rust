//! VideoDiff — 1:1 port of `vsg_core/analysis/videodiff.py`.
//!
//! Visual frame matching to find timing offset between two video files.
//! Uses ffmpeg to extract frames, dhash for perceptual hashing,
//! and RANSAC regression for robust offset estimation.

use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::models::settings::AppSettings;

/// Result from native VideoDiff analysis — `VideoDiffResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoDiffResult {
    pub offset_ms: i32,
    pub raw_offset_ms: f64,
    pub matched_frames: usize,
    pub inlier_count: usize,
    pub inlier_ratio: f64,
    pub mean_residual_ms: f64,
    pub confidence: String,
    pub ref_frames_extracted: usize,
    pub target_frames_extracted: usize,
    pub speed_drift_detected: bool,
}

const FRAME_W: usize = 32;
const FRAME_H: usize = 32;

/// Probe native frame rate — `_probe_fps`
fn probe_fps(video_path: &str) -> f64 {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=r_frame_rate",
            "-of", "csv=p=0", video_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    match output {
        Ok(o) => {
            let rate_str = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if rate_str.contains('/') {
                let parts: Vec<&str> = rate_str.split('/').collect();
                if parts.len() == 2 {
                    let num: f64 = parts[0].parse().unwrap_or(23.976);
                    let den: f64 = parts[1].parse().unwrap_or(1.0);
                    if den != 0.0 { num / den } else { 23.976 }
                } else {
                    23.976
                }
            } else {
                rate_str.parse().unwrap_or(23.976)
            }
        }
        Err(_) => 23.976,
    }
}

/// Compute dhash of a grayscale frame — difference hash
fn dhash(pixels: &[u8], width: usize, height: usize) -> u64 {
    // Resize to 9x8 for 8x8 difference hash
    let hash_w = 9;
    let hash_h = 8;
    let mut resized = vec![0u8; hash_w * hash_h];

    for y in 0..hash_h {
        for x in 0..hash_w {
            let src_x = x * width / hash_w;
            let src_y = y * height / hash_h;
            resized[y * hash_w + x] = pixels[src_y * width + src_x];
        }
    }

    let mut hash: u64 = 0;
    for y in 0..hash_h {
        for x in 0..hash_w - 1 {
            if resized[y * hash_w + x] < resized[y * hash_w + x + 1] {
                hash |= 1 << (y * (hash_w - 1) + x);
            }
        }
    }
    hash
}

/// Hamming distance between two hashes
fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Extract frame hashes from video — `_extract_frame_hashes`
fn extract_frame_hashes(
    video_path: &str,
    sample_fps: f64,
    log: &dyn Fn(&str),
) -> (Vec<u64>, Vec<f64>) {
    let native_fps = probe_fps(video_path);
    let use_fps = if sample_fps > 0.0 { sample_fps } else { native_fps };

    let fps_str = format!("{use_fps}");
    let size_str = format!("{}x{}", FRAME_W, FRAME_H);

    let output = Command::new("ffmpeg")
        .args([
            "-nostdin", "-v", "error",
            "-i", video_path,
            "-vf", &format!("fps={fps_str},scale={size_str},format=gray"),
            "-f", "rawvideo", "-pix_fmt", "gray", "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            log(&format!("[VideoDiff] ffmpeg failed: {e}"));
            return (Vec::new(), Vec::new());
        }
    };

    let frame_size = FRAME_W * FRAME_H;
    let raw = &output.stdout;
    let n_frames = raw.len() / frame_size;

    let mut hashes = Vec::with_capacity(n_frames);
    let mut timestamps = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let frame = &raw[i * frame_size..(i + 1) * frame_size];
        hashes.push(dhash(frame, FRAME_W, FRAME_H));
        timestamps.push(i as f64 / use_fps * 1000.0); // ms
    }

    log(&format!(
        "[VideoDiff] Extracted {} frames from {}",
        n_frames,
        std::path::Path::new(video_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));

    (hashes, timestamps)
}

/// Run native VideoDiff analysis — `run_native_videodiff`
pub fn run_native_videodiff(
    ref_path: &str,
    target_path: &str,
    settings: &AppSettings,
    log: &dyn Fn(&str),
) -> Option<VideoDiffResult> {
    let sample_fps = settings.videodiff_sample_fps;
    let match_threshold = settings.videodiff_match_threshold;
    let min_matches = settings.videodiff_min_matches as usize;
    let inlier_threshold_ms = settings.videodiff_inlier_threshold_ms;

    log("[VideoDiff] Starting frame-based analysis...");

    // Extract frames
    let (ref_hashes, ref_times) = extract_frame_hashes(ref_path, sample_fps, log);
    let (tgt_hashes, tgt_times) = extract_frame_hashes(target_path, sample_fps, log);

    if ref_hashes.is_empty() || tgt_hashes.is_empty() {
        log("[VideoDiff] Failed to extract frames from one or both sources.");
        return None;
    }

    // Match frames by hamming distance
    let mut matches: Vec<(f64, f64)> = Vec::new(); // (ref_time, tgt_time)
    for (i, &ref_hash) in ref_hashes.iter().enumerate() {
        let mut best_dist = u32::MAX;
        let mut best_j = 0usize;
        for (j, &tgt_hash) in tgt_hashes.iter().enumerate() {
            let dist = hamming_distance(ref_hash, tgt_hash);
            if dist < best_dist {
                best_dist = dist;
                best_j = j;
            }
        }
        if best_dist <= match_threshold as u32 {
            matches.push((ref_times[i], tgt_times[best_j]));
        }
    }

    log(&format!("[VideoDiff] Found {} frame matches", matches.len()));

    if matches.len() < min_matches {
        log(&format!(
            "[VideoDiff] Insufficient matches ({} < {min_matches})",
            matches.len()
        ));
        return None;
    }

    // RANSAC: find offset with most inliers
    let offsets: Vec<f64> = matches.iter().map(|(r, t)| t - r).collect();
    let mut best_inlier_count = 0usize;
    let mut best_inliers: Vec<bool> = vec![false; offsets.len()];

    for &candidate in &offsets {
        let inliers: Vec<bool> = offsets
            .iter()
            .map(|&o| (o - candidate).abs() <= inlier_threshold_ms)
            .collect();
        let count = inliers.iter().filter(|&&b| b).count();
        if count > best_inlier_count {
            best_inlier_count = count;
            best_inliers = inliers;
        }
    }

    // Refine offset using inlier mean
    let inlier_offsets: Vec<f64> = offsets
        .iter()
        .zip(&best_inliers)
        .filter(|(_, &is_inlier)| is_inlier)
        .map(|(&o, _)| o)
        .collect();
    let raw_offset = inlier_offsets.iter().sum::<f64>() / inlier_offsets.len() as f64;
    let rounded_offset = raw_offset.round() as i32;

    // Mean residual
    let mean_residual = inlier_offsets
        .iter()
        .map(|&o| (o - raw_offset).abs())
        .sum::<f64>()
        / inlier_offsets.len() as f64;

    let inlier_ratio = best_inlier_count as f64 / matches.len() as f64;

    // Confidence
    let confidence = if inlier_ratio > 0.8 && mean_residual < 50.0 {
        "HIGH"
    } else if inlier_ratio > 0.5 && mean_residual < 100.0 {
        "MEDIUM"
    } else {
        "LOW"
    };

    // Speed drift detection (simple check)
    let speed_drift = false; // TODO: implement residual correlation check

    let result = VideoDiffResult {
        offset_ms: rounded_offset,
        raw_offset_ms: raw_offset,
        matched_frames: matches.len(),
        inlier_count: best_inlier_count,
        inlier_ratio,
        mean_residual_ms: mean_residual,
        confidence: confidence.to_string(),
        ref_frames_extracted: ref_hashes.len(),
        target_frames_extracted: tgt_hashes.len(),
        speed_drift_detected: speed_drift,
    };

    log(&format!("\n{}", "=".repeat(60)));
    log("[VideoDiff] RESULTS");
    log(&"=".repeat(60));
    log(&format!("  Offset: {rounded_offset}ms (raw: {raw_offset:.3}ms)"));
    log(&format!(
        "  Matched frames: {} / {}",
        matches.len(),
        ref_hashes.len().min(tgt_hashes.len())
    ));
    log(&format!(
        "  Inliers: {best_inlier_count} / {} ({:.1}%)",
        matches.len(),
        inlier_ratio * 100.0
    ));
    log(&format!("  Mean residual: {mean_residual:.1}ms"));
    log(&format!("  Confidence: {confidence}"));

    Some(result)
}

/// Convenience alias — `run_videodiff`
pub fn run_videodiff(
    ref_path: &str,
    target_path: &str,
    settings: &AppSettings,
    log: &dyn Fn(&str),
) -> Option<VideoDiffResult> {
    run_native_videodiff(ref_path, target_path, settings, log)
}
