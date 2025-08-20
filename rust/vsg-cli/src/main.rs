use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as PCommand;
use vsg_core::analyze::audio_xcorr::{analyze_audio_xcorr_detailed, Band, Method, StereoMode, XCorrParams};
use vsg_core::extract::run::run_mkvextract;
use vsg_core::fsutil::{default_output_dir, default_work_dir};
use vsg_core::model::SelectionManifest;

#[derive(Parser, Debug)]
#[command(name = "vsg", version, about = "Video-Sync-GUI-Rust CLI")]
struct Cli { #[command(subcommand)] cmd: SubCmd }

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

#[derive(Copy, Clone, Debug, ValueEnum)] enum SampleRate { S12000, S24000, S48000 }
impl SampleRate { fn hz(self)->u32{ match self { SampleRate::S12000=>12000, SampleRate::S24000=>24000, SampleRate::S48000=>48000 } } }
#[derive(Copy, Clone, Debug, ValueEnum)] enum Stereo { Mono, Left, Right, Mid, Best }
impl From<Stereo> for StereoMode { fn from(s:Stereo)->Self{ match s { Stereo::Mono=>StereoMode::Mono, Stereo::Left=>StereoMode::Left, Stereo::Right=>StereoMode::Right, Stereo::Mid=>StereoMode::Mid, Stereo::Best=>StereoMode::Best } } }
#[derive(Copy, Clone, Debug, ValueEnum)] enum CorrMethod { Fft, Compat }
impl From<CorrMethod> for Method { fn from(m:CorrMethod)->Self{ match m { CorrMethod::Fft=>Method::Fft, CorrMethod::Compat=>Method::Compat } } }
#[derive(Copy, Clone, Debug, ValueEnum)] enum BandSel { None, Voice }
impl From<BandSel> for Band { fn from(b:BandSel)->Self{ match b{ BandSel::None=>Band::None, BandSel::Voice=>Band::Voice } } }

#[derive(serde::Deserialize)]
struct ProbeTrack { id:u32, #[serde(rename="type")] _kind:String, codec_id:Option<String>, codec:Option<String>, language:Option<String> }
#[derive(serde::Deserialize)]
struct ProbeFile { tracks:Vec<ProbeTrack> }

fn mkvmerge_probe_json(input:&str)->ProbeFile{
    let out = PCommand::new("mkvmerge").arg("-J").arg(input).output().expect("spawn mkvmerge -J");
    if !out.status.success() { panic!("mkvmerge -J failed: {}", String::from_utf8_lossy(&out.stderr)); }
    let txt = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<ProbeFile>(&txt).expect("parse mkvmerge -J")
}

fn enrich_with_probe(sel:&mut SelectionManifest){
    use std::collections::{HashMap, HashSet};
    let mut inputs:HashSet<String>=HashSet::new();
    for e in sel.ref_tracks.iter().chain(sel.sec_tracks.iter()).chain(sel.ter_tracks.iter()){ inputs.insert(e.file_path.clone()); }
    let mut map:HashMap<(String,u32),(Option<String>,Option<String>)>=HashMap::new();
    for inp in inputs.iter(){ let pf=mkvmerge_probe_json(inp);
        for t in pf.tracks { map.insert((inp.clone(), t.id), (t.codec_id.or(t.codec), t.language)); }
    }
    for e in sel.ref_tracks.iter_mut().chain(sel.sec_tracks.iter_mut()).chain(sel.ter_tracks.iter_mut()){
        if e.codec.is_none() || e.language.is_none(){
            if let Some((c,l)) = map.get(&(e.file_path.clone(), e.track_id)){
                if e.codec.is_none(){ e.codec = c.clone(); }
                if e.language.is_none(){ e.language = l.clone(); }
            }
        }
    }
}

fn lang_from_filename(name:&str)->Option<String>{
    // expects NNN_audio.<lang>.<ext>
    if let Some(rest) = name.splitn(2, "_audio.").nth(1) {
        if let Some(lang) = rest.split('.').next() { return Some(lang.to_string()); }
    }
    None
}

fn file_lang_from_path(p:&str)->String{
    let fname = Path::new(p).file_name().and_then(|s| s.to_str()).unwrap_or(p);
    lang_from_filename(fname).unwrap_or_else(|| "und".into())
}

fn main(){
    let cli = Cli::parse();
    match cli.cmd {
        SubCmd::Extract { manifest, work_dir, out_dir, keep_temp:_ } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let _out = out_dir.unwrap_or_else(|| default_output_dir());
            fs::create_dir_all(&work).expect("create work dir");
            if let Some(m)=manifest { 
                let text = fs::read_to_string(&m).expect("read manifest");
                let mut sel:SelectionManifest = serde_json::from_str(&text).expect("parse manifest");
                enrich_with_probe(&mut sel);
                // Save enriched manifest and run extraction
                let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
                fs::create_dir_all(&manifest_dir).expect("manifest dir");
                let mut sel_copy = manifest_dir.clone(); sel_copy.push("selection.json");
                fs::write(&sel_copy, serde_json::to_string_pretty(&sel).unwrap()).expect("write selection copy");
                let summary = run_mkvextract(&sel, &work).expect("mkvextract failed");
                let mut log_path = manifest_dir.clone(); log_path.push("extract.log");
                let lines = summary.files.iter().map(|s| format!("EXTRACTED {}", s)).collect::<Vec<_>>().join("\n");
                fs::write(&log_path, lines).expect("write log");
                println!("Selection manifest: {}", sel_copy.to_string_lossy());
            } else {
                eprintln!("Use --manifest <selection.json>"); std::process::exit(2);
            }
        }
        SubCmd::Analyze { from_manifest, ref_audio_path, sec_audio_path, ter_audio_path, lang, chunks, chunk_dur, duration_s, sample_rate, stereo_mode, method, band, work_dir } => {
            let work = work_dir.unwrap_or_else(|| default_work_dir());
            let mut manifest_dir = work.clone(); manifest_dir.push("manifest");
            fs::create_dir_all(&manifest_dir).expect("manifest dir");

            // Resolve paths and languages
            let mut ref_path = ref_audio_path.clone();
            let mut sec_path = sec_audio_path.clone();
            let mut ter_path = ter_audio_path.clone();
            let mut ref_lang: String = "und".into();
            let mut sec_lang: String = "und".into();
            let mut ter_lang: String = "und".into();

            if let Some(m) = from_manifest.as_ref(){
                let txt = fs::read_to_string(m).expect("read selection");
                let sel:SelectionManifest = serde_json::from_str(&txt).expect("parse selection");
                // REF first audio index & language (from manifest)
                let ref_idx_lang = sel.ref_tracks.iter().enumerate().find(|(_,t)| t.r#type=="audio").map(|(i,t)| (i, t.language.clone().unwrap_or_else(||"und".into())));
                if let Some((i, rlang)) = ref_idx_lang {
                    ref_lang = rlang.clone();
                    // choose REF file by index prefix
                    if ref_path.is_none(){
                        let ref_dir = work.join("ref");
                        if let Ok(entries) = fs::read_dir(&ref_dir){
                            for e in entries.flatten(){
                                let name = e.file_name().to_string_lossy().to_string();
                                if name.starts_with(&format!("{:03}_audio.", i)){
                                    ref_lang = lang_from_filename(&name).unwrap_or(ref_lang);
                                    ref_path = Some(ref_dir.join(&name).to_string_lossy().to_string());
                                    break;
                                }
                            }
                        }
                    }
                    // desired language
                    let desired = lang.clone().unwrap_or(ref_lang.clone());

                    // SEC pick matching language else first
                    if sec_path.is_none(){
                        let sec_dir = work.join("sec");
                        if let Ok(entries) = fs::read_dir(&sec_dir){
                            let mut first_audio=None;
                            for e in entries.flatten(){
                                let name=e.file_name().to_string_lossy().to_string();
                                if name.contains("_audio."){
                                    let full = sec_dir.join(&name).to_string_lossy().to_string();
                                    if first_audio.is_none(){ first_audio=Some(full.clone()); }
                                    if name.contains(&format!(".{}.", desired)){
                                        sec_lang = lang_from_filename(&name).unwrap_or("und".into());
                                        sec_path = Some(full); break;
                                    }
                                }
                            }
                            if sec_path.is_none(){
                                if let Some(f)=first_audio { sec_lang = file_lang_from_path(&f); sec_path=Some(f); }
                            }
                        }
                    }
                    // TER
                    if ter_path.is_none(){
                        let ter_dir = work.join("ter");
                        if let Ok(entries) = fs::read_dir(&ter_dir){
                            let mut first_audio=None;
                            for e in entries.flatten(){
                                let name=e.file_name().to_string_lossy().to_string();
                                if name.contains("_audio."){
                                    let full = ter_dir.join(&name).to_string_lossy().to_string();
                                    if first_audio.is_none(){ first_audio=Some(full.clone()); }
                                    if name.contains(&format!(".{}.", desired)){
                                        ter_lang = lang_from_filename(&name).unwrap_or("und".into());
                                        ter_path = Some(full); break;
                                    }
                                }
                            }
                            if ter_path.is_none(){
                                if let Some(f)=first_audio { ter_lang = file_lang_from_path(&f); ter_path=Some(f); }
                            }
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

            let ref_lang_for_match = ref_lang.clone(); // avoid borrowing json later

            let mut json = serde_json::json!({
                "meta": {
                    "ref_audio_path": ref_audio_path,
                    "ref_language": ref_lang,
                    "sec_audio_path": sec_path,
                    "ter_audio_path": ter_path,
                    "sec_language": sec_lang,
                    "ter_language": ter_lang
                },
                "params": {
                    "chunks": chunks, "chunk_dur_s": chunk_dur, "sample_rate": sample_rate.hz(),
                    "stereo_mode": format!("{:?}", stereo_mode),
                    "method": format!("{:?}", method),
                    "band": format!("{:?}", band)
                },
                "runs": {},
                "final": {}
            });

            // Closure that does NOT capture `json`, avoiding E0502
            let run_one = |_label:&str, ref_path:&str, other_path:&str, lang:&str, params:&XCorrParams, ref_lang_for_match:&str| -> serde_json::Value {
                let (res, chunks_vec) = analyze_audio_xcorr_detailed(ref_path, other_path, duration_s, params).expect("xcorr detailed");
                let chunks: Vec<_> = chunks_vec.iter().map(|c| serde_json::json!({
                    "center_s": c.center_s,
                    "window_samples": c.window_samples,
                    "lag_ns": c.lag_ns,
                    "lag_ms": c.lag_ms,
                    "peak": c.peak
                })).collect();
                let matched = !lang.is_empty() && lang == ref_lang_for_match;
                serde_json::json!({
                    "language": lang,
                    "language_matched": matched,
                    "chunks": chunks,
                    "summary": {
                        "median_delay_ns": res.delay_ns,
                        "median_delay_ms": res.delay_ms,
                        "peak_max": res.peak_score
                    }
                })
            };

            let mut delays_ms_signed = serde_json::Map::new();
            let mut delays_ns_signed = serde_json::Map::new();
            let mut peaks = serde_json::Map::new();

            if let Some(sec) = json["meta"]["sec_audio_path"].as_str() {
                let entry = run_one("sec", json["meta"]["ref_audio_path"].as_str().unwrap(), sec, json["meta"]["sec_language"].as_str().unwrap_or("und"), &params, &ref_lang_for_match);
                delays_ms_signed.insert("sec".into(), entry["summary"]["median_delay_ms"].clone());
                delays_ns_signed.insert("sec".into(), entry["summary"]["median_delay_ns"].clone());
                peaks.insert("sec".into(), entry["summary"]["peak_max"].clone());
                json["runs"]["sec"] = entry;
            }
            if let Some(ter) = json["meta"]["ter_audio_path"].as_str() {
                let entry = run_one("ter", json["meta"]["ref_audio_path"].as_str().unwrap(), ter, json["meta"]["ter_language"].as_str().unwrap_or("und"), &params, &ref_lang_for_match);
                delays_ms_signed.insert("ter".into(), entry["summary"]["median_delay_ms"].clone());
                delays_ns_signed.insert("ter".into(), entry["summary"]["median_delay_ns"].clone());
                peaks.insert("ter".into(), entry["summary"]["peak_max"].clone());
                json["runs"]["ter"] = entry;
            }

            // Global shift so all are non-negative in ms domain
            let mut present = vec![0i64];
            if let Some(v) = delays_ms_signed.get("sec").and_then(|x| x.as_i64()) { present.push(v); }
            if let Some(v) = delays_ms_signed.get("ter").and_then(|x| x.as_i64()) { present.push(v); }
            let minv = *present.iter().min().unwrap_or(&0);
            let global_shift_ms = if minv < 0 { -minv } else { 0 };

            let mut delays_ms_positive = serde_json::Map::new();
            if let Some(v) = delays_ms_signed.get("sec").and_then(|x| x.as_i64()) { delays_ms_positive.insert("sec".into(), serde_json::json!(v + global_shift_ms)); }
            if let Some(v) = delays_ms_signed.get("ter").and_then(|x| x.as_i64()) { delays_ms_positive.insert("ter".into(), serde_json::json!(v + global_shift_ms)); }

            json["final"] = serde_json::json!({
                "delays_ms_signed": delays_ms_signed,
                "delays_ns_signed": delays_ns_signed,
                "global_shift_ms": global_shift_ms,
                "delays_ms_positive": delays_ms_positive,
                "peaks": peaks,
                "notes": [
                    "delays_ms_signed: signed median delay per source (ms)",
                    "delays_ns_signed: same in nanoseconds (ns)",
                    "global_shift_ms: minimum shift to make all delays non-negative",
                    "delays_ms_positive: per-source delays after applying global_shift_ms",
                    "runs.*.chunks: detailed per-chunk lags/peaks for QA"
                ]
            });

            let mut outp = manifest_dir.clone(); outp.push("analysis.json");
            fs::write(&outp, serde_json::to_string_pretty(&json).unwrap()).expect("write analysis manifest");
            println!("Analysis manifest: {}", outp.to_string_lossy());
        }
    }
}
