//! Use cases: what the menus and the command palette ask plugin commands (`is_visible`,
//! `is_enabled`, `is_checked`, `description`), and starting a command that asks for its arguments
//! (`input()`).
//!
//! Menus and the palette are drawn every frame, so the answers are kept here: they are asked once,
//! in one request, when a menu or the palette opens.

use std::collections::HashMap;

use entities::command::Command;
use serde_json::{json, Value};

use crate::editor::Editor;

/// A plugin command's answers for one set of arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginCommandState {
    pub visible: bool,
    pub enabled: bool,
    pub checked: bool,
    pub description: Option<String>,
    /// `want_event()`: from a context menu, the command gets the click as its `event` argument.
    pub want_event: bool,
}

impl PluginCommandState {
    fn from_answer(answer: &Value) -> Self {
        let flag = |key: &str, default: bool| answer.get(key).and_then(Value::as_bool).unwrap_or(default);
        Self {
            visible: flag("visible", true),
            enabled: flag("enabled", true),
            checked: flag("checked", false),
            description: answer.get("description").and_then(Value::as_str).map(String::from),
            want_event: flag("want_event", false),
        }
    }
}

/// Every command of a menu, its submenus' too.
pub(crate) fn menu_commands(items: &[entities::menu::MenuItem], commands: &mut Vec<Command>) {
    for item in items {
        commands.extend(item.command.clone());
        menu_commands(&item.children, commands);
    }
}

/// A command and its arguments, but `event` (a context menu adds it to the commands that want it).
fn state_key(command: &Command) -> String {
    match &command.args {
        Value::Object(args) if args.contains_key("event") => {
            let mut args = args.clone();
            args.remove("event");
            format!("{} {}", command.name, Value::Object(args))
        }
        args => format!("{} {args}", command.name),
    }
}

impl Editor {
    /// Asks the plugins about every command in the menus and the palette. A menu or the palette is
    /// opening; nothing is sent when no command defines these methods.
    pub fn refresh_plugin_command_states(&mut self) {
        if !self.wants_plugin_event("command_states") {
            return;
        }
        let mut commands = Vec::new();
        menu_commands(&self.state.main_menu, &mut commands);
        commands.extend(self.state.palette.iter().map(|entry| entry.command.clone()));
        if let Some(states) = self.ask_plugin_command_states(commands) {
            self.state.plugins.command_states = states;
        }
    }

    /// The plugins' answers about the plugin commands among `commands`, in one query; `None` when
    /// nothing was asked or the host did not answer.
    pub(crate) fn ask_plugin_command_states(&mut self, mut commands: Vec<Command>) -> Option<HashMap<String, PluginCommandState>> {
        if !self.wants_plugin_event("command_states") {
            return None;
        }
        commands.retain(|c| self.is_plugin_command(&c.name));
        let mut keys = std::collections::HashSet::new();
        commands.retain(|c| keys.insert(state_key(c)));
        if commands.is_empty() {
            return None;
        }
        let view = self.active_document().map(|d| d.id);
        let asked: Vec<Value> = commands.iter().map(|c| json!([c.name, c.args])).collect();
        let Some(Value::Array(answers)) = self.plugin_query("command_states", view, json!([asked])) else {
            return None;
        };
        Some(commands.iter().zip(&answers).map(|(c, answer)| (state_key(c), PluginCommandState::from_answer(answer))).collect())
    }

    /// The last answers about a command; `None` for commands the plugins were not asked about.
    pub fn plugin_command_state(&self, command: &Command) -> Option<&PluginCommandState> {
        self.state.plugins.command_states.get(&state_key(command))
    }

    /// A command chosen in the palette: when its `input()` returns a handler, the plugin asks the
    /// user for the arguments and runs the command itself. False when it should just run.
    pub fn start_plugin_command_input(&mut self, command: &Command) -> bool {
        if !self.is_plugin_command(&command.name) {
            return false;
        }
        let view = self.active_document().map(|d| d.id);
        self.plugin_query("command_input", view, json!([command.name, command.args])) == Some(Value::Bool(true))
    }
}
