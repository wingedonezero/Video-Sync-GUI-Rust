//! Track names auditor — 1:1 port of `vsg_core/postprocess/auditors/track_names.py`.
//!
//! Verifies that track names in the final MKV match the merge plan.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies track names match the merge plan — `TrackNamesAuditor`
pub struct TrackNamesAuditor;

impl Auditor for TrackNamesAuditor {
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

            let track = &tracks[i];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let actual_name = props
                .get("track_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Determine expected name
            let expected_name = if !plan_item.custom_name.is_empty() {
                plan_item.custom_name.as_str()
            } else if plan_item.apply_track_name {
                plan_item.track.props.name.as_str()
            } else {
                ""
            };

            if actual_name != expected_name {
                runner.log_message(&format!(
                    "  \u{26a0} Track {}: name mismatch (expected='{}', actual='{}')",
                    i, expected_name, actual_name
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Track names verified");
        }

        issues
    }
}
