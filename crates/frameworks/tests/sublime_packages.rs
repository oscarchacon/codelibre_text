//! Checks against the packages of the real Sublime Text on this machine; skipped without one.
//! Run with `cargo test -p codelibre_text --test sublime_packages -- --ignored --nocapture`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use codelibre_text::gateways::python_plugin_host::{find_interpreter, PythonPluginHost};
use codelibre_text::gateways::{
    child_processes::ChildProcesses, curl_downloads::CurlDownloads, file_error_log::FileErrorLog, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell,
    std_file_system::StdFileSystem, syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard,
};
use codelibre_text::syntax_engine::SyntaxEngine;
use entities::command::Command;
use entities::keymap::KeyCombo;
use entities::selection::Selection;
use interface_adapters::controllers::command_controller;
use interface_adapters::controllers::keyboard_controller::{key_pressed, KeyHandling};
use interface_adapters::gateways::package_archives::unpack_all;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
use use_cases::gateways::{
    ArchiveFolder, DialogGateway, FileFilter, MessageAnswer, MessageDialog, PackagePaths, ResourceGateway,
};
use use_cases::{Editor, Gateways};

/// The `Packages` folder of the Sublime Text installation, with the packages it ships.
fn install_dir() -> Option<PathBuf> {
    let dir = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("ProgramFiles")?).join("Sublime Text")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Sublime Text.app/Contents/MacOS")
    } else {
        PathBuf::from("/opt/sublime_text")
    }
    .join("Packages");
    dir.is_dir().then_some(dir)
}

/// Sublime Text's data folder (the parent of its `Packages` and `Installed Packages`).
fn sublime_data_dir() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    Some(if cfg!(target_os = "windows") {
        base.config_dir().join("Sublime Text")
    } else if cfg!(target_os = "macos") {
        base.data_dir().join("Sublime Text")
    } else {
        base.config_dir().join("sublime-text")
    })
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
        downloads: Box::new(CurlDownloads::default()),
        error_log: Box::new(FileErrorLog::new(dir.path().join("Errors.log"))),
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

/// Answers OK to every message dialog (the install question); never picks a file.
struct SayOk;

impl DialogGateway for SayOk {
    fn pick_files(&self, _: Option<&Path>, _: &[FileFilter]) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_folder(&self, _: Option<&Path>) -> Option<PathBuf> {
        None
    }
    fn pick_folders(&self, _: Option<&Path>) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_save_path(&self, _: Option<&Path>, _: &str, _: &[FileFilter]) -> Option<PathBuf> {
        None
    }
    fn show_message(&self, _: &MessageDialog) -> MessageAnswer {
        MessageAnswer::Ok
    }
}

/// An editor on a temporary profile in `dir` with the real gateways, Sublime's shipped packages
/// (if installed) and our own `Installed Packages`.
fn scratch_editor(dir: &Path) -> Editor {
    let own = ArchiveFolder {
        archives: dir.join("Installed Packages"),
        unpacked: dir.join("Cache").join("own"),
        sublime: false,
        shipped: false,
    };
    let shipped = install_dir().map(|archives| ArchiveFolder { archives, unpacked: dir.join("Cache").join("shipped"), sublime: true, shipped: true });
    let paths = PackagePaths { packages: dir.join("Packages"), sublime_packages: None, archive_folders: shipped.into_iter().chain([own]).collect() };
    let engine = Rc::new(RefCell::new(SyntaxEngine::new(paths.clone())));
    let mut editor = Editor::new(Gateways {
        file_system: Box::new(StdFileSystem),
        dialogs: Box::new(SayOk),
        clipboard: Box::new(SystemClipboard),
        resources: Box::new(SublimeResourceGateway::new(paths)),
        syntaxes: Box::new(SyntectSyntaxes(engine)),
        sessions: Box::new(SessionFileGateway::new(dir.join("Session.sublime_session"))),
        file_index: Box::new(FolderIndex::default()),
        shell: Box::new(OsShell),
        processes: Box::new(ChildProcesses::default()),
        downloads: Box::new(CurlDownloads::default()),
        error_log: Box::new(FileErrorLog::new(dir.join("Errors.log"))),
        plugins: Box::new(PythonPluginHost::new(dir.join("Lib").join("python"))),
    });
    editor.set_event_command_runner(command_controller::run_during_event);
    editor
}

