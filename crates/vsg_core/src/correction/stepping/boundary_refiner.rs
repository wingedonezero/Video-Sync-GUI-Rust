//! Boundary refiner — 1:1 port of `vsg_core/correction/stepping/boundary_refiner.py`.
//!
//! Find precise splice points at transition boundaries.
//!
//! For each transition zone the EDL builder identified, this module:
//!   1. Converts to Source 2 timeline (correct subtract convention)
//!   2. Searches Source 2 PCM for silence (RMS + VAD combined)
//!   3. Detects transients (drum hits, impacts) to avoid them
//!   4. Snaps to the CENTER of the best silence zone
//!   5. Nudges to nearest zero-crossing to prevent clicks
//!   6. Optionally aligns to a video keyframe

use std::collections::HashMap;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

use super::timeline;
use super::types::{BoundaryResult, SilenceZone, SplicePoint, TransitionZone};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// For each transition zone, find the best splice point in Source 2 audio — `refine_boundaries`
///
/// Parameters:
/// - `src2_pcm`: Mono i32 PCM of Source 2 (the target / analysis track).
/// - `src2_sr`: Sample rate of src2_pcm.
#[allow(clippy::too_many_arguments)]
pub fn refine_boundaries(
    transition_zones: &[TransitionZone],
    src2_pcm: &[i32],
    src2_sr: i32,
    settings: &AppSettings,
    log: &dyn Fn(&str),
    ref_video_path: Option<&str>,
    tool_paths: Option<&HashMap<String, String>>,
    runner: Option<&CommandRunner>,
) -> Vec<SplicePoint> {
    let mut splice_points: Vec<SplicePoint> = Vec::new();

    for (i, zone) in transition_zones.iter().enumerate() {
        log(&format!(
            "  [Boundary {}] Refining transition (correction {:+.0}ms)...",
            i + 1,
            zone.correction_ms,
        ));

        // Midpoint of the transition zone in ref timeline
        let ref_mid = (zone.ref_start_s + zone.ref_end_s) / 2.0;
        // Convert to Source 2 timeline using delay BEFORE the transition
        let src2_mid = timeline::ref_to_src2(ref_mid, zone.delay_before_ms);

        let search_window_s = settings.stepping_silence_search_window_s;
        let search_start = (src2_mid - search_window_s).max(0.0);
        let search_end = src2_mid + search_window_s;

        log(&format!(
            "    Ref midpoint: {ref_mid:.2}s -> Src2 search: [{search_start:.2}s - {search_end:.2}s]"
        ));

        // --- Find silence zones ---
        let boundary = find_best_silence(
            src2_pcm,
            src2_sr,
            search_start,
            search_end,
            src2_mid,
            settings,
            log,
        );
        let best_zone = boundary.zone.clone();

        // Determine splice time
        let mut splice_src2;
        if let Some(ref bz) = best_zone {
            splice_src2 = bz.center_s;
            log(&format!(
                "    Splice: {:.3}s  (silence {:.0}ms @ {:.1}dB, source={}, score={:.1})",
                splice_src2, bz.duration_ms, bz.avg_db, bz.source, boundary.score,
            ));
        } else {
            splice_src2 = src2_mid;
            log(&format!(
                "    No silence found -- using raw midpoint {splice_src2:.3}s"
            ));
        }

        // --- Zero-crossing snap (prevents waveform discontinuity clicks) ---
        let pre_zc = splice_src2;
        splice_src2 = snap_to_zero_crossing(src2_pcm, src2_sr, splice_src2, 2.0);
        if (splice_src2 - pre_zc).abs() > 1e-10 {
            log(&format!(
                "    Zero-crossing snap: {pre_zc:.4}s -> {splice_src2:.4}s \
                 (shift {:.2}ms)",
                (splice_src2 - pre_zc) * 1000.0,
            ));
        }

        // --- Optional video snap ---
        let mut snap_meta: HashMap<String, serde_json::Value> = HashMap::new();
        if settings.stepping_snap_to_video_frames {
            if let (Some(video_path), Some(tp), Some(r)) =
                (ref_video_path, tool_paths, runner)
            {
                let splice_ref =
                    timeline::src2_to_ref(splice_src2, zone.delay_before_ms);
                if let Some(snapped_ref) =
                    snap_to_video_frame(splice_ref, video_path, settings, tp, r, log)
                {
                    if (snapped_ref - splice_ref).abs() > 1e-10 {
                        // Ensure the snapped position is still within (or near) the
                        // silence zone so we don't create a click.
                        let snapped_src2 =
                            timeline::ref_to_src2(snapped_ref, zone.delay_before_ms);
                        if let Some(ref bz) = best_zone {
                            if snapped_src2 >= bz.start_s && snapped_src2 <= bz.end_s {
                                splice_src2 = snapped_src2;
                                snap_meta.insert(
                                    "video_snapped".to_string(),
                                    serde_json::Value::Bool(true),
                                );
                                snap_meta.insert(
                                    "video_snap_offset_s".to_string(),
                                    serde_json::json!(snapped_ref - splice_ref),
                                );
                                log(&format!(
                                    "    Video snap: {splice_ref:.3}s -> {snapped_ref:.3}s \
                                     (within silence zone)"
                                ));
                            } else {
                                log(&format!(
                                    "    Video snap rejected: {snapped_src2:.3}s \
                                     falls outside silence zone"
                                ));
                            }
                        }
                    }
                }
            }
        }

        let splice_ref_final =
            timeline::src2_to_ref(splice_src2, zone.delay_before_ms);
        splice_points.push(SplicePoint {
            ref_time_s: splice_ref_final,
            src2_time_s: splice_src2,
            delay_before_ms: zone.delay_before_ms,
            delay_after_ms: zone.delay_after_ms,
            correction_ms: zone.correction_ms,
            silence_zone: best_zone,
            boundary_result: Some(boundary),
            snap_metadata: snap_meta,
        });
    }

    splice_points
}

