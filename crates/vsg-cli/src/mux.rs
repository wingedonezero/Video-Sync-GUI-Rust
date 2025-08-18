
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Deserialize, Debug)]
struct MkvmergeJson { tracks: Vec<TrackJson>, attachments: Option<Vec<AttachmentJson>> }
#[derive(Deserialize, Debug)]
struct TrackJson {
    id: u32,
    #[serde(rename="type")] kind: String,
    codec_id: Option<String>,
    properties: Option<PropsJson>,
}
#[derive(Deserialize, Debug)]
struct PropsJson { language: Option<String>, track_name: Option<String> }
#[derive(Deserialize, Debug)]
struct AttachmentJson { id: u32, file_name: Option<String>, mime_type: Option<String> }

fn run_out(cmd: &str, args: &[&str]) -> Result<(i32, String, String)> {
    let out = Command::new(cmd).args(args).output()
        .with_context(|| format!("spawn {}", cmd))?;
    let code = out.status.code().unwrap_or(-1);
    Ok((code, String::from_utf8_lossy(&out.stdout).into_owned(),
              String::from_utf8_lossy(&out.stderr).into_owned()))
}
fn run_ok(cmd: &str, args: &[&str]) -> Result<String> {
    let (code, out, err) = run_out(cmd, args)?;
    if code != 0 {
        anyhow::bail!("Process failed: {} {:?} (code {})\nstderr:\n{}", cmd, args, code, err);
    }
    Ok(out)
}
fn probe(mkvmerge: &str, path: &Utf8PathBuf) -> Result<MkvmergeJson> {
    let txt = run_ok(mkvmerge, &["-J", path.as_str()])?;
    Ok(serde_json::from_str(&txt).context("parse mkvmerge -J json")?)
}

pub struct MuxArgs {
    pub mkvmerge: String,
    pub mkvextract: Option<String>,
    pub staging_root: Utf8PathBuf,   // temp_root/session dir
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

#[derive(Clone)]
struct SelFile {
    path: PathBuf,
    kind: &'static str,   // "video" | "audio" | "subtitles" | "attachments"
    src_group: &'static str, // "REF" | "SEC" | "TER"
    input_track_id: u32,  // original mkvmerge track id
}

fn ext_for_codec(kind: &str, codec_id: Option<&str>, fallback_name: Option<&str>) -> &'static str {
    match kind {
        "audio" => {
            if let Some(c) = codec_id {
                let lc = c.to_ascii_lowercase();
                if lc.contains("truehd") { "thd" }
                else if lc.contains("ac-3") || lc.contains("ac3") { "ac3" }
                else if lc.contains("aac") { "aac" }
                else if lc.contains("pcm") || lc.contains("wav") { "wav" }
                else { "audio" }
            } else { "audio" }
        }
        "subtitles" => {
            if let Some(name) = fallback_name {
                let ln = name.to_ascii_lowercase();
                if ln.ends_with(".ass") { "ass" }
                else if ln.ends_with(".sup") { "sup" }
                else { "sub" }
            } else { "sub" }
        }
        _ => "bin",
    }
}

fn mkvextract_tracks(mkvextract: &str, src: &Utf8PathBuf, out_pairs: &[(u32, PathBuf)]) -> Result<()> {
    if out_pairs.is_empty() { return Ok(()); }
    // Build: mkvextract tracks "src.mkv" id1:"path1" id2:"path2"
    let mut args: Vec<String> = vec!["tracks".into(), src.to_string()];
    for (id, p) in out_pairs.iter() {
        args.push(format!("{}:{}", id, p.display()));
    }
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ok(mkvextract, &str_args)?;
    Ok(())
}

