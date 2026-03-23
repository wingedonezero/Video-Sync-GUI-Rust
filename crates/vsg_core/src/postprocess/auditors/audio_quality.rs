//! Audio quality auditor — 1:1 port of `vsg_core/postprocess/auditors/audio_quality.py`.
//!
//! Verifies audio quality (bit depth, sample rate) was preserved.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::{get_metadata, Auditor};

/// Verifies audio quality preserved — `AudioQualityAuditor`
pub struct AudioQualityAuditor;

impl Auditor for AudioQualityAuditor {
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

        let final_streams = match ffprobe.get("streams").and_then(|s| s.as_array()) {
            Some(s) => s,
            None => return 0,
        };

        let final_audio: Vec<&serde_json::Value> = final_streams
            .iter()
            .filter(|s| {
                s.get("codec_type")
                    .and_then(|v| v.as_str())
                    .map(|t| t == "audio")
                    .unwrap_or(false)
            })
            .collect();

        let audio_items: Vec<_> = plan_items
            .iter()
            .filter(|item| item.track.track_type == TrackType::Audio)
            .collect();

        for (i, plan_item) in audio_items.iter().enumerate() {
            if i >= final_audio.len() {
                continue;
            }

            // Skip drift-corrected tracks (re-encoded, quality may differ)
            let source_key = format!("{}_{}", plan_item.track.source, plan_item.track.id);
            if ctx.pal_drift_flags.contains_key(&source_key)
                || ctx.linear_drift_flags.contains_key(&source_key)
            {
                continue;
            }

            let source_path = match ctx.sources.get(&plan_item.track.source) {
                Some(p) => p,
                None => continue,
            };

            let source_data =
                get_metadata(source_path, "ffprobe", runner, &ctx.tool_paths);

            if let Some(source_data) = source_data {
                let source_audio: Vec<&serde_json::Value> = source_data
                    .get("streams")
                    .and_then(|s| s.as_array())
                    .map(|streams| {
                        streams
                            .iter()
                            .filter(|s| {
                                s.get("codec_type")
                                    .and_then(|v| v.as_str())
                                    .map(|t| t == "audio")
                                    .unwrap_or(false)
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let src_idx = plan_item.track.id as usize;
                if let Some(src_stream) = source_audio.get(src_idx) {
                    // Check sample rate
                    let src_rate = src_stream
                        .get("sample_rate")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    let final_rate = final_audio[i]
                        .get("sample_rate")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");

                    if src_rate != final_rate && src_rate != "0" {
                        runner.log_message(&format!(
                            "  \u{26a0} Audio track {}: sample rate changed ({} -> {})",
                            i, src_rate, final_rate
                        ));
                        issues += 1;
                    }

                    // Check bit depth (bits_per_raw_sample)
                    let src_bits = src_stream
                        .get("bits_per_raw_sample")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            src_stream
                                .get("bits_per_sample")
                                .and_then(|v| v.as_str())
                        });
                    let final_bits = final_audio[i]
                        .get("bits_per_raw_sample")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            final_audio[i]
                                .get("bits_per_sample")
                                .and_then(|v| v.as_str())
                        });

                    if let (Some(src_b), Some(final_b)) = (src_bits, final_bits) {
                        if src_b != final_b {
                            runner.log_message(&format!(
                                "  \u{26a0} Audio track {}: bit depth changed ({} -> {})",
                                i, src_b, final_b
                            ));
                            issues += 1;
                        }
                    }
                }
            }
        }

        if issues == 0 && !audio_items.is_empty() {
            runner.log_message("  \u{2714} Audio quality verified");
        }

        issues
    }
}
