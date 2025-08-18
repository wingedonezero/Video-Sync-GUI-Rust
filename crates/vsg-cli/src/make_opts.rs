
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct MakeOptsArgs {
    #[arg(long)] pub plan: PathBuf,
    #[arg(long)] pub out_opts: PathBuf,
}

pub fn run(_cmd: MakeOptsArgs) -> Result<()> {
    anyhow::bail!("make-opts: not implemented yet (PR4 target)")
}
