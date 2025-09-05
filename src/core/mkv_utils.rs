// src/core/mkv_utils.rs

use crate::core::config::AppConfig;
use crate::core::process::CommandRunner;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;
use xml::reader::{EventReader, XmlEvent};
use xml::writer::{EmitterConfig, XmlEvent as WriteXmlEvent};

#[derive(Debug, Deserialize, Clone)]
pub struct TrackProperties {
    pub codec_id: Option<String>,
    pub language: Option<String>,
    pub track_name: Option<String>,
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

pub async fn get_audio_stream_index(
    runner: &CommandRunner,
    file_path: &str,
    language: Option<&str>,
) -> Result<Option<usize>, String> {
    let info = get_stream_info(runner, file_path).await?;
    let mut first_audio_idx = None;
    let mut audio_stream_counter = 0;

    for track in info.tracks.iter() {
        if track.r#type == "audio" {
            if first_audio_idx.is_none() {
                first_audio_idx = Some(audio_stream_counter);
            }
            if let Some(lang) = language {
                if track.properties.language.as_deref() == Some(lang) {
                    return Ok(Some(audio_stream_counter)); // Found exact language match
                }
            }
            audio_stream_counter += 1;
        }
    }
    Ok(first_audio_idx) // Return first audio track if no language match
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

pub async fn extract_tracks(runner: &CommandRunner, source_file: &Path, tracks_to_select: &[Track], temp_dir: &Path, role: &str) -> Result<Vec<ExtractedTrack>, String> {
    if tracks_to_select.is_empty() { return Ok(vec![]); }

    let mut mkvextract_specs = Vec::new();
    let mut ffmpeg_jobs = Vec::new();
    let mut extracted_tracks = Vec::new();

    let all_source_tracks = get_stream_info(runner, &source_file.to_string_lossy()).await?.tracks;
    let mut current_audio_idx = -1;

    for track_info in all_source_tracks.iter() {
        if track_info.r#type == "audio" {
            current_audio_idx += 1;
        }

        if let Some(track_to_extract) = tracks_to_select.iter().find(|t| t.id == track_info.id) {
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

async fn probe_keyframes(runner: &CommandRunner, file_path: &str) -> Result<Vec<u64>, String> {
    let result = runner.run("ffprobe", &[
        "-v", "error", "-select_streams", "v:0",
        "-show_entries", "packet=pts_time,flags",
        "-of", "json", file_path
    ]).await?;
    if result.exit_code != 0 { return Err("ffprobe failed to get keyframes".to_string()); }

    let ffprobe_data: FfprobeOutput = serde_json::from_str(&result.stdout)
    .map_err(|e| format!("Failed to parse ffprobe JSON: {}", e))?;

    let mut keyframes_ns: Vec<u64> = ffprobe_data.packets.into_iter()
    .filter(|p| p.flags.contains('K'))
    .filter_map(|p| p.pts_time.parse::<f64>().ok())
    .map(|t| (t * 1_000_000_000.0).round() as u64)
    .collect();

    keyframes_ns.sort_unstable();
    Ok(keyframes_ns)
}

pub async fn process_chapters(runner: &CommandRunner, ref_file: &str, temp_dir: &Path, shift_ms: i64, config: &AppConfig) -> Result<Option<PathBuf>, String> {
    let result = runner.run("mkvextract", &["chapters", ref_file, "-"]).await?;
    if result.exit_code != 0 || result.stdout.trim().is_empty() {
        runner.send_log("No chapters found in reference file.").await;
        return Ok(None);
    }

    let input_xml = result.stdout.trim_start_matches('\u{feff}');

    let keyframes = if config.snap_chapters {
        runner.send_log("[Chapters] Probing keyframes for snapping...").await;
        match probe_keyframes(runner, ref_file).await {
            Ok(kf) if !kf.is_empty() => {
                runner.send_log(&format!("[Chapters] Found {} keyframes.", kf.len())).await;
                Some(kf)
            },
            _ => {
                runner.send_log("[Chapters] Snap skipped: could not load keyframes.").await;
                None
            }
        }
    } else {
        None
    };

    let modified_xml_bytes = transform_chapters(runner, input_xml, shift_ms, config, keyframes.as_deref()).await?;

    let out_path = temp_dir.join(format!("{}_chapters_processed.xml", Path::new(ref_file).file_stem().unwrap().to_string_lossy()));
    fs::write(&out_path, &modified_xml_bytes).await.map_err(|e| e.to_string())?;

    runner.send_log(&format!("[Chapters] Modified chapters written to: {}", out_path.display())).await;
    Ok(Some(out_path))
}


#[derive(Debug, Clone)]
struct ChapterAtom {
    start_ns: u64,
    end_ns: Option<u64>,
    original_events: Vec<XmlEvent>,
}

async fn transform_chapters(runner: &CommandRunner, xml_content: &str, shift_ms: i64, config: &AppConfig, keyframes: Option<&[u64]>) -> Result<Vec<u8>, String> {
    let mut atoms = parse_chapters_to_atoms(xml_content)?;

    // === Pass 2: Manipulate the intermediate representation ===
    atoms.sort_by_key(|a| a.start_ns);
    let shift_ns = shift_ms * 1_000_000;

    for atom in atoms.iter_mut() {
        // Apply shift
        atom.start_ns = atom.start_ns.saturating_add_signed(shift_ns);
        atom.end_ns = atom.end_ns.map(|ns| ns.saturating_add_signed(shift_ns));

        // Apply snapping
        if let Some(kf) = keyframes {
            let threshold_ns = (config.snap_threshold_ms as u64) * 1_000_000;
            atom.start_ns = find_snap_candidate(atom.start_ns, kf, &config.snap_mode, threshold_ns);
            if !config.snap_starts_only {
                atom.end_ns = atom.end_ns.map(|ns| find_snap_candidate(ns, kf, &config.snap_mode, threshold_ns));
            }
        }
    }

    // Apply end time normalization
    let mut fixed_end_times = 0;
    for i in 0..atoms.len() {
        let next_start_ns = if i + 1 < atoms.len() { Some(atoms[i+1].start_ns) } else { None };
        let start_ns = atoms[i].start_ns;

        let mut desired_end_ns = atoms[i].end_ns.unwrap_or(start_ns + 1_000_000_000); // Default 1s duration
        if let Some(next_start) = next_start_ns {
            desired_end_ns = desired_end_ns.min(next_start);
        }
        desired_end_ns = desired_end_ns.max(start_ns + 1); // Ensure end is after start

        if atoms[i].end_ns != Some(desired_end_ns) {
            atoms[i].end_ns = Some(desired_end_ns);
            fixed_end_times += 1;
        }
    }
    if fixed_end_times > 0 {
        runner.send_log(&format!("[Chapters] Normalized {} chapter end times.", fixed_end_times)).await;
    }


    // === Pass 3: Rebuild the XML from the modified data ===
    rebuild_xml_from_atoms(atoms, config)
}

fn parse_chapters_to_atoms(xml_content: &str) -> Result<Vec<ChapterAtom>, String> {
    let mut parser = EventReader::from_str(xml_content);
    let mut atoms = Vec::new();
    let mut top_level_events = Vec::new();

    // Find the start of the first ChapterAtom
    loop {
        match parser.next() {
            Ok(XmlEvent::StartElement { name, .. }) if name.local_name == "ChapterAtom" => {
                break; // Found it
            }
            Ok(XmlEvent::EndDocument) => return Ok(atoms), // No chapters
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }

    // Now we are positioned at the first <ChapterAtom>
    loop {
        let mut current_atom_events = Vec::new();
        let mut current_start = 0;
        let mut current_end = None;
        let mut depth = 1; // Start inside the ChapterAtom
        current_atom_events.push(XmlEvent::StartElement { name: "ChapterAtom".into(), attributes: vec![], namespace: Default::default() });

        loop {
            let event = parser.next().map_err(|e| e.to_string())?;
            match &event {
                XmlEvent::StartElement { name, .. } => {
                    depth += 1;
                    match name.local_name.as_str() {
                        "ChapterTimeStart" => {
                            if let Ok(XmlEvent::Characters(chars)) = parser.next().map_err(|e|e.to_string()) {
                                current_start = parse_time_ns(&chars).unwrap_or(0);
                            }
                            parser.next().ok(); // consume end element
                            depth -=1;
                        },
                        "ChapterTimeEnd" => {
                            if let Ok(XmlEvent::Characters(chars)) = parser.next().map_err(|e|e.to_string()) {
                                current_end = parse_time_ns(&chars);
                            }
                            parser.next().ok(); // consume end element
                            depth -=1;
                        }
                        _ => current_atom_events.push(event.clone())
                    }
                },
                XmlEvent::EndElement { name, .. } => {
                    depth -= 1;
                    if depth == 0 && name.local_name == "ChapterAtom" {
                        current_atom_events.push(event.clone());
                        break;
                    }
                    current_atom_events.push(event.clone());
                },
                XmlEvent::EndDocument => break,
                _ => current_atom_events.push(event.clone())
            }
        }
        atoms.push(ChapterAtom { start_ns: current_start, end_ns: current_end, original_events: current_atom_events });

        if let Ok(XmlEvent::EndDocument) = parser.peek() {
            break;
        }
    }

    Ok(atoms)
}

fn rebuild_xml_from_atoms(atoms: Vec<ChapterAtom>, config: &AppConfig) -> Result<Vec<u8>, String> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut writer = EmitterConfig::new().perform_indent(true).create_writer(&mut buffer);

    writer.write(WriteXmlEvent::start_element("Chapters")).map_err(|e|e.to_string())?;
    writer.write(WriteXmlEvent::start_element("EditionEntry")).map_err(|e|e.to_string())?;

    for (i, atom) in atoms.iter().enumerate() {
        writer.write(WriteXmlEvent::start_element("ChapterAtom")).map_err(|e|e.to_string())?;

        // Write required time elements
        writer.write(WriteXmlEvent::start_element("ChapterTimeStart")).map_err(|e|e.to_string())?;
        writer.write(WriteXmlEvent::characters(&format_time_ns(atom.start_ns))).map_err(|e|e.to_string())?;
        writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;

        if let Some(end_ns) = atom.end_ns {
            writer.write(WriteXmlEvent::start_element("ChapterTimeEnd")).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::characters(&format_time_ns(end_ns))).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;
        }

        // Write the other original events, substituting ChapterDisplay if needed
        let mut display_handled = false;
        for event in &atom.original_events {
            if let Some(we) = event.as_writer_event() {
                if let xml::writer::XmlEvent::StartElement { name, .. } = we {
                    if name.local_name == "ChapterDisplay" {
                        display_handled = true;
                        if config.rename_chapters {
                            writer.write(WriteXmlEvent::start_element("ChapterDisplay")).map_err(|e|e.to_string())?;
                            let name_str = format!("Chapter {:02}", i + 1);
                            writer.write(WriteXmlEvent::start_element("ChapterString")).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::characters(&name_str)).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::start_element("ChapterLanguage")).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::characters("und")).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;
                            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?; // ChapterDisplay
                        } else {
                            writer.write(we).map_err(|e|e.to_string())?;
                        }
                    } else if name.local_name != "ChapterAtom" {
                        writer.write(we).map_err(|e|e.to_string())?;
                    }
                } else {
                    writer.write(we).map_err(|e|e.to_string())?;
                }
            }
        }

