//! Options dialog logic — 1:1 port of `vsg_qt/options_dialog/logic.py`.
//!
//! Syncs widget values ↔ AppSettings model.
//! In Python, OptionsLogic used getattr/setattr for dynamic field access.
//! In Rust, we serialize AppSettings to/from JSON for the same effect.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// OptionsLogic QObject — manages settings sync between UI and config.
        #[qobject]
        #[qml_element]
        type OptionsLogic = super::OptionsLogicRust;

        /// Load settings from JSON. Returns the same JSON for QML to bind.
        #[qinvokable]
        fn load_settings(self: Pin<&mut OptionsLogic>, settings_json: QString) -> QString;

        /// Validate and apply settings from QML. Takes full settings JSON.
        /// Returns JSON array of rejected keys (empty if all valid).
        #[qinvokable]
        fn validate_settings(self: Pin<&mut OptionsLogic>, settings_json: QString) -> QString;

        /// Signal: settings were saved successfully.
        #[qsignal]
        fn settings_saved(self: Pin<&mut OptionsLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use vsg_core::models::settings::AppSettings;

#[derive(Default)]
pub struct OptionsLogicRust {}

impl ffi::OptionsLogic {
    /// Load settings — passes through JSON for QML widget population.
    fn load_settings(self: Pin<&mut Self>, settings_json: QString) -> QString {
        settings_json
    }

    /// Validate settings — deserializes to check for errors.
    fn validate_settings(self: Pin<&mut Self>, settings_json: QString) -> QString {
        let json_str = settings_json.to_string();
        match serde_json::from_str::<AppSettings>(&json_str) {
            Ok(_) => QString::from("[]"),
            Err(e) => {
                let rejected = serde_json::json!([{"key": "unknown", "error": e.to_string()}]);
                let json = serde_json::to_string(&rejected).unwrap_or_else(|_| "[]".to_string());
                QString::from(json.as_str())
            }
        }
    }
}
