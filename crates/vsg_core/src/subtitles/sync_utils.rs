//! Shared utilities for subtitle sync operations — 1:1 port of `sync_utils.py`.
//!
//! Extracts common patterns used across multiple sync plugins to avoid duplication.

use crate::subtitles::data::{SubtitleData, SyncEventData};

/// Apply a flat delay to all non-comment subtitle events.
///
/// Updates each event's start_ms/end_ms and populates SyncEventData metadata.
///
/// Returns the number of events modified (excludes comments).
pub fn apply_delay_to_events(
    subtitle_data: &mut SubtitleData,
    delay_ms: f64,
    snapped_to_frame: bool,
) -> i32 {
    let mut events_synced = 0;

    for event in &mut subtitle_data.events {
        if event.is_comment {
            continue;
        }

        let original_start = event.start_ms;
        let original_end = event.end_ms;

        event.start_ms += delay_ms;
        event.end_ms += delay_ms;

        event.sync = Some(SyncEventData {
            original_start_ms: original_start,
            original_end_ms: original_end,
            start_adjustment_ms: delay_ms,
            end_adjustment_ms: delay_ms,
            snapped_to_frame,
            target_frame_start: None,
            target_frame_end: None,
        });

        events_synced += 1;
    }

    events_synced
}
