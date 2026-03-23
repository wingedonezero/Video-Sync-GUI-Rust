//! Video metadata auditor — 1:1 port of `vsg_core/postprocess/auditors/video_metadata.py`.
//!
//! Verifies video metadata (HDR, resolution, color info) was preserved.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::{get_metadata, Auditor};

/// Verifies video metadata preserved — `VideoMetadataAuditor`
pub struct VideoMetadataAuditor;

impl Auditor for VideoMetadataAuditor {
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

        // Find the video plan item (usually first)
        let video_item = match plan_items
            .iter()
            .find(|item| item.track.track_type == TrackType::Video)
        {
            Some(item) => item,
            None => return 0,
        };

        let source_path = match ctx.sources.get(&video_item.track.source) {
            Some(p) => p,
            None => return 0,
        };

        // Get source ffprobe data
        let source_data =
            get_metadata(source_path, "ffprobe", runner, &ctx.tool_paths);

        let source_ffprobe = match source_data {
            Some(data) => data,
            None => return 0,
        };

        let final_ffprobe = match final_ffprobe_data {
            Some(data) => data,
            None => return 0,
        };

        // Find source video stream
        let source_video = find_video_stream(&source_ffprobe);
        let final_video = find_video_stream(final_ffprobe);

        let (src, fin) = match (source_video, final_video) {
            (Some(s), Some(f)) => (s, f),
            _ => return 0,
        };

        // Check resolution
        let src_w = src.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
        let src_h = src.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
        let fin_w = fin.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
        let fin_h = fin.get("height").and_then(|v| v.as_i64()).unwrap_or(0);

        if (src_w != fin_w || src_h != fin_h) && src_w > 0 {
            runner.log_message(&format!(
                "  \u{26a0} Video resolution changed ({}x{} -> {}x{})",
                src_w, src_h, fin_w, fin_h
            ));
            issues += 1;
        }

        // Check color space
        let color_fields = [
            "color_space",
            "color_transfer",
            "color_primaries",
            "color_range",
        ];

        for field in &color_fields {
            let src_val = src.get(*field).and_then(|v| v.as_str()).unwrap_or("");
            let fin_val = fin.get(*field).and_then(|v| v.as_str()).unwrap_or("");

            if !src_val.is_empty() && src_val != "unknown" && src_val != fin_val {
                runner.log_message(&format!(
                    "  \u{26a0} Video {} changed ('{}' -> '{}')",
                    field, src_val, fin_val
                ));
                issues += 1;
            }
        }

        // Check HDR metadata (mastering display, content light level)
        if let Some(src_side_data) = src.get("side_data_list").and_then(|v| v.as_array()) {
            let fin_side_data = fin
                .get("side_data_list")
                .and_then(|v| v.as_array());

            for src_sd in src_side_data {
                let sd_type = src_sd
                    .get("side_data_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if sd_type.contains("Mastering display")
                    || sd_type.contains("Content light level")
                {
                    let found = fin_side_data
                        .map(|fsd| {
                            fsd.iter().any(|f| {
                                f.get("side_data_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    == sd_type
                            })
                        })
                        .unwrap_or(false);

                    if !found {
                        runner.log_message(&format!(
                            "  \u{26a0} HDR metadata '{}' missing from final MKV",
                            sd_type
                        ));
                        issues += 1;
                    }
                }
            }
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Video metadata verified");
        }

        issues
    }
}

/// Find the first video stream in ffprobe data.
fn find_video_stream(data: &serde_json::Value) -> Option<&serde_json::Value> {
    data.get("streams")
        .and_then(|s| s.as_array())
        .and_then(|streams| {
            streams.iter().find(|s| {
                s.get("codec_type")
                    .and_then(|v| v.as_str())
                    .map(|t| t == "video")
                    .unwrap_or(false)
            })
        })
}
