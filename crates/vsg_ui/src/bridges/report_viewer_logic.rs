//! Report viewer — 1:1 port of `vsg_qt/report_dialogs/report_viewer.py`.
//!
//! Displays a report file with formatted text and navigation.
//! Uses `vsg_core::reporting::ReportWriter` to load and parse reports.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// ReportViewerLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, report_title)]
        #[qproperty(i32, job_count)]
        #[qproperty(i32, selected_job)]
        type ReportViewerLogic = super::ReportViewerLogicRust;

        /// Load report from file path. Returns JSON of the report data.
        #[qinvokable]
        fn load_report(self: Pin<&mut ReportViewerLogic>, path: QString) -> QString;

        /// Get job display data at index as JSON.
        #[qinvokable]
        fn get_job_data(self: Pin<&mut ReportViewerLogic>, index: i32) -> QString;

        /// Get detailed job info at index as JSON.
        #[qinvokable]
        fn get_job_details(self: Pin<&mut ReportViewerLogic>, index: i32) -> QString;

        /// Get status summary text for a job (calls ReportWriter.get_job_status_summary).
        #[qinvokable]
        fn get_job_status_summary(self: Pin<&mut ReportViewerLogic>, index: i32) -> QString;

        /// Get delays summary text for a job (calls ReportWriter.get_delays_summary).
        #[qinvokable]
        fn get_job_delays_summary(self: Pin<&mut ReportViewerLogic>, index: i32) -> QString;

        /// Get report summary as JSON {successful, warnings, failed, total}.
        #[qinvokable]
        fn get_summary(self: Pin<&mut ReportViewerLogic>) -> QString;

        /// Open the report file externally.
        #[qinvokable]
        fn open_externally(self: Pin<&mut ReportViewerLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::path::Path;
use std::process::Command;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::reporting::report_writer::ReportWriter;

pub struct ReportViewerLogicRust {
    report_title: QString,
    job_count: i32,
    selected_job: i32,
    report_data: Option<serde_json::Value>,
    report_path: String,
}

impl Default for ReportViewerLogicRust {
    fn default() -> Self {
        Self {
            report_title: QString::from(""),
            job_count: 0,
            selected_job: -1,
            report_data: None,
            report_path: String::new(),
        }
    }
}

impl ffi::ReportViewerLogic {
    /// Load report from file path — 1:1 port of `report_viewer.py::__init__`.
    fn load_report(mut self: Pin<&mut Self>, path: QString) -> QString {
        let path_str = path.to_string();
        self.as_mut().rust_mut().report_path = path_str.clone();

        match ReportWriter::load(Path::new(&path_str)) {
            Ok(data) => {
                // Extract title
                let title = data
                    .get("batch_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Report")
                    .to_string();
                self.as_mut()
                    .set_report_title(QString::from(title.as_str()));

                // Count jobs
                let jobs = data
                    .get("jobs")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                self.as_mut().set_job_count(jobs as i32);

                let json = serde_json::to_string(&data).unwrap_or_default();
                self.as_mut().rust_mut().report_data = Some(data);
                QString::from(json.as_str())
            }
            Err(e) => {
                self.as_mut()
                    .set_report_title(QString::from("Error loading report"));
                QString::from(
                    serde_json::json!({"error": e}).to_string().as_str(),
                )
            }
        }
    }

    /// Get job display data at index — for table population.
    fn get_job_data(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        let job = self
            .rust()
            .report_data
            .as_ref()
            .and_then(|d| d.get("jobs"))
            .and_then(|j| j.as_array())
            .and_then(|a| a.get(idx));

        match job {
            Some(j) => {
                let json = serde_json::to_string(j).unwrap_or_else(|_| "{}".to_string());
                QString::from(json.as_str())
            }
            None => QString::from("{}"),
        }
    }

    /// Get detailed job info at index — for details panel.
    fn get_job_details(self: Pin<&mut Self>, index: i32) -> QString {
        // Same data as get_job_data — QML formats the detail display
        self.get_job_data(index)
    }

    fn get_job_status_summary(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        let job = self
            .rust()
            .report_data
            .as_ref()
            .and_then(|d| d.get("jobs"))
            .and_then(|j| j.as_array())
            .and_then(|a| a.get(idx));

        match job {
            Some(j) => QString::from(ReportWriter::get_job_status_summary(j).as_str()),
            None => QString::from("Unknown"),
        }
    }

    fn get_job_delays_summary(self: Pin<&mut Self>, index: i32) -> QString {
        let idx = index as usize;
        let job = self
            .rust()
            .report_data
            .as_ref()
            .and_then(|d| d.get("jobs"))
            .and_then(|j| j.as_array())
            .and_then(|a| a.get(idx));

        match job {
            Some(j) => QString::from(ReportWriter::get_delays_summary(j).as_str()),
            None => QString::from("—"),
        }
    }

    fn get_summary(self: Pin<&mut Self>) -> QString {
        let summary = self
            .rust()
            .report_data
            .as_ref()
            .and_then(|d| d.get("summary"))
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let json = serde_json::to_string(&summary).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    /// Open the report file externally via xdg-open.
    fn open_externally(self: Pin<&mut Self>) {
        let path = &self.rust().report_path;
        if !path.is_empty() {
            let _ = Command::new("xdg-open").arg(path).spawn();
        }
    }
}
