//! Unified subtitle data container — 1:1 port of `vsg_core/subtitles/data.py`.
//!
//! Central data structure for subtitle processing:
//! - Load once from file (ASS, SRT) or OCR output
//! - Apply operations (stepping, sync, style patches, etc.)
//! - Write once at the end (single rounding point)
//!
//! All timing is stored as FLOAT MILLISECONDS internally.
//! Rounding happens ONLY at final save (ASS → centiseconds, SRT → milliseconds).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};

// =============================================================================
// Per-Event Metadata (OCR, Sync, Stepping)
// =============================================================================

/// OCR-specific metadata for a single subtitle event — `OCREventData`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCREventData {
    pub index: i32,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub raw_text: String,
    #[serde(default)]
    pub fixes_applied: HashMap<String, i32>,
    #[serde(default)]
    pub unknown_words: Vec<String>,

    // Position data from source image
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub frame_width: i32,
    #[serde(default)]
    pub frame_height: i32,

    // VobSub specific
    #[serde(default)]
    pub is_forced: bool,
    #[serde(default)]
    pub subtitle_colors: Vec<Vec<i32>>,
    #[serde(default)]
    pub dominant_color: Vec<i32>,
}

impl OCREventData {
    pub fn new(index: i32) -> Self {
        Self {
            index,
            image: String::new(),
            confidence: 0.0,
            raw_text: String::new(),
            fixes_applied: HashMap::new(),
            unknown_words: Vec::new(),
            x: 0, y: 0, width: 0, height: 0,
            frame_width: 0, frame_height: 0,
            is_forced: false,
            subtitle_colors: Vec::new(),
            dominant_color: Vec::new(),
        }
    }
}

/// Sync-specific metadata for a single subtitle event — `SyncEventData`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncEventData {
    #[serde(default)]
    pub original_start_ms: f64,
    #[serde(default)]
    pub original_end_ms: f64,
    #[serde(default)]
    pub start_adjustment_ms: f64,
    #[serde(default)]
    pub end_adjustment_ms: f64,
    #[serde(default)]
    pub snapped_to_frame: bool,
    #[serde(default)]
    pub target_frame_start: Option<i32>,
    #[serde(default)]
    pub target_frame_end: Option<i32>,
}

/// Stepping-specific metadata for a single subtitle event — `SteppingEventData`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SteppingEventData {
    #[serde(default)]
    pub original_start_ms: f64,
    #[serde(default)]
    pub original_end_ms: f64,
    #[serde(default)]
    pub segment_index: Option<i32>,
    #[serde(default)]
    pub adjustment_ms: f64,
}

// =============================================================================
// Document-Level OCR Metadata
// =============================================================================

/// Document-level OCR metadata and statistics — `OCRMetadata`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OCRMetadata {
    #[serde(default = "default_tesseract")]
    pub engine: String,
    #[serde(default = "default_eng")]
    pub language: String,
    #[serde(default = "default_vobsub")]
    pub source_format: String,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub source_resolution: Vec<i32>,
    #[serde(default)]
    pub master_palette: Vec<Vec<i32>>,

    // Statistics
    #[serde(default)]
    pub total_subtitles: i32,
    #[serde(default)]
    pub successful: i32,
    #[serde(default)]
    pub failed: i32,
    #[serde(default)]
    pub average_confidence: f64,
    #[serde(default)]
    pub min_confidence: f64,
    #[serde(default)]
    pub max_confidence: f64,
    #[serde(default)]
    pub total_fixes_applied: i32,
    #[serde(default)]
    pub positioned_subtitles: i32,

    #[serde(default)]
    pub fixes_by_type: HashMap<String, i32>,
    #[serde(default)]
    pub unknown_words: Vec<serde_json::Value>,
}

fn default_tesseract() -> String { "tesseract".to_string() }
fn default_eng() -> String { "eng".to_string() }
fn default_vobsub() -> String { "vobsub".to_string() }

