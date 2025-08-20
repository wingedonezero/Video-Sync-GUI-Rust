
use std::io::Read;
use std::process::{Command, Stdio};

use crate::error::VsgError;
use rustfft::{num_complex::Complex, num_traits::Zero, FftPlanner};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StereoMode { Mono, Left, Right, Mid, Best }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method { Fft, Compat }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band { None, Voice }

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
pub struct XCorrResult { pub delay_ns:i128, pub delay_ms:i64, pub peak_score:f32 }

#[derive(Debug, Clone)]
pub struct ChunkResult { pub center_s:f64, pub window_samples:usize, pub lag_ns:i128, pub lag_ms:f64, pub peak:f32 }

fn ffmpeg_filter_string(_stereo: bool, band: Band) -> String {
    let mut filters: Vec<String> = Vec::new();
    if band == Band::Voice {
        filters.push("highpass=f=100".into());
        filters.push("lowpass=f=3000".into());
    }
    if !filters.is_empty() { filters.join(",") } else { "anull".into() }
}

fn ffmpeg_decode_pcm(path:&str, sr:u32, stereo_mode:StereoMode, band:Band) -> Result<(Vec<f32>, Option<Vec<f32>>),VsgError>{
    let stereo = matches!(stereo_mode, StereoMode::Left|StereoMode::Right|StereoMode::Mid|StereoMode::Best);
    let mut cmd=Command::new("ffmpeg");
    cmd.arg("-i").arg(path);
    let af = ffmpeg_filter_string(stereo, band);
    if af != "anull" { cmd.arg("-af").arg(&af); }
    if stereo { cmd.arg("-ac").arg("2"); } else { cmd.arg("-ac").arg("1"); }
    cmd.arg("-ar").arg(format!("{}",sr)).arg("-f").arg("f32le").arg("-");
    let mut child=cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| VsgError::Process(format!("spawn ffmpeg: {}",e)))?;
    let mut buf:Vec<u8>=Vec::new();
    child.stdout.as_mut().ok_or_else(|| VsgError::Process("no stdout".into()))?.read_to_end(&mut buf).map_err(|e| VsgError::Process(format!("ffmpeg read: {}",e)))?;
    let status=child.wait().map_err(|e| VsgError::Process(format!("ffmpeg wait: {}",e)))?;
    if !status.success(){ return Err(VsgError::Process(format!("ffmpeg decode failed: {}",status)));}
    // bytes->f32
    let mut f:Vec<f32>=Vec::with_capacity(buf.len()/4);
    let mut i=0usize; while i+3<buf.len(){ f.push(f32::from_le_bytes([buf[i],buf[i+1],buf[i+2],buf[i+3]])); i+=4; }
    if stereo {
        let mut l:Vec<f32>=Vec::with_capacity(f.len()/2);
        let mut r:Vec<f32>=Vec::with_capacity(f.len()/2);
        let mut idx=0usize; while idx+1<f.len(){ l.push(f[idx]); r.push(f[idx+1]); idx+=2; }
        Ok((l, Some(r)))
    } else { Ok((f,None)) }
}

fn select_chunk_centers(duration_s:f64, chunks:usize)->Vec<f64>{
    if chunks==0 { return vec![]; }
    let step=duration_s/((chunks as f64)+1.0);
    (1..=chunks).map(|k| k as f64 * step).collect()
}

fn slice_window(sig:&[f32], center_s:f64, chunk_dur_s:f64, sr:u32)->Vec<f32>{
    let n=sig.len();
    let center=(center_s*sr as f64) as isize;
    let half=((chunk_dur_s*sr as f64)/2.0) as isize;
    let start=(center-half).max(0) as usize;
    let end=(center+half).min(n as isize) as usize;
    sig[start..end].to_vec()
}

fn next_pow2(mut v:usize)->usize{ if v==0{return 1;} v-=1; v|=v>>1; v|=v>>2; v|=v>>4; v|=v>>8; v|=v>>16; if std::mem::size_of::<usize>()==8 { v|=v>>32; } v+1 }