        if !display_handled && config.rename_chapters {
            writer.write(WriteXmlEvent::start_element("ChapterDisplay")).map_err(|e|e.to_string())?;
            let name_str = format!("Chapter {:02}", i + 1);
            writer.write(WriteXmlEvent::start_element("ChapterString")).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::characters(&name_str)).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::start_element("ChapterLanguage")).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::characters("und")).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?;
            writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?; // ChapterDisplay
        }

        writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?; // ChapterAtom
    }

    writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?; // EditionEntry
    writer.write(WriteXmlEvent::end_element()).map_err(|e|e.to_string())?; // Chapters
    Ok(buffer)
}

fn find_snap_candidate(ts_ns: u64, keyframes: &[u64], mode: &str, threshold_ns: u64) -> u64 {
    let search_result = keyframes.binary_search(&ts_ns);
    let candidate = match search_result {
        Ok(_) => ts_ns,
        Err(i) => {
            let prev_kf = if i > 0 { Some(keyframes[i - 1]) } else { None };
            let next_kf = keyframes.get(i);
            match (prev_kf, next_kf) {
                (Some(p), Some(&n)) => if mode == "previous" { p } else if ts_ns - p < n - ts_ns { p } else { n },
                (Some(p), None) => p,
                (None, Some(&n)) => n,
                (None, None) => ts_ns,
            }
        }
    };
    if (candidate as i64 - ts_ns as i64).abs() as u64 <= threshold_ns {
        candidate
    } else {
        ts_ns
    }
}

fn parse_time_ns(t: &str) -> Result<u64, ()> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() != 3 { return Err(()); }
    let s_frac: Vec<&str> = parts[2].split('.').collect();
    let (ss_str, frac_str_opt) = if s_frac.len() == 2 {
        (s_frac[0], Some(s_frac[1]))
    } else if s_frac.len() == 1 {
        (s_frac[0], None)
    } else {
        return Err(())
    };

    let hh: u64 = parts[0].parse().map_err(|_| ())?;
    let mm: u64 = parts[1].parse().map_err(|_| ())?;
    let ss: u64 = ss_str.parse().map_err(|_| ())?;

    let mut frac_str = frac_str_opt.unwrap_or("0").to_string();
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
