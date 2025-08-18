use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name="vsg-cli")]
#[command(about="Video Sync GUI CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    MakeOpts {
        #[arg(long)]
        reference: PathBuf,
        #[arg(long)]
        secondary: Option<PathBuf>,
        #[arg(long)]
        tertiary: Option<PathBuf>,
        #[arg(long, value_name="OUT_OPTS")]
        out_opts: PathBuf,
        #[arg(long, value_name="OUTPUT")]
        output: PathBuf,
        #[arg(long)]
        sec_delay: Option<i64>,
        #[arg(long)]
        ter_delay: Option<i64>,
        #[arg(long)]
        mkvmerge: Option<PathBuf>,
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::MakeOpts { reference, secondary, tertiary, out_opts, output, sec_delay, ter_delay, mkvmerge } => {
            println!("reference: {:?}", reference);
            println!("secondary: {:?}", secondary);
            println!("tertiary: {:?}", tertiary);
            println!("out_opts: {:?}", out_opts);
            println!("output: {:?}", output);
            println!("sec_delay: {:?}", sec_delay);
            println!("ter_delay: {:?}", ter_delay);
            println!("mkvmerge: {:?}", mkvmerge);
        }
    }
}
