//! Tests for the groups of the window (`use_cases::groups`): layouts, focus, moving views.

use entities::layout::Layout;
use entities::command::Command;
use serde_json::{json, Value};
use test_support::{open, plugin_call, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
use use_cases::editing::TextCommand;
use use_cases::Editor;

fn names_in(editor: &Editor, group: usize) -> Vec<String> {
    editor.group_indexes(group).into_iter().map(|i| editor.state().documents[i].name()).collect()
}

fn active_name(editor: &Editor) -> Option<String> {
    editor.active_document().map(|d| d.name())
}

/// Two columns: a.txt and b.txt on the left, c.txt moved to the right.
fn two_columns(world: &World) -> Editor {
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    open(&mut editor, "/p/b.txt");
    open(&mut editor, "/p/c.txt");
    editor.set_layout(Layout::columns(2));
    editor.move_active_to_group(1);
    editor
}

fn world() -> World {
    World::new().with_file("/p/a.txt", "a").with_file("/p/b.txt", "b").with_file("/p/c.txt", "c").with_file("/p/d.txt", "d")
}

#[test]
fn moving_a_file_to_another_group_takes_the_focus_along() {
    let world = world();
    let editor = two_columns(&world);
    assert_eq!(editor.group_count(), 2);
    assert_eq!(names_in(&editor, 0), vec!["a.txt", "b.txt"]);
    assert_eq!(names_in(&editor, 1), vec!["c.txt"]);
    assert_eq!(editor.active_group(), 1);
    assert_eq!(active_name(&editor).as_deref(), Some("c.txt"));
    // The left group still shows what it showed last.
    let shown = editor.active_index_in_group(0).unwrap();
    assert_eq!(editor.state().documents[shown].name(), "b.txt");
}

#[test]
fn files_open_in_the_focused_group() {
    let world = world();
    let mut editor = two_columns(&world);
    editor.focus_group(0);
    assert_eq!(active_name(&editor).as_deref(), Some("b.txt"));
    open(&mut editor, "/p/d.txt");
    assert_eq!(names_in(&editor, 0), vec!["a.txt", "b.txt", "d.txt"]);
    editor.focus_group(1);
    editor.new_file();
    assert_eq!(names_in(&editor, 1), vec!["c.txt", "untitled"]);
    assert_eq!(editor.active_group(), 1);
}

#[test]
fn a_focused_empty_group_has_no_active_document() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.txt");
    editor.set_layout(Layout::columns(3));
    editor.focus_group(2);
    assert_eq!(editor.active_group(), 2);
    assert!(editor.active_document().is_none());
    // Text commands have nothing to act on; the other group is untouched.
    editor.run_text_command(TextCommand::Insert("x".into()));
    assert_eq!(editor.state().documents[0].text, "a");
    open(&mut editor, "/p/b.txt");
    assert_eq!(names_in(&editor, 2), vec!["b.txt"]);
    // A group past the layout is ignored.
    editor.focus_group(7);
    assert_eq!(editor.active_group(), 2);
}

#[test]
fn closing_the_last_file_of_a_group_keeps_its_focus() {
    let world = world();
    let mut editor = two_columns(&world);
    editor.close_active_document();
    assert_eq!(editor.active_group(), 1);
    assert!(editor.active_document().is_none());
    assert_eq!(names_in(&editor, 0), vec!["a.txt", "b.txt"]);
    // Closing a file of another group leaves the focus alone.
    let a = editor.state().documents[0].id;
    editor.close_documents(vec![a]);
    assert_eq!(editor.active_group(), 1);
    assert!(editor.active_document().is_none());
}

#[test]
fn fewer_groups_move_their_files_to_the_last_one() {
    let world = world();
    let mut editor = two_columns(&world);
    editor.focus_group(0);
    editor.set_layout(Layout::default());
    assert_eq!(names_in(&editor, 0), vec!["a.txt", "b.txt", "c.txt"]);
    assert_eq!(active_name(&editor).as_deref(), Some("b.txt"));

    let mut editor = two_columns(&world);
    editor.set_layout(Layout::default());
    assert_eq!(editor.active_group(), 0, "the focused document went along");
    assert_eq!(active_name(&editor).as_deref(), Some("c.txt"));
}

