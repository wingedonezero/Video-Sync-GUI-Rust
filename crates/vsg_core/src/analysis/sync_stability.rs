//! Sync stability analyzer — 1:1 port of `vsg_core/analysis/sync_stability.py`.

use crate::models::context_types::SyncStabilityIssue;
use crate::models::settings::AppSettings;

use super::types::{ChunkResult, ClusterDiagnostic};

/// Analyze correlation results for variance/stability issues — `analyze_sync_stability`
pub fn analyze_sync_stability(
    chunk_results: &[ChunkResult],
    source_key: &str,
    settings: &AppSettings,
    log: Option<&dyn Fn(&str)>,
    stepping_clusters: Option<&[ClusterDiagnostic]>,
) -> Option<SyncStabilityIssue> {
    if !settings.sync_stability_enabled {
        return None;
    }

    let variance_threshold = settings.sync_stability_variance_threshold;
    let min_chunks = settings.sync_stability_min_windows;
    let outlier_mode = &settings.sync_stability_outlier_mode;
    let outlier_threshold = settings.sync_stability_outlier_threshold;

    let accepted: Vec<&ChunkResult> = chunk_results.iter().filter(|r| r.accepted).collect();

    if (accepted.len() as i32) < min_chunks {
        if let Some(log) = log {
            log(&format!(
                "[Sync Stability] {source_key}: Skipped - only {} windows (need {min_chunks})",
                accepted.len()
            ));
        }
        return None;
    }

    let raw_delays: Vec<f64> = accepted.iter().map(|r| r.raw_delay_ms).collect();

    if let Some(clusters) = stepping_clusters {
        if clusters.len() > 1 {
            return Some(analyze_with_clusters(
                &accepted,
                source_key,
                log,
                clusters,
                variance_threshold,
                outlier_mode,
                outlier_threshold,
            ));
        }
    }

    Some(analyze_uniform(
        &accepted,
        &raw_delays,
        source_key,
        log,
        variance_threshold,
        outlier_mode,
        outlier_threshold,
    ))
}

fn analyze_uniform(
    accepted: &[&ChunkResult],
    raw_delays: &[f64],
    source_key: &str,
    log: Option<&dyn Fn(&str)>,
    variance_threshold: f64,
    outlier_mode: &crate::models::enums::SyncStabilityOutlierMode,
    outlier_threshold: f64,
) -> SyncStabilityIssue {
    use crate::models::context_types::OutlierChunk;

    if raw_delays.len() < 2 {
        return SyncStabilityIssue {
            source: Some(source_key.to_string()),
            variance_detected: Some(false),
            reason: Some("insufficient_chunks".to_string()),
            chunk_count: Some(raw_delays.len() as i32),
            ..Default::default()
        };
    }

    let mean_delay = raw_delays.iter().sum::<f64>() / raw_delays.len() as f64;
    let std_delay = std_dev(raw_delays, mean_delay);
    let min_delay = raw_delays.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_delay = raw_delays.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let max_variance = max_delay - min_delay;

    // Detect outliers
    let mut outliers: Vec<OutlierChunk> = Vec::new();
    let is_any_mode = matches!(
        outlier_mode,
        crate::models::enums::SyncStabilityOutlierMode::Any
    );

    if is_any_mode {
        let reference = raw_delays[0];
        for (i, (chunk, &raw)) in accepted.iter().zip(raw_delays).enumerate() {
            if (raw - reference).abs() > 0.0001 {
                outliers.push(OutlierChunk {
                    chunk_index: Some((i + 1) as i32),
                    time_s: Some(chunk.start_s),
                    delay_ms: Some(raw),
                    deviation_ms: Some(raw - reference),
                    ..Default::default()
                });
            }
        }
    } else {
        for (i, (chunk, &raw)) in accepted.iter().zip(raw_delays).enumerate() {
            let deviation = (raw - mean_delay).abs();
            if deviation > outlier_threshold {
                outliers.push(OutlierChunk {
                    chunk_index: Some((i + 1) as i32),
                    time_s: Some(chunk.start_s),
                    delay_ms: Some(raw),
                    deviation_ms: Some(raw - mean_delay),
                    ..Default::default()
                });
            }
        }
    }

    let variance_detected = if variance_threshold <= 0.0 {
        max_variance > 0.0001
    } else {
        max_variance > variance_threshold
    };

    if let Some(log) = log {
        if variance_detected {
            log(&format!("[Sync Stability] {source_key}: Variance detected!"));
            log(&format!(
                "  - Max variance: {max_variance:.4}ms (threshold: {variance_threshold}ms)"
            ));
            log(&format!("  - Std dev: {std_delay:.4}ms"));
            log(&format!(
                "  - Range: {min_delay:.4}ms to {max_delay:.4}ms"
            ));
            if !outliers.is_empty() {
                log(&format!("  - Outliers: {} window(s)", outliers.len()));
            }
        } else {
            log(&format!(
                "[Sync Stability] {source_key}: OK - consistent results (variance: {max_variance:.4}ms)"
            ));
        }
    }

    let outlier_count = outliers.len() as i32;
    SyncStabilityIssue {
        source: Some(source_key.to_string()),
        variance_detected: Some(variance_detected),
        max_variance_ms: Some(round4(max_variance)),
        std_dev_ms: Some(round4(std_delay)),
        mean_delay_ms: Some(round4(mean_delay)),
        min_delay_ms: Some(round4(min_delay)),
        max_delay_ms: Some(round4(max_delay)),
        chunk_count: Some(accepted.len() as i32),
        outlier_count: Some(outlier_count),
        outliers: outliers.into_iter().take(10).collect(),
        cluster_count: Some(1),
        is_stepping: Some(false),
        ..Default::default()
    }
}

