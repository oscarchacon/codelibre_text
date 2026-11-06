//! Tests for the project use cases (`use_cases::project`): folders, `.sublime-project` files and
//! the file index.

use std::path::{Path, PathBuf};

use serde_json::json;
use test_support::{open, World};
use use_cases::builds::BuildPlan;
use use_cases::project::matches_pattern;

const PROJECT: &str = r#"{
    "name": "Demo",
    "folders": [{ "path": "." }, { "path": "src" }],
    "build_systems": [
        { "name": "Echo", "shell_cmd": "echo $project_name in $project_path" }
    ]
}"#;

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
    assert_eq!(editor.prompt_add_folder(None), None);
    *world.dialogs.folder_to_open.borrow_mut() = Some(PathBuf::from("/p"));
    assert_eq!(editor.prompt_add_folder(None), Some(PathBuf::from("/p")));
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

#[test]
fn a_project_opens_its_folders_has_variables_and_adds_builds() {
    let world = World::new()
        .with_dir("/p")
        .with_dir("/p/src")
        .with_file("/p/a.txt", "x\n")
        .with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    assert!(editor.open_project(Path::new("/p/demo.sublime-project")));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/p"), PathBuf::from("/p/src")]);
    assert_eq!(editor.project_file(), Some(Path::new("/p/demo.sublime-project")));
    assert_eq!(editor.workspace_file(), Some(PathBuf::from("/p/demo.sublime-workspace")));

    let variables = editor.project_variables();
    assert_eq!(variables["project"], "/p/demo.sublime-project");
    assert_eq!(variables["project_path"], "/p");
    assert_eq!(variables["project_name"], "Demo", "the name key wins");
    assert_eq!(variables["project_base_name"], "demo");
    assert_eq!(variables["project_extension"], "sublime-project");

    let names: Vec<&str> = editor.menu_build_systems().iter().map(|b| b.caption.as_str()).collect();
    assert_eq!(names, ["Echo"]);
}

#[test]
fn a_project_build_runs_with_the_project_variables() {
    let world = World::new().with_file("/p/a.txt", "x\n").with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    open(&mut editor, "/p/demo.sublime-project");
    open(&mut editor, "/p/a.txt");
    let BuildPlan::Run(command) = editor.plan_build("", "") else {
        panic!("the project build applies to every file");
    };
    assert_eq!(command.args["shell_cmd"], "echo Demo in /p");
}

#[test]
fn opening_a_project_file_opens_the_project_not_a_tab() {
    let world = World::new().with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    assert!(editor.open_file(Path::new("/p/demo.sublime-project")));
    assert_eq!(editor.project_file(), Some(Path::new("/p/demo.sublime-project")));
    assert!(editor.state().documents.iter().all(|d| d.path.is_none()), "no tab for the project file");
}

#[test]
fn project_data_reads_the_file_and_set_project_data_writes_it() {
    let world = World::new().with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    editor.open_project(Path::new("/p/demo.sublime-project"));
    assert_eq!(editor.project_data()["name"], "Demo");

    let mut data = editor.project_data();
    data["name"] = json!("Changed");
    data["folders"] = json!([{ "path": "/p/src" }]);
    assert!(editor.set_project_data(data));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/p/src")]);
    let written: serde_json::Value = serde_json::from_slice(&world.disk.bytes("/p/demo.sublime-project").unwrap()).unwrap();
    assert_eq!(written["name"], "Changed");
    assert_eq!(written["folders"], json!([{ "path": "src" }]));
}

#[test]
fn a_window_without_a_project_reports_its_folders_as_project_data() {
    let world = World::new().with_dir("/p");
    let mut editor = world.editor();
    editor.add_folder(PathBuf::from("/p"));
    assert_eq!(editor.project_data(), json!({ "folders": [{ "path": "/p" }] }));
    assert_eq!(editor.project_variables().get("project").map(String::as_str), Some(""));
    assert!(editor.workspace_file().is_none());
}

#[test]
fn closing_a_project_closes_its_folders() {
    let world = World::new().with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    editor.open_project(Path::new("/p/demo.sublime-project"));
    editor.close_project();
    assert!(editor.state().folders.is_empty());
    assert!(editor.project_file().is_none());
}

#[test]
fn adding_a_folder_keeps_the_project_data_in_sync() {
    let world = World::new().with_dir("/p").with_dir("/q").with_file("/p/demo.sublime-project", PROJECT);
    let mut editor = world.editor();
    editor.open_project(Path::new("/p/demo.sublime-project"));
    editor.add_folder(PathBuf::from("/q"));
    let folders = editor.project_data()["folders"].clone();
    assert!(folders.as_array().unwrap().iter().any(|f| f["path"] == "/q"), "{folders}");
}

