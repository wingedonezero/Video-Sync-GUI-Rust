//! Track order auditor — 1:1 port of `vsg_core/postprocess/auditors/track_order.py`.
//!
//! Verifies that track order in the final MKV matches the merge plan.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies track order matches the merge plan — `TrackOrderAuditor`
pub struct TrackOrderAuditor;

impl Auditor for TrackOrderAuditor {
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

        // Check track count matches
        if tracks.len() != plan_items.len() {
            runner.log_message(&format!(
                "  \u{26a0} Track count mismatch: expected {}, got {}",
                plan_items.len(),
                tracks.len()
            ));
            issues += 1;
        }

        // Check track type order
        for (i, plan_item) in plan_items.iter().enumerate() {
            if i >= tracks.len() {
                break;
            }

            let track = &tracks[i];
            let actual_type = track
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let expected_type = plan_item.track.track_type.to_string();

            if actual_type != expected_type {
                runner.log_message(&format!(
                    "  \u{26a0} Track {}: type mismatch (expected={}, actual={})",
                    i, expected_type, actual_type
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Track order verified");
        }

        issues
    }
}
