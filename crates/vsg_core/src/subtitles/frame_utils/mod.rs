//! Shared frame timing and video utility functions for subtitle synchronization.
//!
//! This package has been modularized for better maintainability:
//! - timing: Frame/time conversion functions (CFR and VFR support)
//! - video_properties: Video property detection (FPS, interlacing, resolution)
//! - video_reader: Multi-backend video reader (FFmpeg pipe + opencv)
//! - frame_hashing: Perceptual hash and frame comparison functions
//! - frame_audit: Frame alignment audit (centisecond rounding drift)
//! - surgical_rounding: Surgical frame-aware rounding (floor->ceil when needed)
//! - visual_verify: Visual frame verification (SSIM-based)

pub mod frame_audit;
pub mod frame_hashing;
pub mod surgical_rounding;
pub mod timing;
pub mod video_properties;
pub mod video_reader;
pub mod visual_verify;

// Re-exports for backwards compatibility
pub use frame_audit::{run_frame_audit, write_audit_report, FrameAuditIssue, FrameAuditResult};
pub use frame_hashing::{
    compare_frames, compare_frames_multi, compute_frame_hash, compute_hamming_distance,
    compute_mse, compute_perceptual_hash, compute_ssim, MultiMetricResult,
};
pub use surgical_rounding::{
    surgical_round_batch, surgical_round_event, surgical_round_single, SurgicalBatchStats,
    SurgicalEventResult, SurgicalRoundResult,
};
pub use timing::{
    clear_vfr_cache, frame_to_time_aegisub, frame_to_time_floor, frame_to_time_middle,
    get_vfr_timestamps, time_to_frame_aegisub, time_to_frame_floor, time_to_frame_middle,
};
pub use video_properties::{
    compare_video_properties, detect_video_fps, detect_video_properties, get_video_duration_ms,
    get_video_properties,
};
pub use video_reader::VideoReader;
pub use visual_verify::{
    run_visual_verify, write_visual_verify_report, CreditsInfo, RegionStats, SampleResult,
    VisualVerifyResult,
};
