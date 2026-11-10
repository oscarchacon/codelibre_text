//! Use cases: the right-click menus (`Context`, `Side Bar`, `Side Bar Mount Point` and `Tab
//! Context.sublime-menu`) and the commands only they run (`close_by_index`, `remove_folder`…).
//!
//! When one opens, the plugins are asked about its commands with the arguments it fills in
//! (`files`, `dirs`, `paths`; `group`, `index`), so `is_visible(paths)` can hide an entry. A text
//! command whose `want_event()` is true gets the click as `event` (`{"x", "y", "modifier_keys"}`,
//! where `view.window_to_text` finds the char again).

use entities::command::Command;
use entities::menu::{fill_menu, ContextMenuTarget, MenuItem};
use entities::text::line_col;
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::plugin_commands::menu_commands;

/// The layout size of a char in the plugin API (`view.em_width()`, `view.line_height()`).
const EM_WIDTH: f64 = 8.0;
const LINE_HEIGHT: f64 = 16.0;

impl Editor {
    /// A context menu opened on `target`: asks the plugins about its commands.
    pub fn open_context_menu(&mut self, target: &ContextMenuTarget) {
        let mut commands = Vec::new();
        menu_commands(&fill_menu(&target.items(&self.state.context_menus), target), &mut commands);
        if let Some(states) = self.ask_plugin_command_states(commands) {
            self.state.plugins.command_states.extend(states);
        }
    }

    /// The menu `target` opens, its commands' arguments filled in (`event` for those that want it).
    pub fn context_menu_items(&self, target: &ContextMenuTarget) -> Vec<MenuItem> {
        let mut items = fill_menu(&target.items(&self.state.context_menus), target);
        if let Some(event) = self.context_menu_event(target) {
            self.add_event(&mut items, &event);
        }
        items
    }

    fn add_event(&self, items: &mut [MenuItem], event: &Value) {
        for item in items {
            if let Some(command) = &mut item.command {
                let wants = self.plugin_command_state(command).is_some_and(|s| s.want_event);
                if let (true, Value::Object(args)) = (wants, &mut command.args) {
                    args.insert("event".into(), event.clone());
                } else if wants {
                    command.args = json!({ "event": event });
                }
            }
            self.add_event(&mut item.children, event);
        }
    }

    /// Where the text was clicked, in the plugin API's layout coordinates (a char's middle).
    fn context_menu_event(&self, target: &ContextMenuTarget) -> Option<Value> {
        let ContextMenuTarget::Text { document, point } = target else {
            return None;
        };
        let text = &self.state.documents.iter().find(|d| d.id == *document)?.text;
        let (row, col) = line_col(text, (*point).min(text.chars().count()));
        Some(json!({
            "x": (col as f64 + 0.5) * EM_WIDTH,
            "y": (row as f64 + 0.5) * LINE_HEIGHT,
            "modifier_keys": {},
        }))
    }

    /// `close_by_index`, `close_others_by_index`, `close_to_right_by_index` (a tab's menu): the tab
    /// at `index` of `group`, the others of the group, or those after it.
    pub fn close_by_index(&mut self, command: &Command) {
        let group = command.usize_arg("group", self.state.active_group);
        let index = command.usize_arg("index", usize::MAX);
        let tabs = self.group_indexes(group);
        let chosen: Vec<usize> = match command.name.as_str() {
            "close_by_index" => tabs.get(index).copied().into_iter().collect(),
            "close_others_by_index" if index < tabs.len() => {
                tabs.iter().enumerate().filter(|&(i, _)| i != index).map(|(_, &t)| t).collect()
            }
            "close_to_right_by_index" => tabs.iter().skip(index.saturating_add(1)).copied().collect(),
            _ => Vec::new(),
        };
        let ids = chosen.iter().filter_map(|&i| self.state.documents.get(i)).map(|d| d.id).collect::<Vec<_>>();
        if !ids.is_empty() {
            self.close_documents(ids);
        }
    }

    /// `remove_folder {"dirs"}`: the folders leave the window (a top folder's menu).
    pub fn remove_folders(&mut self, command: &Command) {
        for dir in string_list(command, "dirs") {
            self.remove_folder(std::path::Path::new(&dir));
        }
    }

    /// `open_containing_folder {"files"}` (or `paths`): the system's file manager in the folder of
    /// the first one.
    pub fn open_containing_folder(&mut self, command: &Command) {
        let first = string_list(command, "files").into_iter().chain(string_list(command, "paths")).next();
        if let Some(parent) = first.as_deref().and_then(|f| std::path::Path::new(f).parent()) {
            self.gateways.shell.open_in_file_manager(parent);
        }
    }
}

fn string_list(command: &Command, key: &str) -> Vec<String> {
    command.args.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(String::from).collect()
}
