//! OCR Output — SubtitleData Conversion
//!
//! Converts OCR results into structured data for the unified subtitle pipeline.
//! Position handling:
//!     - Bottom subtitles (default): Default style (alignment 2)
//!     - Top subtitles (< 40% of frame): Top style (alignment 8)

use std::collections::HashMap;

/// Configuration for subtitle output.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub preserve_positions: bool,
    pub bottom_threshold_percent: f64,
    pub top_threshold_percent: f64,
    pub style_name: String,
    pub font_name: String,
    pub font_size: i32,
    pub primary_color: String,
    pub outline_color: String,
    pub outline_width: f64,
    pub shadow_depth: f64,
    pub margin_v: i32,
    pub video_width: i32,
    pub video_height: i32,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            preserve_positions: true,
            bottom_threshold_percent: 75.0,
            top_threshold_percent: 40.0,
            style_name: "Default".to_string(),
            font_name: "Arial".to_string(),
            font_size: 48,
            primary_color: "&H00FFFFFF".to_string(),
            outline_color: "&H00000000".to_string(),
            outline_width: 2.0,
            shadow_depth: 1.0,
            margin_v: 30,
            video_width: 0,
            video_height: 0,
        }
    }
}

/// A single OCR line with its classified screen region.
#[derive(Debug, Clone)]
pub struct LineRegion {
    pub text: String,
    /// "top" or "bottom"
    pub region: String,
    /// Y center in source image pixels
    pub y_center: f64,
}

/// Extended subtitle result with all OCR metadata.
#[derive(Debug, Clone)]
pub struct OCRSubtitleResult {
    pub index: usize,
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,

    // OCR metadata
    pub confidence: f64,
    pub raw_ocr_text: String,
    pub fixes_applied: HashMap<String, i32>,
    pub unknown_words: Vec<String>,

    // Position data
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub frame_width: u32,
    pub frame_height: u32,

    // VobSub specific
    pub is_forced: bool,
    pub subtitle_colors: Vec<Vec<u8>>,
    pub dominant_color: Vec<u8>,

    // Per-line region classifications from pipeline
    pub line_regions: Vec<LineRegion>,

    // Debug image reference
    pub debug_image: String,
}

impl Default for OCRSubtitleResult {
    fn default() -> Self {
        Self {
            index: 0,
            start_ms: 0.0,
            end_ms: 0.0,
            text: String::new(),
            confidence: 0.0,
            raw_ocr_text: String::new(),
            fixes_applied: HashMap::new(),
            unknown_words: Vec::new(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            frame_width: 0,
            frame_height: 0,
            is_forced: false,
            subtitle_colors: Vec::new(),
            dominant_color: Vec::new(),
            line_regions: Vec::new(),
            debug_image: String::new(),
        }
    }
}
