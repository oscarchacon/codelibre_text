//! Tests for `controllers::keyboard_controller` (key presses through the keymap).

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo};
use interface_adapters::controllers::command_controller::{poll_plugins, run};
use interface_adapters::controllers::keyboard_controller::{clipboard_shortcut, key_pressed, KeyHandling};
use interface_adapters::view_state::{QuickPanel, ViewRequest, ViewState, WidgetEdit};
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
fn text_contexts_look_at_every_selection_and_match_all_needs_all_of_them() {
    let context = |key: &str, operator: &str, operand: &str, match_all: bool| {
        json!({ "key": key, "operator": operator, "operand": operand, "match_all": match_all })
    };
    let world = World::new()
        .with_binding(bind(&["("], Command::new("one_caret"), vec![context("preceding_text", "regex_contains", "\\bif$", false)]))
        .with_binding(bind(&[")"], Command::new("all_carets"), vec![context("following_text", "regex_match", "^$", true)]))
        .with_binding(bind(&["x"], Command::new("selected"), vec![context("text", "equal", "if", false)]));
    let mut editor = world.editor();
    editor.insert_at_selections("if\nel");
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::caret(2));
    editor.add_carets_on_next_line(true);
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "("), "the first caret follows `if`");
    assert!(press(&mut editor, &mut view, ")"), "nothing follows either caret");
    assert!(!press(&mut editor, &mut view, "x"), "nothing is selected");
    assert_eq!(queued(&mut view), vec!["one_caret", "all_carets"]);

    editor.sync_selection_from_view(id, entities::selection::Selection::caret(1));
    assert!(!press(&mut editor, &mut view, ")"), "`f` follows the caret");
    editor.sync_selection_from_view(id, entities::selection::Selection::new(0, 2));
    assert!(press(&mut editor, &mut view, "x"));
}

// ----- auto-pairing (the built-in keymap's `auto_match_enabled` bindings) ---------------------------

/// A world with the built-in keymap's auto-pairing bindings and `auto_match_enabled` set to `enabled`.
fn auto_pair_world(enabled: bool) -> World {
    let keymap = interface_adapters::gateways::sublime_json::parse(interface_adapters::gateways::builtin_resources::PC_KEYMAP).unwrap();
    let mut world = World::new().with_preferences(json!({ "auto_match_enabled": enabled }));
    for item in keymap.as_array().unwrap() {
        let context = item["context"].as_array().cloned().unwrap_or_default();
        if !context.iter().any(|c| c["key"] == "setting.auto_match_enabled") {
            continue;
        }
        let keys: Vec<&str> = item["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap()).collect();
        let command = Command::with_args(item["command"].as_str().unwrap(), item["args"].clone());
        world = world.with_binding(bind(&keys, command, context));
    }
    world
}

/// Types `keys` (one key each, `backspace` included) as the window does: through the keymap, else
/// as the text widget would.
fn type_keys(editor: &mut Editor, view: &mut ViewState, keys: &[&str]) {
    for &key in keys {
        // A space is not a key name: the widget types it.
        if KeyCombo::parse(key).is_some() && press(editor, view, key) {
            for command in view.take_commands() {
                run(editor, view, &command);
            }
        } else if key == "backspace" {
            editor.run_view_command_on_active(&Command::new("left_delete"));
        } else {
            editor.insert_at_selections(key);
        }
    }
}

fn text_and_caret(editor: &Editor) -> (String, usize, usize) {
    let doc = editor.active_document().unwrap();
    (doc.text.clone(), doc.selection().start(), doc.selection().end())
}

#[test]
fn brackets_and_quotes_are_paired_stepped_over_and_deleted_together() {
    let world = auto_pair_world(true);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    type_keys(&mut editor, &mut view, &["(", "x"]);
    assert_eq!(text_and_caret(&editor), ("(x)".into(), 2, 2));
    type_keys(&mut editor, &mut view, &[")"]);
    assert_eq!(text_and_caret(&editor), ("(x)".into(), 3, 3), "`)` steps over the closing one");
    type_keys(&mut editor, &mut view, &[" ", "[", "{", "\""]);
    assert_eq!(text_and_caret(&editor), ("(x) [{\"\"}]".into(), 7, 7));
    type_keys(&mut editor, &mut view, &["backspace", "backspace"]);
    assert_eq!(text_and_caret(&editor), ("(x) []".into(), 5, 5), "Backspace deletes both of a pair");
    type_keys(&mut editor, &mut view, &["]"]);
    assert_eq!(text_and_caret(&editor), ("(x) []".into(), 6, 6));
}

#[test]
fn pairs_are_not_added_before_text_or_after_a_word_for_quotes() {
    let world = auto_pair_world(true);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    editor.insert_at_selections("word");
    type_keys(&mut editor, &mut view, &["'"]);
    assert_eq!(text_and_caret(&editor).0, "word'", "an apostrophe after a word");
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::caret(0));
    type_keys(&mut editor, &mut view, &["("]);
    assert_eq!(text_and_caret(&editor).0, "(word'", "text follows the caret");
}

