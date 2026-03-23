//! Manual selection logic — 1:1 port of `vsg_qt/manual_selection_dialog/logic.py`.
//!
//! Manages track layout creation, drag-and-drop reordering, and
//! conversion between dialog format and ManualLayoutItem.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// ManualSelectionLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, layout_track_count)]
        type ManualSelectionLogic = super::ManualSelectionLogicRust;

        /// Initialize with track info and optional previous layout (JSON).
        #[qinvokable]
        fn initialize(
            self: Pin<&mut ManualSelectionLogic>,
            track_info_json: QString,
            previous_layout_json: QString,
            previous_attachments_json: QString,
            previous_source_settings_json: QString,
        );

        /// Add a track to the layout by source key and track index.
        #[qinvokable]
        fn add_track_to_layout(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
            track_index: i32,
        );

        /// Remove a track from the layout at the given index.
        #[qinvokable]
        fn remove_track_from_layout(self: Pin<&mut ManualSelectionLogic>, index: i32);

        /// Move a layout track from one index to another.
        #[qinvokable]
        fn move_track(self: Pin<&mut ManualSelectionLogic>, from_index: i32, to_index: i32);

        /// Get the final layout, attachment sources, and source settings as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Get layout track data at index as JSON (for display).
        #[qinvokable]
        fn get_layout_track(self: Pin<&mut ManualSelectionLogic>, index: i32) -> QString;

        /// Get the available tracks for a source as JSON array.
        #[qinvokable]
        fn get_source_tracks(self: Pin<&mut ManualSelectionLogic>, source_key: QString) -> QString;

        /// Get all source keys as JSON array.
        #[qinvokable]
        fn get_source_keys(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Toggle attachment source selection.
        #[qinvokable]
        fn toggle_attachment_source(self: Pin<&mut ManualSelectionLogic>, source_key: QString);

        /// Get current attachment sources as JSON array.
        #[qinvokable]
        fn get_attachment_sources(self: Pin<&mut ManualSelectionLogic>) -> QString;

        /// Update source settings for a source (JSON).
        #[qinvokable]
        fn set_source_settings(
            self: Pin<&mut ManualSelectionLogic>,
            source_key: QString,
            settings_json: QString,
        );

        /// Signal: layout changed, UI needs refresh.
        #[qsignal]
        fn layout_changed(self: Pin<&mut ManualSelectionLogic>);
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

pub struct ManualSelectionLogicRust {
    layout_track_count: i32,
    track_info: HashMap<String, Vec<serde_json::Value>>,
    layout_tracks: Vec<serde_json::Value>,
    attachment_sources: Vec<String>,
    source_settings: HashMap<String, serde_json::Value>,
}

impl Default for ManualSelectionLogicRust {
    fn default() -> Self {
        Self {
            layout_track_count: 0,
            track_info: HashMap::new(),
            layout_tracks: Vec::new(),
            attachment_sources: Vec::new(),
            source_settings: HashMap::new(),
        }
    }
}

impl ffi::ManualSelectionLogic {
    fn initialize(
        mut self: Pin<&mut Self>,
        track_info_json: QString,
        previous_layout_json: QString,
        previous_attachments_json: QString,
        previous_source_settings_json: QString,
    ) {
        // Parse track info
        let info: HashMap<String, Vec<serde_json::Value>> =
            serde_json::from_str(&track_info_json.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().track_info = info;

        // Restore previous layout
        let prev_layout: Vec<serde_json::Value> =
            serde_json::from_str(&previous_layout_json.to_string()).unwrap_or_default();
        if !prev_layout.is_empty() {
            self.as_mut().rust_mut().layout_tracks = prev_layout;
        }

        // Restore attachments
        let prev_attachments: Vec<String> =
            serde_json::from_str(&previous_attachments_json.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().attachment_sources = prev_attachments;

        // Restore source settings
        let prev_settings: HashMap<String, serde_json::Value> =
            serde_json::from_str(&previous_source_settings_json.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().source_settings = prev_settings;

        let count = self.rust().layout_tracks.len() as i32;
        self.as_mut().set_layout_track_count(count);
        self.as_mut().layout_changed();
    }

    fn add_track_to_layout(mut self: Pin<&mut Self>, source_key: QString, track_index: i32) {
        let key = source_key.to_string();
        let idx = track_index as usize;

        let track = self
            .rust()
            .track_info
            .get(&key)
            .and_then(|tracks| tracks.get(idx))
            .cloned();

        if let Some(track) = track {
            // Block video from non-Source 1
            let is_video = track
                .get("type")
                .and_then(|v| v.as_str())
                == Some("video");
            if is_video && key != "Source 1" {
                return;
            }

            self.as_mut().rust_mut().layout_tracks.push(track);
            let count = self.rust().layout_tracks.len() as i32;
            self.as_mut().set_layout_track_count(count);
            self.as_mut().layout_changed();
        }
    }

    fn remove_track_from_layout(mut self: Pin<&mut Self>, index: i32) {
        let idx = index as usize;
        if idx < self.rust().layout_tracks.len() {
            self.as_mut().rust_mut().layout_tracks.remove(idx);
            let count = self.rust().layout_tracks.len() as i32;
            self.as_mut().set_layout_track_count(count);
            self.as_mut().layout_changed();
        }
    }

    fn move_track(mut self: Pin<&mut Self>, from_index: i32, to_index: i32) {
        let from = from_index as usize;
        let to = to_index as usize;
        let len = self.rust().layout_tracks.len();
        if from < len && to < len && from != to {
            let track = self.as_mut().rust_mut().layout_tracks.remove(from);
            self.as_mut().rust_mut().layout_tracks.insert(to, track);
            self.as_mut().layout_changed();
        }
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let result = serde_json::json!({
            "layout": self.rust().layout_tracks,
            "attachment_sources": self.rust().attachment_sources,
            "source_settings": self.rust().source_settings,
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn get_layout_track(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        let track = self.rust().layout_tracks.get(idx);
        match track {
            Some(t) => {
                let json = serde_json::to_string(t).unwrap_or_else(|_| "{}".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("{}"),
        }
    }

    fn get_source_tracks(self: Pin<&mut Self>, source_key: QString) -> QString {
        let key = source_key.to_string();
        let tracks = self.rust().track_info.get(&key);
        match tracks {
            Some(t) => {
                let json = serde_json::to_string(t).unwrap_or_else(|_| "[]".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("[]"),
        }
    }

    fn get_source_keys(self: Pin<&mut Self>) -> QString {
        let mut keys: Vec<&String> = self.rust().track_info.keys().collect();
        keys.sort();
        let json = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn toggle_attachment_source(mut self: Pin<&mut Self>, source_key: QString) {
        let key = source_key.to_string();
        let sources = &mut self.as_mut().rust_mut().attachment_sources;
        if let Some(pos) = sources.iter().position(|s| s == &key) {
            sources.remove(pos);
        } else {
            sources.push(key);
        }
    }

    fn get_attachment_sources(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().attachment_sources)
            .unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn set_source_settings(
        mut self: Pin<&mut Self>,
        source_key: QString,
        settings_json: QString,
    ) {
        let key = source_key.to_string();
        let settings: serde_json::Value =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();
        self.as_mut()
            .rust_mut()
            .source_settings
            .insert(key, settings);
    }
}
