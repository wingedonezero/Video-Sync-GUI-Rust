//! Audio sync auditor — 1:1 port of `vsg_core/postprocess/auditors/audio_sync.py`.
//!
//! Verifies audio sync delays in the final MKV match calculated values.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::{calculate_expected_delay, Auditor};

/// Verifies audio sync delays match calculated values — `AudioSyncAuditor`
pub struct AudioSyncAuditor;

impl Auditor for AudioSyncAuditor {
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
            // Only check audio and video tracks for delay
            if plan_item.track.track_type == TrackType::Subtitles {
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

            // Get actual delay from mkvmerge metadata (minimum_timestamp or default_duration)
            let actual_delay_ns = props
                .get("minimum_timestamp")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let actual_delay_ms = (actual_delay_ns as f64 / 1_000_000.0).round() as i32;

            let expected_delay = calculate_expected_delay(ctx, plan_item);

            // Allow 1ms tolerance for rounding
            let diff = (actual_delay_ms - expected_delay).abs();
            if diff > 1 {
                runner.log_message(&format!(
                    "  \u{26a0} Track {} ({}): delay mismatch (expected={}ms, actual={}ms, diff={}ms)",
                    i,
                    plan_item.track.track_type,
                    expected_delay,
                    actual_delay_ms,
                    diff
                ));
                issues += 1;
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Audio sync delays verified");
        }

        issues
    }
}
