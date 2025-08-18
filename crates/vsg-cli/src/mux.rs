
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

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

#[derive(Clone)]
struct Probe {
    file: PathBuf,
    meta: MkvmergeJson,
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
    let json = run_out(Command::new(mkvmerge).arg("-J").arg(file))?;
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

#[derive(Default)]
struct Inputs {
    ref_probe: Option<Probe>,
    sec_probe: Option<Probe>,
    ter_probe: Option<Probe>,
}

#[derive(Default)]
struct Extracted {
    ref_video: Option<PathBuf>,
    ref_chapters: Option<PathBuf>,
    ref_audio: Vec<(PathBuf, String)>, // (path, lang)
    sec_audio_en: Vec<(PathBuf, String)>,
    sec_subs: Vec<(PathBuf, String)>,
    ter_subs: Vec<(PathBuf, String)>,
    ter_attachments: Vec<PathBuf>,
}

pub fn mux(
    reference: &Path,
    secondary: Option<&Path>,
    tertiary: Option<&Path>,
    output: &Path,
    sec_delay: Option<i32>,
    ter_delay: Option<i32>,
    mkvmerge_path: &Path,
    mkvextract_path: &Path,
    prefer_lang: &str,
    signs_regex: &Regex,
    out_opts: Option<&Path>,
) -> Result<()> {
    let (temp_dir, _outdir) = ensure_dirs()?;

    // 1) probe
    info!("Probing inputs...");
    let mut inputs = Inputs::default();
    inputs.ref_probe = Some(Probe { file: reference.to_path_buf(), meta: mkvmerge_probe(reference, mkvmerge_path)? });
    if let Some(sec) = secondary {
        inputs.sec_probe = Some(Probe { file: sec.to_path_buf(), meta: mkvmerge_probe(sec, mkvmerge_path)? });
    }
    if let Some(ter) = tertiary {
        inputs.ter_probe = Some(Probe { file: ter.to_path_buf(), meta: mkvmerge_probe(ter, mkvmerge_path)? });
    }

    // 2) extract
    info!("Extracting required streams to temp/ ...");
    let mut ex = Extracted::default();

    // REF video (elementary), chapters, and audio
    if let Some(refp) = &inputs.ref_probe {
        // video
        let vtrack = refp.meta.tracks.iter().find(|t| t.r#type == "video")
            .ok_or_else(|| anyhow!("No video track in reference"))?;
        let vext = ext_for_video_codec(&vtrack.codec);
        let vout = temp_dir.join(format!("REF_video{}", vext));
        mkvextract_tracks(&refp.file, &[(vtrack.id, vout.clone())], mkvextract_path)?;
        ex.ref_video = Some(vout);

        // chapters (if present)
        if refp.meta.chapters.is_some() {
            // Use mkvextract chapters
            let chp = temp_dir.join("REF_chapters.xml");
            run(Command::new(mkvextract_path)
                .arg("chapters").arg(&refp.file)
                .arg("-s")
                .arg("-o").arg(&chp))?;
            ex.ref_chapters = Some(chp);
        }
        // audio: keep all
        for at in refp.meta.tracks.iter().filter(|t| t.r#type == "audio") {
            let lang = pick_lang(&at.properties, "und");
            let ext = if at.codec.to_lowerCase().contains("pcm") { ".wav" } else if at.codec.to_lowercase().contains("ac-3") || at.codec.to_lowercase().contains("ac3") { ".ac3" } else if at.codec.to_lowercase().contains("truehd") { ".thd" } else { ".audio" };
            let outp = temp_dir.join(format!("REF_a{}_{}{}", at.id, lang, ext));
            mkvextract_tracks(&refp.file, &[(at.id, outp.clone())], mkvextract_path)?;
            ex.ref_audio.push((outp, lang));
        }
    }

    // SEC: English audio + all subs
    if let Some(secp) = &inputs.sec_probe {
        for t in secp.meta.tracks.iter() {
            match t.r#type.as_str() {
                "audio" => {
                    let lang = pick_lang(&t.properties, "und");
                    if lang.to_lowercase().starts_with(&prefer_lang.to_lowercase()) {
                        let ext = if t.codec.to_lowercase().contains("ac-3") || t.codec.to_lowercase().contains("ac3") { ".ac3" } else if t.codec.to_lowercase().contains("truehd") { ".thd" } else if t.codec.to_lowercase().contains("pcm") { ".wav" } else { ".audio" };
                        let outp = temp_dir.join(format!("SEC_a{}_{}{}", t.id, lang, ext));
                        mkvextract_tracks(&secp.file, &[(t.id, outp.clone())], mkvextract_path)?;
                        ex.sec_audio_en.push((outp, lang));
                    }
                }
                "subtitles" => {
                    let lang = pick_lang(&t.properties, "und");
                    let outp = temp_dir.join(format!("SEC_s{}_{}.ass", t.id, lang));
                    mkvextract_tracks(&secp.file, &[(t.id, outp.clone())], mkvextract_path)?;
                    ex.sec_subs.push((outp, lang));
                }
                _ => {}
            }
        }
    }

    // TER: subs + attachments
    if let Some(terp) = &inputs.ter_probe {
        for t in terp.meta.tracks.iter().filter(|t| t.r#type == "subtitles") {
            let lang = pick_lang(&t.properties, "und");
            let outp = temp_dir.join(format!("TER_s{}_{}.ass", t.id, lang));
            mkvextract_tracks(&terp.file, &[(t.id, outp.clone())], mkvextract_path)?;
            ex.ter_subs.push((outp, lang));
        }
        // attachments
        for (idx, att) in terp.meta.attachments.iter().enumerate() {
            let fname = att.file_name.clone().unwrap_or_else(|| format!("att{}", idx));
            let outp = temp_dir.join(format!("TER_att_{}", fname));
            run(Command::new(mkvextract_path)
                .arg("attachments").arg(&terp.file)
                .arg(format!("{}:{}", att.id.unwrap_or(idx as u32), outp.to_string_lossy())))?;
            ex.ter_attachments.push(outp);
        }
    }

    // 4) assemble mkvmerge options argv
    let mut argv: Vec<String> = Vec::new();

    // output path
    argv.push("--output".into());
    argv.push(output.to_string_lossy().to_string());

    // chapters
    if let Some(chxml) = &ex.ref_chapters {
        argv.push("--chapters".into());
        argv.push(chxml.to_string_lossy().to_string());
    }

    // REF video (elementary)
    if let Some(v) = &ex.ref_video {
        argv.push("(".into());
        argv.push(v.to_string_lossy().to_string());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push("0:und".into());
        argv.push("--default-track-flag".into());
        argv.push("0:no".into());
        argv.push("--compression".into());
        argv.push("0:none".into());
    }

    // SEC English audio (first default)
    let mut audio_default_done = false;
    for (i, (p, lang)) in ex.sec_audio_en.iter().enumerate() {
        argv.push("(".into());
        argv.push(p.to_string_lossy().to_string());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push(format!("0:{}", lang));
        argv.push("--compression".into());
        argv.push("0:none".into());
        argv.push("--default-track-flag".into());
        if !audio_default_done {
            argv.push("0:yes".into());
            audio_default_done = true;
            if let Some(ms) = sec_delay {
                argv.push("--sync".into());
                argv.push(format!("0:{}", ms));
            }
        } else {
            argv.push("0:no".into());
        }
    }

    // REF audio (not default)
    for (p, lang) in &ex.ref_audio {
        argv.push("(".into());
        argv.push(p.to_string_lossy().to_string());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push(format!("0:{}", lang));
        argv.push("--default-track-flag".into());
        argv.push("0:no".into());
        argv.push("--compression".into());
        argv.push("0:none".into());
    }

    // SEC subs
    for (p, lang) in &ex.sec_subs {
        argv.push("(".into());
        argv.push(p.to_string_lossy().to_string());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push(format!("0:{}", lang));
        argv.push("--default-track-flag".into());
        // signs default heuristic
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let is_signs = signs_regex.is_match(fname);
        argv.push(if is_signs { "0:yes".into() } else { "0:no".into() });
        argv.push("--compression".into());
        argv.push("0:none".into());
    }

    // TER subs
    for (p, lang) in &ex.ter_subs {
        argv.push("(".into());
        argv.push(p.to_string_lossy().to_string());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push(format!("0:{}", lang));
        argv.push("--default-track-flag".into());
        argv.push("0:no".into());
        argv.push("--compression".into());
        argv.push("0:none".into());
        if let Some(ms) = ter_delay {
            argv.push("--sync".into());
            argv.push(format!("0:{}", ms));
        }
    }

    // TER attachments
    for p in &ex.ter_attachments {
        argv.push("--attach-file".into());
        argv.push(p.to_string_lossy().to_string());
    }

    // write opts.json if requested
    if let Some(outp) = out_opts {
        fs::write(outp, serde_json::to_string_pretty(&argv)?)?;
        info!("Wrote opts.json -> {}", outp.to_string_lossy());
    }

    // 5) run mkvmerge @opts
    // Save to temp to always run with @opts
    let temp_opts = temp_dir.join("opts.json");
    fs::write(&temp_opts, serde_json::to_string(&argv)?)?;
    run(Command::new(mkvmerge_path).arg(format!("@{}", temp_opts.to_string_lossy())))?;

    // clean temp after success (keep when RUST_LOG=debug? here we always clean)
    let _ = fs::remove_file(&temp_opts);
    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}
