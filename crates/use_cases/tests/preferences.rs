//! Tests for the preferences use cases (`use_cases::preferences`).

use serde_json::{json, Value};
use test_support::{open, World};
use use_cases::gateways::platform_name;

#[test]
fn toggle_setting_changes_the_setting_for_this_session_only() {
    let world = World::new();
    let mut editor = world.editor();
    editor.toggle_setting("word_wrap");
    assert!(editor.view_settings().bool("word_wrap", false));
    assert!(world.resources.user_settings.borrow().is_empty(), "not saved");
}

#[test]
fn set_setting_overrides_any_value() {
    let world = World::new();
    let mut editor = world.editor();
    editor.set_setting("tab_size", json!(2));
    assert_eq!(editor.view_settings().usize("tab_size", 0), 2);
}

#[test]
fn persist_setting_writes_the_user_settings_and_applies_them() {
    let world = World::new();
    let mut editor = world.editor();
    editor.persist_setting("tab_size", json!(8));
    assert_eq!(world.resources.user_settings.borrow()["tab_size"], json!(8));
    assert_eq!(editor.global_settings().usize("tab_size", 0), 8);
}

#[test]
fn persisting_null_removes_the_user_setting() {
    let world = World::new();
    world.resources.user_settings.borrow_mut().insert("tab_size".into(), json!(8));
    let mut editor = world.editor();
    editor.persist_setting("tab_size", Value::Null);
    assert!(!world.resources.user_settings.borrow().contains_key("tab_size"));
    assert_eq!(editor.global_settings().usize("tab_size", 0), 4);
}

#[test]
fn font_size_changes_are_saved_and_clamped() {
    let world = World::new();
    let mut editor = world.editor();
    editor.change_font_size(1.0);
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 12.0);
    editor.persist_setting("font_size", json!(72));
    editor.change_font_size(1.0);
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 72.0);
    editor.persist_setting("font_size", json!(4));
    editor.change_font_size(-1.0);
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 4.0);
}

#[test]
fn reset_font_size_goes_back_to_the_default() {
    let world = World::new();
    let mut editor = world.editor();
    editor.change_font_size(5.0);
    editor.reset_font_size();
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 11.0);
}

#[test]
fn color_scheme_name_is_the_file_stem_of_the_setting() {
    let world = World::new();
    let mut editor = world.editor();
    assert_eq!(editor.color_scheme_name(), "Monokai");
    editor.set_setting("color_scheme", json!("Packages/Color Scheme - Default/Mariana.sublime-color-scheme"));
    assert_eq!(editor.color_scheme_name(), "Mariana");
}

#[test]
fn previewing_a_color_scheme_does_not_save_it() {
    let world = World::new();
    let mut editor = world.editor();
    editor.preview_color_scheme("Daylight");
    assert_eq!(editor.color_scheme_name(), "Daylight");
    assert!(world.resources.user_settings.borrow().is_empty());
}

#[test]
fn applying_a_color_scheme_saves_it() {
    let world = World::new();
    let mut editor = world.editor();
    editor.apply_color_scheme("Daylight");
    assert_eq!(editor.color_scheme_name(), "Daylight");
    assert_eq!(
        world.resources.user_settings.borrow()["color_scheme"],
        json!("Daylight.sublime-color-scheme")
    );
}

#[test]
fn edit_settings_opens_the_defaults_next_to_a_new_user_file() {
    let world = World::new();
    let mut editor = world.editor();
    editor.edit_settings("${packages}/Default/Preferences.sublime-settings", "{\n\t$0\n}\n");

    let docs = &editor.state().documents;
    let defaults = docs.iter().find(|d| d.name() == "Preferences.sublime-settings — Default").unwrap();
    assert!(defaults.read_only);
    assert_eq!(defaults.syntax, "JSON");

    assert_eq!(world.file_text("/packages/User/Preferences.sublime-settings"), "{\n\t\n}\n");
    let user = editor.active_document().unwrap();
    assert_eq!(user.name(), "Preferences.sublime-settings");
    assert_eq!(user.syntax, "JSON");
}

#[test]
fn edit_settings_keeps_an_existing_user_file() {
    let world = World::new().with_file("/packages/User/Preferences.sublime-settings", "{ \"tab_size\": 2 }");
    let mut editor = world.editor();
    editor.edit_settings("${packages}/Default/Preferences.sublime-settings", "{}");
    assert_eq!(world.file_text("/packages/User/Preferences.sublime-settings"), "{ \"tab_size\": 2 }");
}

#[test]
fn edit_key_bindings_uses_the_platform_file_name() {
    let world = World::new();
    let mut editor = world.editor();
    editor.edit_settings("${packages}/Default/Default (${platform}).sublime-keymap", "[\n\t$0\n]\n");
    let file = format!("/packages/User/Default ({}).sublime-keymap", platform_name());
    assert_eq!(world.file_text(&file), "[\n\t\n]\n");
    assert!(editor.state().documents.iter().all(|d| !d.read_only), "no built-in keymap in the fake");
}

#[test]
fn view_settings_include_the_syntax_layer() {
    let world = World::new()
        .with_file("/p/a.py", "")
        .with_syntax_settings("Python", json!({ "tab_size": 2 }));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    assert_eq!(editor.view_settings().usize("tab_size", 0), 2);
    assert_eq!(editor.global_settings().usize("tab_size", 0), 4);
}
