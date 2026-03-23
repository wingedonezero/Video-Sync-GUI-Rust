//! SRT subtitle file parser — 1:1 port of `parsers/srt_parser.py`.
//!
//! Converts to SubtitleData with float millisecond timing.
//! SRT indices are preserved for round-trip if needed.

use std::path::Path;

use crate::subtitles::data::*;

/// Parse SRT file to SubtitleData — `parse_srt_file`
pub fn parse_srt_file(path: &Path) -> Result<SubtitleData, String> {
    let raw = std::fs::read(path)
        .map_err(|e| format!("Failed to read SRT file: {e}"))?;

    let has_bom = raw.starts_with(&[0xEF, 0xBB, 0xBF]);
    let bytes = if has_bom { &raw[3..] } else { &raw };
    let content = String::from_utf8_lossy(bytes).to_string();

    let mut data = SubtitleData::new();
    data.source_path = Some(path.to_path_buf());
    data.source_format = "srt".to_string();
    data.encoding = "utf-8".to_string();
    data.has_bom = has_bom;

    // Add default style for ASS conversion
    data.styles.push(("Default".to_string(), SubtitleStyle::new("Default")));

    // Split into blocks (separated by blank lines)
    let blocks: Vec<&str> = content.split("\n\n").collect();

    for (block_idx, block) in blocks.iter().enumerate() {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 2 {
            continue;
        }

        // First line might be index
        let first_line = lines[0].trim();
        let mut timing_line_idx = 0;
        let mut srt_index: Option<i32> = None;

        if first_line.chars().all(|c| c.is_ascii_digit()) && !first_line.is_empty() {
            srt_index = first_line.parse().ok();
            timing_line_idx = 1;
        }

        if timing_line_idx >= lines.len() {
            continue;
        }

        // Parse timing line: HH:MM:SS,mmm --> HH:MM:SS,mmm
        let timing_line = lines[timing_line_idx].trim();
        let (start_ms, end_ms) = match parse_srt_timing(timing_line) {
            Some(t) => t,
            None => continue,
        };

        // Remaining lines are text
        let text_lines: Vec<&str> = lines[timing_line_idx + 1..].to_vec();
        let text = text_lines.join("\n");

        // Convert SRT HTML tags to ASS
        let text = convert_srt_tags_to_ass(&text);

        let mut event = SubtitleEvent::new(start_ms, end_ms, &text);
        event.srt_index = srt_index;
        event.original_index = Some(block_idx as i32);
        data.events.push(event);
    }

    Ok(data)
}

/// Parse SRT timing line — returns (start_ms, end_ms) or None
fn parse_srt_timing(line: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_srt_timestamp(parts[0].trim())?;
    let end = parse_srt_timestamp(parts[1].trim())?;
    Some((start, end))
}

/// Parse SRT timestamp HH:MM:SS,mmm or HH:MM:SS.mmm to float ms
fn parse_srt_timestamp(ts: &str) -> Option<f64> {
    let ts = ts.replace(',', ".");
    let parts: Vec<&str> = ts.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: i64 = parts[0].parse().ok()?;
    let minutes: i64 = parts[1].parse().ok()?;

    let sec_parts: Vec<&str> = parts[2].splitn(2, '.').collect();
    let seconds: i64 = sec_parts[0].parse().ok()?;
    let millis: i64 = if sec_parts.len() > 1 {
        sec_parts[1].parse().ok()?
    } else {
        0
    };

    Some((hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + millis) as f64)
}

/// Convert SRT HTML tags to ASS override tags — `_convert_srt_tags_to_ass`
fn convert_srt_tags_to_ass(text: &str) -> String {
    let mut result = text.to_string();

    // <b>text</b> -> {\b1}text{\b0}
    result = result.replace("<b>", "{\\b1}").replace("<B>", "{\\b1}");
    result = result.replace("</b>", "{\\b0}").replace("</B>", "{\\b0}");

    // <i>text</i> -> {\i1}text{\i0}
    result = result.replace("<i>", "{\\i1}").replace("<I>", "{\\i1}");
    result = result.replace("</i>", "{\\i0}").replace("</I>", "{\\i0}");

    // <u>text</u> -> {\u1}text{\u0}
    result = result.replace("<u>", "{\\u1}").replace("<U>", "{\\u1}");
    result = result.replace("</u>", "{\\u0}").replace("</U>", "{\\u0}");

    // Remove </font> tags
    result = result.replace("</font>", "{\\c}").replace("</FONT>", "{\\c}");

    // Remove other HTML tags (simple approach)
    // For font color tags, a full regex would be needed for the color conversion
    // but for now, strip remaining tags
    let mut clean = String::new();
    let mut in_tag = false;
    for ch in result.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            clean.push(ch);
        }
    }

    // Convert \n to ASS line break
    clean = clean.replace('\n', "\\N");

    clean
}
