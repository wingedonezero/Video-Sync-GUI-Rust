//! Stepping correction entry points — 1:1 port of `vsg_core/correction/stepping/run.py`.
//!
//! `run_stepping_correction` is the main coordinator called by
//! `AudioCorrectionStep`. It orchestrates the pipeline:
//!
//!   1. Load dense analysis data from temp folder
//!   2. Build transition zones from clusters
//!   3. Refine boundaries with silence detection in Source 2
//!   4. Assemble corrected audio from EDL
//!   5. QA-check the result
//!   6. Apply the verified EDL to all audio tracks from the same source

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analysis::correlation::decode::get_audio_stream_info;
use crate::extraction::tracks::extract_tracks;
use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;
use crate::models::media::{StreamProps, Track};
use crate::models::settings::AppSettings;
use crate::orchestrator::steps::context::Context;

use super::audio_assembly::{assemble_corrected_audio, decode_to_memory, get_audio_properties};
use super::boundary_refiner::refine_boundaries;
use super::data_io::load_stepping_data;
use super::edl_builder::{build_segments_from_splice_points, find_transition_zones};
use super::qa_check::verify_correction;
use super::types::AudioSegment;

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Stepping correction coordinator — called by AudioCorrectionStep — `run_stepping_correction`
pub fn run_stepping_correction(ctx: &mut Context, runner: &CommandRunner) {
    let log = |msg: &str| {
        runner.log_message(msg);
    };

    let ref_file_path = ctx.sources.get("Source 1").cloned();

    // Collect segment_flags keys upfront to avoid borrow issues
    let segment_keys: Vec<String> = ctx.segment_flags.keys().cloned().collect();

    for analysis_track_key in &segment_keys {
        let source_key = analysis_track_key
            .split('_')
            .next()
            .unwrap_or("")
            .to_string();

        let flag_info = match ctx.segment_flags.get(analysis_track_key) {
            Some(f) => f.clone(),
            None => continue,
        };

        let subs_only = flag_info.subs_only.unwrap_or(false);

        // Find audio tracks from this source to correct
        let target_indices: Vec<usize> = ctx
            .extracted_items
            .as_ref()
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        item.track.source == source_key
                            && item.track.track_type == TrackType::Audio
                            && !item.is_preserved
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();

        if target_indices.is_empty() && !subs_only {
            log(&format!(
                "[SteppingCorrection] Skipping {source_key}: no audio tracks to correct."
            ));
            continue;
        }

        // --- Load dense data ---
        let data_path = match flag_info.stepping_data_path.as_deref() {
            Some(p) => p.to_string(),
            None => {
                log(&format!(
                    "[SteppingCorrection] ERROR: No dense data for {analysis_track_key}. Cannot proceed."
                ));
                continue;
            }
        };

        log(&format!(
            "[SteppingCorrection] Loading dense data from {}...",
            Path::new(&data_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));

        let stepping_data = match load_stepping_data(&data_path) {
            Ok(d) => d,
            Err(e) => {
                log(&format!("[SteppingCorrection] ERROR: {e}"));
                continue;
            }
        };

        log(&format!(
            "  {} windows, {} clusters",
            stepping_data.windows.len(),
            stepping_data.clusters.len()
        ));

        // --- Build transition zones from clusters ---
        let zones = find_transition_zones(
            &stepping_data,
            &flag_info,
            &ctx.settings,
            &log,
        );

        if zones.is_empty() {
            log("[SteppingCorrection] No transitions found. Audio delay appears uniform.");
            continue;
        }

        // --- Decode Source 2 mono PCM for silence detection ---
        let analysis_path = match find_analysis_track(ctx, analysis_track_key, runner) {
            Some(p) => p,
            None => continue,
        };

        let analysis_path_str = analysis_path.to_string_lossy().to_string();
        let (idx, _) =
            get_audio_stream_info(&analysis_path_str, None, runner, &ctx.tool_paths);
        let idx = match idx {
            Some(i) => i,
            None => {
                log(&format!("[ERROR] No audio stream in {analysis_path_str}"));
                continue;
            }
        };

        let (_, _, src2_sr) = match get_audio_properties(
            &analysis_path_str,
            idx,
            runner,
            &ctx.tool_paths,
        ) {
            Ok(props) => props,
            Err(e) => {
                log(&format!("[ERROR] {e}"));
                continue;
            }
        };

        let src2_pcm = match decode_to_memory(
            &analysis_path_str,
            idx,
            src2_sr,
            runner,
            &ctx.tool_paths,
            1, // mono
            Some(&log),
        ) {
            Some(pcm) => pcm,
            None => continue,
        };

        // --- Refine boundaries (silence detection in Source 2) ---
        let splice_points = refine_boundaries(
            &zones,
            &src2_pcm,
            src2_sr,
            &ctx.settings,
            &log,
            ref_file_path.as_deref(),
            Some(&ctx.tool_paths),
            Some(runner),
        );

        if splice_points.is_empty() {
            log("[SteppingCorrection] Boundary refinement produced no splice points.");
            continue;
        }

        // --- Build final EDL ---
        // Anchor = first cluster's delay
        let first_cluster = stepping_data
            .clusters
            .iter()
            .min_by(|a, b| {
                a.time_range
                    .0
                    .partial_cmp(&b.time_range.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        let first_cluster = match first_cluster {
            Some(c) => c,
            None => continue,
        };

        let anchor_ms = first_cluster.mean_delay_ms.round() as i32;
        let anchor_raw = first_cluster.mean_delay_ms;

        // Convert splice points to segment tuples
        let seg_tuples: Vec<(f64, f64, f64)> = splice_points
            .iter()
            .map(|sp| (sp.src2_time_s, sp.delay_after_ms, sp.delay_after_ms))
            .collect();

        let edl = build_segments_from_splice_points(
            anchor_ms,
            anchor_raw,
            &seg_tuples,
            &log,
        );

        if edl.len() <= 1 {
            log("[SteppingCorrection] Only one segment -- no stepping correction needed.");
            continue;
        }

        // Store EDL for subtitle adjustment (as JSON)
        let edl_json: Vec<serde_json::Value> = edl
            .iter()
            .map(|seg| serde_json::json!(seg))
            .collect();
        ctx.stepping_edls
            .insert(source_key.clone(), edl_json);

        if subs_only {
            log(&format!(
                "[SteppingCorrection] Subs-only mode -- EDL with {} segments stored.",
                edl.len()
            ));
            continue;
        }

        // --- QA: Assemble a mono check track and verify ---
        let qa_filename = format!("qa_{}.flac", source_key.replace(' ', "_"));
        let qa_path = ctx.temp_dir.join(&qa_filename);

        let qa_ok = assemble_corrected_audio(
            &edl,
            &analysis_path_str,
            &qa_path,
            runner,
            &ctx.tool_paths,
            &ctx.settings,
            &log,
            Some(1),
            Some("mono"),
            Some(src2_sr),
            Some(&src2_pcm),
        );

        if !qa_ok {
            log("[SteppingCorrection] QA assembly failed.");
            ctx.stepping_edls.remove(&source_key);
            continue;
        }

        let qa_path_str = qa_path.to_string_lossy().to_string();
        let (passed, _qa_meta) = verify_correction(
            &qa_path_str,
            ref_file_path.as_deref().unwrap_or(""),
            anchor_ms,
            &ctx.settings,
            runner,
            &ctx.tool_paths,
            &log,
            false,
        );

        if !passed {
            log("[SteppingCorrection] QA check FAILED -- skipping correction.");
            ctx.stepping_edls.remove(&source_key);
            continue;
        }

        // Store audit metadata from splice points for post-mux auditor
        if ctx.segment_flags.contains_key(analysis_track_key) {
            let boundary_audit: Vec<serde_json::Value> = splice_points
                .iter()
                .map(|sp| {
                    let br = sp.boundary_result.as_ref();
                    serde_json::json!({
                        "target_time_s": sp.src2_time_s,
                        "delay_change_ms": sp.correction_ms,
                        "no_silence_found": sp.silence_zone.is_none(),
                        "zone_start": sp.silence_zone.as_ref().map(|z| z.start_s).unwrap_or(0.0),
                        "zone_end": sp.silence_zone.as_ref().map(|z| z.end_s).unwrap_or(0.0),
                        "avg_db": sp.silence_zone.as_ref().map(|z| z.avg_db).unwrap_or(0.0),
                        "score": br.map(|b| b.score).unwrap_or(0.0),
                        "overlaps_speech": br.map(|b| b.overlaps_speech).unwrap_or(false),
                        "near_transient": br.map(|b| b.near_transient).unwrap_or(false),
                        "video_snap_skipped": sp.snap_metadata.get("video_snap_skipped")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    })
                })
                .collect();

            if let Some(flags) = ctx.segment_flags.get_mut(analysis_track_key) {
                flags.audit_metadata = Some(boundary_audit);
            }
        }

        // --- Apply to all audio tracks from this source ---
        log(&format!(
            "[SteppingCorrection] QA passed -- applying to {} audio track(s).",
            target_indices.len()
        ));

        for &idx in &target_indices {
            let items = ctx.extracted_items.as_ref().unwrap();
            let target_item = &items[idx];

            let extracted_path = match &target_item.extracted_path {
                Some(p) => p.clone(),
                None => continue,
            };

            let corrected_filename = format!(
                "corrected_{}.flac",
                extracted_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            let corrected_path = ctx.temp_dir.join(&corrected_filename);
            let extracted_path_str = extracted_path.to_string_lossy().to_string();

            let ok = assemble_corrected_audio(
                &edl,
                &extracted_path_str,
                &corrected_path,
                runner,
                &ctx.tool_paths,
                &ctx.settings,
                &log,
                None,
                None,
                None,
                None,
            );

            if !ok {
                log(&format!(
                    "[ERROR] Assembly failed for {} -- keeping original.",
                    extracted_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ));
                continue;
            }

            swap_corrected_track(ctx, idx, &corrected_path, &ctx.settings.clone(), &log);
        }

        // Drop src2_pcm to free memory (it goes out of scope naturally)
    }
}

// ---------------------------------------------------------------------------
// apply_plan_to_file (kept for external use / subtitle EDL replay)
// ---------------------------------------------------------------------------

/// Apply a pre-generated EDL to a given audio file — `apply_plan_to_file`
///
/// Returns the path to the corrected FLAC, or `None` on failure.
pub fn apply_plan_to_file(
    target_audio_path: &str,
    edl: &[AudioSegment],
    temp_dir: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    settings: &AppSettings,
    log: Option<&dyn Fn(&str)>,
) -> Option<PathBuf> {
    let noop = |_: &str| {};
    let log_fn: &dyn Fn(&str) = match log {
        Some(l) => l,
        None => &noop,
    };

    let corrected_filename = format!(
        "corrected_{}.flac",
        Path::new(target_audio_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let corrected_path = temp_dir.join(&corrected_filename);

    let ok = assemble_corrected_audio(
        edl,
        target_audio_path,
        &corrected_path,
        runner,
        tool_paths,
        settings,
        log_fn,
        None,
        None,
        None,
        None,
    );

    if ok {
        Some(corrected_path)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Locate the extracted analysis audio track, extracting if needed — `_find_analysis_track`
fn find_analysis_track(
    ctx: &Context,
    analysis_track_key: &str,
    runner: &CommandRunner,
) -> Option<PathBuf> {
    // Check if the analysis track is already among extracted items
    if let Some(items) = ctx.extracted_items.as_ref() {
        for item in items {
            let key = format!("{}_{}", item.track.source, item.track.id);
            if key == analysis_track_key && item.track.track_type == TrackType::Audio {
                if let Some(ref path) = item.extracted_path {
                    return Some(path.clone());
                }
            }
        }
    }

    // Not in layout -- extract internally
    let parts: Vec<&str> = analysis_track_key.splitn(2, '_').collect();
    if parts.len() < 2 {
        return None;
    }
    let source_key = parts[0];
    let track_id: i32 = match parts[1].parse() {
        Ok(id) => id,
        Err(_) => return None,
    };

    let source_path = ctx.sources.get(source_key)?;

    runner.log_message(&format!(
        "[SteppingCorrection] Analysis track {analysis_track_key} \
         not in layout -- extracting internally..."
    ));

    let role = format!("{source_key}_internal");
    match extract_tracks(
        source_path,
        &ctx.temp_dir,
        runner,
        &ctx.tool_paths,
        &role,
        Some(&[track_id]),
    ) {
        Ok(tracks) => {
            tracks.first().map(|first| PathBuf::from(&first.path))
        }
        Err(e) => {
            runner.log_message(&format!(
                "[ERROR] Internal extraction failed for {analysis_track_key}: {e}"
            ));
            None
        }
    }
}

/// Preserve the original and point the item to the corrected FLAC — `_swap_corrected_track`
fn swap_corrected_track(
    ctx: &mut Context,
    target_idx: usize,
    corrected_path: &Path,
    settings: &AppSettings,
    log: &dyn Fn(&str),
) {
    let items = match ctx.extracted_items.as_mut() {
        Some(items) => items,
        None => return,
    };

    let target_item = &items[target_idx];
    let original_props = target_item.track.props.clone();

    // Build preserved item
    let preserved_label = &settings.stepping_preserved_track_label;
    let preserved_name = if !preserved_label.is_empty() {
        if !original_props.name.is_empty() {
            format!("{} ({})", original_props.name, preserved_label)
        } else {
            preserved_label.clone()
        }
    } else {
        original_props.name.clone()
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

    // Update main track -> corrected FLAC
    let corrected_label = &settings.stepping_corrected_track_label;
    let corrected_name = if !corrected_label.is_empty() {
        if !original_props.name.is_empty() {
            format!("{} ({})", original_props.name, corrected_label)
        } else {
            corrected_label.clone()
        }
    } else {
        original_props.name.clone()
    };

    let target_item = &mut items[target_idx];
    target_item.extracted_path = Some(corrected_path.to_path_buf());
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

    log(&format!(
        "[SUCCESS] Stepping correction applied for {}",
        if !original_props.name.is_empty() {
            &original_props.name
        } else {
            "track"
        }
    ));
}
