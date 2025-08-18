
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
struct MkvmergeJson { tracks: Vec<TrackJson>, }
#[derive(Deserialize)]
struct TrackJson {
    #[serde(rename="type")] kind: String,
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
            out.status.code(), stderr);
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
fn probe(mkvmerge: &str, path: &Utf8PathBuf) -> Result<MkvmergeJson> {
    let txt = run_capture(mkvmerge, &["-J", path.as_str()])?;
    Ok(serde_json::from_str(&txt).context("parse mkvmerge -J json")?)
}

pub struct MuxArgs {
    pub mkvmerge: String,
    pub reference: Utf8PathBuf,
    pub secondary: Option<Utf8PathBuf>,
    pub tertiary:  Option<Utf8PathBuf>,
    pub output:    Utf8PathBuf,
    pub sec_delay: i32,
    pub ter_delay: i32,
    pub prefer_lang: Option<String>,
    pub signs_pattern: String,
    pub apply_dialnorm: bool,
    pub out_opts: Option<Utf8PathBuf>,
}

fn write_json_array(path: &Path, tokens: &[String]) -> Result<()> {
    let json = serde_json::to_string_pretty(tokens)?;
    fs::write(path, json)?;
    Ok(())
}

pub fn run_mux(args: MuxArgs) -> Result<()> {
    // Reference must exist and have video
    if !Path::new(args.reference.as_str()).exists() {
        anyhow::bail!("Reference does not exist: {}", args.reference);
    }
    let ref_probe = probe(&args.mkvmerge, &args.reference)?;
    if !ref_probe.tracks.iter().any(|t| t.kind == "video") {
        anyhow::bail!("Reference has no video track: {}", args.reference);
    }

    // Build @opts tokens, explicitly filtering kinds per file.
    let mut t: Vec<String> = Vec::new();
    t.push("--output".into()); t.push(args.output.to_string());

    // Reference: video only (drop audio/subs)
    t.push("(".into()); t.push(args.reference.to_string()); t.push(")".into());
    t.push("--no-audio".into()); t.push("--no-subtitles".into());
    // compression none for ref video track 0
    t.push("--compression".into()); t.push("0:none".into());

    // Secondary: audio only
    if let Some(sec) = &args.secondary {
        if Path::new(sec.as_str()).exists() {
            let sec_probe = probe(&args.mkvmerge, sec).ok();
            t.push("(".into()); t.push(sec.to_string()); t.push(")".into());
            t.push("--no-video".into()); t.push("--no-subtitles".into());
            // set per-track compression and sync for all audio tracks present
            if let Some(p) = sec_probe {
                let mut audio_ix = 0;
                let pref = args.prefer_lang.as_ref().map(|s| s.to_ascii_lowercase());
                // determine order with preferred language first
                let mut auds: Vec<usize> = p.tracks.iter().enumerate()
                    .filter(|(_, tr)| tr.kind=="audio")
                    .map(|(i,_)| i).collect();
                if let Some(lang) = pref {
                    let (mut best, mut rest): (Vec<usize>, Vec<usize>) = auds.into_iter()
                        .partition(|i| p.tracks[*i].properties.as_ref()
                            .and_then(|pr| pr.language.as_ref())
                            .map(|s| s.to_ascii_lowercase()==lang).unwrap_or(false));
                    best.append(&mut rest);
                    auds = best;
                }
                // default-track-flag first audio yes, others no
                for (pos, ix) in auds.iter().enumerate() {
                    t.push("--compression".into()); t.push(format!("{ix}:none"));
                    t.push("--sync".into()); t.push(format!("{ix}:{}", args.sec_delay));
                    t.push("--default-track-flag".into());
                    t.push(format!("{ix}:{}", if pos==0 {"yes"} else {"no"}));
                    audio_ix += 1;
                }
            }
        } else {
            eprintln!("[warn] secondary missing: {}", sec);
        }
    }

    // Tertiary: subtitles only
    if let Some(ter) = &args.tertiary {
        if Path::new(ter.as_str()).exists() {
            let ter_probe = probe(&args.mkvmerge, ter).ok();
            t.push("(".into()); t.push(ter.to_string()); t.push(")".into());
            t.push("--no-video".into()); t.push("--no-audio".into());
            if let Some(p) = ter_probe {
                for (ix, tr) in p.tracks.iter().enumerate().filter(|(_,tr)| tr.kind=="subtitles") {
                    t.push("--compression".into()); t.push(format!("{ix}:none"));
                    t.push("--sync".into()); t.push(format!("{ix}:{}", args.ter_delay));
                    // signs default if name matches
                    if let Some(name) = tr.properties.as_ref().and_then(|pr| pr.track_name.as_ref()) {
                        if Regex::new(&args.signs_pattern).unwrap_or_else(|_| Regex::new("(?i)sign|song").unwrap())
                            .is_match(name) {
                            t.push("--default-track-flag".into()); t.push(format!("{ix}:yes"));
                        }
                    }
                }
            }
        } else {
            eprintln!("[warn] tertiary missing: {}", ter);
        }
    }

    // Write opts and run
    let opts_path = if let Some(p) = &args.out_opts {
        Path::new(p.as_str()).to_path_buf()
    } else {
        let p = std::env::temp_dir().join("vsg_mux_opts.json");
        p
    };
    write_json_array(&opts_path, &t)?;
    println!("Merge Summary: global_shift=0 ms, secondary={} ms, tertiary={} ms",
             args.sec_delay, args.ter_delay);
    println!("Wrote opts.json -> {}", opts_path.display());

    // Run mkvmerge @opts
    let status = Command::new(&args.mkvmerge)
        .arg(format!("@{}", opts_path.display()))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| "spawn mkvmerge failed")?;
    if !status.success() {
        anyhow::bail!("mkvmerge failed with status {:?}", status.code());
    }

    // Cleanup if we created the temp file and user didn't request saving it
    if args.out_opts.is_none() {
        let _ = fs::remove_file(&opts_path);
    }
    Ok(())
}
