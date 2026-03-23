//! Sync mode dispatcher for subtitle processing — 1:1 port of `sync_dispatcher.py`.
//!
//! Coordinates sync mode application with optimizations:
//! - Video-verified caching (avoid redundant frame matching)
//! - Source 1 reference handling (skip frame matching for reference)
//! - Plugin dispatch for all sync modes

use std::collections::HashMap;
use std::path::Path;

use crate::subtitles::data::{OperationResult, SubtitleData};
use crate::subtitles::sync_modes::{get_sync_plugin, SyncParams};
use crate::subtitles::sync_utils::apply_delay_to_events;

/// Information about a subtitle track item needed for sync dispatch.
///
/// This is a simplified view of the Python `ExtractedItem` used by the dispatcher.
pub struct SyncItemInfo {
    /// Source key, e.g. "Source 1", "Source 2"
    pub source_key: String,
    /// Track ID
    pub track_id: i32,
    /// Track source string
    pub track_source: String,
    /// Whether this is a generated track
    pub is_generated: bool,
    /// Sync exclusion styles
    pub sync_exclusion_styles: Vec<String>,
    /// Sync exclusion mode
    pub sync_exclusion_mode: String,
}

/// Context information needed by the sync dispatcher.
pub struct SyncContext {
    /// Raw source delays in ms (from correlation)
    pub raw_source_delays_ms: HashMap<String, f64>,
    /// Subtitle-specific delays in ms (e.g., from video-verified)
    pub subtitle_delays_ms: HashMap<String, f64>,
    /// Raw global shift in ms
    pub raw_global_shift_ms: f64,
    /// Sources that have been video-verified
    pub video_verified_sources: HashMap<String, VideoVerifiedCache>,
    /// Whether this source key should use time_based_use_raw_values
    pub time_based_use_raw_values: bool,
}

/// Cached video-verified result for a source.
#[derive(Debug, Clone)]
pub struct VideoVerifiedCache {
    pub corrected_delay_ms: f64,
    pub fallback: bool,
    pub details: HashMap<String, serde_json::Value>,
}

/// Apply sync mode to subtitle data.
///
/// Handles:
/// - Video-verified caching (use pre-computed delays)
/// - Video-verified Source 1 reference case
/// - Plugin dispatch for all sync modes
pub fn apply_sync_mode(
    item: &SyncItemInfo,
    subtitle_data: &mut SubtitleData,
    ctx: &SyncContext,
    sync_mode: &str,
    log: &dyn Fn(&str),
) -> OperationResult {
    // Get delays
    let total_delay_ms = if let Some(&delay) = ctx.subtitle_delays_ms.get(&item.source_key) {
        delay
    } else if let Some(&delay) = ctx.raw_source_delays_ms.get(&item.source_key) {
        delay
    } else {
        0.0
    };
    let global_shift_ms = ctx.raw_global_shift_ms;

    log(&format!("[Sync] Mode: {sync_mode}"));
    log(&format!(
        "[Sync] Delay: {total_delay_ms:+.3}ms (global: {global_shift_ms:+.3}ms)"
    ));

    // OPTIMIZATION 1: Check if video-verified was already computed for this source
    if sync_mode == "video-verified" {
        if let Some(cached) = ctx.video_verified_sources.get(&item.source_key) {
            return apply_cached_video_verified(
                subtitle_data,
                cached,
                &item.source_key,
                log,
            );
        }
    }

    // OPTIMIZATION 2: For video-verified mode, Source 1 is the reference
    if sync_mode == "video-verified" && item.source_key == "Source 1" {
        return apply_video_verified_reference(
            subtitle_data,
            total_delay_ms,
            log,
        );
    }

    // NORMAL PATH: Use sync plugin
    if let Some(plugin) = get_sync_plugin(sync_mode) {
        log(&format!("[Sync] Using plugin: {}", plugin.name()));

        let mut params = SyncParams::new(total_delay_ms, global_shift_ms);
        params.log = Some(log);
        params.extra.insert(
            "time_based_use_raw_values".to_string(),
            serde_json::json!(ctx.time_based_use_raw_values),
        );

        let result = plugin.apply(subtitle_data, &params);

        if result.success {
            log(&format!("[Sync] {}", result.summary));
        } else {
            log(&format!(
                "[Sync] WARNING: {}",
                result.error.as_deref().unwrap_or("Sync failed")
            ));
        }

        result
    } else {
        // Unknown sync mode
        log(&format!("[Sync] ERROR: Unknown sync mode: {sync_mode}"));
        OperationResult::err("sync", &format!("Unknown sync mode: {sync_mode}"))
    }
}

