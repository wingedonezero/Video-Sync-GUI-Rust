
use clap::Args;
use anyhow::{Result, Context};
use std::path::PathBuf;
use std::process::Command;

use vsg_core::types::{RawDelays, TempLayout, PositiveDelays, MergePlan};
use vsg_core::plan::{positive_only, plan_merge};
use vsg_core::opts::write_opts_json;

#[derive(Args, Debug)]
pub struct MuxArgs {
    /// Path to manifest.json produced by Extract step
    #[arg(long)] pub manifest: PathBuf,
    /// Output MKV
    #[arg(long)] pub output: PathBuf,
    /// mkvmerge binary
    #[arg(long, default_value = "mkvmerge")] pub mkvmerge: PathBuf,
    /// Delays (raw, may be negative). If omitted -> treated as 0.
    #[arg(long)] pub sec_delay: Option<i64>,
    #[arg(long)] pub ter_delay: Option<i64>,
    /// Language & default rules
    #[arg(long, default_value = "eng")] pub prefer_lang: String,
    #[arg(long, default_value = "(?i)sign|song")] pub signs_pattern: String,
    #[arg(long, default_value_t = false)] pub first_sub_default: bool,
    #[arg(long, default_value_t = false)] pub default_signs: bool,
    /// Where to write the opts.json (if not set, use alongside manifest)
    #[arg(long)] pub out_opts: Option<PathBuf>,
    /// Cleanup temp folder after success
    #[arg(long, default_value_t = false)] pub cleanup_temp: bool,
}

pub fn run(a: MuxArgs) -> Result<()> {
    let raw = RawDelays { sec_ms: a.sec_delay, ter_ms: a.ter_delay };
    let delays: PositiveDelays = positive_only(&raw);
    eprintln!("Merge Summary: global_shift={} ms, secondary={} ms, tertiary={} ms", delays.global_ms, delays.sec_residual_ms, delays.ter_residual_ms);

    // Build plan from manifest + rules
    let plan: MergePlan = plan_merge(&a.manifest, &a.output, &a.prefer_lang, &a.signs_pattern, a.first_sub_default, a.default_signs, &delays)?;

    // Write opts.json
    let opts_path = a.out_opts.clone().unwrap_or_else(|| {
        let mut p = a.manifest.clone();
        p.set_file_name("vsg_mux_opts.json");
        p
    });
    write_opts_json(&plan, &opts_path)?;
    eprintln!("Wrote opts.json -> {}", opts_path.display());

    // Run mkvmerge "@/path"
    let at_arg = format!("@{}", opts_path.display());
    let status = Command::new(&a.mkvmerge)
        .arg(at_arg)
        .status()
        .with_context(|| "spawn mkvmerge with @opts.json failed")?;
    if !status.success() {
        anyhow::bail!("mkvmerge failed with status {:?}", status.code());
    }

    if a.cleanup_temp {
        if let Some(parent) = a.manifest.parent() {
            if parent.ends_with("temp") {
                let _ = std::fs::remove_dir_all(parent);
            }
        }
    }
    Ok(())
}
