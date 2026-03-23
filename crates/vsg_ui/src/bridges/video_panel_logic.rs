//! Video panel — 1:1 port of `vsg_qt/subtitle_editor/video_panel.py`.
//!
//! Video preview panel with playback controls and subtitle overlay.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// VideoPanelLogic QObject.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, video_path)]
        #[qproperty(f64, current_time)]
        #[qproperty(f64, duration)]
        #[qproperty(bool, is_playing)]
        type VideoPanelLogic = super::VideoPanelLogicRust;

        /// Load a video file for preview.
        #[qinvokable]
        fn load_video(self: Pin<&mut VideoPanelLogic>, path: QString);

        /// Seek to a specific time in seconds.
        #[qinvokable]
        fn seek_to(self: Pin<&mut VideoPanelLogic>, time_seconds: f64);

        /// Toggle play/pause.
        #[qinvokable]
        fn toggle_playback(self: Pin<&mut VideoPanelLogic>);

        /// Signal: playback position changed.
        #[qsignal]
        fn position_changed(self: Pin<&mut VideoPanelLogic>, time_seconds: f64);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct VideoPanelLogicRust {
    video_path: QString,
    current_time: f64,
    duration: f64,
    is_playing: bool,
}

impl ffi::VideoPanelLogic {
    fn load_video(self: Pin<&mut Self>, _path: QString) {}
    fn seek_to(self: Pin<&mut Self>, _time_seconds: f64) {}
    fn toggle_playback(self: Pin<&mut Self>) {}
}
