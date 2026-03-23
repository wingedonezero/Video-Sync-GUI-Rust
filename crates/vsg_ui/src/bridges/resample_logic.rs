//! Resample dialog logic — 1:1 port of `vsg_qt/resample_dialog/ui.py`.
//!
//! Dialog for subtitle rescaling: source resolution → destination resolution.
//! Uses ffprobe to detect video resolution for the "From Video" feature.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// ResampleLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, source_x)]
        #[qproperty(i32, source_y)]
        #[qproperty(i32, dest_x)]
        #[qproperty(i32, dest_y)]
        type ResampleLogic = super::ResampleLogicRust;

        /// Initialize with current resolution and video path.
        #[qinvokable]
        fn initialize(self: Pin<&mut ResampleLogic>, data_json: QString);

        /// Probe video file for resolution. Returns true if successful.
        #[qinvokable]
        fn probe_video_resolution(self: Pin<&mut ResampleLogic>, video_path: QString) -> bool;

        /// Get result as (dest_x, dest_y) JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut ResampleLogic>) -> QString;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::process::Command;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

pub struct ResampleLogicRust {
    source_x: i32,
    source_y: i32,
    dest_x: i32,
    dest_y: i32,
}

impl Default for ResampleLogicRust {
    fn default() -> Self {
        Self {
            source_x: 0,
            source_y: 0,
            dest_x: 0,
            dest_y: 0,
        }
    }
}

impl ffi::ResampleLogic {
    fn initialize(mut self: Pin<&mut Self>, data_json: QString) {
        let data: serde_json::Value =
            serde_json::from_str(&data_json.to_string()).unwrap_or_default();
        if let Some(x) = data.get("current_x").and_then(|v| v.as_i64()) {
            self.as_mut().set_source_x(x as i32);
        }
        if let Some(y) = data.get("current_y").and_then(|v| v.as_i64()) {
            self.as_mut().set_source_y(y as i32);
        }
    }

    /// Probe video file for resolution — 1:1 port of `_probe_video_resolution()`.
    fn probe_video_resolution(mut self: Pin<&mut Self>, video_path: QString) -> bool {
        let path = video_path.to_string();
        if path.is_empty() {
            return false;
        }

        // Run ffprobe to get video stream dimensions
        let output = Command::new("ffprobe")
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_streams",
                "-select_streams", "v:0",
                &path,
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json_str = String::from_utf8_lossy(&out.stdout);
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(stream) = data
                        .get("streams")
                        .and_then(|s| s.as_array())
                        .and_then(|a| a.first())
                    {
                        let width = stream.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
                        let height =
                            stream.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
                        if width > 0 && height > 0 {
                            self.as_mut().set_dest_x(width as i32);
                            self.as_mut().set_dest_y(height as i32);
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        let result = serde_json::json!({
            "dest_x": *self.as_ref().dest_x(),
            "dest_y": *self.as_ref().dest_y(),
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }
}
