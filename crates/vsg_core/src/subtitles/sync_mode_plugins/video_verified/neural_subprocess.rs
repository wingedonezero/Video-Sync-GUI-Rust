//! Neural feature matching -- direct implementation using tch-rs.
//!
//! In the Python version, this was a subprocess worker that isolated the ISC
//! model + GPU allocation in a separate process. In Rust, since we use tch-rs
//! directly (no Python subprocess overhead), we implement the neural matching
//! as a direct function call.
//!
//! The subprocess pattern is unnecessary in Rust because:
//! 1. tch-rs manages GPU memory natively through Rust's ownership system
//! 2. No Python GIL contention to worry about
//! 3. CUDA contexts are properly managed by tch-rs
//!
//! This module provides a simplified entry point that wraps
//! `neural_matcher::calculate_neural_verified_offset()` for cases where
//! the caller wants a fire-and-forget interface.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

use super::neural_matcher::calculate_neural_verified_offset;

/// Result of a neural matching run.
#[derive(Debug, Clone)]
pub struct NeuralMatchResult {
    pub success: bool,
    pub final_offset_ms: Option<f64>,
    pub details: HashMap<String, serde_json::Value>,
    pub error: Option<String>,
}

/// Run neural feature matching directly using tch-rs.
///
/// This replaces the Python subprocess approach. In Rust, GPU isolation
/// happens naturally through tch-rs's CUDA context management.
///
/// # Arguments
/// * `source_video` - Path to source video
/// * `target_video` - Path to target video (Source 1)
/// * `total_delay_ms` - Total delay from audio correlation
/// * `global_shift_ms` - Global shift component
/// * `settings` - AppSettings
/// * `runner` - CommandRunner for logging
/// * `temp_dir` - Temp directory for caches
/// * `video_duration_ms` - Optional video duration
/// * `debug_output_dir` - Optional directory for debug reports
/// * `source_key` - Source identifier for debug naming
pub fn run_neural_matching(
    source_video: &str,
    target_video: &str,
    total_delay_ms: f64,
    global_shift_ms: f64,
    settings: &AppSettings,
    runner: &CommandRunner,
    temp_dir: Option<PathBuf>,
    video_duration_ms: Option<f64>,
    debug_output_dir: Option<PathBuf>,
    source_key: &str,
) -> NeuralMatchResult {
    match calculate_neural_verified_offset(
        source_video,
        target_video,
        total_delay_ms,
        global_shift_ms,
        Some(settings),
        runner,
        temp_dir,
        video_duration_ms,
        debug_output_dir,
        source_key,
    ) {
        (Some(offset), details) => NeuralMatchResult {
            success: true,
            final_offset_ms: Some(offset),
            details,
            error: None,
        },
        (None, details) => {
            let error = details
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Neural matching returned None")
                .to_string();
            NeuralMatchResult {
                success: false,
                final_offset_ms: None,
                details,
                error: Some(error),
            }
        }
    }
}

/// Write a debug report for neural matching results.
///
/// In the Python version, this was part of the subprocess communication.
/// Here it's a standalone utility function.
pub fn write_neural_debug_report(
    debug_output_dir: &std::path::Path,
    source_video: &str,
    target_video: &str,
    source_key: &str,
    results: &HashMap<String, serde_json::Value>,
) -> Result<PathBuf, std::io::Error> {
    std::fs::create_dir_all(debug_output_dir)?;

    let tgt_stem = std::path::Path::new(target_video)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let key_sanitized = if source_key.is_empty() {
        "unknown".to_string()
    } else {
        source_key.replace(' ', "")
    };

    let report_name = format!("{}_{}_neural_verify.txt", tgt_stem, key_sanitized);
    let report_path = debug_output_dir.join(&report_name);

    let mut lines = Vec::new();
    lines.push("=".repeat(80));
    lines.push("NEURAL FEATURE MATCHING DEBUG REPORT".to_string());
    lines.push("=".repeat(80));
    lines.push(format!("Source: {}", source_video));
    lines.push(format!("Target: {}", target_video));

    if let Some(confidence) = results.get("confidence").and_then(|v| v.as_str()) {
        lines.push(format!("Confidence: {}", confidence));
    }
    if let Some(mean_score) = results.get("mean_score").and_then(|v| v.as_f64()) {
        lines.push(format!("Mean score: {:.4}", mean_score));
    }
    if let Some(total_time) = results.get("total_time_s").and_then(|v| v.as_f64()) {
        lines.push(format!("Total time: {:.1}s", total_time));
    }

    lines.push(String::new());
    lines.push("=".repeat(80));
    lines.push("END OF REPORT".to_string());
    lines.push("=".repeat(80));

    std::fs::write(&report_path, lines.join("\n"))?;

    Ok(report_path)
}
