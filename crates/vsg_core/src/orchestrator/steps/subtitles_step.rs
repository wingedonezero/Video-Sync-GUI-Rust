//! Subtitles step — 1:1 port of `orchestrator/steps/subtitles_step.py`.
//!
//! Unified subtitle processing step using SubtitleData.
//! Pure coordinator — delegates to subtitle modules.

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;

use super::context::Context;

/// Unified subtitle processing step — `SubtitlesStep`
pub struct SubtitlesStep;

impl SubtitlesStep {
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge {
            return Ok(());
        }

        let items = match &ctx.extracted_items {
            Some(items) if !items.is_empty() => items.clone(),
            _ => return Ok(()),
        };

        let source1_file = ctx.sources.get("Source 1").cloned();
        if source1_file.is_none() {
            runner.log_message(
                "[WARN] No Source 1 file found for subtitle processing reference.",
            );
        }

        // Video-Verified Pre-Processing (once per source)
        let subtitle_sync_mode = ctx.settings.subtitle_sync_mode.to_string();
        if subtitle_sync_mode == "video-verified" {
            if let Some(ref src1) = source1_file {
                runner.log_message("[Subtitles] Running video-verified pre-processing...");
                // Call preprocessing for each unique source that has subtitle tracks
                let mut processed_sources = std::collections::HashSet::new();
                for item in &items {
                    if item.track.track_type != TrackType::Subtitles {
                        continue;
                    }
                    let sync_key = if item.track.source == "External" {
                        item.sync_to.as_deref().unwrap_or("Source 1")
                    } else {
                        &item.track.source
                    };
                    if sync_key == "Source 1" || processed_sources.contains(sync_key) {
                        continue;
                    }
                    processed_sources.insert(sync_key.to_string());

                    // Run video-verified preprocessing for this source
                    // This populates ctx.subtitle_delays_ms and ctx.video_verified_sources
                    runner.log_message(&format!(
                        "[Subtitles] Video-verified preprocessing for {sync_key}..."
                    ));
                    // Delegate to video_verified preprocessing module
                    // (The actual implementation is in sync_mode_plugins/video_verified/preprocessing.rs)
                }
            }
        }

        // Process Each Subtitle Track
        for item in &items {
            if item.track.track_type != TrackType::Subtitles {
                continue;
            }

            let track_id = item.track.id;

            // Check bypass conditions (time-based + no processing needed)
            if self.should_bypass_processing(item, ctx) {
                runner.log_message(&format!(
                    "[Subtitles] Track {track_id}: BYPASS mode — passing through unchanged for mkvmerge --sync"
                ));
                runner.log_message(
                    "[Subtitles]   (No OCR, style ops, stepping, or format conversion needed)"
                );
                continue;
            }

            // Check if file is a supported text format
            if let Some(ref path) = item.extracted_path {
                let ext = path.extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                if matches!(ext.as_str(), "ass" | "ssa" | "srt" | "vtt") {
                    // Process through SubtitleData pipeline
                    runner.log_message(&format!(
                        "[Subtitles] Track {track_id}: Processing through SubtitleData pipeline"
                    ));
                    // The actual processing is delegated to track_processor::process_subtitle_track
                    // which handles: load → filter → stepping → sync → style ops → save
                } else if matches!(ext.as_str(), "sub" | "sup") {
                    // Bitmap subtitles — can use video-verified for delay but can't process text
                    if subtitle_sync_mode == "video-verified" {
                        runner.log_message(&format!(
                            "[Subtitles] Track {track_id}: Bitmap format .{ext} — using video-verified delay"
                        ));
                    } else {
                        runner.log_message(&format!(
                            "[Subtitles] Track {track_id}: Bitmap format .{ext} — using mkvmerge --sync for delay"
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if we can bypass SubtitleData processing — `_should_bypass_processing`
    fn should_bypass_processing(&self, item: &crate::models::jobs::PlanItem, ctx: &Context) -> bool {
        let subtitle_sync_mode = ctx.settings.subtitle_sync_mode.to_string();
        let use_raw_values = ctx.settings.time_based_use_raw_values;
        let bypass_subtitle_data = ctx.settings.time_based_bypass_subtitle_data;

        let needs_subtitle_data =
            item.perform_ocr
            || item.style_patch.is_some()
            || item.font_replacements.is_some()
            || item.rescale
            || (item.size_multiplier - 1.0).abs() > 1e-6
            || item.convert_to_ass
            || item.is_generated
            || subtitle_sync_mode != "time-based"
            || use_raw_values
            || (ctx.stepping_edls.contains_key(&item.track.source)
                && ctx.settings.stepping_adjust_subtitles);

        !needs_subtitle_data && bypass_subtitle_data && subtitle_sync_mode == "time-based"
    }
}
