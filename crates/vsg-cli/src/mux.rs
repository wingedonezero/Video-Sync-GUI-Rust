// crates/vsg-cli/src/mux.rs
// Extract → Assemble → Merge pipeline, mirroring the Python behavior.
// Notes:
// - Only REF video is kept from container; everything else is extracted to elementary streams.
// - Final order: REF video -> SEC English audio (first default) -> REF audio -> SEC subs -> TER subs -> TER attachments.
// - Delays are applied to SEC/TER audio via --sync 0:<ms> (add-never-subtract convention).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use clap::Args;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Args, Debug)]
pub struct MuxArgs {
    /// Reference input MKV (video source; also audio/subs get extracted from here)
    #[arg(long)]
    pub reference: PathBuf,
    /// Secondary input MKV (eng audio + subs will be extracted)
    #[arg(long)]
    pub secondary: Option<PathBuf>,
    /// Tertiary input MKV (subs + attachments will be extracted)
    #[arg(long)]
    pub tertiary: Option<PathBuf>,
    /// Output MKV file path
    #[arg(long)]
    pub output: PathBuf,
    /// Secondary raw delay in ms (add-never-subtract convention)
    #[arg(long, default_value_t = 0)]
    pub sec_delay: i32,
    /// Tertiary raw delay in ms (add-never-subtract convention)
    #[arg(long, default_value_t = 0)]
    pub ter_delay: i32,
    /// Optional mkvmerge path (default: mkvmerge in PATH)
    #[arg(long, default_value = "mkvmerge")]
    pub mkvmerge: String,
    /// Optional mkvextract path (default: mkvextract in PATH)
    #[arg(long, default_value = "mkvextract")]
    pub mkvextract: String,
    /// Optional path to write the constructed @opts.json; if omitted, written into temp/ and removed with temp.
    #[arg(long)]
    pub out_opts: Option<PathBuf>,
    /// Preferred language for secondary audio (default: eng)
    #[arg(long, default_value = "eng")]
    pub prefer_lang: String,
    /// Regex to detect Signs/Songs subtitle tracks for default flag
    #[arg(long, default_value = "(?i)sign|song")]
    pub signs_pattern: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Track {
    id: i64,
    #[serde(rename = "type")]
    kind: String,
    codec: Option<String>,
    properties: Option<TrackProps>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrackProps {
    language: Option<String>,
    track_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MkvmergeJson {
    tracks: Vec<Track>,
    attachments: Option<Vec<Attachment>>,
}

#[derive(Debug, Clone, Deserialize)]
struct Attachment {
    id: i64,
    file_name: Option<String>,
    mime_type: Option<String>,
}

fn run_and_capture(bin: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(bin).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
    if !out.status.success() {
        let s = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("Process failed: {} {:?}\n{}", bin, args, s));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn probe(mkvmerge: &str, path: &Path) -> Result<MkvmergeJson> {
    let txt = run_and_capture(mkvmerge, &["-J", path.to_str().unwrap()])?;
    let v: MkvmergeJson = serde_json::from_str(&txt).context("Failed to parse mkvmerge -J JSON")?;
    Ok(v)
}

fn ensure_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("create_dir_all {:?}", p))?;
    Ok(())
}

fn ext_for_codec(codec: &str, kind: &str) -> &'static str {
    match kind {
        "subtitles" => {
            if codec.to_lowercase().contains("pgs") { "sup" }
            else if codec.to_lowercase().contains("subrip") { "srt" }
            else { "ass" } // default to ass
        }
        "audio" => {
            let c = codec.to_lowercase();
            if c.contains("truehd") { "thd" }
            else if c.contains("dts") { "dts" }
            else if c.contains("ac-3") || c.contains("e-ac-3") || c.contains("ac3") { "ac3" }
            else if c.contains("aac") { "aac" }
            else if c.contains("pcm") || c.contains("wav") { "wav" }
            else { "mka" }
        }
        _ => "bin"
    }
}

fn mkvextract_tracks(mkvextract: &str, src: &Path, mappings: &[(i64, PathBuf)]) -> Result<()> {
    if mappings.is_empty() { return Ok(()); }
    let mut args: Vec<String> = vec!["tracks".to_string(), src.to_string_lossy().to_string()];
    for (id, out) in mappings {
        args.push(format!("{}:{}", id, out.to_string_lossy()));
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_and_capture(mkvextract, &args_ref)?;
    Ok(())
}

fn mkvextract_attachments(mkvextract: &str, src: &Path, mappings: &[(i64, PathBuf)]) -> Result<()> {
    if mappings.is_empty() { return Ok(()); }
    let mut args: Vec<String> = vec!["attachments".to_string(), src.to_string_lossy().to_string()];
    for (id, out) in mappings {
        args.push(format!("{}:{}", id, out.to_string_lossy()));
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_and_capture(mkvextract, &args_ref)?;
    Ok(())
}

fn mkvextract_chapters(mkvextract: &str, src: &Path, out: &Path) -> Result<()> {
    run_and_capture(mkvextract, &["chapters", src.to_str().unwrap(), out.to_str().unwrap()])?;
    Ok(())
}

fn is_eng(lang_opt: &Option<String>, prefer: &str) -> bool {
    let lang = lang_opt.as_deref().unwrap_or("").to_lowercase();
    let prefer = prefer.to_lowercase();
    lang == "en" || lang == "eng" || lang == prefer
}

pub fn mux(args: &MuxArgs) -> Result<()> {
    // staging dirs
    let temp_dir = PathBuf::from("temp");
    ensure_dir(&temp_dir)?;
    let out_dir = PathBuf::from("output");
    ensure_dir(&out_dir)?;

    // Probe all present inputs
    let ref_info = probe(&args.mkvmerge, &args.reference).context("probe REF")?;
    let sec_info = if let Some(p) = &args.secondary { Some(probe(&args.mkvmerge, p).context("probe SEC")?) } else { None };
    let ter_info = if let Some(p) = &args.tertiary { Some(probe(&args.mkvmerge, p).context("probe TER")?) } else { None };

    // Build extraction lists
    let mut ref_audio_maps: Vec<(i64, PathBuf)> = vec![];
    let mut ref_sub_maps: Vec<(i64, PathBuf)> = vec![];
    let mut sec_eng_audio_maps: Vec<(i64, PathBuf)> = vec![];
    let mut sec_sub_maps: Vec<(i64, PathBuf)> = vec![];
    let mut ter_sub_maps: Vec<(i64, PathBuf)> = vec![];
    let mut ter_att_maps: Vec<(i64, PathBuf)> = vec![];

    // REF audio+subs
    for t in &ref_info.tracks {
        match t.kind.as_str() {
            "audio" => {
                let ext = ext_for_codec(t.codec.as_deref().unwrap_or(""), "audio");
                let out = temp_dir.join(format!("REF_a{}_{}.{}", t.id, t.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or("und".to_string()), ext));
                ref_audio_maps.push((t.id, out));
            }
            "subtitles" => {
                let ext = ext_for_codec(t.codec.as_deref().unwrap_or(""), "subtitles");
                let out = temp_dir.join(format!("REF_s{}_{}.{}", t.id, t.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or("und".to_string()), ext));
                ref_sub_maps.push((t.id, out));
            }
            _ => {}
        }
    }

    // SEC eng audio + all subs
    if let (Some(sec_path), Some(s)) = (&args.secondary, &sec_info) {
        for t in &s.tracks {
            match t.kind.as_str() {
                "audio" => {
                    if is_eng(&t.properties.as_ref().and_then(|p| p.language.clone()), &args.prefer_lang) {
                        let ext = ext_for_codec(t.codec.as_deref().unwrap_or(""), "audio");
                        let out = temp_dir.join(format!("SEC_a{}_{}.{}", t.id, t.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or("eng".to_string()), ext));
                        sec_eng_audio_maps.push((t.id, out));
                    }
                }
                "subtitles" => {
                    let ext = ext_for_codec(t.codec.as_deref().unwrap_or(""), "subtitles");
                    let out = temp_dir.join(format!("SEC_s{}_{}.{}", t.id, t.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or("und".to_string()), ext));
                    sec_sub_maps.push((t.id, out));
                }
                _ => {}
            }
        }
        // extract sec tracks
        mkvextract_tracks(&args.mkvextract, sec_path, &sec_eng_audio_maps)?;
        mkvextract_tracks(&args.mkvextract, sec_path, &sec_sub_maps)?;
    }

    // TER subs + attachments
    if let (Some(ter_path), Some(t)) = (&args.tertiary, &ter_info) {
        for tr in &t.tracks {
            if tr.kind == "subtitles" {
                let ext = ext_for_codec(tr.codec.as_deref().unwrap_or(""), "subtitles");
                let out = temp_dir.join(format!("TER_s{}_{}.{}", tr.id, tr.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or("und".to_string()), ext));
                ter_sub_maps.push((tr.id, out));
            }
        }
        if let Some(atts) = &t.attachments {
            for att in atts {
                let name = att.file_name.clone().unwrap_or(format!("att{}.bin", att.id));
                let out = temp_dir.join(format!("TER_att{}_{}", att.id, name));
                ter_att_maps.push((att.id, out));
            }
        }
        // extract TER
        mkvextract_tracks(&args.mkvextract, ter_path, &ter_sub_maps)?;
        mkvextract_attachments(&args.mkvextract, ter_path, &ter_att_maps)?;
    }

    // REF: extract chapters + audio+subs (not video)
    let ref_chapters = temp_dir.join("REF_chapters.xml");
    mkvextract_chapters(&args.mkvextract, &args.reference, &ref_chapters).ok(); // chapters may be absent
    mkvextract_tracks(&args.mkvextract, &args.reference, &ref_audio_maps)?;
    mkvextract_tracks(&args.mkvextract, &args.reference, &ref_sub_maps)?;

    // Build @opts.json (argv array)
    let mut argv: Vec<String> = Vec::new();
    argv.push("--output".into());
    argv.push(args.output.to_string_lossy().into_owned());

    // chapters if present
    if ref_chapters.exists() {
        argv.push("--chapters".into());
        argv.push(ref_chapters.to_string_lossy().into_owned());
    }

    // ( REF video only ) by suppressing audio/subs on the container
    argv.push("(".into());
    argv.push(args.reference.to_string_lossy().into_owned());
    argv.push(")".into());
    argv.push("--no-audio".into());
    argv.push("--no-subtitles".into());

    // helper to append a single-track elementary stream with flags
    let mut add_track = |path: &Path, lang: &str, default_yes: bool, sync_ms: Option<i32>| {
        argv.push("(".into());
        argv.push(path.to_string_lossy().into_owned());
        argv.push(")".into());
        argv.push("--language".into());
        argv.push(format!("0:{}", lang));
        argv.push("--compression".into());
        argv.push("0:none".into());
        if let Some(ms) = sync_ms {
            if ms != 0 {
                argv.push("--sync".into());
                argv.push(format!("0:{}", ms));
            }
        }
        if default_yes {
            argv.push("--default-track-flag".into());
            argv.push("0:yes".into());
        } else {
            argv.push("--default-track-flag".into());
            argv.push("0:no".into());
        }
    };

    // 1) SEC English audio (first default=yes) with sec delay
    let mut first_audio_done = false;
    for (_id, path) in &sec_eng_audio_maps {
        let lang = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").split('_').nth(2).unwrap_or("eng");
        add_track(path, lang, !first_audio_done, Some(args.sec_delay));
        if !first_audio_done { first_audio_done = true; }
    }

    // 2) REF audio (no defaults, no delay)
    for (_id, path) in &ref_audio_maps {
        let lang = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").split('_').nth(2).unwrap_or("und");
        add_track(path, lang, false, None);
    }

    // Decide signs pattern regex
    let signs_re = Regex::new(&args.signs_pattern).unwrap_or(Regex::new("(?i)sign|song").unwrap());

    // 3) SEC subs (default to yes if filename matches signs)
    for (_id, path) in &sec_sub_maps {
        let lang = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").split('_').nth(2).unwrap_or("und");
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let is_signs = signs_re.is_match(name);
        add_track(path, lang, is_signs, None);
    }

    // 4) TER subs (similar default detection)
    for (_id, path) in &ter_sub_maps {
        let lang = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").split('_').nth(2).unwrap_or("und");
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let is_signs = signs_re.is_match(name);
        add_track(path, lang, is_signs, Some(args.ter_delay));
    }

    // 5) TER attachments
    for (_id, path) in &ter_att_maps {
        argv.push("--attach-file".into());
        argv.push(path.to_string_lossy().into_owned());
    }

    // Write opts.json
    let opts_path = if let Some(p) = &args.out_opts { p.clone() } else { temp_dir.join("opts.json") };
    let json_arr: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    let json_value = Value::Array(json_arr.iter().map(|s| Value::String((*s).to_string())).collect());
    fs::write(&opts_path, serde_json::to_string_pretty(&json_value)?).context("write opts.json")?;

    println!("Merge Summary: global_shift=0 ms, secondary={} ms, tertiary={} ms", args.sec_delay, args.ter_delay);
    println!("Wrote opts.json -> {}", opts_path.to_string_lossy());

    // Run mkvmerge @opts.json
    let status = Command::new(&args.mkvmerge).args(&["@".to_string() + opts_path.to_str().unwrap()]).status()?;
    if !status.success() {
        eprintln!("mkvmerge failed with status {:?}", status.code());
        return Err(anyhow!("mkvmerge failed"));
    }

    // Cleanup temp unless user explicitly asked to keep opts somewhere else in temp/
    if args.out_opts.is_none() {
        fs::remove_dir_all(&temp_dir).ok();
    }

    Ok(())
}
