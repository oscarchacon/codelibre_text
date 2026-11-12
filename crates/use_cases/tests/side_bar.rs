//! Tests for the side bar's file and folder commands (`use_cases::side_bar`): `new_file_at`,
//! `new_folder`, `rename_path`, `delete_file` and `delete_folder`, as Sublime's `Default/side_bar.py`.

use std::path::{Path, PathBuf};

use entities::command::Command;
use serde_json::json;
use test_support::World;
use use_cases::gateways::MessageAnswer;
use use_cases::Editor;

fn editor_with_project(world: &World) -> Editor {
    world.disk.put("/p/src/main.rs", "fn main() {}\n");
    world.disk.put("/p/notes.md", "# notes\n");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/p"));
    editor
}

fn command(name: &str, args: serde_json::Value) -> Command {
    Command::with_args(name, args)
}

fn no_commands() -> impl FnMut(&mut Editor, &Command) {
    |_: &mut Editor, _: &Command| {}
}

#[test]
fn each_command_shows_only_on_what_it_acts_on() {
    let world = World::new();
    let editor = editor_with_project(&world);
    let shows = |name: &str, args| editor.side_bar_command_visible(&command(name, args));
    // On a folder: `dirs` and `paths` hold it, `files` is empty.
    let folder = json!({ "dirs": ["/p/src"], "paths": ["/p/src"], "files": [] });
    assert!(shows("new_file_at", folder.clone()));
    assert!(shows("new_folder", folder.clone()));
    assert!(shows("rename_path", folder.clone()));
    assert!(shows("delete_folder", folder.clone()));
    assert!(!shows("delete_file", folder));
    // On a file.
    let file = json!({ "dirs": [], "paths": ["/p/notes.md"], "files": ["/p/notes.md"] });
    assert!(!shows("new_file_at", file.clone()));
    assert!(!shows("new_folder", file.clone()));
    assert!(!shows("delete_folder", file.clone()));
    assert!(shows("rename_path", file.clone()));
    assert!(shows("delete_file", file));
    assert!(shows("open_containing_folder", json!({ "paths": [] })), "other commands always show");
}

#[test]
fn new_file_at_starts_its_first_save_in_the_folder() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.new_file_at(&command("new_file_at", json!({ "dirs": ["/p/src"] })));
    assert!(editor.active_document().unwrap().path.is_none());
    assert_eq!(editor.view_settings().str("default_dir"), Some("/p/src"));
    *world.dialogs.save_path.borrow_mut() = Some(PathBuf::from("/p/src/lib.rs"));
    editor.save_active(false);
    assert_eq!(world.dialogs.last_save_dir.borrow().as_deref(), Some(Path::new("/p/src")));
}

#[test]
fn new_folder_asks_for_a_name_and_makes_the_folder() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    let revision = editor.state().folders_revision;
    editor.new_folder(&command("new_folder", json!({ "dirs": ["/p/src"] })), &mut no_commands());
    let panel = editor.plugin_input_panel().expect("the input panel opened");
    assert_eq!(panel.caption, "Folder Name:");
    assert_eq!(panel.document.text, "");
    editor.change_plugin_input_panel("util/deep", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert!(editor.plugin_input_panel().is_none());
    assert!(world.disk.is_dir("/p/src/util/deep"));
    assert!(editor.state().folders_revision > revision, "the side bar reads its folders again");
}

#[test]
fn a_cancelled_or_empty_name_does_nothing() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    let revision = editor.state().folders_revision;
    editor.new_folder(&command("new_folder", json!({ "dirs": ["/p"] })), &mut no_commands());
    editor.change_plugin_input_panel("gone", &mut no_commands());
    editor.finish_plugin_input_panel(false, &mut no_commands());
    editor.new_folder(&command("new_folder", json!({ "dirs": ["/p"] })), &mut no_commands());
    editor.change_plugin_input_panel("   ", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert!(!world.disk.is_dir("/p/gone"));
    assert_eq!(editor.state().folders_revision, revision);
}

#[test]
fn rename_path_selects_the_name_without_its_extension() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.rename_path(&command("rename_path", json!({ "paths": ["/p/notes.md"] })), &mut no_commands());
    let panel = editor.plugin_input_panel().unwrap();
    assert_eq!(panel.caption, "New Name:");
    assert_eq!(panel.document.text, "notes.md");
    let selection = panel.document.selection();
    assert_eq!((selection.start(), selection.end()), (0, 5));
    // A folder's whole name.
    editor.rename_path(&command("rename_path", json!({ "paths": ["/p/src"] })), &mut no_commands());
    let selection = editor.plugin_input_panel().unwrap().document.selection();
    assert_eq!((selection.start(), selection.end()), (0, 3));
}

