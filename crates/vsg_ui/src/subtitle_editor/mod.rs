//! Subtitle editor — 1:1 port of `vsg_qt/subtitle_editor/`.
//!
//! Bridges are in `bridges/` directory (subtitle_editor_logic, events_table_logic, etc.)
//! This module contains non-QObject pure Rust code:
//! - `state/` → editor state management + undo system
//! - `tabs/` → tab trait definition
//! - `player/` → video player thread
//! - `utils/` → time formatting + CPS calculation

pub mod state;
pub mod tabs;
pub mod player;
pub mod utils;
pub mod tab_panel;
