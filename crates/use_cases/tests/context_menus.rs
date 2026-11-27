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
    MenuItem {
        caption: command.into(),
        id: None,
        command: Some(Command::with_args(command, args)),
        checkbox: false,
        children: vec![],
    }
}

fn names_in(editor: &Editor, group: usize) -> Vec<String> {
    editor
        .group_indexes(group)
        .into_iter()
        .map(|i| editor.state().documents[i].name())
        .collect()
}

fn four_files() -> (World, Editor) {
    let world = World::new()
        .with_file("/p/a.txt", "a")
        .with_file("/p/b.txt", "b")
        .with_file("/p/c.txt", "c")
        .with_file("/p/d.txt", "d");
    let mut editor = world.editor();
    for file in ["/p/a.txt", "/p/b.txt", "/p/c.txt", "/p/d.txt"] {
        open(&mut editor, file);
    }
    (world, editor)
}

#[test]
fn a_tab_closes_itself_the_others_or_those_to_its_right() {
    let (_world, mut editor) = four_files();
    editor.close_by_index(&Command::with_args(
        "close_by_index",
        json!({ "group": 0, "index": 1 }),
    ));
    assert_eq!(names_in(&editor, 0), ["a.txt", "c.txt", "d.txt"]);
    editor.close_by_index(&Command::with_args(
        "close_to_right_by_index",
        json!({ "group": 0, "index": 1 }),
    ));
    assert_eq!(names_in(&editor, 0), ["a.txt", "c.txt"]);
    editor.close_by_index(&Command::with_args(
        "close_others_by_index",
        json!({ "group": 0, "index": 1 }),
    ));
    assert_eq!(names_in(&editor, 0), ["c.txt"]);
    editor.close_by_index(&Command::with_args(
        "close_by_index",
        json!({ "group": 0, "index": 5 }),
    ));
    assert_eq!(names_in(&editor, 0), ["c.txt"], "no such tab");
}

#[test]
fn closing_by_index_stays_in_its_group() {
    let (_world, mut editor) = four_files();
    editor.set_layout(Layout::columns(2));
    editor.move_active_to_group(1);
    editor.close_by_index(&Command::with_args(
        "close_others_by_index",
        json!({ "group": 0, "index": 0 }),
    ));
    assert_eq!(names_in(&editor, 0), ["a.txt"]);
    assert_eq!(names_in(&editor, 1), ["d.txt"]);
}

#[test]
fn a_top_folder_leaves_the_window_and_a_file_opens_its_folder() {
    let world = World::new().with_dir("/p").with_file("/p/a.txt", "a");
    let mut editor = world.editor_opening(&["/p"]);
    editor.open_containing_folder(&Command::with_args(
        "open_containing_folder",
        json!({ "paths": ["/p/a.txt"] }),
    ));
    assert_eq!(*world.opened_folders.borrow(), [PathBuf::from("/p")]);
    editor.remove_folders(&Command::with_args(
        "remove_folder",
        json!({ "dirs": ["/p"] }),
    ));
    assert!(editor.state().folders.is_empty());
}

#[test]
fn the_plugins_are_asked_with_the_filled_arguments_and_commands_that_want_it_get_the_event() {
    let world = World::new()
        .with_file("/p/a.txt", "one\ntwo")
        .with_context_menus(ContextMenus {
            text: vec![
                entry("goto_thing", json!({})),
                entry("plain", json!({})),
                entry("copy", Value::Null),
            ],
            side_bar: vec![entry("side", json!({ "paths": [] }))],
            ..ContextMenus::default()
        })
        .with_plugins(
            &[("goto_thing", Text), ("plain", Text), ("side", Text)],
            &["command_states"],
        );
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    let document = editor.active_document().unwrap().id;

    let side = ContextMenuTarget::SideBar {
        path: PathBuf::from("/p/a.txt"),
        is_dir: false,
        root: false,
    };
    world
        .plugins
        .then([PluginMessage::Answer(json!([{ "visible": false }]))]);
    editor.open_context_menu(&side);
    let asked = world.plugins.queries.borrow()[0].2[0].clone();
    assert_eq!(asked, json!([["side", { "paths": ["/p/a.txt"] }]]));
    let side_command = editor.context_menu_items(&side)[0].command.clone().unwrap();
    assert!(!editor.plugin_command_state(&side_command).unwrap().visible);

    let text = ContextMenuTarget::Text {
        document,
        point: 5,
        modifier_keys: vec!["ctrl".into(), "shift".into()],
    };
    world
        .plugins
        .then([PluginMessage::Answer(json!([{ "want_event": true }, {}]))]);
    editor.open_context_menu(&text);
    let items = editor.context_menu_items(&text);
    let args: Vec<Value> = items
        .iter()
        .map(|i| i.command.clone().unwrap().args)
        .collect();
    // "two" is line 1; char 5 is its column 1: the middle of that char, 8×16 per char.
    assert_eq!(
        args[0],
        json!({ "event": { "x": 12.0, "y": 24.0, "modifier_keys": { "ctrl": true, "shift": true } } })
    );
    assert_eq!(args[1], json!({}));
    assert_eq!(
        args[2],
        Value::Null,
        "a built-in command is not asked and gets no event"
    );
    assert!(
        editor
            .plugin_command_state(items[0].command.as_ref().unwrap())
            .is_some(),
        "found without its event"
    );
}

