//! Video Sync GUI — CXX-Qt entry point.
//!
//! 1:1 port of the Python entry in `main.py`.
//! Initializes Qt, loads the QML engine, and shows the main window.

// Allow dead code for:
// - QObject signals (called from QML, not Rust)
// - Utility functions/structs not yet wired to QML
// - Worker runner (will be called from QML thread management)
#![allow(dead_code)]

// ── CXX-Qt bridges — all QObject definitions (required to be in one directory) ──
mod bridges;

// ── Non-bridge modules — pure Rust logic, helpers, state ──
// These map 1:1 to the Python vsg_qt/ directory structure.

// Worker runner: vsg_qt/worker/runner.py
mod worker;

// Track widget helpers: vsg_qt/track_widget/helpers.py
mod track_widget;

// Options dialog tabs: vsg_qt/options_dialog/tabs.py
mod options_dialog;

// Subtitle editor state & utils (non-QObject code):
mod subtitle_editor;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

fn main() {
    // Create the Qt application
    let mut app = QGuiApplication::new();

    // Create the QML engine
    let mut engine = QQmlApplicationEngine::new();

    // Load the main QML file from Qt resources
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            &QString::from("qrc:/qt/qml/com/vsg/ui/qml/main.qml"),
        ));
    }

    // Run the event loop
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
