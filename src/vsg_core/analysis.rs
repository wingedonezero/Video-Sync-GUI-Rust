// src/vsg_core/analysis.rs

use crate::config::Config;
use crate::process;
use anyhow::{anyhow, Result};
use hound::{WavReader, WavSpec};
use ndarray::{Array1, ArrayView1};
use num_complex::Complex;
use regex::Regex;
use rustfft::{FftPlanner, FftDirection};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use which::which;

// --- Data Structures ---

#[derive(Debug, Clone, Copy)]
pub struct CorrelationResult {
    pub delay_ms: i64,
    pub match_pct: f64,
    pub start_time: f64,
}

#[derive(Deserialize, Debug)]
struct MkvMergeTrackProperties {
    language: Option<String>,
}

#[derive(Deserialize, Debug)]
struct MkvMergeTrack {
    id: u64,
    #[serde(rename = "type")]
    track_type: String,
    properties: MkvMergeTrackProperties,
}

#[derive(Deserialize, Debug)]
struct MkvMergeOutput {
    tracks: Vec<MkvMergeTrack>,
}

// --- Public API ---

/// Orchestrates the audio correlation workflow by analyzing chunks.
pub fn run_audio_correlation<F>(
    config: &Config,
    ref_file: &Path,
    target_file: &Path,
    temp_dir: &Path,
    ref_lang: Option<&str>,
    target_lang: Option<&str>,
    log_callback: Arc<Mutex<F>>,
) -> Result<Vec<CorrelationResult>>
where
F: FnMut(String) + Send + 'static,
{
    // 1. Find audio stream indices
    let ref_idx = get_audio_stream_index(config, ref_file, ref_lang, Arc::clone(&log_callback))?
    .ok_or_else(|| anyhow!("Could not find a suitable audio stream in reference file"))?;
    let target_idx = get_audio_stream_index(config, target_file, target_lang, Arc::clone(&log_callback))?
    .ok_or_else(|| anyhow!("Could not find a suitable audio stream in target file"))?;

    log_callback.lock().unwrap()(format!(
        "Selected streams for analysis: REF (lang='{}', index={}), TGT (lang='{}', index={})",
                                         ref_lang.unwrap_or("first"), ref_idx, target_lang.unwrap_or("first"), target_idx
    ));

    // 2. Get reference file duration
    let duration_str = process::run_command(
        config,
        "ffprobe",
        &["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", ref_file.to_str().unwrap()],
                                            Arc::clone(&log_callback),
    )?;
    let duration = duration_str.trim().parse::<f64>()?;

    // 3. Calculate chunk start times, distributed across the middle 80% of the file
    let chunks = config.scan_chunk_count as f64;
    let scan_range = (duration * 0.8).max(0.0);
    let start_offset = duration * 0.1;
    let start_times: Vec<f64> = (0..config.scan_chunk_count)
    .map(|i| {
        start_offset + (scan_range / (chunks - 1.0).max(1.0) * (i as f64))
    })
    .collect();

    // 4. Process each chunk
    let mut results = Vec::new();
    for (i, &start_time) in start_times.iter().enumerate() {
        let ref_wav = temp_dir.join(format!("ref_chunk_{}.wav", i));
        let target_wav = temp_dir.join(format!("target_chunk_{}.wav", i));

        extract_audio_chunk(config, ref_file, &ref_wav, start_time, config.scan_chunk_duration as f64, ref_idx, Arc::clone(&log_callback))?;
        extract_audio_chunk(config, target_file, &target_wav, start_time, config.scan_chunk_duration as f64, target_idx, Arc::clone(&log_callback))?;

        if let Ok(result) = find_audio_delay(&ref_wav, &target_wav) {
            log_callback.lock().unwrap()(format!(
                "Chunk @{:.0}s -> Delay {:+} ms (Match {:.2f}%)",
                                                 start_time, result.delay_ms, result.match_pct
            ));
            results.push(CorrelationResult { start_time, ..result });
        } else {
            log_callback.lock().unwrap()(format!(
                "Chunk @{:.0}s -> Correlation failed.", start_time
            ));
        }

        // Cleanup
        std::fs::remove_file(ref_wav)?;
        std::fs::remove_file(target_wav)?;
    }

    Ok(results)
}


