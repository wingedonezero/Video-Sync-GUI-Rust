
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct PlanArgs {
    #[arg(long)] pub from_extract: PathBuf,
    #[arg(long, default_value = "eng")] pub prefer_lang: String,
    #[arg(long, default_value = "(?i)sign|song")] pub signs_pattern: String,
    #[arg(long, default_value_t = false)] pub first_sub_default: bool,
    #[arg(long, default_value_t = false)] pub default_signs: bool,
    #[arg(long)] pub sec_delay: Option<i64>,
    #[arg(long)] pub ter_delay: Option<i64>,
    #[arg(long)] pub print: bool,
}

pub fn run(_cmd: PlanArgs) -> Result<()> {
    anyhow::bail!("plan-merge: not implemented yet (PR3 target)")
}
