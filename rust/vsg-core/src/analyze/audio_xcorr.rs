
use std::io::Read;
use std::process::{Command, Stdio};

use crate::error::VsgError;
use rustfft::{num_complex::Complex, num_traits::Zero, FftPlanner};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StereoMode {
    Mono,
    Left,
    Right,
    Mid,
    Best, // compute L/L and R/R, pick stronger peak
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Fft,
    Compat, // currently same as FFT correlation but kept for future exact Python parity
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    None,
    Voice, // ~100-3000 Hz
}

#[derive(Clone, Copy, Debug)]
pub struct XCorrParams {
    pub chunks: usize,
    pub chunk_dur_s: f64,
    pub sample_rate: u32,
    pub min_match: f64,
    pub stereo_mode: StereoMode,
    pub method: Method,
    pub band: Band,
}

#[derive(Debug)]
pub struct XCorrResult {
    pub delay_ns: i128,
    pub delay_ms: i64,
    pub peak_score: f32,
}

fn ffmpeg_filter_string(_stereo: bool, band: Band) -> String {
    let mut filters: Vec<String> = Vec::new();
    if band == Band::Voice {
        filters.push("highpass=f=100".into());
        filters.push("lowpass=f=3000".into());
    }
    if !filters.is_empty() { filters.join(",") } else { "anull".into() }
}

fn ffmpeg_decode_pcm(path: &str, sr: u32, stereo_mode: StereoMode, band: Band) -> Result<(Vec<f32>, Option<Vec<f32>>), VsgError> {
    let stereo = matches!(stereo_mode, StereoMode::Left | StereoMode::Right | StereoMode::Mid | StereoMode::Best);
    // Build ffmpeg command
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(path);
    let af = ffmpeg_filter_string(stereo, band);
    if af != "anull" { cmd.arg("-af").arg(&af); }
    if stereo { cmd.arg("-ac").arg("2"); } else { cmd.arg("-ac").arg("1"); }
    cmd.arg("-ar").arg(format!("{}", sr));
    cmd.arg("-f").arg("f32le");
    cmd.arg("-");
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| VsgError::Process(format!("spawn ffmpeg: {}", e)))?;
    let mut buf: Vec<u8> = Vec::new();
    child.stdout.as_mut().ok_or_else(|| VsgError::Process("no stdout".into()))?.read_to_end(&mut buf).map_err(|e| VsgError::Process(format!("ffmpeg read: {}", e)))?;
    let status = child.wait().map_err(|e| VsgError::Process(format!("ffmpeg wait: {}", e)))?;
    if !status.success() { return Err(VsgError::Process(format!("ffmpeg decode failed: {}", status))); }
    // Convert to f32
    let mut f: Vec<f32> = Vec::with_capacity(buf.len() / 4);
    let mut i = 0usize;
    while i + 3 < buf.len() {
        let bytes = [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]];
        f.push(f32::from_le_bytes(bytes));
        i += 4;
    }
    if stereo {
        // Split interleaved to L/R
        let mut l: Vec<f32> = Vec::with_capacity(f.len() / 2);
        let mut r: Vec<f32> = Vec::with_capacity(f.len() / 2);
        let mut idx = 0usize;
        while idx + 1 < f.len() {
            l.push(f[idx]);
            r.push(f[idx + 1]);
            idx += 2;
        }
        Ok((l, Some(r)))
    } else {
        Ok((f, None))
    }
}

fn select_chunk_centers(duration_s: f64, chunks: usize) -> Vec<f64> {
    if chunks == 0 { return vec![]; }
    let step = duration_s / ((chunks as f64) + 1.0);
    (1..=chunks).map(|k| k as f64 * step).collect()
}

fn slice_window(sig: &[f32], center_s: f64, chunk_dur_s: f64, sr: u32) -> Vec<f32> {
    let n = sig.len();
    let center = (center_s * sr as f64) as isize;
    let half = ((chunk_dur_s * sr as f64) / 2.0) as isize;
    let start = (center - half).max(0) as usize;
    let end = (center + half).min(n as isize) as usize;
    sig[start..end].to_vec()
}

fn next_pow2(mut v: usize) -> usize {
    if v == 0 { return 1; }
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    if std::mem::size_of::<usize>() == 8 { v |= v >> 32; }
    v + 1
}

fn xcorr_fft(ref_win: &[f32], other_win: &[f32]) -> (isize, f32) {
    // Zero-pad to next pow2 >= len(ref)+len(other)-1
    let conv_len = ref_win.len() + other_win.len() - 1;
    let fft_len = next_pow2(conv_len);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_len);
    let ifft = planner.plan_fft_inverse(fft_len);

    let mut a: Vec<Complex<f32>> = vec![Complex::zero(); fft_len];
    let mut b: Vec<Complex<f32>> = vec![Complex::zero(); fft_len];

    for (i, &v) in ref_win.iter().enumerate() { a[i].re = v; }
    for (i, &v) in other_win.iter().enumerate() { b[i].re = v; }

    fft.process(&mut a);
    fft.process(&mut b);
    for i in 0..fft_len { a[i] = a[i].conj() * b[i]; }
    ifft.process(&mut a);

    // Find peak
    let mut best_idx = 0usize;
    let mut best_val = f32::MIN;
    for i in 0..fft_len {
        let v = a[i].re;
        if v > best_val { best_val = v; best_idx = i; }
    }
    // Map to signed lag in samples (index 0 corresponds to -(ref_len-1))
    let lag_samples = best_idx as isize - (ref_win.len() as isize - 1);

    // Parabolic interpolation around peak for sub-sample
    let idx = best_idx as isize;
    let y1 = if idx > 0 { a[(idx - 1) as usize].re } else { a[idx as usize].re };
    let y2 = a[idx as usize].re;
    let y3 = if idx + 1 < a.len() as isize { a[(idx + 1) as usize].re } else { a[idx as usize].re };
    let denom = y1 - 2.0 * y2 + y3;
    let frac = if denom.abs() > 1e-6 { 0.5 * (y1 - y3) / denom } else { 0.0 };
    let refined = lag_samples as f32 + frac;

    (refined as isize, best_val)
}

