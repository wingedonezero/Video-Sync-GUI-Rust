//! Track extraction — 1:1 port of `vsg_core/extraction/tracks.py`.
//!
//! Handles probing MKV files with mkvmerge -J / ffprobe,
//! building track descriptions, and extracting tracks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::io::runner::CommandRunner;

// ─── Codec ID mapping ────────────────────────────────────────────────────────

/// Maps MKV codec IDs to human-friendly names — `_CODEC_ID_MAP`
fn codec_id_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        // Video
        ("V_MPEGH/ISO/HEVC", "HEVC/H.265"),
        ("V_MPEG4/ISO/AVC", "AVC/H.264"),
        ("V_MPEG1", "MPEG-1"),
        ("V_MPEG2", "MPEG-2"),
        ("V_VP9", "VP9"),
        ("V_AV1", "AV1"),
        // Audio
        ("A_AC3", "AC-3"),
        ("A_EAC3", "E-AC3 / DD+"),
        ("A_DTS", "DTS"),
        ("A_TRUEHD", "TrueHD"),
        ("A_FLAC", "FLAC"),
        ("A_AAC", "AAC"),
        ("A_OPUS", "Opus"),
        ("A_VORBIS", "Vorbis"),
        ("A_PCM/INT/LIT", "PCM"),
        ("A_MS/ACM", "MS-ACM"),
        // Subtitles
        ("S_HDMV/PGS", "PGS"),
        ("S_TEXT/UTF8", "SRT"),
        ("S_TEXT/ASS", "ASS"),
        ("S_TEXT/SSA", "SSA"),
        ("S_VOBSUB", "VobSub"),
    ])
}

// ─── Helper functions ────────────────────────────────────────────────────────

/// Gets a friendly channel layout string — `_get_channel_layout_str`
fn get_channel_layout_str(props: &serde_json::Value) -> Option<String> {
    if let Some(layout) = props.get("channel_layout").and_then(|v| v.as_str()) {
        return Some(layout.to_string());
    }
    if let Some(channels) = props.get("audio_channels").and_then(|v| v.as_i64()) {
        return match channels {
            1 => Some("Mono".to_string()),
            2 => Some("Stereo".to_string()),
            6 => Some("5.1".to_string()),
            8 => Some("7.1".to_string()),
            _ => None,
        };
    }
    None
}

