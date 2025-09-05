// src/core/mkv_utils.rs

use serde::Deserialize;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use xml::reader::{EventReader, XmlEvent};
use xml::writer::{EmitterConfig, EventWriter, XmlEvent as WriteXmlEvent};

use crate::core::config::AppConfig;
use crate::core::process::CommandRunner;

#[derive(Debug, Deserialize, Clone)]
pub struct TrackProperties {
    pub codec_id: Option<String>,
    pub language: Option<String>,
    pub track_name: Option<String>,
    // Add fields needed for A_MS/ACM fallback
    pub audio_bits_per_sample: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Track {
    pub id: u64,
    pub r#type: String,
    pub properties: TrackProperties,
}

#[derive(Debug, Deserialize)]
pub struct Attachment {
    pub id: u64,
    pub file_name: String,
}

#[derive(Debug, Deserialize)]
pub struct MkvMergeIdentify {
    pub tracks: Vec<Track>,
    pub attachments: Option<Vec<Attachment>>,
}

#[derive(Debug, Deserialize)]
struct FfprobePacket {
    pts_time: String,
    flags: String,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    packets: Vec<FfprobePacket>,
}

pub async fn get_stream_info(runner: &CommandRunner, file_path: &str) -> Result<MkvMergeIdentify, String> {
    let result = runner.run("mkvmerge", &["-J", file_path]).await?;
    if result.exit_code != 0 {
        return Err(format!("mkvmerge failed to identify file: {}", file_path));
    }
    serde_json::from_str(&result.stdout).map_err(|e| format!("Failed to parse mkvmerge JSON: {}", e))
}

pub async fn get_audio_stream_index(runner: &CommandRunner, file_path: &str) -> Result<Option<usize>, String> {
    let info = get_stream_info(runner, file_path).await?;
    let audio_track_pos = info.tracks.iter().position(|t| t.r#type == "audio");
    Ok(audio_track_pos)
}

pub async fn get_duration_s(runner: &CommandRunner, file_path: &str) -> Result<f64, String> {
    let result = runner.run("ffprobe", &["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", file_path]).await?;
    if result.exit_code != 0 {
        return Err("ffprobe failed to get duration".to_string());
    }
    result.stdout.trim().parse::<f64>().map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
pub struct ExtractedTrack {
    pub path: PathBuf,
    pub original_track: Track,
}

fn ext_for_codec(ttype: &str, codec_id: &str) -> &'static str {
    match ttype {
        "video" => "bin",
        "audio" => match codec_id {
            "A_AAC" => "aac", "A_AC3" => "ac3", "A_EAC3" => "eac3", "A_DTS" => "dts",
            "A_FLAC" => "flac", "A_OPUS" => "opus", "A_TRUEHD" => "thd", "A_VORBIS" => "ogg",
            "A_PCM" => "wav", "A_MS/ACM" => "wav", _ => "bin",
        },
        "subtitles" => match codec_id {
            "S_TEXT/ASS" => "ass", "S_TEXT/SSA" => "ssa", "S_TEXT/UTF8" => "srt",
            "S_HDMV/PGS" => "sup", "S_VOBSUB" => "sub", _ => "sub",
        },
        _ => "bin",
    }
}

fn pcm_codec_from_bit_depth(bit_depth: Option<u32>) -> &'static str {
    match bit_depth.unwrap_or(16) {
        bd if bd >= 64 => "pcm_f64le",
        bd if bd >= 32 => "pcm_s32le",
        bd if bd >= 24 => "pcm_s24le",
        _ => "pcm_s16le",
    }
}

pub async fn extract_tracks(runner: &CommandRunner, source_file: &Path, tracks: &[Track], temp_dir: &Path, role: &str) -> Result<Vec<ExtractedTrack>, String> {
    if tracks.is_empty() { return Ok(vec![]); }

    let mut mkvextract_specs = Vec::new();
    let mut ffmpeg_jobs = Vec::new();
    let mut extracted_tracks = Vec::new();
    let mut audio_idx_counter = -1;
    let total_audio_tracks = tracks.iter().filter(|t| t.r#type == "audio").count();

    let all_source_tracks = get_stream_info(runner, &source_file.to_string_lossy()).await?.tracks;
    let mut current_audio_idx = -1;

    for (i, track_info) in all_source_tracks.iter().enumerate() {
        if track_info.r#type == "audio" {
            current_audio_idx += 1;
        }

        if let Some(track_to_extract) = tracks.iter().find(|t| t.id == track_info.id) {
            let codec = track_info.properties.codec_id.as_deref().unwrap_or("");
            let ext = ext_for_codec(&track_info.r#type, codec);
            let out_path = temp_dir.join(format!("{}_track_{}_{}.{}", role, source_file.file_stem().unwrap().to_string_lossy(), track_info.id, ext));

            extracted_tracks.push(ExtractedTrack {
                path: out_path.clone(),
                                  original_track: track_to_extract.clone(),
            });

            if track_info.r#type == "audio" && codec == "A_MS/ACM" {
                ffmpeg_jobs.push((current_audio_idx, out_path, track_info.clone()));
            } else {
                mkvextract_specs.push(format!("{}:{}", track_info.id, out_path.to_string_lossy()));
            }
        }
    }

    if !mkvextract_specs.is_empty() {
        let mut args = vec!["tracks", &source_file.to_string_lossy()];
        let specs_str: Vec<_> = mkvextract_specs.iter().map(AsRef::as_ref).collect();
        args.extend_from_slice(&specs_str);
        runner.run("mkvextract", &args).await?;
    }

    for (idx, path, track) in ffmpeg_jobs {
        let copy_cmd = runner.run("ffmpeg", &["-y", "-v", "error", "-i", &source_file.to_string_lossy(), "-map", &format!("0:a:{}", idx), "-c:a", "copy", &path.to_string_lossy()]).await?;
        if copy_cmd.exit_code != 0 {
            runner.send_log(&format!("[WARN] A_MS/ACM stream copy failed for track {}. Falling back to PCM encode.", track.id)).await;
            let pcm_codec = pcm_codec_from_bit_depth(track.properties.audio_bits_per_sample);
            runner.run("ffmpeg", &["-y", "-v", "error", "-i", &source_file.to_string_lossy(), "-map", &format!("0:a:{}", idx), "-acodec", pcm_codec, &path.to_string_lossy()]).await?;
        }
    }

    Ok(extracted_tracks)
}

pub async fn extract_attachments(runner: &CommandRunner, source_file: &str, temp_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let info = get_stream_info(runner, source_file).await?;
    let attachments = match info.attachments {
        Some(a) => a,
        None => return Ok(vec![]),
    };

    let mut specs = Vec::new();
    let mut out_paths = Vec::new();
    for attachment in attachments {
        let out_path = temp_dir.join(&attachment.file_name);
        specs.push(format!("{}:{}", attachment.id, out_path.to_string_lossy()));
        out_paths.push(out_path);
    }

    if !specs.is_empty() {
        let mut args = vec!["attachments", source_file];
        let specs_str: Vec<_> = specs.iter().map(AsRef::as_ref).collect();
        args.extend_from_slice(&specs_str);
        runner.run("mkvextract", &args).await?;
    }

    Ok(out_paths)
}


pub async fn process_chapters(runner: &CommandRunner, ref_file: &str, temp_dir: &Path, shift_ms: i64, config: &AppConfig) -> Result<Option<PathBuf>, String> {
    let result = runner.run("mkvextract", &["chapters", ref_file, "-"]).await?;
    if result.exit_code != 0 || result.stdout.trim().is_empty() {
        return Ok(None);
    }

    // The rest of this function will be implemented in the next batch to keep this update manageable.
    // For now, it just extracts and saves the original chapters.
    let out_path = temp_dir.join(format!("{}_chapters_processed.xml", Path::new(ref_file).file_stem().unwrap().to_string_lossy()));
    fs::write(&out_path, &result.stdout).map_err(|e| e.to_string())?;

    Ok(Some(out_path))
}
