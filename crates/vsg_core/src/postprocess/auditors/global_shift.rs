//! Global shift auditor — 1:1 port of `vsg_core/postprocess/auditors/global_shift.py`.
//!
//! Verifies that the global shift was applied correctly.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies global shift was applied correctly — `GlobalShiftAuditor`
pub struct GlobalShiftAuditor;

impl Auditor for GlobalShiftAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let delays = match &ctx.delays {
            Some(d) => d,
            None => return 0,
        };

        let global_shift = delays.global_shift_ms;

        if global_shift == 0 && !ctx.global_shift_is_required {
            // No global shift needed or applied
            return 0;
        }

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        let tracks = match final_mkvmerge_data.get("tracks").and_then(|t| t.as_array()) {
            Some(t) => t,
            None => return 0,
        };

        // Check Source 1 video track has the global shift applied
        for (i, plan_item) in plan_items.iter().enumerate() {
            if plan_item.track.source != "Source 1" {
                continue;
            }
            if plan_item.track.track_type != TrackType::Video {
                continue;
            }
            if i >= tracks.len() {
                continue;
            }

            let track = &tracks[i];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let actual_delay_ns = props
                .get("minimum_timestamp")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let actual_delay_ms = (actual_delay_ns as f64 / 1_000_000.0).round() as i32;

            let diff = (actual_delay_ms - global_shift).abs();
            if diff > 1 {
                runner.log_message(&format!(
                    "  \u{26a0} Global shift mismatch on video track \
                     (expected={}ms, actual={}ms)",
                    global_shift, actual_delay_ms
                ));
                issues += 1;
            }
        }

        if issues == 0 && global_shift != 0 {
            runner.log_message(&format!(
                "  \u{2714} Global shift verified ({}ms)",
                global_shift
            ));
        }

        issues
    }
}
