// src/core/analysis.rs
//
// Rust port of vsg_core/analysis.py
// - Audio stream discovery via mkvmerge JSON
// - WAV chunk extraction via ffmpeg
// - Cross-correlation (FFT-based), z-score normalize
// - Chunk scan orchestration w/ configuration parity
// - VideoDiff execution & [Result] parse
//
// Notes:
// * We expect chunks to be mono 48k PCM (as extracted).
// * Delay = argmax(crosscorr) - (len(sec)-1) samples, then ms.
// * Match% computed like Python: max|corr| / sqrt(sum(x^2)*sum(y^2)) * 100.

use crate::core::command_runner::CommandRunner;
use crate::core::mkv_utils;
use hound::{WavReader, WavSpec};
use regex::Regex;
use serde_json::{Map as JsonMap, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CorrelationResult {
    pub delay: i64,        // ms rounded
    pub match_pct: f64,    // %
    pub raw_delay_s: f64,  // seconds (signed)
    pub start: f64,        // chunk start seconds
}

pub fn get_audio_stream_index(
    mkv_path: &str,
    runner: &CommandRunner,
    language: Option<&str>,
) -> Option<usize> {
    let info = mkv_utils::get_stream_info(mkv_path, runner)?;
    let mut audio_idx = usize::MAX; // will start at 0 when we see first audio
    let mut first_found: Option<usize> = None;

    for track in info.get("tracks").and_then(|t| t.as_array()).unwrap_or(&vec![]) {
        if track.get("type").and_then(|v| v.as_str()) == Some("audio") {
            audio_idx = if audio_idx == usize::MAX { 0 } else { audio_idx + 1 };
            if first_found.is_none() {
                first_found = Some(audio_idx);
            }
            if let Some(lang_want) = language {
                let lang = track
                .get("properties")
                .and_then(|p| p.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
                if lang == lang_want {
                    return Some(audio_idx);
                }
            }
        }
    }
    first_found
}

pub fn extract_audio_chunk(
    source_file: &str,
    output_wav: &str,
    start_time: f64,
    duration: f64,
    runner: &CommandRunner,
    stream_index: usize,
) -> bool {
    // ffmpeg -y -v error -ss <start> -i <src> -map 0:a:<idx> -t <dur> -vn -acodec pcm_s16le -ar 48000 -ac 1 <out.wav>
    let cmd = [
        "ffmpeg",
        "-y",
        "-v",
        "error",
        "-ss",
        &format!("{}", start_time),
        "-i",
        source_file,
        "-map",
        &format!("0:a:{}", stream_index),
        "-t",
        &format!("{}", duration),
        "-vn",
        "-acodec",
        "pcm_s16le",
        "-ar",
        "48000",
        "-ac",
        "1",
        output_wav,
    ];
    runner.run(&cmd).is_some()
}

fn read_wav_mono_f32(path: &Path) -> Option<(Vec<f32>, u32)> {
    let mut reader = WavReader::open(path).ok()?;
    let spec: WavSpec = reader.spec();
    let sample_rate = spec.sample_rate;

    // Support common formats: PCM 16/24/32, float
    // Convert to f32
    let mut buf: Vec<f32> = Vec::new();

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            if bits <= 16 {
                for s in reader.samples::<i16>() {
                    let v = s.ok()? as f32 / 32768.0;
                    buf.push(v);
                }
            } else if bits <= 24 {
                // hound doesn't expose i24; read as i32 and scale
                for s in reader.samples::<i32>() {
                    let v = s.ok()? as f32 / 8_388_608.0; // 2^23
                    buf.push(v);
                }
            } else {
                for s in reader.samples::<i32>() {
                    let v = s.ok()? as f32 / 2_147_483_648.0; // 2^31
                    buf.push(v);
                }
            }
        }
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                buf.push(s.ok()?);
            }
        }
    }

    // If channels > 1 (shouldn't happen due to -ac 1), downmix naive
    let channels = spec.channels.max(1);
    if channels > 1 {
        let mut mono = Vec::with_capacity(buf.len() / channels as usize);
        for frame in buf.chunks(channels as usize) {
            let sum: f32 = frame.iter().copied().sum();
            mono.push(sum / channels as f32);
        }
        return Some((mono, sample_rate));
    }
    Some((buf, sample_rate))
}