// ---------------------------------------------------------------------------
// Combined silence finder
// ---------------------------------------------------------------------------

/// Find the best splice-worthy silence zone near `target_s` — `_find_best_silence`
///
/// Combines RMS energy detection, WebRTC VAD, and transient avoidance.
/// The intersection of "RMS quiet" + "VAD non-speech" gives the safest
/// splice candidates. Falls back to RMS-only or VAD-only if the
/// intersection is empty.
fn find_best_silence(
    pcm: &[i32],
    sr: i32,
    start_s: f64,
    end_s: f64,
    target_s: f64,
    settings: &AppSettings,
    log: &dyn Fn(&str),
) -> BoundaryResult {
    let threshold_db = settings.stepping_silence_threshold_db;
    let min_duration_ms = settings.stepping_silence_min_duration_ms;

    // RMS silence detection
    let rms_zones = find_silence_zones_rms(pcm, sr, start_s, end_s, threshold_db, min_duration_ms);
    log(&format!("    RMS: {} silence zone(s)", rms_zones.len()));

    // VAD non-speech gap detection
    let mut vad_gaps: Vec<SilenceZone> = Vec::new();
    if settings.stepping_vad_enabled {
        vad_gaps = find_vad_gaps(
            pcm,
            sr,
            start_s,
            end_s,
            settings.stepping_vad_aggressiveness,
            min_duration_ms,
        );
        log(&format!("    VAD: {} non-speech gap(s)", vad_gaps.len()));
    }

    // Transient detection
    let mut transient_times: Vec<f64> = Vec::new();
    if settings.stepping_transient_detection_enabled {
        transient_times = detect_transients(
            pcm,
            sr,
            start_s,
            end_s,
            settings.stepping_transient_threshold,
            10.0,
        );
        if !transient_times.is_empty() {
            log(&format!(
                "    Transients: {} detected",
                transient_times.len()
            ));
        }
    }

    // Try intersection first
    let combined = if !vad_gaps.is_empty() {
        intersect_zones(&rms_zones, &vad_gaps)
    } else {
        vec![]
    };
    log(&format!(
        "    Combined: {} overlapping zone(s)",
        combined.len()
    ));

    // Pick best from combined -> rms -> vad (preference order)
    let candidates = if !combined.is_empty() {
        &combined
    } else if !rms_zones.is_empty() {
        &rms_zones
    } else if !vad_gaps.is_empty() {
        &vad_gaps
    } else {
        return BoundaryResult {
            zone: None,
            score: 0.0,
            near_transient: false,
            overlaps_speech: false,
        };
    };

    let (best_zone, score, near_transient) =
        pick_best_zone(candidates, target_s, settings, log, &transient_times);

    // If VAD was enabled and we fell back to RMS-only (no combined zones),
    // the winning zone likely overlaps speech detected by VAD.
    let overlaps_speech = settings.stepping_vad_enabled
        && !vad_gaps.is_empty()
        && combined.is_empty()
        && best_zone.source == "rms";

    BoundaryResult {
        zone: Some(best_zone),
        score,
        near_transient,
        overlaps_speech,
    }
}