fn xcorr_fft_with_ref(ref_win:&[f32], other_win:&[f32], pre_ref_fft:Option<&[Complex<f32>]>, planner:&mut FftPlanner<f32>)->(isize,f32,Vec<Complex<f32>>){
    let conv_len = ref_win.len() + other_win.len() - 1;
    let fft_len = next_pow2(conv_len);
    let fft = planner.plan_fft_forward(fft_len);
    let ifft = planner.plan_fft_inverse(fft_len);

    // Prepare spectra
    let mut a:Vec<Complex<f32>> = vec![Complex::zero(); fft_len];
    let mut b:Vec<Complex<f32>> = vec![Complex::zero(); fft_len];

    // If precomputed ref FFT provided and same length, copy; else compute
    let use_pre = pre_ref_fft.map(|p| p.len()==fft_len).unwrap_or(false);
    if use_pre {
        let pref = pre_ref_fft.unwrap();
        a.copy_from_slice(pref);
    } else {
        for (i,&v) in ref_win.iter().enumerate(){ a[i].re = v; }
        fft.process(&mut a);
    }

    for (i,&v) in other_win.iter().enumerate(){ b[i].re = v; }
    fft.process(&mut b);

    // Cross-corr: IFFT(conj(A) * B)
    for i in 0..fft_len { a[i] = a[i].conj() * b[i]; }
    ifft.process(&mut a);

    // Peak
    let mut best_idx=0usize; let mut best_val=f32::MIN;
    for i in 0..fft_len { let v=a[i].re; if v>best_val { best_val=v; best_idx=i; } }
    let lag_samples = best_idx as isize - (ref_win.len() as isize - 1);

    // Sub-sample parabolic interpolation
    let idx = best_idx as isize;
    let y1 = if idx>0 { a[(idx-1) as usize].re } else { a[idx as usize].re };
    let y2 = a[idx as usize].re;
    let y3 = if idx+1 < a.len() as isize { a[(idx+1) as usize].re } else { a[idx as usize].re };
    let denom = y1 - 2.0*y2 + y3;
    let frac = if denom.abs()>1e-6 { 0.5*(y1 - y3)/denom } else { 0.0 };
    let refined = lag_samples as f32 + frac;

    (refined as isize, best_val, if use_pre { Vec::new() } else { a })
}

fn median_i128(v:&[i128])->i128{
    if v.is_empty(){ return 0; }
    let mut x = v.to_vec(); x.sort(); x[x.len()/2]
}

