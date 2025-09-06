// src/core/analysis.rs

use crate::core::process::CommandRunner;
use crate::core::{config::AppConfig, mkv_utils};
use hound::WavReader;
use ndarray::Array1;
use regex::Regex;
use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use tokio::fs;

#[derive(Debug, Clone, Copy)]
pub struct CorrelationResult {
    pub delay_ms: i64,
    pub match_pct: f64,
    pub start_time_s: f64,
}

/// Extracts a mono, 48kHz WAV audio chunk using ffmpeg.
async fn extract_audio_chunk(
    runner: &CommandRunner,
    source_file: &str,
    output_wav: &Path,
    start_time: f64,
    duration: f64,
    stream_index: usize,
) -> Result<(), String> {
    let result = runner
    .run(
        "ffmpeg",
         &[
             "-y",
         "-v",
         "error",
         "-ss",
         &start_time.to_string(),
         "-i",
         source_file,
         "-map",
         &format!("0:a:{}", stream_index),
         "-t",
         &duration.to_string(),
         "-vn",
         "-acodec",
         "pcm_s16le",
         "-ar",
         "48000",
         "-ac",
         "1",
         &output_wav.to_string_lossy(),
         ],
    )
    .await?;

    if result.exit_code == 0 {
        Ok(())
    } else {
        Err("ffmpeg failed to extract audio chunk".to_string())
    }
}

/// Calculates delay between two WAV files using cross-correlation.
fn find_audio_delay(ref_wav: &Path, sec_wav: &Path) -> Result<(i64, f64), String> {
    let mut ref_reader = WavReader::open(ref_wav).map_err(|e| e.to_string())?;
    let mut sec_reader = WavReader::open(sec_wav).map_err(|e| e.to_string())?;

    if ref_reader.spec().sample_rate != sec_reader.spec().sample_rate {
        return Err("Sample rates do not match".to_string());
    }
    let sample_rate = ref_reader.spec().sample_rate as f64;

    let ref_samples: Vec<f64> = ref_reader
    .samples::<i16>()
    .map(|s| s.unwrap_or(0) as f64)
    .collect();
    let sec_samples: Vec<f64> = sec_reader
    .samples::<i16>()
    .map(|s| s.unwrap_or(0) as f64)
    .collect();

    let mut ref_sig = Array1::from(ref_samples);
    let mut sec_sig = Array1::from(sec_samples);

    ref_sig -= ref_sig.mean().unwrap_or(0.0);
    sec_sig -= sec_sig.mean().unwrap_or(0.0);

    let ref_std = ref_sig.std(0.0);
    if ref_std > 1e-9 {
        ref_sig /= ref_std;
    }
    let sec_std = sec_sig.std(0.0);
    if sec_std > 1e-9 {
        sec_sig /= sec_std;
    }

    let n = ref_sig.len() + sec_sig.len() - 1;
    let n_fft = n.next_power_of_two();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n_fft);
    let ifft = planner.plan_fft_inverse(n_fft);

    let mut ref_buf: Vec<Complex<f64>> = ref_sig.iter().map(|&x| Complex::new(x, 0.0)).collect();
    ref_buf.resize(n_fft, Complex::default());
    fft.process(&mut ref_buf);

    let mut sec_buf: Vec<Complex<f64>> = sec_sig.iter().map(|&x| Complex::new(x, 0.0)).collect();
    sec_buf.resize(n_fft, Complex::default());
    fft.process(&mut sec_buf);

    for (r, s) in ref_buf.iter_mut().zip(sec_buf.iter()) {
        *r *= s.conj();
    }

    ifft.process(&mut ref_buf);
    let correlation: Array1<f64> = ref_buf.iter().map(|c| c.re / n_fft as f64).collect();

    // FIX: specify concrete float type to avoid E0689 ambiguity
    let mut max_corr: f64 = 0.0;
    let mut lag_samples = 0;
    for (i, &val) in correlation.iter().enumerate() {
        if val.abs() > max_corr.abs() {
            max_corr = val;
            lag_samples = i as i64;
        }
    }

    let lag_samples_linear = if lag_samples >= (n_fft as i64 / 2) {
        lag_samples - n_fft as i64
    } else {
        lag_samples
    };

    let delay_s = -(lag_samples_linear as f64) / sample_rate;
    let delay_ms = (delay_s * 1000.0).round() as i64;

    let norm_factor =
    (ref_sig.mapv(|x| x.powi(2)).sum() * sec_sig.mapv(|x| x.powi(2)).sum()).sqrt();
    let match_pct = if norm_factor > 1e-9 {
        (max_corr.abs() / norm_factor) * 100.0
    } else {
        0.0
    };

    Ok((delay_ms, match_pct))
}

