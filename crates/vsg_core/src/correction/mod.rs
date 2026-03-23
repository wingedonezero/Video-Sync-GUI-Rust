//! Audio correction module — 1:1 port of `vsg_core/correction/__init__.py`.
//!
//! Three correction strategies:
//! - **Linear**: constant drift correction via ffmpeg resampling
//! - **PAL**: PAL speed correction via rubberband tempo adjustment
//! - **Stepping**: segmented correction for stepped delay changes

pub mod linear;
pub mod pal;
pub mod stepping;

pub use linear::run_linear_correction;
pub use pal::run_pal_correction;
pub use stepping::{run_stepping_correction, apply_plan_to_file, AudioSegment};
