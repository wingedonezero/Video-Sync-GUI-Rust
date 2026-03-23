//! SRT and VTT subtitle file parsers — 1:1 port of `parsers/srt_parser.py`.
//!
//! Converts to SubtitleData with float millisecond timing.
//! SRT indices are preserved for round-trip if needed.
//! VTT (WebVTT) files are also supported with optional hour fields.

use std::path::Path;

use regex::Regex;

use crate::subtitles::data::*;

/// Encodings to try in order — matches Python's ENCODINGS_TO_TRY
const ENCODINGS_TO_TRY: &[&str] = &["utf-8", "windows-1252", "iso-8859-1"];

/// Detect file encoding — `detect_encoding`
///
/// Returns (encoding_name, has_bom).
/// Tries BOM detection first, then attempts each encoding.
fn detect_encoding(path: &Path) -> (String, bool) {
    let raw = match std::fs::read(path) {
        Ok(r) => r,
        Err(_) => return ("utf-8".to_string(), false),
    };

    // BOM detection
    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return ("utf-8-sig".to_string(), true);
    }
    if raw.starts_with(&[0xFF, 0xFE]) {
        return ("utf-16-le".to_string(), true);
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        return ("utf-16-be".to_string(), true);
    }

    // Try UTF-8 first (most common)
    if std::str::from_utf8(&raw).is_ok() {
        return ("utf-8".to_string(), false);
    }

    // Try other encodings via encoding_rs
    for &enc_name in ENCODINGS_TO_TRY {
        if let Some(encoding) = encoding_rs::Encoding::for_label(enc_name.as_bytes()) {
            let (result, _, had_errors) = encoding.decode(&raw);
            if !had_errors {
                return (enc_name.to_string(), false);
            }
            // Even with errors, if most of it decoded, it's probably this encoding
            if result.len() > raw.len() / 2 {
                return (enc_name.to_string(), false);
            }
        }
    }

    ("utf-8".to_string(), false)
}

/// Read file with detected encoding — returns content string
fn read_with_encoding(path: &Path) -> Result<(String, String, bool), String> {
    let raw = std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;

    let (enc_name, _has_bom) = detect_encoding(path);

    // Handle BOMs
    if enc_name == "utf-8-sig" {
        let bytes = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &raw[3..]
        } else {
            &raw
        };
        let content = String::from_utf8_lossy(bytes).to_string();
        return Ok((content, "utf-8".to_string(), true));
    }

    if enc_name == "utf-16-le" {
        let bytes = if raw.starts_with(&[0xFF, 0xFE]) {
            &raw[2..]
        } else {
            &raw
        };
        let content: String = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .map(|c| char::from_u32(c as u32).unwrap_or('\u{FFFD}'))
            .collect();
        return Ok((content, enc_name.to_string(), true));
    }

    if enc_name == "utf-16-be" {
        let bytes = if raw.starts_with(&[0xFE, 0xFF]) {
            &raw[2..]
        } else {
            &raw
        };
        let content: String = bytes
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .map(|c| char::from_u32(c as u32).unwrap_or('\u{FFFD}'))
            .collect();
        return Ok((content, enc_name.to_string(), true));
    }

    // Try UTF-8 first
    if let Ok(s) = String::from_utf8(raw.clone()) {
        return Ok((s, "utf-8".to_string(), false));
    }

    // Use encoding_rs for other encodings
    if let Some(encoding) = encoding_rs::Encoding::for_label(enc_name.as_bytes()) {
        let (result, _, _) = encoding.decode(&raw);
        return Ok((result.to_string(), enc_name.to_string(), false));
    }

    // Final fallback: lossy UTF-8
    Ok((String::from_utf8_lossy(&raw).to_string(), "utf-8".to_string(), false))
}

/// Parse SRT file to SubtitleData — `parse_srt_file`
pub fn parse_srt_file(path: &Path) -> Result<SubtitleData, String> {
    let (content, encoding, has_bom) = read_with_encoding(path)?;

    let mut data = SubtitleData::new();
    data.source_path = Some(path.to_path_buf());
    data.source_format = "srt".to_string();
    data.encoding = encoding;
    data.has_bom = has_bom;

    // Add default style for ASS conversion
    data.styles
        .push(("Default".to_string(), SubtitleStyle::new("Default")));

    // SRT timing pattern: HH:MM:SS,mmm --> HH:MM:SS,mmm
    let timing_re = Regex::new(
        r"(\d{1,2}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{1,2}):(\d{2}):(\d{2})[,.](\d{3})",
    )
    .unwrap();

    // Split into blocks (separated by blank lines)
    let blocks: Vec<&str> = Regex::new(r"\n\s*\n")
        .unwrap()
        .split(content.trim())
        .collect();

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

        if !first_line.is_empty() && first_line.chars().all(|c| c.is_ascii_digit()) {
            srt_index = first_line.parse().ok();
            timing_line_idx = 1;
        }

        if timing_line_idx >= lines.len() {
            continue;
        }

        // Parse timing line
        let timing_match = match timing_re.captures(lines[timing_line_idx].trim()) {
            Some(m) => m,
            None => continue,
        };

        let start_ms = parse_groups_to_ms(&timing_match, 1);
        let end_ms = parse_groups_to_ms(&timing_match, 5);

        // Remaining lines are text
        let text = lines[timing_line_idx + 1..].join("\n");
        let text = convert_srt_tags_to_ass(&text);

        let mut event = SubtitleEvent::new(start_ms, end_ms, &text);
        event.srt_index = srt_index;
        event.original_index = Some(block_idx as i32);
        data.events.push(event);
    }

    Ok(data)
}

