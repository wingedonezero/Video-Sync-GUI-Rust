//! Base tab — 1:1 port of `vsg_qt/subtitle_editor/tabs/base_tab.py`.
//!
//! Common interface for editor tabs.

/// Trait that all editor tabs implement.
pub trait EditorTab {
    /// Refresh the tab contents from editor state.
    fn refresh(&mut self);

    /// Apply any pending changes from this tab.
    fn apply_changes(&mut self);

    /// Get the tab's display name.
    fn tab_name(&self) -> &str;
}