impl Default for OCRMetadata {
    fn default() -> Self {
        Self {
            engine: default_tesseract(),
            language: default_eng(),
            source_format: default_vobsub(),
            source_file: String::new(),
            source_resolution: vec![0, 0],
            master_palette: Vec::new(),
            total_subtitles: 0, successful: 0, failed: 0,
            average_confidence: 0.0, min_confidence: 0.0, max_confidence: 0.0,
            total_fixes_applied: 0, positioned_subtitles: 0,
            fixes_by_type: HashMap::new(),
            unknown_words: Vec::new(),
        }
    }
}

// =============================================================================
// Style Definition
// =============================================================================

/// ASS/SSA style definition with all 23 fields — `SubtitleStyle`
///
/// Field order matches ASS V4+ Styles format line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStyle {
    pub name: String,
    #[serde(default = "default_arial")]
    pub fontname: String,
    #[serde(default = "default_fontsize")]
    pub fontsize: f64,
    #[serde(default = "default_primary_color")]
    pub primary_color: String,
    #[serde(default = "default_secondary_color")]
    pub secondary_color: String,
    #[serde(default = "default_black_color")]
    pub outline_color: String,
    #[serde(default = "default_black_color")]
    pub back_color: String,
    #[serde(default)]
    pub bold: i32,
    #[serde(default)]
    pub italic: i32,
    #[serde(default)]
    pub underline: i32,
    #[serde(default)]
    pub strike_out: i32,
    #[serde(default = "default_100")]
    pub scale_x: f64,
    #[serde(default = "default_100")]
    pub scale_y: f64,
    #[serde(default)]
    pub spacing: f64,
    #[serde(default)]
    pub angle: f64,
    #[serde(default = "default_1")]
    pub border_style: i32,
    #[serde(default = "default_2f")]
    pub outline: f64,
    #[serde(default = "default_2f")]
    pub shadow: f64,
    #[serde(default = "default_2")]
    pub alignment: i32,
    #[serde(default = "default_10")]
    pub margin_l: i32,
    #[serde(default = "default_10")]
    pub margin_r: i32,
    #[serde(default = "default_10")]
    pub margin_v: i32,
    #[serde(default = "default_1")]
    pub encoding: i32,
}

fn default_arial() -> String { "Arial".to_string() }
fn default_fontsize() -> f64 { 48.0 }
fn default_primary_color() -> String { "&H00FFFFFF".to_string() }
fn default_secondary_color() -> String { "&H000000FF".to_string() }
fn default_black_color() -> String { "&H00000000".to_string() }
fn default_100() -> f64 { 100.0 }
fn default_1() -> i32 { 1 }
fn default_2() -> i32 { 2 }
fn default_2f() -> f64 { 2.0 }
fn default_10() -> i32 { 10 }