/// Finds the best delay from a list of correlation results.
pub fn best_from_results(config: &Config, results: &[CorrelationResult]) -> Option<CorrelationResult> {
    if results.is_empty() {
        return None;
    }

    // Filter out low-confidence chunks
    let valid_results: Vec<_> = results.iter().filter(|r| r.match_pct >= config.min_match_pct).collect();
    if valid_results.is_empty() {
        return None;
    }

    // Tally the most frequent delay
    let mut counts = HashMap::new();
    for r in &valid_results {
        *counts.entry(r.delay_ms).or_insert(0) += 1;
    }

    let max_freq = *counts.values().max()?;

    // Get all delays with that frequency
    let contenders: Vec<_> = counts.into_iter().filter(|(_, freq)| *freq == max_freq).map(|(delay, _)| delay).collect();

    // Among the contenders, find the one with the highest match percentage
    valid_results.into_iter()
    .filter(|r| contenders.contains(&r.delay_ms))
    .max_by(|a, b| a.match_pct.partial_cmp(&b.match_pct).unwrap_or(std::cmp::Ordering::Equal))
    .cloned()
}

/// Runs the videodiff tool and parses its final result line.
pub fn run_videodiff<F>(
    config: &Config,
    ref_file: &Path,
    target_file: &Path,
    log_callback: Arc<Mutex<F>>,
) -> Result<(i64, f64)>
where
F: FnMut(String) + Send + 'static,
{
    let videodiff_path = if !config.videodiff_path.is_empty() {
        config.videodiff_path.clone()
    } else {
        which("videodiff")?.to_str().unwrap().to_string()
    };

    let output = process::run_command(
        config,
        &videodiff_path,
        &[ref_file.to_str().unwrap(), target_file.to_str().unwrap()],
                                      log_callback,
    )?;

    let result_re = Regex::new(r"(?i)\[Result\].*(itsoffset|ss)\s*:\s*(-?\d+(?:\.\d+)?)s.*?error:\s*([0-9.]+)")?;

    // Search from the end of the output for the last result line
    for line in output.lines().rev() {
        if let Some(caps) = result_re.captures(line) {
            let kind = caps.get(1).unwrap().as_str();
            let s_val: f64 = caps.get(2).unwrap().as_str().parse()?;
            let err_val: f64 = caps.get(3).unwrap().as_str().parse()?;

            let mut delay_ms = (s_val * 1000.0).round() as i64;
            if kind.eq_ignore_ascii_case("ss") {
                delay_ms = -delay_ms;
            }

            if err_val >= config.videodiff_error_min && err_val <= config.videodiff_error_max {
                return Ok((delay_ms, err_val));
            } else {
                return Err(anyhow!(
                    "VideoDiff error ({:.2}) is outside the allowed bounds [{:.2}, {:.2}]",
                                   err_val, config.videodiff_error_min, config.videodiff_error_max
                ));
            }
        }
    }

    Err(anyhow!("Could not find a valid '[Result]' line in videodiff output."))
}

// --- Private Helper Functions ---

/// Finds the index of the first audio stream, optionally matching a language.
fn get_audio_stream_index<F>(
    config: &Config,
    mkv_path: &Path,
    language: Option<&str>,
    log_callback: Arc<Mutex<F>>,
) -> Result<Option<usize>>
where
F: FnMut(String) + Send + 'static,
{
    let output = process::run_command(config, "mkvmerge", &["-J", mkv_path.to_str().unwrap()], log_callback)?;
    let data: MkvMergeOutput = serde_json::from_str(&output)?;

    let mut audio_track_counter: usize = 0;
    let mut first_audio_idx: Option<usize> = None;

    for track in data.tracks {
        if track.track_type == "audio" {
            if first_audio_idx.is_none() {
                first_audio_idx = Some(audio_track_counter);
            }
            if let Some(lang_code) = language {
                if track.properties.language.as_deref() == Some(lang_code) {
                    return Ok(Some(audio_track_counter));
                }
            }
            audio_track_counter += 1;
        }
    }
    Ok(first_audio_idx)
}

