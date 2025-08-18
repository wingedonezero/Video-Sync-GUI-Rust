
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use vsg_core::{Delays, TrackSel};
use vsg_core::{write_opts_and_maybe_run, build_mkvmerge_tokens};

#[derive(Deserialize)]
struct MkvmergeJson { tracks: Vec<TrackJson>, }

#[derive(Deserialize)]
struct TrackJson {
    #[serde(rename="type")]
    kind: String,
    codec_id: Option<String>,
    properties: Option<PropsJson>,
}

#[derive(Deserialize)]
struct PropsJson { language: Option<String>, track_name: Option<String>, }

fn run_capture(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output()
        .with_context(|| format!("failed to run {}", cmd))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("Process failed: {:?} (code {:?})\nstderr:\n{}", 
            std::iter::once(cmd).chain(args.iter().copied()).collect::<Vec<_>>(),
            out.status.code(),
            stderr);
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn probe_tracks(mkvmerge: &str, path: &Utf8PathBuf) -> Result<MkvmergeJson> {
    let txt = run_capture(mkvmerge, &["-J", path.as_str()])?;
    let val: MkvmergeJson = serde_json::from_str(&txt).context("parse mkvmerge -J json")?;
    Ok(val)
}

pub struct MuxArgs {
    pub mkvmerge: String,
    pub reference: Utf8PathBuf,
    pub secondary: Option<Utf8PathBuf>,
    pub tertiary:  Option<Utf8PathBuf>,
    pub output:    Utf8PathBuf,
    pub sec_delay: i32,
    pub ter_delay: i32,
    pub chapters_xml: Option<Utf8PathBuf>,
    pub prefer_lang: Option<String>,
    pub signs_pattern: String,
    pub apply_dialnorm: bool,
    pub out_opts: Option<Utf8PathBuf>,
}

pub fn run_mux(args: MuxArgs) -> Result<()> {
    if !Path::new(args.reference.as_str()).exists() {
        anyhow::bail!("Reference file does not exist: {}", args.reference);
    }
    let ref_info = probe_tracks(&args.mkvmerge, &args.reference)?;

    let sec_info = if let Some(s) = &args.secondary {
        if Path::new(s.as_str()).exists() {
            probe_tracks(&args.mkvmerge, s).ok()
        } else { eprintln!("[warn] secondary file not found: {}", s); None }
    } else { None };

    let ter_info = if let Some(t) = &args.tertiary {
        if Path::new(t.as_str()).exists() {
            probe_tracks(&args.mkvmerge, t).ok()
        } else { eprintln!("[warn] tertiary file not found: {}", t); None }
    } else { None };

    let mut plan: Vec<TrackSel> = Vec::new();
    if let Some(v) = ref_info.tracks.iter().find(|t| t.kind == "video") {
        plan.push(TrackSel {
            path: args.reference.to_string(),
            kind: "video",
            lang: v.properties.as_ref().and_then(|p| p.language.clone()),
            name: v.properties.as_ref().and_then(|p| p.track_name.clone()),
            from_group: "ref",
            codec_id: v.codec_id.clone(),
        });
    } else {
        anyhow::bail!("No video track found in reference: {}", args.reference);
    }

    if let (Some(sec), Some(sec_path)) = (sec_info.as_ref(), args.secondary.as_ref()) {
        let mut auds: Vec<&TrackJson> = sec.tracks.iter().filter(|t| t.kind == "audio").collect();
        if let Some(lang) = args.prefer_lang.as_ref() {
            let l = lang.to_ascii_lowercase();
            let lang_auds: Vec<&TrackJson> = auds.iter().copied()
                .filter(|t| t.properties.as_ref().and_then(|p| p.language.as_ref())
                    .map(|s| s.to_ascii_lowercase()==l).unwrap_or(false)).collect();
            if !lang_auds.is_empty() { auds = lang_auds; }
        }
        for a in auds {
            plan.push(TrackSel {
                path: sec_path.to_string(),
                kind: "audio",
                lang: a.properties.as_ref().and_then(|p| p.language.clone()),
                name: a.properties.as_ref().and_then(|p| p.track_name.clone()),
                from_group: "sec",
                codec_id: a.codec_id.clone(),
            });
        }
    }

    if let (Some(ter), Some(ter_path)) = (ter_info.as_ref(), args.tertiary.as_ref()) {
        for s in ter.tracks.iter().filter(|t| t.kind == "subtitles") {
            plan.push(TrackSel {
                path: ter_path.to_string(),
                kind: "subtitles",
                lang: s.properties.as_ref().and_then(|p| p.language.clone()),
                name: s.properties.as_ref().and_then(|p| p.track_name.clone()),
                from_group: "ter",
                codec_id: s.codec_id.clone(),
            });
        }
    }

    let delays = Delays { global_shift_ms: 0, secondary_ms: args.sec_delay, tertiary_ms: args.ter_delay };
    let signs_re = Regex::new(&args.signs_pattern).unwrap_or_else(|_| Regex::new("(?i)sign|song").unwrap());
    let tokens = build_mkvmerge_tokens(
        Path::new(args.output.as_str()),
        args.chapters_xml.as_ref().map(|p| Path::new(p.as_str())),
        &plan,
        &delays,
        Some(&signs_re),
        true,
        args.apply_dialnorm,
    );

    write_opts_and_maybe_run(&tokens,
        args.out_opts.as_ref().map(|p| Path::new(p.as_str())),
        Some(Path::new(&args.mkvmerge)))?;
    Ok(())
}
