
use clap::Args;
use anyhow::Result;
use std::path::PathBuf;
use regex::Regex;
use vsg_core::types::{Sources, ToolPaths, TempLayout};
use vsg_core::extract::extract_plan_and_execute;

#[derive(Args, Debug)]
pub struct ExtractArgs {
    #[arg(long)] pub reference: PathBuf,
    #[arg(long)] pub secondary: Option<PathBuf>,
    #[arg(long)] pub tertiary: Option<PathBuf>,
    #[arg(long, default_value = "./temp")] pub temp_root: PathBuf,
    #[arg(long, default_value = "./output")] pub out_dir: PathBuf,
    #[arg(long, default_value = "mkvmerge")] pub mkvmerge: PathBuf,
    #[arg(long, default_value = "mkvextract")] pub mkvextract: PathBuf,
    #[arg(long, default_value = "ffmpeg")] pub ffmpeg: PathBuf,
    #[arg(long, default_value = "eng")] pub prefer_lang: String,
    #[arg(long, default_value = "(?i)sign|song")] pub signs_pattern: String,
}

pub fn run(cmd: ExtractArgs) -> Result<()> {
    let src = Sources { reference: cmd.reference, secondary: cmd.secondary, tertiary: cmd.tertiary };
    let tools = ToolPaths { mkvmerge: cmd.mkvmerge, mkvextract: cmd.mkvextract, ffmpeg: cmd.ffmpeg };
    let temp = TempLayout { root: cmd.temp_root, out_dir: cmd.out_dir };
    let _plan = extract_plan_and_execute(&src, &tools, &temp, &cmd.prefer_lang, &cmd.signs_pattern)?;
    println!("extract: ok (see manifest under temp root)");
    Ok(())
}
