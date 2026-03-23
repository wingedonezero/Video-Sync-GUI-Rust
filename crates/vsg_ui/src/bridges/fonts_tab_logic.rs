//! Fonts tab — 1:1 port of `vsg_qt/subtitle_editor/tabs/fonts_tab.py`.
//!
//! Font management for subtitle styles.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// FontsTabLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, font_mapping_count)]
        type FontsTabLogic = super::FontsTabLogicRust;

        /// Get font mapping at index as JSON.
        #[qinvokable]
        fn get_font_mapping(self: Pin<&mut FontsTabLogic>, index: i32) -> QString;

        /// Update a font mapping.
        #[qinvokable]
        fn update_font_mapping(
            self: Pin<&mut FontsTabLogic>,
            index: i32,
            new_font: QString,
        );

        /// Signal: font mappings changed.
        #[qsignal]
        fn mappings_changed(self: Pin<&mut FontsTabLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct FontsTabLogicRust {
    font_mapping_count: i32,
}

impl ffi::FontsTabLogic {
    fn get_font_mapping(self: Pin<&mut Self>, _index: i32) -> QString { QString::from("{}") }
    fn update_font_mapping(self: Pin<&mut Self>, _index: i32, _new_font: QString) {}
}
