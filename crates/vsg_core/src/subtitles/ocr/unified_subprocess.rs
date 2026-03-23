//! Unified OCR execution module.
//!
//! In Python, this ran OCR in a subprocess for process isolation.
//! In the Rust port, OCR runs in-process directly via `run_ocr_unified`,
//! so no subprocess spawning is needed. This module provides a thin
//! convenience wrapper that calls the OCR pipeline directly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{error, info};

use super::{run_ocr_unified, PipelineResult};

/// JSON prefix used by the Python subprocess protocol.
/// Kept for compatibility if interop with the Python version is needed.
pub const JSON_PREFIX: &str = "__VSG_UNIFIED_OCR_JSON__ ";

/// Simple log callback that prints to stdout.
fn stdout_logger(message: &str) {
    println!("{}", message);
}

/// Run unified OCR directly (no subprocess).
///
/// This is the Rust equivalent of the Python `unified_subprocess.main()`,
/// but instead of spawning a subprocess, it calls `run_ocr_unified` directly.
///
/// # Arguments
/// * `subtitle_path` - Path to subtitle file (.idx/.sub/.sup)
/// * `lang` - OCR language code (e.g., "eng")
/// * `settings` - OCR settings dictionary
/// * `work_dir` - Working directory for OCR temp files
/// * `logs_dir` - Directory for OCR logs/reports
/// * `debug_output_dir` - Directory for OCR debug output
/// * `track_id` - Track ID for OCR output
///
/// # Returns
/// `Some(PipelineResult)` if OCR succeeded, `None` on failure.
pub fn run_ocr_direct(
    subtitle_path: &str,
    lang: &str,
    settings: &HashMap<String, serde_json::Value>,
    work_dir: &Path,
    logs_dir: &Path,
    debug_output_dir: Option<&Path>,
    track_id: u32,
    log_callback: Option<&dyn Fn(&str)>,
) -> Option<PipelineResult> {
    let cb: &dyn Fn(&str) = match log_callback {
        Some(cb) => cb,
        None => &stdout_logger,
    };

    let debug_dir = debug_output_dir.unwrap_or(logs_dir);

    let result = run_ocr_unified(
        subtitle_path,
        lang,
        settings,
        Some(work_dir),
        Some(logs_dir),
        Some(debug_dir),
        track_id,
        Some(cb),
    );

    match result {
        Some(ref r) if r.success && r.subtitle_count > 0 => {
            info!(
                "[OCR] Successfully processed {} subtitles",
                r.subtitle_count
            );
        }
        Some(ref r) if !r.success => {
            error!(
                "[OCR] OCR failed: {}",
                r.error.as_deref().unwrap_or("Unknown error")
            );
        }
        Some(ref r) if r.subtitle_count == 0 => {
            error!("[OCR] OCR returned no subtitle data");
            return None;
        }
        None => {
            error!("[OCR] OCR returned no result");
        }
        _ => {}
    }

    result
}

/// Emit a JSON payload in the subprocess protocol format.
///
/// This is only useful if interoperating with the Python subprocess protocol.
pub fn emit_json_payload(payload: &serde_json::Value) {
    if let Ok(json_str) = serde_json::to_string(payload) {
        println!("{}{}", JSON_PREFIX, json_str);
    }
}
