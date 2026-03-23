//! Visual frame verification for video-verified sync.
//!
//! Samples frames at regular intervals across the entire video and compares
//! raw Y-plane content between source and target using global SSIM.
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/visual_verify.py`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use super::video_reader::VideoReader;

// ============================================================================
// Data Model
// ============================================================================

/// Result of comparing one sample point across source and target.
#[derive(Debug, Clone)]
pub struct SampleResult {
    pub sample_index: usize,
    pub time_s: f64,
    pub source_frame: i64,
    pub target_frame: i64,
    pub base_dist: f64,
    pub best_delta: i64,
    pub best_dist: f64,
    pub classification: String,
    pub is_static: bool,
    pub region: String,
}

/// Aggregated statistics for one region of the video.
#[derive(Debug, Clone)]
pub struct RegionStats {
    pub name: String,
    pub total: i64,
    pub exact: i64,
    pub within_1: i64,
    pub within_2: i64,
    pub unmatchable: i64,
    pub static_frames: i64,
    pub mean_base_dist: f64,
    pub mean_best_dist: f64,
}

/// Credits region detection result.
#[derive(Debug, Clone, Default)]
pub struct CreditsInfo {
    pub detected: bool,
    pub boundary_time_s: Option<f64>,
    pub boundary_sample: Option<usize>,
    pub num_credits_samples: i64,
}

/// Complete visual verification result for a sync job.
#[derive(Debug, Clone)]
pub struct VisualVerifyResult {
    pub job_name: String,
    pub source_path: String,
    pub target_path: String,
    pub offset_ms: f64,
    pub frame_offset: i64,
    pub source_fps: f64,
    pub target_fps: f64,
    pub sample_interval_s: f64,
    pub search_range: i64,
    pub total_samples: usize,
    pub total_duration_s: f64,
    pub verify_timestamp: DateTime<Local>,
    pub source_content_type: String,
    pub target_content_type: String,

    // Global stats (main content only, excluding credits)
    pub main_exact: i64,
    pub main_within_1: i64,
    pub main_within_2: i64,
    pub main_unmatchable: i64,
    pub main_total: i64,
    pub main_static: i64,

    pub samples: Vec<SampleResult>,
    pub regions: Vec<RegionStats>,
    pub credits: CreditsInfo,
}

impl VisualVerifyResult {
    /// Percentage of main-content samples within +/-2 frames.
    pub fn accuracy_pct(&self) -> f64 {
        if self.main_total == 0 {
            return 0.0;
        }
        100.0 * self.main_within_2 as f64 / self.main_total as f64
    }

    /// True if any main-content samples are unmatchable.
    pub fn has_real_drift(&self) -> bool {
        self.main_unmatchable > 0
    }
}

// ============================================================================
// Internal Helpers - Frame Comparison
// ============================================================================

/// Compute global SSIM distance between two grayscale pixel arrays.
///
/// Returns distance in range [0, 100]: 0.0 = identical, >50.0 = different content.
fn global_ssim_dist(y1: &[u8], y2: &[u8]) -> f64 {
    let n = y1.len().min(y2.len()) as f64;
    if n == 0.0 {
        return 100.0;
    }

    let c1 = (0.01 * 255.0_f64).powi(2);
    let c2 = (0.03 * 255.0_f64).powi(2);

    let mu1: f64 = y1.iter().map(|&p| p as f64).sum::<f64>() / n;
    let mu2: f64 = y2.iter().map(|&p| p as f64).sum::<f64>() / n;
    let s1: f64 = y1.iter().map(|&p| (p as f64 - mu1).powi(2)).sum::<f64>() / n;
    let s2: f64 = y2.iter().map(|&p| (p as f64 - mu2).powi(2)).sum::<f64>() / n;
    let s12: f64 = y1
        .iter()
        .zip(y2.iter())
        .map(|(&p1, &p2)| (p1 as f64 - mu1) * (p2 as f64 - mu2))
        .sum::<f64>()
        / n;

    let ssim = ((2.0 * mu1 * mu2 + c1) * (2.0 * s12 + c2))
        / ((mu1.powi(2) + mu2.powi(2) + c1) * (s1 + s2 + c2));
    (1.0 - ssim) * 100.0
}

// ============================================================================
// Per-Sample Verification
// ============================================================================

/// Verify a single sample point.
fn verify_sample(
    src_reader: &VideoReader,
    tgt_reader: &VideoReader,
    time_s: f64,
    offset_ms: f64,
    search_range: i64,
    sample_index: usize,
) -> Option<SampleResult> {
    let src_fps = src_reader.fps.unwrap_or(29.970);
    let tgt_fps = tgt_reader.fps.unwrap_or(29.970);

    // Source frame at time T
    let src_frame = (time_s * src_fps) as i64;

    // Target frame at time T + offset_ms/1000
    let target_time_s = time_s + (offset_ms / 1000.0);
    let tgt_frame = (target_time_s * tgt_fps) as i64;

    // Extract frames
    let src_img = src_reader.get_frame_at_index(src_frame)?;
    let tgt_img = tgt_reader.get_frame_at_index(tgt_frame)?;

    // Base comparison at expected offset
    let base_dist = global_ssim_dist(&src_img.data, &tgt_img.data);

    // Search +/-N frames for best match
    let mut best_delta: i64 = 0;
    let mut best_dist = base_dist;

    for delta in -search_range..=search_range {
        if delta == 0 {
            continue;
        }
        let test_frame = tgt_frame + delta;
        if test_frame < 0 {
            continue;
        }
        if let Some(tgt_delta) = tgt_reader.get_frame_at_index(test_frame) {
            let dist = global_ssim_dist(&src_img.data, &tgt_delta.data);
            if dist < best_dist {
                best_dist = dist;
                best_delta = delta;
            }
        }
    }

    // Classify
    let classification = if best_dist >= 50.0 {
        "unmatchable".to_string()
    } else if best_delta == 0 && base_dist < 2.0 {
        "exact".to_string()
    } else if best_dist < 50.0 && best_delta != 0 {
        format!("off_by_{}", best_delta.abs())
    } else {
        "exact".to_string()
    };

    let is_static = (base_dist - best_dist).abs() < 1.0;

    Some(SampleResult {
        sample_index,
        time_s,
        source_frame: src_frame,
        target_frame: tgt_frame,
        base_dist,
        best_delta,
        best_dist,
        classification,
        is_static,
        region: String::new(),
    })
}

// ============================================================================
// Credits Detection & Region Assignment
// ============================================================================

fn detect_credits_region(samples: &[SampleResult]) -> CreditsInfo {
    if samples.len() < 5 {
        return CreditsInfo::default();
    }

    let mut consecutive_bad = 0;
    let mut boundary_idx = None;

    for i in (0..samples.len()).rev() {
        if samples[i].best_dist > 25.0 {
            consecutive_bad += 1;
            boundary_idx = Some(i);
        } else {
            break;
        }
    }

    if consecutive_bad >= 3 {
        let idx = boundary_idx.unwrap();
        CreditsInfo {
            detected: true,
            boundary_time_s: Some(samples[idx].time_s),
            boundary_sample: Some(idx),
            num_credits_samples: consecutive_bad,
        }
    } else {
        CreditsInfo::default()
    }
}

fn assign_regions(
    samples: &mut [SampleResult],
    duration_s: f64,
    credits: &CreditsInfo,
) {
    let early_boundary = 300.0; // 5 minutes

    let content_end = if credits.detected {
        credits.boundary_time_s.unwrap_or(duration_s)
    } else {
        duration_s
    };

    let late_boundary = content_end * 0.9;

    for s in samples.iter_mut() {
        if credits.detected {
            if let Some(boundary_sample) = credits.boundary_sample {
                if s.sample_index >= boundary_sample {
                    s.region = "credits".to_string();
                    continue;
                }
            }
        }

        if s.time_s <= early_boundary {
            s.region = "early".to_string();
        } else if s.time_s >= late_boundary {
            s.region = "late".to_string();
        } else {
            s.region = "main".to_string();
        }
    }
}

fn compute_region_stats(samples: &[SampleResult]) -> Vec<RegionStats> {
    let region_order = ["early", "main", "late", "credits"];
    let mut stats = Vec::new();

    for &name in &region_order {
        let region_list: Vec<&SampleResult> =
            samples.iter().filter(|s| s.region == name).collect();
        if region_list.is_empty() {
            continue;
        }

        let total = region_list.len() as i64;

        let eff_delta = |s: &&SampleResult| -> i64 {
            if s.is_static { 0 } else { s.best_delta }
        };

        let exact = region_list
            .iter()
            .filter(|s| eff_delta(s) == 0 && s.best_dist < 50.0)
            .count() as i64;
        let within_1 = region_list
            .iter()
            .filter(|s| eff_delta(s).abs() <= 1 && s.best_dist < 50.0)
            .count() as i64;
        let within_2 = region_list
            .iter()
            .filter(|s| eff_delta(s).abs() <= 2 && s.best_dist < 50.0)
            .count() as i64;
        let unmatchable = region_list
            .iter()
            .filter(|s| s.classification == "unmatchable")
            .count() as i64;
        let static_frames = region_list.iter().filter(|s| s.is_static).count() as i64;

        let mean_base_dist = if total > 0 {
            region_list.iter().map(|s| s.base_dist).sum::<f64>() / total as f64
        } else {
            0.0
        };
        let mean_best_dist = if total > 0 {
            region_list.iter().map(|s| s.best_dist).sum::<f64>() / total as f64
        } else {
            0.0
        };

        stats.push(RegionStats {
            name: name.to_string(),
            total,
            exact,
            within_1,
            within_2,
            unmatchable,
            static_frames,
            mean_base_dist,
            mean_best_dist,
        });
    }

    stats
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Run visual frame verification across the entire video.
pub fn run_visual_verify(
    source_video: &str,
    target_video: &str,
    offset_ms: f64,
    frame_offset: i64,
    source_fps: f64,
    target_fps: f64,
    job_name: &str,
    sample_interval_s: f64,
    search_range: i64,
    temp_dir: Option<PathBuf>,
    source_content_type: &str,
    target_content_type: &str,
    log: Option<&dyn Fn(&str)>,
) -> VisualVerifyResult {
    let log_fn = |msg: &str| {
        if let Some(f) = log {
            f(msg);
        }
    };

    log_fn("[VisualVerify] Starting visual frame verification...");
    log_fn(&format!(
        "[VisualVerify] Source: {}",
        Path::new(source_video).file_name().unwrap_or_default().to_string_lossy()
    ));
    log_fn(&format!(
        "[VisualVerify] Target: {}",
        Path::new(target_video).file_name().unwrap_or_default().to_string_lossy()
    ));
    log_fn(&format!(
        "[VisualVerify] Offset: {:+.3}ms (frame_offset: {})",
        offset_ms, frame_offset
    ));

    let empty_result = || VisualVerifyResult {
        job_name: job_name.to_string(),
        source_path: source_video.to_string(),
        target_path: target_video.to_string(),
        offset_ms,
        frame_offset,
        source_fps,
        target_fps,
        sample_interval_s,
        search_range,
        total_samples: 0,
        total_duration_s: 0.0,
        verify_timestamp: Local::now(),
        source_content_type: source_content_type.to_string(),
        target_content_type: target_content_type.to_string(),
        main_exact: 0,
        main_within_1: 0,
        main_within_2: 0,
        main_unmatchable: 0,
        main_total: 0,
        main_static: 0,
        samples: Vec::new(),
        regions: Vec::new(),
        credits: CreditsInfo::default(),
    };

    // Create a dummy runner for VideoReader construction
    let settings = crate::models::settings::AppSettings::default();
    let runner = crate::io::runner::CommandRunner::new(settings, Box::new(|_: &str| {}));

    // Open both clips
    let src_reader = VideoReader::new(source_video, &runner, temp_dir.clone());
    let tgt_reader = VideoReader::new(target_video, &runner, temp_dir);

    let src_fps_detected = src_reader.fps.unwrap_or(29.970);
    let tgt_fps_detected = tgt_reader.fps.unwrap_or(29.970);
    let src_frame_count = src_reader.get_frame_count();
    let tgt_frame_count = tgt_reader.get_frame_count();

    let src_duration_s = if src_fps_detected > 0.0 {
        src_frame_count as f64 / src_fps_detected
    } else {
        0.0
    };
    let tgt_duration_s = if tgt_fps_detected > 0.0 {
        tgt_frame_count as f64 / tgt_fps_detected
    } else {
        0.0
    };
    let duration_s = src_duration_s.min(tgt_duration_s);

    if duration_s <= 0.0 {
        log_fn("[VisualVerify] ERROR: Could not determine video duration");
        return empty_result();
    }

    log_fn(&format!(
        "[VisualVerify] Duration: {:.1}s ({:.1} min)",
        duration_s,
        duration_s / 60.0
    ));

    // Generate sample times
    let start_time = 2.0;
    let mut sample_times: Vec<f64> = Vec::new();
    let mut t = start_time;
    while t < duration_s {
        let target_time = t - (offset_ms / 1000.0);
        if target_time >= 0.0 && target_time < tgt_duration_s {
            sample_times.push(t);
        }
        t += sample_interval_s;
    }

    log_fn(&format!(
        "[VisualVerify] Samples to check: {}",
        sample_times.len()
    ));

    // Verify each sample
    let mut samples: Vec<SampleResult> = Vec::new();
    for (i, &time_s) in sample_times.iter().enumerate() {
        if let Some(result) = verify_sample(
            &src_reader,
            &tgt_reader,
            time_s,
            offset_ms,
            search_range,
            i,
        ) {
            samples.push(result);
        }

        if (i + 1) % 50 == 0 {
            log_fn(&format!(
                "[VisualVerify] Progress: {}/{}",
                i + 1,
                sample_times.len()
            ));
        }
    }

    log_fn(&format!(
        "[VisualVerify] Completed: {} samples verified",
        samples.len()
    ));

    // Detect credits region
    let credits = detect_credits_region(&samples);

    // Assign region labels
    assign_regions(&mut samples, duration_s, &credits);

    // Compute per-region stats
    let regions = compute_region_stats(&samples);

    // Compute main-content stats
    let main_samples: Vec<&SampleResult> =
        samples.iter().filter(|s| s.region != "credits").collect();
    let main_total = main_samples.len() as i64;

    let eff_delta = |s: &&SampleResult| -> i64 {
        if s.is_static { 0 } else { s.best_delta }
    };

    let main_exact = main_samples
        .iter()
        .filter(|s| eff_delta(s) == 0 && s.best_dist < 50.0)
        .count() as i64;
    let main_within_1 = main_samples
        .iter()
        .filter(|s| eff_delta(s).abs() <= 1 && s.best_dist < 50.0)
        .count() as i64;
    let main_within_2 = main_samples
        .iter()
        .filter(|s| eff_delta(s).abs() <= 2 && s.best_dist < 50.0)
        .count() as i64;
    let main_unmatchable = main_samples
        .iter()
        .filter(|s| s.classification == "unmatchable")
        .count() as i64;
    let main_static = main_samples.iter().filter(|s| s.is_static).count() as i64;

    let result = VisualVerifyResult {
        job_name: job_name.to_string(),
        source_path: source_video.to_string(),
        target_path: target_video.to_string(),
        offset_ms,
        frame_offset,
        source_fps,
        target_fps,
        sample_interval_s,
        search_range,
        total_samples: samples.len(),
        total_duration_s: duration_s,
        verify_timestamp: Local::now(),
        source_content_type: source_content_type.to_string(),
        target_content_type: target_content_type.to_string(),
        main_exact,
        main_within_1,
        main_within_2,
        main_unmatchable,
        main_total,
        main_static,
        samples,
        regions,
        credits,
    };

    log_fn(&format!(
        "[VisualVerify] Main content accuracy (+/-2): {:.1}% ({}/{})",
        result.accuracy_pct(),
        main_within_2,
        main_total
    ));

    result
}

// ============================================================================
// Report Writing
// ============================================================================

fn format_time(seconds: f64) -> String {
    if seconds < 0.0 {
        return format!("-{}", format_time(-seconds));
    }
    let h = (seconds / 3600.0) as i64;
    let m = ((seconds % 3600.0) / 60.0) as i64;
    let s = seconds % 60.0;
    if h > 0 {
        format!("{}:{:02}:{:04.1}", h, m, s)
    } else {
        format!("{}:{:04.1}", m, s)
    }
}

/// Write a visual verification report to a text file.
pub fn write_visual_verify_report(
    result: &VisualVerifyResult,
    output_dir: &Path,
    log: Option<&dyn Fn(&str)>,
) -> PathBuf {
    let _ = std::fs::create_dir_all(output_dir);

    let timestamp_str = result.verify_timestamp.format("%Y%m%d_%H%M%S").to_string();
    let safe_job: String = result
        .job_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = format!("{}_{}_visual_verify.txt", safe_job, timestamp_str);
    let output_path = output_dir.join(&filename);

    let mut lines: Vec<String> = Vec::new();

    // Header
    lines.push("=".repeat(70));
    lines.push("VISUAL FRAME VERIFICATION REPORT".to_string());
    lines.push("=".repeat(70));
    lines.push(String::new());
    lines.push(format!("Job: {}", result.job_name));
    lines.push(format!(
        "Timestamp: {}",
        result.verify_timestamp.format("%Y-%m-%d %H:%M:%S")
    ));
    lines.push(format!("Source: {}", result.source_path));
    lines.push(format!("Target: {}", result.target_path));
    lines.push(format!(
        "Offset applied: {:+.3}ms (frame_offset: {})",
        result.offset_ms, result.frame_offset
    ));
    lines.push(format!(
        "Source FPS: {:.3} | Target FPS: {:.3}",
        result.source_fps, result.target_fps
    ));
    lines.push(format!(
        "Content: source={}, target={}",
        result.source_content_type, result.target_content_type
    ));
    lines.push(format!(
        "Sample interval: {}s | Search range: +/-{} frames",
        result.sample_interval_s, result.search_range
    ));
    lines.push(format!(
        "Duration: {:.1}s | Samples: {}",
        result.total_duration_s, result.total_samples
    ));
    lines.push(String::new());

    // Overall accuracy
    lines.push("=".repeat(70));
    lines.push("OVERALL ACCURACY (excluding credits)".to_string());
    lines.push("=".repeat(70));
    lines.push(String::new());

    let mt = result.main_total;
    if mt > 0 {
        lines.push(format!(
            "  Exact match (delta=0): {:4}/{} ({:.1}%)",
            result.main_exact,
            mt,
            100.0 * result.main_exact as f64 / mt as f64
        ));
        lines.push(format!(
            "  Within +/-1 frame:     {:4}/{} ({:.1}%)",
            result.main_within_1,
            mt,
            100.0 * result.main_within_1 as f64 / mt as f64
        ));
        lines.push(format!(
            "  Within +/-2 frames:    {:4}/{} ({:.1}%)",
            result.main_within_2,
            mt,
            100.0 * result.main_within_2 as f64 / mt as f64
        ));
        lines.push(format!(
            "  Unmatchable:           {:4}/{} ({:.1}%)",
            result.main_unmatchable,
            mt,
            100.0 * result.main_unmatchable as f64 / mt as f64
        ));
        lines.push(format!(
            "  Static/low-info:       {:4}/{} ({:.1}%)",
            result.main_static,
            mt,
            100.0 * result.main_static as f64 / mt as f64
        ));
    }
    lines.push(String::new());

    // Verdict
    let accuracy = result.accuracy_pct();
    let verdict = if accuracy >= 95.0 {
        "GOOD"
    } else if accuracy >= 85.0 {
        "FAIR"
    } else if accuracy >= 70.0 {
        "MARGINAL"
    } else {
        "POOR"
    };

    lines.push("=".repeat(70));
    lines.push("VERDICT".to_string());
    lines.push("=".repeat(70));
    lines.push(String::new());
    lines.push(format!("  Offset verification: {}", verdict));
    lines.push(format!(
        "  Main content accuracy (within +/-2): {:.1}%",
        accuracy
    ));
    lines.push(String::new());
    lines.push("=".repeat(70));
    lines.push("END OF REPORT".to_string());
    lines.push("=".repeat(70));

    let _ = std::fs::write(&output_path, lines.join("\n"));

    if let Some(log_fn) = log {
        log_fn(&format!(
            "[VisualVerify] Report written to: {}",
            output_path.display()
        ));
    }

    output_path
}
