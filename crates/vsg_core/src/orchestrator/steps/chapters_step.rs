//! Chapters step — 1:1 port of `vsg_core/orchestrator/steps/chapters_step.py`.

use std::collections::HashMap;

use crate::chapters::process::process_chapters;
use crate::io::runner::CommandRunner;

use super::context::Context;

/// Extracts/modifies chapter XML from Source 1 — `ChaptersStep`
pub struct ChaptersStep;

impl ChaptersStep {
    /// Run the chapters step.
    pub fn run(&self, ctx: &mut Context, runner: &CommandRunner) -> Result<(), String> {
        if !ctx.and_merge {
            ctx.chapters_xml = None;
            return Ok(());
        }

        let source1_file = match ctx.sources.get("Source 1") {
            Some(f) => f.clone(),
            None => {
                runner.log_message("[WARN] No Source 1 file found for chapter processing.");
                runner.log_message("[INFO] Chapters will be omitted from the final file.");
                ctx.chapters_xml = None;
                return Ok(());
            }
        };

        // CRITICAL: Chapters must be shifted by the SAME amount as video container delay
        let shift_ms = ctx.delays.as_ref().map(|d| d.global_shift_ms).unwrap_or(0);

        if shift_ms != 0 {
            runner.log_message(&format!(
                "[Chapters] Applying global shift of +{shift_ms}ms to chapter timestamps"
            ));
            runner.log_message(
                "[Chapters] This matches the video container delay for correct keyframe alignment",
            );
        } else {
            runner.log_message("[Chapters] No global shift needed for chapters");
        }

        // Convert tool_paths for the function signature
        let tool_paths: HashMap<String, String> = ctx.tool_paths.clone();

        match process_chapters(
            &source1_file,
            &ctx.temp_dir,
            runner,
            &tool_paths,
            &ctx.settings,
            shift_ms,
        ) {
            Some(xml_path) => {
                runner.log_message(&format!(
                    "[Chapters] Successfully processed chapters: {xml_path}"
                ));
                ctx.chapters_xml = Some(xml_path);
            }
            None => {
                runner.log_message("[Chapters] No chapters found in source file");
                ctx.chapters_xml = None;
            }
        }

        Ok(())
    }
}
