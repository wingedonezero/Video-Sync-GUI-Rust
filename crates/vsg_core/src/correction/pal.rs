//! PAL speed correction — 1:1 port of `vsg_core/correction/pal.py`.
//!
//! Corrects audio drift due to PAL speed-up using a pitch-corrected
//! rubberband tempo adjustment via ffmpeg.

use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::models::media::{StreamProps, Track};
use crate::orchestrator::steps::context::Context;

/// Corrects audio drift due to PAL speed-up — `run_pal_correction`
pub fn run_pal_correction(ctx: &mut Context, runner: &CommandRunner) {
    let pal_flags: Vec<String> = ctx
        .pal_drift_flags
        .keys()
        .cloned()
        .collect();

    for analysis_track_key in pal_flags {
        let source_key = analysis_track_key
            .split('_')
            .next()
            .unwrap_or("")
            .to_string();

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
                "[PALCorrector] Could not find target audio tracks for {source_key} in the layout. Skipping."
            ));
            continue;
        }

        runner.log_message(&format!(
            "[PALCorrector] Applying PAL speed correction to {} track(s) from {source_key}...",
            target_indices.len()
        ));

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
                    "pal_corrected_{}.flac",
                    original_path.file_stem().unwrap_or_default().to_string_lossy()
                ));

            // PAL tempo ratio: slow down 25fps content to match 23.976fps
            let tempo_ratio = (24000.0 / 1001.0) / 25.0;
            let filter_arg = format!("rubberband=tempo={tempo_ratio}");

            let original_path_str = original_path.to_string_lossy().to_string();
            let corrected_path_str = corrected_path.to_string_lossy().to_string();

            let cmd: Vec<&str> = vec![
                "ffmpeg",
                "-y",
                "-nostdin",
                "-v", "error",
                "-i", &original_path_str,
                "-af", &filter_arg,
                "-c:a", "flac",
                &corrected_path_str,
            ];

            if runner.run(&cmd, &ctx.tool_paths).is_none() {
                runner.log_message(&format!(
                    "[ERROR] PAL drift correction failed for {}. \
                     This may be because your ffmpeg build lacks librubberband support.",
                    original_path.file_name().unwrap_or_default().to_string_lossy()
                ));
                continue;
            }

            runner.log_message(&format!(
                "[SUCCESS] PAL correction successful for '{}'",
                original_path.file_name().unwrap_or_default().to_string_lossy()
            ));

            // Build preserved item
            let items = ctx.extracted_items.as_mut().unwrap();
            let target_item = &items[idx];
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

            // Update main track to point to corrected FLAC
            let corrected_name = if !original_props.name.is_empty() {
                format!("{} (PAL Corrected)", original_props.name)
            } else {
                "PAL Corrected".to_string()
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
