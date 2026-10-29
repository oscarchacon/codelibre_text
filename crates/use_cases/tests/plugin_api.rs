//! Tests for the `sublime_api` functions plugins call (`use_cases::plugin_api`). Each test makes
//! the scripted plugin host call one function and checks the editor's answer.

use entities::command::Command;
use entities::selection::Selection;
use serde_json::{json, Value};
use test_support::{active_text, plugin_call, status, World};
use use_cases::gateways::PluginCommandKind::Text;
use use_cases::gateways::{MessageAnswer, MessageButtons, MessageDialog, MessageLevel, PluginMessage};
use use_cases::Editor;

fn editor_with(world: &World, text: &str) -> Editor {
    let mut editor = world.editor();
    editor.insert_at_selections(text);
    editor
}

fn view_id(editor: &Editor) -> u64 {
    editor.active_document().unwrap().id
}

/// The editor's answer when a plugin calls `function(args...)`.
fn call(world: &World, editor: &mut Editor, function: &str, args: Value) -> Result<Value, String> {
    world.plugins.then([plugin_call(function, args), PluginMessage::Done { error: None }]);
    editor.run_plugin_command(&Command::new("probe"), &mut |_, _| {});
    world.plugins.replies.borrow_mut().pop().expect("the editor answered")
}

fn plugin_world() -> World {
    World::new().with_plugin_commands(&[("probe", Text)])
}

#[test]
fn reading_the_text() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "héllo\nworld");
    let v = view_id(&editor);
    assert_eq!(call(&world, &mut editor, "view_size", json!([v])), Ok(json!(11)));
    assert_eq!(call(&world, &mut editor, "view_cached_substr", json!([v, 1, 4])), Ok(json!("éll")));
    assert_eq!(call(&world, &mut editor, "view_cached_substr", json!([v, 9, 99])), Ok(json!("ld")), "clamped");
    assert_eq!(call(&world, &mut editor, "view_rowcol", json!([v, 8])), Ok(json!([1, 2])));
    assert_eq!(call(&world, &mut editor, "view_text_point", json!([v, 1, 2])), Ok(json!(8)));
}

#[test]
fn lines_and_words() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "one two\nthree\nfour");
    let v = view_id(&editor);
    assert_eq!(call(&world, &mut editor, "view_line", json!([v, 5, 5])), Ok(json!([0, 7])));
    assert_eq!(call(&world, &mut editor, "view_full_line", json!([v, 5, 5])), Ok(json!([0, 8])));
    assert_eq!(call(&world, &mut editor, "view_full_line", json!([v, 15, 15])), Ok(json!([14, 18])), "last line has no newline");
    assert_eq!(call(&world, &mut editor, "view_lines", json!([v, 2, 10])), Ok(json!([[0, 7], [8, 13]])));
    assert_eq!(call(&world, &mut editor, "view_word", json!([v, 5, 5])), Ok(json!([4, 7])));
}

#[test]
fn inserting_erasing_and_replacing_move_the_selections() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "abcdef");
    let v = view_id(&editor);
    editor.sync_selection_from_view(v, Selection::caret(4));
    assert_eq!(call(&world, &mut editor, "view_insert", json!([v, 4, "XY"])), Ok(json!(2)));
    assert_eq!(active_text(&editor), "abcdXYef");
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(6), "a caret at the insertion moves after it");

    call(&world, &mut editor, "view_erase", json!([v, 0, 2])).unwrap();
    assert_eq!(active_text(&editor), "cdXYef");
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(4));

    call(&world, &mut editor, "view_replace", json!([v, 2, 4, "-"])).unwrap();
    assert_eq!(active_text(&editor), "cd-ef");
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(3), "inside the replaced text: its end");
}

#[test]
fn read_only_views_are_not_changed() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "abc");
    let v = view_id(&editor);
    call(&world, &mut editor, "view_set_read_only", json!([v, true])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_is_read_only", json!([v])), Ok(json!(true)));
    assert_eq!(call(&world, &mut editor, "view_insert", json!([v, 0, "x"])), Ok(json!(0)));
    assert_eq!(active_text(&editor), "abc");
}

