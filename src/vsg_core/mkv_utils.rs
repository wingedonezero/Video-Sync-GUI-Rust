// src/vsg_core/mkv_utils.rs

use crate::config::Config;
use crate::process;
use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use xml::{
    reader::{EventReader, XmlEvent as XmlReadEvent},
    writer::{EmitterConfig, EventWriter, XmlEvent as XmlWriteEvent},
};

// --- Data Structures ---

#[derive(Deserialize, Debug, Clone)]
pub struct TrackProperties {
    pub language: Option<String>,
    pub track_name: Option<String>,
    pub codec_id: Option<String>,
    pub audio_bits_per_sample: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Track {
    pub id: u64,
    #[serde(rename = "type")]
    pub track_type: String,
    pub properties: TrackProperties,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MkvMergeAttachment {
    pub id: u64,
    pub file_name: String,
}

#[derive(Deserialize, Debug)]
struct MkvMergeOutput {
    tracks: Vec<Track>,
    attachments: Vec<MkvMergeAttachment>,
}

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub source: String,
    pub id: u64,
    pub track_type: String,
    pub codec_id: String,
    pub lang: String,
    pub name: String,
}

#[derive(Debug)]
pub struct ExtractedTrack {
    pub source: String,
    pub id: u64,
    pub track_type: String,
    pub lang: String,
    pub name: String,
    pub codec_id: String,
    pub path: PathBuf,
}

#[derive(Deserialize, Debug)]
struct FfprobePacket {
    pts_time: Option<String>,
    flags: Option<String>,
}

#[derive(Deserialize, Debug)]
struct FfprobeKeyframeOutput {
    packets: Vec<FfprobePacket>,
}

#[derive(Debug, Clone, Default)]
struct ChapterAtom {
    start_ns: i64,
    end_ns: Option<i64>,
    display: String,
}

// --- Public API ---

/// Gets detailed stream and attachment information from a file using `mkvmerge -J`.
fn get_full_stream_info<F>(
    config: &Config,
    file_path: &Path,
    log_callback: Arc<Mutex<F>>,
) -> Result<MkvMergeOutput>
where
F: FnMut(String) + Send + 'static,
{
    let output = process::run_command(
        config,
        "mkvmerge",
        &["-J", file_path.to_str().unwrap()],
                                      log_callback,
    )?;
    let data: MkvMergeOutput = serde_json::from_str(&output)?;
    Ok(data)
}

/// Gathers track info from all source files, intended for the track selection dialog.
pub fn get_track_info_for_dialog<F>(
    config: &Config,
    ref_file: &Path,
    sec_file: Option<&Path>,
    ter_file: Option<&Path>,
    log_callback: Arc<Mutex<F>>,
) -> Result<HashMap<String, Vec<TrackInfo>>>
where
F: FnMut(String) + Send + 'static,
{
    let mut all_tracks = HashMap::new();
    let sources = [("REF", Some(ref_file)), ("SEC", sec_file), ("TER", ter_file)];

    for (source_name, path_opt) in sources {
        if let Some(path) = path_opt {
            if !path.exists() { continue; }
            let info = get_full_stream_info(config, path, Arc::clone(&log_callback))?;
            let info_vec: Vec<TrackInfo> = info.tracks.into_iter().map(|t| TrackInfo {
                source: source_name.to_string(),
                                                                       id: t.id,
                                                                       track_type: t.track_type,
                                                                       codec_id: t.properties.codec_id.unwrap_or_else(|| "N/A".to_string()),
                                                                       lang: t.properties.language.unwrap_or_else(|| "und".to_string()),
                                                                       name: t.properties.track_name.unwrap_or_default(),
            }).collect();
            all_tracks.insert(source_name.to_string(), info_vec);
        }
    }
    Ok(all_tracks)
}

/// Extracts specified tracks from an MKV file.
pub fn extract_tracks<F>(
    config: &Config,
    mkv_path: &Path,
    temp_dir: &Path,
    role: &str,
    specific_tracks: &[u64],
    log_callback: Arc<Mutex<F>>,
) -> Result<Vec<ExtractedTrack>>
where
F: FnMut(String) + Send + 'static,
{
    if specific_tracks.is_empty() { return Ok(Vec::new()); }

    let info = get_full_stream_info(config, mkv_path, Arc::clone(&log_callback))?;
    let mut extracted_list = Vec::new();
    let mut mkvextract_specs = Vec::new();
    let mut ffmpeg_jobs = Vec::new();
    let mut audio_idx_counter: i32 = -1;

    for track in info.tracks {
        if track.track_type == "audio" { audio_idx_counter += 1; }
        if !specific_tracks.contains(&track.id) { continue; }

        let codec_id = track.properties.codec_id.clone().unwrap_or_default();
        let ext = ext_for_codec(&track.track_type, &codec_id);
        let stem = mkv_path.file_stem().unwrap().to_str().unwrap();
        let mut out_path = temp_dir.join(format!("{}_track_{}_{}.{}", role, stem, track.id, ext));

        if track.track_type == "audio" && codec_id.to_uppercase().contains("A_MS/ACM") {
            out_path.set_extension("wav");
            let pcm_codec = pcm_codec_from_bit_depth(track.properties.audio_bits_per_sample);
            ffmpeg_jobs.push((audio_idx_counter, pcm_codec.to_string(), out_path.clone()));
        } else {
            mkvextract_specs.push(format!("{}:{}", track.id, out_path.display()));
        }

        extracted_list.push(ExtractedTrack {
            source: role.to_uppercase(), id: track.id, track_type: track.track_type,
                            lang: track.properties.language.unwrap_or_else(|| "und".to_string()),
                            name: track.properties.track_name.unwrap_or_default(),
                            codec_id, path: out_path,
        });
    }

    if !mkvextract_specs.is_empty() {
        let mut args: Vec<String> = vec![mkv_path.to_str().unwrap().to_string(), "tracks".to_string()];
        args.extend(mkvextract_specs);
        let args_str: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
        process::run_command(config, "mkvextract", &args_str, Arc::clone(&log_callback))?;
    }

    for (idx, pcm_codec, out_path) in ffmpeg_jobs {
        log_callback.lock().unwrap()(format!("[A_MS/ACM] Attempting stream copy for track index {}...", idx));
        let _ = process::run_command(
            config, "ffmpeg", &[
                "-y", "-v", "error", "-nostdin", "-i", mkv_path.to_str().unwrap(),
                                     "-map", &format!("0:a:{}", idx), "-c:a", "copy", out_path.to_str().unwrap()
            ], Arc::clone(&log_callback));

        if !out_path.exists() || fs::metadata(&out_path)?.len() == 0 {
            log_callback.lock().unwrap()(format!("[A_MS/ACM] Stream copy failed. Falling back to PCM encode ({}).", pcm_codec));
            process::run_command(
                config, "ffmpeg", &[
                    "-y", "-v", "error", "-nostdin", "-i", mkv_path.to_str().unwrap(),
                                 "-map", &format!("0:a:{}", idx), "-acodec", &pcm_codec, out_path.to_str().unwrap()
                ], Arc::clone(&log_callback))?;
        } else {
            log_callback.lock().unwrap()("[A_MS/ACM] Stream copy succeeded.".to_string());
        }
    }
    Ok(extracted_list)
}

/// Extracts all attachments from a file.
pub fn extract_attachments<F>(
    config: &Config,
    mkv_path: &Path,
    temp_dir: &Path,
    role: &str,
    log_callback: Arc<Mutex<F>>,
) -> Result<Vec<PathBuf>>
where
F: FnMut(String) + Send + 'static,
{
    let info = get_full_stream_info(config, mkv_path, Arc::clone(&log_callback))?;
    if info.attachments.is_empty() { return Ok(Vec::new()); }

    let mut specs = Vec::new();
    let mut out_paths = Vec::new();

    for attachment in info.attachments {
        let out_path = temp_dir.join(format!("{}_att_{}_{}", role, attachment.id, attachment.file_name));
        specs.push(format!("{}:{}", attachment.id, out_path.display()));
        out_paths.push(out_path);
    }

    let mut args: Vec<String> = vec![mkv_path.to_str().unwrap().to_string(), "attachments".to_string()];
    args.extend(specs);
    let args_str: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    process::run_command(config, "mkvextract", &args_str, Arc::clone(&log_callback))?;

    Ok(out_paths)
}

/// Extracts, modifies, and saves chapter information, including keyframe snapping.
pub fn process_chapters<F>(
    config: &Config,
    ref_mkv: &Path,
    temp_dir: &Path,
    shift_ms: i64,
    log_callback: Arc<Mutex<F>>,
) -> Result<Option<PathBuf>>
where
F: FnMut(String) + Send + 'static,
{
    let xml_content = match process::run_command(config, "mkvextract", &["chapters", ref_mkv.to_str().unwrap(), "-"], Arc::clone(&log_callback)) {
        Ok(content) if !content.trim().is_empty() => content,
        _ => {
            log_callback.lock().unwrap()("[Chapters] No chapters found in reference file.".to_string());
            return Ok(None);
        }
    };

    let mut chapters = parse_chapter_xml(&xml_content)?;
    if chapters.is_empty() { return Ok(None); }

    let keyframes_ns = if config.snap_chapters {
        probe_keyframes_ns(config, ref_mkv, Arc::clone(&log_callback))?
    } else { Vec::new() };

    for (i, chapter) in chapters.iter_mut().enumerate() {
        if config.rename_chapters {
            chapter.display = format!("Chapter {:02}", i + 1);
        }
        if shift_ms != 0 {
            let shift_ns = shift_ms * 1_000_000;
            chapter.start_ns += shift_ns;
            chapter.end_ns = chapter.end_ns.map(|ns| ns + shift_ns);
        }
        if config.snap_chapters && !keyframes_ns.is_empty() {
            let (snapped, moved, on_kf, too_far) = snap_timestamp(chapter.start_ns, &keyframes_ns, config);
            chapter.start_ns = snapped;
            if !config.snap_starts_only {
                chapter.end_ns = chapter.end_ns.map(|ns| snap_timestamp(ns, &keyframes_ns, config).0);
            }
        }
    }

    normalize_chapter_end_times(&mut chapters);

    let out_path = temp_dir.join(format!("{}_chapters_modified.xml", ref_mkv.file_stem().unwrap().to_str().unwrap()));
    write_chapter_xml(&out_path, &chapters)?;
    log_callback.lock().unwrap()(format!("[Chapters] Chapters XML written to: {}", out_path.display()));

    Ok(Some(out_path))
}


// --- Private Helper Functions ---

fn probe_keyframes_ns<F>(config: &Config, ref_video_path: &Path, log_callback: Arc<Mutex<F>>) -> Result<Vec<i64>> where F: FnMut(String) + Send + 'static {
    let out = process::run_command(config, "ffprobe", &[
        "-v", "error", "-select_streams", "v:0",
        "-show_entries", "packet=pts_time,flags", "-of", "json", ref_video_path.to_str().unwrap()
    ], log_callback)?;

