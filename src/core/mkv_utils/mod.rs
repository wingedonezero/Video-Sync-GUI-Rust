// src/core/mkv_utils/mod.rs

pub mod codecs;
pub mod streams;
pub mod extract;
pub mod attachments;
pub mod chapters;

pub use attachments::extract_attachments;
pub use chapters::process_chapters;
pub use codecs::{ext_for_codec, pcm_codec_from_bit_depth};
pub use extract::{extract_tracks, ExtractedTrack};
pub use streams::{get_stream_info, get_track_info_for_dialog, TrackDialogInfo};

// chapters.rs will come in Step 3B

use crate::core::command_runner::CommandRunner;
use crate::core::mkv_utils::codecs::{ext_for_codec, pcm_codec_from_bit_depth};
use crate::types::tracks::{ExtractedTrack, TrackDialogInfo};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Run `mkvmerge -J` and return raw JSON (or None on failure).
pub fn get_stream_info(mkv_path: &str, runner: &CommandRunner) -> Option<Value> {
    let out = runner.run(&["mkvmerge", "-J", mkv_path])?;
    serde_json::from_str::<Value>(&out).ok()
}

/// Build REF/SEC/TER track inventories for the manual selection dialog.
pub fn get_track_info_for_dialog(
    ref_file: &str,
    sec_file: Option<&str>,
    ter_file: Option<&str>,
    runner: &CommandRunner,
) -> HashMap<String, Vec<TrackDialogInfo>> {
    let mut all: HashMap<String, Vec<TrackDialogInfo>> =
    HashMap::from([("REF".into(), vec![]), ("SEC".into(), vec![]), ("TER".into(), vec![])]);

    for (source, file_opt) in [
        ("REF", Some(ref_file)),
        ("SEC", sec_file),
        ("TER", ter_file),
    ] {
        if let Some(file) = file_opt {
            if !Path::new(file).exists() {
                continue;
            }
            if let Some(info) = get_stream_info(file, runner) {
                if let Some(tracks) = info.get("tracks").and_then(|v| v.as_array()) {
                    for t in tracks {
                        let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let id = t.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                        let props = t.get("properties").cloned().unwrap_or(Value::Null);
                        let (lang, name, codec_id) = parse_track_props(&props);
                        all.get_mut(source).unwrap().push(TrackDialogInfo {
                            source: source.to_string(),
                                                          original_path: file.to_string(),
                                                          id,
                                                          r#type: ttype.to_string(),
                                                          codec_id,
                                                          lang,
                                                          name,
                        });
                    }
                }
            }
        }
    }
    all
}

