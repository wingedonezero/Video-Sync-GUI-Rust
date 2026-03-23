//! Audio assembly — 1:1 port of `vsg_core/correction/stepping/audio_assembly.py`.
//!
//! Audio reconstruction from an EDL (Edit Decision List).
//!
//! Given a list of `AudioSegment` entries, this module:
//!   1. Inserts silence where the delay increases (gap between clusters)
//!   2. Trims audio where the delay decreases (overlap)
//!   3. Applies per-segment drift correction when needed
//!   4. Concatenates all pieces via FFmpeg into a single FLAC

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::analysis::correlation::decode::get_audio_stream_info;
use crate::io::runner::CommandRunner;
use crate::models::enums::ResampleEngine;
use crate::models::settings::AppSettings;

use super::types::AudioSegment;

// ---------------------------------------------------------------------------
// Audio probing / decoding helpers
// ---------------------------------------------------------------------------

/// Return `(channels, channel_layout, sample_rate)` via ffprobe — `get_audio_properties`
pub fn get_audio_properties(
    file_path: &str,
    stream_index: i32,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> Result<(i32, String, i32), String> {
    let select_arg = format!("a:{stream_index}");
    let cmd: Vec<&str> = vec![
        "ffprobe",
        "-v", "error",
        "-select_streams", &select_arg,
        "-show_entries", "stream=channels,channel_layout,sample_rate",
        "-of", "json",
        file_path,
    ];

    let out = runner
        .run(&cmd, tool_paths)
        .ok_or_else(|| format!("Could not probe audio properties for {file_path}"))?;

    let info: serde_json::Value = serde_json::from_str(&out)
        .map_err(|e| format!("Failed to parse ffprobe JSON: {e}"))?;

    let stream = info
        .get("streams")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "No streams found in ffprobe output".to_string())?;

    let channels = stream
        .get("channels")
        .and_then(|v| v.as_i64())
        .unwrap_or(2) as i32;

    let default_layout = match channels {
        1 => "mono",
        2 => "stereo",
        6 => "5.1(side)",
        8 => "7.1",
        _ => "stereo",
    };

    let layout = stream
        .get("channel_layout")
        .and_then(|v| v.as_str())
        .unwrap_or(default_layout)
        .to_string();

    let sample_rate = stream
        .get("sample_rate")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<i32>().ok())
                .or_else(|| v.as_i64().map(|i| i as i32))
        })
        .unwrap_or(48000);

    Ok((channels, layout, sample_rate))
}

