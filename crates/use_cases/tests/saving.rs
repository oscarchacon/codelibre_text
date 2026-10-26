//! Tests for the saving use cases (`use_cases::saving`).

use std::path::PathBuf;

use serde_json::json;
use test_support::{open, status, World};
use use_cases::editing::TextCommand;
use use_cases::gateways::{PluginCommandKind, PluginMessage};

#[test]
fn save_writes_the_file_with_its_original_line_endings() {
    let world = World::new().with_file("/p/a.txt", "one\r\ntwo");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::DuplicateLine);

    assert!(editor.save_active(false));
    assert_eq!(world.file_text("/p/a.txt"), "one\r\none\r\ntwo");
    assert!(!editor.active_document().unwrap().is_dirty());
    assert!(status(&editor).starts_with("Saved"));
}

#[test]
fn saving_an_untitled_document_asks_for_a_path_and_detects_the_syntax() {
    let world = World::new();
    *world.dialogs.save_path.borrow_mut() = Some(PathBuf::from("/p/script.py"));
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("print(1)".into()));

    assert!(editor.save_active(false));
    assert_eq!(world.dialogs.save_requests.borrow().as_slice(), ["print(1)"]);
    assert_eq!(world.file_text("/p/script.py"), "print(1)");
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.name(), "script.py");
    assert_eq!(doc.syntax, "Python");
}

#[test]
fn cancelling_the_save_dialog_saves_nothing() {
    let world = World::new();
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert("draft".into()));
    assert!(!editor.save_active(false));
    assert!(editor.active_document().unwrap().is_dirty());
}

#[test]
fn save_as_always_asks_for_a_path() {
    let world = World::new().with_file("/p/a.txt", "a");
    *world.dialogs.save_path.borrow_mut() = Some(PathBuf::from("/p/copy.txt"));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    assert!(editor.save_active(true));
    assert_eq!(world.file_text("/p/copy.txt"), "a");
    assert_eq!(editor.active_document().unwrap().name(), "copy.txt");
}

#[test]
fn read_only_documents_cannot_be_saved() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_scratch("Defaults", "{}", "JSON", true);
    assert!(!editor.save_active(false));
    assert_eq!(status(&editor), "This view is read only");
}

#[test]
fn write_errors_are_reported_and_the_document_stays_dirty() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("x".into()));
    *world.disk.fail_writes.borrow_mut() = Some("disk full".into());

    assert!(!editor.save_active(false));
    assert!(status(&editor).contains("disk full"));
    assert!(editor.active_document().unwrap().is_dirty());
}

#[test]
fn trailing_white_space_is_trimmed_on_save_when_enabled() {
    for setting in [json!(true), json!("all")] {
        let world = World::new()
            .with_file("/p/a.txt", "a  \nb\t")
            .with_preferences(json!({ "trim_trailing_white_space_on_save": setting }));
        let mut editor = world.editor();
        open(&mut editor, "/p/a.txt");
        editor.save_active(false);
        assert_eq!(world.file_text("/p/a.txt"), "a\nb");
    }
}

#[test]
fn trailing_white_space_is_kept_by_default() {
    let world = World::new().with_file("/p/a.txt", "a  ");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.save_active(false);
    assert_eq!(world.file_text("/p/a.txt"), "a  ");
}

#[test]
fn a_final_newline_is_added_when_enabled() {
    let world = World::new()
        .with_file("/p/a.txt", "a")
        .with_preferences(json!({ "ensure_newline_at_eof_on_save": true }));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.save_active(false);
    assert_eq!(world.file_text("/p/a.txt"), "a\n");
}

#[test]
fn save_all_saves_modified_files_but_not_untitled_ones() {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("1".into()));
    open(&mut editor, "/p/b.txt");
    editor.new_file();
    editor.run_text_command(TextCommand::Insert("draft".into()));

    editor.save_all();
    assert_eq!(world.file_text("/p/a.txt"), "1a");
    assert_eq!(world.file_text("/p/b.txt"), "b");
    assert!(world.dialogs.save_requests.borrow().is_empty(), "untitled files are skipped");
    assert_eq!(editor.state().active, 2, "the active tab does not change");
}

#[test]
fn saving_a_settings_file_in_packages_reloads_the_settings() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_file(std::path::Path::new("/packages/User/Preferences.sublime-settings"));
    editor.run_text_command(TextCommand::Insert("{}".into()));
    let loads = world.resources.settings_loads.get();
    editor.save_active(false);
    assert_eq!(world.resources.settings_loads.get(), loads + 1);
}

#[test]
fn saving_a_keymap_in_packages_reloads_the_packages() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_file(std::path::Path::new("/packages/User/Default (Windows).sublime-keymap"));
    editor.run_text_command(TextCommand::Insert("[]".into()));
    let loads = world.resources.ui_loads.borrow().len();
    editor.save_active(false);
    assert_eq!(world.resources.ui_loads.borrow().len(), loads + 1);
}

#[test]
fn saving_a_plugin_reloads_only_the_plugins() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_file(std::path::Path::new("/packages/User/hello.py"));
    editor.run_text_command(TextCommand::Insert("import sublime\n".into()));
    let ui_loads = world.resources.ui_loads.borrow().len();
    world.plugins.then([PluginMessage::Loaded { commands: vec![("hello".into(), PluginCommandKind::Text)], errors: vec![] }]);
    editor.save_active(false);
    assert_eq!(world.plugins.loads.borrow().len(), 1);
    assert_eq!(world.resources.ui_loads.borrow().len(), ui_loads);
    assert!(editor.is_plugin_command("hello"));
}

#[test]
fn saving_files_outside_packages_does_not_reload_anything() {
    let world = World::new().with_file("/p/a.sublime-keymap", "[]");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.sublime-keymap");
    let loads = world.resources.ui_loads.borrow().len();
    editor.run_text_command(TextCommand::Insert(" ".into()));
    editor.save_active(false);
    assert_eq!(world.resources.ui_loads.borrow().len(), loads);
}
