use anyhow::Result;
use clap::Args;
use regex::Regex;
use std::path::PathBuf;

/// CLI arguments for muxing.
#[derive(Debug, Args)]
pub struct MuxArgs {
    /// Reference video file
    #[arg(long)]
    pub reference: PathBuf,
    /// Secondary video file (optional)
    #[arg(long)]
    pub secondary: Option<PathBuf>,
    /// Tertiary video file (optional)
    #[arg(long)]
    pub tertiary: Option<PathBuf>,
    /// Output MKV
    #[arg(long)]
    pub output: PathBuf,
    /// Path to mkvmerge
    #[arg(long)]
    pub mkvmerge: PathBuf,
    /// Path to mkvextract (optional)
    #[arg(long)]
    pub mkvextract: Option<PathBuf>,
    /// Language preference
    #[arg(long, default_value = "eng")]
    pub prefer_lang: String,
    /// Regex for signs/songs subtitles
    #[arg(long, default_value = "(?i)sign|song")]
    pub signs_pattern: String,
    /// JSON out-opts file
    #[arg(long)]
    pub out_opts: Option<PathBuf>,
    /// Optional delay override for secondary
    #[arg(long)]
    pub sec_delay_ms: Option<i64>,
    /// Optional delay override for tertiary
    #[arg(long)]
    pub ter_delay_ms: Option<i64>,
}

/// Entrypoint for `vsg-cli mux`.
pub fn run(args: MuxArgs) -> Result<()> {
    let re = Regex::new(&args.signs_pattern)?;
    tracing::info!("Muxing with regex {:?}", re);

    // Call your existing mux logic here.
    // If you had a `pub fn mux(cfg: &MuxConfig) -> Result<()>`, adapt args into cfg.

    Ok(())
}
