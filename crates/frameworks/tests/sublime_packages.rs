//! Checks against the packages of the real Sublime Text on this machine; skipped without one.
//! Run with `cargo test -p codelibre_text --test sublime_packages -- --ignored --nocapture`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use codelibre_text::gateways::python_plugin_host::{find_interpreter, PythonPluginHost};
use codelibre_text::gateways::{
    child_processes::ChildProcesses, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell,
    std_file_system::StdFileSystem, syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard,
};
use codelibre_text::syntax_engine::SyntaxEngine;
use interface_adapters::gateways::package_archives::unpack_all;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use use_cases::gateways::{ArchiveFolder, PackagePaths, ResourceGateway};
use use_cases::{Editor, Gateways};

fn install_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("ProgramFiles")?).join("Sublime Text").join("Packages");
    dir.is_dir().then_some(dir)
}

fn paths(dir: &Path, archives: PathBuf) -> PackagePaths {
    PackagePaths {
        packages: dir.join("Packages"),
        sublime_packages: None,
        archive_folders: vec![ArchiveFolder { archives, unpacked: dir.join("Cache"), sublime: true, shipped: true }],
    }
}

#[test]
#[ignore = "reads the Sublime Text installed here"]
fn the_packages_sublime_ships_load() {
    let Some(archives) = install_dir() else {
        eprintln!("skipped: no Sublime Text here");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path(), archives);
    let mut errors = Vec::new();

    let started = Instant::now();
    unpack_all(&paths, true, &mut errors);
    println!("unpacked in {:?}, {} errors", started.elapsed(), errors.len());

    let started = Instant::now();
    let gateway = SublimeResourceGateway::new(paths.clone());
    let ui = gateway.load_ui_resources(true, &mut errors);
    println!("{} bindings, {} palette entries in {:?}", ui.keymap.bindings.len(), ui.palette.len(), started.elapsed());
    println!("{} build systems", gateway.find_resources("*.sublime-build", true).len());

    let started = Instant::now();
    let mut engine = SyntaxEngine::new(paths);
    engine.reload(true, &mut errors);
    println!("{} syntaxes, {} schemes in {:?}", engine.syntax_names().len(), engine.scheme_names().len(), started.elapsed());
    println!("{} errors", errors.len());
    for error in errors.iter().take(40) {
        println!("  {}", error.chars().take(160).collect::<String>());
    }
    assert!(engine.syntax_names().iter().any(|s| s == "Python"));
    assert!(!ui.keymap.bindings.iter().any(|b| b.command.name.starts_with("vi_")), "Vintage is ignored");
    assert_eq!(errors, Vec::<String>::new());
}

#[test]
#[ignore = "reads the Sublime Text installed here and runs its plugins"]
fn the_plugins_sublime_ships_load() {
    let Some(archives) = install_dir() else {
        eprintln!("skipped: no Sublime Text here");
        return;
    };
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path(), archives);
    let engine = Rc::new(RefCell::new(SyntaxEngine::new(paths.clone())));
    let mut editor = Editor::new(Gateways {
        file_system: Box::new(StdFileSystem),
        dialogs: Box::new(NativeDialogs),
        clipboard: Box::new(SystemClipboard),
        resources: Box::new(SublimeResourceGateway::new(paths)),
        syntaxes: Box::new(SyntectSyntaxes(engine)),
        sessions: Box::new(SessionFileGateway::new(dir.path().join("Session.sublime_session"))),
        file_index: Box::new(FolderIndex::default()),
        shell: Box::new(OsShell),
        processes: Box::new(ChildProcesses::default()),
        plugins: Box::new(PythonPluginHost::new(dir.path().join("Lib").join("python"))),
    });
    let started = Instant::now();
    editor.start(Vec::new());
    println!("started in {:?}", started.elapsed());
    println!("console:\n{}", editor.state().console);
    for error in &editor.state().load_errors {
        println!("error: {error}");
    }
    assert!(editor.state().load_errors.is_empty());
}
