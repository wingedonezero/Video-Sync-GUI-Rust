//! Subtitle clamping auditor — 1:1 port of `vsg_core/postprocess/auditors/subtitle_clamping.py`.
//!
//! Checks for negative timestamp clamping in subtitle tracks.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Checks for negative timestamp clamping — `SubtitleClampingAuditor`
pub struct SubtitleClampingAuditor;

impl Auditor for SubtitleClampingAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        _final_mkvmerge_data: &serde_json::Value,
        _final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let mut issues = 0;

        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        for plan_item in plan_items {
            if plan_item.track.track_type != TrackType::Subtitles {
                continue;
            }

            // Check if clamping info exists
            if let Some(clamping_info) = &plan_item.clamping_info {
                let clamped_count = clamping_info
                    .get("clamped_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let total_events = clamping_info
                    .get("total_events")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if clamped_count > 0 {
                    let min_timestamp = clamping_info
                        .get("min_original_timestamp_ms")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    runner.log_message(&format!(
                        "  \u{26a0} Subtitle track ({} {}): {} of {} events clamped \
                         (earliest original: {:.1}ms)",
                        plan_item.track.source,
                        plan_item.track.id,
                        clamped_count,
                        total_events,
                        min_timestamp
                    ));
                    issues += 1;
                }

                let lost_count = clamping_info
                    .get("lost_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if lost_count > 0 {
                    runner.log_message(&format!(
                        "  \u{26a0} Subtitle track ({} {}): {} events lost due to \
                         negative timestamps that could not be clamped",
                        plan_item.track.source,
                        plan_item.track.id,
                        lost_count
                    ));
                    issues += 1;
                }
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} No subtitle clamping issues");
        }

        issues
    }
}