fn analyze_with_clusters(
    accepted: &[&ChunkResult],
    source_key: &str,
    log: Option<&dyn Fn(&str)>,
    stepping_clusters: &[ClusterDiagnostic],
    variance_threshold: f64,
    outlier_mode: &crate::models::enums::SyncStabilityOutlierMode,
    outlier_threshold: f64,
) -> SyncStabilityIssue {
    use crate::models::context_types::{ClusterIssue, OutlierChunk};

    let mut cluster_issues: Vec<ClusterIssue> = Vec::new();
    let mut total_outliers: Vec<OutlierChunk> = Vec::new();
    let mut max_cluster_variance: f64 = 0.0;

    let is_any_mode = matches!(
        outlier_mode,
        crate::models::enums::SyncStabilityOutlierMode::Any
    );

    for cluster in stepping_clusters {
        let cluster_delays = &cluster.raw_delays;
        if cluster_delays.len() < 2 {
            continue;
        }

        let cluster_mean =
            cluster_delays.iter().sum::<f64>() / cluster_delays.len() as f64;
        let cluster_min = cluster_delays.iter().cloned().fold(f64::INFINITY, f64::min);
        let cluster_max = cluster_delays
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let cluster_variance = cluster_max - cluster_min;

        max_cluster_variance = max_cluster_variance.max(cluster_variance);

        let mut cluster_outliers = Vec::new();
        let reference = cluster_delays[0];

        for (i, &raw) in cluster_delays.iter().enumerate() {
            if is_any_mode {
                if (raw - reference).abs() > 0.0001 {
                    cluster_outliers.push(OutlierChunk {
                        cluster_id: Some(cluster.cluster_id),
                        chunk_index: cluster
                            .chunk_numbers
                            .get(i)
                            .copied()
                            .or(Some((i + 1) as i32)),
                        delay_ms: Some(raw),
                        deviation_ms: Some(raw - reference),
                        ..Default::default()
                    });
                }
            } else {
                let deviation = (raw - cluster_mean).abs();
                if deviation > outlier_threshold {
                    cluster_outliers.push(OutlierChunk {
                        cluster_id: Some(cluster.cluster_id),
                        chunk_index: cluster
                            .chunk_numbers
                            .get(i)
                            .copied()
                            .or(Some((i + 1) as i32)),
                        delay_ms: Some(raw),
                        deviation_ms: Some(raw - cluster_mean),
                        ..Default::default()
                    });
                }
            }
        }

        if !cluster_outliers.is_empty() {
            cluster_issues.push(ClusterIssue {
                cluster_id: Some(cluster.cluster_id),
                mean_delay: Some(cluster_mean),
                variance: Some(cluster_variance),
                outlier_count: Some(cluster_outliers.len() as i32),
            });
            total_outliers.extend(cluster_outliers);
        }
    }

    let variance_detected = if variance_threshold <= 0.0 {
        max_cluster_variance > 0.0001
    } else {
        max_cluster_variance > variance_threshold
    };

    if let Some(log) = log {
        if variance_detected {
            log(&format!(
                "[Sync Stability] {source_key}: Variance within stepping clusters!"
            ));
            log(&format!(
                "  - Max intra-cluster variance: {max_cluster_variance:.4}ms"
            ));
            log(&format!(
                "  - {} cluster(s) with outliers",
                cluster_issues.len()
            ));
        } else {
            log(&format!(
                "[Sync Stability] {source_key}: OK - clusters internally consistent"
            ));
        }
    }

    SyncStabilityIssue {
        source: Some(source_key.to_string()),
        variance_detected: Some(variance_detected),
        max_variance_ms: Some(round4(max_cluster_variance)),
        chunk_count: Some(accepted.len() as i32),
        outlier_count: Some(total_outliers.len() as i32),
        outliers: total_outliers.into_iter().take(10).collect(),
        cluster_count: Some(stepping_clusters.len() as i32),
        is_stepping: Some(true),
        cluster_issues,
        ..Default::default()
    }
}

fn std_dev(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let variance =
        values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
