// src/core/mkv_utils/extract.rs

use super::codecs::{ext_for_codec, pcm_codec_from_bit_depth};
use super::streams::{get_stream_info, MkvAttachment};
use crate::core::command_runner::CommandRunner;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ExtractedTrack {
    pub id: i64,
    #[serde(rename = "type")]
    pub ttype: String,
    pub lang: String,
    pub name: String,
    pub path: String,      // extracted file path
    pub codec_id: String,
    pub source: String,    // "REF"|"SEC"|"TER"
}

/// Extract specified track types from an MKV file (Python parity).
/// - When `specific_tracks` is provided, only those mkvmerge IDs are extracted.
/// - For A_MS/ACM audio: attempt ffmpeg stream copy; on failure, re-encode to PCM according to bit depth.
/// - Non-ACM tracks are extracted with `mkvextract tracks`.
pub fn extract_tracks(
    mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    role: &str,
    audio: bool,
    subs: bool,
    all_tracks: bool,
    specific_tracks: Option<&[i64]>,
) -> Vec<ExtractedTrack> {
    let info_val = match get_stream_info(mkv, runner) {
        Some(v) => v,
        None => {
            runner.log(&format!("Could not get stream info for extraction from {}", mkv));
            return vec![];
        }
    };

    // Flatten track info
    let tracks = info_val
    .get("tracks")
    .and_then(|t| t.as_array())
    .cloned()
    .unwrap_or_default();

    let mut tracks_to_extract: Vec<ExtractedTrack> = vec![];
    let mut specs: Vec<(i64, PathBuf)> = vec![]; // for mkvextract
    let mut ffmpeg_jobs: Vec<(usize, i64, PathBuf, String)> = vec![]; // (audio_idx, tid, out, pcm_codec)

    let mut audio_idx: i64 = -1;

    for t in tracks {
        let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let tid = t.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);

        let want = if let Some(specific) = specific_tracks {
            specific.contains(&tid)
        } else {
            all_tracks || (audio && ttype == "audio") || (subs && ttype == "subtitles")
        };
        if !want { continue; }

        if ttype == "audio" {
            audio_idx += 1;
        }

        let props = t.get("properties").cloned().unwrap_or(Value::Null);
        let codec = props.get("codec_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let lang = props.get("language").and_then(|v| v.as_str()).unwrap_or("und").to_string();
        let name = props.get("track_name").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let stem = Path::new(mkv).file_stem().unwrap_or_default().to_string_lossy().to_string();
        let mut out_path = temp_dir.join(format!("{}_track_{}_{}.{}", role, stem, tid, ext_for_codec(ttype, &codec)));

        let mut record = ExtractedTrack{
            id: tid, ttype: ttype.to_string(), lang, name,
            path: out_path.to_string_lossy().to_string(),
            codec_id: codec.clone(),
            source: role.to_uppercase(),
        };

        // Special handling for A_MS/ACM
        if ttype == "audio" && codec.to_uppercase().contains("A_MS/ACM") {
            out_path.set_extension("wav");
            record.path = out_path.to_string_lossy().to_string();

            // choose PCM codec by audio_bits_per_sample or bit_depth
            let bit_depth = props.get("audio_bits_per_sample").and_then(|v| v.as_i64())
            .or_else(|| props.get("bit_depth").and_then(|v| v.as_i64()));
            let pcm = pcm_codec_from_bit_depth(bit_depth).to_string();

            // attempt stream copy first
            let idx = audio_idx.max(0) as usize;
            ffmpeg_jobs.push((idx, tid, out_path.clone(), pcm));
        } else {
            specs.push((tid, out_path.clone()));
        }

        tracks_to_extract.push(record);
    }

    // mkvextract for non-ACM items
    if !specs.is_empty() {
        // build "tid:path" list
        let mut cmd = vec!["mkvextract".to_string(), mkv.to_string(), "tracks".to_string()];
        for (tid, path) in &specs {
            cmd.push(format!("{}:{}", tid, path.to_string_lossy()));
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        let _ = runner.run(&refs);
    }

    // ffmpeg jobs for ACM: try copy, else PCM
    for (audio_index, tid, out, pcm_codec) in ffmpeg_jobs {
        let copy_cmd = [
            "ffmpeg", "-y", "-v", "error", "-nostdin", "-i", mkv,
            "-map", &format!("0:a:{}", audio_index), "-vn", "-sn", "-c:a", "copy",
            out.to_string_lossy().as_ref()
        ];
        runner.log(&format!("Attempting stream copy for A_MS/ACM (track {}) -> {}", tid, out.file_name().unwrap_or_default().to_string_lossy()));
        if runner.run(&copy_cmd).is_some() {
            runner.log(&format!("Stream copy succeeded for A_MS/ACM (track {})", tid));
        } else {
            runner.log(&format!("Stream copy refused for A_MS/ACM (track {}). Falling back to {}.", tid, pcm_codec));
            let pcm_cmd = [
                "ffmpeg", "-y", "-v", "error", "-nostdin", "-i", mkv,
                "-map", &format!("0:a:{}", audio_index), "-vn", "-sn", "-acodec", &pcm_codec,
                out.to_string_lossy().as_ref()
            ];
            let _ = runner.run(&pcm_cmd);
        }
    }

    tracks_to_extract
}