impl SubtitleStyle {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fontname: "Arial".to_string(),
            fontsize: 48.0,
            primary_color: "&H00FFFFFF".to_string(),
            secondary_color: "&H000000FF".to_string(),
            outline_color: "&H00000000".to_string(),
            back_color: "&H00000000".to_string(),
            bold: 0, italic: 0, underline: 0, strike_out: 0,
            scale_x: 100.0, scale_y: 100.0,
            spacing: 0.0, angle: 0.0,
            border_style: 1, outline: 2.0, shadow: 2.0,
            alignment: 2,
            margin_l: 10, margin_r: 10, margin_v: 10,
            encoding: 1,
        }
    }

    /// Parse style from Format fields and values — `from_format_line`
    pub fn from_format_line(format_fields: &[String], values: &[String]) -> Self {
        let mut field_map: HashMap<String, String> = HashMap::new();
        for (i, field_name) in format_fields.iter().enumerate() {
            if i < values.len() {
                field_map.insert(
                    field_name.trim().to_lowercase(),
                    values[i].trim().to_string(),
                );
            }
        }

        let get = |key: &str, alt: &str, default: &str| -> String {
            field_map.get(key)
                .or_else(|| field_map.get(alt))
                .map(|s| s.to_string())
                .unwrap_or_else(|| default.to_string())
        };

        let get_f = |key: &str, default: f64| -> f64 {
            field_map.get(key).and_then(|s| s.parse().ok()).unwrap_or(default)
        };

        let get_i = |key: &str, default: i32| -> i32 {
            field_map.get(key).and_then(|s| s.parse().ok()).unwrap_or(default)
        };

        Self {
            name: get("name", "name", "Default"),
            fontname: get("fontname", "fontname", "Arial"),
            fontsize: get_f("fontsize", 48.0),
            primary_color: get("primarycolour", "primarycolor", "&H00FFFFFF"),
            secondary_color: get("secondarycolour", "secondarycolor", "&H000000FF"),
            outline_color: get("outlinecolour", "outlinecolor", "&H00000000"),
            back_color: get("backcolour", "backcolor", "&H00000000"),
            bold: get_i("bold", 0),
            italic: get_i("italic", 0),
            underline: get_i("underline", 0),
            strike_out: get_i("strikeout", 0),
            scale_x: get_f("scalex", 100.0),
            scale_y: get_f("scaley", 100.0),
            spacing: get_f("spacing", 0.0),
            angle: get_f("angle", 0.0),
            border_style: get_i("borderstyle", 1),
            outline: get_f("outline", 2.0),
            shadow: get_f("shadow", 2.0),
            alignment: get_i("alignment", 2),
            margin_l: get_i("marginl", 10),
            margin_r: get_i("marginr", 10),
            margin_v: get_i("marginv", 10),
            encoding: get_i("encoding", 1),
        }
    }

    /// Convert to values list matching format fields — `to_format_values`
    pub fn to_format_values(&self, format_fields: &[String]) -> Vec<String> {
        let mut value_map: HashMap<String, String> = HashMap::new();
        value_map.insert("name".to_string(), self.name.clone());
        value_map.insert("fontname".to_string(), self.fontname.clone());
        value_map.insert("fontsize".to_string(), format_number(self.fontsize));
        value_map.insert("primarycolour".to_string(), self.primary_color.clone());
        value_map.insert("primarycolor".to_string(), self.primary_color.clone());
        value_map.insert("secondarycolour".to_string(), self.secondary_color.clone());
        value_map.insert("secondarycolor".to_string(), self.secondary_color.clone());
        value_map.insert("outlinecolour".to_string(), self.outline_color.clone());
        value_map.insert("outlinecolor".to_string(), self.outline_color.clone());
        value_map.insert("backcolour".to_string(), self.back_color.clone());
        value_map.insert("backcolor".to_string(), self.back_color.clone());
        value_map.insert("bold".to_string(), self.bold.to_string());
        value_map.insert("italic".to_string(), self.italic.to_string());
        value_map.insert("underline".to_string(), self.underline.to_string());
        value_map.insert("strikeout".to_string(), self.strike_out.to_string());
        value_map.insert("scalex".to_string(), format_number(self.scale_x));
        value_map.insert("scaley".to_string(), format_number(self.scale_y));
        value_map.insert("spacing".to_string(), format_number(self.spacing));
        value_map.insert("angle".to_string(), format_number(self.angle));
        value_map.insert("borderstyle".to_string(), self.border_style.to_string());
        value_map.insert("outline".to_string(), format_number(self.outline));
        value_map.insert("shadow".to_string(), format_number(self.shadow));
        value_map.insert("alignment".to_string(), self.alignment.to_string());
        value_map.insert("marginl".to_string(), self.margin_l.to_string());
        value_map.insert("marginr".to_string(), self.margin_r.to_string());
        value_map.insert("marginv".to_string(), self.margin_v.to_string());
        value_map.insert("encoding".to_string(), self.encoding.to_string());

        format_fields
            .iter()
            .map(|f| value_map.get(&f.trim().to_lowercase()).cloned().unwrap_or_default())
            .collect()
    }
}