    let data: FfprobeKeyframeOutput = serde_json::from_str(&out)?;
    let mut kfs_ns: Vec<i64> = data.packets.into_iter()
    .filter_map(|p| {
        if p.flags.unwrap_or_default().contains('K') {
            p.pts_time.and_then(|t| t.parse::<f64>().ok()).map(|t_sec| (t_sec * 1e9) as i64)
        } else {
            None
        }
    })
    .collect();
    kfs_ns.sort_unstable();
    log_callback.lock().unwrap()(format!("[Chapters] Found {} keyframes for snapping.", kfs_ns.len()));
    Ok(kfs_ns)
}

fn snap_timestamp(ts_ns: i64, keyframes_ns: &[i64], config: &Config) -> (i64, bool, bool, bool) {
    if keyframes_ns.is_empty() { return (ts_ns, false, false, false); }

    let threshold_ns = config.snap_threshold_ms as i64 * 1_000_000;

    let i = match keyframes_ns.binary_search(&ts_ns) {
        Ok(_) => return (ts_ns, false, true, false), // Already on a keyframe
        Err(i) => i,
    };

    let prev_kf = if i > 0 { Some(keyframes_ns[i - 1]) } else { None };
    let next_kf = if i < keyframes_ns.len() { Some(keyframes_ns[i]) } else { None };

    let candidate_ns = match (prev_kf, next_kf) {
        (Some(prev), Some(next)) => {
            if config.snap_mode == "previous" {
                prev
            } else { // nearest
                if (ts_ns - prev).abs() <= (next - ts_ns).abs() { prev } else { next }
            }
        },
        (Some(prev), None) => prev,
        (None, Some(next)) => next,
        (None, None) => ts_ns,
    };

    if (ts_ns - candidate_ns).abs() <= threshold_ns {
        (candidate_ns, true, false, false)
    } else {
        (ts_ns, false, false, true) // Too far, don't snap
    }
}

