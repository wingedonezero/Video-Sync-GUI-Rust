// crates/vsg-core/src/analysis/xcorr.rs
use super::{AnalyzeParams, AnalysisResult, FinalResult, PassDetail};
use anyhow::Result;
use std::path::Path;
use crate::analysis::ffmpeg_decode::decode_to_f32_mono_48k;

fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| {
        let x = (std::f32::consts::PI*2.0*i as f32)/(n as f32);
        0.5*(1.0 - x.cos())
    }).collect()
}

fn ncc(a: &[f32], b: &[f32], max_shift: usize) -> (isize, f32) {
    // Deterministic normalized cross-correlation within ±max_shift (samples).
    let n = a.len().min(b.len());
    let mut best_shift = 0isize;
    let mut best = -1.0f32;
    let wa = hann(n);
    let wb = hann(n);
    for s in -(max_shift as isize)..=(max_shift as isize) {
        let (mut num, mut da, mut db) = (0f64,0f64,0f64);
        let mut count = 0usize;
        for i in 0..n {
            let j = (i as isize + s);
            if j < 0 || j >= n as isize { continue; }
            let va = a[i] as f64 * wa[i] as f64;
            let vb = b[j as usize] as f64 * wb[j as usize] as f64;
            num += va*vb;
            da += va*va;
            db += vb*vb;
            count += 1;
        }
        if count > n/3 {
            let denom = (da.sqrt()*db.sqrt()) as f32;
            if denom > 0.0 {
                let score = (num as f32)/denom;
                if score > best {
                    best = score;
                    best_shift = s;
                }
            }
        }
    }
    (best_shift, best)
}

pub fn analyze_pair(reference: &Path, target: &Path, ffmpeg: &str, params: &AnalyzeParams) -> Result<AnalysisResult> {
    // decode both sides
    let mut a = decode_to_f32_mono_48k(reference, ffmpeg)?;
    let mut b = decode_to_f32_mono_48k(target, ffmpeg)?;

    // normalize RMS to ~ -20 dBFS (target RMS ~0.1)
    let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64)*(*v as f64)).sum::<f64>() / (x.len().max(1) as f64)).sqrt() as f32;
    let ar = rms(&a);
    let br = rms(&b);
    if ar > 0.0 { let g = 0.1f32/ar; for v in &mut a { *v *= g; } }
    if br > 0.0 { let g = 0.1f32/br; for v in &mut b { *v *= g; } }

    // parameters
    let sr = params.sample_rate as f32;
    let pass_len = (params.chunk_ms as f32/1000.0*sr) as usize;
    let hop = (params.hop_ms as f32/1000.0*sr) as usize;
    let max_shift = (params.max_shift_ms as f32/1000.0*sr) as usize;

    // choose evenly spaced passes across the shorter side
    let total = a.len().min(b.len());
    let stride = ((total.saturating_sub(pass_len)) / params.passes.max(1)) .max(hop).max(1);

    let mut passes = Vec::new();

    for p in 0..params.passes {
        let start = p*stride;
        if start+pass_len > total { break; }
        let a_win = &a[start..start+pass_len];
        let b_win = &b[start..start+pass_len];

        // split into sub-chunks with hop to get multiple votes
        let mut chunk_scores = Vec::new();
        let mut chunk_peaks = Vec::new();
        let mut inliers = 0usize;
        let mut total_chunks = 0usize;
        let mut votes: Vec<(isize, f32)> = Vec::new();

        let mut cstart = 0usize;
        while cstart + hop <= pass_len {
            let cend = (cstart + hop*2).min(pass_len);
            if cend - cstart < hop { break; }
            let (shift, score) = ncc(&a_win[cstart..cend], &b_win[cstart..cend], max_shift);
            total_chunks += 1;
            if score >= params.min_match {
                inliers += 1;
                votes.push((shift, score));
            }
            chunk_scores.push(score);
            let ms = (shift as f32 / sr)*1000.0;
            chunk_peaks.push(ms);
            cstart += hop;
        }

        // select pass shift: median of rounded ms among inlier votes
        let mut rounded: Vec<i64> = votes.iter().map(|(s,_sc)| (( *s as f32 / sr)*1000.0).round() as i64).collect();
        rounded.sort();
        let pass_shift_ms = if rounded.is_empty() { 0 } else { rounded[rounded.len()/2] };
        // confidence: median of inlier scores
        let mut scs: Vec<f32> = votes.iter().map(|(_s,sc)| *sc).collect();
        scs.sort_by(|a,b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let conf = if scs.is_empty() { 0.0 } else { scs[scs.len()/2] };

        passes.push(PassDetail{
            index: p+1,
            start_ms: ((start as f32/sr)*1000.0) as i64,
            end_ms: (((start+pass_len) as f32/sr)*1000.0) as i64,
            inliers,
            total_chunks,
            confidence: conf,
            shift_ms: pass_shift_ms as f64,
        });
    }

    // Final selection per your rule:
    // 1) group passes by rounded ms -> take group with max inliers sum
    // 2) tie-break by higher average confidence
    use std::collections::HashMap;
    let mut groups: HashMap<i64, (usize, f32, usize)> = HashMap::new(); // shift -> (inliers_sum, avg_conf_acc, count)
    for p in &passes {
        let e = groups.entry(p.shift_ms as i64).or_insert((0, 0.0, 0));
        e.0 += p.inliers;
        e.1 += p.confidence;
        e.2 += 1;
    }
    let mut best_shift = 0i64;
    let mut best_inliers = 0usize;
    let mut best_avg = 0f32;
    for (k,(inl, avg_acc, cnt)) in groups {
        let avg = if cnt>0 { avg_acc/(cnt as f32) } else { 0.0 };
        if inl > best_inliers || (inl == best_inliers && avg > best_avg) {
            best_inliers = inl;
            best_avg = avg;
            best_shift = k;
        }
    }

    let final_res = FinalResult{
        global_shift_ms: best_shift,
        confidence: best_avg,
        passes_used: passes.len(),
        total_passes: passes.len(),
    };

    Ok(AnalysisResult{
        method: "audio_xcorr_fft".into(),
        sample_rate_hz: params.sample_rate,
        chunk_ms: params.chunk_ms,
        hop_ms: params.hop_ms,
        search_window_ms: params.max_shift_ms,
        min_match: params.min_match,
        mode: "previous".into(),
        passes,
        result: final_res,
    })
}
