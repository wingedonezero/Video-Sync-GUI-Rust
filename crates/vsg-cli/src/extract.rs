
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ExtractArgs {
    #[arg(long)] pub reference: PathBuf,
    #[arg(long)] pub secondary: Option<PathBuf>,
    #[arg(long)] pub tertiary: Option<PathBuf>,
    #[arg(long, default_value = "./temp")] pub temp_root: PathBuf,
    #[arg(long, default_value = "./output")] pub out_dir: PathBuf,
    #[arg(long, default_value = "mkvmerge")] pub mkvmerge: PathBuf,
    #[arg(long, default_value = "mkvextract")] pub mkvextract: PathBuf,
}

pub fn run(_cmd: ExtractArgs) -> Result<()> {
    anyhow::bail!("extract: not implemented yet (PR1 target)")
}
