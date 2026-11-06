//! Use cases: hot exit (restore folders and unsaved buffers on the next start) and quitting.

use std::path::{Path, PathBuf};

use entities::document::{Document, RegionSet};
use entities::layout::Layout;
use entities::selection::Selection;

use crate::editor::{Editor, PendingClose};

/// What is remembered between runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Session {
    pub folders: Vec<PathBuf>,
    /// The window's `.sublime-project` file, if it has one.
    pub project: Option<PathBuf>,
    pub documents: Vec<SessionDocument>,
    pub active: usize,
    /// UI layout flags owned by the outer circles (e.g. `side_bar`), stored as-is.
    pub layout: Vec<(String, bool)>,
    /// The groups of the window (`None`: one group).
    pub groups: Option<Layout>,
    pub active_group: usize,
    /// The other windows, each as a session of its own (their `layout` flags and `windows`
    /// unused).
    pub windows: Vec<Session>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDocument {
    pub path: Option<PathBuf>,
    /// Unsaved contents; only stored for documents with changes.
    pub unsaved_text: Option<String>,
    pub syntax: String,
    pub selection: Selection,
    pub group: usize,
    /// The `PERSISTENT` regions of the view.
    pub regions: Vec<RegionSet>,
}

impl Editor {
    /// The user asked to quit. Returns true if the window may close now; otherwise the user is
    /// asked about unsaved changes (`pending_close`).
    /// With `hot_exit`, unsaved changes are kept in the session instead of asking.
    pub fn request_exit(&mut self, layout: Vec<(String, bool)>) -> bool {
        let hot_exit = self.global_settings().bool("hot_exit", true);
        let has_unsaved = self.state.documents.iter().any(Document::is_dirty) || self.other_windows().any(|(_, docs, ..)| docs.iter().any(Document::is_dirty));
        if hot_exit || !has_unsaved {
            self.save_session(layout);
            true
        } else {
            self.state.pending_close = Some(PendingClose::Exit);
            false
        }
    }

    pub fn save_session(&mut self, layout: Vec<(String, bool)>) {
        // The first window is the session's; the others go with it.
        let current = self.current_window();
        let first = self.window_ids()[0];
        self.switch_to_window(first);
        let mut session = window_session(&self.state.documents, self.state.active, &self.state.layout, self.state.active_group, &self.state.folders, self.state.project.as_ref().and_then(|p| p.file.as_deref()));
        session.layout = layout;
        session.windows = self.other_windows().map(|(_, docs, active, layout, group, folders, project)| window_session(docs, active, layout, group, folders, project)).collect();
        self.switch_to_window(current);
        if let Err(e) = self.gateways.sessions.save(&session) {
            self.notify(self.tr_args("Could not save the session: {error}", &[("error", &e)]));
        }
    }

    /// Layout flags saved with the session, e.g. whether the side bar was visible.
    pub fn session_layout(&self) -> Vec<(String, bool)> {
        self.gateways.sessions.load().map(|s| s.layout).unwrap_or_default()
    }

    pub(crate) fn restore_session(&mut self) {
        let Some(mut session) = self.gateways.sessions.load() else {
            return;
        };
        let others = std::mem::take(&mut session.windows);
        self.restore_window(session);
        // Each other window opens as it was; the first stays the current one.
        let first = self.current_window();
        for window in others {
            self.new_window();
            self.restore_window(window);
        }
        self.switch_to_window(first);
    }

    fn restore_window(&mut self, session: Session) {
        for folder in session.folders {
            if self.gateways.file_system.is_dir(&folder) {
                self.add_folder(folder);
            }
        }
        if let Some(project) = session.project.filter(|path| self.gateways.file_system.exists(path)) {
            self.open_project(&project);
        }
        if let Some(layout) = session.groups {
            self.set_layout(layout);
        }
        let mut loaded = Vec::new();
        for saved in session.documents {
            let id = self.next_document_id();
            let mut doc = match &saved.path {
                Some(path) if self.gateways.file_system.exists(path) => {
                    let Ok(bytes) = self.gateways.file_system.read(path) else { continue };
                    let modified = self.gateways.file_system.modified(path);
                    let Ok(doc) = Document::from_bytes(id, path, bytes, modified) else { continue };
                    loaded.push(id);
                    doc
                }
                path => {
                    let mut doc = Document::new_untitled(id, self.default_line_ending());
                    doc.path = path.clone();
                    doc
                }
            };
            if let Some(text) = saved.unsaved_text {
                doc.restore_unsaved_text(text);
            }
            doc.syntax = self.gateways.syntaxes.resolve(&saved.syntax);
            doc.set_selection(saved.selection);
            doc.restore_regions(saved.regions);
            doc.group = saved.group.min(self.group_count() - 1);
            self.state.documents.push(doc);
        }
        if session.active < self.state.documents.len() {
            self.activate(session.active);
        } else {
            self.focus_group(session.active_group);
        }
        loaded.into_iter().for_each(|id| self.plugin_event("on_load", id));
    }
}

/// One window as the session keeps it: its files (not the scratch views), folders and groups.
fn window_session(documents: &[Document], active: usize, layout: &Layout, active_group: usize, folders: &[PathBuf], project: Option<&Path>) -> Session {
    Session {
        folders: folders.to_vec(),
        project: project.map(Path::to_path_buf),
        documents: documents
            .iter()
            .filter(|d| d.scratch_name.is_none() && (d.path.is_some() || !d.text.is_empty()))
            .map(|d| SessionDocument {
                path: d.path.clone(),
                unsaved_text: d.is_dirty().then(|| d.text.clone()),
                syntax: d.syntax.clone(),
                selection: d.selection(),
                group: d.group,
                regions: d.persistent_regions(),
            })
            .collect(),
        active,
        layout: Vec::new(),
        groups: (layout.group_count() > 1).then(|| layout.clone()),
        active_group,
        windows: Vec::new(),
    }
}
