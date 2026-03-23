use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("com.vsg.ui")
            // Entry + Main window
            .qml_file("qml/main.qml")
            .qml_file("qml/MainWindow.qml")
            // Dialogs — 1:1 with Python vsg_qt/ ui.py files
            .qml_file("qml/AddJobDialog.qml")
            .qml_file("qml/JobQueueDialog.qml")
            .qml_file("qml/OptionsDialog.qml")
            .qml_file("qml/ManualSelectionDialog.qml")
            .qml_file("qml/TrackWidget.qml")
            .qml_file("qml/TrackSettingsDialog.qml")
            .qml_file("qml/SyncExclusionDialog.qml")
            .qml_file("qml/SourceSettingsDialog.qml")
            .qml_file("qml/ResampleDialog.qml")
            .qml_file("qml/FavoritesDialog.qml")
            .qml_file("qml/FontManagerDialog.qml")
            .qml_file("qml/OCRDictionaryDialog.qml")
            .qml_file("qml/BatchCompletionDialog.qml")
            .qml_file("qml/ReportViewerDialog.qml")
            // Subtitle editor
            .qml_file("qml/subtitle_editor/SubtitleEditorWindow.qml"),
    )
    // All CXX-Qt bridge files in src/bridges/ (single directory required by QTBUG-93443).
    .files([
        "src/bridges/main_controller.rs",
        "src/bridges/worker_signals.rs",
        "src/bridges/job_queue_logic.rs",
        "src/bridges/add_job_logic.rs",
        "src/bridges/options_logic.rs",
        "src/bridges/manual_selection_logic.rs",
        "src/bridges/source_section_logic.rs",
        "src/bridges/track_widget_logic.rs",
        "src/bridges/track_settings_logic.rs",
        "src/bridges/sync_exclusion_logic.rs",
        "src/bridges/source_settings_logic.rs",
        "src/bridges/resample_logic.rs",
        "src/bridges/favorites_logic.rs",
        "src/bridges/font_manager_logic.rs",
        "src/bridges/ocr_dictionary_logic.rs",
        "src/bridges/batch_completion_logic.rs",
        "src/bridges/report_viewer_logic.rs",
        "src/bridges/subtitle_editor_logic.rs",
        "src/bridges/events_table_logic.rs",
        "src/bridges/video_panel_logic.rs",
        "src/bridges/styles_tab_logic.rs",
        "src/bridges/fonts_tab_logic.rs",
        "src/bridges/filtering_tab_logic.rs",
    ])
    .build();
}
