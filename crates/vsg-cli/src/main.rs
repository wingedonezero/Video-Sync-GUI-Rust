mod analyze;
// Keep existing mux command if present in repo:
#[allow(unused)]
mod mux;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name="vsg-cli")]
#[command(version, about="Video Sync CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Analyze audio offset (reference-based chunks)
    Analyze(analyze::AnalyzeArgs),
    /// Existing mux command (delegated to your mux module)
    Mux(mux::MuxArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Commands::Analyze(args) => analyze::run(args)?,
        Commands::Mux(args)     => mux::run(args)?,
    }
    Ok(())
}
