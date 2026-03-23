//! Integrated OCR System for VOB and PGS Subtitles
//!
//! This module provides a complete OCR pipeline for converting image-based subtitles
//! (VobSub .sub/.idx and PGS .sup) to text-based formats (ASS/SRT).
//!
//! Components:
//!     - parsers: Extract subtitle images and metadata from VOB/PGS files
//!     - preprocessing: Adaptive image preprocessing for optimal OCR accuracy
//!     - engine: OCR engine wrapper with confidence tracking
//!     - postprocess: Pattern fixes and dictionary validation
//!     - report: OCR quality reporting (unknown words, confidence, fixes)
//!     - output: ASS/SRT generation with position support
//!
//! The pipeline is designed to:
//!     1. Parse image-based subtitle formats, extracting timing and position
//!     2. Preprocess images adaptively based on quality analysis
//!     3. Run OCR with confidence tracking per line
//!     4. Apply pattern-based and dictionary-validated fixes
//!     5. Generate output with position tags for non-bottom subtitles
//!     6. Report unknown words and low-confidence results

pub mod parsers;
pub mod engine;
pub mod pipeline;
pub mod preprocessing;
pub mod postprocess;
pub mod output;
pub mod report;
pub mod debug;
pub mod dictionaries;
pub mod romaji_dictionary;
pub mod word_lists;
pub mod wrapper;
pub mod subtitle_edit;
pub mod unified_subprocess;
pub mod preview_subprocess;

// Re-exports
pub use engine::{OCRConfig, OCREngine, OCRResult, OCRLineResult};
pub use pipeline::{OCRPipeline, PipelineConfig, PipelineResult};
pub use report::{LowConfidenceLine, OCRReport, UnknownWord};
pub use romaji_dictionary::{KanaToRomaji, RomajiDictionary, get_romaji_dictionary, is_romaji_word};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Language code mapping from 3-letter codes to Tesseract codes.
fn lang_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("eng", "eng");
    m.insert("jpn", "jpn");
    m.insert("spa", "spa");
    m.insert("fra", "fra");
    m.insert("deu", "deu");
    m.insert("chi", "chi_sim");
    m.insert("kor", "kor");
    m.insert("por", "por");
    m.insert("ita", "ita");
    m.insert("rus", "rus");
    m
}

/// Build OCR settings dict from application settings for the internal OCR pipeline.
pub fn build_ocr_settings(settings: &HashMap<String, serde_json::Value>, lang: &str) -> HashMap<String, serde_json::Value> {
    use serde_json::Value;

    let lmap = lang_map();
    let tesseract_lang = lmap.get(lang).copied().unwrap_or(lang);

    let get_val = |key: &str, default: Value| -> Value {
        settings.get(key).cloned().unwrap_or(default)
    };

    let mut result = HashMap::new();
    result.insert("ocr_language".into(), Value::String(tesseract_lang.to_string()));
    result.insert("ocr_preprocess_auto".into(), get_val("ocr_preprocess_auto", Value::Bool(true)));
    result.insert("ocr_force_binarization".into(), get_val("ocr_force_binarization", Value::Bool(false)));
    result.insert("ocr_upscale_threshold".into(), get_val("ocr_upscale_threshold", Value::Number(40.into())));
    result.insert("ocr_target_height".into(), get_val("ocr_target_height", Value::Number(80.into())));
    result.insert("ocr_border_size".into(), get_val("ocr_border_size", Value::Number(5.into())));
    result.insert("ocr_binarization_method".into(), get_val("ocr_binarization_method", Value::String("otsu".into())));
    result.insert("ocr_denoise".into(), get_val("ocr_denoise", Value::Bool(false)));
    result.insert("ocr_engine".into(), get_val("ocr_engine", Value::String("tesseract".into())));
    result.insert("ocr_psm".into(), get_val("ocr_psm", Value::Number(7.into())));
    result.insert("ocr_char_whitelist".into(), get_val("ocr_char_whitelist", Value::String(String::new())));
    result.insert("ocr_char_blacklist".into(), get_val("ocr_char_blacklist", Value::String("|".into())));
    result.insert("ocr_multi_pass".into(), get_val("ocr_multi_pass", Value::Bool(true)));
    result.insert("ocr_low_confidence_threshold".into(), get_val("ocr_low_confidence_threshold", serde_json::json!(60.0)));
    result.insert("ocr_cleanup_enabled".into(), get_val("ocr_cleanup_enabled", Value::Bool(true)));
    result.insert("ocr_custom_wordlist_path".into(), get_val("ocr_custom_wordlist_path", Value::String(String::new())));
    result.insert("ocr_output_format".into(), get_val("ocr_output_format", Value::String("ass".into())));
    result.insert("ocr_preserve_positions".into(), get_val("ocr_preserve_positions", Value::Bool(true)));
    result.insert("ocr_bottom_threshold".into(), get_val("ocr_bottom_threshold", serde_json::json!(75.0)));
    result.insert("ocr_video_width".into(), get_val("ocr_video_width", Value::Number(0.into())));
    result.insert("ocr_video_height".into(), get_val("ocr_video_height", Value::Number(0.into())));
    result.insert("ocr_font_size_ratio".into(), get_val("ocr_font_size_ratio", serde_json::json!(5.80)));
    result.insert("ocr_generate_report".into(), get_val("ocr_generate_report", Value::Bool(true)));
    result.insert("ocr_save_debug_images".into(), get_val("ocr_save_debug_images", Value::Bool(false)));
    result.insert("ocr_debug_output".into(), get_val("ocr_debug_output", Value::Bool(false)));
    result.insert("ocr_max_workers".into(), get_val("ocr_max_workers", Value::Number(1.into())));
    result
}