/// Apply video-verified sync using cached pre-computed delay.
fn apply_cached_video_verified(
    subtitle_data: &mut SubtitleData,
    cached: &VideoVerifiedCache,
    source_key: &str,
    log: &dyn Fn(&str),
) -> OperationResult {
    if cached.fallback {
        log(&format!(
            "[Sync] Using audio correlation fallback for {source_key} (frame matching failed)"
        ));
    } else {
        log(&format!(
            "[Sync] Using pre-computed video-verified delay for {source_key}"
        ));
    }
    log(&format!(
        "[Sync]   Delay: {:+.1}ms",
        cached.corrected_delay_ms
    ));

    // Apply the delay directly to subtitle events (like time-based mode)
    let events_synced = apply_delay_to_events(subtitle_data, cached.corrected_delay_ms, false);

    log(&format!(
        "[Sync] Applied {:+.1}ms to {events_synced} events",
        cached.corrected_delay_ms
    ));

    // Include target_fps in details for surgical rounding at save time
    let details = cached.details.clone();
    // target_fps would be set by caller if available

    let mut result = OperationResult::ok("sync");
    result.events_affected = events_synced;
    result.summary = if cached.fallback {
        format!(
            "Audio correlation fallback: {:+.1}ms applied to {events_synced} events",
            cached.corrected_delay_ms
        )
    } else {
        format!(
            "Video-verified (pre-computed): {:+.1}ms applied to {events_synced} events",
            cached.corrected_delay_ms
        )
    };
    result.details = details;
    result
}

/// Apply video-verified for Source 1 (reference video).
///
/// Source 1 is the reference, so no frame matching is needed.
/// Just apply the delay directly (which is just global_shift for Source 1).
fn apply_video_verified_reference(
    subtitle_data: &mut SubtitleData,
    total_delay_ms: f64,
    log: &dyn Fn(&str),
) -> OperationResult {
    log("[Sync] Source 1 is reference - applying delay directly without frame matching");

    let events_synced = apply_delay_to_events(subtitle_data, total_delay_ms, false);

    log(&format!(
        "[Sync] Applied {total_delay_ms:+.1}ms to {events_synced} events (reference)"
    ));

    let mut result = OperationResult::ok("sync");
    result.events_affected = events_synced;
    result.summary = format!(
        "Video-verified (Source 1 reference): {total_delay_ms:+.1}ms applied to {events_synced} events"
    );
    result
}

/// Build a unique label for a track (used in audit report filenames).
pub fn build_track_label(track_id: i32, track_source: &str, is_generated: bool) -> String {
    let source = track_source.replace(' ', "");
    if is_generated {
        format!("{source}_t{track_id}_gen")
    } else {
        format!("{source}_t{track_id}")
    }
}

/// Build a unique job name for frame audit reports.
///
/// Combines target video stem with track label to prevent
/// reports from different tracks overwriting each other.
pub fn build_audit_job_name(
    target_video: Option<&Path>,
    track_id: i32,
    track_source: &str,
    is_generated: bool,
) -> String {
    let video_stem = target_video
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let track_label = build_track_label(track_id, track_source, is_generated);
    format!("{video_stem}_{track_label}")
}