#[test]
fn a_tab_closes_the_unmodified_tabs_and_those_of_deleted_files() {
    let (_world, mut editor) = four_files();
    // b and d are modified.
    for name in ["b.txt", "d.txt"] {
        let id = editor
            .state()
            .documents
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id;
        editor.text_for_editing(id).unwrap().push('!');
    }
    editor.close_by_index(&Command::with_args(
        "close_unmodified_to_right_by_index",
        json!({ "group": 0, "index": 1 }),
    ));
    assert_eq!(names_in(&editor, 0), ["a.txt", "b.txt", "d.txt"], "c went");
    editor.close_by_index(&Command::with_args(
        "close_unmodified",
        json!({ "group": 0, "index": 0 }),
    ));
    assert_eq!(names_in(&editor, 0), ["b.txt", "d.txt"]);

    let (world, mut editor) = four_files();
    world.disk.remove_dir("/p/c.txt");
    editor.close_by_index(&Command::with_args(
        "close_deleted_files",
        json!({ "group": 0 }),
    ));
    assert_eq!(
        names_in(&editor, 0),
        ["a.txt", "b.txt", "d.txt"],
        "c is no longer on disk"
    );
}

/// Sublime 4215 closes the group's selected tabs (or the others) whichever tab was clicked.
#[test]
fn close_selected_and_unselected_are_the_selected_tabs_and_the_others() {
    let (_world, mut editor) = four_files();
    let id = |editor: &Editor, i: usize| editor.state().documents[i].id;
    let (b, d) = (id(&editor, 1), id(&editor, 3));
    editor.select_documents(&[b, d]);
    editor.close_by_index(&Command::with_args(
        "close_unselected",
        json!({ "group": 0, "index": 0 }),
    ));
    assert_eq!(names_in(&editor, 0), ["b.txt", "d.txt"]);

    let (_world, mut editor) = four_files();
    let (b, d) = (id(&editor, 1), id(&editor, 3));
    editor.select_documents(&[b, d]);
    editor.close_by_index(&Command::with_args(
        "close_selected",
        json!({ "group": 0, "index": 0 }),
    ));
    assert_eq!(names_in(&editor, 0), ["a.txt", "c.txt"]);

    // With one tab selected, it is the active one.
    editor.close_by_index(&Command::with_args(
        "close_selected",
        json!({ "group": 0, "index": 0 }),
    ));
    assert_eq!(names_in(&editor, 0).len(), 1);
}

#[test]
fn an_image_s_menu_copies_its_path_opens_its_folder_and_reopens_it_as_text() {
    let world = World::new()
        .with_file("/p/logo.gif", "GIF89a text")
        .with_file("/p/photo.png", vec![0x89, b'P', b'N', b'G', 0, 0, 0, 1]);
    let mut editor = world.editor();
    open(&mut editor, "/p/logo.gif");
    let image = editor.active_document().unwrap().id;
    assert_eq!(
        editor.active_document().unwrap().sheet,
        entities::document::SheetKind::Image
    );

    editor.copy_file_path();
    assert_eq!(
        *world.clipboard.borrow(),
        Ok(PathBuf::from("/p/logo.gif").display().to_string())
    );

    editor.open_package_folder("$file_path", "$file_name");
    assert_eq!(
        *world.selected_files.borrow(),
        [PathBuf::from("/p/logo.gif")],
        "the window's variables, as Sublime expands them"
    );

    editor.reopen_as_text();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.sheet, entities::document::SheetKind::Text);
    assert_eq!(doc.text, "GIF89a text");
    assert_ne!(doc.id, image, "a new view in its place");
    assert_eq!(names_in(&editor, 0), ["logo.gif"]);

    open(&mut editor, "/p/photo.png");
    editor.reopen_as_text();
    assert_eq!(
        editor.active_document().unwrap().sheet,
        entities::document::SheetKind::Image,
        "a binary file stays a picture"
    );
}

#[test]
fn a_console_line_is_echoed_and_runs_in_the_plugin_host() {
    let world = World::new().with_plugins(&[], &[]);
    let mut editor = world.editor();
    world.plugins.then([
        PluginMessage::Log("2\n".into()),
        PluginMessage::Done { error: None },
    ]);
    editor.run_console_line("1 + 1", &mut |_: &mut Editor, _: &Command| {});
    assert_eq!(*world.plugins.console_lines.borrow(), ["1 + 1"]);
    let console = &editor.state().console;
    assert!(console.ends_with(">>> 1 + 1\n2\n"), "{console:?}");
    editor.run_console_line("  ", &mut |_: &mut Editor, _: &Command| {});
    assert_eq!(
        world.plugins.console_lines.borrow().len(),
        1,
        "nothing to run"
    );
}
