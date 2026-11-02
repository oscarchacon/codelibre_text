//! Tests for the `Editor` itself: startup, status messages, settings and key resolution.

use std::path::PathBuf;

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo, KeyContext, KeyResult};
use serde_json::Value;
use test_support::{status, World};

struct NoContext;

impl KeyContext for NoContext {
    fn query(&self, _: &str, _: &str, _: &Value) -> Option<bool> {
        None
    }
}

#[test]
fn starting_without_paths_shows_one_empty_tab() {
    let world = World::new();
    let editor = world.editor();
    assert_eq!(editor.state().documents.len(), 1);
    assert_eq!(editor.active_document().unwrap().name(), "untitled");
    assert_eq!(world.resources.settings_loads.get(), 1);
}

#[test]
fn starting_with_files_and_folders_opens_them() {
    let world = World::new().with_dir("/proj").with_file("/proj/a.rs", "fn main() {}");
    let editor = world.editor_opening(&["/proj", "/proj/a.rs"]);
    assert_eq!(editor.state().folders, vec![PathBuf::from("/proj")]);
    assert_eq!(editor.state().documents.len(), 1);
    assert_eq!(editor.active_document().unwrap().syntax, "Rust");
}

#[test]
fn status_messages_can_be_shown_and_cleared() {
    let world = World::new();
    let mut editor = world.editor();
    editor.notify("hello");
    assert_eq!(status(&editor), "hello");
    editor.clear_status();
    assert!(editor.state().status.is_none());
}

#[test]
fn key_presses_are_resolved_through_the_loaded_keymap() {
    let world = World::new().with_binding(Binding {
        keys: vec![KeyCombo::parse("ctrl+s").unwrap()],
        command: Command::new("save"),
        context: vec![],
    });
    let mut editor = world.editor();
    match editor.press_key(&[KeyCombo::parse("ctrl+s").unwrap()], &NoContext) {
        KeyResult::Command(command) => assert_eq!(command.name, "save"),
        _ => panic!("ctrl+s should be bound"),
    }
    assert!(matches!(editor.press_key(&[KeyCombo::parse("ctrl+q").unwrap()], &NoContext), KeyResult::Unbound));
}

#[test]
fn without_documents_view_settings_are_the_global_ones() {
    let world = World::new();
    let mut editor = world.editor();
    editor.close_all_documents();
    assert!(editor.active_document().is_none());
    assert_eq!(editor.view_settings().usize("tab_size", 0), 4);
}
