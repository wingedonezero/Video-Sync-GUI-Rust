use std::process::{Command, Stdio};
use std::io::Read;
use crate::error::VsgError;
use rustfft::{FftPlanner, num_complex::Complex, num_traits::Zero};

pub struct XCorrParams { pub chunks:usize, pub chunk_dur_s:f64, pub sample_rate:u32, pub min_match:f64 }
pub struct XCorrResult { pub delay_ns:i128, pub delay_ms:i64 }

fn ffmpeg_decode_to_pcm_s16le_mono(path:&str, sample_rate:u32)->Result<Vec<i16>,VsgError>{
  // decode to MONO at requested sample rate to reduce compute & memory
  let mut cmd=Command::new("ffmpeg");
  cmd.arg("-i").arg(path).arg("-f").arg("s16le").arg("-ac").arg("1").arg("-ar").arg(format!("{}",sample_rate)).arg("-");
  let mut child=cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| VsgError::Process(format!("spawn ffmpeg: {}",e)))?;
  let mut buf:Vec<u8>=Vec::new();
  child.stdout.as_mut().unwrap().read_to_end(&mut buf).map_err(|e| VsgError::Process(format!("ffmpeg read: {}",e)))?;
  let status=child.wait().map_err(|e| VsgError::Process(format!("ffmpeg wait: {}",e)))?;
  if !status.success(){ return Err(VsgError::Process(format!("ffmpeg decode failed: {}",status))); }
  let mut pcm:Vec<i16>=Vec::with_capacity(buf.len()/2);
  let mut i=0usize; while i+1<buf.len(){ pcm.push(i16::from_le_bytes([buf[i],buf[i+1]])); i+=2; }
  Ok(pcm)
}

fn select_chunk_centers(duration_s:f64, chunks:usize)->Vec<f64>{
  if chunks==0{return vec![];}
  let step=duration_s/((chunks as f64)+1.0);
  (1..=chunks).map(|k| k as f64 * step).collect()
}

fn slice_window_i16(pcm:&[i16], center_s:f64, chunk_dur_s:f64, sr:u32)->Vec<f32>{
  let n=pcm.len();
  let center=(center_s*sr as f64) as isize;
  let half=((chunk_dur_s*sr as f64)/2.0) as isize;
  let start=(center-half).max(0) as usize;
  let end=(center+half).min(n as isize) as usize;
  pcm[start..end].iter().map(|&x| x as f32).collect()
}

fn next_pow2(mut v:usize)->usize{ if v==0 {return 1;} v-=1; v|=v>>1; v|=v>>2; v|=v>>4; v|=v>>8; v|=v>>16; if std::mem::size_of::<usize>()==8 { v|=v>>32; } v+1 }

fn xcorr_fft_lag_ns(ref_win:&[f32], other_win:&[f32], sr:u32)->i128{
  // zero-pad to next power of two >= len(ref)+len(other)-1
  let n = ref_win.len().min(other_win.len());
  if n < 3 { return 0; }
  let conv_len = ref_win.len() + other_win.len() - 1;
  let fft_len = next_pow2(conv_len);

  let mut planner = FftPlanner::<f32>::new();
  let fft = planner.plan_fft_forward(fft_len);
  let ifft = planner.plan_fft_inverse(fft_len);

  let mut a: Vec<Complex<f32>> = vec![Complex::zero(); fft_len];
  let mut b: Vec<Complex<f32>> = vec![Complex::zero(); fft_len];

  for (i,&v) in ref_win.iter().enumerate() { a[i].re = v; }
  for (i,&v) in other_win.iter().enumerate() { b[i].re = v; }

  fft.process(&mut a);
  fft.process(&mut b);
  // cross-corr via FFT: IFFT( conj(FFT(a)) * FFT(b) )
  for i in 0..fft_len { a[i] = a[i].conj() * b[i]; }
  ifft.process(&mut a);

  // find peak index
  let mut best_idx = 0usize;
  let mut best_val = f32::MIN;
  for i in 0..fft_len {
    let v = a[i].re;
    if v > best_val { best_val = v; best_idx = i; }
  }

  // convert index to signed lag: correlation sequence is length conv_len,
  // where index 0 corresponds to - (ref_len-1). We map to lag in samples.
  let ref_len = ref_win.len();
  let lag_samples_i = best_idx as isize - (ref_len as isize - 1);

  // quadratic interpolation around the peak for sub-sample refinement
  let idx = best_idx as isize;
  let y1 = if idx>0 { a[(idx-1) as usize].re } else { a[idx as usize].re };
  let y2 = a[idx as usize].re;
  let y3 = if idx+1 < a.len() as isize { a[(idx+1) as usize].re } else { a[idx as usize].re };
  let denom = y1 - 2.0*y2 + y3;
  let frac = if denom.abs() > 1e-6 { 0.5*(y1 - y3)/denom } else { 0.0 };
  let lag_samples = lag_samples_i as f32 + frac;

  let ns_per_sample=1_000_000_000.0 / (sr as f32);
  (lag_samples as f64 * ns_per_sample as f64) as i128
}

pub fn analyze_audio_xcorr(ref_audio:&str, other_audio:&str, duration_s:f64, params:&XCorrParams)->Result<XCorrResult,VsgError>{
  let sr=params.sample_rate;
  let ref_pcm=ffmpeg_decode_to_pcm_s16le_mono(ref_audio,sr)?;
  let oth_pcm=ffmpeg_decode_to_pcm_s16le_mono(other_audio,sr)?;
  let centers=select_chunk_centers(duration_s, params.chunks);
  let mut lags:Vec<i128>=Vec::new();
  for c in centers{
    let rw=slice_window_i16(&ref_pcm,c,params.chunk_dur_s,sr);
    let ow=slice_window_i16(&oth_pcm,c,params.chunk_dur_s,sr);
    lags.push(xcorr_fft_lag_ns(&rw,&ow,sr));
  }
  lags.sort(); let median_ns= if lags.is_empty(){0}else{ lags[lags.len()/2] };
  let ms=(median_ns as f64)/1.0e6;
  let ms_rounded= if (ms.abs() - ms.abs().floor()).abs() < 1e-9 { if ms>=0.0 { ms.ceil() } else { ms.floor() } } else { ms.round() };
  Ok(XCorrResult{ delay_ns: median_ns, delay_ms: ms_rounded as i64 })
}