fn zscore_normalize(x: &mut [f32]) {
    if x.is_empty() {
        return;
    }
    let mean = x.iter().copied().sum::<f32>() / x.len() as f32;
    let mut var = 0.0f32;
    for v in x.iter() {
        let d = *v - mean;
        var += d * d;
    }
    var /= x.len().max(1) as f32;
    let std = var.sqrt();
    let denom = if std <= 1e-9 { 1e-9 } else { std };
    for v in x.iter_mut() {
        *v = (*v - mean) / denom;
    }
}

fn cross_correlation_fft(a: &[f32], b: &[f32]) -> Vec<f64> {
    // full correlation via FFT:
    // corr = ifft( FFT(a_pad) * conj(FFT(b_pad)) )
    use rustfft::{num_complex::Complex, FftPlanner};

    let n = a.len();
    let m = b.len();
    let size = (n + m - 1).next_power_of_two();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(size);
    let ifft = planner.plan_fft_inverse(size);

    let mut a_freq = vec![Complex::<f64>::new(0.0, 0.0); size];
    let mut b_freq = vec![Complex::<f64>::new(0.0, 0.0); size];

    for (i, &v) in a.iter().enumerate() {
        a_freq[i].re = v as f64;
    }
    for (i, &v) in b.iter().enumerate() {
        b_freq[i].re = v as f64;
    }

    fft.process(&mut a_freq);
    fft.process(&mut b_freq);

    // conj on b
    for x in b_freq.iter_mut() {
        x.im = -x.im;
    }

    // pointwise multiply
    let mut prod = vec![Complex::<f64>::new(0.0, 0.0); size];
    for i in 0..size {
        prod[i] = a_freq[i] * b_freq[i];
    }

    // inverse
    ifft.process(&mut prod);

    // normalize by size (rustfft doesn't normalize inverse)
    let norm = size as f64;
    for x in prod.iter_mut() {
        x.re /= norm;
        x.im /= norm;
    }

    // The correlation sequence length is n+m-1
    prod[..(n + m - 1)]
    .iter()
    .map(|c| c.re)
    .collect::<Vec<f64>>()
}

pub fn find_audio_delay(
    ref_wav: &Path,
    sec_wav: &Path,
    log: &CommandRunner,
) -> (Option<i64>, f64, Option<f64>) {
    // Load WAVs
    let (mut r, rate_r) = match read_wav_mono_f32(ref_wav) {
        Some(t) => t,
        None => {
            log.log("Error in find_audio_delay: failed to read reference WAV");
            return (None, 0.0, None);
        }
    };
    let (mut s, rate_s) = match read_wav_mono_f32(sec_wav) {
        Some(t) => t,
        None => {
            log.log("Error in find_audio_delay: failed to read secondary WAV");
            return (None, 0.0, None);
        }
    };

    if rate_r != rate_s {
        log.log("Sample rates do not match, skipping correlation.");
        return (None, 0.0, None);
    }

    zscore_normalize(&mut r);
    zscore_normalize(&mut s);

    let corr = cross_correlation_fft(&r, &s);
    // argmax
    let mut argmax = 0usize;
    let mut vmax = f64::NEG_INFINITY;
    for (i, &v) in corr.iter().enumerate() {
        if v > vmax {
            vmax = v;
            argmax = i;
        }
    }
    let lag_samples = argmax as i64 - (s.len() as i64 - 1);
    let raw_delay_s = lag_samples as f64 / rate_r as f64;

    // norm factor
    let sumsq_r: f64 = r.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let sumsq_s: f64 = s.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let norm_factor = (sumsq_r * sumsq_s).sqrt().max(1e-9);
    let match_pct = (corr.iter().map(|v| v.abs()).fold(0.0_f64, f64::max) / norm_factor) * 100.0;

    (Some((raw_delay_s * 1000.0).round() as i64), match_pct, Some(raw_delay_s))
}

