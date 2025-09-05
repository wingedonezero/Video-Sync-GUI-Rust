// src/core/subtitle_utils.rs

use std::path::{Path, PathBuf};
use crate::core::process::CommandRunner;

/// Converts an SRT file to ASS format using ffmpeg.
pub async fn convert_srt_to_ass(
    runner: &CommandRunner,
    subtitle_path: &Path,
) -> Result<PathBuf, String> {
    if subtitle_path.extension().map_or(true, |ext| ext.to_string_lossy().to_lowercase() != "srt") {
        return Ok(subtitle_path.to_path_buf());
    }

    let output_path = subtitle_path.with_extension("ass");
    let result = runner
    .run(
        "ffmpeg",
        &["-y", "-i", &subtitle_path.to_string_lossy(), &output_path.to_string_lossy()],
    )
    .await?;

    if result.exit_code == 0 && output_path.exists() {
        Ok(output_path)
    } else {
        Err("Failed to convert SRT to ASS".to_string())
    }
}

/// Converts an H:MM:SS.cs timestamp string to seconds as a float.
pub fn parse_ass_time(time_str: &str) -> f64 {
    let parts: Vec<&str> = time_str.trim().split(':').collect();
    if parts.len() != 3 {
        return 0.0;
    }
    let s_cs: Vec<&str> = parts[2].split('.').collect();
    if s_cs.len() != 2 {
        return 0.0;
    }

    let h: f64 = parts[0].parse().unwrap_or(0.0);
    let m: f64 = parts[1].parse().unwrap_or(0.0);
    let s: f64 = s_cs[0].parse().unwrap_or(0.0);
    let cs: f64 = s_cs[1].parse().unwrap_or(0.0);

    h * 3600.0 + m * 60.0 + s + cs / 100.0
}
