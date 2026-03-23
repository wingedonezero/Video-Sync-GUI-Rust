//! Filtering tab — 1:1 port of `vsg_qt/subtitle_editor/tabs/filtering_tab.py`.
//!
//! Event filtering by style, layer, or text pattern.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// FilteringTabLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, filter_text)]
        #[qproperty(QString, filter_style)]
        #[qproperty(i32, visible_event_count)]
        type FilteringTabLogic = super::FilteringTabLogicRust;

        /// Apply the current filter. Returns JSON array of visible event indices.
        #[qinvokable]
        fn apply_filter(self: Pin<&mut FilteringTabLogic>) -> QString;

        /// Clear all filters.
        #[qinvokable]
        fn clear_filter(self: Pin<&mut FilteringTabLogic>);

        /// Signal: filter changed, events table needs refresh.
        #[qsignal]
        fn filter_changed(self: Pin<&mut FilteringTabLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct FilteringTabLogicRust {
    filter_text: QString,
    filter_style: QString,
    visible_event_count: i32,
}

impl ffi::FilteringTabLogic {
    fn apply_filter(self: Pin<&mut Self>) -> QString { QString::from("[]") }
    fn clear_filter(self: Pin<&mut Self>) {}
}
