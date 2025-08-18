// crates/vsg-cli/src/main.rs
use clap::{Parser, Subcommand, Args};
use anyhow::Result;
use std::path::PathBuf;
use regex::Regex;

mod mux;
mod cmd_analyze;

#[derive(Parser, Debug)]
#[command(name="vsg-cli", version, about="Video Sync GUI - CLI tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Analyze audio offset between reference and target via cross-correlation
    Analyze(AnalyzeArgs),
    /// Extract streams and build mkvmerge option file, then mux
    Mux(MuxArgs),
}

#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    /// Reference media (baseline)
    #[arg(long)]
    reference: PathBuf,
    /// Target media to align to reference
    #[arg(long)]
    target: PathBuf,
    /// Number of passes
    #[arg(long, default_value_t=10)]
    passes: usize,
    /// Chunk length in ms
    #[arg(long, default_value_t=15000)]
    chunk_ms: i64,
    /// Hop size in ms
    #[arg(long, default_value_t=5000)]
    hop_ms: i64,
    /// Max search window in ms (±)
    #[arg(long, default_value_t=3000)]
    max_shift_ms: i64,
    /// Minimum match score [0..1]
    #[arg(long, default_value_t=0.18)]
    min_match: f32,
    /// ffmpeg path
    #[arg(long, default_value="ffmpeg")]
    ffmpeg: String,
    /// Save debug JSON
    #[arg(long)]
    save_debug: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct MuxArgs {
    #[arg(long)]
    reference: PathBuf,
    #[arg(long)]
    secondary: Option<PathBuf>,
    #[arg(long)]
    tertiary: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    /// Raw ms delay for secondary (negative allowed)
    #[arg(long)]
    sec_delay: Option<i64>,
    /// Raw ms delay for tertiary (negative allowed)
    #[arg(long)]
    ter_delay: Option<i64>,
    /// Path to mkvmerge
    #[arg(long, default_value="/usr/bin/mkvmerge")]
    mkvmerge: PathBuf,
    /// Path to mkvextract
    #[arg(long, default_value="/usr/bin/mkvextract")]
    mkvextract: PathBuf,
    /// Write the generated @opts.json to this path too
    #[arg(long)]
    out_opts: Option<PathBuf>,
    /// Preferred language (for picking ENG audio from secondary, etc.)
    #[arg(long, default_value="eng")]
    prefer_lang: String,
    /// Regex to identify "Signs/Songs" subtitles
    #[arg(long, default_value="(?i)sign|song")]
    signs_pattern: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Analyze(a) => {
            let args = cmd_analyze::AnalyzeArgs {
                reference: a.reference,
                target: a.target,
                passes: a.passes,
                chunk_ms: a.chunk_ms,
                hop_ms: a.hop_ms,
                max_shift_ms: a.max_shift_ms,
                min_match: a.min_match,
                ffmpeg: a.ffmpeg,
                save_debug: a.save_debug,
            };
            cmd_analyze::run(&args)?;
        }
        Commands::Mux(m) => {
            let re = Regex::new(&m.signs_pattern).map_err(|e| anyhow::anyhow!(e))?;
            let cfg = mux::MuxConfig {
                reference: &m.reference,
                secondary: m.secondary.as_deref(),
                tertiary: m.tertiary.as_deref(),
                output: &m.output,
                mkvmerge: &m.mkvmerge,
                mkvextract: &m.mkvextract,
                prefer_lang: &m.prefer_lang,
                signs_regex: &re,
                out_opts: m.out_opts.as_deref(),
                sec_delay_ms: m.sec_delay,
                ter_delay_ms: m.ter_delay,
            };
            mux::mux(&cfg)?;
        }
    }
    Ok(())
}