// ---------------------------------------------------------------------------
// RMS silence detection
// ---------------------------------------------------------------------------

/// Find contiguous quiet regions using RMS energy in 50 ms windows — `find_silence_zones_rms`
pub fn find_silence_zones_rms(
    pcm: &[i32],
    sample_rate: i32,
    start_s: f64,
    end_s: f64,
    threshold_db: f64,
    min_duration_ms: f64,
) -> Vec<SilenceZone> {
    let start_sample = (start_s * sample_rate as f64).max(0.0) as usize;
    let end_sample = (end_s * sample_rate as f64).min(pcm.len() as f64) as usize;
    if end_sample <= start_sample {
        return vec![];
    }

    let window_size = ((0.05 * sample_rate as f64) as usize).max(1);
    let min_silence_samples = ((min_duration_ms / 1000.0) * sample_rate as f64) as usize;

    let mut zones: Vec<SilenceZone> = Vec::new();
    let mut run_start: Option<f64> = None;
    let mut run_dbs: Vec<f64> = Vec::new();

    let mut pos = start_sample;
    while pos + window_size <= end_sample {
        let window = &pcm[pos..pos + window_size];
        if window.is_empty() {
            pos += window_size;
            continue;
        }

        let rms = compute_rms_i32(window);
        let db = if rms > 1e-10 {
            20.0 * (rms / 2_147_483_648.0).log10()
        } else {
            -96.0
        };

        if db < threshold_db {
            if run_start.is_none() {
                run_start = Some(pos as f64 / sample_rate as f64);
                run_dbs = vec![db];
            } else {
                run_dbs.push(db);
            }
        } else if let Some(rs) = run_start.take() {
            maybe_emit_zone(
                &mut zones,
                rs,
                pos as f64 / sample_rate as f64,
                &run_dbs,
                min_silence_samples,
                sample_rate,
                "rms",
            );
            run_dbs.clear();
        }

        pos += window_size;
    }

    if let Some(rs) = run_start.take() {
        maybe_emit_zone(
            &mut zones,
            rs,
            end_sample as f64 / sample_rate as f64,
            &run_dbs,
            min_silence_samples,
            sample_rate,
            "rms",
        );
    }

    zones
}

// ---------------------------------------------------------------------------
// VAD non-speech gap detection
// ---------------------------------------------------------------------------

