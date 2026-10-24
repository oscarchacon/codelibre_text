//! Tests for the built-in resources in `assets/Default/` (compiled into the binary).

use interface_adapters::gateways::{builtin_resources, color_scheme_file, sublime_json};
use serde_json::Value;

#[test]
fn every_builtin_json_resource_is_valid() {
    for (name, source) in [
        ("Default.sublime-keymap", builtin_resources::KEYMAP),
        ("Default.sublime-commands", builtin_resources::PALETTE_COMMANDS),
        ("Main.sublime-menu", builtin_resources::MAIN_MENU),
        ("Preferences.sublime-settings", builtin_resources::PREFERENCES),
    ] {
        sublime_json::parse(source).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn the_default_preferences_cover_the_documented_settings() {
    let preferences = sublime_json::parse(builtin_resources::PREFERENCES).unwrap();
    for key in [
        "color_scheme", "font_size", "tab_size", "translate_tabs_to_spaces", "auto_indent", "word_wrap",
        "line_numbers", "highlight_line", "rulers", "hot_exit", "import_sublime_packages",
        "folder_exclude_patterns", "file_exclude_patterns",
    ] {
        assert!(preferences.get(key).is_some(), "missing default for {key}");
    }
}

#[test]
fn every_keymap_entry_has_keys_and_a_command() {
    let keymap = sublime_json::parse(builtin_resources::KEYMAP).unwrap();
    for binding in keymap.as_array().unwrap() {
        assert!(binding["keys"].as_array().is_some_and(|k| !k.is_empty()), "{binding}");
        assert!(binding["command"].is_string(), "{binding}");
    }
}

#[test]
fn every_palette_entry_has_a_caption_and_a_command() {
    let palette = sublime_json::parse(builtin_resources::PALETTE_COMMANDS).unwrap();
    for entry in palette.as_array().unwrap() {
        assert!(entry["caption"].is_string() && entry["command"].is_string(), "{entry}");
    }
}

#[test]
fn the_main_menu_has_the_sublime_top_level_menus() {
    let menu = sublime_json::parse(builtin_resources::MAIN_MENU).unwrap();
    let ids: Vec<&str> = menu.as_array().unwrap().iter().filter_map(|m| m["id"].as_str()).collect();
    assert_eq!(ids, vec!["file", "edit", "selection", "find", "view", "goto", "tools", "preferences", "help"]);
}

#[test]
fn the_builtin_color_schemes_define_the_editor_colors() {
    for (name, source) in builtin_resources::COLOR_SCHEMES {
        let scheme = color_scheme_file::parse(source, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        for global in ["background", "foreground", "caret", "selection", "line_highlight", "gutter_foreground"] {
            assert!(scheme.globals.contains_key(global), "{name} has no {global}");
        }
        assert!(scheme.rules.len() > 10, "{name} has rules");
    }
}

/// Every `"command"` value anywhere in a resource.
pub fn commands_in(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(command)) = map.get("command") {
                out.push(command.clone());
            }
            map.values().for_each(|v| commands_in(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| commands_in(v, out)),
        _ => {}
    }
}

#[test]
fn the_builtin_resources_reference_commands() {
    let mut commands = Vec::new();
    for source in [builtin_resources::KEYMAP, builtin_resources::MAIN_MENU, builtin_resources::PALETTE_COMMANDS] {
        commands_in(&sublime_json::parse(source).unwrap(), &mut commands);
    }
    assert!(commands.len() > 100);
}