// =============================================================================
// Event Definition
// =============================================================================

/// Single subtitle event with FLOAT MILLISECOND timing — `SubtitleEvent`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleEvent {
    /// Timing - FLOAT MS for precision
    pub start_ms: f64,
    pub end_ms: f64,

    /// Content
    pub text: String,
    #[serde(default = "default_style")]
    pub style: String,

    /// ASS fields
    #[serde(default)]
    pub layer: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub margin_l: i32,
    #[serde(default)]
    pub margin_r: i32,
    #[serde(default)]
    pub margin_v: i32,
    #[serde(default)]
    pub effect: String,

    /// Type
    #[serde(default)]
    pub is_comment: bool,

    /// Aegisub extradata reference
    #[serde(default)]
    pub extradata_ids: Vec<i32>,

    /// Source tracking
    #[serde(default)]
    pub original_index: Option<i32>,
    #[serde(default)]
    pub srt_index: Option<i32>,

    /// Optional per-event metadata
    #[serde(default)]
    pub ocr: Option<OCREventData>,
    #[serde(default)]
    pub sync: Option<SyncEventData>,
    #[serde(default)]
    pub stepping: Option<SteppingEventData>,
}

fn default_style() -> String { "Default".to_string() }

impl SubtitleEvent {
    pub fn new(start_ms: f64, end_ms: f64, text: &str) -> Self {
        Self {
            start_ms,
            end_ms,
            text: text.to_string(),
            style: "Default".to_string(),
            layer: 0,
            name: String::new(),
            margin_l: 0, margin_r: 0, margin_v: 0,
            effect: String::new(),
            is_comment: false,
            extradata_ids: Vec::new(),
            original_index: None,
            srt_index: None,
            ocr: None,
            sync: None,
            stepping: None,
        }
    }

    /// Get duration in milliseconds.
    pub fn duration_ms(&self) -> f64 {
        self.end_ms - self.start_ms
    }

    /// Parse event from Format fields and values — `from_format_line`
    pub fn from_format_line(
        format_fields: &[String],
        values: &[String],
        is_comment: bool,
    ) -> Self {
        let mut field_map: HashMap<String, String> = HashMap::new();
        let mut text_idx: Option<usize> = None;

        for (i, field_name) in format_fields.iter().enumerate() {
            let key = field_name.trim().to_lowercase();
            if key == "text" {
                text_idx = Some(i);
                break;
            }
            if i < values.len() {
                field_map.insert(key, values[i].trim().to_string());
            }
        }

        // Text is everything from text_idx onwards (may contain commas)
        let text = if let Some(idx) = text_idx {
            if idx < values.len() {
                values[idx..].join(",")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let start_ms = parse_ass_time(field_map.get("start").map(|s| s.as_str()).unwrap_or("0:00:00.00"));
        let end_ms = parse_ass_time(field_map.get("end").map(|s| s.as_str()).unwrap_or("0:00:00.00"));

        // Parse extradata IDs if present
        let mut extradata_ids = Vec::new();
        let ed_str = field_map.get("extradataid")
            .or_else(|| field_map.get("extradata"))
            .map(|s| s.as_str())
            .unwrap_or("");
        if !ed_str.is_empty() {
            // Format: {=N} or {=N,M,...}
            if let Some(start) = ed_str.find("{=") {
                if let Some(end) = ed_str[start..].find('}') {
                    let nums = &ed_str[start + 2..start + end];
                    for n in nums.split(',') {
                        if let Ok(id) = n.trim().parse::<i32>() {
                            extradata_ids.push(id);
                        }
                    }
                }
            }
        }

        let get_i = |key: &str, default: i32| -> i32 {
            field_map.get(key).and_then(|s| s.parse().ok()).unwrap_or(default)
        };

        Self {
            start_ms,
            end_ms,
            text,
            style: field_map.get("style").cloned().unwrap_or_else(|| "Default".to_string()),
            layer: get_i("layer", 0),
            name: field_map.get("name")
                .or_else(|| field_map.get("actor"))
                .cloned()
                .unwrap_or_default(),
            margin_l: get_i("marginl", 0),
            margin_r: get_i("marginr", 0),
            margin_v: get_i("marginv", 0),
            effect: field_map.get("effect").cloned().unwrap_or_default(),
            is_comment,
            extradata_ids,
            original_index: None,
            srt_index: None,
            ocr: None,
            sync: None,
            stepping: None,
        }
    }

    /// Convert to values list matching format fields — `to_format_values`
    pub fn to_format_values(&self, format_fields: &[String]) -> Vec<String> {
        let mut value_map: HashMap<String, String> = HashMap::new();
        value_map.insert("layer".to_string(), self.layer.to_string());
        value_map.insert("start".to_string(), "__START_MS__".to_string());
        value_map.insert("end".to_string(), "__END_MS__".to_string());
        value_map.insert("style".to_string(), self.style.clone());
        value_map.insert("name".to_string(), self.name.clone());
        value_map.insert("actor".to_string(), self.name.clone());
        value_map.insert("marginl".to_string(), self.margin_l.to_string());
        value_map.insert("marginr".to_string(), self.margin_r.to_string());
        value_map.insert("marginv".to_string(), self.margin_v.to_string());
        value_map.insert("effect".to_string(), self.effect.clone());
        value_map.insert("text".to_string(), self.text.clone());

        format_fields
            .iter()
            .map(|f| value_map.get(&f.trim().to_lowercase()).cloned().unwrap_or_default())
            .collect()
    }
}

// =============================================================================
// Embedded Content
// =============================================================================

/// Embedded font from [Fonts] section — `EmbeddedFont`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedFont {
    pub name: String,
    pub data: String,
}

/// Embedded graphic from [Graphics] section — `EmbeddedGraphic`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedGraphic {
    pub name: String,
    pub data: String,
}