/// Decode an audio stream to i32 PCM in memory — `decode_to_memory`
pub fn decode_to_memory(
    file_path: &str,
    stream_index: i32,
    sample_rate: i32,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    channels: i32,
    log: Option<&dyn Fn(&str)>,
) -> Option<Vec<i32>> {
    let map_arg = format!("0:a:{stream_index}");
    let channels_str = channels.to_string();
    let sr_str = sample_rate.to_string();

    let cmd: Vec<&str> = vec![
        "ffmpeg",
        "-nostdin",
        "-v", "error",
        "-i", file_path,
        "-map", &map_arg,
        "-ac", &channels_str,
        "-ar", &sr_str,
        "-f", "s32le",
        "-",
    ];

    let pcm_bytes = runner.run_binary(&cmd, tool_paths, None)?;

    // Ensure buffer alignment (4 bytes per i32 sample)
    let elem = 4;
    let aligned = (pcm_bytes.len() / elem) * elem;
    if aligned != pcm_bytes.len() {
        if let Some(log_fn) = log {
            log_fn(&format!(
                "[BUFFER ALIGNMENT] Trimmed {} bytes from {}",
                pcm_bytes.len() - aligned,
                Path::new(file_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
        }
    }

    let pcm: Vec<i32> = pcm_bytes[..aligned]
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Some(pcm)
}

// ---------------------------------------------------------------------------
// Assembly
// ---------------------------------------------------------------------------

/// Build a corrected FLAC from an EDL — `assemble_corrected_audio`
///
/// If `target_pcm`, `channels`, `channel_layout`, `sample_rate` are supplied
/// the file is not re-decoded. Otherwise the function probes and decodes
/// `target_audio_path` itself.
///
/// Returns `true` on success.
#[allow(clippy::too_many_arguments)]
pub fn assemble_corrected_audio(
    edl: &[AudioSegment],
    target_audio_path: &str,
    output_path: &Path,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    settings: &AppSettings,
    log: &dyn Fn(&str),
    ch_arg: Option<i32>,
    cl_arg: Option<&str>,
    sr_arg: Option<i32>,
    pcm_arg: Option<&[i32]>,
) -> bool {
    // Probe / decode if needed — resolve all audio parameters
    let owned_pcm: Vec<i32>;
    let channels: i32;
    let channel_layout: String;
    let sample_rate: i32;
    let pcm_slice: &[i32];

    if let (Some(ch), Some(cl), Some(sr), Some(pcm)) = (ch_arg, cl_arg, sr_arg, pcm_arg) {
        channels = ch;
        channel_layout = cl.to_string();
        sample_rate = sr;
        owned_pcm = vec![];
        let _ = &owned_pcm; // keep alive
        pcm_slice = pcm;
    } else {
        let (idx, _) = get_audio_stream_info(target_audio_path, None, runner, tool_paths);
        let idx = match idx {
            Some(i) => i,
            None => {
                log(&format!("[ERROR] No audio stream in {target_audio_path}"));
                return false;
            }
        };

        let props = match get_audio_properties(target_audio_path, idx, runner, tool_paths) {
            Ok(p) => p,
            Err(e) => {
                log(&format!("[ERROR] {e}"));
                return false;
            }
        };
        channels = props.0;
        channel_layout = props.1;
        sample_rate = props.2;

        owned_pcm = match decode_to_memory(
            target_audio_path, idx, sample_rate, runner, tool_paths, channels, Some(log),
        ) {
            Some(d) => d,
            None => return false,
        };
        pcm_slice = &owned_pcm;
    }

    log(&format!(
        "  [Assembly] Building {} segment(s) -> {}",
        edl.len(),
        output_path.file_name().unwrap_or_default().to_string_lossy()
    ));

    let stem = output_path.file_stem().unwrap_or_default().to_string_lossy();
    let assembly_dir = output_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("assembly_{stem}"));
    let _ = fs::create_dir_all(&assembly_dir);

    let mut segment_files: Vec<String> = Vec::new();
    let base_delay_ms = edl[0].delay_ms;
    let mut current_delay = base_delay_ms;

    let result = (|| -> Result<(), String> {
        let pcm_duration_s =
            pcm_slice.len() as f64 / (sample_rate as f64 * channels as f64);

        for (i, segment) in edl.iter().enumerate() {
            let gap_ms = segment.delay_ms - current_delay;

            if gap_ms.abs() > 10 {
                if gap_ms > 0 {
                    // Insert silence
                    log(&format!(
                        "    At {:.3}s: insert {}ms silence",
                        segment.start_s, gap_ms
                    ));
                    let silence_file =
                        assembly_dir.join(format!("silence_{i:03}.flac"));
                    let silence_samples =
                        ((gap_ms as f64 / 1000.0) * sample_rate as f64) as usize
                            * channels as usize;
                    let silence_pcm = vec![0i32; silence_samples];

                    if !encode_flac(
                        &silence_pcm,
                        &silence_file,
                        sample_rate,
                        channels,
                        &channel_layout,
                        runner,
                        tool_paths,
                    ) {
                        return Err(format!(
                            "Silence encode failed at segment {i}"
                        ));
                    }
                    segment_files.push(format!(
                        "file '{}'",
                        silence_file
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ));
                } else {
                    // Remove audio (negative gap)
                    log(&format!(
                        "    At {:.3}s: remove {}ms audio",
                        segment.start_s, -gap_ms
                    ));
                }
            }

            current_delay = segment.delay_ms;

            // Extract this segment's audio
            let seg_start = segment.start_s;
            let seg_end = if i + 1 < edl.len() {
                edl[i + 1].start_s
            } else {
                pcm_duration_s
            };

            let mut actual_start = seg_start;
            if gap_ms < 0 {
                actual_start += (-gap_ms) as f64 / 1000.0;
            }

            if seg_end <= actual_start {
                continue;
            }

            let start_sample =
                (actual_start * sample_rate as f64) as usize * channels as usize;
            let end_sample = ((seg_end * sample_rate as f64) as usize
                * channels as usize)
                .min(pcm_slice.len());

            if start_sample >= end_sample {
                continue;
            }

            let chunk = &pcm_slice[start_sample..end_sample];
            if chunk.is_empty() {
                continue;
            }

            let seg_file =
                assembly_dir.join(format!("segment_{i:03}.flac"));
            if !encode_flac(
                chunk,
                &seg_file,
                sample_rate,
                channels,
                &channel_layout,
                runner,
                tool_paths,
            ) {
                return Err(format!("Segment {i} encode failed"));
            }

            // Apply drift correction if significant
            let mut final_seg_file = seg_file.clone();
            if segment.drift_rate_ms_s.abs() > 0.5 {
                log(&format!(
                    "    Drift correction ({:+.2} ms/s) on segment {i}",
                    segment.drift_rate_ms_s
                ));
                let corrected_file = assembly_dir
                    .join(format!("segment_{i:03}_corrected.flac"));
                if !apply_drift_correction(
                    &seg_file,
                    &corrected_file,
                    segment.drift_rate_ms_s,
                    sample_rate,
                    settings,
                    runner,
                    tool_paths,
                    log,
                ) {
                    return Err(format!(
                        "Drift correction failed for segment {i}"
                    ));
                }
                final_seg_file = corrected_file;
            }

            segment_files.push(format!(
                "file '{}'",
                final_seg_file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
        }

        if segment_files.is_empty() {
            return Err("No segments generated for assembly.".to_string());
        }

        // Concatenate
        let concat_list = assembly_dir.join("concat_list.txt");
        fs::write(&concat_list, segment_files.join("\n"))
            .map_err(|e| format!("Failed to write concat list: {e}"))?;

        let concat_list_str = concat_list.to_string_lossy().to_string();
        let output_path_str = output_path.to_string_lossy().to_string();

        let concat_cmd: Vec<&str> = vec![
            "ffmpeg",
            "-y",
            "-v", "error",
            "-f", "concat",
            "-safe", "0",
            "-i", &concat_list_str,
            "-map_metadata", "-1",
            "-map_metadata:s:a", "-1",
            "-fflags", "+bitexact",
            "-c:a", "flac",
            &output_path_str,
        ];

        if runner.run(&concat_cmd, tool_paths).is_none() {
            return Err("FFmpeg concat failed.".to_string());
        }

        log(&format!(
            "  [Assembly] Done: {}",
            output_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        Ok(())
    })();

    // Cleanup assembly dir
    if assembly_dir.exists() {
        let _ = fs::remove_dir_all(&assembly_dir);
    }

    match result {
        Ok(()) => true,
        Err(e) => {
            log(&format!("  [Assembly] ERROR: {e}"));
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Encode raw i32 PCM to FLAC via ffmpeg — `_encode_flac`
fn encode_flac(
    pcm: &[i32],
    out_path: &Path,
    sample_rate: i32,
    channels: i32,
    channel_layout: &str,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
) -> bool {
    let sr_str = sample_rate.to_string();
    let ch_str = channels.to_string();
    let out_str = out_path.to_string_lossy().to_string();

    let cmd: Vec<&str> = vec![
        "ffmpeg",
        "-y",
        "-v", "error",
        "-nostdin",
        "-f", "s32le",
        "-ar", &sr_str,
        "-ac", &ch_str,
        "-channel_layout", channel_layout,
        "-i", "-",
        "-map_metadata", "-1",
        "-map_metadata:s:a", "-1",
        "-fflags", "+bitexact",
        "-c:a", "flac",
        &out_str,
    ];

    // Convert i32 slice to bytes
    let pcm_bytes: Vec<u8> = pcm
        .iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();

    runner
        .run_binary(&cmd, tool_paths, Some(&pcm_bytes))
        .is_some()
}

/// Apply tempo change to correct within-segment drift — `_apply_drift_correction`
#[allow(clippy::too_many_arguments)]
fn apply_drift_correction(
    input_path: &Path,
    output_path: &Path,
    drift_rate_ms_s: f64,
    sample_rate: i32,
    settings: &AppSettings,
    runner: &CommandRunner,
    tool_paths: &HashMap<String, String>,
    log: &dyn Fn(&str),
) -> bool {
    let tempo_ratio = 1000.0 / (1000.0 + drift_rate_ms_s);
    let engine = &settings.segment_resample_engine;

    let filter_chain: String = match engine {
        ResampleEngine::Rubberband => {
            let mut rb_opts = vec![format!("tempo={tempo_ratio}")];
            if !settings.segment_rb_pitch_correct {
                rb_opts.push(format!("pitch={tempo_ratio}"));
            }
            rb_opts.push(format!(
                "transients={}",
                settings.segment_rb_transients
            ));
            if settings.segment_rb_smoother {
                rb_opts.push("smoother=on".to_string());
            }
            if settings.segment_rb_pitchq {
                rb_opts.push("pitchq=on".to_string());
            }
            format!("rubberband={}", rb_opts.join(":"))
        }
        ResampleEngine::Atempo => {
            format!("atempo={tempo_ratio}")
        }
        ResampleEngine::Aresample => {
            let new_sr = sample_rate as f64 * tempo_ratio;
            format!("asetrate={new_sr},aresample={sample_rate}")
        }
    };

    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();

    let cmd: Vec<&str> = vec![
        "ffmpeg",
        "-y",
        "-nostdin",
        "-v", "error",
        "-i", &input_str,
        "-af", &filter_chain,
        "-map_metadata", "-1",
        "-map_metadata:s:a", "-1",
        "-fflags", "+bitexact",
        &output_str,
    ];

    let result = runner.run(&cmd, tool_paths);
    if result.is_none() {
        let engine_str = engine.to_string();
        let mut msg = format!("Drift correction with '{engine_str}' failed.");
        if matches!(engine, ResampleEngine::Rubberband) {
            msg.push_str(" (Ensure FFmpeg includes librubberband).");
        }
        log(&format!("    [ERROR] {msg}"));
        return false;
    }
    true
}
