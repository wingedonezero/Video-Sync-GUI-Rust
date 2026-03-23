//! Source settings dialog logic — 1:1 port of `vsg_qt/source_settings_dialog/dialog.py`.
//!
//! Per-source audio/video track settings (reference audio track selection,
//! correlation track selection, source separation toggle).

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// SourceSettingsLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, source_key)]
        #[qproperty(i32, selected_track)]
        #[qproperty(bool, use_source_separation)]
        #[qproperty(bool, is_source1)]
        type SourceSettingsLogic = super::SourceSettingsLogicRust;

        /// Initialize with source data JSON (source_key, audio_tracks, current_settings).
        #[qinvokable]
        fn initialize(self: Pin<&mut SourceSettingsLogic>, source_json: QString);

        /// Get audio tracks for display as JSON array.
        /// Each entry: {display, index} where index=-1 is "Auto (Language Fallback)".
        #[qinvokable]
        fn get_audio_tracks(self: Pin<&mut SourceSettingsLogic>) -> QString;

        /// Get modified source settings as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut SourceSettingsLogic>) -> QString;

        /// Check if any non-default settings are configured.
        #[qinvokable]
        fn has_non_default_settings(self: Pin<&mut SourceSettingsLogic>) -> bool;

        /// Reset to defaults.
        #[qinvokable]
        fn reset_to_defaults(self: Pin<&mut SourceSettingsLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct SourceSettingsLogicRust {
    source_key: QString,
    selected_track: i32, // -1 = Auto (Language Fallback), 0+ = track index
    use_source_separation: bool,
    is_source1: bool,
    audio_tracks: Vec<serde_json::Value>,
}

impl Default for SourceSettingsLogicRust {
    fn default() -> Self {
        Self {
            source_key: QString::from(""),
            selected_track: -1, // Auto
            use_source_separation: false,
            is_source1: false,
            audio_tracks: Vec::new(),
        }
    }
}

impl ffi::SourceSettingsLogic {
    fn initialize(mut self: Pin<&mut Self>, source_json: QString) {
        let json_str = source_json.to_string();
        let data: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();

        let key = data
            .get("source_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let is_source1 = key == "Source 1";

        self.as_mut().set_source_key(QString::from(key.as_str()));
        self.as_mut().set_is_source1(is_source1);

        // Store audio tracks
        if let Some(tracks) = data.get("audio_tracks").and_then(|v| v.as_array()) {
            self.as_mut().rust_mut().audio_tracks = tracks.clone();
        }

        // Apply existing settings
        if let Some(settings) = data.get("current_settings").and_then(|v| v.as_object()) {
            if is_source1 {
                if let Some(track) = settings.get("correlation_ref_track").and_then(|v| v.as_i64())
                {
                    self.as_mut().set_selected_track(track as i32);
                }
            } else {
                if let Some(track) = settings
                    .get("correlation_source_track")
                    .and_then(|v| v.as_i64())
                {
                    self.as_mut().set_selected_track(track as i32);
                }
                if let Some(sep) = settings
                    .get("use_source_separation")
                    .and_then(|v| v.as_bool())
                {
                    self.as_mut().set_use_source_separation(sep);
                }
            }
        }
    }

    fn get_audio_tracks(self: Pin<&mut Self>) -> QString {
        // Build display entries: first entry is "Auto", then real tracks
        let mut entries = vec![serde_json::json!({
            "display": "Auto (Language Fallback)",
            "index": -1
        })];

        for (i, track) in self.rust().audio_tracks.iter().enumerate() {
            let desc = track.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let lang = track.get("lang").and_then(|v| v.as_str()).unwrap_or("und");
            let name = track.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let codec = track.get("codec_id").and_then(|v| v.as_str()).unwrap_or("unknown");
            let channels = track.get("audio_channels").and_then(|v| v.as_str()).unwrap_or("");

            let display = if !desc.is_empty() {
                format!("Track {i}: {desc}")
            } else {
                let mut parts = vec![format!("Track {i}")];
                if !lang.is_empty() && lang != "und" {
                    parts.push(format!("[{}]", lang.to_uppercase()));
                }
                if !name.is_empty() {
                    parts.push(format!("\"{name}\""));
                }
                if !channels.is_empty() {
                    parts.push(format!("({channels}ch)"));
                }
                let codec_short = codec.replace("A_", "").split('/').next().unwrap_or("").to_string();
                if !codec_short.is_empty() {
                    parts.push(format!("- {codec_short}"));
                }
                parts.join(" ")
            };

            entries.push(serde_json::json!({"display": display, "index": i}));
        }

        let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let track = *self.as_ref().selected_track();
        // -1 means Auto → null in JSON (matching Python's None)
        let track_value = if track < 0 {
            serde_json::Value::Null
        } else {
            serde_json::json!(track)
        };

        let result = if *self.as_ref().is_source1() {
            serde_json::json!({"correlation_ref_track": track_value})
        } else {
            serde_json::json!({
                "correlation_source_track": track_value,
                "use_source_separation": *self.as_ref().use_source_separation(),
            })
        };
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn has_non_default_settings(self: Pin<&mut Self>) -> bool {
        if *self.as_ref().is_source1() {
            *self.as_ref().selected_track() >= 0 // Non-auto
        } else {
            *self.as_ref().selected_track() >= 0 || *self.as_ref().use_source_separation()
        }
    }

    fn reset_to_defaults(mut self: Pin<&mut Self>) {
        self.as_mut().set_selected_track(-1); // Auto
        self.as_mut().set_use_source_separation(false);
    }
}
