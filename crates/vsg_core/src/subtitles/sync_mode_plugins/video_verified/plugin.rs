//! VideoVerifiedSync plugin class.
//!
//! This is the SyncPlugin entry point that integrates with the subtitle pipeline.
//! The actual frame matching algorithm lives in matcher.rs.
//!
//! 1:1 port of `video_verified/plugin.py`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;
use crate::subtitles::data::{OperationRecord, OperationResult, SubtitleData};
use crate::subtitles::sync_utils::apply_delay_to_events;

use super::matcher::calculate_video_verified_offset;

/// Video-Verified sync mode.
///
/// Uses audio correlation as starting point, then verifies with frame
/// matching to determine the TRUE video-to-video offset for subtitle timing.
pub struct VideoVerifiedSync;

impl VideoVerifiedSync {
    pub const NAME: &'static str = "video-verified";
    pub const DESCRIPTION: &'static str =
        "Audio correlation verified against video frame matching with sub-frame precision";

    /// Apply video-verified sync to subtitle data.
    pub fn apply(
        &self,
        subtitle_data: &mut SubtitleData,
        total_delay_ms: f64,
        global_shift_ms: f64,
        target_fps: Option<f64>,
        source_video: Option<&str>,
        target_video: Option<&str>,
        runner: &CommandRunner,
        settings: Option<&AppSettings>,
        temp_dir: Option<PathBuf>,
        track_label: Option<&str>,
    ) -> OperationResult {
        let default_settings = AppSettings::default();
        let settings = settings.unwrap_or(&default_settings);

        let log = |msg: &str| {
            runner.log_message(msg);
        };

        log("[VideoVerified] === Video-Verified Sync Mode ===");
        log(&format!(
            "[VideoVerified] Events: {}",
            subtitle_data.events.len()
        ));

        let (source_video, target_video) = match (source_video, target_video) {
            (Some(s), Some(t)) => (s, t),
            _ => {
                return OperationResult {
                    success: false,
                    operation: "sync".to_string(),
                    events_affected: 0,
                    styles_affected: 0,
                    summary: String::new(),
                    details: HashMap::new(),
                    error: Some(
                        "Both source and target videos required for video-verified mode"
                            .to_string(),
                    ),
                };
            }
        };

        let pure_correlation_ms = total_delay_ms - global_shift_ms;

        // Estimate duration from subtitle events
        let video_duration = subtitle_data
            .events
            .iter()
            .map(|e| e.end_ms)
            .fold(0.0f64, f64::max)
            + 60000.0;

        // Calculate video-verified offset
        let (final_offset_ms, details) = calculate_video_verified_offset(
            source_video,
            target_video,
            total_delay_ms,
            global_shift_ms,
            Some(settings),
            runner,
            temp_dir.clone(),
            Some(video_duration),
        );

        let final_offset_ms = final_offset_ms.unwrap_or(total_delay_ms);
        let video_offset_ms = details
            .get("video_offset_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(pure_correlation_ms);
        let selection_reason = details
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Generate job name
        let video_stem = Path::new(target_video)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let job_name = if let Some(label) = track_label {
            if label.is_empty() {
                video_stem
            } else {
                format!("{}_{}", video_stem, label)
            }
        } else {
            video_stem
        };

        // Apply the offset
        self.apply_offset(
            subtitle_data,
            final_offset_ms,
            global_shift_ms,
            pure_correlation_ms,
            video_offset_ms,
            &selection_reason,
            &details,
            runner,
            settings,
            target_fps,
            &job_name,
        )
    }

    fn apply_offset(
        &self,
        subtitle_data: &mut SubtitleData,
        final_offset_ms: f64,
        global_shift_ms: f64,
        audio_correlation_ms: f64,
        video_offset_ms: f64,
        selection_reason: &str,
        details: &HashMap<String, serde_json::Value>,
        runner: &CommandRunner,
        settings: &AppSettings,
        target_fps: Option<f64>,
        job_name: &str,
    ) -> OperationResult {
        let log = |msg: &str| {
            runner.log_message(msg);
        };

        log(&format!(
            "[VideoVerified] Applying {:+.3}ms to {} events",
            final_offset_ms,
            subtitle_data.events.len()
        ));

        let events_synced = apply_delay_to_events(subtitle_data, final_offset_ms, false);

        // Run frame alignment audit (always when FPS available)
        if let Some(fps) = target_fps {
            self.run_frame_audit(subtitle_data, fps, final_offset_ms, job_name, settings, &log);
        }

        // Build summary
        let summary = if (video_offset_ms - audio_correlation_ms).abs() > 1.0 {
            format!(
                "VideoVerified: {} events, {:+.1}ms (audio={:+.0}->video={:+.0})",
                events_synced, final_offset_ms, audio_correlation_ms, video_offset_ms
            )
        } else {
            format!(
                "VideoVerified: {} events, {:+.1}ms",
                events_synced, final_offset_ms
            )
        };

        // Record operation
        let record = OperationRecord {
            operation: "sync".to_string(),
            timestamp: chrono::Local::now().to_rfc3339(),
            parameters: serde_json::json!({
                "mode": Self::NAME,
                "final_offset_ms": final_offset_ms,
                "global_shift_ms": global_shift_ms,
                "audio_correlation_ms": audio_correlation_ms,
                "video_offset_ms": video_offset_ms,
                "selection_reason": selection_reason,
            }),
            events_affected: events_synced,
            styles_affected: 0,
            summary: summary.clone(),
        };
        subtitle_data.operations.push(record);

        log(&format!(
            "[VideoVerified] Sync complete: {} events",
            events_synced
        ));
        log("[VideoVerified] ===================================");

        let mut result_details = HashMap::new();
        result_details.insert("audio_correlation_ms".to_string(), serde_json::json!(audio_correlation_ms));
        result_details.insert("video_offset_ms".to_string(), serde_json::json!(video_offset_ms));
        result_details.insert("final_offset_ms".to_string(), serde_json::json!(final_offset_ms));
        result_details.insert("selection_reason".to_string(), serde_json::json!(selection_reason));
        result_details.insert("target_fps".to_string(), serde_json::json!(target_fps));
        for (k, v) in details {
            result_details.insert(k.clone(), v.clone());
        }

        OperationResult {
            success: true,
            operation: "sync".to_string(),
            events_affected: events_synced,
            styles_affected: 0,
            summary,
            details: result_details,
            error: None,
        }
    }

    fn run_frame_audit(
        &self,
        subtitle_data: &SubtitleData,
        fps: f64,
        offset_ms: f64,
        job_name: &str,
        settings: &AppSettings,
        log: &dyn Fn(&str),
    ) {
        use crate::subtitles::frame_utils::frame_audit::run_frame_audit;

        log("[FrameAudit] Running frame alignment audit...");

        let rounding_mode = &settings.subtitle_rounding.to_string();

        let result = run_frame_audit(
            subtitle_data,
            fps,
            rounding_mode,
            offset_ms,
            job_name,
            Some(log),
        );

        let total = result.total_events as f64;
        if total > 0.0 {
            let start_pct = 100.0 * result.start_ok as f64 / total;
            let end_pct = 100.0 * result.end_ok as f64 / total;
            log(&format!(
                "[FrameAudit] Start times OK: {}/{} ({:.1}%)",
                result.start_ok, result.total_events, start_pct
            ));
            log(&format!(
                "[FrameAudit] End times OK: {}/{} ({:.1}%)",
                result.end_ok, result.total_events, end_pct
            ));

            if result.has_issues() {
                log(&format!(
                    "[FrameAudit] Issues found: {} events with frame drift",
                    result.issues.len()
                ));
                if result.predicted_corrections > 0 {
                    log(&format!(
                        "[FrameAudit] Surgical rounding will correct: {} timing points ({} events)",
                        result.predicted_corrections, result.predicted_correction_events
                    ));
                }
            } else {
                log("[FrameAudit] No frame drift issues detected");
            }
        }
    }
}
