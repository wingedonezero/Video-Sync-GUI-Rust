use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("com.vsg.ui")
            .qml_file("qml/main.qml")
            .qml_file("qml/MainWindow.qml"),
    )
    // All CXX-Qt bridge files in src/bridges/ (single directory required by QTBUG-93443).
    // Each file maps 1:1 to a Python vsg_qt/ source file.
    .files([
        "src/bridges/main_controller.rs",         // main_window/controller.py
        "src/bridges/worker_signals.rs",           // worker/signals.py
        "src/bridges/job_queue_logic.rs",           // job_queue_dialog/logic.py
        "src/bridges/add_job_logic.rs",             // add_job_dialog/ui.py
        "src/bridges/options_logic.rs",             // options_dialog/logic.py
        "src/bridges/manual_selection_logic.rs",    // manual_selection_dialog/logic.py
        "src/bridges/source_section_logic.rs",      // manual_selection_dialog/widgets.py
        "src/bridges/track_widget_logic.rs",        // track_widget/logic.py
        "src/bridges/track_settings_logic.rs",      // track_settings_dialog/logic.py
        "src/bridges/sync_exclusion_logic.rs",      // sync_exclusion_dialog/ui.py
        "src/bridges/source_settings_logic.rs",     // source_settings_dialog/dialog.py
        "src/bridges/resample_logic.rs",            // resample_dialog/ui.py
        "src/bridges/favorites_logic.rs",           // favorites_dialog/ui.py
        "src/bridges/font_manager_logic.rs",        // font_manager_dialog/ui.py
        "src/bridges/ocr_dictionary_logic.rs",      // ocr_dictionary_dialog/ui.py
        "src/bridges/batch_completion_logic.rs",    // report_dialogs/batch_completion_dialog.py
        "src/bridges/report_viewer_logic.rs",       // report_dialogs/report_viewer.py
        "src/bridges/subtitle_editor_logic.rs",     // subtitle_editor/editor_window.py
        "src/bridges/events_table_logic.rs",        // subtitle_editor/events_table.py
        "src/bridges/video_panel_logic.rs",         // subtitle_editor/video_panel.py
        "src/bridges/styles_tab_logic.rs",          // subtitle_editor/tabs/styles_tab.py
        "src/bridges/fonts_tab_logic.rs",           // subtitle_editor/tabs/fonts_tab.py
        "src/bridges/filtering_tab_logic.rs",       // subtitle_editor/tabs/filtering_tab.py
    ])
    .build();
}