/// Find non-speech gaps using WebRTC VAD — `find_vad_gaps`
///
/// Returns the *gaps* (regions where VAD says no speech), each tagged
/// with the RMS energy measured from the original PCM.
pub fn find_vad_gaps(
    pcm: &[i32],
    sample_rate: i32,
    start_s: f64,
    end_s: f64,
    aggressiveness: i32,
    min_gap_ms: f64,
) -> Vec<SilenceZone> {
    let vad_sr: i32 = if sample_rate >= 16000 { 16000 } else { 8000 };
    let frame_ms: i32 = 30;
    let frame_samples = (vad_sr * frame_ms / 1000) as usize;

    let start_sample = (start_s * sample_rate as f64).max(0.0) as usize;
    let end_sample = (end_s * sample_rate as f64).min(pcm.len() as f64) as usize;
    if end_sample <= start_sample {
        return vec![];
    }

    let segment = &pcm[start_sample..end_sample];
    if segment.is_empty() {
        return vec![];
    }

    // Downsample if needed (simple decimation) and convert i32 -> i16
    let step = (sample_rate / vad_sr) as usize;
    let audio_int16: Vec<i16> = segment
        .iter()
        .step_by(step.max(1))
        .map(|&s| (s >> 16) as i16) // i32 -> i16 by right-shifting 16 bits
        .collect();

    let vad_sr_enum = match vad_sr {
        8000 => webrtc_vad::SampleRate::Rate8kHz,
        16000 => webrtc_vad::SampleRate::Rate16kHz,
        32000 => webrtc_vad::SampleRate::Rate32kHz,
        48000 => webrtc_vad::SampleRate::Rate48kHz,
        _ => webrtc_vad::SampleRate::Rate16kHz,
    };

    let vad_mode = match aggressiveness {
        0 => webrtc_vad::VadMode::Quality,
        1 => webrtc_vad::VadMode::LowBitrate,
        2 => webrtc_vad::VadMode::Aggressive,
        _ => webrtc_vad::VadMode::VeryAggressive,
    };

    let mut vad = webrtc_vad::Vad::new_with_rate_and_mode(vad_sr_enum, vad_mode);

    // Collect per-frame speech decisions
    let mut gap_start: Option<f64> = None;
    let mut gaps: Vec<SilenceZone> = Vec::new();

    let mut i = 0;
    while i + frame_samples <= audio_int16.len() {
        let frame = &audio_int16[i..i + frame_samples];
        let t = start_s + (i as f64 / vad_sr as f64);

        let is_speech = vad.is_voice_segment(frame).unwrap_or(true);

        if !is_speech {
            if gap_start.is_none() {
                gap_start = Some(t);
            }
        } else if let Some(gs) = gap_start.take() {
            let gap_end = t;
            let dur_ms = (gap_end - gs) * 1000.0;
            if dur_ms >= min_gap_ms {
                let avg_db = rms_db_range(pcm, sample_rate, gs, gap_end);
                gaps.push(SilenceZone {
                    start_s: gs,
                    end_s: gap_end,
                    center_s: (gs + gap_end) / 2.0,
                    avg_db,
                    duration_ms: dur_ms,
                    source: "vad".to_string(),
                });
            }
        }

        i += frame_samples;
    }

    // Close trailing gap
    if let Some(gs) = gap_start {
        let gap_end = end_s;
        let dur_ms = (gap_end - gs) * 1000.0;
        if dur_ms >= min_gap_ms {
            let avg_db = rms_db_range(pcm, sample_rate, gs, gap_end);
            gaps.push(SilenceZone {
                start_s: gs,
                end_s: gap_end,
                center_s: (gs + gap_end) / 2.0,
                avg_db,
                duration_ms: dur_ms,
                source: "vad".to_string(),
            });
        }
    }

    gaps
}

// ---------------------------------------------------------------------------
// Zone intersection + scoring
// ---------------------------------------------------------------------------

/// Return the temporal overlaps between RMS zones and VAD gaps — `_intersect_zones`
fn intersect_zones(
    rms_zones: &[SilenceZone],
    vad_gaps: &[SilenceZone],
) -> Vec<SilenceZone> {
    let mut combined: Vec<SilenceZone> = Vec::new();
    for rz in rms_zones {
        for vg in vad_gaps {
            let ol_start = rz.start_s.max(vg.start_s);
            let ol_end = rz.end_s.min(vg.end_s);
            if ol_end > ol_start {
                let dur_ms = (ol_end - ol_start) * 1000.0;
                combined.push(SilenceZone {
                    start_s: ol_start,
                    end_s: ol_end,
                    center_s: (ol_start + ol_end) / 2.0,
                    avg_db: rz.avg_db,
                    duration_ms: dur_ms,
                    source: "combined".to_string(),
                });
            }
        }
    }
    combined
}

