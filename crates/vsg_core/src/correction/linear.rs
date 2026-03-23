//! Linear drift correction — 1:1 port of `vsg_core/correction/linear.py`.
//!
//! Corrects constant audio drift by resampling the audio speed via ffmpeg.
//! Creates preserved copies of original tracks.

use std::collections::HashMap;

use crate::io::runner::CommandRunner;
use crate::models::enums::{ResampleEngine, TrackType};
use crate::models::media::{StreamProps, Track};
use crate::orchestrator::steps::context::Context;

/// Helper to get the sample rate of the first audio stream via ffprobe.
fn get_sample_rate(
    file_path: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> i32 {
    let cmd: Vec<&str> = vec![
        "ffprobe",
        "-v", "error",
        "-select_streams", "a:0",
        "-show_entries", "stream=sample_rate",
        "-of", "json",
        file_path,
    ];

    let out = match runner.run(&cmd, tool_paths) {
        Some(o) => o,
        None => {
            runner.log_message("[WARN] Could not probe sample rate, defaulting to 48000 Hz.");
            return 48000;
        }
    };

    match serde_json::from_str::<serde_json::Value>(&out) {
        Ok(val) => {
            if let Some(streams) = val.get("streams").and_then(|v| v.as_array()) {
                if let Some(first) = streams.first() {
                    if let Some(sr) = first.get("sample_rate").and_then(|v| v.as_str()) {
                        return sr.parse::<i32>().unwrap_or(48000);
                    }
                    if let Some(sr) = first.get("sample_rate").and_then(|v| v.as_i64()) {
                        return sr as i32;
                    }
                }
            }
            runner.log_message("[WARN] Failed to parse sample rate, defaulting to 48000 Hz.");
            48000
        }
        Err(_) => {
            runner.log_message("[WARN] Failed to parse sample rate, defaulting to 48000 Hz.");
            48000
        }
    }
}

/// Corrects constant audio drift by resampling the audio speed — `run_linear_correction`
pub fn run_linear_correction(ctx: &mut Context, runner: &CommandRunner) {
    let linear_flags: Vec<(String, f64)> = ctx
        .linear_drift_flags
        .iter()
        .map(|(k, v)| {
            let rate = v.rate.unwrap_or(0.0);
            (k.clone(), rate)
        })
        .collect();

    for (analysis_track_key, drift_rate_ms_s) in linear_flags {
        let source_key = analysis_track_key.split('_').next().unwrap_or("").to_string();

        let extracted_items = match ctx.extracted_items.as_ref() {
            Some(items) => items,
            None => continue,
        };

        // Find ALL audio tracks from this source that are not preserved
        let target_indices: Vec<usize> = extracted_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.track.source == source_key
                    && item.track.track_type == TrackType::Audio
                    && !item.is_preserved
            })
            .map(|(i, _)| i)
            .collect();

        if target_indices.is_empty() {
            runner.log_message(&format!(
                "[LinearCorrector] Could not find target audio tracks for {source_key} in the layout. Skipping."
            ));
            continue;
        }

        runner.log_message(&format!(
            "[LinearCorrector] Applying drift correction to {} track(s) from {source_key} (rate: {drift_rate_ms_s:.2} ms/s)...",
            target_indices.len()
        ));

        // Process each target track
        for &idx in &target_indices {
            let items = ctx.extracted_items.as_ref().unwrap();
            let target_item = &items[idx];

            let original_path = match &target_item.extracted_path {
                Some(p) => p.clone(),
                None => continue,
            };

            let corrected_path = original_path
                .parent()
                .unwrap_or(original_path.as_path())
                .join(format!(
                    "drift_corrected_{}.flac",
                    original_path.file_stem().unwrap_or_default().to_string_lossy()
                ));

            let tempo_ratio = 1000.0 / (1000.0 + drift_rate_ms_s);
            let sample_rate = get_sample_rate(
                &original_path.to_string_lossy(),
                runner,
                &ctx.tool_paths,
            );

            let resample_engine = &ctx.settings.segment_resample_engine;

            let filter_chain = match resample_engine {
                ResampleEngine::Rubberband => {
                    runner.log_message(
                        "    - Using 'rubberband' engine for high-quality resampling."
                    );
                    let mut rb_opts = vec![format!("tempo={tempo_ratio}")];

                    if !ctx.settings.segment_rb_pitch_correct {
                        rb_opts.push(format!("pitch={tempo_ratio}"));
                    }

                    rb_opts.push(format!("transients={}", ctx.settings.segment_rb_transients));

                    if ctx.settings.segment_rb_smoother {
                        rb_opts.push("smoother=on".to_string());
                    }

                    if ctx.settings.segment_rb_pitchq {
                        rb_opts.push("pitchq=on".to_string());
                    }

                    format!("rubberband={}", rb_opts.join(":"))
                }
                ResampleEngine::Atempo => {
                    runner.log_message("    - Using 'atempo' engine for fast resampling.");
                    format!("atempo={tempo_ratio}")
                }
                ResampleEngine::Aresample => {
                    runner.log_message(
                        "    - Using 'aresample' engine for high-quality resampling."
                    );
                    let new_sample_rate = sample_rate as f64 * tempo_ratio;
                    format!("asetrate={new_sample_rate},aresample={sample_rate}")
                }
            };

            let original_path_str = original_path.to_string_lossy().to_string();
            let corrected_path_str = corrected_path.to_string_lossy().to_string();

            let resample_cmd: Vec<&str> = vec![
                "ffmpeg",
                "-y",
                "-nostdin",
                "-v", "error",
                "-i", &original_path_str,
                "-af", &filter_chain,
                "-c:a", "flac",
                &corrected_path_str,
            ];

            if runner.run(&resample_cmd, &ctx.tool_paths).is_none() {
                let engine_str = resample_engine.to_string();
                let mut error_msg = format!(
                    "Linear drift correction with '{}' failed for {}.",
                    engine_str,
                    original_path.file_name().unwrap_or_default().to_string_lossy()
                );
                if matches!(resample_engine, ResampleEngine::Rubberband) {
                    error_msg.push_str(" (Ensure your FFmpeg build includes 'librubberband').");
                }
                runner.log_message(&format!("[ERROR] {error_msg}"));
                continue;
            }

            runner.log_message(&format!(
                "[SUCCESS] Linear drift correction successful for '{}'",
                original_path.file_name().unwrap_or_default().to_string_lossy()
            ));

            // Now mutate: preserve original and update main track
            let items = ctx.extracted_items.as_mut().unwrap();
            let target_item = &items[idx];

            // Build preserved item
            let original_props = target_item.track.props.clone();
            let preserved_name = if !original_props.name.is_empty() {
                format!("{} (Original)", original_props.name)
            } else {
                "Original".to_string()
            };
            let mut preserved_item = target_item.clone();
            preserved_item.is_preserved = true;
            preserved_item.is_default = false;
            preserved_item.track = Track {
                source: preserved_item.track.source.clone(),
                id: preserved_item.track.id,
                track_type: preserved_item.track.track_type,
                props: StreamProps {
                    codec_id: original_props.codec_id.clone(),
                    lang: original_props.lang.clone(),
                    name: preserved_name,
                },
            };

            // Update the main track to point to corrected FLAC
            let corrected_name = if !original_props.name.is_empty() {
                format!("{} (Drift Corrected)", original_props.name)
            } else {
                "Drift Corrected".to_string()
            };

            let target_item = &mut items[idx];
            target_item.extracted_path = Some(corrected_path.clone());
            target_item.is_corrected = true;
            target_item.container_delay_ms = 0;
            target_item.track = Track {
                source: target_item.track.source.clone(),
                id: target_item.track.id,
                track_type: target_item.track.track_type,
                props: StreamProps {
                    codec_id: "FLAC".to_string(),
                    lang: original_props.lang.clone(),
                    name: corrected_name,
                },
            };
            target_item.apply_track_name = true;

            items.push(preserved_item);
        }
    }
}
