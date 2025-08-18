use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};
use vsg_core::tracks::probe_streams;
use serde_json;

#[derive(Parser)]
#[command(name="vsg-cli", version, about="Video Sync CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Analyze {
        #[arg(long)] reference: String,
        #[arg(long)] secondary: Option<String>,
        #[arg(long)] tertiary: Option<String>,
    },
    Merge {
        #[arg(long)] opts_json: String,
        #[arg(long)] output: String,
    },
    /// Run mkvmerge -J and print parsed tracks as JSON
    Probe {
        /// Input media file (MKV/MKA/MP4 supported by mkvmerge probe)
        #[arg(long)]
        file: String,
        /// Optional path to mkvmerge (if not on PATH)
        #[arg(long, default_value="")]
        mkvmerge: String,
    },
}

fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let cli = Cli::parse();
    match cli.command {
        Command::Analyze { reference, secondary, tertiary } => {
            println!("Analyze stub: ref={reference}, sec={:?}, ter={:?}", secondary, tertiary);
        }
        Command::Merge { opts_json, output } => {
            println!("Merge stub: @{}, out={}", opts_json, output);
        }
        Command::Probe { file, mkvmerge } => {
            let pr = probe_streams(&mkvmerge, &file)?;
            println!("{}", serde_json::to_string_pretty(&pr)?);
        }
    }
    Ok(())
}
