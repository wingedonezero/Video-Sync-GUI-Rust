//! Base classes and data structures for subtitle image parsing.
//!
//! Provides common interfaces and data structures used by all subtitle parsers
//! (VobSub, PGS, etc.).

use std::path::Path;
use image::RgbaImage;

/// Represents a single subtitle image extracted from an image-based format.
#[derive(Clone)]
pub struct SubtitleImage {
    /// Sequential index of this subtitle (0-based)
    pub index: usize,
    /// Start time in milliseconds
    pub start_ms: i64,
    /// End time in milliseconds (may be 0 if unknown)
    pub end_ms: i64,
    /// The subtitle bitmap as an RGBA image
    pub image: RgbaImage,
    /// X coordinate of top-left corner (for positioning)
    pub x: u32,
    /// Y coordinate of top-left corner (for positioning)
    pub y: u32,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Width of the video frame (for calculating relative position)
    pub frame_width: u32,
    /// Height of the video frame (for calculating relative position)
    pub frame_height: u32,
    /// Whether this is a forced subtitle
    pub is_forced: bool,
    /// Color palette if applicable (for indexed color images) — (R, G, B, A)
    pub palette: Option<Vec<(u8, u8, u8, u8)>>,
}

impl SubtitleImage {
    /// Create a new SubtitleImage. Sets width/height from the image dimensions
    /// if they are zero.
    pub fn new(
        index: usize,
        start_ms: i64,
        end_ms: i64,
        image: RgbaImage,
    ) -> Self {
        let (w, h) = image.dimensions();
        Self {
            index,
            start_ms,
            end_ms,
            width: w,
            height: h,
            image,
            x: 0,
            y: 0,
            frame_width: 720,
            frame_height: 480,
            is_forced: false,
            palette: None,
        }
    }

    /// Return start time as HH:MM:SS.mmm string.
    pub fn start_time(&self) -> String {
        Self::ms_to_timestamp(self.start_ms)
    }

    /// Return end time as HH:MM:SS.mmm string.
    pub fn end_time(&self) -> String {
        Self::ms_to_timestamp(self.end_ms)
    }

    /// Return duration in milliseconds.
    pub fn duration_ms(&self) -> i64 {
        self.end_ms - self.start_ms
    }

    /// Return Y position as percentage of frame height.
    ///
    /// 0% = top of frame, 100% = bottom of frame.
    pub fn y_position_percent(&self) -> f64 {
        if self.frame_height == 0 {
            return 100.0; // Assume bottom if unknown
        }
        let center_y = self.y as f64 + (self.height as f64 / 2.0);
        (center_y / self.frame_height as f64) * 100.0
    }

    /// Return X position as percentage of frame width (center of subtitle).
    pub fn x_position_percent(&self) -> f64 {
        if self.frame_width == 0 {
            return 50.0; // Assume centered if unknown
        }
        let center_x = self.x as f64 + (self.width as f64 / 2.0);
        (center_x / self.frame_width as f64) * 100.0
    }

    /// Check if subtitle is positioned at the bottom of the frame.
    pub fn is_bottom_positioned(&self, threshold_percent: f64) -> bool {
        self.y_position_percent() >= threshold_percent
    }

    /// Check if subtitle is positioned at the top of the frame.
    pub fn is_top_positioned(&self, threshold_percent: f64) -> bool {
        self.y_position_percent() <= threshold_percent
    }

    /// Convert milliseconds to HH:MM:SS.mmm format.
    pub fn ms_to_timestamp(ms: i64) -> String {
        let ms = ms.max(0);
        let hours = ms / 3_600_000;
        let remainder = ms % 3_600_000;
        let minutes = remainder / 60_000;
        let remainder = remainder % 60_000;
        let seconds = remainder / 1_000;
        let milliseconds = remainder % 1_000;
        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, milliseconds)
    }
}

/// Result of parsing a subtitle file.
#[derive(Default)]
pub struct ParseResult {
    /// List of extracted subtitle images
    pub subtitles: Vec<SubtitleImage>,
    /// Information about the source format
    pub format_info: std::collections::HashMap<String, String>,
    /// Any errors encountered during parsing
    pub errors: Vec<String>,
    /// Any warnings generated during parsing
    pub warnings: Vec<String>,
}

impl ParseResult {
    /// Return true if parsing succeeded (no errors and has subtitles).
    pub fn success(&self) -> bool {
        self.errors.is_empty() && !self.subtitles.is_empty()
    }

    /// Return number of subtitles extracted.
    pub fn subtitle_count(&self) -> usize {
        self.subtitles.len()
    }
}

/// Abstract base trait for subtitle image parsers.
pub trait SubtitleImageParser {
    /// Parse a subtitle file and extract images.
    fn parse(&self, file_path: &Path, work_dir: Option<&Path>) -> ParseResult;

    /// Check if this parser can handle the given file.
    fn can_parse(&self, file_path: &Path) -> bool;
}

/// Detect the appropriate parser for a file based on extension.
pub fn detect_parser(file_path: &Path) -> Option<Box<dyn SubtitleImageParser>> {
    let suffix = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match suffix.as_str() {
        "idx" | "sub" => Some(Box::new(super::vobsub::VobSubParser::new())),
        // Future: "sup" => Some(Box::new(super::pgs::PGSParser::new())),
        _ => None,
    }
}
