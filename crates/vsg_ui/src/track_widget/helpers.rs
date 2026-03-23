//! Track widget helpers — 1:1 port of `vsg_qt/track_widget/helpers.py`.
//!
//! Display formatting utilities for track information.

/// Format a track codec for display — e.g., "A_AAC" → "AAC".
pub fn format_codec_display(codec: &str) -> String {
    // Strip common MKV codec prefixes
    let stripped = codec
        .strip_prefix("A_")
        .or_else(|| codec.strip_prefix("V_"))
        .or_else(|| codec.strip_prefix("S_"))
        .unwrap_or(codec);
    stripped.to_string()
}

/// Format track type for display — e.g., "audio" → "Audio".
pub fn format_track_type(track_type: &str) -> &str {
    match track_type {
        "audio" => "Audio",
        "video" => "Video",
        "subtitles" => "Subtitles",
        _ => track_type,
    }
}

/// Build a display description for a track.
pub fn build_track_description(
    track_type: &str,
    codec: &str,
    language: &str,
    name: &str,
) -> String {
    let type_str = format_track_type(track_type);
    let codec_str = format_codec_display(codec);

    let mut parts = vec![format!("{type_str}: {codec_str}")];

    if !language.is_empty() {
        parts.push(format!("[{language}]"));
    }
    if !name.is_empty() {
        parts.push(format!("- {name}"));
    }

    parts.join(" ")
}
