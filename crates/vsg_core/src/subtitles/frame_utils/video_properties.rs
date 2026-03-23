//! Video property detection functions for subtitle synchronization.
//!
//! Contains:
//! - FPS detection (ffprobe)
//! - MediaInfo-based detection (MPEG-2 picture header analysis)
//! - Multi-source cross-validated video property detection
//! - Video property comparison for sync strategy selection
//!
//! 1:1 port of `vsg_core/subtitles/frame_utils/video_properties.py`.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::io::runner::CommandRunner;

/// Video properties detected from a video file.
pub type VideoProperties = HashMap<String, Value>;

/// Detect video properties using MediaInfo.
///
/// MediaInfo reads MPEG-2 picture coding extension headers to determine
/// scan type and pulldown patterns.
fn detect_mediainfo_properties(
    video_path: &str,
    runner: &CommandRunner,
) -> HashMap<String, Value> {
    // Check if mediainfo is available
    let mediainfo_path = which::which("mediainfo");
    if mediainfo_path.is_err() {
        runner.log_message("[MediaInfo] mediainfo not found - skipping");
        return HashMap::new();
    }

    let inform = "Video;\
        mi_fps=%FrameRate%\\n\
        mi_fps_mode=%FrameRate_Mode%\\n\
        mi_scan_type=%ScanType%\\n\
        mi_scan_order=%ScanOrder%\\n\
        mi_original_fps=%FrameRate_Original%\\n\
        mi_codec=%Format%\\n";

    let result = Command::new("mediainfo")
        .arg(format!("--Inform={}", inform))
        .arg(video_path)
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                runner.log_message("[MediaInfo] WARNING: mediainfo returned non-zero");
                return HashMap::new();
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut props: HashMap<String, Value> = HashMap::new();

            for line in stdout.trim().lines() {
                if !line.contains('=') {
                    continue;
                }
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() != 2 {
                    continue;
                }
                let key = parts[0].trim();
                let value = parts[1].trim();
                if value.is_empty() {
                    continue;
                }

                if key == "mi_fps" || key == "mi_original_fps" {
                    if let Ok(v) = value.parse::<f64>() {
                        props.insert(key.to_string(), Value::from(v));
                    }
                } else {
                    props.insert(key.to_string(), Value::String(value.to_string()));
                }
            }

            props
        }
        Err(exc) => {
            runner.log_message(&format!(
                "[MediaInfo] WARNING: detection failed: {}",
                exc
            ));
            HashMap::new()
        }
    }
}

