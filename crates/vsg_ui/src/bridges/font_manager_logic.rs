//! Font manager dialog logic — 1:1 port of `vsg_qt/font_manager_dialog/ui.py`.
//!
//! Font replacement UI for subtitle tracks. Allows mapping
//! original fonts to replacement fonts.
//! Uses `vsg_core::font_manager` for font scanning and validation.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// FontManagerLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, replacement_count)]
        type FontManagerLogic = super::FontManagerLogicRust;

        /// Initialize with data JSON (subtitle_path, current_replacements, fonts_dir).
        #[qinvokable]
        fn initialize(self: Pin<&mut FontManagerLogic>, data_json: QString);

        /// Get file fonts (from subtitle) as JSON array.
        #[qinvokable]
        fn get_file_fonts(self: Pin<&mut FontManagerLogic>) -> QString;

        /// Get user fonts (from fonts directory) as JSON array.
        #[qinvokable]
        fn get_user_fonts(self: Pin<&mut FontManagerLogic>) -> QString;

        /// Add a font replacement mapping.
        #[qinvokable]
        fn add_replacement(
            self: Pin<&mut FontManagerLogic>,
            style_name: QString,
            new_font: QString,
            font_path: QString,
        );

        /// Remove a font replacement by style name.
        #[qinvokable]
        fn remove_replacement(self: Pin<&mut FontManagerLogic>, style_name: QString);

        /// Clear all replacements.
        #[qinvokable]
        fn clear_all(self: Pin<&mut FontManagerLogic>);

        /// Get all replacements as JSON (keyed by style name).
        #[qinvokable]
        fn get_result(self: Pin<&mut FontManagerLogic>) -> QString;

        /// Validate all replacements. Returns JSON array of issues.
        #[qinvokable]
        fn validate(self: Pin<&mut FontManagerLogic>) -> QString;

        /// Signal: replacements changed.
        #[qsignal]
        fn replacements_changed(self: Pin<&mut FontManagerLogic>);
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
use vsg_core::font_manager::validate_font_replacements;

/// A single font replacement entry.
#[derive(Clone)]
struct FontReplacement {
    style_name: String,
    new_font: String,
    font_path: String,
}

pub struct FontManagerLogicRust {
    replacement_count: i32,
    replacements: Vec<FontReplacement>,
    subtitle_path: String,
    fonts_dir: String,
}

impl Default for FontManagerLogicRust {
    fn default() -> Self {
        Self {
            replacement_count: 0,
            replacements: Vec::new(),
            subtitle_path: String::new(),
            fonts_dir: String::new(),
        }
    }
}

impl ffi::FontManagerLogic {
    fn initialize(mut self: Pin<&mut Self>, data_json: QString) {
        let data: serde_json::Value =
            serde_json::from_str(&data_json.to_string()).unwrap_or_default();

        self.as_mut().rust_mut().subtitle_path = data
            .get("subtitle_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        self.as_mut().rust_mut().fonts_dir = data
            .get("fonts_dir")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Restore existing replacements
        if let Some(existing) = data.get("current_replacements").and_then(|v| v.as_object()) {
            for (style, info) in existing {
                let font = info
                    .get("font_name")
                    .or_else(|| info.get("new_font"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let path = info
                    .get("font_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.as_mut().rust_mut().replacements.push(FontReplacement {
                    style_name: style.clone(),
                    new_font: font,
                    font_path: path,
                });
            }
        }

        let count = self.rust().replacements.len() as i32;
        self.as_mut().set_replacement_count(count);
    }

    fn get_file_fonts(self: Pin<&mut Self>) -> QString {
        // Scan subtitle file for fonts — delegate to vsg_core
        let path = &self.rust().subtitle_path;
        if path.is_empty() {
            return QString::from("[]");
        }

        // Parse subtitle to get style→font mapping
        match vsg_core::subtitles::data::SubtitleData::from_file(std::path::Path::new(path)) {
            Ok(data) => {
                let fonts: Vec<serde_json::Value> = data
                    .styles
                    .iter()
                    .map(|(name, style)| {
                        serde_json::json!({
                            "style": name,
                            "font": style.fontname,
                        })
                    })
                    .collect();
                let json = serde_json::to_string(&fonts).unwrap_or_else(|_| "[]".to_string());
                QString::from(json.as_str())
            }
            Err(_) => QString::from("[]"),
        }
    }

    fn get_user_fonts(self: Pin<&mut Self>) -> QString {
        // Scan fonts directory for available fonts
        let dir = &self.rust().fonts_dir;
        if dir.is_empty() {
            return QString::from("[]");
        }

        let mut scanner = vsg_core::font_manager::FontScanner::new(std::path::Path::new(dir));
        let fonts = scanner.scan(true);
        // Convert to JSON manually since FontInfo may not implement Serialize
        let font_values: Vec<serde_json::Value> = fonts
            .iter()
            .map(|f| serde_json::json!({
                "family": f.family_name,
                "path": f.file_path.to_string_lossy(),
            }))
            .collect();
        let json = serde_json::to_string(&font_values).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }

    fn add_replacement(
        mut self: Pin<&mut Self>,
        style_name: QString,
        new_font: QString,
        font_path: QString,
    ) {
        let name = style_name.to_string();
        // Remove existing replacement for this style if any
        self.as_mut()
            .rust_mut()
            .replacements
            .retain(|r| r.style_name != name);
        self.as_mut().rust_mut().replacements.push(FontReplacement {
            style_name: name,
            new_font: new_font.to_string(),
            font_path: font_path.to_string(),
        });
        let count = self.rust().replacements.len() as i32;
        self.as_mut().set_replacement_count(count);
        self.as_mut().replacements_changed();
    }

    fn remove_replacement(mut self: Pin<&mut Self>, style_name: QString) {
        let name = style_name.to_string();
        self.as_mut()
            .rust_mut()
            .replacements
            .retain(|r| r.style_name != name);
        let count = self.rust().replacements.len() as i32;
        self.as_mut().set_replacement_count(count);
        self.as_mut().replacements_changed();
    }

    fn clear_all(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().replacements.clear();
        self.as_mut().set_replacement_count(0);
        self.as_mut().replacements_changed();
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let mut result: HashMap<String, serde_json::Value> = HashMap::new();
        for r in &self.rust().replacements {
            result.insert(
                r.style_name.clone(),
                serde_json::json!({
                    "font_name": r.new_font,
                    "font_path": r.font_path,
                }),
            );
        }
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn validate(self: Pin<&mut Self>) -> QString {
        let mut replacements_map: HashMap<String, HashMap<String, serde_json::Value>> =
            HashMap::new();
        for r in &self.rust().replacements {
            let mut entry = HashMap::new();
            entry.insert(
                "font_name".to_string(),
                serde_json::Value::String(r.new_font.clone()),
            );
            entry.insert(
                "font_path".to_string(),
                serde_json::Value::String(r.font_path.clone()),
            );
            replacements_map.insert(r.style_name.clone(), entry);
        }

        let issues = validate_font_replacements(&replacements_map);
        let json = serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string());
        QString::from(json.as_str())
    }
}
