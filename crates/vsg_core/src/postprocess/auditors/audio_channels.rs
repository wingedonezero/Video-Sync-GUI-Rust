//! Audio channels auditor — 1:1 port of `vsg_core/postprocess/auditors/audio_channels.py`.
//!
//! Verifies that audio channel counts were preserved in the final MKV.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::{get_metadata, Auditor};

/// Verifies audio channel count preserved — `AudioChannelsAuditor`
pub struct AudioChannelsAuditor;

impl Auditor for AudioChannelsAuditor {
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

        let ffprobe = match final_ffprobe_data {
            Some(data) => data,
            None => return 0,
        };

        let streams = match ffprobe.get("streams").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => return 0,
        };

        // Get audio streams from ffprobe
        let audio_streams: Vec<&serde_json::Value> = streams
            .iter()
            .filter(|s| {
                s.get("codec_type")
                    .and_then(|v| v.as_str())
                    .map(|t| t == "audio")
                    .unwrap_or(false)
            })
            .collect();

        // Get audio plan items
        let audio_items: Vec<_> = plan_items
            .iter()
            .filter(|item| item.track.track_type == TrackType::Audio)
            .collect();

        for (i, plan_item) in audio_items.iter().enumerate() {
            if i >= audio_streams.len() {
                runner.log_message(&format!(
                    "  \u{26a0} Audio track {} missing from final MKV",
                    i
                ));
                issues += 1;
                continue;
            }

            // Get source channel count
            let source_path = ctx.sources.get(&plan_item.track.source);
            if source_path.is_none() {
                continue;
            }

            let source_data = get_metadata(
                source_path.unwrap(),
                "ffprobe",
                runner,
                &ctx.tool_paths,
            );

            if let Some(source_data) = source_data {
                let source_streams = source_data
                    .get("streams")
                    .and_then(|s| s.as_array());

                if let Some(source_streams) = source_streams {
                    // Find the matching audio stream in source
                    let source_audio: Vec<&serde_json::Value> = source_streams
                        .iter()
                        .filter(|s| {
                            s.get("codec_type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "audio")
                                .unwrap_or(false)
                        })
                        .collect();

                    let source_idx = plan_item.track.id as usize;
                    if let Some(source_stream) = source_audio.get(source_idx) {
                        let source_channels = source_stream
                            .get("channels")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        let final_channels = audio_streams[i]
                            .get("channels")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        if source_channels != final_channels && source_channels > 0 {
                            runner.log_message(&format!(
                                "  \u{26a0} Audio track {}: channel count changed ({} -> {})",
                                i, source_channels, final_channels
                            ));
                            issues += 1;
                        }
                    }
                }
            }
        }

        if issues == 0 && !audio_items.is_empty() {
            runner.log_message("  \u{2714} Audio channel counts verified");
        }

        issues
    }
}
