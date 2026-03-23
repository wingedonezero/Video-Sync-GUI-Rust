//! OCR coordination wrapper for subtitle processing step.
//!
//! Handles OCR execution and creates preserved copies of original image-based subtitles.
//! In the Rust port, OCR runs in-process (no subprocess needed).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::models::media::{StreamProps, Track};
use super::{run_ocr_unified, PipelineResult};

/// Process OCR for a track and create preserved copy of original.
///
/// Returns:
///     - `Some(PipelineResult)` if OCR succeeded
///     - `None` if OCR failed/skipped
///
/// Side effects:
///     - Pushes preserved copy info into `preserved_items_out`
///     - The caller should update the item's extracted_path and track codec
pub fn process_ocr_with_preservation(
    extracted_path: &Path,
    track: &Track,
    settings: &HashMap<String, serde_json::Value>,
    temp_dir: &Path,
    logs_dir: &Path,
    debug_output_dir: &Path,
    log_callback: Option<&dyn Fn(&str)>,
) -> Option<ProcessedOcrResult> {
    let ocr_work_dir = temp_dir.join("ocr");

    // Run OCR (always in-process in Rust port)
    let idx_path = extracted_path.with_extension("idx");
    let result = run_ocr_unified(
        idx_path.to_str().unwrap_or_default(),
        &track.props.lang,
        settings,
        Some(ocr_work_dir.as_path()),
        Some(logs_dir),
        Some(debug_output_dir),
        track.id as u32,
        log_callback,
    );

    // Check OCR result
    let pipeline_result = match result {
        Some(r) => r,
        None => {
            if let Some(cb) = log_callback {
                cb(&format!(
                    "[OCR] ERROR: OCR failed for track {}",
                    track.id
                ));
                cb("[OCR] Keeping original image-based subtitle");
            }
            return None;
        }
    };

    if !pipeline_result.success {
        if let Some(cb) = log_callback {
            cb(&format!(
                "[OCR] ERROR: OCR failed for track {}: {}",
                track.id,
                pipeline_result.error.as_deref().unwrap_or("Unknown error")
            ));
            cb("[OCR] Keeping original image-based subtitle");
        }
        return None;
    }

    if pipeline_result.subtitle_count == 0 {
        if let Some(cb) = log_callback {
            cb(&format!(
                "[OCR] WARNING: OCR produced no events for track {}",
                track.id
            ));
        }
        return None;
    }

    // OCR succeeded - build preserved copy info
    let preserved_track = Track {
        source: track.source.clone(),
        id: track.id,
        track_type: track.track_type.clone(),
        props: StreamProps {
            codec_id: track.props.codec_id.clone(),
            lang: track.props.lang.clone(),
            name: if track.props.name.is_empty() {
                "Original".to_string()
            } else {
                format!("{} (Original)", track.props.name)
            },
        },
    };

    // Updated track reflecting OCR output
    let ocr_track = Track {
        source: track.source.clone(),
        id: track.id,
        track_type: track.track_type.clone(),
        props: StreamProps {
            codec_id: "S_TEXT/ASS".to_string(),
            lang: track.props.lang.clone(),
            name: track.props.name.clone(),
        },
    };

    Some(ProcessedOcrResult {
        pipeline_result,
        ass_path: extracted_path.with_extension("ass"),
        preserved_track,
        ocr_track,
    })
}

/// Result of successful OCR processing.
pub struct ProcessedOcrResult {
    /// The OCR pipeline result with subtitle count, timing, etc.
    pub pipeline_result: PipelineResult,
    /// Path to the output .ass file.
    pub ass_path: PathBuf,
    /// Track info for the preserved (original) copy.
    pub preserved_track: Track,
    /// Updated track info reflecting OCR output (S_TEXT/ASS codec).
    pub ocr_track: Track,
}
