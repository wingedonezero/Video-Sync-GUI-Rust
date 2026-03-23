//! Undo manager — 1:1 port of `vsg_qt/subtitle_editor/state/undo_manager.py`.
//!
//! Tracks undo/redo history for subtitle editing operations.

/// A single undoable action.
#[derive(Clone, Debug)]
pub struct UndoAction {
    /// Description of what this action does.
    pub description: String,
    /// Serialized state before this action (JSON snapshot).
    pub before_state: String,
    /// Serialized state after this action (JSON snapshot).
    pub after_state: String,
}

/// Manages undo/redo stacks.
#[derive(Default)]
pub struct UndoManager {
    undo_stack: Vec<UndoAction>,
    redo_stack: Vec<UndoAction>,
    max_history: usize,
}

impl UndoManager {
    /// Create a new undo manager with default history size.
    pub fn new() -> Self {
        Self {
            max_history: 100,
            ..Self::default()
        }
    }

    /// Push a new action onto the undo stack.
    pub fn push(&mut self, action: UndoAction) {
        self.redo_stack.clear();
        self.undo_stack.push(action);
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last action. Returns the action if available.
    pub fn undo(&mut self) -> Option<&UndoAction> {
        if let Some(action) = self.undo_stack.pop() {
            self.redo_stack.push(action);
            self.redo_stack.last()
        } else {
            None
        }
    }

    /// Redo the last undone action. Returns the action if available.
    pub fn redo(&mut self) -> Option<&UndoAction> {
        if let Some(action) = self.redo_stack.pop() {
            self.undo_stack.push(action);
            self.undo_stack.last()
        } else {
            None
        }
    }

    /// Check if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