// =============================================================================
// Operation Tracking
// =============================================================================

/// Record of an operation applied to subtitle data — `OperationRecord`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub events_affected: i32,
    #[serde(default)]
    pub styles_affected: i32,
    #[serde(default)]
    pub summary: String,
}

impl OperationRecord {
    pub fn new(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            timestamp: Local::now().to_rfc3339(),
            parameters: serde_json::json!({}),
            events_affected: 0,
            styles_affected: 0,
            summary: String::new(),
        }
    }
}

/// Result of applying an operation — `OperationResult`
#[derive(Debug, Clone)]
pub struct OperationResult {
    pub success: bool,
    pub operation: String,
    pub events_affected: i32,
    pub styles_affected: i32,
    pub summary: String,
    pub details: HashMap<String, serde_json::Value>,
    pub error: Option<String>,
}

impl OperationResult {
    pub fn ok(operation: &str) -> Self {
        Self {
            success: true,
            operation: operation.to_string(),
            events_affected: 0,
            styles_affected: 0,
            summary: String::new(),
            details: HashMap::new(),
            error: None,
        }
    }

    pub fn err(operation: &str, error: &str) -> Self {
        Self {
            success: false,
            operation: operation.to_string(),
            events_affected: 0,
            styles_affected: 0,
            summary: String::new(),
            details: HashMap::new(),
            error: Some(error.to_string()),
        }
    }
}

// =============================================================================
// Main SubtitleData Container
// =============================================================================

/// Universal subtitle data container — `SubtitleData`
///
/// THE SINGLE source of truth for subtitle processing.
/// All timing stored as FLOAT MILLISECONDS.
pub struct SubtitleData {
    // Source information
    pub source_path: Option<PathBuf>,
    pub source_format: String,
    pub encoding: String,
    pub has_bom: bool,

    // ASS Script Info (preserved in order)
    pub script_info: Vec<(String, String)>,

    // Aegisub sections
    pub aegisub_garbage: Vec<(String, String)>,
    pub aegisub_extradata: Vec<String>,

