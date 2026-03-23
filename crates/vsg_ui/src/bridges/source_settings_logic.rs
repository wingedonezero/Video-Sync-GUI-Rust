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
        type SourceSettingsLogic = super::SourceSettingsLogicRust;

        /// Initialize with source data JSON (source_key, audio_tracks, current_settings).
        #[qinvokable]
        fn initialize(self: Pin<&mut SourceSettingsLogic>, source_json: QString);

        /// Get audio tracks for display as JSON array.
        #[qinvokable]
        fn get_audio_tracks(self: Pin<&mut SourceSettingsLogic>) -> QString;

        /// Get modified source settings as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut SourceSettingsLogic>) -> QString;

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
    selected_track: i32,
    use_source_separation: bool,
    audio_tracks: Vec<serde_json::Value>,
    is_source1: bool,
}

impl Default for SourceSettingsLogicRust {
    fn default() -> Self {
        Self {
            source_key: QString::from(""),
            selected_track: 0,
            use_source_separation: false,
            audio_tracks: Vec::new(),
            is_source1: false,
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
        self.as_mut().rust_mut().is_source1 = is_source1;

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
        let json =
            serde_json::to_string(&self.rust().audio_tracks).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let track = *self.as_ref().selected_track();
        let result = if self.rust().is_source1 {
            serde_json::json!({"correlation_ref_track": track})
        } else {
            serde_json::json!({
                "correlation_source_track": track,
                "use_source_separation": *self.as_ref().use_source_separation(),
            })
        };
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn reset_to_defaults(mut self: Pin<&mut Self>) {
        self.as_mut().set_selected_track(0);
        self.as_mut().set_use_source_separation(false);
    }
}
