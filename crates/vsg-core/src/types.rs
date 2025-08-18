
use camino::Utf8PathBuf;
use regex::Regex;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPaths {
    pub mkvmerge: PathBuf,
    pub mkvextract: PathBuf,
    pub ffmpeg: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sources {
    pub reference: PathBuf,
    pub secondary: Option<PathBuf>,
    pub tertiary: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempLayout {
    pub root: PathBuf,
    pub out_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OrderRules {
    pub prefer_lang: String,
    pub signs_regex: Regex,
    pub first_sub_default: bool,
    pub default_signs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDelays {
    pub sec_ms: Option<i64>,
    pub ter_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositiveDelays {
    pub global_ms: i64,
    pub sec_residual_ms: i64,
    pub ter_residual_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrackKind { Video, Audio, Subtitle }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub source: String,              // "REF" | "SEC" | "TER"
    pub id: u32,
    pub kind: TrackKind,
    pub codec: String,
    pub lang: Option<String>,
    pub name: Option<String>,
    pub default_flag: bool,
    pub order_in_src: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractPlan {
    pub tracks: Vec<(TrackMeta, PathBuf)>,
    pub chapters_xml: Option<PathBuf>,
    pub attachments: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTrack {
    pub meta: TrackMeta,
    pub file: PathBuf,
    pub mkvmerge_track_opts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub final_order: Vec<PlannedTrack>,
    pub chapters: Option<PathBuf>,
    pub delays: crate::types::PositiveDelays,
}
