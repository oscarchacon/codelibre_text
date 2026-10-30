//! Checks against the packages of the real Sublime Text on this machine; skipped without one.
//! Run with `cargo test -p codelibre_text --test sublime_packages -- --ignored --nocapture`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use codelibre_text::gateways::python_plugin_host::{find_interpreter, PythonPluginHost};
use codelibre_text::gateways::{
    child_processes::ChildProcesses, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell,
    std_file_system::StdFileSystem, syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard,
};
use codelibre_text::syntax_engine::SyntaxEngine;
use entities::command::Command;
use interface_adapters::controllers::command_controller;
use interface_adapters::gateways::package_archives::unpack_all;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
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

/// Package Control copied to a temporary profile, with nothing else: nothing of the user's is
/// read or written. Only loads it and lets its timers run; it installs nothing.
#[test]
#[ignore = "runs the Package Control installed in Sublime Text, in a temporary profile"]
fn package_control_loads_in_a_scratch_profile() {
    let Some(config) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        return;
    };
    let source = config.join("Sublime Text").join("Installed Packages").join("Package Control.sublime-package");
    if !source.is_file() || find_interpreter(None).is_err() {
        eprintln!("skipped: no Package Control or no Python here");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let archives = dir.path().join("Installed Packages");
    std::fs::create_dir_all(&archives).unwrap();
    std::fs::copy(&source, archives.join("Package Control.sublime-package")).unwrap();
    let own = ArchiveFolder { archives, unpacked: dir.path().join("Cache").join("own"), sublime: false, shipped: false };
    let shipped = install_dir().map(|archives| ArchiveFolder {
        archives,
        unpacked: dir.path().join("Cache").join("shipped"),
        sublime: true,
        shipped: true,
    });
    let paths = PackagePaths {
        packages: dir.path().join("Packages"),
        sublime_packages: None,
        archive_folders: shipped.into_iter().chain([own]).collect(),
    };
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
    editor.set_event_command_runner(command_controller::run_during_event);
    editor.start(Vec::new());
    let mut view = ViewState::default();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("plugin commands: {:?}", editor.is_plugin_command("install_package"));
    let user_preferences = dir.path().join("Packages/User/Preferences.sublime-settings");
    println!("preferences at start: {:?}", std::fs::read_to_string(&user_preferences).ok());
    let ignored = editor.global_settings().get("ignored_packages").cloned();
    println!("global ignored: {ignored:?}");
    assert_eq!(ignored, Some(serde_json::json!(["Vintage"])), "{}", editor.state().console);
    // The list of packages: read from the network. Nothing is chosen, so nothing is installed.
    command_controller::run(&mut editor, &mut view, &Command::new("install_package"));
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline && editor.plugin_quick_panel().is_none() {
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(20));
    }
    match editor.plugin_quick_panel() {
        Some(panel) => println!("packages offered: {}, first: {:?}", panel.items.len(), panel.items.first().map(|i| &i.caption)),
        None => println!("no list of packages"),
    }
    // A color scheme has no code: installing it runs nothing of a third party.
    let chosen = editor.plugin_quick_panel().and_then(|panel| panel.items.iter().position(|i| i.caption == "Dracula Color Scheme"));
    if let Some(index) = chosen {
        command_controller::answer_plugin_quick_panel(&mut editor, &mut view, Some(index));
        let installed = dir.path().join("Installed Packages").join("Dracula Color Scheme.sublime-package");
        let deadline = Instant::now() + Duration::from_secs(40);
        while Instant::now() < deadline && !installed.is_file() {
            command_controller::poll_plugins(&mut editor, &mut view);
            std::thread::sleep(Duration::from_millis(20));
        }
        for _ in 0..150 {
            command_controller::poll_plugins(&mut editor, &mut view);
            std::thread::sleep(Duration::from_millis(20));
        }
        println!("installed: {}", installed.is_file());
        println!("settings: {:?}", std::fs::read_to_string(dir.path().join("Packages/User/Package Control.sublime-settings")).ok());
        println!("preferences: {:?}", std::fs::read_to_string(dir.path().join("Packages/User/Preferences.sublime-settings")).ok());
        editor.reload_packages();
        println!("after reload: {:?}", editor.state().status.as_ref().map(|s| s.text.clone()));
        command_controller::poll_plugins(&mut editor, &mut view);
        let found = editor.search(use_cases::search::SearchKind::ColorSchemes, "Dracula");
        println!("schemes: {:?}", found.iter().map(|c| &c.caption).collect::<Vec<_>>());
        assert!(installed.is_file());
        assert!(found.iter().any(|c| c.caption == "Dracula"));
        assert_eq!(editor.global_settings().get("ignored_packages"), Some(&serde_json::json!(["Vintage"])));
    }
    println!("status: {:?}", editor.state().status.as_ref().map(|s| s.text.clone()));
    println!("load errors: {}", editor.state().load_errors.len());
    for error in &editor.state().load_errors {
        println!("error: {error}");
    }
    println!("console:\n{}", editor.state().console);
    assert!(!editor.state().console.contains("Traceback"));
}