    // Custom/unknown sections (preserved as raw lines)
    pub custom_sections: Vec<(String, Vec<String>)>,

    // Section ordering
    pub section_order: Vec<String>,

    // Styles
    pub styles: Vec<(String, SubtitleStyle)>,
    pub styles_format: Vec<String>,

    // Events
    pub events: Vec<SubtitleEvent>,
    pub events_format: Vec<String>,

    // Embedded content
    pub fonts: Vec<EmbeddedFont>,
    pub graphics: Vec<EmbeddedGraphic>,

    // Operation tracking
    pub operations: Vec<OperationRecord>,

    // OCR metadata
    pub ocr_metadata: Option<OCRMetadata>,

    // Comments before sections
    pub section_comments: HashMap<String, Vec<String>>,

    // Header lines before first section
    pub header_lines: Vec<String>,
}

impl SubtitleData {
    pub fn new() -> Self {
        Self {
            source_path: None,
            source_format: "ass".to_string(),
            encoding: "utf-8".to_string(),
            has_bom: false,
            script_info: Vec::new(),
            aegisub_garbage: Vec::new(),
            aegisub_extradata: Vec::new(),
            custom_sections: Vec::new(),
            section_order: Vec::new(),
            styles: Vec::new(),
            styles_format: default_styles_format(),
            events: Vec::new(),
            events_format: default_events_format(),
            fonts: Vec::new(),
            graphics: Vec::new(),
            operations: Vec::new(),
            ocr_metadata: None,
            section_comments: HashMap::new(),
            header_lines: Vec::new(),
        }
    }

    // =========================================================================
    // Factory Methods
    // =========================================================================

