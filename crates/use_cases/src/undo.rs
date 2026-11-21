//! Use cases: `undo` / `redo` (and Sublime's `soft_undo`, `soft_redo`, `redo_or_repeat`) on the
//! documents' own history (`entities::undo`), at once, also while a plugin's event runs; and
//! where one undo step ends: each command's edits are one, a plugin's edits outside a command
//! (a timer, an event) one per frame.

use entities::document::DocumentId;

use crate::editor::Editor;

impl Editor {
    /// `undo`: the active view's last step. False when there was nothing to undo.
    pub fn undo(&mut self) -> bool {
        self.undo_or_redo(true)
    }

    /// `redo`: does again what the last `undo` took back.
    pub fn redo(&mut self) -> bool {
        self.undo_or_redo(false)
    }

    fn undo_or_redo(&mut self, undo: bool) -> bool {
        let Some(doc) = self.active_document_mut() else {
            return false;
        };
        let done = if undo { doc.undo() } else { doc.redo() };
        if done {
            let head = doc.selection().head;
            doc.request_show(head, false);
            let id = doc.id;
            self.unfold_around_carets(id);
            self.sync_clones();
        }
        done
    }

    /// A command starts or ends: the edits before it, and its own, are steps of their own.
    pub fn close_undo_steps(&mut self) {
        for doc in &mut self.state.documents {
            doc.close_command_undo_step();
        }
    }

    /// A plugin edited the view `id` (`view.insert`, `erase`, `replace`).
    pub(crate) fn note_plugin_edit(&mut self, id: DocumentId) {
        if !self.state.plugin_edited_views.contains(&id) {
            self.state.plugin_edited_views.push(id);
        }
    }

    /// Every frame: what plugins edited outside a command since the last one is one step.
    pub fn close_plugin_undo_steps(&mut self) {
        for id in std::mem::take(&mut self.state.plugin_edited_views) {
            if let Some(doc) = self.any_document_mut(id) {
                doc.close_command_undo_step();
            }
        }
    }
}
