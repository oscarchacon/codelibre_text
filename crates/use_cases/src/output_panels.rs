//! Use cases: output panels (`window.create_output_panel`), the views at the bottom of the window
//! where build systems and plugins write their output.
//!
//! A panel is a document that is not a tab: plugins see it as a view (its id is a document id),
//! and `show_panel {"panel": "output.<name>"}` shows it in place of the find panel.

use entities::document::{Document, DocumentId, LineEnding};

use crate::editor::Editor;

pub struct OutputPanel {
    pub name: String,
    pub document: Document,
}

/// The panels plugins created, and the one shown (by name).
#[derive(Default)]
pub struct OutputPanels {
    panels: Vec<OutputPanel>,
    shown: Option<String>,
}

impl Editor {
    /// Creates the panel `name`, or empties it if it exists (as Sublime does), and returns its view id.
    pub(crate) fn create_output_panel(&mut self, name: &str) -> DocumentId {
        if let Some(id) = self.find_output_panel(name) {
            let doc = self.any_document_mut(id).expect("found above");
            let read_only = std::mem::replace(&mut doc.read_only, false);
            doc.replace_range(0, doc.char_count(), "");
            doc.read_only = read_only;
            return id;
        }
        let id = self.next_document_id();
        let mut document = Document::new_untitled(id, LineEnding::Unix);
        document.scratch = true;
        self.state.output_panels.panels.push(OutputPanel { name: name.to_string(), document });
        id
    }

    pub(crate) fn find_output_panel(&self, name: &str) -> Option<DocumentId> {
        self.state.output_panels.panels.iter().find(|p| p.name == name).map(|p| p.document.id)
    }

    pub(crate) fn destroy_output_panel(&mut self, name: &str) {
        if let Some(id) = self.find_output_panel(name) {
            self.state.plugins.view_settings.remove(&id);
        }
        self.state.output_panels.panels.retain(|p| p.name != name);
        if self.state.output_panels.shown.as_deref() == Some(name) {
            self.state.output_panels.shown = None;
        }
    }

    /// Names of every panel, as `window.panels()` lists them (`output.<name>`).
    pub(crate) fn output_panel_names(&self) -> Vec<String> {
        self.state.output_panels.panels.iter().map(|p| format!("output.{}", p.name)).collect()
    }

    /// `show_panel {"panel": "output.<name>"}`. False when there is no such panel.
    pub fn show_output_panel(&mut self, name: &str) -> bool {
        let exists = self.find_output_panel(name).is_some();
        if exists {
            self.state.output_panels.shown = Some(name.to_string());
        }
        exists
    }

    pub fn hide_output_panel(&mut self) {
        self.state.output_panels.shown = None;
    }

    pub fn shown_output_panel(&self) -> Option<&OutputPanel> {
        let name = self.state.output_panels.shown.as_deref()?;
        self.state.output_panels.panels.iter().find(|p| p.name == name)
    }

    /// A tab's document or an output panel's.
    pub(crate) fn any_document(&self, id: DocumentId) -> Option<&Document> {
        let panels = self.state.output_panels.panels.iter().map(|p| &p.document);
        self.state.documents.iter().chain(panels).find(|d| d.id == id)
    }

    pub(crate) fn any_document_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        let panels = self.state.output_panels.panels.iter_mut().map(|p| &mut p.document);
        self.state.documents.iter_mut().chain(panels).find(|d| d.id == id)
    }

    pub(crate) fn is_output_panel(&self, id: DocumentId) -> bool {
        self.state.output_panels.panels.iter().any(|p| p.document.id == id)
    }

    /// Sublime's `append` command: adds text at the end of a view; `force` also writes to a
    /// read-only one (build output panels are read-only).
    pub(crate) fn append_text(&mut self, id: DocumentId, characters: &str, force: bool) -> Result<(), String> {
        let doc = self.any_document_mut(id).ok_or("the view is closed")?;
        let read_only = doc.read_only;
        doc.read_only = read_only && !force;
        let end = doc.char_count();
        doc.replace_range(end, end, characters);
        doc.read_only = read_only;
        Ok(())
    }
}
