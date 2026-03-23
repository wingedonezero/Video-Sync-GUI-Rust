//! Codec integrity auditor — 1:1 port of `vsg_core/postprocess/auditors/codec_integrity.py`.
//!
//! Verifies that codecs were preserved (stream copy) for video and audio tracks.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies codecs were preserved during muxing — `CodecIntegrityAuditor`
pub struct CodecIntegrityAuditor;

impl Auditor for CodecIntegrityAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        let tracks = match final_mkvmerge_data.get("tracks").and_then(|t| t.as_array()) {
            Some(t) => t,
            None => {
                runner.log_message("  \u{26a0} Could not read tracks from final MKV metadata");
                return 1;
            }
        };

        for (i, plan_item) in plan_items.iter().enumerate() {
            if i >= tracks.len() {
                break;
            }

            // Only check video and audio tracks (subtitles may be converted)
            if plan_item.track.track_type == TrackType::Subtitles {
                continue;
            }

            let track = &tracks[i];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let actual_codec = props
                .get("codec_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let expected_codec = &plan_item.track.props.codec_id;

            // For audio tracks with drift correction, codec may change (re-encoding)
            let source_key = format!("{}_{}", plan_item.track.source, plan_item.track.id);
            let has_drift = ctx.pal_drift_flags.contains_key(&source_key)
                || ctx.linear_drift_flags.contains_key(&source_key);

            if has_drift && plan_item.track.track_type == TrackType::Audio {
                // Drift-corrected audio may be re-encoded, skip codec check
                continue;
            }

            if actual_codec != expected_codec {
                runner.log_message(&format!(
                    "  \u{26a0} Track {}: codec mismatch (expected='{}', actual='{}')",
                    i, expected_codec, actual_codec
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Codec integrity verified");
        }

        issues
    }
}
