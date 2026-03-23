//! Pipeline context — 1:1 port of `vsg_core/orchestrator/steps/context.py`.
//!
//! The Context struct carries all state through the pipeline steps.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::models::context_types::{
    DriftFlagsEntry, ManualLayoutItem, SegmentFlagsEntry, SteppingQualityIssue,
    SyncStabilityIssue, VideoVerifiedResult,
};
use crate::models::jobs::{Delays, PlanItem};
use crate::models::settings::AppSettings;

/// Pipeline context — carries all state through steps — `Context`
pub struct Context {
    // Provided by Orchestrator entry
    pub settings: AppSettings,
    pub tool_paths: HashMap<String, String>,
    pub log: Box<dyn Fn(&str) + Send + Sync>,
    pub progress: Box<dyn Fn(f64) + Send + Sync>,
    pub output_dir: String,
    pub temp_dir: PathBuf,
    pub sources: HashMap<String, String>,
    pub and_merge: bool,
    pub manual_layout: Vec<ManualLayoutItem>,
    pub attachment_sources: Vec<String>,

    /// Per-source correlation settings (from job layout).
    pub source_settings: HashMap<String, serde_json::Value>,

    // Filled along the pipeline
    pub delays: Option<Delays>,
    pub extracted_items: Option<Vec<PlanItem>>,
    pub chapters_xml: Option<String>,
    pub attachments: Option<Vec<String>>,

    /// Flags for tracks needing segmented (stepping) correction.
    /// Key format: "{source}_{track_id}" e.g. "Source 2_1"
    pub segment_flags: HashMap<String, SegmentFlagsEntry>,

    /// Flags for tracks needing PAL drift correction.
    pub pal_drift_flags: HashMap<String, DriftFlagsEntry>,

    /// Flags for tracks needing linear drift correction.
    pub linear_drift_flags: HashMap<String, DriftFlagsEntry>,

    /// Source 1's reference audio container delay.
    pub source1_audio_container_delay_ms: f64,

    /// All container delays by source and track ID.
    /// Format: {source_key: {track_id: delay_ms}}
    pub container_delays: HashMap<String, HashMap<i32, i32>>,

    /// Whether a global shift is necessary.
    pub global_shift_is_required: bool,

    /// Timing sync mode.
    pub sync_mode: String,

    /// Sources with stepping detected (for final report).
    pub stepping_sources: Vec<String>,

    /// Sources where stepping was detected but correction is disabled.
    pub stepping_detected_disabled: Vec<String>,

    /// Sources where stepping was detected but skipped due to source separation.
    pub stepping_detected_separated: Vec<String>,

    /// EDLs for stepping correction by source.
    pub stepping_edls: HashMap<String, Vec<serde_json::Value>>,

    /// Stepping quality issues for reporting.
    pub stepping_quality_issues: Vec<SteppingQualityIssue>,

    /// Sync stability issues (correlation variance) for reporting.
    pub sync_stability_issues: Vec<SyncStabilityIssue>,

    /// Video-verified subtitle sync results per source.
    pub video_verified_sources: HashMap<String, VideoVerifiedResult>,

    /// Subtitle-specific delays (from sync modes like video-verified).
    /// SEPARATE from audio delays in delays.source_delays_ms.
    pub subtitle_delays_ms: HashMap<String, f64>,

    /// Frame audit results from video-verified sync.
    pub frame_audit_results: HashMap<String, serde_json::Value>,

    /// Cached video properties per source.
    pub video_properties: HashMap<String, serde_json::Value>,

    // Results/summaries
    pub out_file: Option<String>,
    pub tokens: Option<Vec<String>>,
}

impl Context {
    /// Create a new Context with required fields, everything else defaulted.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settings: AppSettings,
        tool_paths: HashMap<String, String>,
        log: Box<dyn Fn(&str) + Send + Sync>,
        progress: Box<dyn Fn(f64) + Send + Sync>,
        output_dir: String,
        temp_dir: PathBuf,
        sources: HashMap<String, String>,
        and_merge: bool,
        manual_layout: Vec<ManualLayoutItem>,
        attachment_sources: Vec<String>,
        source_settings: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            settings,
            tool_paths,
            log,
            progress,
            output_dir,
            temp_dir,
            sources,
            and_merge,
            manual_layout,
            attachment_sources,
            source_settings,
            delays: None,
            extracted_items: None,
            chapters_xml: None,
            attachments: None,
            segment_flags: HashMap::new(),
            pal_drift_flags: HashMap::new(),
            linear_drift_flags: HashMap::new(),
            source1_audio_container_delay_ms: 0.0,
            container_delays: HashMap::new(),
            global_shift_is_required: false,
            sync_mode: "positive_only".to_string(),
            stepping_sources: Vec::new(),
            stepping_detected_disabled: Vec::new(),
            stepping_detected_separated: Vec::new(),
            stepping_edls: HashMap::new(),
            stepping_quality_issues: Vec::new(),
            sync_stability_issues: Vec::new(),
            video_verified_sources: HashMap::new(),
            subtitle_delays_ms: HashMap::new(),
            frame_audit_results: HashMap::new(),
            video_properties: HashMap::new(),
            out_file: None,
            tokens: None,
        }
    }
}
