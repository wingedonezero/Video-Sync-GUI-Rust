//! Dolby Vision auditor — 1:1 port of `vsg_core/postprocess/auditors/dolby_vision.py`.
//!
//! Verifies that Dolby Vision metadata was preserved in the final MKV.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Verifies Dolby Vision metadata preserved — `DolbyVisionAuditor`
pub struct DolbyVisionAuditor;

impl Auditor for DolbyVisionAuditor {
    fn run(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        _final_mkv_path: &Path,
        final_mkvmerge_data: &serde_json::Value,
        final_ffprobe_data: Option<&serde_json::Value>,
    ) -> i32 {
        let plan_items = match &ctx.extracted_items {
            Some(items) => items,
            None => return 0,
        };

        // Check if any source video track has Dolby Vision
        let has_dv_source = plan_items.iter().any(|item| {
            if item.track.track_type != TrackType::Video {
                return false;
            }
            let codec = &item.track.props.codec_id;
            codec.contains("HEVC") || codec.contains("hevc") || codec.contains("H265")
        });

        if !has_dv_source {
            // No HEVC video track, DV check not applicable
            return 0;
        }

        // Check ffprobe data for DV side data
        let ffprobe = match final_ffprobe_data {
            Some(data) => data,
            None => return 0,
        };

        let streams = match ffprobe.get("streams").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => return 0,
        };

        let mut issues = 0;

        for stream in streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if codec_type != "video" {
                continue;
            }

            // Check for Dolby Vision side data
            let side_data = stream
                .get("side_data_list")
                .and_then(|v| v.as_array());

            if let Some(side_data_list) = side_data {
                let has_dv = side_data_list.iter().any(|sd| {
                    let sd_type = sd
                        .get("side_data_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    sd_type.contains("DOVI") || sd_type.contains("Dolby Vision")
                });

                if has_dv {
                    runner.log_message("  \u{2714} Dolby Vision metadata preserved");
                } else {
                    // Check mkvmerge data for DV codec private
                    let mkv_tracks = final_mkvmerge_data
                        .get("tracks")
                        .and_then(|t| t.as_array());

                    if let Some(mkv_tracks) = mkv_tracks {
                        let has_dv_mkv = mkv_tracks.iter().any(|t| {
                            let props = t.get("properties").unwrap_or(&serde_json::Value::Null);
                            let codec_id = props
                                .get("codec_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            codec_id.contains("HEVC")
                                && props.get("codec_private_data").is_some()
                        });

                        if !has_dv_mkv {
                            runner.log_message(
                                "  \u{26a0} Dolby Vision metadata may not be preserved"
                            );
                            issues += 1;
                        }
                    }
                }
            }
        }

        issues
    }
}
