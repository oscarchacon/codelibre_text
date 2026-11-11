//! Tests for the Python plugin host (`gateways::python_plugin_host` and `assets/python/*.py`).
//!
//! They start a real Python 3 interpreter and play the editor's side of the protocol. When no
//! interpreter is found they are skipped (with a message), since Python is optional at runtime.

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use codelibre_text::gateways::{
    child_processes::ChildProcesses, curl_downloads::CurlDownloads, file_error_log::FileErrorLog, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell, std_file_system::StdFileSystem,
    syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard,
};
use codelibre_text::gateways::python_plugin_host::{bundled_interpreter, find_interpreter, PythonPluginHost};
use codelibre_text::syntax_engine::SyntaxEngine;
use entities::command::Command;
use interface_adapters::controllers::command_controller;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
use serde_json::{json, Value};
use use_cases::gateways::{PackagePaths, PluginCommandKind, PluginHostGateway, PluginLoad, PluginMessage, PluginPackage};
use use_cases::plugin_popups::HoverZone;
use use_cases::{Editor, Gateways};

/// A host with the plugins of `files` (name, contents) loaded from a package called `MyPkg`.
/// `None` when there is no Python 3 here.
fn host_with(dir: &Path, files: &[(&str, &str)]) -> Option<(PythonPluginHost, PluginMessage)> {
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return None;
    }
    let package = dir.join("Packages").join("MyPkg");
    fs::create_dir_all(&package).unwrap();
    for (name, contents) in files {
        fs::write(package.join(name), contents).unwrap();
    }
    let mut host = PythonPluginHost::new(dir.join("Lib").join("python"));
    host.load(&PluginLoad {
        packages: vec![PluginPackage { name: "MyPkg".into(), dirs: vec![package] }],
        dependencies: Vec::new(),
        packages_path: dir.join("Packages"),
        archives: Vec::new(),
        shipped_packages: None,
        interpreter: None,
    })
    .unwrap();
    let loaded = next(&mut host, &mut String::new());
    Some((host, loaded))
}

/// The next message that is not printed output (which is appended to `log`).
fn next(host: &mut PythonPluginHost, log: &mut String) -> PluginMessage {
    loop {
        match host.receive().unwrap() {
            PluginMessage::Log(text) => log.push_str(&text),
            message => return message,
        }
    }
}

fn expect_call(host: &mut PythonPluginHost, log: &mut String, function: &str) -> Vec<Value> {
    match next(host, log) {
        PluginMessage::Call { function: f, args } if f == function => args,
        other => panic!("expected a call to {function}, got {other:?}"),
    }
}

/// The next message sent outside a request (by a timer or a thread), polling like the window.
fn next_idle(host: &mut PythonPluginHost, log: &mut String) -> PluginMessage {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.try_receive().unwrap() {
            Some(PluginMessage::Log(text)) => log.push_str(&text),
            Some(message) => return message,
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            None => panic!("the plugins sent nothing; console:\n{log}"),
        }
    }
}

/// An editor with the real plugin host and `plugin` in the package `MyPkg`; `None` without Python.
fn editor_with(dir: &Path, plugin: &str) -> Option<Editor> {
    let editor = editor_with_load_errors(dir, plugin)?;
    assert!(editor.state().load_errors.is_empty(), "{:?}", editor.state().load_errors);
    Some(editor)
}

/// [`editor_with`], when the plugin is expected to fail to load.
fn editor_with_load_errors(dir: &Path, plugin: &str) -> Option<Editor> {
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return None;
    }
    let packages = dir.join("Packages");
    fs::create_dir_all(packages.join("MyPkg")).unwrap();
    fs::write(packages.join("MyPkg").join("plugin.py"), plugin).unwrap();
    let paths = PackagePaths { packages, sublime_packages: None, archive_folders: Vec::new() };
    let engine = Rc::new(RefCell::new(SyntaxEngine::new(paths.clone())));
    let mut editor = Editor::new(Gateways {
        file_system: Box::new(StdFileSystem),
        dialogs: Box::new(NativeDialogs),
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
    editor.start(Vec::new());
    Some(editor)
}

const HELLO: &str = r#"
import sublime
import sublime_plugin
from .helper import GREETING


class HelloCommand(sublime_plugin.TextCommand):
    def run(self, edit, name="world"):
        print("saying hello")
        self.view.insert(edit, self.view.sel()[0].b, GREETING + name)


class BrokenCommand(sublime_plugin.WindowCommand):
    def run(self):
        raise ValueError("boom")


class AddCaretsCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        sel = self.view.sel()
        sel.clear()
        sel.add(4)
        sel.add(sublime.Region(1, 2))
        sel.add(1)


class HTMLPrintCommand(sublime_plugin.ApplicationCommand):
    def run(self):
        sublime.load_settings("MyPkg.sublime-settings").set("count", 2)
        sublime.save_settings("MyPkg.sublime-settings")
"#;

#[test]
fn loads_plugins_and_runs_their_commands() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("hello.py", HELLO),
        ("helper.py", "GREETING = 'hello '\n"),
        ("bad.py", "def oops(:\n"),
    ];
    let Some((mut host, loaded)) = host_with(dir.path(), &files) else {
        return;
    };
    let PluginMessage::Loaded { commands, errors, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    assert_eq!(
        commands,
        vec![
            ("add_carets".to_string(), PluginCommandKind::Text),
            ("broken".to_string(), PluginCommandKind::Window),
            ("h_tMLPrint".to_string(), PluginCommandKind::Application),
            ("hello".to_string(), PluginCommandKind::Text),
        ],
        "named like Sublime names them"
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("bad.py") && errors[0].contains("SyntaxError"), "{}", errors[0]);

    // A text command: it reads the selection and inserts at the caret.
    let mut log = String::new();
    host.run(PluginCommandKind::Text, "hello", &json!({"name": "bob"}), Some(7)).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "view_selection_get"), vec![json!(7)]);
    host.reply(Ok(json!([[3, 3]]))).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "view_insert"), vec![json!(7), json!(3), json!("hello bob")]);
    host.reply(Ok(json!(9))).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "saying hello\n", "print() goes to the console, not the protocol");

    // An exception becomes the command's error, and the host keeps working.
    host.run(PluginCommandKind::Window, "broken", &Value::Null, None).unwrap();
    match next(&mut host, &mut log) {
        PluginMessage::Done { error: Some(traceback) } => assert!(traceback.contains("ValueError: boom"), "{traceback}"),
        other => panic!("expected an error, got {other:?}"),
    }

    // Selections: cleared, then added; overlapping ones and repeated carets merge.
    host.run(PluginCommandKind::Text, "add_carets", &Value::Null, Some(7)).unwrap();
    let mut sets: Vec<Value> = Vec::new();
    loop {
        match next(&mut host, &mut log) {
            PluginMessage::Call { function, .. } if function == "view_selection_get" => {
                assert!(!sets.is_empty(), "a cleared selection is not read back from the editor");
                host.reply(Ok(sets.last().unwrap().clone())).unwrap();
            }
            PluginMessage::Call { function, args } => {
                assert_eq!(function, "view_selection_set");
                sets.push(args[1].clone());
                host.reply(Ok(Value::Null)).unwrap();
            }
            message => {
                assert_eq!(message, PluginMessage::Done { error: None });
                break;
            }
        }
    }
    assert_eq!(sets, vec![json!([[4, 4]]), json!([[1, 2], [4, 4]]), json!([[1, 2], [4, 4]])]);

    // Settings files.
    host.run(PluginCommandKind::Application, "h_tMLPrint", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "settings_set");
    assert_eq!(args, vec![json!(["file", "MyPkg.sublime-settings"]), json!("count"), json!(2)]);
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "save_settings"), vec![json!("MyPkg.sublime-settings")]);
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });

    host.stop();
    assert!(host.receive().is_err(), "a stopped host answers nothing");
}

#[test]
fn plugins_of_an_archive_see_the_paths_inside_it() {
    if find_interpreter(None).is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let unpacked = dir.path().join("Cache");
    let package = unpacked.join("Zipped");
    fs::create_dir_all(package.join("lib").join("deep")).unwrap();
    fs::write(package.join("lib").join("__init__.py"), "").unwrap();
    fs::write(package.join("lib").join("deep").join("__init__.py"), "VALUE = 'inner'\n").unwrap();
    let plugin = r#"
import sublime_plugin
from .lib import deep
print(__file__)
print(__spec__.origin)
print(deep.__file__)
print(deep.VALUE)
"#;
    fs::write(package.join("plugin.py"), plugin).unwrap();
    let archives = dir.path().join("Installed Packages");
    let mut host = PythonPluginHost::new(dir.path().join("Lib").join("python"));
    host.load(&PluginLoad {
        packages: vec![PluginPackage { name: "Zipped".into(), dirs: vec![package] }],
        dependencies: Vec::new(),
        packages_path: dir.path().join("Packages"),
        archives: vec![(unpacked, archives.clone())],
        shipped_packages: None,
        interpreter: None,
    })
    .unwrap();
    let mut log = String::new();
    match next(&mut host, &mut log) {
        PluginMessage::Loaded { errors, .. } => assert_eq!(errors, Vec::<String>::new()),
        other => panic!("{other:?}"),
    }
    let root = archives.join("Zipped.sublime-package");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines[0], root.join("plugin.py").display().to_string());
    assert_eq!(lines[1], root.join("plugin.py").display().to_string());
    assert_eq!(lines[2], root.join("lib").join("deep").join("__init__.py").display().to_string());
    assert_eq!(lines[3], "inner");
    assert!(dir.path().join("Lib").join("Packages").is_dir(), "next to the executable plugins are told about");
    host.stop();
}

