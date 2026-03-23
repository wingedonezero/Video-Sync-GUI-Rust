//! CXX-Qt bridge definitions — all QObject bridges in one directory.
//!
//! Due to Qt bug QTBUG-93443, all CXX-Qt bridge files for a QML module
//! must reside in the same directory. Each file here maps 1:1 to a
//! Python logic file in vsg_qt/.
//!
//! Naming convention: `{module}_{role}.rs`
//!   - main_controller.rs       → vsg_qt/main_window/controller.py
//!   - worker_signals.rs        → vsg_qt/worker/signals.py
//!   - job_queue_logic.rs       → vsg_qt/job_queue_dialog/logic.py
//!   - add_job_logic.rs         → vsg_qt/add_job_dialog/ui.py
//!   - options_logic.rs         → vsg_qt/options_dialog/logic.py
//!   - manual_selection_logic.rs → vsg_qt/manual_selection_dialog/logic.py
//!   - source_section_logic.rs  → vsg_qt/manual_selection_dialog/widgets.py
//!   - track_widget_logic.rs    → vsg_qt/track_widget/logic.py
//!   - track_settings_logic.rs  → vsg_qt/track_settings_dialog/logic.py
//!   - sync_exclusion_logic.rs  → vsg_qt/sync_exclusion_dialog/ui.py
//!   - source_settings_logic.rs → vsg_qt/source_settings_dialog/dialog.py
//!   - resample_logic.rs        → vsg_qt/resample_dialog/ui.py
//!   - favorites_logic.rs       → vsg_qt/favorites_dialog/ui.py
//!   - font_manager_logic.rs    → vsg_qt/font_manager_dialog/ui.py
//!   - ocr_dictionary_logic.rs  → vsg_qt/ocr_dictionary_dialog/ui.py
//!   - batch_completion_logic.rs → vsg_qt/report_dialogs/batch_completion_dialog.py
//!   - report_viewer_logic.rs   → vsg_qt/report_dialogs/report_viewer.py
//!   - subtitle_editor_logic.rs → vsg_qt/subtitle_editor/editor_window.py
//!   - events_table_logic.rs    → vsg_qt/subtitle_editor/events_table.py
//!   - video_panel_logic.rs     → vsg_qt/subtitle_editor/video_panel.py
//!   - styles_tab_logic.rs      → vsg_qt/subtitle_editor/tabs/styles_tab.py
//!   - fonts_tab_logic.rs       → vsg_qt/subtitle_editor/tabs/fonts_tab.py
//!   - filtering_tab_logic.rs   → vsg_qt/subtitle_editor/tabs/filtering_tab.py

pub mod main_controller;
pub mod worker_signals;
pub mod job_queue_logic;
pub mod add_job_logic;
pub mod options_logic;
pub mod manual_selection_logic;
pub mod source_section_logic;
pub mod track_widget_logic;
pub mod track_settings_logic;
pub mod sync_exclusion_logic;
pub mod source_settings_logic;
pub mod resample_logic;
pub mod favorites_logic;
pub mod font_manager_logic;
pub mod ocr_dictionary_logic;
pub mod batch_completion_logic;
pub mod report_viewer_logic;
pub mod subtitle_editor_logic;
pub mod events_table_logic;
pub mod video_panel_logic;
pub mod styles_tab_logic;
pub mod fonts_tab_logic;
pub mod filtering_tab_logic;
