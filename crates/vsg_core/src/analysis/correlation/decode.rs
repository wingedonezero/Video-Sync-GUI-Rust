//! Audio decoding — 1:1 port of `correlation/decode.py`.

use std::collections::HashMap;
use std::path::Path;

use crate::io::runner::CommandRunner;

// ── Language Normalization ───────────────────────────────────────────────────

/// 2-letter to 3-letter ISO 639 mapping.
fn lang2to3() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("en", "eng"), ("ja", "jpn"), ("jp", "jpn"), ("zh", "zho"), ("cn", "zho"),
        ("es", "spa"), ("de", "deu"), ("fr", "fra"), ("it", "ita"), ("pt", "por"),
        ("ru", "rus"), ("ko", "kor"), ("ar", "ara"), ("tr", "tur"), ("pl", "pol"),
        ("nl", "nld"), ("sv", "swe"), ("no", "nor"), ("fi", "fin"), ("da", "dan"),
        ("cs", "ces"), ("sk", "slk"), ("sl", "slv"), ("hu", "hun"), ("el", "ell"),
        ("he", "heb"), ("id", "ind"), ("vi", "vie"), ("th", "tha"), ("hi", "hin"),
        ("ur", "urd"), ("fa", "fas"), ("uk", "ukr"), ("ro", "ron"), ("bg", "bul"),
        ("sr", "srp"), ("hr", "hrv"), ("ms", "msa"), ("bn", "ben"), ("ta", "tam"),
        ("te", "tel"),
    ])
}

/// Normalize a 2-letter language code to 3-letter ISO 639-2 — `normalize_lang`
pub fn normalize_lang(lang: Option<&str>) -> Option<String> {
    let lang = lang?;
    let s = lang.trim().to_lowercase();
    if s.is_empty() || s == "und" {
        return None;
    }
    if s.len() == 2 {
        let map = lang2to3();
        Some(map.get(s.as_str()).unwrap_or(&s.as_str()).to_string())
    } else {
        Some(s)
    }
}

// ── Stream Selection ─────────────────────────────────────────────────────────

/// Find the best audio stream — `get_audio_stream_info`
///
/// Returns (stream_index, track_id) or (None, None).
pub fn get_audio_stream_info(
    mkv_path: &str,
    lang: Option<&str>,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> (Option<i32>, Option<i32>) {
    let out = match runner.run(&["mkvmerge", "-J", mkv_path], tool_paths) {
        Some(o) => o,
        None => return (None, None),
    };

    let info: serde_json::Value = match serde_json::from_str(&out) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    let empty = vec![];
    let tracks = info.get("tracks").and_then(|v| v.as_array()).unwrap_or(&empty);
    let audio_tracks: Vec<&serde_json::Value> = tracks
        .iter()
        .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("audio"))
        .collect();

    if audio_tracks.is_empty() {
        return (None, None);
    }

    if let Some(lang) = lang {
        for (i, t) in audio_tracks.iter().enumerate() {
            let track_lang = t
                .get("properties")
                .and_then(|p| p.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if track_lang == lang {
                let tid = t.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                return (Some(i as i32), Some(tid));
            }
        }
    }

    // Fallback to first audio track
    let first = audio_tracks[0];
    let tid = first.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    (Some(0), Some(tid))
}

// ── Audio Decoding ───────────────────────────────────────────────────────────

/// Default sample rate for all correlation work.
pub const DEFAULT_SR: i64 = 48000;

/// Decode one audio stream to a mono f32 array — `decode_audio`
pub fn decode_audio(
    file_path: &str,
    stream_index: i32,
    sr: i64,
    use_soxr: bool,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Result<Vec<f32>, String> {
    let map_arg = format!("0:a:{stream_index}");
    let sr_str = sr.to_string();

    let mut cmd: Vec<&str> = vec![
        "ffmpeg", "-nostdin", "-v", "error", "-i", file_path, "-map", &map_arg,
    ];

    if use_soxr {
        cmd.extend(&["-resampler", "soxr"]);
    }

    cmd.extend(&["-ac", "1", "-ar", &sr_str, "-f", "f32le", "-"]);

    let pcm_bytes = runner
        .run_binary(&cmd, tool_paths, None)
        .ok_or_else(|| format!("ffmpeg decode failed for {}", Path::new(file_path).file_name().unwrap_or_default().to_string_lossy()))?;

    runner.log_message(&format!(
        "[DECODE RAW] Received {} bytes for {}",
        pcm_bytes.len(),
        Path::new(file_path).file_name().unwrap_or_default().to_string_lossy()
    ));

    // Ensure buffer size is a multiple of 4 (f32 element size)
    let element_size = 4;
    let aligned_size = (pcm_bytes.len() / element_size) * element_size;
    let pcm_bytes = if aligned_size != pcm_bytes.len() {
        let trimmed = pcm_bytes.len() - aligned_size;
        runner.log_message(&format!(
            "[BUFFER ALIGNMENT] Trimmed {trimmed} bytes from {}",
            Path::new(file_path).file_name().unwrap_or_default().to_string_lossy()
        ));
        &pcm_bytes[..aligned_size]
    } else {
        &pcm_bytes
    };

    // Convert bytes to f32 samples
    let samples: Vec<f32> = pcm_bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Ok(samples)
}
