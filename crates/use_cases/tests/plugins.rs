//! Tests for the plugin use cases (`use_cases::plugins`), with a scripted plugin host.

use entities::command::Command;
use serde_json::{json, Value};
use test_support::{active_text, plugin_call, status, World, PACKAGES};
use use_cases::gateways::PluginCommandKind::{Text, Window};
use use_cases::gateways::PluginMessage;
use use_cases::Editor;

fn done() -> PluginMessage {
    PluginMessage::Done { error: None }
}

/// Runs a command the way the command controller does, recording the built-in commands that
/// plugins ask for. Returns false if no plugin defines it.
fn run(editor: &mut Editor, name: &str, builtins: &mut Vec<Command>) -> bool {
    editor.run_plugin_command(&Command::new(name), &mut |_, command| builtins.push(command.clone()))
}

#[test]
fn without_plugin_files_no_host_is_started() {
    let world = World::new().with_file(&format!("{PACKAGES}/Theme/x.sublime-settings"), "{}");
    let editor = world.editor();
    assert!(world.plugins.loads.borrow().is_empty());
    assert!(!editor.is_plugin_command("anything"));
}

#[test]
fn packages_with_python_files_are_loaded_user_last() {
    let world = World::new()
        .with_file(&format!("{PACKAGES}/User/mine.py"), "")
        .with_file(&format!("{PACKAGES}/Zeta/z.py"), "")
        .with_file(&format!("{PACKAGES}/Alpha/a.py"), "")
        .with_file(&format!("{PACKAGES}/Docs/readme.md"), "");
    world.plugins.then([PluginMessage::Loaded { commands: vec![("hello".into(), Text)], errors: vec![] }]);
    let editor = world.editor();
    let loads = world.plugins.loads.borrow();
    let names: Vec<&str> = loads[0].packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "Zeta", "User"]);
    assert_eq!(loads[0].packages_path.to_str(), Some(PACKAGES));
    assert_eq!(loads[0].interpreter, None);
    assert!(editor.is_plugin_command("hello"));
}

#[test]
fn the_python_plugins_setting_turns_plugins_off_and_python_interpreter_is_passed_on() {
    let world = World::new()
        .with_plugin_commands(&[("hello", Text)])
        .with_preferences(json!({"python_plugins": false}));
    let editor = world.editor();
    assert!(world.plugins.loads.borrow().is_empty());
    assert!(!editor.is_plugin_command("hello"));

    let world = World::new()
        .with_plugin_commands(&[])
        .with_preferences(json!({"python_interpreter": "C:/Python/python.exe"}));
    world.editor();
    assert_eq!(world.plugins.loads.borrow()[0].interpreter.as_deref(), Some("C:/Python/python.exe"));
}

#[test]
fn load_problems_are_package_load_errors_and_go_to_the_console() {
    let world = World::new().with_file(&format!("{PACKAGES}/P/p.py"), "");
    *world.plugins.load_error.borrow_mut() = Some("no Python 3 interpreter found".into());
    let editor = world.editor();
    assert_eq!(editor.state().load_errors, ["Python plugins: no Python 3 interpreter found"]);
    assert!(status(&editor).contains("1 package resource failed to load"));

    let world = World::new().with_file(&format!("{PACKAGES}/P/p.py"), "");
    world.plugins.then([PluginMessage::Loaded { commands: vec![], errors: vec!["p.py: SyntaxError".into()] }]);
    let mut editor = world.editor();
    assert_eq!(editor.state().load_errors, ["p.py: SyntaxError"]);
    assert!(editor.state().console.contains("p.py: SyntaxError"));

    // Reloading replaces the previous plugin errors.
    world.plugins.then([PluginMessage::Loaded { commands: vec![("ok".into(), Text)], errors: vec![] }]);
    editor.reload_plugins();
    assert!(editor.state().load_errors.is_empty());
    assert_eq!(status(&editor), "Plugins reloaded: 1 command");
}

#[test]
fn a_text_command_edits_the_active_view_through_api_calls() {
    let world = World::new().with_plugin_commands(&[("hello", Text)]);
    let mut editor = world.editor();
    editor.insert_at_selections("abc");
    let id = editor.active_document().unwrap().id;
    world.plugins.then([plugin_call("view_insert", json!([id, 1, "XY"])), done()]);

    assert!(run(&mut editor, "hello", &mut Vec::new()));
    assert_eq!(active_text(&editor), "aXYbc");
    assert_eq!(world.plugins.replies.borrow().as_slice(), [Ok(json!(2))]);
    assert_eq!(world.plugins.runs.borrow()[0], (Text, "hello".to_string(), Value::Null, Some(id)));
}

