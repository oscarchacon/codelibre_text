//! Use cases: the quick panel and the input panel that plugins open (`window.show_quick_panel`,
//! `window.show_input_panel`), and the user's answers, which go back to the plugin's callbacks.
//!
//! The panels are drawn by the outer circles: a plugin's request is kept here until the command
//! controller takes it ([`Editor::take_plugin_panel_request`]) and shows it.

use serde_json::{json, Value};

use crate::editor::Editor;
use crate::plugins::CommandRunner;

/// `sublime.MONOSPACE_FONT`.
const MONOSPACE_FONT: u64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct PluginPanelItem {
    pub caption: String,
    pub detail: String,
    pub hint: String,
}

/// A list a plugin asked the user to pick from.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginQuickPanel {
    pub(crate) callback: u64,
    pub items: Vec<PluginPanelItem>,
    pub selected: usize,
    pub placeholder: String,
    pub monospace: bool,
    wants_highlight: bool,
}

/// A one-line text field a plugin asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginInputPanel {
    pub(crate) callback: u64,
    pub caption: String,
    pub initial_text: String,
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
            .map(|row| PluginPanelItem { caption: text(row.get(0)), detail: text(row.get(1)), hint: text(row.get(2)) })
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
            wants_highlight: args.get(6).and_then(Value::as_bool).unwrap_or(false),
        });
        self.state.plugin_panels.requests.push(PluginPanelRequest::QuickPanel);
        Ok(Value::Null)
    }

    /// `window_show_input_panel(window, caption, initial_text, callback, wants_changes)`.
    pub(crate) fn plugin_show_input_panel(&mut self, args: &[Value], run_command: &mut CommandRunner) -> Result<Value, String> {
        let text = |i: usize| args.get(i).and_then(Value::as_str).unwrap_or_default().to_string();
        let callback = args.get(3).and_then(Value::as_u64).ok_or("argument 4 must be a callback id")?;
        self.finish_plugin_input_panel(None, run_command);
        self.state.plugin_panels.input_panel = Some(PluginInputPanel {
            callback,
            caption: text(1),
            initial_text: text(2),
            wants_changes: args.get(4).and_then(Value::as_bool).unwrap_or(false),
        });
        self.state.plugin_panels.requests.push(PluginPanelRequest::InputPanel);
        Ok(Value::Null)
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

    /// The user pressed Enter in the plugin's input panel (`Some(text)`), or closed it (`None`).
    pub fn finish_plugin_input_panel(&mut self, text: Option<&str>, run_command: &mut CommandRunner) {
        if let Some(panel) = self.state.plugin_panels.input_panel.take() {
            match text {
                Some(text) => self.plugin_callback(panel.callback, "done", json!([text]), run_command),
                None => self.plugin_callback(panel.callback, "cancel", json!([]), run_command),
            }
        }
    }

    /// The text of the plugin's input panel changed (the plugin hears it if it passed `on_change`).
    pub fn change_plugin_input_panel(&mut self, text: &str, run_command: &mut CommandRunner) {
        if let Some(panel) = &self.state.plugin_panels.input_panel {
            if panel.wants_changes {
                let callback = panel.callback;
                self.plugin_callback(callback, "change", json!([text]), run_command);
            }
        }
    }
}