/// Pick the best silence zone from candidates — `_pick_best_zone`
///
/// Scoring:
///   - Closeness to `target_s` (most important)
///   - Duration (longer is safer)
///   - Depth (quieter is better)
///   - Transient penalty (zones containing transients score lower)
fn pick_best_zone(
    candidates: &[SilenceZone],
    target_s: f64,
    settings: &AppSettings,
    _log: &dyn Fn(&str),
    transient_times: &[f64],
) -> (SilenceZone, f64, bool) {
    let threshold_db = settings.stepping_silence_threshold_db;
    let search_window = settings.stepping_silence_search_window_s;

    let weight_silence = settings.stepping_fusion_weight_silence as f64;
    let weight_duration = settings.stepping_fusion_weight_duration as f64;

    let mut best: Option<SilenceZone> = None;
    let mut best_score = f64::NEG_INFINITY;
    let mut best_near_transient = false;

    for zone in candidates {
        let distance = (zone.center_s - target_s).abs();
        let distance_score = ((search_window - distance) / search_window).max(0.0) * 5.0;
        let depth_score =
            ((threshold_db - zone.avg_db) / 10.0).max(0.0) * weight_silence;
        let dur_score = (zone.duration_ms / 1000.0).min(1.0) * weight_duration;

        // Transient penalty: count transients within or near (+/-50ms) the zone
        let mut transient_penalty = 0.0;
        let mut zone_has_transient = false;
        if !transient_times.is_empty() {
            let margin = 0.05; // 50ms safety margin
            let count = transient_times
                .iter()
                .filter(|&&t| {
                    t >= (zone.start_s - margin) && t <= (zone.end_s + margin)
                })
                .count();
            zone_has_transient = count > 0;
            // Each transient near the zone applies a heavy penalty
            transient_penalty = count as f64 * 3.0;
        }

        let score = distance_score + depth_score + dur_score - transient_penalty;

        if score > best_score {
            best_score = score;
            best = Some(zone.clone());
            best_near_transient = zone_has_transient;
        }
    }

    // candidates is non-empty, so best is always Some
    (best.unwrap(), best_score, best_near_transient)
}

// ---------------------------------------------------------------------------
// Transient detection
// ---------------------------------------------------------------------------

/// Detect transients (sudden amplitude jumps) in a PCM region — `detect_transients`
///
/// Scans with small RMS windows and looks for frame-to-frame dB jumps
/// that exceed `threshold_db`. Returns the timestamps (in seconds)
/// where transients are detected.
///
/// These are places we do NOT want to splice -- drum hits, impacts,
/// consonant onsets -- because cutting there produces audible clicks.
pub fn detect_transients(
    pcm: &[i32],
    sr: i32,
    start_s: f64,
    end_s: f64,
    threshold_db: f64,
    window_ms: f64,
) -> Vec<f64> {
    let start_sample = (start_s * sr as f64).max(0.0) as usize;
    let end_sample = (end_s * sr as f64).min(pcm.len() as f64) as usize;
    if end_sample <= start_sample {
        return vec![];
    }

    let window_size = ((window_ms / 1000.0) * sr as f64) as usize;
    let window_size = window_size.max(1);
    let mut transients: Vec<f64> = Vec::new();
    let mut prev_db: Option<f64> = None;

    let mut pos = start_sample;
    while pos + window_size <= end_sample {
        let window = &pcm[pos..pos + window_size];
        if window.is_empty() {
            pos += window_size;
            continue;
        }

        let rms = compute_rms_i32(window);
        let db = if rms > 1e-10 {
            20.0 * (rms / 2_147_483_648.0).log10()
        } else {
            -96.0
        };

        if let Some(prev) = prev_db {
            let jump = db - prev; // positive = sudden louder
            if jump >= threshold_db {
                let t = pos as f64 / sr as f64;
                transients.push(t);
            }
        }

        prev_db = Some(db);
        pos += window_size;
    }

    transients
}

// ---------------------------------------------------------------------------
// Zero-crossing snap
// ---------------------------------------------------------------------------

