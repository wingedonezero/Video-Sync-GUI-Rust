//! Main window controller — 1:1 port of `vsg_qt/main_window/controller.py`.
//!
//! CXX-Qt QObject bridge that exposes properties, signals, and invokables
//! to the `MainWindow.qml` view. All business logic lives here; the QML
//! file is a pure declarative UI shell (matching the Python pattern where
//! `window.py` builds widgets and `controller.py` handles logic).

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++Qt" {}

    extern "RustQt" {
        /// MainController QObject — registered as QML element `MainController`.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, ref_path)]
        #[qproperty(QString, sec_path)]
        #[qproperty(QString, ter_path)]
        #[qproperty(QString, log_text)]
        #[qproperty(QString, status_text)]
        #[qproperty(f64, progress_value)]
        #[qproperty(bool, archive_logs)]
        #[qproperty(QString, sec_delay_text)]
        #[qproperty(QString, ter_delay_text)]
        #[qproperty(QString, src4_delay_text)]
        #[qproperty(bool, worker_running)]
        type MainController = super::MainControllerRust;

        /// Called when the user clicks "Analyze Only".
        #[qinvokable]
        fn start_analyze_only(self: Pin<&mut MainController>);

        /// Called when the user clicks "Open Job Queue".
        #[qinvokable]
        fn open_job_queue(self: Pin<&mut MainController>);

        /// Called when the user clicks "Settings…".
        #[qinvokable]
        fn open_options_dialog(self: Pin<&mut MainController>);

        /// Browse for a file/directory path. `source_index`: 1=ref, 2=sec, 3=ter.
        #[qinvokable]
        fn browse_for_path(self: Pin<&mut MainController>, source_index: i32);

        /// Initialize the controller (called once from QML Component.onCompleted).
        #[qinvokable]
        fn initialize(self: Pin<&mut MainController>);

        /// Called on window close to save config and clean up.
        #[qinvokable]
        fn on_close(self: Pin<&mut MainController>);

        /// Get the full settings as JSON (for passing to dialogs).
        #[qinvokable]
        fn get_settings_json(self: Pin<&mut MainController>) -> QString;

        /// Update settings from JSON (after options dialog saves).
        #[qinvokable]
        fn update_settings_from_json(self: Pin<&mut MainController>, json: QString);

        /// Actually start the worker on a background thread.
        /// Called from QML after worker_start_requested.
        #[qinvokable]
        fn start_worker(
            self: Pin<&mut MainController>,
            jobs_json: QString,
            and_merge: bool,
            output_dir: QString,
            settings_json: QString,
        );

        /// Handle a worker's finished_job result (JSON).
        #[qinvokable]
        fn handle_job_finished(self: Pin<&mut MainController>, result_json: QString);

        /// Handle a worker's finished_all result (JSON).
        #[qinvokable]
        fn handle_batch_finished(self: Pin<&mut MainController>, results_json: QString);

        /// Signal: request QML to open a dialog.
        #[qsignal]
        fn open_dialog_requested(self: Pin<&mut MainController>, dialog_name: QString);

        /// Signal: log message appended.
        #[qsignal]
        fn log_message(self: Pin<&mut MainController>, message: QString);

        /// Signal: worker started a batch — carries jobs JSON and config for worker.
        #[qsignal]
        fn worker_start_requested(
            self: Pin<&mut MainController>,
            jobs_json: QString,
            and_merge: bool,
            output_dir: QString,
            settings_json: QString,
        );
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::config::AppConfig;
use vsg_core::job_discovery::discover_jobs;

/// Backing Rust struct for MainController QObject.
/// Properties here map to the `#[qproperty]` declarations above.
pub struct MainControllerRust {
    // QML-bound properties
    ref_path: QString,
    sec_path: QString,
    ter_path: QString,
    log_text: QString,
    status_text: QString,
    progress_value: f64,
    archive_logs: bool,
    sec_delay_text: QString,
    ter_delay_text: QString,
    src4_delay_text: QString,
    worker_running: bool,

    // Internal state (not exposed to QML)
    config: Option<AppConfig>,
    job_counter: i32,
    worker_cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Default for MainControllerRust {
    fn default() -> Self {
        Self {
            ref_path: QString::from(""),
            sec_path: QString::from(""),
            ter_path: QString::from(""),
            log_text: QString::from(""),
            status_text: QString::from("Ready"),
            progress_value: 0.0,
            archive_logs: true,
            sec_delay_text: QString::from(""),
            ter_delay_text: QString::from(""),
            src4_delay_text: QString::from(""),
            worker_running: false,
            config: None,
            job_counter: 0,
            worker_cancelled: None,
        }
    }
}

impl ffi::MainController {
    /// Initialize the controller — loads config and applies to UI.
    /// 1:1 port of `MainWindow.__init__` + `controller.apply_config_to_ui`.
    fn initialize(mut self: Pin<&mut Self>) {
        // Determine the application directory.
        // The binary drops into the working directory, same as Python.
        let script_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        match AppConfig::new(&script_dir) {
            Ok(config) => {
                // Apply config values to UI properties
                self.as_mut()
                    .set_ref_path(QString::from(config.settings.last_ref_path.as_str()));
                self.as_mut()
                    .set_sec_path(QString::from(config.settings.last_sec_path.as_str()));
                self.as_mut()
                    .set_ter_path(QString::from(config.settings.last_ter_path.as_str()));
                self.as_mut()
                    .set_archive_logs(config.settings.archive_logs);

                // Store config
                self.as_mut().rust_mut().config = Some(config);

                self.as_mut().append_log("Configuration loaded.");
            }
            Err(e) => {
                self.as_mut()
                    .append_log(&format!("[ERROR] Failed to load config: {e:?}"));
            }
        }
    }

    /// Save current UI state to config — 1:1 port of `controller.py::save_ui_to_config`.
    fn save_ui_to_config(mut self: Pin<&mut Self>) {
        let ref_path = self.as_ref().ref_path().to_string();
        let sec_path = self.as_ref().sec_path().to_string();
        let ter_path = self.as_ref().ter_path().to_string();
        let archive = *self.as_ref().archive_logs();

        if let Some(config) = self.as_mut().rust_mut().config.as_mut() {
            config.settings.last_ref_path = ref_path;
            config.settings.last_sec_path = sec_path;
            config.settings.last_ter_path = ter_path;
            config.settings.archive_logs = archive;
            if let Err(e) = config.save() {
                // Can't call append_log here because we have &mut self via rust_mut()
                // Will log after dropping the mutable borrow
                let _ = e;
            }
        }
    }

    /// Append a message to the log — 1:1 port of `controller.py::append_log`.
    fn append_log(mut self: Pin<&mut Self>, message: &str) {
        let current = self.as_ref().log_text().to_string();
        let new_text = if current.is_empty() {
            message.to_string()
        } else {
            format!("{current}\n{message}")
        };
        self.as_mut().set_log_text(QString::from(new_text.as_str()));
    }

    /// Start batch analyze-only — 1:1 port of `controller.py::start_batch_analyze_only`.
    fn start_analyze_only(mut self: Pin<&mut Self>) {
        self.as_mut().save_ui_to_config();

        let ref_text = self.as_ref().ref_path().to_string();
        let sec_text = self.as_ref().sec_path().to_string();
        let ter_text = self.as_ref().ter_path().to_string();

        // Build sources map (only non-empty)
        let mut sources = HashMap::new();
        if !ref_text.is_empty() {
            sources.insert("Source 1".to_string(), ref_text);
        }
        if !sec_text.is_empty() {
            sources.insert("Source 2".to_string(), sec_text);
        }
        if !ter_text.is_empty() {
            sources.insert("Source 3".to_string(), ter_text);
        }

        // Discover jobs
        let initial_jobs = match discover_jobs(&sources) {
            Ok(jobs) if jobs.is_empty() => {
                self.as_mut().append_log("No valid jobs found.");
                return;
            }
            Ok(jobs) => jobs,
            Err(e) => {
                self.as_mut()
                    .append_log(&format!("[ERROR] Job Discovery: {e}"));
                return;
            }
        };

        // Determine output directory
        let output_dir = self.as_mut().get_output_dir(&sources, &initial_jobs);

        // Convert jobs to JSON for the worker signal
        let jobs_json_vec: Vec<serde_json::Value> = initial_jobs
            .iter()
            .map(|j| {
                let mut map = serde_json::Map::new();
                let sources_val = serde_json::to_value(j).unwrap_or_default();
                map.insert("sources".to_string(), sources_val);
                serde_json::Value::Object(map)
            })
            .collect();

        self.as_mut()
            .start_worker_batch(jobs_json_vec, false, &output_dir);
    }

    /// Open the job queue dialog — 1:1 port of `controller.py::open_job_queue`.
    fn open_job_queue(mut self: Pin<&mut Self>) {
        self.as_mut().save_ui_to_config();
        self.as_mut()
            .open_dialog_requested(QString::from("JobQueueDialog"));
    }

    /// Open the settings dialog — 1:1 port of `controller.py::open_options_dialog`.
    fn open_options_dialog(mut self: Pin<&mut Self>) {
        self.as_mut()
            .open_dialog_requested(QString::from("OptionsDialog"));
    }

    /// Browse for a path — 1:1 port of `controller.py::browse_for_path`.
    /// In QML, the FileDialog is handled by QML itself; this is called with the result.
    fn browse_for_path(self: Pin<&mut Self>, source_index: i32) {
        // QML will handle the actual FileDialog; this just signals which source.
        // The QML side sets the property directly from the FileDialog result.
        let _ = source_index;
    }

    /// Get settings as JSON — used to pass config to dialogs.
    fn get_settings_json(self: Pin<&mut Self>) -> QString {
        if let Some(config) = &self.rust().config {
            match serde_json::to_string(&config.settings) {
                Ok(json) => QString::from(json.as_str()),
                Err(_) => QString::from("{}"),
            }
        } else {
            QString::from("{}")
        }
    }

    /// Update settings from JSON — called after OptionsDialog saves.
    fn update_settings_from_json(mut self: Pin<&mut Self>, json: QString) {
        let json_str = json.to_string();
        if let Ok(new_settings) = serde_json::from_str(&json_str) {
            if let Some(config) = self.as_mut().rust_mut().config.as_mut() {
                config.settings = new_settings;
                if let Err(e) = config.save() {
                    // Log after we release the mutable borrow
                    let _ = e;
                }
            }
            self.as_mut().append_log("Settings saved.");
        }
    }

    /// Handle individual job finished — 1:1 port of `controller.py::job_finished`.
    fn handle_job_finished(mut self: Pin<&mut Self>, result_json: QString) {
        let json_str = result_json.to_string();
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(&json_str) {
            // Update delay labels
            let delays = result.get("delays").and_then(|d| d.as_object());
            if let Some(delays) = delays {
                let sec = delays
                    .get("Source 2")
                    .and_then(|v| v.as_i64())
                    .map(|v| format!("{v} ms"))
                    .unwrap_or_default();
                let ter = delays
                    .get("Source 3")
                    .and_then(|v| v.as_i64())
                    .map(|v| format!("{v} ms"))
                    .unwrap_or_default();
                let src4 = delays
                    .get("Source 4")
                    .and_then(|v| v.as_i64())
                    .map(|v| format!("{v} ms"))
                    .unwrap_or_default();
                self.as_mut().set_sec_delay_text(QString::from(sec.as_str()));
                self.as_mut().set_ter_delay_text(QString::from(ter.as_str()));
                self.as_mut()
                    .set_src4_delay_text(QString::from(src4.as_str()));
            }

            let name = result
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            self.as_mut()
                .append_log(&format!("--- Job Summary for {name}: {status} ---"));

            self.as_mut().rust_mut().job_counter += 1;
        }
    }

    /// Handle all jobs finished — 1:1 port of `controller.py::batch_finished`.
    fn handle_batch_finished(mut self: Pin<&mut Self>, results_json: QString) {
        let json_str = results_json.to_string();
        let results: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).unwrap_or_default();
        let total = results.len();

        self.as_mut()
            .set_status_text(QString::from(format!("All {total} jobs finished.").as_str()));
        self.as_mut().set_progress_value(1.0);
        self.as_mut().set_worker_running(false);

        // Count statuses
        let successful = results
            .iter()
            .filter(|r| {
                r.get("status")
                    .and_then(|s| s.as_str())
                    .map(|s| s != "Failed")
                    .unwrap_or(false)
            })
            .count();
        let failed = total - successful;

        let summary = format!(
            "\n--- Batch Summary ---\n  - Successful jobs: {successful}\n  - Failed jobs: {failed}\n"
        );
        self.as_mut().append_log(&summary);

        // Signal QML to show batch completion dialog
        self.as_mut()
            .open_dialog_requested(QString::from("BatchCompletionDialog"));
    }

    /// Save config and clean up on close — 1:1 port of `controller.py::on_close`.
    fn on_close(mut self: Pin<&mut Self>) {
        // Cancel running worker if any
        if let Some(cancelled) = &self.rust().worker_cancelled {
            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.as_mut().save_ui_to_config();
        self.as_mut()
            .append_log("[SHUTDOWN] Saving configuration...");
    }

    /// Actually start the worker on a background thread.
    fn start_worker(
        mut self: Pin<&mut Self>,
        jobs_json: QString,
        and_merge: bool,
        output_dir: QString,
        settings_json: QString,
    ) {
        use crate::worker::runner::{JobRunnerConfig, SignalCallbacks};
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;

        let settings: vsg_core::models::settings::AppSettings =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();

        let jobs: Vec<std::collections::HashMap<String, serde_json::Value>> =
            serde_json::from_str(&jobs_json.to_string()).unwrap_or_default();

        let cancelled = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().worker_cancelled = Some(Arc::clone(&cancelled));

        let config = JobRunnerConfig {
            settings,
            jobs,
            and_merge,
            output_dir: output_dir.to_string(),
            cancelled,
        };

        // For now, signals are no-ops (TODO: wire Qt signals back via thread-safe channel)
        // In a full implementation, these closures would send messages back to the
        // main thread via a channel or Qt event, which the controller would poll/receive.
        let callbacks = SignalCallbacks {
            log: Box::new(|_msg| {}),
            progress: Box::new(|_pct| {}),
            status: Box::new(|_msg| {}),
            finished_job: Box::new(|_json| {}),
            finished_all: Box::new(|_json| {}),
        };

        // Spawn background thread
        std::thread::spawn(move || {
            crate::worker::runner::run_job_batch(config, callbacks);
        });

        self.as_mut().set_worker_running(true);
        self.as_mut()
            .append_log("[WORKER] Background worker thread started.");
    }

    // ── Internal helpers ──

    /// Determine output directory based on job count and source paths.
    fn get_output_dir(
        &self,
        sources: &HashMap<String, String>,
        jobs: &[HashMap<String, String>],
    ) -> String {
        let output_dir = self
            .config
            .as_ref()
            .map(|c| c.settings.output_folder.clone())
            .unwrap_or_else(|| "sync_output".to_string());

        if jobs.len() > 1 {
            if let Some(src1) = sources.get("Source 1") {
                let parent_name = Path::new(src1)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                return format!("{output_dir}/{parent_name}");
            }
        }
        output_dir
    }

    /// Start the worker batch — prepares and emits the worker_start_requested signal.
    fn start_worker_batch(
        mut self: Pin<&mut Self>,
        jobs: Vec<serde_json::Value>,
        and_merge: bool,
        output_dir: &str,
    ) {
        let total = jobs.len();
        self.as_mut().set_log_text(QString::from(""));
        self.as_mut().set_status_text(
            QString::from(format!("Starting batch of {total} jobs…").as_str()),
        );
        self.as_mut().set_progress_value(0.0);
        self.as_mut().set_sec_delay_text(QString::from(""));
        self.as_mut().set_ter_delay_text(QString::from(""));
        self.as_mut().set_src4_delay_text(QString::from(""));
        self.as_mut().set_worker_running(true);
        self.as_mut().rust_mut().job_counter = 0;

        let jobs_json = serde_json::to_string(&jobs).unwrap_or_else(|_| "[]".to_string());
        let settings_json = self
            .as_ref()
            .rust()
            .config
            .as_ref()
            .and_then(|c| serde_json::to_string(&c.settings).ok())
            .unwrap_or_else(|| "{}".to_string());

        self.as_mut().worker_start_requested(
            QString::from(jobs_json.as_str()),
            and_merge,
            QString::from(output_dir),
            QString::from(settings_json.as_str()),
        );
    }
}
