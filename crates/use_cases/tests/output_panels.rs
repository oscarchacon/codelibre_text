//! Tests for output panels (`use_cases::output_panels`), driven like plugins drive them: the
//! scripted plugin host calls `window_create_output_panel` and friends.

use entities::command::Command;
use serde_json::{json, Value};
use test_support::{plugin_call, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
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
        call(&world, &mut editor, "run_command", json!(["view", panel, "select_all", {}])),
        Err("select_all cannot run in an output panel yet (only append and plugin commands)".into())
    );
}
