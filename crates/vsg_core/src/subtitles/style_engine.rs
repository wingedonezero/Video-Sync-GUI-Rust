//! Style engine using SubtitleData for subtitle manipulation — 1:1 port of `style_engine.py`.
//!
//! Provides style modification operations (font replacement, size scaling,
//! rescaling, color changes) using the unified SubtitleData system.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use sha2::{Digest, Sha256};

use super::data::{SubtitleData, SubtitleStyle};

// =============================================================================
// Color Conversion Helpers
// =============================================================================

/// Convert ASS color format to Qt hex format.
///
/// ASS format: &HAABBGGRR (alpha, blue, green, red)
/// Qt format:  #AARRGGBB (alpha, red, green, blue)
///
/// Note: ASS alpha is inverted (00 = opaque, FF = transparent)
///       Qt alpha is normal (FF = opaque, 00 = transparent)
pub fn ass_color_to_qt(ass_color: &str) -> String {
    // Remove &H prefix and ensure 8 characters
    let color = ass_color
        .trim_start_matches("&H")
        .trim_start_matches("&h")
        .to_uppercase();
    let color = format!("{:0>8}", color);

    // Parse AABBGGRR
    let ass_alpha = u8::from_str_radix(&color[0..2], 16).unwrap_or(0);
    let blue = &color[2..4];
    let green = &color[4..6];
    let red = &color[6..8];

    // Convert ASS alpha (inverted) to Qt alpha (normal)
    let qt_alpha = 255 - ass_alpha;

    format!("#{qt_alpha:02X}{red}{green}{blue}")
}

/// Convert Qt hex format to ASS color format.
///
/// Qt format:  #AARRGGBB (alpha, red, green, blue)
/// ASS format: &HAABBGGRR (alpha, blue, green, red)
///
/// Note: Qt alpha is normal (FF = opaque, 00 = transparent)
///       ASS alpha is inverted (00 = opaque, FF = transparent)
pub fn qt_color_to_ass(qt_color: &str) -> String {
    // Remove # prefix
    let color = qt_color.trim_start_matches('#').to_uppercase();
    let color = format!("{:0>8}", color);

    // Parse AARRGGBB
    let qt_alpha = u8::from_str_radix(&color[0..2], 16).unwrap_or(255);
    let red = &color[2..4];
    let green = &color[4..6];
    let blue = &color[6..8];

    // Convert Qt alpha to ASS alpha (inverted)
    let ass_alpha = 255 - qt_alpha;

    format!("&H{ass_alpha:02X}{blue}{green}{red}")
}

// =============================================================================
// Style Engine
// =============================================================================

/// Handles loading, parsing, manipulating, and saving subtitle styles
/// using the SubtitleData system.
pub struct StyleEngine {
    pub path: PathBuf,
    pub data: Option<SubtitleData>,
    temp_file: Option<PathBuf>,
    temp_dir: PathBuf,
}

impl StyleEngine {
    /// Initialize the style engine.
    ///
    /// `subtitle_path`: Path to the subtitle file
    /// `temp_dir`: Temp directory for preview files. If None, uses a sibling directory.
    pub fn new(subtitle_path: &Path, temp_dir: Option<&Path>) -> Result<Self, String> {
        let path = subtitle_path.to_path_buf();
        let temp_dir = if let Some(td) = temp_dir {
            td.to_path_buf()
        } else {
            // Fallback: use a sibling directory next to the subtitle file
            let td = path.parent().unwrap_or(Path::new(".")).join(".style_editor_temp");
            std::fs::create_dir_all(&td)
                .map_err(|e| format!("Failed to create temp dir: {e}"))?;
            td
        };

        let mut engine = Self {
            path,
            data: None,
            temp_file: None,
            temp_dir,
        };
        engine.load()?;
        Ok(engine)
    }

    /// Loads the subtitle file into SubtitleData.
    pub fn load(&mut self) -> Result<(), String> {
        self.data = Some(SubtitleData::from_file(&self.path)?);
        Ok(())
    }

