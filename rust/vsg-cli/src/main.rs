use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use vsg_core::probe::load_probe;
use vsg_core::extract::select::Defaults;
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_work_dir, default_output_dir};
use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr, XCorrParams};
use vsg_core::analyze::videodiff::run_videodiff;
use vsg_core::model::SelectionManifest;

#[derive(Parser, Debug)]
#[command(name="vsg", version, about="Video-Sync-GUI-Rust CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Step 1: extraction (selection + mkvextract)
    Extract {
        /// Optional manifest: if provided, we ignore --ref/--sec/--ter flags and run mkvextract using this manifest
        #[arg(long)]
        manifest: Option<PathBuf>,

        /// Reference media file path
        #[arg(long)]
        ref_file: Option<String>,
        /// Secondary media file path
        #[arg(long)]
        sec_file: Option<String>,
        /// Tertiary media file path
        #[arg(long)]
        ter_file: Option<String>,

        /// Probe JSON for REF (dev/fixtures)
        #[arg(long)]
        ref_probe: Option<String>,
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
    },
    /// Step 2: analysis (audio-xcorr or videodiff)
    Analyze {
        #[arg(long)] ref_audio_path: String,
        #[arg(long)] sec_audio_path: Option<String>,
        #[arg(long)] ter_audio_path: Option<String>,
        #[arg(long, default_value_t=10)] chunks: usize,
        #[arg(long, default_value_t=8.0)] chunk_dur: f64,
        #[arg(long, default_value_t=48000)] sample_rate: u32,
        #[arg(long, default_value_t=0.80)] min_match: f64,
        #[arg(long)] duration_s: f64,
        /// Optional videodiff mode: provide two video paths
        #[arg(long)] videodiff: Option<String>,
        #[arg(long)] ref_video_path: Option<String>,
        #[arg(long)] other_video_path: Option<String>,
        #[arg(long)] err_min: Option<f64>,
        #[arg(long)] err_max: Option<f64>,
        #[arg(long)] work_dir: Option<PathBuf>,
        #[arg(long, default_value_t=false)] keep_temp: bool,
    }
}

