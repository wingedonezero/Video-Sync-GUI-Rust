//! Analysis-specific result types — 1:1 port of `vsg_core/analysis/types.py`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Result from correlating one audio chunk pair — `ChunkResult`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResult {
    /// Rounded delay in milliseconds
    pub delay_ms: i32,
    /// Precise float delay in milliseconds
    pub raw_delay_ms: f64,
    /// Match quality / confidence score (0-100)
    pub match_pct: f64,
    /// Chunk start position in seconds
    pub start_s: f64,
    /// True if match_pct >= threshold
    pub accepted: bool,
}

/// Result of audio track selection — `TrackSelection`
#[derive(Debug, Clone)]
pub struct TrackSelection {
    pub track_id: i32,
    pub track_index: i32,
    pub selected_by: String,
    pub language: String,
    pub codec: String,
    pub channels: i32,
    pub formatted_name: String,
}

/// Result of delay calculation — `DelayCalculation`
#[derive(Debug, Clone)]
pub struct DelayCalculation {
    pub rounded_ms: i32,
    pub raw_ms: f64,
    pub selection_method: String,
    pub accepted_windows: usize,
    pub total_windows: usize,
}

/// Container delay information for a source — `ContainerDelayInfo`
#[derive(Debug, Clone)]
pub struct ContainerDelayInfo {
    pub video_delay_ms: f64,
    pub audio_delays_ms: HashMap<i32, f64>,
    pub selected_audio_delay_ms: f64,
}

/// Global shift calculation result — `GlobalShiftCalculation`
#[derive(Debug, Clone)]
pub struct GlobalShiftCalculation {
    pub shift_ms: i32,
    pub raw_shift_ms: f64,
    pub most_negative_ms: i32,
    pub most_negative_raw_ms: f64,
    pub applied: bool,
}

// ─── Drift / Stepping Diagnosis ──────────────────────────────────────────────

/// Quality validation thresholds — `QualityThresholds`
#[derive(Debug, Clone)]
pub struct QualityThresholds {
    pub min_cluster_percentage: f64,
    pub min_cluster_duration_s: f64,
    pub min_match_quality_pct: f64,
    pub min_total_clusters: i32,
}

/// A single validation check result — `ValidationCheck`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheck {
    pub passed: bool,
    pub value: f64,
    pub threshold: f64,
    pub label: String,
}

/// Validation result for a single stepping cluster — `ClusterValidation`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterValidation {
    pub valid: bool,
    pub checks: HashMap<String, ValidationCheck>,
    pub passed_count: i32,
    pub total_checks: i32,
    pub cluster_size: usize,
    pub cluster_percentage: f64,
    pub cluster_duration_s: f64,
    pub avg_match_quality: f64,
    pub min_match_quality: f64,
    pub time_range: (f64, f64),
}

/// Detailed cluster composition — `ClusterDiagnostic`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterDiagnostic {
    pub cluster_id: i32,
    pub mean_delay_ms: f64,
    pub std_delay_ms: f64,
    pub chunk_count: usize,
    pub chunk_numbers: Vec<i32>,
    pub raw_delays: Vec<f64>,
    pub time_range: (f64, f64),
    pub mean_match_pct: f64,
    pub min_match_pct: f64,
}

/// Diagnosis result — union of outcomes from `diagnose_audio_issue`
#[derive(Debug, Clone)]
pub enum DiagnosisResult {
    /// No drift or stepping detected
    Uniform,
    /// Linear or PAL drift detected
    Drift {
        diagnosis: String,
        rate: f64,
    },
    /// Stepped delay clusters detected
    Stepping {
        cluster_count: usize,
        cluster_details: Vec<ClusterDiagnostic>,
        valid_clusters: HashMap<i32, Vec<i32>>,
        invalid_clusters: HashMap<i32, Vec<i32>>,
        validation_results: HashMap<i32, ClusterValidation>,
        correction_mode: String,
        fallback_mode: Option<String>,
    },
}
