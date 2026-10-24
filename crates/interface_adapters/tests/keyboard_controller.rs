//! Tests for `controllers::keyboard_controller` (key presses through the keymap).

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use interface_adapters::controllers::keyboard_controller::{key_pressed, KeyHandling};
use interface_adapters::view_state::{QuickPanel, ViewState};
use serde_json::{json, Value};
use test_support::{status, World};
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