#[test]
fn selections_are_read_and_replaced() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "abcdef");
    let v = view_id(&editor);
    call(&world, &mut editor, "view_selection_set", json!([v, [[0, 1], [5, 3]]])).unwrap();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::new(5, 3), "the last region is the primary one");
    assert_eq!(doc.extra_selections(), &[Selection::new(0, 1)]);
    assert_eq!(call(&world, &mut editor, "view_selection_get", json!([v])), Ok(json!([[0, 1], [5, 3]])));
}

#[test]
fn find_and_find_all_with_sublime_flags() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "a.b A.B a.b");
    let v = view_id(&editor);
    assert_eq!(call(&world, &mut editor, "view_find", json!([v, "a.b", 1, 0])), Ok(json!([8, 11])), "regex, from 1");
    assert_eq!(call(&world, &mut editor, "view_find", json!([v, "a.b", 1, 2])), Ok(json!([4, 7])), "IGNORECASE");
    assert_eq!(call(&world, &mut editor, "view_find", json!([v, "zzz", 0, 0])), Ok(json!([-1, -1])));
    assert_eq!(call(&world, &mut editor, "view_find_all", json!([v, "a.b", 1])), Ok(json!([[0, 3], [8, 11]])), "LITERAL");
    assert!(call(&world, &mut editor, "view_find_all", json!([v, "(", 0])).is_err(), "bad regex");
}

