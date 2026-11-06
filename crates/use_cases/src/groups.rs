//! Use cases: the groups of the window (the panes of its layout), which one has the focus, and
//! moving views between them (Sublime's `set_layout`, `focus_group`, `move_to_group`).
//!
//! Each document knows its group (`Document::group`); the tabs of a group are its documents in
//! the order of `EditorState::documents`. The view a group shows is the one of its documents used
//! most recently. The focused group may be empty: then there is no active document.

use entities::document::DocumentId;
use entities::layout::Layout;

use crate::editor::Editor;

/// `EditorState::active` when the focused group has no documents.
pub(crate) const NO_DOCUMENT: usize = usize::MAX;

impl Editor {
    pub fn layout(&self) -> &Layout {
        &self.state.layout
    }

    pub fn group_count(&self) -> usize {
        self.state.layout.group_count()
    }

    /// The group with the focus (where new files open, what the keys act on).
    pub fn active_group(&self) -> usize {
        self.state.active_group
    }

    /// Indexes (in `documents`) of the tabs of `group`, in order.
    pub fn group_indexes(&self, group: usize) -> Vec<usize> {
        (0..self.state.documents.len()).filter(|&i| self.state.documents[i].group == group).collect()
    }

    /// The document `group` shows: the one of its documents used last.
    pub fn active_index_in_group(&self, group: usize) -> Option<usize> {
        let in_group = |&i: &usize| self.state.documents[i].group == group;
        self.state
            .recently_used
            .iter()
            .filter_map(|id| self.index_of(*id))
            .find(in_group)
            .or_else(|| (0..self.state.documents.len()).find(in_group))
    }

    /// Splits the window as `layout` says. Documents of groups it no longer has go to its last
    /// group, as in Sublime.
    pub fn set_layout(&mut self, layout: Layout) {
        let last = layout.group_count() - 1;
        for doc in &mut self.state.documents {
            doc.group = doc.group.min(last);
        }
        self.state.layout = layout;
        match self.active_document().map(|d| d.group) {
            Some(group) => self.state.active_group = group,
            None => self.focus_group(self.state.active_group.min(last)),
        }
    }

    /// Gives the focus to `group` and its document, if it has one.
    pub fn focus_group(&mut self, group: usize) {
        if group >= self.group_count() {
            return;
        }
        match self.active_index_in_group(group) {
            Some(index) => self.activate(index),
            None => {
                self.state.active_group = group;
                self.state.active = NO_DOCUMENT;
            }
        }
    }

    /// `focus_neighboring_group`: the next (or previous) group, wrapping around.
    pub fn focus_neighboring_group(&mut self, forward: bool) {
        let group = self.state.layout.neighbor(self.state.active_group, forward);
        self.focus_group(group);
    }

    /// Puts a document in `group` at `index` among its tabs (the end when beyond them). The
    /// focus does not change, unless the document was the active one: it takes the focus along.
    pub fn set_view_index(&mut self, id: DocumentId, group: usize, index: usize) {
        if group >= self.group_count() {
            return;
        }
        let Some(from) = self.index_of(id) else { return };
        let active = self.active_document().map(|d| d.id);
        let mut doc = self.state.documents.remove(from);
        doc.group = group;
        let tabs = self.group_indexes(group);
        let to = match tabs.get(index) {
            Some(&at) => at,
            None => tabs.last().map_or(self.state.documents.len(), |&last| last + 1),
        };
        self.state.documents.insert(to, doc);
        if let Some(active) = active.and_then(|a| self.index_of(a)) {
            self.state.active = active;
            self.state.active_group = self.state.documents[active].group;
        }
    }

    /// Where a document is: its group and its index among the group's tabs.
    pub fn view_index(&self, id: DocumentId) -> Option<(usize, usize)> {
        let index = self.index_of(id)?;
        let group = self.state.documents[index].group;
        Some((group, self.group_indexes(group).iter().position(|&i| i == index)?))
    }

