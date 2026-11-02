//! Use cases: output panels (`window.create_output_panel`), the views at the bottom of the window
//! where build systems and plugins write their output.
//!
//! A panel is a document that is not a tab: plugins see it as a view (its id is a document id),
//! and `show_panel {"panel": "output.<name>"}` shows it in place of the find panel.
//!
//! Lines matching the view settings `result_file_regex` / `result_line_regex` are results: places
//! in files that `next_result` and `prev_result` go through (relative paths are under `result_base_dir`).

use std::path::Path;

use entities::build::{find_results, ResultLocation};
use entities::document::{Document, DocumentId, LineEnding};

use crate::editor::Editor;

pub struct OutputPanel {
    pub name: String,
    pub document: Document,
    /// The line of the result `next_result` went to last.
    pub current_result: Option<usize>,
    /// Changes every time a result is visited, for the view to scroll to it.
    pub result_revision: u64,
}

/// The panels plugins created, and the one shown (by name).
#[derive(Default)]
pub struct OutputPanels {
    panels: Vec<OutputPanel>,
    shown: Option<String>,
    /// The panel `next_result` goes through: the last one shown that has results.
    results: Option<String>,
}

impl Editor {
    /// Creates the panel `name`, or empties it if it exists (as Sublime does), and returns its view id.
    pub(crate) fn create_output_panel(&mut self, name: &str) -> DocumentId {
        if let Some(id) = self.find_output_panel(name) {
            let doc = self.any_document_mut(id).expect("found above");
            let read_only = std::mem::replace(&mut doc.read_only, false);
            doc.replace_range(0, doc.char_count(), "");
            doc.read_only = read_only;
            self.panel_mut(name).expect("found above").current_result = None;
            return id;
        }
        let id = self.next_document_id();
        let mut document = Document::new_untitled(id, LineEnding::Unix);
        document.scratch = true;
        self.state.output_panels.panels.push(OutputPanel { name: name.to_string(), document, current_result: None, result_revision: 0 });
        id
    }

    fn panel_mut(&mut self, name: &str) -> Option<&mut OutputPanel> {
        self.state.output_panels.panels.iter_mut().find(|p| p.name == name)
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
        if self.state.output_panels.results.as_deref() == Some(name) {
            self.state.output_panels.results = None;
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
            self.remember_results_panel(name);
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

    /// `next_result` / `prev_result` go through this panel from now on, if it has results.
    pub(crate) fn remember_results_panel(&mut self, name: &str) {
        let has_results = self.find_output_panel(name).is_some_and(|id| {
            let settings = self.settings_of(id);
            ["result_file_regex", "result_line_regex"].iter().any(|key| settings.str(key).is_some_and(|r| !r.is_empty()))
        });
        if has_results {
            self.state.output_panels.results = Some(name.to_string());
        }
    }

    /// The results in a view, by its `result_*` settings.
    pub(crate) fn output_results(&self, id: DocumentId) -> Vec<ResultLocation> {
        let Some(doc) = self.any_document(id) else {
            return Vec::new();
        };
        let settings = self.settings_of(id);
        let regex = |key: &str| settings.str(key).unwrap_or_default().to_string();
        find_results(&doc.text, &regex("result_file_regex"), &regex("result_line_regex")).unwrap_or_default()
    }

    /// `next_result` / `prev_result`: goes to the next result of the last panel that had them,
    /// and shows that panel. False when there are none.
    pub fn next_result(&mut self, forward: bool) -> bool {
        let Some(panel) = self.state.output_panels.results.clone() else {
            return false;
        };
        let Some(id) = self.find_output_panel(&panel) else {
            return false;
        };
        let results = self.output_results(id);
        let current = self.panel_mut(&panel).and_then(|p| p.current_result);
        // Past the last one it starts again.
        let result = if forward {
            results.iter().find(|r| current.is_none_or(|line| r.line > line)).or(results.first())
        } else {
            results.iter().rev().find(|r| current.is_none_or(|line| r.line < line)).or(results.last())
        };
        let Some(result) = result.cloned() else {
            return false;
        };
        self.state.output_panels.shown = Some(panel.clone());
        self.visit_result(&panel, &result);
        true
    }

    /// The user double-clicked a line of the panel shown. False when it is not a result.
    pub fn open_result_at_line(&mut self, line: usize) -> bool {
        let Some((name, id)) = self.shown_output_panel().map(|p| (p.name.clone(), p.document.id)) else {
            return false;
        };
        let Some(result) = self.output_results(id).into_iter().find(|r| r.line == line) else {
            return false;
        };
        self.remember_results_panel(&name);
        self.visit_result(&name, &result);
        true
    }

    fn visit_result(&mut self, panel: &str, result: &ResultLocation) {
        let base = self.find_output_panel(panel).and_then(|id| self.settings_of(id).str("result_base_dir").map(String::from));
        if let Some(panel) = self.panel_mut(panel) {
            panel.current_result = Some(result.line);
            panel.result_revision += 1;
        }
        let path = Path::new(base.as_deref().unwrap_or_default()).join(&result.file);
        // `open_file` would make an empty buffer for a file that is not there.
        if !self.gateways.file_system.exists(&path) {
            self.notify(self.tr_args("File not found: {path}", &[("path", &path.display())]));
            return;
        }
        if self.open_file(&path) {
            self.go_to_position(result.row - 1, result.column - 1);
        }
        if !result.message.is_empty() {
            self.notify(result.message.clone());
        }
    }

    /// A tab's document, an output panel's or the input panel's.
    pub(crate) fn any_document(&self, id: DocumentId) -> Option<&Document> {
        let panels = self.state.output_panels.panels.iter().map(|p| &p.document);
        let input = self.state.plugin_panels.input_document();
        self.state.documents.iter().chain(panels).chain(input).find(|d| d.id == id)
    }

    pub(crate) fn any_document_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        let panels = self.state.output_panels.panels.iter_mut().map(|p| &mut p.document);
        let input = self.state.plugin_panels.input_document_mut();
        self.state.documents.iter_mut().chain(panels).chain(input).find(|d| d.id == id)
    }

    /// A view that is not a tab.
    pub(crate) fn is_panel_view(&self, id: DocumentId) -> bool {
        self.state.output_panels.panels.iter().any(|p| p.document.id == id)
            || self.state.plugin_panels.input_document().is_some_and(|d| d.id == id)
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
