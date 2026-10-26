//! Use cases: Sublime Text plugins written in Python (`TextCommand`, `WindowCommand`,
//! `ApplicationCommand`), and the console where their output goes.
//!
//! Plugins run in a separate process behind [`PluginHostGateway`](crate::gateways::PluginHostGateway).
//! While a plugin command runs, the plugin calls back into the editor (`sublime_api` functions,
//! served by [`crate::plugin_api`]) until the host says the command is done. See `docs/plugins.md`.

use std::collections::HashMap;

use entities::command::Command;
use entities::document::DocumentId;
use entities::selection::Selection;
use serde_json::{Map, Value};

use crate::editor::Editor;
use crate::gateways::{PluginCommandKind, PluginLoad, PluginMessage, PluginPackage};

/// The console keeps at most this many bytes; the oldest output goes first.
const CONSOLE_LIMIT: usize = 200_000;
/// Plugin commands that run each other deeper than this are stopped.
const MAX_DEPTH: usize = 16;
const CONSOLE_NAME: &str = "Console";

/// Runs another command (built-in or plugin) when a plugin calls `run_command`. The command
/// controller passes itself in: built-in commands live in an outer circle.
pub type CommandRunner<'a> = dyn FnMut(&mut Editor, &Command) + 'a;

/// A settings file opened by a plugin with `sublime.load_settings`.
#[derive(Default)]
pub(crate) struct SettingsFile {
    pub values: Map<String, Value>,
    /// What plugins changed since the last `save_settings` (`Null`: erased).
    pub changes: Map<String, Value>,
}

#[derive(Default)]
pub struct PluginState {
    pub(crate) commands: HashMap<String, PluginCommandKind>,
    /// Settings plugins set on a view (`view.settings().set(...)`).
    pub(crate) view_settings: HashMap<DocumentId, Map<String, Value>>,
    pub(crate) settings_files: HashMap<String, SettingsFile>,
    pub(crate) window_settings: Map<String, Value>,
    /// The errors of the last load (also in the package load errors).
    pub(crate) errors: Vec<String>,
    depth: usize,
}

impl Editor {
    /// Starts the plugin host with the plugins of every package (when there are any) and
    /// registers their commands. Problems go to the package load errors and the console.
    pub fn load_plugins(&mut self) {
        self.state.plugins.commands.clear();
        self.state.plugins.settings_files.clear();
        let old_errors = std::mem::take(&mut self.state.plugins.errors);
        self.state.load_errors.retain(|e| !old_errors.contains(e));
        let settings = self.global_settings();
        let packages = if settings.bool("python_plugins", true) {
            self.plugin_packages()
        } else {
            Vec::new()
        };
        if packages.is_empty() {
            self.gateways.plugins.stop();
            return;
        }
        let request = PluginLoad {
            packages,
            packages_path: self.gateways.resources.paths().packages.clone(),
            interpreter: settings.str("python_interpreter").filter(|s| !s.is_empty()).map(String::from),
        };
        if let Err(e) = self.gateways.plugins.load(&request) {
            self.plugin_load_error(format!("Python plugins: {e}"));
            return;
        }
        match self.converse(&mut |editor: &mut Editor, command: &Command| {
            editor.log(&format!("{}: built-in commands cannot run while plugins load\n", command.name));
        }) {
            Ok(PluginMessage::Loaded { commands, errors }) => {
                self.state.plugins.commands = commands.into_iter().collect();
                errors.into_iter().for_each(|e| self.plugin_load_error(e));
            }
            Ok(_) => {}
            Err(e) => self.plugin_load_error(format!("Python plugins: {e}")),
        }
    }

    /// Runs a plugin command on the active view (text commands) or the window.
    /// Returns false when no plugin defines it.
    pub fn run_plugin_command(&mut self, command: &Command, run_command: &mut CommandRunner) -> bool {
        let Some(&kind) = self.state.plugins.commands.get(&command.name) else {
            return false;
        };
        if self.state.plugins.depth >= MAX_DEPTH {
            self.report_plugin_error(&command.name, "plugin commands run each other too deeply");
            return true;
        }
        let view = self.active_document().map(|d| d.id);
        self.state.plugins.depth += 1;
        let result = self
            .gateways
            .plugins
            .run(kind, &command.name, &command.args, view)
            .and_then(|()| self.converse(run_command));
        self.state.plugins.depth -= 1;
        match result {
            Ok(PluginMessage::Done { error: Some(traceback) }) => self.report_plugin_error(&command.name, &traceback),
            Ok(_) => {}
            Err(e) => self.report_plugin_error(&command.name, &e),
        }
        true
    }

