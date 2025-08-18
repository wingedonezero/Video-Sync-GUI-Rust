
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use clap::Parser;

#[derive(Parser, Debug)]
pub struct MuxCmd {
    /// Reference (video kept; audio also extracted)
    #[arg(long)]
    pub reference: PathBuf,
    /// Secondary (extract ENG audio + all subs)
    #[arg(long)]
    pub secondary: Option<PathBuf>,
    /// Tertiary (extract subs + attachments)
    #[arg(long)]
    pub tertiary: Option<PathBuf>,
    /// Output MKV
    #[arg(long)]
    pub output: PathBuf,
    /// Secondary raw delay (ms)
    #[arg(long, default_value_t = 0)]
    pub sec_delay: i64,
    /// Tertiary raw delay (ms)
    #[arg(long, default_value_t = 0)]
    pub ter_delay: i64,
    /// mkvmerge path
    #[arg(long, default_value = "mkvmerge")]
    pub mkvmerge: String,
    /// mkvextract path
    #[arg(long, default_value = "mkvextract")]
    pub mkvextract: String,
    /// Keep option file here (if provided, staging not removed)
    #[arg(long)]
    pub out_opts: Option<PathBuf>,
    /// Prefer language for SEC audio (eng by default)
    #[arg(long, default_value = "eng")]
    pub prefer_lang: String,
    /// Regex to detect Signs/Songs (for default sub)
    #[arg(long, default_value = "(?i)sign|song")]
    pub signs_pattern: String,
    /// Optional staging root; defaults to <exe_dir>/temp
    #[arg(long)]
    pub staging_root: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Track {
    id: u32,
    #[serde(rename = "type")]
    kind: String,
    properties: TrackProps,
    codec: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrackProps {
    language: Option<String>,
    track_name: Option<String>,
}

fn run(cmd: &mut Command) -> anyhow::Result<String> {
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!("Process failed: {:?} (code {:?})", cmd, out.status.code());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn probe_tracks(mkvmerge: &str, path: &Path) -> anyhow::Result<Vec<Track>> {
    let txt = run(Command::new(mkvmerge).arg("-J").arg(path))?;
    let v: Value = serde_json::from_str(&txt)?;
    let arr = v.get("tracks").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for t in arr {
        let tr: Track = serde_json::from_value(t)?;
        out.push(tr);
    }
    Ok(out)
}

fn ext_for(kind: &str, codec: &Option<String>) -> &'static str {
    match kind {
        "audio" => {
            if let Some(c) = codec {
                let lc = c.to_lowercase();
                if lc.contains("truehd") { return "thd"; }
                if lc.contains("ac-3") || lc.contains("ac3") { return "ac3"; }
                if lc.contains("aac") { return "aac"; }
                if lc.contains("pcm") { return "wav"; }
                if lc.contains("dts") { return "dts"; }
            }
            "mka"
        }
        "subtitles" => {
            if let Some(c) = codec {
                let lc = c.to_lowercase();
                if lc.contains("pgs") { return "sup"; }
                if lc.contains("ass") { return "ass"; }
                if lc.contains("srt") { return "srt"; }
            }
            "sub"
        }
        _ => "bin"
    }
}

fn extract_tracks(mkvextract: &str, src: &Path, tracks: &[(u32, &str, &Option<String>)], out_dir: &Path, tag: &str) -> anyhow::Result<Vec<PathBuf>> {
    if tracks.is_empty() { return Ok(vec![]); }
    fs::create_dir_all(out_dir)?;
    // Build mkvextract command: mkvextract tracks input.mkv id:file id:file ...
    let mut args = Vec::new();
    args.push("tracks".to_string());
    args.push(src.to_string_lossy().to_string());
    let mut outs = Vec::new();
    for (tid, kind, codec) in tracks {
        let ext = ext_for(kind, codec);
        let file = out_dir.join(format!("{}_{}_t{:02}.{}", tag, &kind[..3].to_uppercase(), tid, ext));
        args.push(format!("{}:{}", tid, file.to_string_lossy()));
        outs.push(file);
    }
    let status = Command::new(mkvextract).args(args).status()?;
    if !status.success() {
        anyhow::bail!("mkvextract failed on {}", src.display());
    }
    Ok(outs)
}

fn extract_attachments(mkvextract: &str, src: &Path, ids: &[u32], out_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if ids.is_empty() { return Ok(vec![]); }
    fs::create_dir_all(out_dir)?;
    // mkvextract attachments input.mkv id:name ...
    // We don't know names—mkvextract will use original names if omitted, but require id:name.
    let mut files = Vec::new();
    for id in ids {
        let name = format!("TER_attach_{:02}", id);
        let out = out_dir.join(&name);
        let status = Command::new(mkvextract)
            .arg("attachments").arg(src)
            .arg(format!("{}:{}", id, out.to_string_lossy()))
            .status()?;
        if !status.success() {
            anyhow::bail!("mkvextract attachments failed on {}", src.display());
        }
        files.push(out);
    }
    Ok(files)
}

pub fn run_mux(cmd: MuxCmd) -> anyhow::Result<()> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|x| x.to_path_buf())).unwrap_or_else(|| PathBuf::from("."));
    let staging_root = cmd.staging_root.unwrap_or_else(|| exe_dir.join("temp"));
    fs::create_dir_all(&staging_root)?;
    let keep_staging = cmd.out_opts.is_some();

    let ref_tracks = probe_tracks(&cmd.mkvmerge, &cmd.reference)?;
    let sec_tracks = if let Some(sec) = &cmd.secondary {
        probe_tracks(&cmd.mkvmerge, sec)?
    } else { vec![] };
    let ter_tracks = if let Some(ter) = &cmd.tertiary {
        probe_tracks(&cmd.mkvmerge, ter)?
    } else { vec![] };

    // Identify
    let ref_vid: Vec<_> = ref_tracks.iter().filter(|t| t.kind=="video").collect();
    let ref_aud: Vec<_> = ref_tracks.iter().filter(|t| t.kind=="audio").collect();
    let ref_sub: Vec<_> = ref_tracks.iter().filter(|t| t.kind=="subtitles").collect();

    let pref = cmd.prefer_lang.to_lowercase();
    let sec_aud: Vec<_> = sec_tracks.iter().filter(|t| t.kind=="audio" && t.properties.language.as_deref().map(|x| x.to_lowercase()).map(|x| x=="eng" || x=="en" || x==pref).unwrap_or(false)).collect();
    let sec_sub: Vec<_> = sec_tracks.iter().filter(|t| t.kind=="subtitles").collect();

    let ter_sub: Vec<_> = ter_tracks.iter().filter(|t| t.kind=="subtitles").collect();
    let ter_attach_ids: Vec<u32> = {
        // mkvmerge -J has "attachments" separate
        let txt = run(Command::new(&cmd.mkvmerge).arg("-J").arg(cmd.tertiary.as_ref().unwrap_or(&cmd.reference)))?;
        let v: Value = serde_json::from_str(&txt)?;
        v.get("attachments")
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.get("id").and_then(|i| i.as_u64()).map(|u| u as u32)).collect())
            .unwrap_or_else(|| vec![])
    };

    // Extract chapters from REF
    let chapters_xml = staging_root.join("REF_chapters.xml");
    let ch_status = Command::new(&cmd.mkvextract)
        .arg("chapters").arg(&cmd.reference)
        .arg("-s").arg(chapters_xml.to_string_lossy().to_string())
        .status()?;
    if !ch_status.success() {
        // continue without chapters
    }

    // Extract REF audio + subs so we can order deterministically
    let ref_aud_files = extract_tracks(&cmd.mkvextract, &cmd.reference,
        &ref_aud.iter().map(|t| (t.id, t.kind.as_str(), &t.codec)).collect::<Vec<_>>(),
        &staging_root, "REF")?;
    let ref_sub_files = extract_tracks(&cmd.mkvextract, &cmd.reference,
        &ref_sub.iter().map(|t| (t.id, t.kind.as_str(), &t.codec)).collect::<Vec<_>>(),
        &staging_root, "REF")?;

    // Extract SEC ENG audio + all subs
    let sec_aud_files = if let Some(sec) = &cmd.secondary {
        extract_tracks(&cmd.mkvextract, sec,
            &sec_aud.iter().map(|t| (t.id, t.kind.as_str(), &t.codec)).collect::<Vec<_>>(),
            &staging_root, "SEC")?
    } else { vec![] };
    let sec_sub_files = if let Some(sec) = &cmd.secondary {
        extract_tracks(&cmd.mkvextract, sec,
            &sec_sub.iter().map(|t| (t.id, t.kind.as_str(), &t.codec)).collect::<Vec<_>>(),
            &staging_root, "SEC")?
    } else { vec![] };

    // Extract TER subs & attachments
    let ter_sub_files = if let Some(ter) = &cmd.tertiary {
        extract_tracks(&cmd.mkvextract, ter,
            &ter_sub.iter().map(|t| (t.id, t.kind.as_str(), &t.codec)).collect::<Vec<_>>(),
            &staging_root, "TER")?
    } else { vec![] };
    let ter_attach_files = if let Some(ter) = &cmd.tertiary {
        extract_attachments(&cmd.mkvextract, ter, &ter_attach_ids, &staging_root)?
    } else { vec![] };

    // Build @opts argv
    let mut argv: Vec<String> = Vec::new();
    argv.push("--output".into());
    argv.push(cmd.output.to_string_lossy().to_string());

    if chapters_xml.exists() {
        argv.push("--chapters".into());
        argv.push(chapters_xml.to_string_lossy().to_string());
    }

    // 1) REF video only (suppress audio/subs from container)
    argv.push("--no-audio".into());
    argv.push("--no-subtitles".into());
    argv.push("(".into());
    argv.push(cmd.reference.to_string_lossy().to_string());
    argv.push(")".into());

    // helper to add single-track file with flags
    let mut track_order: Vec<(usize, usize)> = Vec::new();
    let mut file_index = 1usize; // ref video container is 0

    let mut add_track = |file: &Path, lang: Option<&str>, default_yes: bool, sync_ms: i64| {
        argv.push("--compression".into()); argv.push("0:none".into());
        if let Some(l) = lang {
            argv.push("--language".into()); argv.push(format!("0:{}", l));
        }
        argv.push("--default-track-flag".into()); argv.push(format!("0:{}", if default_yes {"yes"} else {"no"}));
        if sync_ms != 0 {
            argv.push("--sync".into()); argv.push(format!("0:{}", sync_ms));
        }
        argv.push("(".into()); argv.push(file.to_string_lossy().to_string()); argv.push(")".into());
        track_order.push((file_index, 0));
        file_index += 1;
    };

    // 2) SEC English audio (first default)
    for (i, f) in sec_aud_files.iter().enumerate() {
        let def = i==0;
        add_track(f, Some("eng"), def, cmd.sec_delay);
    }

    // 3) REF audio
    for f in &ref_aud_files {
        add_track(f, None, false, 0);
    }

    // 4) SEC subs
    for f in &sec_sub_files {
        add_track(f, None, false, 0);
    }

    // 5) TER subs (apply ter delay; basic signs default detection by file name)
    for f in &ter_sub_files {
        let fname = f.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let def = regex::Regex::new(&cmd.signs_pattern).ok().map(|re| re.is_match(fname)).unwrap_or(false);
        add_track(f, None, def, cmd.ter_delay);
    }

    // 6) TER attachments
    for a in &ter_attach_files {
        argv.push("--attach-file".into());
        argv.push(a.to_string_lossy().to_string());
    }

    // 7) explicit track-order
    if !track_order.is_empty() {
        argv.push("--track-order".into());
        let order = track_order.iter().map(|(fi,ti)| format!("{}:{}", fi, ti)).collect::<Vec<_>>().join(",");
        argv.push(order);
    }

    // Write opts.json
    let opts_path = cmd.out_opts.clone().unwrap_or_else(|| staging_root.join("opts.json"));
    let pretty = serde_json::to_string_pretty(&argv)?;
    fs::write(&opts_path, pretty.as_bytes())?;
    println!("Wrote opts.json -> {}", opts_path.display());

    // Run mkvmerge @opts
    let status = Command::new(&cmd.mkvmerge).arg(format!("@{}", opts_path.to_string_lossy())).status()?;
    if !status.success() {
        anyhow::bail!("mkvmerge failed building output");
    }

    // cleanup
    if !keep_staging {
        let _ = fs::remove_dir_all(&staging_root);
    }

    Ok(())
}
