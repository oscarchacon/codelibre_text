//! Use cases: multi-selected tabs (Sublime 4). Each group of the window may have several tabs
//! selected, shown side by side; its active document is the focused one. Selection > Tab
//! Selection's commands, Ctrl / Shift + click on a tab, `close_selected` / `close_unselected`,
//! `move_to_group` (the selected tabs go together) and the API's `select_sheets` work on them.
//!
//! A document's `tab_selected` says it is one of its group's selected tabs; that counts only
//! while the group's active document is selected too (else the active one alone is). Activating
//! a tab that is not selected leaves it alone again ([`Editor::activate`]); so do `next_view`,
//! `select_by_index`, opening a file and moving a tab. The rules are in
//! [`entities::tab_selection`].

use entities::document::DocumentId;
use entities::tab_selection::{TabSelection, TabSelectionCommand};

use crate::editor::Editor;
use crate::gateways::{MessageButtons, MessageLevel};

impl Editor {
    /// The selected tabs of `group` (positions among its tabs); `None` when it has none.
    pub fn tab_selection(&self, group: usize) -> Option<TabSelection> {
        let tabs = self.group_indexes(group);
        let shown = self.active_index_in_group(group)?;
        let focused = tabs.iter().position(|&i| i == shown)?;
        let documents = &self.state.documents;
        Some(if documents[shown].tab_selected {
            TabSelection::new(
                tabs.iter()
                    .enumerate()
                    .filter(|(_, &i)| documents[i].tab_selected)
                    .map(|(position, _)| position),
                focused,
            )
        } else {
            TabSelection::single(focused)
        })
    }

    /// Indexes (in the window) of the selected tabs of `group`, in tab order: the documents it
    /// shows side by side.
    pub fn selected_indexes_in_group(&self, group: usize) -> Vec<usize> {
        let tabs = self.group_indexes(group);
        self.tab_selection(group)
            .map(|selection| selection.selected().iter().map(|&p| tabs[p]).collect())
            .unwrap_or_default()
    }

    /// Every document the window shows: the selected tabs of each group.
    pub fn shown_indexes(&self) -> Vec<usize> {
        (0..self.group_count())
            .flat_map(|group| self.selected_indexes_in_group(group))
            .collect()
    }

    /// The focused group has several tabs selected (the `group_has_multiselect` context).
    pub fn group_has_multiselect(&self) -> bool {
        self.tab_selection(self.state.active_group)
            .is_some_and(|selection| selection.is_multiple())
    }

    /// Selection > Tab Selection's commands, on the focused group.
    pub fn run_tab_selection_command(&mut self, command: TabSelectionCommand) {
        let group = self.state.active_group;
        if let Some(selection) = self.tab_selection(group) {
            let count = self.group_indexes(group).len();
            self.set_tab_selection(group, &selection.apply(command, count));
        }
    }

    /// A click on the tab of the document at `index`: with `toggle` (Ctrl, Cmd on macOS) it joins
    /// or leaves its group's selection, with `range` (Shift) the tabs from the focused one to it
    /// are selected; a plain click focuses it (a selected tab keeps the others selected).
    pub fn click_tab(&mut self, index: usize, toggle: bool, range: bool) {
        let Some(group) = self.state.documents.get(index).map(|d| d.group) else {
            return;
        };
        let tabs = self.group_indexes(group);
        let (Some(position), Some(selection)) = (
            tabs.iter().position(|&i| i == index),
            self.tab_selection(group),
        ) else {
            return;
        };
        let selection = if range {
            selection.extend_to(position)
        } else if toggle {
            selection.toggle(position)
        } else {
            self.activate(index);
            return;
        };
        self.set_tab_selection(group, &selection);
    }