/// Parse WebVTT file to SubtitleData — `parse_vtt_file`
pub fn parse_vtt_file(path: &Path) -> Result<SubtitleData, String> {
    let (content, encoding, has_bom) = read_with_encoding(path)?;

    let mut data = SubtitleData::new();
    data.source_path = Some(path.to_path_buf());
    data.source_format = "vtt".to_string();
    data.encoding = encoding;
    data.has_bom = has_bom;

    data.styles
        .push(("Default".to_string(), SubtitleStyle::new("Default")));

    // VTT timing: optional_hours:MM:SS.mmm --> optional_hours:MM:SS.mmm
    let timing_re = Regex::new(
        r"(?:(\d{1,2}):)?(\d{2}):(\d{2})[.](\d{3})\s*-->\s*(?:(\d{1,2}):)?(\d{2}):(\d{2})[.](\d{3})",
    )
    .unwrap();

    // Skip WEBVTT header
    let lines: Vec<&str> = content.lines().collect();
    let mut start_idx = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().to_uppercase().starts_with("WEBVTT") {
            start_idx = i + 1;
            break;
        }
    }

    // Rejoin and split into blocks
    let remaining = lines[start_idx..].join("\n");
    let blocks: Vec<&str> = Regex::new(r"\n\s*\n")
        .unwrap()
        .split(remaining.trim())
        .collect();

    for (block_idx, block) in blocks.iter().enumerate() {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let lines: Vec<&str> = block.lines().collect();
        if lines.is_empty() {
            continue;
        }

        // Find timing line (may not be first — VTT can have cue identifiers)
        let mut timing_match = None;
        let mut timing_line_idx = 0;
        for (i, line) in lines.iter().enumerate() {
            if let Some(m) = timing_re.captures(line.trim()) {
                timing_match = Some(m);
                timing_line_idx = i;
                break;
            }
        }

        let timing_match = match timing_match {
            Some(m) => m,
            None => continue,
        };

        // Extract timing — groups 1 and 5 are optional hours
        let h1: i64 = timing_match
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let m1: i64 = timing_match[2].parse().unwrap_or(0);
        let s1: i64 = timing_match[3].parse().unwrap_or(0);
        let ms1: i64 = timing_match[4].parse().unwrap_or(0);

        let h2: i64 = timing_match
            .get(5)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let m2: i64 = timing_match[6].parse().unwrap_or(0);
        let s2: i64 = timing_match[7].parse().unwrap_or(0);
        let ms2: i64 = timing_match[8].parse().unwrap_or(0);

        let start_ms = (h1 * 3_600_000 + m1 * 60_000 + s1 * 1_000 + ms1) as f64;
        let end_ms = (h2 * 3_600_000 + m2 * 60_000 + s2 * 1_000 + ms2) as f64;

        // Text is after timing line
        let text = lines[timing_line_idx + 1..].join("\n");
        let text = convert_vtt_tags_to_ass(&text);

        let mut event = SubtitleEvent::new(start_ms, end_ms, &text);
        event.original_index = Some(block_idx as i32);
        data.events.push(event);
    }

    Ok(data)
}

/// Parse regex capture groups at offset into milliseconds
fn parse_groups_to_ms(caps: &regex::Captures, offset: usize) -> f64 {
    let h: i64 = caps[offset].parse().unwrap_or(0);
    let m: i64 = caps[offset + 1].parse().unwrap_or(0);
    let s: i64 = caps[offset + 2].parse().unwrap_or(0);
    let ms: i64 = caps[offset + 3].parse().unwrap_or(0);
    (h * 3_600_000 + m * 60_000 + s * 1_000 + ms) as f64
}

