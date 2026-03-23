//! EDL builder — 1:1 port of `vsg_core/correction/stepping/edl_builder.py`.
//!
//! Build transition zones and EDL segments from dense analysis data.
//! Replaces the old coarse-scan + binary-search pipeline by using the
//! cluster boundaries that DBSCAN already identified during analysis.

use std::collections::HashMap;

use crate::models::context_types::SegmentFlagsEntry;
use crate::models::settings::AppSettings;

use super::types::{AudioSegment, SteppingData, TransitionZone};

/// Identify transition zones between adjacent clusters — `find_transition_zones`
///
/// Each `TransitionZone` represents the gap in the reference timeline
/// between the last window of one cluster and the first window of the next.
/// Downstream code refines the precise splice point within this zone.
pub fn find_transition_zones(
    stepping_data: &SteppingData,
    segment_flags: &SegmentFlagsEntry,
    _settings: &AppSettings,
    log: &dyn Fn(&str),
) -> Vec<TransitionZone> {
    if segment_flags.valid_clusters.is_empty() {
        log("[EDL Builder] No valid clusters -- nothing to build");
        return vec![];
    }

    // Map cluster_id -> ClusterDiagnostic for fast lookup
    let cluster_map: HashMap<i32, &_> = stepping_data
        .clusters
        .iter()
        .map(|c| (c.cluster_id, c))
        .collect();

    // Keep only clusters that passed validation, sorted by time_range start
    let mut valid_ids: Vec<i32> = segment_flags
        .valid_clusters
        .keys()
        .copied()
        .filter(|cid| cluster_map.contains_key(cid))
        .collect();

    valid_ids.sort_by(|a, b| {
        let ta = cluster_map[a].time_range.0;
        let tb = cluster_map[b].time_range.0;
        ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
    });

    if valid_ids.len() < 2 {
        log("[EDL Builder] Only one valid cluster -- no transitions to build");
        return vec![];
    }

    let mut zones: Vec<TransitionZone> = Vec::new();
    for i in 0..valid_ids.len() - 1 {
        let c_before = cluster_map[&valid_ids[i]];
        let c_after = cluster_map[&valid_ids[i + 1]];

        let correction = c_after.mean_delay_ms - c_before.mean_delay_ms;

        let zone = TransitionZone {
            ref_start_s: c_before.time_range.1,
            ref_end_s: c_after.time_range.0,
            delay_before_ms: c_before.mean_delay_ms,
            delay_after_ms: c_after.mean_delay_ms,
            correction_ms: correction,
        };
        log(&format!(
            "[EDL Builder] Transition {}: ref [{:.1}s - {:.1}s]  \
             delay {:+.0}ms -> {:+.0}ms  correction {:+.0}ms",
            i + 1,
            zone.ref_start_s,
            zone.ref_end_s,
            c_before.mean_delay_ms,
            c_after.mean_delay_ms,
            correction,
        ));
        zones.push(zone);
    }

    log(&format!(
        "[EDL Builder] Found {} transition zone(s)",
        zones.len()
    ));
    zones
}

/// Convert refined splice points into an EDL segment list — `build_segments_from_splice_points`
///
/// Parameters:
/// - `anchor_delay_ms`: The delay of the first cluster (anchor).
/// - `anchor_delay_raw`: Raw (unrounded) anchor delay for subtitle precision.
/// - `splice_points`: List of `(src2_time_s, delay_after_ms, delay_after_raw)` tuples, sorted by time.
///
/// Returns sorted EDL with an anchor segment at t=0 followed by one segment per splice point.
pub fn build_segments_from_splice_points(
    anchor_delay_ms: i32,
    anchor_delay_raw: f64,
    splice_points: &[(f64, f64, f64)],
    log: &dyn Fn(&str),
) -> Vec<AudioSegment> {
    let mut edl: Vec<AudioSegment> = vec![AudioSegment {
        start_s: 0.0,
        end_s: 0.0,
        delay_ms: anchor_delay_ms,
        delay_raw: anchor_delay_raw,
        drift_rate_ms_s: 0.0,
    }];

    for &(src2_time_s, delay_after_ms, delay_after_raw) in splice_points {
        edl.push(AudioSegment {
            start_s: src2_time_s,
            end_s: src2_time_s,
            delay_ms: delay_after_ms.round() as i32,
            delay_raw: delay_after_raw,
            drift_rate_ms_s: 0.0,
        });
    }

    edl.sort_by(|a, b| {
        a.start_s
            .partial_cmp(&b.start_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    log(&format!(
        "[EDL Builder] Built EDL with {} segment(s):",
        edl.len()
    ));
    for (i, seg) in edl.iter().enumerate() {
        log(&format!(
            "  Segment {}: @{:.3}s  delay={:+}ms (raw {:.3}ms)",
            i + 1,
            seg.start_s,
            seg.delay_ms,
            seg.delay_raw,
        ));
    }

    edl
}
