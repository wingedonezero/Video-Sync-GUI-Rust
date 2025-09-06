// src/core/subtitle_utils.rs
//
// Rust port of vsg_core/subtitle_utils.py:
// - convert_srt_to_ass(): ffmpeg SRT -> ASS (no-op when not .srt)
// - rescale_subtitle(): update PlayResX/PlayResY in ASS/SSA to match video resolution from ffprobe
// - multiply_font_size(): adjust size in Style: lines (ASS/SSA)
// All functions log via CommandRunner and mirror Python behavior/messages.

use crate::core::command_runner::CommandRunner;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn convert_srt_to_ass(subtitle_path: &str, runner: &CommandRunner) -> String {
    let p = Path::new(subtitle_path);
    if p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("srt"))
        .unwrap_or(false)
        {
            let output = p.with_extension("ass");
            runner.log(&format!(
                "[SubConvert] Converting {} to ASS format...",
                p.file_name().unwrap_or_default().to_string_lossy()
            ));
            let cmd = [
                "ffmpeg",
                "-y",
                "-i",
                subtitle_path,
                output.to_string_lossy().as_ref(),
            ];
            let _ = runner.run(&cmd);
            if output.exists() {
                return output.to_string_lossy().to_string();
            } else {
                runner.log(&format!(
                    "[SubConvert] WARN: Failed to convert {}.",
                    p.file_name().unwrap_or_default().to_string_lossy()
                ));
                return subtitle_path.to_string();
            }
        }
        subtitle_path.to_string()
}

pub fn rescale_subtitle(subtitle_path: &str, video_path: &str, runner: &CommandRunner) -> bool {
    let p = Path::new(subtitle_path);
    let ext_ok = p
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.eq_ignore_ascii_case("ass") || e.eq_ignore_ascii_case("ssa"))
    .unwrap_or(false);
    if !ext_ok {
        return false;
    }

    let cmd = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height",
        "-of",
        "json",
        video_path,
    ];

    let out = match runner.run(&cmd) {
        Some(s) => s,
        None => {
            runner.log(&format!(
                "[Rescale] WARN: Could not get video resolution for {}.",
                Path::new(video_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
            ));
            return false;
        }
    };

    let (vid_w, vid_h) = match serde_json::from_str::<Value>(&out)
    .ok()
    .and_then(|v| v.get("streams").and_then(|s| s.as_array()).and_then(|arr| arr.get(0)).cloned())
    .and_then(|st| {
        let w = st.get("width").and_then(|x| x.as_i64())?;
        let h = st.get("height").and_then(|x| x.as_i64())?;
        Some((w as i32, h as i32))
    }) {
        Some(t) => t,
        None => {
            runner.log("[Rescale] WARN: Failed to parse video resolution.");
            return false;
        }
    };

    // Read with BOM tolerance (utf-8-sig behavior)
    let mut raw = Vec::new();
    if fs::File::open(p).and_then(|mut f| f.read_to_end(&mut raw)).is_err() {
        runner.log(&format!(
            "[Rescale] WARN: Could not read {}.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        return false;
    }
    let content = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&raw[3..]).into_owned()
    } else {
        String::from_utf8_lossy(&raw).into_owned()
    };

    let re_x = Regex::new(r"(?m)^\s*PlayResX:\s*(\d+)").unwrap();
    let re_y = Regex::new(r"(?m)^\s*PlayResY:\s*(\d+)").unwrap();

    let sub_w = re_x
    .captures(&content)
    .and_then(|c| c.get(1))
    .and_then(|m| m.as_str().parse::<i32>().ok());
    let sub_h = re_y
    .captures(&content)
    .and_then(|c| c.get(1))
    .and_then(|m| m.as_str().parse::<i32>().ok());

    if sub_w.is_none() || sub_h.is_none() {
        runner.log(&format!(
            "[Rescale] INFO: {} has no PlayResX/Y tags.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        return false;
    }
    let (sub_w, sub_h) = (sub_w.unwrap(), sub_h.unwrap());

    if sub_w == vid_w && sub_h == vid_h {
        runner.log(&format!(
            "[Rescale] INFO: {} already matches video resolution ({}x{}).",
                            p.file_name().unwrap_or_default().to_string_lossy(),
                            vid_w,
                            vid_h
        ));
        return false;
    }

    runner.log(&format!(
        "[Rescale] Rescaling {} from {}x{} to {}x{}.",
        p.file_name().unwrap_or_default().to_string_lossy(),
                        sub_w,
                        sub_h,
                        vid_w,
                        vid_h
    ));

    // Replace values
    let updated = re_x
    .replace(&content, format!("PlayResX: {}", vid_w))
    .to_string();
    let updated = re_y
    .replace(&updated, format!("PlayResY: {}", vid_h))
    .to_string();

    if fs::write(p, updated.as_bytes()).is_ok() {
        true
    } else {
        runner.log(&format!(
            "[Rescale] ERROR: Could not process {}: write failed.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        false
    }
}

pub fn multiply_font_size(subtitle_path: &str, multiplier: f64, runner: &CommandRunner) -> bool {
    let p = Path::new(subtitle_path);
    let ext_ok = p
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.eq_ignore_ascii_case("ass") || e.eq_ignore_ascii_case("ssa"))
    .unwrap_or(false);
    if !ext_ok || (multiplier - 1.0).abs() < f64::EPSILON {
        return false;
    }

    runner.log(&format!(
        "[Font Size] Applying {:.2}x size multiplier to {}.",
        multiplier,
        p.file_name().unwrap_or_default().to_string_lossy()
    ));

    // Read with BOM tolerance
    let mut raw = Vec::new();
    if fs::File::open(p).and_then(|mut f| f.read_to_end(&mut raw)).is_err() {
        runner.log(&format!(
            "[Font Size] ERROR: Could not read {}.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        return false;
    }
    let content = if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&raw[3..]).into_owned()
    } else {
        String::from_utf8_lossy(&raw).into_owned()
    };

    // Parse line-by-line and rewrite "Style:" lines’ Fontsize field (3rd CSV field after "Style: Name,Fontname,Fontsize,...")
    let mut new_lines = Vec::with_capacity(1024);
    let mut modified = 0usize;

    for line in content.lines() {
        let trimmed_low = line.trim_start().to_ascii_lowercase();
        if trimmed_low.starts_with("style:") {
            // split into at most 4 segments: "Style: Name", "Fontname", "Fontsize", "rest..."
            let mut parts = line.splitn(4, ',').collect::<Vec<_>>();
            if parts.len() >= 4 {
                // parts[0] includes "Style: Name"
                // parts[1] is Fontname
                // parts[2] is Fontsize
                // parts[3] is remainder
                if let Ok(orig_size) = parts[2].trim().parse::<f64>() {
                    let new_size = (orig_size * multiplier).round() as i64;
                    let rebuilt = format!("{},{},{},{}", parts[0], parts[1], new_size, parts[3]);
                    new_lines.push(rebuilt);
                    modified += 1;
                    continue;
                }
            }
        }
        new_lines.push(line.to_string());
    }

    if modified > 0 {
        let joined = new_lines.join("\n");
        if fs::write(p, joined.as_bytes()).is_ok() {
            runner.log(&format!("[Font Size] Modified {} style definition(s).", modified));
            true
        } else {
            runner.log(&format!(
                "[Font Size] ERROR: Could not process {}: write failed.",
                p.file_name().unwrap_or_default().to_string_lossy()
            ));
            false
        }
    } else {
        runner.log(&format!(
            "[Font Size] WARN: No style definitions found to modify in {}.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        false
    }
}