/// Package Control prefixes paths with `\\?\` on Windows, after which `/` is no separator.
#[test]
fn plugins_get_paths_with_the_platforms_separators() {
    if find_interpreter(None).is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("Packages").join("MyPkg");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("plugin.py"), "import sublime\nprint(sublime.packages_path())\nprint(sublime.installed_packages_path())\n").unwrap();
    let mut host = PythonPluginHost::new(dir.path().join("Lib").join("python"));
    let slashed = std::path::PathBuf::from(dir.path().join("Packages").display().to_string().replace('\\', "/"));
    host.load(&PluginLoad {
        packages: vec![PluginPackage { name: "MyPkg".into(), dirs: vec![package] }],
        dependencies: Vec::new(),
        packages_path: slashed,
        archives: Vec::new(),
        shipped_packages: None,
        interpreter: None,
    })
    .unwrap();
    let mut log = String::new();
    assert!(matches!(next(&mut host, &mut log), PluginMessage::Loaded { .. }));
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines[0], dir.path().join("Packages").display().to_string());
    assert_eq!(lines[1], dir.path().join("Installed Packages").display().to_string());
    host.stop();
}

#[test]
fn plugins_import_package_control_3_dependencies_from_the_variant_for_this_python() {
    if find_interpreter(None).is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let packages = dir.path().join("Packages");
    let write = |path: &str, contents: &str| {
        let path = packages.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    };
    write("oldlib/dependency-metadata.json", "{}");
    write("oldlib/all/oldlib.py", "WHERE = 'all'\n");
    write("oldlib/st3/oldlib.py", "WHERE = 'st3'\n");
    write("oldlib/st3_windows_x64/oldlib.py", "WHERE = 'compiled for 3.3'\n");
    write("pure/.sublime-dependency", "");
    write("pure/all/pure/__init__.py", "WHERE = 'all'\n");
    write("Uses/plugin.py", "import oldlib, pure\nprint(oldlib.WHERE, pure.WHERE)\n");
    let mut host = PythonPluginHost::new(dir.path().join("Lib").join("python"));
    let mut load = |dependencies: Vec<std::path::PathBuf>| {
        host.load(&PluginLoad {
            packages: vec![PluginPackage { name: "Uses".into(), dirs: vec![packages.join("Uses")] }],
            dependencies,
            packages_path: packages.clone(),
            archives: Vec::new(),
            shipped_packages: None,
            interpreter: None,
        })
        .unwrap();
        let mut log = String::new();
        let PluginMessage::Loaded { errors, .. } = next(&mut host, &mut log) else {
            panic!("expected Loaded; console:\n{log}");
        };
        (errors, log)
    };
    let (errors, log) = load(vec![packages.join("oldlib"), packages.join("pure")]);
    assert_eq!(errors, Vec::<String>::new());
    assert_eq!(log.trim(), "st3 all");

    // Gone from the next load: off the path (the plugin changed, so it runs again).
    write("Uses/plugin.py", "import os, sys\nprint(sorted(os.path.basename(os.path.dirname(p)) for p in sys.path if os.sep + 'Packages' + os.sep in p))\n");
    let (errors, log) = load(vec![packages.join("oldlib")]);
    assert_eq!(errors, Vec::<String>::new());
    assert_eq!(log.trim(), "['oldlib']");
    host.stop();
}

#[test]
fn errors_from_the_editor_become_python_exceptions() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin

class SizeCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        try:
            self.view.size()
        except RuntimeError as e:
            print("caught:", e)
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("size.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Text, "size", &Value::Null, Some(1)).unwrap();
    expect_call(&mut host, &mut log, "view_size");
    host.reply(Err("the view is closed".into())).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "caught: the view is closed\n");
}

#[test]
fn a_plugin_running_another_plugin_command_is_answered_in_between() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin

class OuterCommand(sublime_plugin.WindowCommand):
    def run(self):
        self.window.run_command("inner")
        print("outer done")

class InnerCommand(sublime_plugin.WindowCommand):
    def run(self):
        print("inner ran")
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("nested.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "outer", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "run_command");
    assert_eq!(args, vec![json!("window"), json!(1), json!("inner"), Value::Null]);
    // The editor finds that "inner" is a plugin command and asks the host to run it meanwhile.
    host.run(PluginCommandKind::Window, "inner", &Value::Null, None).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "inner ran\nouter done\n");
}

/// The whole path: the editor starts the real host, the command controller sends it a command,
/// and the plugin edits the view, changes the selection and runs a built-in command.
#[test]
fn end_to_end_through_the_editor_and_the_command_controller() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class ShoutCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        self.view.insert(edit, 0, "hello world")
        self.view.sel().clear()
        self.view.sel().add(sublime.Region(0, 5))
        self.view.run_command("upper_case")
        sublime.status_message("shouted " + self.view.substr(sublime.Region(0, self.view.size())))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    assert!(editor.is_plugin_command("shout"));

    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("shout"));
    let status = editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    assert_eq!(status, "shouted HELLO world", "console:\n{}", editor.state().console);
    assert_eq!(editor.active_document().unwrap().text, "HELLO world");
}

/// `sublime_plugin.reload_plugin` imports a changed plugin file again: the editor hears its new
/// commands at once, the old ones go, `plugin_unloaded` / `plugin_loaded` run.
#[test]
fn reload_plugin_loads_a_changed_plugin_and_the_editor_hears_its_commands() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import os
import sublime
import sublime_plugin

def plugin_loaded():
    print("loaded first")

def plugin_unloaded():
    print("unloaded first")

class FirstCommand(sublime_plugin.WindowCommand):
    def run(self):
        sublime.status_message("first")

class ReloadIt(sublime_plugin.WindowCommand):
    def run(self):
        path = os.path.join(os.path.dirname(__file__), "plugin.py")
        with open(path) as f:
            source = f.read()
        with open(path, "w") as f:
            f.write(source.replace("First", "Second").replace("first", "second"))
        sublime_plugin.reload_plugin("MyPkg.plugin")
"#
    .replace("class ReloadIt", "class ReloadItCommand");
    let Some(mut editor) = editor_with(dir.path(), &plugin) else {
        return;
    };
    assert!(editor.is_plugin_command("first"));
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("reload_it"));
    let console = editor.state().console.clone();
    assert!(editor.is_plugin_command("second"), "console:\n{console}");
    assert!(!editor.is_plugin_command("first"), "the old command is gone");
    assert!(console.contains("unloaded first") && console.contains("loaded second"), "console:\n{console}");
    command_controller::run(&mut editor, &mut view, &Command::new("second"));
    assert_eq!(editor.state().status.as_ref().map(|s| s.text.as_str()), Some("second"));
}

/// `Default.paste_from_history` is the editor's history, and the modules newer Pythons removed
/// (`asyncore`, `asynchat`, `distutils.*`) import.
#[test]
fn the_clipboard_history_and_removed_modules_are_there_for_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin
from Default.paste_from_history import g_clipboard_history

class ProbeCommand(sublime_plugin.WindowCommand):
    def run(self):
        import asyncore, asynchat
        from distutils.util import strtobool
        from distutils.spawn import find_executable
        from distutils.dir_util import mkpath
        g_clipboard_history.push_text("copied in Terminus")
        sublime.status_message("%s %s %s" % (g_clipboard_history.get()[0][0], strtobool("on"), asynchat.async_chat.__name__))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("probe"));
    let status = editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    assert_eq!(status, "copied in Terminus 1 async_chat", "console:\n{}", editor.state().console);
    assert_eq!(editor.clipboard_history(), ["copied in Terminus"]);
}

/// `Default.indentation` (Alignment), `Default.history_list` (Anaconda) and `Default.side_bar`
/// (FileManager subclasses `RenamePathCommand`): imported as packages do, and they work.
#[test]
fn default_indentation_history_list_and_side_bar_are_there_for_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.txt");
    std::fs::write(&old, "x").unwrap();
    let plugin = r#"
import os

