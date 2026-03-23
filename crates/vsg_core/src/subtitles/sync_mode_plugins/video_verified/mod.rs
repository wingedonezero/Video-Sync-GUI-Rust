//! Video-Verified sync plugin package.
//!
//! Provides frame matching to find the TRUE video-to-video offset
//! for subtitle timing, addressing cases where audio correlation differs
//! from the actual video alignment.
//!
//! Public API:
//!   - `calculate_video_verified_offset()`: Classic frame matching (phash/SSIM/MSE)
//!   - `calculate_neural_verified_offset()`: Neural feature matching (ISC model)
//!   - `VideoVerifiedSync`: SyncPlugin implementation for the subtitle pipeline

pub mod candidates;
pub mod isc_model;
pub mod matcher;
pub mod neural_matcher;
pub mod neural_subprocess;
pub mod offset;
pub mod plugin;
pub mod preprocessing;
pub mod quality;
pub mod verification;

pub use matcher::calculate_video_verified_offset;
pub use neural_matcher::calculate_neural_verified_offset;
pub use plugin::VideoVerifiedSync;
