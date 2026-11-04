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

/// Lets the plugins and the external-change checks run until `done` or `seconds` pass; true if done.
fn run_until(editor: &mut Editor, view: &mut ViewState, seconds: u64, mut done: impl FnMut(&Editor) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        command_controller::poll_plugins(editor, view);
        editor.check_external_changes();
        if done(editor) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Runs a Package Control command whose quick panel lists packages and picks `package` in it.
fn pick_package(editor: &mut Editor, view: &mut ViewState, command: &str, package: &str) {
    command_controller::run(editor, view, &Command::new(command));
    run_until(editor, view, 45, |e| e.plugin_quick_panel().is_some());
    let panel = editor.plugin_quick_panel().unwrap_or_else(|| panic!("{command}: no list\n{}", editor.state().console));
    let index = panel.items.iter().position(|i| i.caption == package);
    let index = index.unwrap_or_else(|| panic!("{command}: {package} not in {:?}", panel.items.iter().map(|i| &i.caption).collect::<Vec<_>>()));
    command_controller::answer_plugin_quick_panel(editor, view, Some(index));
}

/// The `version` in the `package-metadata.json` of an installed `.sublime-package`.
fn archive_version(archive: &Path) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).ok()?).ok()?;
    let metadata: serde_json::Value = serde_json::from_reader(zip.by_name("package-metadata.json").ok()?).ok()?;
    metadata["version"].as_str().map(str::to_string)
}

/// Rewrites `archive` with `version` in its `package-metadata.json`, as if an older release were installed.
fn set_archive_version(archive: &Path, version: &str) {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let mut files = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
        if file.name() == "package-metadata.json" {
            let mut metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            metadata["version"] = serde_json::json!(version);
            bytes = serde_json::to_vec(&metadata).unwrap();
        }
        files.push((file.name().to_string(), file.is_dir(), bytes));
    }
    drop(zip);
    let mut writer = zip::ZipWriter::new(std::fs::File::create(archive).unwrap());
    for (name, is_dir, bytes) in files {
        if is_dir {
            writer.add_directory(name, zip::write::SimpleFileOptions::default()).unwrap();
        } else {
            writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut writer, &bytes).unwrap();
        }
    }
    writer.finish().unwrap();
}

/// Package Control installs a package with plugins and libraries, its command runs, it is
/// upgraded and removed. Runs third-party code from the network: `CODELIBRE_TEST_PACKAGE`
/// (default MarkdownPreview), its command `CODELIBRE_TEST_COMMAND` (default `markdown_preview`)
/// with the JSON `CODELIBRE_TEST_ARGS` (default `{"target": "sublime"}`: HTML in a new view).
#[test]
#[ignore = "installs a package with Package Control and runs its plugins, in a temporary profile"]
fn package_control_installs_upgrades_and_removes_a_package_with_plugins() {
    let Some(data) = sublime_data_dir() else {
        return;
    };
    let source = data.join("Installed Packages").join("Package Control.sublime-package");
    if !source.is_file() || find_interpreter(None).is_err() {
        eprintln!("skipped: no Package Control or no Python here");
        return;
    }
    let package = std::env::var("CODELIBRE_TEST_PACKAGE").unwrap_or_else(|_| "MarkdownPreview".into());
    let command = std::env::var("CODELIBRE_TEST_COMMAND").unwrap_or_else(|_| "markdown_preview".into());
    let args = std::env::var("CODELIBRE_TEST_ARGS").unwrap_or_else(|_| r#"{"target": "sublime"}"#.into());
    let dir = tempfile::tempdir().unwrap();
    let archives = dir.path().join("Installed Packages");
    std::fs::create_dir_all(&archives).unwrap();
    std::fs::copy(&source, archives.join("Package Control.sublime-package")).unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# Title\n\nSome *text*.\n").unwrap();
    let mut editor = scratch_editor(dir.path());
    editor.start(vec![notes.clone()]);
    let mut view = ViewState::default();
    run_until(&mut editor, &mut view, 15, |_| false);
    let console = |editor: &Editor| editor.state().console.clone();
    let report = |editor: &Editor| {
        let lib = dir.path().join("Lib");
        let listing = |name: &str| std::fs::read_dir(lib.join(name)).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>()).unwrap_or_default();
        println!("python33: {:?}\npython38: {:?}", listing("python33"), listing("python38"));
        println!("load errors: {:?}", editor.state().load_errors);
        println!("status: {:?}", editor.state().status.as_ref().map(|s| s.text.clone()));
    };

    // Install.
    pick_package(&mut editor, &mut view, "install_package", &package);
    let archive = archives.join(format!("{package}.sublime-package"));
    let unpacked = dir.path().join("Packages").join(&package);
    let loaded = run_until(&mut editor, &mut view, 120, |e| e.is_plugin_command(&command));
    report(&editor);
    println!("archive {}, folder {}, version {:?}", archive.is_file(), unpacked.is_dir(), archive_version(&archive));
    assert!(loaded, "{command} did not load\nconsole:\n{}", console(&editor));
    run_until(&mut editor, &mut view, 5, |_| false);

    // Its command runs on the Markdown file (Package Control's message view is the active one).
    editor.open_file(&notes);
    let before = editor.state().documents.len();
    command_controller::run(&mut editor, &mut view, &Command::with_args(&command, serde_json::from_str(&args).unwrap()));
    let opened = run_until(&mut editor, &mut view, 20, |e| e.state().documents.len() > before);
    let output = editor.active_document().map(|d| d.text.clone()).unwrap_or_default();
    println!("documents {before} → {}, active {:?}:\n{}", editor.state().documents.len(), editor.active_document().map(|d| d.name()), output.chars().take(300).collect::<String>());
    assert!(!console(&editor).contains("Traceback"), "console:\n{}", console(&editor));
    if package == "MarkdownPreview" {
        let heading = output.find("<h1").map(|at| &output[at..]).and_then(|h| h.find("</h1>").map(|end| &h[..end])).unwrap_or_default();
        println!("heading: {heading}");
        assert!(opened && heading.contains("Title"), "console:\n{}", console(&editor));
    }

    // Upgrade: the installed one says it is older.
    if archive.is_file() {
        let version = archive_version(&archive).unwrap();
        set_archive_version(&archive, "0.0.1");
        run_until(&mut editor, &mut view, 5, |_| false);
        pick_package(&mut editor, &mut view, "upgrade_package", &package);
        let upgraded = run_until(&mut editor, &mut view, 120, |_| archive_version(&archive).is_some_and(|v| v != "0.0.1"));
        run_until(&mut editor, &mut view, 5, |_| false);
        report(&editor);
        println!("upgraded to {:?} (was {version})", archive_version(&archive));
        assert!(upgraded, "console:\n{}", console(&editor));
        assert!(editor.is_plugin_command(&command), "{command} after the upgrade\nconsole:\n{}", console(&editor));
    }

    // Remove.
    pick_package(&mut editor, &mut view, "remove_package", &package);
    let removed = run_until(&mut editor, &mut view, 60, |e| !archive.exists() && !unpacked.exists() && !e.is_plugin_command(&command));
    run_until(&mut editor, &mut view, 5, |_| false);
    report(&editor);
    println!("console:\n{}", console(&editor));
    assert!(removed, "archive {}, folder {}, command {}", archive.exists(), unpacked.exists(), editor.is_plugin_command(&command));
    assert!(!console(&editor).contains("Traceback"));
}