/// Parses an ffprobe PCM codec name like 'pcm_s24le' — `_parse_pcm_codec_name`
fn parse_pcm_codec_name(name: &str) -> Option<String> {
    // Match pcm_([suf])(\d+)([bl]e)?
    let rest = name.strip_prefix("pcm_")?;
    let mut chars = rest.chars();
    let type_char = chars.next()?;
    if !matches!(type_char, 's' | 'u' | 'f') {
        return None;
    }

    // Skip digits
    let remaining: String = chars.collect();
    let endian_part = remaining.trim_start_matches(|c: char| c.is_ascii_digit());

    let type_name = match type_char {
        's' => "Signed",
        'u' => "Unsigned",
        'f' => "Floating Point",
        _ => return None,
    };

    let endian = match endian_part {
        "le" => Some("Little Endian"),
        "be" => Some("Big Endian"),
        _ => None,
    };

    let mut parts = vec![type_name.to_string()];
    if let Some(e) = endian {
        parts.push(e.to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

/// Builds a rich MediaInfo-like description string — `_build_track_description`
fn build_track_description(track: &serde_json::Value) -> String {
    let props = track
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let ttype = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let codec_id = props
        .get("codec_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let ffprobe_info = track
        .get("ffprobe_info")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // --- Base Codec Name ---
    let profile = ffprobe_info
        .get("profile")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let codec_long_name = ffprobe_info
        .get("codec_long_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let map = codec_id_map();

    let friendly_codec = if profile.contains("DTS-HD MA") {
        "DTS-HD MA".to_string()
    } else if profile.contains("DTS-HD HRA") {
        "DTS-HD HRA".to_string()
    } else if codec_long_name.contains("Atmos") {
        "TrueHD / Atmos".to_string()
    } else if codec_id.starts_with("V_MPEG") && !map.contains_key(codec_id) {
        "MPEG".to_string()
    } else {
        map.get(codec_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| codec_id.to_string())
    };

    let lang = props
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("und");
    let track_name = props.get("track_name").and_then(|v| v.as_str());
    let name_part = track_name
        .map(|n| format!(" '{n}'"))
        .unwrap_or_default();

    let mut base_info = format!("{friendly_codec} ({lang}){name_part}");
    let mut details: Vec<String> = Vec::new();

    if ttype == "video" {
        if let (Some(w), Some(h)) = (
            ffprobe_info.get("width").and_then(|v| v.as_i64()),
            ffprobe_info.get("height").and_then(|v| v.as_i64()),
        ) {
            details.push(format!("{w}x{h}"));
        }

        if let Some(fps_str) = ffprobe_info.get("r_frame_rate").and_then(|v| v.as_str()) {
            if fps_str != "0/1" {
                if let Some((num, den)) = fps_str.split_once('/') {
                    if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                        if d != 0.0 {
                            details.push(format!("{:.3} fps", n / d));
                        }
                    }
                }
            }
        }

        if let Some(br) = ffprobe_info.get("bit_rate").and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        }) {
            let mbps = br / 1_000_000.0;
            details.push(format!("{mbps:.1} Mb/s"));
        }

        if !profile.is_empty() {
            let mut profile_str = profile.to_string();
            if let Some(level) = ffprobe_info.get("level").and_then(|v| v.as_i64()) {
                let level_s = level.to_string();
                if level_s.len() > 1 {
                    let chars: Vec<char> = level_s.chars().collect();
                    profile_str.push_str(&format!("@L{}.{}", chars[0], &level_s[1..]));
                } else {
                    profile_str.push_str(&format!("@L{level_s}"));
                }
            }
            details.push(profile_str);
        }

        let color_transfer = ffprobe_info
            .get("color_transfer")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if color_transfer == "smpte2084" {
            details.push("HDR".to_string());
        } else if color_transfer == "arib-std-b67" {
            details.push("HLG".to_string());
        }

        if let Some(side_data) = ffprobe_info
            .get("side_data_list")
            .and_then(|v| v.as_array())
        {
            if side_data.iter().any(|s| {
                s.get("side_data_type")
                    .and_then(|v| v.as_str())
                    == Some("DOVI configuration record")
            }) {
                details.push("Dolby Vision".to_string());
            }
        }
    } else if ttype == "audio" {
        if let Some(br) = ffprobe_info.get("bit_rate").and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| v.as_i64())
        }) {
            let kbps = br / 1000;
            details.push(format!("{kbps} kb/s"));
        }

        if let Some(freq) = props.get("audio_sampling_frequency").and_then(|v| v.as_i64()) {
            details.push(format!("{freq} Hz"));
        }

        if let Some(bits) = props.get("audio_bits_per_sample").and_then(|v| v.as_i64()) {
            details.push(format!("{bits}-bit"));
        }

        if let Some(ch) = props.get("audio_channels").and_then(|v| v.as_i64()) {
            details.push(format!("{ch} ch"));
        }

        let props_value = serde_json::Value::Object(
            props.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        );
        if let Some(layout) = get_channel_layout_str(&props_value) {
            details.push(layout);
        }

        if friendly_codec == "PCM" {
            if let Some(codec_name) = ffprobe_info.get("codec_name").and_then(|v| v.as_str()) {
                if let Some(pcm_details) = parse_pcm_codec_name(codec_name) {
                    base_info.push_str(&format!(" ({pcm_details})"));
                }
            }
        }
    }

    if details.is_empty() {
        base_info
    } else {
        format!("{base_info} | {}", details.join(", "))
    }
}

