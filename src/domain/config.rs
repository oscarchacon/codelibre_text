//! Configuration - Global settings storage

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub theme: Theme,
    pub font_size: f32,
    pub font_family: String,
    pub line_height: f32,
    pub word_wrap: bool,
    pub show_line_numbers: bool,
    pub show_minimap: bool,
    pub show_sidebar: bool,
    pub show_status_bar: bool,
    pub show_command_palette: bool,
    pub tab_size: usize,
    pub use_tabs: bool,
    pub soft_wrap: bool,
    pub mouse_wheel_zoom: bool,
    pub highlight_current_line: bool,
    pub bracket_match: bool,
    pub auto_save: bool,
    pub auto_save_delay: u64,
    pub spell_check: bool,
    pub find_case_sensitive: bool,
    pub find_whole_word: bool,
    pub find_regex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    Custom(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            font_size: 14.0,
            font_family: "monospace".to_string(),
            line_height: 1.5,
            word_wrap: false,
            show_line_numbers: true,
            show_minimap: false,
            show_sidebar: true,
            show_status_bar: true,
            show_command_palette: false,
            tab_size: 4,
            use_tabs: false,
            soft_wrap: false,
            mouse_wheel_zoom: true,
            highlight_current_line: true,
            bracket_match: true,
            auto_save: false,
            auto_save_delay: 3000,
            spell_check: false,
            find_case_sensitive: false,
            find_whole_word: false,
            find_regex: false,
        }
    }
}
