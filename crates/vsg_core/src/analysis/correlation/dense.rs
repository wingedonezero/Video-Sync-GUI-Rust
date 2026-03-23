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

    // Summary logging — 1:1 with Python _log_dense_summary
    log_dense_summary(
        &results,
        silence_count,
        method.name(),
        _outlier_threshold_ms,
        duration_s,
        scan_start as f64 / sr as f64,
        scan_end as f64 / sr as f64,
        log,
        _dbscan_epsilon_ms,
        _dbscan_min_samples_pct,
    );

    results
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Format seconds as M:SS or H:MM:SS — `_fmt_time`
fn fmt_time(seconds: f64) -> String {
    let s = seconds.round() as i64;
    if s < 3600 {
        format!("{}:{:02}", s / 60, s % 60)
    } else {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    }
}

// ── Summary Logging ──────────────────────────────────────────────────────────

/// Log a detailed summary of the dense correlation results — `_log_dense_summary`
#[allow(clippy::too_many_arguments)]
fn log_dense_summary(
    results: &[ChunkResult],
    silence_count: usize,
    method_name: &str,
    outlier_threshold_ms: f64,
    file_duration_s: f64,
    scan_start_s: f64,
    scan_end_s: f64,
    log: &dyn Fn(&str),
    dbscan_epsilon_ms: f64,
    dbscan_min_samples_pct: f64,
) {
    let accepted: Vec<&ChunkResult> = results.iter().filter(|r| r.accepted).collect();
    let rejected: Vec<&ChunkResult> = results.iter().filter(|r| !r.accepted).collect();
    let total = results.len();

    if accepted.is_empty() {
        log(&format!(
            "\n  [Summary] {method_name}: NO ACCEPTED WINDOWS (all {total} rejected)"
        ));
        return;
    }

    let delays: Vec<f64> = accepted.iter().map(|r| r.raw_delay_ms).collect();
    let confs: Vec<f64> = accepted.iter().map(|r| r.match_pct).collect();
    let rounded_delays: Vec<i32> = accepted.iter().map(|r| r.delay_ms).collect();
    let times: Vec<f64> = accepted.iter().map(|r| r.start_s).collect();

    let mut sorted_delays = delays.clone();
    sorted_delays.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_delay = if sorted_delays.len() % 2 == 0 {
        (sorted_delays[sorted_delays.len() / 2 - 1] + sorted_delays[sorted_delays.len() / 2]) / 2.0
    } else {
        sorted_delays[sorted_delays.len() / 2]
    };
    let mean_delay = delays.iter().sum::<f64>() / delays.len() as f64;
    let std_delay = {
        let variance = delays.iter().map(|d| (d - mean_delay).powi(2)).sum::<f64>() / delays.len() as f64;
        variance.sqrt()
    };
    let min_delay = delays.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_delay = delays.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Outlier detection
    let outlier_count = delays.iter().filter(|&&d| (d - median_delay).abs() > outlier_threshold_ms).count();
    let outlier_pct = outlier_count as f64 / accepted.len() as f64 * 100.0;

    // Inlier statistics
    let inlier_delays: Vec<f64> = delays.iter().filter(|&&d| (d - median_delay).abs() <= outlier_threshold_ms).copied().collect();
    let inlier_median = if !inlier_delays.is_empty() {
        let mut sorted = inlier_delays.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        }
    } else {
        median_delay
    };
    let inlier_std = if inlier_delays.len() > 1 {
        let mean = inlier_delays.iter().sum::<f64>() / inlier_delays.len() as f64;
        let var = inlier_delays.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / inlier_delays.len() as f64;
        var.sqrt()
    } else {
        0.0
    };

    // Agreement: % of windows agreeing on top rounded delay
    let mut delay_counts: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    for &d in &rounded_delays {
        *delay_counts.entry(d).or_insert(0) += 1;
    }
    let mut most_common: Vec<(i32, usize)> = delay_counts.into_iter().collect();
    most_common.sort_by(|a, b| b.1.cmp(&a.1));
    let (top_delay, top_count) = most_common[0];
    let agreement_pct = top_count as f64 / accepted.len() as f64 * 100.0;

    // Classification
    let classification = classify_result(std_delay, outlier_pct, agreement_pct, accepted.len(), inlier_std);

    // Coverage
    let time_span_s = if times.len() > 1 { times.last().unwrap() - times.first().unwrap() } else { 0.0 };
    let coverage_pct = if file_duration_s > 0.0 { time_span_s / file_duration_s * 100.0 } else { 0.0 };

    log(&format!("\n{}", "─".repeat(70)));
    log(&format!("  CORRELATION SUMMARY — {method_name}"));
    log(&"─".repeat(70));

    log(&format!("  Result:      {classification}"));

    log(&format!(
        "  Coverage:    {} - {} ({} analyzed, {coverage_pct:.0}% of {})",
        fmt_time(scan_start_s), fmt_time(scan_end_s), fmt_time(time_span_s), fmt_time(file_duration_s)
    ));

    log(&format!(
        "  Windows:     {} accepted, {} rejected, {} silence ({} total)",
        accepted.len(), rejected.len(), silence_count, total + silence_count
    ));

    log(&format!(
        "  Agreement:   {agreement_pct:.1}% at {top_delay:+}ms ({top_count}/{} windows)",
        accepted.len()
    ));

    log(&format!(
        "  Delay:       {median_delay:+.3}ms median, {mean_delay:+.3}ms mean, {std_delay:.3}ms std"
    ));
    log(&format!("               [{min_delay:+.3}, {max_delay:+.3}]ms range"));

    if outlier_count > 0 {
        log(&format!(
            "  Inliers:     {inlier_median:+.3}ms median, {inlier_std:.3}ms std \
             ({} windows, excluding {outlier_count} outliers)",
            inlier_delays.len()
        ));
    }

    log(&format!(
        "  Outliers:    {outlier_count}/{} ({outlier_pct:.1}%) >{outlier_threshold_ms:.0}ms from median",
        accepted.len()
    ));

    // Confidence tiers
    let n = confs.len();
    let t90 = confs.iter().filter(|&&c| c >= 90.0).count();
    let t70 = confs.iter().filter(|&&c| (70.0..90.0).contains(&c)).count();
    let t50 = confs.iter().filter(|&&c| (50.0..70.0).contains(&c)).count();
    let tlow = confs.iter().filter(|&&c| c < 50.0).count();
    let mean_conf = confs.iter().sum::<f64>() / n as f64;
    let min_conf = confs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_conf = confs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    log(&format!(
        "  Confidence:  ≥90%: {t90} ({:.0}%) | 70-89%: {t70} ({:.0}%) | \
         50-69%: {t50} ({:.0}%) | <50%: {tlow} ({:.0}%)",
        t90 as f64 / n as f64 * 100.0,
        t70 as f64 / n as f64 * 100.0,
        t50 as f64 / n as f64 * 100.0,
        tlow as f64 / n as f64 * 100.0,
    ));
    log(&format!(
        "               mean={mean_conf:.1}%, min={min_conf:.1}%, max={max_conf:.1}%"
    ));

    // Delay distribution (top values with bar chart)
    let top_n = 6.min(most_common.len());
    log(&format!("  Delay distribution (top {top_n}):"));
    for &(delay_val, count) in &most_common[..top_n] {
        let pct = count as f64 / accepted.len() as f64 * 100.0;
        let bar_len = (pct / 2.0).min(50.0) as usize;
        let bar: String = "█".repeat(bar_len);
        log(&format!("    {delay_val:+6}ms: {count:5} ({pct:5.1}%) {bar}"));
    }

    // Cluster analysis for stepping detection
    log_cluster_analysis(&accepted, &delays, log, dbscan_epsilon_ms, dbscan_min_samples_pct);

    log(&"─".repeat(70));
}

