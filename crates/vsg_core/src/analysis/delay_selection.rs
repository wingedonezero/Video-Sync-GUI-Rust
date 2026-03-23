//! Delay selection logic — 1:1 port of `vsg_core/analysis/delay_selection.py`.

use std::collections::HashMap;

use crate::models::settings::AppSettings;

use super::types::{ChunkResult, DelayCalculation};

/// Find the delay that dominates the early portion of the file — `find_first_stable_segment_delay`
pub fn find_first_stable_segment_delay(
    results: &[ChunkResult],
    settings: &AppSettings,
    return_raw: bool,
    log: &dyn Fn(&str),
    override_early_pct: Option<f64>,
) -> Option<f64> {
    let early_pct = override_early_pct.unwrap_or(settings.first_stable_early_pct);

    let accepted: Vec<&ChunkResult> = results.iter().filter(|r| r.accepted).collect();
    if accepted.len() < 3 {
        return None;
    }

    let early_count = 3.max((accepted.len() as f64 * early_pct / 100.0) as usize);
    let early_windows = &accepted[..early_count.min(accepted.len())];

    // Find dominant delay in early region
    let mut early_counts: HashMap<i32, usize> = HashMap::new();
    for r in early_windows {
        *early_counts.entry(r.delay_ms).or_insert(0) += 1;
    }

    let (top_delay, top_count) = early_counts
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(&d, &c)| (d, c))?;

    let mut early_agreement = top_count as f64 / early_windows.len() as f64 * 100.0;

    if early_agreement < 60.0 {
        // Try +/-1ms cluster
        let cluster_count: usize = early_counts
            .iter()
            .filter(|(&d, _)| (d - top_delay).abs() <= 1)
            .map(|(_, &c)| c)
            .sum();
        early_agreement = cluster_count as f64 / early_windows.len() as f64 * 100.0;

        if early_agreement < 60.0 {
            log(&format!(
                "[First Stable] No dominant delay in first {} windows ({early_pct:.0}% early region) \
                 (best: {top_delay:+}ms at {early_agreement:.0}% agreement)",
                early_windows.len()
            ));
            return None;
        }
    }

    // Collect ALL raw values matching this delay (+/-1ms) across the full file
    let matching_raw: Vec<f64> = accepted
        .iter()
        .filter(|r| (r.delay_ms - top_delay).abs() <= 1)
        .map(|r| r.raw_delay_ms)
        .collect();

    let raw_avg = matching_raw.iter().sum::<f64>() / matching_raw.len() as f64;
    let rounded_avg = raw_avg.round() as i32;

    log(&format!(
        "[First Stable] Dominant delay in first {} windows ({early_pct:.0}% early region): \
         {top_delay:+}ms ({early_agreement:.0}% agreement), \
         averaged {} matching windows -> {rounded_avg:+}ms (raw: {raw_avg:+.6}ms)",
        early_windows.len(),
        matching_raw.len()
    ));

    if return_raw {
        Some(raw_avg)
    } else {
        Some(rounded_avg as f64)
    }
}

