//! Frame candidate generation and checkpoint selection for video-verified sync.
//!
//! 1:1 port of `video_verified/candidates.py`.

use std::collections::BTreeSet;

/// Generate candidate frame offsets to test, centered on the correlation value.
///
/// Works for any offset size - small (< 3 frames) or large (24+ frames).
/// Searches in a window around the correlation-derived frame offset.
///
/// # Arguments
/// * `correlation_frames` - Audio correlation converted to frames (can be fractional)
/// * `search_range_frames` - How many frames on each side to search
///
/// # Returns
/// Sorted list of integer frame offsets to test
pub fn generate_frame_candidates(
    correlation_frames: f64,
    search_range_frames: i32,
) -> Vec<i64> {
    let mut candidates = BTreeSet::new();

    // Round correlation to nearest frame
    let base_frame = correlation_frames.round() as i64;

    // Always include zero (in case correlation is just wrong)
    candidates.insert(0i64);

    // Search window around correlation
    for delta in -search_range_frames..=search_range_frames {
        candidates.insert(base_frame + delta as i64);
    }

    candidates.into_iter().collect()
}

/// Select checkpoint times evenly distributed across the video.
///
/// Places checkpoints at evenly-spaced intervals within the middle 80%
/// of the video (10% to 90%), avoiding the very start and end where
/// intros/outros may differ between sources.
///
/// For 9 checkpoints this produces: [10%, 20%, 30%, 40%, 50%, 60%, 70%, 80%, 90%]
pub fn select_checkpoint_times(duration_ms: f64, num_checkpoints: usize) -> Vec<f64> {
    let mut checkpoints = Vec::with_capacity(num_checkpoints);

    let margin_pct = 10.0;
    let start_pct = margin_pct;
    let end_pct = 100.0 - margin_pct;
    let span_pct = end_pct - start_pct; // 80

    for i in 0..num_checkpoints {
        // Center each checkpoint in its segment
        let pos = start_pct + span_pct * (i as f64 + 0.5) / num_checkpoints as f64;
        let time_ms = duration_ms * pos / 100.0;
        checkpoints.push(time_ms);
    }

    checkpoints
}
