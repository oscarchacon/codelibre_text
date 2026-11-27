//! Tests for multi-selected tabs (`use_cases::tab_selection`). The answers are Sublime 4215's
//! (a probe plugin in a portable copy: six scratch views, `select_sheets`, `focus_view`, then a
//! command or a close).

use entities::command::Command;
use entities::document::DocumentId;
use entities::layout::Layout;
use entities::tab_selection::TabSelectionCommand as C;
use serde_json::{json, Value};
use test_support::{open, plugin_call, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
use use_cases::Editor;

const NAMES: [&str; 6] = ["t0", "t1", "t2", "t3", "t4", "t5"];

fn six_tabs() -> (World, Editor) {
    let mut world = World::new().with_plugin_commands(&[("probe", Text)]);
    for name in NAMES {
        world = world.with_file(&format!("/p/{name}"), name);
    }
    let mut editor = world.editor();
    for name in NAMES {
        open(&mut editor, &format!("/p/{name}"));
    }
    (world, editor)
}

fn id(editor: &Editor, name: &str) -> DocumentId {
    editor
        .state()
        .documents
        .iter()
        .find(|d| d.name() == name)
        .expect("open")
        .id
}

/// `select_sheets([…])` then `focus_view(focus)`.
fn select(editor: &mut Editor, names: &[&str], focus: &str) {
    let ids: Vec<DocumentId> = names.iter().map(|n| id(editor, n)).collect();
    editor.select_documents(&ids);
    let focus = id(editor, focus);
    editor.activate_document(focus);
}

fn names(editor: &Editor, indexes: Vec<usize>) -> Vec<String> {
    indexes
        .into_iter()
        .map(|i| editor.state().documents[i].name())
        .collect()
}

fn selected(editor: &Editor) -> Vec<String> {
    names(
        editor,
        editor.selected_indexes_in_group(editor.active_group()),
    )
}

fn tabs(editor: &Editor, group: usize) -> Vec<String> {
    names(editor, editor.group_indexes(group))
}

fn active(editor: &Editor) -> String {
    editor.active_document().unwrap().name()
}

/// The focused group's selected tabs are `selected`, `focused` with the focus.
fn check(editor: &Editor, selected_tabs: &[&str], focused: &str) {
    assert_eq!(selected(editor), selected_tabs);
    assert_eq!(active(editor), focused);
}

#[test]
fn select_sheets_selects_them_and_focuses_the_last_unless_the_focused_one_is_among_them() {
    let (_world, mut editor) = six_tabs();
    assert_eq!(selected(&editor), ["t5"]);
    assert!(!editor.group_has_multiselect());

    let ids = [id(&editor, "t3"), id(&editor, "t1")];
    editor.select_documents(&ids);
    assert_eq!(selected(&editor), ["t1", "t3"]);
    assert_eq!(active(&editor), "t1");
    assert!(editor.group_has_multiselect());

    let ids = [id(&editor, "t0"), id(&editor, "t1")];
    editor.select_documents(&ids);
    check(&editor, &["t0", "t1"], "t1");
}

#[test]
fn focusing_a_selected_tab_keeps_the_selection_and_another_tab_drops_it() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    let t1 = id(&editor, "t1");
    editor.activate_document(t1);
    check(&editor, &["t1", "t3"], "t1");
    assert_eq!(names(&editor, editor.shown_indexes()), ["t1", "t3"]);

    let t4 = id(&editor, "t4");
    editor.activate_document(t4);
    assert_eq!(selected(&editor), ["t4"]);
    assert_eq!(names(&editor, editor.shown_indexes()), ["t4"]);
}

#[test]
fn tab_selection_commands_work_on_the_focused_group() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t2", "t4"], "t2");
    editor.run_tab_selection_command(C::SelectToRight);
    check(&editor, &["t1", "t2", "t4", "t5"], "t5");
    editor.run_tab_selection_command(C::FocusByIndex(1));
    assert_eq!(active(&editor), "t2");
    editor.run_tab_selection_command(C::UnselectCurrent);
    check(&editor, &["t1", "t4", "t5"], "t4");
    editor.run_tab_selection_command(C::UnselectToLeft);
    assert_eq!(selected(&editor), ["t4", "t5"]);
    editor.run_tab_selection_command(C::UnselectOthers);
    assert_eq!(selected(&editor), ["t4"]);
    assert!(!editor.group_has_multiselect());
}

#[test]
fn closing_the_focused_selected_tab_focuses_the_next_selected_one() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3", "t4"], "t3");
    editor.close_active_document();
    check(&editor, &["t1", "t4"], "t4");
    // Closing a tab that is not selected leaves the selection alone.
    let t0 = id(&editor, "t0");
    editor.close_documents(vec![t0]);
    check(&editor, &["t1", "t4"], "t4");

    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3", "t4"], "t4");
    editor.close_active_document();
    check(&editor, &["t1", "t3"], "t3");
}

