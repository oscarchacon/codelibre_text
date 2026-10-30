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
    child_processes::ChildProcesses, folder_index::FolderIndex, native_dialogs::NativeDialogs, os_shell::OsShell, std_file_system::StdFileSystem,
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
        plugins: Box::new(PythonPluginHost::new(dir.join("Lib").join("python"))),
    });
    editor.set_event_command_runner(command_controller::run_during_event);
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

#[test]
fn regions_statuses_and_show_reach_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let plugin = r#"
import sublime
import sublime_plugin

class MarkCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        self.view.insert(edit, 0, "alpha beta\ngamma")
        self.view.add_regions("words", [sublime.Region(6, 10), 0], "region.redish", "dot",
                              sublime.DRAW_NO_FILL | sublime.DRAW_NO_OUTLINE | sublime.DRAW_SQUIGGLY_UNDERLINE,
                              annotations=["unused"])
        self.view.set_status("count", "2 marks")
        self.view.sel().clear()
        self.view.sel().add(sublime.Region(11, 12))
        self.view.show_at_center(self.view.sel())
        sublime.status_message("%s %s" % (self.view.get_regions("words"), self.view.get_status("count")))
"#;
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
    assert_eq!(doc.show_request(), Some(entities::document::ShowRequest { point: 11, center: true }));
    assert_eq!(editor.plugin_statuses(doc.id), ["2 marks"]);
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
            json!([["plain", "", ""], ["caption", "detail one · detail two", ""], ["item", "a · b", "note", ""]]),
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
    let default = json!({"visible": true, "enabled": true, "checked": false, "description": null});
    assert_eq!(
        next(&mut host, &mut log),
        PluginMessage::Answer(json!([
            default,
            {"visible": true, "enabled": false, "checked": true, "description": "Fancy True"},
            {"visible": false, "enabled": false, "checked": true, "description": "Fancy False"},
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
    assert_eq!(args[1], json!([["one", "", "note", "f"]]));
    assert_eq!(args[2], json!(2));
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