/// Classify result as UNIFORM/STEPPING/NOISY/LOW DATA — `_classify_result`
fn classify_result(
    std_delay: f64,
    outlier_pct: f64,
    agreement_pct: f64,
    n_accepted: usize,
    inlier_std: f64,
) -> String {
    if n_accepted < 20 {
        return format!("⚠ LOW DATA (only {n_accepted} accepted windows)");
    }

    if agreement_pct >= 90.0 && inlier_std < 5.0 {
        return format!("UNIFORM ({agreement_pct:.1}% agreement, std={inlier_std:.3}ms)");
    }

    if agreement_pct >= 70.0 && inlier_std < 10.0 {
        return format!("UNIFORM ({agreement_pct:.1}% agreement, std={inlier_std:.3}ms, minor noise)");
    }

    if agreement_pct < 70.0 && std_delay > 50.0 && outlier_pct > 10.0 {
        return format!("STEPPING (std={std_delay:.0}ms, {agreement_pct:.0}% top agreement — see clusters)");
    }

    if outlier_pct > 30.0 {
        return format!("⚠ NOISY ({outlier_pct:.0}% outliers, {agreement_pct:.0}% agreement)");
    }

    format!("MODERATE ({agreement_pct:.0}% agreement, std={std_delay:.1}ms)")
}