#[test]
fn view_properties() {
    let world = plugin_world().with_file("/p/a.rs", "fn main() {}\n");
    let mut editor = world.editor();
    test_support::open(&mut editor, "/p/a.rs");
    let v = view_id(&editor);
    assert_eq!(call(&world, &mut editor, "view_file_name", json!([v])), Ok(json!("/p/a.rs")));
    assert_eq!(call(&world, &mut editor, "view_syntax", json!([v])), Ok(json!("Rust")));
    assert_eq!(call(&world, &mut editor, "view_encoding", json!([v])), Ok(json!("UTF-8")));
    assert_eq!(call(&world, &mut editor, "view_line_endings", json!([v])), Ok(json!("Unix")));
    call(&world, &mut editor, "view_set_line_endings", json!([v, "Windows"])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_is_dirty", json!([v])), Ok(json!(true)));
    call(&world, &mut editor, "view_assign_syntax", json!([v, "Packages/Python/Python.sublime-syntax"])).unwrap();
    assert_eq!(editor.active_document().unwrap().syntax, "Python");
    call(&world, &mut editor, "view_set_name", json!([v, "Results"])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_name", json!([v])), Ok(json!("Results")));
}

#[test]
fn closed_views_and_unknown_functions() {
    let world = plugin_world();
    let mut editor = world.editor();
    assert_eq!(call(&world, &mut editor, "view_is_valid", json!([999])), Ok(json!(false)));
    assert_eq!(call(&world, &mut editor, "view_size", json!([999])), Err("the view is closed".into()));
    assert_eq!(
        call(&world, &mut editor, "window_set_layout", json!([1, {}])),
        Err("sublime_api.window_set_layout is not implemented yet".into())
    );
    assert_eq!(call(&world, &mut editor, "view_size", json!(["x"])), Err("argument 1 must be a number".into()));
    assert_eq!(
        call(&world, &mut editor, "window_show_quick_panel", json!([1, [["a"]], 0, 0, "", "x"])),
        Err("argument 6 must be a callback id".into())
    );
}

#[test]
fn the_window_and_its_views() {
    let world = plugin_world().with_file("/p/a.txt", "a").with_dir("/p");
    let mut editor = world.editor();
    editor.add_folder("/p".into());
    let first = view_id(&editor);
    let opened = call(&world, &mut editor, "window_open_file", json!(["/p/a.txt"])).unwrap();
    assert_eq!(call(&world, &mut editor, "window_active_view", json!([])), Ok(opened.clone()));
    assert_eq!(call(&world, &mut editor, "window_find_open_file", json!(["/p/a.txt"])), Ok(opened.clone()));
    assert_eq!(call(&world, &mut editor, "window_find_open_file", json!(["/p/b.txt"])), Ok(Value::Null));
    assert_eq!(call(&world, &mut editor, "window_views", json!([])).unwrap().as_array().unwrap().len(), 1, "a.txt replaced the empty tab");
    let new = call(&world, &mut editor, "window_new_file", json!([])).unwrap();
    assert_ne!(new, opened);
    call(&world, &mut editor, "window_focus_view", json!([opened])).unwrap();
    assert_eq!(json!(view_id(&editor)), opened);
    assert_eq!(call(&world, &mut editor, "window_folders", json!([])), Ok(json!(["/p"])));
    assert_ne!(json!(first), new);
}

#[test]
fn messages_and_the_clipboard() {
    let world = plugin_world();
    let mut editor = world.editor();
    call(&world, &mut editor, "status_message", json!(["hi"])).unwrap();
    assert_eq!(status(&editor), "hi");
    call(&world, &mut editor, "error_message", json!(["bad"])).unwrap();
    assert!(editor.state().console.ends_with("bad\n"));
    assert_eq!(
        world.dialogs.messages.borrow().last(),
        Some(&MessageDialog {
            level: MessageLevel::Error,
            title: "CodeLibre Text".into(),
            text: "bad".into(),
            buttons: MessageButtons::Ok,
        })
    );
    call(&world, &mut editor, "set_clipboard", json!(["copied"])).unwrap();
    assert_eq!(call(&world, &mut editor, "get_clipboard", json!([])), Ok(json!("copied")));
}

#[test]
fn questions_answer_with_the_button_pressed() {
    let world = plugin_world();
    let mut editor = world.editor();
    world.dialogs.message_answers.borrow_mut().extend([MessageAnswer::Ok, MessageAnswer::Cancel, MessageAnswer::No]);
    assert_eq!(call(&world, &mut editor, "ok_cancel_dialog", json!(["Install?", "Install", ""])), Ok(json!(true)));
    assert_eq!(
        world.dialogs.messages.borrow().last().unwrap().buttons,
        MessageButtons::OkCancel { ok: "Install".into(), cancel: "Cancel".into() }
    );
    assert_eq!(call(&world, &mut editor, "ok_cancel_dialog", json!(["Install?", "", ""])), Ok(json!(false)));
    assert_eq!(
        world.dialogs.messages.borrow().last().unwrap().buttons,
        MessageButtons::OkCancel { ok: "OK".into(), cancel: "Cancel".into() },
        "default labels"
    );
    assert_eq!(call(&world, &mut editor, "yes_no_cancel_dialog", json!(["Keep?", "", "Drop", "Tool"])), Ok(json!(2)));
    let last = world.dialogs.messages.borrow().last().unwrap().clone();
    assert_eq!(last.title, "Tool");
    assert_eq!(last.buttons, MessageButtons::YesNoCancel { yes: "Yes".into(), no: "Drop".into(), cancel: "Cancel".into() });
    assert_eq!(call(&world, &mut editor, "yes_no_cancel_dialog", json!(["Keep?", "", "", ""])), Ok(json!(0)), "closed");
}

#[test]
fn file_dialogs_answer_with_paths() {
    let world = plugin_world();
    let mut editor = world.editor();
    assert_eq!(call(&world, &mut editor, "open_dialog", json!([null])), Ok(Value::Null), "cancelled");
    *world.dialogs.files_to_open.borrow_mut() = vec!["/p/a.txt".into(), "/p/b.txt".into()];
    assert_eq!(call(&world, &mut editor, "open_dialog", json!(["/p"])), Ok(json!(["/p/a.txt", "/p/b.txt"])));
    assert_eq!(world.dialogs.last_open_dir.borrow().as_deref(), Some(std::path::Path::new("/p")));

    *world.dialogs.save_path.borrow_mut() = Some("/p/out.json".into());
    assert_eq!(call(&world, &mut editor, "save_dialog", json!([null, "out", "json"])), Ok(json!("/p/out.json")));
    assert_eq!(world.dialogs.save_requests.borrow().last().map(String::as_str), Some("out.json"));

    *world.dialogs.folder_to_open.borrow_mut() = Some("/p".into());
    assert_eq!(call(&world, &mut editor, "select_folder_dialog", json!([null])), Ok(json!(["/p"])));
}

#[test]
fn resources_are_found_and_loaded() {
    let world = plugin_world()
        .with_resource("Packages/Tool/Tool.sublime-settings", "{\r\n}")
        .with_resource("Packages/Tool/icon.png", vec![0x89, 0x50, 0x00, 0xff]);
    let mut editor = world.editor();
    assert_eq!(
        call(&world, &mut editor, "find_resources", json!(["*.sublime-settings"])),
        Ok(json!(["Packages/Tool/Tool.sublime-settings"]))
    );
    assert_eq!(call(&world, &mut editor, "load_resource", json!(["Packages/Tool/Tool.sublime-settings"])), Ok(json!("{\n}")));
    assert_eq!(call(&world, &mut editor, "load_binary_resource", json!(["Packages/Tool/icon.png"])), Ok(json!("895000ff")));
    assert!(call(&world, &mut editor, "load_resource", json!(["Packages/Tool/icon.png"])).is_err(), "not text");
    assert_eq!(
        call(&world, &mut editor, "load_resource", json!(["Packages/Tool/nope"])),
        Err("resource not found: Packages/Tool/nope".into())
    );
}

#[test]
fn statuses_by_key_until_erased_or_closed() {
    let world = plugin_world();
    let mut editor = world.editor();
    let v = view_id(&editor);
    call(&world, &mut editor, "view_set_status", json!([v, "z_git", "main"])).unwrap();
    call(&world, &mut editor, "view_set_status", json!([v, "a_lint", "2 errors"])).unwrap();
    assert_eq!(editor.plugin_statuses(v), ["2 errors", "main"], "sorted by key");
    assert_eq!(call(&world, &mut editor, "view_get_status", json!([v, "z_git"])), Ok(json!("main")));
    call(&world, &mut editor, "view_erase_status", json!([v, "a_lint"])).unwrap();
    assert_eq!(editor.plugin_statuses(v), ["main"]);
    assert_eq!(call(&world, &mut editor, "view_get_status", json!([v, "a_lint"])), Ok(json!("")));
    assert!(call(&world, &mut editor, "view_set_status", json!([999, "k", "v"])).is_err());

    editor.close_active_document();
    assert!(editor.plugin_statuses(v).is_empty(), "forgotten with the view");
}

#[test]
fn regions_are_kept_by_the_editor_and_move_with_the_text() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "let x = 1;");
    let v = view_id(&editor);
    call(&world, &mut editor, "view_add_regions", json!([v, "lint", [[4, 5]], "region.redish", "dot", 2048])).unwrap();
    let set = &editor.active_document().unwrap().region_sets()[0];
    assert_eq!((set.scope.as_str(), set.icon.as_str(), set.flags), ("region.redish", "dot", 2048));
    call(&world, &mut editor, "view_insert", json!([v, 0, "  "])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_get_regions", json!([v, "lint"])), Ok(json!([[6, 7]])));
    call(&world, &mut editor, "view_erase_regions", json!([v, "lint"])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_get_regions", json!([v, "lint"])), Ok(json!([])));
}

#[test]
fn show_asks_the_view_to_scroll() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "a\nb\nc");
    let v = view_id(&editor);
    call(&world, &mut editor, "view_show", json!([v, 4, true])).unwrap();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.show_request(), Some(entities::document::ShowRequest { point: 4, center: true }));
}

