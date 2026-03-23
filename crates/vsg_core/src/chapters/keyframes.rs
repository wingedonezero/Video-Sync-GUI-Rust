//! Keyframe probing — 1:1 port of `vsg_core/chapters/keyframes.py`.

use std::collections::HashMap;

use crate::io::runner::CommandRunner;

/// Probe keyframes from video using ffprobe — `probe_keyframes_ns`
///
/// Returns sorted list of keyframe timestamps in nanoseconds.
pub fn probe_keyframes_ns(
    ref_video_path: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Vec<i64> {
    let out = runner.run(
        &[
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time,flags",
            "-of",
            "json",
            ref_video_path,
        ],
        tool_paths,
    );

    let out = match out {
        Some(o) => o,
        None => {
            runner.log_message("[WARN] ffprobe for keyframes produced no output.");
            return Vec::new();
        }
    };

    let data: serde_json::Value = match serde_json::from_str(&out) {
        Ok(v) => v,
        Err(e) => {
            runner.log_message(&format!(
                "[WARN] Could not parse ffprobe keyframe JSON: {e}"
            ));
            return Vec::new();
        }
    };

    let empty = vec![];
    let packets = data
        .get("packets")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut kfs_ns: Vec<i64> = packets
        .iter()
        .filter_map(|p| {
            let pts_time = p.get("pts_time")?.as_str()?;
            let flags = p.get("flags").and_then(|v| v.as_str()).unwrap_or("");
            if !flags.contains('K') {
                return None;
            }
            let secs: f64 = pts_time.parse().ok()?;
            Some((secs * 1_000_000_000.0).round() as i64)
        })
        .collect();

    kfs_ns.sort();
    runner.log_message(&format!(
        "[Chapters] Found {} keyframes for snapping.",
        kfs_ns.len()
    ));
    kfs_ns
}
