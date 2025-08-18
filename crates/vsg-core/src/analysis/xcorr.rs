
use std::io::Read;
use std::process::{Command, Stdio};
use anyhow::{anyhow, Context, Result};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct XcorrParams {
    pub chunk_sec: f32,
    pub chunks: usize,
    pub lag_ms: i64,
    pub min_match_pct: f32,
    pub ffmpeg_path: String,
    pub save_debug: Option<std::path::PathBuf>,
}
impl Default for XcorrParams {
    fn default() -> Self {
        Self {
            chunk_sec: 15.0,
            chunks: 10,
            lag_ms: 2000,
            min_match_pct: 20.0,
            ffmpeg_path: "ffmpeg".to_string(),
            save_debug: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowResult {
    pub index: usize,
    pub t0_ns: i128,
    pub best_lag_ms: i64,
    pub match_pct: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XcorrResult {
    pub raw_delay_ms: i64,
    pub votes: usize,
    pub avg_match_pct: f32,
    pub windows: Vec<WindowResult>,
}

pub fn analyze(reference: &std::path::Path, target: &std::path::Path, params: &XcorrParams) -> Result<XcorrResult> {
    let ref_ns = probe_duration_ns(reference, &params.ffmpeg_path)
        .with_context(|| "Failed to probe reference duration")?;

    let chunk_ns = (params.chunk_sec * 1_000_000_000.0) as i128;
    let mut t0s: Vec<i128> = Vec::new();

    if ref_ns <= chunk_ns {
        t0s.push(0);
    } else {
        let usable = ref_ns - chunk_ns;
        if params.chunks <= 1 {
            t0s.push(0);
        } else {
            let step = usable / (params.chunks as i128 - 1);
            for k in 0..params.chunks {
                t0s.push((k as i128) * step);
            }
        }
    }

    let mut results: Vec<WindowResult> = Vec::new();
    for (idx, &t0_ns) in t0s.iter().enumerate() {
        let ref_pcm = decode_window(reference, t0_ns, params.chunk_sec, &params.ffmpeg_path)?;
        let tgt_pcm = decode_window(target,     t0_ns, params.chunk_sec, &params.ffmpeg_path)?;
        if ref_pcm.len() != tgt_pcm.len() || ref_pcm.is_empty() { continue; }

        let (best_lag, best_r) = correlate_best_lag_ms(&ref_pcm, &tgt_pcm, params.lag_ms);
        let match_pct = ((best_r + 1.0) * 50.0).clamp(0.0, 100.0);
        results.push(WindowResult { index: idx, t0_ns, best_lag_ms: best_lag, match_pct });
    }

    if results.is_empty() {
        return Err(anyhow!("No valid chunks decoded"));
    }

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<i64, (usize, f32)> = BTreeMap::new();
    for r in &results {
        let e = buckets.entry(r.best_lag_ms).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += r.match_pct;
    }
    let mut best_lag = 0i64;
    let mut best_votes = 0usize;
    let mut best_avg = 0f32;
    for (lag, (votes, sum)) in buckets {
        let avg = sum / votes as f32;
        if votes > best_votes || (votes == best_votes && avg > best_avg) {
            best_lag = lag;
            best_votes = votes;
            best_avg = avg;
        }
    }
    if best_avg < params.min_match_pct {
        return Err(anyhow!("Winner below min_match_pct (avg={best_avg:.1}%)"));
    }

    let out = XcorrResult {
        raw_delay_ms: best_lag,
        votes: best_votes,
        avg_match_pct: best_avg,
        windows: results.clone(),
    };
    if let Some(path) = &params.save_debug {
        std::fs::write(path, serde_json::to_vec_pretty(&out)?)?;
    }
    Ok(out)
}

fn probe_duration_ns(path: &std::path::Path, ffmpeg: &str) -> Result<i128> {
    use std::process::Command;
    use std::process::Stdio;
    let out = Command::new(ffmpeg)
        .args(["-i", path.to_str().unwrap_or_default()])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .context("spawn ffmpeg -i")?
        .wait_with_output()?;
    let s = String::from_utf8_lossy(&out.stderr);
    for line in s.lines() {
        if let Some(pos) = line.find("Duration: ") {
            let ts = &line[pos + 10 ..].split(',').next().unwrap_or("").trim();
            if let Some(ns) = parse_hms_to_ns(ts) { return Ok(ns); }
        }
    }
    Err(anyhow!("Could not parse duration for {}", path.display()))
}

fn parse_hms_to_ns(hms: &str) -> Option<i128> {
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 { return None; }
    let h: i128 = parts[0].parse().ok()?;
    let m: i128 = parts[1].parse().ok()?;
    let s: f64  = parts[2].parse().ok()?;
    let total = (h * 3600 + m * 60) as f64 + s;
    Some((total * 1e9) as i128)
}

fn decode_window(path: &std::path::Path, t0_ns: i128, dur_sec: f32, ffmpeg: &str) -> Result<Vec<f32>> {
    use std::process::Command;
    use std::process::Stdio;
    let t0 = (t0_ns as f64) / 1e9;
    let dur = dur_sec as f64;
    let mut child = Command::new(ffmpeg)
        .args([
            "-ss", &format!("{:.9}", t0),
            "-t", &format!("{:.3}", dur),
            "-i", path.to_str().unwrap_or_default(),
            "-vn", "-ac", "1", "-ar", "8000",
            "-f", "f32le",
            "-hide_banner", "-nostats", "-loglevel", "error",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn ffmpeg decode {}", path.display()))?;

    let mut buf = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut buf)?;
    let status = child.wait()?;
    if !status.success() { return Err(anyhow!("ffmpeg decode failed for {}", path.display())); }
    if buf.len() % 4 != 0 { return Err(anyhow!("ffmpeg produced non-f32le size")); }
    let mut pcm = Vec::with_capacity(buf.len()/4);
    for ch in buf.chunks_exact(4) {
        pcm.push(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]));
    }
    Ok(pcm)
}

fn correlate_best_lag_ms(ref_pcm: &[f32], tgt_pcm: &[f32], max_lag_ms: i64) -> (i64, f32) {
    fn z(x: &[f32]) -> Vec<f32> {
        let mean = x.iter().copied().map(|v| v as f64).sum::<f64>() / (x.len() as f64);
        let var = x.iter().copied().map(|v| {
            let d = (v as f64) - mean;
            d*d
        }).sum::<f64>() / (x.len().max(1) as f64);
        let std = var.sqrt().max(1e-12);
        x.iter().map(|v| (((*v as f64) - mean)/std) as f32).collect()
    }
    let xr = z(ref_pcm);
    let yt = z(tgt_pcm);

    let max_s = (max_lag_ms.abs() as usize) * 8; // 8kHz -> 8 samples/ms
    let len = xr.len().min(yt.len());

    let mut best_r = -1.0f32;
    let mut best_s = 0isize;

    for s in -(max_s as isize)..=(max_s as isize) {
        let mut sum_xy = 0f64;
        let mut sum_x2 = 0f64;
        let mut sum_y2 = 0f64;
        let mut n = 0usize;

        for i in 0..len {
            let j = i as isize + s;
            if j < 0 || j >= len as isize { continue; }
            let x = xr[i] as f64;
            let y = yt[j as usize] as f64;
            sum_xy += x*y;
            sum_x2 += x*x;
            sum_y2 += y*y;
            n += 1;
        }
        if n < 64 { continue; }
        let denom = (sum_x2.sqrt() * sum_y2.sqrt());
        if denom <= 1e-12 { continue; }
        let r = (sum_xy / denom) as f32;
        if r > best_r {
            best_r = r;
            best_s = s;
        }
    }
    let best_lag_ms = (best_s as i64) / 8;
    (best_lag_ms, best_r)
}
