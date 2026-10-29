//! [`ResourceGateway`] for Sublime Text's settings, key bindings, menus and command palette files.

use std::path::{Path, PathBuf};

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo, Keymap};
use entities::localization::{normalize_language_code, Catalog, Language};
use entities::menu::{merge_menu, MenuItem, PaletteCommand};
use entities::settings::SettingsLayers;
use serde_json::{Map, Value};
use use_cases::gateways::{platform_name, PackagePaths, ResourceGateway, UiResources};
use use_cases::project::matches_pattern;

use super::package_files::{find_resources, is_in_default_package, resource_name, resource_path};
use super::{builtin_resources, po_file, sublime_json};

const PREFERENCES: &str = "Preferences.sublime-settings";

pub struct SublimeResourceGateway {
    paths: PackagePaths,
    system_language: Option<String>,
}

impl SublimeResourceGateway {
    pub fn new(paths: PackagePaths) -> Self {
        let _ = std::fs::create_dir_all(paths.user_dir());
        Self { paths, system_language: None }
    }

    /// The operating system's language, read by the caller (it is not a file).
    pub fn with_system_language(mut self, language: Option<String>) -> Self {
        self.system_language = language;
        self
    }

    /// `Localization/<language>.po` files of every package, in load order.
    fn translation_files(&self, include_sublime: bool) -> Vec<PathBuf> {
        find_resources(&self.paths, include_sublime, |n| n.ends_with(".po"))
            .into_iter()
            .filter(|p| p.parent().and_then(|d| d.file_name()).is_some_and(|d| d == "Localization"))
            .collect()
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
        self.read_user_settings_file(PREFERENCES)
    }

    fn write_user_settings(&self, values: &Map<String, Value>) -> Result<(), String> {
        self.write_user_settings_file(PREFERENCES, values)
    }

    fn load_settings_file(&self, file_name: &str, include_sublime: bool, errors: &mut Vec<String>) -> Map<String, Value> {
        let stem = file_name.trim_end_matches(".sublime-settings");
        let platform_file = format!("{stem} ({}).sublime-settings", platform_name());
        let mut values = Map::new();
        for path in find_resources(&self.paths, include_sublime, |n| n == file_name || n == platform_file) {
            if is_in_default_package(&path) {
                continue;
            }
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| sublime_json::parse(&text).map_err(|e| e.to_string()));
            match parsed {
                Ok(Value::Object(map)) => values.extend(map),
                Ok(_) => {}
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        values
    }

    fn read_user_settings_file(&self, file_name: &str) -> Map<String, Value> {
        std::fs::read_to_string(self.paths.user_dir().join(file_name))
            .ok()
            .and_then(|text| sublime_json::parse(&text).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    }

    fn write_user_settings_file(&self, file_name: &str, values: &Map<String, Value>) -> Result<(), String> {
        let path = self.paths.user_dir().join(file_name);
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

    fn languages(&self, include_sublime: bool) -> Vec<Language> {
        let mut languages: Vec<Language> = Vec::new();
        let mut add = |code: String, text: &str| {
            if languages.iter().any(|l| l.code == code) {
                return;
            }
            let name = po_file::parse(text).ok().and_then(|po| po.language_name).unwrap_or_else(|| code.clone());
            languages.push(Language { code, name });
        };
        for (code, text) in builtin_resources::TRANSLATIONS {
            add(code.to_string(), text);
        }
        for path in self.translation_files(include_sublime) {
            let code = path.file_stem().map(|s| normalize_language_code(&s.to_string_lossy())).unwrap_or_default();
            if let Ok(text) = std::fs::read_to_string(&path) {
                add(code, &text);
            }
        }
        languages.sort_by_key(|l| l.name.to_lowercase());
        languages
    }

    fn load_translations(&self, language: &str, include_sublime: bool, errors: &mut Vec<String>) -> Catalog {
        let mut sources: Vec<(String, String)> = builtin_resources::TRANSLATIONS
            .iter()
            .filter(|(code, _)| *code == language)
            .map(|(code, text)| (format!("Default/Localization/{code}.po"), text.to_string()))
            .collect();
        for path in self.translation_files(include_sublime) {
            let code = path.file_stem().map(|s| normalize_language_code(&s.to_string_lossy()));
            if code.as_deref() == Some(language) {
                match std::fs::read_to_string(&path) {
                    Ok(text) => sources.push((path.display().to_string(), text)),
                    Err(e) => errors.push(format!("{}: {e}", path.display())),
                }
            }
        }
        let mut catalog = Catalog::default();
        for (origin, text) in sources {
            match po_file::parse(&text) {
                Ok(po) => catalog.merge(po.catalog),
                Err(e) => errors.push(format!("{origin}: {e}")),
            }
        }
        catalog
    }

    fn find_resources(&self, pattern: &str, include_sublime: bool) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for path in find_resources(&self.paths, include_sublime, |n| matches_pattern(pattern, n)) {
            match resource_name(&self.paths, include_sublime, &path) {
                Some(name) if !names.contains(&name) => names.push(name),
                _ => {}
            }
        }
        names
    }

    fn load_resource(&self, name: &str, include_sublime: bool) -> Result<Vec<u8>, String> {
        let path = resource_path(&self.paths, include_sublime, name).ok_or_else(|| format!("resource not found: {name}"))?;
        std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn system_language(&self) -> Option<String> {
        self.system_language.clone()
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
