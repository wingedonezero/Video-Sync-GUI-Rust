//! Surgical frame-aware rounding for subtitle timestamps.
//!
//! When floor rounding to centiseconds (10ms precision) would land a timestamp
//! on the wrong frame, uses ceil instead. Only adjusts timestamps that need it;
//! all others remain identical to plain floor behavior.
//!
//! Algorithm:
//! 1. Floor is default (matches Aegisub and ASS convention)
//! 2. Check if floor lands on the correct frame
//! 3. If not, use ceil (minimal adjustment: +10ms)
//! 4. Coordinate end with start to preserve duration when safe
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/surgical_rounding.py`.

use crate::subtitles::data::SubtitleEvent;

const EPSILON: f64 = 1e-6;

/// Result of surgical rounding for a single timestamp.
#[derive(Debug, Clone)]
pub struct SurgicalRoundResult {
    /// Final rounded value in ms (centisecond-aligned)
    pub centisecond_ms: i64,
    /// True if ceil was used instead of floor
    pub was_adjusted: bool,
    /// Frame the exact time maps to
    pub target_frame: i64,
    /// Frame floor rounding would produce
    pub floor_frame: i64,
    /// "floor", "ceil", or "coordinated_ceil"
    pub method: String,
}

/// Result of surgical rounding for a single event (start + end).
#[derive(Debug, Clone)]
pub struct SurgicalEventResult {
    pub start: SurgicalRoundResult,
    pub end: SurgicalRoundResult,
    /// Output duration equals floor-floor duration
    pub duration_preserved: bool,
    /// End was coordinated with start
    pub coordination_applied: bool,
}

/// Aggregate statistics from surgical rounding across all events.
#[derive(Debug, Clone, Default)]
pub struct SurgicalBatchStats {
    pub total_events: i64,
    /// events * 2 (start + end)
    pub total_timing_points: i64,
    pub starts_adjusted: i64,
    pub ends_adjusted: i64,
    /// Subset of ends_adjusted done via coordination
    pub ends_coordinated: i64,
    pub durations_preserved: i64,
    pub durations_changed: i64,
    pub points_identical_to_floor: i64,
    pub points_different_from_floor: i64,
    /// Events where at least one point changed
    pub events_with_adjustments: i64,
}

/// Convert time to frame number (floor with epsilon protection).
fn time_to_frame(time_ms: f64, frame_duration_ms: f64) -> i64 {
    ((time_ms + EPSILON) / frame_duration_ms) as i64
}

/// Surgically round a single timestamp to centiseconds.
///
/// Uses floor by default. Only switches to ceil when floor
/// would land on the wrong frame.
///
/// # Arguments
/// * `exact_ms` - Exact time in milliseconds (float, after offset applied)
/// * `frame_duration_ms` - Duration of one frame in milliseconds
pub fn surgical_round_single(exact_ms: f64, frame_duration_ms: f64) -> SurgicalRoundResult {
    let target_frame = time_to_frame(exact_ms, frame_duration_ms);

    // Try floor first (current default behavior)
    let floor_cs = (exact_ms / 10.0).floor() as i64 * 10;
    let floor_frame = time_to_frame(floor_cs as f64, frame_duration_ms);

    if floor_frame == target_frame {
        return SurgicalRoundResult {
            centisecond_ms: floor_cs,
            was_adjusted: false,
            target_frame,
            floor_frame,
            method: "floor".to_string(),
        };
    }

    // Floor failed - try ceil
    let ceil_cs = (exact_ms / 10.0).ceil() as i64 * 10;
    if time_to_frame(ceil_cs as f64, frame_duration_ms) == target_frame {
        return SurgicalRoundResult {
            centisecond_ms: ceil_cs,
            was_adjusted: true,
            target_frame,
            floor_frame,
            method: "ceil".to_string(),
        };
    }

    // Fallback: ceil of frame start boundary
    let frame_start = target_frame as f64 * frame_duration_ms;
    let fallback_cs = (frame_start / 10.0).ceil() as i64 * 10;
    SurgicalRoundResult {
        centisecond_ms: fallback_cs,
        was_adjusted: true,
        target_frame,
        floor_frame,
        method: "ceil".to_string(),
    }
}

/// Surgically round an event's start and end with coordination.
///
/// Coordination rule: If start was adjusted (floor->ceil) and end's floor
/// is already correct, try ceil for end too. Use ceil(end) only if it:
/// 1. Still maps to the correct frame
/// 2. Preserves the original floor-floor duration
///
/// This prevents unnecessary duration changes caused by adjusting only
/// one side of the event.
pub fn surgical_round_event(
    start_ms: f64,
    end_ms: f64,
    frame_duration_ms: f64,
) -> SurgicalEventResult {
    // Round start
    let start_result = surgical_round_single(start_ms, frame_duration_ms);

    // Round end independently first
    let mut end_result = surgical_round_single(end_ms, frame_duration_ms);

    // Coordination: if start was adjusted and end was NOT adjusted
    let mut coordination_applied = false;
    if start_result.was_adjusted && !end_result.was_adjusted {
        // What would floor-floor duration have been?
        let floor_start = (start_ms / 10.0).floor() as i64 * 10;
        let floor_end = (end_ms / 10.0).floor() as i64 * 10;
        let original_floor_duration = floor_end - floor_start;

        // Try ceil for end too
        let ceil_end = (end_ms / 10.0).ceil() as i64 * 10;
        let end_target_frame = time_to_frame(end_ms, frame_duration_ms);

        if time_to_frame(ceil_end as f64, frame_duration_ms) == end_target_frame {
            // Ceil end is on correct frame -- check duration
            let coordinated_duration = ceil_end - start_result.centisecond_ms;
            if coordinated_duration == original_floor_duration {
                end_result = SurgicalRoundResult {
                    centisecond_ms: ceil_end,
                    was_adjusted: true,
                    target_frame: end_target_frame,
                    floor_frame: end_result.floor_frame,
                    method: "coordinated_ceil".to_string(),
                };
                coordination_applied = true;
            }
        }
    }

    // Check duration preservation against floor-floor baseline
    let floor_start = (start_ms / 10.0).floor() as i64 * 10;
    let floor_end = (end_ms / 10.0).floor() as i64 * 10;
    let floor_duration = floor_end - floor_start;
    let output_duration = end_result.centisecond_ms - start_result.centisecond_ms;
    let duration_preserved = output_duration == floor_duration;

    SurgicalEventResult {
        start: start_result,
        end: end_result,
        duration_preserved,
        coordination_applied,
    }
}

/// Apply surgical rounding to all non-comment events.
///
/// Returns (results_by_index, aggregate_stats).
/// results_by_index maps event index -> SurgicalEventResult
/// (only for non-comment events that were analyzed).
pub fn surgical_round_batch(
    events: &[SubtitleEvent],
    frame_duration_ms: f64,
) -> (std::collections::HashMap<usize, SurgicalEventResult>, SurgicalBatchStats) {
    let mut results: std::collections::HashMap<usize, SurgicalEventResult> =
        std::collections::HashMap::new();
    let mut stats = SurgicalBatchStats::default();

    for (idx, event) in events.iter().enumerate() {
        if event.is_comment {
            continue;
        }

        stats.total_events += 1;
        stats.total_timing_points += 2;

        let result = surgical_round_event(event.start_ms, event.end_ms, frame_duration_ms);
        results.insert(idx, result.clone());

        if result.start.was_adjusted {
            stats.starts_adjusted += 1;
        }
        if result.end.was_adjusted {
            stats.ends_adjusted += 1;
        }
        if result.coordination_applied {
            stats.ends_coordinated += 1;
        }
        if result.duration_preserved {
            stats.durations_preserved += 1;
        } else {
            stats.durations_changed += 1;
        }

        // Track identity with floor
        if result.start.was_adjusted {
            stats.points_different_from_floor += 1;
        } else {
            stats.points_identical_to_floor += 1;
        }
        if result.end.was_adjusted {
            stats.points_different_from_floor += 1;
        } else {
            stats.points_identical_to_floor += 1;
        }

        if result.start.was_adjusted || result.end.was_adjusted {
            stats.events_with_adjustments += 1;
        }
    }

    (results, stats)
}
