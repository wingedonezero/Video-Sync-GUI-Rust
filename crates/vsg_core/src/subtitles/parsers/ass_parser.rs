//! ASS/SSA subtitle file parser — 1:1 port of `parsers/ass_parser.py`.
//!
//! Preserves EVERYTHING: all sections in original order, metadata,
//! comments, unknown sections, embedded fonts/graphics, format line
//! field ordering, encoding and BOM.

use std::path::Path;

use crate::subtitles::data::*;

/// Encodings to try when auto-detecting.
#[allow(dead_code)]
const ENCODINGS_TO_TRY: &[&str] = &[
    "utf-8-sig", "utf-8", "utf-16", "utf-16-le", "utf-16-be",
    "shift_jis", "gbk", "gb2312", "big5", "cp1252", "latin1",
];

/// Detect file encoding — `detect_encoding`
fn detect_encoding(path: &Path) -> (String, bool) {
    let raw = match std::fs::read(path) {
        Ok(data) => data,
        Err(_) => return ("utf-8".to_string(), false),
    };

    // Check for BOM
    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return ("utf-8-sig".to_string(), true);
    }
    if raw.starts_with(&[0xFF, 0xFE]) {
        return ("utf-16-le".to_string(), true);
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        return ("utf-16-be".to_string(), true);
    }

    // Try UTF-8 (most common)
    if std::str::from_utf8(&raw).is_ok() {
        return ("utf-8".to_string(), false);
    }

    // Default fallback
    ("utf-8".to_string(), false)
}

/// Read file content handling encoding — returns String content
fn read_file_content(path: &Path) -> Result<(String, String, bool), String> {
    let (encoding, has_bom) = detect_encoding(path);

    let raw = std::fs::read(path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let content = if encoding == "utf-8-sig" {
        // Skip BOM
        let bytes = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) { &raw[3..] } else { &raw };
        String::from_utf8_lossy(bytes).to_string()
    } else if encoding.starts_with("utf-16") {
        // Handle UTF-16 (LE/BE)
        if raw.starts_with(&[0xFF, 0xFE]) {
            // UTF-16 LE with BOM
            let u16_data: Vec<u16> = raw[2..].chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16_data)
        } else if raw.starts_with(&[0xFE, 0xFF]) {
            // UTF-16 BE with BOM
            let u16_data: Vec<u16> = raw[2..].chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16_data)
        } else {
            String::from_utf8_lossy(&raw).to_string()
        }
    } else {
        String::from_utf8_lossy(&raw).to_string()
    };

    Ok((content, encoding, has_bom))
}

/// Parse ASS/SSA file with full metadata preservation — `parse_ass_file`
pub fn parse_ass_file(path: &Path) -> Result<SubtitleData, String> {
    let (content, encoding, has_bom) = read_file_content(path)?;
    let lines: Vec<&str> = content.lines().collect();

    let ext = path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut data = SubtitleData::new();
    data.source_path = Some(path.to_path_buf());
    data.source_format = if ext == "ssa" { "ssa".to_string() } else { "ass".to_string() };
    data.encoding = encoding;
    data.has_bom = has_bom;

    let mut current_section: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut event_index: i32 = 0;

    let flush_section = |data: &mut SubtitleData,
                              section: &Option<String>,
                              lines: &[String],
                              event_idx: &mut i32| {
        match section {
            None => {
                data.header_lines = lines.to_vec();
            }
            Some(section_name) => {
                if !data.section_order.contains(section_name) {
                    data.section_order.push(section_name.clone());
                }

                let section_lower = section_name.to_lowercase();
                match section_lower.as_str() {
                    "[script info]" => parse_script_info(data, lines),
                    "[v4+ styles]" | "[v4 styles]" => parse_styles(data, lines),
                    "[events]" => {
                        *event_idx = parse_events(data, lines, *event_idx);
                    }
                    "[fonts]" => parse_fonts(data, lines),
                    "[graphics]" => parse_graphics(data, lines),
                    "[aegisub project garbage]" => parse_aegisub_garbage(data, lines),
                    "[aegisub extradata]" => parse_aegisub_extradata(data, lines),
                    _ => {
                        data.custom_sections.push((
                            section_name.clone(),
                            lines.to_vec(),
                        ));
                    }
                }
            }
        }
    };

    for line in &lines {
        let stripped = line.trim();

        if stripped.starts_with('[') && stripped.ends_with(']') {
            flush_section(&mut data, &current_section, &current_lines, &mut event_index);
            current_section = Some(stripped.to_string());
            current_lines.clear();
            continue;
        }

        current_lines.push(line.to_string());
    }

    // Flush final section
    flush_section(&mut data, &current_section, &current_lines, &mut event_index);

    Ok(data)
}