    /// `window.select_sheets`: each group the documents are in gets them as its selection (its
    /// focused one stays focused if it is among them, else the last of them takes the focus);
    /// the first one's group takes the window's focus.
    pub fn select_documents(&mut self, ids: &[DocumentId]) {
        let mut groups: Vec<usize> = Vec::new();
        for id in ids {
            if let Some(index) = self.index_of(*id) {
                let group = self.state.documents[index].group;
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
        }
        // The first group last: it keeps the focus.
        for &group in groups.iter().rev() {
            let tabs = self.group_indexes(group);
            let positions: Vec<usize> = ids
                .iter()
                .filter_map(|id| tabs.iter().position(|&i| self.state.documents[i].id == *id))
                .collect();
            let Some(&last) = positions.last() else {
                continue;
            };
            let focused = self
                .tab_selection(group)
                .map(|s| s.focused())
                .filter(|f| positions.contains(f))
                .unwrap_or(last);
            self.set_tab_selection(group, &TabSelection::new(positions, focused));
        }
    }

    /// `select_by_index` (Alt+1…): the tab at `index` of the focused group, alone.
    pub fn activate_in_group(&mut self, index: usize) {
        let group = self.state.active_group;
        if let Some(&at) = self.group_indexes(group).get(index) {
            self.clear_tab_selection(group);
            self.activate(at);
        }
    }

    /// `select_last_tab` (Alt+9): the last tab of the focused group, alone.
    pub fn activate_last_in_group(&mut self) {
        let count = self.group_indexes(self.state.active_group).len();
        if count > 0 {
            self.activate_in_group(count - 1);
        }
    }

    /// The document that takes the focus when the focused one of a selection of several tabs
    /// closes: the next selected tab to its right, else to its left.
    pub(crate) fn next_selected_after_closing(&self, id: DocumentId) -> Option<DocumentId> {
        let index = self.index_of(id)?;
        let group = self.state.documents[index].group;
        if self.active_index_in_group(group) != Some(index) {
            return None;
        }
        let next = self.tab_selection(group)?.without_focused()?.focused();
        let at = *self.group_indexes(group).get(next)?;
        Some(self.state.documents[at].id)
    }

    /// `move_to_group` with several tabs selected: they all go to the end of `group`'s tabs, still
    /// selected, the focused one with the focus.
    pub(crate) fn move_selected_to_group(&mut self, group: usize) {
        let from = self.state.active_group;
        let ids: Vec<DocumentId> = self
            .selected_indexes_in_group(from)
            .into_iter()
            .map(|i| self.state.documents[i].id)
            .collect();
        let Some(focused) = self.active_document().map(|d| d.id) else {
            return;
        };
        for &id in &ids {
            self.set_view_index(id, group, usize::MAX);
        }
        if ids.len() > 1 {
            for &id in &ids {
                if let Some(doc) = self.any_document_mut(id) {
                    doc.tab_selected = true;
                }
            }
        }
        self.activate_document(focused);
    }

    /// `clone_file {"add_to_selection": true}` (the tab menu's Split View): the clone joins the
    /// selection of the active tab, side by side with it, and takes the focus.
    pub fn clone_into_selection(&mut self) -> Option<DocumentId> {
        let group = self.state.active_group;
        let mut selected: Vec<DocumentId> = self
            .selected_indexes_in_group(group)
            .into_iter()
            .map(|i| self.state.documents[i].id)
            .collect();
        let id = self.clone_active_document()?;
        selected.push(id);
        for id in selected {
            if let Some(doc) = self.any_document_mut(id) {
                doc.tab_selected = true;
            }
        }
        self.activate_document(id);
        Some(id)
    }

    /// `primary_j_changed` (Ctrl+J Ctrl+J, Cmd+J Cmd+J on macOS): Join Lines moved to
    /// Ctrl+Shift+J in Sublime 4, so that Ctrl+J starts the Tab Selection chords; it says so, as
    /// Sublime's `Default/keymap.py` does.
    pub fn explain_primary_j_changed(&mut self) {
        let primary = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let text = self.tr_args(
            "Key Binding Changed\n\nThe key binding for Join Lines has changed to {primary}+Shift+J.\n\nThe binding was changed to open up {primary}+J to be used as the first key press for various new key bindings, such as Unselect to Left, Unselect to Right, Select to Left and Select to Right.",
            &[("primary", &primary)],
        );
        self.plugin_dialog(MessageLevel::Info, "", &text, MessageButtons::Ok);
    }

    /// No tab of `group` is selected besides its active one.
    pub(crate) fn clear_tab_selection(&mut self, group: usize) {
        for doc in &mut self.state.documents {
            if doc.group == group {
                doc.tab_selected = false;
            }
        }
    }

    /// Makes `selection` (positions among `group`'s tabs) the group's selected tabs and focuses
    /// the focused one.
    fn set_tab_selection(&mut self, group: usize, selection: &TabSelection) {
        let tabs = self.group_indexes(group);
        let multiple = selection.is_multiple();
        for (position, &index) in tabs.iter().enumerate() {
            self.state.documents[index].tab_selected = multiple && selection.contains(position);
        }
        if let Some(&focused) = tabs.get(selection.focused()) {
            self.activate(focused);
        }
    }
}
