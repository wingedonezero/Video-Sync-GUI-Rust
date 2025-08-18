// crates/vsg-cli/src/mux.rs
// Updated mux pipeline: extract elementary streams, apply positive-only delay scheme, build @opts.json

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct MkvmergeJson {
    tracks: Vec<MkvTrack>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    #[serde(default)]
    chapters: Option<Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct MkvTrack {
    id: u32,
    r#type: String,
    codec: String,
    properties: Option<TrackProps>,
}

#[derive(Debug, Deserialize, Clone)]
struct Attachment {
    id: Option<u32>,
    file_name: Option<String>,
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct TrackProps {
    language: Option<String>,
    track_name: Option<String>,
}

fn run(cmd: &mut Command) -> Result<()> {
    let display = format!("{:?}", cmd);
    let status = cmd.status().with_context(|| format!("spawn failed: {}", display))?;
    if !status.success() {
        return Err(anyhow!("Process failed: {}", display));
    }
    Ok(())
}

fn run_out(cmd: &mut Command) -> Result<String> {
    let display = format!("{:?}", cmd);
    let out = cmd.output().with_context(|| format!("spawn failed: {}", display))?;
    if !out.status.success() {
        return Err(anyhow!("Process failed: {}", display));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn mkvmerge_probe(file: &Path, mkvmerge: &Path) -> Result<MkvmergeJson> {
    let json = run_out(&mut Command::new(mkvmerge).arg("-J").arg(file))?;
    let v: MkvmergeJson = serde_json::from_str(&json).context("parse mkvmerge -J")?;
    Ok(v)
}

fn ensure_dirs() -> Result<(PathBuf, PathBuf)> {
    let temp = PathBuf::from("./temp");
    let outdir = PathBuf::from("./output");
    fs::create_dir_all(&temp)?;
    fs::create_dir_all(&outdir)?;
    Ok((temp, outdir))
}

fn ext_for_video_codec(codec: &str) -> &'static str {
    let lc = codec.to_lowercase();
    if lc.contains("avc") || lc.contains("h.264") { ".h264" }
    else if lc.contains("hevc") || lc.contains("h.265") { ".hevc" }
    else if lc.contains("mpeg-2") { ".m2v" }
    else if lc.contains("vc-1") { ".vc1" }
    else { ".bin" }
}

fn mkvextract_tracks(src: &Path, pairs: &[(u32, PathBuf)], mkvextract: &Path) -> Result<()> {
    if pairs.is_empty() { return Ok(()); }
    let mut cmd = Command::new(mkvextract);
    cmd.arg("tracks").arg(src);
    for (id, out) in pairs {
        cmd.arg(format!("{}:{}", id, out.to_string_lossy()));
    }
    run(&mut cmd)
}

fn pick_lang(props: &Option<TrackProps>, fallback: &str) -> String {
    props.as_ref().and_then(|p| p.language.clone()).unwrap_or_else(|| fallback.to_string())
}

pub struct MuxConfig<'a> {
    pub reference: &'a Path,
    pub secondary: Option<&'a Path>,
    pub tertiary: Option<&'a Path>,
    pub output: &'a Path,
    pub mkvmerge: &'a Path,
    pub mkvextract: &'a Path,
    pub prefer_lang: &'a str,
    pub signs_regex: &'a Regex,
    pub out_opts: Option<&'a Path>,
    pub sec_delay_ms: Option<i64>, // raw (analysis or provided)
    pub ter_delay_ms: Option<i64>, // raw (analysis or provided)
}

pub fn mux(cfg: &MuxConfig) -> Result<()> {
    let (temp_dir, _outdir) = ensure_dirs()?;

    // 1) probe inputs
    let ref_json = mkvmerge_probe(cfg.reference, cfg.mkvmerge)?;
    let sec_json = if let Some(s) = cfg.secondary { Some(mkvmerge_probe(s, cfg.mkvmerge)?) } else { None };
    let ter_json = if let Some(t) = cfg.tertiary  { Some(mkvmerge_probe(t, cfg.mkvmerge)?) } else { None };

    // 2) extract required
    // REF video
    let vtrack = ref_json.tracks.iter().find(|t| t.r#type=="video").ok_or_else(|| anyhow!("no video in ref"))?;
    let vext = ext_for_video_codec(&vtrack.codec);
    let ref_video = temp_dir.join(format!("REF_video{}", vext));
    mkvextract_tracks(cfg.reference, &[(vtrack.id, ref_video.clone())], cfg.mkvextract)?;

    // REF audio (all)
    let mut ref_audio: Vec<(PathBuf, String)> = Vec::new();
    for at in ref_json.tracks.iter().filter(|t| t.r#type=="audio") {
        let lang = pick_lang(&at.properties, "und");
        let ext = if at.codec.to_lowercase().contains("truehd") { ".thd" }
                  else if at.codec.to_lowercase().contains("ac-3") || at.codec.to_lowercase().contains("ac3"){ ".ac3" }
                  else if at.codec.to_lowercase().contains("pcm") { ".wav" }
                  else { ".audio" };
        let p = temp_dir.join(format!("REF_a{}_{}{}", at.id, lang, ext));
        mkvextract_tracks(cfg.reference, &[(at.id, p.clone())], cfg.mkvextract)?;
        ref_audio.push((p, lang));
    }

    // SEC: English audio + all subs
    let mut sec_audio_en: Vec<(PathBuf, String)> = Vec::new();
    let mut sec_subs: Vec<(PathBuf, String)> = Vec::new();
    if let (Some(secp), Some(sjson)) = (cfg.secondary, &sec_json) {
        for t in sjson.tracks.iter() {
            if t.r#type=="audio" {
                let lang = pick_lang(&t.properties, "und");
                if lang.to_lowercase().starts_with(&cfg.prefer_lang.to_lowercase()) {
                    let ext = if t.codec.to_lowercase().contains("truehd") { ".thd" }
                              else if t.codec.to_lowercase().contains("ac-3") || t.codec.to_lowercase().contains("ac3"){ ".ac3" }
                              else if t.codec.to_lowercase().contains("pcm") { ".wav" }
                              else { ".audio" };
                    let p = temp_dir.join(format!("SEC_a{}_{}{}", t.id, lang, ext));
                    mkvextract_tracks(secp, &[(t.id, p.clone())], cfg.mkvextract)?;
                    sec_audio_en.push((p, lang));
                }
            } else if t.r#type=="subtitles" {
                let lang = pick_lang(&t.properties, "und");
                let p = temp_dir.join(format!("SEC_s{}_{}.ass", t.id, lang));
                mkvextract_tracks(secp, &[(t.id, p.clone())], cfg.mkvextract)?;
                sec_subs.push((p, lang));
            }
        }
    }

    // TER: subs + attachments
    let mut ter_subs: Vec<(PathBuf, String)> = Vec::new();
    if let (Some(terp), Some(tjson)) = (cfg.tertiary, &ter_json) {
        for t in tjson.tracks.iter().filter(|t| t.r#type=="subtitles") {
            let lang = pick_lang(&t.properties, "und");
            let p = temp_dir.join(format!("TER_s{}_{}.ass", t.id, lang));
            mkvextract_tracks(terp, &[(t.id, p.clone())], cfg.mkvextract)?;
            ter_subs.push((p, lang));
        }
        if !tjson.attachments.is_empty() {
            for (idx, att) in tjson.attachments.iter().enumerate() {
                let id = att.id.unwrap_or(idx as u32);
                let name = att.file_name.clone().unwrap_or_else(|| format!("att{}", id));
                let outp = temp_dir.join(format!("TER_att_{}", name));
                let mut attcmd = Command::new(cfg.mkvextract);
                attcmd.arg("attachments").arg(terp).arg(format!("{}:{}", id, outp.to_string_lossy()));
                run(&mut attcmd)?;
            }
        }
    }

    // Chapters (if any)
    let mut chapters_path: Option<PathBuf> = None;
    if ref_json.chapters.is_some() {
        let p = temp_dir.join("REF_chapters.xml");
        let mut chcmd = Command::new(cfg.mkvextract);
        chcmd.arg("chapters").arg(cfg.reference).arg("-s").arg("-o").arg(&p);
        run(&mut chcmd)?;
        chapters_path = Some(p);
    }

    // 3) Positive-only delay scheme
    let raw_sec = cfg.sec_delay_ms.unwrap_or(0);
    let raw_ter = cfg.ter_delay_ms.unwrap_or(0);
    let mut deltas = vec![0i64, raw_sec, raw_ter];
    let min_neg = *deltas.iter().min().unwrap_or(&0);
    let global = if min_neg < 0 { -min_neg } else { 0 };
    let vid_delay = global;
    let sec_resid = raw_sec + global;
    let ter_resid = raw_ter + global;

    // 4) Assemble @opts.json argv in your deterministic order:
    let mut argv: Vec<String> = Vec::new();
    argv.push("--output".into());
    argv.push(cfg.output.to_string_lossy().to_string());

    if let Some(ch) = &chapters_path {
        argv.push("--chapters".into());
        argv.push(ch.to_string_lossy().to_string());
    }

    // REF video
    argv.extend(["(".into(), ref_video.to_string_lossy().to_string(), ")".into()]);
    argv.extend(["--language".into(), "0:und".into()]);
    argv.extend(["--default-track-flag".into(), "0:no".into()]);
    argv.extend(["--compression".into(), "0:none".into()]);
    if vid_delay != 0 {
        argv.extend(["--sync".into(), format!("0:{}", vid_delay)]);
    }

    // SEC English audio (first default yes), with residual
    let mut audio_default_done = false;
    for (p, lang) in &sec_audio_en {
        argv.extend(["(".into(), p.to_string_lossy().to_string(), ")".into()]);
        argv.extend(["--language".into(), format!("0:{}", lang)]);
        argv.extend(["--compression".into(), "0:none".into()]);
        argv.extend(["--default-track-flag".into(), if !audio_default_done { "0:yes".into() } else { "0:no".into() }]);
        if !audio_default_done { audio_default_done = true; }
        if sec_resid != 0 {
            argv.extend(["--sync".into(), format!("0:{}", sec_resid)]);
        }
    }

    // REF audio (no default; apply global so they follow delayed video)
    for (p, lang) in &ref_audio {
        argv.extend(["(".into(), p.to_string_lossy().to_string(), ")".into()]);
        argv.extend(["--language".into(), format!("0:{}", lang)]);
        argv.extend(["--default-track-flag".into(), "0:no".into()]);
        argv.extend(["--compression".into(), "0:none".into()]);
        if vid_delay != 0 {
            argv.extend(["--sync".into(), format!("0:{}", vid_delay)]);
        }
    }

    // SEC subs (signs default heuristic), apply global
    for (p, lang) in &sec_subs {
        argv.extend(["(".into(), p.to_string_lossy().to_string(), ")".into()]);
        argv.extend(["--language".into(), format!("0:{}", lang)]);
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let is_signs = cfg.signs_regex.is_match(fname);
        argv.extend(["--default-track-flag".into(), if is_signs {"0:yes".into()} else {"0:no".into()} ]);
        argv.extend(["--compression".into(), "0:none".into()]);
        if vid_delay != 0 {
            argv.extend(["--sync".into(), format!("0:{}", vid_delay)]);
        }
    }

    // TER subs (apply ter_resid)
    for (p, lang) in &ter_subs {
        argv.extend(["(".into(), p.to_string_lossy().to_string(), ")".into()]);
        argv.extend(["--language".into(), format!("0:{}", lang)]);
        argv.extend(["--default-track-flag".into(), "0:no".into()]);
        argv.extend(["--compression".into(), "0:none".into()]);
        if ter_resid != 0 {
            argv.extend(["--sync".into(), format!("0:{}", ter_resid)]);
        }
    }

    // TER attachments
    if let Ok(rd) = std::fs::read_dir(&temp_dir) {
        for e in rd {
            if let Ok(ent) = e {
                let p = ent.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("TER_att_") {
                        argv.extend(["--attach-file".into(), p.to_string_lossy().to_string()]);
                    }
                }
            }
        }
    }

    // write opts.json
    let temp_opts = temp_dir.join("opts.json");
    fs::write(&temp_opts, serde_json::to_string_pretty(&argv)?)?;
    if let Some(out) = cfg.out_opts {
        fs::write(out, serde_json::to_string_pretty(&argv)?)?;
        println!("Wrote opts.json -> {}", out.display());
    } else {
        println!("Wrote opts.json -> {}", temp_opts.display());
    }

    println!(
        "Merge Summary: global_shift={} ms, secondary={} ms, tertiary={} ms",
        vid_delay, sec_resid, ter_resid
    );

    // run mkvmerge @opts
    run(&mut Command::new(cfg.mkvmerge).arg(format!("@{}", temp_opts.to_string_lossy())))?;

    // cleanup temp folder
    let _ = fs::remove_file(&temp_opts);
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}
