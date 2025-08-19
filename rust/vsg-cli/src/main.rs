use clap::Parser;
use std::path::PathBuf;
use vsg_core::probe::load_probe;
use vsg_core::extract::select::Defaults;
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_work_dir, default_output_dir};
use vsg_core::model::SelectionManifest;

/// Step 1: Extraction (selection + mkvextract runner)
#[derive(Parser, Debug)]
#[command(name="vsg", version, about="Video-Sync-GUI-Rust CLI")]
struct Args {
    /// Reference media file path
    #[arg(long)]
    ref_file: String,
    /// Secondary media file path
    #[arg(long)]
    sec_file: Option<String>,
    /// Tertiary media file path
    #[arg(long)]
    ter_file: Option<String>,

    /// Probe JSON for REF (dev/fixtures)
    #[arg(long)]
    ref_probe: String,
    /// Probe JSON for SEC (dev/fixtures)
    #[arg(long)]
    sec_probe: Option<String>,
    /// Probe JSON for TER (dev/fixtures)
    #[arg(long)]
    ter_probe: Option<String>,

    /// Work dir (temp). If omitted, defaults to <binary_dir>/_work/job_<timestamp>
    #[arg(long)]
    work_dir: Option<PathBuf>,

    /// Output dir (final outputs). If omitted, defaults to <binary_dir>/_out
    #[arg(long)]
    out_dir: Option<PathBuf>,

    /// Keep extracted media files (otherwise cleaned on success)
    #[arg(long, default_value_t=false)]
    keep_temp: bool,
}

fn main() {
    let args = Args::parse();

    let work = args.work_dir.unwrap_or_else(|| default_work_dir());
    let _out = args.out_dir.unwrap_or_else(|| default_output_dir());
    std::fs::create_dir_all(&work).expect("create work dir");

    // load probes
    let refp = load_probe(&args.ref_probe).expect("load ref probe");
    let secp = args.sec_probe.as_ref().map(|p| load_probe(p).expect("load sec probe"));
    let terp = args.ter_probe.as_ref().map(|p| load_probe(p).expect("load ter probe"));

    // selection using defaults (later: flags/predicates)
    let sel = Defaults::select(&args.ref_file, &refp, args.sec_file.as_deref(), secp.as_ref(), args.ter_file.as_deref(), terp.as_ref());

    // persist selection manifest
    let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
    std::fs::create_dir_all(&manifest_dir).expect("manifest dir");
    let mut sel_path = manifest_dir.clone(); sel_path.push("selection.json");
    std::fs::write(&sel_path, serde_json::to_string_pretty(&sel).unwrap()).expect("write selection");

    // run mkvextract according to selection
    let summary = run_mkvextract(&sel, &work).expect("mkvextract failed");

    // write a tiny log
    let mut log_path = manifest_dir.clone(); log_path.push("extract.log");
    let lines = summary.files.iter().map(|s| format!("EXTRACTED {}", s)).collect::<Vec<_>>().join("
");
    std::fs::write(&log_path, lines).expect("write log");

    // cleanup policy for Step 1: keep manifests, optionally delete extracted media
    if !args.keep_temp {
        // remove ref/sec/ter subdirs, keep manifest
        for d in ["ref","sec","ter"] {
            let mut p = work.clone(); p.push(d);
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    println!("Selection manifest: {}", sel_path.to_string_lossy());
}