/// Classify content type by cross-validating ffprobe and MediaInfo results.
///
/// Returns (content_type, confidence).
///
/// content_type: "progressive", "interlaced", "soft_telecine", or "unknown"
/// confidence: "high", "medium", or "low"
fn classify_content_type(
    ffprobe_props: &VideoProperties,
    mi_props: &HashMap<String, Value>,
    runner: &CommandRunner,
) -> (String, String) {
    let codec = ffprobe_props
        .get("codec_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_mpeg2 = codec == "mpeg2video" || codec == "mpeg1video";

    // ffprobe signals
    let fp_interlaced = ffprobe_props
        .get("interlaced")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let fp_field_order = ffprobe_props
        .get("field_order")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let fp_is_vfr = ffprobe_props
        .get("is_vfr")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // MediaInfo signals
    let mi_fps_mode = mi_props
        .get("mi_fps_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mi_scan_type = mi_props
        .get("mi_scan_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mi_scan_order = mi_props
        .get("mi_scan_order")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let has_mediainfo = !mi_props.is_empty();

    // 1. Non-MPEG2: progressive encode (the normal/current path)
    if !is_mpeg2 {
        if fp_interlaced {
            runner.log_message(
                "[ContentType] Non-MPEG2 interlaced (H.264/HEVC interlaced encode)",
            );
            return ("interlaced".to_string(), "medium".to_string());
        }
        runner.log_message("[ContentType] Progressive encode (non-MPEG2)");
        return ("progressive".to_string(), "high".to_string());
    }

    // 2. MPEG-2 with MediaInfo available: use MPEG-2 flag analysis
    if has_mediainfo {
        let is_mi_vfr = mi_fps_mode.eq_ignore_ascii_case("VFR");
        let is_mi_pulldown = mi_scan_order.to_lowercase().contains("pulldown");
        let is_mi_interlaced = mi_scan_type.eq_ignore_ascii_case("interlaced");

        // Soft telecine: MediaInfo reads repeat_first_field flags
        if is_mi_vfr && is_mi_pulldown {
            let confidence = if fp_is_vfr || fp_field_order == "progressive" {
                "high"
            } else {
                runner.log_message(
                    "[ContentType] NOTE: MediaInfo=VFR+Pulldown but ffprobe=interlaced - trusting MediaInfo",
                );
                "medium"
            };
            runner.log_message(&format!(
                "[ContentType] Soft telecine detected (MediaInfo: {}, {})",
                mi_fps_mode, mi_scan_order
            ));
            return ("soft_telecine".to_string(), confidence.to_string());
        }

        if is_mi_vfr && !is_mi_pulldown {
            runner.log_message(&format!(
                "[ContentType] VFR MPEG-2 without standard pulldown (ScanOrder: {})",
                mi_scan_order
            ));
            return ("soft_telecine".to_string(), "medium".to_string());
        }

        // Pure interlaced: MediaInfo says CFR + Interlaced
        if !is_mi_vfr && is_mi_interlaced {
            let confidence = if fp_interlaced { "high" } else {
                runner.log_message(
                    "[ContentType] NOTE: MediaInfo=Interlaced but ffprobe=progressive - trusting MediaInfo",
                );
                "medium"
            };
            runner.log_message(&format!(
                "[ContentType] Pure interlaced (MediaInfo: {}, {})",
                mi_fps_mode, mi_scan_type
            ));
            return ("interlaced".to_string(), confidence.to_string());
        }

        // MediaInfo says CFR + Progressive MPEG-2
        if !is_mi_vfr && !is_mi_interlaced {
            runner.log_message(&format!(
                "[ContentType] Progressive MPEG-2 (MediaInfo: {}, {})",
                mi_fps_mode, mi_scan_type
            ));
            return ("progressive".to_string(), "medium".to_string());
        }
    }

    // 3. MPEG-2 without MediaInfo: ffprobe-only fallback
    runner.log_message(
        "[ContentType] WARNING: MediaInfo unavailable - using ffprobe only (less reliable for MPEG-2)",
    );

    let is_dvd = ffprobe_props
        .get("is_dvd")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if fp_is_vfr {
        runner.log_message("[ContentType] Likely soft telecine (ffprobe VFR detected)");
        return ("soft_telecine".to_string(), "low".to_string());
    }

    if fp_interlaced && is_dvd {
        runner.log_message(
            "[ContentType] MPEG-2 DVD interlaced (could be pure interlaced or hard telecine)",
        );
        return ("interlaced".to_string(), "low".to_string());
    }

    if fp_interlaced {
        return ("interlaced".to_string(), "low".to_string());
    }

    ("unknown".to_string(), "low".to_string())
}

/// Detect frame rate from video file using ffprobe.
///
/// Returns frame rate as float (e.g., 23.976), or 23.976 as fallback.
pub fn detect_video_fps(video_path: &str, runner: &CommandRunner) -> f64 {
    let filename = Path::new(video_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    runner.log_message(&format!(
        "[FPS Detection] Detecting FPS from: {}",
        filename
    ));

    let result = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-select_streams", "v:0",
            "-show_entries", "stream=r_frame_rate",
            "-of", "json",
            video_path,
        ])
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                runner.log_message(
                    "[FPS Detection] WARNING: ffprobe failed, using default 23.976 fps",
                );
                return 23.976;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(&stdout) {
                Ok(data) => {
                    let r_frame_rate = data["streams"][0]["r_frame_rate"]
                        .as_str()
                        .unwrap_or("24000/1001");

                    let fps = if r_frame_rate.contains('/') {
                        let parts: Vec<&str> = r_frame_rate.split('/').collect();
                        let num: f64 = parts[0].parse().unwrap_or(24000.0);
                        let denom: f64 = parts[1].parse().unwrap_or(1001.0);
                        if denom != 0.0 { num / denom } else { 23.976 }
                    } else {
                        r_frame_rate.parse().unwrap_or(23.976)
                    };

                    runner.log_message(&format!(
                        "[FPS Detection] Detected FPS: {:.3} ({})",
                        fps, r_frame_rate
                    ));
                    fps
                }
                Err(_) => {
                    runner.log_message(
                        "[FPS Detection] WARNING: Failed to parse ffprobe output",
                    );
                    23.976
                }
            }
        }
        Err(e) => {
            runner.log_message(&format!(
                "[FPS Detection] WARNING: FPS detection failed: {}",
                e
            ));
            runner.log_message("[FPS Detection] Using default: 23.976 fps");
            23.976
        }
    }
}