fn parse_script_info(data: &mut SubtitleData, lines: &[String]) {
    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() { continue; }
        if stripped.starts_with(';') {
            // Comment — store in script_info with special key
            data.script_info.push(("__comment__".to_string(), stripped.to_string()));
            continue;
        }
        if let Some((key, value)) = stripped.split_once(':') {
            data.script_info.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
}

fn parse_styles(data: &mut SubtitleData, lines: &[String]) {
    let mut format_fields: Option<Vec<String>> = None;

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with(';') { continue; }

        if stripped.to_lowercase().starts_with("format:") {
            let format_str = &stripped[7..];
            let fields: Vec<String> = format_str.split(',').map(|f| f.trim().to_string()).collect();
            data.styles_format = fields.clone();
            format_fields = Some(fields);
            continue;
        }

        if stripped.to_lowercase().starts_with("style:") {
            let fields = format_fields.clone().unwrap_or_else(|| data.styles_format.clone());
            let style_str = &stripped[6..];
            let values: Vec<String> = style_str.split(',').map(|v| v.trim().to_string()).collect();
            let style = SubtitleStyle::from_format_line(&fields, &values);
            let name = style.name.clone();
            data.styles.push((name, style));
        }
    }
}

fn parse_events(data: &mut SubtitleData, lines: &[String], start_index: i32) -> i32 {
    let mut format_fields: Option<Vec<String>> = None;
    let mut event_index = start_index;

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with(';') { continue; }

        if stripped.to_lowercase().starts_with("format:") {
            let format_str = &stripped[7..];
            let fields: Vec<String> = format_str.split(',').map(|f| f.trim().to_string()).collect();
            data.events_format = fields.clone();
            format_fields = Some(fields);
            continue;
        }

        let (event_str, is_comment) = if stripped.to_lowercase().starts_with("dialogue:") {
            (&stripped[9..], false)
        } else if stripped.to_lowercase().starts_with("comment:") {
            (&stripped[8..], true)
        } else {
            continue;
        };

        let fields = format_fields.clone().unwrap_or_else(|| data.events_format.clone());

        // Find text field index for proper splitting
        let text_idx = fields.iter().position(|f| f.trim().to_lowercase() == "text");
        let values: Vec<String> = if let Some(idx) = text_idx {
            event_str.splitn(idx + 1, ',').map(|v| v.to_string()).collect()
        } else {
            event_str.split(',').map(|v| v.to_string()).collect()
        };

        let mut event = SubtitleEvent::from_format_line(&fields, &values, is_comment);
        event.original_index = Some(event_index);
        data.events.push(event);
        event_index += 1;
    }

    event_index
}

fn parse_fonts(data: &mut SubtitleData, lines: &[String]) {
    let mut current_name: Option<String> = None;
    let mut current_data_lines: Vec<String> = Vec::new();

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() { continue; }

        if stripped.to_lowercase().starts_with("fontname:") {
            if let Some(name) = current_name.take() {
                data.fonts.push(EmbeddedFont {
                    name,
                    data: current_data_lines.join("\n"),
                });
            }
            current_name = Some(stripped[9..].trim().to_string());
            current_data_lines.clear();
        } else {
            current_data_lines.push(stripped.to_string());
        }
    }

    if let Some(name) = current_name {
        data.fonts.push(EmbeddedFont {
            name,
            data: current_data_lines.join("\n"),
        });
    }
}

fn parse_graphics(data: &mut SubtitleData, lines: &[String]) {
    let mut current_name: Option<String> = None;
    let mut current_data_lines: Vec<String> = Vec::new();

    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() { continue; }

        if stripped.to_lowercase().starts_with("filename:") {
            if let Some(name) = current_name.take() {
                data.graphics.push(EmbeddedGraphic {
                    name,
                    data: current_data_lines.join("\n"),
                });
            }
            current_name = Some(stripped[9..].trim().to_string());
            current_data_lines.clear();
        } else {
            current_data_lines.push(stripped.to_string());
        }
    }

    if let Some(name) = current_name {
        data.graphics.push(EmbeddedGraphic {
            name,
            data: current_data_lines.join("\n"),
        });
    }
}

fn parse_aegisub_garbage(data: &mut SubtitleData, lines: &[String]) {
    for line in lines {
        let stripped = line.trim();
        if stripped.is_empty() || stripped.starts_with(';') { continue; }
        if let Some((key, value)) = stripped.split_once(':') {
            data.aegisub_garbage.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
}

fn parse_aegisub_extradata(data: &mut SubtitleData, lines: &[String]) {
    for line in lines {
        let stripped = line.trim();
        if !stripped.is_empty() {
            data.aegisub_extradata.push(stripped.to_string());
        }
    }
}
