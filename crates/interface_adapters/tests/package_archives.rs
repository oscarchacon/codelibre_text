//! Tests for `gateways::package_archives` and how unpacked packages join the others.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use interface_adapters::gateways::package_archives::unpack_all;
use interface_adapters::gateways::package_files::{find_resources, ignored_packages, resource_path};
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use use_cases::gateways::{ArchiveFolder, PackagePaths, ResourceGateway};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn write_zip(path: &Path, files: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, contents) in files {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// Ours (`Installed Packages`) and Sublime's archives, in a temporary folder.
fn paths(dir: &Path) -> PackagePaths {
    let folder = |name: &str, sublime: bool| ArchiveFolder {
        archives: dir.join(name).join("Installed Packages"),
        unpacked: dir.join("Cache").join(name),
        sublime,
        shipped: false,
    };
    PackagePaths {
        packages: dir.join("Packages"),
        sublime_packages: Some(dir.join("Sublime").join("Packages")),
        archive_folders: vec![folder("Sublime", true), folder("Own", false)],
    }
}

fn names(found: &[PathBuf]) -> Vec<String> {
    found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

fn unpacked(dir: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    let paths = paths(dir);
    unpack_all(&paths, true, &mut errors);
    assert_eq!(errors, Vec::<String>::new());
    names(&find_resources(&paths, true, |n| n.ends_with(".txt")))
}

#[test]
fn a_zipped_package_is_found_like_an_unpacked_one() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Own/Installed Packages/Pkg.sublime-package"), &[("a.txt", "A"), ("sub/b.txt", "B")]);
    assert_eq!(unpacked(dir.path()), ["a.txt", "b.txt"]);
    let path = resource_path(&paths(dir.path()), true, "Packages/Pkg/sub/b.txt").unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "B");
}

#[test]
fn a_loose_file_overrides_the_zipped_one_and_the_rest_of_the_zip_stays() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Own/Installed Packages/Pkg.sublime-package"), &[("a.txt", "zipped"), ("b.txt", "zipped")]);
    fs::create_dir_all(dir.path().join("Packages/Pkg")).unwrap();
    fs::write(dir.path().join("Packages/Pkg/a.txt"), "loose").unwrap();
    unpacked(dir.path());

    let paths = paths(dir.path());
    let found = find_resources(&paths, true, |n| n.ends_with(".txt"));
    assert_eq!(names(&found), ["a.txt", "b.txt"], "each file once");
    assert_eq!(fs::read_to_string(&found[0]).unwrap(), "loose");
    assert_eq!(fs::read_to_string(&found[1]).unwrap(), "zipped");
    assert_eq!(fs::read_to_string(resource_path(&paths, true, "Packages/Pkg/a.txt").unwrap()).unwrap(), "loose");
}

#[test]
fn our_archives_win_over_sublime_s() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Sublime/Installed Packages/Pkg.sublime-package"), &[("a.txt", "theirs")]);
    write_zip(&dir.path().join("Own/Installed Packages/Pkg.sublime-package"), &[("a.txt", "ours")]);
    unpacked(dir.path());
    let found = find_resources(&paths(dir.path()), true, |n| n.ends_with(".txt"));
    assert_eq!(found.len(), 1);
    assert_eq!(fs::read_to_string(&found[0]).unwrap(), "ours");
}

#[test]
fn sublime_s_archives_follow_import_sublime_packages() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Sublime/Installed Packages/Pkg.sublime-package"), &[("a.txt", "x")]);
    let paths = paths(dir.path());
    let mut errors = Vec::new();
    unpack_all(&paths, false, &mut errors);
    assert!(!dir.path().join("Cache/Sublime/Pkg").exists());
    unpack_all(&paths, true, &mut errors);
    assert_eq!(find_resources(&paths, false, |n| n.ends_with(".txt")).len(), 0);
    assert_eq!(find_resources(&paths, true, |n| n.ends_with(".txt")).len(), 1);
}

#[test]
fn sublime_s_default_and_package_control_are_not_imported_but_ours_are() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["Default", "Package Control"] {
        write_zip(&dir.path().join(format!("Sublime/Installed Packages/{name}.sublime-package")), &[("a.txt", "x")]);
    }
    assert!(unpacked(dir.path()).is_empty());
    write_zip(&dir.path().join("Own/Installed Packages/Package Control.sublime-package"), &[("a.txt", "x")]);
    assert_eq!(unpacked(dir.path()), ["a.txt"]);
}

#[test]
fn an_archive_is_unpacked_again_only_when_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("Own/Installed Packages/Pkg.sublime-package");
    write_zip(&archive, &[("a.txt", "one")]);
    unpacked(dir.path());
    let file = dir.path().join("Cache/Own/Pkg/a.txt");
    fs::write(&file, "touched").unwrap();
    unpacked(dir.path());
    assert_eq!(fs::read_to_string(&file).unwrap(), "touched", "the same archive is not unpacked again");

    write_zip(&archive, &[("b.txt", "two, longer")]);
    assert_eq!(unpacked(dir.path()), ["b.txt"], "nothing of the old version stays");
}

