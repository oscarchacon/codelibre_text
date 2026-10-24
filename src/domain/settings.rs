//! Settings management - User and default settings

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub theme: String,
    pub font_size: f32,
    pub font_family: String,
    pub line_height: f32,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub show_minimap: bool,
    pub tab_size: usize,
    pub use_tabs: bool,
    pub highlight_current_line: bool,
    pub mouse_wheel_zoom: bool,
    pub auto_save: bool,
    pub auto_save_delay: u64,
    pub color_scheme: String,
    pub detect_indentation: bool,
    pub trim_trailing_white_space: bool,
    pub ensure_newline_at_eof: bool,
    pub delete_empty_lines_on_save: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 14.0,
            font_family: "JetBrains Mono".to_string(),
            line_height: 1.5,
            word_wrap: false,
            show_line_numbers: true,
            show_minimap: false,
            tab_size: 4,
            use_tabs: false,
            highlight_current_line: true,
            mouse_wheel_zoom: true,
            auto_save: false,
            auto_save_delay: 3000,
            color_scheme: "CodeLibre Text Dark".to_string(),
            detect_indentation: true,
            trim_trailing_white_space: true,
            ensure_newline_at_eof: true,
            delete_empty_lines_on_save: true,
        }
    }
}

/// Settings manager
#[derive(Debug)]
pub struct SettingsManager {
    pub defaults: UserSettings,
    pub user_settings: UserSettings,
    pub settings_path: PathBuf,
}

impl SettingsManager {
    pub fn new() -> Self {
        let base = directories::BaseDirs::new();
        let settings_path = base
            .as_ref()
            .and_then(|b| Some(b.config_dir().to_path_buf()))
            .map(|p| p.join("codelibre_text").join("User"))
            .unwrap_or_default();

        Self {
            defaults: UserSettings::default(),
            user_settings: UserSettings::default(),
            settings_path,
        }
    }

    pub fn load(&mut self) {
        let settings_file = self.settings_path.join("Default (User).sublime-settings");
        if let Ok(content) = std::fs::read_to_string(&settings_file) {
            if let Ok(parsed) = serde_json::from_str::<UserSettings>(&content) {
                self.user_settings = parsed;
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.settings_path)?;
        let settings_file = self.settings_path.join("Default (User).sublime-settings");
        let content = serde_json::to_string_pretty(&self.user_settings)?;
        std::fs::write(&settings_file, content)?;
        Ok(())
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        match key {
            "theme" => Some(self.user_settings.theme.clone()),
            "font_family" => Some(self.user_settings.font_family.clone()),
            "color_scheme" => Some(self.user_settings.color_scheme.clone()),
            _ => None,
        }
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        match key {
            "font_size" => Some(self.user_settings.font_size),
            "line_height" => Some(self.user_settings.line_height),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match key {
            "word_wrap" => Some(self.user_settings.word_wrap),
            "show_line_numbers" => Some(self.user_settings.show_line_numbers),
            "show_minimap" => Some(self.user_settings.show_minimap),
            "use_tabs" => Some(self.user_settings.use_tabs),
            "highlight_current_line" => Some(self.user_settings.highlight_current_line),
            "mouse_wheel_zoom" => Some(self.user_settings.mouse_wheel_zoom),
            "auto_save" => Some(self.user_settings.auto_save),
            "detect_indentation" => Some(self.user_settings.detect_indentation),
            "trim_trailing_white_space" => Some(self.user_settings.trim_trailing_white_space),
            "ensure_newline_at_eof" => Some(self.user_settings.ensure_newline_at_eof),
            "delete_empty_lines_on_save" => Some(self.user_settings.delete_empty_lines_on_save),
            _ => None,
        }
    }

    pub fn get_usize(&self, key: &str) -> Option<usize> {
        match key {
            "tab_size" => Some(self.user_settings.tab_size),
            _ => None,
        }
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        match key {
            "auto_save_delay" => Some(self.user_settings.auto_save_delay),
            _ => None,
        }
    }

    pub fn update_setting(&mut self, key: &str, value: &str) {
        match key {
            "theme" => self.user_settings.theme = value.to_string(),
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.user_settings.font_size = size;
                }
            }
            "font_family" => self.user_settings.font_family = value.to_string(),
            "line_height" => {
                if let Ok(height) = value.parse::<f32>() {
                    self.user_settings.line_height = height;
                }
            }
            "word_wrap" => self.user_settings.word_wrap = value.parse::<bool>().unwrap_or(false),
            "show_line_numbers" => {
                self.user_settings.show_line_numbers = value.parse::<bool>().unwrap_or(true);
            }
            "show_minimap" => {
                self.user_settings.show_minimap = value.parse::<bool>().unwrap_or(false);
            }
            "tab_size" => {
                if let Ok(size) = value.parse::<usize>() {
                    self.user_settings.tab_size = size;
                }
            }
            "use_tabs" => self.user_settings.use_tabs = value.parse::<bool>().unwrap_or(false),
            "highlight_current_line" => {
                self.user_settings.highlight_current_line = value.parse::<bool>().unwrap_or(true);
            }
            "auto_save" => self.user_settings.auto_save = value.parse::<bool>().unwrap_or(false),
            "auto_save_delay" => {
                if let Ok(delay) = value.parse::<u64>() {
                    self.user_settings.auto_save_delay = delay;
                }
            }
            "color_scheme" => self.user_settings.color_scheme = value.to_string(),
            "detect_indentation" => {
                self.user_settings.detect_indentation = value.parse::<bool>().unwrap_or(true);
            }
            "trim_trailing_white_space" => {
                self.user_settings.trim_trailing_white_space = value.parse::<bool>().unwrap_or(true);
            }
            _ => {}
        }

        // Save settings
        if let Err(e) = self.save() {
            eprintln!("Failed to save settings: {}", e);
        }
    }
}
