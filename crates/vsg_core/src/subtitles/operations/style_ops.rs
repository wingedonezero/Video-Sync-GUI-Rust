//! Style operations for SubtitleData — 1:1 port of `operations/style_ops.py`.
//!
//! Operations:
//! - apply_style_patch: Apply attribute changes to styles
//! - apply_font_replacement: Replace font names
//! - apply_size_multiplier: Scale font sizes
//! - apply_rescale: Rescale to target resolution (Aegisub "Add Borders" style)
//! - apply_style_filter: Filter events by style name (include/exclude)

use std::collections::{HashMap, HashSet};

use chrono::Local;
use regex::Regex;

use crate::subtitles::data::{OperationRecord, OperationResult, SubtitleData};
use crate::subtitles::style_engine::qt_color_to_ass;

// =============================================================================
// Override Tag Scaling
// =============================================================================

/// Find the position of the matching closing paren.
fn find_matching_paren(text: &str, open_pos: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 1;
    let mut pos = open_pos + 1;
    while pos < bytes.len() {
        if bytes[pos] == b'(' {
            depth += 1;
        } else if bytes[pos] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

/// Split \t() content into timing/accel params and the tag block.
///
/// In ASS, \t() format is: \t([t1,t2,][accel,]\tags...)
/// The tag block always starts with '\'. Everything before that
/// first backslash is the numeric timing/acceleration parameters.
fn split_transform_args(content: &str) -> (&str, &str) {
    for (i, ch) in content.char_indices() {
        if ch == '\\' {
            let timing = content[..i].trim_end_matches([',', ' ']);
            let tags = &content[i..];
            return (timing, tags);
        }
    }
    // No backslash found — everything is timing/accel, no tags
    (content, "")
}

/// Scale a numeric value, add offset, and format cleanly.
fn scale_value(val: &str, scale_factor: f64, offset: f64) -> String {
    match val.trim().parse::<f64>() {
        Ok(v) => {
            let scaled = v * scale_factor + offset;
            let formatted = format!("{:.3}", scaled);
            // Trim trailing zeros and trailing dot
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            trimmed.to_string()
        }
        Err(_) => val.to_string(),
    }
}

// Tags that need vertical scaling (absolute pixel sizes)
const SCALE_H_TAGS: &[&str] = &["fs", "blur", "be", "fsp", "ybord", "yshad", "pbo", "shad"];
// Tags that need uniform/horizontal scaling (absolute pixel sizes)
const SCALE_TAGS: &[&str] = &["bord", "xbord", "xshad"];
// Position tags with (x, y) that need offsets
const POS_TAGS: &[&str] = &["pos", "org"];
// Clip tags with (x1, y1, x2, y2) that need offsets
const CLIP_TAGS: &[&str] = &["clip", "iclip"];

/// Scale parenthesised tag arguments based on tag type.
fn scale_tag(
    tag_name: &str,
    args: &str,
    scale: f64,
    scale_h: f64,
    offset_x: f64,
    offset_y: f64,
) -> String {
    if args.is_empty() {
        return args.to_string();
    }

    let tag_lower = tag_name.to_lowercase();
    let parts: Vec<&str> = args.split(',').map(|p| p.trim()).collect();
    let mut scaled_parts = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        if SCALE_TAGS.contains(&tag_lower.as_str()) {
            // Uniform/horizontal pixel measurements (no offset)
            scaled_parts.push(scale_value(part, scale, 0.0));
        } else if SCALE_H_TAGS.contains(&tag_lower.as_str()) {
            // Vertical pixel measurements and font size (no offset)
            scaled_parts.push(scale_value(part, scale_h, 0.0));
        } else if POS_TAGS.contains(&tag_lower.as_str()) && i < 2 {
            // Position tags (x, y) -- need offsets
            let pos_offset = if i == 0 { offset_x } else { offset_y };
            scaled_parts.push(scale_value(part, scale, pos_offset));
        } else if CLIP_TAGS.contains(&tag_lower.as_str()) && i < 4 {
            // Clip rectangles -- need offsets
            let clip_offset = if i == 0 || i == 2 { offset_x } else { offset_y };
            scaled_parts.push(scale_value(part, scale, clip_offset));
        } else if tag_lower == "move" {
            // move tag: (x1, y1, x2, y2, t1, t2) -- positions need offsets
            if i == 0 || i == 2 {
                // x coordinates
                scaled_parts.push(scale_value(part, scale, offset_x));
            } else if i == 1 || i == 3 {
                // y coordinates
                scaled_parts.push(scale_value(part, scale, offset_y));
            } else {
                // time values -- preserve
                scaled_parts.push(part.to_string());
            }
        } else {
            // Everything else (time-based, percentages, etc.) -- preserve
            scaled_parts.push(part.to_string());
        }
    }

    scaled_parts.join(",")
}

/// Process non-\t tags via regex.
fn process_simple_tags(
    content: &str,
    scale: f64,
    scale_h: f64,
    offset_x: f64,
    offset_y: f64,
) -> String {
    let tag_pattern = Regex::new(r"\\([a-zA-Z]+)(\([^)]*\)|(?:\-?\d+(?:\.\d+)?))?").unwrap();

    tag_pattern
        .replace_all(content, |caps: &regex::Captures| {
            let tag_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let tag_lower = tag_name.to_lowercase();
            let args_or_value = caps.get(2).map(|m| m.as_str());

            match args_or_value {
                None => caps[0].to_string(),
                Some(aov) if aov.starts_with('(') => {
                    // Tag with parentheses -- scale args
                    let args = &aov[1..aov.len() - 1];
                    let scaled_args = scale_tag(tag_name, args, scale, scale_h, offset_x, offset_y);
                    format!("\\{tag_name}({scaled_args})")
                }
                Some(value) => {
                    // Shorthand numeric value
                    if SCALE_H_TAGS.contains(&tag_lower.as_str()) {
                        let scaled = scale_value(value, scale_h, 0.0);
                        format!("\\{tag_name}{scaled}")
                    } else if SCALE_TAGS.contains(&tag_lower.as_str()) {
                        let scaled = scale_value(value, scale, 0.0);
                        format!("\\{tag_name}{scaled}")
                    } else {
                        caps[0].to_string()
                    }
                }
            }
        })
        .to_string()
}

/// Process all tags within a single {...} block.
///
/// Handles \t() transform blocks with balanced-paren matching
/// and recursive tag processing.
fn process_override_block(
    block_content: &str,
    scale: f64,
    scale_h: f64,
    offset_x: f64,
    offset_y: f64,
) -> String {
    let mut segments: Vec<String> = Vec::new();
    let mut last_end = 0;
    let mut i = 0;
    let bytes = block_content.as_bytes();

    while i < bytes.len() {
        // Look for \t( — the transform tag with opening paren
        if bytes[i] == b'\\'
            && i + 2 < bytes.len()
            && bytes[i + 1] == b't'
            && bytes[i + 2] == b'('
        {
            // Process everything before this \t with the regex pass
            if i > last_end {
                segments.push(process_simple_tags(
                    &block_content[last_end..i],
                    scale,
                    scale_h,
                    offset_x,
                    offset_y,
                ));
            }

            if let Some(close) = find_matching_paren(block_content, i + 2) {
                let t_content = &block_content[i + 3..close];
                let (timing, tags) = split_transform_args(t_content);

                if !tags.is_empty() {
                    // Recursively process tags inside the transform
                    let processed_tags =
                        process_override_block(tags, scale, scale_h, offset_x, offset_y);
                    if !timing.is_empty() {
                        segments.push(format!("\\t({timing},{processed_tags})"));
                    } else {
                        segments.push(format!("\\t({processed_tags})"));
                    }
                } else {
                    // No tags, just timing/accel -- preserve as-is
                    segments.push(format!("\\t({timing})"));
                }

                i = close + 1;
                last_end = i;
            } else {
                // Unmatched paren -- don't touch, advance past \t
                i += 3;
            }
        } else {
            i += 1;
        }
    }

    // Process remaining content after the last \t() block
    if last_end < block_content.len() {
        segments.push(process_simple_tags(
            &block_content[last_end..],
            scale,
            scale_h,
            offset_x,
            offset_y,
        ));
    }

    segments.join("")
}

/// Scales all ASS override tags using uniform scaling and adds border offsets.
/// Maintains aspect ratio like Aegisub's "Add Borders" resampling.
/// Uses vertical scaling (scale_h) for font sizes to match Aegisub behavior.
///
/// Matches Aegisub's resolution resampler behaviour:
/// - Percentage-based tags (\fscx, \fscy) are NOT rescaled
/// - \t() transform blocks are recursively processed
/// - \iclip is handled identically to \clip
fn scale_override_tags(
    text: &str,
    scale: f64,
    scale_h: f64,
    offset_x: f64,
    offset_y: f64,
) -> String {
    let block_pattern = Regex::new(r"\{([^}]*)\}").unwrap();

    block_pattern
        .replace_all(text, |caps: &regex::Captures| {
            let block_content = &caps[1];
            let scaled_content =
                process_override_block(block_content, scale, scale_h, offset_x, offset_y);
            format!("{{{scaled_content}}}")
        })
        .to_string()
}

// =============================================================================
// Color / Bool Attribute Conversion
// =============================================================================

/// Color attributes that need Qt->ASS conversion.
const COLOR_ATTRIBUTES: &[&str] = &[
    "primary_color",
    "secondary_color",
    "outline_color",
    "back_color",
];

/// Bool attributes that need bool->int conversion (-1 for True, 0 for False).
const BOOL_ATTRIBUTES: &[&str] = &["bold", "italic", "underline", "strike_out"];

/// Convert patch values to SubtitleStyle-compatible format.
fn convert_patch_value(attr_name: &str, value: &serde_json::Value) -> serde_json::Value {
    if COLOR_ATTRIBUTES.contains(&attr_name) {
        if let Some(s) = value.as_str() {
            if s.starts_with('#') {
                return serde_json::json!(qt_color_to_ass(s));
            }
        }
        return value.clone();
    }

    if BOOL_ATTRIBUTES.contains(&attr_name) {
        if let Some(b) = value.as_bool() {
            return serde_json::json!(if b { -1 } else { 0 });
        }
        return value.clone();
    }

    value.clone()
}

// =============================================================================
// Style Attribute Mapping
// =============================================================================

/// Map common attribute names to SubtitleStyle field names.
fn map_style_attribute(attr: &str) -> String {
    let lower = attr.to_lowercase().replace('-', "_");
    match lower.as_str() {
        "font" | "font_name" => "fontname".to_string(),
        "size" | "font_size" => "fontsize".to_string(),
        "primarycolor" => "primary_color".to_string(),
        "secondarycolor" => "secondary_color".to_string(),
        "outlinecolor" => "outline_color".to_string(),
        "backcolor" => "back_color".to_string(),
        "color" | "colour" | "primary" => "primary_color".to_string(),
        "secondary" => "secondary_color".to_string(),
        "outline_colour" => "outline_color".to_string(),
        "back_colour" => "back_color".to_string(),
        "strikeout" => "strike_out".to_string(),
        "border" => "border_style".to_string(),
        "align" => "alignment".to_string(),
        "marginl" => "margin_l".to_string(),
        "marginr" => "margin_r".to_string(),
        "marginv" => "margin_v".to_string(),
        "scalex" => "scale_x".to_string(),
        "scaley" => "scale_y".to_string(),
        _ => lower,
    }
}

/// Set a style attribute by name. Returns true if the attribute was found and set.
fn set_style_attr(
    style: &mut crate::subtitles::data::SubtitleStyle,
    attr_name: &str,
    value: &serde_json::Value,
) -> bool {
    let converted = convert_patch_value(attr_name, value);
    match attr_name {
        "fontname" => {
            if let Some(v) = converted.as_str() {
                style.fontname = v.to_string();
                return true;
            }
        }
        "fontsize" => {
            if let Some(v) = converted.as_f64() {
                style.fontsize = v;
                return true;
            }
        }
        "primary_color" => {
            if let Some(v) = converted.as_str() {
                style.primary_color = v.to_string();
                return true;
            }
        }
        "secondary_color" => {
            if let Some(v) = converted.as_str() {
                style.secondary_color = v.to_string();
                return true;
            }
        }
        "outline_color" => {
            if let Some(v) = converted.as_str() {
                style.outline_color = v.to_string();
                return true;
            }
        }
        "back_color" => {
            if let Some(v) = converted.as_str() {
                style.back_color = v.to_string();
                return true;
            }
        }
        "bold" => {
            if let Some(v) = converted.as_i64() {
                style.bold = v as i32;
                return true;
            }
        }
        "italic" => {
            if let Some(v) = converted.as_i64() {
                style.italic = v as i32;
                return true;
            }
        }
        "underline" => {
            if let Some(v) = converted.as_i64() {
                style.underline = v as i32;
                return true;
            }
        }
        "strike_out" => {
            if let Some(v) = converted.as_i64() {
                style.strike_out = v as i32;
                return true;
            }
        }
        "scale_x" => {
            if let Some(v) = converted.as_f64() {
                style.scale_x = v;
                return true;
            }
        }
        "scale_y" => {
            if let Some(v) = converted.as_f64() {
                style.scale_y = v;
                return true;
            }
        }
        "spacing" => {
            if let Some(v) = converted.as_f64() {
                style.spacing = v;
                return true;
            }
        }
        "angle" => {
            if let Some(v) = converted.as_f64() {
                style.angle = v;
                return true;
            }
        }
        "border_style" => {
            if let Some(v) = converted.as_i64() {
                style.border_style = v as i32;
                return true;
            }
        }
        "outline" => {
            if let Some(v) = converted.as_f64() {
                style.outline = v;
                return true;
            }
        }
        "shadow" => {
            if let Some(v) = converted.as_f64() {
                style.shadow = v;
                return true;
            }
        }
        "alignment" => {
            if let Some(v) = converted.as_i64() {
                style.alignment = v as i32;
                return true;
            }
        }
        "margin_l" => {
            if let Some(v) = converted.as_i64() {
                style.margin_l = v as i32;
                return true;
            }
        }
        "margin_r" => {
            if let Some(v) = converted.as_i64() {
                style.margin_r = v as i32;
                return true;
            }
        }
        "margin_v" => {
            if let Some(v) = converted.as_i64() {
                style.margin_v = v as i32;
                return true;
            }
        }
        "encoding" => {
            if let Some(v) = converted.as_i64() {
                style.encoding = v as i32;
                return true;
            }
        }
        _ => {}
    }
    false
}

// =============================================================================
// Public Operations
// =============================================================================

/// Apply attribute patches to styles.
pub fn apply_style_patch(
    data: &mut SubtitleData,
    patches: &HashMap<String, HashMap<String, serde_json::Value>>,
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    if patches.is_empty() {
        let mut result = OperationResult::ok("style_patch");
        result.summary = "No patches provided".to_string();
        return result;
    }

    let mut styles_affected = 0;
    let mut changes: Vec<String> = Vec::new();

    for (style_name, attributes) in patches {
        if data.get_style(style_name).is_none() {
            log_msg(&format!(
                "[StylePatch] WARNING: Style '{style_name}' not found"
            ));
            continue;
        }

        for (attr, value) in attributes {
            let attr_name = map_style_attribute(attr);
            if let Some(style) = data.get_style_mut(style_name) {
                if set_style_attr(style, &attr_name, value) {
                    changes.push(format!("{style_name}.{attr_name}: -> {value}"));
                } else {
                    log_msg(&format!(
                        "[StylePatch] WARNING: Unknown attribute '{attr}' for style '{style_name}'"
                    ));
                }
            }
        }

        styles_affected += 1;
    }

    // Record operation
    let record = OperationRecord {
        operation: "style_patch".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!({
            "patches": patches.keys().collect::<Vec<_>>(),
        }),
        events_affected: 0,
        styles_affected,
        summary: format!("Patched {styles_affected} style(s)"),
    };
    data.operations.push(record.clone());

    log_msg(&format!(
        "[StylePatch] Applied {} change(s) to {styles_affected} style(s)",
        changes.len()
    ));

    let mut result = OperationResult::ok("style_patch");
    result.styles_affected = styles_affected;
    result.summary = record.summary;
    result.details.insert(
        "changes".to_string(),
        serde_json::json!(changes),
    );
    result
}

/// Replace font names in styles.
pub fn apply_font_replacement(
    data: &mut SubtitleData,
    replacements: &HashMap<String, String>,
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    if replacements.is_empty() {
        let mut result = OperationResult::ok("font_replacement");
        result.summary = "No replacements provided".to_string();
        return result;
    }

    let mut styles_affected = 0;
    let mut changes: Vec<String> = Vec::new();

    for (_, style) in &mut data.styles {
        let old_font = style.fontname.clone();
        if let Some(new_font) = replacements.get(&old_font) {
            style.fontname = new_font.clone();
            changes.push(format!("{}: {} -> {}", style.name, old_font, new_font));
            styles_affected += 1;
        }
    }

    // Record operation
    let record = OperationRecord {
        operation: "font_replacement".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!(replacements),
        events_affected: 0,
        styles_affected,
        summary: format!("Replaced fonts in {styles_affected} style(s)"),
    };
    data.operations.push(record.clone());

    log_msg(&format!(
        "[FontReplacement] Replaced {styles_affected} font(s)"
    ));

    let mut result = OperationResult::ok("font_replacement");
    result.styles_affected = styles_affected;
    result.summary = record.summary;
    result.details.insert(
        "changes".to_string(),
        serde_json::json!(changes),
    );
    result
}

/// Apply font size multiplier to all styles.
pub fn apply_size_multiplier(
    data: &mut SubtitleData,
    multiplier: f64,
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    // Skip if multiplier is effectively 1.0
    if (multiplier - 1.0).abs() < 1e-6 {
        let mut result = OperationResult::ok("size_multiplier");
        result.summary = "Multiplier is 1.0, no changes".to_string();
        return result;
    }

    // Validate range
    if !(0.1..=10.0).contains(&multiplier) {
        return OperationResult::err(
            "size_multiplier",
            &format!("Multiplier {multiplier} out of range (0.1-10.0)"),
        );
    }

    let mut styles_affected = 0;
    let mut changes: Vec<String> = Vec::new();

    for (_, style) in &mut data.styles {
        let old_size = style.fontsize;
        let new_size = old_size * multiplier;
        style.fontsize = new_size;
        changes.push(format!("{}: {} -> {:.1}", style.name, old_size, new_size));
        styles_affected += 1;
    }

    // Record operation
    let record = OperationRecord {
        operation: "size_multiplier".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!({"multiplier": multiplier}),
        events_affected: 0,
        styles_affected,
        summary: format!("Scaled {styles_affected} style(s) by {multiplier}x"),
    };
    data.operations.push(record.clone());

    log_msg(&format!(
        "[SizeMultiplier] Scaled {styles_affected} style(s) by {multiplier}x"
    ));

    let mut result = OperationResult::ok("size_multiplier");
    result.styles_affected = styles_affected;
    result.summary = record.summary;
    result.details.insert(
        "changes".to_string(),
        serde_json::json!(changes),
    );
    result
}

/// Rescale subtitle to target resolution using Aegisub "Add Borders" style.
///
/// Uses uniform scaling (min of scale_x, scale_y) to maintain aspect ratio.
/// Position tags get border offsets for centering. Font sizes use vertical
/// scaling (scale_h) to match Aegisub behavior.
pub fn apply_rescale(
    data: &mut SubtitleData,
    target_resolution: (i32, i32),
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    let (target_x, target_y) = target_resolution;

    // Get current resolution from script_info
    let current_x: i32 = data
        .script_info
        .iter()
        .find(|(k, _)| k == "PlayResX")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let current_y: i32 = data
        .script_info
        .iter()
        .find(|(k, _)| k == "PlayResY")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);

    // If no resolution set, just set it
    if current_x == 0 || current_y == 0 {
        // Set or update PlayResX/PlayResY
        set_script_info(data, "PlayResX", &target_x.to_string());
        set_script_info(data, "PlayResY", &target_y.to_string());

        let record = OperationRecord {
            operation: "rescale".to_string(),
            timestamp: Local::now().to_rfc3339(),
            parameters: serde_json::json!({"target": format!("{target_x}x{target_y}")}),
            events_affected: 0,
            styles_affected: 0,
            summary: format!("Set resolution to {target_x}x{target_y}"),
        };
        data.operations.push(record.clone());
        log_msg(&format!("[Rescale] Set resolution to {target_x}x{target_y}"));

        let mut result = OperationResult::ok("rescale");
        result.summary = record.summary;
        return result;
    }

    // If already at target, skip
    if current_x == target_x && current_y == target_y {
        log_msg(&format!(
            "[Rescale] Already at target resolution {target_x}x{target_y}"
        ));
        let mut result = OperationResult::ok("rescale");
        result.summary = "Already at target resolution".to_string();
        return result;
    }

    // Calculate scale factors (Aegisub "Add Borders" style)
    let scale_x = target_x as f64 / current_x as f64;
    let scale_y = target_y as f64 / current_y as f64;
    let scale = scale_x.min(scale_y); // Uniform scale to maintain aspect ratio
    let scale_h = scale_y; // Vertical scaling for font sizes (Aegisub convention)

    // Calculate effective size and border offsets for position tags
    let new_w = (current_x as f64 * scale + 0.5) as i32;
    let new_h = (current_y as f64 * scale + 0.5) as i32;
    let offset_x = (target_x - new_w) as f64 / 2.0;
    let offset_y = (target_y - new_h) as f64 / 2.0;

    log_msg(&format!(
        "[Rescale] Rescaling from {current_x}x{current_y} to {target_x}x{target_y} \
         (uniform scale: {scale:.4}, font scale: {scale_h:.4}, \
         borders: {offset_x:.1}x, {offset_y:.1}y)"
    ));

    // Scale styles (margins are edge-relative, so no offsets needed)
    let mut styles_affected = 0;
    for (_, style) in &mut data.styles {
        // Use vertical scaling for font size (Aegisub convention)
        style.fontsize = (style.fontsize * scale_h + 0.5) as i64 as f64;

        // Outline and shadow scale with vertical factor
        style.outline *= scale_h;
        style.shadow *= scale_h;

        // Margins are edge-relative, scale uniformly without offsets
        style.margin_l = (style.margin_l as f64 * scale + 0.5) as i32;
        style.margin_r = (style.margin_r as f64 * scale + 0.5) as i32;
        style.margin_v = (style.margin_v as f64 * scale + 0.5) as i32;

        styles_affected += 1;
    }

    // Scale event margins and inline override tags
    let mut events_affected = 0;
    for event in &mut data.events {
        // Scale event margins (edge-relative, no offsets)
        if event.margin_l != 0 {
            event.margin_l = (event.margin_l as f64 * scale + 0.5) as i32;
        }
        if event.margin_r != 0 {
            event.margin_r = (event.margin_r as f64 * scale + 0.5) as i32;
        }
        if event.margin_v != 0 {
            event.margin_v = (event.margin_v as f64 * scale + 0.5) as i32;
        }

        // Scale inline override tags (position tags get offsets)
        if event.text.contains('{') {
            event.text = scale_override_tags(&event.text, scale, scale_h, offset_x, offset_y);
            events_affected += 1;
        }
    }

    // Update script info
    set_script_info(data, "PlayResX", &target_x.to_string());
    set_script_info(data, "PlayResY", &target_y.to_string());

    // Record operation
    let record = OperationRecord {
        operation: "rescale".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!({
            "from": format!("{current_x}x{current_y}"),
            "to": format!("{target_x}x{target_y}"),
            "scale": scale,
            "scale_h": scale_h,
            "offset_x": offset_x,
            "offset_y": offset_y,
        }),
        events_affected,
        styles_affected,
        summary: format!("Rescaled from {current_x}x{current_y} to {target_x}x{target_y}"),
    };
    data.operations.push(record.clone());

    log_msg(&format!(
        "[Rescale] Successfully rescaled to {target_x}x{target_y}"
    ));

    let mut result = OperationResult::ok("rescale");
    result.styles_affected = styles_affected;
    result.events_affected = events_affected;
    result.summary = record.summary;
    result.details.insert("scale".to_string(), serde_json::json!(scale));
    result.details.insert("scale_h".to_string(), serde_json::json!(scale_h));
    result.details.insert("offset_x".to_string(), serde_json::json!(offset_x));
    result.details.insert("offset_y".to_string(), serde_json::json!(offset_y));
    result
}

