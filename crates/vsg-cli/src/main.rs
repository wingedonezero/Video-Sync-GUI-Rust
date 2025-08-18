
mod analyze_audio;
mod extract;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name="vsg")]
#[command(version, about="Video Sync CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    AnalyzeAudio(analyze_audio::AnalyzeArgs),
    Extract(extract::ExtractArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Commands::AnalyzeAudio(a) => analyze_audio::run(a)?,
        Commands::Extract(a) => extract::run(a)?,
    }
    Ok(())
}
