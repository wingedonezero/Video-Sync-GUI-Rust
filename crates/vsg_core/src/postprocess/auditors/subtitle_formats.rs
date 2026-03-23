//! Subtitle formats auditor — 1:1 port of `vsg_core/postprocess/auditors/subtitle_formats.py`.
//!
//! Validates subtitle format conversions in the final MKV.

use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::orchestrator::steps::context::Context;

use super::base::Auditor;

/// Expected codec IDs after ASS conversion.
const ASS_CODEC_IDS: &[&str] = &["S_TEXT/ASS", "S_TEXT/SSA"];

/// Expected codec IDs for SRT format.
const SRT_CODEC_IDS: &[&str] = &["S_TEXT/UTF8"];

/// Validates subtitle format conversions — `SubtitleFormatsAuditor`
pub struct SubtitleFormatsAuditor;

impl Auditor for SubtitleFormatsAuditor {
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
            None => return 0,
        };

        let mut sub_idx = 0;
        for plan_item in plan_items {
            if plan_item.track.track_type != TrackType::Subtitles {
                continue;
            }

            // Find the matching subtitle track in final MKV
            let sub_tracks: Vec<&serde_json::Value> = tracks
                .iter()
                .filter(|t| {
                    t.get("type")
                        .and_then(|v| v.as_str())
                        .map(|tt| tt == "subtitles")
                        .unwrap_or(false)
                })
                .collect();

            if sub_idx >= sub_tracks.len() {
                sub_idx += 1;
                continue;
            }

            let track = sub_tracks[sub_idx];
            let props = track
                .get("properties")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let actual_codec = props
                .get("codec_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if plan_item.convert_to_ass {
                // Should be ASS/SSA format after conversion
                if !ASS_CODEC_IDS.contains(&actual_codec) {
                    runner.log_message(&format!(
                        "  \u{26a0} Subtitle track {}: expected ASS format after conversion, got '{}'",
                        sub_idx, actual_codec
                    ));
                    issues += 1;
                }
            } else if plan_item.perform_ocr {
                // OCR output should be ASS or SRT
                let is_text = ASS_CODEC_IDS.contains(&actual_codec)
                    || SRT_CODEC_IDS.contains(&actual_codec);
                if !is_text {
                    runner.log_message(&format!(
                        "  \u{26a0} Subtitle track {}: expected text format after OCR, got '{}'",
                        sub_idx, actual_codec
                    ));
                    issues += 1;
                }
            } else {
                // Codec should be preserved (or remuxed to MKV equivalent)
                let original_codec = &plan_item.track.props.codec_id;

                // Allow compatible codec mappings
                let compatible = actual_codec == original_codec
                    || (original_codec == "S_TEXT/ASS" && actual_codec == "S_TEXT/SSA")
                    || (original_codec == "S_TEXT/SSA" && actual_codec == "S_TEXT/ASS")
                    || (original_codec.starts_with("S_VOBSUB")
                        && actual_codec.starts_with("S_VOBSUB"))
                    || (original_codec.starts_with("S_HDMV/PGS")
                        && actual_codec.starts_with("S_HDMV/PGS"));

                if !compatible {
                    runner.log_message(&format!(
                        "  \u{26a0} Subtitle track {}: codec changed unexpectedly ('{}' -> '{}')",
                        sub_idx, original_codec, actual_codec
                    ));
                    issues += 1;
                }
            }

            sub_idx += 1;
        }

        if issues == 0 {
            runner.log_message("  \u{2714} Subtitle formats verified");
        }

        issues
    }
}
