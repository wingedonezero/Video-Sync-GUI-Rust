//! Chapter backup — 1:1 port of `vsg_core/postprocess/chapter_backup.py`.
//!
//! Extracts, merges, and re-injects chapter XML to preserve languages
//! across FFmpeg timestamp normalization.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::io::runner::CommandRunner;

/// Extract chapters XML from MKV file — `extract_chapters_xml`
pub fn extract_chapters_xml(
    mkv_path: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Option<String> {
    let out = runner.run(
        &["mkvextract", &mkv_path.to_string_lossy(), "chapters", "-"],
        tool_paths,
    )?;
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Inject chapters XML into MKV file — `inject_chapters`
pub fn inject_chapters(
    mkv_path: &Path,
    chapters_xml: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) {
    // Write XML to temp file
    let temp_path = mkv_path.with_extension("chapters.xml");
    if fs::write(&temp_path, chapters_xml).is_err() {
        runner.log_message("[WARNING] Failed to write chapters XML to temp file");
        return;
    }

    // Inject with mkvpropedit
    runner.run(
        &[
            "mkvpropedit",
            &mkv_path.to_string_lossy(),
            "--chapters",
            &temp_path.to_string_lossy(),
        ],
        tool_paths,
    );

    // Cleanup
    let _ = fs::remove_file(&temp_path);
}
