//! Video-verified preprocessing for subtitle synchronization.
//!
//! Pre-computes frame-corrected delays for all subtitle sources by running
//! video-to-video frame matching once per source (not per track).
//!
//! 1:1 port of `video_verified/preprocessing.py`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;
use crate::subtitles::frame_utils::video_properties::detect_video_properties;

use super::matcher::calculate_video_verified_offset;
use super::neural_matcher::calculate_neural_verified_offset;

/// Dispatch to classic or neural matcher based on settings.
fn calculate_offset_for_method(
    source_video: &str,
    target_video: &str,
    total_delay_ms: f64,
    global_shift_ms: f64,
    settings: &AppSettings,
    runner: &CommandRunner,
    temp_dir: Option<PathBuf>,
    source_key: &str,
    debug_output_dir: Option<PathBuf>,
) -> (Option<f64>, HashMap<String, serde_json::Value>) {
    let method = settings.video_verified_method.to_string();

    if method == "neural" {
        return calculate_neural_verified_offset(
            source_video,
            target_video,
            total_delay_ms,
            global_shift_ms,
            Some(settings),
            runner,
            temp_dir,
            None,
            debug_output_dir,
            source_key,
        );
    }

    // Classic method (default)
    calculate_video_verified_offset(
        source_video,
        target_video,
        total_delay_ms,
        global_shift_ms,
        Some(settings),
        runner,
        temp_dir,
        None,
    )
}

/// Run video-verified frame matching once per unique source.
///
/// Pre-computes the frame-corrected delays for all sources that have
/// subtitle tracks, storing them so that ALL subtitle tracks from each
/// source use the corrected delay.
///
/// # Arguments
/// * `sources_with_subs` - Set of source keys that have subtitle tracks
/// * `source_files` - Map from source key to video file path
/// * `source1_file` - Path to the reference (Source 1) video
/// * `settings` - Application settings
/// * `runner` - CommandRunner for logging
/// * `temp_dir` - Temp directory for caches
/// * `raw_source_delays` - Raw audio correlation delays per source
/// * `global_shift_ms` - Global shift value
///
/// # Returns
/// Map from source key to (corrected_delay_ms, details)
pub fn run_per_source_preprocessing(
    sources_with_subs: &[String],
    source_files: &HashMap<String, PathBuf>,
    source1_file: &Path,
    settings: &AppSettings,
    runner: &CommandRunner,
    temp_dir: Option<PathBuf>,
    raw_source_delays: &HashMap<String, f64>,
    global_shift_ms: f64,
) -> HashMap<String, (f64, HashMap<String, serde_json::Value>)> {
    runner.log_message(
        "[VideoVerified] =====================================================",
    );
    runner.log_message("[VideoVerified] Video-to-Video Frame Alignment");
    runner.log_message(
        "[VideoVerified] =====================================================",
    );
    runner.log_message(&format!(
        "[VideoVerified] Reference: Source 1 ({})",
        source1_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));

    if sources_with_subs.is_empty() {
        runner.log_message(
            "[VideoVerified] No subtitle tracks from other sources, skipping",
        );
        return HashMap::new();
    }

    runner.log_message(&format!(
        "[VideoVerified] Aligning: {} -> Source 1",
        sources_with_subs.join(", ")
    ));

    let mut results: HashMap<String, (f64, HashMap<String, serde_json::Value>)> = HashMap::new();

    // Detect Source 1 properties
    let _source1_props =
        detect_video_properties(&source1_file.to_string_lossy(), runner);

    for source_key in sources_with_subs {
        let source_video = match source_files.get(source_key) {
            Some(p) => p,
            None => {
                runner.log_message(&format!(
                    "[VideoVerified] WARNING: No video file for {}, skipping",
                    source_key
                ));
                continue;
            }
        };

        runner.log_message(&format!(
            "\n[VideoVerified] --- {} vs Source 1 ---",
            source_key
        ));

        let total_delay_ms = raw_source_delays
            .get(source_key)
            .copied()
            .unwrap_or(0.0);
        let original_delay = total_delay_ms;

        // Detect source video properties
        let source_props =
            detect_video_properties(&source_video.to_string_lossy(), runner);

        // Gate: skip frame matching for MPEG-2 (DVD) or interlaced content
        let codec = source_props
            .get("codec_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let is_mpeg2 = codec == "mpeg2video" || codec == "mpeg1video";
        let content_type = source_props
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if is_mpeg2 || content_type == "interlaced" {
            let reason = if is_mpeg2 { "MPEG-2" } else { "interlaced" };
            runner.log_message(&format!(
                "[VideoVerified] {}: {} content detected (type={}, codec={})",
                source_key, reason, content_type, codec
            ));
            runner.log_message(&format!(
                "[VideoVerified] {}: skipping frame matching, using audio correlation ({:+.1}ms)",
                source_key, original_delay
            ));

            let mut details = HashMap::new();
            details.insert(
                "reason".to_string(),
                serde_json::json!(format!("skipped-{}-content", reason)),
            );
            details.insert("content_type".to_string(), serde_json::json!(content_type));
            details.insert("codec".to_string(), serde_json::json!(codec));

            results.insert(source_key.clone(), (original_delay, details));
            continue;
        }

        // Calculate frame-corrected delay
        match calculate_offset_for_method(
            &source_video.to_string_lossy(),
            &source1_file.to_string_lossy(),
            total_delay_ms,
            global_shift_ms,
            settings,
            runner,
            temp_dir.clone(),
            source_key,
            None,
        ) {
            (Some(corrected_delay_ms), details) => {
                let frame_diff_ms = corrected_delay_ms - original_delay;
                runner.log_message(&format!(
                    "[VideoVerified] {} -> Source 1: {:+.3}ms (audio: {:+.3}ms, delta: {:+.3}ms)",
                    source_key, corrected_delay_ms, original_delay, frame_diff_ms
                ));
                results.insert(source_key.clone(), (corrected_delay_ms, details));
            }
            (None, details) => {
                runner.log_message(&format!(
                    "[VideoVerified] {}: frame matching failed, using audio correlation",
                    source_key
                ));
                results.insert(source_key.clone(), (original_delay, details));
            }
        }
    }

    runner.log_message(
        "\n[VideoVerified] =====================================================",
    );
    runner.log_message("[VideoVerified] Frame alignment complete");
    runner.log_message(
        "[VideoVerified] =====================================================\n",
    );

    results
}
