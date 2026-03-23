//! Track flags auditor — 1:1 port of `vsg_core/postprocess/auditors/track_flags.py`.
//!
//! Verifies that default/forced flags in the final MKV match the merge plan.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies default/forced track flags match the merge plan — `TrackFlagsAuditor`
pub struct TrackFlagsAuditor;

impl Auditor for TrackFlagsAuditor {
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
                runner.log_message(&format!(
                    "  \u{26a0} Track {} missing from final MKV (expected {} tracks, got {})",
                    i,
                    plan_items.len(),
                    tracks.len()
                ));
                issues += 1;
                continue;
            }

            let track = &tracks[i];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            // Check default flag
            let actual_default = props
                .get("default_track")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let expected_default = plan_item.is_default;

            if actual_default != expected_default {
                runner.log_message(&format!(
                    "  \u{26a0} Track {}: default flag mismatch (expected={}, actual={})",
                    i, expected_default, actual_default
                ));
                issues += 1;
            }

            // Check forced flag
            let actual_forced = props
                .get("forced_track")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let expected_forced = plan_item.is_forced_display;

            if actual_forced != expected_forced {
                runner.log_message(&format!(
                    "  \u{26a0} Track {}: forced flag mismatch (expected={}, actual={})",
                    i, expected_forced, actual_forced
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Track flags verified");
        }

        issues
    }
}
