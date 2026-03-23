//! Add job dialog logic — 1:1 port of `vsg_qt/add_job_dialog/ui.py`.
//!
//! Handles dynamic source inputs, drag-and-drop, and job discovery.
//! Python combined UI + logic in `ui.py`; here we separate:
//! - This file: logic (QObject bridge)
//! - `AddJobDialog.qml`: layout

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// AddJobLogic QObject — handles source input and job discovery.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, source_count)]
        type AddJobLogic = super::AddJobLogicRust;

        /// Get the path text for a source at the given index.
        #[qinvokable]
        fn get_source_path(self: Pin<&mut AddJobLogic>, index: i32) -> QString;

        /// Set the path text for a source at the given index.
        #[qinvokable]
        fn set_source_path(self: Pin<&mut AddJobLogic>, index: i32, path: QString);

        /// Add another source input slot.
        #[qinvokable]
        fn add_source_input(self: Pin<&mut AddJobLogic>);

        /// Populate sources from dropped file paths (JSON array of strings).
        #[qinvokable]
        fn populate_from_paths(self: Pin<&mut AddJobLogic>, paths_json: QString);

        /// Discover jobs from current source paths. Returns JSON of discovered jobs,
        /// or empty array on failure (error emitted via discovery_error signal).
        #[qinvokable]
        fn find_jobs(self: Pin<&mut AddJobLogic>) -> QString;

        /// Signal: source count changed (UI needs to update input fields).
        #[qsignal]
        fn sources_changed(self: Pin<&mut AddJobLogic>);

        /// Signal: error during job discovery.
        #[qsignal]
        fn discovery_error(self: Pin<&mut AddJobLogic>, message: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::collections::HashMap;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::job_discovery::discover_jobs;

/// Backing Rust struct for AddJobLogic.
pub struct AddJobLogicRust {
    source_count: i32,
    source_paths: Vec<String>,
}

impl Default for AddJobLogicRust {
    fn default() -> Self {
        Self {
            source_count: 2,
            source_paths: vec![String::new(), String::new()],
        }
    }
}

impl ffi::AddJobLogic {
    /// Get the path at the given source index.
    fn get_source_path(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        self.rust()
            .source_paths
            .get(idx)
            .map(|s| QString::from(s.as_str()))
            .unwrap_or_else(|| QString::from(""))
    }

    /// Set the path at the given source index.
    fn set_source_path(self: Pin<&mut Self>, index: i32, path: QString) {
        let idx = index as usize;
        let path_str = path.to_string();
        let paths = &mut self.rust_mut().source_paths;
        if idx < paths.len() {
            paths[idx] = path_str;
        }
    }

    /// Add another source input slot — 1:1 port of `add_source_input()`.
    fn add_source_input(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().source_paths.push(String::new());
        let count = self.rust().source_paths.len() as i32;
        self.as_mut().set_source_count(count);
        self.as_mut().sources_changed();
    }

    /// Populate sources from dropped file paths — 1:1 port of `populate_sources_from_paths()`.
    fn populate_from_paths(mut self: Pin<&mut Self>, paths_json: QString) {
        let json_str = paths_json.to_string();
        let paths: Vec<String> = serde_json::from_str(&json_str).unwrap_or_default();

        self.as_mut().rust_mut().source_paths.clear();
        for path in &paths {
            self.as_mut().rust_mut().source_paths.push(path.clone());
        }
        // Ensure at least 2 inputs
        while self.rust().source_paths.len() < 2 {
            self.as_mut().rust_mut().source_paths.push(String::new());
        }

        let count = self.rust().source_paths.len() as i32;
        self.as_mut().set_source_count(count);
        self.as_mut().sources_changed();
    }

    /// Discover jobs — 1:1 port of `find_and_accept()`.
    fn find_jobs(mut self: Pin<&mut Self>) -> QString {
        // Build sources map from non-empty paths
        let mut sources = HashMap::new();
        for (i, path) in self.rust().source_paths.iter().enumerate() {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                sources.insert(format!("Source {}", i + 1), trimmed.to_string());
            }
        }

        if !sources.contains_key("Source 1") {
            self.as_mut()
                .discovery_error(QString::from("Source 1 (Reference) cannot be empty."));
            return QString::from("[]");
        }

        match discover_jobs(&sources) {
            Ok(jobs) if jobs.is_empty() => {
                self.as_mut().discovery_error(QString::from(
                    "No matching jobs could be discovered from the provided paths.",
                ));
                QString::from("[]")
            }
            Ok(jobs) => {
                // Wrap each job's sources in a job object
                let job_objects: Vec<serde_json::Value> = jobs
                    .iter()
                    .map(|j| serde_json::json!({"sources": j}))
                    .collect();
                let json = serde_json::to_string(&job_objects).unwrap_or_else(|_| "[]".to_string());
                QString::from(json.as_str())
            }
            Err(e) => {
                self.as_mut()
                    .discovery_error(QString::from(e.as_str()));
                QString::from("[]")
            }
        }
    }
}
