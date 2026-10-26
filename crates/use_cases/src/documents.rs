//! Use cases: create, open, switch between, close and reload documents.

use std::path::Path;
use std::time::Duration;

use entities::document::{Document, DocumentError, DocumentId, LineEnding};
use entities::selection::Selection;

use crate::editor::{CloseChoice, Editor, PendingClose};

impl Editor {
    pub fn new_file(&mut self) {
        let id = self.next_document_id();
        let doc = Document::new_untitled(id, self.default_line_ending());
        self.state.documents.push(doc);
        self.activate(self.state.documents.len() - 1);
    }

    /// Opens a file, or focuses it if it is already open. A path that does not exist becomes an
    /// empty buffer that will be saved there (like `subl new.txt`). Returns false on error.
    pub fn open_file(&mut self, path: &Path) -> bool {
        let path = self.gateways.file_system.canonicalize(path);
        if let Some(index) = self
            .state
            .documents
            .iter()
            .position(|d| d.path.as_deref() == Some(path.as_path()))
        {
            self.activate(index);
            return true;
        }

        let id = self.next_document_id();
        let mut doc = if self.gateways.file_system.exists(&path) {
            let bytes = match self.gateways.file_system.read(&path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.notify(format!("{}: {e}", path.display()));
                    return false;
                }
            };
            let modified = self.gateways.file_system.modified(&path);
            match Document::from_bytes(id, &path, bytes, modified) {
                Ok(doc) => doc,
                Err(DocumentError::Binary) => {
                    self.notify(self.tr_args("{path} looks like a binary file", &[("path", &path.display())]));
                    return false;
                }
            }
        } else {
            let mut doc = Document::new_untitled(id, self.default_line_ending());
            doc.path = Some(path.clone());
            doc
        };
        doc.syntax = self.gateways.syntaxes.detect(&path, doc.first_line());
        self.insert_document(doc);
        true
    }

    /// Shows the native "Open File" dialog, in `initial_directory` (Sublime's argument, which may
    /// use `${packages}`) or else the active file's folder.
    pub fn prompt_open_files(&mut self, initial_directory: Option<&str>) {
        let start = self.dialog_start(initial_directory);
        for path in self.gateways.dialogs.pick_files(start.as_deref()) {
            self.open_file(&path);
        }
    }

    /// Opens a buffer that is not a file (e.g. the default settings, read only).
    pub fn open_scratch(&mut self, name: &str, text: &str, syntax: &str, read_only: bool) {
        if let Some(index) = self.state.documents.iter().position(|d| d.scratch_name.as_deref() == Some(name)) {
            self.activate(index);
            return;
        }
        let id = self.next_document_id();
        let syntax = self.gateways.syntaxes.resolve(syntax);
        self.state
            .documents
            .push(Document::scratch(id, name, text, &syntax, read_only));
        self.activate(self.state.documents.len() - 1);
    }

    /// An empty, unmodified untitled tab is replaced by the opened file, as in Sublime.
    fn insert_document(&mut self, doc: Document) {
        let replaces_pristine = self.state.documents.len() == 1
            && self
                .active_document()
                .is_some_and(|d| d.path.is_none() && d.scratch_name.is_none() && d.text.is_empty());
        if replaces_pristine {
            let old = std::mem::replace(&mut self.state.documents[0], doc);
            self.gateways.syntaxes.forget(old.id);
            self.state.recently_used.retain(|id| *id != old.id);
            self.activate(0);
        } else {
            let index = (self.state.active + 1).min(self.state.documents.len());
            self.state.documents.insert(index, doc);
            self.activate(index);
        }
    }

    pub fn activate(&mut self, index: usize) {
        if let Some(doc) = self.state.documents.get(index) {
            let id = doc.id;
            self.state.active = index;
            self.state.recently_used.retain(|d| *d != id);
            self.state.recently_used.insert(0, id);
        }
    }

    pub fn activate_document(&mut self, id: DocumentId) {
        if let Some(index) = self.index_of(id) {
            self.activate(index);
        }
    }

    /// Next / previous tab in tab order.
    pub fn cycle_documents(&mut self, forward: bool) {
        let count = self.state.documents.len();
        if count > 0 {
            let active = self.state.active;
            self.activate(if forward { (active + 1) % count } else { (active + count - 1) % count });
        }
    }

    /// Ctrl+Tab: the previously used document (or the least recently used one going backwards).
    pub fn cycle_recent_documents(&mut self, forward: bool) {
        let target = if forward {
            self.state.recently_used.get(1)
        } else {
            self.state.recently_used.last()
        };
        if let Some(id) = target.copied() {
            self.activate_document(id);
        }
    }

    /// Closes documents. Dirty ones are not closed: the user is asked first (see `pending_close`).
    pub fn close_documents(&mut self, ids: Vec<DocumentId>) {
        let mut need_confirmation = Vec::new();
        for id in ids {
            match self.index_of(id) {
                Some(index) if self.state.documents[index].is_dirty() => need_confirmation.push(id),
                Some(index) => self.remove_document(index),
                None => {}
            }
        }
        if let Some(first) = need_confirmation.first() {
            self.activate_document(*first);
            self.state.pending_close = Some(PendingClose::Documents(need_confirmation));
        }
    }

    pub fn close_active_document(&mut self) {
        if let Some(id) = self.active_document().map(|d| d.id) {
            self.close_documents(vec![id]);
        }
    }

    pub fn close_all_documents(&mut self) {
        let ids = self.state.documents.iter().map(|d| d.id).collect();
        self.close_documents(ids);
    }

    /// The user's answer to "Save changes?". Returns true if the application may now exit.
    pub fn resolve_pending_close(&mut self, choice: CloseChoice) -> bool {
        let Some(pending) = self.state.pending_close.take() else {
            return false;
        };
        match (pending, choice) {
            (_, CloseChoice::Cancel) => false,
            (PendingClose::Exit, _) => true,
            (PendingClose::Documents(mut ids), choice) => {
                let id = ids.remove(0);
                if let Some(index) = self.index_of(id) {
                    self.activate(index);
                    let can_close = choice == CloseChoice::DontSave || self.save_active(false);
                    if can_close {
                        if let Some(index) = self.index_of(id) {
                            self.remove_document(index);
                        }
                    }
                }
                if !ids.is_empty() {
                    self.close_documents(ids);
                }
                false
            }
        }
    }

    fn remove_document(&mut self, index: usize) {
        let doc = self.state.documents.remove(index);
        self.gateways.syntaxes.forget(doc.id);
        if let Some(path) = doc.path {
            self.state.closed_files.push(path);
        }
        self.state.recently_used.retain(|d| *d != doc.id);
        if self.state.documents.is_empty() {
            self.state.active = 0;
        } else if let Some(recent) = self.state.recently_used.first().and_then(|id| self.index_of(*id)) {
            self.activate(recent);
        } else {
            self.activate(index.min(self.state.documents.len() - 1));
        }
    }

    pub fn reopen_closed_file(&mut self) {
        if let Some(path) = self.state.closed_files.pop() {
            self.open_file(&path);
        }
    }

    /// `set_file_type`: accepts a syntax name or a `Packages/…/X.sublime-syntax` path.
    pub fn set_syntax(&mut self, name_or_path: &str) {
        let syntax = self.gateways.syntaxes.resolve(name_or_path);
        if let Some(doc) = self.active_document_mut() {
            doc.syntax = syntax;
        }
    }

    pub fn set_line_ending(&mut self, line_ending: LineEnding) {
        if let Some(doc) = self.active_document_mut() {
            doc.line_ending = line_ending;
        }
    }

    /// Re-reads the active file from disk, discarding changes.
    pub fn revert_active_document(&mut self) {
        if let Some(index) = self.active_document().map(|_| self.state.active) {
            if let Err(e) = self.reload_document(index) {
                self.notify(e);
            }
        }
    }

    fn reload_document(&mut self, index: usize) -> Result<(), String> {
        let doc = &self.state.documents[index];
        let Some(path) = doc.path.clone() else {
            return Ok(());
        };
        let bytes = self
            .gateways
            .file_system
            .read(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let modified = self.gateways.file_system.modified(&path);
        let fresh = Document::from_bytes(doc.id, &path, bytes, modified)
            .map_err(|_| self.tr_args("{path} looks like a binary file", &[("path", &path.display())]))?;
        self.state.documents[index].reload_from(fresh);
        Ok(())
    }

    /// Reloads files changed by other programs (unless they have unsaved changes here).
    /// Cheap to call every frame: it only checks every 1.5 seconds.
    pub fn check_external_changes(&mut self) {
        if self.state.last_disk_check.elapsed() < Duration::from_millis(1500) {
            return;
        }
        self.state.last_disk_check = std::time::Instant::now();
        for index in 0..self.state.documents.len() {
            let doc = &self.state.documents[index];
            let Some(path) = doc.path.clone() else { continue };
            let on_disk = self.gateways.file_system.modified(&path);
            if doc.modified_at.is_none() || on_disk.is_none() || on_disk == doc.modified_at {
                continue;
            }
            if doc.is_dirty() {
                let name = self.document_name(doc);
                self.state.documents[index].modified_at = on_disk;
                self.notify(self.tr_args("{name} changed on disk; you have unsaved changes", &[("name", &name)]));
            } else if let Err(e) = self.reload_document(index) {
                self.notify(e);
            }
        }
    }

    /// The view reports where the user moved the caret.
    pub fn sync_selection_from_view(&mut self, id: DocumentId, selection: Selection) {
        if let Some(index) = self.index_of(id) {
            self.state.documents[index].sync_selection_from_view(selection);
        }
    }

    /// Where a file or folder dialog opens: the `initial_directory` argument if given.
    pub(crate) fn dialog_start(&self, initial_directory: Option<&str>) -> Option<std::path::PathBuf> {
        match initial_directory.filter(|d| !d.is_empty()) {
            Some(dir) => Some(self.gateways.resources.paths().expand(dir)),
            None => self.current_directory(),
        }
    }

    /// Folder of the active file, or the first open folder (start point for file dialogs).
    pub(crate) fn current_directory(&self) -> Option<std::path::PathBuf> {
        self.active_document()
            .and_then(|d| d.path.as_ref())
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| self.state.folders.first().cloned())
    }
}