#[test]
fn renaming_a_file_moves_its_open_view_with_it() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.open_file(Path::new("/p/notes.md"));
    editor.rename_path(&command("rename_path", json!({ "paths": ["/p/notes.md"] })), &mut no_commands());
    editor.change_plugin_input_panel("todo.md", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert!(world.disk.bytes("/p/notes.md").is_none());
    assert_eq!(world.disk.bytes("/p/todo.md").unwrap(), b"# notes\n");
    assert_eq!(editor.active_document().unwrap().path.as_deref(), Some(Path::new("/p/todo.md")));
}

#[test]
fn renaming_a_folder_moves_the_files_open_inside_it_and_a_top_folder() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.open_file(Path::new("/p/src/main.rs"));
    editor.rename_path(&command("rename_path", json!({ "paths": ["/p/src"] })), &mut no_commands());
    editor.change_plugin_input_panel("code", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert_eq!(world.disk.bytes("/p/code/main.rs").unwrap(), b"fn main() {}\n");
    assert_eq!(editor.active_document().unwrap().path.as_deref(), Some(Path::new("/p/code/main.rs")));

    editor.rename_path(&command("rename_path", json!({ "paths": ["/p"] })), &mut no_commands());
    editor.change_plugin_input_panel("q", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert_eq!(editor.state().folders, vec![PathBuf::from("/q")]);
    assert_eq!(editor.active_document().unwrap().path.as_deref(), Some(Path::new("/q/code/main.rs")));
}

#[test]
fn renaming_onto_an_existing_name_is_refused() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    world.disk.put("/p/todo.md", "keep me");
    editor.rename_path(&command("rename_path", json!({ "paths": ["/p/notes.md"] })), &mut no_commands());
    editor.change_plugin_input_panel("todo.md", &mut no_commands());
    editor.finish_plugin_input_panel(true, &mut no_commands());
    assert_eq!(world.disk.bytes("/p/todo.md").unwrap(), b"keep me");
    assert!(world.disk.bytes("/p/notes.md").is_some());
    assert!(editor.state().status.as_ref().unwrap().text.contains("already exists"));
}

#[test]
fn delete_file_asks_then_moves_it_to_the_trash_and_closes_its_view() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.open_file(Path::new("/p/notes.md"));
    let delete = command("delete_file", json!({ "files": ["/p/notes.md"] }));
    // Cancelled: nothing happens.
    editor.delete_file(&delete);
    assert_eq!(world.dialogs.messages.borrow().last().unwrap().text, "Delete File /p/notes.md?");
    assert!(world.disk.bytes("/p/notes.md").is_some());
    world.dialogs.message_answers.borrow_mut().push_back(MessageAnswer::Ok);
    editor.delete_file(&delete);
    assert_eq!(*world.disk.trashed.borrow(), vec![PathBuf::from("/p/notes.md")]);
    assert!(world.disk.bytes("/p/notes.md").is_none());
    assert!(editor.state().documents.iter().all(|d| d.path.as_deref() != Some(Path::new("/p/notes.md"))));
}

#[test]
fn delete_file_without_prompt_keeps_a_view_with_unsaved_changes() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    editor.open_file(Path::new("/p/notes.md"));
    editor.run_text_command(use_cases::editing::TextCommand::Insert("x".into()));
    let asked = world.dialogs.messages.borrow().len();
    editor.delete_file(&command("delete_file", json!({ "files": ["/p/notes.md", "/p/src/main.rs"], "prompt": false })));
    assert_eq!(world.dialogs.messages.borrow().len(), asked, "no question with prompt false");
    assert_eq!(world.disk.trashed.borrow().len(), 2);
    let kept = editor.state().documents.iter().find(|d| d.path.as_deref() == Some(Path::new("/p/notes.md")));
    assert!(kept.is_some_and(|d| d.is_dirty()), "the unsaved changes are still there");
}

#[test]
fn delete_folder_asks_once_for_several() {
    let world = World::new();
    let mut editor = editor_with_project(&world);
    world.disk.put("/p/docs/a.md", "a");
    world.dialogs.message_answers.borrow_mut().push_back(MessageAnswer::Ok);
    editor.delete_folder(&command("delete_folder", json!({ "dirs": ["/p/src", "/p/docs"] })));
    assert_eq!(world.dialogs.messages.borrow().len(), 1);
    assert_eq!(world.dialogs.messages.borrow()[0].text, "Delete 2 Folders?");
    assert_eq!(*world.disk.trashed.borrow(), vec![PathBuf::from("/p/src"), PathBuf::from("/p/docs")]);
    assert!(world.disk.bytes("/p/src/main.rs").is_none() && world.disk.bytes("/p/docs/a.md").is_none());
}
