//! Tests for output panels (`use_cases::output_panels`), driven like plugins drive them: the
//! scripted plugin host calls `window_create_output_panel` and friends.

use entities::command::Command;
use serde_json::{json, Value};
use test_support::{active_text, open, plugin_call, status, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::{PluginMessage, ProcessEvent};
use use_cases::Editor;

/// The editor's answer when a plugin calls `function(args...)`.
fn call(world: &World, editor: &mut Editor, function: &str, args: Value) -> Result<Value, String> {
    world.plugins.then([plugin_call(function, args), PluginMessage::Done { error: None }]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    world.plugins.replies.borrow_mut().pop().expect("the editor answered")
}

fn setup() -> (World, Editor, u64) {
    let world = World::new().with_plugin_commands(&[("probe", Text)]);
    let mut editor = world.editor();
    let panel = call(&world, &mut editor, "window_create_output_panel", json!([1, "exec"])).unwrap();
    let id = panel.as_u64().expect("a view id");
    (world, editor, id)
}

fn panel_text(editor: &Editor) -> String {
    editor.shown_output_panel().map(|p| p.document.text.clone()).unwrap_or_default()
}

#[test]
fn a_panel_is_a_view_but_not_a_tab() {
    let (world, mut editor, panel) = setup();
    assert_eq!(call(&world, &mut editor, "window_views", json!([1])).unwrap().as_array().unwrap().len(), 1);
    assert_eq!(call(&world, &mut editor, "view_is_valid", json!([panel])), Ok(json!(true)));
    call(&world, &mut editor, "view_insert", json!([panel, 0, "built"])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_size", json!([panel])), Ok(json!(5)));
    let settings = json!(["view", panel]);
    call(&world, &mut editor, "settings_set", json!([settings, "result_file_regex", "^(.*):(\\d+)"])).unwrap();
    assert_eq!(call(&world, &mut editor, "settings_get", json!([settings, "result_file_regex"])), Ok(json!([true, "^(.*):(\\d+)"])));
    assert_eq!(call(&world, &mut editor, "view_is_dirty", json!([panel])), Ok(json!(false)), "panels never ask to save");
    assert_ne!(editor.active_document().unwrap().id, panel);
}

#[test]
fn panels_are_found_listed_emptied_and_destroyed() {
    let (world, mut editor, panel) = setup();
    assert_eq!(call(&world, &mut editor, "window_find_output_panel", json!([1, "exec"])), Ok(json!(panel)));
    assert_eq!(call(&world, &mut editor, "window_find_output_panel", json!([1, "nope"])), Ok(Value::Null));
    assert_eq!(call(&world, &mut editor, "window_panels", json!([1])), Ok(json!(["output.exec"])));

    call(&world, &mut editor, "view_insert", json!([panel, 0, "old output"])).unwrap();
    call(&world, &mut editor, "view_set_read_only", json!([panel, true])).unwrap();
    assert_eq!(call(&world, &mut editor, "window_create_output_panel", json!([1, "exec"])), Ok(json!(panel)), "the same view");
    assert_eq!(call(&world, &mut editor, "view_size", json!([panel])), Ok(json!(0)), "emptied, even read-only");

    call(&world, &mut editor, "window_destroy_output_panel", json!([1, "exec"])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_is_valid", json!([panel])), Ok(json!(false)));
    assert_eq!(call(&world, &mut editor, "window_panels", json!([1])), Ok(json!([])));
}

#[test]
fn shown_panel() {
    let (world, mut editor, _) = setup();
    assert!(!editor.show_output_panel("nope"));
    assert_eq!(call(&world, &mut editor, "window_active_panel", json!([1])), Ok(Value::Null));
    assert!(editor.show_output_panel("exec"));
    assert_eq!(editor.shown_output_panel().unwrap().name, "exec");
    assert_eq!(call(&world, &mut editor, "window_active_panel", json!([1])), Ok(json!("output.exec")));
    editor.hide_output_panel();
    assert!(editor.shown_output_panel().is_none());

    editor.show_output_panel("exec");
    call(&world, &mut editor, "window_destroy_output_panel", json!([1, "exec"])).unwrap();
    assert!(editor.shown_output_panel().is_none(), "a destroyed panel is no longer shown");
}

#[test]
fn append_writes_at_the_end_and_force_writes_to_read_only_panels() {
    let (world, mut editor, panel) = setup();
    editor.show_output_panel("exec");
    let append = |characters: &str, force: bool| json!(["view", panel, "append", {"characters": characters, "force": force}]);
    call(&world, &mut editor, "run_command", append("one\n", false)).unwrap();
    call(&world, &mut editor, "view_set_read_only", json!([panel, true])).unwrap();
    call(&world, &mut editor, "run_command", append("ignored\n", false)).unwrap();
    call(&world, &mut editor, "run_command", append("two\n", true)).unwrap();
    assert_eq!(panel_text(&editor), "one\ntwo\n");
    assert_eq!(call(&world, &mut editor, "view_is_read_only", json!([panel])), Ok(json!(true)), "still read-only");
}

#[test]
fn append_also_works_in_tabs_and_other_built_in_commands_do_not_run_in_panels() {
    let (world, mut editor, panel) = setup();
    let tab = editor.active_document().unwrap().id;
    call(&world, &mut editor, "run_command", json!(["view", tab, "append", {"characters": "x"}])).unwrap();
    assert_eq!(editor.active_document().unwrap().text, "x");
    assert_eq!(
        call(&world, &mut editor, "run_command", json!(["view", panel, "sort_lines", {}])),
        Err("sort_lines cannot run in a panel yet (only append, insert, select_all and plugin commands)".into())
    );
}

/// A panel with results, as a plugin would make it.
fn results_panel(world: &World, editor: &mut Editor, text: &str) -> u64 {
    let id = call(world, editor, "window_create_output_panel", json!([1, "mine"])).unwrap().as_u64().unwrap();
    let setting = |key: &str, value: &str| json!([["view", id], key, value]);
    call(world, editor, "settings_set", setting("result_file_regex", "^(.*):(\\d+):(\\d+) (.*)$")).unwrap();
    call(world, editor, "settings_set", setting("result_base_dir", "/p")).unwrap();
    call(world, editor, "view_insert", json!([id, 0, text])).unwrap();
    assert!(editor.show_output_panel("mine"));
    id
}

fn caret(editor: &Editor) -> (String, usize) {
    let doc = editor.active_document().unwrap();
    (doc.name(), doc.selection().head)
}

fn results_world() -> (World, Editor) {
    let world = World::new()
        .with_file("/p/a.txt", "one\ntwo\n")
        .with_file("/p/b.txt", "three\n")
        .with_plugin_commands(&[("probe", Text)]);
    let editor = world.editor();
    (world, editor)
}

#[test]
fn next_and_previous_result_walk_the_panel_and_wrap_around() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "a.txt:2:2 first\nnoise\nb.txt:1:3 second\n");

    assert!(editor.next_result(true));
    assert_eq!(caret(&editor), ("a.txt".to_string(), 5));
    assert_eq!(status(&editor), "first");
    assert_eq!(editor.shown_output_panel().unwrap().current_result, Some(0));
    assert!(editor.next_result(true));
    assert_eq!(caret(&editor), ("b.txt".to_string(), 2));
    assert!(editor.next_result(true));
    assert_eq!(caret(&editor).0, "a.txt", "back to the first");
    assert!(editor.next_result(false));
    assert_eq!(caret(&editor).0, "b.txt", "back to the last");
    assert!(editor.next_result(false));
    assert_eq!(caret(&editor).0, "a.txt");
}

#[test]
fn prev_result_starts_at_the_last_one() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "a.txt:1:1 x\nb.txt:1:1 y\n");
    assert!(editor.next_result(false));
    assert_eq!(caret(&editor).0, "b.txt");
}

