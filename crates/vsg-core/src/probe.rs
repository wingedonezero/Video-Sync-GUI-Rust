
use anyhow::{Result, Context};
use serde_json::Value;
use std::process::Command;
use std::path::Path;
use crate::types::*;
use crate::process::{run_quiet, must_succeed};

pub fn probe_one(source_tag: &str, file: &Path, mkvmerge: &Path) -> Result<ProbeResult> {
    let out = run_quiet(Command::new(mkvmerge).arg("-J").arg(file))
        .with_context(|| format!("spawn mkvmerge -J {}", file.display()))?;
    let out = must_succeed(out, "mkvmerge -J failed")?;
    let v: Value = serde_json::from_slice(&out.stdout)
        .with_context(|| "parse mkvmerge -J JSON")?;

    let mut tracks = Vec::new();
    if let Some(arr) = v.get("tracks").and_then(|t| t.as_array()) {
        for (order, tr) in arr.iter().enumerate() {
            let id = tr.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let ttype = tr.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let kind = match ttype {
                "video" => TrackKind::Video,
                "audio" => TrackKind::Audio,
                "subtitles" => TrackKind::Subtitle,
                _ => continue,
            };
            let codec = tr.get("codec").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let lang = tr.get("properties").and_then(|p| p.get("language")).and_then(|x| x.as_str()).map(|s| s.to_string());
            let name = tr.get("properties").and_then(|p| p.get("track_name")).and_then(|x| x.as_str()).map(|s| s.to_string());
            let default_flag = tr.get("properties").and_then(|p| p.get("default_track")).and_then(|x| x.as_bool()).unwrap_or(false);
            tracks.push(TrackMeta {
                source: source_tag.to_string(),
                id,
                kind,
                codec,
                lang,
                name,
                default_flag,
                order_in_src: order as u32,
            });
        }
    }

    let mut attachments = Vec::new();
    if let Some(arr) = v.get("attachments").and_then(|a| a.as_array()) {
        for att in arr {
            let id = att.get("id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            let file_name = att.get("file_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let content_type = att.get("content_type").and_then(|x| x.as_str()).map(|s| s.to_string());
            attachments.push(AttachmentMeta { id, file_name, content_type });
        }
    }

    let has_chapters = v.get("chapters").is_some();

    Ok(ProbeResult { tracks, attachments, has_chapters })
}

pub fn probe_all(src: &Sources, tools: &ToolPaths) -> Result<(ProbeResult, Option<ProbeResult>, Option<ProbeResult>)> {
    let refp = probe_one("REF", &src.reference, &tools.mkvmerge)?;
    let secp = if let Some(s) = &src.secondary { Some(probe_one("SEC", s, &tools.mkvmerge)?) } else { None };
    let terp = if let Some(t) = &src.tertiary  { Some(probe_one("TER", t, &tools.mkvmerge)?) } else { None };
    Ok((refp, secp, terp))
}