pub fn run_mux(args: MuxArgs) -> Result<()> {
    // Prepare staging dir
    let stage_dir = Path::new(args.staging_root.as_str());
    fs::create_dir_all(stage_dir)?;

    let signs_re = Regex::new(&args.signs_pattern).unwrap_or_else(|_| Regex::new("(?i)sign|song").unwrap());

    // Probe
    let ref_probe = probe(&args.mkvmerge, &args.reference)?;
    let sec_probe = if let Some(sec) = &args.secondary { Some(probe(&args.mkvmerge, sec).ok()) } else { None };
    let ter_probe = if let Some(ter) = &args.tertiary  { Some(probe(&args.mkvmerge, ter).ok()) } else { None };

    // 1) Build extraction plan (files we will create in staging)
    let mut staged_files: Vec<SelFile> = Vec::new();

    // REF video (keep as container; select video later)
    if ref_probe.tracks.iter().any(|t| t.kind=="video") {
        staged_files.push(SelFile{ path: PathBuf::from(args.reference.as_str()), kind: "video", src_group: "REF", input_track_id: 0 });
    } else {
        anyhow::bail!("Reference has no video: {}", args.reference);
    }
    // SEC audios: extract all audio tracks (preferred language first will be handled by track flags/order)
    if let (Some(Some(sp)), Some(sec)) = (sec_probe.as_ref(), args.secondary.as_ref()) {
        if let Some(mkvextract) = &args.mkvextract {
            let mut pairs: Vec<(u32, PathBuf)> = Vec::new();
            for tr in sp.tracks.iter().filter(|t| t.kind=="audio") {
                let ext = ext_for_codec("audio", tr.codec_id.as_deref(), None);
                let name = format!("SEC_{:02}_t{:02}.{}", tr.id, tr.id, ext);
                pairs.push((tr.id, stage_dir.join(name)));
            }
            mkvextract_tracks(mkvextract, sec, &pairs)?;
            for (id, out) in pairs {
                staged_files.push(SelFile{ path: out, kind: "audio", src_group:"SEC", input_track_id:id });
            }
        } else {
            // No mkvextract set: include container with --no-video/--no-subtitles
            staged_files.push(SelFile{ path: PathBuf::from(sec.as_str()), kind:"audio", src_group:"SEC", input_track_id:0 });
        }
    }
    // TER subtitles + attachments
    if let (Some(Some(tp)), Some(ter)) = (ter_probe.as_ref(), args.tertiary.as_ref()) {
        if let Some(mkvextract) = &args.mkvextract {
            // Subtitles
            let mut sub_pairs: Vec<(u32, PathBuf)> = Vec::new();
            for tr in tp.tracks.iter().filter(|t| t.kind=="subtitles") {
                let ext = ext_for_codec("subtitles", None, None);
                let name = format!("TER_{:02}_t{:02}.{}", tr.id, tr.id, ext);
                sub_pairs.push((tr.id, stage_dir.join(name)));
            }
            mkvextract_tracks(mkvextract, ter, &sub_pairs)?;
            for (id, out) in sub_pairs {
                staged_files.push(SelFile{ path: out, kind: "subtitles", src_group:"TER", input_track_id:id });
            }
            // Attachments
            if let Some(atts) = &tp.attachments {
                for att in atts {
                    // mkvextract attachments src mkvextract will extract by id; we preserve file name
                    let name = att.file_name.clone().unwrap_or_else(|| format!("TER_attach_{}.bin", att.id));
                    let out = stage_dir.join(&name);
                    let args2 = vec!["attachments", ter.as_str(), &format!("{}:{}", att.id, out.display())];
                    run_ok(mkvextract, &args2.iter().map(|s| s.as_ref()).collect::<Vec<&str>>())?;
                    staged_files.push(SelFile{ path: out, kind:"attachments", src_group:"TER", input_track_id: att.id });
                }
            }
        } else {
            staged_files.push(SelFile{ path: PathBuf::from(ter.as_str()), kind:"subtitles", src_group:"TER", input_track_id:0 });
        }
    }

    // 2) Build tokens in final order:
    // [REF video] -> [SEC audio...] -> [REF audio...] -> [TER subs...] -> [SEC subs if any later] -> [REF other subs] -> [TER attachments]
    let mut tokens: Vec<String> = Vec::new();
    tokens.push("--output".into());
    tokens.push(args.output.to_string());

    // Helper to add a single-source container with kind filters
    let mut file_index: u32 = 0;
    let mut track_order: Vec<String> = Vec::new();
    let mut push_container = |path: &Path, no_video: bool, no_audio: bool, no_subs: bool| {
        tokens.push("(".into()); tokens.push(path.display().to_string()); tokens.push(")".into());
        if no_video { tokens.push("--no-video".into()); }
        if no_audio { tokens.push("--no-audio".into()); }
        if no_subs  { tokens.push("--no-subtitles".into()); }
        // compression none for the first track (0) in this container
        tokens.push("--compression".into()); tokens.push("0:none".into());
        file_index += 1;
    };

    // REF video from original container
    push_container(Path::new(args.reference.as_str()), false, true, true);
    track_order.push(format!("{}:0", file_index-1)); // video track 0

    // SEC audio files
    for s in staged_files.iter().filter(|s| s.src_group=="SEC" && s.kind=="audio") {
        // Each extracted file is single-track; add and set audio flags
        push_container(&s.path, true, false, true);
        let ix = file_index-1;
        tokens.push("--sync".into()); tokens.push(format!("0:{}", args.sec_delay));
        tokens.push("--default-track-flag".into()); tokens.push(if track_order.iter().all(|to| !to.ends_with(":0")) { "0:yes".into() } else { "0:no".into() });
        track_order.push(format!("{}:0", ix));
    }

    // REF audio from reference container (if any)
    let ref_audios: Vec<&TrackJson> = ref_probe.tracks.iter().filter(|t| t.kind=="audio").collect();
    if !ref_audios.is_empty() {
        push_container(Path::new(args.reference.as_str()), true, false, true);
        let ix = file_index-1;
        // For each audio track in ref, we rely on container order; apply compression per-index
        for (i, _tr) in ref_audios.iter().enumerate() {
            tokens.push("--compression".into()); tokens.push(format!("{}:none", i));
            track_order.push(format!("{}:{}", ix, i));
        }
    }

    // TER subtitles
    for s in staged_files.iter().filter(|s| s.src_group=="TER" && s.kind=="subtitles") {
        push_container(&s.path, true, true, false);
        let ix = file_index-1;
        tokens.push("--sync".into()); tokens.push(format!("0:{}", args.ter_delay));
        // signs default
        let name_lc = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
        if Regex::new(&args.signs_pattern).unwrap_or_else(|_| Regex::new("(?i)sign|song").unwrap()).is_match(&name_lc) {
            tokens.push("--default-track-flag".into()); tokens.push("0:yes".into());
        }
        track_order.push(format!("{}:0", ix));
    }

    // Attachments (TER only for now)
    for s in staged_files.iter().filter(|s| s.kind=="attachments") {
        let fname = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("attach.bin").to_string();
        tokens.push("--attachment-name".into()); tokens.push(fname.clone());
        // basic mime guess
        let mime = if fname.to_ascii_lowercase().ends_with(".ttf") || fname.to_ascii_lowercase().ends_with(".otf") {"font/ttf"} else {"application/octet-stream"};
        tokens.push("--attachment-mime-type".into()); tokens.push(mime.into());
        tokens.push("--attach-file".into()); tokens.push(s.path.display().to_string());
    }

    // track-order
    if !track_order.is_empty() {
        tokens.push("--track-order".into());
        tokens.push(track_order.join(","));
    }

    // Write @opts
    let opts_path = if let Some(p) = &args.out_opts { PathBuf::from(p.as_str()) } else { stage_dir.join("opts.json") };
    fs::write(&opts_path, serde_json::to_string_pretty(&tokens)?)?;
    println!("Wrote opts.json -> {}", opts_path.display());

    // Run mkvmerge @opts
    let status = Command::new(&args.mkvmerge)
        .arg(format!("@{}", opts_path.display()))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        anyhow::bail!("mkvmerge failed");
    }

    // Cleanup: if out_opts not requested, remove staging dir
    if args.out_opts.is_none() {
        let _ = fs::remove_dir_all(stage_dir);
    }
    Ok(())
}
