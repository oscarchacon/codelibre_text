//! Use cases: the quick panel and the input panel that plugins open (`window.show_quick_panel`,
//! `window.show_input_panel`), and the user's answers, which go back to the plugin's callbacks.
//!
//! The panels are drawn by the outer circles: a plugin's request is kept here until the command
//! controller takes it ([`Editor::take_plugin_panel_request`]) and shows it.

use entities::document::{Document, LineEnding};
use entities::selection::Selection;
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::plugins::CommandRunner;

/// `sublime.MONOSPACE_FONT`.
const MONOSPACE_FONT: u64 = 1;
/// `sublime.KEEP_OPEN_ON_FOCUS_LOST`.
const KEEP_OPEN_ON_FOCUS_LOST: u64 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct PluginPanelItem {
    pub caption: String,
    pub detail: String,
    pub hint: String,
    /// The letter of its `kind`, shown in a badge; empty for none.
    pub kind: String,
}

/// A list a plugin asked the user to pick from.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginQuickPanel {
    pub(crate) callback: u64,
    pub items: Vec<PluginPanelItem>,
    pub selected: usize,
    pub placeholder: String,
    pub monospace: bool,
    /// A click outside does not close it.
    pub keep_open: bool,
    wants_highlight: bool,
}

/// A one-line text field a plugin asked for. Its text is a document, so `show_input_panel`
/// returns a real view; the view pushes every edit and caret move here.
pub struct PluginInputPanel {
    pub(crate) callback: u64,
    pub caption: String,
    pub document: Document,
    wants_changes: bool,
}

/// Which panel a plugin has just asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginPanelRequest {
    QuickPanel,
    InputPanel,
}

#[derive(Default)]
pub(crate) struct PluginPanels {
    quick_panel: Option<PluginQuickPanel>,
    input_panel: Option<PluginInputPanel>,
    requests: Vec<PluginPanelRequest>,
}

impl PluginPanels {
    pub(crate) fn input_document(&self) -> Option<&Document> {
        self.input_panel.as_ref().map(|p| &p.document)
    }

    pub(crate) fn input_document_mut(&mut self) -> Option<&mut Document> {
        self.input_panel.as_mut().map(|p| &mut p.document)
    }
}

impl Editor {
    /// `window_show_quick_panel(window, rows, flags, selected_index, placeholder, callback, wants_highlight)`.
    /// A plugin quick panel that is still open is cancelled first, as in Sublime.
    pub(crate) fn plugin_show_quick_panel(&mut self, args: &[Value], run_command: &mut CommandRunner) -> Result<Value, String> {
        let text = |value: Option<&Value>| value.and_then(Value::as_str).unwrap_or_default().to_string();
        let items: Vec<PluginPanelItem> = args
            .get(1)
            .and_then(Value::as_array)
            .ok_or("argument 2 must be a list of items")?
            .iter()
            .map(|row| PluginPanelItem {
                caption: text(row.get(0)),
                detail: text(row.get(1)),
                hint: text(row.get(2)),
                kind: text(row.get(3)),
            })
            .collect();
        let flags = args.get(2).and_then(Value::as_u64).unwrap_or(0);
        let selected = args.get(3).and_then(Value::as_i64).unwrap_or(-1);
        let callback = args.get(5).and_then(Value::as_u64).ok_or("argument 6 must be a callback id")?;
        self.answer_plugin_quick_panel(None, run_command);
        self.state.plugin_panels.quick_panel = Some(PluginQuickPanel {
            callback,
            selected: usize::try_from(selected).unwrap_or(0).min(items.len().saturating_sub(1)),
            items,
            placeholder: text(args.get(4)),
            monospace: flags & MONOSPACE_FONT != 0,
            keep_open: flags & KEEP_OPEN_ON_FOCUS_LOST != 0,
            wants_highlight: args.get(6).and_then(Value::as_bool).unwrap_or(false),
        });
        self.state.plugin_panels.requests.push(PluginPanelRequest::QuickPanel);
        Ok(Value::Null)
    }

