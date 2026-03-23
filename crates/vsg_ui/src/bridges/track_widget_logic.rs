//! Track widget logic — 1:1 port of `vsg_qt/track_widget/logic.py`.
//!
//! Manages enable/disable state, track settings, and interaction
//! for a single track in the layout.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// TrackWidgetLogic QObject — logic for a single track display.
        #[qobject]
        #[qml_element]
        #[qproperty(bool, enabled)]
        #[qproperty(QString, track_type)]
        #[qproperty(QString, description)]
        #[qproperty(QString, language)]
        #[qproperty(QString, codec)]
        #[qproperty(i32, track_id)]
        #[qproperty(QString, source_key)]
        #[qproperty(bool, is_default)]
        #[qproperty(bool, is_forced)]
        #[qproperty(bool, perform_ocr)]
        #[qproperty(bool, convert_to_ass)]
        #[qproperty(bool, rescale)]
        #[qproperty(f64, size_multiplier)]
        #[qproperty(QString, custom_name)]
        #[qproperty(QString, custom_lang)]
        #[qproperty(QString, summary_text)]
        #[qproperty(QString, badge_text)]
        #[qproperty(bool, is_generated)]
        type TrackWidgetLogic = super::TrackWidgetLogicRust;

        /// Initialize from track data JSON.
        #[qinvokable]
        fn initialize(self: Pin<&mut TrackWidgetLogic>, track_json: QString);

        /// Get the current track configuration as JSON — 1:1 port of `get_config()`.
        #[qinvokable]
        fn get_config(self: Pin<&mut TrackWidgetLogic>) -> QString;

        /// Refresh the summary and badge text from current state.
        #[qinvokable]
        fn refresh_display(self: Pin<&mut TrackWidgetLogic>);

        /// Apply settings from TrackSettingsDialog result (JSON).
        #[qinvokable]
        fn apply_settings(self: Pin<&mut TrackWidgetLogic>, settings_json: QString);

        /// Signal: track data was modified.
        #[qsignal]
        fn track_modified(self: Pin<&mut TrackWidgetLogic>);

        /// Signal: request to open settings dialog for this track.
        #[qsignal]
        fn open_settings_requested(self: Pin<&mut TrackWidgetLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct TrackWidgetLogicRust {
    enabled: bool,
    track_type: QString,
    description: QString,
    language: QString,
    codec: QString,
    track_id: i32,
    source_key: QString,
    is_default: bool,
    is_forced: bool,
    perform_ocr: bool,
    convert_to_ass: bool,
    rescale: bool,
    size_multiplier: f64,
    custom_name: QString,
    custom_lang: QString,
    summary_text: QString,
    badge_text: QString,
    is_generated: bool,
    // Internal state not exposed as properties
    track_data: serde_json::Value,
}

impl Default for TrackWidgetLogicRust {
    fn default() -> Self {
        Self {
            enabled: true,
            track_type: QString::from(""),
            description: QString::from(""),
            language: QString::from(""),
            codec: QString::from(""),
            track_id: 0,
            source_key: QString::from(""),
            is_default: false,
            is_forced: false,
            perform_ocr: false,
            convert_to_ass: false,
            rescale: false,
            size_multiplier: 1.0,
            custom_name: QString::from(""),
            custom_lang: QString::from(""),
            summary_text: QString::from(""),
            badge_text: QString::from(""),
            is_generated: false,
            track_data: serde_json::Value::Null,
        }
    }
}

impl ffi::TrackWidgetLogic {
    /// Initialize from track data JSON — sets all properties from the track dict.
    fn initialize(mut self: Pin<&mut Self>, track_json: QString) {
        let json_str = track_json.to_string();
        let data: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();

        // Extract and set properties
        let track_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let codec = data.get("codec_id").and_then(|v| v.as_str()).unwrap_or("");
        let lang = data.get("lang").and_then(|v| v.as_str()).unwrap_or("und");
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let source = data.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let id = data.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        self.as_mut().set_track_type(QString::from(track_type));
        self.as_mut().set_codec(QString::from(codec));
        self.as_mut().set_language(QString::from(lang));
        self.as_mut().set_source_key(QString::from(source));
        self.as_mut().set_track_id(id);
        self.as_mut().set_is_default(
            data.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false),
        );
        self.as_mut().set_is_forced(
            data.get("is_forced_display").and_then(|v| v.as_bool()).unwrap_or(false),
        );
        self.as_mut().set_perform_ocr(
            data.get("perform_ocr").and_then(|v| v.as_bool()).unwrap_or(false),
        );
        self.as_mut().set_convert_to_ass(
            data.get("convert_to_ass").and_then(|v| v.as_bool()).unwrap_or(false),
        );
        self.as_mut().set_is_generated(
            data.get("is_generated").and_then(|v| v.as_bool()).unwrap_or(false),
        );
        self.as_mut().set_size_multiplier(
            data.get("size_multiplier").and_then(|v| v.as_f64()).unwrap_or(1.0),
        );

        // Build description
        let codec_display = crate::track_widget::helpers::format_codec_display(codec);
        let type_display = crate::track_widget::helpers::format_track_type(track_type);
        let desc = if name.is_empty() {
            format!("{type_display}: {codec_display} [{lang}]")
        } else {
            format!("{type_display}: {codec_display} [{lang}] - {name}")
        };
        self.as_mut().set_description(QString::from(desc.as_str()));

        // Store full track data for later
        self.as_mut().rust_mut().track_data = data;

        self.as_mut().refresh_display_impl();
    }

    /// Get the current configuration — 1:1 port of `logic.py::get_config()`.
    fn get_config(self: Pin<&mut Self>) -> QString {
        let config = serde_json::json!({
            "is_default": *self.as_ref().is_default(),
            "is_forced_display": *self.as_ref().is_forced(),
            "apply_track_name": true,
            "perform_ocr": *self.as_ref().perform_ocr(),
            "convert_to_ass": *self.as_ref().convert_to_ass(),
            "rescale": *self.as_ref().rescale(),
            "size_multiplier": *self.as_ref().size_multiplier(),
            "custom_lang": self.as_ref().custom_lang().to_string(),
            "custom_name": self.as_ref().custom_name().to_string(),
            "is_generated": *self.as_ref().is_generated(),
            "style_patch": self.rust().track_data.get("style_patch"),
            "font_replacements": self.rust().track_data.get("font_replacements"),
            "source_track_id": self.rust().track_data.get("source_track_id"),
            "filter_config": self.rust().track_data.get("filter_config"),
            "original_style_list": self.rust().track_data.get("original_style_list"),
            "sync_exclusion_styles": self.rust().track_data.get("sync_exclusion_styles").unwrap_or(&serde_json::json!([])),
            "sync_exclusion_mode": self.rust().track_data.get("sync_exclusion_mode").unwrap_or(&serde_json::json!("")),
            "sync_exclusion_original_style_list": self.rust().track_data.get("sync_exclusion_original_style_list").unwrap_or(&serde_json::json!([])),
        });
        let json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    /// Refresh display text.
    fn refresh_display(self: Pin<&mut Self>) {
        self.refresh_display_impl();
    }

    /// Apply settings from TrackSettingsDialog.
    fn apply_settings(mut self: Pin<&mut Self>, settings_json: QString) {
        let settings: serde_json::Value =
            serde_json::from_str(&settings_json.to_string()).unwrap_or_default();

        if let Some(lang) = settings.get("custom_lang").and_then(|v| v.as_str()) {
            self.as_mut().set_custom_lang(QString::from(lang));
        }
        if let Some(name) = settings.get("custom_name").and_then(|v| v.as_str()) {
            self.as_mut().set_custom_name(QString::from(name));
        }
        if let Some(ocr) = settings.get("perform_ocr").and_then(|v| v.as_bool()) {
            self.as_mut().set_perform_ocr(ocr);
        }
        if let Some(conv) = settings.get("convert_to_ass").and_then(|v| v.as_bool()) {
            self.as_mut().set_convert_to_ass(conv);
        }
        if let Some(resc) = settings.get("rescale").and_then(|v| v.as_bool()) {
            self.as_mut().set_rescale(resc);
        }
        if let Some(mult) = settings.get("size_multiplier").and_then(|v| v.as_f64()) {
            self.as_mut().set_size_multiplier(mult);
        }

        // Store sync exclusion data in track_data
        for key in &[
            "sync_exclusion_styles",
            "sync_exclusion_mode",
            "sync_exclusion_original_style_list",
        ] {
            if let Some(val) = settings.get(*key) {
                if let Some(obj) = self.as_mut().rust_mut().track_data.as_object_mut() {
                    obj.insert(key.to_string(), val.clone());
                }
            }
        }

        self.as_mut().refresh_display_impl();
        self.as_mut().track_modified();
    }

    /// Internal: refresh summary and badge text.
    fn refresh_display_impl(mut self: Pin<&mut Self>) {
        let track_type = self.as_ref().track_type().to_string();
        let desc = self.as_ref().description().to_string();
        let source = self.as_ref().source_key().to_string();

        // Build summary
        let summary = format!("[{source}] {desc}");
        self.as_mut().set_summary_text(QString::from(summary.as_str()));

        // Build badges
        let mut badges = Vec::new();
        if *self.as_ref().is_default() {
            badges.push("DEFAULT");
        }
        if *self.as_ref().is_forced() {
            badges.push("FORCED");
        }
        if *self.as_ref().is_generated() {
            badges.push("GENERATED");
        }
        if !self.as_ref().custom_lang().to_string().is_empty() {
            badges.push("LANG");
        }
        if !self.as_ref().custom_name().to_string().is_empty() {
            badges.push("NAMED");
        }
        if track_type == "subtitles" {
            if *self.as_ref().perform_ocr() {
                badges.push("OCR");
            }
            if *self.as_ref().convert_to_ass() {
                badges.push("→ASS");
            }
            if self
                .rust()
                .track_data
                .get("style_patch")
                .map_or(false, |v| !v.is_null())
            {
                badges.push("STYLED");
            }
            if self
                .rust()
                .track_data
                .get("sync_exclusion_styles")
                .and_then(|v| v.as_array())
                .map_or(false, |a| !a.is_empty())
            {
                badges.push("SYNC-EX");
            }
        }

        let badge_str = badges.join(" | ");
        self.as_mut().set_badge_text(QString::from(badge_str.as_str()));
    }
}
