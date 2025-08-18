
mod analyze_audio;
mod extract;
mod plan_merge;
mod make_opts;
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
    AnalyzeAudio(analyze_audio::AnalyzeArgs),
    Extract(extract::ExtractArgs),
    PlanMerge(plan_merge::PlanArgs),
    MakeOpts(make_opts::MakeOptsArgs),
    Mux(mux::MuxArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Commands::AnalyzeAudio(a) => analyze_audio::run(a)?,
        Commands::Extract(a) => extract::run(a)?,
        Commands::PlanMerge(a) => plan_merge::run(a)?,
        Commands::MakeOpts(a) => make_opts::run(a)?,
        Commands::Mux(a) => mux::run(a)?,
    }
    Ok(())
}