#[test]
fn a_selection_is_wrapped_in_the_pair() {
    let world = auto_pair_world(true);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    editor.insert_at_selections("ab");
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::new(0, 2));
    type_keys(&mut editor, &mut view, &["["]);
    assert_eq!(text_and_caret(&editor), ("[ab]".into(), 1, 3), "the text stays selected");
}

#[test]
fn every_caret_gets_its_pair() {
    let world = auto_pair_world(true);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    editor.insert_at_selections("a\nb");
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::caret(1));
    editor.add_carets_on_next_line(true);
    type_keys(&mut editor, &mut view, &["("]);
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.text, "a()\nb()");
    let carets: Vec<usize> = doc.selections().iter().map(|s| s.head).collect();
    assert_eq!(carets.len(), 2);
    assert!(carets.contains(&2) && carets.contains(&6), "{carets:?}");
}

#[test]
fn pairing_is_off_without_the_setting_and_in_a_focused_panel() {
    let world = auto_pair_world(false);
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "("), "auto_match_enabled is false");

    let world = auto_pair_world(true);
    let mut editor = world.editor();
    view.find_panel.visible = true;
    view.text_has_focus = false;
    assert!(!press(&mut editor, &mut view, "("), "typed in the find panel");
    view.text_has_focus = true;
    assert!(press(&mut editor, &mut view, "("), "the text has the focus again");
}

