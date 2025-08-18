// crates/vsg-core/src/analysis/mod.rs
pub mod ffmpeg_decode;
pub mod xcorr;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassDetail {
    pub index: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub inliers: usize,
    pub total_chunks: usize,
    pub confidence: f32,
    pub shift_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub method: String,
    pub sample_rate_hz: u32,
    pub chunk_ms: i64,
    pub hop_ms: i64,
    pub search_window_ms: i64,
    pub min_match: f32,
    pub mode: String,
    pub passes: Vec<PassDetail>,
    pub result: FinalResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalResult {
    pub global_shift_ms: i64,
    pub confidence: f32,
    pub passes_used: usize,
    pub total_passes: usize,
}

#[derive(Debug, Clone)]
pub struct AnalyzeParams {
    pub passes: usize,
    pub chunk_ms: i64,
    pub hop_ms: i64,
    pub max_shift_ms: i64,
    pub min_match: f32,
    pub sample_rate: u32,
}

impl Default for AnalyzeParams {
    fn default() -> Self {
        Self {
            passes: 10,
            chunk_ms: 15_000,
            hop_ms: 5_000,
            max_shift_ms: 3_000,
            min_match: 0.18,
            sample_rate: 48_000,
        }
    }
}