/// Analyze and log delay clusters — `_log_cluster_analysis`
fn log_cluster_analysis(
    accepted: &[&ChunkResult],
    delays: &[f64],
    log: &dyn Fn(&str),
    dbscan_epsilon_ms: f64,
    dbscan_min_samples_pct: f64,
) {
    if accepted.len() < 10 {
        return;
    }

    // Quick check: if std is very small, uniform — no cluster analysis needed
    let mean = delays.iter().sum::<f64>() / delays.len() as f64;
    let std = (delays.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / delays.len() as f64).sqrt();
    if std < 10.0 {
        return;
    }

    // DBSCAN clustering — same impl as drift_detection
    let min_samples = 2.max((accepted.len() as f64 * dbscan_min_samples_pct / 100.0) as usize);
    let labels = crate::analysis::drift_detection::dbscan_1d(delays, dbscan_epsilon_ms, min_samples);

    let mut cluster_members: std::collections::HashMap<i32, Vec<usize>> = std::collections::HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        if label != -1 {
            cluster_members.entry(label).or_default().push(i);
        }
    }

    let unique_labels: Vec<i32> = cluster_members.keys().copied().collect();
    if unique_labels.len() < 2 {
        return;
    }

    let noise_count = labels.iter().filter(|&&l| l == -1).count();

    log(&format!(
        "\n  Cluster Analysis ({} groups, {} noise points):",
        unique_labels.len(), noise_count
    ));

    // Build cluster info sorted by time
    struct ClusterEntry {
        mean_d: f64, std_d: f64, pct: f64, count: usize,
        t_start: f64, t_end: f64, span_s: f64, mean_conf: f64,
    }
    let mut cluster_info: Vec<ClusterEntry> = Vec::new();
    for members in cluster_members.values() {
        let cd: Vec<f64> = members.iter().map(|&i| delays[i]).collect();
        let ct: Vec<f64> = members.iter().map(|&i| accepted[i].start_s).collect();
        let cc: Vec<f64> = members.iter().map(|&i| accepted[i].match_pct).collect();

        let mean_d = cd.iter().sum::<f64>() / cd.len() as f64;
        let std_d = (cd.iter().map(|d| (d - mean_d).powi(2)).sum::<f64>() / cd.len() as f64).sqrt();
        let count = members.len();
        let pct = count as f64 / accepted.len() as f64 * 100.0;
        let t_start = ct.iter().cloned().fold(f64::INFINITY, f64::min);
        let t_end = ct.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let span_s = t_end - t_start;
        let mean_conf = cc.iter().sum::<f64>() / cc.len() as f64;

        cluster_info.push(ClusterEntry { mean_d, std_d, pct, count, t_start, t_end, span_s, mean_conf });
    }

    cluster_info.sort_by(|a, b| a.t_start.partial_cmp(&b.t_start).unwrap());

    for (i, c) in cluster_info.iter().enumerate() {
        let jump_str = if i > 0 {
            let jump = c.mean_d - cluster_info[i - 1].mean_d;
            let direction = if jump > 0.0 { "+" } else { "" };
            format!("  [jump: {direction}{jump:.0}ms]")
        } else {
            String::new()
        };

        log(&format!(
            "    Cluster {}: {:+.1}ms (std={:.1}ms, n={}, {:.1}%) \
             @ {} - {} ({}) conf={:.1}%{jump_str}",
            i + 1, c.mean_d, c.std_d, c.count, c.pct,
            fmt_time(c.t_start), fmt_time(c.t_end), fmt_time(c.span_s), c.mean_conf
        ));
    }

    // Transition detection
    if cluster_info.len() >= 2 {
        log("\n  Transitions:");

        // Sort results by time and find transitions
        let mut indexed: Vec<(usize, f64)> = accepted.iter().enumerate().map(|(i, r)| (i, r.start_s)).collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let sorted_labels: Vec<i32> = indexed.iter().map(|&(i, _)| labels[i]).collect();
        let sorted_delays_arr: Vec<f64> = indexed.iter().map(|&(i, _)| accepted[i].raw_delay_ms).collect();
        let sorted_times: Vec<f64> = indexed.iter().map(|&(_, t)| t).collect();

        let mut prev_real_label: i32 = -1;
        for &l in &sorted_labels {
            if l != -1 {
                prev_real_label = l;
                break;
            }
        }

        let mut transitions = Vec::new();
        for i in 1..sorted_labels.len() {
            let cur = sorted_labels[i];
            if cur == -1 { continue; }
            if cur != prev_real_label && prev_real_label != -1 {
                // Find last non-noise point before this
                let mut found = false;
                for j in (0..i).rev() {
                    if sorted_labels[j] == prev_real_label {
                        transitions.push((
                            sorted_delays_arr[j], sorted_delays_arr[i],
                            sorted_times[j], sorted_times[i],
                        ));
                        found = true;
                        break;
                    }
                }
                if !found {
                    transitions.push((0.0, sorted_delays_arr[i], 0.0, sorted_times[i]));
                }
                prev_real_label = cur;
            } else if cur != -1 {
                prev_real_label = cur;
            }
        }

        if !transitions.is_empty() {
            for (d_before, d_after, t_before, t_after) in &transitions {
                log(&format!(
                    "    {d_before:+.1}ms → {d_after:+.1}ms between {} and {}",
                    fmt_time(*t_before), fmt_time(*t_after)
                ));
            }
        } else {
            log("    (no clean transitions detected)");
        }
    }
}
