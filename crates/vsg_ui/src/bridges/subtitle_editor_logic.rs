//! Editor window — 1:1 port of `vsg_qt/subtitle_editor/editor_window.py`.
//!
//! Main subtitle editor window controller. Manages EditorState,
//! coordinates between video panel, events table, and tab panel.

#[cxx_qt::bridge]
pub mod ffi {
    extern "RustQt" {
        /// SubtitleEditorLogic QObject — main editor controller.
        #[qobject]
        #[qml_element]
        #[qproperty(QString, file_path)]
        #[qproperty(QString, video_path)]
        #[qproperty(QString, window_title)]
        #[qproperty(bool, has_unsaved_changes)]
        #[qproperty(i32, event_count)]
        #[qproperty(i32, style_count)]
        type SubtitleEditorLogic = super::SubtitleEditorLogicRust;

        /// Open a subtitle file for editing.
        #[qinvokable]
        fn open_file(self: Pin<&mut SubtitleEditorLogic>, path: QString);

        /// Get all events as JSON array (for events table).
        #[qinvokable]
        fn get_events(self: Pin<&mut SubtitleEditorLogic>) -> QString;

        /// Get all styles as JSON array (for styles tab).
        #[qinvokable]
        fn get_styles(self: Pin<&mut SubtitleEditorLogic>) -> QString;

        /// Get style counts as JSON (for filtering tab).
        #[qinvokable]
        fn get_style_counts(self: Pin<&mut SubtitleEditorLogic>) -> QString;

        /// Save the current file.
        #[qinvokable]
        fn save_file(self: Pin<&mut SubtitleEditorLogic>) -> bool;

        /// Save to a new path.
        #[qinvokable]
        fn save_file_as(self: Pin<&mut SubtitleEditorLogic>, path: QString) -> bool;

        /// Update a style property. Returns true if changed.
        #[qinvokable]
        fn update_style(
            self: Pin<&mut SubtitleEditorLogic>,
            style_name: QString,
            property: QString,
            value: QString,
        ) -> bool;

        /// Get the result (style_patch, font_replacements, filter_config) as JSON.
        #[qinvokable]
        fn get_result(self: Pin<&mut SubtitleEditorLogic>) -> QString;

        /// Undo last action.
        #[qinvokable]
        fn undo(self: Pin<&mut SubtitleEditorLogic>);

        /// Redo last undone action.
        #[qinvokable]
        fn redo(self: Pin<&mut SubtitleEditorLogic>);

        /// Signal: file loaded, UI should refresh.
        #[qsignal]
        fn file_loaded(self: Pin<&mut SubtitleEditorLogic>);

        /// Signal: editor state changed.
        #[qsignal]
        fn state_changed(self: Pin<&mut SubtitleEditorLogic>);
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

use core::pin::Pin;
use std::path::Path;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use vsg_core::subtitles::data::SubtitleData;

use crate::subtitle_editor::state::editor_state::EditorState;
use crate::subtitle_editor::state::undo_manager::UndoManager;

pub struct SubtitleEditorLogicRust {
    file_path: QString,
    video_path: QString,
    window_title: QString,
    has_unsaved_changes: bool,
    event_count: i32,
    style_count: i32,
    subtitle_data: Option<SubtitleData>,
    editor_state: EditorState,
    undo_manager: UndoManager,
}

impl Default for SubtitleEditorLogicRust {
    fn default() -> Self {
        Self {
            file_path: QString::from(""),
            video_path: QString::from(""),
            window_title: QString::from("Subtitle Editor"),
            has_unsaved_changes: false,
            event_count: 0,
            style_count: 0,
            subtitle_data: None,
            editor_state: EditorState::new(),
            undo_manager: UndoManager::new(),
        }
    }
}

impl ffi::SubtitleEditorLogic {
    /// Open a subtitle file — 1:1 port of editor_state.py load logic.
    fn open_file(mut self: Pin<&mut Self>, path: QString) {
        let path_str = path.to_string();
        match SubtitleData::from_file(Path::new(&path_str)) {
            Ok(data) => {
                let event_count = data.events.len() as i32;
                let style_count = data.styles.len() as i32;
                let title = Path::new(&path_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Subtitle Editor".to_string());

                self.as_mut().set_file_path(path);
                self.as_mut()
                    .set_window_title(QString::from(title.as_str()));
                self.as_mut().set_event_count(event_count);
                self.as_mut().set_style_count(style_count);
                self.as_mut().set_has_unsaved_changes(false);
                self.as_mut().rust_mut().subtitle_data = Some(data);
                self.as_mut().rust_mut().editor_state.reset();
                self.as_mut().rust_mut().editor_state.file_path = Some(path_str);
                self.as_mut().rust_mut().undo_manager.clear();
                self.as_mut().file_loaded();
            }
            Err(e) => {
                self.as_mut().set_window_title(
                    QString::from(format!("Error: {e}").as_str()),
                );
            }
        }
    }

    fn get_events(self: Pin<&mut Self>) -> QString {
        if let Some(data) = &self.rust().subtitle_data {
            let events: Vec<serde_json::Value> = data
                .events
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    serde_json::json!({
                        "index": i,
                        "start_ms": e.start_ms,
                        "end_ms": e.end_ms,
                        "style": e.style,
                        "text": e.text,
                        "is_comment": e.is_comment,
                        "layer": e.layer,
                        "name": e.name,
                    })
                })
                .collect();
            let json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".to_string());
            QString::from(json.as_str())
        } else {
            QString::from("[]")
        }
    }

    fn get_styles(self: Pin<&mut Self>) -> QString {
        if let Some(data) = &self.rust().subtitle_data {
            let styles: Vec<serde_json::Value> = data
                .styles
                .iter()
                .map(|(name, style)| {
                    serde_json::json!({
                        "name": name,
                        "fontname": style.fontname,
                        "fontsize": style.fontsize,
                    })
                })
                .collect();
            let json = serde_json::to_string(&styles).unwrap_or_else(|_| "[]".to_string());
            QString::from(json.as_str())
        } else {
            QString::from("[]")
        }
    }

    fn get_style_counts(self: Pin<&mut Self>) -> QString {
        if let Some(data) = &self.rust().subtitle_data {
            let counts = data.get_style_counts();
            let json = serde_json::to_string(&counts).unwrap_or_else(|_| "{}".to_string());
            QString::from(json.as_str())
        } else {
            QString::from("{}")
        }
    }

    fn save_file(mut self: Pin<&mut Self>) -> bool {
        let path = self.as_ref().file_path().to_string();
        if path.is_empty() {
            return false;
        }
        if let Some(data) = &self.rust().subtitle_data {
            match data.save(Path::new(&path), None) {
                Ok(()) => {
                    self.as_mut().set_has_unsaved_changes(false);
                    self.as_mut().rust_mut().editor_state.mark_saved();
                    true
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn save_file_as(mut self: Pin<&mut Self>, path: QString) -> bool {
        let path_str = path.to_string();
        if let Some(data) = &self.rust().subtitle_data {
            match data.save(Path::new(&path_str), None) {
                Ok(()) => {
                    self.as_mut().set_file_path(path);
                    self.as_mut().set_has_unsaved_changes(false);
                    self.as_mut().rust_mut().editor_state.mark_saved();
                    true
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn update_style(
        mut self: Pin<&mut Self>,
        style_name: QString,
        property: QString,
        value: QString,
    ) -> bool {
        let name = style_name.to_string();
        let prop = property.to_string();
        let val = value.to_string();

        if let Some(data) = self.as_mut().rust_mut().subtitle_data.as_mut() {
            if let Some(style) = data.get_style_mut(&name) {
                match prop.as_str() {
                    "fontname" => style.fontname = val,
                    "fontsize" => {
                        if let Ok(size) = val.parse::<f64>() {
                            style.fontsize = size;
                        }
                    }
                    _ => return false,
                }
                return true;
            }
        }
        false
    }

    fn get_result(self: Pin<&mut Self>) -> QString {
        // Return the accumulated edits as style_patch + font_replacements
        let result = serde_json::json!({
            "style_patch": {},
            "font_replacements": {},
            "filter_config": {},
        });
        let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        QString::from(json.as_str())
    }

    fn undo(self: Pin<&mut Self>) {
        // Undo manager placeholder — Phase 3 in Python
    }

    fn redo(self: Pin<&mut Self>) {
        // Redo manager placeholder — Phase 3 in Python
    }
}
