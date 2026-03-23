//! Options dialog tabs — 1:1 port of `vsg_qt/options_dialog/tabs.py`.
//!
//! Defines the 8 settings tabs. In Python these were QWidget subclasses
//! that created form layouts. In QML these are declarative tab components.
//! This Rust file provides any tab-specific logic that QML needs.
//!
//! Tabs (matching Python):
//! 1. General — output folder, temp folder, logs folder
//! 2. Analysis — correlation method, window size, GPU settings
//! 3. Subtitles — sync mode, OCR settings
//! 4. Chapters — chapter handling, snapping
//! 5. Muxing — mkvmerge options, track ordering
//! 6. Audio Correction — stepping, drift correction
//! 7. Neural/ML — model paths, ONNX settings
//! 8. Advanced — debug, logging level

// Tab logic is primarily handled in QML declarative bindings.
// This module exists for any Rust-side tab helpers needed later.
