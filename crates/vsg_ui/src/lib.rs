//! VSG UI - User Interface (CXX-Qt implementation)
//!
//! This crate contains the Qt-based GUI implementation using CXX-Qt.

pub mod bridge;
pub mod core_integration;
pub mod windows;

/// Returns the UI version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
