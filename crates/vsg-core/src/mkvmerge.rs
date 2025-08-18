use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use crate::tracks::{ProbeResult, TrackMeta};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenList(pub Vec<String>);

fn first_of_kind<'a>(pr: &'a ProbeResult, kind: &str) -> Option<&'a TrackMeta> {
    pr.tracks.iter().find(|t| t.kind == kind)
}

/// Build tokens for a simple, common case:
/// - one video from ref
/// - one audio from secondary (if provided)
/// - one subtitle from tertiary (if provided)
/// Track order: video -> sec audio -> ter subs
/// Delays are *already* adjusted by always-add policy (all >= 0).
pub fn build_simple_tokens(
    output_path: &str,
    ref_path: &str,
    ref_probe: &ProbeResult,
    sec_path: Option<&str>,
    sec_probe: Option<&ProbeResult>,
    ter_path: Option<&str>,
    ter_probe: Option<&ProbeResult>,
    ref_delay_ms: i32,
    sec_delay_ms: Option<i32>,
    ter_delay_ms: Option<i32>,
) -> Result<TokenList> {
    if !Path::new(output_path).exists() {
        // parent can be missing; mkvmerge will create. We don't enforce here.
    }
    // choose first video from ref
    let ref_video = first_of_kind(ref_probe, "video")
        .ok_or_else(|| anyhow!("no video track in reference file"))?;
    // audio/sub stubs
    let sec_audio = if let (Some(p), Some(pp)) = (sec_path, sec_probe) {
        first_of_kind(pp, "audio").map(|t| (p, t))
    } else { None };
    let ter_subs = if let (Some(p), Some(pp)) = (ter_path, ter_probe) {
        first_of_kind(pp, "subtitles").map(|t| (p, t))
    } else { None };

    let mut v: Vec<String> = Vec::new();
    v.push("--output".into()); v.push(output_path.into());

    // ref: include only the chosen video track
    v.push("(".into()); v.push(ref_path.into()); v.push(")".into());
    v.push("--video-tracks".into()); v.push(format!("{}", ref_video.id));
    // Flags for the first/only video
    v.push("--default-track-flag".into()); v.push("0:yes".into());
    v.push("--compression".into()); v.push("0:none".into());
    // sync for ref (apply delay if non-zero)
    if ref_delay_ms != 0 {
        v.push("--sync".into()); v.push(format!("0:{}", ref_delay_ms));
    }

    // secondary audio (file index 1)
    if let Some((sec_path_s, sec_a)) = sec_audio {
        v.push("(".into()); v.push(sec_path_s.into()); v.push(")".into());
        v.push("--audio-tracks".into()); v.push(format!("{}", sec_a.id));
        // language/default
        let lang = sec_a.lang.clone().unwrap_or_else(|| "und".into());
        v.push("--language".into()); v.push(format!("0:{}", lang));
        v.push("--default-track-flag".into()); v.push("0:yes".into());
        v.push("--compression".into()); v.push("0:none".into());
        if let Some(d) = sec_delay_ms {
            if d != 0 {
                v.push("--sync".into()); v.push(format!("0:{}", d));
            }
        }
    }

    // tertiary subs (file index 2 or 1 depending on sec presence)
    if let Some((ter_path_s, ter_s)) = ter_subs {
        v.push("(".into()); v.push(ter_path_s.into()); v.push(")".into());
        v.push("--subtitle-tracks".into()); v.push(format!("{}", ter_s.id));
        let lang = ter_s.lang.clone().unwrap_or_else(|| "und".into());
        v.push("--language".into()); v.push(format!("0:{}", lang));
        v.push("--default-track-flag".into()); v.push("0:no".into());
        v.push("--compression".into()); v.push("0:none".into());
        if let Some(d) = ter_delay_ms {
            if d != 0 {
                v.push("--sync".into()); v.push(format!("0:{}", d));
            }
        }
    }

    // Build track-order: indices are based on file positions in this command.
    // file 0: ref video track id -> index 0
    // file 1: sec audio (if any) -> index 0 in its file
    // file 2: ter subs (if any) -> index 0 in its file
    let mut order = Vec::new();
    order.push("0:0".to_string()); // first track from ref input
    let mut fidx = 1;
    if sec_audio.is_some() {
        order.push(format!("{}:0", fidx));
        fidx += 1;
    }
    if ter_subs.is_some() {
        order.push(format!("{}:0", fidx));
    }
    if !order.is_empty() {
        v.push("--track-order".into());
        v.push(order.join(","));
    }

    Ok(TokenList(v))
}

pub fn write_opts_json(path: &str, tokens: &TokenList) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(&tokens.0)?)?;
    Ok(())
}
