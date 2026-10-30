//! Tests for the listener methods that answer the editor (`use_cases::plugin_hooks`):
//! `on_query_context` for key bindings, `on_text_command` / `on_window_command` and the
//! `on_post_*_command` notices.

use entities::command::Command;
use entities::keymap::{Binding, KeyCombo, KeyContext, KeyResult};
use serde_json::{json, Value};
use test_support::World;
use use_cases::gateways::PluginMessage;
use use_cases::plugin_hooks::CommandTarget;

/// Knows no context key: every condition goes to the plugins.
struct NoContext;

impl KeyContext for NoContext {
    fn query(&self, _key: &str, _operator: &str, _operand: &Value) -> Option<bool> {
        None
    }
}

fn plugin_context_binding() -> Binding {
    Binding {
        keys: vec![KeyCombo::parse("ctrl+k").unwrap()],
        command: Command::new("plugin_thing"),
        context: vec![json!({ "key": "my_plugin.active", "operator": "not_equal", "operand": false })],
    }
}

fn press(editor: &mut use_cases::Editor) -> Option<String> {
    match editor.press_key(&[KeyCombo::parse("ctrl+k").unwrap()], &NoContext) {
        KeyResult::Command(command) => Some(command.name),
        _ => None,
    }
}

#[test]
fn unknown_key_contexts_ask_the_plugins() {
    let world = World::new().with_binding(plugin_context_binding()).with_plugins(&[], &["on_query_context"]);
    let mut editor = world.editor();
    let view = editor.active_document().unwrap().id;
    world.plugins.then([PluginMessage::Answer(json!(true))]);
    assert_eq!(press(&mut editor).as_deref(), Some("plugin_thing"));
    assert_eq!(
        world.plugins.queries.borrow()[0],
        ("on_query_context".to_string(), Some(view), json!(["my_plugin.active", "not_equal", false, false]))
    );
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    assert_eq!(press(&mut editor), None, "no listener knows the key: the binding does not apply");
    world.plugins.then([PluginMessage::Answer(json!(false))]);
    assert_eq!(press(&mut editor), None);
}

#[test]
fn without_a_listener_nothing_is_asked() {
    let world = World::new().with_binding(plugin_context_binding()).with_plugins(&[], &["on_modified"]);
    let mut editor = world.editor();
    assert_eq!(press(&mut editor), None);
    assert!(world.plugins.queries.borrow().is_empty());
}

#[test]
fn listeners_may_replace_a_command() {
    let world = World::new().with_plugins(&[], &["on_text_command", "on_window_command"]);
    let mut editor = world.editor();
    let view = editor.active_document().unwrap().id;
    let paste = Command::new("paste");

    world.plugins.then([PluginMessage::Answer(json!(["paste_and_indent", {"x": 1}]))]);
    let rewritten = editor.plugin_rewrite_command(&paste, CommandTarget::View);
    assert_eq!(rewritten, Command::with_args("paste_and_indent", json!({"x": 1})));
    assert_eq!(world.plugins.queries.borrow()[0], ("on_text_command".to_string(), Some(view), json!(["paste", null])));

    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    assert_eq!(editor.plugin_rewrite_command(&paste, CommandTarget::View), paste, "no answer: unchanged");

    world.plugins.then([PluginMessage::Answer(json!(["hide_panel"]))]);
    let rewritten = editor.plugin_rewrite_command(&Command::new("show_panel"), CommandTarget::Window);
    assert_eq!(rewritten.name, "hide_panel");
    assert_eq!(world.plugins.queries.borrow()[2].0, "on_window_command");
    assert_eq!(world.plugins.queries.borrow()[2].1, None, "window events have no view");
}

#[test]
fn listeners_hear_about_commands_that_ran() {
    let world = World::new().with_plugins(&[], &["on_post_text_command"]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    editor.plugin_command_ran(&Command::with_args("insert", json!({"characters": "x"})), CommandTarget::View);
    editor.plugin_command_ran(&Command::new("new_file"), CommandTarget::Window);
    let queries = world.plugins.queries.borrow();
    assert_eq!(queries.len(), 1, "nobody listens to on_post_window_command");
    assert_eq!(queries[0].0, "on_post_text_command");
    assert_eq!(queries[0].2, json!(["insert", {"characters": "x"}]));
}

#[test]
fn a_failing_listener_is_logged_and_changes_nothing() {
    let world = World::new().with_plugins(&[], &["on_text_command"]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Done { error: Some("Traceback: boom".into()) }]);
    let command = Command::new("upper_case");
    assert_eq!(editor.plugin_rewrite_command(&command, CommandTarget::View), command);
    assert!(editor.state().console.contains("on_text_command: Traceback: boom"));
}

#[test]
fn match_all_goes_to_on_query_context() {
    let mut binding = plugin_context_binding();
    binding.context[0]["match_all"] = json!(true);
    let world = World::new().with_binding(binding).with_plugins(&[], &["on_query_context"]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Answer(json!(true))]);
    press(&mut editor);
    assert_eq!(world.plugins.queries.borrow()[0].2[3], json!(true));
}

#[test]
fn post_command_events_go_out_for_their_async_variant_too() {
    let world = World::new().with_plugins(&[], &["on_post_text_command_async"]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    editor.plugin_command_ran(&Command::new("x"), CommandTarget::View);
    assert_eq!(world.plugins.queries.borrow()[0].0, "on_post_text_command");
}