/// Extract specified tracks from an MKV into temp_dir.
/// - If `specific_tracks` is Some, extract only those track IDs.
/// - Otherwise, when `all_tracks` true, extract all audio+subs.
/// - Otherwise, gate by `audio` / `subs` booleans.
/// For A_MS/ACM audio, attempt ffmpeg stream copy; on failure, encode PCM per bit depth.
#[allow(clippy::too_many_arguments)]
pub fn extract_tracks(
    mkv: &str,
    temp_dir: &Path,
    runner: &CommandRunner,
    role: &str,                       // "ref" | "sec" | "ter"
    audio: bool,
    subs: bool,
    all_tracks: bool,
    specific_tracks: Option<&[i64]>,
) -> Result<Vec<ExtractedTrack>, String> {
    let info = get_stream_info(mkv, runner).ok_or_else(|| {
        format!("Could not get stream info for extraction from {}", mkv)
    })?;

    let tracks = info
    .get("tracks")
    .and_then(|v| v.as_array())
    .cloned()
    .unwrap_or_default();

    let mut tracks_to_extract: Vec<ExtractedTrack> = vec![];
    let mut specs: Vec<(i64, PathBuf)> = vec![];
    let mut ffmpeg_jobs: Vec<FfmpegJob> = vec![];
    let mut audio_idx: i32 = -1;

    for t in tracks {
        let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let id = t.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);

        let want = if let Some(ids) = specific_tracks {
            ids.contains(&id)
        } else if all_tracks {
            true
        } else {
            (audio && ttype == "audio") || (subs && ttype == "subtitles")
        };

        if !want {
            continue;
        }

        if ttype == "audio" {
            // NOTE: parity with Python — this counts only selected audio tracks.
            // If earlier audio tracks were skipped, index may not match ffmpeg's logical index,
            // but we reproduce the original behavior exactly.
            audio_idx += 1;
        }

        let props = t.get("properties").cloned().unwrap_or(Value::Null);
        let (lang, name, codec_id) = parse_track_props(&props);
        let ext = ext_for_codec(ttype, &codec_id);

        let out_path = temp_dir.join(format!(
            "{}_track_{}_{}.{}",
            role.to_ascii_lowercase(),
                                             Path::new(mkv).file_stem().unwrap_or_default().to_string_lossy(),
                                             id,
                                             ext
        ));

        let mut rec = ExtractedTrack {
            id,
            r#type: ttype.to_string(),
            lang: if lang.is_empty() { "und".into() } else { lang },
            name,
            path: out_path.to_string_lossy().to_string(),
            codec_id: codec_id.clone(),
            source: role.to_ascii_uppercase(),
        };

        // A_MS/ACM → prefer stream copy; if refused, transcode to PCM_* based on bit depth
        if ttype == "audio" && codec_id.to_ascii_uppercase().contains("A_MS/ACM") {
            let mut wav_path = out_path.clone();
            wav_path.set_extension("wav");
            rec.path = wav_path.to_string_lossy().to_string();

            let bit_depth = props
            .get("audio_bits_per_sample")
            .and_then(|v| v.as_i64())
            .or_else(|| props.get("bit_depth").and_then(|v| v.as_i64()));

            let pcm = pcm_codec_from_bit_depth(bit_depth);
            ffmpeg_jobs.push(FfmpegJob {
                audio_idx,
                tid: id,
                out: wav_path,
                pcm: pcm.to_string(),
            });
        } else {
            specs.push((id, out_path));
        }

        tracks_to_extract.push(rec);
    }

    // mkvextract segment for non-ACM tracks
    if !specs.is_empty() {
        let mut cmd: Vec<String> = vec!["mkvextract".into(), mkv.into(), "tracks".into()];
        for (tid, path) in &specs {
            cmd.push(format!("{}:{}", tid, path.to_string_lossy()));
        }
        let args: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        let _ = runner.run(&args);
    }

    // ffmpeg jobs for A_MS/ACM
    for job in ffmpeg_jobs {
        let copy_cmd = [
            "ffmpeg",
            "-y",
            "-v",
            "error",
            "-nostdin",
            "-i",
            mkv,
            "-map",
            &format!("0:a:{}", job.audio_idx),
            "-vn",
            "-sn",
            "-c:a",
            "copy",
            job.out.to_string_lossy().as_ref(),
        ];
        runner.log(&format!(
            "Attempting stream copy for A_MS/ACM (track {}) -> {}",
                            job.tid,
                            job.out.file_name().unwrap_or_default().to_string_lossy()
        ));
        if runner.run(&copy_cmd).is_some() {
            runner.log(&format!(
                "Stream copy succeeded for A_MS/ACM (track {})",
                                job.tid
            ));
        } else {
            runner.log(&format!(
                "Stream copy refused for A_MS/ACM (track {}). Falling back to {}.",
                                job.tid, job.pcm
            ));
            let pcm_cmd = [
                "ffmpeg",
                "-y",
                "-v",
                "error",
                "-nostdin",
                "-i",
                mkv,
                "-map",
                &format!("0:a:{}", job.audio_idx),
                "-vn",
                "-sn",
                "-acodec",
                &job.pcm,
                job.out.to_string_lossy().as_ref(),
            ];
            let _ = runner.run(&pcm_cmd);
        }
    }

    Ok(tracks_to_extract)
}

#[derive(Debug)]
struct FfmpegJob {
    audio_idx: i32, // note: parity with Python (index among selected audio)
    tid: i64,
    out: PathBuf,
    pcm: String,
}

fn parse_track_props(props: &Value) -> (String, String, String) {
    // Return (lang, name, codec_id)
    let mut lang = "und".to_string();
    let mut name = String::new();
    let mut codec_id = String::new();

    if let Some(obj) = props.as_object() {
        if let Some(v) = obj.get("language").and_then(|v| v.as_str()) {
            lang = v.to_string();
        }
        if let Some(v) = obj.get("track_name").and_then(|v| v.as_str()) {
            name = v.to_string();
        }
        if let Some(v) = obj.get("codec_id").and_then(|v| v.as_str()) {
            codec_id = v.to_string();
        }
    }
    (lang, name, codec_id)
}
