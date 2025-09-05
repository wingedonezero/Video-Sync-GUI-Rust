// src/core/mkv_utils.rs

use serde::Deserialize;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use xml::reader::{EventReader, XmlEvent};
use xml::writer::{EmitterConfig, EventWriter, XmlEvent as WriteXmlEvent};

use crate::core::process::CommandRunner;

#[derive(Debug, Deserialize, Clone)]
pub struct TrackProperties {
    pub codec_id: Option<String>,
    pub language: Option<String>,
    pub track_name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Track {
    pub id: u64,
    pub r#type: String,
    pub properties: TrackProperties,
}

#[derive(Debug, Deserialize)]
pub struct MkvMergeIdentify {
    pub tracks: Vec<Track>,
}

pub async fn get_stream_info(runner: &CommandRunner, file_path: &str) -> Result<MkvMergeIdentify, String> {
    let result = runner.run("mkvmerge", &["-J", file_path]).await?;
    if result.exit_code != 0 {
        return Err(format!("mkvmerge failed to identify file: {}", file_path));
    }
    serde_json::from_str(&result.stdout).map_err(|e| e.to_string())
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
            "A_FLAC" => "flac", "A_OPUS" => "opus", "A_TRUEHD" => "thd", _ => "bin",
        },
        "subtitles" => match codec_id {
            "S_TEXT/ASS" => "ass", "S_TEXT/SSA" => "ssa", "S_TEXT/UTF8" => "srt",
            "S_HDMV/PGS" => "sup", _ => "sub",
        },
        _ => "bin",
    }
}

pub async fn extract_tracks(runner: &CommandRunner, source_file: &Path, tracks: &[Track], temp_dir: &Path) -> Result<Vec<ExtractedTrack>, String> {
    if tracks.is_empty() { return Ok(vec![]); }

    let mut specs = Vec::new();
    let mut extracted_tracks = Vec::new();

    for track in tracks {
        let ext = ext_for_codec(&track.r#type, track.properties.codec_id.as_deref().unwrap_or(""));
        let out_path = temp_dir.join(format!("{}_track_{}.{}", source_file.file_stem().unwrap().to_string_lossy(), track.id, ext));
        specs.push(format!("{}:{}", track.id, out_path.to_string_lossy()));
        extracted_tracks.push(ExtractedTrack {
            path: out_path,
            original_track: track.clone(),
        });
    }

    let mut args = vec!["tracks", &source_file.to_string_lossy()];
    let specs_str: Vec<_> = specs.iter().map(AsRef::as_ref).collect();
    args.extend_from_slice(&specs_str);

    let result = runner.run("mkvextract", &args).await?;
    if result.exit_code != 0 { return Err("mkvextract failed".to_string()); }

    Ok(extracted_tracks)
}

pub async fn process_chapters(runner: &CommandRunner, ref_file: &str, temp_dir: &Path, shift_ms: i64) -> Result<Option<PathBuf>, String> {
    let result = runner.run("mkvextract", &["chapters", ref_file, "-"]).await?;
    if result.exit_code != 0 || result.stdout.trim().is_empty() {
        return Ok(None);
    }

    let out_path = temp_dir.join(format!("{}_chapters.xml", Path::new(ref_file).file_stem().unwrap().to_string_lossy()));
    let input_xml = result.stdout.trim_start_matches('\u{feff}'); // Remove BOM
    let input = BufReader::new(input_xml.as_bytes());
    let parser = EventReader::new(input);

    let file = File::create(&out_path).map_err(|e| e.to_string())?;
    let mut writer = EmitterConfig::new().perform_indent(true).create_writer(file);
    let shift_ns = shift_ms * 1_000_000;
    let mut in_time_tag = false;
    let mut rename_counter = 0;
    let mut in_display = false;

    for event in parser {
        match event.map_err(|e| e.to_string())? {
            XmlEvent::StartElement { name, .. } => {
                in_time_tag = name.local_name == "ChapterTimeStart" || name.local_name == "ChapterTimeEnd";
                if name.local_name == "ChapterDisplay" {
                    in_display = true;
                    rename_counter += 1;
                    // For renaming, we reconstruct the whole ChapterDisplay block
                    let new_display = format!("Chapter {:02}", rename_counter);
                    writer.write(WriteXmlEvent::start_element("ChapterDisplay")).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::start_element("ChapterString")).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::characters(&new_display)).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::end_element()).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::start_element("ChapterLanguage")).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::characters("und")).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::end_element()).map_err(|e| e.to_string())?;
                    writer.write(WriteXmlEvent::end_element()).map_err(|e| e.to_string())?;
                } else {
                    writer.write(WriteXmlEvent::from(name.clone())).map_err(|e| e.to_string())?;
                }
            }
            XmlEvent::EndElement { name } => {
                in_time_tag = false;
                if name.local_name == "ChapterDisplay" {
                    in_display = false;
                }
                if !in_display {
                    writer.write(WriteXmlEvent::from(name.clone())).map_err(|e| e.to_string())?;
                }
            }
            XmlEvent::Characters(s) => {
                if in_time_tag && shift_ns != 0 {
                    if let Ok(ns) = parse_time_ns(&s) {
                        let new_ns = ns.saturating_add_signed(shift_ns);
                        writer.write(format_time_ns(new_ns)).map_err(|e| e.to_string())?;
                    } else {
                        writer.write(s).map_err(|e| e.to_string())?;
                    }
                } else if !in_display {
                    writer.write(s).map_err(|e| e.to_string())?;
                }
            }
            other_event => {
                if let Some(e) = other_event.as_writer_event() {
                    if !in_display {
                        writer.write(e).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
    }
    Ok(Some(out_path))
}

fn parse_time_ns(t: &str) -> Result<u64, ()> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() != 3 { return Err(()); }
    let s_frac: Vec<&str> = parts[2].split('.').collect();
    if s_frac.len() != 2 { return Err(()); }
    let hh: u64 = parts[0].parse().map_err(|_| ())?;
    let mm: u64 = parts[1].parse().map_err(|_| ())?;
    let ss: u64 = s_frac[0].parse().map_err(|_| ())?;
    let mut frac_str = s_frac[1].to_string();
    if frac_str.len() > 9 { frac_str.truncate(9); }
    else { frac_str.push_str(&"0".repeat(9 - frac_str.len())); }
    let ns: u64 = frac_str.parse().map_err(|_| ())?;
    Ok((hh * 3600 + mm * 60 + ss) * 1_000_000_000 + ns)
}

fn format_time_ns(ns: u64) -> String {
    let total_s = ns / 1_000_000_000;
    let frac_ns = ns % 1_000_000_000;
    let hh = total_s / 3600;
    let mm = (total_s % 3600) / 60;
    let ss = total_s % 60;
    format!("{:02}:{:02}:{:02}.{:09}", hh, mm, ss, frac_ns)
}