    /// Load subtitle from file, auto-detecting format — `from_file`
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let ext = path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "ass" | "ssa" => {
                super::parsers::ass_parser::parse_ass_file(path)
            }
            "srt" => {
                super::parsers::srt_parser::parse_srt_file(path)
            }
            _ => Err(format!("Unsupported subtitle format: .{ext}")),
        }
    }

    // =========================================================================
    // Save Methods
    // =========================================================================

    /// Save as ASS file — `save_ass`
    pub fn save_ass(&self, path: &Path, rounding: &str) -> Result<(), String> {
        super::writers::ass_writer::write_ass_file(self, path, rounding)
    }

    /// Save as SRT file — `save_srt`
    pub fn save_srt(&self, path: &Path, rounding: &str) -> Result<(), String> {
        super::writers::srt_writer::write_srt_file(self, path, rounding)
    }

    /// Save to file, format determined by extension — `save`
    pub fn save(&self, path: &Path, rounding: Option<&str>) -> Result<(), String> {
        let ext = path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let rounding_mode = rounding.unwrap_or("floor");

        match ext.as_str() {
            "ass" | "ssa" => self.save_ass(path, rounding_mode),
            "srt" => self.save_srt(path, rounding_mode),
            _ => Err(format!("Unsupported output format: .{ext}")),
        }
    }

    // =========================================================================
    // Style Access (ordered map behavior)
    // =========================================================================

    /// Get a style by name.
    pub fn get_style(&self, name: &str) -> Option<&SubtitleStyle> {
        self.styles.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    /// Get a mutable style by name.
    pub fn get_style_mut(&mut self, name: &str) -> Option<&mut SubtitleStyle> {
        self.styles.iter_mut().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    /// Add or replace a style.
    pub fn set_style(&mut self, name: &str, style: SubtitleStyle) {
        if let Some(existing) = self.styles.iter_mut().find(|(n, _)| n == name) {
            existing.1 = style;
        } else {
            self.styles.push((name.to_string(), style));
        }
    }

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Get only dialogue events (not comments) — `get_dialogue_events`
    pub fn get_dialogue_events(&self) -> Vec<&SubtitleEvent> {
        self.events.iter().filter(|e| !e.is_comment).collect()
    }

    /// Get events with specific style — `get_events_by_style`
    pub fn get_events_by_style(&self, style_name: &str) -> Vec<&SubtitleEvent> {
        self.events.iter().filter(|e| e.style == style_name).collect()
    }

    /// Get event count per style name — `get_style_counts`
    pub fn get_style_counts(&self) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for event in &self.events {
            if !event.is_comment {
                *counts.entry(event.style.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// Quick static method to get style counts from a file path without
    /// keeping the full SubtitleData in memory.
    ///
    /// 1:1 port of Python's `SubtitleData.get_style_counts_from_file(path)`.
    /// Used by UI validation dialogs that only need style enumeration.
    pub fn get_style_counts_from_file(path: &Path) -> Result<HashMap<String, usize>, String> {
        let data = Self::from_file(path)?;
        Ok(data.get_style_counts())
    }

    /// Get (min_start_ms, max_end_ms) — `get_timing_range`
    pub fn get_timing_range(&self) -> (f64, f64) {
        if self.events.is_empty() {
            return (0.0, 0.0);
        }
        let min_start = self.events.iter().map(|e| e.start_ms).fold(f64::INFINITY, f64::min);
        let max_end = self.events.iter().map(|e| e.end_ms).fold(f64::NEG_INFINITY, f64::max);
        (min_start, max_end)
    }

    /// Validate data integrity — `validate`
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let style_names: Vec<&str> = self.styles.iter().map(|(n, _)| n.as_str()).collect();

        for (i, event) in self.events.iter().enumerate() {
            if event.end_ms <= event.start_ms {
                warnings.push(format!(
                    "Event {i}: end ({}) <= start ({})",
                    event.end_ms, event.start_ms
                ));
            }
            if event.start_ms < 0.0 {
                warnings.push(format!("Event {i}: negative start time ({})", event.start_ms));
            }
            if !style_names.contains(&event.style.as_str()) && event.style != "Default" {
                warnings.push(format!(
                    "Event {i}: references unknown style '{}'",
                    event.style
                ));
            }
        }
        warnings
    }

    /// Sort events by start time — `sort_events_by_time`
    pub fn sort_events_by_time(&mut self) {
        self.events.sort_by(|a, b| {
            a.start_ms.partial_cmp(&b.start_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.layer.cmp(&b.layer))
        });
    }

    /// Shift all event timing by offset — `shift_timing`
    pub fn shift_timing(&mut self, offset_ms: f64) {
        for event in &mut self.events {
            event.start_ms = (event.start_ms + offset_ms).max(0.0);
            event.end_ms = (event.end_ms + offset_ms).max(0.0);
        }
    }

    /// Remove events by style — `remove_events_by_style`
    pub fn remove_events_by_style(&mut self, style_name: &str) -> usize {
        let original_count = self.events.len();
        self.events.retain(|e| e.style != style_name);
        original_count - self.events.len()
    }

    /// Remove overlapping events — `remove_overlapping_events`
    pub fn remove_overlapping_events(&mut self) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut unique = Vec::new();
        let mut removed = 0;

        for event in self.events.drain(..) {
            let key = format!("{}|{}|{}", event.start_ms as i64, event.end_ms as i64, event.text);
            if seen.insert(key) {
                unique.push(event);
            } else {
                removed += 1;
            }
        }
        self.events = unique;
        removed
    }
}

impl Default for SubtitleData {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse ASS timestamp to float milliseconds — `_parse_ass_time`
pub fn parse_ass_time(time_str: &str) -> f64 {
    let parts: Vec<&str> = time_str.trim().splitn(3, ':').collect();
    if parts.len() != 3 {
        return 0.0;
    }
    let hours: i64 = parts[0].parse().unwrap_or(0);
    let minutes: i64 = parts[1].parse().unwrap_or(0);

    let sec_parts: Vec<&str> = parts[2].splitn(2, '.').collect();
    let seconds: i64 = sec_parts[0].parse().unwrap_or(0);
    let centiseconds: i64 = if sec_parts.len() > 1 {
        sec_parts[1].parse().unwrap_or(0)
    } else {
        0
    };

    (hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + centiseconds * 10) as f64
}

/// Format float milliseconds to ASS timestamp — `_format_ass_time`
///
/// THIS IS WHERE ROUNDING HAPPENS.
pub fn format_ass_time(ms: f64, rounding: &str) -> String {
    let total_cs = match rounding {
        "ceil" => (ms / 10.0).ceil() as i64,
        "round" => (ms / 10.0).round() as i64,
        _ => (ms / 10.0).floor() as i64, // "floor" default
    };
    let total_cs = total_cs.max(0);

    let cs = total_cs % 100;
    let total_seconds = total_cs / 100;
    let seconds = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let minutes = total_minutes % 60;
    let hours = total_minutes / 60;

    format!("{hours}:{minutes:02}:{seconds:02}.{cs:02}")
}

/// Format number, removing unnecessary decimals — `_format_number`
pub fn format_number(value: f64) -> String {
    if value == (value as i64) as f64 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Default styles format fields.
fn default_styles_format() -> Vec<String> {
    vec![
        "Name", "Fontname", "Fontsize",
        "PrimaryColour", "SecondaryColour", "OutlineColour", "BackColour",
        "Bold", "Italic", "Underline", "StrikeOut",
        "ScaleX", "ScaleY", "Spacing", "Angle",
        "BorderStyle", "Outline", "Shadow", "Alignment",
        "MarginL", "MarginR", "MarginV", "Encoding",
    ].into_iter().map(String::from).collect()
}

/// Default events format fields.
fn default_events_format() -> Vec<String> {
    vec![
        "Layer", "Start", "End", "Style", "Name",
        "MarginL", "MarginR", "MarginV", "Effect", "Text",
    ].into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ass_time_basic() {
        assert_eq!(parse_ass_time("0:00:00.00"), 0.0);
        assert_eq!(parse_ass_time("0:00:01.00"), 1000.0);
        assert_eq!(parse_ass_time("0:00:01.50"), 1500.0);
        assert_eq!(parse_ass_time("1:23:45.67"), 5025670.0);
    }

    #[test]
    fn format_ass_time_basic() {
        assert_eq!(format_ass_time(0.0, "floor"), "0:00:00.00");
        assert_eq!(format_ass_time(1000.0, "floor"), "0:00:01.00");
        assert_eq!(format_ass_time(1500.0, "floor"), "0:00:01.50");
        assert_eq!(format_ass_time(5025670.0, "floor"), "1:23:45.67");
    }

    #[test]
    fn parse_format_round_trip() {
        let test_values = [0.0, 1000.0, 1500.0, 5025670.0, 60000.0];
        for ms in test_values {
            let formatted = format_ass_time(ms, "floor");
            let parsed = parse_ass_time(&formatted);
            assert_eq!(parsed, ms, "Round trip failed for {ms}ms -> {formatted}");
        }
    }

    #[test]
    fn format_number_works() {
        assert_eq!(format_number(48.0), "48");
        assert_eq!(format_number(100.0), "100");
        assert_eq!(format_number(2.5), "2.5");
    }

    #[test]
    fn subtitle_style_default() {
        let style = SubtitleStyle::new("Default");
        assert_eq!(style.fontname, "Arial");
        assert_eq!(style.fontsize, 48.0);
        assert_eq!(style.alignment, 2);
    }

    #[test]
    fn subtitle_event_duration() {
        let event = SubtitleEvent::new(1000.0, 5000.0, "Test");
        assert_eq!(event.duration_ms(), 4000.0);
    }

    #[test]
    fn subtitle_data_timing_range() {
        let mut data = SubtitleData::new();
        data.events.push(SubtitleEvent::new(1000.0, 2000.0, "First"));
        data.events.push(SubtitleEvent::new(3000.0, 5000.0, "Second"));
        let (min_start, max_end) = data.get_timing_range();
        assert_eq!(min_start, 1000.0);
        assert_eq!(max_end, 5000.0);
    }
}
