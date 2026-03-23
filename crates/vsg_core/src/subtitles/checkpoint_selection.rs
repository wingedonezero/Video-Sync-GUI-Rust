//! Smart checkpoint selection for subtitle sync verification — 1:1 port of `checkpoint_selection.py`.
//!
//! Selects representative dialogue events while avoiding OP/ED sequences.

use crate::subtitles::data::SubtitleEvent;

/// Smart checkpoint selection: avoid OP/ED, prefer dialogue events.
///
/// Strategy:
/// - Filter out first/last 2 minutes (OP/ED likely)
/// - Prefer longer duration events (likely dialogue, not signs)
/// - Use repeatable selection based on event count
/// - Return 3 checkpoints: early (1/6), middle (1/2), late (5/6)
pub fn select_smart_checkpoints<'a>(
    subtitle_events: &'a [SubtitleEvent],
    log: &dyn Fn(&str),
) -> Vec<&'a SubtitleEvent> {
    let total_events = subtitle_events.len();
    if total_events == 0 {
        return Vec::new();
    }

    // Calculate video duration to determine safe zones
    let first_start = subtitle_events[0].start_ms;
    let last_end = subtitle_events[total_events - 1].end_ms;
    let duration_ms = last_end - first_start;

    // Define safe zone: skip first/last 2 minutes (120000ms)
    let op_zone_ms: f64 = 120000.0;
    let ed_zone_ms: f64 = 120000.0;

    let mut safe_start_ms = first_start + op_zone_ms;
    let mut safe_end_ms = last_end - ed_zone_ms;

    // If video is too short, just use middle third
    if duration_ms < (op_zone_ms + ed_zone_ms) {
        safe_start_ms = first_start + (duration_ms / 3.0);
        safe_end_ms = last_end - (duration_ms / 3.0);
    }

    // Filter events in safe zone
    let safe_events: Vec<usize> = subtitle_events
        .iter()
        .enumerate()
        .filter(|(_, e)| safe_start_ms <= e.start_ms && e.start_ms <= safe_end_ms)
        .map(|(i, _)| i)
        .collect();

    let safe_indices = if safe_events.len() < 3 {
        // Not enough safe events, fall back to middle third of all events
        let start_idx = total_events / 3;
        let end_idx = 2 * total_events / 3;
        log("[Checkpoint Selection] Using middle third (not enough events in safe zone)");
        (start_idx..end_idx).collect::<Vec<_>>()
    } else {
        safe_events
    };

    if safe_indices.is_empty() {
        // Last resort: use first/mid/last of all events
        return vec![
            &subtitle_events[0],
            &subtitle_events[total_events / 2],
            &subtitle_events[total_events - 1],
        ];
    }

    // Prefer longer duration events (dialogue over signs)
    // Sort by duration descending, take top 40%
    let mut by_duration: Vec<usize> = safe_indices.clone();
    by_duration.sort_by(|&a, &b| {
        let dur_a = subtitle_events[a].end_ms - subtitle_events[a].start_ms;
        let dur_b = subtitle_events[b].end_ms - subtitle_events[b].start_ms;
        dur_b
            .partial_cmp(&dur_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_count = 3.max(by_duration.len() * 40 / 100);
    let mut top_events: Vec<usize> = by_duration[..top_count.min(by_duration.len())].to_vec();

    // Sort these back by start time for temporal ordering
    top_events.sort_by(|&a, &b| {
        subtitle_events[a]
            .start_ms
            .partial_cmp(&subtitle_events[b].start_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let checkpoints: Vec<&SubtitleEvent> = if top_events.len() >= 3 {
        // Pick early (1/6), middle (1/2), late (5/6)
        let early = top_events[top_events.len() / 6];
        let middle = top_events[top_events.len() / 2];
        let late = top_events[5 * top_events.len() / 6];
        vec![
            &subtitle_events[early],
            &subtitle_events[middle],
            &subtitle_events[late],
        ]
    } else {
        top_events
            .iter()
            .map(|&i| &subtitle_events[i])
            .collect()
    };

    log(&format!(
        "[Checkpoint Selection] Selected {} dialogue events:",
        checkpoints.len()
    ));
    for (i, e) in checkpoints.iter().enumerate() {
        let duration = e.end_ms - e.start_ms;
        log(&format!(
            "  {}. Time: {}ms, Duration: {}ms",
            i + 1,
            e.start_ms as i64,
            duration as i64
        ));
    }

    checkpoints
}
