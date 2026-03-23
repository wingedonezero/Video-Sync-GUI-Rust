//! Job pipeline — 1:1 port of `vsg_core/pipeline.py`.
//!
//! Coordinates sync job execution using modular components.
//! This is the top-level entry point that the UI calls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::io::runner::CommandRunner;
use crate::models::context_types::ManualLayoutItem;
use crate::models::jobs::PipelineResult;
use crate::models::settings::AppSettings;
use crate::pipeline_components::log_manager::LogManager;
use crate::pipeline_components::output_writer::OutputWriter;
use crate::pipeline_components::sync_executor::SyncExecutor;
use crate::pipeline_components::tool_validator::ToolValidator;

/// Orchestrates video sync job execution — `JobPipeline`
pub struct JobPipeline {
    pub settings: AppSettings,
    gui_log_callback: Arc<dyn Fn(&str) + Send + Sync>,
    progress: Arc<dyn Fn(f64) + Send + Sync>,
    tool_paths: HashMap<String, String>,
}

impl JobPipeline {
    /// Create a new JobPipeline.
    pub fn new(
        config: AppSettings,
        log_callback: Box<dyn Fn(&str) + Send + Sync>,
        progress_callback: Box<dyn Fn(f64) + Send + Sync>,
    ) -> Self {
        Self {
            settings: config,
            gui_log_callback: Arc::from(log_callback),
            progress: Arc::from(progress_callback),
            tool_paths: HashMap::new(),
        }
    }

    /// Run a complete sync job — `run_job()`
    pub fn run_job(
        &mut self,
        sources: &HashMap<String, String>,
        and_merge: bool,
        output_dir_str: &str,
        manual_layout: Option<Vec<ManualLayoutItem>>,
        attachment_sources: Option<Vec<String>>,
        source_settings: Option<HashMap<String, serde_json::Value>>,
    ) -> PipelineResult {
        // --- 1. Input Validation ---
        let source1_file = match sources.get("Source 1") {
            Some(f) => f.clone(),
            None => {
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: String::new(),
                    error: Some("Job is missing Source 1 (Reference).".to_string()),
                    ..PipelineResult::empty()
                };
            }
        };

        let output_dir = PathBuf::from(output_dir_str);
        let _ = std::fs::create_dir_all(&output_dir);

