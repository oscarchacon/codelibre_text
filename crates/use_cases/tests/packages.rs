//! Tests for the package use cases (`use_cases::packages`).

use std::path::PathBuf;

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use entities::menu::{MenuItem, PaletteCommand};
use serde_json::json;
use test_support::{status, World};

fn save_binding() -> Binding {
    Binding { keys: vec![KeyCombo::parse("ctrl+s").unwrap()], command: Command::new("save"), context: vec![] }
}

#[test]
fn startup_loads_key_bindings_menus_and_palette_entries() {
    let world = World::new()
        .with_binding(save_binding())
        .with_menu(MenuItem { caption: "File".into(), id: Some("file".into()), command: None, checkbox: false, children: vec![] })
        .with_palette(PaletteCommand { caption: "File: Save".into(), command: Command::new("save") });
    let editor = world.editor();
    let state = editor.state();
    assert_eq!(state.keymap.bindings.len(), 1);
    assert_eq!(state.main_menu[0].caption, "File");
    assert_eq!(state.palette[0].caption, "File: Save");
}

#[test]
fn sublime_packages_are_included_by_default() {
    let world = World::new();
    world.editor();
    assert_eq!(*world.resources.ui_loads.borrow(), vec![true]);
    assert_eq!(*world.syntaxes.reloads.borrow(), vec![true]);
}

#[test]
fn import_sublime_packages_false_leaves_them_out() {
    let world = World::new().with_preferences(json!({ "import_sublime_packages": false }));
    world.editor();
    assert_eq!(*world.resources.ui_loads.borrow(), vec![false]);
    assert_eq!(*world.syntaxes.reloads.borrow(), vec![false]);
}

#[test]
fn load_errors_are_kept_and_announced() {
    let world = World::new();
    world.resources.errors.borrow_mut().push("bad.sublime-keymap: expected `]`".into());
    let editor = world.editor();
    assert_eq!(editor.state().load_errors, vec!["bad.sublime-keymap: expected `]`"]);
    assert!(status(&editor).contains("1 package resources failed to load"));
}

#[test]
fn reload_packages_reads_everything_again_and_reports_counts() {
    let world = World::new().with_binding(save_binding());
    let mut editor = world.editor();
    world.resources.bindings.borrow_mut().push(save_binding());
    editor.reload_packages();
    assert_eq!(editor.state().keymap.bindings.len(), 2);
    assert_eq!(world.resources.ui_loads.borrow().len(), 2);
    assert_eq!(status(&editor), "Packages reloaded: 6 syntaxes, 3 color schemes, 2 key bindings, 0 plugin commands");
}

#[test]
fn reload_settings_only_reads_the_settings() {
    let world = World::new();
    let mut editor = world.editor();
    world.resources.preferences.borrow_mut().push(json!({ "tab_size": 3 }));
    editor.reload_settings();
    assert_eq!(editor.global_settings().usize("tab_size", 0), 3);
    assert_eq!(world.resources.ui_loads.borrow().len(), 1);
}

#[test]
fn loading_packages_rescans_the_folders() {
    let world = World::new();
    world.editor();
    assert_eq!(world.file_index.rebuilds.borrow().len(), 1);
}

#[test]
fn open_package_file_expands_the_packages_variable() {
    let world = World::new().with_file("/packages/User/notes.txt", "hi");
    let mut editor = world.editor();
    editor.open_package_file("${packages}/User/notes.txt", false);
    assert_eq!(editor.active_document().unwrap().text, "hi");
}

#[test]
fn open_package_folder_opens_the_file_manager() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_package_folder("${packages}");
    assert_eq!(*world.opened_folders.borrow(), vec![PathBuf::from("/packages")]);
}

#[test]
fn packages_location_reports_both_folders() {
    let world = World::new();
    let editor = world.editor();
    assert_eq!(editor.packages_location(), (PathBuf::from("/packages"), None));
}
