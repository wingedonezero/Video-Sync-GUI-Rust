//! Base auditor — 1:1 port of `vsg_core/postprocess/auditors/base.py`.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::models::jobs::PlanItem;
use crate::orchestrator::steps::context::Context;

/// Base trait for all audit modules — `BaseAuditor`
pub trait Auditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32;
}

/// Get metadata from a source file — `_get_metadata`
pub fn get_metadata(
    file_path: &str,
    tool: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Option<serde_json::Value> {
    let out = if tool == "mkvmerge" {
        runner.run(&["mkvmerge", "-J", file_path], tool_paths)?
    } else {
        runner.run(
            &["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", file_path],
            tool_paths,
        )?
    };
    serde_json::from_str(&out).ok()
}

/// Calculate expected delay for a plan item — `_calculate_expected_delay`
pub fn calculate_expected_delay(ctx: &Context, plan_item: &PlanItem) -> i32 {
    let tr = &plan_item.track;
    let delays = match &ctx.delays {
        Some(d) => d,
        None => return 0,
    };

    if tr.source == "Source 1" && tr.track_type == TrackType::Audio {
        let container_delay = (plan_item.container_delay_ms as f64).round() as i32;
        return container_delay + delays.global_shift_ms;
    }
    if tr.source == "Source 1" && tr.track_type == TrackType::Video {
        return delays.global_shift_ms;
    }
    if tr.track_type == TrackType::Subtitles && plan_item.stepping_adjusted {
        return 0;
    }
    if tr.track_type == TrackType::Subtitles && plan_item.frame_adjusted {
        return 0;
    }

    let sync_key = if tr.source == "External" {
        plan_item.sync_to.as_deref().unwrap_or("Source 1")
    } else {
        &tr.source
    };

    if tr.track_type == TrackType::Subtitles {
        if let Some(&delay) = ctx.subtitle_delays_ms.get(sync_key) {
            return delay.round() as i32;
        }
    }

    delays.source_delays_ms.get(sync_key).copied().unwrap_or(0)
}
