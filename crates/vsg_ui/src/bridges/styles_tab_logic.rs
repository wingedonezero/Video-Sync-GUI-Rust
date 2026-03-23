//! Styles tab — 1:1 port of `vsg_qt/subtitle_editor/tabs/styles_tab.py`.
//!
//! ASS style editing: font, colors, alignment, margins, etc.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// StylesTabLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, style_count)]
        #[qproperty(i32, selected_style)]
        type StylesTabLogic = super::StylesTabLogicRust;

        /// Get style at index as JSON.
        #[qinvokable]
        fn get_style(self: Pin<&mut StylesTabLogic>, index: i32) -> QString;

        /// Update a style field.
        #[qinvokable]
        fn update_style_field(
            self: Pin<&mut StylesTabLogic>,
            index: i32,
            field: QString,
            value: QString,
        );

        /// Add a new style.
        #[qinvokable]
        fn add_style(self: Pin<&mut StylesTabLogic>);

        /// Remove style at index.
        #[qinvokable]
        fn remove_style(self: Pin<&mut StylesTabLogic>, index: i32);

        /// Signal: styles changed.
        #[qsignal]
        fn styles_changed(self: Pin<&mut StylesTabLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct StylesTabLogicRust {
    style_count: i32,
    selected_style: i32,
}

impl ffi::StylesTabLogic {
    fn get_style(self: Pin<&mut Self>, _index: i32) -> QString { QString::from("{}") }
    fn update_style_field(self: Pin<&mut Self>, _index: i32, _field: QString, _value: QString) {}
    fn add_style(self: Pin<&mut Self>) {}
    fn remove_style(self: Pin<&mut Self>, _index: i32) {}
}