/// Detect comprehensive video properties for sync strategy selection.
///
/// Detects FPS, interlacing, field order, telecine, duration, frame count,
/// and resolution (width/height).
pub fn detect_video_properties(video_path: &str, runner: &CommandRunner) -> VideoProperties {
    let filename = Path::new(video_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    runner.log_message(&format!(
        "[VideoProps] Detecting properties for: {}",
        filename
    ));

    // Default/fallback values
    let mut props = VideoProperties::new();
    props.insert("fps".to_string(), Value::from(23.976));
    props.insert("fps_fraction".to_string(), serde_json::json!([24000, 1001]));
    props.insert("original_fps".to_string(), Value::Null);
    props.insert("original_fps_fraction".to_string(), Value::Null);
    props.insert("is_vfr".to_string(), Value::Bool(false));
    props.insert("is_soft_telecine".to_string(), Value::Bool(false));
    props.insert("interlaced".to_string(), Value::Bool(false));
    props.insert("field_order".to_string(), Value::String("progressive".to_string()));
    props.insert("scan_type".to_string(), Value::String("progressive".to_string()));
    props.insert("content_type".to_string(), Value::String("progressive".to_string()));
    props.insert("is_sd".to_string(), Value::Bool(false));
    props.insert("is_dvd".to_string(), Value::Bool(false));
    props.insert("duration_ms".to_string(), Value::from(0.0));
    props.insert("frame_count".to_string(), Value::from(0));
    props.insert("width".to_string(), Value::from(1920));
    props.insert("height".to_string(), Value::from(1080));
    props.insert("detection_source".to_string(), Value::String("fallback".to_string()));

    let result = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-select_streams", "v:0",
            "-show_entries", "stream=r_frame_rate,avg_frame_rate,field_order,nb_frames,duration,codec_name,width,height",
            "-show_entries", "format=duration",
            "-show_entries", "stream_side_data=",
            "-of", "json",
            video_path,
        ])
        .output();

    let output = match result {
        Ok(o) => o,
        Err(e) => {
            runner.log_message(&format!("[VideoProps] WARNING: ffprobe failed: {}", e));
            return props;
        }
    };

    if !output.status.success() {
        runner.log_message("[VideoProps] WARNING: ffprobe failed, using defaults");
        return props;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Value = match serde_json::from_str(&stdout) {
        Ok(d) => d,
        Err(_) => {
            runner.log_message("[VideoProps] WARNING: Failed to parse ffprobe JSON");
            return props;
        }
    };

    let streams = data.get("streams").and_then(|s| s.as_array());
    if streams.is_none() || streams.unwrap().is_empty() {
        runner.log_message("[VideoProps] WARNING: No video streams found");
        return props;
    }

    let stream = &streams.unwrap()[0];
    props.insert("detection_source".to_string(), Value::String("ffprobe".to_string()));

    // Parse FPS from r_frame_rate and avg_frame_rate
    let r_frame_rate = stream["r_frame_rate"].as_str().unwrap_or("24000/1001");
    let avg_frame_rate = stream["avg_frame_rate"].as_str().unwrap_or(r_frame_rate);

    let (r_fps, r_fps_fraction) = parse_fps_fraction(r_frame_rate, 23.976);
    let (a_fps, _) = parse_fps_fraction(avg_frame_rate, r_fps);

    // Detect VFR: significant difference between r_frame_rate and avg_frame_rate
    let fps_diff_pct = if r_fps > 0.0 {
        (r_fps - a_fps).abs() / r_fps * 100.0
    } else {
        0.0
    };

    if fps_diff_pct > 1.0 {
        props.insert("is_vfr".to_string(), Value::Bool(true));
        props.insert("fps".to_string(), Value::from(a_fps));
        props.insert("fps_fraction".to_string(), serde_json::json!([(a_fps * 1000.0) as i64, 1000]));
        props.insert("original_fps".to_string(), Value::from(r_fps));
        props.insert("original_fps_fraction".to_string(), serde_json::json!(r_fps_fraction));

        if (r_fps - 23.976).abs() < 0.1 && a_fps > 24.0 && a_fps < 25.0 {
            props.insert("is_soft_telecine".to_string(), Value::Bool(true));
        }
    } else {
        props.insert("fps".to_string(), Value::from(r_fps));
        props.insert("fps_fraction".to_string(), serde_json::json!(r_fps_fraction));
    }

    // Parse resolution
    let width = stream["width"].as_i64().unwrap_or(1920);
    let height = stream["height"].as_i64().unwrap_or(1080);
    props.insert("width".to_string(), Value::from(width));
    props.insert("height".to_string(), Value::from(height));

    // Parse field_order for interlacing detection
    let field_order = stream["field_order"].as_str().unwrap_or("progressive");

    match field_order {
        "tt" | "tb" => {
            props.insert("interlaced".to_string(), Value::Bool(true));
            props.insert("field_order".to_string(), Value::String("tff".to_string()));
            props.insert("scan_type".to_string(), Value::String("interlaced".to_string()));
        }
        "bb" | "bt" => {
            props.insert("interlaced".to_string(), Value::Bool(true));
            props.insert("field_order".to_string(), Value::String("bff".to_string()));
            props.insert("scan_type".to_string(), Value::String("interlaced".to_string()));
        }
        "progressive" => {
            props.insert("interlaced".to_string(), Value::Bool(false));
            props.insert("field_order".to_string(), Value::String("progressive".to_string()));
            props.insert("scan_type".to_string(), Value::String("progressive".to_string()));
        }
        _ => {
            props.insert("field_order".to_string(), Value::String("unknown".to_string()));
        }
    }

    // Parse duration - try stream first, then format
    let duration_str = stream["duration"].as_str();
    if let Some(dur) = duration_str.filter(|s| *s != "N/A") {
        if let Ok(d) = dur.parse::<f64>() {
            props.insert("duration_ms".to_string(), Value::from(d * 1000.0));
        }
    } else {
        // Try format-level duration
        if let Some(format_dur) = data.get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|d| d.as_str())
            .filter(|s| *s != "N/A")
        {
            if let Ok(d) = format_dur.parse::<f64>() {
                props.insert("duration_ms".to_string(), Value::from(d * 1000.0));
            }
        }
    }

    // Parse frame count
    let nb_frames = stream["nb_frames"].as_str();
    if let Some(nf) = nb_frames.filter(|s| *s != "N/A") {
        if let Ok(n) = nf.parse::<i64>() {
            props.insert("frame_count".to_string(), Value::from(n));
        }
    } else {
        let duration_ms = props.get("duration_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let fps = props.get("fps").and_then(|v| v.as_f64()).unwrap_or(23.976);
        if duration_ms > 0.0 && fps > 0.0 {
            let count = (duration_ms * fps / 1000.0) as i64;
            props.insert("frame_count".to_string(), Value::from(count));
        }
    }

    // Detect SD content and DVD characteristics
    props.insert("is_sd".to_string(), Value::Bool(height <= 576));

    let codec = stream["codec_name"].as_str().unwrap_or("");
    let is_dvd_codec = codec == "mpeg2video" || codec == "mpeg1video";

    let is_ntsc_dvd = is_dvd_codec
        && (height == 480 || height == 486)
        && (width == 720 || width == 704);
    let is_pal_dvd = is_dvd_codec
        && (height == 576 || height == 578)
        && (width == 720 || width == 704);
    props.insert("is_dvd".to_string(), Value::Bool(is_ntsc_dvd || is_pal_dvd));
    props.insert("codec_name".to_string(), Value::String(codec.to_string()));

    // MediaInfo detection (MPEG-2 picture header analysis)
    let mi_props = detect_mediainfo_properties(video_path, runner);
    if !mi_props.is_empty() {
        props.insert("detection_source".to_string(), Value::String("ffprobe+mediainfo".to_string()));
        props.insert("mediainfo".to_string(), serde_json::to_value(&mi_props).unwrap_or(Value::Null));

        // Use MediaInfo original_fps when available
        if let Some(mi_orig) = mi_props.get("mi_original_fps").and_then(|v| v.as_f64()) {
            if mi_orig > 0.0 {
                props.insert("original_fps".to_string(), Value::from(mi_orig));

                let orig_frac = if (mi_orig - 23.976).abs() < 0.01 {
                    vec![24000, 1001]
                } else if (mi_orig - 29.970).abs() < 0.01 {
                    vec![30000, 1001]
                } else if (mi_orig - 25.0).abs() < 0.01 {
                    vec![25, 1]
                } else {
                    vec![(mi_orig * 1000.0) as i64, 1000]
                };
                props.insert("original_fps_fraction".to_string(), serde_json::json!(orig_frac));
            }
        }

        // Override VFR / soft-telecine from MediaInfo
        if let Some(mode) = mi_props.get("mi_fps_mode").and_then(|v| v.as_str()) {
            if mode.eq_ignore_ascii_case("VFR") {
                props.insert("is_vfr".to_string(), Value::Bool(true));
                if let Some(scan_order) = mi_props.get("mi_scan_order").and_then(|v| v.as_str()) {
                    if scan_order.to_lowercase().contains("pulldown") {
                        props.insert("is_soft_telecine".to_string(), Value::Bool(true));
                    }
                }
            }
        }
    }

    // Cross-validated content type classification
    let (content_type, detection_confidence) =
        classify_content_type(&props, &mi_props, runner);
    props.insert("content_type".to_string(), Value::String(content_type.clone()));
    props.insert("detection_confidence".to_string(), Value::String(detection_confidence.clone()));

    // Logging
    let fps_val = props.get("fps").and_then(|v| v.as_f64()).unwrap_or(23.976);
    let is_vfr = props.get("is_vfr").and_then(|v| v.as_bool()).unwrap_or(false);
    if is_vfr {
        runner.log_message(&format!("[VideoProps] FPS: {:.3} (VFR)", fps_val));
    } else {
        runner.log_message(&format!("[VideoProps] FPS: {:.3}", fps_val));
    }
    runner.log_message(&format!("[VideoProps] Resolution: {}x{}", width, height));
    runner.log_message(&format!(
        "[VideoProps] Scan: {}, Field order: {}",
        props.get("scan_type").and_then(|v| v.as_str()).unwrap_or("?"),
        props.get("field_order").and_then(|v| v.as_str()).unwrap_or("?")
    ));
    let duration_ms = props.get("duration_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let frame_count = props.get("frame_count").and_then(|v| v.as_i64()).unwrap_or(0);
    runner.log_message(&format!(
        "[VideoProps] Duration: {:.0}ms, Frames: ~{}",
        duration_ms, frame_count
    ));

    let dvd_note = if props.get("is_dvd").and_then(|v| v.as_bool()).unwrap_or(false) {
        " (DVD)"
    } else {
        ""
    };
    let sd_note = if props.get("is_sd").and_then(|v| v.as_bool()).unwrap_or(false)
        && !props.get("is_dvd").and_then(|v| v.as_bool()).unwrap_or(false)
    {
        " (SD)"
    } else {
        ""
    };
    runner.log_message(&format!(
        "[VideoProps] Content type: {}{}{} [confidence: {}]",
        content_type, dvd_note, sd_note, detection_confidence
    ));
    runner.log_message(&format!(
        "[VideoProps] Detection source: {}",
        props.get("detection_source").and_then(|v| v.as_str()).unwrap_or("?")
    ));

    props
}

/// Parse a frame rate string like "24000/1001" into (fps_float, [num, denom]).
fn parse_fps_fraction(rate_str: &str, default: f64) -> (f64, Vec<i64>) {
    if rate_str.contains('/') {
        let parts: Vec<&str> = rate_str.split('/').collect();
        let num: i64 = parts[0].parse().unwrap_or(24000);
        let denom: i64 = parts[1].parse().unwrap_or(1001);
        let fps = if denom != 0 { num as f64 / denom as f64 } else { default };
        (fps, vec![num, denom])
    } else {
        let fps = rate_str.parse().unwrap_or(default);
        (fps, vec![(fps * 1000.0) as i64, 1000])
    }
}

/// Get video properties including resolution.
///
/// Convenience wrapper around detect_video_properties.
pub fn get_video_properties(video_path: &str, runner: &CommandRunner) -> VideoProperties {
    detect_video_properties(video_path, runner)
}

/// Get video duration in milliseconds.
///
/// Convenience function that extracts just the duration from video properties.
pub fn get_video_duration_ms(video_path: &str, runner: &CommandRunner) -> f64 {
    let props = detect_video_properties(video_path, runner);
    props
        .get("duration_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Compare video properties between source and target to determine sync strategy.
pub fn compare_video_properties(
    source_props: &VideoProperties,
    target_props: &VideoProperties,
    runner: &CommandRunner,
) -> HashMap<String, Value> {
    runner.log_message("[VideoProps] ----------------------------------------");
    runner.log_message("[VideoProps] Comparing source vs target properties...");

    let src_type = source_props
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let tgt_type = target_props
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let pair_type = format!("{}+{}", src_type, tgt_type);

    let mut result = HashMap::new();
    result.insert("strategy".to_string(), Value::String("frame-based".to_string()));
    result.insert("pair_type".to_string(), Value::String(pair_type.clone()));
    result.insert("fps_match".to_string(), Value::Bool(true));
    result.insert("fps_ratio".to_string(), Value::from(1.0));
    result.insert("content_type_match".to_string(), Value::Bool(src_type == tgt_type));
    result.insert("needs_scaling".to_string(), Value::Bool(false));
    result.insert("scale_factor".to_string(), Value::from(1.0));
    result.insert("warnings".to_string(), serde_json::json!([]));

    let source_fps = source_props.get("fps").and_then(|v| v.as_f64()).unwrap_or(23.976);
    let target_fps = target_props.get("fps").and_then(|v| v.as_f64()).unwrap_or(23.976);

    runner.log_message(&format!("[VideoProps] Source: {} @ {:.3}fps", src_type, source_fps));
    runner.log_message(&format!("[VideoProps] Target: {} @ {:.3}fps", tgt_type, target_fps));

    // FPS comparison
    let fps_diff_pct = if target_fps > 0.0 {
        (source_fps - target_fps).abs() / target_fps * 100.0
    } else {
        0.0
    };
    let ratio = if target_fps > 0.0 { source_fps / target_fps } else { 1.0 };
    result.insert("fps_ratio".to_string(), Value::from(ratio));

    if fps_diff_pct < 0.1 {
        result.insert("fps_match".to_string(), Value::Bool(true));
        runner.log_message(&format!(
            "[VideoProps] FPS: MATCH ({:.3} ~ {:.3})",
            source_fps, target_fps
        ));
    } else {
        result.insert("fps_match".to_string(), Value::Bool(false));
        runner.log_message(&format!(
            "[VideoProps] FPS: MISMATCH ({:.3} vs {:.3}, diff={:.2}%)",
            source_fps, target_fps, fps_diff_pct
        ));
    }

    // PAL speedup detection
    let mut warnings: Vec<String> = Vec::new();
    if ratio > 1.04 && ratio < 1.05 {
        result.insert("needs_scaling".to_string(), Value::Bool(true));
        result.insert("scale_factor".to_string(), Value::from(target_fps / source_fps));
        result.insert("strategy".to_string(), Value::String("scale".to_string()));
        warnings.push(format!("PAL speedup detected (ratio={:.4})", ratio));
        runner.log_message("[VideoProps] PAL speedup detected");
    } else if ratio > 0.0 && (1.0 / ratio) > 0.95 && (1.0 / ratio) < 0.96 {
        result.insert("needs_scaling".to_string(), Value::Bool(true));
        result.insert("scale_factor".to_string(), Value::from(target_fps / source_fps));
        result.insert("strategy".to_string(), Value::String("scale".to_string()));
        warnings.push("Reverse PAL detected".to_string());
        runner.log_message("[VideoProps] Reverse PAL detected");
    }

    // Pair type strategy
    let content_match = src_type == tgt_type;
    let needs_scaling = result.get("needs_scaling").and_then(|v| v.as_bool()).unwrap_or(false);

    if src_type == "progressive" && tgt_type == "progressive" {
        let fps_match = result.get("fps_match").and_then(|v| v.as_bool()).unwrap_or(true);
        if fps_match {
            result.insert("strategy".to_string(), Value::String("frame-based".to_string()));
            runner.log_message("[VideoProps] Pair: progressive+progressive -> frame-based (CFR)");
        }
    } else if src_type == "interlaced" && tgt_type == "interlaced" {
        let fps_match = result.get("fps_match").and_then(|v| v.as_bool()).unwrap_or(true);
        if fps_match {
            result.insert("strategy".to_string(), Value::String("frame-based".to_string()));
            runner.log_message("[VideoProps] Pair: interlaced+interlaced -> frame-based (same CFR)");
        } else {
            warnings.push("Both interlaced but different FPS - unusual".to_string());
        }
    } else if !content_match && !needs_scaling {
        let is_29_vs_23 = ((source_fps - 29.970).abs() < 0.1 && (target_fps - 23.976).abs() < 0.1)
            || ((target_fps - 29.970).abs() < 0.1 && (source_fps - 23.976).abs() < 0.1);
        if is_29_vs_23 {
            result.insert("strategy".to_string(), Value::String("cross-fps".to_string()));
            runner.log_message(&format!(
                "[VideoProps] Pair: {} -> cross-fps (29.970<->23.976, 5:4 ratio)",
                pair_type
            ));
            warnings.push(
                "Cross-FPS pair detected (29.970<->23.976) - frame mapping not yet implemented"
                    .to_string(),
            );
        } else {
            result.insert("strategy".to_string(), Value::String("timestamp-based".to_string()));
            runner.log_message(&format!(
                "[VideoProps] Pair: {} -> timestamp-based",
                pair_type
            ));
            warnings.push(format!(
                "Mixed content types ({}) with non-standard FPS ratio",
                pair_type
            ));
        }
    } else if pair_type.contains("soft_telecine") {
        result.insert("strategy".to_string(), Value::String("timestamp-based".to_string()));
        runner.log_message(&format!(
            "[VideoProps] Pair: {} -> timestamp-based (VFR involved)",
            pair_type
        ));
        if warnings.is_empty() {
            warnings.push("Soft telecine in pair - VFR timestamp handling needed".to_string());
        }
    }

    result.insert("warnings".to_string(), serde_json::json!(warnings));

    // Detection confidence
    let src_conf = source_props
        .get("detection_confidence")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let tgt_conf = target_props
        .get("detection_confidence")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let conf_order = |c: &str| -> i32 {
        match c {
            "high" => 3,
            "medium" => 2,
            "low" => 1,
            _ => 0,
        }
    };
    let pair_confidence = if conf_order(src_conf) <= conf_order(tgt_conf) {
        src_conf
    } else {
        tgt_conf
    };
    result.insert("detection_confidence".to_string(), Value::String(pair_confidence.to_string()));

    let strategy = result.get("strategy").and_then(|v| v.as_str()).unwrap_or("?");
    runner.log_message(&format!(
        "[VideoProps] Strategy: {} (pair: {}, confidence: {})",
        strategy, pair_type, pair_confidence
    ));
    for warn in &warnings {
        runner.log_message(&format!("[VideoProps] WARNING: {}", warn));
    }
    runner.log_message("[VideoProps] ----------------------------------------");

    result
}