#[test]
fn tab_keys_stay_in_the_focused_group() {
    let world = world();
    let mut editor = two_columns(&world);
    editor.focus_group(0);
    editor.cycle_documents(true);
    assert_eq!(active_name(&editor).as_deref(), Some("a.txt"));
    editor.cycle_documents(true);
    assert_eq!(active_name(&editor).as_deref(), Some("b.txt"));
    editor.activate_in_group(0);
    assert_eq!(active_name(&editor).as_deref(), Some("a.txt"));
    editor.focus_group(1);
    editor.activate_in_group(1);
    assert_eq!(active_name(&editor).as_deref(), Some("c.txt"), "group 1 has one tab");
    editor.focus_neighboring_group(true);
    assert_eq!(editor.active_group(), 0);
    editor.move_active_to_neighboring_group(false);
    assert_eq!(names_in(&editor, 1), vec!["c.txt", "a.txt"]);
}

/// The editor's answer when a plugin calls `function(args...)`.
fn call(world: &World, editor: &mut Editor, function: &str, args: Value) -> Result<Value, String> {
    world.plugins.then([plugin_call(function, args), PluginMessage::Done { error: None }]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    world.plugins.replies.borrow_mut().pop().expect("the editor answered")
}

#[test]
fn plugins_see_and_change_the_groups() {
    let world = world().with_plugin_commands(&[("probe", Text)]);
    let mut editor = two_columns(&world);
    assert_eq!(call(&world, &mut editor, "window_num_groups", json!([])), Ok(json!(2)));
    assert_eq!(call(&world, &mut editor, "window_active_group", json!([])), Ok(json!(1)));
    assert_eq!(call(&world, &mut editor, "window_get_layout", json!([])), Ok(Layout::columns(2).to_json()));
    let ids: Vec<u64> = editor.state().documents.iter().map(|d| d.id).collect();
    assert_eq!(call(&world, &mut editor, "window_views_in_group", json!([0])), Ok(json!([ids[0], ids[1]])));
    assert_eq!(call(&world, &mut editor, "window_active_view_in_group", json!([0])), Ok(json!(ids[1])));
    assert_eq!(call(&world, &mut editor, "window_get_view_index", json!([ids[2]])), Ok(json!([1, 0])));
    assert_eq!(call(&world, &mut editor, "window_get_view_index", json!([999])), Ok(json!([-1, -1])));
    // A view moved to another group: the focus stays.
    call(&world, &mut editor, "window_set_view_index", json!([ids[0], 1, 0])).unwrap();
    assert_eq!(call(&world, &mut editor, "window_views_in_group", json!([1])), Ok(json!([ids[0], ids[2]])));
    assert_eq!(editor.active_group(), 1);
    call(&world, &mut editor, "window_focus_group", json!([0])).unwrap();
    assert_eq!(active_name(&editor).as_deref(), Some("b.txt"));
    call(&world, &mut editor, "window_set_layout", json!([Layout::grid().to_json()])).unwrap();
    assert_eq!(editor.group_count(), 4);
    assert!(call(&world, &mut editor, "window_set_layout", json!([{ "cols": [0.0] }])).is_err());
}

#[test]
fn the_session_keeps_the_groups() {
    let world = world();
    let mut editor = two_columns(&world);
    editor.save_session(Vec::new());
    let session = world.session.borrow().clone().unwrap();
    assert_eq!(session.groups, Some(Layout::columns(2)));
    assert_eq!(session.documents.iter().map(|d| d.group).collect::<Vec<_>>(), vec![0, 0, 1]);

    let restored = world.with_session(session);
    let editor = restored.editor();
    assert_eq!(editor.group_count(), 2);
    assert_eq!(names_in(&editor, 1), vec!["c.txt"]);
    assert_eq!(editor.active_group(), 1);
}