    /// `window_show_input_panel(window, caption, initial_text, callback, wants_changes)`. Answers
    /// the panel's view id.
    pub(crate) fn plugin_show_input_panel(&mut self, args: &[Value], run_command: &mut CommandRunner) -> Result<Value, String> {
        let text = |i: usize| args.get(i).and_then(Value::as_str).unwrap_or_default().to_string();
        let callback = args.get(3).and_then(Value::as_u64).ok_or("argument 4 must be a callback id")?;
        self.finish_plugin_input_panel(false, run_command);
        let id = self.next_document_id();
        let mut document = Document::new_untitled(id, LineEnding::Unix);
        document.scratch = true;
        document.replace_range(0, 0, &text(2));
        self.state.plugin_panels.input_panel = Some(PluginInputPanel {
            callback,
            caption: text(1),
            document,
            wants_changes: args.get(4).and_then(Value::as_bool).unwrap_or(false),
        });
        self.state.plugin_panels.requests.push(PluginPanelRequest::InputPanel);
        Ok(json!(id))
    }

    /// The next panel a plugin asked for, for the controller to show.
    pub fn take_plugin_panel_request(&mut self) -> Option<PluginPanelRequest> {
        let requests = &mut self.state.plugin_panels.requests;
        (!requests.is_empty()).then(|| requests.remove(0))
    }

    pub fn plugin_quick_panel(&self) -> Option<&PluginQuickPanel> {
        self.state.plugin_panels.quick_panel.as_ref()
    }

    pub fn plugin_input_panel(&self) -> Option<&PluginInputPanel> {
        self.state.plugin_panels.input_panel.as_ref()
    }

    /// The user picked item `index` of the plugin's quick panel, or cancelled it (`None`: -1).
    pub fn answer_plugin_quick_panel(&mut self, index: Option<usize>, run_command: &mut CommandRunner) {
        if let Some(panel) = self.state.plugin_panels.quick_panel.take() {
            let index = index.map_or(-1, |i| i as i64);
            self.plugin_callback(panel.callback, "select", json!([index]), run_command);
        }
    }

    /// The user moved to item `index` (the plugin hears it if it passed `on_highlight`).
    pub fn highlight_plugin_quick_panel_item(&mut self, index: usize, run_command: &mut CommandRunner) {
        if let Some(panel) = self.state.plugin_panels.quick_panel.as_mut() {
            panel.selected = index;
            if panel.wants_highlight {
                let callback = panel.callback;
                self.plugin_callback(callback, "highlight", json!([index]), run_command);
            }
        }
    }

    /// The user pressed Enter in the plugin's input panel (`done`), or closed it.
    pub fn finish_plugin_input_panel(&mut self, done: bool, run_command: &mut CommandRunner) {
        if let Some(panel) = self.state.plugin_panels.input_panel.take() {
            self.state.plugins.view_settings.remove(&panel.document.id);
            if done {
                self.plugin_callback(panel.callback, "done", json!([panel.document.text]), run_command);
            } else {
                self.plugin_callback(panel.callback, "cancel", json!([]), run_command);
            }
        }
    }

    /// The user edited the plugin's input panel (the plugin hears it if it passed `on_change`).
    pub fn change_plugin_input_panel(&mut self, text: &str, run_command: &mut CommandRunner) {
        if let Some(panel) = &mut self.state.plugin_panels.input_panel {
            panel.document.text = text.to_string();
            panel.document.note_text_changed();
            if panel.wants_changes {
                let callback = panel.callback;
                self.plugin_callback(callback, "change", json!([text]), run_command);
            }
        }
    }

    /// The user moved the caret in the plugin's input panel.
    pub fn select_in_plugin_input_panel(&mut self, selection: Selection) {
        if let Some(panel) = &mut self.state.plugin_panels.input_panel {
            panel.document.sync_selection_from_view(selection);
        }
    }
}