#[test]
fn next_view_collapses_the_selection_towards_its_way_first() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    editor.cycle_documents(true);
    check(&editor, &["t3"], "t3");
    editor.cycle_documents(true);
    assert_eq!(active(&editor), "t4");

    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t1");
    editor.cycle_documents(true);
    check(&editor, &["t3"], "t3");

    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    editor.cycle_documents(false);
    check(&editor, &["t1"], "t1");
}

#[test]
fn select_by_index_and_select_last_tab_leave_one_tab_selected() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    editor.activate_in_group(1);
    check(&editor, &["t1"], "t1");
    select(&mut editor, &["t1", "t3"], "t3");
    editor.activate_last_in_group();
    check(&editor, &["t5"], "t5");
}

#[test]
fn ctrl_and_shift_clicks_on_tabs() {
    let (_world, mut editor) = six_tabs();
    let index = |editor: &Editor, name| {
        editor
            .state()
            .documents
            .iter()
            .position(|d| d.name() == name)
            .unwrap()
    };
    editor.click_tab(index(&editor, "t1"), false, false);
    editor.click_tab(index(&editor, "t3"), true, false);
    check(&editor, &["t1", "t3"], "t3");
    editor.click_tab(index(&editor, "t1"), true, false);
    assert_eq!(selected(&editor), ["t3"]);
    editor.click_tab(index(&editor, "t5"), false, true);
    check(&editor, &["t3", "t4", "t5"], "t5");
    // A plain click on a selected tab focuses it, the others stay selected.
    editor.click_tab(index(&editor, "t4"), false, false);
    assert_eq!((selected(&editor).len(), active(&editor)), (3, "t4".into()));
}

#[test]
fn move_to_group_takes_every_selected_tab_and_new_pane_too() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    editor.new_pane(true);
    assert_eq!(tabs(&editor, 0), ["t0", "t2", "t4", "t5"]);
    assert_eq!(tabs(&editor, 1), ["t1", "t3"]);
    assert_eq!(
        (editor.active_group(), selected(&editor), active(&editor)),
        (1, vec!["t1".to_string(), "t3".into()], "t3".into())
    );
    // Each group has its own selection.
    editor.focus_group(0);
    assert_eq!(selected(&editor), ["t5"]);
    assert_eq!(
        names(&editor, editor.selected_indexes_in_group(1)),
        ["t1", "t3"]
    );

    let (_world, mut editor) = six_tabs();
    editor.set_layout(Layout::columns(2));
    select(&mut editor, &["t1", "t3"], "t3");
    editor.move_active_to_group(1);
    assert_eq!(tabs(&editor, 1), ["t1", "t3"]);
    assert_eq!(selected(&editor), ["t1", "t3"]);
}

#[test]
fn moving_a_tab_or_merging_groups_leaves_one_tab_selected() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    let t0 = id(&editor, "t0");
    editor.set_view_index(t0, 0, 5);
    assert_eq!(selected(&editor), ["t3"]);

    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    editor.new_pane(true);
    editor.set_layout(Layout::columns(1));
    assert_eq!(editor.selected_indexes_in_group(0).len(), 1);
}

#[test]
fn split_view_clones_the_tab_beside_it() {
    let (_world, mut editor) = six_tabs();
    let t1 = id(&editor, "t1");
    editor.activate_document(t1);
    let clone = editor.clone_into_selection().unwrap();
    assert_eq!(editor.active_document().unwrap().id, clone);
    assert_eq!(selected(&editor), ["t1", "t1"]);
}

#[test]
fn the_commands_by_name_and_the_multiselect_context() {
    let (_world, mut editor) = six_tabs();
    select(&mut editor, &["t1", "t3"], "t3");
    let run = |editor: &mut Editor, name: &str, args: Value| {
        let command = Command::with_args(name, args);
        let tabs = C::from_name(&command.name, command.usize_arg("index", 0)).unwrap();
        editor.run_tab_selection_command(tabs);
    };
    run(&mut editor, "focus_by_index", json!({ "index": 0 }));
    assert_eq!(active(&editor), "t1");
    run(&mut editor, "select_to_left", json!({}));
    assert_eq!(selected(&editor), ["t0", "t1", "t3"]);
}

fn call(world: &World, editor: &mut Editor, function: &str, args: Value) -> Value {
    world.plugins.then([
        plugin_call(function, args),
        PluginMessage::Done { error: None },
    ]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    world
        .plugins
        .replies
        .borrow_mut()
        .pop()
        .expect("the editor answered")
        .expect("no error")
}

#[test]
fn plugins_read_and_set_the_selected_sheets() {
    let (world, mut editor) = six_tabs();
    let (t1, t3) = (id(&editor, "t1"), id(&editor, "t3"));
    call(
        &world,
        &mut editor,
        "window_select_sheets",
        json!([[t1, t3]]),
    );
    assert_eq!(selected(&editor), ["t1", "t3"]);
    assert_eq!(
        call(
            &world,
            &mut editor,
            "window_selected_sheets_in_group",
            json!([0])
        ),
        json!([t1, t3])
    );
}
