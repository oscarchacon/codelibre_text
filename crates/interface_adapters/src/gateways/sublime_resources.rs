//! [`ResourceGateway`] for Sublime Text's settings, key bindings, menus and command palette files.

use std::path::Path;

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo, Keymap};
use entities::menu::{merge_menu, MenuItem, PaletteCommand};
use entities::settings::SettingsLayers;
use serde_json::{Map, Value};
use use_cases::gateways::{platform_name, PackagePaths, ResourceGateway, UiResources};

use super::package_files::{find_resources, is_in_default_package};
use super::{builtin_resources, sublime_json};

pub struct SublimeResourceGateway {
    paths: PackagePaths,
}

impl SublimeResourceGateway {
    pub fn new(paths: PackagePaths) -> Self {
        let _ = std::fs::create_dir_all(paths.user_dir());
        Self { paths }
    }

    /// Built-in file followed by the matching files of every package (except Sublime's `Default`).
    fn sources(&self, include_sublime: bool, builtin: &str, matches: impl Fn(&str) -> bool) -> Vec<(String, String)> {
        let mut sources = vec![("Default".to_string(), builtin.to_string())];
        for path in find_resources(&self.paths, include_sublime, matches) {
            if is_in_default_package(&path) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                sources.push((path.display().to_string(), text));
            }
        }
        sources
    }
}

impl ResourceGateway for SublimeResourceGateway {
    fn paths(&self) -> &PackagePaths {
        &self.paths
    }

    fn load_settings(&self, errors: &mut Vec<String>) -> SettingsLayers {
        let mut layers = SettingsLayers::default();
        let mut add = |origin: &str, text: &str, layers: &mut SettingsLayers, name: &str| match sublime_json::parse(text) {
            Ok(value) => {
                let platform_prefs = format!("Preferences ({}).sublime-settings", platform_name());
                if name == "Preferences.sublime-settings" || name == platform_prefs {
                    layers.add_preferences(&value);
                } else {
                    layers.add_syntax_layer(name.trim_end_matches(".sublime-settings"), value);
                }
            }
            Err(e) => errors.push(format!("{origin}: {e}")),
        };
        add("Default", builtin_resources::PREFERENCES, &mut layers, "Preferences.sublime-settings");

        // The user decides whether Sublime Text's own packages are read too.
        let include_sublime = self
            .read_user_settings()
            .get("import_sublime_packages")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        for path in find_resources(&self.paths, include_sublime, |n| n.ends_with(".sublime-settings")) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if let Ok(text) = std::fs::read_to_string(&path) {
                add(&path.display().to_string(), &text, &mut layers, &name);
            }
        }
        layers
    }

    fn load_ui_resources(&self, include_sublime: bool, errors: &mut Vec<String>) -> UiResources {
        let platform_keymap = format!("Default ({}).sublime-keymap", platform_name());
        let mut keymap = Keymap::default();
        for (origin, text) in self.sources(include_sublime, builtin_resources::KEYMAP, |n| {
            n == "Default.sublime-keymap" || n == platform_keymap
        }) {
            match sublime_json::parse(&text) {
                Ok(Value::Array(items)) => items.iter().filter_map(parse_binding).for_each(|b| keymap.add(b)),
                Ok(_) => {}
                Err(e) => errors.push(format!("{origin}: {e}")),
            }
        }

        let mut main_menu = Vec::new();
        for (origin, text) in self.sources(include_sublime, builtin_resources::MAIN_MENU, |n| n == "Main.sublime-menu") {
            match sublime_json::parse(&text) {
                Ok(Value::Array(items)) => merge_menu(&mut main_menu, items.iter().filter_map(parse_menu_item).collect()),
                Ok(_) => {}
                Err(e) => errors.push(format!("{origin}: {e}")),
            }
        }

        let mut palette = Vec::new();
        for (origin, text) in self.sources(include_sublime, builtin_resources::PALETTE_COMMANDS, |n| {
            n.ends_with(".sublime-commands")
        }) {
            match sublime_json::parse(&text) {
                Ok(Value::Array(items)) => palette.extend(items.iter().filter_map(parse_palette_command)),
                Ok(_) => {}
                Err(e) => errors.push(format!("{origin}: {e}")),
            }
        }

        UiResources { keymap, main_menu, palette }
    }

    fn read_user_settings(&self) -> Map<String, Value> {
        std::fs::read_to_string(self.paths.user_settings_file())
            .ok()
            .and_then(|text| sublime_json::parse(&text).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    fn write_user_settings(&self, values: &Map<String, Value>) -> Result<(), String> {
        let path = self.paths.user_settings_file();
        let text = serde_json::to_string_pretty(values).map_err(|e| e.to_string())?;
        std::fs::write(&path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    fn default_resource(&self, file_name: &str) -> Option<String> {
        if file_name == "Preferences.sublime-settings" {
            Some(builtin_resources::PREFERENCES.to_string())
        } else if Path::new(file_name).extension().is_some_and(|e| e == "sublime-keymap") {
            Some(builtin_resources::KEYMAP.to_string())
        } else {
            None
        }
    }
}

fn command_from(name: Option<&Value>, args: Option<&Value>) -> Option<Command> {
    Some(Command::with_args(name?.as_str()?, args.cloned().unwrap_or(Value::Null)))
}

/// `{"keys": ["ctrl+k", "ctrl+b"], "command": "...", "args": {...}, "context": [...]}`
fn parse_binding(item: &Value) -> Option<Binding> {
    let keys = item
        .get("keys")?
        .as_array()?
        .iter()
        .map(|k| k.as_str().and_then(KeyCombo::parse))
        .collect::<Option<Vec<_>>>()?;
    Some(Binding {
        keys,
        command: command_from(item.get("command"), item.get("args"))?,
        context: item.get("context").and_then(Value::as_array).cloned().unwrap_or_default(),
    })
}

/// `{"caption": "&File", "id": "file", "children": [...]}` or `{"command": "...", "args": {...}}`
fn parse_menu_item(item: &Value) -> Option<MenuItem> {
    let item = item.as_object()?;
    let command = command_from(item.get("command"), item.get("args"));
    let caption = item
        .get("caption")
        .and_then(Value::as_str)
        .map(|c| c.replace('&', ""))
        .or_else(|| command.as_ref().map(|c| c.name.clone()))
        .unwrap_or_default();
    Some(MenuItem {
        caption,
        id: item.get("id").and_then(Value::as_str).map(String::from),
        command,
        checkbox: item.get("checkbox").and_then(Value::as_bool).unwrap_or(false),
        children: item
            .get("children")
            .and_then(Value::as_array)
            .map(|c| c.iter().filter_map(parse_menu_item).collect())
            .unwrap_or_default(),
    })
}

/// `{"caption": "File: Save", "command": "save", "args": {...}}`
fn parse_palette_command(item: &Value) -> Option<PaletteCommand> {
    Some(PaletteCommand {
        caption: item.get("caption")?.as_str()?.to_string(),
        command: command_from(item.get("command"), item.get("args"))?,
    })
}