/// Determines PCM codec from bit depth — `_pcm_codec_from_bit_depth`
fn pcm_codec_from_bit_depth(bit_depth: Option<i64>) -> &'static str {
    let bd = bit_depth.unwrap_or(16);
    if bd >= 64 {
        "pcm_f64le"
    } else if bd >= 32 {
        "pcm_s32le"
    } else if bd >= 24 {
        "pcm_s24le"
    } else {
        "pcm_s16le"
    }
}

/// Determines file extension from codec ID — `_ext_for_codec`
#[allow(clippy::if_same_then_else)]
fn ext_for_codec(ttype: &str, codec_id: &str) -> &'static str {
    let cid = codec_id.to_uppercase();
    match ttype {
        "video" => {
            if cid.contains("V_MPEGH/ISO/HEVC") { "h265" }
            else if cid.contains("V_MPEG4/ISO/AVC") { "h264" }
            else if cid.contains("V_MPEG") { "mpg" }
            else if cid.contains("V_VP9") { "vp9" }
            else if cid.contains("V_AV1") { "av1" }
            else { "bin" }
        }
        "audio" => {
            if cid.contains("A_TRUEHD") { "thd" }
            else if cid.contains("A_EAC3") { "eac3" }
            else if cid.contains("A_AC3") { "ac3" }
            else if cid.contains("A_DTS") { "dts" }
            else if cid.contains("A_AAC") { "aac" }
            else if cid.contains("A_FLAC") { "flac" }
            else if cid.contains("A_OPUS") { "opus" }
            else if cid.contains("A_VORBIS") { "ogg" }
            else if cid.contains("A_PCM") { "wav" }
            else { "bin" }
        }
        "subtitles" => {
            if cid.contains("S_TEXT/ASS") { "ass" }
            else if cid.contains("S_TEXT/SSA") { "ssa" }
            else if cid.contains("S_TEXT/UTF8") { "srt" }
            else if cid.contains("S_HDMV/PGS") { "sup" }
            else if cid.contains("S_VOBSUB") { "sub" }
            else { "sub" }
        }
        _ => "bin",
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Get stream info from mkvmerge -J — `get_stream_info`
pub fn get_stream_info(
    mkv_path: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Option<serde_json::Value> {
    let out = runner.run(&["mkvmerge", "-J", mkv_path], tool_paths)?;
    match serde_json::from_str(&out) {
        Ok(v) => Some(v),
        Err(_) => {
            runner.log_message("[ERROR] Failed to parse mkvmerge -J JSON output.");
            None
        }
    }
}

/// Get stream info including container delays — `get_stream_info_with_delays`
pub fn get_stream_info_with_delays(
    mkv_path: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Option<serde_json::Value> {
    let out = runner.run(&["mkvmerge", "-J", mkv_path], tool_paths)?;
    let mut info: serde_json::Value = match serde_json::from_str(&out) {
        Ok(v) => v,
        Err(_) => {
            runner.log_message("[ERROR] Failed to parse mkvmerge -J JSON output.");
            return None;
        }
    };

    // Extract container delays for each track
    if let Some(tracks) = info.get_mut("tracks").and_then(|v| v.as_array_mut()) {
        for track in tracks.iter_mut() {
            let track_type = track
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let min_timestamp = track
                .get("properties")
                .and_then(|p| p.get("minimum_timestamp"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            // ONLY read container delays for audio and video tracks
            // Subtitles don't have meaningful container delays in MKV
            let container_delay_ms = if matches!(track_type, "audio" | "video") && min_timestamp != 0 {
                // Use round() for proper rounding of negative values
                // int() truncates toward zero: int(-1001.825) = -1001 (wrong)
                // round() rounds to nearest: round(-1001.825) = -1002 (correct)
                (min_timestamp as f64 / 1_000_000.0).round() as i64
            } else {
                0
            };

            track["container_delay_ms"] = serde_json::json!(container_delay_ms);
        }
    }

    Some(info)
}

/// Get detailed stream info from ffprobe — `_get_detailed_stream_info`
pub fn get_detailed_stream_info(
    filepath: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> HashMap<i64, serde_json::Value> {
    let out = runner.run(
        &[
            "ffprobe",
            "-v",
            "error",
            "-show_streams",
            "-of",
            "json",
            filepath,
        ],
        tool_paths,
    );

    let out = match out {
        Some(o) => o,
        None => return HashMap::new(),
    };

    let ffprobe_data: serde_json::Value = match serde_json::from_str(&out) {
        Ok(v) => v,
        Err(_) => {
            runner.log_message("[WARN] Failed to parse ffprobe JSON output.");
            return HashMap::new();
        }
    };

    let mut result = HashMap::new();
    if let Some(streams) = ffprobe_data.get("streams").and_then(|v| v.as_array()) {
        for stream in streams {
            if let Some(index) = stream.get("index").and_then(|v| v.as_i64()) {
                result.insert(index, stream.clone());
            }
        }
    }
    result
}

/// Extracted track record — returned by extract_tracks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedTrack {
    pub id: i32,
    #[serde(rename = "type")]
    pub track_type: String,
    pub lang: String,
    pub name: String,
    pub path: String,
    pub codec_id: String,
    pub source: String,
}

/// Extract tracks from MKV with enhanced error detection — `extract_tracks`
///
/// NOW REPORTS: Which source, which specific track failed, with full details.
pub fn extract_tracks(
    mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    role: &str,
    specific_tracks: Option<&[i32]>,
) -> Result<Vec<ExtractedTrack>, String> {
    let info = get_stream_info(mkv, runner, tool_paths)
        .ok_or_else(|| format!("Could not get stream info for extraction from {mkv}"))?;

    let mut tracks_to_extract: Vec<ExtractedTrack> = Vec::new();
    let mut specs: Vec<String> = Vec::new();

    struct FfmpegJob {
        idx: i32,
        tid: i32,
        out: String,
        pcm: String,
        name: String,
    }
    let mut ffmpeg_jobs: Vec<FfmpegJob> = Vec::new();
    let mut audio_idx: i32 = -1;

    let empty_tracks = vec![];
    let tracks = info
        .get("tracks")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_tracks);

    for track in tracks {
        let ttype = track.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let tid = track.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        if let Some(specific) = specific_tracks {
            if !specific.contains(&tid) {
                // Still count audio index for ffmpeg -map
                if ttype == "audio" {
                    audio_idx += 1;
                }
                continue;
            }
        }

        if ttype == "audio" {
            audio_idx += 1;
        }

        let props = track.get("properties").unwrap_or(&serde_json::Value::Null);
        let codec = props
            .get("codec_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let ext = ext_for_codec(ttype, codec);
        let safe_role = role.replace(' ', "_");
        let mkv_stem = Path::new(mkv)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let out_path = temp_dir.join(format!("{safe_role}_track_{mkv_stem}_{tid}.{ext}"));

        let record = ExtractedTrack {
            id: tid,
            track_type: ttype.to_string(),
            lang: props
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("und")
                .to_string(),
            name: props
                .get("track_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            path: out_path.to_string_lossy().to_string(),
            codec_id: codec.to_string(),
            source: role.to_string(),
        };

        if ttype == "audio" && codec.to_uppercase().contains("A_MS/ACM") {
            let wav_path = out_path.with_extension("wav");
            let bit_depth = props
                .get("audio_bits_per_sample")
                .or_else(|| props.get("bit_depth"))
                .and_then(|v| v.as_i64());
            let pcm_codec = pcm_codec_from_bit_depth(bit_depth);

            let mut record = record;
            record.path = wav_path.to_string_lossy().to_string();
            tracks_to_extract.push(record);

            ffmpeg_jobs.push(FfmpegJob {
                idx: audio_idx,
                tid,
                out: wav_path.to_string_lossy().to_string(),
                pcm: pcm_codec.to_string(),
                name: props
                    .get("track_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        } else {
            specs.push(format!("{}:{}", tid, out_path.display()));
            tracks_to_extract.push(record);
        }
    }

    // === ENHANCED: Extraction with detailed per-track error reporting ===
    if !specs.is_empty() {
        runner.log_message(&format!(
            "[{role}] Extracting {} track(s) with mkvextract...",
            specs.len()
        ));

        let mut mkvextract_args: Vec<&str> = vec!["mkvextract", mkv, "tracks"];
        let spec_refs: Vec<&str> = specs.iter().map(|s| s.as_str()).collect();
        mkvextract_args.extend(&spec_refs);

        let result = runner.run(&mkvextract_args, tool_paths);

        if result.is_none() {
            runner.log_message(&format!("[{role}] [ERROR] mkvextract command failed!"));

            // Check which tracks succeeded/failed
            let mut failed_tracks: Vec<String> = Vec::new();
            let mut successful_tracks: Vec<String> = Vec::new();

            for spec in &specs {
                let parts: Vec<&str> = spec.splitn(2, ':').collect();
                if parts.len() != 2 {
                    continue;
                }
                let tid: i32 = parts[0].parse().unwrap_or(0);
                let out_path = PathBuf::from(parts[1]);
                let track_info = tracks_to_extract.iter().find(|t| t.id == tid);

                let track_name = track_info
                    .map(|t| {
                        if t.name.is_empty() {
                            format!("Track {tid}")
                        } else {
                            t.name.clone()
                        }
                    })
                    .unwrap_or_else(|| format!("Track {tid}"));
                let track_type = track_info
                    .map(|t| capitalize(&t.track_type))
                    .unwrap_or_default();
                let track_lang = track_info
                    .map(|t| t.lang.as_str())
                    .unwrap_or("und");
                let track_codec = track_info
                    .map(|t| t.codec_id.as_str())
                    .unwrap_or("unknown");

                if out_path.exists() {
                    if let Ok(meta) = std::fs::metadata(&out_path) {
                        if meta.len() > 0 {
                            let size_mb = meta.len() as f64 / (1024.0 * 1024.0);
                            successful_tracks.push(format!(
                                "  OK {track_name} (ID {tid}, {track_type}, {track_lang}, {track_codec}) [{size_mb:.1} MB]"
                            ));
                            continue;
                        }
                    }
                    failed_tracks.push(format!(
                        "  FAIL {track_name} (ID {tid}, {track_type}, {track_lang}, {track_codec}) - empty (0 bytes)"
                    ));
                } else {
                    failed_tracks.push(format!(
                        "  FAIL {track_name} (ID {tid}, {track_type}, {track_lang}, {track_codec}) - not created"
                    ));
                }
            }

            let mkv_name = Path::new(mkv)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let sep = "=".repeat(80);
            let mut error_msg = format!("\n{sep}\nEXTRACTION FAILED\n{sep}\n");
            error_msg.push_str(&format!("Source: {role}\n"));
            error_msg.push_str(&format!("File: {mkv_name}\n"));
            error_msg.push_str(&format!("Full Path: {mkv}\n"));
            error_msg.push_str(&format!("{sep}\n\n"));

            if !successful_tracks.is_empty() {
                error_msg.push_str(&format!(
                    "Successfully extracted ({} tracks):\n",
                    successful_tracks.len()
                ));
                error_msg.push_str(&successful_tracks.join("\n"));
                error_msg.push_str("\n\n");
            }

            if !failed_tracks.is_empty() {
                error_msg.push_str(&format!(
                    "FAILED to extract ({} tracks):\n",
                    failed_tracks.len()
                ));
                error_msg.push_str(&failed_tracks.join("\n"));
                error_msg.push_str("\n\n");
                error_msg.push_str(
                    "The track(s) marked FAIL above failed to extract.\n\
                     These specific tracks have issues and need investigation.\n\n",
                );
            } else {
                error_msg.push_str(
                    "All tracks appear extracted, but mkvextract returned an error.\n\
                     This may indicate a warning or non-fatal issue.\n\n",
                );
            }

            error_msg.push_str(
                "Possible causes:\n\
                 - Corrupted track data in the source file\n\
                 - Insufficient disk space in temp directory\n\
                 - Insufficient read/write permissions\n\
                 - Unsupported codec or malformed stream data\n\
                 - Hardware/storage errors (bad sectors)\n\
                 - File system issues (FAT32 4GB limit, etc.)\n\n",
            );

            error_msg.push_str(&format!(
                "Troubleshooting:\n\
                 1. Verify source integrity: mkvmerge -i \"{mkv}\"\n\n\
                 2. Try extracting failed track(s) manually:\n"
            ));
            for line in failed_tracks.iter().take(3) {
                if let Some(id_str) = line.split("ID ").nth(1).and_then(|s| s.split(',').next()) {
                    error_msg.push_str(&format!(
                        "   mkvextract \"{mkv}\" tracks {id_str}:test_track_{id_str}.bin\n"
                    ));
                }
            }
            error_msg.push_str(&format!(
                "\n3. Check disk space in: {}\n\n\
                 4. Try playing source file to check for corruption\n\n\
                 5. Check log file for detailed mkvextract error messages\n{sep}\n",
                temp_dir.display()
            ));

            return Err(error_msg);
        }

        runner.log_message(&format!(
            "[{role}] Successfully extracted {} track(s)",
            specs.len()
        ));

        // Post-extraction verification
        let mut verification_failed: Vec<String> = Vec::new();
        for spec in &specs {
            let parts: Vec<&str> = spec.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }
            let tid: i32 = parts[0].parse().unwrap_or(0);
            let out_path = PathBuf::from(parts[1]);
            let track_info = tracks_to_extract.iter().find(|t| t.id == tid);
            let track_name = track_info
                .map(|t| {
                    if t.name.is_empty() {
                        format!("Track {tid}")
                    } else {
                        t.name.clone()
                    }
                })
                .unwrap_or_else(|| format!("Track {tid}"));

            if !out_path.exists() {
                verification_failed.push(format!(
                    "  - {track_name} (ID {tid}): File not created"
                ));
            } else if let Ok(meta) = std::fs::metadata(&out_path) {
                if meta.len() == 0 {
                    verification_failed.push(format!(
                        "  - {track_name} (ID {tid}): File is empty (0 bytes)"
                    ));
                }
            }
        }

        if !verification_failed.is_empty() {
            let mkv_name = Path::new(mkv)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let sep = "=".repeat(80);
            let mut error_msg = format!(
                "\n{sep}\nPOST-EXTRACTION VERIFICATION FAILED\n{sep}\n"
            );
            error_msg.push_str(&format!("Source: {role}\nFile: {mkv_name}\n{sep}\n\n"));
            error_msg.push_str("Tracks failed verification:\n");
            error_msg.push_str(&verification_failed.join("\n"));
            error_msg.push_str(
                "\n\nmkvextract reported success but some files are missing/empty.\n\
                 This may indicate:\n\
                 - A bug in mkvextract\n\
                 - Filesystem issues (delayed writes, caching)\n\
                 - Antivirus interference\n\
                 - Disk I/O errors\n",
            );
            error_msg.push_str(&format!("{sep}\n"));
            return Err(error_msg);
        }
    }

    // Handle A_MS/ACM audio with ffmpeg
    for job in &ffmpeg_jobs {
        let track_name = if job.name.is_empty() {
            format!("Track {}", job.tid)
        } else {
            job.name.clone()
        };

        runner.log_message(&format!(
            "[{role}] Extracting A_MS/ACM track '{track_name}' (ID {})...",
            job.tid
        ));

        let idx_str = format!("0:a:{}", job.idx);
        let copy_cmd: Vec<&str> = vec![
            "ffmpeg", "-y", "-v", "error", "-nostdin", "-i", mkv, "-map", &idx_str, "-vn",
            "-sn", "-c:a", "copy", &job.out,
        ];

        if runner.run(&copy_cmd, tool_paths).is_none() {
            runner.log_message(&format!(
                "[{role}] Stream copy refused. Falling back to PCM ({})...",
                job.pcm
            ));

            let pcm_cmd: Vec<&str> = vec![
                "ffmpeg", "-y", "-v", "error", "-nostdin", "-i", mkv, "-map", &idx_str, "-vn",
                "-sn", "-acodec", &job.pcm, &job.out,
            ];

            if runner.run(&pcm_cmd, tool_paths).is_none() {
                let mkv_name = Path::new(mkv)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let sep = "=".repeat(80);
                let error_msg = format!(
                    "\n{sep}\n\
                     A_MS/ACM AUDIO EXTRACTION FAILED\n\
                     {sep}\n\
                     Source: {role}\n\
                     File: {mkv_name}\n\
                     Track: {track_name} (ID {})\n\
                     Codec: A_MS/ACM\n\
                     {sep}\n\n\
                     Both stream copy and PCM conversion failed.\n\n\
                     This track may:\n\
                     - Use an unsupported ACM codec variant\n\
                     - Be corrupted or have malformed headers\n\
                     - Require specific codec drivers\n\n\
                     Troubleshooting:\n\
                     1. Try playing this audio track in VLC\n\
                     2. Try: mkvextract \"{mkv}\" tracks {}:test.wav\n\
                     3. Consider remuxing the source file\n\
                     {sep}\n",
                    job.tid, job.tid
                );
                return Err(error_msg);
            }

            runner.log_message(&format!("[{role}] Converted to {}", job.pcm));
        } else {
            runner.log_message(&format!("[{role}] Extracted successfully"));
        }
    }

    Ok(tracks_to_extract)
}

/// Gets track info for the UI dialog — `get_track_info_for_dialog`
pub fn get_track_info_for_dialog(
    sources: &HashMap<String, String>,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> HashMap<String, Vec<serde_json::Value>> {
    let mut all_tracks: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for key in sources.keys() {
        all_tracks.insert(key.clone(), Vec::new());
    }

    for (source_key, filepath) in sources {
        if filepath.is_empty() || !Path::new(filepath).exists() {
            continue;
        }

        let mkvmerge_info = match get_stream_info(filepath, runner, tool_paths) {
            Some(info) => info,
            None => continue,
        };

        let empty_tracks = vec![];
        let mkv_tracks = mkvmerge_info
            .get("tracks")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty_tracks);

        if mkv_tracks.is_empty() {
            continue;
        }

        let ffprobe_details = get_detailed_stream_info(filepath, runner, tool_paths);

        // Group ffprobe streams by type, sorted by index
        let mut ffprobe_by_type: HashMap<&str, Vec<&serde_json::Value>> = HashMap::new();
        let mut sorted_streams: Vec<(&i64, &serde_json::Value)> =
            ffprobe_details.iter().collect();
        sorted_streams.sort_by_key(|(idx, _)| *idx);

        for (_, stream) in &sorted_streams {
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mapped_type = match codec_type {
                "subtitle" => "subtitles",
                other => other,
            };
            ffprobe_by_type
                .entry(mapped_type)
                .or_default()
                .push(stream);
        }

        let mut type_counters: HashMap<String, usize> = HashMap::new();

        for track in mkv_tracks {
            let mut track = track.clone();
            let track_type = track
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let type_index = type_counters.entry(track_type.clone()).or_insert(0);

            if let Some(streams) = ffprobe_by_type.get(track_type.as_str()) {
                if *type_index < streams.len() {
                    track["ffprobe_info"] = streams[*type_index].clone();
                }
            }
            *type_index += 1;

            let props = track.get("properties").cloned().unwrap_or_default();

            let audio_channels = if track_type == "audio" {
                props
                    .get("audio_channels")
                    .and_then(|v| v.as_i64())
                    .map(|c| serde_json::json!(c))
                    .unwrap_or(serde_json::json!(""))
            } else {
                serde_json::json!("")
            };

            let record = serde_json::json!({
                "source": source_key,
                "original_path": filepath,
                "id": track.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                "type": &track_type,
                "codec_id": props.get("codec_id").and_then(|v| v.as_str()).unwrap_or("N/A"),
                "lang": props.get("language").and_then(|v| v.as_str()).unwrap_or("und"),
                "name": props.get("track_name").and_then(|v| v.as_str()).unwrap_or(""),
                "audio_channels": audio_channels,
                "description": build_track_description(&track),
            });

            all_tracks
                .entry(source_key.clone())
                .or_default()
                .push(record);
        }
    }

    all_tracks
}

/// Capitalize first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_for_codec_video() {
        assert_eq!(ext_for_codec("video", "V_MPEGH/ISO/HEVC"), "h265");
        assert_eq!(ext_for_codec("video", "V_MPEG4/ISO/AVC"), "h264");
        assert_eq!(ext_for_codec("video", "V_AV1"), "av1");
    }

    #[test]
    fn ext_for_codec_audio() {
        assert_eq!(ext_for_codec("audio", "A_TRUEHD"), "thd");
        assert_eq!(ext_for_codec("audio", "A_FLAC"), "flac");
        assert_eq!(ext_for_codec("audio", "A_AAC"), "aac");
        assert_eq!(ext_for_codec("audio", "A_PCM/INT/LIT"), "wav");
    }

    #[test]
    fn ext_for_codec_subtitles() {
        assert_eq!(ext_for_codec("subtitles", "S_TEXT/ASS"), "ass");
        assert_eq!(ext_for_codec("subtitles", "S_TEXT/UTF8"), "srt");
        assert_eq!(ext_for_codec("subtitles", "S_HDMV/PGS"), "sup");
    }

    #[test]
    fn pcm_codec_from_bit_depth_works() {
        assert_eq!(pcm_codec_from_bit_depth(Some(16)), "pcm_s16le");
        assert_eq!(pcm_codec_from_bit_depth(Some(24)), "pcm_s24le");
        assert_eq!(pcm_codec_from_bit_depth(Some(32)), "pcm_s32le");
        assert_eq!(pcm_codec_from_bit_depth(Some(64)), "pcm_f64le");
        assert_eq!(pcm_codec_from_bit_depth(None), "pcm_s16le");
    }

    #[test]
    fn parse_pcm_codec_name_works() {
        assert_eq!(
            parse_pcm_codec_name("pcm_s24le"),
            Some("Signed Little Endian".to_string())
        );
        assert_eq!(
            parse_pcm_codec_name("pcm_f32be"),
            Some("Floating Point Big Endian".to_string())
        );
        assert_eq!(parse_pcm_codec_name("not_pcm"), None);
    }

    #[test]
    fn channel_layout_mapping() {
        let props = serde_json::json!({"audio_channels": 6});
        assert_eq!(get_channel_layout_str(&props), Some("5.1".to_string()));

        let props = serde_json::json!({"audio_channels": 2});
        assert_eq!(get_channel_layout_str(&props), Some("Stereo".to_string()));

        let props = serde_json::json!({"channel_layout": "7.1(wide)"});
        assert_eq!(
            get_channel_layout_str(&props),
            Some("7.1(wide)".to_string())
        );
    }

    #[test]
    fn codec_id_map_has_all_entries() {
        let map = codec_id_map();
        assert_eq!(map["V_MPEGH/ISO/HEVC"], "HEVC/H.265");
        assert_eq!(map["A_TRUEHD"], "TrueHD");
        assert_eq!(map["S_TEXT/ASS"], "ASS");
        assert_eq!(map.len(), 21);
    }
}
