//! Tests for the project use cases (`use_cases::project`): folders and the file index.

use std::path::{Path, PathBuf};

use test_support::World;
use use_cases::project::matches_pattern;

fn last_rebuild(world: &World) -> (Vec<PathBuf>, Vec<String>, Vec<String>) {
    world.file_index.rebuilds.borrow().last().cloned().expect("a rebuild")
}

#[test]
fn adding_a_folder_indexes_it_with_the_exclude_patterns() {
    let world = World::new().with_dir("/p");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/p"));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/p")]);
    let (folders, folder_excludes, file_excludes) = last_rebuild(&world);
    assert_eq!(folders, vec![PathBuf::from("/p")]);
    assert_eq!(folder_excludes, vec![".git", "target"]);
    assert_eq!(file_excludes, vec!["*.pyc"]);
}

#[test]
fn a_folder_is_only_added_once() {
    let world = World::new().with_dir("/p");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/p"));
    let rebuilds = world.file_index.rebuilds.borrow().len();
    editor.add_folder(PathBuf::from("/p"));
    assert_eq!(editor.state().folders.len(), 1);
    assert_eq!(world.file_index.rebuilds.borrow().len(), rebuilds);
}

#[test]
fn prompt_add_folder_uses_the_folder_dialog() {
    let world = World::new().with_dir("/p");
    let mut editor = world.editor();
    assert_eq!(editor.prompt_add_folder(), None);
    *world.dialogs.folder_to_open.borrow_mut() = Some(PathBuf::from("/p"));
    assert_eq!(editor.prompt_add_folder(), Some(PathBuf::from("/p")));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/p")]);
}

#[test]
fn removing_and_closing_folders_reindexes() {
    let world = World::new().with_dir("/a").with_dir("/b");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/a"));
    editor.add_folder(PathBuf::from("/b"));
    editor.remove_folder(Path::new("/a"));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/b")]);
    assert_eq!(last_rebuild(&world).0, vec![PathBuf::from("/b")]);
    editor.close_all_folders();
    assert!(editor.state().folders.is_empty());
    assert!(last_rebuild(&world).0.is_empty());
}

#[test]
fn poll_reports_a_finished_scan_once() {
    let world = World::new();
    let mut editor = world.editor();
    editor.refresh_file_index();
    assert!(editor.is_indexing_files());
    assert!(editor.poll_file_index());
    assert!(!editor.poll_file_index());
    assert!(!editor.is_indexing_files());
}

#[test]
fn list_folder_puts_folders_first_sorted_and_hides_excluded_entries() {
    let world = World::new()
        .with_file("/p/zeta.txt", "")
        .with_file("/p/Alpha.txt", "")
        .with_file("/p/cache.pyc", "")
        .with_dir("/p/src")
        .with_dir("/p/.git")
        .with_dir("/p/target")
        .with_dir("/p/Docs");
    let editor = world.editor();
    let entries: Vec<(String, bool)> = editor
        .list_folder(Path::new("/p"))
        .into_iter()
        .map(|e| (e.path.file_name().unwrap().to_string_lossy().into_owned(), e.is_dir))
        .collect();
    assert_eq!(
        entries,
        vec![
            ("Docs".into(), true),
            ("src".into(), true),
            ("Alpha.txt".into(), false),
            ("zeta.txt".into(), false),
        ]
    );
}

#[test]
fn exclude_patterns_support_wildcards() {
    assert!(matches_pattern("*.pyc", "a.pyc"));
    assert!(matches_pattern("node_modules", "node_modules"));
    assert!(matches_pattern("file?.txt", "file1.txt"));
    assert!(matches_pattern("*", "anything"));
    assert!(matches_pattern("a*b*c", "aXXbYYc"));
    assert!(!matches_pattern("*.o", "a.obj"));
    assert!(!matches_pattern("file?.txt", "file.txt"));
    assert!(!matches_pattern("node_modules", "node_modules2"));
}