    pub fn is_plugin_command(&self, name: &str) -> bool {
        self.state.plugins.commands.contains_key(name)
    }

    /// Adds text to the console.
    pub fn log(&mut self, text: &str) {
        let console = &mut self.state.console;
        console.push_str(text);
        if console.len() > CONSOLE_LIMIT {
            let mut cut = console.len() - CONSOLE_LIMIT;
            while !console.is_char_boundary(cut) {
                cut += 1;
            }
            console.drain(..cut);
        }
    }

    /// `show_panel {"panel": "console"}`: the console in a read-only tab, scrolled to the end.
    pub fn show_console(&mut self) {
        let text = self.state.console.clone();
        let existing = self
            .state
            .documents
            .iter()
            .position(|d| d.scratch_name.as_deref() == Some(CONSOLE_NAME) && d.read_only);
        match existing {
            Some(index) => {
                self.state.documents[index].text = text;
                self.activate(index);
            }
            None => self.open_scratch(CONSOLE_NAME, &text, "Plain Text", true),
        }
        if let Some(doc) = self.active_document_mut() {
            let end = doc.char_count();
            doc.set_selection(Selection::caret(end));
        }
    }

    /// `show_panel {"panel": "console", "toggle": true}` (Ctrl+`): closes the console when it is the
    /// active tab, shows it otherwise.
    pub fn toggle_console(&mut self) {
        match self.active_document() {
            Some(doc) if doc.scratch_name.as_deref() == Some(CONSOLE_NAME) && doc.read_only => {
                let id = doc.id;
                self.close_documents(vec![id]);
            }
            _ => self.show_console(),
        }
    }

    /// Answers the host's calls until it finishes the current request (`Loaded` or `Done`).
    fn converse(&mut self, run_command: &mut CommandRunner) -> Result<PluginMessage, String> {
        loop {
            match self.gateways.plugins.receive()? {
                PluginMessage::Call { function, args } => {
                    let result = self.plugin_call(&function, &args, run_command);
                    self.gateways.plugins.reply(result)?;
                }
                PluginMessage::Log(text) => self.log(&text),
                finished => return Ok(finished),
            }
        }
    }

    /// Packages with `.py` files at their top level, in Sublime's order (`User` last). Sublime's
    /// own `Default` package is skipped: it is written for Sublime's native core.
    fn plugin_packages(&self) -> Vec<PluginPackage> {
        let paths = self.gateways.resources.paths().clone();
        let mut roots = Vec::new();
        if self.global_settings().bool("import_sublime_packages", true) {
            roots.extend(paths.sublime_packages.map(|root| (root, true)));
        }
        roots.push((paths.packages, false));

        let fs = &self.gateways.file_system;
        let mut packages: Vec<PluginPackage> = Vec::new();
        for (root, is_sublime) in roots {
            for entry in fs.list_dir(&root).into_iter().filter(|e| e.is_dir) {
                let name = entry.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if is_sublime && name == "Default" {
                    continue;
                }
                let has_plugins = fs
                    .list_dir(&entry.path)
                    .iter()
                    .any(|f| !f.is_dir && f.path.extension().is_some_and(|e| e == "py"));
                if !has_plugins {
                    continue;
                }
                match packages.iter_mut().find(|p| p.name == name) {
                    Some(package) => package.dirs.push(entry.path),
                    None => packages.push(PluginPackage { name, dirs: vec![entry.path] }),
                }
            }
        }
        packages.sort_by_key(|p| {
            let rank = match p.name.as_str() {
                "Default" => 0,
                "User" => 2,
                _ => 1,
            };
            (rank, p.name.to_lowercase())
        });
        packages
    }

    fn plugin_load_error(&mut self, error: String) {
        self.log(&format!("{error}\n"));
        self.state.plugins.errors.push(error.clone());
        self.state.load_errors.push(error);
    }

    fn report_plugin_error(&mut self, name: &str, error: &str) {
        self.log(&format!("{name}: {error}\n"));
        let summary = error.trim_end().lines().last().unwrap_or(error).to_string();
        self.notify(self.tr_args("{name} failed: {error} (View > Show Console)", &[("name", &name), ("error", &summary)]));
    }
}