#[test]
fn unpacking_says_whether_it_changed_something_and_leaves_no_staging_folder() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("Own/Installed Packages/Pkg.sublime-package");
    write_zip(&archive, &[("a.txt", "one")]);
    let paths = paths(dir.path());
    let unpack = || unpack_all(&paths, true, &mut Vec::new());
    assert!(unpack(), "unpacked");
    assert!(!unpack(), "the same archive");
    write_zip(&archive, &[("a.txt", "two, longer")]);
    assert!(unpack(), "another version");
    fs::remove_file(&archive).unwrap();
    assert!(unpack(), "removed");
    assert!(!unpack());
    let cache: Vec<_> = fs::read_dir(dir.path().join("Cache")).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(cache, ["Own"], "packages are unpacked next to their folder and moved there whole");
}

#[test]
fn the_gateway_unpacks_in_a_thread_when_it_has_a_waker() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Own/Installed Packages/Pkg.sublime-package"), &[("a.txt", "A")]);
    let gateway = SublimeResourceGateway::new(paths(dir.path()));
    assert!(!gateway.start_unpacking(true), "without a waker the caller unpacks");
    assert_eq!(gateway.poll_unpacking(), None);

    let (woken, wakes) = channel();
    let woken = Mutex::new(woken);
    let gateway = gateway.with_background_unpacking(Arc::new(move || woken.lock().unwrap().send(()).unwrap()));
    assert!(gateway.start_unpacking(true));
    wakes.recv_timeout(Duration::from_secs(30)).expect("the window is woken when it ends");
    assert_eq!(gateway.poll_unpacking(), Some(true), "a package was unpacked");
    assert_eq!(gateway.poll_unpacking(), None, "told once");
    assert_eq!(fs::read_to_string(dir.path().join("Cache/Own/Pkg/a.txt")).unwrap(), "A");

    assert!(gateway.start_unpacking(true));
    wakes.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(gateway.poll_unpacking(), Some(false), "nothing changed");
}

#[test]
fn an_archive_that_is_gone_takes_its_package_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("Own/Installed Packages/Pkg.sublime-package");
    write_zip(&archive, &[("a.txt", "x")]);
    unpacked(dir.path());
    fs::remove_file(&archive).unwrap();
    assert!(unpacked(dir.path()).is_empty());
    assert!(!dir.path().join("Cache/Own/Pkg").exists());
}

#[test]
fn a_broken_archive_is_reported_and_the_others_load() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Own/Installed Packages");
    write_zip(&folder.join("Good.sublime-package"), &[("a.txt", "x")]);
    fs::write(folder.join("Bad.sublime-package"), "not a zip").unwrap();
    let paths = paths(dir.path());
    let mut errors = Vec::new();
    unpack_all(&paths, true, &mut errors);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Bad.sublime-package"), "{errors:?}");
    assert_eq!(names(&find_resources(&paths, true, |n| n.ends_with(".txt"))), ["a.txt"]);
}

#[test]
fn names_that_climb_out_of_the_package_are_not_written() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Own/Installed Packages/Pkg.sublime-package"), &[("../../evil.txt", "x"), ("a.txt", "y")]);
    assert_eq!(unpacked(dir.path()), ["a.txt"]);
    assert!(!dir.path().join("Cache/evil.txt").exists() && !dir.path().join("evil.txt").exists());
}

#[test]
fn ignored_packages_are_not_found_and_vintage_is_ignored_by_default() {
    let dir = tempfile::tempdir().unwrap();
    write_zip(&dir.path().join("Own/Installed Packages/Vintage.sublime-package"), &[("v.txt", "x")]);
    write_zip(&dir.path().join("Own/Installed Packages/Other.sublime-package"), &[("o.txt", "x")]);
    assert_eq!(unpacked(dir.path()), ["o.txt"]);
    assert_eq!(ignored_packages(&paths(dir.path()), true), ["Vintage"]);

    fs::create_dir_all(dir.path().join("Packages/User")).unwrap();
    fs::write(dir.path().join("Packages/User/Preferences.sublime-settings"), r#"{ "ignored_packages": ["Other"] }"#).unwrap();
    assert_eq!(unpacked(dir.path()), ["v.txt"], "the user's list replaces the default");
}

#[test]
fn shipped_packages_leave_out_their_tests_but_keep_their_syntaxes() {
    let dir = tempfile::tempdir().unwrap();
    let mut paths = paths(dir.path());
    paths.archive_folders[0].shipped = true;
    let files = [("P.sublime-syntax", "x"), ("tests/t.py", "x"), ("P.sublime-build", "{}")];
    write_zip(&dir.path().join("Sublime/Installed Packages/P.sublime-package"), &files);
    write_zip(&dir.path().join("Own/Installed Packages/Q.sublime-package"), &files);
    unpack_all(&paths, true, &mut Vec::new());
    assert_eq!(
        names(&find_resources(&paths, true, |n| n.starts_with('P'))),
        ["P.sublime-build", "P.sublime-syntax", "P.sublime-build", "P.sublime-syntax"]
    );
    assert_eq!(names(&find_resources(&paths, true, |n| n.ends_with(".py"))), ["t.py"], "only the user's package keeps its tests");
}
