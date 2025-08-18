
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use regex::Regex;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::env;

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
    pub staging_root: Option<Utf8PathBuf>,   // if None -> default to <exe_dir>/temp
    pub reference: Utf8PathBuf,
    pub secondary: Option<Utf8PathBuf>,
    pub tertiary:  Option<Utf8PathBuf>,
    pub output:    Utf8PathBuf,              // if parent missing -> <exe_dir>/output
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
    track_name: Option<String>,
    language: Option<String>,
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

fn mkvextract_chapters(mkvextract: &str, src: &Utf8PathBuf, out_xml: &Path) -> Result<()> {
    // mkvextract chapters src.mkv (XML to stdout)
    let (code, out, err) = run_out(mkvextract, &["chapters", src.as_str()])?;
    if code != 0 { anyhow::bail!("mkvextract chapters failed: {}", err); }
    fs::write(out_xml, out.as_bytes())?;
    Ok(())
}

pub fn run_mux(args: MuxArgs) -> Result<()> {
    // Resolve default dirs (temp/output) relative to exe dir
    let exe_dir = env::current_exe().ok().and_then(|p| p.parent().map(|q| q.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let default_temp = exe_dir.join("temp");
    let default_out_dir = exe_dir.join("output");
    if !default_out_dir.exists() { let _ = fs::create_dir_all(&default_out_dir); }

    let stage_dir_path = if let Some(sr) = &args.staging_root {
        PathBuf::from(sr.as_str())
    } else {
        default_temp
    };
    fs::create_dir_all(&stage_dir_path)?;

    // Output parent ensure
    let out_parent = Path::new(args.output.as_str()).parent().map(|p| p.to_path_buf()).unwrap_or(default_out_dir.clone());
    if !out_parent.exists() { let _ = fs::create_dir_all(&out_parent); }

    let signs_re = Regex::new(&args.signs_pattern).unwrap_or_else(|_| Regex::new("(?i)sign|song").unwrap());
    let prefer_lang = args.prefer_lang.clone().unwrap_or_else(|| "eng".to_string());

    // Probe
    let ref_probe = probe(&args.mkvmerge, &args.reference)?;
    let sec_probe = if let Some(sec) = &args.secondary { Some(probe(&args.mkvmerge, sec).ok()) } else { None };
    let ter_probe = if let Some(ter) = &args.tertiary  { Some(probe(&args.mkvmerge, ter).ok()) } else { None };

    // 1) Build extraction plan (staged files)
    let mut staged_files: Vec<SelFile> = Vec::new();

    // REF video container
    if ref_probe.tracks.iter().any(|t| t.kind=="video") {
        staged_files.push(SelFile{ path: PathBuf::from(args.reference.as_str()), kind: "video", src_group: "REF", input_track_id: 0, track_name: None, language: None });
    } else {
        anyhow::bail!("Reference has no video: {}", args.reference);
    }

    // REF chapters extract for later rename/mutate
    let chapters_xml = stage_dir_path.join("REF_chapters.xml");
    if let Some(mkvextract) = &args.mkvextract {
        let _ = mkvextract_chapters(mkvextract, &args.reference, &chapters_xml);
    }

    // SEC audios (eng/en only) + SEC subs (all)
    if let (Some(Some(sp)), Some(sec)) = (sec_probe.as_ref(), args.secondary.as_ref()) {
        if let Some(mkvextract) = &args.mkvextract {
            // audios with lang filter
            let mut a_pairs: Vec<(u32, PathBuf, Option<String>, Option<String>)> = Vec::new();
            for tr in sp.tracks.iter().filter(|t| t.kind=="audio") {
                let lang = tr.properties.as_ref().and_then(|p| p.language.clone()).unwrap_or_default().to_ascii_lowercase();
                if lang=="eng" || lang=="en" {
                    let ext = ext_for_codec("audio", tr.codec_id.as_deref(), None);
                    let name = format!("SEC_AUD_t{:02}.{}", tr.id, ext);
                    a_pairs.push((tr.id, stage_dir_path.join(&name), tr.properties.as_ref().and_then(|p| p.track_name.clone()), tr.properties.as_ref().and_then(|p| p.language.clone())));
                }
            }
            if !a_pairs.is_empty() {
                let pairs_only: Vec<(u32, PathBuf)> = a_pairs.iter().map(|(i,p,_,_)| (*i, p.clone())).collect();
                mkvextract_tracks(mkvextract, sec, &pairs_only)?;
                for (id, out, name, lang) in a_pairs {
                    staged_files.push(SelFile{ path: out, kind: "audio", src_group:"SEC", input_track_id:id, track_name:name, language:lang });
                }
            }
            // subs all
            let mut s_pairs: Vec<(u32, PathBuf, Option<String>, Option<String>)> = Vec::new();
            for tr in sp.tracks.iter().filter(|t| t.kind=="subtitles") {
                let name = format!("SEC_SUB_t{:02}.ass"); // guess; extension doesn't matter for mkvmerge
                s_pairs.push((tr.id, stage_dir_path.join(&name), tr.properties.as_ref().and_then(|p| p.track_name.clone()), tr.properties.as_ref().and_then(|p| p.language.clone())));
            }
            if !s_pairs.is_empty() {
                let pairs_only: Vec<(u32, PathBuf)> = s_pairs.iter().map(|(i,p,_,_)| (*i, p.clone())).collect();
                mkvextract_tracks(mkvextract, sec, &pairs_only)?;
                for (id, out, name, lang) in s_pairs {
                    staged_files.push(SelFile{ path: out, kind: "subtitles", src_group:"SEC", input_track_id:id, track_name:name, language:lang });
                }
            }
        }
    }

    // TER subtitles + attachments
    if let (Some(Some(tp)), Some(ter)) = (ter_probe.as_ref(), args.tertiary.as_ref()) {
        if let Some(mkvextract) = &args.mkvextract {
            // subs
            let mut sub_pairs: Vec<(u32, PathBuf, Option<String>, Option<String>)> = Vec::new();
            for tr in tp.tracks.iter().filter(|t| t.kind=="subtitles") {
                let name = format!("TER_SUB_t{:02}.ass");
                sub_pairs.push((tr.id, stage_dir_path.join(&name), tr.properties.as_ref().and_then(|p| p.track_name.clone()), tr.properties.as_ref().and_then(|p| p.language.clone())));
            }
            if !sub_pairs.is_empty() {
                let pairs_only: Vec<(u32, PathBuf)> = sub_pairs.iter().map(|(i,p,_,_)| (*i, p.clone())).collect();
                mkvextract_tracks(mkvextract, ter, &pairs_only)?;
                for (id, out, name, lang) in sub_pairs {
                    staged_files.push(SelFile{ path: out, kind: "subtitles", src_group:"TER", input_track_id:id, track_name:name, language:lang });
                }
            }
            // attachments
            if let Some(atts) = &tp.attachments {
                for att in atts {
                    let fname = att.file_name.clone().unwrap_or_else(|| format!("TER_attach_{}.bin", att.id));
                    let out = stage_dir_path.join(&fname);
                    let args2 = vec!["attachments", ter.as_str(), &format!("{}:{}", att.id, out.display())];
                    run_ok(mkvextract, &args2.iter().map(|s| s.as_ref()).collect::<Vec<&str>>())?;
                    staged_files.push(SelFile{ path: out, kind:"attachments", src_group:"TER", input_track_id: att.id, track_name: None, language: None });
                }
            }
        }
    }

    // 2) Build tokens in final order:
    let mut tokens: Vec<String> = Vec::new();
    tokens.push("--output".into());
    tokens.push(args.output.to_string());

    let mut file_index: u32 = 0;
    let mut track_order: Vec<String> = Vec::new();
    let mut push_container = |path: &Path, no_video: bool, no_audio: bool, no_subs: bool| {
        tokens.push("(".into()); tokens.push(path.display().to_string()); tokens.push(")".into());
        if no_video { tokens.push("--no-video".into()); }
        if no_audio { tokens.push("--no-audio".into()); }
        if no_subs  { tokens.push("--no-subtitles".into()); }
        tokens.push("--compression".into()); tokens.push("0:none".into());
        file_index += 1;
    };
    let mut add_lang_name = |language: &Option<String>, name: &Option<String>| {
        if let Some(lang) = language.as_ref() {
            tokens.push("--language".into()); tokens.push(format!("0:{}", lang));
        }
        if let Some(tn) = name.as_ref() {
            if !tn.is_empty() {
                tokens.push("--track-name".into()); tokens.push(format!("0:{}", tn));
            }
        }
    };

    // REF video
    push_container(Path::new(args.reference.as_str()), false, true, true);
    track_order.push(format!("{}:0", file_index-1));

    // SEC audios (already filtered to ENG/EN), maintain order of original ids
    let sec_aud: Vec<SelFile> = staged_files.iter().cloned().filter(|s| s.src_group=="SEC" && s.kind=="audio").collect();
    let mut first_audio = true;
    for s in sec_aud {
        push_container(&s.path, true, false, true);
        let ix = file_index-1;
        add_lang_name(&s.language, &s.track_name);
        tokens.push("--sync".into()); tokens.push(format!("0:{}", args.sec_delay));
        tokens.push("--default-track-flag".into()); tokens.push(if first_audio { "0:yes".into() } else { "0:no".into() });
        first_audio = false;
        track_order.push(format!("{}:0", ix));
    }

    // REF audios (preserve order as in ref container)
    let ref_audios: Vec<&TrackJson> = ref_probe.tracks.iter().filter(|t| t.kind=="audio").collect();
    if !ref_audios.is_empty() {
        push_container(Path::new(args.reference.as_str()), true, false, true);
        let ix = file_index-1;
        for (i, tr) in ref_audios.iter().enumerate() {
            tokens.push("--compression".into()); tokens.push(format!("{}:none", i));
            // propagate language/name
            if let Some(p) = &tr.properties {
                if let Some(lang) = &p.language {
                    tokens.push("--language".into()); tokens.push(format!("{}:{}", i, lang));
                }
                if let Some(tn) = &p.track_name {
                    if !tn.is_empty() {
                        tokens.push("--track-name".into()); tokens.push(format!("{}:{}", i, tn));
                    }
                }
            }
            track_order.push(format!("{}:{}", ix, i));
        }
    }

    // SEC subs
    for s in staged_files.iter().filter(|s| s.src_group=="SEC" && s.kind=="subtitles") {
        push_container(&s.path, true, true, false);
        let ix = file_index-1;
        add_lang_name(&s.language, &s.track_name);
        track_order.push(format!("{}:0", ix));
    }

    // TER subs (+ default if signs)
    for s in staged_files.iter().filter(|s| s.src_group=="TER" && s.kind=="subtitles") {
        push_container(&s.path, true, true, false);
        let ix = file_index-1;
        add_lang_name(&s.language, &s.track_name);
        tokens.push("--sync".into()); tokens.push(format!("0:{}", args.ter_delay));
        let fname_lc = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
        if signs_re.is_match(&fname_lc) {
            tokens.push("--default-track-flag".into()); tokens.push("0:yes".into());
        }
        track_order.push(format!("{}:0", ix));
    }

    // Attachments
    for s in staged_files.iter().filter(|s| s.kind=="attachments") {
        let fname = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("attach.bin").to_string();
        tokens.push("--attachment-name".into()); tokens.push(fname.clone());
        let mime = if fname.to_ascii_lowercase().ends_with(".ttf") || fname.to_ascii_lowercase().ends_with(".otf") {"font/ttf"} else {"application/octet-stream"};
        tokens.push("--attachment-mime-type".into()); tokens.push(mime.into());
        tokens.push("--attach-file".into()); tokens.push(s.path.display().to_string());
    }

    // Chapters from REF if we extracted
    if chapters_xml.exists() {
        tokens.push("--chapters".into());
        tokens.push(chapters_xml.display().to_string());
    }

    // track-order
    if !track_order.is_empty() {
        tokens.push("--track-order".into());
        tokens.push(track_order.join(","));
    }

    // Write @opts
    let opts_path: PathBuf = if let Some(p) = &args.out_opts { PathBuf::from(p.as_str()) } else { stage_dir_path.join("opts.json") };
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

    // Cleanup: if out_opts not requested, remove staging dir AND empty temp dir
    if args.out_opts.is_none() {
        let _ = fs::remove_dir_all(&stage_dir_path);
    }
    Ok(())
}
