//! Tests for plugins' popups, phantoms and `on_hover` (`use_cases::plugin_popups`), with a scripted host.

use entities::command::Command;
use entities::selection::Selection;
use serde_json::{json, Value};
use test_support::{plugin_call, World, PACKAGES};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::PluginMessage;
use use_cases::plugin_popups::HoverZone;
use use_cases::Editor;

fn done() -> PluginMessage {
    PluginMessage::Done { error: None }
}

/// The editor's answer when a plugin calls `function(args...)`.
fn call(world: &World, editor: &mut Editor, function: &str, args: Value) -> Result<Value, String> {
    world.plugins.then([plugin_call(function, args), done()]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    world.plugins.replies.borrow_mut().pop().expect("the editor answered")
}

fn editor_with_text(world: &World, text: &str) -> (Editor, u64) {
    let mut editor = world.editor();
    editor.insert_at_selections(text);
    let id = editor.active_document().unwrap().id;
    (editor, id)
}

fn callbacks(world: &World) -> Vec<(u64, String, Value)> {
    world.plugins.callbacks.borrow().clone()
}

#[test]
fn a_popup_is_shown_updated_and_hidden_when_the_caret_moves() {
    let world = World::new().with_plugin_commands(&[("probe", Text)]);
    let (mut editor, v) = editor_with_text(&world, "hello");
    assert_eq!(call(&world, &mut editor, "view_show_popup", json!([v, "<b>hi</b>", 0, -1, 0, 0, 7])), Ok(Value::Null));
    let popup = editor.plugin_popup().expect("shown");
    assert_eq!((popup.view, popup.location, popup.max_width, popup.content.as_str()), (v, 5, 320.0, "<b>hi</b>"), "at the caret");
    assert_eq!(call(&world, &mut editor, "view_is_popup_visible", json!([v])), Ok(json!(true)));
    call(&world, &mut editor, "view_update_popup", json!([v, "new"])).unwrap();
    assert_eq!(editor.plugin_popup().unwrap().content, "new");

    // A link in it.
    world.plugins.then([done()]);
    editor.navigate_plugin_popup("subl:go", &mut |_, _| {});
    assert_eq!(callbacks(&world).last(), Some(&(7, "navigate".into(), json!(["subl:go"]))));

    // The caret moves: it goes, and `on_hide` runs at the next poll.
    editor.sync_selection_from_view(v, Selection::caret(0));
    world.plugins.then([done()]);
    editor.poll_plugins(&mut |_, _| {});
    assert!(editor.plugin_popup().is_none());
    assert_eq!(callbacks(&world).last(), Some(&(7, "hide".into(), json!([]))));
    assert_eq!(call(&world, &mut editor, "view_is_popup_visible", json!([v])), Ok(json!(false)));
}

#[test]
fn a_popup_kept_on_selection_changes_goes_with_esc_or_another_popup() {
    let world = World::new().with_plugin_commands(&[("probe", Text)]);
    let (mut editor, v) = editor_with_text(&world, "hello");
    // KEEP_ON_SELECTION_MODIFIED, at char 2, no callbacks.
    call(&world, &mut editor, "view_show_popup", json!([v, "a", 16, 2, 100, 50, null])).unwrap();
    editor.sync_selection_from_view(v, Selection::caret(1));
    editor.poll_plugins(&mut |_, _| {});
    let popup = editor.plugin_popup().expect("kept");
    assert_eq!((popup.location, popup.max_width, popup.max_height), (2, 100.0, 50.0));

    // Another one replaces it; its `on_hide` runs.
    call(&world, &mut editor, "view_show_popup", json!([v, "b", 16, 2, 100, 50, 3])).unwrap();
    call(&world, &mut editor, "view_show_popup", json!([v, "c", 16, 2, 100, 50, 4])).unwrap();
    world.plugins.then([done()]);
    editor.poll_plugins(&mut |_, _| {});
    assert_eq!(editor.plugin_popup().unwrap().content, "c");
    assert_eq!(callbacks(&world), vec![(3, "hide".into(), json!([]))]);

    // Esc.
    editor.hide_plugin_popup();
    world.plugins.then([done()]);
    editor.poll_plugins(&mut |_, _| {});
    assert!(editor.plugin_popup().is_none());
    assert_eq!(callbacks(&world).last(), Some(&(4, "hide".into(), json!([]))));
}

#[test]
fn phantoms_move_with_edits_answer_their_links_and_are_erased() {
    let world = World::new().with_plugin_commands(&[("probe", Text)]);
    let (mut editor, v) = editor_with_text(&world, "one two");
    let first = call(&world, &mut editor, "view_add_phantom", json!([v, "k", 4, 7, "<i>x</i>", 2, 5])).unwrap();
    let second = call(&world, &mut editor, "view_add_phantom", json!([v, "k", 0, 3, "y", 1, null])).unwrap();
    assert_ne!(first, second);
    call(&world, &mut editor, "view_insert", json!([v, 0, ">> "])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_query_phantom", json!([v, first])), Ok(json!([[7, 10]])));
    assert_eq!(call(&world, &mut editor, "view_query_phantoms", json!([v, [first, second, 99]])), Ok(json!([[7, 10], [3, 6]])));

    world.plugins.then([done()]);
    editor.navigate_plugin_phantom(first.as_u64().unwrap(), "subl:fix", &mut |_, _| {});
    assert_eq!(callbacks(&world), vec![(5, "navigate".into(), json!(["subl:fix"]))]);

    call(&world, &mut editor, "view_erase_phantom_by_id", json!([v, first])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_query_phantom", json!([v, first])), Ok(json!([])));
    call(&world, &mut editor, "view_erase_phantoms", json!([v, "k"])).unwrap();
    assert!(editor.active_document().unwrap().phantoms().is_empty());
}

#[test]
fn on_hover_reaches_the_listeners_that_want_it() {
    let world = World::new().with_file(&format!("{PACKAGES}/P/p.py"), "");
    world.plugins.then([PluginMessage::Loaded { commands: vec![], events: vec!["on_hover".into()], errors: vec![] }]);
    let (mut editor, v) = editor_with_text(&world, "hello");
    assert!(editor.plugins_hear_hover());
    world.plugins.then([PluginMessage::Answer(Value::Null)]);
    editor.plugin_hover(v, 3, HoverZone::Text);
    let queries = world.plugins.queries.borrow();
    assert_eq!(queries.last(), Some(&("on_hover".to_string(), Some(v), json!([3, 1]))));
}

// ----- region icons ---------------------------------------------------------------------------------

#[test]
fn a_region_icon_is_a_resource_its_2x_picture_first() {
    let world = World::new()
        .with_resource("Packages/Gutter/icons/added.png", "1x")
        .with_resource("Packages/Gutter/icons/added@2x.png", "2x")
        .with_resource("Packages/Gutter/icons/removed.png", "removed");
    let editor = world.editor();
    let bytes = |icon: &str| editor.region_icon(icon).map(|b| String::from_utf8(b.to_vec()).unwrap());
    assert_eq!(bytes("Packages/Gutter/icons/added.png").as_deref(), Some("2x"));
    assert_eq!(bytes("Packages/Gutter/icons/removed").as_deref(), Some("removed"), "without its .png");
    assert_eq!(bytes("Packages/Gutter/icons/missing.png"), None);
    for built_in in ["dot", "circle", "bookmark", ""] {
        assert_eq!(bytes(built_in), None, "{built_in} is drawn, not read");
    }
}
