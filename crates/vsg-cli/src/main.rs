
mod mux;

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
    /// Build opts.json & run mkvmerge
    Mux(mux::MuxArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Commands::Mux(a) => mux::run(a)?,
    }
    Ok(())
}