/// Find delay from the earliest cluster — `_find_early_cluster_delay`
fn find_early_cluster_delay(
    accepted: &[&ChunkResult],
    settings: &AppSettings,
    return_raw: bool,
    log: &dyn Fn(&str),
) -> Option<f64> {
    let early_pct = settings.early_cluster_early_pct;
    let min_presence_pct = settings.early_cluster_min_presence_pct;

    if accepted.len() < 3 {
        return None;
    }

    let early_count = 3.max((accepted.len() as f64 * early_pct / 100.0) as usize);
    let early_windows = &accepted[..early_count.min(accepted.len())];

    // Find all delay clusters in the early region
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for r in early_windows {
        *counts.entry(r.delay_ms).or_insert(0) += 1;
    }

    struct ClusterInfo {
        raw_values: Vec<f64>,
        early_presence: usize,
        presence_pct: f64,
        first_window_idx: usize,
    }

    let mut cluster_info: HashMap<i32, ClusterInfo> = HashMap::new();

    for &delay_val in counts.keys() {
        let mut raw_values = Vec::new();
        let mut early_presence = 0usize;
        let mut first_window_idx: Option<usize> = None;

        for (idx, r) in accepted.iter().enumerate() {
            if (r.delay_ms - delay_val).abs() <= 1 {
                raw_values.push(r.raw_delay_ms);
                if idx < early_count {
                    early_presence += 1;
                }
                if first_window_idx.is_none() {
                    first_window_idx = Some(idx);
                }
            }
        }

        let pct = early_presence as f64 / early_windows.len() as f64 * 100.0;

        cluster_info.insert(
            delay_val,
            ClusterInfo {
                raw_values,
                early_presence,
                presence_pct: pct,
                first_window_idx: first_window_idx.unwrap_or(usize::MAX),
            },
        );
    }

    // Filter to clusters with enough early presence
    let mut qualifying: Vec<(i32, &ClusterInfo)> = cluster_info
        .iter()
        .filter(|(_, info)| info.presence_pct >= min_presence_pct)
        .map(|(&d, info)| (d, info))
        .collect();

    if qualifying.is_empty() {
        log(&format!(
            "[Early Cluster] No cluster met minimum presence ({min_presence_pct:.1}%) \
             in first {} windows ({early_pct:.0}% early region)",
            early_windows.len()
        ));
        return None;
    }

    // Pick the cluster that appears first in time
    qualifying.sort_by_key(|(_, info)| info.first_window_idx);
    let (winner_delay, winner_info) = qualifying[0];

    let raw_avg = winner_info.raw_values.iter().sum::<f64>() / winner_info.raw_values.len() as f64;
    let rounded_avg = raw_avg.round() as i32;

    log(&format!(
        "[Early Cluster] Found {} qualifying cluster(s) in first {} windows ({early_pct:.0}% early region), \
         selected earliest: {winner_delay:+}ms with {}/{} early windows ({:.1}% presence), \
         total {} matching windows, raw avg: {raw_avg:+.6}ms -> rounded to {rounded_avg:+}ms",
        qualifying.len(),
        early_windows.len(),
        winner_info.early_presence,
        early_windows.len(),
        winner_info.presence_pct,
        winner_info.raw_values.len()
    ));

    if return_raw {
        Some(raw_avg)
    } else {
        Some(rounded_avg as f64)
    }
}

/// Select final delay from correlation results — `calculate_delay`
pub fn calculate_delay(
    results: &[ChunkResult],
    settings: &AppSettings,
    delay_mode: &str,
    log: &dyn Fn(&str),
    role_tag: &str,
) -> Option<DelayCalculation> {
    let accepted: Vec<&ChunkResult> = results.iter().filter(|r| r.accepted).collect();
    let total_windows = results.len();
    let min_accepted = 10.max((total_windows as f64 * settings.min_accepted_pct / 100.0) as usize);

    if accepted.len() < min_accepted {
        let actual_pct = if total_windows > 0 {
            accepted.len() as f64 / total_windows as f64 * 100.0
        } else {
            0.0
        };
        log(&format!(
            "[ERROR] Analysis failed: Only {}/{total_windows} windows accepted \
             ({actual_pct:.1}%, need {:.0}%).",
            accepted.len(),
            settings.min_accepted_pct
        ));
        return None;
    }

    let delays: Vec<i32> = accepted.iter().map(|r| r.delay_ms).collect();
    let raw_delays: Vec<f64> = accepted.iter().map(|r| r.raw_delay_ms).collect();

    let (winner, winner_raw, method_label) = match delay_mode {
        "First Stable" => {
            let stable_rounded =
                find_first_stable_segment_delay(results, settings, false, log, None);
            let stable_raw =
                find_first_stable_segment_delay(results, settings, true, log, None);
            match stable_rounded {
                None => {
                    log("[WARNING] No stable early region found, falling back to mode (clustered).");
                    let (w, wr) = mode_clustered_fallback(&accepted);
                    (w, wr, "mode clustered (first stable fallback)".to_string())
                }
                Some(sr) => (
                    sr as i32,
                    stable_raw.unwrap_or(sr),
                    "first stable".to_string(),
                ),
            }
        }
        "Mode (Early Cluster)" => {
            let ec_rounded = find_early_cluster_delay(&accepted, settings, false, log);
            let ec_raw = find_early_cluster_delay(&accepted, settings, true, log);
            match ec_rounded {
                None => {
                    log("[WARNING] No qualifying early cluster found, falling back to mode (clustered).");
                    let (w, wr) = mode_clustered_fallback(&accepted);
                    (w, wr, "mode clustered (early cluster fallback)".to_string())
                }
                Some(ecr) => (
                    ecr as i32,
                    ec_raw.unwrap_or(ecr),
                    "early cluster".to_string(),
                ),
            }
        }
        "Average" => {
            let raw_avg = raw_delays.iter().sum::<f64>() / raw_delays.len() as f64;
            let winner = raw_avg.round() as i32;
            log(&format!(
                "[Delay Selection] Average of {} raw values: {raw_avg:+.6}ms -> rounded to {winner:+}ms",
                raw_delays.len()
            ));
            (winner, raw_avg, "average".to_string())
        }
        "Mode (Clustered)" => {
            let (w, wr) = mode_clustered_calc(&accepted, &delays, log);
            (w, wr, "mode (clustered)".to_string())
        }
        _ => {
            // Mode (Most Common) - default
            let mut counts: HashMap<i32, usize> = HashMap::new();
            for &d in &delays {
                *counts.entry(d).or_insert(0) += 1;
            }
            let (&winner, &count) = counts.iter().max_by_key(|(_, &c)| c).unwrap();
            let matching_raw: Vec<f64> = accepted
                .iter()
                .filter(|r| r.delay_ms == winner)
                .map(|r| r.raw_delay_ms)
                .collect();
            let winner_raw = if matching_raw.is_empty() {
                winner as f64
            } else {
                matching_raw.iter().sum::<f64>() / matching_raw.len() as f64
            };
            log(&format!(
                "[Delay Selection] Mode (Most Common): {winner:+}ms ({count}/{} windows), \
                 raw avg: {winner_raw:+.6}ms",
                accepted.len()
            ));
            (winner, winner_raw, "mode".to_string())
        }
    };

    log(&format!(
        "{} delay determined: {winner:+}ms (raw: {winner_raw:+.6}ms) [{method_label}]",
        capitalize_first(role_tag)
    ));

    Some(DelayCalculation {
        rounded_ms: winner,
        raw_ms: winner_raw,
        selection_method: method_label,
        accepted_windows: accepted.len(),
        total_windows: results.len(),
    })
}

