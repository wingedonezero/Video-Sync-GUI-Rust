// crates/vsg-core/src/analysis/ffmpeg_decode.rs
use anyhow::{Result, anyhow};
use std::path::Path;
use std::process::{Command, Stdio};

/// Decode input media to mono f32le 48k via ffmpeg.
pub fn decode_to_f32_mono_48k(path: &Path, ffmpeg: &str) -> Result<Vec<f32>> {
    let out = Command::new(ffmpeg)
        .args([
            "-nostdin","-hide_banner","-v","error",
            "-i", path.to_str().ok_or_else(|| anyhow!("bad path"))?,
            "-vn","-ac","1","-ar","48000","-f","f32le","-",
        ])
        .stdout(Stdio::piped())
        .output()?;
    if !out.status.success() {
        return Err(anyhow!("ffmpeg failed decoding {:?}", path));
    }
    let bytes = out.stdout;
    if bytes.len() % 4 != 0 { return Err(anyhow!("unexpected f32le size")); }
    let mut samples = Vec::with_capacity(bytes.len()/4);
    for chunk in bytes.chunks_exact(4) {
        let v = f32::from_le_bytes([chunk[0],chunk[1],chunk[2],chunk[3]]);
        samples.push(v);
    }
    Ok(samples)
}
