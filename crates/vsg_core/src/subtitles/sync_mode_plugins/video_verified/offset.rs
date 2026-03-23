//! Sub-frame offset calculation and VFR frame lookup for video-verified sync.
//!
//! 1:1 port of `video_verified/offset.py`.

use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::subtitles::frame_utils::video_reader::VideoReader;

/// Cache for VFR timestamps (expensive to create).
static VFR_TIMESTAMPS_CACHE: Lazy<Mutex<HashMap<String, bool>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Track which videos we've logged VFR usage for.
static VFR_LOGGED_VIDEOS: Lazy<Mutex<std::collections::HashSet<String>>> =
    Lazy::new(|| Mutex::new(std::collections::HashSet::new()));

/// Get frame number for a given time using VFR timestamps.
///
/// For soft-telecine sources, this would use VideoTimestamps to get
/// accurate frame numbers. In the Rust port, we fall back to CFR
/// calculation since we don't have the VideoTimestamps library.
pub fn get_vfr_frame_for_time(
    _video_path: &str,
    _time_ms: f64,
    is_soft_telecine: bool,
    _log: Option<&dyn Fn(&str)>,
) -> Option<i64> {
    if !is_soft_telecine {
        return None;
    }

    // In the Python version, this uses VideoTimestamps.from_video_file()
    // for VFR content. In Rust, we don't have this library, so we fall
    // back to CFR calculation (caller will handle this).
    None
}

/// Calculate the final offset in milliseconds.
///
/// By default, uses simple frame-based calculation:
///     offset_ms = frame_offset * frame_duration_ms
///
/// Optionally, can use PTS-based calculation for VFR content.
pub fn calculate_subframe_offset(
    frame_offset: i64,
    match_details: &[HashMap<String, serde_json::Value>],
    _checkpoint_times: &[f64],
    source_reader: &VideoReader,
    target_reader: &VideoReader,
    _fps: f64,
    frame_duration_ms: f64,
    log: &dyn Fn(&str),
    use_pts_precision: bool,
) -> f64 {
    // Default: simple frame-based calculation
    let frame_based_offset = frame_offset as f64 * frame_duration_ms;

    if !use_pts_precision {
        log(&format!(
            "[VideoVerified] Frame-based offset: {:+} frames = {:+.3}ms",
            frame_offset, frame_based_offset
        ));
        return frame_based_offset;
    }

    // PTS precision mode - use actual container timestamps
    log("[VideoVerified] Using PTS precision mode");

    // Prioritize sequence-verified matches
    let sequence_verified_matches: Vec<_> = match_details
        .iter()
        .filter(|m| {
            m.get("sequence_verified")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .collect();

    let good_matches = if !sequence_verified_matches.is_empty() {
        log(&format!(
            "[VideoVerified] Using {} sequence-verified checkpoints for PTS calculation",
            sequence_verified_matches.len()
        ));
        sequence_verified_matches
    } else {
        let single_matches: Vec<_> = match_details
            .iter()
            .filter(|m| {
                m.get("is_match")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .collect();
        if !single_matches.is_empty() {
            log(&format!(
                "[VideoVerified] No sequence-verified matches, using {} single-frame matches",
                single_matches.len()
            ));
        }
        single_matches
    };

    if good_matches.is_empty() {
        log(&format!(
            "[VideoVerified] No good matches for PTS, using frame-based: {:+.3}ms",
            frame_based_offset
        ));
        return frame_based_offset;
    }

    // Calculate offset from each matched pair using PTS
    let mut pts_offsets = Vec::new();

    for m in &good_matches {
        let source_idx = m
            .get("source_frame")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let target_idx = m
            .get("target_frame")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if let (Some(source_pts), Some(target_pts)) = (
            source_reader.get_frame_pts(source_idx),
            target_reader.get_frame_pts(target_idx),
        ) {
            let offset = target_pts - source_pts;
            pts_offsets.push(offset);
            log(&format!(
                "[VideoVerified]   Frame {}->{}:  PTS {:.3}ms->{:.3}ms = {:+.3}ms",
                source_idx, target_idx, source_pts, target_pts, offset
            ));
        }
    }

    if pts_offsets.is_empty() {
        log(&format!(
            "[VideoVerified] PTS lookup failed, using frame-based: {:+.3}ms",
            frame_based_offset
        ));
        return frame_based_offset;
    }

    // Use median offset (robust to outliers)
    pts_offsets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_idx = pts_offsets.len() / 2;
    let sub_frame_offset = if pts_offsets.len() % 2 == 0 {
        (pts_offsets[median_idx - 1] + pts_offsets[median_idx]) / 2.0
    } else {
        pts_offsets[median_idx]
    };

    log(&format!(
        "[VideoVerified] PTS-based offset from {} pairs: {:+.3}ms",
        pts_offsets.len(),
        sub_frame_offset
    ));

    sub_frame_offset
}
