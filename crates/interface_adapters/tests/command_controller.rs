//! Tests for `controllers::command_controller` (Sublime command names → use cases).

use std::path::PathBuf;

use entities::command::Command;
use entities::document::LineEnding;
use entities::selection::Selection;
use interface_adapters::controllers::command_controller::{close_quick_panel, run};
use interface_adapters::gateways::{builtin_resources, sublime_json};
use interface_adapters::view_state::{ViewRequest, ViewState, WidgetEdit};
use serde_json::{json, Value};
use test_support::{active_text, open, status, World};
use use_cases::editing::TextCommand;
use use_cases::search::SearchKind;
use use_cases::Editor;

fn cmd(name: &str) -> Command {
    Command::new(name)
}

fn cmd_with(name: &str, args: Value) -> Command {
    Command::with_args(name, args)
}

/// An editor showing `text` with `selection`, plus a fresh view state.
fn setup(world: &World, text: &str, selection: Selection) -> (Editor, ViewState) {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, selection);
    (editor, ViewState::default())
}

fn text_after(command: Command, text: &str, selection: Selection) -> String {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, text, selection);
    run(&mut editor, &mut view, &command);
    active_text(&editor)
}

// ----- files ---------------------------------------------------------------------------------------

#[test]
fn new_file_and_close() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "", Selection::caret(0));
    run(&mut editor, &mut view, &cmd("new_file"));
    assert_eq!(editor.state().documents.len(), 2);
    run(&mut editor, &mut view, &cmd("close"));
    run(&mut editor, &mut view, &cmd("close_file"));
    assert!(editor.state().documents.is_empty());
}

#[test]
fn save_and_save_as() {
    let world = World::new().with_file("/p/a.txt", "a");
    *world.dialogs.save_path.borrow_mut() = Some(PathBuf::from("/p/b.txt"));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("1".into()));
    run(&mut editor, &mut view, &cmd("save"));
    assert_eq!(world.file_text("/p/a.txt"), "1a");
    run(&mut editor, &mut view, &cmd("prompt_save_as"));
    assert_eq!(world.file_text("/p/b.txt"), "1a");
}

#[test]
fn open_folder_expands_it_and_shows_the_side_bar() {
    let world = World::new().with_dir("/p");
    *world.dialogs.folder_to_open.borrow_mut() = Some(PathBuf::from("/p"));
    let mut editor = world.editor();
    let mut view = ViewState::default();
    view.show_side_bar = false;
    run(&mut editor, &mut view, &cmd("prompt_open_folder"));
    assert!(view.show_side_bar);
    assert!(view.expanded_folders.contains(&PathBuf::from("/p")));
    assert_eq!(editor.state().folders, vec![PathBuf::from("/p")]);
}

#[test]
fn open_file_and_open_dir_expand_package_paths() {
    let world = World::new().with_file("/packages/User/a.txt", "hi");
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("open_file", json!({ "file": "${packages}/User/a.txt" })));
    assert_eq!(active_text(&editor), "hi");
    run(&mut editor, &mut view, &cmd_with("open_dir", json!({ "dir": "${packages}" })));
    assert_eq!(*world.opened_folders.borrow(), vec![PathBuf::from("/packages")]);
}

#[test]
fn close_window_and_exit_ask_the_window_to_close() {
    for name in ["close_window", "exit"] {
        let world = World::new();
        let mut editor = world.editor();
        let mut view = ViewState::default();
        run(&mut editor, &mut view, &cmd(name));
        assert_eq!(view.take_requests(), vec![ViewRequest::CloseWindow]);
    }
}

#[test]
fn refresh_folder_list_reindexes() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    let before = world.file_index.rebuilds.borrow().len();
    run(&mut editor, &mut view, &cmd("refresh_folder_list"));
    assert_eq!(world.file_index.rebuilds.borrow().len(), before + 1);
}

#[test]
fn reopen_revert_and_close_all() {
    let world = World::new().with_file("/p/a.txt", "a");
    let mut editor = world.editor();
    let mut view = ViewState::default();
    open(&mut editor, "/p/a.txt");
    editor.run_text_command(TextCommand::Insert("x".into()));
    run(&mut editor, &mut view, &cmd("revert"));
    assert_eq!(active_text(&editor), "a");
    run(&mut editor, &mut view, &cmd("close_all"));
    assert!(editor.state().documents.is_empty());
    run(&mut editor, &mut view, &cmd("reopen_last_file"));
    assert_eq!(active_text(&editor), "a");
}

// ----- quick panel ---------------------------------------------------------------------------------

