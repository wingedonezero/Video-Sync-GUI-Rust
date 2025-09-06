// src/vsg_core/subtitle_utils.rs

use crate::config::Config;
use crate::process;
use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct FfprobeStream {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
}

/// Converts an SRT subtitle file to ASS format using ffmpeg.
/// Returns the new path if successful, otherwise returns the original path.
pub fn convert_srt_to_ass<F>(
    config: &Config,
    subtitle_path: &Path,
    log_callback: Arc<Mutex<F>>,
) -> Result<PathBuf>
where
F: FnMut(String) + Send + 'static,
{
    if !subtitle_path.extension().map_or(false, |s| s == "srt") {
        return Ok(subtitle_path.to_path_buf());
    }

    let output_path = subtitle_path.with_extension("ass");
    log_callback.lock().unwrap()(format!(
        "[SubConvert] Converting {} to ASS format...",
        subtitle_path.display()
    ));

    process::run_command(
        config,
        "ffmpeg",
        &[
            "-y",
            "-i",
            subtitle_path.to_str().unwrap(),
                         output_path.to_str().unwrap(),
        ],
        log_callback,
    )?;

    if output_path.exists() {
        Ok(output_path)
    } else {
        Ok(subtitle_path.to_path_buf())
    }
}

/// Rescales an ASS/SSA subtitle's PlayResX/Y tags to match a video file's resolution.
pub fn rescale_subtitle<F>(
    config: &Config,
    subtitle_path: &Path,
    video_path: &Path,
    log_callback: Arc<Mutex<F>>,
) -> Result<()>
where
F: FnMut(String) + Send + 'static,
{
    // 1. Get video resolution using ffprobe
    let ffprobe_out = process::run_command(
        config,
        "ffprobe",
        &[
            "-v", "error", "-select_streams", "v:0", "-show_entries",
            "stream=width,height", "-of", "json", video_path.to_str().unwrap(),
        ],
        Arc::clone(&log_callback),
    )?;

    let video_info: FfprobeOutput = serde_json::from_str(&ffprobe_out)?;
    let (vid_w, vid_h) = video_info.streams.first()
    .map(|s| (s.width, s.height))
    .ok_or_else(|| anyhow!("Could not parse video resolution from ffprobe"))?;

    // 2. Read and modify subtitle file content
    let content = fs::read_to_string(subtitle_path)?;
    let playresx_re = Regex::new(r"(?im)^\s*PlayResX:\s*\d+")?;
    let playresy_re = Regex::new(r"(?im)^\s*PlayResY:\s*\d+")?;

    if !playresx_re.is_match(&content) || !playresy_re.is_match(&content) {
        log_callback.lock().unwrap()(format!(
            "[Rescale] INFO: {} has no PlayResX/Y tags to modify.",
            subtitle_path.display()
        ));
        return Ok(());
    }

    log_callback.lock().unwrap()(format!(
        "[Rescale] Rescaling {} to {}x{}.",
        subtitle_path.display(), vid_w, vid_h
    ));

    let new_content = playresx_re.replace(&content, format!("PlayResX: {}", vid_w));
    let new_content = playresy_re.replace(&new_content, format!("PlayResY: {}", vid_h));

    fs::write(subtitle_path, new_content)?;
    Ok(())
}

/// Multiplies the font size in an ASS/SSA file's style definitions.
pub fn multiply_font_size<F>(
    subtitle_path: &Path,
    multiplier: f64,
    log_callback: Arc<Mutex<F>>,
) -> Result<()>
where
F: FnMut(String) + Send + 'static,
{
    if (multiplier - 1.0).abs() < 1e-6 {
        return Ok(());
    }

    log_callback.lock().unwrap()(format!(
        "[Font Size] Applying {:.2}x size multiplier to {}.",
        multiplier, subtitle_path.display()
    ));

    let content = fs::read_to_string(subtitle_path)?;
    let mut new_lines = Vec::new();
    let mut modified_count = 0;

    for line in content.lines() {
        if line.to_lowercase().starts_with("style:") {
            let mut parts: Vec<&str> = line.splitn(4, ',').collect();
            if parts.len() >= 3 {
                if let Ok(original_size) = parts[2].trim().parse::<f64>() {
                    let new_size = (original_size * multiplier).round() as u32;
                    parts[2] = ""; // We can't use the old part, so we build a new string
                    new_lines.push(format!("{},{},{},{}", parts[0], parts[1], new_size, parts.get(3).unwrap_or(&"")));
                    modified_count += 1;
                    continue;
                }
            }
        }
        new_lines.push(line.to_string());
    }

    if modified_count > 0 {
        fs::write(subtitle_path, new_lines.join("\n"))?;
        log_callback.lock().unwrap()(format!(
            "[Font Size] Modified {} style definition(s).", modified_count
        ));
    } else {
        log_callback.lock().unwrap()(format!(
            "[Font Size] WARN: No style definitions found to modify in {}.",
            subtitle_path.display()
        ));
    }

    Ok(())
}
