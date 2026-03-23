//! Job worker runner — 1:1 port of `vsg_qt/worker/runner.py`.
//!
//! Runs sync jobs in a background thread, emitting signals back to the
//! UI thread. In Python this was a QRunnable; in Rust we use a plain
//! thread (spawned via std::thread or tokio::task::spawn_blocking) and
//! communicate back through the WorkerSignals QObject.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use vsg_core::models::jobs::PipelineResult;
use vsg_core::models::settings::AppSettings;
use vsg_core::pipeline::JobPipeline;

/// Holds the data needed to run a batch of jobs.
/// This is the Rust equivalent of JobWorker.__init__ parameters.
pub struct JobRunnerConfig {
    pub settings: AppSettings,
    pub jobs: Vec<HashMap<String, serde_json::Value>>,
    pub and_merge: bool,
    pub output_dir: String,
    pub cancelled: Arc<AtomicBool>,
}

/// Safe signal emission callbacks — 1:1 port of `_safe_log`, `_safe_progress`, etc.
///
/// In Python these guarded against RuntimeError when signals were deleted
/// during GUI shutdown. In Rust we use closures that capture weak references
/// or simply ignore send failures.
pub struct SignalCallbacks {
    pub log: Box<dyn Fn(&str) + Send + Sync>,
    pub progress: Box<dyn Fn(f64) + Send + Sync>,
    pub status: Box<dyn Fn(&str) + Send + Sync>,
    pub finished_job: Box<dyn Fn(&str) + Send + Sync>,
    pub finished_all: Box<dyn Fn(&str) + Send + Sync>,
}

/// Run the job batch — 1:1 port of `JobWorker.run()`.
///
/// This function is meant to be called from a background thread.
/// It iterates through jobs, calls `JobPipeline.run_job()` for each,
/// and emits progress/status/log signals via the callbacks.
pub fn run_job_batch(config: JobRunnerConfig, signals: SignalCallbacks) {
    let log_cb = Arc::new(signals.log);
    let progress_cb = Arc::new(signals.progress);

    let mut pipeline = JobPipeline::new(
        config.settings.clone(),
        {
            let cb = Arc::clone(&log_cb);
            Box::new(move |msg: &str| cb(msg))
        },
        {
            let cb = Arc::clone(&progress_cb);
            Box::new(move |pct: f64| cb(pct))
        },
    );

    let mut all_results: Vec<serde_json::Value> = Vec::new();
    let total_jobs = config.jobs.len();

    for (i, job_data) in config.jobs.iter().enumerate() {
        // Check for cancellation — 1:1 with `if self.cancelled: break`
        if config.cancelled.load(Ordering::Relaxed) {
            (log_cb)(&format!(
                "[WORKER] Cancelled by user, stopping at job {}/{}",
                i + 1,
                total_jobs
            ));
            break;
        }

        let sources = extract_sources(job_data);
        let source1_file = match sources.get("Source 1") {
            Some(f) if !f.is_empty() => f.clone(),
            _ => {
                (log_cb)(&format!(
                    "[FATAL WORKER ERROR] Job {} is missing 'Source 1'. Skipping.",
                    i + 1
                ));
                continue;
            }
        };

        let source1_name = Path::new(&source1_file)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        (signals.status)(&format!(
            "Processing {}/{}: {}",
            i + 1,
            total_jobs,
            source1_name
        ));

        // Extract optional fields from job_data
        let manual_layout = extract_manual_layout(job_data);
        let attachment_sources = extract_string_array(job_data, "attachment_sources");
        let source_settings = extract_source_settings(job_data);

        // Wrap pipeline.run_job in catch_unwind for panic safety
        // (1:1 with Python's try/except around pipeline.run_job)
        let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline.run_job(
                &sources,
                config.and_merge,
                &config.output_dir,
                manual_layout,
                attachment_sources,
                source_settings,
            )
        }));

        let result_json = match run_result {
            Ok(pipeline_result) => pipeline_result_to_json(&pipeline_result, job_data),
            Err(panic_info) => {
                let panic_msg = panic_info
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("Unknown panic");

                (log_cb)(&format!(
                    "[FATAL WORKER ERROR] Job {}/{} panicked: {}",
                    i + 1,
                    total_jobs,
                    panic_msg
                ));

                // Build an error result matching Python's except block
                serde_json::json!({
                    "status": "Failed",
                    "error": format!("Pipeline panic: {}", panic_msg),
                    "name": source1_name,
                    "job_data_for_batch_check": job_data,
                })
            }
        };

        let result_str = serde_json::to_string(&result_json).unwrap_or_default();
        (signals.finished_job)(&result_str);
        all_results.push(result_json);
    }

    let all_str = serde_json::to_string(&all_results).unwrap_or_default();
    (signals.finished_all)(&all_str);
}

// ── Helper functions ──

fn extract_sources(job_data: &HashMap<String, serde_json::Value>) -> HashMap<String, String> {
    job_data
        .get("sources")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_manual_layout(
    job_data: &HashMap<String, serde_json::Value>,
) -> Option<Vec<vsg_core::models::context_types::ManualLayoutItem>> {
    job_data
        .get("manual_layout")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn extract_string_array(
    job_data: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<Vec<String>> {
    job_data.get(key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
    })
}

fn extract_source_settings(
    job_data: &HashMap<String, serde_json::Value>,
) -> Option<HashMap<String, serde_json::Value>> {
    job_data
        .get("source_settings")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn pipeline_result_to_json(
    result: &PipelineResult,
    job_data: &HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    let mut json = serde_json::to_value(result).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            "job_data_for_batch_check".to_string(),
            serde_json::to_value(job_data).unwrap_or(serde_json::Value::Null),
        );
    }
    json
}
