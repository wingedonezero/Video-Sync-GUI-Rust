//! Audio object-based auditor — 1:1 port of `vsg_core/postprocess/auditors/audio_object_based.py`.
//!
//! Verifies that object-based audio metadata (Atmos, DTS:X) was preserved.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Object-based audio codecs that carry spatial metadata.
const OBJECT_BASED_CODECS: &[&str] = &[
    "A_TRUEHD",   // TrueHD (may contain Atmos)
    "A_EAC3",     // E-AC-3 (may contain Atmos / JOC)
    "A_DTS",      // DTS (may contain DTS:X)
];

/// Verifies object-based audio metadata preserved — `AudioObjectBasedAuditor`
pub struct AudioObjectBasedAuditor;

impl Auditor for AudioObjectBasedAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        // Check if any audio tracks use object-based codecs
        let object_audio_items: Vec<_> = plan_items
            .iter()
            .filter(|item| {
                item.track.track_type == TrackType::Audio
                    && OBJECT_BASED_CODECS
                        .iter()
                        .any(|c| item.track.props.codec_id.starts_with(c))
            })
            .collect();

        if object_audio_items.is_empty() {
            return 0;
        }

        let tracks = match final_mkvmerge_data.get("tracks").and_then(|t| t.as_array()) {
            Some(t) => t,
            None => return 0,
        };

        // Verify object-based audio codecs are preserved in final
        for plan_item in &object_audio_items {
            let expected_codec = &plan_item.track.props.codec_id;

            // Find matching track in final MKV
            let found = tracks.iter().any(|t| {
                let props = t.get("properties").unwrap_or(&serde_json::Value::Null);
                let codec = props
                    .get("codec_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                codec == expected_codec
            });

            if !found {
                runner.log_message(&format!(
                    "  \u{26a0} Object-based audio codec '{}' not found in final MKV",
                    expected_codec
                ));
                issues += 1;
            }
        }

        // Check ffprobe for Atmos/JOC metadata if available
        if let Some(ffprobe) = final_ffprobe_data {
            if let Some(streams) = ffprobe.get("streams").and_then(|s| s.as_array()) {
                for stream in streams {
                    let codec_type = stream
                        .get("codec_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if codec_type != "audio" {
                        continue;
                    }

                    let profile = stream
                        .get("profile")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if profile.contains("atmos") || profile.contains("Atmos") {
                        runner.log_message("  \u{2714} Dolby Atmos metadata detected");
                    }
                }
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Object-based audio metadata verified");
        }

        issues
    }
}
