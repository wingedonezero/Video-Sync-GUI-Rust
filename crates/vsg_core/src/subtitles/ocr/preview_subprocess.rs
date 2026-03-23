//! Preview OCR functionality.
//!
//! In Python, this ran preview OCR in a subprocess.
//! In the Rust port, preview OCR runs in-process directly via `run_preview_ocr`.

use std::path::{Path, PathBuf};

use tracing::{error, info};

use super::run_preview_ocr;

/// JSON prefix used by the Python subprocess protocol.
/// Kept for compatibility if interop with the Python version is needed.
pub const JSON_PREFIX: &str = "__VSG_PREVIEW_JSON__ ";

/// Run preview OCR directly (no subprocess).
///
/// This is the Rust equivalent of the Python `preview_subprocess.main()`,
/// but calls `run_preview_ocr` directly instead of spawning a subprocess.
///
/// # Arguments
/// * `subtitle_path` - Path to subtitle file (.idx/.sub/.sup)
/// * `lang` - OCR language code (e.g., "eng")
/// * `output_dir` - Output directory for preview OCR files
/// * `log_callback` - Optional callback for log messages
///
/// # Returns
/// `Some((json_path, ass_path))` if preview OCR succeeded, `None` on failure.
pub fn run_preview_direct(
    subtitle_path: &str,
    lang: &str,
    output_dir: &Path,
    log_callback: Option<&dyn Fn(&str)>,
) -> Option<(String, String)> {
    let cb: Option<&dyn Fn(&str)> = log_callback;

    let result = run_preview_ocr(subtitle_path, lang, output_dir, cb);

    match result {
        Some((ref json_path, ref ass_path)) => {
            info!(
                "[Preview OCR] Success: json={}, ass={}",
                json_path, ass_path
            );
        }
        None => {
            if let Some(cb) = log_callback {
                cb("[Preview OCR] Preview OCR returned no result");
            }
            error!("[Preview OCR] Preview OCR returned no result");
        }
    }

    result
}

/// Emit a JSON payload in the subprocess protocol format.
///
/// This is only useful if interoperating with the Python subprocess protocol.
pub fn emit_preview_json_payload(payload: &serde_json::Value) {
    if let Ok(json_str) = serde_json::to_string(payload) {
        println!("{}{}", JSON_PREFIX, json_str);
    }
}
