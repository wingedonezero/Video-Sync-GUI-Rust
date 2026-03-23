//! Container delay extraction — 1:1 port of `vsg_core/analysis/container_delays.py`.

use std::collections::HashMap;

use crate::extraction::tracks::get_stream_info_with_delays;
use crate::io::runner::CommandRunner;

use super::types::ContainerDelayInfo;

/// Extract container delay information from a media file — `get_container_delay_info`
pub fn get_container_delay_info(
    source_file: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    log: &dyn Fn(&str),
    source_label: &str,
) -> Option<ContainerDelayInfo> {
    let stream_info = get_stream_info_with_delays(source_file, runner, tool_paths)?;

    let empty_tracks = vec![];
    let tracks = stream_info
        .get("tracks")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_tracks);

    // Extract all container delays
    let mut container_delays: HashMap<i32, f64> = HashMap::new();
    for track in tracks {
        let tid = track.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let delay_ms = track
            .get("container_delay_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        container_delays.insert(tid, delay_ms);

        let track_type = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if delay_ms != 0.0 && matches!(track_type, "video" | "audio") {
            let label = if source_label.is_empty() {
                String::new()
            } else {
                format!("{source_label} ")
            };
            log(&format!(
                "[Container Delay] {label}{} track {tid} has container delay: {delay_ms:+.1}ms",
                capitalize(track_type)
            ));
        }
    }

    // Find video track delay
    let video_tracks: Vec<&serde_json::Value> = tracks
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("video"))
        .collect();
    let video_delay_ms = video_tracks
        .first()
        .and_then(|t| t.get("id").and_then(|v| v.as_i64()))
        .map(|tid| *container_delays.get(&(tid as i32)).unwrap_or(&0.0))
        .unwrap_or(0.0);

    // Convert audio delays to be relative to video
    let mut audio_delays_relative: HashMap<i32, f64> = HashMap::new();
    for track in tracks {
        if track.get("type").and_then(|v| v.as_str()) != Some("audio") {
            continue;
        }
        let tid = track.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let absolute_delay = container_delays.get(&tid).copied().unwrap_or(0.0);
        audio_delays_relative.insert(tid, absolute_delay - video_delay_ms);
    }

    Some(ContainerDelayInfo {
        video_delay_ms,
        audio_delays_ms: audio_delays_relative,
        selected_audio_delay_ms: 0.0,
    })
}

/// Calculate final delay by combining correlation and container delays — `calculate_delay_chain`
pub fn calculate_delay_chain(
    correlation_delay_ms: i32,
    correlation_delay_raw: f64,
    container_delay_ms: f64,
    log: &dyn Fn(&str),
    source_key: &str,
) -> (i32, f64) {
    let final_delay_ms = (correlation_delay_ms as f64 + container_delay_ms).round() as i32;
    let final_delay_raw = correlation_delay_raw + container_delay_ms;

    log(&format!("[Delay Calculation] {source_key} delay chain:"));
    log(&format!(
        "[Delay Calculation]   Correlation delay: {correlation_delay_raw:+.6}ms (raw) → {correlation_delay_ms:+}ms (rounded)"
    ));
    if container_delay_ms != 0.0 {
        log(&format!(
            "[Delay Calculation]   + Container delay:  {container_delay_ms:+.6}ms"
        ));
        log(&format!(
            "[Delay Calculation]   = Final delay:      {final_delay_raw:+.6}ms (raw) → {final_delay_ms:+}ms (rounded)"
        ));
    }

    (final_delay_ms, final_delay_raw)
}

/// Determine which Source 1 audio track was actually used for correlation — `find_actual_correlation_track_delay`
pub fn find_actual_correlation_track_delay(
    container_info: &ContainerDelayInfo,
    stream_info: Option<&serde_json::Value>,
    correlation_ref_track: Option<i32>,
    ref_lang: Option<&str>,
    default_delay_ms: f64,
    log: &dyn Fn(&str),
) -> f64 {
    let stream_info = match stream_info {
        Some(info) => info,
        None => return default_delay_ms,
    };

    let empty = vec![];
    let tracks = stream_info
        .get("tracks")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let audio_tracks: Vec<&serde_json::Value> = tracks
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("audio"))
        .collect();

    // Priority 1: Explicit per-job track selection
    if let Some(ref_track_idx) = correlation_ref_track {
        if ref_track_idx >= 0 && (ref_track_idx as usize) < audio_tracks.len() {
            let ref_track_id = audio_tracks[ref_track_idx as usize]
                .get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;
            let track_delay = container_info
                .audio_delays_ms
                .get(&ref_track_id)
                .copied()
                .unwrap_or(0.0);
            if track_delay != default_delay_ms {
                log(&format!(
                    "[Container Delay Override] Using Source 1 audio index {ref_track_idx} (track ID {ref_track_id}) delay: \
                     {track_delay:+.3}ms (global reference was {default_delay_ms:+.3}ms)"
                ));
                return track_delay;
            }
        }
    }
    // Priority 2: Language matching fallback
    else if let Some(lang) = ref_lang {
        let lang_lower = lang.trim().to_lowercase();
        for (i, track) in audio_tracks.iter().enumerate() {
            let track_lang = track
                .get("properties")
                .and_then(|p| p.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if track_lang == lang_lower {
                let ref_track_id = track.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let track_delay = container_info
                    .audio_delays_ms
                    .get(&ref_track_id)
                    .copied()
                    .unwrap_or(0.0);
                if track_delay != default_delay_ms {
                    log(&format!(
                        "[Container Delay Override] Using Source 1 audio index {i} (track ID {ref_track_id}, lang={lang}) delay: \
                         {track_delay:+.3}ms (global reference was {default_delay_ms:+.3}ms)"
                    ));
                    return track_delay;
                }
                break;
            }
        }
    }

    default_delay_ms
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}
