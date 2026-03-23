//! Post-merge finalizer — 1:1 port of `vsg_core/postprocess/finalizer.py`.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;
use crate::models::settings::AppSettings;

use super::chapter_backup::{extract_chapters_xml, inject_chapters};

/// Check if timestamp rebasing is needed — `check_if_rebasing_is_needed`
pub fn check_if_rebasing_is_needed(
    mkv_path: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> bool {
    runner.log_message("[Finalize] Checking if timestamp rebasing is necessary...");

    let out = runner.run(
        &[
            "ffprobe", "-v", "error", "-select_streams", "v:0",
            "-show_entries", "packet=pts_time",
            "-of", "default=noprint_wrappers=1:nokey=1",
            "-read_intervals", "%+#1",
            &mkv_path.to_string_lossy(),
        ],
        tool_paths,
    );

    if let Some(out) = out {
        if let Ok(pts) = out.trim().parse::<f64>() {
            if pts > 0.01 {
                runner.log_message(&format!(
                    "[Finalize] First video timestamp is {pts:.3}s. Rebasing is required."
                ));
                return true;
            }
        }
    }

    runner.log_message(
        "[Finalize] Video timestamps already start at zero. Rebasing is not required.",
    );
    false
}

/// Finalize merged file with chapter preservation — `finalize_merged_file`
pub fn finalize_merged_file(
    temp_output_path: &Path,
    final_output_path: &Path,
    runner: &CommandRunner,
    settings: &AppSettings,
    tool_paths: &HashMap<String, String>,
) {
    runner.log_message("--- Post-Merge: Finalizing File ---");

    // Step 1: Backup chapters
    runner.log_message("[Finalize] Backing up chapter languages...");
    let original_chapters_xml =
        extract_chapters_xml(temp_output_path, runner, tool_paths);

    // Step 2: FFmpeg timestamp normalization
    let ffmpeg_temp = temp_output_path.with_extension("normalized.mkv");
    runner.log_message("[Finalize] Step 1/2: Rebasing timestamps with FFmpeg...");

    let result = runner.run(
        &[
            "ffmpeg", "-y", "-i", &temp_output_path.to_string_lossy(),
            "-c", "copy", "-map", "0",
            "-fflags", "+genpts", "-avoid_negative_ts", "make_zero",
            &ffmpeg_temp.to_string_lossy(),
        ],
        tool_paths,
    );

    if result.is_none() {
        runner.log_message(
            "[WARNING] Timestamp normalization with FFmpeg failed. Using original file.",
        );
        let _ = std::fs::rename(temp_output_path, final_output_path);
        return;
    }

    // Replace original with normalized
    let _ = std::fs::remove_file(temp_output_path);
    let _ = std::fs::rename(&ffmpeg_temp, temp_output_path);
    runner.log_message("Timestamp normalization successful.");

    // Step 3: Restore chapters
    if let Some(ref chapters_xml) = original_chapters_xml {
        runner.log_message("[Finalize] Restoring original chapters...");
        inject_chapters(temp_output_path, chapters_xml, runner, tool_paths);
    }

    // Step 4: Optional tag stripping
    if settings.post_mux_strip_tags {
        runner.log_message("[Finalize] Step 2/2: Stripping ENCODER tag with mkvpropedit...");
        runner.run(
            &["mkvpropedit", &temp_output_path.to_string_lossy(), "--tags", "all:"],
            tool_paths,
        );
        runner.log_message("Stripped ENCODER tag successfully.");
    }

    // Step 5: Move to final location
    let _ = move_file(temp_output_path, final_output_path);
    runner.log_message("[Finalize] Post-merge finalization complete.");
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).or_else(|_| {
        std::fs::copy(from, to)
            .map_err(|e| format!("Copy failed: {e}"))?;
        let _ = std::fs::remove_file(from);
        Ok(())
    })
}
