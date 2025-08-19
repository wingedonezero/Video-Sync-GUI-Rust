use std::process::{Command, Stdio}; use std::io::Read; use crate::error::VsgError;

pub struct XCorrParams { pub chunks:usize, pub chunk_dur_s:f64, pub sample_rate:u32, pub min_match:f64 }

pub struct XCorrResult { pub delay_ns:i128, pub delay_ms:i64 }

fn ffmpeg_decode_to_pcm_s16le(path:&str, sample_rate:u32)->Result<Vec<i16>,VsgError>{
  let mut cmd=Command::new("ffmpeg"); cmd.arg("-i").arg(path).arg("-f").arg("s16le").arg("-ac").arg("2").arg("-ar").arg(format!("{}",sample_rate)).arg("-");
  let mut child=cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| VsgError::Process(format!("spawn ffmpeg: {}",e)))?;
  let mut buf:Vec<u8>=Vec::new();
  child.stdout.as_mut().unwrap().read_to_end(&mut buf).map_err(|e| VsgError::Process(format!("ffmpeg read: {}",e)))?;
  let status=child.wait().map_err(|e| VsgError::Process(format!("ffmpeg wait: {}",e)))?;
  if !status.success(){ return Err(VsgError::Process(format!("ffmpeg decode failed: {}",status))); }
  let mut pcm:Vec<i16>=Vec::with_capacity(buf.len()/2);
  let mut i=0usize; while i+1<buf.len(){ pcm.push(i16::from_le_bytes([buf[i],buf[i+1]])); i+=2; }
  Ok(pcm)
}

fn select_chunk_centers(duration_s:f64, chunks:usize)->Vec<f64>{ if chunks==0{return vec![];} let step=duration_s/((chunks as f64)+1.0); (1..=chunks).map(|k| k as f64 * step).collect() }

fn slice_window(pcm:&[i16], center_s:f64, chunk_dur_s:f64, sr:u32)->Vec<i16>{
  let n=pcm.len(); let center=(center_s*sr as f64) as isize; let half=((chunk_dur_s*sr as f64)/2.0) as isize;
  let start=(center-half).max(0) as usize; let end=(center+half).min(n as isize) as usize; pcm[start..end].to_vec()
}

fn corr_at(ref_win:&[i16], other_win:&[i16], lag:isize)->i64{
  let n=ref_win.len().min(other_win.len());
  let mut sum:i64=0;
  for i in 0..n{ let j=i as isize + lag; if j<0 || j>=n as isize { continue; } sum += (ref_win[i] as i32 as i64) * (other_win[j as usize] as i32 as i64); }
  sum
}

fn cross_corr_lag_ns(ref_win:&[i16], other_win:&[i16], sr:u32)->i128{
  let n=ref_win.len().min(other_win.len()); if n<3 { return 0; }
  let max_lag=(n as isize)/4;
  let mut best_sum:i64=i64::MIN; let mut best_lag:isize=0;
  for lag in -max_lag..=max_lag{ let s=corr_at(ref_win,other_win,lag); if s>best_sum{ best_sum=s; best_lag=lag; } }
  let y1=corr_at(ref_win,other_win,best_lag-1) as f64;
  let y2=best_sum as f64;
  let y3=corr_at(ref_win,other_win,best_lag+1) as f64;
  let denom=(y1 - 2.0*y2 + y3); let frac= if denom.abs()>1e-9 { 0.5*(y1 - y3)/denom } else { 0.0 };
  let lag_samples=(best_lag as f64 + frac);
  let ns_per_sample=1_000_000_000.0 / (sr as f64);
  (lag_samples * ns_per_sample) as i128
}

pub fn analyze_audio_xcorr(ref_audio:&str, other_audio:&str, duration_s:f64, params:&XCorrParams)->Result<XCorrResult,VsgError>{
  let sr=params.sample_rate;
  let ref_pcm=ffmpeg_decode_to_pcm_s16le(ref_audio,sr)?;
  let oth_pcm=ffmpeg_decode_to_pcm_s16le(other_audio,sr)?;
  let centers=select_chunk_centers(duration_s, params.chunks);
  let mut lags:Vec<i128>=Vec::new();
  for c in centers{
    let rw=slice_window(&ref_pcm,c,params.chunk_dur_s,sr);
    let ow=slice_window(&oth_pcm,c,params.chunk_dur_s,sr);
    lags.push(cross_corr_lag_ns(&rw,&ow,sr));
  }
  lags.sort(); let median_ns= if lags.is_empty(){0}else{ lags[lags.len()/2] };
  let ms=(median_ns as f64)/1.0e6;
  let ms_rounded= if (ms.abs() - ms.abs().floor()).abs() < 1e-9 { if ms>=0.0 { ms.ceil() } else { ms.floor() } } else { ms.round() };
  Ok(XCorrResult{ delay_ns: median_ns, delay_ms: ms_rounded as i64 })
}
