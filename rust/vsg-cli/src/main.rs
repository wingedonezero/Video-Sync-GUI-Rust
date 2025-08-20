use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use std::process::Command as PCommand;
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_work_dir, default_output_dir};
use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr, XCorrParams};
use vsg_core::analyze::videodiff::run_videodiff;
use vsg_core::model::SelectionManifest;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name="vsg", version, about="Video-Sync-GUI-Rust CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: SubCmd,
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    Extract {
        #[arg(long)] manifest: Option<PathBuf>,
        #[arg(long)] ref_file: Option<String>,
        #[arg(long)] sec_file: Option<String>,
        #[arg(long)] ter_file: Option<String>,
        #[arg(long)] ref_probe: Option<String>,
        #[arg(long)] sec_probe: Option<String>,
        #[arg(long)] ter_probe: Option<String>,
        #[arg(long)] work_dir: Option<PathBuf>,
        #[arg(long)] out_dir: Option<PathBuf>,
        #[arg(long, default_value_t=false)] keep_temp: bool,
    },
    Analyze {
        /// Path to enriched selection (will auto-pick matching languages)
        #[arg(long)] from_manifest: Option<PathBuf>,

        /// Manual paths (still supported)
        #[arg(long)] ref_audio_path: Option<String>,
        #[arg(long)] sec_audio_path: Option<String>,
        #[arg(long)] ter_audio_path: Option<String>,

        /// Desired language to match (defaults to REF track language in manifest)
        #[arg(long)] lang: Option<String>,

        #[arg(long, default_value_t=10)] chunks: usize,
        #[arg(long, default_value_t=8.0)] chunk_dur: f64,
        /// Lower sample-rate speeds up XCorr; 12000 is a good default
        #[arg(long, default_value_t=12000)] sample_rate: u32,
        #[arg(long, default_value_t=0.80)] min_match: f64,
        #[arg(long)] duration_s: f64,
        #[arg(long)] videodiff: Option<String>,
        #[arg(long)] ref_video_path: Option<String>,
        #[arg(long)] other_video_path: Option<String>,
        #[arg(long)] err_min: Option<f64>,
        #[arg(long)] err_max: Option<f64>,
        #[arg(long)] work_dir: Option<PathBuf>,
        #[arg(long, default_value_t=false)] keep_temp: bool,
    }
}

