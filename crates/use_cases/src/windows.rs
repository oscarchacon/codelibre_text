//! Use cases: several windows (Sublime's `new_window`, File > New Window). Each window has its
//! own views, layout and groups, folders, output panels and settings (`window.settings()`); the
//! use cases work on the current window, and [`Editor::switch_to_window`] swaps another one in
//! (the window being drawn, or the one a plugin's call is about). Buffers, settings files,
//! packages and plugins are the application's.

use std::path::{Path, PathBuf};

use entities::document::{Document, DocumentId};
use entities::layout::Layout;
use entities::project::Project;
use serde_json::{json, Map, Value};

use crate::editor::Editor;
use crate::output_panels::OutputPanels;

/// A window's id, as plugins see it (`sublime.Window.id()`). The first window is 1.
pub type WindowId = u64;

/// What a window that is not the current one keeps until it is swapped in again.
#[derive(Default)]
pub(crate) struct WindowSlot {
    id: WindowId,
    documents: Vec<Document>,
    active: usize,
    layout: Layout,
    active_group: usize,
    folders: Vec<PathBuf>,
    project: Option<Project>,
    recently_used: Vec<DocumentId>,
    closed_files: Vec<PathBuf>,
    output_panels: OutputPanels,
    settings: Map<String, Value>,
    /// The user asked to close it (its unsaved files may still be asked about).
    closing: bool,
}

impl Editor {
    /// The window the use cases work on.
    pub fn current_window(&self) -> WindowId {
        self.state.window_id
    }

    /// Every window, oldest first.
    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids: Vec<WindowId> = std::iter::once(self.state.window_id).chain(self.state.windows.iter().map(|w| w.id)).collect();
        ids.sort_unstable();
        ids
    }

    /// Makes `id` the current window (the views, layout… of the others are put away). False when
    /// there is no such window.
    pub fn switch_to_window(&mut self, id: WindowId) -> bool {
        if id == self.state.window_id {
            return true;
        }
        let Some(at) = self.state.windows.iter().position(|w| w.id == id) else { return false };
        let mut slot = self.state.windows.remove(at);
        self.swap_window(&mut slot);
        // The slot now holds the window that was current.
        slot.id = std::mem::replace(&mut self.state.window_id, id);
        self.state.windows.push(slot);
        true
    }

    fn swap_window(&mut self, slot: &mut WindowSlot) {
        let state = &mut self.state;
        std::mem::swap(&mut state.documents, &mut slot.documents);
        std::mem::swap(&mut state.active, &mut slot.active);
        std::mem::swap(&mut state.layout, &mut slot.layout);
        std::mem::swap(&mut state.active_group, &mut slot.active_group);
        std::mem::swap(&mut state.folders, &mut slot.folders);
        std::mem::swap(&mut state.project, &mut slot.project);
        std::mem::swap(&mut state.recently_used, &mut slot.recently_used);
        std::mem::swap(&mut state.closed_files, &mut slot.closed_files);
        std::mem::swap(&mut state.output_panels, &mut slot.output_panels);
        std::mem::swap(&mut state.plugins.window_settings, &mut slot.settings);
        std::mem::swap(&mut state.window_closing, &mut slot.closing);
        // What belongs to a view of the window that was current goes with it.
        state.auto_complete = None;
        state.auto_complete_due = None;
        state.find_cache = None;
    }

    /// `new_window`: an empty window, which becomes the current one (plugins hear `on_new_window`).
    pub fn new_window(&mut self) -> WindowId {
        self.state.next_window_id += 1;
        let id = self.state.next_window_id;
        self.state.windows.push(WindowSlot { id, ..WindowSlot::default() });
        self.switch_to_window(id);
        self.plugin_window_event("on_new_window", id);
        id
    }

    /// The window a view (or output panel) is in.
    pub fn window_of_view(&self, view: DocumentId) -> Option<WindowId> {
        if self.any_document(view).is_some() {
            return Some(self.state.window_id);
        }
        self.state
            .windows
            .iter()
            .find(|w| w.documents.iter().any(|d| d.id == view) || w.output_panels.has_view(view))
            .map(|w| w.id)
    }

    /// Closing a window (its close button, `close_window`): its files close, asking about the
    /// unsaved ones. True when it is gone; otherwise it goes once they are answered (see
    /// [`Self::finish_closing_windows`]). The last window is not closed: the application is.
    pub fn close_window(&mut self, id: WindowId) -> bool {
        if self.window_ids().len() < 2 || !self.switch_to_window(id) {
            return false;
        }
        self.plugin_window_event("on_pre_close_window", id);
        self.state.window_closing = true;
        self.close_all_documents();
        self.finish_closing_windows();
        self.window_of_view_or_none(id).is_none()
    }

    /// Windows being closed whose files are all closed now go; the current window becomes another
    /// one if it went.
    pub fn finish_closing_windows(&mut self) {
        if self.state.window_closing && self.state.documents.is_empty() && self.state.pending_close.is_none() && !self.state.windows.is_empty() {
            let gone = self.state.window_id;
            let next = self.state.windows.iter().map(|w| w.id).min().expect("checked above");
            self.switch_to_window(next);
            self.state.windows.retain(|w| w.id != gone);
        }
    }

    /// Whether a window is still there.
    fn window_of_view_or_none(&self, id: WindowId) -> Option<WindowId> {
        self.window_ids().into_iter().find(|w| *w == id)
    }

    /// The title a window shows: the name of its active file (and its project's folder).
    pub fn window_document_name(&self, id: WindowId) -> Option<String> {
        if id == self.state.window_id {
            return self.active_document().map(|d| self.document_name(d));
        }
        let slot = self.state.windows.iter().find(|w| w.id == id)?;
        slot.documents.get(slot.active).map(|d| self.document_name(d))
    }

    /// The views of every window but the current one, by window (for the session).
    pub(crate) fn other_windows(&self) -> impl Iterator<Item = (WindowId, &[Document], usize, &Layout, usize, &[PathBuf], Option<&Path>)> {
        self.state
            .windows
            .iter()
            .map(|w| (w.id, w.documents.as_slice(), w.active, &w.layout, w.active_group, w.folders.as_slice(), w.project.as_ref().and_then(|p| p.file.as_deref())))
    }

    /// An `EventListener` method that takes the window (`on_new_window`, `on_pre_close_window`).
    pub(crate) fn plugin_window_event(&mut self, name: &str, window: WindowId) {
        if self.wants_plugin_event(name) || self.wants_plugin_event(&format!("{name}_async")) {
            self.ask_plugins(name, None, json!([window]));
        }
    }
}
