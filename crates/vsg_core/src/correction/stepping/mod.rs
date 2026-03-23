//! Stepping correction package — 1:1 port of `vsg_core/correction/stepping/__init__.py`.
//!
//! Decomposes stepped delay correction into focused modules:
//! - `types`: Structs (AudioSegment, SplicePoint, SilenceZone, etc.)
//! - `timeline`: Reference <-> Source 2 timeline conversion
//! - `data_io`: Dense analysis data serialization (JSON in temp folder)
//! - `edl_builder`: Build transition zones from dense cluster data
//! - `boundary_refiner`: Silence detection (RMS + VAD), video snap
//! - `audio_assembly`: FFmpeg segment extraction, drift correction, concat
//! - `qa_check`: Post-correction quality verification
//! - `run`: Entry point (run_stepping_correction, apply_plan_to_file)

pub mod audio_assembly;
pub mod boundary_refiner;
pub mod data_io;
pub mod edl_builder;
pub mod qa_check;
pub mod run;
pub mod timeline;
pub mod types;

pub use run::{apply_plan_to_file, run_stepping_correction};
pub use types::AudioSegment;