/// Nudge `target_s` to the nearest zero crossing in the PCM — `_snap_to_zero_crossing`
///
/// A zero crossing is where the waveform crosses through zero amplitude.
/// Splicing at a zero crossing avoids the discontinuity that causes an
/// audible click. Searches +/-search_radius_ms around the target.
///
/// Returns the snapped time (seconds), or the original if no crossing
/// is found within the radius.
pub fn snap_to_zero_crossing(
    pcm: &[i32],
    sr: i32,
    target_s: f64,
    search_radius_ms: f64,
) -> f64 {
    let target_sample = (target_s * sr as f64) as usize;
    let radius_samples = ((search_radius_ms / 1000.0) * sr as f64) as usize;
    let radius_samples = radius_samples.max(1);

    let lo = target_sample.saturating_sub(radius_samples);
    let hi = (target_sample + radius_samples).min(pcm.len().saturating_sub(1));
    if hi <= lo {
        return target_s;
    }

    let segment = &pcm[lo..=hi];

    // Find sign changes: where consecutive samples have different signs
    let mut nearest_crossing: Option<usize> = None;
    let mut min_dist = usize::MAX;

    for i in 0..segment.len() - 1 {
        let s0 = segment[i] as i64;
        let s1 = segment[i + 1] as i64;
        // Sign change: one positive, one negative (or zero)
        if (s0 > 0 && s1 <= 0) || (s0 < 0 && s1 >= 0) || (s0 == 0) {
            let abs_pos = lo + i;
            let dist = abs_pos.abs_diff(target_sample);
            if dist < min_dist {
                min_dist = dist;
                nearest_crossing = Some(abs_pos);
            }
        }
    }

    match nearest_crossing {
        Some(idx) => idx as f64 / sr as f64,
        None => target_s,
    }
}

// ---------------------------------------------------------------------------
// Video frame snapping
// ---------------------------------------------------------------------------

/// Try to snap `boundary_ref_s` to a nearby keyframe — `_snap_to_video_frame`
///
/// Returns the snapped position or `None` if nothing suitable.
fn snap_to_video_frame(
    boundary_ref_s: f64,
    video_file: &str,
    settings: &AppSettings,
    tool_paths: &HashMap<String, String>,
    runner: &CommandRunner,
    _log: &dyn Fn(&str),
) -> Option<f64> {
    let max_offset = settings.stepping_video_snap_max_offset_s;

    // Get keyframe positions via ffprobe
    let cmd: Vec<&str> = vec![
        "ffprobe",
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "packet=pts_time,flags",
        "-of", "json",
        video_file,
    ];

    let result = runner.run(&cmd, tool_paths)?;
    let data: serde_json::Value = serde_json::from_str(&result).ok()?;

    let packets = data.get("packets")?.as_array()?;
    let keyframes: Vec<f64> = packets
        .iter()
        .filter(|p| {
            p.get("flags")
                .and_then(|v| v.as_str())
                .map(|f| f.contains('K'))
                .unwrap_or(false)
        })
        .filter_map(|p| {
            p.get("pts_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        })
        .collect();

    if keyframes.is_empty() {
        return None;
    }

    let nearest = keyframes
        .iter()
        .copied()
        .min_by(|a, b| {
            let da = (a - boundary_ref_s).abs();
            let db = (b - boundary_ref_s).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })?;

    if (nearest - boundary_ref_s).abs() <= max_offset {
        Some(nearest)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute RMS dB for a time range in i32 PCM — `_rms_db_range`
fn rms_db_range(pcm: &[i32], sr: i32, start_s: f64, end_s: f64) -> f64 {
    let s = (start_s * sr as f64).max(0.0) as usize;
    let e = (end_s * sr as f64).min(pcm.len() as f64) as usize;
    if e <= s {
        return -96.0;
    }
    let segment = &pcm[s..e];
    let rms = compute_rms_i32(segment);
    if rms > 1e-10 {
        20.0 * (rms / 2_147_483_648.0).log10()
    } else {
        -96.0
    }
}

/// Compute RMS of i32 PCM samples.
fn compute_rms_i32(samples: &[i32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64;
            f * f
        })
        .sum();
    (sum_sq / samples.len() as f64).sqrt()
}

/// Append a SilenceZone if it meets the minimum duration — `_maybe_emit_zone`
fn maybe_emit_zone(
    zones: &mut Vec<SilenceZone>,
    run_start: f64,
    run_end: f64,
    run_dbs: &[f64],
    min_silence_samples: usize,
    sample_rate: i32,
    source: &str,
) {
    let dur_samples = ((run_end - run_start) * sample_rate as f64) as usize;
    if dur_samples >= min_silence_samples {
        let avg_db = if run_dbs.is_empty() {
            -96.0
        } else {
            run_dbs.iter().sum::<f64>() / run_dbs.len() as f64
        };
        zones.push(SilenceZone {
            start_s: run_start,
            end_s: run_end,
            center_s: (run_start + run_end) / 2.0,
            avg_db,
            duration_ms: (run_end - run_start) * 1000.0,
            source: source.to_string(),
        });
    }
}
