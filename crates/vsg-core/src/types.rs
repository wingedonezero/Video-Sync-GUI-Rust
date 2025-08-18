
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
    pub signs_regex: regex::Regex,
    pub first_sub_default: bool,
    pub default_signs: bool,
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
pub struct AttachmentMeta {
    pub id: u32,
    pub file_name: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub tracks: Vec<TrackMeta>,
    pub attachments: Vec<AttachmentMeta>,
    pub has_chapters: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractItem {
    pub meta: TrackMeta,
    pub out_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractPlan {
    pub ref_video: Option<ExtractItem>,
    pub sec_tracks: Vec<ExtractItem>,  // eng audio + subs
    pub ter_subs: Vec<ExtractItem>,    // signs + subs only
    pub ter_attachments: Vec<(AttachmentMeta, PathBuf)>,
    pub chapters_xml: Option<PathBuf>,
    pub manifest_path: PathBuf,
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
    pub attachments: Vec<PathBuf>,
    pub output_file: PathBuf,
}