#[test]
fn scratch_views_close_without_asking() {
    let world = plugin_world();
    let mut editor = editor_with(&world, "unsaved");
    let v = view_id(&editor);
    assert_eq!(call(&world, &mut editor, "view_is_scratch", json!([v])), Ok(json!(false)));
    assert_eq!(call(&world, &mut editor, "view_close", json!([v])), Ok(json!(false)), "dirty: the user is asked");
    assert!(editor.state().pending_close.is_some());
    editor.resolve_pending_close(use_cases::editor::CloseChoice::Cancel);

    call(&world, &mut editor, "view_set_scratch", json!([v, true])).unwrap();
    assert_eq!(call(&world, &mut editor, "view_is_dirty", json!([v])), Ok(json!(false)));
    assert_eq!(call(&world, &mut editor, "view_close", json!([v])), Ok(json!(true)));
    assert_eq!(call(&world, &mut editor, "view_is_valid", json!([v])), Ok(json!(false)));
}

#[test]
fn view_settings_layer_on_top_of_the_syntax_settings() {
    let world = plugin_world();
    let mut editor = world.editor();
    let target = json!(["view", view_id(&editor)]);
    assert_eq!(call(&world, &mut editor, "settings_get", json!([target, "tab_size"])), Ok(json!([true, 4])));
    assert_eq!(call(&world, &mut editor, "settings_get", json!([target, "nope"])), Ok(json!([false, null])));
    call(&world, &mut editor, "settings_set", json!([target, "tab_size", 2])).unwrap();
    assert_eq!(editor.view_settings().usize("tab_size", 0), 2, "built-in features see it");
    assert_eq!(editor.global_settings().usize("tab_size", 0), 4, "only this view");
    call(&world, &mut editor, "settings_erase", json!([target, "tab_size"])).unwrap();
    assert_eq!(editor.view_settings().usize("tab_size", 0), 4);
    let all = call(&world, &mut editor, "settings_all", json!([target])).unwrap();
    assert_eq!(all["auto_indent"], json!(true));
}

