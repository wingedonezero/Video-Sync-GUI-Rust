
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;
use vsg_core::analysis::{XcorrParams, analyze};

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    #[arg(long)] pub reference: PathBuf,
    #[arg(long)] pub target: PathBuf,
    #[arg(long, default_value_t = 15.0)] pub chunk_sec: f32,
    #[arg(long, default_value_t = 10)] pub chunks: usize,
    #[arg(long, default_value_t = 2000)] pub lag_ms: i64,
    #[arg(long, default_value_t = 20.0)] pub min_match: f32,
    #[arg(long)] pub save_debug: Option<PathBuf>,
    #[arg(long, default_value = "ffmpeg")] pub ffmpeg: String,
}

pub fn run(cmd: AnalyzeArgs) -> Result<()> {
    let mut p = XcorrParams::default();
    p.chunk_sec = cmd.chunk_sec;
    p.chunks = cmd.chunks;
    p.lag_ms = cmd.lag_ms;
    p.min_match_pct = cmd.min_match;
    p.ffmpeg_path = cmd.ffmpeg.clone();
    p.save_debug = cmd.save_debug.clone();

    let res = analyze(&cmd.reference, &cmd.target, &p)?;
    println!("raw_delay_ms={}", res.raw_delay_ms);
    println!("votes={}, avg_match={:.1}%", res.votes, res.avg_match_pct);
    if let Some(path) = cmd.save_debug {
        println!("debug_json={}", path.display());
    }
    Ok(())
}