fn choose_delay_ns(lags_ns: &[i128]) -> i128 {
    if lags_ns.is_empty() { 0 } else {
        let mut v = lags_ns.to_vec();
        v.sort();
        v[v.len() / 2]
    }
}

pub fn analyze_audio_xcorr(
    ref_audio: &str,
    other_audio: &str,
    duration_s: f64,
    params: &XCorrParams,
) -> Result<XCorrResult, VsgError> {
    let sr = params.sample_rate;

    // Decode according to stereo mode/band
    let (ref_l, ref_r_opt) = ffmpeg_decode_pcm(ref_audio, sr, params.stereo_mode, params.band)?;
    let (oth_l, oth_r_opt) = ffmpeg_decode_pcm(other_audio, sr, params.stereo_mode, params.band)?;

    // Build the channel views according to stereo_mode
    match params.stereo_mode {
        StereoMode::Mono => analyze_audio_xcorr_raw(&ref_l, &oth_l, duration_s, params),
        StereoMode::Left => analyze_audio_xcorr_raw(&ref_l, &oth_l, duration_s, params),
        StereoMode::Right => {
            let rr = ref_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let or = oth_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            analyze_audio_xcorr_raw(rr, or, duration_s, params)
        }
        StereoMode::Mid => {
            let rr = ref_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let or = oth_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let ref_mid: Vec<f32> = ref_l.iter().zip(rr.iter()).map(|(l, r)| 0.5 * (*l + *r)).collect();
            let oth_mid: Vec<f32> = oth_l.iter().zip(or.iter()).map(|(l, r)| 0.5 * (*l + *r)).collect();
            analyze_audio_xcorr_raw(&ref_mid, &oth_mid, duration_s, params)
        }
        StereoMode::Best => analyze_audio_xcorr_best_lr(&ref_l, ref_r_opt.as_deref(), &oth_l, oth_r_opt.as_deref(), duration_s, params),
    }
}

fn analyze_audio_xcorr_best_lr(
    ref_l: &[f32],
    ref_r: Option<&[f32]>,
    oth_l: &[f32],
    oth_r: Option<&[f32]>,
    duration_s: f64,
    params: &XCorrParams,
) -> Result<XCorrResult, VsgError> {
    let sr = params.sample_rate;
    let centers = select_chunk_centers(duration_s, params.chunks);
    let mut lags_ns: Vec<i128> = Vec::new();
    let mut best_peak = 0.0f32;

    for c in centers {
        let rw_l = slice_window(ref_l, c, params.chunk_dur_s, sr);
        let ow_l = slice_window(oth_l, c, params.chunk_dur_s, sr);
        let (lag_l, peak_l) = xcorr_fft(&rw_l, &ow_l);

        let (lag_samp, peak) = if let (Some(rr), Some(or)) = (ref_r, oth_r) {
            let rw_r = slice_window(rr, c, params.chunk_dur_s, sr);
            let ow_r = slice_window(or, c, params.chunk_dur_s, sr);
            let (lag_r, peak_r) = xcorr_fft(&rw_r, &ow_r);
            if peak_r > peak_l { (lag_r, peak_r) } else { (lag_l, peak_l) }
        } else {
            (lag_l, peak_l)
        };
        if peak > best_peak { best_peak = peak; }

        let ns_per_sample = 1_000_000_000.0 / (sr as f32);
        lags_ns.push((lag_samp as f32 * ns_per_sample) as i128);
    }

    let delay_ns = choose_delay_ns(&lags_ns);
    let ms = (delay_ns as f64) / 1.0e6;
    let delay_ms = ms.round() as i64;
    Ok(XCorrResult { delay_ns, delay_ms, peak_score: best_peak })
}

fn analyze_audio_xcorr_raw(
    ref_sig: &[f32],
    oth_sig: &[f32],
    duration_s: f64,
    params: &XCorrParams,
) -> Result<XCorrResult, VsgError> {
    let sr = params.sample_rate;
    let centers = select_chunk_centers(duration_s, params.chunks);
    let mut lags_ns: Vec<i128> = Vec::new();
    let mut best_peak = 0.0f32;

    for c in centers {
        let rw = slice_window(ref_sig, c, params.chunk_dur_s, sr);
        let ow = slice_window(oth_sig, c, params.chunk_dur_s, sr);
        let (lag_samp, peak) = xcorr_fft(&rw, &ow);
        if peak > best_peak { best_peak = peak; }
        let ns_per_sample = 1_000_000_000.0 / (sr as f32);
        lags_ns.push((lag_samp as f32 * ns_per_sample) as i128);
    }

    let delay_ns = choose_delay_ns(&lags_ns);
    let ms = (delay_ns as f64) / 1.0e6;
    let delay_ms = ms.round() as i64;
    Ok(XCorrResult { delay_ns, delay_ms, peak_score: best_peak })
}
