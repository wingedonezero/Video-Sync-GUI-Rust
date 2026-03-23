//! Subtitle track processing pipeline — 1:1 port of `track_processor.py`.
//!
//! Processes a single subtitle track through the unified SubtitleData flow:
//! 1. Load into SubtitleData (or use provided from OCR)
//! 2. Apply style filtering (if generated track)
//! 3. Apply stepping
//! 4. Apply sync mode
//! 5. Apply style operations (font, patch, rescale, size)
//! 6. Save JSON + ASS/SRT (single rounding point)
//!
//! All operations modify SubtitleData in place and return OperationResult.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::subtitles::data::{OperationResult, SubtitleData};
use crate::subtitles::diagnostics::{
    check_timestamp_precision, parse_ass_time_str, read_raw_ass_timestamps,
};
use crate::subtitles::operations::stepping::EdlSegment;
use crate::subtitles::operations::style_ops;
use crate::subtitles::sync_dispatcher::{
    apply_sync_mode, SyncContext, SyncItemInfo,
};

/// Configuration for processing a subtitle track.
pub struct TrackProcessConfig {
    /// Path to the extracted subtitle file
    pub extracted_path: PathBuf,
    /// Track ID
    pub track_id: i32,
    /// Track source string (e.g. "Source 1")
    pub track_source: String,
    /// Source key for sync (may differ from track_source for external subs)
    pub source_key: String,
    /// Whether this is a generated (split) track
    pub is_generated: bool,

    /// Sync mode (e.g. "time-based", "video-verified")
    pub sync_mode: String,
    /// Whether to convert SRT to ASS
    pub convert_to_ass: bool,
    /// Whether stepping was already applied
    pub stepping_adjusted: bool,
    /// Whether frame-level adjustments were applied
    pub frame_adjusted: bool,

    /// Style filter config
    pub filter_config: Option<FilterConfig>,
    /// Font replacements (old_name -> new_name)
    pub font_replacements: HashMap<String, String>,
    /// Style patches (style_name -> {attr -> value})
    pub style_patch: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Whether to rescale
    pub rescale: bool,
    /// Size multiplier
    pub size_multiplier: f64,

    /// Sync exclusion styles
    pub sync_exclusion_styles: Vec<String>,
    /// Sync exclusion mode
    pub sync_exclusion_mode: String,

    /// Subtitle rounding mode (floor, ceil, round)
    pub rounding_mode: String,
    /// Temp directory for JSON output
    pub temp_dir: PathBuf,
}

/// Style filter configuration.
pub struct FilterConfig {
    pub filter_styles: Vec<String>,
    pub filter_mode: String,
    pub forced_include: Vec<usize>,
    pub forced_exclude: Vec<usize>,
}

/// Result of processing a subtitle track.
pub struct TrackProcessResult {
    /// Updated output path
    pub output_path: PathBuf,
    /// Whether stepping was applied
    pub stepping_adjusted: bool,
    /// Whether frame-level sync was applied
    pub frame_adjusted: bool,
    /// Sync result details
    pub sync_result: Option<OperationResult>,
    /// Number of operations applied
    pub operations_count: usize,
    /// Clamping info for negative timestamps
    pub clamping_info: Option<ClampingInfo>,
}

/// Info about clamped negative timestamps.
pub struct ClampingInfo {
    pub events_clamped: usize,
    pub delay_ms: f64,
    pub min_time_ms: f64,
    pub max_time_ms: f64,
}