#[test]
fn show_overlay_opens_the_palette_or_goto_with_preset_text() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    view.focus_editor = false;
    run(&mut editor, &mut view, &cmd_with("show_overlay", json!({ "overlay": "command_palette" })));
    assert_eq!(view.quick_panel.as_ref().unwrap().kind, SearchKind::Commands);

    run(&mut editor, &mut view, &cmd_with("show_overlay", json!({ "overlay": "goto", "text": ":" })));
    let panel = view.quick_panel.as_ref().unwrap();
    assert_eq!(panel.kind, SearchKind::Goto);
    assert_eq!(panel.query, ":");
    assert!(panel.focus_requested);
    assert!(!view.focus_editor, "the quick panel keeps the focus");
}

#[test]
fn unknown_overlays_are_reported() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("show_overlay", json!({ "overlay": "minimap" })));
    assert!(view.quick_panel.is_none());
    assert_eq!(status(&editor), "Overlay not supported: minimap");
}

#[test]
fn select_color_scheme_starts_on_the_current_scheme_and_cancelling_restores_it() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("select_color_scheme"));
    let panel = view.quick_panel.as_ref().unwrap();
    assert_eq!(panel.kind, SearchKind::ColorSchemes);
    assert_eq!(panel.results[panel.selected].caption, "Monokai");

    editor.preview_color_scheme("Daylight");
    run(&mut editor, &mut view, &cmd("hide_overlay"));
    assert!(view.quick_panel.is_none());
    assert_eq!(editor.color_scheme_name(), "Monokai");
    assert!(view.focus_editor);
}

#[test]
fn closing_a_normal_quick_panel_changes_no_setting() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("show_overlay", json!({ "overlay": "goto" })));
    close_quick_panel(&mut editor, &mut view);
    assert!(view.quick_panel.is_none());
    assert_eq!(editor.color_scheme_name(), "Monokai");
}

// ----- find panel ----------------------------------------------------------------------------------

#[test]
fn show_panel_find_and_replace() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "cat dog", Selection::new(0, 3));
    run(&mut editor, &mut view, &cmd_with("show_panel", json!({ "panel": "find" })));
    assert!(view.find_panel.visible && view.find_panel.focus_requested && !view.find_panel.replace_mode);
    assert_eq!(editor.state().find.query, "cat", "the selection becomes the query");

    run(&mut editor, &mut view, &cmd_with("show_panel", json!({ "panel": "replace" })));
    assert!(view.find_panel.replace_mode);

    run(&mut editor, &mut view, &cmd("hide_panel"));
    assert!(!view.find_panel.visible);
    assert!(view.focus_editor);
}

#[test]
fn output_panels_are_not_supported() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("show_panel", json!({ "panel": "output.exec" })));
    assert!(!view.find_panel.visible);
    assert_eq!(status(&editor), "Panel not supported: exec");
}

#[test]
fn find_next_without_a_query_opens_the_find_panel() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("find_next"));
    assert!(view.find_panel.visible && view.find_panel.focus_requested);
}

#[test]
fn find_under_searches_the_word_at_the_caret() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "foo bar foo", Selection::caret(1));
    run(&mut editor, &mut view, &cmd("find_under"));
    assert_eq!(editor.state().find.query, "foo");
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(8, 11));
    run(&mut editor, &mut view, &cmd("find_under_prev"));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(0, 3));
}

#[test]
fn slurp_find_string_and_find_under_expand() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "ab ab", Selection::new(0, 2));
    run(&mut editor, &mut view, &cmd("slurp_find_string"));
    assert_eq!(editor.state().find.query, "ab");
    run(&mut editor, &mut view, &cmd("find_under_expand"));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(3, 5));
}

#[test]
fn replace_commands() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "a a a", Selection::new(0, 1));
    editor.set_find_options(entities::find::FindOptions { query: "a".into(), ..Default::default() });
    editor.set_replacement("b".into());
    run(&mut editor, &mut view, &cmd("replace_next"));
    assert_eq!(active_text(&editor), "b a a");
    run(&mut editor, &mut view, &cmd("replace_all"));
    assert_eq!(active_text(&editor), "b b b");
}

// ----- view ----------------------------------------------------------------------------------------

#[test]
fn toggling_side_bar_status_bar_and_full_screen() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("toggle_side_bar"));
    run(&mut editor, &mut view, &cmd("toggle_status_bar"));
    run(&mut editor, &mut view, &cmd("toggle_full_screen"));
    assert!(!view.show_side_bar && !view.show_status_bar && view.fullscreen);
    assert_eq!(view.take_requests(), vec![ViewRequest::SetFullscreen(true)]);
    run(&mut editor, &mut view, &cmd("toggle_distraction_free"));
    assert_eq!(view.take_requests(), vec![ViewRequest::SetFullscreen(false)]);
}

#[test]
fn font_size_commands() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("increase_font_size"));
    run(&mut editor, &mut view, &cmd("increase_font_size"));
    run(&mut editor, &mut view, &cmd("decrease_font_size"));
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 12.0);
    run(&mut editor, &mut view, &cmd("reset_font_size"));
    assert_eq!(editor.global_settings().f32("font_size", 0.0), 11.0);
}

