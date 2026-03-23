//! Events table — 1:1 port of `vsg_qt/subtitle_editor/events_table.py`.
//!
//! Table view of subtitle events with inline editing.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// EventsTableLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, event_count)]
        #[qproperty(i32, selected_row)]
        type EventsTableLogic = super::EventsTableLogicRust;

        /// Get event data at row as JSON.
        #[qinvokable]
        fn get_event(self: Pin<&mut EventsTableLogic>, row: i32) -> QString;

        /// Update an event field.
        #[qinvokable]
        fn update_event_field(
            self: Pin<&mut EventsTableLogic>,
            row: i32,
            field: QString,
            value: QString,
        );

        /// Insert a new event at the given row.
        #[qinvokable]
        fn insert_event(self: Pin<&mut EventsTableLogic>, row: i32);

        /// Delete event at the given row.
        #[qinvokable]
        fn delete_event(self: Pin<&mut EventsTableLogic>, row: i32);

        /// Signal: events changed, table needs refresh.
        #[qsignal]
        fn events_changed(self: Pin<&mut EventsTableLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct EventsTableLogicRust {
    event_count: i32,
    selected_row: i32,
}

impl ffi::EventsTableLogic {
    fn get_event(self: Pin<&mut Self>, _row: i32) -> QString { QString::from("{}") }
    fn update_event_field(self: Pin<&mut Self>, _row: i32, _field: QString, _value: QString) {}
    fn insert_event(self: Pin<&mut Self>, _row: i32) {}
    fn delete_event(self: Pin<&mut Self>, _row: i32) {}
}
