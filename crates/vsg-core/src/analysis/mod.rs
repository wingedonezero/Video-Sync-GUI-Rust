//! Audio analysis entry point(s).
pub mod xcorr;
pub use xcorr::{AnalyzeParams, ChunkResult, AnalyzeResult, analyze_audio_offsets};