    /// Saves changes to a temp file for preview, not the original.
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(ref data) = self.data {
            if self.temp_file.is_none() {
                let source_stem = self.path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "preview".to_string());
                let unique_id = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    % 1_000_000;
                self.temp_file = Some(
                    self.temp_dir
                        .join(format!("preview_{source_stem}_{unique_id}.ass")),
                );
            }
            if let Some(ref temp_path) = self.temp_file {
                data.save_ass(temp_path, "floor")?;
            }
        }
        Ok(())
    }

    /// Saves changes back to the original file.
    pub fn save_to_original(&self) -> Result<(), String> {
        if let Some(ref data) = self.data {
            data.save(&self.path, None)?;
        }
        Ok(())
    }

    /// Get path to temp file for preview. Creates/updates it if needed.
    pub fn get_preview_path(&mut self) -> Result<String, String> {
        self.save()?;
        Ok(self.temp_file
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string()))
    }

    /// Clean up resources - remove temp file and release data.
    pub fn cleanup(&mut self) {
        if let Some(ref temp_path) = self.temp_file {
            let _ = std::fs::remove_file(temp_path);
            self.temp_file = None;
        }
        self.data = None;
    }

    /// Returns a list of all style names defined in the file.
    pub fn get_style_names(&self) -> Vec<String> {
        if let Some(ref data) = self.data {
            data.styles.iter().map(|(n, _)| n.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Returns a dictionary of attributes for a given style.
    pub fn get_style_attributes(&self, style_name: &str) -> HashMap<String, serde_json::Value> {
        let mut result = HashMap::new();
        let data = match &self.data {
            Some(d) => d,
            None => return result,
        };
        let style = match data.get_style(style_name) {
            Some(s) => s,
            None => return result,
        };

        result.insert("fontname".to_string(), serde_json::json!(style.fontname));
        result.insert("fontsize".to_string(), serde_json::json!(style.fontsize));
        result.insert("primarycolor".to_string(), serde_json::json!(ass_color_to_qt(&style.primary_color)));
        result.insert("secondarycolor".to_string(), serde_json::json!(ass_color_to_qt(&style.secondary_color)));
        result.insert("outlinecolor".to_string(), serde_json::json!(ass_color_to_qt(&style.outline_color)));
        result.insert("backcolor".to_string(), serde_json::json!(ass_color_to_qt(&style.back_color)));
        result.insert("bold".to_string(), serde_json::json!(style.bold != 0));
        result.insert("italic".to_string(), serde_json::json!(style.italic != 0));
        result.insert("underline".to_string(), serde_json::json!(style.underline != 0));
        result.insert("strikeout".to_string(), serde_json::json!(style.strike_out != 0));
        result.insert("outline".to_string(), serde_json::json!(style.outline));
        result.insert("shadow".to_string(), serde_json::json!(style.shadow));
        result.insert("marginl".to_string(), serde_json::json!(style.margin_l));
        result.insert("marginr".to_string(), serde_json::json!(style.margin_r));
        result.insert("marginv".to_string(), serde_json::json!(style.margin_v));
        result
    }

    /// Updates attributes for a given style.
    pub fn update_style_attributes(
        &mut self,
        style_name: &str,
        attributes: &HashMap<String, serde_json::Value>,
    ) {
        let data = match &mut self.data {
            Some(d) => d,
            None => return,
        };
        let style = match data.get_style_mut(style_name) {
            Some(s) => s,
            None => return,
        };

        for (key, value) in attributes {
            match key.as_str() {
                "alignment" => {
                    // Explicitly ignore alignment if it ever slips through
                    continue;
                }
                "fontname" => {
                    if let Some(v) = value.as_str() {
                        style.fontname = v.to_string();
                    }
                }
                "fontsize" => {
                    if let Some(v) = value.as_f64() {
                        style.fontsize = v;
                    }
                }
                "primarycolor" => {
                    if let Some(v) = value.as_str() {
                        style.primary_color = qt_color_to_ass(v);
                    }
                }
                "secondarycolor" => {
                    if let Some(v) = value.as_str() {
                        style.secondary_color = qt_color_to_ass(v);
                    }
                }
                "outlinecolor" => {
                    if let Some(v) = value.as_str() {
                        style.outline_color = qt_color_to_ass(v);
                    }
                }
                "backcolor" => {
                    if let Some(v) = value.as_str() {
                        style.back_color = qt_color_to_ass(v);
                    }
                }
                "bold" => {
                    if let Some(v) = value.as_bool() {
                        style.bold = if v { -1 } else { 0 };
                    }
                }
                "italic" => {
                    if let Some(v) = value.as_bool() {
                        style.italic = if v { -1 } else { 0 };
                    }
                }
                "underline" => {
                    if let Some(v) = value.as_bool() {
                        style.underline = if v { -1 } else { 0 };
                    }
                }
                "strikeout" => {
                    if let Some(v) = value.as_bool() {
                        style.strike_out = if v { -1 } else { 0 };
                    }
                }
                "outline" => {
                    if let Some(v) = value.as_f64() {
                        style.outline = v;
                    }
                }
                "shadow" => {
                    if let Some(v) = value.as_f64() {
                        style.shadow = v;
                    }
                }
                "marginl" => {
                    if let Some(v) = value.as_i64() {
                        style.margin_l = v as i32;
                    }
                }
                "marginr" => {
                    if let Some(v) = value.as_i64() {
                        style.margin_r = v as i32;
                    }
                }
                "marginv" => {
                    if let Some(v) = value.as_i64() {
                        style.margin_v = v as i32;
                    }
                }
                _ => {}
            }
        }
    }

    /// Returns all subtitle events.
    pub fn get_events(&self) -> Vec<HashMap<String, serde_json::Value>> {
        let data = match &self.data {
            Some(d) => d,
            None => return Vec::new(),
        };

        let tag_pattern = Regex::new(r"\{[^}]+\}").unwrap();

        data.events
            .iter()
            .enumerate()
            .filter(|(_, event)| !event.is_comment)
            .map(|(i, event)| {
                let mut map = HashMap::new();
                map.insert("line_num".to_string(), serde_json::json!(i + 1));
                map.insert("start".to_string(), serde_json::json!(event.start_ms as i64));
                map.insert("end".to_string(), serde_json::json!(event.end_ms as i64));
                map.insert("style".to_string(), serde_json::json!(event.style));
                map.insert("text".to_string(), serde_json::json!(event.text));
                map.insert(
                    "plaintext".to_string(),
                    serde_json::json!(tag_pattern.replace_all(&event.text, "").to_string()),
                );
                map
            })
            .collect()
    }

    /// Extracts the raw [V4+ Styles] block as a list of strings.
    pub fn get_raw_style_block(&self) -> Option<Vec<String>> {
        let content = std::fs::read_to_string(&self.path).ok()?;
        let mut in_styles_block = false;
        let mut style_lines = Vec::new();

        for line in content.lines() {
            let line_strip = line.trim();
            let lower = line_strip.to_lowercase();
            if lower == "[v4+ styles]" || lower == "[v4 styles]" {
                in_styles_block = true;
            } else if in_styles_block
                && (line_strip.starts_with("Format:") || line_strip.starts_with("Style:"))
            {
                style_lines.push(line.to_string());
            } else if in_styles_block && line_strip.starts_with('[') {
                break;
            }
        }

        if style_lines.is_empty() {
            None
        } else {
            Some(style_lines)
        }
    }

    /// Overwrites the [V4+ Styles] block with the provided lines.
    pub fn set_raw_style_block(&mut self, style_lines: &[String]) {
        let data = match &mut self.data {
            Some(d) => d,
            None => return,
        };
        if style_lines.is_empty() {
            return;
        }

        let mut format_line: Option<Vec<String>> = None;
        let mut styles = Vec::new();

        for line in style_lines {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("Format:") {
                format_line = Some(
                    rest
                        .split(',')
                        .map(|f| f.trim().to_string())
                        .collect(),
                );
            } else if let Some(rest) = line.strip_prefix("Style:") {
                let values: Vec<String> = rest.split(',').map(|s| s.to_string()).collect();
                if let Some(ref fmt) = format_line {
                    let style = SubtitleStyle::from_format_line(fmt, &values);
                    styles.push(style);
                }
            }
        }

        // Update data
        if let Some(fmt) = format_line {
            data.styles_format = fmt;
        }
        data.styles.clear();
        for style in styles {
            let name = style.name.clone();
            data.styles.push((name, style));
        }

        let _ = self.save();
    }

    /// Reset a single style to its original state by reloading from disk.
    pub fn reset_style(&mut self, style_name: &str) {
        if self.data.is_none() {
            return;
        }
        if let Ok(original_data) = SubtitleData::from_file(&self.path) {
            if let Some(original_style) = original_data.get_style(style_name) {
                if let Some(ref mut data) = self.data {
                    data.set_style(style_name, original_style.clone());
                }
            }
        }
    }

    /// Reset all styles to original state by reloading from disk.
    pub fn reset_all_styles(&mut self) {
        if self.data.is_none() {
            return;
        }
        if let Ok(original_data) = SubtitleData::from_file(&self.path) {
            if let Some(ref mut data) = self.data {
                data.styles = original_data.styles;
            }
        }
    }

    // =========================================================================
    // Script Info Access (for resample dialog)
    // =========================================================================

    /// Access to script info for compatibility.
    pub fn info(&self) -> &[(String, String)] {
        match &self.data {
            Some(d) => &d.script_info,
            None => &[],
        }
    }

    /// Get script info value by key.
    pub fn get_info(&self, key: &str) -> Option<&str> {
        if let Some(ref data) = self.data {
            data.script_info
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        } else {
            None
        }
    }

    /// Set script info value.
    pub fn set_info(&mut self, key: &str, value: &str) {
        if let Some(ref mut data) = self.data {
            if let Some(entry) = data.script_info.iter_mut().find(|(k, _)| k == key) {
                entry.1 = value.to_string();
            } else {
                data.script_info.push((key.to_string(), value.to_string()));
            }
        }
    }

    // =========================================================================
    // Static Methods
    // =========================================================================

    /// Merges styles from a template file into a target file.
    /// Only styles with matching names are updated; unique styles in the target are preserved.
    pub fn merge_styles_from_template(
        target_path: &str,
        template_path: &str,
    ) -> Result<bool, String> {
        let mut target_data = SubtitleData::from_file(Path::new(target_path))?;
        let template_data = SubtitleData::from_file(Path::new(template_path))?;

        let mut updated_count = 0;
        // Collect names to update first to avoid borrow conflict
        let names_to_update: Vec<String> = target_data
            .styles
            .iter()
            .filter(|(name, _)| template_data.get_style(name).is_some())
            .map(|(name, _)| name.clone())
            .collect();

        for name in &names_to_update {
            if let Some(template_style) = template_data.get_style(name) {
                let cloned = template_style.clone();
                if let Some(target_style) = target_data.get_style_mut(name) {
                    *target_style = cloned;
                    updated_count += 1;
                }
            }
        }

        if updated_count > 0 {
            target_data.save(Path::new(target_path), None)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Generates a unique hash of the [V4+ Styles] block for content matching.
    pub fn get_content_signature(subtitle_path: &str) -> Option<String> {
        let content = std::fs::read_to_string(subtitle_path).ok()?;
        let mut in_styles_block = false;
        let mut style_lines = Vec::new();

        for line in content.lines() {
            let line_strip = line.trim();
            let lower = line_strip.to_lowercase();
            if lower == "[v4+ styles]" || lower == "[v4 styles]" {
                in_styles_block = true;
            } else if in_styles_block && line_strip.starts_with("Style:") {
                style_lines.push(line_strip.to_string());
            } else if in_styles_block && line_strip.is_empty() {
                break;
            }
        }

        if style_lines.is_empty() {
            return None;
        }

        style_lines.sort();
        let joined = style_lines.join("\n");
        let mut hasher = Sha256::new();
        hasher.update(joined.as_bytes());
        Some(format!("{:x}", hasher.finalize()))
    }

    /// Generates a fallback signature from the track name (e.g., 'Signs [LostYears]').
    pub fn get_name_signature(track_name: &str) -> Option<String> {
        if track_name.is_empty() {
            return None;
        }
        let sanitized: String = track_name
            .chars()
            .filter(|c| !r#"\/*?:"<>|"#.contains(*c))
            .collect();
        let sanitized = sanitized.trim().to_string();
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    }
}

impl Drop for StyleEngine {
    fn drop(&mut self) {
        self.cleanup();
    }
}
