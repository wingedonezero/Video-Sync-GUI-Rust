//! Drift detection — 1:1 port of `vsg_core/analysis/drift_detection.py`.
//!
//! Analyzes correlation chunks to diagnose sync issue type:
//! uniform, PAL drift, linear drift, or stepping.
//! Implements DBSCAN clustering directly (no sklearn dependency).

use std::collections::HashMap;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

use super::types::*;

/// Get video framerate via ffprobe — `_get_video_framerate`
fn get_video_framerate(
    video_path: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> f64 {
    let out = runner.run(
        &[
            "ffprobe", "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=avg_frame_rate",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path,
        ],
        tool_paths,
    );
    let out = match out {
        Some(o) => o,
        None => return 0.0,
    };
    let trimmed = out.trim();
    if !trimmed.contains('/') {
        return 0.0;
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() != 2 {
        return 0.0;
    }
    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let den: f64 = parts[1].parse().unwrap_or(0.0);
    if den != 0.0 { num / den } else { 0.0 }
}

/// Get quality thresholds based on mode — `_get_quality_thresholds`
fn get_quality_thresholds(settings: &AppSettings) -> QualityThresholds {
    let mode = settings.stepping_quality_mode.to_string();
    match mode.as_str() {
        "strict" => QualityThresholds {
            min_cluster_percentage: 10.0,
            min_cluster_duration_s: 30.0,
            min_match_quality_pct: 90.0,
            min_total_clusters: 2,
        },
        "lenient" => QualityThresholds {
            min_cluster_percentage: 3.0,
            min_cluster_duration_s: 10.0,
            min_match_quality_pct: 75.0,
            min_total_clusters: 2,
        },
        "custom" => QualityThresholds {
            min_cluster_percentage: settings.stepping_min_cluster_percentage,
            min_cluster_duration_s: settings.stepping_min_cluster_duration_s,
            min_match_quality_pct: settings.stepping_min_match_quality_pct,
            min_total_clusters: settings.stepping_min_total_clusters,
        },
        _ => QualityThresholds { // "normal" default
            min_cluster_percentage: 5.0,
            min_cluster_duration_s: 20.0,
            min_match_quality_pct: 85.0,
            min_total_clusters: 2,
        },
    }
}

/// Simple DBSCAN implementation — replaces sklearn.cluster.DBSCAN
///
/// Returns cluster labels for each point (-1 = noise).
pub fn dbscan_1d(values: &[f64], eps: f64, min_samples: usize) -> Vec<i32> {
    let n = values.len();
    let mut labels = vec![-1i32; n];
    let mut cluster_id = 0i32;

    for i in 0..n {
        if labels[i] != -1 {
            continue; // Already assigned
        }

        // Find neighbors within eps
        let neighbors: Vec<usize> = (0..n)
            .filter(|&j| (values[i] - values[j]).abs() <= eps)
            .collect();

        if neighbors.len() < min_samples {
            continue; // Noise point (might be claimed later)
        }

        // Start new cluster
        labels[i] = cluster_id;
        let mut seed_set: Vec<usize> = neighbors
            .iter()
            .filter(|&&j| j != i)
            .copied()
            .collect();

        let mut idx = 0;
        while idx < seed_set.len() {
            let q = seed_set[idx];
            if labels[q] == -1 || labels[q] == -2 {
                labels[q] = cluster_id;
            }
            if labels[q] != cluster_id {
                idx += 1;
                continue;
            }

            // Find q's neighbors
            let q_neighbors: Vec<usize> = (0..n)
                .filter(|&j| (values[q] - values[j]).abs() <= eps)
                .collect();

            if q_neighbors.len() >= min_samples {
                for &qn in &q_neighbors {
                    if labels[qn] == -1 {
                        seed_set.push(qn);
                        labels[qn] = cluster_id;
                    }
                }
            }

            idx += 1;
        }

        cluster_id += 1;
    }

    labels
}

/// Validate a single cluster — `_validate_cluster`
fn validate_cluster(
    members: &[usize],
    accepted_chunks: &[&ChunkResult],
    total_chunks: usize,
    thresholds: &QualityThresholds,
    chunk_duration: f64,
) -> ClusterValidation {
    let cluster_size = members.len();
    let cluster_percentage = if total_chunks > 0 {
        cluster_size as f64 / total_chunks as f64 * 100.0
    } else {
        0.0
    };

    let chunk_times: Vec<f64> = members.iter().map(|&i| accepted_chunks[i].start_s).collect();
    let min_time = chunk_times.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_time = chunk_times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let cluster_duration_s = (max_time - min_time) + chunk_duration;

    let match_qualities: Vec<f64> = members.iter().map(|&i| accepted_chunks[i].match_pct).collect();
    let avg_match = match_qualities.iter().sum::<f64>() / match_qualities.len().max(1) as f64;
    let min_match = match_qualities.iter().cloned().fold(f64::INFINITY, f64::min);

    let mut checks = HashMap::new();
    checks.insert("percentage".to_string(), ValidationCheck {
        passed: cluster_percentage >= thresholds.min_cluster_percentage,
        value: cluster_percentage,
        threshold: thresholds.min_cluster_percentage,
        label: "Cluster size".to_string(),
    });
    checks.insert("duration".to_string(), ValidationCheck {
        passed: cluster_duration_s >= thresholds.min_cluster_duration_s,
        value: cluster_duration_s,
        threshold: thresholds.min_cluster_duration_s,
        label: "Duration".to_string(),
    });
    checks.insert("match_quality".to_string(), ValidationCheck {
        passed: avg_match >= thresholds.min_match_quality_pct,
        value: avg_match,
        threshold: thresholds.min_match_quality_pct,
        label: "Match quality".to_string(),
    });

    let all_passed = checks.values().all(|c| c.passed);
    let passed_count = checks.values().filter(|c| c.passed).count() as i32;

    ClusterValidation {
        valid: all_passed,
        checks,
        passed_count,
        total_checks: 3,
        cluster_size,
        cluster_percentage,
        cluster_duration_s,
        avg_match_quality: avg_match,
        min_match_quality: min_match,
        time_range: (min_time, max_time + chunk_duration),
    }
}

/// Analyze correlation chunks to diagnose sync issue type — `diagnose_audio_issue`
pub fn diagnose_audio_issue(
    video_path: &str,
    chunks: &[ChunkResult],
    settings: &AppSettings,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    codec_id: &str,
) -> DiagnosisResult {
    let accepted: Vec<&ChunkResult> = chunks.iter().filter(|c| c.accepted).collect();
    if accepted.len() < 6 {
        return DiagnosisResult::Uniform;
    }

    let times: Vec<f64> = accepted.iter().map(|c| c.start_s).collect();
    let delays: Vec<f64> = accepted.iter().map(|c| c.delay_ms as f64).collect();

    // --- Test 1: PAL Drift ---
    let framerate = get_video_framerate(video_path, runner, tool_paths);
    let is_pal = (framerate - 25.0).abs() < 0.1;
    if is_pal {
        let (slope, _) = linear_fit(&times, &delays);
        if (slope - 40.9).abs() < 5.0 {
            runner.log_message(&format!(
                "[PAL Drift Detected] Framerate is ~25fps and audio drift rate is {slope:.2} ms/s."
            ));
            return DiagnosisResult::Drift {
                diagnosis: "PAL_DRIFT".to_string(),
                rate: slope,
            };
        }
    }

    // --- Test 2: Stepping (DBSCAN) ---
    let epsilon_ms = settings.detection_dbscan_epsilon_ms;
    let min_samples_pct = settings.detection_dbscan_min_samples_pct;
    let min_samples = 2.max((delays.len() as f64 * min_samples_pct / 100.0) as usize);

    runner.log_message(&format!(
        "[DBSCAN] eps={epsilon_ms:.0}ms, min_samples={min_samples} ({min_samples_pct:.1}% of {} windows)",
        delays.len()
    ));

    let labels = dbscan_1d(&delays, epsilon_ms, min_samples);
    let mut cluster_members: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        if label != -1 {
            cluster_members.entry(label).or_default().push(i);
        }
    }

    let unique_clusters: Vec<i32> = cluster_members.keys().copied().collect();

    if unique_clusters.len() > 1 {
        let correction_mode = settings.stepping_correction_mode.to_string();

        if correction_mode == "disabled" {
            runner.log_message(&format!(
                "[Stepping] Found {} timing clusters, but stepping correction is disabled.",
                unique_clusters.len()
            ));
            return DiagnosisResult::Uniform;
        }

        let thresholds = get_quality_thresholds(settings);
        let chunk_duration = settings.dense_window_s;

        runner.log_message(&format!(
            "[Stepping Detection] Found {} timing clusters",
            unique_clusters.len()
        ));

        // Validate clusters
        let mut valid_clusters: HashMap<i32, Vec<i32>> = HashMap::new();
        let mut invalid_clusters: HashMap<i32, Vec<i32>> = HashMap::new();
        let mut validation_results: HashMap<i32, ClusterValidation> = HashMap::new();

        for (&label, members) in &cluster_members {
            let validation = validate_cluster(
                members, &accepted, accepted.len(), &thresholds, chunk_duration,
            );
            if validation.valid {
                valid_clusters.insert(label, members.iter().map(|&i| i as i32).collect());
            } else {
                invalid_clusters.insert(label, members.iter().map(|&i| i as i32).collect());
            }
            validation_results.insert(label, validation);
        }

        // Build cluster diagnostics
        let verbose = settings.stepping_diagnostics_verbose;
        let cluster_details = build_cluster_diagnostics(
            &accepted, &cluster_members, verbose, &|msg| runner.log_message(msg),
        );

        // Decide based on correction mode
        match correction_mode.as_str() {
            "full" | "strict" => {
                if !invalid_clusters.is_empty() {
                    runner.log_message(&format!(
                        "[Stepping Rejected] {}/{} clusters failed validation in '{correction_mode}' mode.",
                        invalid_clusters.len(), cluster_members.len()
                    ));
                    return DiagnosisResult::Uniform;
                }
                if (valid_clusters.len() as i32) < thresholds.min_total_clusters {
                    return DiagnosisResult::Uniform;
                }
                DiagnosisResult::Stepping {
                    cluster_count: valid_clusters.len(),
                    cluster_details,
                    valid_clusters,
                    invalid_clusters,
                    validation_results,
                    correction_mode,
                    fallback_mode: None,
                }
            }
            "filtered" => {
                if (valid_clusters.len() as i32) < thresholds.min_total_clusters {
                    return DiagnosisResult::Uniform;
                }
                let fallback = settings.stepping_filtered_fallback.to_string();
                if fallback == "reject" && !invalid_clusters.is_empty() {
                    return DiagnosisResult::Uniform;
                }
                DiagnosisResult::Stepping {
                    cluster_count: valid_clusters.len(),
                    cluster_details,
                    valid_clusters,
                    invalid_clusters,
                    validation_results,
                    correction_mode,
                    fallback_mode: Some(fallback),
                }
            }
            _ => DiagnosisResult::Uniform,
        }
    } else {
        // --- Test 3: General Linear Drift ---
        let (slope, intercept) = linear_fit(&times, &delays);

        let codec_lower = codec_id.to_lowercase();
        let is_lossless = codec_lower.contains("pcm")
            || codec_lower.contains("flac")
            || codec_lower.contains("truehd");

        let slope_threshold = if is_lossless {
            settings.drift_detection_slope_threshold_lossless
        } else {
            settings.drift_detection_slope_threshold_lossy
        };
        let r2_threshold = if is_lossless {
            settings.drift_detection_r2_threshold_lossless
        } else {
            settings.drift_detection_r2_threshold
        };

        runner.log_message(&format!(
            "[DriftDiagnosis] Codec: {codec_lower} (lossless={is_lossless}). Using R²>{r2_threshold:.2}, slope>{slope_threshold:.1} ms/s."
        ));

        if slope.abs() > slope_threshold {
            let predicted: Vec<f64> = times.iter().map(|&t| slope * t + intercept).collect();
            let r_squared = r_squared_calc(&delays, &predicted);

            if r_squared > r2_threshold {
                runner.log_message(&format!(
                    "[Linear Drift Detected] R²={r_squared:.3}, slope={slope:.2} ms/s."
                ));
                return DiagnosisResult::Drift {
                    diagnosis: "LINEAR_DRIFT".to_string(),
                    rate: slope,
                };
            }
        }

        DiagnosisResult::Uniform
    }
}

/// Format chunk numbers as ranges — `_format_chunk_range`
/// e.g. [1,2,3,5,25,26,27] → "1-3,5,25-27"
fn format_chunk_range(chunk_numbers: &[i32]) -> String {
    if chunk_numbers.is_empty() {
        return String::new();
    }

    let mut sorted = chunk_numbers.to_vec();
    sorted.sort_unstable();

    let mut ranges = Vec::new();
    let mut start = sorted[0];
    let mut end = sorted[0];

    for &num in &sorted[1..] {
        if num == end + 1 {
            end = num;
        } else {
            if start == end {
                ranges.push(format!("{start}"));
            } else {
                ranges.push(format!("{start}-{end}"));
            }
            start = num;
            end = num;
        }
    }
    if start == end {
        ranges.push(format!("{start}"));
    } else {
        ranges.push(format!("{start}-{end}"));
    }
    ranges.join(",")
}

/// Analyze and report patterns in delay transitions — `_analyze_transition_patterns`
fn analyze_transition_patterns(cluster_info: &[ClusterDiagnostic], log: &dyn Fn(&str)) {
    if cluster_info.len() < 2 {
        return;
    }

    // Calculate all jumps
    let jumps: Vec<f64> = cluster_info
        .windows(2)
        .map(|w| w[1].mean_delay_ms - w[0].mean_delay_ms)
        .collect();

    let all_positive = jumps.iter().all(|&j| j > 0.0);
    let all_negative = jumps.iter().all(|&j| j < 0.0);

    // Check for consistent jump sizes (50ms tolerance)
    let jump_sizes: Vec<f64> = jumps.iter().map(|j| j.abs()).collect();
    let mean_jump = jump_sizes.iter().sum::<f64>() / jump_sizes.len() as f64;
    let consistent_jumps = jump_sizes.iter().all(|&j| (j - mean_jump).abs() < 50.0);

    log("[Transition Analysis]:");

    if all_positive {
        log("  → All delays INCREASE (accumulating lag = missing content)");
    } else if all_negative {
        log("  → All delays DECREASE (accumulating lead = extra content)");
    } else {
        log("  → Mixed pattern (some increases, some decreases)");
    }

    if consistent_jumps && jumps.len() > 1 {
        log(&format!(
            "  → Consistent jump size: ~{mean_jump:.0}ms per transition"
        ));
        log("  → Likely cause: Regular reel changes or commercial breaks");
    } else {
        let jump_strs: Vec<String> = jumps.iter().map(|j| format!("{j:+.0}ms")).collect();
        log(&format!(
            "  → Variable jump sizes: {}",
            jump_strs.join(", ")
        ));
        log("  → Likely cause: Scene-specific edits or variable content changes");
    }

    // Low match score warnings (70% threshold)
    let low_match_clusters: Vec<&ClusterDiagnostic> = cluster_info
        .iter()
        .filter(|c| c.min_match_pct < 70.0)
        .collect();
    if !low_match_clusters.is_empty() {
        log(&format!(
            "  ⚠ {} clusters have chunks with match < 70%",
            low_match_clusters.len()
        ));
        log("  → Possible silence sections or content mismatches at transitions");
    }
}

/// Build cluster diagnostics with verbose logging — `_build_cluster_diagnostics`
fn build_cluster_diagnostics(
    accepted: &[&ChunkResult],
    cluster_members: &HashMap<i32, Vec<usize>>,
    verbose: bool,
    log: &dyn Fn(&str),
) -> Vec<ClusterDiagnostic> {
    let mut info: Vec<ClusterDiagnostic> = Vec::new();

    for (&label, members) in cluster_members {
        let raw_delays: Vec<f64> = members.iter().map(|&i| accepted[i].raw_delay_ms).collect();
        let mean_delay = raw_delays.iter().sum::<f64>() / raw_delays.len() as f64;
        let std_delay = std_dev_pop(&raw_delays, mean_delay);
        let times: Vec<f64> = members.iter().map(|&i| accepted[i].start_s).collect();
        let min_time = times.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_time = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let match_scores: Vec<f64> = members.iter().map(|&i| accepted[i].match_pct).collect();
        let mean_match = match_scores.iter().sum::<f64>() / match_scores.len() as f64;
        let min_match = match_scores.iter().cloned().fold(f64::INFINITY, f64::min);
        let chunk_numbers: Vec<i32> = members.iter().map(|&i| (i + 1) as i32).collect();

        info.push(ClusterDiagnostic {
            cluster_id: label,
            mean_delay_ms: mean_delay,
            std_delay_ms: std_delay,
            chunk_count: members.len(),
            chunk_numbers,
            raw_delays,
            time_range: (min_time, max_time),
            mean_match_pct: mean_match,
            min_match_pct: min_match,
        });
    }

    info.sort_by(|a, b| a.mean_delay_ms.partial_cmp(&b.mean_delay_ms).unwrap());

    // Verbose logging — 1:1 with Python
    if verbose && !info.is_empty() {
        log("[Cluster Diagnostics] Detailed composition:");

        for (i, cluster) in info.iter().enumerate() {
            let chunk_range = format_chunk_range(&cluster.chunk_numbers);
            let delay_jump = if i > 0 {
                let prev_delay = info[i - 1].mean_delay_ms;
                let jump = cluster.mean_delay_ms - prev_delay;
                let direction = if jump > 0.0 { "↑" } else { "↓" };
                format!(" [{direction}{:+.0}ms jump]", jump.abs())
            } else {
                String::new()
            };

            log(&format!(
                "  Cluster {}: delay={:+.0}±{:.1}ms, chunks {} (@{:.1}s - @{:.1}s), \
                 match={:.1}% (min={:.1}%){delay_jump}",
                i + 1,
                cluster.mean_delay_ms,
                cluster.std_delay_ms,
                chunk_range,
                cluster.time_range.0,
                cluster.time_range.1,
                cluster.mean_match_pct,
                cluster.min_match_pct,
            ));
        }

        // Analyze transition patterns
        analyze_transition_patterns(&info, log);
    }

    info
}

/// Simple linear regression (least squares) — returns (slope, intercept)
fn linear_fit(x: &[f64], y: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    if n < 2.0 {
        return (0.0, 0.0);
    }
    let sum_x: f64 = x.iter().sum();
    let sum_y: f64 = y.iter().sum();
    let sum_xy: f64 = x.iter().zip(y).map(|(a, b)| a * b).sum();
    let sum_xx: f64 = x.iter().map(|a| a * a).sum();

    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-12 {
        return (0.0, sum_y / n);
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;
    (slope, intercept)
}

/// R-squared calculation
fn r_squared_calc(actual: &[f64], predicted: &[f64]) -> f64 {
    let mean_actual = actual.iter().sum::<f64>() / actual.len() as f64;
    let ss_res: f64 = actual.iter().zip(predicted).map(|(a, p)| (a - p).powi(2)).sum();
    let ss_tot: f64 = actual.iter().map(|a| (a - mean_actual).powi(2)).sum();
    if ss_tot.abs() < 1e-12 {
        return 0.0;
    }
    1.0 - ss_res / ss_tot
}

/// Population standard deviation
fn std_dev_pop(values: &[f64], mean: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let variance = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}