/// Popular packages with plugins, for [`package_control_installs_the_listed_packages_at_start`].
const POPULAR_PACKAGES: &[&str] = &[
    "A File Icon", "AdvancedNewFile", "BracketHighlighter", "ColorHelper", "DocBlockr", "Emmet", "FileDiffs", "GitGutter",
    "LSP", "MarkdownEditing", "Origami", "PackageResourceViewer", "Pretty JSON", "SideBarEnhancements", "SublimeLinter",
    "Terminus", "Text Pastry", "TrailingSpaces",
];

/// Package Control installs at start the packages `installed_packages` lists and it does not
/// find (`satisfy_packages`, as when the settings come from another machine), then they load.
/// Runs third-party code from the network: the packages are `CODELIBRE_TEST_PACKAGES`
/// (comma-separated) or [`POPULAR_PACKAGES`]. Fails listing every load error.
#[test]
#[ignore = "installs packages with Package Control and runs their plugins, in a temporary profile"]
fn package_control_installs_the_listed_packages_at_start() {
    let Some(data) = sublime_data_dir() else {
        return;
    };
    let source = data.join("Installed Packages").join("Package Control.sublime-package");
    if !source.is_file() || find_interpreter(None).is_err() {
        eprintln!("skipped: no Package Control or no Python here");
        return;
    }
    let packages: Vec<String> = match std::env::var("CODELIBRE_TEST_PACKAGES") {
        Ok(list) => list.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        Err(_) => POPULAR_PACKAGES.iter().map(|p| p.to_string()).collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    let archives = dir.path().join("Installed Packages");
    std::fs::create_dir_all(&archives).unwrap();
    std::fs::copy(&source, archives.join("Package Control.sublime-package")).unwrap();
    let user = dir.path().join("Packages").join("User");
    std::fs::create_dir_all(&user).unwrap();
    let mut listed = packages.clone();
    listed.push("Package Control".into());
    std::fs::write(user.join("Package Control.sublime-settings"), serde_json::json!({ "installed_packages": listed }).to_string()).unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# Title\n\nSome *text*.\n").unwrap();

    let mut editor = scratch_editor(dir.path());
    editor.start(vec![notes]);
    let mut view = ViewState::default();
    let installed = |package: &str| archives.join(format!("{package}.sublime-package")).is_file() || dir.path().join("Packages").join(package).join("package-metadata.json").is_file();
    let started = Instant::now();
    let all = run_until(&mut editor, &mut view, 600, |_| packages.iter().all(|p| installed(p)));
    // The last ones reload once their folder holds still; Package Control re-enables them.
    run_until(&mut editor, &mut view, 15, |_| false);
    println!("installed in {:?}: {all}", started.elapsed());
    for package in &packages {
        println!("  {package}: {}", if installed(package) { "installed" } else { "MISSING" });
    }
    let console = editor.state().console.clone();
    println!("console:\n{console}");
    let errors = editor.state().load_errors.clone();
    println!("{} load errors", errors.len());
    for error in &errors {
        // The package, then the exception's last line.
        let first = error.lines().next().unwrap_or_default();
        let last = error.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default();
        println!("  {first}\n      {last}");
    }
    println!("ignored: {:?}", editor.global_settings().get("ignored_packages"));
    assert!(all, "not every package was installed");
    // Syntaxes syntect cannot read (Sublime Text 4's `extends`…) are a limit of syntect, not of plugins.
    let plugin_errors = errors.iter().filter(|e| !e.lines().next().unwrap_or_default().contains(".sublime-syntax")).count();
    assert_eq!(plugin_errors, 0, "{plugin_errors} plugin load errors");
}

/// `upgrade_all_packages` when Package Control and another package are out of date: it upgrades
/// itself alone first (it is disabled meanwhile, so its plugins unload and load again), then the
/// other one the next time. Runs third-party code from the network.
#[test]
#[ignore = "upgrades Package Control and a package with it, in a temporary profile"]
fn package_control_upgrades_itself_then_the_other_packages() {
    let Some(data) = sublime_data_dir() else {
        return;
    };
    let source = data.join("Installed Packages").join("Package Control.sublime-package");
    if !source.is_file() || find_interpreter(None).is_err() {
        eprintln!("skipped: no Package Control or no Python here");
        return;
    }
    let other = "Text Pastry";
    let dir = tempfile::tempdir().unwrap();
    let archives = dir.path().join("Installed Packages");
    std::fs::create_dir_all(&archives).unwrap();
    let package_control = archives.join("Package Control.sublime-package");
    std::fs::copy(&source, &package_control).unwrap();
    let user = dir.path().join("Packages").join("User");
    std::fs::create_dir_all(&user).unwrap();
    let settings = serde_json::json!({ "installed_packages": [other, "Package Control"], "auto_upgrade": false });
    std::fs::write(user.join("Package Control.sublime-settings"), settings.to_string()).unwrap();
    let mut editor = scratch_editor(dir.path());
    editor.start(Vec::new());
    let mut view = ViewState::default();
    let other_archive = archives.join(format!("{other}.sublime-package"));
    assert!(run_until(&mut editor, &mut view, 120, |_| other_archive.is_file()), "{}", editor.state().console);
    run_until(&mut editor, &mut view, 10, |_| false);

    set_archive_version(&package_control, "0.0.1");
    set_archive_version(&other_archive, "0.0.1");
    run_until(&mut editor, &mut view, 5, |_| false);
    let upgrade_all = Command::with_args("upgrade_all_packages", serde_json::json!({ "unattended": true }));
    let old = |archive: &Path| archive_version(archive).as_deref() == Some("0.0.1");

    command_controller::run(&mut editor, &mut view, &upgrade_all);
    let itself = run_until(&mut editor, &mut view, 120, |e| !old(&package_control) && e.is_plugin_command("install_package"));
    run_until(&mut editor, &mut view, 10, |_| false);
    println!("first: Package Control {:?}, {other} {:?}", archive_version(&package_control), archive_version(&other_archive));
    println!("ignored: {:?}", editor.global_settings().get("ignored_packages"));
    assert!(itself, "Package Control did not upgrade itself and load again\n{}", editor.state().console);
    assert!(old(&other_archive), "Package Control upgrades itself alone");

    command_controller::run(&mut editor, &mut view, &upgrade_all);
    let others = run_until(&mut editor, &mut view, 120, |_| !old(&other_archive));
    run_until(&mut editor, &mut view, 10, |_| false);
    println!("second: {other} {:?}", archive_version(&other_archive));
    println!("console:\n{}", editor.state().console);
    assert!(others, "{other} was not upgraded");
    assert!(editor.is_plugin_command("install_package"));
    assert!(editor.is_plugin_command("text_pastry_insert_text") || editor.state().load_errors.is_empty(), "{:?}", editor.state().load_errors);
    assert!(editor.state().load_errors.is_empty(), "{:?}", editor.state().load_errors);
    assert_eq!(editor.global_settings().get("ignored_packages"), Some(&serde_json::json!(["Vintage"])));
    assert!(!editor.state().console.contains("Traceback"));
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
