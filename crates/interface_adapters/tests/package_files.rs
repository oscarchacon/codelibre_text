//! Tests for `gateways::package_files` (finding resources in package folders).

use std::fs;
use std::path::{Path, PathBuf};

use interface_adapters::gateways::package_files::{find_resources, is_in_default_package};
use use_cases::gateways::PackagePaths;

fn write(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "{}").unwrap();
}

/// Package and file name of each found resource, e.g. `"User/Preferences.sublime-settings"`.
fn short(found: &[PathBuf]) -> Vec<String> {
    found
        .iter()
        .map(|p| {
            let package = p.parent().unwrap().file_name().unwrap().to_string_lossy();
            format!("{package}/{}", p.file_name().unwrap().to_string_lossy())
        })
        .collect()
}

#[test]
fn packages_load_default_first_then_alphabetically_then_user_last() {
    let dir = tempfile::tempdir().unwrap();
    let packages = dir.path().join("Packages");
    for package in ["User", "zeta", "Default", "Alpha"] {
        write(&packages.join(package).join("Preferences.sublime-settings"));
    }
    let paths = PackagePaths { packages, sublime_packages: None };
    let found = find_resources(&paths, false, |n| n == "Preferences.sublime-settings");
    assert_eq!(
        short(&found),
        vec![
            "Default/Preferences.sublime-settings",
            "Alpha/Preferences.sublime-settings",
            "zeta/Preferences.sublime-settings",
            "User/Preferences.sublime-settings",
        ]
    );
}

#[test]
fn only_matching_files_are_returned_including_subfolders() {
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("Packages").join("Rust");
    write(&package.join("Rust.sublime-syntax"));
    write(&package.join("syntax").join("Cargo.sublime-syntax"));
    write(&package.join("README.md"));
    let paths = PackagePaths { packages: dir.path().join("Packages"), sublime_packages: None };
    let found = find_resources(&paths, false, |n| n.ends_with(".sublime-syntax"));
    assert_eq!(found.len(), 2);
}

#[test]
fn very_deep_files_are_not_searched() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("Packages/P/a/b/c/d/e/f");
    write(&deep.join("Deep.sublime-syntax"));
    write(&dir.path().join("Packages/P/a/b/Shallow.sublime-syntax"));
    let paths = PackagePaths { packages: dir.path().join("Packages"), sublime_packages: None };
    assert_eq!(short(&find_resources(&paths, false, |n| n.ends_with(".sublime-syntax"))), vec!["b/Shallow.sublime-syntax"]);
}

#[test]
fn sublime_text_packages_are_only_read_when_included() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("Sublime/Packages/Theme/Mariana.sublime-color-scheme"));
    write(&dir.path().join("Packages/User/Mine.sublime-color-scheme"));
    let paths = PackagePaths {
        packages: dir.path().join("Packages"),
        sublime_packages: Some(dir.path().join("Sublime/Packages")),
    };
    let matches = |n: &str| n.ends_with(".sublime-color-scheme");
    assert_eq!(short(&find_resources(&paths, false, matches)), vec!["User/Mine.sublime-color-scheme"]);
    assert_eq!(
        short(&find_resources(&paths, true, matches)),
        vec!["Theme/Mariana.sublime-color-scheme", "User/Mine.sublime-color-scheme"]
    );
}

#[test]
fn missing_package_folders_find_nothing() {
    let paths = PackagePaths { packages: PathBuf::from("/does/not/exist"), sublime_packages: None };
    assert!(find_resources(&paths, true, |_| true).is_empty());
}

#[test]
fn files_of_the_default_package_are_recognized() {
    assert!(is_in_default_package(Path::new("/Packages/Default/Main.sublime-menu")));
    assert!(!is_in_default_package(Path::new("/Packages/User/Main.sublime-menu")));
    assert!(!is_in_default_package(Path::new("/Packages/Default/sub/Main.sublime-menu")));
}
