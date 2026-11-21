//! Use cases: the right-click menus (`Context`, `Side Bar`, `Side Bar Mount Point` and `Tab
//! Context.sublime-menu`) and the commands only they run (`close_by_index`, `remove_folder`…).
//!
//! When one opens, the plugins are asked about its commands with the arguments it fills in
//! (`files`, `dirs`, `paths`; `group`, `index`), so `is_visible(paths)` can hide an entry. A text
//! command whose `want_event()` is true gets the click as `event` (`{"x", "y", "modifier_keys"}`:
//! the keys held at the right click, and a point where `view.window_to_text` finds the char
//! again). The text's menu opens on the text, on its gutter (at the line's first char) and on an
//! output panel, whose commands then run on the panel's view.

use entities::command::Command;
use entities::document::{Document, DocumentError, DocumentId, SheetKind};
use entities::menu::{fill_menu, ContextMenuTarget, MenuItem};
use entities::text::selected_text;
use serde_json::{json, Value};

use crate::editor::Editor;
use crate::plugin_commands::menu_commands;
use crate::plugins::CommandRunner;

impl Editor {
    /// A context menu opened on `target`: asks the plugins about its commands.
    pub fn open_context_menu(&mut self, target: &ContextMenuTarget) {
        let mut commands = Vec::new();
        menu_commands(
            &fill_menu(&target.items(&self.state.context_menus), target),
            &mut commands,
        );
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
                let wants = self
                    .plugin_command_state(command)
                    .is_some_and(|s| s.want_event);
                if let (true, Value::Object(args)) = (wants, &mut command.args) {
                    args.insert("event".into(), event.clone());
                } else if wants {
                    command.args = json!({ "event": event });
                }
            }
            self.add_event(&mut item.children, event);
        }
    }

    /// Where the text was clicked, in the plugin API's window coordinates (the middle of the char),
    /// from the view's geometry, so `view.window_to_text(event)` finds the char again.
    fn context_menu_event(&self, target: &ContextMenuTarget) -> Option<Value> {
        let ContextMenuTarget::Text {
            document,
            point,
            modifier_keys,
        } = target
        else {
            return None;
        };
        let doc = self.state.documents.iter().find(|d| d.id == *document)?;
        let geometry = doc.geometry();
        let (x, y) = geometry.text_to_layout(&doc.text, (*point).min(doc.char_count()));
        let (x, y) = geometry
            .layout_to_window((x + geometry.em_width / 2.0, y + geometry.line_height / 2.0));
        let keys: serde_json::Map<String, Value> = modifier_keys
            .iter()
            .map(|key| (key.clone(), Value::Bool(true)))
            .collect();
        Some(json!({ "x": x, "y": y, "modifier_keys": keys }))
    }

    /// A command chosen in the context menu of an output panel's view: it runs on that view, as
    /// Sublime runs a menu's command on the view clicked. Plugin commands get the panel as their
    /// view, `copy` copies its selections, and the built-in commands that work in a panel act on
    /// it; the others (`cut`, `paste`: the panel is read-only) say why they cannot. False when
    /// `view` is not a panel's (the command then runs as usual).
    pub fn run_in_panel_view(
        &mut self,
        view: DocumentId,
        command: &Command,
        run_command: &mut CommandRunner,
    ) -> bool {
        if !self.is_panel_view(view) {
            return false;
        }
        if command.name == "copy" {
            self.copy_view_selections(view);
            return true;
        }
        if let Err(e) = self.plugin_run_command("view", view, command, run_command) {
            self.notify(e);
        }
        true
    }

    /// The selections of any view (a panel's too), one per line, to the clipboard and its history.
    fn copy_view_selections(&mut self, view: DocumentId) {
        let Some(doc) = self.any_document(view) else {
            return;
        };
        let text = doc
            .selections()
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| selected_text(&doc.text, s))
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            return;
        }
        if let Err(e) = self.gateways.clipboard.write_text(&text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.push_clipboard_history(&text);
    }

    /// The commands of a tab's menu (`Tab Context` and `Tab Sidebar Context.sublime-menu`) that
    /// close tabs of `group`: `close_by_index` the tab at `index`, `close_others_by_index` the
    /// others, `close_to_right_by_index` those after it, `close_unmodified` the unmodified ones,
    /// `close_unmodified_to_right_by_index` the unmodified ones after it, `close_deleted_files`
    /// those whose file is no longer on disk. With one tab selected at a time (tabs are not
    /// multi-selected), `close_selected` is the tab clicked and `close_unselected` the others.
    pub fn close_by_index(&mut self, command: &Command) {
        let group = command.usize_arg("group", self.state.active_group);
        let index = command.usize_arg("index", usize::MAX);
        let tabs = self.group_indexes(group);
        let unmodified = |&i: &usize| !self.state.documents[i].is_dirty();
        let chosen: Vec<usize> = match command.name.as_str() {
            "close_by_index" | "close_selected" => tabs.get(index).copied().into_iter().collect(),
            "close_unmodified" => tabs.iter().copied().filter(unmodified).collect(),
            "close_unmodified_to_right_by_index" => tabs
                .iter()
                .skip(index.saturating_add(1))
                .copied()
                .filter(unmodified)
                .collect(),
            "close_deleted_files" => tabs
                .iter()
                .copied()
                .filter(|&i| {
                    let doc = &self.state.documents[i];
                    doc.path
                        .as_deref()
                        .is_some_and(|path| !self.gateways.file_system.exists(path))
                })
                .collect(),
            "close_others_by_index" | "close_unselected" if index < tabs.len() => tabs
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != index)
                .map(|(_, &t)| t)
                .collect(),
            "close_to_right_by_index" => {
                tabs.iter().skip(index.saturating_add(1)).copied().collect()
            }
            _ => Vec::new(),
        };
        let ids = chosen
            .iter()
            .filter_map(|&i| self.state.documents.get(i))
            .map(|d| d.id)
            .collect::<Vec<_>>();
        if !ids.is_empty() {
            self.close_documents(ids);
        }
    }

    /// `copy_path` (Sublime's `Default/copy_path.py`, in the image's menu): the active view's file
    /// path to the clipboard. A view without a file has nothing to copy.
    pub fn copy_file_path(&mut self) {
        let Some(path) = self.active_document().and_then(|d| d.path.clone()) else {
            return;
        };
        let text = path.display().to_string();
        if let Err(e) = self.gateways.clipboard.write_text(&text) {
            self.notify(self.tr_args("Clipboard unavailable: {error}", &[("error", &e)]));
            return;
        }
        self.push_clipboard_history(&text);
    }

    /// `force_reopen_as {"type": "text"}` (the image's menu, "Reopen File as Text"): the active
    /// image sheet becomes a view of the file's text in its place (an SVG's XML); a binary file
    /// stays a picture and the status bar says why.
    pub fn reopen_as_text(&mut self) {
        let Some(doc) = self.active_document() else {
            return;
        };
        let (Some(path), true) = (doc.path.clone(), doc.sheet == SheetKind::Image) else {
            return;
        };
        let (old, group) = (doc.id, doc.group);
        let bytes = match self.gateways.file_system.read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.notify(format!("{}: {e}", path.display()));
                return;
            }
        };
        let id = self.next_document_id();
        let modified = self.gateways.file_system.modified(&path);
        let mut text = match Document::from_bytes(id, &path, bytes, modified) {
            Ok(doc) => doc,
            Err(DocumentError::Binary) => {
                self.notify(self.tr_args(
                    "{path} looks like a binary file",
                    &[("path", &path.display())],
                ));
                return;
            }
        };
        text.syntax = self.gateways.syntaxes.detect(&path, text.first_line());
        text.group = group;
        let index = self.state.active;
        self.state.documents[index] = text;
        self.gateways.syntaxes.forget(old);
        self.state.recently_used.retain(|used| *used != old);
        self.activate_document(id);
        self.plugin_event("on_load", id);
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
        let first = string_list(command, "files")
            .into_iter()
            .chain(string_list(command, "paths"))
            .next();
        if let Some(parent) = first
            .as_deref()
            .and_then(|f| std::path::Path::new(f).parent())
        {
            self.gateways.shell.open_in_file_manager(parent);
        }
    }
}

fn string_list(command: &Command, key: &str) -> Vec<String> {
    command
        .args
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(String::from)
        .collect()
}
