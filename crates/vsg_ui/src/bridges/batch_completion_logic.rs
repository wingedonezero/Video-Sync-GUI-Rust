//! Batch completion dialog — 1:1 port of `vsg_qt/report_dialogs/batch_completion_dialog.py`.
//!
//! Shows summary after batch processing: success/warning/fail counts,
//! stepping info, and a button to open the report.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// BatchCompletionLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, total_jobs)]
        #[qproperty(i32, successful)]
        #[qproperty(i32, warnings)]
        #[qproperty(i32, failed)]
        #[qproperty(QString, report_path)]
        #[qproperty(QString, stepping_jobs_json)]
        #[qproperty(QString, stepping_disabled_jobs_json)]
        type BatchCompletionLogic = super::BatchCompletionLogicRust;

        /// Open the report file in the system viewer.
        #[qinvokable]
        fn open_report(self: Pin<&mut BatchCompletionLogic>);

        /// Open the report in the ReportViewer dialog.
        #[qinvokable]
        fn show_report(self: Pin<&mut BatchCompletionLogic>);

        /// Signal: request to open ReportViewer.
        #[qsignal]
        fn open_report_viewer(self: Pin<&mut BatchCompletionLogic>, path: QString);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::process::Command;

use cxx_qt_lib::QString;

#[derive(Default)]
pub struct BatchCompletionLogicRust {
    total_jobs: i32,
    successful: i32,
    warnings: i32,
    failed: i32,
    report_path: QString,
    stepping_jobs_json: QString,
    stepping_disabled_jobs_json: QString,
}

impl ffi::BatchCompletionLogic {
    /// Open report_path with system default viewer (xdg-open on Linux).
    fn open_report(self: Pin<&mut Self>) {
        let path = self.report_path().to_string();
        if !path.is_empty() {
            let _ = Command::new("xdg-open").arg(&path).spawn();
        }
    }

    /// Signal QML to open the ReportViewer dialog.
    fn show_report(mut self: Pin<&mut Self>) {
        let path = self.as_ref().report_path().clone();
        self.as_mut().open_report_viewer(path);
    }
}