fn extract_from_manifest(manifest_path:&PathBuf, work:&PathBuf) {
    let text = fs::read_to_string(manifest_path).expect("read manifest");
    let sel: SelectionManifest = serde_json::from_str(&text).expect("parse manifest");
    let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
    fs::create_dir_all(&manifest_dir).expect("manifest dir");
    let mut sel_copy = manifest_dir.clone(); sel_copy.push("selection.json");
    fs::write(&sel_copy, serde_json::to_string_pretty(&sel).unwrap()).expect("write selection copy");

    let summary = run_mkvextract(&sel, work).expect("mkvextract failed");
    let mut log_path = manifest_dir.clone(); log_path.push("extract.log");
    let lines = summary.files.iter().map(|s| format!("EXTRACTED {}", s)).collect::<Vec<_>>().join("\n");
    fs::write(&log_path, lines).expect("write log");
    println!("Selection manifest: {}", sel_copy.to_string_lossy());
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Extract { manifest, ref_file, sec_file, ter_file, ref_probe, sec_probe, ter_probe, work_dir, out_dir, keep_temp } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let _out = out_dir.unwrap_or_else(|| default_output_dir());
            fs::create_dir_all(&work).expect("create work dir");

            if let Some(mani) = manifest {
                extract_from_manifest(&mani, &work);
            } else {
                // legacy flag flow: build selection via probes and Defaults
                let ref_file = ref_file.expect("--ref-file required (or use --manifest)");
                let refp = load_probe(&ref_probe.expect("--ref-probe required")).expect("load ref probe");
                let secp = if let (Some(secf), Some(p)) = (sec_file.as_ref(), sec_probe.as_ref()) {
                    Some(load_probe(p).expect("load sec probe"))
                } else { None };
                let terp = if let (Some(terf), Some(p)) = (ter_file.as_ref(), ter_probe.as_ref()) {
                    Some(load_probe(p).expect("load ter probe"))
                } else { None };

                let sel = Defaults::select(&ref_file, &refp, sec_file.as_deref(), secp.as_ref(), ter_file.as_deref(), terp.as_ref());

                let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
                fs::create_dir_all(&manifest_dir).expect("manifest dir");
                let mut sel_path = manifest_dir.clone(); sel_path.push("selection.json");
                fs::write(&sel_path, serde_json::to_string_pretty(&sel).unwrap()).expect("write selection");

                let summary = run_mkvextract(&sel, &work).expect("mkvextract failed");
                let mut log_path = manifest_dir.clone(); log_path.push("extract.log");
                let lines = summary.files.iter().map(|s| format!("EXTRACTED {}", s)).collect::<Vec<_>>().join("\n");
                fs::write(&log_path, lines).expect("write log");

                if !keep_temp {
                    for d in ["ref","sec","ter"] {
                        let mut p = work.clone(); p.push(d);
                        let _ = fs::remove_dir_all(&p);
                    }
                }
                println!("Selection manifest: {}", sel_path.to_string_lossy());
            }
        }
        Command::Analyze { ref_audio_path, sec_audio_path, ter_audio_path, chunks, chunk_dur, sample_rate, min_match, duration_s, videodiff, ref_video_path, other_video_path, err_min, err_max, work_dir, keep_temp:_ } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
            fs::create_dir_all(&manifest_dir).expect("manifest dir");

            let mut result = serde_json::json!({
                "method":"audio-xcorr",
                "params": {"chunks":chunks,"chunk_dur":chunk_dur,"min_match":min_match,"sample_rate":sample_rate},
                "delays_ms_signed": {},
                "global_shift_ms": 0,
                "delays_ms_positive": {}
            });

            if let Some(sec) = sec_audio_path.as_ref() {
                let r = analyze_audio_xcorr(&ref_audio_path, sec, duration_s, &XCorrParams{chunks,chunk_dur_s:chunk_dur,sample_rate,min_match}).expect("xcorr sec");
                result["delays_ms_signed"]["sec"] = serde_json::json!(r.delay_ms);
            }
            if let Some(ter) = ter_audio_path.as_ref() {
                let r = analyze_audio_xcorr(&ref_audio_path, ter, duration_s, &XCorrParams{chunks,chunk_dur_s:chunk_dur,sample_rate,min_match}).expect("xcorr ter");
                result["delays_ms_signed"]["ter"] = serde_json::json!(r.delay_ms);
            }

            // Global positive-only shift
            let mut present = vec![0i64];
            if let Some(v) = result["delays_ms_signed"].get("sec").and_then(|x| x.as_i64()) { present.push(v); }
            if let Some(v) = result["delays_ms_signed"].get("ter").and_then(|x| x.as_i64()) { present.push(v); }
            let minv = *present.iter().min().unwrap_or(&0i64);
            let g = if minv < 0 { -minv } else { 0 };
            result["global_shift_ms"] = serde_json::json!(g);
            if let Some(v) = result["delays_ms_signed"].get("sec").and_then(|x| x.as_i64()) { result["delays_ms_positive"]["sec"] = serde_json::json!(v + g as i64); }
            if let Some(v) = result["delays_ms_signed"].get("ter").and_then(|x| x.as_i64()) { result["delays_ms_positive"]["ter"] = serde_json::json!(v + g as i64); }

            // Optional videodiff
            if let (Some(vp), Some(rv), Some(ov)) = (videodiff.as_ref(), ref_video_path.as_ref(), other_video_path.as_ref()) {
                let vd = run_videodiff(vp, rv, ov).expect("videodiff");
                result["method"] = serde_json::json!("videodiff");
                result["delays_ms_signed"]["sec"] = serde_json::json!(vd.delay_ms);
                if let (Some(lo), Some(hi)) = (err_min, err_max) {
                    if let Some(e) = vd.error {
                        if e < lo || e > hi { panic!("VideoDiff confidence out of bounds: {}", e); }
                    }
                }
                if let Some(e) = vd.error { result["error"] = serde_json::json!(e); }
                // recompute positive-only for single pair
                let g = if vd.delay_ms < 0 { -vd.delay_ms } else { 0 };
                result["global_shift_ms"] = serde_json::json!(g);
                result["delays_ms_positive"]["sec"] = serde_json::json!(vd.delay_ms + g);
            }

            let mut outp = manifest_dir.clone(); outp.push("analysis.json");
            fs::write(&outp, serde_json::to_string_pretty(&result).unwrap()).expect("write analysis manifest");

            println!("Analysis manifest: {}", outp.to_string_lossy());
        }
    }
}