#[test]
fn plugin_settings_files_are_loaded_changed_and_saved_to_user() {
    let world = plugin_world();
    world
        .resources
        .settings_files
        .borrow_mut()
        .insert("Tool.sublime-settings".into(), json!({"a": 1, "b": 2}).as_object().unwrap().clone());
    world
        .resources
        .user_settings_files
        .borrow_mut()
        .insert("Tool.sublime-settings".into(), json!({"b": 3, "keep": true}).as_object().unwrap().clone());
    let mut editor = world.editor();
    let target = json!(["file", "Tool.sublime-settings"]);
    assert_eq!(call(&world, &mut editor, "settings_get", json!([target, "b"])), Ok(json!([true, 3])), "User wins");
    call(&world, &mut editor, "settings_set", json!([target, "a", 10])).unwrap();
    call(&world, &mut editor, "settings_erase", json!([target, "b"])).unwrap();
    assert_eq!(call(&world, &mut editor, "settings_get", json!([target, "a"])), Ok(json!([true, 10])));
    call(&world, &mut editor, "save_settings", json!(["Tool.sublime-settings"])).unwrap();
    assert_eq!(
        Value::Object(world.resources.user_settings_files.borrow()["Tool.sublime-settings"].clone()),
        json!({"a": 10, "keep": true})
    );
}

#[test]
fn preferences_set_by_plugins_apply_now_and_persist_on_save() {
    let world = plugin_world();
    let mut editor = world.editor();
    let target = json!(["file", "Preferences.sublime-settings"]);
    call(&world, &mut editor, "settings_set", json!([target, "font_size", 14])).unwrap();
    assert_eq!(editor.global_settings().usize("font_size", 0), 14);
    assert!(world.resources.user_settings.borrow().get("font_size").is_none(), "not saved yet");
    call(&world, &mut editor, "save_settings", json!(["Preferences.sublime-settings"])).unwrap();
    assert_eq!(world.resources.user_settings.borrow()["font_size"], json!(14));
}

#[test]
fn window_settings_live_in_memory() {
    let world = plugin_world();
    let mut editor = world.editor();
    let target = json!(["window", 1]);
    call(&world, &mut editor, "settings_set", json!([target, "x", [1, 2]])).unwrap();
    assert_eq!(call(&world, &mut editor, "settings_get", json!([target, "x"])), Ok(json!([true, [1, 2]])));
    assert!(call(&world, &mut editor, "settings_get", json!([["planet", 3], "x"])).is_err());
}