#[test]
fn with_no_results_there_is_nowhere_to_go() {
    let (world, mut editor) = results_world();
    assert!(!editor.next_result(true));
    results_panel(&world, &mut editor, "nothing here\n");
    assert!(!editor.next_result(true));
    assert!(!editor.open_result_at_line(0));
}

#[test]
fn a_file_that_is_not_there_is_reported_and_no_buffer_opens() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "gone.txt:1:1 x\n");
    let tabs = editor.state().documents.len();
    assert!(editor.next_result(true));
    assert_eq!(editor.state().documents.len(), tabs);
    assert!(status(&editor).starts_with("File not found: "), "{}", status(&editor));
}

#[test]
fn next_result_shows_a_hidden_panel() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "a.txt:1:1 x\n");
    editor.hide_output_panel();
    assert!(editor.next_result(true));
    assert_eq!(editor.shown_output_panel().unwrap().name, "mine");
}

#[test]
fn emptying_a_panel_forgets_its_current_result() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "a.txt:1:1 x\n");
    editor.next_result(true);
    call(&world, &mut editor, "window_create_output_panel", json!([1, "mine"])).unwrap();
    assert_eq!(editor.shown_output_panel().unwrap().current_result, None);
}

#[test]
fn a_destroyed_panel_has_no_more_results() {
    let (world, mut editor) = results_world();
    results_panel(&world, &mut editor, "a.txt:1:1 x\n");
    call(&world, &mut editor, "window_destroy_output_panel", json!([1, "mine"])).unwrap();
    assert!(!editor.next_result(true));
}

#[test]
fn plugins_can_list_the_results() {
    let (world, mut editor) = results_world();
    let id = results_panel(&world, &mut editor, "a.txt:2:1 bad\n");
    assert_eq!(call(&world, &mut editor, "view_find_all_results", json!([id])), Ok(json!([["a.txt", 2, 1, "bad"]])));
}

#[test]
fn the_results_of_a_build_are_followed() {
    let world = World::new().with_file("/p/a.txt", "one\ntwo\n");
    let mut editor = world.editor();
    editor.exec(&json!({ "cmd": ["x"], "file_regex": "^(.*):(\\d+)$", "working_dir": "/p" }));
    world.processes.events.borrow_mut().push_back(ProcessEvent::Output("a.txt:2\n".into()));
    editor.poll_builds();
    assert!(editor.next_result(true), "the panel need not have been shown");
    assert_eq!(active_text(&editor), "one\ntwo\n");
    open(&mut editor, "/p/a.txt");
    assert_eq!(editor.active_document().unwrap().selection().head, 4);
}
