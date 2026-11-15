//! Tests for File > Open Recent (`use_cases::recent`).

use std::path::PathBuf;

use test_support::{open, World};
use use_cases::session::Session;

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn opened_files_and_added_folders_are_remembered_newest_first() {
    let world = World::new()
        .with_file("/p/a.txt", "a")
        .with_file("/p/b.txt", "b")
        .with_dir("/q");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    // A new file (not on disk yet) is not a recent one.
    open(&mut editor, "/p/new.txt");
    editor.add_folder("/q".into());
    assert_eq!(editor.file_history(), paths(&["/p/b.txt", "/p/a.txt"]));
    assert_eq!(editor.folder_history(), paths(&["/q"]));
}

#[test]
fn open_recent_file_opens_the_file_at_its_index() {
    let world = World::new()
        .with_file("/p/a.txt", "first")
        .with_file("/p/b.txt", "second");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    editor.close_active_document();
    editor.open_recent_file(0);
    assert_eq!(editor.active_document().unwrap().text, "second");
    // Out of range does nothing.
    editor.open_recent_file(7);
    assert_eq!(editor.active_document().unwrap().text, "second");
}

#[test]
fn open_recent_folder_uses_an_empty_window_or_else_a_new_one() {
    let world = World::new().with_dir("/q").with_dir("/r");
    let mut editor = world.editor();
    editor.close_active_document();
    editor.add_folder("/q".into());
    editor.add_folder("/r".into());
    editor.close_all_folders();
    // This window is empty now: the folder goes here.
    editor.open_recent_folder(1);
    assert_eq!(editor.state().folders, paths(&["/q"]));
    assert_eq!(editor.window_ids().len(), 1);
    // It has a folder: the next one opens a new window.
    editor.open_recent_folder(1);
    assert_eq!(editor.window_ids().len(), 2);
    assert_eq!(editor.state().folders, paths(&["/r"]));
}

#[test]
fn a_recent_folder_that_is_gone_says_so() {
    let world = World::new().with_session(Session {
        folder_history: paths(&["/gone"]),
        ..Default::default()
    });
    let mut editor = world.editor();
    editor.open_recent_folder(0);
    assert!(editor.state().folders.is_empty());
    assert_eq!(editor.window_ids().len(), 1);
    assert!(test_support::status(&editor).contains("/gone"));
}

#[test]
fn clear_recent_files_empties_both_lists() {
    let world = World::new().with_file("/p/a.txt", "a").with_dir("/q");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.add_folder("/q".into());
    editor.clear_recent_files();
    assert!(editor.file_history().is_empty());
    assert!(editor.folder_history().is_empty());
}

#[test]
fn the_session_keeps_the_lists() {
    let world = World::new()
        .with_file("/p/a.txt", "a")
        .with_dir("/q")
        .with_dir("/s");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.add_folder("/q".into());
    editor.save_session(Vec::new());
    let saved = world.session.borrow().clone().unwrap();
    assert_eq!(saved.file_history, paths(&["/p/a.txt"]));
    assert_eq!(saved.folder_history, paths(&["/q"]));

    // Restoring a window's folders does not reorder the saved history.
    let world = World::new()
        .with_dir("/q")
        .with_dir("/s")
        .with_session(Session {
            folders: paths(&["/s"]),
            file_history: paths(&["/p/a.txt"]),
            folder_history: paths(&["/q", "/s"]),
            ..Default::default()
        });
    let editor = world.editor();
    assert_eq!(editor.file_history(), paths(&["/p/a.txt"]));
    assert_eq!(editor.folder_history(), paths(&["/q", "/s"]));
}
