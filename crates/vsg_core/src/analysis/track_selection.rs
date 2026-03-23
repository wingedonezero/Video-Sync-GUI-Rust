//! Audio track selection — 1:1 port of `vsg_core/analysis/track_selection.py`.

use super::types::TrackSelection;

/// Audio codec ID to friendly name mapping.
fn codec_friendly_name(codec_id: &str) -> String {
    let map = [
        ("A_FLAC", "FLAC"),
        ("A_AAC", "AAC"),
        ("A_AC3", "AC3"),
        ("A_EAC3", "E-AC3"),
        ("A_DTS", "DTS"),
        ("A_TRUEHD", "TrueHD"),
        ("A_OPUS", "Opus"),
        ("A_VORBIS", "Vorbis"),
        ("A_PCM", "PCM"),
        ("A_MP3", "MP3"),
    ];
    // Exact match first
    for (prefix, name) in &map {
        if codec_id == *prefix {
            return name.to_string();
        }
    }
    // Prefix match
    for (prefix, name) in &map {
        if codec_id.starts_with(prefix) {
            return name.to_string();
        }
    }
    codec_id.replace("A_", "")
}

/// Format audio track details for logging — `format_track_details`
pub fn format_track_details(track: &serde_json::Value, index: i32) -> String {
    let props = track.get("properties").unwrap_or(&serde_json::Value::Null);
    let lang = props.get("language").and_then(|v| v.as_str()).unwrap_or("und");
    let codec_id = props.get("codec_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let codec_name = codec_friendly_name(codec_id);
    let channels = props.get("audio_channels").and_then(|v| v.as_i64()).unwrap_or(2);
    let channel_str = match channels {
        1 => "Mono".to_string(),
        2 => "2.0".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        n => format!("{n}ch"),
    };
    let track_name = props.get("track_name").and_then(|v| v.as_str()).unwrap_or("");

    let mut parts = vec![format!("Track {index}: {lang}")];
    parts.push(format!("{codec_name} {channel_str}"));
    if !track_name.is_empty() {
        parts.push(format!("'{track_name}'"));
    }
    parts.join(", ")
}

/// Select an audio track for correlation analysis — `select_audio_track`
pub fn select_audio_track(
    audio_tracks: &[serde_json::Value],
    language: Option<&str>,
    explicit_index: Option<i32>,
    log: &dyn Fn(&str),
    source_label: &str,
) -> Option<TrackSelection> {
    if audio_tracks.is_empty() {
        log(&format!("[WARN] No audio tracks found in {source_label}."));
        return None;
    }

    let mut selected_track: Option<&serde_json::Value> = None;
    let mut selected_index: i32 = 0;
    let mut selection_reason = "first";

    // Priority 1: Explicit track index
    if let Some(explicit) = explicit_index {
        if explicit >= 0 && (explicit as usize) < audio_tracks.len() {
            selected_track = Some(&audio_tracks[explicit as usize]);
            selected_index = explicit;
            selection_reason = "explicit";
            log(&format!(
                "[{source_label}] Selected (explicit): {}",
                format_track_details(&audio_tracks[explicit as usize], explicit)
            ));
        } else {
            log(&format!(
                "[WARN] Invalid track index {explicit}, falling back to first track"
            ));
            selected_track = Some(&audio_tracks[0]);
            selected_index = 0;
            selection_reason = "first";
            log(&format!(
                "[{source_label}] Selected (fallback): {}",
                format_track_details(&audio_tracks[0], 0)
            ));
        }
    }
    // Priority 2: Language matching
    else if let Some(lang) = language {
        let lang_lower = lang.trim().to_lowercase();
        for (idx, track) in audio_tracks.iter().enumerate() {
            let track_lang = track
                .get("properties")
                .and_then(|p| p.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if track_lang == lang_lower {
                selected_track = Some(track);
                selected_index = idx as i32;
                selection_reason = "language";
                log(&format!(
                    "[{source_label}] Selected (lang={lang}): {}",
                    format_track_details(track, idx as i32)
                ));
                break;
            }
        }
    }

    // Priority 3: First track fallback
    if selected_track.is_none() {
        selected_track = Some(&audio_tracks[0]);
        selected_index = 0;
        selection_reason = "first";
        log(&format!(
            "[{source_label}] Selected (first track): {}",
            format_track_details(&audio_tracks[0], 0)
        ));
    }

    let track = selected_track?;
    let track_id = track.get("id").and_then(|v| v.as_i64())? as i32;
    let props = track.get("properties").unwrap_or(&serde_json::Value::Null);
    let lang_code = props.get("language").and_then(|v| v.as_str()).unwrap_or("und").to_string();
    let codec_id = props.get("codec_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    let channels = props.get("audio_channels").and_then(|v| v.as_i64()).unwrap_or(2) as i32;

    Some(TrackSelection {
        track_id,
        track_index: selected_index,
        selected_by: selection_reason.to_string(),
        language: lang_code,
        codec: codec_friendly_name(codec_id),
        channels,
        formatted_name: format_track_details(track, selected_index),
    })
}
