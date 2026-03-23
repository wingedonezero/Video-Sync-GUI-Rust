//! Job types — 1:1 port of `vsg_core/models/jobs.py`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::context_types::{
    FilterConfig, FontReplacements, SteppingQualityIssue, StylePatch, SyncStabilityIssue,
};
use super::media::Track;

/// Audio sync delays between sources — `Delays`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Delays {
    /// Rounded delay per source key, e.g. {"Source 2": -150}
    #[serde(default)]
    pub source_delays_ms: HashMap<String, i32>,
    /// Unrounded delays for VideoTimestamps precision
    #[serde(default)]
    pub raw_source_delays_ms: HashMap<String, f64>,
    /// Global shift applied to eliminate negative delays
    #[serde(default)]
    pub global_shift_ms: i32,
    /// Unrounded global shift for VideoTimestamps precision
    #[serde(default)]
    pub raw_global_shift_ms: f64,
}

/// A single track in the merge plan with all processing options — `PlanItem`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub track: Track,
    #[serde(default)]
    pub extracted_path: Option<PathBuf>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_forced_display: bool,
    #[serde(default)]
    pub apply_track_name: bool,
    #[serde(default)]
    pub convert_to_ass: bool,
    #[serde(default)]
    pub rescale: bool,
    #[serde(default = "default_size_multiplier")]
    pub size_multiplier: f64,
    #[serde(default)]
    pub style_patch: Option<StylePatch>,
    #[serde(default)]
    pub font_replacements: Option<FontReplacements>,
    #[serde(default)]
    pub user_modified_path: Option<String>,
    #[serde(default)]
    pub sync_to: Option<String>,
    #[serde(default)]
    pub is_preserved: bool,
    #[serde(default)]
    pub is_corrected: bool,
    #[serde(default)]
    pub correction_source: Option<String>,
    #[serde(default)]
    pub perform_ocr: bool,
    #[serde(default)]
    pub container_delay_ms: i32,
    #[serde(default)]
    pub custom_lang: String,
    #[serde(default)]
    pub custom_name: String,
    #[serde(default)]
    pub aspect_ratio: Option<String>,
    #[serde(default)]
    pub stepping_adjusted: bool,
    #[serde(default)]
    pub frame_adjusted: bool,

    // Generated track fields
    #[serde(default)]
    pub is_generated: bool,
    #[serde(default)]
    pub source_track_id: Option<i32>,
    #[serde(default)]
    pub filter_config: Option<FilterConfig>,
    #[serde(default)]
    pub original_style_list: Vec<String>,

    // Sync exclusion fields
    #[serde(default)]
    pub sync_exclusion_styles: Vec<String>,
    #[serde(default = "default_exclude")]
    pub sync_exclusion_mode: String,
    #[serde(default)]
    pub sync_exclusion_original_style_list: Vec<String>,

    // Stats
    #[serde(default)]
    pub framelocked_stats: Option<serde_json::Value>,
    #[serde(default)]
    pub clamping_info: Option<serde_json::Value>,
    #[serde(default)]
    pub video_verified_bitmap: bool,
    #[serde(default)]
    pub video_verified_details: Option<serde_json::Value>,
}

fn default_size_multiplier() -> f64 {
    1.0
}

fn default_exclude() -> String {
    "exclude".to_string()
}

/// The complete merge plan for a job — `MergePlan`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergePlan {
    pub items: Vec<PlanItem>,
    pub delays: Delays,
    #[serde(default)]
    pub chapters_xml: Option<PathBuf>,
    #[serde(default)]
    pub attachments: Vec<PathBuf>,
    /// Subtitle-specific delays (e.g., from video-verified mode)
    #[serde(default)]
    pub subtitle_delays_ms: HashMap<String, f64>,
}

/// Detailed result from pipeline.run_job() — `PipelineResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    /// "Merged", "Analyzed", or "Failed"
    pub status: String,
    pub name: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub delays: Option<HashMap<String, i32>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub issues: i32,
    #[serde(default)]
    pub stepping_sources: Vec<String>,
    #[serde(default)]
    pub stepping_detected_disabled: Vec<String>,
    #[serde(default)]
    pub stepping_detected_separated: Vec<String>,
    #[serde(default)]
    pub stepping_quality_issues: Vec<SteppingQualityIssue>,
    #[serde(default)]
    pub sync_stability_issues: Vec<SyncStabilityIssue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_default_is_empty() {
        let d = Delays::default();
        assert!(d.source_delays_ms.is_empty());
        assert_eq!(d.global_shift_ms, 0);
        assert_eq!(d.raw_global_shift_ms, 0.0);
    }

    #[test]
    fn delays_json_round_trip() {
        let mut d = Delays::default();
        d.source_delays_ms.insert("Source 2".to_string(), -150);
        d.global_shift_ms = 150;
        let json = serde_json::to_string(&d).unwrap();
        let parsed: Delays = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source_delays_ms["Source 2"], -150);
        assert_eq!(parsed.global_shift_ms, 150);
    }
}