import sublime
import sublime_plugin
from Default.indentation import line_and_normed_pt
from Default.history_list import get_jump_history_for_view
from Default.side_bar import RenamePathCommand


class HiddenRenamePathCommand(RenamePathCommand):
    def is_visible(self):
        return False


class ProbeCommand(sublime_plugin.TextCommand):
    def run(self, edit, old):
        self.view.settings().set("tab_size", 4)
        self.view.insert(edit, 0, "\tab = 1\nsecond line")
        row_col = line_and_normed_pt(self.view, 3)
        history = get_jump_history_for_view(self.view)
        self.view.sel().clear()
        self.view.sel().add(sublime.Region(1))
        history.push_selection(self.view)
        self.view.sel().clear()
        self.view.sel().add(sublime.Region(12))
        went_back = history.back(self.view)
        RenamePathCommand(self.view.window()).on_done(old, os.path.dirname(old), "new.txt")
        sublime.status_message("%s %s %s" % (row_col, went_back, self.view.sel()[0].a))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    let probe = Command::with_args("probe", serde_json::json!({ "old": old.to_string_lossy() }));
    command_controller::run(&mut editor, &mut view, &probe);
    let status = editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    assert_eq!(status, "(0, 6) True 1", "console:\n{}", editor.state().console);
    assert!(dir.path().join("new.txt").is_file() && !old.exists());
    assert!(editor.is_plugin_command("hidden_rename_path"));
}

/// `certifi`, which Sublime ships for every plugin host and `lsp_utils` imports without declaring
/// it: its bundle is a file `ssl` loads.
#[test]
fn certifi_is_there_for_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import os
import ssl

import certifi
import sublime
import sublime_plugin


class ProbeCommand(sublime_plugin.WindowCommand):
    def run(self):
        context = ssl.create_default_context(cafile=certifi.where())
        authorities = context.cert_store_stats()["x509_ca"]
        sublime.status_message("%s %s" % (os.path.isfile(certifi.where()), authorities > 100))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("probe"));
    assert_eq!(editor.state().status.as_ref().map(|s| s.text.as_str()), Some("True True"), "console:\n{}", editor.state().console);
}

/// A package that imports one not installed yet (LSP-json before LSP: Package Control installs
/// them one at a time) loads once that one arrives, without changing itself.
#[test]
fn a_package_that_could_not_import_another_loads_once_it_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin
from Base.core import GREETING


class HelperCommand(sublime_plugin.WindowCommand):
    def run(self):
        sublime.status_message(GREETING)
"#;
    let Some(mut editor) = editor_with_load_errors(dir.path(), plugin) else {
        return;
    };
    assert!(!editor.is_plugin_command("helper"));
    assert!(editor.state().load_errors.iter().any(|e| e.contains("No module named 'Base'")), "{:?}", editor.state().load_errors);

    let base = dir.path().join("Packages").join("Base");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("core.py"), "GREETING = 'from Base'\n").unwrap();
    editor.load_plugins();
    assert!(editor.state().load_errors.is_empty(), "{:?}", editor.state().load_errors);
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("helper"));
    assert_eq!(editor.state().status.as_ref().map(|s| s.text.as_str()), Some("from Base"));
}

/// The modules newer Pythons removed entirely (`cgi`, `imghdr`, `pipes`, `uu`) and the names
/// they dropped (the `collections` ABCs, `inspect.getargspec`, `ssl.wrap_socket`…) are there for
/// plugins and the libraries Package Control installs (`compat.py` and `shims/`).
#[test]
fn removed_modules_and_standard_library_names_are_there_for_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import io

import sublime
import sublime_plugin


class ProbeCommand(sublime_plugin.WindowCommand):
    def run(self):
        import cgi
        import collections
        import collections.abc
        import configparser
        import imghdr
        import inspect
        import pkgutil
        import pipes
        import ssl
        import threading
        import time
        import typing
        import unittest
        import uu

        assert issubclass(dict, collections.Mapping)
        spec = inspect.getargspec(lambda a, b=1: None)
        assert spec.args == ["a", "b"] and spec.defaults == (1,), spec
        assert inspect.formatargspec(["a", "b"], defaults=(1,)) == "(a, b=1)"
        assert hasattr(unittest.TestCase, "assertEquals")
        unittest.TestCase("assertEquals").assertEquals(1, 1)
        case = unittest.TestCase("assertDictContainsSubset")
        case.assertDictContainsSubset({"a": 1}, {"a": 1, "b": 2})
        try:
            case.assertDictContainsSubset({"a": 1}, {"a": 2, "b": 2})
            raise LookupError("a subset with a different value passed")
        except AssertionError:
            pass

        class OldTests(unittest.TestCase):
            def test_one(self):
                pass

            def test_two(self):
                pass

        assert unittest.makeSuite(OldTests).countTestCases() == 2
        assert unittest.getTestCaseNames(OldTests, "test") == ["test_one", "test_two"]
        import types
        assert unittest.findTestCases(types.ModuleType("empty")).countTestCases() == 0

        import asyncio
        import fractions

        @asyncio.coroutine
        def old_style():
            yield

        assert asyncio.iscoroutinefunction(old_style)
        loop = asyncio.new_event_loop()
        try:
            assert asyncio.all_tasks(loop) == set()
            assert asyncio.current_task(loop) is None
        finally:
            loop.close()
        assert fractions.gcd(6, 8) == 2
        assert issubclass(configparser.SafeConfigParser, configparser.ConfigParser)
        assert hasattr(configparser.ConfigParser, "readfp")

        assert callable(ssl.wrap_socket) and callable(ssl.match_hostname)
        cert = {"subjectAltName": (("DNS", "*.example.com"),)}
        ssl.match_hostname(cert, "a.example.com")
        try:
            ssl.match_hostname(cert, "a.b.example.com")
            raise AssertionError("a wildcard matched two labels")
        except ssl.CertificateError:
            pass

        assert hasattr(typing, "ByteString")
        assert callable(threading.currentThread) and callable(threading.Thread.isAlive)
        assert time.clock() >= 0
        assert pkgutil.ImpImporter is not None and pkgutil.get_loader("json") is not None
        import importlib.metadata
        assert hasattr(importlib.metadata.entry_points(), "get")

        assert cgi.escape("<a>") == "&lt;a&gt;"
        assert cgi.parse_header('text/html; charset="utf-8"') == ("text/html", {"charset": "utf-8"})
        assert cgi.parse_qs("a=1&a=2") == {"a": ["1", "2"]}
        assert imghdr.what(None, b"\x89PNG\r\n\x1a\n" + b"\0" * 24) == "png"
        assert imghdr.what(None, b"GIF89a" + b"\0" * 26) == "gif"
        assert pipes.quote("a b") == "'a b'"

        data = b"hello uu\n" * 20
        encoded = io.BytesIO()
        uu.encode(io.BytesIO(data), encoded, name="x.txt", mode=0o644)
        encoded.seek(0)
        decoded = io.BytesIO()
        uu.decode(encoded, decoded, quiet=True)
        assert decoded.getvalue() == data
        sublime.status_message("shims ok")
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("probe"));
    let status = editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default();
    assert_eq!(status, "shims ok", "console:\n{}", editor.state().console);
}

#[test]
fn a_build_like_command_writes_to_an_output_panel_from_a_thread() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import threading
import sublime
import sublime_plugin

class FakeBuildCommand(sublime_plugin.WindowCommand):
    def run(self):
        panel = self.window.create_output_panel("exec")
        panel.set_read_only(True)
        panel.settings().set("result_file_regex", "^(.+):(\\d+)")
        self.window.run_command("show_panel", {"panel": "output.exec"})
        def work():
            for line in ("compiling\n", "a.rs:3: error\n"):
                panel.run_command("append", {"characters": line, "force": True, "scroll_to_end": True})
            sublime.status_message("built %s %s" % (self.window.active_panel(), panel.size()))
        threading.Thread(target=work).start()
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("fake_build"));
    assert_eq!(editor.shown_output_panel().map(|p| p.name.as_str()), Some("exec"), "console:\n{}", editor.state().console);

    // The thread's calls are answered between frames.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !editor_status(&editor).starts_with("built") && Instant::now() < deadline {
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(editor_status(&editor), "built output.exec 24", "console:\n{}", editor.state().console);
    assert_eq!(editor.shown_output_panel().unwrap().document.text, "compiling\na.rs:3: error\n");
    assert_eq!(editor.state().documents.len(), 1, "the panel is not a tab");
}

/// Build plugins subclass `Default.exec.ExecCommand`; the editor's own `exec` stays.
#[test]
fn a_subclass_of_default_exec_runs_a_process_into_the_exec_panel() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin
from Default.exec import ExecCommand, AsyncProcess, ProcessListener

