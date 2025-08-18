use clap::Args;
use anyhow::Result;
use std::path::PathBuf;
use vsg_core::analysis::{AnalyzeParams, analyze_audio_offsets};

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Reference MKV (timeline owner)
    #[arg(long)]
    pub reference: PathBuf,
    /// Target MKV to align to the reference
    #[arg(long)]
    pub target: PathBuf,
    /// Chunk length in seconds
    #[arg(long, default_value_t = 15.0)]
    pub chunk_sec: f32,
    /// Number of chunks
    #[arg(long, default_value_t = 10)]
    pub chunks: usize,
    /// +/- lag search window in ms
    #[arg(long, default_value_t = 2000)]
    pub lag_ms: i64,
    /// Minimum acceptable average match % for the winner
    #[arg(long, default_value_t = 20.0)]
    pub min_match: f32,
    /// Save detailed JSON
    #[arg(long)]
    pub save_debug: Option<PathBuf>,
    /// Path to ffmpeg (for decode)
    #[arg(long, default_value = "ffmpeg")]
    pub ffmpeg: String,
}

pub fn run(cmd: AnalyzeArgs) -> Result<()> {
    let mut p = AnalyzeParams::default();
    p.chunk_sec = cmd.chunk_sec;
    p.chunks = cmd.chunks;
    p.lag_ms = cmd.lag_ms;
    p.min_match_pct = cmd.min_match;
    p.ffmpeg_path = cmd.ffmpeg;
    p.save_debug = cmd.save_debug.clone();

    let res = analyze_audio_offsets(&cmd.reference, &cmd.target, &p)?;
    println!("raw_delay_ms={}", res.raw_delay_ms);
    println!("votes={}, avg_match={:.1}%", res.chosen_votes, res.chosen_avg_match);
    if let Some(path) = cmd.save_debug {
        println!("debug_json={}", path.display());
    }
    Ok(())
}