fn parse_chapter_xml(xml_content: &str) -> Result<Vec<ChapterAtom>> {
    let parser = EventReader::new(xml_content.as_bytes());
    let mut chapters = Vec::new();
    let mut current_atom: Option<ChapterAtom> = None;
    let mut tag_stack = Vec::new();

    for event in parser {
        match event? {
            XmlReadEvent::StartElement { name, .. } => {
                let local_name = name.local_name.clone();
                if local_name == "ChapterAtom" {
                    current_atom = Some(ChapterAtom::default());
                }
                tag_stack.push(local_name);
            }
            XmlReadEvent::EndElement { name } => {
                if name.local_name == "ChapterAtom" {
                    if let Some(atom) = current_atom.take() { chapters.push(atom); }
                }
                tag_stack.pop();
            }
            XmlReadEvent::Characters(text) => {
                if let (Some(atom), Some(tag)) = (current_atom.as_mut(), tag_stack.last()) {
                    match tag.as_str() {
                        "ChapterTimeStart" => atom.start_ns = parse_ns(&text).unwrap_or(0),
                        "ChapterTimeEnd" => atom.end_ns = parse_ns(&text).ok(),
                        "ChapterString" => atom.display = text,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(chapters)
}

fn normalize_chapter_end_times(chapters: &mut [ChapterAtom]) {
    for i in 0..chapters.len() {
        let next_start_ns = if i + 1 < chapters.len() { Some(chapters[i+1].start_ns) } else { None };
        let start_ns = chapters[i].start_ns;

        let desired_end_ns = match (chapters[i].end_ns, next_start_ns) {
            (Some(end), Some(next_start)) => end.min(next_start),
            (Some(end), None) => end,
            (None, Some(next_start)) => next_start,
            (None, None) => start_ns + 1_000_000_000, // Default to 1s duration
        };
        chapters[i].end_ns = Some(desired_end_ns.max(start_ns + 1));
    }
}

fn write_chapter_xml(path: &Path, chapters: &[ChapterAtom]) -> Result<()> {
    let file = fs::File::create(path)?;
    let mut writer = EmitterConfig::new().perform_indent(true).create_writer(file);
    writer.write(XmlWriteEvent::start_element("Chapters"))?;
    writer.write(XmlWriteEvent::start_element("EditionEntry"))?;
    for chapter in chapters {
        writer.write(XmlWriteEvent::start_element("ChapterAtom"))?;
        writer.write(XmlWriteEvent::start_element("ChapterTimeStart"))?;
        writer.write(XmlWriteEvent::characters(&fmt_ns(chapter.start_ns)))?;
        writer.write(XmlWriteEvent::end_element())?;
        if let Some(end_ns) = chapter.end_ns {
            writer.write(XmlWriteEvent::start_element("ChapterTimeEnd"))?;
            writer.write(XmlWriteEvent::characters(&fmt_ns(end_ns)))?;
            writer.write(XmlWriteEvent::end_element())?;
        }
        writer.write(XmlWriteEvent::start_element("ChapterDisplay"))?;
        writer.write(XmlWriteEvent::start_element("ChapterString"))?;
        writer.write(XmlWriteEvent::characters(&chapter.display))?;
        writer.write(XmlWriteEvent::end_element())?;
        writer.write(XmlWriteEvent::start_element("ChapterLanguage"))?;
        writer.write(XmlWriteEvent::characters("und"))?;
        writer.write(XmlWriteEvent::end_element())?;
        writer.write(XmlWriteEvent::end_element())?; // ChapterDisplay
        writer.write(XmlWriteEvent::end_element())?; // ChapterAtom
    }
    writer.write(XmlWriteEvent::end_element())?; // EditionEntry
    writer.write(XmlWriteEvent::end_element())?; // Chapters
    Ok(())
}

fn ext_for_codec(ttype: &str, codec_id: &str) -> &'static str {
    let cid = codec_id.to_uppercase();
    match ttype {
        "video" => "bin",
        "audio" => match cid.as_str() {
            _ if cid.contains("A_TRUEHD") => "thd",
            _ if cid.contains("A_EAC3") => "eac3",
            _ if cid.contains("A_AC3") => "ac3",
            _ if cid.contains("A_DTS") => "dts",
            _ if cid.contains("A_AAC") => "aac",
            _ if cid.contains("A_FLAC") => "flac",
            _ if cid.contains("A_OPUS") => "opus",
            _ if cid.contains("A_PCM") => "wav",
            _ => "bin",
        },
        "subtitles" => match cid.as_str() {
            _ if cid.contains("S_TEXT/ASS") => "ass",
            _ if cid.contains("S_TEXT/SSA") => "ssa",
            _ if cid.contains("S_TEXT/UTF8") => "srt",
            _ if cid.contains("S_HDMV/PGS") => "sup",
            _ => "sub",
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

fn parse_ns(t: &str) -> Result<i64> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() < 3 { return Err(anyhow!("Invalid timestamp format")); }
    let hh: i64 = parts[0].parse()?;
    let mm: i64 = parts[1].parse()?;

    let sec_parts: Vec<&str> = parts[2].split('.').collect();
    let ss: i64 = sec_parts[0].parse()?;
    let nano_str = format!("{:0<9}", sec_parts.get(1).unwrap_or(&"0"));
    let nano: i64 = nano_str[..9].parse()?;

    Ok((hh * 3600 + mm * 60 + ss) * 1_000_000_000 + nano)
}

fn fmt_ns(ns: i64) -> String {
    let ns = ns.max(0);
    let total_s = ns / 1_000_000_000;
    let hh = total_s / 3600;
    let mm = (total_s % 3600) / 60;
    let ss = total_s % 60;
    let nano = ns % 1_000_000_000;
    format!("{:02}:{:02}:{:02}.{:09}", hh, mm, ss, nano)
}
