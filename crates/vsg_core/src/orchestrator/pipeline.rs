//! Orchestrator pipeline — 1:1 port of `vsg_core/orchestrator/pipeline.py`.
//!
//! Runs the modular steps in order with validation at each stage.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::context_types::ManualLayoutItem;
use crate::models::settings::AppSettings;

use super::steps::analysis_step::AnalysisStep;
use super::steps::attachments_step::AttachmentsStep;
use super::steps::audio_correction_step::AudioCorrectionStep;
use super::steps::chapters_step::ChaptersStep;
use super::steps::context::Context;
use super::steps::extract_step::ExtractStep;
use super::steps::mux_step::MuxStep;
use super::steps::subtitles_step::SubtitlesStep;
use super::validation::StepValidator;

/// Runs the modular steps in order with validation — `Orchestrator`
pub struct Orchestrator;

impl Orchestrator {
    /// Executes the pipeline steps with validation — `run()`
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        settings: &AppSettings,
        tool_paths: &HashMap<String, String>,
        log: Box<dyn Fn(&str) + Send + Sync>,
        progress: Box<dyn Fn(f64) + Send + Sync>,
        sources: &HashMap<String, String>,
        and_merge: bool,
        output_dir: &str,
        manual_layout: Vec<ManualLayoutItem>,
        attachment_sources: Vec<String>,
        source_settings: HashMap<String, serde_json::Value>,
    ) -> Result<Context, String> {
        let source1_file = sources
            .get("Source 1")
            .ok_or("Job is missing Source 1 (Reference).")?;

        let base_temp = if !settings.temp_root.is_empty() {
            PathBuf::from(&settings.temp_root)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("temp_work")
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem = Path::new(source1_file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let job_temp = base_temp.join(format!("orch_{stem}_{timestamp}"));
        let _ = std::fs::create_dir_all(&job_temp);

        let mut ctx = Context::new(
            settings.clone(),
            tool_paths.clone(),
            log,
            progress,
            output_dir.to_string(),
            job_temp,
            sources.clone(),
            and_merge,
            manual_layout,
            attachment_sources,
            source_settings,
        );

        // Helper: create a runner from current settings
        // Note: Steps also use ctx.log directly for important messages
        let make_runner = |settings: &AppSettings| -> crate::io::runner::CommandRunner {
            crate::io::runner::CommandRunner::new(
                settings.clone(),
                Box::new(|_msg: &str| {}),
            )
        };

        // --- Analysis Phase ---
        (ctx.log)("--- Analysis Phase ---");
        (ctx.progress)(0.10);
        {
            let runner = make_runner(&ctx.settings);
            AnalysisStep.run(&mut ctx, &runner)
                .map_err(|e| format!("Analysis phase failed: {e}"))?;
        }
        StepValidator::validate_analysis(&ctx)
            .map_err(|e| format!("Analysis validation failed: {e}"))?;
        (ctx.log)("[Validation] Analysis phase validated successfully.");

        if !and_merge {
            (ctx.log)("--- Analysis Complete (No Merge) ---");
            (ctx.progress)(1.0);
            return Ok(ctx);
        }

        // --- Extraction Phase ---
        (ctx.log)("--- Extraction Phase ---");
        (ctx.progress)(0.40);
        {
            let runner = make_runner(&ctx.settings);
            ExtractStep.run(&mut ctx, &runner)
                .map_err(|e| format!("Extraction phase failed: {e}"))?;
        }
        StepValidator::validate_extraction(&ctx)
            .map_err(|e| format!("Extraction validation failed: {e}"))?;
        (ctx.log)("[Validation] Extraction phase validated successfully.");

        // --- Audio Correction Phase (conditional) ---
        if ctx.settings.stepping_enabled
            && (!ctx.segment_flags.is_empty()
                || !ctx.pal_drift_flags.is_empty()
                || !ctx.linear_drift_flags.is_empty())
        {
            (ctx.log)("--- Advanced Audio Correction Phase ---");
            (ctx.progress)(0.50);
            {
                let runner = make_runner(&ctx.settings);
                AudioCorrectionStep.run(&mut ctx, &runner)
                    .map_err(|e| format!("Audio correction phase failed: {e}"))?;
            }
            StepValidator::validate_correction(&ctx)
                .map_err(|e| format!("Audio correction validation failed: {e}"))?;
            (ctx.log)("[Validation] Audio correction phase validated successfully.");
        }

        // --- Subtitle Processing Phase ---
        (ctx.log)("--- Subtitle Processing Phase ---");
        {
            let runner = make_runner(&ctx.settings);
            SubtitlesStep.run(&mut ctx, &runner)
                .map_err(|e| format!("Subtitle processing phase failed: {e}"))?;
        }
        StepValidator::validate_subtitles(&ctx)
            .map_err(|e| format!("Subtitle processing validation failed: {e}"))?;
        (ctx.log)("[Validation] Subtitle processing phase validated successfully.");

        // --- Chapters Phase (non-fatal) ---
        (ctx.log)("--- Chapters Phase ---");
        {
            let runner = make_runner(&ctx.settings);
            if let Err(e) = ChaptersStep.run(&mut ctx, &runner) {
                (ctx.log)(&format!("[WARNING] Chapters phase had issues (non-fatal): {e}"));
            } else {
                (ctx.log)("[Validation] Chapters phase completed.");
            }
        }

        // --- Attachments Phase (non-fatal) ---
        (ctx.log)("--- Attachments Phase ---");
        (ctx.progress)(0.60);
        {
            let runner = make_runner(&ctx.settings);
            if let Err(e) = AttachmentsStep.run(&mut ctx, &runner) {
                (ctx.log)(&format!("[WARNING] Attachments phase had issues (non-fatal): {e}"));
            } else {
                (ctx.log)("[Validation] Attachments phase completed.");
            }
        }

        // --- Merge Planning Phase ---
        (ctx.log)("--- Merge Planning Phase ---");
        (ctx.progress)(0.75);
        {
            let runner = make_runner(&ctx.settings);
            MuxStep.run(&mut ctx, &runner)
                .map_err(|e| format!("Merge planning phase failed: {e}"))?;
        }
        StepValidator::validate_mux(&ctx)
            .map_err(|e| format!("Merge planning validation failed: {e}"))?;
        (ctx.log)("[Validation] Merge planning phase validated successfully.");

        (ctx.progress)(0.80);

        Ok(ctx)
    }
}
