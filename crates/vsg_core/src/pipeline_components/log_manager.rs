//! Log management — 1:1 port of `vsg_core/pipeline_components/log_manager.py`.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Log callback type.
pub type GuiLogCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Result type for setup_job_log.
pub type JobLogSetupResult = Result<(JobLogHandle, Box<dyn Fn(&str) + Send + Sync>), String>;

/// Manages logging setup and cleanup for jobs — `LogManager`
pub struct LogManager;

/// Handle to a job's log file for cleanup.
pub struct JobLogHandle {
    log_path: PathBuf,
    #[allow(dead_code)]
    file: Arc<Mutex<File>>,
}

impl LogManager {
    /// Sets up logging for a job — `setup_job_log`
    ///
    /// Returns a tuple of (log_handle, log_to_all_function).
    /// The log_to_all function writes to both the log file and the GUI callback.
    pub fn setup_job_log(
        job_name: &str,
        log_dir: &Path,
        gui_log_callback: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> JobLogSetupResult {
        let _ = fs::create_dir_all(log_dir);
        let log_path = log_dir.join(format!("{job_name}.log"));
        let file = File::create(&log_path)
            .map_err(|e| format!("Failed to create log file: {e}"))?;
        let file = Arc::new(Mutex::new(file));

        let handle = JobLogHandle {
            log_path,
            file: Arc::clone(&file),
        };

        // Create unified log function that writes to file + GUI
        let log_to_all: Box<dyn Fn(&str) + Send + Sync> = Box::new(move |message: &str| {
            let trimmed = message.trim_end();
            if let Ok(mut f) = file.lock() {
                let _ = writeln!(f, "{trimmed}");
            }
            gui_log_callback(message);
        });

        Ok((handle, log_to_all))
    }

    /// Cleans up logger resources — `cleanup_log`
    pub fn cleanup_log(_handle: JobLogHandle) {
        // File is closed when the handle is dropped (RAII)
    }
}

impl JobLogHandle {
    /// Get the path to the log file.
    pub fn path(&self) -> &Path {
        &self.log_path
    }
}
