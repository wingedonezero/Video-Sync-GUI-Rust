//! Manual selection widgets — 1:1 port of `vsg_qt/manual_selection_dialog/widgets.py`.
//!
//! Defines SourceSection and AttachmentSection components.
//! In Python these were QGroupBox subclasses; in QML they are
//! declarative components. This file provides backing logic.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// SourceSectionLogic — manages a collapsible source track section.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, source_key)]
        #[qproperty(QString, source_path)]
        #[qproperty(i32, track_count)]
        #[qproperty(bool, expanded)]
        type SourceSectionLogic = super::SourceSectionLogicRust;

        /// Get track info at index as JSON.
        #[qinvokable]
        fn get_track_info(self: Pin<&mut SourceSectionLogic>, index: i32) -> QString;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct SourceSectionLogicRust {
    source_key: QString,
    source_path: QString,
    track_count: i32,
    expanded: bool,
}

impl ffi::SourceSectionLogic {
    fn get_track_info(self: Pin<&mut Self>, _index: i32) -> QString {
        // TODO: Return track info as JSON
        QString::from("{}")
    }
}
