//! Stepping correction types — 1:1 port of `vsg_core/correction/stepping/types.py`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::analysis::types::{ChunkResult, ClusterDiagnostic};

// ---------------------------------------------------------------------------
// Core EDL type (kept mutable for drift_rate_ms_s update)
// ---------------------------------------------------------------------------

/// Represents an action point on the target timeline for assembly — `AudioSegment`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSegment {
    pub start_s: f64,
    pub end_s: f64,
    pub delay_ms: i32,
    #[serde(default)]
    pub delay_raw: f64,
    #[serde(default)]
    pub drift_rate_ms_s: f64,
}

impl PartialEq for AudioSegment {
    fn eq(&self, other: &Self) -> bool {
        self.start_s == other.start_s
            && self.end_s == other.end_s
            && self.delay_ms == other.delay_ms
    }
}

impl Eq for AudioSegment {}

impl std::hash::Hash for AudioSegment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.start_s.to_bits().hash(state);
        self.end_s.to_bits().hash(state);
        self.delay_ms.hash(state);
    }
}

// ---------------------------------------------------------------------------
// Dense analysis data loaded from temp folder
// ---------------------------------------------------------------------------

/// Dense analysis data loaded from temp folder JSON — `SteppingData`
#[derive(Debug, Clone)]
pub struct SteppingData {
    pub source_key: String,
    pub track_id: i32,
    pub windows: Vec<ChunkResult>,
    pub clusters: Vec<ClusterDiagnostic>,
}

// ---------------------------------------------------------------------------
// Transition / splice types
// ---------------------------------------------------------------------------

/// Region where delay changes — needs boundary refinement — `TransitionZone`
#[derive(Debug, Clone)]
pub struct TransitionZone {
    /// End of cluster A's last window
    pub ref_start_s: f64,
    /// Start of cluster B's first window
    pub ref_end_s: f64,
    pub delay_before_ms: f64,
    pub delay_after_ms: f64,
    /// delay_after - delay_before
    pub correction_ms: f64,
}

/// A detected silent region in audio — `SilenceZone`
#[derive(Debug, Clone)]
pub struct SilenceZone {
    pub start_s: f64,
    pub end_s: f64,
    pub center_s: f64,
    pub avg_db: f64,
    pub duration_ms: f64,
    /// "rms", "vad", or "combined"
    pub source: String,
}

/// Rich result from silence zone selection for audit trail — `BoundaryResult`
#[derive(Debug, Clone)]
pub struct BoundaryResult {
    /// Best zone, or None if nothing found
    pub zone: Option<SilenceZone>,
    /// Composite score from _pick_best_zone (0 if no zone)
    pub score: f64,
    /// True if transients detected near the chosen zone
    pub near_transient: bool,
    /// True if zone is RMS-only (VAD detected speech there)
    pub overlaps_speech: bool,
}

/// Precise splice location for a single transition — `SplicePoint`
#[derive(Debug, Clone)]
pub struct SplicePoint {
    /// Position in reference (Source 1) timeline
    pub ref_time_s: f64,
    /// Position in target (Source 2) timeline
    pub src2_time_s: f64,
    pub delay_before_ms: f64,
    pub delay_after_ms: f64,
    /// delay_after - delay_before
    pub correction_ms: f64,
    /// Best silence zone for splice
    pub silence_zone: Option<SilenceZone>,
    /// Rich audit data
    pub boundary_result: Option<BoundaryResult>,
    /// Video snap metadata
    pub snap_metadata: HashMap<String, serde_json::Value>,
}