#[test]
fn unknown_commands_are_not_plugin_commands() {
    let world = World::new().with_plugin_commands(&[("hello", Text)]);
    let mut editor = world.editor();
    assert!(!run(&mut editor, "goodbye", &mut Vec::new()));
    assert!(world.plugins.runs.borrow().is_empty());
}

#[test]
fn a_failing_command_shows_the_exception_and_logs_the_traceback() {
    let world = World::new().with_plugin_commands(&[("hello", Text)]);
    let mut editor = world.editor();
    let traceback = "Traceback (most recent call last):\n  File \"p.py\", line 3\nValueError: boom\n";
    world.plugins.then([PluginMessage::Log("working\n".into()), PluginMessage::Done { error: Some(traceback.into()) }]);
    run(&mut editor, "hello", &mut Vec::new());
    assert_eq!(status(&editor), "hello failed: ValueError: boom (View > Show Console)");
    assert!(editor.state().console.contains("working\nhello: Traceback"));
}

#[test]
fn a_host_that_stops_answering_is_reported() {
    let world = World::new().with_plugin_commands(&[("hello", Text)]);
    let mut editor = world.editor();
    run(&mut editor, "hello", &mut Vec::new());
    assert_eq!(status(&editor), "hello failed: the plugin host stopped (View > Show Console)");
}

#[test]
fn plugins_run_built_in_commands_through_the_controller() {
    let world = World::new().with_plugin_commands(&[("hello", Window)]);
    let mut editor = world.editor();
    world.plugins.then([plugin_call("run_command", json!(["window", 1, "upper_case", {"x": 1}])), done()]);
    let mut builtins = Vec::new();
    run(&mut editor, "hello", &mut builtins);
    assert_eq!(builtins, [Command::with_args("upper_case", json!({"x": 1}))]);
    assert_eq!(world.plugins.replies.borrow().as_slice(), [Ok(Value::Null)]);
}

#[test]
fn plugins_run_other_plugin_commands_while_they_wait() {
    let world = World::new().with_plugin_commands(&[("outer", Window), ("inner", Window)]);
    let mut editor = world.editor();
    // outer calls run_command("inner"); inner finishes; then outer finishes.
    world.plugins.then([plugin_call("run_command", json!(["window", 1, "inner", null])), done(), done()]);
    let mut builtins = Vec::new();
    run(&mut editor, "outer", &mut builtins);
    let runs: Vec<String> = world.plugins.runs.borrow().iter().map(|r| r.1.clone()).collect();
    assert_eq!(runs, ["outer", "inner"]);
    assert!(builtins.is_empty());
    assert!(!status(&editor).contains("failed"), "{}", status(&editor));
}

#[test]
fn a_view_command_runs_with_that_view_active() {
    let world = World::new().with_plugin_commands(&[("hello", Window)]);
    let mut editor = world.editor();
    let first = editor.active_document().unwrap().id;
    editor.new_file();
    let second = editor.active_document().unwrap().id;
    world.plugins.then([plugin_call("run_command", json!(["view", first, "save", null])), done()]);
    let mut active_during = None;
    editor.run_plugin_command(&Command::new("hello"), &mut |editor, _| {
        active_during = editor.active_document().map(|d| d.id);
    });
    assert_eq!(active_during, Some(first));
    assert_eq!(editor.active_document().unwrap().id, second, "the previous view comes back");
}

#[test]
fn the_console_opens_as_a_read_only_tab_and_is_refreshed() {
    let world = World::new().with_plugin_commands(&[("hello", Text)]);
    let mut editor = world.editor();
    editor.log("first\n");
    editor.show_console();
    let doc = editor.active_document().unwrap();
    assert_eq!((doc.name(), doc.text.as_str(), doc.read_only), ("Console".to_string(), "first\n", true));
    assert_eq!(doc.selection().head, 6, "scrolled to the end");

    editor.new_file();
    editor.log("second\n");
    editor.show_console();
    assert_eq!(active_text(&editor), "first\nsecond\n");
    assert_eq!(editor.state().documents.iter().filter(|d| d.name() == "Console").count(), 1);
}
