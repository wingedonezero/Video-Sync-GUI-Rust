use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Serialize, Deserialize};
use regex::Regex;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Attachment {
    file: PathBuf,
    name: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrackProps {
    language: Option<String>,
    track_name: Option<String>,
    default: bool,
    compression_none: bool,
    sync_ms: i64,
}

#[allow(dead_code)]
pub struct MuxConfig<'a> {
    pub reference: &'a Path,
    pub secondary: Option<&'a Path>,
    pub tertiary: Option<&'a Path>,
    pub output: &'a Path,
    pub mkvmerge: &'a Path,
    pub mkvextract: &'a Path,
    pub prefer_lang: &'a str,
    pub signs_regex: &'a Regex,
    pub out_opts: Option<&'a Path>,
    pub sec_delay_ms: Option<i64>,
    pub ter_delay_ms: Option<i64>,
}

pub fn mux(cfg: &MuxConfig) -> Result<()> {
    // Keep behavior as-is for now; wire up full extraction/opts in next bundle.
    // Touch fields to avoid dead_code warnings without changing logic:
    let _ = (
        cfg.reference,
        cfg.secondary,
        cfg.tertiary,
        cfg.output,
        cfg.mkvmerge,
        cfg.mkvextract,
        cfg.prefer_lang,
        cfg.signs_regex,
        cfg.out_opts,
        cfg.sec_delay_ms,
        cfg.ter_delay_ms,
    );
    Ok(())
}

#[allow(dead_code)]
// Positive-only delay scheme
fn compute_positive_only_delays(raw_sec: i64, raw_ter: i64) -> (i64, i64, i64) {
    let min_val = raw_sec.min(raw_ter).min(0);
    let global = min_val.unsigned_abs() as i64;
    let deltas = vec![0i64, raw_sec, raw_ter];
    let residuals: Vec<i64> = deltas.into_iter().map(|d| d + global).collect();
    (global, residuals[1], residuals[2])
}
