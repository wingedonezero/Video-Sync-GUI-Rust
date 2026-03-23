//! Layout persistence — 1:1 port of `vsg_core/job_layouts/persistence.py`.
//!
//! Handles saving and loading job layouts to/from JSON files.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use serde_json::Value;

/// Log callback type.
type LogCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Handles saving and loading job layouts to/from JSON files — `LayoutPersistence`.
pub struct LayoutPersistence {
    layouts_dir: PathBuf,
    log: LogCallback,
}

impl LayoutPersistence {
    /// Create a new `LayoutPersistence`, ensuring the layouts directory exists.
    pub fn new(layouts_dir: PathBuf, log: LogCallback) -> Self {
        let _ = fs::create_dir_all(&layouts_dir);
        Self { layouts_dir, log }
    }

    /// Saves a job layout using a temporary file to prevent corruption.
    pub fn save_layout(&self, job_id: &str, layout_data: &mut Value) -> bool {
        // Ensure directory exists right before saving
        if let Err(e) = fs::create_dir_all(&self.layouts_dir) {
            (self.log)(&format!(
                "[LayoutPersistence] Error creating directory: {e}"
            ));
            return false;
        }

        let layout_file = self.layouts_dir.join(format!("{job_id}.json"));
        let temp_file = self.layouts_dir.join(format!("{job_id}.tmp"));

        // Add save timestamp
        if let Some(obj) = layout_data.as_object_mut() {
            obj.insert(
                "saved_timestamp".to_string(),
                Value::String(Local::now().to_rfc3339()),
            );
        }

        match serde_json::to_string_pretty(layout_data) {
            Ok(json_str) => {
                // Write to temp file first, then rename (atomic)
                if let Err(e) = fs::write(&temp_file, &json_str) {
                    (self.log)(&format!(
                        "[LayoutPersistence] Error writing temp file for {job_id}: {e}"
                    ));
                    return false;
                }
                if let Err(e) = fs::rename(&temp_file, &layout_file) {
                    (self.log)(&format!(
                        "[LayoutPersistence] Error renaming temp file for {job_id}: {e}"
                    ));
                    // Clean up temp file
                    let _ = fs::remove_file(&temp_file);
                    return false;
                }
                true
            }
            Err(e) => {
                (self.log)(&format!(
                    "[LayoutPersistence] Error serializing layout for {job_id}: {e}"
                ));
                false
            }
        }
    }

    /// Loads a job layout from a JSON file.
    pub fn load_layout(&self, job_id: &str) -> Option<Value> {
        let layout_file = self.layouts_dir.join(format!("{job_id}.json"));
        if !layout_file.exists() {
            return None;
        }

        match fs::read_to_string(&layout_file) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(data) => Some(data),
                Err(e) => {
                    (self.log)(&format!(
                        "[LayoutPersistence] Error parsing layout for {job_id}: {e}"
                    ));
                    None
                }
            },
            Err(e) => {
                (self.log)(&format!(
                    "[LayoutPersistence] Error reading layout for {job_id}: {e}"
                ));
                None
            }
        }
    }

    /// Checks if a layout file exists.
    pub fn layout_exists(&self, job_id: &str) -> bool {
        self.layouts_dir.join(format!("{job_id}.json")).exists()
    }

    /// Deletes a specific layout file.
    pub fn delete_layout(&self, job_id: &str) -> bool {
        let layout_file = self.layouts_dir.join(format!("{job_id}.json"));
        if layout_file.exists() {
            if let Err(e) = fs::remove_file(&layout_file) {
                (self.log)(&format!(
                    "[LayoutPersistence] Error deleting layout {job_id}: {e}"
                ));
                return false;
            }
        }
        true
    }

    /// Removes all layout files and the layouts directory.
    pub fn cleanup_all(&self) {
        if self.layouts_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&self.layouts_dir) {
                (self.log)(&format!(
                    "[LayoutPersistence] Error during cleanup: {e}"
                ));
            } else {
                (self.log)("[LayoutPersistence] Cleaned up all temporary layout files.");
            }
        }
    }
}
