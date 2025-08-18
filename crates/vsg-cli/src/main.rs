mod analyze;
mod mux;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "vsg-cli")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Analyze(analyze::AnalyzeArgs),
    Mux(mux::MuxArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze(args) => analyze::run(args)?,
        Commands::Mux(args) => mux::run(args)?,
    }

    Ok(())
}
