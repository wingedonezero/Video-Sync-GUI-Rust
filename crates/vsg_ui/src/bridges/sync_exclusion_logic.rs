//! Sync exclusion dialog logic — 1:1 port of `vsg_qt/sync_exclusion_dialog/ui.py`.
//!
//! Manages sync exclusion zone configuration: style-based exclusions
//! and preview of affected events. Uses SubtitleData to read styles from files.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// SyncExclusionLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, mode)]
        #[qproperty(i32, total_events)]
        #[qproperty(i32, excluded_events)]
        type SyncExclusionLogic = super::SyncExclusionLogicRust;

        /// Initialize with track data JSON (needs subtitle_path, existing config).
        #[qinvokable]
        fn initialize(self: Pin<&mut SyncExclusionLogic>, track_json: QString);

        /// Get available styles with event counts as JSON array.
        #[qinvokable]
        fn get_available_styles(self: Pin<&mut SyncExclusionLogic>) -> QString;

        /// Set the selected exclusion styles (JSON array of style names).
        #[qinvokable]
        fn set_exclusion_styles(self: Pin<&mut SyncExclusionLogic>, styles_json: QString);

        /// Update the exclusion mode ("exclude" or "include") and refresh preview.
        #[qinvokable]
        fn update_mode(self: Pin<&mut SyncExclusionLogic>, new_mode: QString);

        /// Get the final exclusion config as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut SyncExclusionLogic>) -> QString;

        /// Signal: preview counts updated.
        #[qsignal]
        fn preview_updated(self: Pin<&mut SyncExclusionLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::collections::HashMap;
use std::path::Path;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::subtitles::data::SubtitleData;

pub struct SyncExclusionLogicRust {
    mode: QString,
    total_events: i32,
    excluded_events: i32,
    style_counts: HashMap<String, usize>,
    selected_styles: Vec<String>,
    original_style_list: Vec<String>,
}

impl Default for SyncExclusionLogicRust {
    fn default() -> Self {
        Self {
            mode: QString::from("exclude"),
            total_events: 0,
            excluded_events: 0,
            style_counts: HashMap::new(),
            selected_styles: Vec::new(),
            original_style_list: Vec::new(),
        }
    }
}

impl ffi::SyncExclusionLogic {
    /// Initialize from track data — extracts styles from subtitle file.
    fn initialize(mut self: Pin<&mut Self>, track_json: QString) {
        let json_str = track_json.to_string();
        let track: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();

        // Get subtitle path from track data
        let subtitle_path = track
            .get("subtitle_path")
            .or_else(|| track.get("original_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !subtitle_path.is_empty() {
            match SubtitleData::get_style_counts_from_file(Path::new(subtitle_path)) {
                Ok(counts) => {
                    let total: usize = counts.values().sum();
                    self.as_mut().set_total_events(total as i32);
                    let styles: Vec<String> = counts.keys().cloned().collect();
                    self.as_mut().rust_mut().original_style_list = styles;
                    self.as_mut().rust_mut().style_counts = counts;
                }
                Err(_) => {}
            }
        }

        // Restore existing config if present
        if let Some(existing_styles) = track.get("sync_exclusion_styles").and_then(|v| v.as_array())
        {
            let styles: Vec<String> = existing_styles
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            self.as_mut().rust_mut().selected_styles = styles;
        }

        if let Some(m) = track.get("sync_exclusion_mode").and_then(|v| v.as_str()) {
            self.as_mut().set_mode(QString::from(m));
        }

        self.as_mut().update_preview();
    }

    fn get_available_styles(self: Pin<&mut Self>) -> QString {
        let styles: Vec<serde_json::Value> = self
            .rust()
            .style_counts
            .iter()
            .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
            .collect();
        let json = serde_json::to_string(&styles).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn set_exclusion_styles(mut self: Pin<&mut Self>, styles_json: QString) {
        let json_str = styles_json.to_string();
        let styles: Vec<String> = serde_json::from_str(&json_str).unwrap_or_default();
        self.as_mut().rust_mut().selected_styles = styles;
        self.as_mut().update_preview();
    }

    fn update_mode(mut self: Pin<&mut Self>, new_mode: QString) {
        self.as_mut().set_mode(new_mode);
        self.as_mut().update_preview();
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let result = serde_json::json!({
            "sync_exclusion_styles": self.rust().selected_styles,
            "sync_exclusion_mode": self.as_ref().mode().to_string(),
            "sync_exclusion_original_style_list": self.rust().original_style_list,
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    /// Update the preview counts.
    fn update_preview(mut self: Pin<&mut Self>) {
        let mode = self.as_ref().mode().to_string();
        let excluded: usize = self
            .rust()
            .selected_styles
            .iter()
            .filter_map(|s| self.rust().style_counts.get(s))
            .sum();

        let count = if mode == "exclude" {
            excluded as i32
        } else {
            self.rust().total_events - excluded as i32
        };

        self.as_mut().set_excluded_events(count);
        self.as_mut().preview_updated();
    }
}