/// Presses `key` like the window does and runs what it queued; true when a binding took it.
fn press(editor: &mut Editor, view: &mut ViewState, key: &str) -> bool {
    let taken = matches!(key_pressed(editor, view, &[KeyCombo::parse(key).unwrap()]), KeyHandling::Consumed);
    for command in view.take_commands() {
        command_controller::run(editor, view, &command);
    }
    taken
}

/// The `selector` context with Sublime's real language keymaps and syntect's scopes: Markdown's
/// `*` wraps the selection (`insert_snippet` with `${0:$SELECTION}`) only in Markdown text.
#[test]
#[ignore = "reads the Sublime Text installed here"]
fn language_key_bindings_apply_by_scope() {
    if install_dir().is_none() {
        eprintln!("skipped: no Sublime Text here");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("Packages").join("User");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("Preferences.sublime-settings"), r#"{ "auto_match_enabled": true }"#).unwrap();
    let (notes, code) = (dir.path().join("notes.md"), dir.path().join("a.py"));
    std::fs::write(&notes, "a word b\n").unwrap();
    std::fs::write(&code, "a word b\n").unwrap();
    let mut editor = scratch_editor(dir.path());
    editor.start(vec![code.clone(), notes.clone()]);
    let mut view = ViewState::default();

    let id = editor.active_document().unwrap().id;
    println!("syntax {:?}, scope at 2: {:?}", editor.active_document().unwrap().syntax, editor.scope_name(id, 2));
    editor.sync_selection_from_view(id, Selection::new(2, 6));
    assert!(press(&mut editor, &mut view, "*"), "Markdown's binding takes `*`");
    let doc = editor.active_document().unwrap();
    println!("markdown: {:?} {:?}", doc.text, doc.selection());
    assert_eq!(doc.text, "a *word* b\n");
    assert_eq!(doc.selection(), Selection::new(3, 7), "the word stays selected");

    editor.open_file(&code);
    let id = editor.active_document().unwrap().id;
    println!("syntax {:?}, scope at 2: {:?}", editor.active_document().unwrap().syntax, editor.scope_name(id, 2));
    editor.sync_selection_from_view(id, Selection::new(2, 6));
    assert!(!press(&mut editor, &mut view, "*"), "no binding for `*` in Python: the widget types it");
    assert_eq!(editor.active_document().unwrap().text, "a word b\n");
}

/// A plugin asks the scopes of a Python file (syntect's), through the real plugin host.
#[test]
#[ignore = "runs Python"]
fn plugins_read_the_scopes_of_the_text() {
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("Packages").join("MyPkg");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("probe.py"),
        r#"
import sublime, sublime_plugin

class ProbeCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        v = self.view
        sublime.status_message(repr([v.scope_name(5), v.match_selector(5, "string"), v.match_selector(0, "string"),
                                     v.extract_scope(5), v.find_by_selector("string.quoted"),
                                     v.expand_to_scope(5, "string")]))
"#,
    )
    .unwrap();
    let code = dir.path().join("a.py");
    std::fs::write(&code, "s = \"hi\"\nn = 1\n").unwrap();
    let mut editor = scratch_editor(dir.path());
    editor.start(vec![code]);
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("probe"));
    let status = editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    println!("{status}\nconsole:\n{}", editor.state().console);
    assert!(status.contains("string.quoted.double.python"), "{status}");
    assert!(status.contains("True, False, Region(4, 8)"), "the string with its quotes: {status}");
}