#[test]
fn toggle_and_set_setting() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("toggle_setting", json!({ "setting": "word_wrap" })));
    assert!(editor.view_settings().bool("word_wrap", false));
    run(&mut editor, &mut view, &cmd_with("set_setting", json!({ "setting": "tab_size", "value": 2 })));
    assert_eq!(editor.view_settings().usize("tab_size", 0), 2);
}

#[test]
fn switching_tabs() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    editor.new_file();
    editor.new_file();
    run(&mut editor, &mut view, &cmd_with("select_by_index", json!({ "index": 0 })));
    assert_eq!(editor.state().active, 0);
    run(&mut editor, &mut view, &cmd("next_view"));
    assert_eq!(editor.state().active, 1);
    run(&mut editor, &mut view, &cmd("prev_view"));
    assert_eq!(editor.state().active, 0);
    // Recently used, newest first: 0, 1, 2.
    run(&mut editor, &mut view, &cmd("next_view_in_stack"));
    assert_eq!(editor.state().active, 1, "back to the previous document");
    run(&mut editor, &mut view, &cmd("prev_view_in_stack"));
    assert_eq!(editor.state().active, 2, "to the least recently used document");
}

#[test]
fn set_file_type_and_line_endings() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("set_file_type", json!({ "syntax": "Packages/Rust/Rust.sublime-syntax" })));
    assert_eq!(editor.active_document().unwrap().syntax, "Rust");
    run(&mut editor, &mut view, &cmd_with("set_line_ending", json!({ "type": "windows" })));
    assert_eq!(editor.active_document().unwrap().line_ending, LineEnding::Windows);
    run(&mut editor, &mut view, &cmd_with("set_line_ending", json!({ "type": "unix" })));
    assert_eq!(editor.active_document().unwrap().line_ending, LineEnding::Unix);
}

// ----- text ----------------------------------------------------------------------------------------

#[test]
fn text_commands_by_sublime_name() {
    let caret = Selection::caret;
    let cases: Vec<(Command, &str, Selection, &str)> = vec![
        (cmd("duplicate_line"), "a", caret(0), "a\na"),
        (cmd("swap_line_up"), "a\nb", caret(2), "b\na"),
        (cmd("swap_line_down"), "a\nb", caret(0), "b\na"),
        (cmd("indent"), "a", caret(0), "\ta"),
        (cmd("unindent"), "\ta", caret(0), "a"),
        (cmd("join_lines"), "a\nb", caret(0), "a b"),
        (cmd("upper_case"), "ab", caret(0), "AB"),
        (cmd("lower_case"), "AB", caret(0), "ab"),
        (cmd("title_case"), "ab cd", Selection::new(0, 5), "Ab Cd"),
        (cmd("swap_case"), "aB", caret(0), "Ab"),
        (cmd("transpose"), "ab", caret(1), "ba"),
        (cmd("expand_tabs"), "\tx", caret(0), "    x"),
        (cmd("unexpand_tabs"), "    x", caret(0), "\tx"),
        (cmd("trim_trailing_white_space"), "a  ", caret(0), "a"),
        (cmd_with("sort_lines", json!({ "case_sensitive": false })), "b\nA", caret(0), "A\nb"),
        (cmd_with("permute_lines", json!({ "operation": "reverse" })), "1\n2", caret(0), "2\n1"),
        (cmd_with("permute_lines", json!({ "operation": "unique" })), "1\n1", caret(0), "1"),
        (cmd_with("insert", json!({ "characters": "!" })), "a", caret(1), "a!"),
        (cmd_with("insert_snippet", json!({ "contents": "fn ${1:name}() {$0}" })), "", caret(0), "fn name() {}"),
        (cmd_with("toggle_comment", json!({ "block": true })), "x", Selection::new(0, 1), "x"),
    ];
    for (command, text, selection, expected) in cases {
        let name = command.name.clone();
        assert_eq!(text_after(command, text, selection), expected, "{name} on {text:?}");
    }
}

#[test]
fn expand_selection_to_line_and_word() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "ab cd\nx", Selection::caret(1));
    run(&mut editor, &mut view, &cmd_with("expand_selection", json!({ "to": "word" })));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(0, 2));
    run(&mut editor, &mut view, &cmd_with("expand_selection", json!({ "to": "line" })));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(0, 6));
    run(&mut editor, &mut view, &cmd_with("expand_selection", json!({ "to": "brackets" })));
    assert_eq!(status(&editor), "expand_selection to brackets is not supported");
}