/// Filter events by style name.
pub fn apply_style_filter(
    data: &mut SubtitleData,
    styles: &[String],
    mode: &str,
    forced_include: Option<&[usize]>,
    forced_exclude: Option<&[usize]>,
    log: Option<&dyn Fn(&str)>,
) -> OperationResult {
    let log_msg = |msg: &str| {
        if let Some(log_fn) = log {
            log_fn(msg);
        }
    };

    let forced_include_set: HashSet<usize> =
        forced_include.unwrap_or(&[]).iter().copied().collect();
    let forced_exclude_set: HashSet<usize> =
        forced_exclude.unwrap_or(&[]).iter().copied().collect();

    if styles.is_empty() && forced_include_set.is_empty() && forced_exclude_set.is_empty() {
        let mut result = OperationResult::ok("style_filter");
        result.summary = "No styles specified for filtering".to_string();
        return result;
    }

    let original_count = data.events.len();
    let styles_set: HashSet<&str> = styles.iter().map(|s| s.as_str()).collect();

    // Track which styles were found
    let mut found_styles: HashSet<String> = HashSet::new();
    for event in &data.events {
        if styles_set.contains(event.style.as_str()) {
            found_styles.insert(event.style.clone());
        }
    }

    // Filter events
    let original_events: Vec<_> = data.events.drain(..).collect();

    let mode_desc = if mode == "include" {
        // Keep only events with styles in the list or forced includes.
        for (idx, event) in original_events.into_iter().enumerate() {
            if forced_exclude_set.contains(&idx) {
                continue;
            }
            if forced_include_set.contains(&idx) || styles_set.contains(event.style.as_str()) {
                data.events.push(event);
            }
        }
        "included"
    } else {
        // mode == "exclude"
        // Remove events with styles in the list unless forced to include.
        for (idx, event) in original_events.into_iter().enumerate() {
            if forced_exclude_set.contains(&idx) {
                continue;
            }
            if forced_include_set.contains(&idx) || !styles_set.contains(event.style.as_str()) {
                data.events.push(event);
            }
        }
        "excluded"
    };

    let filtered_count = data.events.len();
    let removed_count = original_count - filtered_count;

    // Check for missing styles
    let missing_styles: HashSet<String> = styles_set
        .iter()
        .filter(|s| !found_styles.contains(**s))
        .map(|s| s.to_string())
        .collect();

    log_msg(&format!(
        "[StyleFilter] {} {} style(s), removed {removed_count}/{original_count} events",
        capitalize(mode_desc),
        found_styles.len()
    ));

    if !missing_styles.is_empty() {
        let missing: Vec<_> = missing_styles.iter().cloned().collect();
        let mut sorted = missing;
        sorted.sort();
        log_msg(&format!(
            "[StyleFilter] WARNING: Styles not found in file: {}",
            sorted.join(", ")
        ));
    }

    // Record operation
    let mut found_sorted: Vec<_> = found_styles.iter().cloned().collect();
    found_sorted.sort();
    let record = OperationRecord {
        operation: "style_filter".to_string(),
        timestamp: Local::now().to_rfc3339(),
        parameters: serde_json::json!({
            "styles": styles,
            "mode": mode,
            "forced_include": forced_include.unwrap_or(&[]),
            "forced_exclude": forced_exclude.unwrap_or(&[]),
        }),
        events_affected: removed_count as i32,
        styles_affected: 0,
        summary: format!(
            "{} styles: {}, removed {removed_count} events",
            capitalize(mode_desc),
            if found_sorted.is_empty() {
                "none".to_string()
            } else {
                found_sorted.join(", ")
            }
        ),
    };
    data.operations.push(record.clone());

    let mut result = OperationResult::ok("style_filter");
    result.events_affected = removed_count as i32;
    result.summary = record.summary;
    result.details.insert(
        "original_count".to_string(),
        serde_json::json!(original_count),
    );
    result.details.insert(
        "filtered_count".to_string(),
        serde_json::json!(filtered_count),
    );
    result.details.insert(
        "removed_count".to_string(),
        serde_json::json!(removed_count),
    );
    result.details.insert(
        "styles_found".to_string(),
        serde_json::json!(found_sorted),
    );
    let mut missing_sorted: Vec<_> = missing_styles.into_iter().collect();
    missing_sorted.sort();
    result.details.insert(
        "styles_missing".to_string(),
        serde_json::json!(missing_sorted),
    );
    result
}

// =============================================================================
// Helpers
// =============================================================================

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

fn set_script_info(data: &mut SubtitleData, key: &str, value: &str) {
    if let Some(entry) = data.script_info.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value.to_string();
    } else {
        data.script_info.push((key.to_string(), value.to_string()));
    }
}