/// Convert SRT HTML tags to ASS override tags — `_convert_srt_tags_to_ass`
pub fn convert_srt_tags_to_ass(text: &str) -> String {
    let mut result = text.to_string();

    // <b>text</b> -> {\b1}text{\b0}
    let re_b_open = Regex::new(r"(?i)<b>").unwrap();
    let re_b_close = Regex::new(r"(?i)</b>").unwrap();
    result = re_b_open.replace_all(&result, "{\\b1}").to_string();
    result = re_b_close.replace_all(&result, "{\\b0}").to_string();

    // <i>text</i> -> {\i1}text{\i0}
    let re_i_open = Regex::new(r"(?i)<i>").unwrap();
    let re_i_close = Regex::new(r"(?i)</i>").unwrap();
    result = re_i_open.replace_all(&result, "{\\i1}").to_string();
    result = re_i_close.replace_all(&result, "{\\i0}").to_string();

    // <u>text</u> -> {\u1}text{\u0}
    let re_u_open = Regex::new(r"(?i)<u>").unwrap();
    let re_u_close = Regex::new(r"(?i)</u>").unwrap();
    result = re_u_open.replace_all(&result, "{\\u1}").to_string();
    result = re_u_close.replace_all(&result, "{\\u0}").to_string();

    // <font color="...">text</font> -> {\c&HBBGGRR&}text{\c}
    // Convert #RRGGBB to ASS &HBBGGRR& format
    let re_font_color =
        Regex::new(r#"(?i)<font\s+color=["']?([^"'>\s]+)["']?\s*>"#).unwrap();
    result = re_font_color
        .replace_all(&result, |caps: &regex::Captures| {
            let color = &caps[1];
            if let Some(stripped) = color.strip_prefix('#') {
                if stripped.len() == 6 {
                    let r = &stripped[0..2];
                    let g = &stripped[2..4];
                    let b = &stripped[4..6];
                    return format!("{{\\c&H{b}{g}{r}&}}");
                }
            }
            String::new()
        })
        .to_string();

    let re_font_close = Regex::new(r"(?i)</font>").unwrap();
    result = re_font_close.replace_all(&result, "{\\c}").to_string();

    // Remove remaining HTML tags
    let re_tags = Regex::new(r"<[^>]+>").unwrap();
    result = re_tags.replace_all(&result, "").to_string();

    // Convert \n to ASS line break
    result = result.replace('\n', "\\N");

    result
}

/// Convert VTT tags to ASS override tags — `_convert_vtt_tags_to_ass`
pub fn convert_vtt_tags_to_ass(text: &str) -> String {
    // VTT uses similar tags to SRT
    let mut result = convert_srt_tags_to_ass(text);

    // VTT-specific: <c.classname>text</c>
    let re_c_open = Regex::new(r"<c[^>]*>").unwrap();
    let re_c_close = Regex::new(r"</c>").unwrap();
    result = re_c_open.replace_all(&result, "").to_string();
    result = re_c_close.replace_all(&result, "").to_string();

    // Voice spans: <v name>text</v>
    let re_v_open = Regex::new(r"<v[^>]*>").unwrap();
    let re_v_close = Regex::new(r"</v>").unwrap();
    result = re_v_open.replace_all(&result, "").to_string();
    result = re_v_close.replace_all(&result, "").to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_tag_conversion_basic() {
        assert_eq!(
            convert_srt_tags_to_ass("<b>bold</b>"),
            "{\\b1}bold{\\b0}"
        );
        assert_eq!(
            convert_srt_tags_to_ass("<i>italic</i>"),
            "{\\i1}italic{\\i0}"
        );
    }

    #[test]
    fn srt_tag_conversion_color() {
        let red_input = "<font color=\"#FF0000\">red</font>";
        assert_eq!(
            convert_srt_tags_to_ass(red_input),
            "{\\c&H0000FF&}red{\\c}"
        );
        let green_input = "<font color=\"#00FF00\">green</font>";
        assert_eq!(
            convert_srt_tags_to_ass(green_input),
            "{\\c&H00FF00&}green{\\c}"
        );
    }

    #[test]
    fn srt_tag_conversion_case_insensitive() {
        assert_eq!(
            convert_srt_tags_to_ass("<B>bold</B>"),
            "{\\b1}bold{\\b0}"
        );
    }

    #[test]
    fn srt_tag_strip_unknown() {
        assert_eq!(convert_srt_tags_to_ass("<div>text</div>"), "text");
    }

    #[test]
    fn vtt_tag_conversion() {
        assert_eq!(
            convert_vtt_tags_to_ass("<c.yellow>text</c>"),
            "text"
        );
        assert_eq!(
            convert_vtt_tags_to_ass("<v Speaker>text</v>"),
            "text"
        );
    }

    #[test]
    fn line_break_conversion() {
        assert_eq!(
            convert_srt_tags_to_ass("line1\nline2"),
            "line1\\Nline2"
        );
    }
}
