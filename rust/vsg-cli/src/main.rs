
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::PathBuf;
use std::process::Command as PCommand;
use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr, Band, Method, StereoMode, XCorrParams};
use vsg_core::analyze::videodiff::run_videodiff;
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_output_dir, default_work_dir};
use vsg_core::model::SelectionManifest;

#[derive(Parser, Debug)]
#[command(name = "vsg", version, about = "Video-Sync-GUI-Rust CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: SubCmd,
}

#[derive(Subcommand, Debug)]
enum SubCmd {
    Extract {
        #[arg(long)] manifest: Option<PathBuf>,
        #[arg(long)] work_dir: Option<PathBuf>,
        #[arg(long)] out_dir: Option<PathBuf>,
        #[arg(long, default_value_t = false)] keep_temp: bool,
    },
    Analyze {
        #[arg(long)] from_manifest: Option<PathBuf>,
        #[arg(long)] ref_audio_path: Option<String>,
        #[arg(long)] sec_audio_path: Option<String>,
        #[arg(long)] ter_audio_path: Option<String>,
        #[arg(long)] lang: Option<String>,
        #[arg(long, default_value_t = 10)] chunks: usize,
        #[arg(long, default_value_t = 8.0)] chunk_dur: f64,
        #[arg(long)] duration_s: f64,
        #[arg(long, value_enum, default_value_t = SampleRate::S24000)] sample_rate: SampleRate,
        #[arg(long, value_enum, default_value_t = Stereo::Best)] stereo_mode: Stereo,
        #[arg(long, value_enum, default_value_t = CorrMethod::Fft)] method: CorrMethod,
        #[arg(long, value_enum, default_value_t = BandSel::None)] band: BandSel,
        #[arg(long)] work_dir: Option<PathBuf>,
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum SampleRate { S12000, S24000, S48000 }
impl SampleRate { fn hz(self) -> u32 { match self { SampleRate::S12000 => 12000, SampleRate::S24000 => 24000, SampleRate::S48000 => 48000 } } }

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Stereo { Mono, Left, Right, Mid, Best }
impl From<Stereo> for StereoMode {
    fn from(s: Stereo) -> Self {
        match s {
            Stereo::Mono => StereoMode::Mono,
            Stereo::Left => StereoMode::Left,
            Stereo::Right => StereoMode::Right,
            Stereo::Mid => StereoMode::Mid,
            Stereo::Best => StereoMode::Best,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CorrMethod { Fft, Compat }
impl From<CorrMethod> for Method {
    fn from(m: CorrMethod) -> Self { match m { CorrMethod::Fft => Method::Fft, CorrMethod::Compat => Method::Compat } }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum BandSel { None, Voice }
impl From<BandSel> for Band { fn from(b: BandSel) -> Self { match b { BandSel::None => Band::None, BandSel::Voice => Band::Voice } } }

#[derive(serde::Deserialize)]
struct ProbeTrack { id: u32, #[serde(rename = "type")] _kind: String, codec_id: Option<String>, codec: Option<String>, language: Option<String> }
#[derive(serde::Deserialize)]
struct ProbeFile { tracks: Vec<ProbeTrack> }

fn mkvmerge_probe_json(input: &str) -> ProbeFile {
    let out = PCommand::new("mkvmerge").arg("-J").arg(input).output().expect("spawn mkvmerge -J");
    if !out.status.success() { panic!("mkvmerge -J failed: {}", String::from_utf8_lossy(&out.stderr)); }
    let txt = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<ProbeFile>(&txt).expect("parse mkvmerge -J")
}

fn enrich_with_probe(sel: &mut SelectionManifest) {
    use std::collections::HashSet;
    use std::collections::HashMap;
    let mut inputs: HashSet<String> = HashSet::new();
    for e in sel.ref_tracks.iter().chain(sel.sec_tracks.iter()).chain(sel.ter_tracks.iter()) { inputs.insert(e.file_path.clone()); }
    let mut map: HashMap<(String, u32), (Option<String>, Option<String>)> = HashMap::new();
    for inp in inputs.iter() {
        let pf = mkvmerge_probe_json(inp);
        for t in pf.tracks { map.insert((inp.clone(), t.id), (t.codec_id.or(t.codec), t.language)); }
    }
    for e in sel.ref_tracks.iter_mut().chain(sel.sec_tracks.iter_mut()).chain(sel.ter_tracks.iter_mut()) {
        if e.codec.is_none() || e.language.is_none() {
            if let Some((c, l)) = map.get(&(e.file_path.clone(), e.track_id)) {
                if e.codec.is_none() { e.codec = c.clone(); }
                if e.language.is_none() { e.language = l.clone(); }
            }
        }
    }
}

fn extract_from_manifest(manifest_path: &PathBuf, work: &PathBuf) {
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
        SubCmd::Extract { manifest, work_dir, out_dir, keep_temp: _ } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let _out = out_dir.unwrap_or_else(|| default_output_dir());
            fs::create_dir_all(&work).expect("create work dir");
            if let Some(m) = manifest { extract_from_manifest(&m, &work); } else {
                eprintln!("Use --manifest <selection.json>");
                std::process::exit(2);
            }
        }
        SubCmd::Analyze {
            from_manifest, ref_audio_path, sec_audio_path, ter_audio_path, lang,
            chunks, chunk_dur, duration_s, sample_rate, stereo_mode, method, band, work_dir
        } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
            fs::create_dir_all(&manifest_dir).expect("manifest dir");

            let mut ref_path = ref_audio_path.clone();
            let mut sec_path = sec_audio_path.clone();
            let mut ter_path = ter_audio_path.clone();

            if let Some(m) = from_manifest.as_ref() {
                let txt = fs::read_to_string(m).expect("read selection");
                let sel: SelectionManifest = serde_json::from_str(&txt).expect("parse selection");
                let ref_idx_lang = sel.ref_tracks.iter().enumerate().find(|(_, t)| t.r#type == "audio").map(|(i, t)| (i, t.language.clone().unwrap_or_else(|| "und".into())));
                if let Some((i, ref_lang)) = ref_idx_lang {
                    if ref_path.is_none() {
                        let ref_dir = work.join("ref");
                        if let Ok(entries) = fs::read_dir(&ref_dir) {
                            for e in entries.flatten() {
                                let name = e.file_name().to_string_lossy().to_string();
                                if name.starts_with(&format!("{:03}_audio.", i)) {
                                    ref_path = Some(ref_dir.join(name).to_string_lossy().to_string());
                                    break;
                                }
                            }
                        }
                    }
                    let desired = lang.clone().unwrap_or(ref_lang);
                    let sec_dir = work.join("sec");
                    if sec_path.is_none() {
                        if let Ok(entries) = fs::read_dir(&sec_dir) {
                            let mut first_audio = None;
                            for e in entries.flatten() {
                                let name = e.file_name().to_string_lossy().to_string();
                                if name.contains("_audio.") {
                                    let full = sec_dir.join(&name).to_string_lossy().to_string();
                                    if first_audio.is_none() { first_audio = Some(full.clone()); }
                                    if name.contains(&format!(".{}.", desired)) { sec_path = Some(full); break; }
                                }
                            }
                            if sec_path.is_none() { sec_path = first_audio; }
                        }
                    }
                    let ter_dir = work.join("ter");
                    if ter_path.is_none() {
                        if let Ok(entries) = fs::read_dir(&ter_dir) {
                            let mut first_audio = None;
                            for e in entries.flatten() {
                                let name = e.file_name().to_string_lossy().to_string();
                                if name.contains("_audio.") {
                                    let full = ter_dir.join(&name).to_string_lossy().to_string();
                                    if first_audio.is_none() { first_audio = Some(full.clone()); }
                                    if name.contains(&format!(".{}.", desired)) { ter_path = Some(full); break; }
                                }
                            }
                            if ter_path.is_none() { ter_path = first_audio; }
                        }
                    }
                }
            }

            let ref_audio_path = ref_path.expect("ref audio path not resolved");
            let params = XCorrParams {
                chunks,
                chunk_dur_s: chunk_dur,
                sample_rate: sample_rate.hz(),
                min_match: 0.8,
                stereo_mode: stereo_mode.into(),
                method: method.into(),
                band: band.into(),
            };

            let mut result = serde_json::json!({
                "method":"audio-xcorr",
                "params": {
                    "chunks": chunks, "chunk_dur": chunk_dur,
                    "sample_rate": sample_rate.hz(),
                    "stereo_mode": format!("{:?}", stereo_mode),
                    "method": format!("{:?}", method),
                    "band": format!("{:?}", band)
                },
                "delays_ms_signed": {},
                "global_shift_ms": 0,
                "delays_ms_positive": {},
                "peaks": {}
            });

            if let Some(sec) = sec_path.as_ref() {
                let r = analyze_audio_xcorr(&ref_audio_path, sec, duration_s, &params).expect("xcorr sec");
                result["delays_ms_signed"]["sec"] = serde_json::json!(r.delay_ms);
                result["peaks"]["sec"] = serde_json::json!(r.peak_score);
            }
            if let Some(ter) = ter_path.as_ref() {
                let r = analyze_audio_xcorr(&ref_audio_path, ter, duration_s, &params).expect("xcorr ter");
                result["delays_ms_signed"]["ter"] = serde_json::json!(r.delay_ms);
                result["peaks"]["ter"] = serde_json::json!(r.peak_score);
            }

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
