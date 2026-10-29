//! Tests for `controllers::keyboard_controller` (key presses through the keymap).

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use interface_adapters::controllers::keyboard_controller::{clipboard_shortcut, key_pressed, KeyHandling};
use interface_adapters::view_state::{QuickPanel, ViewState};
use serde_json::{json, Value};
use test_support::{plugin_call, status, World};
use use_cases::gateways::{PluginCommandKind, PluginMessage};
use use_cases::search::SearchKind;
use use_cases::Editor;

fn key(spec: &str) -> Vec<KeyCombo> {
    vec![KeyCombo::parse(spec).unwrap()]
}

fn bind(keys: &[&str], command: Command, context: Vec<Value>) -> Binding {
    Binding { keys: keys.iter().map(|k| KeyCombo::parse(k).unwrap()).collect(), command, context }
}

fn press(editor: &mut Editor, view: &mut ViewState, spec: &str) -> bool {
    matches!(key_pressed(editor, view, &key(spec)), KeyHandling::Consumed)
}

fn queued(view: &mut ViewState) -> Vec<String> {
    view.take_commands().into_iter().map(|c| c.name).collect()
}

#[test]
fn bound_keys_queue_their_command_and_are_consumed() {
    let world = World::new().with_binding(bind(&["ctrl+s"], Command::new("save"), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "ctrl+s"));
    assert_eq!(queued(&mut view), vec!["save"]);
}

#[test]
fn unbound_keys_pass_through_to_the_text() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "a"));
    assert!(queued(&mut view).is_empty());
}

#[test]
fn bindings_to_text_widget_commands_let_the_key_through() {
    let world = World::new()
        .with_binding(bind(&["ctrl+z"], Command::new("undo"), vec![]))
        .with_binding(bind(&["ctrl+v"], Command::new("paste"), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "ctrl+z"));
    assert!(!press(&mut editor, &mut view, "ctrl+v"));
    assert!(queued(&mut view).is_empty(), "the widget undoes/pastes itself");
}

#[test]
fn clipboard_shortcuts_go_to_the_widget_unless_a_whole_line_is_copied() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let consumed = |editor: &mut Editor, view: &mut ViewState, command: &str| {
        matches!(clipboard_shortcut(editor, view, command), KeyHandling::Consumed)
    };
    // Empty selection, but the text area does not have the focus (e.g. the find field does).
    assert!(!consumed(&mut editor, &mut view, "copy"));
    view.text_has_focus = true;
    assert!(consumed(&mut editor, &mut view, "copy"));
    assert!(consumed(&mut editor, &mut view, "cut"));
    assert!(!consumed(&mut editor, &mut view, "paste"), "nothing was copied as a line yet");
    assert_eq!(queued(&mut view), vec!["copy", "cut"]);

    let world = World::new().with_preferences(json!({ "copy_with_empty_selection": false }));
    let mut editor = world.editor();
    view.text_has_focus = true;
    assert!(!consumed(&mut editor, &mut view, "copy"));
}

#[test]
fn chords_show_a_status_message_until_completed() {
    let world = World::new().with_binding(bind(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar"), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "ctrl+k"));
    assert_eq!(status(&editor), "Ctrl+K was pressed. Waiting for second key…");
    assert!(press(&mut editor, &mut view, "ctrl+b"));
    assert!(editor.state().status.is_none());
    assert_eq!(queued(&mut view), vec!["toggle_side_bar"]);
}

fn escape_world() -> World {
    World::new()
        .with_binding(bind(
            &["escape"],
            Command::new("hide_overlay"),
            vec![json!({ "key": "overlay_visible", "operator": "equal", "operand": true })],
        ))
        .with_binding(bind(
            &["escape"],
            Command::new("hide_panel"),
            vec![json!({ "key": "panel_visible", "operator": "equal", "operand": true })],
        ))
}

#[test]
fn overlay_visible_context() {
    let world = escape_world();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "escape"), "nothing open: escape goes to the text");
    view.quick_panel = Some(QuickPanel::new(SearchKind::Goto, String::new()));
    assert!(press(&mut editor, &mut view, "escape"));
    assert_eq!(queued(&mut view), vec!["hide_overlay"]);
}

#[test]
fn panel_visible_context() {
    let world = escape_world();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    view.find_panel.visible = true;
    assert!(press(&mut editor, &mut view, "escape"));
    assert_eq!(queued(&mut view), vec!["hide_panel"]);
}

#[test]
fn plugins_answer_contexts_the_editor_does_not_know() {
    let world = World::new()
        .with_binding(bind(
            &["tab"],
            Command::new("expand_abbreviation"),
            vec![json!({ "key": "emmet_abbreviation", "operator": "equal", "operand": true })],
        ))
        .with_plugins(&[], &["on_query_context"]);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    world.plugins.then([PluginMessage::Answer(json!(true))]);
    assert!(press(&mut editor, &mut view, "tab"));
    assert_eq!(queued(&mut view), vec!["expand_abbreviation"]);
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    assert!(!press(&mut editor, &mut view, "tab"), "no answer: Tab goes to the text");
}

#[test]
fn an_output_panel_is_visible_but_has_no_focus() {
    let world = escape_world()
        .with_binding(bind(
            &["enter"],
            Command::new("find_next"),
            vec![json!({ "key": "panel_has_focus", "operator": "equal", "operand": true })],
        ))
        .with_plugin_commands(&[("build", PluginCommandKind::Window)]);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    world.plugins.then([plugin_call("window_create_output_panel", json!([1, "exec"])), PluginMessage::Done { error: None }]);
    editor.run_plugin_command(&Command::new("build"), &mut |_, _| {});
    editor.show_output_panel("exec");
    assert!(!press(&mut editor, &mut view, "enter"), "Enter still types in the text");
    assert!(press(&mut editor, &mut view, "escape"));
    assert_eq!(queued(&mut view), vec!["hide_panel"]);
}

#[test]
fn setting_and_selection_contexts() {
    let world = World::new()
        .with_preferences(json!({ "vintage": true }))
        .with_binding(bind(&["j"], Command::new("move_down"), vec![json!({ "key": "setting.vintage", "operand": true })]))
        .with_binding(bind(&["ctrl+x"], Command::new("cut_line"), vec![json!({ "key": "selection_empty", "operand": true })]))
        .with_binding(bind(&["tab"], Command::new("next_field"), vec![json!({ "key": "has_next_field", "operand": true })]))
        .with_binding(bind(&["f9"], Command::new("weird"), vec![json!({ "key": "selector", "operand": "source.rust" })]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "j"));
    assert!(press(&mut editor, &mut view, "ctrl+x"));
    assert!(!press(&mut editor, &mut view, "tab"), "no snippet fields");
    assert!(!press(&mut editor, &mut view, "f9"), "unknown contexts never match");
    assert_eq!(queued(&mut view), vec!["move_down", "cut_line"]);
}

#[test]
fn not_equal_context_operator() {
    let world = World::new().with_binding(bind(
        &["f2"],
        Command::new("only_without_overlay"),
        vec![json!({ "key": "overlay_visible", "operator": "not_equal", "operand": true })],
    ));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "f2"));
    view.quick_panel = Some(QuickPanel::new(SearchKind::Commands, String::new()));
    assert!(!press(&mut editor, &mut view, "f2"));
}
