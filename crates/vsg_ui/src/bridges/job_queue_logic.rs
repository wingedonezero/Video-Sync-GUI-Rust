//! Job queue logic — 1:1 port of `vsg_qt/job_queue_dialog/logic.py`.
//!
//! Manages the list of jobs, table population, layout copy/paste,
//! and configuration via ManualSelectionDialog.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// JobQueueLogic QObject — manages the job queue state.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, job_count)]
        #[qproperty(bool, has_clipboard)]
        type JobQueueLogic = super::JobQueueLogicRust;

        /// Initialize with temp_root for layout manager.
        #[qinvokable]
        fn initialize(self: Pin<&mut JobQueueLogic>, temp_root: QString);

        /// Add jobs from JSON array. Sorts naturally and appends.
        #[qinvokable]
        fn add_jobs(self: Pin<&mut JobQueueLogic>, jobs_json: QString);

        /// Remove jobs at indices (JSON array of ints).
        #[qinvokable]
        fn remove_jobs(self: Pin<&mut JobQueueLogic>, rows_json: QString);

        /// Get display data for a row as JSON.
        #[qinvokable]
        fn get_job_display_data(self: Pin<&mut JobQueueLogic>, row: i32) -> QString;

        /// Get sources for a job at the given row as JSON.
        #[qinvokable]
        fn get_job_sources(self: Pin<&mut JobQueueLogic>, row: i32) -> QString;

        /// Copy layout from the job at the given row.
        #[qinvokable]
        fn copy_layout(self: Pin<&mut JobQueueLogic>, row: i32);

        /// Paste layout to jobs at indices (JSON array of ints). Returns count pasted.
        #[qinvokable]
        fn paste_layout(self: Pin<&mut JobQueueLogic>, rows_json: QString) -> i32;

        /// Move jobs by direction (-1 up, +1 down). rows_json is JSON array of ints.
        #[qinvokable]
        fn move_jobs(self: Pin<&mut JobQueueLogic>, rows_json: QString, direction: i32);

        /// Save a configured layout for a job. Returns true on success.
        #[qinvokable]
        fn save_job_layout(
            self: Pin<&mut JobQueueLogic>,
            row: i32,
            layout_json: QString,
            attachments_json: QString,
            track_info_json: QString,
            source_settings_json: QString,
        ) -> bool;

        /// Load existing layout for a job. Returns JSON or empty string.
        #[qinvokable]
        fn load_job_layout(self: Pin<&mut JobQueueLogic>, row: i32) -> QString;

        /// Get the final configured jobs as JSON (for starting the worker).
        #[qinvokable]
        fn get_final_jobs(self: Pin<&mut JobQueueLogic>) -> QString;

        /// Get track info for a job (calls mkvmerge/ffprobe). Returns JSON or empty.
        #[qinvokable]
        fn get_track_info_for_job(self: Pin<&mut JobQueueLogic>, row: i32) -> QString;

        /// Configure a job at a row — returns existing layout data JSON for dialog prepopulation.
        /// QML uses this to open ManualSelectionDialog with the right data.
        #[qinvokable]
        fn get_configure_data(self: Pin<&mut JobQueueLogic>, row: i32) -> QString;

        /// Clean up all temporary layout files.
        #[qinvokable]
        fn cleanup_all(self: Pin<&mut JobQueueLogic>);

        /// Signal emitted when the job list changes.
        #[qsignal]
        fn jobs_changed(self: Pin<&mut JobQueueLogic>);

        /// Signal emitted with a log message.
        #[qsignal]
        fn log_message(self: Pin<&mut JobQueueLogic>, message: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::job_layouts::JobLayoutManager;

/// Backing Rust struct for JobQueueLogic.
#[derive(Default)]
pub struct JobQueueLogicRust {
    job_count: i32,
    has_clipboard: bool,
    jobs: Vec<serde_json::Value>,
    layout_manager: Option<JobLayoutManager>,
    layout_clipboard: Option<serde_json::Value>,
}


/// Natural sort key for filenames — 1:1 port of `natural_sort_key()`.
fn natural_sort_key(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_digits = false;

    for ch in s.chars() {
        let is_digit = ch.is_ascii_digit();
        if is_digit != in_digits && !current.is_empty() {
            parts.push(if in_digits {
                format!("{:>020}", current) // zero-pad for numeric sort
            } else {
                current.to_lowercase()
            });
            current.clear();
        }
        current.push(ch);
        in_digits = is_digit;
    }
    if !current.is_empty() {
        parts.push(if in_digits {
            format!("{:>020}", current)
        } else {
            current.to_lowercase()
        });
    }
    parts
}

fn get_sources(job: &serde_json::Value) -> HashMap<String, String> {
    job.get("sources")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn get_source1_name(job: &serde_json::Value) -> String {
    job.get("sources")
        .and_then(|s| s.get("Source 1"))
        .and_then(|v| v.as_str())
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default()
}

impl ffi::JobQueueLogic {
    fn initialize(mut self: Pin<&mut Self>, temp_root: QString) {
        let temp_root_str = temp_root.to_string();
        let log_cb: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|_msg: &str| {});
        self.as_mut().rust_mut().layout_manager =
            Some(JobLayoutManager::new(&temp_root_str, log_cb));
    }

    fn add_jobs(mut self: Pin<&mut Self>, jobs_json: QString) {
        let json_str = jobs_json.to_string();
        let mut new_jobs: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).unwrap_or_default();

        for job in &mut new_jobs {
            if let Some(obj) = job.as_object_mut() {
                obj.insert(
                    "status".to_string(),
                    serde_json::json!("Needs Configuration"),
                );
            }
        }

        // Natural sort by Source 1 filename
        new_jobs.sort_by(|a, b| {
            natural_sort_key(&get_source1_name(a)).cmp(&natural_sort_key(&get_source1_name(b)))
        });

        self.as_mut().rust_mut().jobs.extend(new_jobs);
        let count = self.rust().jobs.len() as i32;
        self.as_mut().set_job_count(count);
        self.as_mut().jobs_changed();
    }

    fn remove_jobs(mut self: Pin<&mut Self>, rows_json: QString) {
        let json_str = rows_json.to_string();
        let mut rows: Vec<usize> = serde_json::from_str(&json_str).unwrap_or_default();
        rows.sort_unstable();
        rows.reverse();

        for row in rows {
            if row < self.rust().jobs.len() {
                if let Some(lm) = &self.rust().layout_manager {
                    let sources = get_sources(&self.rust().jobs[row]);
                    let job_id = lm.generate_job_id(&sources);
                    lm.delete_layout(&job_id);
                }
                self.as_mut().rust_mut().jobs.remove(row);
            }
        }

        let count = self.rust().jobs.len() as i32;
        self.as_mut().set_job_count(count);
        self.as_mut().jobs_changed();
    }

    fn get_job_display_data(self: Pin<&mut Self>, row: i32) -> QString {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return QString::from("{}");
        }

        let job = &self.rust().jobs[idx];
        let sources = job.get("sources").cloned().unwrap_or_default();

        let source_names: Vec<String> = sources
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|v| {
                        v.as_str().map(|p| {
                            Path::new(p)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let is_configured = self.rust().layout_manager.as_ref().is_some_and(|lm| {
            let src_map = get_sources(job);
            lm.layout_exists(&lm.generate_job_id(&src_map))
        });

        let status = if is_configured {
            "Configured"
        } else {
            "Needs Configuration"
        };

        let display = serde_json::json!({
            "order": row + 1,
            "status": status,
            "sources_display": source_names.join(" + "),
        });

        let json = serde_json::to_string(&display).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn get_job_sources(self: Pin<&mut Self>, row: i32) -> QString {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return QString::from("{}");
        }
        let sources = self.rust().jobs[idx]
            .get("sources")
            .cloned()
            .unwrap_or_default();
        let json = serde_json::to_string(&sources).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn copy_layout(mut self: Pin<&mut Self>, row: i32) {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return;
        }

        let sources = get_sources(&self.rust().jobs[idx]);

        if let Some(lm) = &self.rust().layout_manager {
            let job_id = lm.generate_job_id(&sources);
            if let Some(layout_data) = lm.load_job_layout(&job_id) {
                self.as_mut().rust_mut().layout_clipboard = Some(layout_data);
                self.as_mut().set_has_clipboard(true);
                let name = get_source1_name(&self.rust().jobs[idx]);
                self.as_mut()
                    .log_message(QString::from(format!("[Queue] Copied layout from {name}.").as_str()));
                return;
            }
        }
        self.as_mut().rust_mut().layout_clipboard = None;
        self.as_mut().set_has_clipboard(false);
    }

    fn paste_layout(mut self: Pin<&mut Self>, rows_json: QString) -> i32 {
        let json_str = rows_json.to_string();
        let rows: Vec<usize> = serde_json::from_str(&json_str).unwrap_or_default();

        let clipboard = match self.rust().layout_clipboard.clone() {
            Some(c) => c,
            None => return 0,
        };

        let mut updated = 0i32;
        for &row in &rows {
            if row >= self.rust().jobs.len() {
                continue;
            }

            if let Some(lm) = &self.rust().layout_manager {
                let target_sources = get_sources(&self.rust().jobs[row]);
                let target_job_id = lm.generate_job_id(&target_sources);

                let mut target_data = clipboard.clone();
                if let Some(obj) = target_data.as_object_mut() {
                    obj.insert("job_id".to_string(), serde_json::json!(target_job_id));
                    obj.insert("sources".to_string(), serde_json::json!(target_sources));
                }

                if lm.persistence.save_layout(&target_job_id, &mut target_data) {
                    updated += 1;
                }
            }
        }

        if updated > 0 {
            self.as_mut().jobs_changed();
        }
        updated
    }

    fn move_jobs(mut self: Pin<&mut Self>, rows_json: QString, direction: i32) {
        let json_str = rows_json.to_string();
        let mut rows: Vec<usize> = serde_json::from_str(&json_str).unwrap_or_default();
        rows.sort_unstable();

        let len = self.rust().jobs.len();
        if rows.is_empty() || len == 0 {
            return;
        }

        if direction == -1 && rows[0] > 0 {
            for &row in &rows {
                self.as_mut().rust_mut().jobs.swap(row, row - 1);
            }
        } else if direction == 1 && *rows.last().unwrap_or(&0) < len - 1 {
            for &row in rows.iter().rev() {
                self.as_mut().rust_mut().jobs.swap(row, row + 1);
            }
        }

        self.as_mut().jobs_changed();
    }

    fn save_job_layout(
        self: Pin<&mut Self>,
        row: i32,
        layout_json: QString,
        attachments_json: QString,
        track_info_json: QString,
        source_settings_json: QString,
    ) -> bool {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return false;
        }

        let sources = get_sources(&self.rust().jobs[idx]);
        let layout: Vec<serde_json::Value> =
            serde_json::from_str(&layout_json.to_string()).unwrap_or_default();
        let attachments: Vec<String> =
            serde_json::from_str(&attachments_json.to_string()).unwrap_or_default();
        let track_info: HashMap<String, Vec<serde_json::Value>> =
            serde_json::from_str(&track_info_json.to_string()).unwrap_or_default();
        let source_settings: HashMap<String, serde_json::Value> =
            serde_json::from_str(&source_settings_json.to_string()).unwrap_or_default();

        if let Some(lm) = &self.rust().layout_manager {
            let job_id = lm.generate_job_id(&sources);
            return lm.save_job_layout(
                &job_id,
                &layout,
                &attachments,
                &sources,
                &track_info,
                Some(&source_settings),
            );
        }
        false
    }

    fn load_job_layout(self: Pin<&mut Self>, row: i32) -> QString {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return QString::from("");
        }

        let sources = get_sources(&self.rust().jobs[idx]);
        if let Some(lm) = &self.rust().layout_manager {
            let job_id = lm.generate_job_id(&sources);
            if let Some(data) = lm.load_job_layout(&job_id) {
                let json = serde_json::to_string(&data).unwrap_or_default();
                return QString::from(json.as_str());
            }
        }
        QString::from("")
    }

    fn get_final_jobs(self: Pin<&mut Self>) -> QString {
        let mut final_jobs = Vec::new();

        for job in &self.rust().jobs {
            let sources = get_sources(job);
            if let Some(lm) = &self.rust().layout_manager {
                let job_id = lm.generate_job_id(&sources);
                if let Some(layout_data) = lm.load_job_layout(&job_id) {
                    let mut final_job = job.clone();
                    if let Some(obj) = final_job.as_object_mut() {
                        let mut enhanced = layout_data
                            .get("enhanced_layout")
                            .cloned()
                            .unwrap_or_default();
                        if let Some(arr) = enhanced.as_array_mut() {
                            arr.sort_by_key(|item| {
                                item.get("user_order_index")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0)
                            });
                        }
                        obj.insert("manual_layout".to_string(), enhanced);
                        obj.insert(
                            "attachment_sources".to_string(),
                            layout_data
                                .get("attachment_sources")
                                .cloned()
                                .unwrap_or(serde_json::json!([])),
                        );
                        obj.insert(
                            "source_settings".to_string(),
                            layout_data
                                .get("source_settings")
                                .cloned()
                                .unwrap_or(serde_json::json!({})),
                        );
                    }
                    final_jobs.push(final_job);
                }
            }
        }

        let json = serde_json::to_string(&final_jobs).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn get_track_info_for_job(self: Pin<&mut Self>, row: i32) -> QString {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return QString::from("{}");
        }

        let job = &self.rust().jobs[idx];

        // Check cache
        if let Some(cached) = job.get("track_info") {
            if !cached.is_null() {
                let json = serde_json::to_string(cached).unwrap_or_default();
                return QString::from(json.as_str());
            }
        }

        // Build tool paths from system PATH
        let sources = get_sources(job);
        let tool_names = ["mkvmerge", "mkvextract", "ffmpeg", "ffprobe"];
        let tool_paths: HashMap<String, String> = tool_names
            .iter()
            .filter_map(|&name| {
                which::which(name)
                    .ok()
                    .map(|p| (name.to_string(), p.to_string_lossy().to_string()))
            })
            .collect();

        // Create a runner with default settings for track probing
        let settings = vsg_core::models::settings::AppSettings::default();
        let log_cb: Box<dyn Fn(&str) + Send + Sync> = Box::new(|_| {});
        let runner = vsg_core::io::runner::CommandRunner::new(settings, log_cb);

        let info = vsg_core::extraction::tracks::get_track_info_for_dialog(
            &sources, &runner, &tool_paths,
        );
        let json = serde_json::to_string(&info).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn get_configure_data(self: Pin<&mut Self>, row: i32) -> QString {
        let idx = row as usize;
        if idx >= self.rust().jobs.len() {
            return QString::from("{}");
        }

        let sources = get_sources(&self.rust().jobs[idx]);

        // Load existing layout if any
        let mut result = serde_json::json!({
            "sources": sources,
        });

        if let Some(lm) = &self.rust().layout_manager {
            let job_id = lm.generate_job_id(&sources);
            if let Some(layout_data) = lm.load_job_layout(&job_id) {
                // Convert enhanced_layout to dialog format (sorted by user_order_index)
                if let Some(enhanced) = layout_data.get("enhanced_layout") {
                    let mut items: Vec<serde_json::Value> = enhanced
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    items.sort_by_key(|item| {
                        item.get("user_order_index")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                    });
                    result["previous_layout"] = serde_json::json!(items);
                }
                if let Some(att) = layout_data.get("attachment_sources") {
                    result["previous_attachments"] = att.clone();
                }
                if let Some(ss) = layout_data.get("source_settings") {
                    result["previous_source_settings"] = ss.clone();
                }
            }
        }

        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn cleanup_all(self: Pin<&mut Self>) {
        if let Some(lm) = &self.rust().layout_manager {
            lm.cleanup_all();
        }
    }
}
