//! Use cases: settings, font size, color scheme and editing the settings files.

use std::path::Path;

use serde_json::Value;

use crate::editor::Editor;
use crate::gateways::platform_name;

const DEFAULT_COLOR_SCHEME: &str = "Monokai";

impl Editor {
    /// `toggle_setting`: flips a boolean setting for this session (not saved).
    pub fn toggle_setting(&mut self, key: &str) {
        self.state.settings.toggle_override(key);
    }

    /// `set_setting`: sets a value for this session (not saved).
    pub fn set_setting(&mut self, key: &str, value: Value) {
        self.state.settings.set_override(key, value);
    }

    /// Writes a setting to the user's `Preferences.sublime-settings` (`Value::Null` removes it).
    pub fn persist_setting(&mut self, key: &str, value: Value) {
        let mut values = self.gateways.resources.read_user_settings();
        if value.is_null() {
            values.remove(key);
        } else {
            values.insert(key.to_string(), value);
        }
        if let Err(e) = self.gateways.resources.write_user_settings(&values) {
            self.notify(e);
        }
        self.reload_settings();

        // Keep an open, unmodified copy of the file in sync.
        let user_file = self.gateways.resources.paths().user_settings_file();
        if let Some(index) = self
            .state
            .documents
            .iter()
            .position(|d| d.path.as_deref() == Some(user_file.as_path()) && !d.is_dirty())
        {
            let active = self.state.active;
            self.state.active = index;
            self.revert_active_document();
            self.state.active = active;
        }
    }

    pub fn change_font_size(&mut self, delta: f32) {
        let size = self.global_settings().f32("font_size", 11.0) + delta;
        self.persist_setting("font_size", Value::from(size.clamp(4.0, 72.0)));
    }

    pub fn reset_font_size(&mut self) {
        self.persist_setting("font_size", Value::Null);
    }

    /// File stem of the `color_scheme` setting (e.g. `Monokai`).
    pub fn color_scheme_name(&self) -> String {
        let setting = self.global_settings().str("color_scheme").unwrap_or(DEFAULT_COLOR_SCHEME).to_string();
        Path::new(&setting)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or(setting)
    }

    /// Shows a color scheme without saving it (while moving through the picker).
    pub fn preview_color_scheme(&mut self, name: &str) {
        self.set_setting("color_scheme", Value::String(name.to_string()));
    }

    /// Saves the chosen color scheme in the user's settings.
    pub fn apply_color_scheme(&mut self, name: &str) {
        self.persist_setting("color_scheme", Value::String(format!("{name}.sublime-color-scheme")));
        self.set_setting("color_scheme", Value::String(name.to_string()));
    }

    /// Sublime's `edit_settings`: opens the built-in defaults (read only) next to the user's file,
    /// creating the user's file from `default_contents` if needed.
    pub fn edit_settings(&mut self, base_file: &str, default_contents: &str) {
        let base_file = base_file.replace("${platform}", platform_name());
        let file_name = base_file.rsplit('/').next().unwrap_or(&base_file).to_string();

        if let Some(defaults) = self.gateways.resources.default_resource(&file_name) {
            self.open_scratch(&format!("{file_name} — Default"), &defaults, "JSON", true);
        }
        let user_file = self.gateways.resources.paths().user_dir().join(&file_name);
        if !self.gateways.file_system.exists(&user_file) {
            let contents = default_contents.replace("$0", "");
            if let Err(e) = self.gateways.file_system.write(&user_file, contents.as_bytes()) {
                self.notify(format!("{}: {e}", user_file.display()));
            }
        }
        if self.open_file(&user_file) {
            self.set_syntax("JSON");
        }
    }
}
