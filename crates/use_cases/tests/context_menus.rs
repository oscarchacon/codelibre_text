//! Tests for the right-click menus (`use_cases::context_menus`).

use std::path::PathBuf;

use entities::command::Command;
use entities::layout::Layout;
use entities::menu::{ContextMenuTarget, ContextMenus, MenuItem};
use serde_json::{json, Value};
use test_support::{open, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
use use_cases::Editor;

fn entry(command: &str, args: Value) -> MenuItem {
    MenuItem { caption: command.into(), id: None, command: Some(Command::with_args(command, args)), checkbox: false, children: vec![] }
}

fn names_in(editor: &Editor, group: usize) -> Vec<String> {
    editor.group_indexes(group).into_iter().map(|i| editor.state().documents[i].name()).collect()
}

fn four_files() -> (World, Editor) {
    let world = World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b").with_file("/p/c.txt", "c").with_file("/p/d.txt", "d");
    let mut editor = world.editor();
    for file in ["/p/a.txt", "/p/b.txt", "/p/c.txt", "/p/d.txt"] {
        open(&mut editor, file);
    }
    (world, editor)
}

#[test]
fn a_tab_closes_itself_the_others_or_those_to_its_right() {
    let (_world, mut editor) = four_files();
    editor.close_by_index(&Command::with_args("close_by_index", json!({ "group": 0, "index": 1 })));
    assert_eq!(names_in(&editor, 0), ["a.txt", "c.txt", "d.txt"]);
    editor.close_by_index(&Command::with_args("close_to_right_by_index", json!({ "group": 0, "index": 1 })));
    assert_eq!(names_in(&editor, 0), ["a.txt", "c.txt"]);
    editor.close_by_index(&Command::with_args("close_others_by_index", json!({ "group": 0, "index": 1 })));
    assert_eq!(names_in(&editor, 0), ["c.txt"]);
    editor.close_by_index(&Command::with_args("close_by_index", json!({ "group": 0, "index": 5 })));
    assert_eq!(names_in(&editor, 0), ["c.txt"], "no such tab");
}

#[test]
fn closing_by_index_stays_in_its_group() {
    let (_world, mut editor) = four_files();
    editor.set_layout(Layout::columns(2));
    editor.move_active_to_group(1);
    editor.close_by_index(&Command::with_args("close_others_by_index", json!({ "group": 0, "index": 0 })));
    assert_eq!(names_in(&editor, 0), ["a.txt"]);
    assert_eq!(names_in(&editor, 1), ["d.txt"]);
}

#[test]
fn a_top_folder_leaves_the_window_and_a_file_opens_its_folder() {
    let world = World::new().with_dir("/p").with_file("/p/a.txt", "a");
    let mut editor = world.editor_opening(&["/p"]);
    editor.open_containing_folder(&Command::with_args("open_containing_folder", json!({ "paths": ["/p/a.txt"] })));
    assert_eq!(*world.opened_folders.borrow(), [PathBuf::from("/p")]);
    editor.remove_folders(&Command::with_args("remove_folder", json!({ "dirs": ["/p"] })));
    assert!(editor.state().folders.is_empty());
}

#[test]
fn the_plugins_are_asked_with_the_filled_arguments_and_commands_that_want_it_get_the_event() {
    let world = World::new()
        .with_file("/p/a.txt", "one\ntwo")
        .with_context_menus(ContextMenus {
            text: vec![entry("goto_thing", json!({})), entry("plain", json!({})), entry("copy", Value::Null)],
            side_bar: vec![entry("side", json!({ "paths": [] }))],
            ..ContextMenus::default()
        })
        .with_plugins(&[("goto_thing", Text), ("plain", Text), ("side", Text)], &["command_states"]);
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let document = editor.active_document().unwrap().id;

    let side = ContextMenuTarget::SideBar { path: PathBuf::from("/p/a.txt"), is_dir: false, root: false };
    world.plugins.then([PluginMessage::Answer(json!([{ "visible": false }]))]);
    editor.open_context_menu(&side);
    let asked = world.plugins.queries.borrow()[0].2[0].clone();
    assert_eq!(asked, json!([["side", { "paths": ["/p/a.txt"] }]]));
    let side_command = editor.context_menu_items(&side)[0].command.clone().unwrap();
    assert!(!editor.plugin_command_state(&side_command).unwrap().visible);

    let text = ContextMenuTarget::Text { document, point: 5 };
    world.plugins.then([PluginMessage::Answer(json!([{ "want_event": true }, {}]))]);
    editor.open_context_menu(&text);
    let items = editor.context_menu_items(&text);
    let args: Vec<Value> = items.iter().map(|i| i.command.clone().unwrap().args).collect();
    // "two" is line 1; char 5 is its column 1: the middle of that char, 8×16 per char.
    assert_eq!(args[0], json!({ "event": { "x": 12.0, "y": 24.0, "modifier_keys": {} } }));
    assert_eq!(args[1], json!({}));
    assert_eq!(args[2], Value::Null, "a built-in command is not asked and gets no event");
    assert!(editor.plugin_command_state(items[0].command.as_ref().unwrap()).is_some(), "found without its event");
}