pub fn run_audio_correlation(
    ref_file: &str,
    target_file: &str,
    temp_dir: &Path,
    config: &JsonMap<String, Value>,
    runner: &CommandRunner,
    ref_lang: Option<&str>,
    target_lang: Option<&str>,
    role_tag: &str,
) -> Vec<CorrelationResult> {
    // select streams
    let idx1 = get_audio_stream_index(ref_file, runner, ref_lang);
    let idx2 = get_audio_stream_index(target_file, runner, target_lang);

    runner.log(&format!(
        "Selected streams for analysis: REF (lang='{}', index={}), {} (lang='{}', index={})",
                        ref_lang.unwrap_or("first"),
                        idx1.map(|x| x as i64).unwrap_or(-1),
                        role_tag.to_uppercase(),
                        target_lang.unwrap_or("first"),
                        idx2.map(|x| x as i64).unwrap_or(-1)
    ));

    if idx1.is_none() || idx2.is_none() {
        panic!("Could not locate required audio streams for correlation.");
    }
    let i1 = idx1.unwrap();
    let i2 = idx2.unwrap();

    // duration via ffprobe csv
    let out = runner.run(&[
        "ffprobe",
        "-v",
        "error",
        "-show_entries",
        "format=duration",
        "-of",
        "csv=p=0",
        ref_file,
    ]);
    let duration: f64 = out
    .as_ref()
    .and_then(|s| s.trim().parse::<f64>().ok())
    .unwrap_or(0.0);

    let chunks = config
    .get("scan_chunk_count")
    .and_then(|v| v.as_i64())
    .unwrap_or(10)
    .max(1) as usize;

    let chunk_dur = config
    .get("scan_chunk_duration")
    .and_then(|v| v.as_i64())
    .unwrap_or(15)
    .max(1) as f64;

    let scan_range = (duration * 0.8).max(0.0);
    let start_offset = duration * 0.1;
    let den = if chunks > 1 { (chunks - 1) as f64 } else { 1.0 };
    let mut starts = Vec::with_capacity(chunks);
    for i in 0..chunks {
        starts.push(start_offset + (scan_range / den) * i as f64);
    }

    let mut results: Vec<CorrelationResult> = Vec::new();

    for (i, start_time) in starts.iter().enumerate() {
        let tmp1 = temp_dir.join(format!(
            "wav_ref_{}_{}_{}.wav",
            Path::new(ref_file).file_stem().unwrap_or_default().to_string_lossy(),
                                         *start_time as i64,
                                         i + 1
        ));
        let tmp2 = temp_dir.join(format!(
            "wav_{}_{}_{}_{}.wav",
            role_tag,
            Path::new(target_file).file_stem().unwrap_or_default().to_string_lossy(),
                                         *start_time as i64,
                                         i + 1
        ));

        let mut cleanup = |p: &Path| {
            let _ = fs::remove_file(p);
        };

        // extract both chunks
        let ok1 = extract_audio_chunk(ref_file, &tmp1.to_string_lossy(), *start_time, chunk_dur, runner, i1);
        let ok2 = extract_audio_chunk(target_file, &tmp2.to_string_lossy(), *start_time, chunk_dur, runner, i2);

        if ok1 && ok2 {
            let (delay_opt, match_pct, raw_opt) = find_audio_delay(&tmp1, &tmp2, runner);
            if let Some(delay_ms) = delay_opt {
                results.push(CorrelationResult {
                    delay: delay_ms,
                    match_pct,
                    raw_delay_s: raw_opt.unwrap_or(0.0),
                             start: *start_time,
                });
                runner.log(&format!(
                    "Chunk @{}s -> Delay {:+} ms (Match {:.2}%)",
                                    *start_time as i64, delay_ms, match_pct
                ));
            }
        }

        cleanup(&tmp1);
        cleanup(&tmp2);
    }

    results
}

