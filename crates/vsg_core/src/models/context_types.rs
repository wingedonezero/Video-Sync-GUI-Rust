//! Context types — 1:1 port of `vsg_core/models/context_types.py`.
//!
//! Python TypedDicts become Rust structs. Fields with `total=False` in Python
//! become `Option<T>` in Rust. Fields with `Required` stay non-optional.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─── Manual Layout Types ─────────────────────────────────────────────────────

/// A single track selection from the user's manual layout — `ManualLayoutItem`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManualLayoutItem {
    // Core identification
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub id: Option<i32>,
    #[serde(rename = "type", default)]
    pub track_type: Option<String>,

    // Stream properties
    #[serde(default)]
    pub codec_id: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub name: Option<String>,

    // Track flags
    #[serde(default)]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub is_forced_display: Option<bool>,
    #[serde(default)]
    pub apply_track_name: Option<bool>,

    // Processing options
    #[serde(default)]
    pub perform_ocr: Option<bool>,
    #[serde(default)]
    pub convert_to_ass: Option<bool>,
    #[serde(default)]
    pub rescale: Option<bool>,
    #[serde(default)]
    pub size_multiplier: Option<f64>,

    // Custom metadata overrides
    #[serde(default)]
    pub custom_lang: Option<String>,
    #[serde(default)]
    pub custom_name: Option<String>,

    // Sync configuration
    #[serde(default)]
    pub sync_to: Option<String>,
    #[serde(default)]
    pub correction_source: Option<String>,

    // Style modifications (subtitle tracks)
    #[serde(default)]
    pub style_patch: Option<StylePatch>,
    #[serde(default)]
    pub font_replacements: Option<FontReplacements>,

    // Generated track fields
    #[serde(default)]
    pub is_generated: Option<bool>,
    #[serde(default)]
    pub source_track_id: Option<i32>,
    #[serde(default)]
    pub filter_config: Option<FilterConfig>,
    #[serde(default)]
    pub original_style_list: Option<Vec<String>>,

    // External subtitle fields
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub needs_configuration: Option<bool>,

    // Sync exclusion fields
    #[serde(default)]
    pub sync_exclusion_styles: Option<Vec<String>>,
    #[serde(default)]
    pub sync_exclusion_mode: Option<String>,
    #[serde(default)]
    pub sync_exclusion_original_style_list: Option<Vec<String>>,
}

// ─── Source Settings Types ───────────────────────────────────────────────────

/// Settings for Source 1 (reference source) — `Source1Settings`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Source1Settings {
    #[serde(default)]
    pub correlation_ref_track: Option<i32>,
}

/// Settings for Source 2+ (sources to be synced) — `SourceNSettings`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceNSettings {
    #[serde(default)]
    pub correlation_source_track: Option<i32>,
    #[serde(default)]
    pub use_source_separation: Option<bool>,
}

// ─── Stepping Correction Types ───────────────────────────────────────────────

/// Stepping correction metadata for a single track — `SegmentFlagsEntry`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentFlagsEntry {
    /// Base delay from correlation analysis (always present)
    pub base_delay: i32,
    #[serde(default)]
    pub cluster_details: Vec<serde_json::Value>,
    #[serde(default)]
    pub valid_clusters: HashMap<i32, Vec<i32>>,
    #[serde(default)]
    pub invalid_clusters: HashMap<i32, Vec<i32>>,
    #[serde(default)]
    pub validation_results: HashMap<i32, serde_json::Value>,
    #[serde(default)]
    pub correction_mode: Option<String>,
    #[serde(default)]
    pub fallback_mode: Option<String>,
    #[serde(default)]
    pub subs_only: Option<bool>,
    #[serde(default)]
    pub stepping_data_path: Option<String>,
    #[serde(default)]
    pub audit_metadata: Option<Vec<serde_json::Value>>,
}

// ─── Drift Correction Types ─────────────────────────────────────────────────

/// Drift correction metadata — `DriftFlagsEntry`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftFlagsEntry {
    #[serde(default)]
    pub rate: Option<f64>,
}

// ─── Quality Issue Types ─────────────────────────────────────────────────────

/// Details specific to each quality issue type — `QualityIssueDetails`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityIssueDetails {
    #[serde(default)]
    pub segment_index: Option<i32>,
    #[serde(default)]
    pub expected_silence_at: Option<f64>,
    #[serde(default)]
    pub actual_content: Option<String>,
    #[serde(default)]
    pub threshold_exceeded_by: Option<f64>,
}

/// A quality issue detected during stepping correction — `SteppingQualityIssue`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteppingQualityIssue {
    pub source: String,
    pub issue_type: String,
    pub severity: String,
    pub message: String,
    pub details: QualityIssueDetails,
}

