//! Batch report writer — 1:1 port of `vsg_core/reporting/report_writer.py`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde_json::{json, Value};

/// Manages persistent batch reports — `ReportWriter`
pub struct ReportWriter {
    logs_folder: PathBuf,
    current_report_path: Option<PathBuf>,
    report_data: Value,
}

impl ReportWriter {
    const REPORT_VERSION: &'static str = "1.0";

    pub fn new(logs_folder: &Path) -> Self {
        let _ = fs::create_dir_all(logs_folder);
        Self {
            logs_folder: logs_folder.to_path_buf(),
            current_report_path: None,
            report_data: json!({}),
        }
    }

    /// Initialize a new report file — `create_report`
    pub fn create_report(
        &mut self,
        batch_name: &str,
        is_batch: bool,
        output_dir: &str,
        total_jobs: usize,
    ) -> PathBuf {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let safe_name = Self::sanitize_filename(batch_name);
        let filename = if is_batch {
            format!("{safe_name}_batch_report_{timestamp}.json")
        } else {
            format!("{safe_name}_report_{timestamp}.json")
        };

        let path = self.logs_folder.join(&filename);
        self.current_report_path = Some(path.clone());

        self.report_data = json!({
            "version": Self::REPORT_VERSION,
            "created_at": Local::now().to_rfc3339(),
            "finalized_at": null,
            "batch_name": batch_name,
            "is_batch": is_batch,
            "output_directory": output_dir,
            "total_jobs": total_jobs,
            "summary": { "successful": 0, "warnings": 0, "failed": 0, "total_issues": 0 },
            "jobs": [],
        });

        self.write_report();
        path
    }

    /// Add a completed job's results — `add_job`
    pub fn add_job(&mut self, job_result: &HashMap<String, Value>, job_index: usize) {
        if self.report_data.is_null() {
            return;
        }

        let entry = json!({
            "index": job_index,
            "name": job_result.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown"),
            "status": job_result.get("status").and_then(|v| v.as_str()).unwrap_or("Unknown"),
            "output_path": job_result.get("output"),
            "completed_at": Local::now().to_rfc3339(),
            "delays": job_result.get("delays").unwrap_or(&json!({})),
            "error": job_result.get("error"),
            "stepping": {
                "applied_to": job_result.get("stepping_sources").unwrap_or(&json!([])),
                "detected_disabled": job_result.get("stepping_detected_disabled").unwrap_or(&json!([])),
                "detected_separated": job_result.get("stepping_detected_separated").unwrap_or(&json!([])),
                "quality_issues": job_result.get("stepping_quality_issues").unwrap_or(&json!([])),
            },
            "audit_results": {
                "total_issues": job_result.get("issues").and_then(|v| v.as_i64()).unwrap_or(0),
                "details": job_result.get("audit_details").unwrap_or(&json!([])),
            },
            "sync_stability": job_result.get("sync_stability_issues").unwrap_or(&json!([])),
        });

        if let Some(jobs) = self.report_data["jobs"].as_array_mut() {
            jobs.push(entry);
        }
        self.write_report();
    }

    /// Finalize the report — `finalize`
    pub fn finalize(&mut self) -> Value {
        if self.report_data.is_null() {
            return json!({});
        }

        let mut successful = 0i64;
        let mut warnings = 0i64;
        let mut failed = 0i64;
        let mut total_issues = 0i64;

        if let Some(jobs) = self.report_data["jobs"].as_array() {
            for job in jobs {
                let status = job["status"].as_str().unwrap_or("Unknown");
                let issues = job["audit_results"]["total_issues"].as_i64().unwrap_or(0);

                if status == "Failed" {
                    failed += 1;
                } else if issues > 0 {
                    warnings += 1;
                } else {
                    successful += 1;
                }
                total_issues += issues;
            }
        }

        self.report_data["summary"] = json!({
            "successful": successful,
            "warnings": warnings,
            "failed": failed,
            "total_issues": total_issues,
        });
        self.report_data["finalized_at"] = json!(Local::now().to_rfc3339());

        self.write_report();
        self.report_data["summary"].clone()
    }

    pub fn get_report_path(&self) -> Option<&Path> {
        self.current_report_path.as_deref()
    }

    pub fn load(report_path: &Path) -> Result<Value, String> {
        let content = fs::read_to_string(report_path)
            .map_err(|e| format!("Failed to read report: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse report: {e}"))
    }

    fn write_report(&self) {
        let path = match &self.current_report_path {
            Some(p) => p,
            None => return,
        };
        let json_str = serde_json::to_string_pretty(&self.report_data).unwrap_or_default();
        let temp_path = self.logs_folder.join("report_.tmp");
        if fs::write(&temp_path, &json_str).is_ok() {
            let _ = fs::rename(&temp_path, path);
        }
    }

    pub fn sanitize_filename(name: &str) -> String {
        let invalid = "<>:\"/\\|?*";
        let mut result: String = name
            .chars()
            .map(|c| if invalid.contains(c) { '_' } else { c })
            .collect();
        while result.contains("__") {
            result = result.replace("__", "_");
        }
        result = result.trim_matches(|c| c == '_' || c == '.' || c == ' ').to_string();
        if result.len() > 100 {
            result.truncate(100);
        }
        if result.is_empty() { "unnamed".to_string() } else { result }
    }

    pub fn get_job_status_summary(job: &Value) -> String {
        let status = job["status"].as_str().unwrap_or("Unknown");
        if status == "Failed" { return "Failed".to_string(); }
        let issues = job["audit_results"]["total_issues"].as_i64().unwrap_or(0);
        if issues > 0 {
            format!("Warning ({issues} issue{})", if issues != 1 { "s" } else { "" })
        } else {
            "Success".to_string()
        }
    }

    pub fn get_delays_summary(job: &Value) -> String {
        let delays = match job["delays"].as_object() {
            Some(d) if !d.is_empty() => d,
            _ => return "-".to_string(),
        };
        let mut parts: Vec<String> = delays
            .iter()
            .map(|(source, delay)| {
                let short = source.replace("Source ", "S");
                let d = delay.as_i64().unwrap_or(0);
                let sign = if d >= 0 { "+" } else { "" };
                format!("{short}: {sign}{d}ms")
            })
            .collect();
        parts.sort();
        parts.join(", ")
    }
}
