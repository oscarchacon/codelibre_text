//! Use cases: the listener methods that answer the editor (`on_query_context`, `on_text_command`,
//! `on_window_command`) and the ones told after a command (`on_post_text_command`,
//! `on_post_window_command`).
//!
//! Each is a `query` to the plugin host, sent only when some listener defines that method.

use std::cell::RefCell;

use entities::command::Command;
use entities::document::DocumentId;
use entities::keymap::{KeyCombo, KeyContext, KeyResult};
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::gateways::PluginMessage;

/// Where a command runs, for the hooks: on the active view or on the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTarget {
    View,
    Window,
}

/// Key binding contexts the editor does not know go to the plugins' `on_query_context`.
struct WithPlugins<'a> {
    context: &'a dyn KeyContext,
    editor: RefCell<&'a mut Editor>,
}

impl KeyContext for WithPlugins<'_> {
    fn query(&self, key: &str, operator: &str, operand: &Value) -> Option<bool> {
        self.query_all(key, operator, operand, false)
    }

    fn query_all(&self, key: &str, operator: &str, operand: &Value, match_all: bool) -> Option<bool> {
        let known = self.context.query(key, operator, operand);
        known.or_else(|| self.editor.borrow_mut().plugin_query_context(key, operator, operand, match_all))
    }
}

impl Editor {
    /// Resolves a key press through the keymap; `context` answers the conditions it knows and the
    /// plugins the others.
    pub fn press_key(&mut self, alternatives: &[KeyCombo], context: &dyn KeyContext) -> KeyResult {
        if !self.wants_plugin_event("on_query_context") {
            return self.state.keymap.press(alternatives, context);
        }
        // The keymap is set aside while the plugins (which may call the editor) answer.
        let mut keymap = std::mem::take(&mut self.state.keymap);
        let result = keymap.press(alternatives, &WithPlugins { context, editor: RefCell::new(self) });
        self.state.keymap = keymap;
        result
    }

    /// `on_query_context(view, key, operator, operand, match_all)`: true or false from the first
    /// listener that knows the key, `None` when none does.
    fn plugin_query_context(&mut self, key: &str, operator: &str, operand: &Value, match_all: bool) -> Option<bool> {
        let view = self.active_document()?.id;
        let answer = self.plugin_query("on_query_context", Some(view), json!([key, operator, operand, match_all]))?;
        answer.as_bool()
    }

    /// `on_text_command` / `on_window_command`: a listener may replace a command (name and
    /// arguments) before it runs. Returns the command to run.
    pub fn plugin_rewrite_command(&mut self, command: &Command, target: CommandTarget) -> Command {
        let (event, view) = match target {
            CommandTarget::View => ("on_text_command", self.active_document().map(|d| d.id)),
            CommandTarget::Window => ("on_window_command", None),
        };
        if target == CommandTarget::View && view.is_none() {
            return command.clone();
        }
        let args = json!([command.name, command.args]);
        match self.plugin_query(event, view, args) {
            Some(Value::Array(answer)) => match answer.first().and_then(Value::as_str) {
                Some(name) => Command::with_args(name, answer.get(1).cloned().unwrap_or(Value::Null)),
                None => command.clone(),
            },
            _ => command.clone(),
        }
    }

    /// `on_post_text_command` / `on_post_window_command`, after a command ran.
    pub fn plugin_command_ran(&mut self, command: &Command, target: CommandTarget) {
        let (event, view) = match target {
            CommandTarget::View => ("on_post_text_command", self.active_document().map(|d| d.id)),
            CommandTarget::Window => ("on_post_window_command", None),
        };
        if target == CommandTarget::View && view.is_none() {
            return;
        }
        if self.wants_plugin_event(event) || self.wants_plugin_event(&format!("{event}_async")) {
            self.ask_plugins(event, view, json!([command.name, command.args]));
        }
    }

    /// Some listener wants to hear about text commands, also the ones the text widget does itself.
    pub fn plugins_hear_text_commands(&self) -> bool {
        ["on_text_command", "on_post_text_command", "on_post_text_command_async"].iter().any(|e| self.wants_plugin_event(e))
    }

    pub(crate) fn wants_plugin_event(&self, name: &str) -> bool {
        self.state.plugins.wants(name)
    }

    /// `on_init(views)`: the views that exist when the plugins are loaded.
    pub(crate) fn plugin_init(&mut self) {
        let views: Vec<DocumentId> = self.state.documents.iter().map(|d| d.id).collect();
        self.plugin_query("on_init", None, json!([views]));
    }

    /// `on_exit()`: the window is closing.
    pub fn plugin_exit(&mut self) {
        self.plugin_query("on_exit", None, json!([]));
    }

    /// Sends a query when some listener defines that method; the answer (`None` for `null`).
    /// Built-in commands the listeners run are handled as in events.
    pub(crate) fn plugin_query(&mut self, name: &str, view: Option<DocumentId>, args: Value) -> Option<Value> {
        if !self.wants_plugin_event(name) {
            return None;
        }
        self.ask_plugins(name, view, args)
    }

    /// A query the host always understands, whether or not a listener wants it.
    pub(crate) fn ask_plugins(&mut self, name: &str, view: Option<DocumentId>, args: Value) -> Option<Value> {
        if !self.enter_plugin(name) {
            return None;
        }
        let result = self.gateways.plugins.query(name, view, &args).and_then(|()| self.converse_during_event());
        self.leave_plugin();
        match result {
            Ok(PluginMessage::Answer(Value::Null)) => None,
            Ok(PluginMessage::Answer(value)) => Some(value),
            Ok(PluginMessage::Done { error: Some(traceback) }) => {
                self.log(&format!("{name}: {traceback}\n"));
                None
            }
            Ok(_) => None,
            Err(e) => {
                self.plugin_host_failed(&e);
                None
            }
        }
    }
}