pub fn run_videodiff(
    ref_file: &str,
    target_file: &str,
    config: &JsonMap<String, Value>,
    runner: &CommandRunner,
) -> Result<(i64, f64), String> {
    // videodiff path: prefer config["videodiff_path"], else PATH
    let vd_path = config
    .get("videodiff_path")
    .and_then(|v| v.as_str())
    .filter(|s| !s.is_empty())
    .map(|s| s.to_string())
    .or_else(|| which::which("videodiff").ok().map(|p| p.to_string_lossy().to_string()))
    .ok_or_else(|| "videodiff executable not found".to_string())?;

    if !Path::new(&vd_path).exists() {
        return Err(format!("videodiff executable not found at '{}'", vd_path));
    }

    let out = runner.run(&[&vd_path, ref_file, target_file]).ok_or_else(|| {
        "videodiff produced no output or failed to run.".to_string()
    })?;

    // find last line containing [Result] and 'ss:' or 'itsoffset:'
    let mut last_line = String::new();
    for line in out.lines().rev() {
        if line.contains("[Result]") && (line.contains("ss:") || line.contains("itsoffset:")) {
            last_line = line.to_string();
            break;
        }
    }
    if last_line.is_empty() {
        return Err("Could not find a valid '[Result]' line in videodiff output.".into());
    }

    // parse kind:(ss|itsoffset) seconds and error
    let re = Regex::new(r"(itsoffset|ss)\s*:\s*(-?\d+(?:\.\d+)?)s.*?error:\s*([0-9.]+)")
    .unwrap();
    let caps = re
    .captures(&last_line)
    .ok_or_else(|| format!("Could not parse videodiff result line: '{}'", last_line))?;

    let kind = caps.get(1).unwrap().as_str().to_lowercase();
    let seconds: f64 = caps.get(2).unwrap().as_str().parse().unwrap_or(0.0);
    let mut delay_ms = (seconds * 1000.0).round() as i64;

    // parity: if kind == "ss", invert sign
    if kind == "ss" {
        delay_ms = -delay_ms;
    }

    let error_value: f64 = caps.get(3).unwrap().as_str().parse().unwrap_or(0.0);
    runner.log(&format!(
        "[VideoDiff] Result -> {} {:.5}s, error {:.2} => delay {:+} ms",
        kind, seconds, error_value, delay_ms
    ));

    Ok((delay_ms, error_value))
}

/// High-level analyzer used by the pipeline for each secondary/tertiary:
/// - when analysis_mode == "VideoDiff", run_videodiff (and caller enforces error bounds)
/// - else run_audio_correlation and pick the best candidate by match% & mode,
///   mirroring Python's _best_from_results strategy there (handled upstream).
pub fn analyze(
    ref_file: &str,
    other_file: Option<&str>,
    config: &JsonMap<String, Value>,
    runner: &CommandRunner,
) -> Option<i64> {
    let other = match other_file {
        Some(s) => s,
        None => return None,
    };
    let mode = config
    .get("analysis_mode")
    .and_then(|v| v.as_str())
    .unwrap_or("Audio Correlation");

    if mode.eq_ignore_ascii_case("VideoDiff") {
        match run_videodiff(ref_file, other, config, runner) {
            Ok((delay_ms, _err)) => Some(delay_ms),
            Err(e) => {
                runner.log(&format!("[ERROR] {}", e));
                None
            }
        }
    } else {
        // audio correlation over chunks, then choose "best" like pipeline does
        let ref_lang = config
        .get("analysis_lang_ref")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
        let other_lang = if other_file.is_some() && other == other_file.unwrap() {
            // decide by which role calls this
            // The pipeline calls this for "sec" first, then "ter"; we don't know the role here,
            // so we’ll just attempt with provided config; the caller _run_analysis picks best afterwards.
            config
            .get("analysis_lang_sec")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        } else {
            None
        };

        let temp_root = config
        .get("temp_root")
        .and_then(|v| v.as_str())
        .unwrap_or("temp_work");
        let temp_dir = PathBuf::from(temp_root);

        let role = "sec"; // label only; upstream logs will specify role and ter is called separately too
        let results = run_audio_correlation(
            ref_file,
            other,
            &temp_dir,
            config,
            runner,
            ref_lang,
            other_lang,
            role,
        );

        // Pick best result like Python's _best_from_results (pipeline method).
        // Here we mimic that filtering (min_match_pct) and selection.
        let min_pct = config
        .get("min_match_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

        let mut valid: Vec<&CorrelationResult> =
        results.iter().filter(|r| r.match_pct > min_pct).collect();
        if valid.is_empty() {
            return None;
        }

        // Count delays
        use std::collections::HashMap;
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for r in &valid {
            *counts.entry(r.delay).or_insert(0) += 1;
        }
        let max_freq = counts.values().copied().max().unwrap_or(1);
        let contenders: Vec<i64> = counts
        .into_iter()
        .filter_map(|(d, f)| if f == max_freq { Some(d) } else { None })
        .collect();

        // best of each contender by match_pct
        let mut best_overall: Option<&CorrelationResult> = None;
        let mut best_match = f64::MIN;

        for d in contenders {
            if let Some(best_for_d) = valid
                .iter()
                .copied()
                .filter(|r| r.delay == d)
                .max_by(|a, b| a.match_pct.partial_cmp(&b.match_pct).unwrap())
                {
                    if best_for_d.match_pct > best_match {
                        best_match = best_for_d.match_pct;
                        best_overall = Some(best_for_d);
                    }
                }
        }

        best_overall.map(|r| r.delay)
    }
}
