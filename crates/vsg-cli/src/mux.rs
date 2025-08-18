
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct MuxArgs {
    #[arg(long)] pub opts: PathBuf,
    #[arg(long, default_value = "mkvmerge")] pub mkvmerge: PathBuf,
    #[arg(long)] pub output: PathBuf,
    #[arg(long, default_value_t = true)] pub clean_temp_on_success: bool,
    #[arg(long, default_value_t = false)] pub keep_temp: bool,
}

pub fn run(_cmd: MuxArgs) -> Result<()> {
    anyhow::bail!("mux: not implemented yet (PR5 target)")
}
