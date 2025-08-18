use anyhow::Result;

pub struct CorrResult { pub delay_ms: i32, pub match_pct: f32 }

pub fn run_videodiff(_videodiff: &str, _ref: &str, _sec: &str) -> Result<(i32, f32)> {
    // TODO: spawn videodiff and parse final-line values.
    Ok((0, 100.0))
}

pub fn run_audio_correlation_workflow(_ffmpeg: &str, _ref: &str, _sec: &str) -> Result<Vec<CorrResult>> {
    // TODO: extract chunks via ffmpeg and compute cross-correlation (phase 2).
    Ok(vec![])
}
