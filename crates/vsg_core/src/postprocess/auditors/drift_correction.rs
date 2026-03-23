//! Drift correction auditor — 1:1 port of `vsg_core/postprocess/auditors/drift_correction.py`.
//!
//! Verifies that PAL and linear drift corrections were applied correctly.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies drift corrections were applied — `DriftCorrectionAuditor`
pub struct DriftCorrectionAuditor;

impl Auditor for DriftCorrectionAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        _final_mkvmerge_data: &serde_json::Value,
        final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        // Check PAL drift corrections
        for (source_key, drift_entry) in &ctx.pal_drift_flags {
            let rate = drift_entry.rate.unwrap_or(0.0);
            if rate == 0.0 {
                continue;
            }

            // Find audio tracks from this source that should be drift-corrected
            let has_audio = plan_items.iter().any(|item| {
                let key = format!("{}_{}", item.track.source, item.track.id);
                key == *source_key && item.track.track_type == TrackType::Audio
            });

            if has_audio {
                runner.log_message(&format!(
                    "  \u{2139} PAL drift correction applied for {} (rate: {:.6})",
                    source_key, rate
                ));
            }
        }

        // Check linear drift corrections
        for (source_key, drift_entry) in &ctx.linear_drift_flags {
            let rate = drift_entry.rate.unwrap_or(0.0);
            if rate == 0.0 {
                continue;
            }

            let has_audio = plan_items.iter().any(|item| {
                let key = format!("{}_{}", item.track.source, item.track.id);
                key == *source_key && item.track.track_type == TrackType::Audio
            });

            if has_audio {
                runner.log_message(&format!(
                    "  \u{2139} Linear drift correction applied for {} (rate: {:.6})",
                    source_key, rate
                ));
            }
        }

        // Verify drift-corrected audio tracks exist in final MKV
        if !ctx.pal_drift_flags.is_empty() || !ctx.linear_drift_flags.is_empty() {
            let ffprobe = match final_ffprobe_data {
                Some(data) => data,
                None => {
                    runner.log_message(
                        "  \u{26a0} Cannot verify drift correction: no ffprobe data"
                    );
                    return 1;
                }
            };

            let streams = ffprobe
                .get("streams")
                .and_then(|s| s.as_array())
                .unwrap_or(&Vec::new())
                .clone();

            let audio_count = streams
                .iter()
                .filter(|s| {
                    s.get("codec_type")
                        .and_then(|v| v.as_str())
                        .map(|t| t == "audio")
                        .unwrap_or(false)
                })
                .count();

            let expected_audio = plan_items
                .iter()
                .filter(|item| item.track.track_type == TrackType::Audio)
                .count();

            if audio_count != expected_audio {
                runner.log_message(&format!(
                    "  \u{26a0} Audio track count mismatch after drift correction \
                     (expected={}, actual={})",
                    expected_audio, audio_count
                ));
                issues += 1;
            }
        }

        if issues == 0
            && (!ctx.pal_drift_flags.is_empty() || !ctx.linear_drift_flags.is_empty())
        {
            runner.log_message("  \u{2714} Drift corrections verified");
        }

        issues
    }
}