        let job_name = Path::new(&source1_file)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let source1_name = Path::new(&source1_file)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // --- 2. Setup Logging ---
        let gui_cb = Arc::clone(&self.gui_log_callback);
        let (log_handle, log_to_all) = match LogManager::setup_job_log(
            &job_name,
            &output_dir,
            Arc::from(Box::new(move |msg: &str| gui_cb(msg)) as Box<dyn Fn(&str) + Send + Sync>),
        ) {
            Ok(pair) => pair,
            Err(e) => {
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: source1_name,
                    error: Some(format!("Failed to setup logging: {e}")),
                    ..PipelineResult::empty()
                };
            }
        };

        // --- 3. Validate Tools ---
        match ToolValidator::validate_tools() {
            Ok(paths) => self.tool_paths = paths,
            Err(e) => {
                log_to_all(&format!("[ERROR] {e}"));
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: source1_name,
                    error: Some(e),
                    ..PipelineResult::empty()
                };
            }
        }

        log_to_all(&format!("=== Starting Job: {source1_name} ==="));
        (self.progress)(0.0);

        // --- 4. Validate Merge Requirements ---
        if and_merge && manual_layout.is_none() {
            let err_msg = "Manual layout required for merge.";
            log_to_all(&format!("[ERROR] {err_msg}"));
            return PipelineResult {
                status: "Failed".to_string(),
                name: source1_name,
                error: Some(err_msg.to_string()),
                ..PipelineResult::empty()
            };
        }

        // --- 5. Plan Sync (via Orchestrator) ---
        let orch = crate::orchestrator::pipeline::Orchestrator;
        let progress = Arc::clone(&self.progress);

        let ctx_result = orch.run(
            &self.settings,
            &self.tool_paths,
            Box::new(move |msg: &str| log_to_all(msg)),
            Box::new(move |pct: f64| progress(pct)),
            sources,
            and_merge,
            output_dir_str,
            manual_layout.unwrap_or_default(),
            attachment_sources.unwrap_or_default(),
            source_settings.unwrap_or_default(),
        );

        let ctx = match ctx_result {
            Ok(c) => c,
            Err(e) => {
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: source1_name,
                    error: Some(e),
                    ..PipelineResult::empty()
                };
            }
        };

        // --- 6. Return Early if Analysis Only ---
        if !and_merge {
            (self.progress)(1.0);
            return PipelineResult {
                status: "Analyzed".to_string(),
                name: source1_name,
                delays: ctx.delays.as_ref().map(|d| d.source_delays_ms.clone()),
                stepping_sources: ctx.stepping_sources,
                stepping_detected_disabled: ctx.stepping_detected_disabled,
                stepping_detected_separated: ctx.stepping_detected_separated,
                sync_stability_issues: ctx.sync_stability_issues,
                ..PipelineResult::empty()
            };
        }

        // --- 7-12. Merge Execution ---
        let tokens = match ctx.tokens {
            Some(ref t) => t.clone(),
            None => {
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: source1_name,
                    error: Some("Internal error: mkvmerge tokens were not generated.".to_string()),
                    ..PipelineResult::empty()
                };
            }
        };

        let final_output_path = OutputWriter::prepare_output_path(
            &output_dir,
            &source1_name,
        );
        let temp_output_name = format!("temp_{source1_name}");
        let mkvmerge_output_path = ctx.temp_dir.join(&temp_output_name);

        let mut full_tokens = vec![
            "--output".to_string(),
            mkvmerge_output_path.to_string_lossy().to_string(),
        ];
        full_tokens.extend(tokens);

        let runner = CommandRunner::new(
            self.settings.clone(),
            Box::new(|_msg: &str| {}),
        );

        let opts_path = match OutputWriter::write_mkvmerge_options(
            &full_tokens,
            &ctx.temp_dir,
            &self.settings,
            &runner,
        ) {
            Ok(p) => p,
            Err(e) => {
                return PipelineResult {
                    status: "Failed".to_string(),
                    name: source1_name,
                    error: Some(e),
                    ..PipelineResult::empty()
                };
            }
        };

        if !SyncExecutor::execute_merge(&opts_path, &self.tool_paths, &runner) {
            return PipelineResult {
                status: "Failed".to_string(),
                name: source1_name,
                error: Some("mkvmerge execution failed.".to_string()),
                ..PipelineResult::empty()
            };
        }

        if let Err(e) = SyncExecutor::finalize_output(
            &mkvmerge_output_path,
            &final_output_path,
            &self.settings,
            &self.tool_paths,
            &runner,
        ) {
            return PipelineResult {
                status: "Failed".to_string(),
                name: source1_name,
                error: Some(e),
                ..PipelineResult::empty()
            };
        }

        // --- Cleanup ---
        if ctx.temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&ctx.temp_dir);
        }
        drop(log_handle);

        (self.progress)(1.0);
        PipelineResult {
            status: "Merged".to_string(),
            name: source1_name,
            output: Some(final_output_path.to_string_lossy().to_string()),
            delays: ctx.delays.as_ref().map(|d| d.source_delays_ms.clone()),
            stepping_sources: ctx.stepping_sources,
            stepping_detected_disabled: ctx.stepping_detected_disabled,
            stepping_detected_separated: ctx.stepping_detected_separated,
            stepping_quality_issues: ctx.stepping_quality_issues,
            sync_stability_issues: ctx.sync_stability_issues,
            ..PipelineResult::empty()
        }
    }
}

impl PipelineResult {
    /// Create an empty PipelineResult with defaults.
    pub fn empty() -> Self {
        Self {
            status: String::new(),
            name: String::new(),
            output: None,
            delays: None,
            error: None,
            issues: 0,
            stepping_sources: Vec::new(),
            stepping_detected_disabled: Vec::new(),
            stepping_detected_separated: Vec::new(),
            stepping_quality_issues: Vec::new(),
            sync_stability_issues: Vec::new(),
        }
    }
}