#[test]
fn unsupported_permutations_are_reported() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "b\na", Selection::caret(0));
    run(&mut editor, &mut view, &cmd_with("permute_lines", json!({ "operation": "shuffle" })));
    assert_eq!(active_text(&editor), "b\na");
    assert!(status(&editor).contains("only reverse and unique"));
}

#[test]
fn sublime_line_macros() {
    let macro_cmd = |name: &str| cmd_with("run_macro_file", json!({ "file": format!("res://Packages/Default/{name}.sublime-macro") }));
    let caret = Selection::caret;
    assert_eq!(text_after(macro_cmd("Delete Line"), "a\nb", caret(0)), "b");
    assert_eq!(text_after(macro_cmd("Add Line"), "a", caret(0)), "a\n");
    assert_eq!(text_after(macro_cmd("Add Line Before"), "a", caret(0)), "\na");
    assert_eq!(text_after(macro_cmd("Add Line in Braces"), "{}", caret(1)), "{\n\t\n}");
    assert_eq!(text_after(macro_cmd("Delete to Hard EOL"), "abc", caret(1)), "a");
    assert_eq!(text_after(macro_cmd("Delete to Hard BOL"), "abc", caret(2)), "c");
}

#[test]
fn unknown_macros_are_reported() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "a", Selection::caret(0));
    run(&mut editor, &mut view, &cmd_with("run_macro_file", json!({ "file": "res://Packages/X/Dance.sublime-macro" })));
    assert_eq!(status(&editor), "Macro not supported: res://Packages/X/Dance.sublime-macro");
}

#[test]
fn goto_line_is_one_based() {
    let world = World::new();
    let (mut editor, mut view) = setup(&world, "a\nb\nc", Selection::caret(0));
    run(&mut editor, &mut view, &cmd_with("goto_line", json!({ "line": 3 })));
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(4));
}

// ----- widget edits --------------------------------------------------------------------------------

#[test]
fn undo_redo_and_clipboard_commands_are_left_to_the_text_widget() {
    let world = World::new();
    *world.clipboard.borrow_mut() = Ok("clip".into());
    let mut editor = world.editor();
    let mut view = ViewState::default();
    for name in ["undo", "soft_undo", "redo", "redo_or_repeat", "soft_redo", "select_all", "copy", "cut", "paste", "paste_and_indent"] {
        run(&mut editor, &mut view, &cmd(name));
    }
    let edits: Vec<WidgetEdit> = view
        .take_requests()
        .into_iter()
        .map(|r| match r {
            ViewRequest::WidgetEdit(edit) => edit,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    use WidgetEdit::*;
    assert_eq!(
        edits,
        vec![Undo, Undo, Redo, Redo, Redo, SelectAll, Copy, Cut, Paste("clip".into()), Paste("clip".into())]
    );
    assert!(view.focus_editor);
}

#[test]
fn paste_without_clipboard_access_does_nothing() {
    let world = World::new();
    *world.clipboard.borrow_mut() = Err("busy".into());
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("paste"));
    assert!(view.take_requests().is_empty());
}

// ----- preferences and help ------------------------------------------------------------------------

#[test]
fn edit_settings_reload_packages_and_help_windows() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd_with("edit_settings", json!({ "base_file": "${packages}/Default/Preferences.sublime-settings", "default": "{}" })));
    assert!(world.file_exists("/packages/User/Preferences.sublime-settings"));
    run(&mut editor, &mut view, &cmd("reload_packages"));
    assert!(status(&editor).starts_with("Packages reloaded"));
    run(&mut editor, &mut view, &cmd("show_load_errors"));
    run(&mut editor, &mut view, &cmd("show_about_window"));
    assert!(view.show_load_errors && view.show_about);
}

#[test]
fn unknown_commands_are_reported() {
    let world = World::new();
    let mut editor = world.editor();
    let mut view = ViewState::default();
    run(&mut editor, &mut view, &cmd("git_blame"));
    assert_eq!(status(&editor), "Command not available: git_blame (Python plugin commands are not supported yet)");
}

#[test]
fn every_command_in_the_builtin_resources_is_implemented() {
    let mut commands = Vec::new();
    for source in [builtin_resources::KEYMAP, builtin_resources::MAIN_MENU, builtin_resources::PALETTE_COMMANDS] {
        collect_commands(&sublime_json::parse(source).unwrap(), &mut commands);
    }
    for command in commands {
        let world = World::new();
        let mut editor = world.editor();
        let mut view = ViewState::default();
        run(&mut editor, &mut view, &command);
        assert!(!status(&editor).starts_with("Command not available"), "{} is not implemented", command.name);
    }
}

fn collect_commands(value: &Value, out: &mut Vec<Command>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(name)) = map.get("command") {
                out.push(Command::with_args(name, map.get("args").cloned().unwrap_or(Value::Null)));
            }
            map.values().for_each(|v| collect_commands(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_commands(v, out)),
        _ => {}
    }
}
