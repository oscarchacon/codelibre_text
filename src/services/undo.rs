//! Undo/Redo system based on Command Pattern
//! Supports atomic operations grouped together

use crate::domain::text_buffer::{TextBuffer, TextChange, ChangeType, Position};

/// An undoable action
#[derive(Debug, Clone)]
pub struct UndoableAction {
    pub changes: Vec<TextChange>,
    pub description: String,
}

/// Undo/Redo manager for a text buffer
#[derive(Debug)]
pub struct UndoManager {
    actions: Vec<UndoableAction>,
    redo_stack: Vec<UndoableAction>,
    max_actions: usize,
}

impl UndoManager {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            redo_stack: Vec::new(),
            max_actions: 1000,
        }
    }

    /// Record a change action
    pub fn record(&mut self, action: UndoableAction) {
        // Clear redo stack on new change
        self.redo_stack.clear();

        self.actions.push(action);

        // Limit undo history size
        if self.actions.len() > self.max_actions {
            self.actions.drain(..self.actions.len() - self.max_actions);
        }
    }

    /// Undo the last action
    pub fn undo(&mut self, buffer: &mut TextBuffer) -> bool {
        if let Some(action) = self.actions.pop() {
            // Reverse the changes
            for change in action.changes.iter().rev() {
                match change.change_type {
                    ChangeType::Insert => {
                        buffer.delete(change.position.0..change.position.0 + change.new_text.len());
                    }
                    ChangeType::Delete => {
                        buffer.insert(change.position, &change.old_text);
                    }
                    ChangeType::Replace => {
                        buffer.delete(change.position.0..change.position.0 + change.old_text.len());
                        buffer.insert(change.position, &change.new_text);
                    }
                }
            }
            buffer.set_modified(!self.actions.is_empty());

            // Push to redo stack
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self, buffer: &mut TextBuffer) -> bool {
        if let Some(action) = self.redo_stack.pop() {
            // Apply the changes again
            for change in action.changes.iter() {
                match change.change_type {
                    ChangeType::Insert => {
                        buffer.insert(change.position, &change.new_text);
                    }
                    ChangeType::Delete => {
                        buffer.delete(change.position.0..change.position.0 + change.old_text.len());
                    }
                    ChangeType::Replace => {
                        buffer.delete(change.position.0..change.position.0 + change.old_text.len());
                        buffer.insert(change.position, &change.new_text);
                    }
                }
            }
            buffer.set_modified(true);

            // Push back to undo stack
            self.actions.push(action);
            true
        } else {
            false
        }
    }

    /// Clear undo history
    pub fn clear(&mut self) {
        self.actions.clear();
        self.redo_stack.clear();
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.actions.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get the number of undo actions available
    pub fn undo_count(&self) -> usize {
        self.actions.len()
    }

    /// Get the number of redo actions available
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}
