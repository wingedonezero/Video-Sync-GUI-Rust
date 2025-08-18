
use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use regex::Regex;

use crate::types::*;
use crate::process::{run_quiet, must_succeed};
use crate::probe::probe_all;

fn ext_for_codec(codec: &str, kind: &TrackKind) -> &'static str {
    let lc = codec.to_lowercase();
    match kind {
        TrackKind::Video => {
            if lc.contains("h.264") || lc.contains("avc") { "h264" }
            else if lc.contains("hevc") || lc.contains("h.265") { "h265" }
            else if lc.contains("mpeg-4p10") { "h264" }
            else { "video" }
        }
        TrackKind::Audio => {
            if lc.contains("truehd") { "thd" }
            else if lc.contains("ac-3") || lc.contains("ac3") { "ac3" }
            else if lc.contains("e-ac-3") || lc.contains("eac3") { "eac3" }
            else if lc.contains("dts") { "dts" }
            else if lc.contains("aac") { "aac" }
            else if lc.contains("pcm") || lc.contains("flac") || lc.contains("wav") { "wav" }
            else { "audio" }
        }
        TrackKind::Subtitle => {
            if lc.contains("pgs") || lc.contains("hdmv") { "sup" }
            else if lc.contains("ass") || lc.contains("ssa") { "ass" }
            else if lc.contains("subrip") || lc.contains("srt") { "srt" }
            else { "sub" }
        }
    }
}

fn wants_sec_track(t: &TrackMeta, prefer_lang: &str) -> bool {
    match t.kind {
        TrackKind::Audio => {
            let lang = t.lang.as_deref().unwrap_or("").to_lowercase();
            lang == prefer_lang
        }
        TrackKind::Subtitle => true,
        _ => false,
    }
}

fn wants_ter_track(t: &TrackMeta, signs_re: &Regex) -> bool {
    match t.kind {
        TrackKind::Subtitle => {
            let name_lc = t.name.as_deref().unwrap_or("").to_lowercase();
            signs_re.is_match(&name_lc) || true // include all subs for TER; signs preferred later
        }
        _ => false,
    }
}

fn name_for_track(tag: &str, t: &TrackMeta, idx: usize) -> String {
    let ext = ext_for_codec(&t.codec, &t.kind);
    format!("{}_{}_{:03}.{}", tag, match t.kind {TrackKind::Video=>"v",TrackKind::Audio=>"a",TrackKind::Subtitle=>"s"}, idx, ext)
}

pub fn extract_plan_and_execute(src: &Sources, tools: &ToolPaths, temp: &TempLayout, prefer_lang: &str, signs_pattern: &str) -> Result<ExtractPlan> {
    fs::create_dir_all(&temp.root)?;
    fs::create_dir_all(&temp.out_dir)?;
    let signs_re = Regex::new(signs_pattern).unwrap();

    let (refp, secp, terp) = probe_all(src, tools)?;

    // REF: first video + chapters
    let ref_video = refp.tracks.iter().find(|t| matches!(t.kind, TrackKind::Video)).cloned();
    let ref_video_item = if let Some(v) = ref_video {
        let name = name_for_track("REF", &v, 0);
        let path = temp.root.join(name);
        // mkvextract tracks reference id:path
        let out = run_quiet(Command::new(&tools.mkvextract).arg(&src.reference).arg("tracks").arg(format!("{}:{}", v.id, path.display())))?;
        must_succeed(out, "mkvextract REF video")?;
        Some(ExtractItem { meta: v, out_path: path })
    } else { None };

    // Chapters
    let chapters_xml = if refp.has_chapters {
        let out_xml = temp.root.join("REF_chapters.xml");
        let out = run_quiet(Command::new(&tools.mkvextract).arg(&src.reference).arg("chapters").arg("-s").arg("-o").arg(&out_xml))?;
        must_succeed(out, "mkvextract chapters")?;
        Some(out_xml)
    } else { None };

    // SEC: eng audio + all subs
    let mut sec_tracks = Vec::new();
    if let (Some(sec_src), Some(secp)) = (&src.secondary, &secp) {
        let mut args = Vec::new();
        for (i, t) in secp.tracks.iter().enumerate() {
            if wants_sec_track(t, &prefer_lang.to_lowercase()) {
                let out = temp.root.join(name_for_track("SEC", t, i));
                args.push((t.clone(), out));
            }
        }
        if !args.is_empty() {
            // build mkvextract tracks with many mappings
            let mut cmd = Command::new(&tools.mkvextract);
            cmd.arg(sec_src).arg("tracks");
            for (t, p) in &args {
                cmd.arg(format!("{}:{}", t.id, p.display()));
            }
            let out = run_quiet(cmd)?;
            must_succeed(out, "mkvextract SEC tracks")?;
            for (t,p) in args { sec_tracks.push(ExtractItem { meta: t, out_path: p }); }
        }
    }

    // TER: subs + attachments (fonts)
    let mut ter_subs = Vec::new();
    let mut ter_attachments = Vec::new();
    if let (Some(ter_src), Some(terp)) = (&src.tertiary, &terp) {
        // subs
        let mut sub_maps = Vec::new();
        for (i, t) in terp.tracks.iter().enumerate() {
            if wants_ter_track(t, &signs_re) {
                let out = temp.root.join(name_for_track("TER", t, i));
                sub_maps.push((t.clone(), out));
            }
        }
        if !sub_maps.is_empty() {
            let mut cmd = Command::new(&tools.mkvextract);
            cmd.arg(ter_src).arg("tracks");
            for (t, p) in &sub_maps {
                cmd.arg(format!("{}:{}", t.id, p.display()));
            }
            let out = run_quiet(cmd)?;
            must_succeed(out, "mkvextract TER subs")?;
            for (t,p) in sub_maps { ter_subs.push(ExtractItem { meta: t, out_path: p }); }
        }
        // attachments
        if !terp.attachments.is_empty() {
            let mut cmd = Command::new(&tools.mkvextract);
            cmd.arg(ter_src).arg("attachments");
            for a in &terp.attachments {
                let outp = temp.root.join(format!("TER_attach_{:03}_{}", a.id, a.file_name));
                cmd.arg(format!("{}:{}", a.id, outp.display()));
                ter_attachments.push((a.clone(), outp));
            }
            let out = run_quiet(cmd)?;
            must_succeed(out, "mkvextract TER attachments")?;
        }
    }

    // Manifest
    let manifest_path = temp.root.join("manifest.json");
    let plan = ExtractPlan {
        ref_video: ref_video_item,
        sec_tracks,
        ter_subs,
        ter_attachments,
        chapters_xml,
        manifest_path: manifest_path.clone(),
    };
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&plan)?)?;
    Ok(plan)
}