#[derive(serde::Deserialize)]
struct ProbeTrack { id:u32, #[serde(rename="type")] _kind:String, codec_id:Option<String>, codec:Option<String>, language:Option<String> }
#[derive(serde::Deserialize)]
struct ProbeFile { tracks:Vec<ProbeTrack> }

fn mkvmerge_probe_json(input:&str) -> ProbeFile {
    let out = PCommand::new("mkvmerge").arg("-J").arg(input).output().expect("spawn mkvmerge -J");
    if !out.status.success() {
        panic!("mkvmerge -J failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<ProbeFile>(&txt).expect("parse mkvmerge -J")
}

fn enrich_with_probe(sel:&mut SelectionManifest) {
    // probe each unique input and fill missing codec/language
    use std::collections::HashMap;
    let mut inputs: Vec<String> = Vec::new();
    for s in [&sel.ref_tracks, &sel.sec_tracks, &sel.ter_tracks] {
        for e in s.iter() { if !inputs.contains(&e.file_path) { inputs.push(e.file_path.clone()); } }
    }
    let mut map: HashMap<(String,u32), (Option<String>, Option<String>)> = HashMap::new();
    for inp in inputs.iter() {
        let pf = mkvmerge_probe_json(inp);
        for t in pf.tracks { map.insert((inp.clone(), t.id), (t.codec_id.or(t.codec), t.language)); }
    }
    for e in sel.ref_tracks.iter_mut().chain(sel.sec_tracks.iter_mut()).chain(sel.ter_tracks.iter_mut()) {
        if e.codec.is_none() || e.language.is_none() {
            if let Some((c,l)) = map.get(&(e.file_path.clone(), e.track_id)) {
                if e.codec.is_none() { e.codec = c.clone(); }
                if e.language.is_none() { e.language = l.clone(); }
            }
        }
    }
}

fn extract_from_manifest(manifest_path:&PathBuf, work:&PathBuf) {
    let text = fs::read_to_string(manifest_path).expect("read manifest");
    let mut sel: SelectionManifest = serde_json::from_str(&text).expect("parse manifest");
    enrich_with_probe(&mut sel);
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
        SubCmd::Extract { manifest, ref_file:_, sec_file:_, ter_file:_, ref_probe:_, sec_probe:_, ter_probe:_, work_dir, out_dir, keep_temp:_ } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let _out = out_dir.unwrap_or_else(|| default_output_dir());
            fs::create_dir_all(&work).expect("create work dir");
            if let Some(mani) = manifest { extract_from_manifest(&mani, &work); } else {
                eprintln!("Use --manifest <selection.json> (legacy flags removed in this flow).");
                std::process::exit(2);
            }
        }
        SubCmd::Analyze { from_manifest, ref_audio_path, sec_audio_path, ter_audio_path, lang, chunks, chunk_dur, sample_rate, min_match, duration_s, videodiff, ref_video_path, other_video_path, err_min, err_max, work_dir, keep_temp:_ } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
            fs::create_dir_all(&manifest_dir).expect("manifest dir");

            // Resolve audio paths
            let (mut ref_path, mut sec_path, mut ter_path) = (None, None, None);
            if let Some(m) = from_manifest.as_ref() {
                // read enriched or raw selection
                let txt = fs::read_to_string(m).expect("read selection");
                let sel: SelectionManifest = serde_json::from_str(&txt).expect("parse selection");
                // choose first REF audio
                let ref_audio = sel.ref_tracks.iter().enumerate().find(|(_,t)| t.r#type=="audio");
                if let Some((i, t)) = ref_audio {
                    let desired_lang = lang.clone().or_else(|| t.language.clone());
                    // each extracted filename follows 000_audio.<lang>.<ext>
                    let ref_glob = format!("{:03}_audio.{}", i, t.language.clone().unwrap_or_else(||"und".into()));
                    let ref_dir = work.join("ref");
                    // we don't have glob; construct expected prefix and scan directory
                    if let Ok(entries) = fs::read_dir(&ref_dir) {
                        for e in entries.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.starts_with(&format!("{:03}_audio.", i)) { ref_path = Some(ref_dir.join(name).to_string_lossy().to_string()); break; }
                        }
                    }
                    // SEC choose first matching language else first audio
                    let sec_dir = work.join("sec");
                    if let Ok(entries) = fs::read_dir(&sec_dir) {
                        let mut first_audio=None;
                        for e in entries.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.contains("_audio.") {
                                first_audio.get_or_insert(sec_dir.join(&name).to_string_lossy().to_string());
                                if let Some(dl) = desired_lang.as_ref() {
                                    if name.contains(&format!(".{}.", dl)) { sec_path = Some(sec_dir.join(&name).to_string_lossy().to_string()); break; }
                                }
                            }
                        }
                        if sec_path.is_none() { sec_path = first_audio; }
                    }
                    // TER same rule
                    let ter_dir = work.join("ter");
                    if let Ok(entries) = fs::read_dir(&ter_dir) {
                        let mut first_audio=None;
                        for e in entries.flatten() {
                            let name = e.file_name().to_string_lossy().to_string();
                            if name.contains("_audio.") {
                                first_audio.get_or_insert(ter_dir.join(&name).to_string_lossy().to_string());
                                if let Some(dl) = desired_lang.as_ref() {
                                    if name.contains(&format!(".{}.", dl)) { ter_path = Some(ter_dir.join(&name).to_string_lossy().to_string()); break; }
                                }
                            }
                        }
                        if ter_path.is_none() { ter_path = first_audio; }
                    }
                }
            }
            // allow manual override
            if ref_path.is_none() { ref_path = ref_audio_path.clone(); }
            if sec_path.is_none() { sec_path = sec_audio_path.clone(); }
            if ter_path.is_none() { ter_path = ter_audio_path.clone(); }

            let ref_audio_path = ref_path.expect("ref audio path not resolved");
            let mut result = serde_json::json!({
                "method":"audio-xcorr",
                "params": {"chunks":chunks,"chunk_dur":chunk_dur,"min_match":min_match,"sample_rate":sample_rate},
                "delays_ms_signed": {},
                "global_shift_ms": 0,
                "delays_ms_positive": {}
            });
            if let Some(sec) = sec_path.as_ref() {
                let r = analyze_audio_xcorr(&ref_audio_path, sec, duration_s, &XCorrParams{chunks,chunk_dur_s:chunk_dur,sample_rate,min_match}).expect("xcorr sec");
                result["delays_ms_signed"]["sec"] = serde_json::json!(r.delay_ms);
            }
            if let Some(ter) = ter_path.as_ref() {
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

            let mut outp = manifest_dir.clone(); outp.push("analysis.json");
            fs::write(&outp, serde_json::to_string_pretty(&result).unwrap()).expect("write analysis manifest");
            println!("Analysis manifest: {}", outp.to_string_lossy());
        }
    }
}
