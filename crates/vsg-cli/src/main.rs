
use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};
use vsg_core::tracks::{probe_streams, ProbeResult};
use vsg_core::plan::{build_plan, adjusted_delays, summarize_plan};
use vsg_core::mkvmerge::{build_tokens_with_policy, write_opts_json};

#[derive(Parser)]
#[command(name="vsg-cli", version, about="Video Sync CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Probe {
        #[arg(long)] file: String,
        #[arg(long, default_value="")] mkvmerge: String,
    },
    MakeOpts {
        #[arg(long)] reference: String,
        #[arg(long)] secondary: Option<String>,
        #[arg(long)] tertiary: Option<String>,
        #[arg(long)] out_opts: String,
        #[arg(long)] output: String,
        #[arg(long, default_value_t = 0)] sec_delay: i32,
        #[arg(long, default_value_t = 0)] ter_delay: i32,
        #[arg(long, default_value="")] mkvmerge: String,
        #[arg(long, default_value="eng")] prefer_lang: String,
        #[arg(long, default_value="(?i)sign|song")] signs_pattern: String,
    },
    Mux {
        #[arg(long)] reference: String,
        #[arg(long)] secondary: Option<String>,
        #[arg(long)] tertiary: Option<String>,
        #[arg(long)] output: String,
        #[arg(long, default_value_t = 0)] sec_delay: i32,
        #[arg(long, default_value_t = 0)] ter_delay: i32,
        #[arg(long, default_value="")] mkvmerge: String,
        #[arg(long, default_value="eng")] prefer_lang: String,
        #[arg(long, default_value="(?i)sign|song")] signs_pattern: String,
        #[arg(long, default_value="/tmp/vsg_mux_opts.json")] out_opts: String,
    },
}

fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { file, mkvmerge } => {
            let pr = probe_streams(&mkvmerge, &file)?;
            println!("{}", serde_json::to_string_pretty(&pr)?);
        }
        Command::MakeOpts { reference, secondary, tertiary, out_opts, output, sec_delay, ter_delay, mkvmerge, prefer_lang, signs_pattern } => {
            let ref_probe: ProbeResult = probe_streams(&mkvmerge, &reference)?;
            let sec_probe = if let Some(ref s) = secondary { Some(probe_streams(&mkvmerge, s)?) } else { None };
            let ter_probe = if let Some(ref t) = tertiary { Some(probe_streams(&mkvmerge, t)?) } else { None };

            let p = build_plan(Some(sec_delay), Some(ter_delay));
            let (ref_ms, sec_ms, ter_ms) = adjusted_delays(&p);
            println!("{}", summarize_plan(&p));

            let tokens = build_tokens_with_policy(
                &output,
                &reference,
                &ref_probe,
                secondary.as_deref(),
                sec_probe.as_ref(),
                tertiary.as_deref(),
                ter_probe.as_ref(),
                ref_ms,
                Some(sec_ms),
                Some(ter_ms),
                &prefer_lang,
                &signs_pattern,
            )?;

            write_opts_json(&out_opts, &tokens)?;
            println!("Wrote opts.json -> {}", out_opts);
        }
        Command::Mux { reference, secondary, tertiary, output, sec_delay, ter_delay, mkvmerge, prefer_lang, signs_pattern, out_opts } => {
            let ref_probe: ProbeResult = probe_streams(&mkvmerge, &reference)?;
            let sec_probe = if let Some(ref s) = secondary { Some(probe_streams(&mkvmerge, s)?) } else { None };
            let ter_probe = if let Some(ref t) = tertiary { Some(probe_streams(&mkvmerge, t)?) } else { None };

            let p = build_plan(Some(sec_delay), Some(ter_delay));
            let (ref_ms, sec_ms, ter_ms) = adjusted_delays(&p);
            println!("{}", summarize_plan(&p));

            let tokens = build_tokens_with_policy(
                &output,
                &reference,
                &ref_probe,
                secondary.as_deref(),
                sec_probe.as_ref(),
                tertiary.as_deref(),
                ter_probe.as_ref(),
                ref_ms,
                Some(sec_ms),
                Some(ter_ms),
                &prefer_lang,
                &signs_pattern,
            )?;
            write_opts_json(&out_opts, &tokens)?;
            println!("Wrote opts.json -> {}", &out_opts);
            let status = std::process::Command::new(if mkvmerge.is_empty() { "mkvmerge" } else { &mkvmerge })
                .arg(format!("@{}", &out_opts))
                .status()?;
            if !status.success() {
                eprintln!("mkvmerge failed with status {:?}", status.code());
            }
        }
    }
    Ok(())
}
