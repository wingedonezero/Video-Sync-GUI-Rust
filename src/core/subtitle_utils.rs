// src/core/subtitle_utils.rs

use std::fs;
use std::path::{Path, PathBuf};
use crate::core::process::CommandRunner;
use serde_json::Value;

pub fn read_subtitle_file(file_path: &Path) -> Result<String, String> {
    // Try reading with utf-8-sig first, then fall back.
    let content = fs::read(file_path).map_err(|e| e.to_string())?;
    if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Ok(String::from_utf8_lossy(&content[3..]).into_owned())
    } else {
        Ok(String::from_utf8_lossy(&content).into_owned())
    }
}

pub fn write_subtitle_file(file_path: &Path, content: &str) -> Result<(), String> {
    fs::write(file_path, content).map_err(|e| e.to_string())
}

pub async fn convert_srt_to_ass(runner: &CommandRunner, subtitle_path: &Path) -> Result<PathBuf, String> {
    if subtitle_path.extension().map_or(true, |ext| ext.to_string_lossy().to_lowercase() != "srt") {
        return Ok(subtitle_path.to_path_buf());
    }

    let output_path = subtitle_path.with_extension("ass");
    runner.send_log(&format!("[SubConvert] Converting {} to ASS format...", subtitle_path.display())).await;

    let result = runner.run(
        "ffmpeg",
        &["-y", "-i", &subtitle_path.to_string_lossy(), &output_path.to_string_lossy()],
    ).await?;

    if result.exit_code == 0 && output_path.exists() {
        Ok(output_path)
    } else {
        runner.send_log(&format!("[SubConvert] WARN: Failed to convert {}.", subtitle_path.display())).await;
        Ok(subtitle_path.to_path_buf()) // Return original path on failure
    }
}

pub async fn rescale_subtitle(runner: &CommandRunner, subtitle_path: &Path, video_path: &str) -> Result<bool, String> {
    if subtitle_path.extension().map_or(true, |ext| ext.to_string_lossy().to_lowercase() != "ass" && ext.to_string_lossy().to_lowercase() != "ssa") {
        return Ok(false);
    }

    let result = runner.run("ffprobe", &[
        "-v", "error", "-select_streams", "v:0",
        "-show_entries", "stream=width,height", "-of", "json", video_path
    ]).await?;

    let (vid_w, vid_h) = match serde_json::from_str::<Value>(&result.stdout) {
        Ok(json) => {
            let w = json["streams"][0]["width"].as_u64().unwrap_or(0);
            let h = json["streams"][0]["height"].as_u64().unwrap_or(0);
            (w, h)
        },
        Err(_) => {
            runner.send_log("[Rescale] WARN: Failed to parse video resolution.").await;
            return Ok(false);
        }
    };

    if vid_w == 0 || vid_h == 0 {
        return Ok(false);
    }

    let content = read_subtitle_file(subtitle_path)?;
    let mut new_content = String::new();
    let mut changed = false;
    let mut has_playres_tags = false;

    for line in content.lines() {
        let trimmed_lower = line.trim().to_lowercase();
        if trimmed_lower.starts_with("playresx:") {
            has_playres_tags = true;
            new_content.push_str(&format!("PlayResX: {}\n", vid_w));
            changed = true;
        } else if trimmed_lower.starts_with("playresy:") {
            has_playres_tags = true;
            new_content.push_str(&format!("PlayResY: {}\n", vid_h));
            changed = true;
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    if !has_playres_tags {
        runner.send_log(&format!("[Rescale] INFO: {} has no PlayResX/Y tags.", subtitle_path.display())).await;
        return Ok(false);
    }

    if changed {
        runner.send_log(&format!("[Rescale] Rescaling {} to {}x{}", subtitle_path.display(), vid_w, vid_h)).await;
        write_subtitle_file(subtitle_path, &new_content)?;
    }

    Ok(changed)
}

pub fn multiply_font_size(content: &str, multiplier: f64) -> (String, usize) {
    if (multiplier - 1.0).abs() < 1e-9 {
        return (content.to_string(), 0);
    }

    let mut new_content = String::new();
    let mut modified_count = 0;

    for line in content.lines() {
        if line.to_lowercase().starts_with("style:") {
            let mut parts: Vec<&str> = line.splitn(4, ',').collect();
            if parts.len() >= 3 {
                if let Ok(original_size) = parts[2].trim().parse::<f64>() {
                    let new_size = (original_size * multiplier).round() as u32;
                    let new_line = format!("{},{},{},{}", parts[0], parts[1], new_size, parts.get(3).unwrap_or(&""));
                    new_content.push_str(&new_line);
                    modified_count += 1;
                } else {
                    new_content.push_str(line);
                }
            } else {
                new_content.push_str(line);
            }
        } else {
            new_content.push_str(line);
        }
        new_content.push('\n');
    }

    (new_content, modified_count)
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
