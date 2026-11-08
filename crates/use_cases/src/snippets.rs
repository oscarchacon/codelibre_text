//! Use cases: snippets and their fields. `insert_snippet` (and a snippet completion) selects the
//! first field; Tab (`next_field`, while the `has_next_field` context holds) and Shift+Tab
//! (`prev_field`) visit the others, `$0` (or the end of the snippet) last; Esc (`clear_fields`)
//! stops. A mirrored field (`$1 … $1`) is selected in every place, so typing changes them all.

use entities::document::DocumentId;
use entities::selection::Selection;
use entities::text_commands as tc;

use crate::editor::Editor;

impl Editor {
    /// Replaces the selection of the active document with a snippet and selects its first field.
    /// With several selections, each one gets the snippet (its own `$SELECTION`) and a caret on
    /// its first field; Tab does not visit the other fields then.
    pub fn insert_snippet(&mut self, contents: &str) {
        let Some(doc) = self.active_document_mut() else { return };
        if doc.extra_selections().is_empty() {
            let view = doc.id;
            self.insert_snippet_over(view, None, contents);
            return;
        }
        doc.edit_all(|text, selections| {
            let mut order: Vec<usize> = (0..selections.len()).collect();
            order.sort_by_key(|&i| selections[i].start());
            let mut new = selections.to_vec();
            // In text order: each insertion moves the selections after it.
            let mut shift: isize = 0;
            for i in order {
                let moved = |p: usize| (p as isize + shift).max(0) as usize;
                let selection = Selection::new(moved(selections[i].anchor), moved(selections[i].head));
                let before = text.chars().count() as isize;
                new[i] = tc::insert_snippet(text, selection, contents);
                shift += text.chars().count() as isize - before;
            }
            new
        });
    }

    /// Replaces `replaced` (else the selection) of `view` with a snippet and selects its first field.
    pub(crate) fn insert_snippet_over(&mut self, view: DocumentId, replaced: Option<Selection>, contents: &str) {
        let Some(doc) = self.any_document_mut(view) else { return };
        let mut fields = Vec::new();
        let edited = doc.edit(|text, selection| {
            let (selected, found) = tc::insert_snippet_fields(text, replaced.unwrap_or(selection), contents);
            fields = found;
            Some(selected)
        });
        if edited {
            doc.start_snippet(fields);
        }
    }

    /// The `has_next_field` (`forward`) / `has_prev_field` context.
    pub fn has_snippet_field(&self, forward: bool) -> bool {
        self.active_document().is_some_and(|d| d.has_snippet_field(forward))
    }

    /// `next_field` / `prev_field`.
    pub fn move_to_snippet_field(&mut self, forward: bool) {
        if let Some(doc) = self.active_document_mut() {
            doc.move_to_snippet_field(forward);
        }
    }

    /// `clear_fields`.
    pub fn clear_snippet_fields(&mut self) {
        if let Some(doc) = self.active_document_mut() {
            doc.clear_snippet();
        }
    }
}