class MyBuildCommand(ExecCommand):
    def on_finished(self, proc):
        super().on_finished(proc)
        print("exit", proc.exit_code(), sorted(self.errs_by_file))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    assert!(editor.plugin_command_kind("exec").is_none(), "Default's ExecCommand is not a command");
    let mut view = ViewState::default();
    let args = json!({
        "shell_cmd": "echo a.py:3: oops",
        "file_regex": "^(.+):(\\d+): ",
        "working_dir": dir.path().display().to_string(),
    });
    command_controller::run(&mut editor, &mut view, &Command::with_args("my_build", args));
    assert_eq!(editor.shown_output_panel().map(|p| p.name.as_str()), Some("exec"), "console:\n{}", editor.state().console);

    let deadline = Instant::now() + Duration::from_secs(10);
    while !editor.state().console.contains("exit ") && Instant::now() < deadline {
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(5));
    }
    let console = editor.state().console.clone();
    assert!(console.contains("Running echo a.py:3: oops"), "{console}");
    assert!(console.contains("exit 0 ['a.py']"), "{console}");
    let text = editor.shown_output_panel().unwrap().document.text.clone();
    assert!(text.starts_with("a.py:3: oops\n[Finished in "), "{text:?}");
    assert_eq!(editor_status(&editor), "Build finished with 1 errors");
}

#[test]
fn hover_popups_and_phantom_sets_work_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Hover(sublime_plugin.EventListener):
    def on_hover(self, view, point, zone):
        view.show_popup('<a href="go">go %d %d</a>' % (point, zone), location=point,
                        on_navigate=lambda href: print("navigate", href), on_hide=lambda: print("hidden"))

class PhantomsCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        global SET
        SET = sublime.PhantomSet(self.view, "p")
        SET.update([sublime.Phantom(sublime.Region(0, 1), "<b>a</b>", sublime.LAYOUT_BELOW, lambda h: print("phantom", h)),
                    sublime.Phantom(sublime.Region(2, 3), "b", sublime.LAYOUT_INLINE)])
        print("ids", [p.id for p in SET.phantoms])

class PhantomsAgainCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        SET.update([sublime.Phantom(sublime.Region(0, 1), "<b>a</b>", sublime.LAYOUT_BELOW)])
        print("ids again", [p.id for p in SET.phantoms], self.view.query_phantoms([p.id for p in SET.phantoms]))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    editor.insert_at_selections("hello");
    let v = editor.active_document().unwrap().id;
    assert!(editor.plugins_hear_hover());

    editor.plugin_hover(v, 2, HoverZone::Text);
    let popup = editor.plugin_popup().unwrap_or_else(|| panic!("no popup; console:\n{}", editor.state().console));
    assert_eq!((popup.location, popup.content.as_str()), (2, r#"<a href="go">go 2 1</a>"#));
    command_controller::navigate_plugin_popup(&mut editor, &mut view, "go");
    editor.hide_plugin_popup();
    command_controller::poll_plugins(&mut editor, &mut view);
    let console = editor.state().console.clone();
    assert!(console.contains("navigate go") && console.contains("hidden"), "{console}");

    command_controller::run(&mut editor, &mut view, &Command::new("phantoms"));
    let ids: Vec<u64> = editor.active_document().unwrap().phantoms().iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 2, "console:\n{}", editor.state().console);
    command_controller::navigate_plugin_phantom(&mut editor, &mut view, ids[0], "fix");
    command_controller::run(&mut editor, &mut view, &Command::new("phantoms_again"));
    let console = editor.state().console.clone();
    assert!(console.contains("phantom fix"), "{console}");
    assert!(console.contains(&format!("ids again [{}] [Region(0, 1)]", ids[0])), "the equal phantom keeps its id\n{console}");
    let left: Vec<u64> = editor.active_document().unwrap().phantoms().iter().map(|p| p.id).collect();
    assert_eq!(left, vec![ids[0]]);
}

#[test]
fn plugins_completions_reach_the_auto_complete_list_also_later() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Now(sublime_plugin.EventListener):
    def on_query_completions(self, view, prefix, locations):
        print("asked", prefix, locations, view.is_auto_complete_visible())
        item = sublime.CompletionItem("zeta_fn", "fn", "zeta_fn($1)", sublime.COMPLETION_FORMAT_SNIPPET,
                                      sublime.KIND_FUNCTION, "<b>details</b>")
        return ([item, ["zeta_pair\tpair", "PAIR"], "zeta_str"], sublime.INHIBIT_WORD_COMPLETIONS)

class Later(sublime_plugin.ViewEventListener):
    def on_query_completions(self, prefix, locations):
        completions = sublime.CompletionList()
        sublime.set_timeout_async(lambda: completions.set_completions(["zeta_later"]), 50)
        return completions
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    editor.insert_at_selections("zeta_word ze");
    command_controller::run(&mut editor, &mut view, &Command::new("auto_complete"));
    let triggers = |editor: &Editor| editor.auto_complete().map(|l| l.shown.iter().map(|s| s.completion.trigger.clone()).collect::<Vec<_>>());
    let now = triggers(&editor).unwrap_or_else(|| panic!("no list; console:\n{}", editor.state().console));
    assert_eq!(now.len(), 3, "no words (INHIBIT_WORD_COMPLETIONS): {now:?}");
    assert!(editor.state().console.contains("asked ze [12] False"), "{}", editor.state().console);
    let item = &editor.auto_complete().unwrap().shown.iter().find(|s| s.completion.trigger == "zeta_fn").unwrap().completion;
    assert_eq!((item.annotation.as_str(), item.kind.letter.as_str(), item.details.as_str()), ("fn", "f", "<b>details</b>"));

    let deadline = Instant::now() + Duration::from_secs(10);
    while !triggers(&editor).unwrap_or_default().contains(&"zeta_later".to_string()) {
        assert!(Instant::now() < deadline, "set_completions never arrived; console:\n{}", editor.state().console);
        command_controller::poll_plugins(&mut editor, &mut view);
        std::thread::sleep(Duration::from_millis(20));
    }
    let pair = triggers(&editor).unwrap().iter().position(|t| t == "zeta_pair").unwrap();
    editor.commit_completion(Some(pair));
    assert_eq!(editor.active_document().unwrap().text, "zeta_word PAIR");
}

#[test]
fn regions_statuses_and_show_reach_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r##"
import sublime
import sublime_plugin

class MarkCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        self.view.insert(edit, 0, "alpha beta\ngamma")
        self.view.add_regions("words", [sublime.Region(6, 10), 0], "region.redish", "dot",
                              sublime.DRAW_NO_FILL | sublime.DRAW_NO_OUTLINE | sublime.DRAW_SQUIGGLY_UNDERLINE,
                              annotations=["<a href='why'>why</a>"], annotation_color="#ff0000",
                              on_navigate=lambda href: print("annotation link", href))
        self.view.set_status("count", "2 marks")
        self.view.sel().clear()
        self.view.sel().add(sublime.Region(11, 12))
        self.view.show_at_center(self.view.sel())
        sublime.status_message("%s %s" % (self.view.get_regions("words"), self.view.get_status("count")))
"##;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("mark"));
    assert_eq!(
        editor_status(&editor),
        "[Region(6, 10), Region(0, 0)] 2 marks",
        "console:\n{}",
        editor.state().console
    );
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.region_sets()[0].flags, 32 | 256 | 2048);
    assert_eq!(doc.region_sets()[0].annotations, ["<a href='why'>why</a>"]);
    assert_eq!(doc.region_sets()[0].annotation_color, "#ff0000");
    assert_eq!(doc.show_request(), Some(entities::document::ShowRequest { point: 11, center: true }));
    assert_eq!(editor.plugin_statuses(doc.id), ["2 marks"]);
    let id = doc.id;
    command_controller::navigate_plugin_annotation(&mut editor, &mut view, id, "words", "why");
    assert!(editor.state().console.contains("annotation link why"), "console:\n{}", editor.state().console);
}

#[test]
fn plugins_using_what_is_not_implemented_still_run() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import imp
from distutils.version import LooseVersion
import sublime
import sublime_plugin

class FancyCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        self.view.insert(edit, 0, "word")
        phantoms = sublime.PhantomSet(self.view, "lint")
        phantoms.update([sublime.Phantom(sublime.Region(0, 4), "<b>x</b>", sublime.LAYOUT_BELOW)])
        self.view.show_popup("<p>hi</p>", sublime.HIDE_ON_MOUSE_MOVE_AWAY)
        self.view.window().set_layout(self.view.window().layout())
        newer = LooseVersion("1.10") > LooseVersion("1.9")
        sublime.status_message("%s %s %s %s %r %s" % (
            newer, self.view.extract_scope(1), sublime.score_selector("source.python string", "source - string"),
            sublime.score_selector("source.python", "source"), self.view.scope_name(1),
            self.view.match_selector(1, "text")))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("fancy"));
    assert_eq!(editor_status(&editor), "True (0, 4) 0 8 'text.plain ' True", "console:\n{}", editor.state().console);
}

