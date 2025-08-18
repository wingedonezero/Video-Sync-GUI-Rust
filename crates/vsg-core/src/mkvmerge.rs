
use anyhow::{Result, anyhow};
use regex::Regex;
use serde::{Serialize, Deserialize};
use crate::tracks::{ProbeResult, TrackMeta};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenList(pub Vec<String>);

fn best_video<'a>(pr: &'a ProbeResult) -> Option<&'a TrackMeta> {
    let mut vids: Vec<&TrackMeta> = pr.tracks.iter().filter(|t| t.kind == "video").collect();
    vids.sort_by_key(|t| {
        let c = t.codec.to_lowercase();
        match true {
            _ if c.contains("hevc") || c.contains("h.265") => 0,
            _ if c.contains("avc")  || c.contains("h.264") => 1,
            _ => 2,
        }
    });
    vids.into_iter().next()
}

fn best_audio<'a>(pr: &'a ProbeResult, prefer_lang: &str) -> Option<&'a TrackMeta> {
    let mut auds: Vec<&TrackMeta> = pr.tracks.iter().filter(|t| t.kind == "audio").collect();
    auds.sort_by_key(|t| if t.lang.as_deref().unwrap_or("") == prefer_lang { 0 } else { 1 });
    auds.sort_by_key(|t| {
        let c = t.codec.to_lowercase();
        match true {
            _ if c.contains("truehd") => 0,
            _ if c.contains("dts-hd") => 1,
            _ if c.contains("dts")    => 2,
            _ if c.contains("ac-3") || c.contains("ac3") => 3,
            _ if c.contains("aac")    => 4,
            _ if c.contains("pcm")    => 5,
            _ => 9,
        }
    });
    auds.into_iter().next()
}

fn best_signs_sub<'a>(pr: &'a ProbeResult, prefer_lang: &str, signs_re: &Regex) -> Option<&'a TrackMeta> {
    let mut subs: Vec<&TrackMeta> = pr.tracks.iter().filter(|t| t.kind == "subtitles").collect();
    subs.sort_by_key(|t| if t.lang.as_deref().unwrap_or("") == prefer_lang { 0 } else { 1 });
    subs.sort_by_key(|t| {
        let name = t.name.as_deref().unwrap_or("");
        if signs_re.is_match(name) { 0 } else { 1 }
    });
    subs.into_iter().next()
}

pub fn build_tokens_with_policy(
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
    prefer_lang: &str,
    signs_pattern: &str,
) -> Result<TokenList> {
    if !Path::new(output_path).parent().map(|p| p.exists()).unwrap_or(true) {
        return Err(anyhow!("output directory does not exist: {}", output_path));
    }

    let signs_re = Regex::new(signs_pattern).map_err(|e| anyhow!("invalid signs regex: {e}"))?;

    let ref_video = best_video(ref_probe).ok_or_else(|| anyhow!("no video track in reference"))?;
    let sec_audio = if let (Some(_p), Some(pp)) = (sec_path, sec_probe) {
        best_audio(pp, prefer_lang)
    } else { None };
    let ter_subs  = if let (Some(_p), Some(pp)) = (ter_path, ter_probe) {
        best_signs_sub(pp, prefer_lang, &signs_re)
    } else { None };

    let mut v: Vec<String> = Vec::new();
    v.push("--output".into()); v.push(output_path.into());

    v.push("(".into()); v.push(ref_path.into()); v.push(")".into());
    v.push("--video-tracks".into()); v.push(format!("{}", ref_video.id));
    v.push("--default-track-flag".into()); v.push("0:yes".into());
    v.push("--compression".into()); v.push("0:none".into());
    if ref_delay_ms != 0 {
        v.push("--sync".into()); v.push(format!("0:{}", ref_delay_ms));
    }

    if let (Some(sec_path_s), Some(sec_a)) = (sec_path, sec_audio) {
        v.push("(".into()); v.push(sec_path_s.into()); v.push(")".into());
        v.push("--audio-tracks".into()); v.push(format!("{}", sec_a.id));
        let lang = sec_a.lang.clone().unwrap_or_else(|| "und".into());
        v.push("--language".into()); v.push(format!("0:{}", lang));
        v.push("--default-track-flag".into()); v.push("0:yes".into());
        v.push("--compression".into()); v.push("0:none".into());
        if let Some(d) = sec_delay_ms { if d != 0 {
            v.push("--sync".into()); v.push(format!("0:{}", d));
        }}
    }

    if let (Some(ter_path_s), Some(ter_s)) = (ter_path, ter_subs) {
        v.push("(".into()); v.push(ter_path_s.into()); v.push(")".into());
        v.push("--subtitle-tracks".into()); v.push(format!("{}", ter_s.id));
        let lang = ter_s.lang.clone().unwrap_or_else(|| "und".into());
        v.push("--language".into()); v.push(format!("0:{}", lang));
        v.push("--default-track-flag".into()); v.push("0:no".into());
        v.push("--compression".into()); v.push("0:none".into());
        if let Some(d) = ter_delay_ms { if d != 0 {
            v.push("--sync".into()); v.push(format!("0:{}", d));
        }}
    }

    let mut order = Vec::new();
    order.push("0:0".to_string());
    let has_sec = sec_audio.is_some() && sec_path.is_some();
    let has_ter = ter_subs.is_some() && ter_path.is_some();
    let mut idx = 1;
    if has_sec { order.push(format!("{}:0", idx)); idx += 1; }
    if has_ter { order.push(format!("{}:0", idx)); }
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
