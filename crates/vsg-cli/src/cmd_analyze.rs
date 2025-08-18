// crates/vsg-cli/src/cmd_analyze.rs
use std::path::PathBuf;
use clap::Args;
use anyhow::Result;
use serde_json::to_writer_pretty;
use vsg_core::analysis::AnalyzeParams;

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    #[arg(long)]
    pub reference: PathBuf,
    #[arg(long)]
    pub target: PathBuf,
    #[arg(long, default_value_t=10)]
    pub passes: usize,
    #[arg(long, default_value_t=15000)]
    pub chunk_ms: i64,
    #[arg(long, default_value_t=5000)]
    pub hop_ms: i64,
    #[arg(long, default_value_t=3000)]
    pub max_shift_ms: i64,
    #[arg(long, default_value_t=0.18)]
    pub min_match: f32,
    #[arg(long, default_value="ffmpeg")]
    pub ffmpeg: String,
    #[arg(long)]
    pub save_debug: Option<PathBuf>,
}

pub fn run(args: &AnalyzeArgs) -> Result<()> {
    let params = AnalyzeParams{
        passes: args.passes,
        chunk_ms: args.chunk_ms,
        hop_ms: args.hop_ms,
        max_shift_ms: args.max_shift_ms,
        min_match: args.min_match,
        sample_rate: 48_000,
    };
    let res = vsg_core::analysis::xcorr::analyze_pair(
        &args.reference, &args.target, &args.ffmpeg, &params)?;

    println!("[analyze] method={} sr={}Hz chunk={}ms hop={}ms search=±{}ms",
        res.method, res.sample_rate_hz, res.chunk_ms, res.hop_ms, res.search_window_ms);
    for p in &res.passes {
        println!("pass {:>2}  {:>6}.{:03}–{:>6}.{:03}  inliers={}/{}  conf={:.2}  shift_ms={}",
            p.index,
            p.start_ms/1000, (p.start_ms%1000).abs(),
            p.end_ms/1000, (p.end_ms%1000).abs(),
            p.inliers, p.total_chunks, p.confidence, p.shift_ms as i64);
    }
    println!("FINAL: shift_ms={}  confidence={:.2}  passes_used={}/{}",
        res.result.global_shift_ms, res.result.confidence, res.result.passes_used, res.result.total_passes);

    if let Some(out) = &args.save_debug {
        let f = std::fs::File::create(out)?;
        to_writer_pretty(f, &res)?;
        println!("Wrote analysis JSON -> {}", out.display());
    }
    Ok(())
}