    /// `move_to_group`: the active document goes to the end of `group`'s tabs, and the focus
    /// goes with it.
    pub fn move_active_to_group(&mut self, group: usize) {
        if let Some(id) = self.active_document().map(|d| d.id) {
            self.set_view_index(id, group, usize::MAX);
            self.activate_document(id);
        }
    }

    /// `move_to_neighboring_group`.
    pub fn move_active_to_neighboring_group(&mut self, forward: bool) {
        let group = self.state.layout.neighbor(self.state.active_group, forward);
        self.move_active_to_group(group);
    }

    /// `select_by_index` (Alt+1…): the tab at `index` of the focused group.
    pub fn activate_in_group(&mut self, index: usize) {
        if let Some(&at) = self.group_indexes(self.state.active_group).get(index) {
            self.activate(at);
        }
    }

    /// The `max_columns` window setting (`set_max_columns`): `new_pane` adds columns up to it,
    /// then rows. Sublime's default is 4.
    pub fn max_columns(&self) -> usize {
        self.state.plugins.window_settings.get("max_columns").and_then(serde_json::Value::as_u64).map_or(4, |n| n.max(1) as usize)
    }

    /// `new_pane`: one more group, the layout flowing in rows of `max_columns`; with `move`, the
    /// active document goes to it (and the focus).
    pub fn new_pane(&mut self, move_view: bool) {
        let count = self.group_count() + 1;
        let layout = Layout::flow(count, self.max_columns());
        let active = self.active_document().map(|d| d.id);
        self.set_layout(layout);
        match active.filter(|_| move_view) {
            Some(id) => {
                self.set_view_index(id, count - 1, usize::MAX);
                self.activate_document(id);
            }
            None => self.focus_group(count - 1),
        }
    }

    /// `close_pane`: the group (the focused one when `None`) goes; its documents join the group
    /// before it (the next one for the first), and the layout flows again.
    pub fn close_pane(&mut self, group: Option<usize>) {
        let count = self.group_count();
        let group = group.unwrap_or(self.state.active_group);
        if count < 2 || group >= count {
            return;
        }
        let into = group.saturating_sub(1);
        for doc in &mut self.state.documents {
            if doc.group == group {
                doc.group = into;
            } else if doc.group > group {
                doc.group -= 1;
            }
        }
        let focused = self.state.active_group;
        self.state.active_group = if focused == group { into } else if focused > group { focused - 1 } else { focused };
        self.set_layout(Layout::flow(count - 1, self.max_columns()));
        self.focus_group(self.state.active_group);
    }

    /// `set_max_columns`: groups flow in rows of at most `columns` from now on (the current ones too).
    pub fn set_max_columns(&mut self, columns: usize) {
        let columns = columns.max(1);
        self.state.plugins.window_settings.insert("max_columns".into(), serde_json::json!(columns));
        let count = self.group_count();
        if count > 1 {
            self.set_layout(Layout::flow(count, columns));
        }
    }

    /// Dragging the line between two columns (`vertical`) or rows: boundary `index` to `fraction`.
    pub fn move_layout_boundary(&mut self, vertical: bool, index: usize, fraction: f64) {
        self.state.layout.move_boundary(vertical, index, fraction);
    }

    /// A tab dragged to `group`, at `index` among its tabs: it goes there and takes the focus.
    pub fn drop_view_in_group(&mut self, id: DocumentId, group: usize, index: usize) {
        if group >= self.group_count() || self.index_of(id).is_none() {
            return;
        }
        self.set_view_index(id, group, index);
        self.activate_document(id);
    }

    /// Where a new document goes: in the focused group, after its active document
    /// (`after_active`) or after its last one.
    pub(crate) fn insertion_index(&self, after_active: bool) -> usize {
        // The active document is always in the focused group.
        if after_active && self.active_document().is_some() {
            return self.state.active + 1;
        }
        match self.group_indexes(self.state.active_group).last() {
            Some(&last) => last + 1,
            None => self.state.documents.len(),
        }
    }
}