#[test]
fn tab_visits_a_snippets_fields_while_there_is_a_next_one() {
    let has_next = json!({ "key": "has_next_field", "operator": "equal", "operand": true });
    let world = World::new().with_binding(bind(&["tab"], Command::new("next_field"), vec![has_next]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "tab"), "no snippet: Tab is Tab");
    run(&mut editor, &mut view, &Command::with_args("insert_snippet", json!({ "contents": "${1:a}(${2:b})" })));
    assert!(press(&mut editor, &mut view, "tab"));
    for command in view.take_commands() {
        run(&mut editor, &mut view, &command);
    }
    assert_eq!(editor.active_document().unwrap().selection(), entities::selection::Selection::new(2, 3), "b");
    assert!(press(&mut editor, &mut view, "tab"), "then the end of the snippet");
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
fn motions_the_widget_cannot_do_run_as_commands() {
    let world = World::new()
        .with_binding(bind(&["ctrl+left"], Command::with_args("move", json!({ "by": "words", "forward": false })), vec![]))
        .with_binding(bind(&["alt+left"], Command::with_args("move", json!({ "by": "subwords", "forward": false })), vec![]))
        .with_binding(bind(&["ctrl+m"], Command::with_args("move_to", json!({ "to": "brackets" })), vec![]))
        .with_binding(bind(&["end"], Command::with_args("move_to", json!({ "to": "eol" })), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "ctrl+left"), "the widget moves by words");
    assert!(!press(&mut editor, &mut view, "end"));
    assert!(press(&mut editor, &mut view, "alt+left"));
    assert!(press(&mut editor, &mut view, "ctrl+m"));
    assert_eq!(queued(&mut view), ["move", "move_to"]);
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

    // Copy and cut of a selection in the text are commands too (the clipboard history sees
    // them; the widget copies a frame later); paste of ordinary text stays the widget's.
    let world = World::new().with_preferences(json!({ "copy_with_empty_selection": false }));
    let mut editor = world.editor();
    view.text_has_focus = true;
    assert!(consumed(&mut editor, &mut view, "copy"));
    assert!(!consumed(&mut editor, &mut view, "paste"));
}

#[test]
fn a_clipboard_shortcut_can_end_a_chord() {
    let modifier = if cfg!(target_os = "macos") { "super" } else { "ctrl" };
    let keys = [format!("{modifier}+k"), format!("{modifier}+v")];
    let world = World::new().with_binding(bind(&[&keys[0], &keys[1]], Command::new("paste_from_history"), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, &keys[0]));
    // egui sends the second key as a paste event, not as a key.
    assert!(matches!(clipboard_shortcut(&mut editor, &mut view, "paste"), KeyHandling::Consumed));
    assert_eq!(queued(&mut view), vec!["paste_from_history"]);
}

#[test]
fn chords_show_a_status_message_until_completed() {
    let world = World::new().with_binding(bind(&["ctrl+k", "ctrl+b"], Command::new("toggle_side_bar"), vec![]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "ctrl+k"));
    assert_eq!(status(&editor), format!("{} was pressed. Waiting for second key…", KeyCombo::parse("ctrl+k").unwrap().display()));
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
        .with_binding(bind(&["f9"], Command::new("weird"), vec![json!({ "key": "unknown_context", "operand": true })]));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    assert!(press(&mut editor, &mut view, "j"));
    assert!(press(&mut editor, &mut view, "ctrl+x"));
    assert!(!press(&mut editor, &mut view, "tab"), "no snippet fields");
    assert!(!press(&mut editor, &mut view, "f9"), "unknown contexts never match");
    assert_eq!(queued(&mut view), vec!["move_down", "cut_line"]);
}

#[test]
fn selector_context_matches_the_scope_at_a_caret() {
    let selector = |operator: &str| json!({ "key": "selector", "operator": operator, "operand": "source.python string" });
    let world = World::new()
        .with_file("/p/a.py", "x = \"hi\"")
        .with_binding(bind(&["*"], Command::new("wrap"), vec![selector("equal")]))
        .with_binding(bind(&["f9"], Command::new("outside"), vec![selector("not_equal")]));
    let mut editor = world.editor_opening(&["/p/a.py"]);
    let mut view = ViewState::default();
    assert!(!press(&mut editor, &mut view, "*"), "the caret is at the start, outside the string");
    assert!(press(&mut editor, &mut view, "f9"));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::caret(5));
    assert!(press(&mut editor, &mut view, "*"));
    assert!(!press(&mut editor, &mut view, "f9"));
    assert_eq!(queued(&mut view), vec!["outside", "wrap"]);
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

/// An editor whose plugins listen to `events`, with Ctrl+Z bound to undo and the text focused.
fn hooked_editor(events: &[&str]) -> (World, Editor, ViewState) {
    let world = World::new()
        .with_binding(bind(&["ctrl+z"], Command::new("undo"), vec![]))
        .with_binding(bind(&["ctrl+left"], Command::with_args("move", json!({ "by": "words", "forward": false })), vec![]))
        .with_plugins(&[], events);
    let editor = world.editor();
    let mut view = ViewState::default();
    view.text_has_focus = true;
    (world, editor, view)
}

#[test]
fn widget_keys_become_commands_when_plugins_listen_to_text_commands() {
    let (_world, mut editor, mut view) = hooked_editor(&["on_text_command"]);
    assert!(press(&mut editor, &mut view, "ctrl+z"), "the widget must not undo as well");
    assert_eq!(queued(&mut view), ["undo"]);
    assert!(press(&mut editor, &mut view, "ctrl+left"), "the listener hears the caret motion");
    assert_eq!(queued(&mut view), ["move"]);
}

#[test]
fn widget_keys_stay_with_the_widget_without_listeners_or_focus() {
    let (_world, mut editor, mut view) = hooked_editor(&["on_modified"]);
    assert!(!press(&mut editor, &mut view, "ctrl+z"));

    let (_world, mut editor, mut view) = hooked_editor(&["on_post_text_command"]);
    view.text_has_focus = false;
    assert!(!press(&mut editor, &mut view, "ctrl+z"), "it undoes in the find field");
    assert!(queued(&mut view).is_empty());
}

#[test]
fn clipboard_shortcuts_become_commands_when_plugins_listen() {
    let (_world, mut editor, mut view) = hooked_editor(&["on_text_command"]);
    assert!(matches!(clipboard_shortcut(&mut editor, &mut view, "paste"), KeyHandling::Consumed));
    assert_eq!(queued(&mut view), ["paste"]);

    view.text_has_focus = false;
    assert!(matches!(clipboard_shortcut(&mut editor, &mut view, "paste"), KeyHandling::PassThrough));
}

#[test]
fn a_listener_replaces_a_widget_command_and_hears_the_end_after_the_widget() {
    let (world, mut editor, mut view) = hooked_editor(&["on_text_command", "on_post_text_command"]);
    *world.clipboard.borrow_mut() = Ok("pasted".to_string());
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    run(&mut editor, &mut view, &Command::new("paste"));
    assert_eq!(view.take_requests(), [ViewRequest::WidgetEdit(WidgetEdit::Paste("pasted".to_string()))]);
    let posts = |world: &World| world.plugins.queries.borrow().iter().filter(|q| q.0 == "on_post_text_command").count();

    poll_plugins(&mut editor, &mut view);
    assert_eq!(posts(&world), 0, "this frame's widgets ran before the edit was asked for");
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    poll_plugins(&mut editor, &mut view);
    assert_eq!(posts(&world), 1, "the next frame's widget did it");
    poll_plugins(&mut editor, &mut view);
    assert_eq!(posts(&world), 1, "once");
}

#[test]
fn a_listener_can_stop_a_widget_edit_by_replacing_it() {
    let (world, mut editor, mut view) = hooked_editor(&["on_text_command"]);
    world.plugins.then([PluginMessage::Answer(json!(["insert", { "characters": "mine" }]))]);
    run(&mut editor, &mut view, &Command::new("paste"));
    assert!(view.take_requests().is_empty());
    assert!(editor.active_document().unwrap().text.contains("mine"));
}

#[test]
fn the_auto_complete_list_takes_the_arrows_and_its_bindings_apply_while_it_shows() {
    let visible = || vec![json!({ "key": "auto_complete_visible", "operator": "equal", "operand": true })];
    let world = World::new()
        .with_file("/p/a.txt", "apple apricot\n")
        .with_binding(bind(&["enter"], Command::new("commit_completion"), visible()))
        .with_binding(bind(&["escape"], Command::new("hide_auto_complete"), visible()));
    let mut editor = world.editor_opening(&["/p/a.txt"]);
    let mut view = ViewState::default();
    view.text_has_focus = true;
    assert!(!press(&mut editor, &mut view, "down"), "no list: the caret moves");
    assert!(!press(&mut editor, &mut view, "enter"));

    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, entities::selection::Selection::caret(14));
    editor.insert_at_selections("ap");
    run(&mut editor, &mut view, &Command::new("auto_complete"));
    let triggers = |editor: &Editor| editor.auto_complete().unwrap().shown.iter().map(|s| s.completion.trigger.clone()).collect::<Vec<_>>();
    assert_eq!(triggers(&editor).len(), 2);
    assert!(press(&mut editor, &mut view, "down"));
    assert_eq!(editor.auto_complete().unwrap().selected, 1);
    let chosen = triggers(&editor)[1].clone();
    assert!(press(&mut editor, &mut view, "enter"));
    assert_eq!(queued(&mut view), vec!["commit_completion"]);
    run(&mut editor, &mut view, &Command::new("commit_completion"));
    assert!(editor.active_document().unwrap().text.ends_with(&format!("
{chosen}")));
    assert!(!editor.auto_complete_visible());
}
