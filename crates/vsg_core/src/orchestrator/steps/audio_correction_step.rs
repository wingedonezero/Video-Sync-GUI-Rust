//! Audio correction step — 1:1 port of `orchestrator/steps/audio_correction_step.py`.
//!
//! Routes audio correction based on diagnosis from AnalysisStep.

use crate::correction::linear::run_linear_correction;
use crate::correction::pal::run_pal_correction;
use crate::correction::stepping::run::run_stepping_correction;
use crate::io::runner::CommandRunner;
use crate::models::enums::TrackType;

use super::context::Context;

/// Routes audio correction based on diagnosis — `AudioCorrectionStep`
pub struct AudioCorrectionStep;

impl AudioCorrectionStep {
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge || !ctx.settings.stepping_enabled {
            return Ok(());
        }

        if !ctx.pal_drift_flags.is_empty() {
            runner.log_message("--- PAL Drift Audio Correction Phase ---");
            run_pal_correction(ctx, runner);
            self.validate_pal_correction(ctx, runner)?;
        } else if !ctx.linear_drift_flags.is_empty() {
            runner.log_message("--- Linear Drift Audio Correction Phase ---");
            run_linear_correction(ctx, runner);
            self.validate_linear_correction(ctx, runner)?;
        } else if !ctx.segment_flags.is_empty() {
            runner.log_message("--- Segmented (Stepping) Audio Correction Phase ---");
            run_stepping_correction(ctx, runner);
            self.validate_stepping_correction(ctx, runner)?;
        }

        Ok(())
    }

    fn validate_pal_correction(&self, ctx: &Context, runner: &CommandRunner) -> Result<(), String> {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return Ok(()),
        };
        for analysis_key in ctx.pal_drift_flags.keys() {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            let audio_tracks: Vec<_> = items.iter()
                .filter(|item| item.track.source == source_key && item.track.track_type == TrackType::Audio && !item.is_preserved)
                .collect();

            if audio_tracks.is_empty() {
                runner.log_message(&format!(
                    "[Validation] PAL correction skipped for {source_key}: No audio tracks in layout."
                ));
                continue;
            }

            let corrected: Vec<_> = audio_tracks.iter().filter(|item| item.is_corrected).collect();
            if corrected.is_empty() {
                return Err(format!("PAL correction failed for {source_key}: No corrected track created."));
            }

            for item in corrected {
                if let Some(ref path) = item.extracted_path {
                    if !path.exists() {
                        return Err(format!("PAL correction failed for {source_key}: File not created at {}", path.display()));
                    }
                }
                runner.log_message(&format!(
                    "[Validation] PAL correction verified for {source_key}: {}",
                    item.extracted_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_default()
                ));
            }
        }
        Ok(())
    }

    fn validate_linear_correction(&self, ctx: &Context, runner: &CommandRunner) -> Result<(), String> {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return Ok(()),
        };
        for analysis_key in ctx.linear_drift_flags.keys() {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            let audio_tracks: Vec<_> = items.iter()
                .filter(|item| item.track.source == source_key && item.track.track_type == TrackType::Audio && !item.is_preserved)
                .collect();

            if audio_tracks.is_empty() {
                runner.log_message(&format!(
                    "[Validation] Linear drift correction skipped for {source_key}: No audio tracks in layout."
                ));
                continue;
            }

            let corrected: Vec<_> = audio_tracks.iter().filter(|item| item.is_corrected).collect();
            if corrected.is_empty() {
                return Err(format!("Linear drift correction failed for {source_key}: No corrected track created."));
            }

            for item in corrected {
                if let Some(ref path) = item.extracted_path {
                    if !path.exists() {
                        return Err(format!("Linear drift correction failed for {source_key}: File not created at {}", path.display()));
                    }
                }
                runner.log_message(&format!(
                    "[Validation] Linear drift correction verified for {source_key}: {}",
                    item.extracted_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_default()
                ));
            }
        }
        Ok(())
    }

    fn validate_stepping_correction(&self, ctx: &Context, runner: &CommandRunner) -> Result<(), String> {
        let items = match &ctx.extracted_items {
            Some(items) => items,
            None => return Ok(()),
        };
        for analysis_key in ctx.segment_flags.keys() {
            let source_key = analysis_key.split('_').next().unwrap_or("");
            let audio_tracks: Vec<_> = items.iter()
                .filter(|item| item.track.source == source_key && item.track.track_type == TrackType::Audio && !item.is_preserved)
                .collect();

            if audio_tracks.is_empty() {
                runner.log_message(&format!(
                    "[Validation] Stepping correction skipped for {source_key}: No audio tracks in layout."
                ));
                continue;
            }

            let corrected: Vec<_> = audio_tracks.iter().filter(|item| item.is_corrected).collect();
            if corrected.is_empty() {
                // Not necessarily an error — corrector may determine no stepping after detailed analysis
                runner.log_message(&format!(
                    "[Validation] No corrected tracks found for {source_key}. \
                     Expected if corrector determined no stepping exists after detailed analysis."
                ));
                runner.log_message(
                    "[Validation] The globally-shifted delay from initial analysis will be used."
                );
                continue;
            }

            for item in corrected {
                if let Some(ref path) = item.extracted_path {
                    if !path.exists() {
                        return Err(format!("Stepping correction failed for {source_key}: File not created at {}", path.display()));
                    }
                }
                runner.log_message(&format!(
                    "[Validation] Stepping correction verified for {source_key}: {}",
                    item.extracted_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_default()
                ));
            }
        }
        Ok(())
    }
}