/// Extracts a mono, 48kHz WAV audio chunk using ffmpeg.
fn extract_audio_chunk<F>(
    config: &Config,
    source_file: &Path,
    output_wav: &Path,
    start_time: f64,
    duration: f64,
    stream_index: usize,
    log_callback: Arc<Mutex<F>>,
) -> Result<()>
where
F: FnMut(String) + Send + 'static,
{
    process::run_command(
        config,
        "ffmpeg",
        &[
            "-y", "-v", "error", "-ss", &start_time.to_string(),
                         "-i", source_file.to_str().unwrap(),
                         "-map", &format!("0:a:{}", stream_index),
                         "-t", &duration.to_string(),
                         "-vn", "-acodec", "pcm_s16le", "-ar", "48000", "-ac", "1",
                         output_wav.to_str().unwrap(),
        ],
        log_callback,
    )?;
    Ok(())
}

/// Calculates delay between two WAV files using cross-correlation.
fn find_audio_delay(ref_wav: &Path, sec_wav: &Path) -> Result<CorrelationResult> {
    let (ref_sig, spec) = read_wav_to_f32(ref_wav)?;
    let (sec_sig, _) = read_wav_to_f32(sec_wav)?;

    let ref_norm = normalize_signal(ref_sig.view());
    let sec_norm = normalize_signal(sec_sig.view());

    // Perform cross-correlation using FFT
    let correlation = cross_correlate(&ref_norm, &sec_norm);

    // Find the lag
    let mut max_corr = f32::NEG_INFINITY;
    let mut lag_idx = 0;
    for (i, &val) in correlation.iter().enumerate() {
        if val > max_corr {
            max_corr = val;
            lag_idx = i;
        }
    }

    // Adjust lag to match scipy's 'full' mode result
    let lag_samples = lag_idx as i64 - (sec_norm.len() - 1) as i64;

    let delay_s = lag_samples as f64 / spec.sample_rate as f64;
    let delay_ms = (delay_s * 1000.0).round() as i64;

    // Calculate match percentage
    let norm_factor = (ref_norm.dot(&ref_norm) * sec_norm.dot(&sec_norm)).sqrt() as f64;
    let match_pct = (max_corr as f64 / (norm_factor + 1e-9)) * 100.0;

    Ok(CorrelationResult { delay_ms, match_pct, start_time: 0.0 })
}

// --- Signal Processing Helpers ---

fn read_wav_to_f32(path: &Path) -> Result<(Array1<f32>, WavSpec)> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();
    let samples: Vec<f32> = reader
    .samples::<i16>()
    .map(|s| s.unwrap() as f32 / 32768.0)
    .collect();
    Ok((Array1::from(samples), spec))
}

fn normalize_signal(signal: ArrayView1<f32>) -> Array1<f32> {
    let mean = signal.mean().unwrap_or(0.0);
    let std_dev = signal.std(0.0);
    if std_dev < 1e-9 {
        Array1::zeros(signal.len())
    } else {
        (signal - mean) / std_dev
    }
}

fn cross_correlate(a: &Array1<f32>, b: &Array1<f32>) -> Array1<f32> {
    let n = a.len() + b.len() - 1;
    let fft_len = n.next_power_of_two();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft(fft_len, FftDirection::Forward);
    let ifft = planner.plan_fft(fft_len, FftDirection::Inverse);

    let mut a_padded = vec![Complex::new(0.0, 0.0); fft_len];
    for (i, &val) in a.iter().enumerate() { a_padded[i].re = val; }

    let mut b_padded = vec![Complex::new(0.0, 0.0); fft_len];
    for (i, &val) in b.iter().enumerate() { b_padded[i].re = val; }

    fft.process(&mut a_padded);
    fft.process(&mut b_padded);

    for (a_val, b_val) in a_padded.iter_mut().zip(b_padded.iter()) {
        *a_val *= b_val.conj();
    }

    ifft.process(&mut a_padded);

    let scale = 1.0 / fft_len as f32;
    a_padded.iter().take(n).map(|c| c.re * scale).collect()
}