/// Process a subtitle track using the unified SubtitleData flow.
///
/// 1. Load into SubtitleData (or use provided SubtitleData from OCR)
/// 2. Apply stepping (if applicable)
/// 3. Apply sync mode
/// 4. Apply style operations
/// 5. Save (single rounding point)
#[allow(clippy::too_many_arguments)]
pub fn process_subtitle_track(
    config: &mut TrackProcessConfig,
    sync_ctx: &SyncContext,
    _source1_file: Option<&Path>,
    ocr_subtitle_data: Option<SubtitleData>,
    stepping_edl: Option<&[EdlSegment]>,
    stepping_boundary_mode: &str,
    stepping_adjust_subtitles: bool,
    target_resolution: Option<(i32, i32)>,
    log: &dyn Fn(&str),
) -> Result<TrackProcessResult, String> {
    // ================================================================
    // STEP 1: Load into SubtitleData (or use provided)
    // ================================================================
    let mut subtitle_data = if let Some(data) = ocr_subtitle_data {
        log(&format!(
            "[SubtitleData] Using OCR SubtitleData for track {}",
            config.track_id
        ));
        log(&format!(
            "[SubtitleData] {} events, OCR metadata preserved",
            data.events.len()
        ));
        data
    } else {
        log(&format!(
            "[SubtitleData] Loading track {}: {}",
            config.track_id,
            config.extracted_path.file_name().unwrap_or_default().to_string_lossy()
        ));

        // DIAGNOSTIC: Read raw timestamps from original file BEFORE parsing
        let mut raw_timestamps_before = Vec::new();
        let ext = config
            .extracted_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext == "ass" || ext == "ssa" {
            raw_timestamps_before =
                read_raw_ass_timestamps(&config.extracted_path, 3);
            if !raw_timestamps_before.is_empty() {
                log("[DIAG] Original file first 3 event timestamps:");
                for (i, (start_str, end_str, style)) in raw_timestamps_before.iter().enumerate() {
                    let start_ms = parse_ass_time_str(start_str);
                    let end_ms = parse_ass_time_str(end_str);
                    let start_precision = check_timestamp_precision(start_str);
                    log(&format!(
                        "[DIAG]   Event {i}: start='{start_str}'({start_ms}ms) end='{end_str}'({end_ms}ms) style='{style}'"
                    ));

                    if start_precision != 2 {
                        log(&format!(
                            "[DIAG] WARNING: Non-standard timestamp precision detected! \
                             Found {start_precision} fractional digits (expected 2 for centiseconds)"
                        ));
                        log("[DIAG] This could cause timing loss during load/save cycle!");
                    }
                }
            }
        }

        let data = SubtitleData::from_file(&config.extracted_path)?;
        log(&format!(
            "[SubtitleData] Loaded {} events, {} styles",
            data.events.len(),
            data.styles.len()
        ));

        // DIAGNOSTIC: Compare parsed timestamps with raw file timestamps
        if !raw_timestamps_before.is_empty() && !data.events.is_empty() {
            log("[DIAG] Parsed SubtitleData first 3 event timestamps:");
            for (i, event) in data.events.iter().take(3).enumerate() {
                log(&format!(
                    "[DIAG]   Event {i}: start={}ms end={}ms style='{}'",
                    event.start_ms, event.end_ms, event.style
                ));
            }

            for (i, (start_str, end_str, _)) in raw_timestamps_before
                .iter()
                .take(3.min(data.events.len()))
                .enumerate()
            {
                let raw_start = parse_ass_time_str(start_str);
                let raw_end = parse_ass_time_str(end_str);
                let parsed_start = data.events[i].start_ms;
                let parsed_end = data.events[i].end_ms;
                if (raw_start - parsed_start).abs() > 0.001
                    || (raw_end - parsed_end).abs() > 0.001
                {
                    log(&format!(
                        "[DIAG] WARNING: Timestamp mismatch at event {i}!"
                    ));
                    log(&format!(
                        "[DIAG]   Raw: start={raw_start}ms, end={raw_end}ms"
                    ));
                    log(&format!(
                        "[DIAG]   Parsed: start={parsed_start}ms, end={parsed_end}ms"
                    ));
                }
            }
        }

        data
    };

    let mut stepping_adjusted = config.stepping_adjusted;
    let mut frame_adjusted = config.frame_adjusted;
    let mut clamping_info = None;

    // ================================================================
    // STEP 1b: Apply Style Filtering (for generated tracks)
    // ================================================================
    if let Some(ref filter_cfg) = config.filter_config {
        if config.is_generated
            && (!filter_cfg.filter_styles.is_empty()
                || !filter_cfg.forced_include.is_empty()
                || !filter_cfg.forced_exclude.is_empty())
        {
            log(&format!(
                "[SubtitleData] Applying style filter for generated track \
                 (forced keep: {}, forced remove: {})...",
                filter_cfg.forced_include.len(),
                filter_cfg.forced_exclude.len()
            ));
            let result = style_ops::apply_style_filter(
                &mut subtitle_data,
                &filter_cfg.filter_styles,
                &filter_cfg.filter_mode,
                Some(&filter_cfg.forced_include),
                Some(&filter_cfg.forced_exclude),
                Some(log),
            );
            if result.success {
                log(&format!("[SubtitleData] Style filter: {}", result.summary));
                if let Some(missing) = result.details.get("styles_missing") {
                    if let Some(arr) = missing.as_array() {
                        if !arr.is_empty() {
                            let names: Vec<_> = arr
                                .iter()
                                .filter_map(|v| v.as_str())
                                .collect();
                            log(&format!(
                                "[SubtitleData] WARNING: Filter styles not found: {}",
                                names.join(", ")
                            ));
                        }
                    }
                }
            } else {
                log(&format!(
                    "[SubtitleData] Style filter failed: {}",
                    result.error.as_deref().unwrap_or("unknown")
                ));
            }
        }
    }

    // ================================================================
    // STEP 2: Apply Stepping (if applicable)
    // ================================================================
    if stepping_adjust_subtitles {
        if let Some(edl) = stepping_edl {
            log("[SubtitleData] Applying stepping correction...");

            let result = crate::subtitles::operations::stepping::apply_stepping(
                &mut subtitle_data,
                edl,
                stepping_boundary_mode,
                Some(log),
            );

            if result.success {
                log(&format!("[SubtitleData] Stepping: {}", result.summary));
                if result.events_affected > 0 {
                    stepping_adjusted = true;
                }
            } else {
                log(&format!(
                    "[SubtitleData] Stepping failed: {}",
                    result.error.as_deref().unwrap_or("unknown")
                ));
            }
        }
    }

    // ================================================================
    // STEP 3: Apply Sync Mode
    // ================================================================
    let mut sync_result = None;
    let mut should_apply_sync = true;
    if stepping_adjusted {
        should_apply_sync = false;
        log(&format!(
            "[SubtitleData] Skipping {} - stepping already applied",
            config.sync_mode
        ));
    }

    if should_apply_sync {
        let item_info = SyncItemInfo {
            source_key: config.source_key.clone(),
            track_id: config.track_id,
            track_source: config.track_source.clone(),
            is_generated: config.is_generated,
            sync_exclusion_styles: config.sync_exclusion_styles.clone(),
            sync_exclusion_mode: config.sync_exclusion_mode.clone(),
        };

        let result = apply_sync_mode(
            &item_info,
            &mut subtitle_data,
            sync_ctx,
            &config.sync_mode,
            log,
        );

        if result.success && result.events_affected > 0 {
            frame_adjusted = true;

            // Check for negative timestamps that will be clamped to 0 when written
            let negative_events: Vec<_> = subtitle_data
                .events
                .iter()
                .filter(|e| e.start_ms < 0.0 && !e.is_comment)
                .collect();
            if !negative_events.is_empty() {
                let min_time = negative_events
                    .iter()
                    .map(|e| e.start_ms)
                    .fold(f64::INFINITY, f64::min);
                let max_time = negative_events
                    .iter()
                    .map(|e| e.start_ms)
                    .fold(f64::NEG_INFINITY, f64::max);
                log(&format!(
                    "[Sync] Warning: {} event(s) have negative timestamps \
                     ({min_time:.0}ms to {max_time:.0}ms), will be clamped to 0ms",
                    negative_events.len()
                ));
                clamping_info = Some(ClampingInfo {
                    events_clamped: negative_events.len(),
                    delay_ms: 0.0,
                    min_time_ms: min_time,
                    max_time_ms: max_time,
                });
            }
        }

        sync_result = Some(result);
    }

    // ================================================================
    // STEP 4: Determine output format
    // ================================================================
    let output_format = if config.convert_to_ass
        && config
            .extracted_path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref()
            == Some("srt")
    {
        log("[SubtitleData] Will convert SRT to ASS at save");
        ".ass"
    } else {
        config
            .extracted_path
            .extension()
            .map(|e| {
                let s = e.to_string_lossy().to_lowercase();
                if s == "ass" || s == "ssa" {
                    ".ass"
                } else {
                    ".srt"
                }
            })
            .unwrap_or(".ass")
    };

    // ================================================================
    // STEP 5: Apply Style Operations
    // ================================================================

    // Font replacements
    if !config.font_replacements.is_empty() {
        log("[SubtitleData] Applying font replacements...");
        let result =
            style_ops::apply_font_replacement(&mut subtitle_data, &config.font_replacements, Some(log));
        if result.success {
            log(&format!(
                "[SubtitleData] Font replacement: {}",
                result.summary
            ));
        }
    }

    // Style patches
    if !config.style_patch.is_empty() {
        log("[SubtitleData] Applying style patch...");
        let result =
            style_ops::apply_style_patch(&mut subtitle_data, &config.style_patch, Some(log));
        if result.success {
            log(&format!("[SubtitleData] Style patch: {}", result.summary));
        }
    }

    // Rescale
    if config.rescale {
        if let Some(target_res) = target_resolution {
            log("[SubtitleData] Applying rescale...");
            let result = style_ops::apply_rescale(&mut subtitle_data, target_res, Some(log));
            if result.success {
                log(&format!("[SubtitleData] Rescale: {}", result.summary));
            }
        }
    }

    // Size multiplier
    if (config.size_multiplier - 1.0).abs() > 1e-6 {
        if (0.5..=3.0).contains(&config.size_multiplier) {
            log(&format!(
                "[SubtitleData] Applying size multiplier: {}x",
                config.size_multiplier
            ));
            let result = style_ops::apply_size_multiplier(
                &mut subtitle_data,
                config.size_multiplier,
                Some(log),
            );
            if result.success {
                log(&format!(
                    "[SubtitleData] Size multiplier: {}",
                    result.summary
                ));
            }
        } else {
            log(&format!(
                "[SubtitleData] WARNING: Ignoring unreasonable size multiplier {:.2}x",
                config.size_multiplier
            ));
        }
    }

    // ================================================================
    // STEP 6: Save JSON (ALWAYS - before ASS/SRT to preserve all data)
    // ================================================================
    let json_path = config
        .temp_dir
        .join(format!("subtitle_data_track_{}.json", config.track_id));
    match serde_json::to_string_pretty(&serde_json::json!({
        "events_count": subtitle_data.events.len(),
        "styles_count": subtitle_data.styles.len(),
        "operations": subtitle_data.operations.len(),
    })) {
        Ok(json_str) => {
            if let Err(e) = std::fs::write(&json_path, json_str) {
                log(&format!(
                    "[SubtitleData] WARNING: Could not save JSON: {e}"
                ));
            } else {
                log(&format!(
                    "[SubtitleData] JSON saved: {}",
                    json_path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Err(e) => {
            log(&format!(
                "[SubtitleData] WARNING: Could not serialize JSON: {e}"
            ));
        }
    }

    // ================================================================
    // STEP 7: Save ASS/SRT (SINGLE ROUNDING POINT)
    // ================================================================
    let output_path = config.extracted_path.with_extension(output_format.trim_start_matches('.'));

    log(&format!(
        "[SubtitleData] Saving to {}...",
        output_path.file_name().unwrap_or_default().to_string_lossy()
    ));

    // DIAGNOSTIC: Log timestamps BEFORE save
    if !subtitle_data.events.is_empty() {
        log(&format!(
            "[DIAG] Pre-save SubtitleData first 3 events (rounding_mode={}):",
            config.rounding_mode
        ));
        for (i, event) in subtitle_data.events.iter().take(3).enumerate() {
            log(&format!(
                "[DIAG]   Event {i}: start={}ms end={}ms",
                event.start_ms, event.end_ms
            ));
        }
    }

    subtitle_data.save(&output_path, Some(&config.rounding_mode))?;

    log(&format!(
        "[SubtitleData] Saved successfully ({} events)",
        subtitle_data.events.len()
    ));

    // DIAGNOSTIC: Read back saved file timestamps to verify
    let saved_ext = output_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if saved_ext == "ass" || saved_ext == "ssa" {
        let saved_timestamps = read_raw_ass_timestamps(&output_path, 3);
        if !saved_timestamps.is_empty() {
            log("[DIAG] Post-save file first 3 event timestamps:");
            for (i, (start_str, end_str, _style)) in saved_timestamps.iter().enumerate() {
                let start_ms = parse_ass_time_str(start_str);
                let end_ms = parse_ass_time_str(end_str);
                log(&format!(
                    "[DIAG]   Event {i}: start='{start_str}'({start_ms}ms) end='{end_str}'({end_ms}ms)"
                ));
            }

            // Compare with pre-save values
            if !subtitle_data.events.is_empty() {
                for (i, (start_str, end_str, _)) in saved_timestamps
                    .iter()
                    .take(3.min(subtitle_data.events.len()))
                    .enumerate()
                {
                    let saved_start_ms = parse_ass_time_str(start_str);
                    let saved_end_ms = parse_ass_time_str(end_str);
                    let pre_start_ms = subtitle_data.events[i].start_ms;
                    let pre_end_ms = subtitle_data.events[i].end_ms;

                    // Calculate expected saved value based on rounding mode
                    let (expected_start_cs, expected_end_cs) = match config.rounding_mode.as_str() {
                        "ceil" => (
                            (pre_start_ms / 10.0).ceil() as i64,
                            (pre_end_ms / 10.0).ceil() as i64,
                        ),
                        "round" => (
                            (pre_start_ms / 10.0).round() as i64,
                            (pre_end_ms / 10.0).round() as i64,
                        ),
                        _ => (
                            // floor (default)
                            (pre_start_ms / 10.0).floor() as i64,
                            (pre_end_ms / 10.0).floor() as i64,
                        ),
                    };

                    let expected_start_ms = (expected_start_cs * 10) as f64;
                    let expected_end_ms = (expected_end_cs * 10) as f64;

                    if (saved_start_ms - expected_start_ms).abs() > 0.001
                        || (saved_end_ms - expected_end_ms).abs() > 0.001
                    {
                        log(&format!(
                            "[DIAG] WARNING: Save rounding mismatch at event {i}!"
                        ));
                        log(&format!(
                            "[DIAG]   Pre-save: start={pre_start_ms}ms, end={pre_end_ms}ms"
                        ));
                        log(&format!(
                            "[DIAG]   Expected: start={expected_start_ms}ms, end={expected_end_ms}ms"
                        ));
                        log(&format!(
                            "[DIAG]   Actual saved: start={saved_start_ms}ms, end={saved_end_ms}ms"
                        ));
                    }
                }
            }
        }
    }

    // Log summary
    log(&format!(
        "[SubtitleData] Track {} complete: {} operations applied",
        config.track_id,
        subtitle_data.operations.len()
    ));

    Ok(TrackProcessResult {
        output_path,
        stepping_adjusted,
        frame_adjusted,
        sync_result,
        operations_count: subtitle_data.operations.len(),
        clamping_info,
    })
}