fn mode_clustered_fallback(accepted: &[&ChunkResult]) -> (i32, f64) {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for r in accepted {
        *counts.entry(r.delay_ms).or_insert(0) += 1;
    }
    let (&mode_winner, _) = counts.iter().max_by_key(|(_, &c)| c).unwrap();
    let cluster_raw: Vec<f64> = accepted
        .iter()
        .filter(|r| (r.delay_ms - mode_winner).abs() <= 1)
        .map(|r| r.raw_delay_ms)
        .collect();
    if cluster_raw.is_empty() {
        (mode_winner, mode_winner as f64)
    } else {
        let raw_avg = cluster_raw.iter().sum::<f64>() / cluster_raw.len() as f64;
        (raw_avg.round() as i32, raw_avg)
    }
}

fn mode_clustered_calc(
    accepted: &[&ChunkResult],
    delays: &[i32],
    log: &dyn Fn(&str),
) -> (i32, f64) {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &d in delays {
        *counts.entry(d).or_insert(0) += 1;
    }
    let (&mode_winner, _) = counts.iter().max_by_key(|(_, &c)| c).unwrap();

    let mut cluster_raw: Vec<f64> = Vec::new();
    let mut cluster_delays: Vec<i32> = Vec::new();
    for r in accepted {
        if (r.delay_ms - mode_winner).abs() <= 1 {
            cluster_raw.push(r.raw_delay_ms);
            cluster_delays.push(r.delay_ms);
        }
    }

    if cluster_raw.is_empty() {
        log(&format!(
            "[Delay Selection] Mode (Clustered): fallback to simple mode = {mode_winner:+}ms"
        ));
        (mode_winner, mode_winner as f64)
    } else {
        let raw_avg = cluster_raw.iter().sum::<f64>() / cluster_raw.len() as f64;
        let winner = raw_avg.round() as i32;
        let mut cluster_counts: HashMap<i32, usize> = HashMap::new();
        for &d in &cluster_delays {
            *cluster_counts.entry(d).or_insert(0) += 1;
        }
        log(&format!(
            "[Delay Selection] Mode (Clustered): most common = {mode_winner:+}ms, \
             cluster [{} to {}] contains {}/{} windows (breakdown: {:?}), \
             raw avg: {raw_avg:+.6}ms -> rounded to {winner:+}ms",
            mode_winner - 1,
            mode_winner + 1,
            cluster_raw.len(),
            accepted.len(),
            cluster_counts
        ));
        (winner, raw_avg)
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}
