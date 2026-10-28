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
    folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell, std_file_system::StdFileSystem,
    syntect_syntaxes::SyntectSyntaxes, system_clipboard::SystemClipboard,
};
use codelibre_text::gateways::python_plugin_host::{find_interpreter, PythonPluginHost};
use codelibre_text::syntax_engine::SyntaxEngine;
use entities::command::Command;
use interface_adapters::controllers::command_controller;
use interface_adapters::gateways::session_file::SessionFileGateway;
use interface_adapters::gateways::sublime_resources::SublimeResourceGateway;
use interface_adapters::view_state::ViewState;
use serde_json::{json, Value};
use use_cases::gateways::{PackagePaths, PluginCommandKind, PluginHostGateway, PluginLoad, PluginMessage, PluginPackage};
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
        packages_path: dir.join("Packages"),
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
    if let Err(e) = find_interpreter(None) {
        eprintln!("skipped: {e}");
        return None;
    }
    let packages = dir.join("Packages");
    fs::create_dir_all(packages.join("MyPkg")).unwrap();
    fs::write(packages.join("MyPkg").join("plugin.py"), plugin).unwrap();
    let paths = PackagePaths { packages, sublime_packages: None };
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
        plugins: Box::new(PythonPluginHost::new(dir.join("Lib").join("python"))),
    });
    editor.start(Vec::new());
    assert!(editor.state().load_errors.is_empty(), "{:?}", editor.state().load_errors);
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
    assert_eq!(events, ["on_post_save_async", "on_modified"]);

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
            json!([["plain", "", ""], ["caption", "detail one · detail two", ""], ["item", "a · b", "note"]]),
            json!(4),
            json!(2),
            json!("Which?"),
            json!(1),
            json!(false),
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
    assert_eq!(args, vec![json!(1), json!("Name:"), json!("bob"), json!(2), json!(false)]);
    host.reply(Ok(Value::Null)).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    host.callback(2, "done", &json!(["alice"])).unwrap();
    assert_eq!(next(&mut host, &mut log), PluginMessage::Done { error: None });
    assert!(log.ends_with("done alice\n"), "{log}");
}

#[test]
fn a_missing_interpreter_is_reported() {
    let error = find_interpreter(Some("/no/such/python")).unwrap_err();
    assert!(error.contains("python_interpreter"), "{error}");
}
