//! OCR Engine Wrapper
//!
//! Provides OCR functionality with:
//!     - Confidence score tracking per line
//!     - Multiple PSM (Page Segmentation Mode) support
//!     - Character whitelist/blacklist configuration
//!     - Line-by-line OCR for better accuracy
//!
//! OCR model inference (tesseract/easyocr/paddleocr) is STUBBED in the Rust port.

use image::GrayImage;
use tracing::{info, warn};
use std::collections::HashMap;

/// Configuration for OCR engine.
#[derive(Debug, Clone)]
pub struct OCRConfig {
    pub language: String,
    pub psm: i32,
    pub oem: i32,
    pub char_whitelist: String,
    pub char_blacklist: String,
    pub min_confidence: f64,
    pub low_confidence_threshold: f64,
    pub enable_multi_pass: bool,
    pub fallback_psm: i32,
}

impl Default for OCRConfig {
    fn default() -> Self {
        Self {
            language: "eng".to_string(),
            psm: 6,
            oem: 3,
            char_whitelist: String::new(),
            char_blacklist: "|".to_string(),
            min_confidence: 0.0,
            low_confidence_threshold: 60.0,
            enable_multi_pass: true,
            fallback_psm: 4,
        }
    }
}

/// Result for a single OCR line.
#[derive(Debug, Clone)]
pub struct OCRLineResult {
    pub text: String,
    pub confidence: f64,
    pub word_confidences: Vec<(String, f64)>,
    pub psm_used: i32,
    pub was_retry: bool,
    /// Y center of this line within the source image (pixels).
    pub y_center: f64,
}

impl Default for OCRLineResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            confidence: 0.0,
            word_confidences: Vec::new(),
            psm_used: 7,
            was_retry: false,
            y_center: 0.0,
        }
    }
}

/// Complete OCR result for a subtitle image.
#[derive(Debug, Clone)]
pub struct OCRResult {
    pub text: String,
    pub lines: Vec<OCRLineResult>,
    pub average_confidence: f64,
    pub min_confidence: f64,
    pub low_confidence: bool,
    pub error: Option<String>,
}

impl Default for OCRResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            lines: Vec::new(),
            average_confidence: 0.0,
            min_confidence: 0.0,
            low_confidence: false,
            error: None,
        }
    }
}

impl OCRResult {
    /// Check if OCR was successful.
    pub fn success(&self) -> bool {
        self.error.is_none() && !self.text.trim().is_empty()
    }
}

/// OCR engine with confidence tracking.
///
/// In the Rust port, actual OCR model inference is stubbed.
/// The engine logs a message when called and returns empty results.
pub struct OCREngine {
    pub config: OCRConfig,
}

impl OCREngine {
    pub fn new(config: OCRConfig) -> Self {
        warn!("OCR model inference not yet available in Rust port");
        Self { config }
    }

    /// Perform OCR on a preprocessed image.
    ///
    /// STUBBED: Returns empty result with a message indicating OCR is not available.
    pub fn ocr_image(&self, _image: &GrayImage) -> OCRResult {
        warn!("OCR model inference not yet available in Rust port");
        OCRResult {
            text: String::new(),
            error: Some("OCR model inference not yet available in Rust port".to_string()),
            ..Default::default()
        }
    }

    /// OCR each line separately for better accuracy.
    ///
    /// STUBBED: Returns empty result.
    pub fn ocr_lines_separately(
        &self,
        _image: &GrayImage,
        _line_images: Option<Vec<GrayImage>>,
    ) -> OCRResult {
        warn!("OCR model inference not yet available in Rust port");
        OCRResult {
            text: String::new(),
            error: Some("OCR model inference not yet available in Rust port".to_string()),
            ..Default::default()
        }
    }

    /// Clean up resources.
    pub fn cleanup(&mut self) {
        // No resources to clean up in stubbed version
    }
}

/// Get list of available OCR languages.
///
/// STUBBED: Returns empty list.
pub fn get_available_languages() -> Vec<String> {
    Vec::new()
}

/// Create OCR engine from settings dictionary.
pub fn create_ocr_engine(settings_dict: &HashMap<String, serde_json::Value>) -> OCREngine {
    let language = settings_dict.get("ocr_language")
        .and_then(|v| v.as_str())
        .unwrap_or("eng")
        .to_string();

    let config = OCRConfig {
        language,
        psm: settings_dict.get("ocr_psm")
            .and_then(|v| v.as_i64())
            .unwrap_or(7) as i32,
        char_whitelist: settings_dict.get("ocr_char_whitelist")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        char_blacklist: settings_dict.get("ocr_char_blacklist")
            .and_then(|v| v.as_str())
            .unwrap_or("|")
            .to_string(),
        low_confidence_threshold: settings_dict.get("ocr_low_confidence_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0),
        enable_multi_pass: settings_dict.get("ocr_multi_pass")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        ..Default::default()
    };

    OCREngine::new(config)
}
