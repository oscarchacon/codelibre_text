//! Tests for the gateways that touch the operating system: the real file system, the folder
//! index and the platform directories. (Native dialogs, the clipboard and the file manager need
//! a desktop session and are checked by running the app.)

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use codelibre_text::gateways::app_directories::{package_paths, session_file};
use codelibre_text::gateways::folder_index::FolderIndex;
use codelibre_text::gateways::std_file_system::StdFileSystem;
use use_cases::gateways::{FileIndexGateway, FileSystemGateway, PackagePaths};

// ----- file system ---------------------------------------------------------------------------------

#[test]
fn write_creates_folders_and_read_returns_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("file.txt");
    StdFileSystem.write(&path, b"hello").unwrap();
    assert_eq!(StdFileSystem.read(&path).unwrap(), b"hello");
    assert!(StdFileSystem.exists(&path));
    assert!(!StdFileSystem.is_dir(&path));
    assert!(StdFileSystem.is_dir(path.parent().unwrap()));
    assert!(StdFileSystem.modified(&path).is_some());
}

#[test]
fn missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.txt");
    assert!(StdFileSystem.read(&missing).is_err());
    assert!(!StdFileSystem.exists(&missing));
    assert!(StdFileSystem.modified(&missing).is_none());
    assert!(StdFileSystem.list_dir(&missing).is_empty());
    assert_eq!(StdFileSystem.canonicalize(&missing), missing, "unchanged when it cannot be resolved");
}

#[test]
fn list_dir_reports_files_and_folders() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let mut entries: Vec<(String, bool)> = StdFileSystem
        .list_dir(dir.path())
        .into_iter()
        .map(|e| (e.path.file_name().unwrap().to_string_lossy().into_owned(), e.is_dir))
        .collect();
    entries.sort();
    assert_eq!(entries, vec![("a.txt".into(), false), ("src".into(), true)]);
}

#[test]
fn canonicalize_resolves_dot_dot_without_the_windows_verbatim_prefix() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    let canonical = StdFileSystem.canonicalize(&dir.path().join("src").join(".."));
    assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
    assert_eq!(canonical, StdFileSystem.canonicalize(dir.path()));
}

// ----- folder index --------------------------------------------------------------------------------

fn wait_for_scan(index: &mut FolderIndex) {
    let start = Instant::now();
    while !index.poll() {
        assert!(start.elapsed() < Duration::from_secs(10), "the scan did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn names(files: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn the_index_lists_files_respecting_excludes_and_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    fs::write(root.join("src/main.rs"), "").unwrap();
    fs::write(root.join("README.md"), "").unwrap();
    fs::write(root.join("debug.log"), "").unwrap();
    fs::write(root.join("cache.pyc"), "").unwrap();
    fs::write(root.join("target/out.bin"), "").unwrap();
    fs::write(root.join(".git/HEAD"), "").unwrap();

    let mut index = FolderIndex::default();
    assert!(!index.is_indexing());
    index.rebuild(vec![root.to_path_buf()], vec![".git".into(), "target".into()], vec!["*.pyc".into()]);
    assert!(index.is_indexing());
    wait_for_scan(&mut index);

    assert!(!index.is_indexing());
    assert_eq!(names(index.files()), vec![".gitignore", "README.md", "main.rs"]);
    assert!(!index.poll(), "a finished scan is reported once");
}

#[test]
fn several_folders_are_indexed_together() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    fs::write(a.path().join("one.txt"), "").unwrap();
    fs::write(b.path().join("two.txt"), "").unwrap();
    let mut index = FolderIndex::default();
    index.rebuild(vec![a.path().to_path_buf(), b.path().to_path_buf()], vec![], vec![]);
    wait_for_scan(&mut index);
    assert_eq!(names(index.files()), vec!["one.txt", "two.txt"]);
}

#[test]
fn rebuilding_with_no_folders_empties_the_index() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.txt"), "").unwrap();
    let mut index = FolderIndex::default();
    index.rebuild(vec![dir.path().to_path_buf()], vec![], vec![]);
    wait_for_scan(&mut index);
    index.rebuild(Vec::new(), vec![], vec![]);
    wait_for_scan(&mut index);
    assert!(index.files().is_empty());
}

// ----- platform directories ------------------------------------------------------------------------

#[test]
fn the_session_lives_next_to_the_packages_folder() {
    let paths = PackagePaths { packages: PathBuf::from("/data/Packages"), sublime_packages: None };
    assert_eq!(session_file(&paths), Path::new("/data").join("Local").join("Session.sublime_session"));
}

#[test]
fn packages_live_in_a_folder_named_packages() {
    let paths = package_paths();
    assert_eq!(paths.packages.file_name().unwrap(), "Packages");
    if let Some(sublime) = paths.sublime_packages {
        assert!(sublime.is_dir(), "only reported when Sublime Text is installed");
    }
}
