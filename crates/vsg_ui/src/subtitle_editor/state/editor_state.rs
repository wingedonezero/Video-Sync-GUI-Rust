//! Editor state — 1:1 port of `vsg_qt/subtitle_editor/state/editor_state.py`.
//!
//! Centralized state for the subtitle editor: loaded subtitle data,
//! current selection, filter state, and modification tracking.

/// The editor state struct — holds all subtitle editor state.
/// Not a QObject; owned by SubtitleEditorLogic.
#[derive(Default)]
pub struct EditorState {
    /// Path to the currently loaded subtitle file.
    pub file_path: Option<String>,
    /// Whether the file has unsaved modifications.
    pub modified: bool,
    /// Currently selected event index.
    pub selected_event: Option<usize>,
    // TODO: subtitle_data: Option<SubtitleData>,
    // TODO: filter_state: FilterState,
    // TODO: video_path: Option<String>,
}

impl EditorState {
    /// Create a new empty editor state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset state (e.g., when closing a file).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Mark as modified.
    pub fn mark_modified(&mut self) {
        self.modified = true;
    }

    /// Mark as saved (not modified).
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }
}
