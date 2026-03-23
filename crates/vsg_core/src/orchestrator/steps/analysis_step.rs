//! Analysis step — 1:1 port of `vsg_core/orchestrator/steps/analysis_step.py`.
//!
//! Pure coordinator: orchestrates audio/video correlation analysis by
//! delegating to the analysis/ module functions. No business logic lives here.

use std::collections::HashMap;
use std::path::Path;

use crate::analysis::container_delays::{
    calculate_delay_chain, find_actual_correlation_track_delay, get_container_delay_info,
};
use crate::analysis::correlation::dense::run_dense_correlation;
use crate::analysis::correlation::filtering::{apply_bandpass, apply_lowpass};
use crate::analysis::correlation::gpu_backend::cleanup_gpu;
use crate::analysis::correlation::run::resolve_method;
use crate::analysis::correlation::{
    decode_audio, get_audio_stream_info, list_methods, normalize_lang, DEFAULT_SR,
};
use crate::analysis::delay_selection::{calculate_delay, find_first_stable_segment_delay};
use crate::analysis::drift_detection::diagnose_audio_issue;
use crate::analysis::global_shift::{apply_global_shift_to_delays, calculate_global_shift};
use crate::analysis::sync_stability::analyze_sync_stability;
use crate::analysis::track_selection::{format_track_details, select_audio_track};
use crate::analysis::types::{ChunkResult, ContainerDelayInfo, DiagnosisResult};
use crate::analysis::videodiff::run_native_videodiff;
use crate::correction::stepping::data_io::save_stepping_data;
use crate::extraction::tracks::get_stream_info;
use crate::io::runner::CommandRunner;
use crate::models::context_types::{DriftFlagsEntry, SegmentFlagsEntry};
use crate::models::enums::{FilteringMethod, SourceSeparationMode, SyncMode};
use crate::models::jobs::Delays;
use crate::models::settings::AppSettings;

use super::context::Context;

// ─── Helper: should source use source-separated mode? ────────────────────────

