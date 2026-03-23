//! Unified subtitle processing system — 1:1 port of `vsg_core/subtitles/`.
//!
//! Provides:
//! - SubtitleData: Universal container for all subtitle formats
//! - SubtitleEditPlan: Non-destructive edit plan system
//! - Parsers for ASS, SRT formats
//! - Writers for ASS, SRT formats
//! - Operations: sync, stepping, style modifications
//! - Frame utilities for video-verified sync
//! - OCR system (VobSub parser, pipeline, model backends stubbed)

pub mod data;
pub mod parsers;
pub mod writers;

pub mod checkpoint_selection;
pub mod diagnostics;
pub mod edit_plan;
#[allow(unused_imports, unused_variables, dead_code, clippy::all)]
pub mod frame_utils;
#[allow(unused_imports, unused_variables, dead_code, clippy::all)]
pub mod ocr;
pub mod operations;
pub mod style_engine;
pub mod sync_dispatcher;
#[allow(unused_imports, unused_variables, dead_code, clippy::all)]
pub mod sync_mode_plugins;
pub mod sync_modes;
pub mod sync_utils;
pub mod track_processor;
