//! OCR dictionary dialog logic — 1:1 port of `vsg_qt/ocr_dictionary_dialog/ui.py`.
//!
//! Custom OCR dictionary editor with tabs for replacements, word lists,
//! SubtitleEdit config, and romaji dictionary builder.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// OCRDictionaryLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, replacement_count)]
        #[qproperty(i32, word_count)]
        type OCRDictionaryLogic = super::OCRDictionaryLogicRust;

        /// Initialize with config directory path.
        #[qinvokable]
        fn initialize(self: Pin<&mut OCRDictionaryLogic>, config_dir: QString);

        /// Get all replacement rules as JSON array.
        #[qinvokable]
        fn get_replacements(self: Pin<&mut OCRDictionaryLogic>) -> QString;

        /// Save replacement rules from JSON array.
        #[qinvokable]
        fn save_replacements(self: Pin<&mut OCRDictionaryLogic>, rules_json: QString) -> bool;

        /// Get user dictionary words as JSON array of strings.
        #[qinvokable]
        fn get_user_words(self: Pin<&mut OCRDictionaryLogic>) -> QString;

        /// Add a word to the user dictionary. Returns success message.
        #[qinvokable]
        fn add_user_word(self: Pin<&mut OCRDictionaryLogic>, word: QString) -> QString;

        /// Get names list as JSON array of strings.
        #[qinvokable]
        fn get_names(self: Pin<&mut OCRDictionaryLogic>) -> QString;

        /// Add a name. Returns success message.
        #[qinvokable]
        fn add_name(self: Pin<&mut OCRDictionaryLogic>, name: QString) -> QString;

        /// Get SE dictionary config as JSON.
        #[qinvokable]
        fn get_se_config(self: Pin<&mut OCRDictionaryLogic>) -> QString;

        /// Save SE dictionary config from JSON.
        #[qinvokable]
        fn save_se_config(self: Pin<&mut OCRDictionaryLogic>, config_json: QString) -> bool;

        /// Reload all dictionaries.
        #[qinvokable]
        fn reload(self: Pin<&mut OCRDictionaryLogic>);

        /// Signal: data changed.
        #[qsignal]
        fn data_changed(self: Pin<&mut OCRDictionaryLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::path::PathBuf;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::subtitles::ocr::dictionaries::OCRDictionaries;

pub struct OCRDictionaryLogicRust {
    replacement_count: i32,
    word_count: i32,
    config_dir_path: Option<PathBuf>,
    dicts: Option<OCRDictionaries>,
}

impl Default for OCRDictionaryLogicRust {
    fn default() -> Self {
        Self {
            replacement_count: 0,
            word_count: 0,
            config_dir_path: None,
            dicts: None,
        }
    }
}

impl ffi::OCRDictionaryLogic {
    fn initialize(mut self: Pin<&mut Self>, config_dir: QString) {
        let dir = PathBuf::from(config_dir.to_string());
        let dicts = OCRDictionaries::new(Some(&dir));
        self.as_mut().rust_mut().config_dir_path = Some(dir);
        self.as_mut().rust_mut().dicts = Some(dicts);
    }

    fn get_replacements(mut self: Pin<&mut Self>) -> QString {
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            let rules = dicts.load_replacements();
            let count = rules.len() as i32;
            // Have to drop the borrow before calling set_replacement_count
            let json = serde_json::to_string(&rules).unwrap_or_else(|_| "[]".to_string());
            // Can't call set_replacement_count here due to borrow, will be set by QML
            return QString::from(json.as_str());
        }
        QString::from("[]")
    }

    fn save_replacements(mut self: Pin<&mut Self>, rules_json: QString) -> bool {
        let rules: Vec<vsg_core::subtitles::ocr::dictionaries::ReplacementRule> =
            serde_json::from_str(&rules_json.to_string()).unwrap_or_default();
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            return dicts.save_replacements(&rules);
        }
        false
    }

    fn get_user_words(mut self: Pin<&mut Self>) -> QString {
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            let words = dicts.load_user_dictionary();
            let mut sorted: Vec<&String> = words.iter().collect();
            sorted.sort();
            let json = serde_json::to_string(&sorted).unwrap_or_else(|_| "[]".to_string());
            return QString::from(json.as_str());
        }
        QString::from("[]")
    }

    fn add_user_word(mut self: Pin<&mut Self>, word: QString) -> QString {
        let word_str = word.to_string();
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            let (success, msg) = dicts.add_user_word(&word_str);
            if success {
                self.as_mut().data_changed();
            }
            return QString::from(msg.as_str());
        }
        QString::from("No dictionary loaded")
    }

    fn get_names(mut self: Pin<&mut Self>) -> QString {
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            let names = dicts.load_names();
            let mut sorted: Vec<&String> = names.iter().collect();
            sorted.sort();
            let json = serde_json::to_string(&sorted).unwrap_or_else(|_| "[]".to_string());
            return QString::from(json.as_str());
        }
        QString::from("[]")
    }

    fn add_name(mut self: Pin<&mut Self>, name: QString) -> QString {
        let name_str = name.to_string();
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            let (success, msg) = dicts.add_name(&name_str);
            if success {
                self.as_mut().data_changed();
            }
            return QString::from(msg.as_str());
        }
        QString::from("No dictionary loaded")
    }

    fn get_se_config(self: Pin<&mut Self>) -> QString {
        if let Some(dir) = &self.rust().config_dir_path {
            let config = vsg_core::subtitles::ocr::subtitle_edit::load_se_config(dir);
            let json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
            return QString::from(json.as_str());
        }
        QString::from("{}")
    }

    fn save_se_config(self: Pin<&mut Self>, config_json: QString) -> bool {
        if let Some(dir) = &self.rust().config_dir_path {
            if let Ok(config) = serde_json::from_str(&config_json.to_string()) {
                return vsg_core::subtitles::ocr::subtitle_edit::save_se_config(dir, &config);
            }
        }
        false
    }

    fn reload(mut self: Pin<&mut Self>) {
        if let Some(dicts) = self.as_mut().rust_mut().dicts.as_mut() {
            dicts.reload();
        }
        self.as_mut().data_changed();
    }
}
