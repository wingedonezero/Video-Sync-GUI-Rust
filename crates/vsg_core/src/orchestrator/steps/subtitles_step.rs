//! Subtitles step — stub for `vsg_core/orchestrator/steps/subtitles_step.py`.
//!
//! Full implementation requires the subtitles/ module.
//! Will be completed when subtitles/ is ported.

use crate::io::runner::CommandRunner;

use super::context::Context;

/// Unified subtitle processing step — `SubtitlesStep`
pub struct SubtitlesStep;

impl SubtitlesStep {
    /// Run the subtitles processing step.
    ///
    /// TODO: Port from subtitles_step.py when subtitles/ module is available.
    /// This step coordinates:
    /// - Video-verified preprocessing (once per source)
    /// - OCR processing
    /// - SubtitleData pipeline (sync, styles, format conversion)
    /// - Bypass mode for time-based sync
    pub fn run(&self, ctx: &mut Context, _runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge || ctx.extracted_items.is_none() {
            return Ok(());
        }

        (ctx.log)("[Subtitles] Subtitle processing not yet implemented in Rust port");
        (ctx.log)("[Subtitles] Subtitle tracks will pass through unchanged");
        Ok(())
    }
}