// ─── Sync Stability Types ────────────────────────────────────────────────────

/// A correlation chunk identified as an outlier — `OutlierChunk`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutlierChunk {
    #[serde(default)]
    pub chunk_index: Option<i32>,
    #[serde(default)]
    pub time_s: Option<f64>,
    #[serde(default)]
    pub delay_ms: Option<f64>,
    #[serde(default)]
    pub deviation_ms: Option<f64>,
    #[serde(default)]
    pub cluster_id: Option<i32>,
}

/// A cluster with internal variance issues — `ClusterIssue`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterIssue {
    #[serde(default)]
    pub cluster_id: Option<i32>,
    #[serde(default)]
    pub mean_delay: Option<f64>,
    #[serde(default)]
    pub variance: Option<f64>,
    #[serde(default)]
    pub outlier_count: Option<i32>,
}

/// Sync stability data for a source — `SyncStabilityIssue`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncStabilityIssue {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub variance_detected: Option<bool>,
    #[serde(default)]
    pub max_variance_ms: Option<f64>,
    #[serde(default)]
    pub std_dev_ms: Option<f64>,
    #[serde(default)]
    pub mean_delay_ms: Option<f64>,
    #[serde(default)]
    pub min_delay_ms: Option<f64>,
    #[serde(default)]
    pub max_delay_ms: Option<f64>,
    #[serde(default)]
    pub chunk_count: Option<i32>,
    #[serde(default)]
    pub outlier_count: Option<i32>,
    #[serde(default)]
    pub outliers: Vec<OutlierChunk>,
    #[serde(default)]
    pub cluster_count: Option<i32>,
    #[serde(default)]
    pub is_stepping: Option<bool>,
    #[serde(default)]
    pub cluster_issues: Vec<ClusterIssue>,
    #[serde(default)]
    pub reason: Option<String>,
}

// ─── PlanItem Style Types ────────────────────────────────────────────────────

/// ASS subtitle style attributes that can be patched — `ASSStyleAttributes`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ASSStyleAttributes {
    #[serde(default)]
    pub fontname: Option<String>,
    #[serde(default)]
    pub fontsize: Option<f64>,
    #[serde(default)]
    pub primary_color: Option<String>,
    #[serde(default)]
    pub secondary_color: Option<String>,
    #[serde(default)]
    pub outline_color: Option<String>,
    #[serde(default)]
    pub back_color: Option<String>,
    #[serde(default)]
    pub bold: Option<i32>,
    #[serde(default)]
    pub italic: Option<i32>,
    #[serde(default)]
    pub underline: Option<i32>,
    #[serde(default)]
    pub strikeout: Option<i32>,
    #[serde(default)]
    pub scale_x: Option<f64>,
    #[serde(default)]
    pub scale_y: Option<f64>,
    #[serde(default)]
    pub spacing: Option<f64>,
    #[serde(default)]
    pub angle: Option<f64>,
    #[serde(default)]
    pub border_style: Option<i32>,
    #[serde(default)]
    pub outline: Option<f64>,
    #[serde(default)]
    pub shadow: Option<f64>,
    #[serde(default)]
    pub alignment: Option<i32>,
    #[serde(default)]
    pub margin_l: Option<i32>,
    #[serde(default)]
    pub margin_r: Option<i32>,
    #[serde(default)]
    pub margin_v: Option<i32>,
    #[serde(default)]
    pub encoding: Option<i32>,
}

/// StylePatch maps style names to their attribute overrides.
pub type StylePatch = HashMap<String, ASSStyleAttributes>;

/// Font replacement configuration for a single style — `FontReplacement`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FontReplacement {
    #[serde(default)]
    pub original_font: Option<String>,
    #[serde(default)]
    pub new_font_name: Option<String>,
    #[serde(default)]
    pub font_file_path: Option<String>,
}

/// FontReplacements maps style names to their font replacement config.
pub type FontReplacements = HashMap<String, FontReplacement>;

// ─── PlanItem Filter Types ───────────────────────────────────────────────────

/// Configuration for style-based track filtering — `FilterConfig`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterConfig {
    #[serde(default)]
    pub filter_mode: Option<String>,
    #[serde(default)]
    pub filter_styles: Vec<String>,
    #[serde(default)]
    pub forced_include: Vec<i32>,
    #[serde(default)]
    pub forced_exclude: Vec<i32>,
}

// ─── Video Verified Types ────────────────────────────────────────────────────

/// Result from video-verified subtitle sync — `VideoVerifiedResult`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoVerifiedResult {
    #[serde(default)]
    pub original_delay_ms: Option<f64>,
    #[serde(default)]
    pub corrected_delay_ms: Option<f64>,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}