/// Orchestrates the audio correlation workflow by analyzing chunks.
pub async fn run_audio_correlation(
    runner: &CommandRunner,
    config: &AppConfig,
    ref_file: &str,
    target_file: &str,
    temp_dir: &Path,
    ref_lang: Option<&str>,
    target_lang: Option<&str>,
) -> Result<Vec<CorrelationResult>, String> {
    let total_duration_s = mkv_utils::get_duration_s(runner, ref_file).await?;
    if total_duration_s < 30.0 {
        return Err("Reference file is too short for analysis".to_string());
    }

    let ref_audio_idx = mkv_utils::get_audio_stream_index(runner, ref_file, ref_lang)
    .await?
    .unwrap_or(0);
    let sec_audio_idx = mkv_utils::get_audio_stream_index(runner, target_file, target_lang)
    .await?
    .unwrap_or(0);

    runner.send_log(&format!(
        "[Analysis] Selected streams for correlation: REF(idx={}), Target(idx={})",
                             ref_audio_idx, sec_audio_idx
    ))
    .await;

    let mut results = Vec::new();
    let chunk_duration = config.scan_chunk_duration as f64;
    let num_chunks = config.scan_chunk_count;

    let scan_range = (total_duration_s * 0.8).max(0.0);
    let start_offset = total_duration_s * 0.1;
    let num_intervals = (num_chunks - 1).max(1);

    for i in 0..num_chunks {
        let start_time_s = start_offset + (scan_range / num_intervals as f64 * i as f64);

        let ref_wav = temp_dir.join(format!("ref_chunk_{}.wav", i));
        let sec_wav = temp_dir.join(format!("sec_chunk_{}.wav", i));

        let ref_task = extract_audio_chunk(
            runner,
            ref_file,
            &ref_wav,
            start_time_s,
            chunk_duration,
            ref_audio_idx,
        );
        let sec_task = extract_audio_chunk(
            runner,
            target_file,
            &sec_wav,
            start_time_s,
            chunk_duration,
            sec_audio_idx,
        );

        let (ref_res, sec_res) = tokio::join!(ref_task, sec_task);

        if ref_res.is_ok() && sec_res.is_ok() {
            if let Ok((delay_ms, match_pct)) = find_audio_delay(&ref_wav, &sec_wav) {
                runner
                .send_log(&format!(
                    "Chunk @{}s -> Delay {:+} ms (Match {:.2}%)",
                                   start_time_s.round() as i64,
                                   delay_ms,
                                   match_pct
                ))
                .await;
                results.push(CorrelationResult {
                    delay_ms,
                    match_pct,
                    start_time_s,
                });
            }
        }

        fs::remove_file(&ref_wav).await.ok();
        fs::remove_file(&sec_wav).await.ok();
    }

    Ok(results)
}

/// Finds the most consistent and strongest delay from a list of chunk results.
pub fn best_from_results(
    results: &[CorrelationResult],
    min_match_pct: f64,
) -> Option<CorrelationResult> {
    if results.is_empty() {
        return None;
    }

    let mut counts = HashMap::new();
    let valid_results: Vec<&CorrelationResult> = results
    .iter()
    .filter(|r| r.match_pct > min_match_pct)
    .collect();

    if valid_results.is_empty() {
        return None;
    }

    for r in &valid_results {
        *counts.entry(r.delay_ms).or_insert(0) += 1;
    }

    let max_freq = *counts.values().max().unwrap_or(&0);

    valid_results
    .into_iter()
    .filter(|r| counts.get(&r.delay_ms).unwrap_or(&0) == &max_freq)
    .max_by(|a, b| a.match_pct.partial_cmp(&b.match_pct).unwrap())
    .cloned()
}

/// Runs the videodiff tool and parses its final result line.
pub async fn run_videodiff(
    runner: &CommandRunner,
    ref_file: &str,
    target_file: &str,
    config: &AppConfig,
) -> Result<(i64, f64), String> {
    let videodiff_path = if !config.videodiff_path.is_empty() {
        &config.videodiff_path
    } else {
        "videodiff"
    };

    let result = runner.run(videodiff_path, &[ref_file, target_file]).await?;
    if result.exit_code != 0 {
        return Err("videodiff execution failed".to_string());
    }

    let re =
    Regex::new(r"(?i)\[Result\].*(itsoffset|ss)\s*:\s*(-?\d+\.?\d*)s.*?error:\s*([0-9.]+)")
    .unwrap();
    let last_result_line = result
    .stdout
    .lines()
    .filter(|line| line.contains("[Result]"))
    .last();

    if let Some(line) = last_result_line {
        if let Some(caps) = re.captures(line) {
            let kind = caps.get(1).map_or("", |m| m.as_str());
            let seconds = f64::from_str(caps.get(2).map_or("0.0", |m| m.as_str())).unwrap_or(0.0);
            let error_val =
            f64::from_str(caps.get(3).map_or("0.0", |m| m.as_str())).unwrap_or(0.0);

            let mut delay_ms = (seconds * 1000.0).round() as i64;
            if kind.eq_ignore_ascii_case("ss") {
                delay_ms = -delay_ms;
            }

            runner
            .send_log(&format!(
                "[VideoDiff] Result -> kind={} seconds={:.3} error={:.2} => delay_ms={}",
                kind, seconds, error_val, delay_ms
            ))
            .await;

            if error_val >= config.videodiff_error_min && error_val <= config.videodiff_error_max {
                return Ok((delay_ms, error_val));
            } else {
                return Err(format!(
                    "VideoDiff error ({:.2}) is outside the allowed bounds of [{:.2}, {:.2}].",
                                   error_val, config.videodiff_error_min, config.videodiff_error_max
                ));
            }
        }
    }

    Err("Could not find or parse a valid '[Result]' line from videodiff output.".to_string())
}
