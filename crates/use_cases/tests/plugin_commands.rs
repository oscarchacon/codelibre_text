//! Tests for what menus and the palette ask plugin commands (`use_cases::plugin_commands`).

use entities::command::Command;
use entities::menu::{MenuItem, PaletteCommand};
use serde_json::{json, Value};
use test_support::World;
use use_cases::gateways::PluginCommandKind::Window;
use use_cases::gateways::PluginMessage;
use use_cases::search::SearchKind;

fn world() -> World {
    World::new()
        .with_palette(PaletteCommand { caption: "Mine: Shown".into(), command: Command::new("shown") })
        .with_palette(PaletteCommand { caption: "Mine: Hidden".into(), command: Command::new("hidden") })
        .with_palette(PaletteCommand { caption: "Mine: Disabled".into(), command: Command::new("disabled") })
        .with_palette(PaletteCommand { caption: String::new(), command: Command::new("described") })
        .with_palette(PaletteCommand { caption: "File: Save".into(), command: Command::new("save") })
        .with_menu(MenuItem {
            caption: "Tools".into(),
            id: None,
            command: None,
            checkbox: false,
            children: vec![MenuItem {
                caption: "Shown".into(),
                id: None,
                command: Some(Command::new("shown")),
                checkbox: false,
                children: vec![],
            }],
        })
        .with_plugins(
            &[("shown", Window), ("hidden", Window), ("disabled", Window), ("described", Window)],
            &["command_states"],
        )
}

fn answers() -> PluginMessage {
    let state = |visible: bool, enabled: bool, description: Value| {
        json!({ "visible": visible, "enabled": enabled, "checked": false, "description": description })
    };
    PluginMessage::Answer(json!([
        state(true, true, Value::Null),
        state(false, true, Value::Null),
        state(true, false, Value::Null),
        state(true, true, json!("Described")),
    ]))
}

fn palette(editor: &use_cases::Editor) -> Vec<String> {
    let results = editor.search(SearchKind::Commands, "").into_iter().map(|c| c.caption);
    let mut captions: Vec<String> = results.filter(|c| !c.starts_with("Set Syntax")).collect();
    captions.sort();
    captions
}

#[test]
fn each_plugin_command_is_asked_once_in_one_request() {
    let world = world();
    let mut editor = world.editor();
    world.plugins.then([answers()]);
    editor.refresh_plugin_command_states();
    let queries = world.plugins.queries.borrow();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].0, "command_states");
    let asked = queries[0].2[0].as_array().unwrap();
    assert_eq!(asked[0], json!(["shown", Value::Null]));
    assert_eq!(asked.len(), 4, "shown is in the menu and the palette; save is not a plugin's: {asked:?}");
}

#[test]
fn the_palette_leaves_out_hidden_and_disabled_commands() {
    let world = world();
    let mut editor = world.editor();
    assert_eq!(palette(&editor).len(), 5, "everything shows until the plugins answer");
    world.plugins.then([answers()]);
    editor.refresh_plugin_command_states();
    assert_eq!(palette(&editor), vec!["Described", "File: Save", "Mine: Shown"]);
}

#[test]
fn nothing_is_asked_when_no_command_defines_these_methods() {
    let world = World::new()
        .with_palette(PaletteCommand { caption: "x".into(), command: Command::new("x") })
        .with_plugin_commands(&[("x", Window)]);
    let mut editor = world.editor();
    editor.refresh_plugin_command_states();
    assert!(world.plugins.queries.borrow().is_empty());
}

#[test]
fn a_command_with_an_input_handler_takes_over() {
    let world = World::new().with_plugins(&[("ask", Window), ("plain", Window)], &["command_input"]);
    let mut editor = world.editor();
    world.plugins.then([PluginMessage::Answer(json!(true))]);
    assert!(editor.start_plugin_command_input(&Command::new("ask")));
    assert_eq!(world.plugins.queries.borrow()[0].2, json!(["ask", Value::Null]));
    world.plugins.then([PluginMessage::Answer(json!(false))]);
    assert!(!editor.start_plugin_command_input(&Command::new("plain")));
    assert!(!editor.start_plugin_command_input(&Command::new("save")), "not a plugin command");
    assert_eq!(world.plugins.queries.borrow().len(), 2);
}