fn should_use_source_separated_mode(
    source_key: &str,
    settings: &AppSettings,
    source_settings: &HashMap<String, serde_json::Value>,
) -> bool {
    if matches!(settings.source_separation_mode, SourceSeparationMode::None) {
        return false;
    }
    let per_source = match source_settings.get(source_key) {
        Some(v) => v,
        None => return false,
    };
    per_source
        .get("use_source_separation")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

// ─── Helper: apply source separation (stubbed) ──────────────────────────────

fn apply_source_separation_if_needed(
    ref_pcm: Vec<f32>,
    tgt_pcm: Vec<f32>,
    _sr: i64,
    settings: &AppSettings,
    log: &dyn Fn(&str),
    _role_tag: &str,
) -> (Vec<f32>, Vec<f32>) {
    let mode = settings.source_separation_mode.to_string();
    if mode.is_empty() || mode == "none" {
        return (ref_pcm, tgt_pcm);
    }

    // Source separation is stubbed in the Rust port
    log("WARNING: Source separation was enabled but is not yet implemented in the Rust port!");
    log("[SOURCE SEPARATION] Falling back to standard correlation without separation.");

    (ref_pcm, tgt_pcm)
}

// ─── Helper: apply filtering ────────────────────────────────────────────────

fn apply_filtering(
    mut ref_pcm: Vec<f32>,
    mut tgt_pcm: Vec<f32>,
    sr: i64,
    settings: &AppSettings,
    log: &dyn Fn(&str),
) -> (Vec<f32>, Vec<f32>) {
    match settings.filtering_method {
        FilteringMethod::DialogueBandPass => {
            log("Applying Dialogue Band-Pass filter...");
            let lowcut = settings.filter_bandpass_lowcut_hz;
            let highcut = settings.filter_bandpass_highcut_hz;
            let order = settings.filter_bandpass_order;
            ref_pcm = apply_bandpass(&ref_pcm, sr as i32, lowcut, highcut, order, Some(log));
            tgt_pcm = apply_bandpass(&tgt_pcm, sr as i32, lowcut, highcut, order, Some(log));
        }
        FilteringMethod::LowPass => {
            let cutoff = settings.audio_bandlimit_hz;
            if cutoff > 0 {
                log(&format!("Applying Low-Pass filter at {cutoff} Hz..."));
                let taps = settings.filter_lowpass_taps;
                ref_pcm = apply_lowpass(&ref_pcm, sr as i32, cutoff, taps, Some(log));
                tgt_pcm = apply_lowpass(&tgt_pcm, sr as i32, cutoff, taps, Some(log));
            }
        }
        FilteringMethod::NoFilter => {}
    }
    (ref_pcm, tgt_pcm)
}

// ─── AnalysisStep ───────────────────────────────────────────────────────────

/// Orchestrates audio/video correlation analysis — `AnalysisStep`
pub struct AnalysisStep;

impl AnalysisStep {
    /// Run the analysis step.
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        let source1_file = ctx
            .sources
            .get("Source 1")
            .cloned()
            .ok_or_else(|| "Context is missing Source 1 for analysis.".to_string())?;

        // --- Part 1: Determine if a global shift is required ---
        let has_secondary_audio = ctx.manual_layout.iter().any(|t| {
            t.track_type.as_deref() == Some("audio") && t.source.as_deref() != Some("Source 1")
        });
        ctx.sync_mode = ctx.settings.sync_mode.to_string();

        (ctx.log)(&"=".repeat(60));
        (ctx.log)(&format!(
            "=== TIMING SYNC MODE: {} ===",
            ctx.settings.sync_mode.to_string().to_uppercase()
        ));
        (ctx.log)(&"=".repeat(60));

        match ctx.settings.sync_mode {
            SyncMode::AllowNegative => {
                ctx.global_shift_is_required = false;
                (ctx.log)("[SYNC MODE] Negative delays are ALLOWED (no global shift).");
                (ctx.log)("[SYNC MODE] Source 1 remains reference (delay = 0).");
                (ctx.log)("[SYNC MODE] Secondary sources can have negative delays.");
            }
            SyncMode::PositiveOnly => {
                ctx.global_shift_is_required = has_secondary_audio;
                if ctx.global_shift_is_required {
                    (ctx.log)(
                        "[SYNC MODE] Positive-only mode - global shift will \
                         eliminate negative delays.",
                    );
                    (ctx.log)("[SYNC MODE] All tracks will be shifted to be non-negative.");
                } else {
                    (ctx.log)("[SYNC MODE] Positive-only mode (but no secondary audio detected).");
                    (ctx.log)(
                        "[SYNC MODE] Global shift will not be applied \
                         (subtitle-only exception).",
                    );
                }
            }
        }

        // Skip analysis if only Source 1 (remux-only mode)
        if ctx.sources.len() == 1 {
            (ctx.log)("--- Analysis Phase: Skipped (Remux-only mode - no sync sources) ---");
            ctx.delays = Some(Delays {
                source_delays_ms: HashMap::new(),
                raw_source_delays_ms: HashMap::new(),
                global_shift_ms: 0,
                raw_global_shift_ms: 0.0,
            });
            return Ok(());
        }

        let mut source_delays: HashMap<String, i32> = HashMap::new();
        let mut raw_source_delays: HashMap<String, f64> = HashMap::new();

        // --- Step 1: Get Source 1's container delays ---
        (ctx.log)("--- Getting Source 1 Container Delays for Analysis ---");
        let source1_container_info = get_container_delay_info(
            &source1_file,
            runner,
            &ctx.tool_paths,
            &*ctx.log,
            "Source 1",
        );

        let mut source1_audio_container_delay = 0.0_f64;
        let source1_video_container_delay;
        let mut source1_stream_info: Option<serde_json::Value> = None;

        if let Some(ref info) = source1_container_info {
            source1_video_container_delay = info.video_delay_ms;

            let ref_lang = ctx.settings.analysis_lang_source1.clone();
            source1_stream_info = get_stream_info(&source1_file, runner, &ctx.tool_paths);

            if let Some(ref si) = source1_stream_info {
                let empty = vec![];
                let tracks = si
                    .get("tracks")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                let audio_tracks_owned: Vec<serde_json::Value> = tracks
                    .iter()
                    .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("audio"))
                    .cloned()
                    .collect();

                let source1_per = ctx.source_settings.get("Source 1");
                let correlation_ref_track = source1_per
                    .and_then(|v| v.get("correlation_ref_track"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32);

                let source1_track_selection = select_audio_track(
                    &audio_tracks_owned,
                    if ref_lang.is_empty() {
                        None
                    } else {
                        Some(ref_lang.as_str())
                    },
                    correlation_ref_track,
                    &*ctx.log,
                    "Source 1",
                );

                if let Some(ref sel) = source1_track_selection {
                    source1_audio_container_delay = info
                        .audio_delays_ms
                        .get(&sel.track_id)
                        .copied()
                        .unwrap_or(0.0);
                    ctx.source1_audio_container_delay_ms = source1_audio_container_delay;

                    if source1_audio_container_delay != 0.0 {
                        (ctx.log)(&format!(
                            "[Container Delay] Audio track {} relative delay \
                             (audio relative to video): {:+.1}ms. \
                             This will be added to all correlation results.",
                            sel.track_id, source1_audio_container_delay
                        ));
                    }
                }
            }
        } else {
            source1_video_container_delay = 0.0;
        }

        // --- Step 2: Run correlation/videodiff for other sources ---
        let is_videodiff_mode =
            ctx.settings.analysis_mode.to_string() == "VideoDiff"
                || ctx.settings.correlation_method.to_string() == "VideoDiff";

        if is_videodiff_mode {
            (ctx.log)("\n--- Running VideoDiff (Frame Matching) Analysis ---");
        } else {
            (ctx.log)("\n--- Running Audio Correlation Analysis ---");
        }

        let mut stepping_sources: Vec<String> = Vec::new();

        let sorted_sources: Vec<(String, String)> = {
            let mut v: Vec<(String, String)> = ctx
                .sources
                .iter()
                .filter(|(k, _)| k.as_str() != "Source 1")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };

        for (source_key, source_file) in &sorted_sources {
            (ctx.log)(&format!("\n[Analyzing {source_key}]"));

            if is_videodiff_mode {
                self.run_videodiff_analysis(
                    ctx,
                    source_key,
                    source_file,
                    &source1_file,
                    source1_video_container_delay,
                    &mut source_delays,
                    &mut raw_source_delays,
                )?;
                continue;
            }

            // Audio correlation mode
            self.run_audio_analysis(
                ctx,
                runner,
                source_key,
                source_file,
                &source1_file,
                source1_audio_container_delay,
                source1_container_info.as_ref(),
                source1_stream_info.as_ref(),
                &mut source_delays,
                &mut raw_source_delays,
                &mut stepping_sources,
            )?;
        }

        // Store stepping sources in context
        ctx.stepping_sources = stepping_sources;

        // Initialize Source 1 with 0ms base delay
        source_delays.insert("Source 1".to_string(), 0);
        raw_source_delays.insert("Source 1".to_string(), 0.0);

        // --- Step 3: Calculate Global Shift ---
        (ctx.log)("\n--- Calculating Global Shift ---");

        let shift = calculate_global_shift(
            &source_delays,
            &raw_source_delays,
            &ctx.manual_layout,
            source1_container_info.as_ref(),
            ctx.global_shift_is_required,
            &*ctx.log,
        );

        if shift.applied {
            let (updated_delays, updated_raw) = apply_global_shift_to_delays(
                &source_delays,
                &raw_source_delays,
                &shift,
                &*ctx.log,
            );
            source_delays = updated_delays;
            raw_source_delays = updated_raw;

            if let (Some(ref info), Some(ref si)) =
                (&source1_container_info, &source1_stream_info)
            {
                (ctx.log)(&format!(
                    "[Delay] Source 1 container delays \
                     (will have +{}ms added during mux):",
                    shift.shift_ms
                ));
                let empty = vec![];
                let tracks = si
                    .get("tracks")
                    .and_then(|v| v.as_array())
                    .unwrap_or(&empty);
                for track in tracks {
                    let track_type = track
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if track_type != "audio" && track_type != "video" {
                        continue;
                    }
                    let tid = track
                        .get("id")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    let delay = if track_type == "audio" {
                        info.audio_delays_ms.get(&tid).copied().unwrap_or(0.0)
                    } else {
                        info.video_delay_ms
                    };
                    let final_delay = delay + shift.shift_ms as f64;
                    let note = if track_type == "video" {
                        " (will be ignored - video defines timeline)"
                    } else {
                        ""
                    };
                    (ctx.log)(&format!(
                        "  - Track {tid} ({track_type}): \
                         {delay:+.1} -> {final_delay:+.1}ms{note}"
                    ));
                }
            }
        }

        // Store calculated delays
        let sync_mode_str = ctx.sync_mode.clone();
        ctx.delays = Some(Delays {
            source_delays_ms: source_delays.clone(),
            raw_source_delays_ms: raw_source_delays.clone(),
            global_shift_ms: shift.shift_ms,
            raw_global_shift_ms: shift.raw_shift_ms,
        });

        // Final summary
        (ctx.log)(&format!(
            "\n[Delay] === FINAL DELAYS (Sync Mode: {}, Global Shift: +{}ms) ===",
            sync_mode_str.to_uppercase(),
            shift.shift_ms
        ));
        let mut sorted_delay_keys: Vec<&String> = source_delays.keys().collect();
        sorted_delay_keys.sort();
        for source_key in &sorted_delay_keys {
            let delay_ms = source_delays[*source_key];
            let raw_ms = raw_source_delays[*source_key];
            (ctx.log)(&format!(
                "  - {source_key}: {delay_ms:+}ms (raw: {raw_ms:+.6}ms)"
            ));
        }

        if sync_mode_str == "allow_negative" && shift.shift_ms == 0 {
            (ctx.log)(
                "\n[INFO] Negative delays retained (allow_negative mode). \
                 Secondary sources may have negative delays.",
            );
        } else if shift.shift_ms > 0 {
            (ctx.log)(&format!(
                "\n[INFO] All delays shifted by +{}ms to eliminate negatives.",
                shift.shift_ms
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------
    // Private helpers - each handles one analysis path
    // -----------------------------------------------------------------

    fn run_videodiff_analysis(
        &self,
        ctx: &Context,
        source_key: &str,
        source_file: &str,
        source1_file: &str,
        source1_video_container_delay: f64,
        source_delays: &mut HashMap<String, i32>,
        raw_source_delays: &mut HashMap<String, f64>,
    ) -> Result<(), String> {
        let log = &*ctx.log;

        let vd_result = run_native_videodiff(
            source1_file,
            source_file,
            &ctx.settings,
            log,
        )
        .ok_or_else(|| {
            format!("VideoDiff analysis failed for {source_key}")
        })?;

        let correlation_delay_ms = vd_result.offset_ms;
        let correlation_delay_raw = vd_result.raw_offset_ms;
        let actual_container_delay = source1_video_container_delay;

        let (final_delay_ms, final_delay_raw) = calculate_delay_chain(
            correlation_delay_ms,
            correlation_delay_raw,
            actual_container_delay,
            log,
            source_key,
        );

        log(&format!(
            "[VideoDiff] Confidence: {} (inliers: {}/{}, residual: {:.1}ms)",
            vd_result.confidence,
            vd_result.inlier_count,
            vd_result.matched_frames,
            vd_result.mean_residual_ms
        ));

        if vd_result.speed_drift_detected {
            log(
                "[VideoDiff] WARNING: Speed drift detected between sources. \
                 The offset is valid but timing may drift over the video duration.",
            );
        }

        source_delays.insert(source_key.to_string(), final_delay_ms);
        raw_source_delays.insert(source_key.to_string(), final_delay_raw);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_audio_analysis(
        &self,
        ctx: &mut Context,
        runner: &CommandRunner,
        source_key: &str,
        source_file: &str,
        source1_file: &str,
        source1_audio_container_delay: f64,
        source1_container_info: Option<&ContainerDelayInfo>,
        source1_stream_info: Option<&serde_json::Value>,
        source_delays: &mut HashMap<String, i32>,
        raw_source_delays: &mut HashMap<String, f64>,
        stepping_sources: &mut Vec<String>,
    ) -> Result<(), String> {
        // --- Get per-source settings ---
        let per_source_settings = ctx.source_settings.get(source_key).cloned();
        let correlation_source_track = per_source_settings
            .as_ref()
            .and_then(|v| v.get("correlation_source_track"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);
        let source1_settings_val = ctx.source_settings.get("Source 1").cloned();
        let correlation_ref_track = source1_settings_val
            .as_ref()
            .and_then(|v| v.get("correlation_ref_track"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);

        // Determine target language
        let tgt_lang = if correlation_source_track.is_some() {
            None
        } else {
            let lang = ctx.settings.analysis_lang_others.clone();
            if lang.is_empty() { None } else { Some(lang) }
        };

        // Log Source 1 track selection if per-job override exists
        if let (Some(ref_track), Some(si)) = (correlation_ref_track, source1_stream_info) {
            let empty = vec![];
            let tracks = si
                .get("tracks")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty);
            let audio_tracks: Vec<&serde_json::Value> = tracks
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("audio"))
                .collect();
            if ref_track >= 0 && (ref_track as usize) < audio_tracks.len() {
                let ref_track_val = audio_tracks[ref_track as usize].clone();
                (ctx.log)(&format!(
                    "[Source 1] Selected (explicit): {}",
                    format_track_details(&ref_track_val, ref_track)
                ));
            } else {
                (ctx.log)(&format!(
                    "[Source 1] WARNING: Invalid track index {ref_track}, \
                     using previously selected track"
                ));
            }
        }

        // Determine if source separation should be applied
        let use_source_separated_settings = should_use_source_separated_mode(
            source_key,
            &ctx.settings,
            &ctx.source_settings,
        );

        // Determine effective delay selection mode
        let effective_delay_mode = if use_source_separated_settings {
            let mode = ctx.settings.delay_selection_mode_source_separated.to_string();
            (ctx.log)("[Analysis Config] Source separation enabled - using:");
            (ctx.log)(&format!(
                "  Correlation: {}",
                ctx.settings.correlation_method_source_separated
            ));
            (ctx.log)(&format!("  Delay Mode: {mode}"));
            mode
        } else {
            let mode = ctx.settings.delay_selection_mode.to_string();
            (ctx.log)("[Analysis Config] Standard mode - using:");
            (ctx.log)(&format!("  Correlation: {}", ctx.settings.correlation_method));
            (ctx.log)(&format!("  Delay Mode: {mode}"));
            mode
        };

        // --- Get stream info and select target track ---
        let stream_info = get_stream_info(source_file, runner, &ctx.tool_paths);
        let stream_info = match stream_info {
            Some(si) => si,
            None => {
                (ctx.log)(&format!(
                    "[WARN] Could not get stream info for {source_key}. Skipping."
                ));
                return Ok(());
            }
        };

        let empty = vec![];
        let tracks = stream_info
            .get("tracks")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        let audio_tracks_owned: Vec<serde_json::Value> = tracks
            .iter()
            .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("audio"))
            .cloned()
            .collect();

        if audio_tracks_owned.is_empty() {
            (ctx.log)(&format!(
                "[WARN] No audio tracks found in {source_key}. Skipping."
            ));
            return Ok(());
        }

        let target_track_selection = select_audio_track(
            &audio_tracks_owned,
            tgt_lang.as_deref(),
            correlation_source_track,
            &*ctx.log,
            source_key,
        );

        let target_track_selection = match target_track_selection {
            Some(sel) => sel,
            None => {
                (ctx.log)(&format!(
                    "[WARN] No suitable audio track found in {source_key} \
                     for analysis. Skipping."
                ));
                return Ok(());
            }
        };

        // Update to the validated index
        let correlation_source_track = Some(target_track_selection.track_index);

        let target_track_id = target_track_selection.track_id;
        let target_codec_id = audio_tracks_owned
            .get(target_track_selection.track_index as usize)
            .and_then(|t| t.get("properties"))
            .and_then(|p| p.get("codec_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // --- Decode, separate, filter, chunk, correlate ---
        let results = self.decode_and_correlate(
            ctx,
            runner,
            source_key,
            source1_file,
            source_file,
            correlation_ref_track,
            correlation_source_track,
            tgt_lang.as_deref(),
            use_source_separated_settings,
        )?;

        // --- Detect stepping BEFORE calculating mode delay ---
        let diagnosis = diagnose_audio_issue(
            source1_file,
            &results,
            &ctx.settings,
            runner,
            &ctx.tool_paths,
            &target_codec_id,
        );

        let mut stepping_override_delay: Option<i32> = None;
        let mut stepping_override_delay_raw: Option<f64> = None;
        let stepping_enabled = ctx.settings.stepping_enabled;

        if matches!(diagnosis, DiagnosisResult::Stepping { .. }) {
            let (ov_delay, ov_raw) = self.handle_stepping(
                ctx,
                source_key,
                &results,
                stepping_enabled,
                use_source_separated_settings,
                &effective_delay_mode,
                stepping_sources,
            );
            stepping_override_delay = ov_delay;
            stepping_override_delay_raw = ov_raw;
        }

        // --- Calculate delay ---
        let correlation_delay_ms;
        let correlation_delay_raw;

        if let (Some(ov_ms), Some(ov_raw)) =
            (stepping_override_delay, stepping_override_delay_raw)
        {
            correlation_delay_ms = ov_ms;
            correlation_delay_raw = ov_raw;
            (ctx.log)(&format!(
                "{} delay determined: {:+} ms (first segment, stepping corrected).",
                capitalize_first(source_key),
                correlation_delay_ms
            ));
        } else {
            let delay_calc = calculate_delay(
                &results,
                &ctx.settings,
                &effective_delay_mode,
                &*ctx.log,
                source_key,
            );

            match delay_calc {
                None => {
                    let accepted_count = results.iter().filter(|r| r.accepted).count();
                    let total_windows = results.len();
                    let min_required = 10.max(
                        (total_windows as f64 * ctx.settings.min_accepted_pct / 100.0) as usize,
                    );

                    return Err(format!(
                        "Analysis failed for {source_key}: Could not determine \
                         a reliable delay.\n\
                         \x20 - Accepted windows: {accepted_count}\n\
                         \x20 - Minimum required: {min_required} ({:.0}% of {total_windows})\n\
                         \x20 - Total windows scanned: {total_windows}\n\
                         \x20 - Match threshold: {}%\n\
                         \n\
                         Possible causes:\n\
                         \x20 - Audio quality is too poor for reliable correlation\n\
                         \x20 - Audio tracks are not from the same source material\n\
                         \x20 - Excessive noise or compression artifacts\n\
                         \x20 - Wrong language tracks selected for analysis\n\
                         \n\
                         Solutions:\n\
                         \x20 - Try lowering the \"Minimum Match %\" threshold\n\
                         \x20 - Try a smaller window size or hop size\n\
                         \x20 - Try selecting different audio tracks\n\
                         \x20 - Use VideoDiff mode instead of Audio Correlation\n\
                         \x20 - Check that both files are from the same video source",
                        ctx.settings.min_accepted_pct, ctx.settings.min_match_pct
                    ));
                }
                Some(calc) => {
                    correlation_delay_ms = calc.rounded_ms;
                    correlation_delay_raw = calc.raw_ms;
                }
            }
        }

        // --- Sync Stability Analysis ---
        let stepping_clusters = if let DiagnosisResult::Stepping {
            ref cluster_details,
            ..
        } = diagnosis
        {
            if cluster_details.is_empty() {
                None
            } else {
                Some(cluster_details.as_slice())
            }
        } else {
            None
        };

        let stability_result = analyze_sync_stability(
            &results,
            source_key,
            &ctx.settings,
            Some(&*ctx.log),
            stepping_clusters,
        );

        if let Some(stability) = stability_result {
            ctx.sync_stability_issues.push(stability);
        }

        // --- Calculate final delay chain ---
        let mut actual_container_delay = source1_audio_container_delay;

        if let (Some(info), Some(si)) = (source1_container_info, source1_stream_info) {
            let ref_lang_str = &ctx.settings.analysis_lang_source1;
            let ref_lang_opt = if ref_lang_str.is_empty() {
                None
            } else {
                Some(ref_lang_str.as_str())
            };
            actual_container_delay = find_actual_correlation_track_delay(
                info,
                Some(si),
                correlation_ref_track,
                ref_lang_opt,
                source1_audio_container_delay,
                &*ctx.log,
            );
        }

        let (final_delay_ms, final_delay_raw) = calculate_delay_chain(
            correlation_delay_ms,
            correlation_delay_raw,
            actual_container_delay,
            &*ctx.log,
            source_key,
        );

        source_delays.insert(source_key.to_string(), final_delay_ms);
        raw_source_delays.insert(source_key.to_string(), final_delay_raw);

        // --- Handle drift detection flags ---
        self.record_drift_flags(
            ctx,
            source_key,
            target_track_id,
            &diagnosis,
            final_delay_ms,
            use_source_separated_settings,
        );

        // --- Save dense data for stepping correction ---
        if let DiagnosisResult::Stepping {
            ref cluster_details,
            ..
        } = diagnosis
        {
            let analysis_track_key = format!("{source_key}_{target_track_id}");
            if ctx.segment_flags.contains_key(&analysis_track_key) {
                match save_stepping_data(
                    &ctx.temp_dir,
                    source_key,
                    target_track_id,
                    &results,
                    cluster_details,
                ) {
                    Ok(data_path) => {
                        if let Some(flags) = ctx.segment_flags.get_mut(&analysis_track_key) {
                            flags.stepping_data_path =
                                Some(data_path.to_string_lossy().to_string());
                        }
                        let name = data_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        (ctx.log)(&format!(
                            "[Stepping] Dense analysis data saved to {name}"
                        ));
                    }
                    Err(e) => {
                        (ctx.log)(&format!(
                            "[Stepping] WARNING: Failed to save stepping data: {e}"
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_and_correlate(
        &self,
        ctx: &Context,
        runner: &CommandRunner,
        source_key: &str,
        source1_file: &str,
        source_file: &str,
        correlation_ref_track: Option<i32>,
        correlation_source_track: Option<i32>,
        tgt_lang: Option<&str>,
        use_source_separated_settings: bool,
    ) -> Result<Vec<ChunkResult>, String> {
        let log = &*ctx.log;
        let settings = &ctx.settings;

        // --- 1. Select streams ---
        let idx_ref;
        if let Some(explicit_ref) = correlation_ref_track {
            idx_ref = explicit_ref;
            log(&format!(
                "Using explicit reference track index: {explicit_ref}"
            ));
        } else {
            let ref_lang_str = &settings.analysis_lang_source1;
            let ref_norm = normalize_lang(if ref_lang_str.is_empty() {
                None
            } else {
                Some(ref_lang_str.as_str())
            });
            let (idx, _) = get_audio_stream_info(
                source1_file,
                ref_norm.as_deref(),
                runner,
                &ctx.tool_paths,
            );
            idx_ref = idx.ok_or_else(|| {
                "Could not locate required audio streams for correlation.".to_string()
            })?;
        }

        let idx_tgt;
        let id_tgt;
        if let Some(explicit_tgt) = correlation_source_track {
            idx_tgt = explicit_tgt;
            id_tgt = None;
            log(&format!(
                "Using explicit target track index: {explicit_tgt}"
            ));
        } else {
            let tgt_norm = normalize_lang(tgt_lang);
            let (idx, tid) = get_audio_stream_info(
                source_file,
                tgt_norm.as_deref(),
                runner,
                &ctx.tool_paths,
            );
            idx_tgt = idx.ok_or_else(|| {
                "Could not locate required audio streams for correlation.".to_string()
            })?;
            id_tgt = tid;
        }

        // Log stream selection
        let ref_desc = if let Some(rt) = correlation_ref_track {
            format!("explicit track {rt}")
        } else {
            let ref_lang_str = &settings.analysis_lang_source1;
            let ref_norm = normalize_lang(if ref_lang_str.is_empty() {
                None
            } else {
                Some(ref_lang_str.as_str())
            });
            format!("lang='{}'", ref_norm.as_deref().unwrap_or("first"))
        };

        let tgt_desc = if let Some(tt) = correlation_source_track {
            format!("explicit track {tt}")
        } else {
            let tgt_norm = normalize_lang(tgt_lang);
            format!("lang='{}'", tgt_norm.as_deref().unwrap_or("first"))
        };

        let id_suffix = if let Some(tid) = id_tgt {
            format!(", track_id={tid}")
        } else {
            String::new()
        };

        log(&format!(
            "Selected streams: REF ({ref_desc}, index={idx_ref}), \
             {} ({tgt_desc}, index={idx_tgt}{id_suffix})",
            source_key.to_uppercase()
        ));

        // --- 2. Decode ---
        let use_soxr = settings.use_soxr;
        let ref_name = Path::new(source1_file)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let tgt_name = Path::new(source_file)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        log(&format!(
            "[DECODE DEBUG] Decoding ref: -map 0:a:{idx_ref} from {ref_name}"
        ));
        let mut ref_pcm =
            decode_audio(source1_file, idx_ref, DEFAULT_SR, use_soxr, runner, &ctx.tool_paths)?;

        log(&format!(
            "[DECODE DEBUG] Decoding tgt: -map 0:a:{idx_tgt} from {tgt_name}"
        ));
        let mut tgt_pcm =
            decode_audio(source_file, idx_tgt, DEFAULT_SR, use_soxr, runner, &ctx.tool_paths)?;

        // Log audio stats
        let ref_min = ref_pcm.iter().cloned().fold(f32::INFINITY, f32::min);
        let ref_max = ref_pcm.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let ref_std = pcm_std(&ref_pcm);
        log(&format!(
            "[DECODE DEBUG] ref_pcm: len={}, min={ref_min:.6}, max={ref_max:.6}, std={ref_std:.6}",
            ref_pcm.len()
        ));

        let tgt_min = tgt_pcm.iter().cloned().fold(f32::INFINITY, f32::min);
        let tgt_max = tgt_pcm.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let tgt_std = pcm_std(&tgt_pcm);
        log(&format!(
            "[DECODE DEBUG] tgt_pcm: len={}, min={tgt_min:.6}, max={tgt_max:.6}, std={tgt_std:.6}",
            tgt_pcm.len()
        ));

        // --- 2b. Source Separation (Optional) ---
        if use_source_separated_settings {
            let (r, t) = apply_source_separation_if_needed(
                ref_pcm, tgt_pcm, DEFAULT_SR, settings, log, source_key,
            );
            ref_pcm = r;
            tgt_pcm = t;
        }

        // --- 3. Filtering ---
        let (ref_filtered, tgt_filtered) =
            apply_filtering(ref_pcm, tgt_pcm, DEFAULT_SR, settings, log);
        ref_pcm = ref_filtered;
        tgt_pcm = tgt_filtered;

        // --- 4 & 5. Correlate (dense sliding window) ---
        let min_match = settings.min_match_pct;

        let multi_corr_enabled = settings.multi_correlation_enabled && !ctx.and_merge;

        let results = if multi_corr_enabled {
            self.run_dense_multi_correlation(
                &ref_pcm,
                &tgt_pcm,
                DEFAULT_SR,
                settings,
                use_source_separated_settings,
                min_match,
                log,
            )
        } else {
            let method = resolve_method(settings, use_source_separated_settings);
            run_dense_correlation(
                &ref_pcm,
                &tgt_pcm,
                DEFAULT_SR,
                &*method,
                settings.dense_window_s,
                settings.dense_hop_s,
                min_match,
                settings.dense_silence_threshold_db,
                settings.dense_outlier_threshold_ms,
                settings.scan_start_percentage,
                settings.scan_end_percentage,
                Some(log),
                settings.detection_dbscan_epsilon_ms,
                settings.detection_dbscan_min_samples_pct,
            )
        };

        // Release GPU resources
        drop(ref_pcm);
        drop(tgt_pcm);
        cleanup_gpu();

        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_dense_multi_correlation(
        &self,
        ref_pcm: &[f32],
        tgt_pcm: &[f32],
        sr: i64,
        settings: &AppSettings,
        use_source_separated: bool,
        min_match: f64,
        log: &dyn Fn(&str),
    ) -> Vec<ChunkResult> {
        // Find enabled methods from the registry
        let method_names = list_methods();

        if method_names.is_empty() {
            log("[MULTI-CORRELATION] No methods registered, falling back to single method");
            let method = resolve_method(settings, use_source_separated);
            return run_dense_correlation(
                ref_pcm,
                tgt_pcm,
                sr,
                &*method,
                settings.dense_window_s,
                settings.dense_hop_s,
                min_match,
                settings.dense_silence_threshold_db,
                settings.dense_outlier_threshold_ms,
                settings.scan_start_percentage,
                settings.scan_end_percentage,
                Some(log),
                settings.detection_dbscan_epsilon_ms,
                settings.detection_dbscan_min_samples_pct,
            );
        }

        log(&format!(
            "\n[MULTI-CORRELATION] Running {} methods (dense sliding window)",
            method_names.len()
        ));

        let mut all_results: Vec<(String, Vec<ChunkResult>)> = Vec::new();

        for method_name in &method_names {
            log(&format!("\n{}", "=".repeat(70)));
            log(&format!("  MULTI-CORRELATION: {method_name}"));
            log(&"=".repeat(70));

            let method = resolve_method(settings, use_source_separated);

            let results = run_dense_correlation(
                ref_pcm,
                tgt_pcm,
                sr,
                &*method,
                settings.dense_window_s,
                settings.dense_hop_s,
                min_match,
                settings.dense_silence_threshold_db,
                settings.dense_outlier_threshold_ms,
                settings.scan_start_percentage,
                settings.scan_end_percentage,
                Some(log),
                settings.detection_dbscan_epsilon_ms,
                settings.detection_dbscan_min_samples_pct,
            );
            all_results.push((method_name.clone(), results));

            // Free GPU memory between methods
            cleanup_gpu();
        }

        // Log comparison summary
        log(&format!("\n{}", "=".repeat(70)));
        log("  MULTI-CORRELATION SUMMARY (Dense)");
        log(&"=".repeat(70));

        for (method_name, method_results) in &all_results {
            let accepted: Vec<&ChunkResult> =
                method_results.iter().filter(|r| r.accepted).collect();
            if !accepted.is_empty() {
                let mut delays: Vec<f64> = accepted.iter().map(|r| r.raw_delay_ms).collect();
                delays.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let median_d = if delays.len() % 2 == 0 {
                    (delays[delays.len() / 2 - 1] + delays[delays.len() / 2]) / 2.0
                } else {
                    delays[delays.len() / 2]
                };
                let mean_d = delays.iter().sum::<f64>() / delays.len() as f64;
                let std_d = (delays
                    .iter()
                    .map(|&d| (d - mean_d).powi(2))
                    .sum::<f64>()
                    / delays.len() as f64)
                    .sqrt();
                let avg_match =
                    accepted.iter().map(|r| r.match_pct).sum::<f64>() / accepted.len() as f64;
                let outliers = delays
                    .iter()
                    .filter(|&&d| (d - median_d).abs() > 50.0)
                    .count();
                log(&format!(
                    "  {method_name}: {median_d:+.3}ms median | \
                     std={std_d:.3}ms | conf={avg_match:.1}% | \
                     accepted={}/{} | outliers={outliers}",
                    accepted.len(),
                    method_results.len()
                ));
            } else {
                log(&format!("  {method_name}: NO ACCEPTED WINDOWS"));
            }
        }

        log(&"=".repeat(70));
        log("");

        // Use first method's results for actual processing
        if let Some((first_name, _)) = all_results.first() {
            log(&format!(
                "[MULTI-CORRELATION] Using '{first_name}' results for delay calculation"
            ));
        }
        all_results
            .into_iter()
            .next()
            .map(|(_, results)| results)
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_stepping(
        &self,
        ctx: &mut Context,
        source_key: &str,
        results: &[ChunkResult],
        stepping_enabled: bool,
        use_source_separated_settings: bool,
        effective_delay_mode: &str,
        stepping_sources: &mut Vec<String>,
    ) -> (Option<i32>, Option<f64>) {
        if stepping_enabled && !use_source_separated_settings {
            stepping_sources.push(source_key.to_string());

            let has_audio_from_source = ctx.manual_layout.iter().any(|t| {
                t.track_type.as_deref() == Some("audio")
                    && t.source.as_deref() == Some(source_key)
            });

            if has_audio_from_source {
                let first_segment_delay = find_first_stable_segment_delay(
                    results, &ctx.settings, false, &*ctx.log, None,
                );
                let first_segment_delay_raw = find_first_stable_segment_delay(
                    results, &ctx.settings, true, &*ctx.log, None,
                );
                if let (Some(delay), Some(delay_raw)) =
                    (first_segment_delay, first_segment_delay_raw)
                {
                    let delay_i = delay as i32;
                    (ctx.log)(&format!(
                        "[Stepping Detected] Found stepping in {source_key}"
                    ));
                    (ctx.log)(&format!(
                        "[Stepping Override] Using first segment's delay: \
                         {delay_i:+}ms (raw: {delay_raw:.3}ms)"
                    ));
                    (ctx.log)(&format!(
                        "[Stepping Override] This delay will be used for \
                         ALL tracks (audio + subtitles) from {source_key}"
                    ));
                    (ctx.log)(
                        "[Stepping Override] Stepping correction will be \
                         applied to audio tracks during processing",
                    );
                    return (Some(delay_i), Some(delay_raw));
                }
            } else {
                (ctx.log)(&format!(
                    "[Stepping Detected] Found stepping in {source_key}"
                ));
                (ctx.log)("[Stepping] No audio tracks from this source are being merged");
                (ctx.log)(&format!(
                    "[Stepping] Using delay_selection_mode='{effective_delay_mode}' \
                     instead of first segment (stepping correction won't run)"
                ));
            }
        } else if use_source_separated_settings {
            ctx.stepping_detected_separated
                .push(source_key.to_string());
            (ctx.log)(&format!(
                "[Stepping Detected] Found stepping in {source_key}"
            ));
            (ctx.log)(
                "[Stepping Disabled] Source separation is enabled - \
                 stepping correction is unreliable on separated stems",
            );
            (ctx.log)(
                "[Stepping Disabled] Separated stems have different \
                 waveform characteristics that break stepping detection",
            );
            (ctx.log)(&format!(
                "[Stepping Disabled] Using delay_selection_mode=\
                 '{effective_delay_mode}' instead"
            ));
        } else {
            ctx.stepping_detected_disabled.push(source_key.to_string());
            (ctx.log)(&format!(
                "[Stepping Detected] Found stepping in {source_key}"
            ));
            (ctx.log)(
                "[Stepping Disabled] Stepping correction is disabled \
                 - timing may be inconsistent",
            );
            (ctx.log)(
                "[Recommendation] Enable 'Stepping Correction' in \
                 settings if you want automatic correction",
            );
            (ctx.log)("[Manual Review] You should manually review this file's sync quality");
        }

        (None, None)
    }

    fn record_drift_flags(
        &self,
        ctx: &mut Context,
        source_key: &str,
        target_track_id: i32,
        diagnosis: &DiagnosisResult,
        final_delay_ms: i32,
        use_source_separated_settings: bool,
    ) {
        if matches!(diagnosis, DiagnosisResult::Uniform) {
            return;
        }

        let analysis_track_key = format!("{source_key}_{target_track_id}");

        match diagnosis {
            DiagnosisResult::Drift {
                ref diagnosis,
                rate,
            } if diagnosis == "PAL_DRIFT" => {
                if use_source_separated_settings {
                    (ctx.log)(&format!(
                        "[PAL Drift Detected] PAL drift detected in \
                         {source_key}, but source separation is enabled. \
                         PAL correction is unreliable on separated stems - skipping."
                    ));
                } else {
                    let source_has_audio = ctx.manual_layout.iter().any(|item| {
                        item.source.as_deref() == Some(source_key)
                            && item.track_type.as_deref() == Some("audio")
                    });
                    if source_has_audio {
                        ctx.pal_drift_flags.insert(
                            analysis_track_key,
                            DriftFlagsEntry { rate: Some(*rate) },
                        );
                    } else {
                        (ctx.log)(&format!(
                            "[PAL Drift Detected] PAL drift detected in \
                             {source_key}, but no audio tracks from this \
                             source are being used. Skipping PAL correction \
                             for {source_key}."
                        ));
                    }
                }
            }
            DiagnosisResult::Drift {
                ref diagnosis,
                rate,
            } if diagnosis == "LINEAR_DRIFT" => {
                if use_source_separated_settings {
                    (ctx.log)(&format!(
                        "[Linear Drift Detected] Linear drift detected in \
                         {source_key}, but source separation is enabled. \
                         Linear drift correction is unreliable on separated \
                         stems - skipping."
                    ));
                } else {
                    let source_has_audio = ctx.manual_layout.iter().any(|item| {
                        item.source.as_deref() == Some(source_key)
                            && item.track_type.as_deref() == Some("audio")
                    });
                    if source_has_audio {
                        ctx.linear_drift_flags.insert(
                            analysis_track_key,
                            DriftFlagsEntry { rate: Some(*rate) },
                        );
                    } else {
                        (ctx.log)(&format!(
                            "[Linear Drift Detected] Linear drift detected in \
                             {source_key}, but no audio tracks from this \
                             source are being used. Skipping linear drift \
                             correction for {source_key}."
                        ));
                    }
                }
            }
            DiagnosisResult::Stepping {
                ref cluster_details,
                ref valid_clusters,
                ref invalid_clusters,
                ref validation_results,
                ref correction_mode,
                ref fallback_mode,
                ..
            } => {
                if use_source_separated_settings {
                    // Already handled in handle_stepping
                } else {
                    let source_has_audio = ctx.manual_layout.iter().any(|item| {
                        item.source.as_deref() == Some(source_key)
                            && item.track_type.as_deref() == Some("audio")
                    });
                    let source_has_subs = ctx.manual_layout.iter().any(|item| {
                        item.source.as_deref() == Some(source_key)
                            && item.track_type.as_deref() == Some("subtitles")
                    });

                    if source_has_audio {
                        let cluster_details_json: Vec<serde_json::Value> = cluster_details
                            .iter()
                            .map(|cd| serde_json::to_value(cd).unwrap_or_default())
                            .collect();

                        let validation_json: HashMap<i32, serde_json::Value> = validation_results
                            .iter()
                            .map(|(k, v)| (*k, serde_json::to_value(v).unwrap_or_default()))
                            .collect();

                        ctx.segment_flags.insert(
                            analysis_track_key,
                            SegmentFlagsEntry {
                                base_delay: final_delay_ms,
                                cluster_details: cluster_details_json,
                                valid_clusters: valid_clusters.clone(),
                                invalid_clusters: invalid_clusters.clone(),
                                validation_results: validation_json,
                                correction_mode: Some(correction_mode.clone()),
                                fallback_mode: Some(
                                    fallback_mode
                                        .clone()
                                        .unwrap_or_else(|| "nearest".to_string()),
                                ),
                                subs_only: Some(false),
                                stepping_data_path: None,
                                audit_metadata: None,
                            },
                        );
                        (ctx.log)(&format!(
                            "[Stepping] Stepping correction will be applied \
                             to audio tracks from {source_key}."
                        ));
                    } else if source_has_subs
                        && ctx.settings.stepping_adjust_subtitles_no_audio
                    {
                        (ctx.log)(&format!(
                            "[Stepping Detected] Stepping detected in \
                             {source_key}. No audio tracks from this source, \
                             but subtitles will use verified stepping EDL."
                        ));

                        let cluster_details_json: Vec<serde_json::Value> = cluster_details
                            .iter()
                            .map(|cd| serde_json::to_value(cd).unwrap_or_default())
                            .collect();

                        let validation_json: HashMap<i32, serde_json::Value> = validation_results
                            .iter()
                            .map(|(k, v)| (*k, serde_json::to_value(v).unwrap_or_default()))
                            .collect();

                        ctx.segment_flags.insert(
                            analysis_track_key,
                            SegmentFlagsEntry {
                                base_delay: final_delay_ms,
                                cluster_details: cluster_details_json,
                                valid_clusters: valid_clusters.clone(),
                                invalid_clusters: invalid_clusters.clone(),
                                validation_results: validation_json,
                                correction_mode: Some(correction_mode.clone()),
                                fallback_mode: Some(
                                    fallback_mode
                                        .clone()
                                        .unwrap_or_else(|| "nearest".to_string()),
                                ),
                                subs_only: Some(true),
                                stepping_data_path: None,
                                audit_metadata: None,
                            },
                        );
                        (ctx.log)(
                            "[Stepping] Full stepping analysis will run \
                             for verified subtitle EDL.",
                        );
                    } else {
                        (ctx.log)(&format!(
                            "[Stepping Detected] Stepping detected in \
                             {source_key}, but no audio or subtitle tracks \
                             from this source are being used. Skipping \
                             stepping correction."
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

// ─── Utility ────────────────────────────────────────────────────────────────

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

fn pcm_std(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean = samples.iter().map(|&s| s as f64).sum::<f64>() / samples.len() as f64;
    let variance =
        samples.iter().map(|&s| (s as f64 - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    variance.sqrt()
}
