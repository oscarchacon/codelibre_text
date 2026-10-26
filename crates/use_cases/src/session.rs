//! Use cases: hot exit (restore folders and unsaved buffers on the next start) and quitting.

use std::path::PathBuf;

use entities::document::Document;
use entities::selection::Selection;

use crate::editor::{Editor, PendingClose};

/// What is remembered between runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Session {
    pub folders: Vec<PathBuf>,
    pub documents: Vec<SessionDocument>,
    pub active: usize,
    /// UI layout flags owned by the outer circles (e.g. `side_bar`), stored as-is.
    pub layout: Vec<(String, bool)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDocument {
    pub path: Option<PathBuf>,
    /// Unsaved contents; only stored for documents with changes.
    pub unsaved_text: Option<String>,
    pub syntax: String,
    pub selection: Selection,
}

impl Editor {
    /// The user asked to quit. Returns true if the window may close now; otherwise the user is
    /// asked about unsaved changes (`pending_close`).
    /// With `hot_exit`, unsaved changes are kept in the session instead of asking.
    pub fn request_exit(&mut self, layout: Vec<(String, bool)>) -> bool {
        let hot_exit = self.global_settings().bool("hot_exit", true);
        let has_unsaved = self.state.documents.iter().any(Document::is_dirty);
        if hot_exit || !has_unsaved {
            self.save_session(layout);
            true
        } else {
            self.state.pending_close = Some(PendingClose::Exit);
            false
        }
    }

    pub fn save_session(&mut self, layout: Vec<(String, bool)>) {
        let session = Session {
            folders: self.state.folders.clone(),
            documents: self
                .state
                .documents
                .iter()
                .filter(|d| d.scratch_name.is_none() && (d.path.is_some() || !d.text.is_empty()))
                .map(|d| SessionDocument {
                    path: d.path.clone(),
                    unsaved_text: d.is_dirty().then(|| d.text.clone()),
                    syntax: d.syntax.clone(),
                    selection: d.selection(),
                })
                .collect(),
            active: self.state.active,
            layout,
        };
        if let Err(e) = self.gateways.sessions.save(&session) {
            self.notify(self.tr_args("Could not save the session: {error}", &[("error", &e)]));
        }
    }

    /// Layout flags saved with the session, e.g. whether the side bar was visible.
    pub fn session_layout(&self) -> Vec<(String, bool)> {
        self.gateways.sessions.load().map(|s| s.layout).unwrap_or_default()
    }

    pub(crate) fn restore_session(&mut self) {
        let Some(session) = self.gateways.sessions.load() else {
            return;
        };
        for folder in session.folders {
            if self.gateways.file_system.is_dir(&folder) {
                self.add_folder(folder);
            }
        }
        for saved in session.documents {
            let id = self.next_document_id();
            let mut doc = match &saved.path {
                Some(path) if self.gateways.file_system.exists(path) => {
                    let Ok(bytes) = self.gateways.file_system.read(path) else { continue };
                    let modified = self.gateways.file_system.modified(path);
                    let Ok(doc) = Document::from_bytes(id, path, bytes, modified) else { continue };
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
            self.state.documents.push(doc);
        }
        if !self.state.documents.is_empty() {
            self.activate(session.active.min(self.state.documents.len() - 1));
        }
    }
}