/// Package Control installs libraries in `Lib/python38`, and in `Lib/python33` for packages of
/// Python 3.3; plugins import from both, `python38` first.
#[test]
fn plugins_import_the_libraries_package_control_installs() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("Lib");
    for (folder, files) in [("python38", ["shared.py", "lib38.py"]), ("python33", ["shared.py", "lib33.py"])] {
        fs::create_dir_all(lib.join(folder)).unwrap();
        for file in files {
            fs::write(lib.join(folder).join(file), format!("WHERE = '{folder}'\n")).unwrap();
        }
    }
    let plugin = "import lib33, lib38, shared\nassert (lib33.WHERE, lib38.WHERE, shared.WHERE) == \
                  ('python33', 'python38', 'python38')\n";
    let Some((_host, loaded)) = host_with(dir.path(), &[("uses_libs.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { errors, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
}

/// Libraries written for Python 3.8 (Markdown) read entry points by group from a dict, which
/// Python 3.12 dropped; the host gives it back.
#[test]
fn libraries_read_entry_points_by_group_as_in_python_3_8() {
    let dir = tempfile::tempdir().unwrap();
    let dist_info = dir.path().join("Lib").join("python38").join("ext-1.0.dist-info");
    fs::create_dir_all(&dist_info).unwrap();
    fs::write(dist_info.join("METADATA"), "Metadata-Version: 2.1\nName: ext\nVersion: 1.0\n").unwrap();
    fs::write(dist_info.join("entry_points.txt"), "[markdown.extensions]\nabbr = ext:Abbr\n").unwrap();
    let plugin = "from importlib import metadata\n\
                  found = metadata.entry_points()\n\
                  assert [e.name for e in found.get('markdown.extensions', ())] == ['abbr'], found\n\
                  assert found.get('no.such.group', ()) == ()\n\
                  assert [e.name for e in found['markdown.extensions']] == ['abbr']\n\
                  assert 'markdown.extensions' in found.keys()\n\
                  assert [e.name for e in metadata.entry_points(group='markdown.extensions')] == ['abbr']\n";
    let Some((_host, loaded)) = host_with(dir.path(), &[("reads_entry_points.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { errors, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
}

/// Sublime Text 4132's enums (LSP) and our own `Default.paste_from_history` (Terminus).
#[test]
fn plugins_find_the_api_enums_and_default_modules() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = "import sublime\n\
                  from Default.paste_from_history import ClipboardHistory, g_clipboard_history\n\
                  assert sublime.RegionFlags.DRAW_NO_FILL | sublime.RegionFlags.HIDDEN == sublime.DRAW_NO_FILL | sublime.HIDDEN\n\
                  assert sublime.PopupFlags.HIDE_ON_MOUSE_MOVE_AWAY == sublime.HIDE_ON_MOUSE_MOVE_AWAY\n\
                  assert sublime.KindId.COLOR_LIGHT == 18 and sublime.QueryOperator.EQUAL == sublime.OP_EQUAL\n\
                  assert ClipboardHistory().caption('  two\\nlines') == 'twolines' and g_clipboard_history.LIST_LIMIT == 15\n\
                  import sys\n\
                  expected = '4203' if sys.version_info >= (3, 14) else '4201' if sys.version_info >= (3, 13) else '4200'\n\
                  assert sublime.version() == expected and sublime.arch() in ('x64', 'x32', 'arm64')\n\
                  folder = {'4203': 'python314', '4201': 'python313'}.get(expected, 'python38')\n\
                  assert [p for p in sys.path if p.endswith(('python314', 'python313', 'python38'))][0].endswith(folder), sys.path\n\
                  import warnings\n\
                  assert any(f[0] == 'ignore' and f[2] is DeprecationWarning for f in warnings.filters)\n";
    let Some((_host, loaded)) = host_with(dir.path(), &[("uses_new_api.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { errors, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    assert!(errors.is_empty(), "{errors:?}");
}

/// Package Control sets `post_handshake_auth` before every download, which Apple's
/// `/usr/bin/python3` (LibreSSL) refuses unless the host lets it.
#[test]
fn plugins_can_set_up_tls_contexts_like_package_control() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import ssl
import sublime
import sublime_plugin

class TlsCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        if hasattr(context, 'post_handshake_auth'):
            context.post_handshake_auth = True
        context.load_default_certs(ssl.Purpose.SERVER_AUTH)
        sublime.status_message("ready %s" % type(ssl.create_default_context()).__name__)
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("tls"));
    assert_eq!(editor_status(&editor), "ready SSLContext", "console:\n{}", editor.state().console);
}

fn editor_status(editor: &Editor) -> String {
    editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default()
}

#[test]
fn timers_and_the_async_thread_call_the_editor_after_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import threading
import sublime
import sublime_plugin

class LaterCommand(sublime_plugin.WindowCommand):
    def run(self):
        sublime.set_timeout(lambda: sublime.status_message("main"), 30)
        on_other_thread = lambda: threading.current_thread() is not threading.main_thread()
        sublime.set_timeout_async(lambda: sublime.status_message("async %s" % on_other_thread()))
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("later.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "later", &Value::Null, None).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None }, "timers wait: {log}");
    let mut messages = Vec::new();
    for _ in 0..2 {
        match next_idle(&mut host, &mut log) {
            PluginMessage::Call { function, args } if function == "status_message" => messages.push(args[0].clone()),
            other => panic!("expected status_message, got {other:?}"),
        }
        host.reply(Ok(Value::Null)).unwrap();
    }
    messages.sort_by_key(|m| m.to_string());
    assert_eq!(messages, [json!("async True"), json!("main")], "{log}");
}

const LISTENERS: &str = r#"
import sublime
import sublime_plugin

class Listener(sublime_plugin.EventListener):
    def on_modified(self, view):
        print("modified", view.id())

    def on_post_save_async(self, view):
        view.set_name("saved")

class Linter(sublime_plugin.ViewEventListener):
    @classmethod
    def is_applicable(cls, settings):
        return settings.get("lint", False)

    def on_modified(self):
        print("lint", self.view.id())
"#;

#[test]
fn event_listeners_hear_the_events_they_define() {
    let dir = tempfile::tempdir().unwrap();
    let Some((mut host, loaded)) = host_with(dir.path(), &[("listeners.py", LISTENERS)]) else {
        return;
    };
    let PluginMessage::Loaded { events, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    // `on_clone` too: view listeners must know the clones (they listen to primary views only).
    assert_eq!(events, ["on_post_save_async", "on_modified", "on_clone"]);

    // The first event of a view asks each ViewEventListener class whether it applies.
    let mut log = String::new();
    host.event("on_modified", 3).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "settings_get"), vec![json!(["view", 3]), json!("lint")]);
    host.reply(Ok(json!([true, true]))).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "modified 3\nlint 3\n");
    host.event("on_modified", 3).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None }, "the listener object is kept");

    // An _async listener runs after the event is done, on the async thread.
    host.event("on_post_save", 3).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    match next_idle(&mut host, &mut log) {
        PluginMessage::Call { function, args } => assert_eq!((function.as_str(), args), ("view_set_name", vec![json!(3), json!("saved")])),
        other => panic!("expected view_set_name, got {other:?}"),
    }
    host.reply(Ok(Value::Null)).unwrap();
}

/// A listener that defines only the `_async` form of a post-command method is still advertised to
/// the editor (it asks for the sync name), and the host queues that form after the command.
#[test]
fn an_async_only_post_command_listener_hears_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Listener(sublime_plugin.EventListener):
    def on_post_text_command_async(self, view, name, args):
        sublime.status_message("after " + name)
"#;
    let Some((mut host, loaded)) = host_with(dir.path(), &[("listener.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { events, .. } = loaded else {
        panic!("expected Loaded, got {loaded:?}");
    };
    assert!(events.iter().any(|e| e == "on_post_text_command"), "events: {events:?}");
    let mut log = String::new();
    host.query("on_post_text_command", Some(1), &json!(["shout", {}])).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Answer(Value::Null));
    match next_idle(&mut host, &mut log) {
        PluginMessage::Call { function, args } => {
            assert_eq!(function, "status_message");
            assert_eq!(args, vec![json!("after shout")]);
        }
        other => panic!("expected status_message, got {other:?}"),
    }
    host.reply(Ok(Value::Null)).unwrap();
}

#[test]
fn a_pre_save_listener_formats_the_file_before_it_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Format(sublime_plugin.EventListener):
    def on_pre_save(self, view):
        view.run_command("shout_all")

class ShoutAllCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        whole = sublime.Region(0, self.view.size())
        self.view.replace(edit, whole, self.view.substr(whole).upper())
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let file = dir.path().join("a.txt");
    fs::write(&file, "abc").unwrap();
    assert!(editor.open_file(&file));
    assert!(editor.save_active(false));
    assert_eq!(fs::read_to_string(&file).unwrap(), "ABC", "console:
{}", editor.state().console);
}

#[test]
fn a_built_in_command_in_on_pre_save_reaches_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Shout(sublime_plugin.EventListener):
    def on_pre_save(self, view):
        view.sel().clear()
        view.sel().add(sublime.Region(0, view.size()))
        view.run_command("upper_case")
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let file = dir.path().join("a.txt");
    fs::write(&file, "quiet").unwrap();
    assert!(editor.open_file(&file));
    let id = editor.active_document().unwrap().id;
    editor.text_for_editing(id).unwrap().push('!');
    assert!(editor.save_active(false), "{}", editor.state().console);
    assert_eq!(fs::read_to_string(&file).unwrap(), "QUIET!", "{}", editor.state().console);
    assert!(!editor.active_document().unwrap().is_dirty());
}

