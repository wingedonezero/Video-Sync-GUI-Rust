//! Track settings logic — 1:1 port of `vsg_qt/track_settings_dialog/logic.py`.
//!
//! Manages per-track settings: language, name, flags, OCR,
//! subtitle conversion, sync exclusions, style editing.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// TrackSettingsLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, custom_lang)]
        #[qproperty(QString, custom_name)]
        #[qproperty(bool, perform_ocr)]
        #[qproperty(bool, convert_to_ass)]
        #[qproperty(bool, rescale)]
        #[qproperty(f64, size_multiplier)]
        #[qproperty(QString, track_type)]
        #[qproperty(QString, codec_id)]
        #[qproperty(bool, ocr_available)]
        #[qproperty(bool, convert_available)]
        #[qproperty(bool, sync_exclusion_available)]
        type TrackSettingsLogic = super::TrackSettingsLogicRust;

        /// Initialize from track data JSON.
        #[qinvokable]
        fn initialize(self: Pin<&mut TrackSettingsLogic>, track_json: QString);

        /// Get the modified settings as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut TrackSettingsLogic>) -> QString;

        /// Get available languages as JSON array of {display, code} objects.
        #[qinvokable]
        fn get_languages(self: Pin<&mut TrackSettingsLogic>) -> QString;

        /// Signal: request to open sync exclusion dialog.
        #[qsignal]
        fn open_sync_exclusion(self: Pin<&mut TrackSettingsLogic>);

        /// Signal: request to open style editor.
        #[qsignal]
        fn open_style_editor(self: Pin<&mut TrackSettingsLogic>);

        /// Signal: request to open font replacement dialog.
        #[qsignal]
        fn open_font_replacements(self: Pin<&mut TrackSettingsLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

pub struct TrackSettingsLogicRust {
    custom_lang: QString,
    custom_name: QString,
    perform_ocr: bool,
    convert_to_ass: bool,
    rescale: bool,
    size_multiplier: f64,
    track_type: QString,
    codec_id: QString,
    ocr_available: bool,
    convert_available: bool,
    sync_exclusion_available: bool,
}

impl Default for TrackSettingsLogicRust {
    fn default() -> Self {
        Self {
            custom_lang: QString::from(""),
            custom_name: QString::from(""),
            perform_ocr: false,
            convert_to_ass: false,
            rescale: false,
            size_multiplier: 1.0,
            track_type: QString::from(""),
            codec_id: QString::from(""),
            ocr_available: false,
            convert_available: false,
            sync_exclusion_available: false,
        }
    }
}

impl ffi::TrackSettingsLogic {
    /// Initialize from track data and determine available options.
    fn initialize(mut self: Pin<&mut Self>, track_json: QString) {
        let data: serde_json::Value =
            serde_json::from_str(&track_json.to_string()).unwrap_or_default();

        let track_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let codec = data
            .get("codec_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_uppercase();

        self.as_mut().set_track_type(QString::from(track_type));
        self.as_mut().set_codec_id(QString::from(codec.as_str()));

        // Determine available options based on codec — 1:1 with Python logic
        let ocr_available = codec.contains("VOBSUB") || codec.contains("PGS") || codec.contains("HDMV");
        let convert_available = codec.contains("UTF8") || codec.contains("SRT");
        let sync_exclusion_available =
            codec.contains("ASS") || codec.contains("SSA");

        self.as_mut().set_ocr_available(ocr_available);
        self.as_mut().set_convert_available(convert_available);
        self.as_mut()
            .set_sync_exclusion_available(sync_exclusion_available);

        // Apply existing values
        if let Some(lang) = data.get("custom_lang").and_then(|v| v.as_str()) {
            self.as_mut().set_custom_lang(QString::from(lang));
        }
        if let Some(name) = data.get("custom_name").and_then(|v| v.as_str()) {
            self.as_mut().set_custom_name(QString::from(name));
        }
        if let Some(ocr) = data.get("perform_ocr").and_then(|v| v.as_bool()) {
            self.as_mut().set_perform_ocr(ocr);
        }
        if let Some(conv) = data.get("convert_to_ass").and_then(|v| v.as_bool()) {
            self.as_mut().set_convert_to_ass(conv);
        }
        if let Some(resc) = data.get("rescale").and_then(|v| v.as_bool()) {
            self.as_mut().set_rescale(resc);
        }
        if let Some(mult) = data.get("size_multiplier").and_then(|v| v.as_f64()) {
            self.as_mut().set_size_multiplier(mult);
        }
    }

    /// Get the settings result — 1:1 port of `read_values()`.
    fn get_result(self: Pin<&mut Self>) -> QString {
        let result = serde_json::json!({
            "custom_lang": self.as_ref().custom_lang().to_string(),
            "custom_name": self.as_ref().custom_name().to_string(),
            "perform_ocr": *self.as_ref().perform_ocr(),
            "convert_to_ass": *self.as_ref().convert_to_ass(),
            "rescale": *self.as_ref().rescale(),
            "size_multiplier": *self.as_ref().size_multiplier(),
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    /// Get language codes — 1:1 port of `LANGUAGE_CODES` from Python logic.py.
    fn get_languages(self: Pin<&mut Self>) -> QString {
        // Exact match with Python's LANGUAGE_CODES list
        let langs = serde_json::json!([
            {"display": "Keep Original", "code": ""},
            {"display": "Undetermined (und)", "code": "und"},
            {"display": "English (eng)", "code": "eng"},
            {"display": "Japanese (jpn)", "code": "jpn"},
            {"display": "Chinese (zho)", "code": "zho"},
            {"display": "Spanish (spa)", "code": "spa"},
            {"display": "French (fra)", "code": "fra"},
            {"display": "German (deu)", "code": "deu"},
            {"display": "Italian (ita)", "code": "ita"},
            {"display": "Portuguese (por)", "code": "por"},
            {"display": "Russian (rus)", "code": "rus"},
            {"display": "Korean (kor)", "code": "kor"},
            {"display": "Arabic (ara)", "code": "ara"},
            {"display": "Turkish (tur)", "code": "tur"},
            {"display": "Polish (pol)", "code": "pol"},
            {"display": "Dutch (nld)", "code": "nld"},
            {"display": "Swedish (swe)", "code": "swe"},
            {"display": "Norwegian (nor)", "code": "nor"},
            {"display": "Finnish (fin)", "code": "fin"},
            {"display": "Danish (dan)", "code": "dan"},
            {"display": "Czech (ces)", "code": "ces"},
            {"display": "Hungarian (hun)", "code": "hun"},
            {"display": "Greek (ell)", "code": "ell"},
            {"display": "Hebrew (heb)", "code": "heb"},
            {"display": "Thai (tha)", "code": "tha"},
            {"display": "Vietnamese (vie)", "code": "vie"},
            {"display": "Hindi (hin)", "code": "hin"},
        ]);
        let json = serde_json::to_string(&langs).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }
}