pub fn analyze_audio_xcorr_detailed(
    ref_audio:&str, other_audio:&str, duration_s:f64, params:&XCorrParams
)->Result<(XCorrResult, Vec<ChunkResult>), VsgError>{
    let sr = params.sample_rate;
    let (ref_l, ref_r_opt) = ffmpeg_decode_pcm(ref_audio, sr, params.stereo_mode, params.band)?;
    let (oth_l, oth_r_opt) = ffmpeg_decode_pcm(other_audio, sr, params.stereo_mode, params.band)?;

    match params.stereo_mode {
        StereoMode::Best => {
            let centers = select_chunk_centers(duration_s, params.chunks);
            let mut planner = FftPlanner::<f32>::new();
            let mut lags_ns:Vec<i128> = Vec::new();
            let mut chunks_out:Vec<ChunkResult> = Vec::new();
            let ns_per_sample = 1_000_000_000.0 / (sr as f32);

            for c in centers {
                let rw_l = slice_window(&ref_l, c, params.chunk_dur_s, sr);
                let ow_l = slice_window(&oth_l, c, params.chunk_dur_s, sr);
                let (lag_l, peak_l, _ref_fft_l) = xcorr_fft_with_ref(&rw_l, &ow_l, None, &mut planner);

                let (lag_samp, peak) = if let (Some(rr), Some(or)) = (ref_r_opt.as_ref(), oth_r_opt.as_ref()) {
                    let rw_r = slice_window(rr, c, params.chunk_dur_s, sr);
                    let ow_r = slice_window(or, c, params.chunk_dur_s, sr);
                    let (lag_r, peak_r, _ref_fft_r) = xcorr_fft_with_ref(&rw_r, &ow_r, None, &mut planner);
                    if peak_r > peak_l { (lag_r, peak_r) } else { (lag_l, peak_l) }
                } else { (lag_l, peak_l) };

                let lag_ns = (lag_samp as f32 * ns_per_sample) as i128;
                lags_ns.push(lag_ns);
                chunks_out.push(ChunkResult{ center_s:c, window_samples: rw_l.len(), lag_ns, lag_ms:(lag_ns as f64)/1.0e6, peak });
            }

            let median = median_i128(&lags_ns);
            let delay_ms = ((median as f64)/1.0e6).round() as i64;
            let peak_score = chunks_out.iter().map(|ch| ch.peak).fold(0.0, f32::max);
            Ok((XCorrResult{delay_ns:median, delay_ms, peak_score}, chunks_out))
        }
        StereoMode::Mid => {
            let rr = ref_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let or = oth_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let ref_mid: Vec<f32> = ref_l.iter().zip(rr.iter()).map(|(l, r)| 0.5 * (*l + *r)).collect();
            let oth_mid: Vec<f32> = oth_l.iter().zip(or.iter()).map(|(l, r)| 0.5 * (*l + *r)).collect();
            analyze_audio_xcorr_detailed_raw(&ref_mid, &oth_mid, duration_s, params)
        }
        StereoMode::Mono | StereoMode::Left => {
            analyze_audio_xcorr_detailed_raw(&ref_l, &oth_l, duration_s, params)
        }
        StereoMode::Right => {
            let rr = ref_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            let or = oth_r_opt.as_ref().ok_or_else(|| VsgError::Process("no right channel".into()))?;
            analyze_audio_xcorr_detailed_raw(rr, or, duration_s, params)
        }
    }
}

fn analyze_audio_xcorr_detailed_raw(
    ref_sig:&[f32], oth_sig:&[f32], duration_s:f64, params:&XCorrParams
)->Result<(XCorrResult, Vec<ChunkResult>), VsgError>{
    let sr = params.sample_rate;
    let centers = select_chunk_centers(duration_s, params.chunks);
    let mut planner = FftPlanner::<f32>::new();
    let mut lags_ns:Vec<i128>=Vec::new();
    let mut chunks_out:Vec<ChunkResult>=Vec::new();
    let ns_per_sample = 1_000_000_000.0 / (sr as f32);

    for c in centers {
        let rw = slice_window(ref_sig, c, params.chunk_dur_s, sr);
        let ow = slice_window(oth_sig, c, params.chunk_dur_s, sr);
        let (lag_samp, peak, _tmp) = xcorr_fft_with_ref(&rw, &ow, None, &mut planner);
        let lag_ns = (lag_samp as f32 * ns_per_sample) as i128;
        lags_ns.push(lag_ns);
        chunks_out.push(ChunkResult{ center_s:c, window_samples: rw.len(), lag_ns, lag_ms:(lag_ns as f64)/1.0e6, peak });
    }

    let median = median_i128(&lags_ns);
    let delay_ms = ((median as f64)/1.0e6).round() as i64;
    let peak_score = chunks_out.iter().map(|ch| ch.peak).fold(0.0, f32::max);
    Ok((XCorrResult{delay_ns:median, delay_ms, peak_score}, chunks_out))
}

// Back-compat simple API
pub fn analyze_audio_xcorr(ref_audio:&str, other_audio:&str, duration_s:f64, params:&XCorrParams)->Result<XCorrResult,VsgError>{
    let (r, _c) = analyze_audio_xcorr_detailed(ref_audio, other_audio, duration_s, params)?;
    Ok(r)
}
