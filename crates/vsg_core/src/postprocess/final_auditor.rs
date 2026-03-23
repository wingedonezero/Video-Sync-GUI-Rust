//! Final auditor — 1:1 port of `vsg_core/postprocess/final_auditor.py`.
//!
//! Orchestrates all post-merge validation checks.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::auditors::base::{get_metadata, Auditor};
use super::auditors::*;

/// Coordinates all post-merge validation — `FinalAuditor`
pub struct FinalAuditor;

impl FinalAuditor {
    /// Run all auditors against the final MKV file — `run`
    pub fn run(
        ctx: &Context,
        runner: &CommandRunner,
        final_mkv_path: &Path,
    ) -> i32 {
        let mut total_issues = 0;

        // Get metadata for the final file
        let mkvmerge_data = match get_metadata(
            &final_mkv_path.to_string_lossy(),
            "mkvmerge",
            runner,
            &ctx.tool_paths,
        ) {
            Some(data) => data,
            None => {
                runner.log_message("[ERROR] Could not read final file metadata for audit.");
                return 1;
            }
        };

        let ffprobe_data = get_metadata(
            &final_mkv_path.to_string_lossy(),
            "ffprobe",
            runner,
            &ctx.tool_paths,
        );

        runner.log_message("--- Running Post-Merge Audit ---");

        // Run all auditors in order (matching Python's run order)
        let auditors: Vec<(&str, Box<dyn Auditor>)> = vec![
            ("Track Flags", Box::new(track_flags::TrackFlagsAuditor)),
            ("Video Metadata", Box::new(video_metadata::VideoMetadataAuditor)),
            ("Dolby Vision", Box::new(dolby_vision::DolbyVisionAuditor)),
            ("Audio Object-Based", Box::new(audio_object_based::AudioObjectBasedAuditor)),
            ("Codec Integrity", Box::new(codec_integrity::CodecIntegrityAuditor)),
            ("Audio Channels", Box::new(audio_channels::AudioChannelsAuditor)),
            ("Audio Quality", Box::new(audio_quality::AudioQualityAuditor)),
            ("Drift Correction", Box::new(drift_correction::DriftCorrectionAuditor)),
            ("Stepping Correction", Box::new(stepping_correction::SteppingCorrectionAuditor)),
            ("Global Shift", Box::new(global_shift::GlobalShiftAuditor)),
            ("Audio Sync", Box::new(audio_sync::AudioSyncAuditor)),
            ("Subtitle Formats", Box::new(subtitle_formats::SubtitleFormatsAuditor)),
            ("Neural Confidence", Box::new(neural_confidence::NeuralConfidenceAuditor)),
            ("Frame Audit", Box::new(frame_audit::FrameAuditAuditor)),
            ("Subtitle Clamping", Box::new(subtitle_clamping::SubtitleClampingAuditor)),
            ("Chapters", Box::new(chapters::ChaptersAuditor)),
            ("Track Order", Box::new(track_order::TrackOrderAuditor)),
            ("Language Tags", Box::new(language_tags::LanguageTagsAuditor)),
            ("Track Names", Box::new(track_names::TrackNamesAuditor)),
            ("Attachments", Box::new(attachments::AttachmentsAuditor)),
        ];

        for (name, auditor) in &auditors {
            let issues = auditor.run(
                ctx,
                runner,
                final_mkv_path,
                &mkvmerge_data,
                ffprobe_data.as_ref(),
            );
            if issues > 0 {
                runner.log_message(&format!("[Audit] {name}: {issues} issue(s)"));
            }
            total_issues += issues;
        }

        if total_issues == 0 {
            runner.log_message("--- Audit Complete: No issues found ---");
        } else {
            runner.log_message(&format!(
                "--- Audit Complete: {total_issues} total issue(s) ---"
            ));
        }

        total_issues
    }
}
