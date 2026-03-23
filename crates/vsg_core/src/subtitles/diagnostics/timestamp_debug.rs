//! Timestamp debugging utilities for ASS/SSA subtitle files — 1:1 port of `diagnostics/timestamp_debug.py`.
//!
//! These functions read raw timestamp strings from subtitle files for comparison
//! with parsed values, helping diagnose timing precision issues.

use std::path::Path;

use regex::Regex;

/// Read raw timestamp strings from an ASS file without full parsing.
///
/// Returns list of (start_str, end_str, style) tuples for first N events.
/// Reads both Dialogue and Comment lines to match SubtitleData.events order.
/// Used for diagnostics to compare original file timestamps with parsed values.
pub fn read_raw_ass_timestamps(
    file_path: &Path,
    max_events: usize,
) -> Vec<(String, String, String)> {
    let mut results = Vec::new();

    // Try to read with different encodings
    let encodings = ["utf-8", "utf-16", "windows-1252", "iso-8859-1"];
    let mut content = None;

    for _enc in &encodings {
        match std::fs::read_to_string(file_path) {
            Ok(c) => {
                // Strip BOM if present
                let c = c.strip_prefix('\u{feff}').unwrap_or(&c).to_string();
                content = Some(c);
                break;
            }
            Err(_) => continue,
        }
    }
    // For non-UTF-8, try reading as bytes and converting
    if content.is_none() {
        if let Ok(bytes) = std::fs::read(file_path) {
            // Try latin1 (always succeeds)
            content = Some(bytes.iter().map(|&b| b as char).collect());
        }
    }

    let content = match content {
        Some(c) => c,
        None => return results,
    };

    // Pattern: Dialogue/Comment: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text
    let pattern = Regex::new(
        r"^(?:Dialogue|Comment):\s*\d+,(\d+:\d+:\d+\.\d+),(\d+:\d+:\d+\.\d+),([^,]*),",
    )
    .unwrap();

    for line in content.lines() {
        if results.len() >= max_events {
            break;
        }
        if let Some(caps) = pattern.captures(line) {
            let start_str = caps.get(1).map(|m: regex::Match| m.as_str().to_string()).unwrap_or_default();
            let end_str = caps.get(2).map(|m: regex::Match| m.as_str().to_string()).unwrap_or_default();
            let style = caps.get(3).map(|m: regex::Match| m.as_str().to_string()).unwrap_or_default();
            results.push((start_str, end_str, style));
        }
    }

    results
}

/// Check the precision of a timestamp string (number of fractional digits).
///
/// Standard ASS uses centiseconds (2 digits: "0:00:00.00").
/// Some tools may output milliseconds (3 digits: "0:00:00.000").
///
/// Returns number of fractional digits.
pub fn check_timestamp_precision(timestamp_str: &str) -> usize {
    if let Some(dot_pos) = timestamp_str.rfind('.') {
        timestamp_str.len() - dot_pos - 1
    } else {
        2 // Default assumption
    }
}

/// Parse ASS timestamp string to float ms (same logic as SubtitleData).
pub fn parse_ass_time_str(time_str: &str) -> f64 {
    let parts: Vec<&str> = time_str.trim().splitn(3, ':').collect();
    if parts.len() != 3 {
        return 0.0;
    }

    let hours: i64 = parts[0].parse().unwrap_or(0);
    let minutes: i64 = parts[1].parse().unwrap_or(0);

    let seconds_cs: Vec<&str> = parts[2].splitn(2, '.').collect();
    let seconds: i64 = seconds_cs[0].parse().unwrap_or(0);
    let centiseconds: i64 = if seconds_cs.len() > 1 {
        seconds_cs[1].parse().unwrap_or(0)
    } else {
        0
    };

    (hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + centiseconds * 10) as f64
}
