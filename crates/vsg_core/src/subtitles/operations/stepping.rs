//! Stepping operation for SubtitleData — 1:1 port of `operations/stepping.py`.
//!
//! Adjusts subtitle timestamps based on EDL (Edit Decision List) from audio stepping.
//! When audio undergoes stepping correction, subtitles need matching adjustments.
//!
//! NOTE: This operation needs refactoring/testing. The feature doesn't work correctly
//! in all cases. The boundary mode handling (start/midpoint/majority) and cumulative
//! offset calculation may need review.

use std::collections::HashMap;

use chrono::Local;

use crate::subtitles::data::{OperationRecord, OperationResult, SubtitleData};

/// Represents an audio segment from the EDL.
///
/// Generic trait so any struct with the right fields can be used.
pub trait AudioSegment {
    fn start_s(&self) -> f64;
    fn delay_ms(&self) -> f64;
    fn delay_raw(&self) -> f64;
}

/// Simple AudioSegment implementation for use in stepping.
#[derive(Debug, Clone)]
pub struct EdlSegment {
    pub start_s: f64,
    pub delay_ms: f64,
    pub delay_raw: f64,
}

impl AudioSegment for EdlSegment {
    fn start_s(&self) -> f64 {
        self.start_s
    }
    fn delay_ms(&self) -> f64 {
        self.delay_ms
    }
    fn delay_raw(&self) -> f64 {
        self.delay_raw
    }
}

/// Apply stepping correction EDL to subtitle timestamps.
///
/// The EDL contains AudioSegment entries that define delay changes across the timeline.
/// For each subtitle, we calculate the cumulative offset at its timestamp and shift it.
pub fn apply_stepping(
    data: &mut SubtitleData,
    edl_segments: &[EdlSegment],
    boundary_mode: &str,
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    // Validate EDL
    if edl_segments.is_empty() {
        log_msg("[Stepping] No EDL provided, skipping");
        let mut result = OperationResult::ok("stepping");
        result.summary = "No EDL provided".to_string();
        return result;
    }

    // Sort EDL by start time
    let mut sorted_edl: Vec<&EdlSegment> = edl_segments.iter().collect();
    sorted_edl.sort_by(|a, b| a.start_s.partial_cmp(&b.start_s).unwrap_or(std::cmp::Ordering::Equal));

    // Counters
    let mut adjusted_count = 0;
    let mut max_adjustment_ms: f64 = 0.0;
    let mut spanning_count = 0;

    // Process each event
    for event in &mut data.events {
        let start_s = event.start_ms / 1000.0;
        let end_s = event.end_ms / 1000.0;

        // Calculate cumulative offset (raw float ms)
        let offset_ms = get_offset_at_time(start_s, end_s, &sorted_edl, boundary_mode);

        // Check if spans boundary
        if spans_boundary(start_s, end_s, &sorted_edl) {
            spanning_count += 1;
        }

        // Apply offset (keep as float - no rounding)
        if offset_ms != 0.0 {
            event.start_ms += offset_ms;
            event.end_ms += offset_ms;
            adjusted_count += 1;
            max_adjustment_ms = max_adjustment_ms.max(offset_ms.abs());
        }
    }

    // Record operation
    let record = OperationRecord {
        operation: "stepping".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!({
            "edl_segments": sorted_edl.len(),
            "boundary_mode": boundary_mode,
        }),
        events_affected: adjusted_count,
        styles_affected: 0,
        summary: format!(
            "Adjusted {adjusted_count}/{} events, max {max_adjustment_ms:+.1}ms",
            data.events.len()
        ),
    };
    data.operations.push(record.clone());

    log_msg(&format!(
        "[Stepping] Adjusted {adjusted_count}/{} events using '{boundary_mode}' mode",
        data.events.len()
    ));
    log_msg(&format!("[Stepping] Max adjustment: {max_adjustment_ms:+.1}ms"));
    if spanning_count > 0 {
        log_msg(&format!(
            "[Stepping] {spanning_count} event(s) span stepping boundaries"
        ));
    }

    let mut result = OperationResult::ok("stepping");
    result.events_affected = adjusted_count;
    result.summary = record.summary;
    result.details.insert(
        "max_adjustment_ms".to_string(),
        serde_json::json!(max_adjustment_ms),
    );
    result.details.insert(
        "spanning_boundaries".to_string(),
        serde_json::json!(spanning_count),
    );
    result.details.insert(
        "edl_segments".to_string(),
        serde_json::json!(sorted_edl.len()),
    );
    result
}

/// Check if subtitle spans a stepping boundary.
fn spans_boundary(start_s: f64, end_s: f64, edl: &[&EdlSegment]) -> bool {
    if edl.len() <= 1 {
        return false;
    }
    edl[1..].iter().any(|segment| start_s < segment.start_s && segment.start_s < end_s)
}

/// Calculate cumulative offset (in float ms) for a subtitle.
fn get_offset_at_time(
    start_s: f64,
    end_s: f64,
    edl: &[&EdlSegment],
    mode: &str,
) -> f64 {
    let get_cumulative_offset_at_time = |time_s: f64| -> f64 {
        if time_s < edl[0].start_s {
            return 0.0;
        }

        let mut cumulative_offset = 0.0;
        let mut base_delay = edl[0].delay_raw;

        for segment in &edl[1..] {
            if segment.start_s <= time_s {
                let segment_delay_raw = segment.delay_raw;
                cumulative_offset += segment_delay_raw - base_delay;
                base_delay = segment_delay_raw;
            } else {
                break;
            }
        }

        cumulative_offset
    };

    match mode {
        "start" => get_cumulative_offset_at_time(start_s),

        "midpoint" => {
            let midpoint_s = (start_s + end_s) / 2.0;
            get_cumulative_offset_at_time(midpoint_s)
        }

        "majority" => {
            let duration = end_s - start_s;
            if duration <= 0.0 {
                return get_cumulative_offset_at_time(start_s);
            }

            // Track duration in each delay region
            let mut region_durations: HashMap<i64, f64> = HashMap::new();

            // Build boundaries within subtitle range
            let mut boundaries: Vec<f64> = edl
                .iter()
                .map(|seg| seg.start_s)
                .chain(std::iter::once(end_s))
                .filter(|&b| start_s <= b && b <= end_s)
                .collect();
            boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            boundaries.dedup();

            if boundaries.is_empty()
                || (boundaries.len() == 1 && (boundaries[0] - end_s).abs() < f64::EPSILON)
            {
                return get_cumulative_offset_at_time(start_s);
            }

            // Calculate duration in each region
            let mut current_time = start_s;
            for boundary in &boundaries {
                if *boundary <= start_s {
                    continue;
                }

                let region_delay = get_cumulative_offset_at_time(current_time);
                let segment_duration = boundary.min(end_s) - current_time;

                // Use i64 key for float comparison (sufficient precision)
                let key = (region_delay * 1000.0) as i64;
                *region_durations.entry(key).or_insert(0.0) += segment_duration;

                current_time = *boundary;
                if current_time >= end_s {
                    break;
                }
            }

            // Return delay with most duration
            if !region_durations.is_empty() {
                let best_key = region_durations
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(k, _)| *k)
                    .unwrap_or(0);
                return best_key as f64 / 1000.0;
            }

            get_cumulative_offset_at_time(start_s)
        }

        // Unknown mode, default to start
        _ => get_cumulative_offset_at_time(start_s),
    }
}
