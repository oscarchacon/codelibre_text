//! Use cases: clones, several views of one buffer (Sublime's File > New View into File,
//! `clone_file`). Each view has its own selections, regions, phantoms and scroll, and they share
//! the text: after a view is edited or saved, [`Editor::sync_clones`] gives the others its buffer,
//! their selections moved over the change. Closing a dirty view with a clone left asks nothing.

use entities::document::DocumentId;

use crate::editor::Editor;

/// What a view looked like the last time its clones followed it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BufferStamp {
    change_count: u64,
    modified_at: Option<std::time::SystemTime>,
    path: Option<std::path::PathBuf>,
    read_only: bool,
    syntax: String,
}

impl Editor {
    /// `clone_file`: a new view of the active document's buffer, after it in its group, active.
    pub fn clone_active_document(&mut self) -> Option<DocumentId> {
        self.active_document()?;
        let id = self.next_document_id();
        let view = self.active_document()?.clone_view(id);
        let at = self.insertion_index(true);
        self.state.documents.insert(at, view);
        self.activate(at);
        self.sync_clones();
        self.plugin_event("on_clone", id);
        Some(id)
    }

    /// The other views of `id`'s buffer, in window order.
    pub fn clones_of(&self, id: DocumentId) -> Vec<DocumentId> {
        let Some(buffer) = self.any_document(id).map(|d| d.buffer) else { return Vec::new() };
        self.state.documents.iter().filter(|d| d.buffer == buffer && d.id != id).map(|d| d.id).collect()
    }

    /// The view a buffer was opened in (the others are clones), or its first view when that one
    /// was closed (`view.is_primary()`, `buffer.primary_view()`).
    pub fn primary_view(&self, id: DocumentId) -> Option<DocumentId> {
        let buffer = self.any_document(id)?.buffer;
        let views: Vec<DocumentId> = self.state.documents.iter().filter(|d| d.buffer == buffer).map(|d| d.id).collect();
        views.iter().copied().find(|v| *v == buffer).or(views.first().copied())
    }

    /// The views of buffers with clones take the buffer of the one that changed (edited, saved,
    /// given another syntax…) since the last time. Cheap when nothing has clones.
    pub fn sync_clones(&mut self) {
        let documents = &self.state.documents;
        let shared: Vec<DocumentId> = documents
            .iter()
            .filter(|d| documents.iter().any(|o| o.buffer == d.buffer && o.id != d.id))
            .map(|d| d.buffer)
            .collect();
        if shared.is_empty() {
            self.state.buffer_stamps.clear();
            return;
        }
        let mut buffers = shared;
        buffers.sort_unstable();
        buffers.dedup();
        for buffer in buffers {
            let views: Vec<usize> = (0..self.state.documents.len()).filter(|&i| self.state.documents[i].buffer == buffer).collect();
            let stamp = |editor: &Editor, i: usize| {
                let d = &editor.state.documents[i];
                BufferStamp { change_count: d.change_count(), modified_at: d.modified_at, path: d.path.clone(), read_only: d.read_only, syntax: d.syntax.clone() }
            };
            let changed = views.iter().copied().find(|&i| self.state.buffer_stamps.get(&self.state.documents[i].id) != Some(&stamp(self, i)));
            if let Some(source) = changed {
                let source_doc = self.state.documents[source].clone();
                for &i in views.iter().filter(|&&i| i != source) {
                    self.state.documents[i].follow_buffer(&source_doc);
                }
            }
            for &i in &views {
                let id = self.state.documents[i].id;
                let now = stamp(self, i);
                self.state.buffer_stamps.insert(id, now);
            }
        }
    }
}
