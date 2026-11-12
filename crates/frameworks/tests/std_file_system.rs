//! Tests for `gateways::std_file_system` on the real disk (a temporary folder). Moving to the trash
//! is not tested here: it would fill the trash of whoever runs the tests.

use std::fs;

use codelibre_text::gateways::std_file_system::StdFileSystem;
use use_cases::gateways::FileSystemGateway;

#[test]
fn create_dir_makes_the_missing_folders_above_it() {
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("a").join("b").join("c");
    StdFileSystem.create_dir(&deep).unwrap();
    assert!(deep.is_dir());
    StdFileSystem.create_dir(&deep).unwrap();
}

#[test]
fn rename_moves_a_file_or_a_folder_with_what_is_in_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src").join("main.rs"), "fn main() {}").unwrap();
    StdFileSystem.rename(&dir.path().join("src"), &dir.path().join("code")).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("code").join("main.rs")).unwrap(), "fn main() {}");
    StdFileSystem.rename(&dir.path().join("code").join("main.rs"), &dir.path().join("code").join("lib.rs")).unwrap();
    assert!(dir.path().join("code").join("lib.rs").is_file());
    assert!(StdFileSystem.rename(&dir.path().join("missing"), &dir.path().join("x")).is_err());
}