/// Check if OCR is available.
///
/// Returns (is_available, message).
pub fn check_ocr_available() -> (bool, String) {
    // In the Rust port, OCR model inference is not yet available.
    (false, "OCR model inference not yet available in Rust port".to_string())
}

/// Run OCR unified — main entry point for OCR -> SubtitleData conversion.
///
/// Currently stubbed: parses VobSub images but OCR inference is not available.
pub fn run_ocr_unified(
    subtitle_path: &str,
    lang: &str,
    settings: &HashMap<String, serde_json::Value>,
    work_dir: Option<&Path>,
    logs_dir: Option<&Path>,
    debug_output_dir: Option<&Path>,
    track_id: u32,
    log_callback: Option<&dyn Fn(&str)>,
) -> Option<PipelineResult> {
    let sub_path = PathBuf::from(subtitle_path);
    let suffix = sub_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let input_path = match suffix.as_str() {
        "idx" => sub_path.clone(),
        "sub" => {
            let idx_path = sub_path.with_extension("idx");
            if !idx_path.exists() {
                if let Some(cb) = log_callback {
                    cb(&format!("[OCR] ERROR: IDX file not found: {}", idx_path.display()));
                }
                return None;
            }
            idx_path
        }
        "sup" => {
            if let Some(cb) = log_callback {
                cb(&format!("[OCR] WARNING: PGS (.sup) support not yet implemented. Skipping {}", sub_path.display()));
            }
            return None;
        }
        _ => {
            if let Some(cb) = log_callback {
                cb(&format!("[OCR] Skipping {}: Unsupported format.", sub_path.display()));
            }
            return None;
        }
    };

    let work = work_dir.map(PathBuf::from).unwrap_or_else(|| sub_path.parent().unwrap().join("ocr_work"));
    let logs = logs_dir.map(PathBuf::from).unwrap_or_else(|| sub_path.parent().unwrap().to_path_buf());
    let debug_dir = debug_output_dir.map(PathBuf::from).unwrap_or_else(|| logs.clone());

    if let Some(cb) = log_callback {
        cb(&format!("[OCR] Starting OCR on {}...", sub_path.display()));
        cb(&format!("[OCR] Language: {}, Mode: Unified (SubtitleData)", lang));
    }

    let ocr_settings = build_ocr_settings(settings, lang);

    let mut pipeline = OCRPipeline::new(
        ocr_settings,
        work,
        logs.clone(),
        Some(debug_dir),
        None::<Box<dyn Fn(&str, f64)>>, // Progress callback simplified
    );

    let result = pipeline.process(&input_path, None, track_id);

    if result.success {
        if let Some(cb) = log_callback {
            cb(&format!(
                "[OCR] Successfully processed {} subtitles in {:.1}s",
                result.subtitle_count, result.duration_seconds
            ));
        }
    } else {
        if let Some(cb) = log_callback {
            cb(&format!("[OCR] ERROR: {}", result.error.as_deref().unwrap_or("Unknown error")));
        }
    }

    Some(result)
}

/// Run fast preview OCR for style editor.
///
/// Currently stubbed: OCR model inference is not available in the Rust port.
pub fn run_preview_ocr(
    _subtitle_path: &str,
    _lang: &str,
    _output_dir: &Path,
    log_callback: Option<&dyn Fn(&str)>,
) -> Option<(String, String)> {
    if let Some(cb) = log_callback {
        cb("[Preview OCR] OCR model inference not yet available in Rust port");
    }
    None
}