/// `install_package_control` with the network: downloads it into a temporary profile, the
/// packages reload by themselves and Package Control's commands are there.
#[test]
#[ignore = "downloads Package Control and runs it, in a temporary profile"]
fn install_package_control_downloads_and_loads_it() {
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut editor = scratch_editor(dir.path());
    editor.start(Vec::new());
    let mut view = ViewState::default();
    assert!(!editor.is_plugin_command("install_package"));
    command_controller::run(&mut editor, &mut view, &Command::new("install_package_control"));
    let status = |editor: &Editor| editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    println!("after the command: {}", status(&editor));

    let installed = dir.path().join("Installed Packages").join("Package Control.sublime-package");
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !editor.is_plugin_command("install_package") {
        editor.poll_downloads();
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("file: {} ({:?} bytes)", installed.is_file(), std::fs::metadata(&installed).map(|m| m.len()).ok());
    println!("status: {}", status(&editor));
    assert!(installed.is_file());
    assert!(!dir.path().join("Installed Packages").join("Package Control.sublime-package.download").exists());
    assert!(editor.is_plugin_command("install_package"), "console:\n{}", editor.state().console);

    // A second time it only says so.
    command_controller::run(&mut editor, &mut view, &Command::new("install_package_control"));
    assert_eq!(status(&editor), "Package Control is already installed");
    // Removing it unloads it once the folder holds still.
    std::fs::remove_file(&installed).unwrap();
    editor.check_installed_packages();
    editor.check_installed_packages();
    assert!(!editor.is_plugin_command("install_package"));
    assert!(!editor.state().console.contains("Traceback"), "{}", editor.state().console);
}

/// Package Control copied to a temporary profile, with nothing else: nothing of the user's is
/// read or written. Only loads it and lets its timers run; it installs nothing.
#[test]
#[ignore = "runs the Package Control installed in Sublime Text, in a temporary profile"]
fn package_control_loads_in_a_scratch_profile() {
    let Some(data) = sublime_data_dir() else {
        return;
    };
    let source = data.join("Installed Packages").join("Package Control.sublime-package");
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
        downloads: Box::new(CurlDownloads::default()),
        error_log: Box::new(FileErrorLog::new(dir.path().join("Errors.log"))),
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

        // Disable Package / Enable Package only change `ignored_packages`: the packages follow at once.
        for (command, ignored) in [("disable_package", true), ("enable_package", false)] {
            command_controller::run(&mut editor, &mut view, &Command::new(command));
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline && editor.plugin_quick_panel().is_none() {
                command_controller::poll_plugins(&mut editor, &mut view);
                std::thread::sleep(Duration::from_millis(20));
            }
            let panel = editor.plugin_quick_panel().unwrap_or_else(|| panic!("{command}: no list"));
            let index = panel.items.iter().position(|i| i.caption == "Dracula Color Scheme").expect("Dracula listed");
            command_controller::answer_plugin_quick_panel(&mut editor, &mut view, Some(index));
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                command_controller::poll_plugins(&mut editor, &mut view);
                editor.check_external_changes();
                let shown = editor.search(use_cases::search::SearchKind::ColorSchemes, "Dracula").iter().any(|c| c.caption == "Dracula");
                if shown != ignored {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let shown = editor.search(use_cases::search::SearchKind::ColorSchemes, "Dracula").iter().any(|c| c.caption == "Dracula");
            println!("{command}: ignored {:?}, Dracula shown: {shown}", editor.global_settings().get("ignored_packages"));
            assert_eq!(shown, !ignored, "{command}");
        }
    }
    println!("status: {:?}", editor.state().status.as_ref().map(|s| s.text.clone()));
    println!("load errors: {}", editor.state().load_errors.len());
    for error in &editor.state().load_errors {
        println!("error: {error}");
    }
    println!("console:\n{}", editor.state().console);
    assert!(!editor.state().console.contains("Traceback"));

    // Preferences > Package Settings > Package Control: its defaults, read from inside the
    // archive, open next to the user's file.
    let menu = interface_adapters::presenters::menu::present(&editor);
    let package_settings = menu.iter().flat_map(|m| &m.children).find(|e| e.caption == "Package Settings").expect("Package Settings");
    let ours = package_settings.children.iter().find(|e| e.caption == "Package Control").expect("Package Control's settings");
    let edit = ours.children.iter().filter_map(|e| e.command.clone()).find(|c| c.name == "edit_settings").expect("edit_settings");
    println!("settings command: {edit:?}");
    command_controller::run(&mut editor, &mut view, &edit);
    let defaults = editor.state().documents.iter().find(|d| d.read_only && d.name().starts_with("Package Control.sublime-settings"));
    assert!(defaults.expect("the defaults").text.contains("\"channels\""));
    assert_eq!(editor.active_document().unwrap().name(), "Package Control.sublime-settings");
}
