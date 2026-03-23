//! SRT subtitle file writer — 1:1 port of `writers/srt_writer.py`.

use std::fs;
use std::path::Path;

use crate::subtitles::data::SubtitleData;

/// Write SubtitleData to SRT file — `write_srt_file`
pub fn write_srt_file(data: &SubtitleData, path: &Path, rounding: &str) -> Result<(), String> {
    let mut lines: Vec<String> = Vec::new();

    let dialogue_events: Vec<_> = data.events.iter().filter(|e| !e.is_comment).collect();

    for (idx, event) in dialogue_events.iter().enumerate() {
        let srt_idx = event.srt_index.unwrap_or((idx + 1) as i32);

        // Index line
        lines.push(srt_idx.to_string());

        // Timing line
        let start_str = format_srt_time(event.start_ms, rounding);
        let end_str = format_srt_time(event.end_ms, rounding);
        lines.push(format!("{start_str} --> {end_str}"));

        // Text (convert ASS tags to SRT)
        let text = convert_ass_to_srt(&event.text);
        lines.push(text);

        // Blank line separator
        lines.push(String::new());
    }

    let content = lines.join("\n");

    let output = if data.has_bom {
        format!("\u{FEFF}{content}")
    } else {
        content
    };

    fs::write(path, output)
        .map_err(|e| format!("Failed to write SRT file: {e}"))
}

/// Format float ms to SRT timestamp HH:MM:SS,mmm — `_format_srt_time`
fn format_srt_time(ms: f64, rounding: &str) -> String {
    let total_ms = round_ms(ms, rounding).max(0);

    let milliseconds = total_ms % 1000;
    let total_seconds = total_ms / 1000;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;

    format!("{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}")
}

/// Round ms to integer — `_round_ms`
fn round_ms(ms: f64, rounding: &str) -> i64 {
    match rounding.to_lowercase().as_str() {
        "ceil" => ms.ceil() as i64,
        "floor" => ms.floor() as i64,
        _ => ms.round() as i64, // "round" default
    }
}

/// Convert ASS tags to SRT — `_convert_ass_to_srt`
fn convert_ass_to_srt(text: &str) -> String {
    let mut result = text.to_string();

    // Convert line breaks
    result = result.replace("\\N", "\n");
    result = result.replace("\\n", "\n");

    // Convert bold
    result = result.replace("{\\b1}", "<b>");
    result = result.replace("{\\b0}", "</b>");

    // Convert italic
    result = result.replace("{\\i1}", "<i>");
    result = result.replace("{\\i0}", "</i>");

    // Convert underline
    result = result.replace("{\\u1}", "<u>");
    result = result.replace("{\\u0}", "</u>");

    // Remove all other ASS override blocks {\\...}
    let mut clean = String::new();
    let mut in_override = false;
    for ch in result.chars() {
        if ch == '{' {
            in_override = true;
        } else if ch == '}' {
            in_override = false;
        } else if !in_override {
            clean.push(ch);
        }
    }

    clean
}
