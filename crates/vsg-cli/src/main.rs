use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing_subscriber::{fmt, EnvFilter};
use vsg_core::tracks::{probe_streams, ProbeResult};
use vsg_core::plan::{build_plan, adjusted_delays, summarize_plan};
use vsg_core::mkvmerge::{build_simple_tokens, write_opts_json};

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
        #[arg(long)] file: String,
        #[arg(long, default_value="")] mkvmerge: String,
    },
    /// Build mkvmerge @opts.json from ref/sec/ter and provided delays.
    MakeOpts {
        #[arg(long)] reference: String,
        #[arg(long)] secondary: Option<String>,
        #[arg(long)] tertiary: Option<String>,
        /// Output path for opts.json
        #[arg(long)] out_opts: String,
        /// Secondary raw delay (ms), before always-add anchoring (default 0)
        #[arg(long, default_value_t = 0)] sec_delay: i32,
        /// Tertiary raw delay (ms), before always-add anchoring (default 0)
        #[arg(long, default_value_t = 0)] ter_delay: i32,
        /// Optional mkvmerge path if not on PATH
        #[arg(long, default_value="")] mkvmerge: String,
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
        Command::MakeOpts { reference, secondary, tertiary, out_opts, sec_delay, ter_delay, mkvmerge } => {
            // Probe files
            let ref_probe: ProbeResult = probe_streams(&mkvmerge, &reference)?;
            let sec_probe = if let Some(ref s) = secondary {
                Some(probe_streams(&mkvmerge, s)?)
            } else { None };
            let ter_probe = if let Some(ref t) = tertiary {
                Some(probe_streams(&mkvmerge, t)?)
            } else { None };

            // Plan (always-add)
            let p = build_plan(Some(sec_delay), Some(ter_delay));
            let (ref_ms, sec_ms, ter_ms) = adjusted_delays(&p);
            println!("{}", summarize_plan(&p));

            // Build tokens
            let tokens = build_simple_tokens(
                "OUT.mkv",
                &reference,
                &ref_probe,
                secondary.as_deref(),
                sec_probe.as_ref(),
                tertiary.as_deref(),
                ter_probe.as_ref(),
                ref_ms,
                Some(sec_ms),
                Some(ter_ms),
            )?;

            // Write @opts.json
            write_opts_json(&out_opts, &tokens)?;
            println!("Wrote opts.json -> {}", out_opts);
        }
    }
    Ok(())
}