#[test]
fn panel_callbacks_reach_the_plugin_once() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

ITEMS = ["plain", ["caption", "detail one", "detail two"], sublime.QuickPanelItem("item", ["a", "b"], "note")]

class PickCommand(sublime_plugin.WindowCommand):
    def run(self):
        self.window.show_quick_panel(ITEMS, self.picked, sublime.WANT_EVENT, 2, placeholder="Which?")

    def picked(self, index, event):
        print("picked", index, sorted(event))

class AskCommand(sublime_plugin.WindowCommand):
    def run(self):
        self.window.show_input_panel("Name:", "bob", lambda text: print("done", text), None, lambda: print("cancelled"))
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("panels.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "pick", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "window_show_quick_panel");
    assert_eq!(
        args,
        vec![
            json!(1),
            json!([
                ["plain", [], "", "", false],
                ["caption", ["detail one", "detail two"], "", "", false],
                ["item", ["a", "b"], "note", "", true]
            ]),
            json!(4),
            json!(2),
            json!("Which?"),
            json!(1),
            json!(false),
            // The window the call is about.
            json!({ "__window__": 1 }),
        ]
    );
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });

    host.callback(1, "select", &json!([2])).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    host.callback(1, "select", &json!([0])).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "picked 2 ['modifier_keys']\n", "a closed panel's callback runs once");

    host.run(PluginCommandKind::Window, "ask", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "window_show_input_panel");
    assert_eq!(args, vec![json!(1), json!("Name:"), json!("bob"), json!(2), json!(false), json!({ "__window__": 1 })]);
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    host.callback(2, "done", &json!(["alice"])).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert!(log.ends_with("done alice\n"), "{log}");
}

#[test]
fn dialogs_resources_and_settings_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class ProbeCommand(sublime_plugin.WindowCommand):
    def run(self):
        print("ok", sublime.ok_cancel_dialog("Go?"))
        print("bin", sublime.load_binary_resource("Packages/T/x.bin"))
        try:
            sublime.load_resource("Packages/T/nope")
        except FileNotFoundError as error:
            print("missing", error)
        settings = sublime.load_settings("T.sublime-settings")
        settings.add_on_change("t", lambda: print("changed"))
        sublime.load_settings("T.sublime-settings").set("a", 1)
        settings.clear_on_change("t")
        settings.set("a", 2)
        print(sublime.decode_value('{"a": [1, 2,], // note\n "b": "x//y,]" /* z */,}'))
        sublime.open_dialog(lambda path: sublime.status_message("picked %s" % path))
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("probe.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "probe", &Value::Null, None).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "ok_cancel_dialog"), vec![json!("Go?"), json!(""), json!("")]);
    host.reply(Ok(json!(true))).unwrap();
    expect_call(&mut host, &mut log, "load_binary_resource");
    host.reply(Ok(json!("00ff"))).unwrap();
    expect_call(&mut host, &mut log, "load_resource");
    host.reply(Err("resource not found: Packages/T/nope".into())).unwrap();
    assert_eq!(expect_call(&mut host, &mut log, "settings_watch"), vec![json!(["file", "T.sublime-settings"])]);
    host.reply(Ok(Value::Null)).unwrap();
    for _ in 0..2 {
        expect_call(&mut host, &mut log, "settings_set");
        host.reply(Ok(Value::Null)).unwrap();
    }
    assert_eq!(expect_call(&mut host, &mut log, "open_dialog"), vec![Value::Null, json!([])]);
    host.reply(Ok(json!(["/a.txt", "/b.txt"]))).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None }, "{log}");
    assert_eq!(
        log,
        "ok True\nbin b'\\x00\\xff'\nmissing resource not found: Packages/T/nope\nchanged\n{'a': [1, 2], 'b': 'x//y,]'}\n",
        "the callback heard only the change made before clear_on_change"
    );
    match next_idle(&mut host, &mut log) {
        PluginMessage::Call { function, args } if function == "status_message" => {
            assert_eq!(args, vec![json!("picked /a.txt")], "one file without multi_select, after the command")
        }
        other => panic!("expected status_message, got {other:?}"),
    }
}

#[test]
fn listeners_answer_queries() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class Contexts(sublime_plugin.EventListener):
    def on_query_context(self, view, key, operator, operand, match_all):
        if key != "my.ctx":
            return None
        return (operator == sublime.OP_EQUAL) == (operand == view.id())

    def on_text_command(self, view, name, args):
        if name == "title_case":
            return ("upper_case", {"from": name})

    def on_post_window_command(self, window, name, args):
        print("ran", name, args, window.id())

class Broken(sublime_plugin.ViewEventListener):
    def on_text_command(self, name, args):
        raise ValueError("ignored")
"#;
    let Some((mut host, loaded)) = host_with(dir.path(), &[("hooks.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { events, .. } = loaded else {
        panic!("expected loaded, got {loaded:?}");
    };
    for event in ["on_query_context", "on_text_command", "on_post_window_command"] {
        assert!(events.contains(&event.to_string()), "{events:?}");
    }
    let mut log = String::new();
    let mut ask = |host: &mut PythonPluginHost, name: &str, view: Option<u64>, args: Value| {
        host.query(name, view, &args).unwrap();
        next(host, &mut log)
    };
    assert_eq!(ask(&mut host, "on_query_context", Some(7), json!(["my.ctx", "equal", 7, false])), PluginMessage::Answer(json!(true)));
    assert_eq!(ask(&mut host, "on_query_context", Some(7), json!(["my.ctx", "not_equal", 7, false])), PluginMessage::Answer(json!(false)));
    assert_eq!(ask(&mut host, "on_query_context", Some(7), json!(["other", "equal", 7, false])), PluginMessage::Answer(Value::Null));
    assert_eq!(
        ask(&mut host, "on_text_command", Some(7), json!(["title_case", null])),
        PluginMessage::Answer(json!(["upper_case", {"from": "title_case"}]))
    );
    assert_eq!(ask(&mut host, "on_text_command", Some(7), json!(["lower_case", null])), PluginMessage::Answer(Value::Null));
    assert_eq!(ask(&mut host, "on_post_window_command", None, json!(["new_file", null])), PluginMessage::Answer(Value::Null));
    assert!(log.contains("ran new_file None 1"), "{log}");
    assert!(log.contains("ValueError: ignored"), "a failing listener's traceback goes to the console: {log}");
}

#[test]
fn commands_say_how_they_show_in_menus() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin

class PlainCommand(sublime_plugin.WindowCommand):
    def run(self):
        pass

class FancyCommand(sublime_plugin.WindowCommand):
    def run(self, on=False):
        pass

    def is_visible(self, on=False):
        return on

    def is_enabled(self):
        return False

    def is_checked(self, on=False):
        return True

    def description(self, on=False):
        return "Fancy %s" % on

    def want_event(self):
        return True

class BrokenCommand(sublime_plugin.WindowCommand):
    def run(self):
        pass

    def is_visible(self):
        raise ValueError("ignored")
"#;
    let Some((mut host, loaded)) = host_with(dir.path(), &[("meta.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { events, .. } = loaded else {
        panic!("expected loaded, got {loaded:?}");
    };
    assert!(events.contains(&"command_states".to_string()), "{events:?}");
    assert!(!events.contains(&"command_input".to_string()), "no command has input(): {events:?}");

    let mut log = String::new();
    let asked = json!([[["plain", null], ["fancy", {"on": true}], ["fancy", null], ["broken", null]]]);
    host.query("command_states", None, &asked).unwrap();
    let default = json!({"visible": true, "enabled": true, "checked": false, "description": null, "want_event": false});
    assert_eq!(
        next(&mut host, &mut log),
        PluginMessage::Answer(json!([
            default,
            {"visible": true, "enabled": false, "checked": true, "description": "Fancy True", "want_event": true},
            {"visible": false, "enabled": false, "checked": true, "description": "Fancy False", "want_event": true},
            default,
        ]))
    );
    assert!(log.contains("ValueError: ignored"), "{log}");
}

#[test]
fn input_handlers_ask_for_the_arguments_and_run_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class GreetCommand(sublime_plugin.WindowCommand):
    def run(self, who, words):
        print("hello", who, words)

    def input(self, args):
        return WhoInputHandler() if "who" not in args else None

class WhoInputHandler(sublime_plugin.ListInputHandler):
    def list_items(self):
        return ["Ana", ("Bob the builder", "bob"), sublime.ListInputItem("Zoe", "zoe")]

    def next_input(self, args):
        return WordsInputHandler()

class WordsInputHandler(sublime_plugin.TextInputHandler):
    def placeholder(self):
        return "Words"

    def validate(self, text):
        return text != "bad"
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run_from_palette(&mut editor, &mut view, Command::with_args("greet", json!({ "who": "me" })));
    assert_eq!(view.take_commands().len(), 1, "with the argument given it just runs");

    command_controller::run_from_palette(&mut editor, &mut view, Command::new("greet"));
    assert!(view.take_commands().is_empty(), "{}", editor.state().console);
    let items = &editor.plugin_quick_panel().expect("the list").items;
    assert_eq!(items[1].caption, "Bob the builder");
    assert_eq!(items[2].caption, "Zoe");

    command_controller::answer_plugin_quick_panel(&mut editor, &mut view, Some(1));
    assert!(view.input_panel.is_some());
    assert_eq!(editor.plugin_input_panel().expect("the text field").caption, "Words");

    command_controller::change_plugin_input(&mut editor, &mut view, "bad");
    command_controller::finish_plugin_input(&mut editor, &mut view, true);
    assert!(view.input_panel.is_some(), "asked again after a failed validation");
    assert_eq!(editor.plugin_input_panel().unwrap().document.text, "bad");
    command_controller::change_plugin_input(&mut editor, &mut view, "good day");
    command_controller::finish_plugin_input(&mut editor, &mut view, true);
    assert!(view.input_panel.is_none());
    assert!(editor.state().console.contains("hello bob good day"), "{}", editor.state().console);
}

#[test]
fn add_on_change_hears_the_editors_own_changes() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime

def plugin_loaded():
    settings = sublime.load_settings("Preferences.sublime-settings")
    settings.add_on_change("t", lambda: print("font", settings.get("font_size")))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    editor.set_setting("font_size", json!(99));
    editor.poll_plugins(&mut |_, _| {});
    assert!(editor.state().console.contains("font 99"), "{}", editor.state().console);
}

#[test]
fn a_missing_interpreter_is_reported() {
    let error = find_interpreter(Some("/no/such/python")).unwrap_err();
    assert!(error.contains("python_interpreter"), "{error}");
}

/// The support modules need `importlib.metadata`, so a `python3` older than 3.8 is turned away
/// with a message instead of failing later inside the host.
#[cfg(unix)]
#[test]
fn an_interpreter_older_than_python_3_8_is_reported() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let python = dir.path().join("python3");
    fs::write(&python, "#!/bin/sh\necho 3.7\n").unwrap();
    fs::set_permissions(&python, fs::Permissions::from_mode(0o755)).unwrap();
    let error = find_interpreter(Some(python.to_str().unwrap())).unwrap_err();
    assert!(error.contains("3.8"), "{error}");
}

/// The Python an installer puts next to the editor is found from the executable's path:
/// beside it (Windows, portable archives), in the macOS app's `Resources`, in `lib` on Linux.
#[test]
fn the_bundled_python_is_found_next_to_the_executable() {
    let program = if cfg!(windows) { "python.exe" } else { "bin/python3" };
    for (executable, python) in [
        ("codelibre_text", "python"),
        ("CodeLibre Text.app/Contents/MacOS/codelibre_text", "CodeLibre Text.app/Contents/Resources/python"),
        ("usr/bin/codelibre_text", "usr/lib/codelibre-text/python"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join(executable);
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        assert_eq!(bundled_interpreter(&executable), None);
        let python = dir.path().join(python).join(program);
        fs::create_dir_all(python.parent().unwrap()).unwrap();
        fs::write(&python, "").unwrap();
        let found = bundled_interpreter(&executable).expect("bundled python");
        assert_eq!(fs::canonicalize(found).unwrap(), fs::canonicalize(&python).unwrap());
    }
}

#[test]
fn text_change_listeners_hear_the_edits() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin

class Watch(sublime_plugin.TextChangeListener):
    def on_text_changed(self, changes):
        for c in changes:
            print("changed", c.a.pt, c.a.row, c.b.pt, c.str, c.len_utf8, self.buffer.id())

    def on_revert(self):
        print("reverted")
"#;
    let Some((mut host, loaded)) = host_with(dir.path(), &[("watch.py", plugin)]) else {
        return;
    };
    let PluginMessage::Loaded { events, .. } = loaded else {
        panic!("expected loaded, got {loaded:?}");
    };
    assert!(events.contains(&"on_text_changed".to_string()), "{events:?}");
    let mut log = String::new();
    let changes = json!([[{ "a": [5, 1, 2], "b": [6, 1, 3], "text": "é" }]]);
    host.query("on_text_changed", Some(7), &changes).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Answer(Value::Null));
    host.event("on_revert", 7).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert_eq!(log, "changed 5 1 6 é 2 7\nreverted\n");
}

#[test]
fn quick_panel_items_send_their_kind_letter() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class PickCommand(sublime_plugin.WindowCommand):
    def run(self):
        item = sublime.QuickPanelItem("one", annotation="note", kind=sublime.KIND_FUNCTION)
        self.window.show_quick_panel([item], None, sublime.KEEP_OPEN_ON_FOCUS_LOST)
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("pick.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "pick", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "window_show_quick_panel");
    assert_eq!(args[1], json!([["one", [], "note", "f", true]]));
    assert_eq!(args[2], json!(2));
}

#[test]
fn quick_panel_item_details_are_sent_as_minihtml_and_annotations_as_text() {
    let dir = tempfile::tempdir().unwrap();
    // As Package Control 4 lists packages.
    let plugin = r#"
import sublime
import sublime_plugin

class PickCommand(sublime_plugin.WindowCommand):
    def run(self):
        item = sublime.QuickPanelItem("1337", [
            "<em>1337 - A &amp; B for &#x27;Sublime&#x27;</em>",
            "<em>install v2.0.0</em> <a href=\"https://github.com/x/y\">github.com/x/y</a><br>more",
        ], "<i>Updated on Wed Jul 04, 2018</i>")
        plain = ["caption", "<b>kept</b> as written"]
        self.window.show_quick_panel([item, plain], None)
"#;
    let Some((mut host, _)) = host_with(dir.path(), &[("pick.py", plugin)]) else {
        return;
    };
    let mut log = String::new();
    host.run(PluginCommandKind::Window, "pick", &Value::Null, None).unwrap();
    let args = expect_call(&mut host, &mut log, "window_show_quick_panel");
    assert_eq!(
        args[1],
        json!([
            [
                "1337",
                [
                    "<em>1337 - A &amp; B for &#x27;Sublime&#x27;</em>",
                    "<em>install v2.0.0</em> <a href=\"https://github.com/x/y\">github.com/x/y</a><br>more"
                ],
                "Updated on Wed Jul 04, 2018",
                "",
                true
            ],
            ["caption", ["<b>kept</b> as written"], "", "", false],
        ])
    );
}

#[test]
fn a_build_from_a_package_runs_for_real_and_its_error_opens_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let Some(mut editor) = editor_with(dir.path(), "") else {
        return;
    };
    let build = r#"{ // Sublime's JSON, with a comment
        "selector": "source.python",
        "shell_cmd": "echo compiling && echo $file_name:2: boom && exit 1",
        "file_regex": "^(\\S+):(\\d+):(\\d*) (.*?)\\s*$",
    }"#;
    fs::write(dir.path().join("Packages").join("MyPkg").join("Echo.sublime-build"), build).unwrap();
    let file = dir.path().join("script.py");
    fs::write(&file, "one\ntwo\n").unwrap();
    assert!(editor.open_file(&file));

    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("build"));
    assert!(editor.build_is_running(), "{}", editor_status(&editor));
    let deadline = Instant::now() + Duration::from_secs(20);
    while editor.build_is_running() {
        assert!(Instant::now() < deadline, "the build did not end");
        editor.poll_builds();
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = editor.shown_output_panel().expect("the results").document.text.clone();
    assert!(output.starts_with("compiling") && output.contains("\nscript.py:2: boom"), "{output}");
    assert!(output.contains("with exit code 1]"), "{output}");
    assert_eq!(editor_status(&editor), "Build finished with 1 error");

    command_controller::run(&mut editor, &mut view, &Command::new("next_result"));
    assert_eq!(editor.active_document().unwrap().selection().head, 4);
    assert_eq!(editor_status(&editor), "boom");
}

#[test]
fn the_input_panel_returns_a_view_plugins_can_change() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class RenameCommand(sublime_plugin.WindowCommand):
    def run(self):
        panel = self.window.show_input_panel("Name:", "report.txt", None, None, None)
        panel.sel().clear()
        panel.sel().add(sublime.Region(0, 6))
        panel.settings().set("mine", 1)
        print("panel", panel.substr(sublime.Region(0, panel.size())), panel.settings().get("mine"), self.window.active_panel())
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run(&mut editor, &mut view, &Command::new("rename"));
    assert!(editor.state().console.contains("panel report.txt 1 input"), "{}", editor.state().console);
    let document = &editor.plugin_input_panel().expect("the panel").document;
    assert_eq!(document.selection(), entities::selection::Selection::new(0, 6));
}

#[test]
fn expand_variables_goes_through_lists_and_dicts() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class ExpandCommand(sublime_plugin.WindowCommand):
    def run(self):
        print(sublime.expand_variables({"cmd": ["$a", 1], "b": "${a}!"}, {"a": "x"}))
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    command_controller::run(&mut editor, &mut ViewState::default(), &Command::new("expand"));
    assert!(editor.state().console.contains("{'cmd': ['x', 1], 'b': 'x!'}"), "{}", editor.state().console);
}

#[test]
fn an_input_panel_after_a_list_keeps_the_keyboard() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime_plugin

class AskCommand(sublime_plugin.WindowCommand):
    def run(self, who, words):
        pass

    def input(self, args):
        return WhoInputHandler()

class WhoInputHandler(sublime_plugin.ListInputHandler):
    def list_items(self):
        return ["a"]

    def next_input(self, args):
        return WordsInputHandler()

class WordsInputHandler(sublime_plugin.TextInputHandler):
    pass
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();
    command_controller::run_from_palette(&mut editor, &mut view, Command::new("ask"));
    command_controller::answer_plugin_quick_panel(&mut editor, &mut view, Some(0));
    assert!(view.input_panel.is_some());
    assert!(!view.focus_editor, "the text would take the keys typed for the panel");
}

/// A plugin of package `name` that says when it loads and unloads, and counts its runs.
fn counting_plugin(name: &str, version: u32) -> String {
    format!(
        r#"
import sublime_plugin

RUNS = 0

def plugin_loaded():
    print("loaded {name} {version}")

def plugin_unloaded():
    print("unloaded {name}")

class {name}CountCommand(sublime_plugin.ApplicationCommand):
    def run(self):
        global RUNS
        RUNS += 1
        print("{name} {version} ran %d" % RUNS)
"#
    )
}

/// Loads `names` from `dir/Packages` and returns the commands and what the plugins printed.
fn load_packages(host: &mut PythonPluginHost, dir: &Path, names: &[&str]) -> (bool, Vec<String>, String) {
    let started = host
        .load(&PluginLoad {
            packages: names
                .iter()
                .map(|name| PluginPackage { name: (*name).into(), dirs: vec![dir.join("Packages").join(name)] })
                .collect(),
            dependencies: Vec::new(),
            packages_path: dir.join("Packages"),
            archives: Vec::new(),
            shipped_packages: None,
            interpreter: None,
        })
        .unwrap();
    let mut log = String::new();
    let PluginMessage::Loaded { commands, errors, .. } = next(host, &mut log) else {
        panic!("expected Loaded; console:\n{log}");
    };
    assert!(errors.is_empty(), "{errors:?}");
    (started, commands.into_iter().map(|(name, _)| name).collect(), log)
}

fn run_and_log(host: &mut PythonPluginHost, command: &str) -> String {
    let mut log = String::new();
    host.run(PluginCommandKind::Application, command, &Value::Null, None).unwrap();
    assert_eq!(next(host, &mut log), PluginMessage::Done { error: None });
    log
}

#[test]
fn reloading_loads_only_the_packages_that_changed() {
    if find_interpreter(None).is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, version: u32| {
        let package = dir.path().join("Packages").join(name);
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("plugin.py"), counting_plugin(name, version)).unwrap();
    };
    write("Kept", 1);
    write("Changed", 1);
    let mut host = PythonPluginHost::new(dir.path().join("Lib").join("python"));
    let (started, commands, log) = load_packages(&mut host, dir.path(), &["Kept", "Changed"]);
    assert!(started);
    assert_eq!(commands, vec!["changed_count", "kept_count"]);
    assert_eq!(log, "loaded Kept 1\nloaded Changed 1\n");
    assert_eq!(run_and_log(&mut host, "kept_count"), "Kept 1 ran 1\n");

    // One package changed, one was added: the host goes on, the unchanged package is not touched.
    write("Changed", 22);
    write("Added", 1);
    let (started, commands, log) = load_packages(&mut host, dir.path(), &["Added", "Changed", "Kept"]);
    assert!(!started, "the host kept running");
    assert_eq!(commands, vec!["added_count", "changed_count", "kept_count"]);
    assert_eq!(log, "unloaded Changed\nloaded Added 1\nloaded Changed 22\n");
    assert_eq!(run_and_log(&mut host, "kept_count"), "Kept 1 ran 2\n", "its module state is kept");
    assert_eq!(run_and_log(&mut host, "changed_count"), "Changed 22 ran 1\n");

    // A package left out (removed, or in `ignored_packages`) is unloaded and its commands go.
    let (_, commands, log) = load_packages(&mut host, dir.path(), &["Changed", "Kept"]);
    assert_eq!(commands, vec!["changed_count", "kept_count"]);
    assert_eq!(log, "unloaded Added\n");
    host.stop();
}

#[test]
fn plugins_split_the_window_and_move_views_between_groups() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class SplitCommand(sublime_plugin.WindowCommand):
    def run(self):
        w = self.window
        first = w.active_view()
        second = w.new_file()
        w.set_layout({"cols": [0.0, 0.5, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 1, 1], [1, 0, 2, 1]]})
        w.set_view_index(second, 1, 0)
        print("groups", w.num_groups(), w.active_group(), w.get_view_index(second), second.sheet().group())
        print("in group 0", [v.id() for v in w.views_in_group(0)] == [first.id()])
        w.focus_group(0)
        print("active", w.active_view_in_group(0).id() == first.id(), w.active_group())
        w.move_sheets_to_group([first.sheet()], 1)
        print("empty", w.views_in_group(0), w.active_view_in_group(0), w.get_layout()["cells"])
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    command_controller::run(&mut editor, &mut ViewState::default(), &Command::new("split"));
    let console = &editor.state().console;
    assert!(console.contains("groups 2 1 (1, 0) 1"), "{console}");
    assert!(console.contains("in group 0 True"), "{console}");
    assert!(console.contains("active True 0"), "{console}");
    assert!(console.contains("empty [] None [[0, 0, 1, 1], [1, 0, 2, 1]]"), "{console}");
    assert_eq!(editor.active_group(), 1, "the moved view took the focus along");
}

/// Typing is a command for the plugins: `on_text_command` sees `insert`, may replace the
/// characters, and `on_post_text_command` is told once the widget typed them.
#[test]
fn a_plugin_hears_typing_and_may_replace_the_characters() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class PairCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        self.view.insert(edit, self.view.sel()[0].b, "()")

class TypingListener(sublime_plugin.EventListener):
    def on_text_command(self, view, command_name, args):
        if command_name == "insert" and args.get("characters") == "(":
            return ("pair", None)
        return None

    def on_post_text_command(self, view, command_name, args):
        sublime.status_message("after " + command_name)
"#;
    let Some(mut editor) = editor_with(dir.path(), plugin) else {
        return;
    };
    let mut view = ViewState::default();

    // The listener replaces the typed character with a command of its own.
    let typed = Command::with_args("insert", json!({ "characters": "(" }));
    assert!(command_controller::hooked_widget_input(&mut editor, &mut view, typed, interface_adapters::view_state::WidgetEdit::Insert("(".into())));
    assert_eq!(editor.active_document().unwrap().text, "()");
    assert_eq!(editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default(), "after pair");

    // Nobody replaces this one: the widget types it, and the listener hears the end afterwards.
    let typed = Command::with_args("insert", json!({ "characters": "x" }));
    let replay = interface_adapters::view_state::WidgetEdit::Insert("x".into());
    assert!(command_controller::hooked_widget_input(&mut editor, &mut view, typed, replay.clone()));
    assert_eq!(view.take_requests(), [interface_adapters::view_state::ViewRequest::WidgetEdit(replay)]);
    command_controller::poll_plugins(&mut editor, &mut view);
    command_controller::poll_plugins(&mut editor, &mut view);
    assert_eq!(editor.state().status.as_ref().map(|s| s.text.clone()).unwrap_or_default(), "after insert");
}
