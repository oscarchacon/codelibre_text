//! Tests for the editing use cases (`use_cases::editing`).

use entities::selection::Selection;
use entities::text_commands::{Case, LineOrder};
use serde_json::json;
use test_support::{active_text, open, status, World};
use use_cases::editing::TextCommand;
use use_cases::Editor;

/// An editor showing `text` with the caret at `caret`.
fn editor_with(world: &World, text: &str, caret: usize) -> Editor {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, Selection::caret(caret));
    editor
}

#[test]
fn insert_types_at_the_caret() {
    let world = World::new();
    let mut editor = editor_with(&world, "ac", 1);
    editor.run_text_command(TextCommand::Insert("b".into()));
    assert_eq!(active_text(&editor), "abc");
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(2));
}

#[test]
fn every_text_command_reaches_its_entity_function() {
    let cases: Vec<(&str, usize, TextCommand, &str)> = vec![
        ("a\nb", 0, TextCommand::DuplicateLine, "a\na\nb"),
        ("a\nb", 2, TextCommand::SwapLine { up: true }, "b\na"),
        ("a\nb", 0, TextCommand::SwapLine { up: false }, "b\na"),
        ("a\nb", 0, TextCommand::DeleteLine, "b"),
        ("x", 0, TextCommand::AddLine { before: true }, "\nx"),
        ("x", 0, TextCommand::AddLine { before: false }, "x\n"),
        ("abc", 1, TextCommand::DeleteToLineEdge { to_end: true }, "a"),
        ("abc", 2, TextCommand::DeleteToLineEdge { to_end: false }, "c"),
        ("a", 0, TextCommand::Indent, "\ta"),
        ("\ta", 0, TextCommand::Unindent, "a"),
        ("a", 1, TextCommand::InsertTab, "a\t"),
        ("{}", 1, TextCommand::NewlineAutoIndent, "{\n\t\n}"),
        ("a\nb", 0, TextCommand::JoinLines, "a b"),
        ("abc", 1, TextCommand::ChangeCase(Case::Upper), "ABC"),
        ("ab", 1, TextCommand::Transpose, "ba"),
        ("\tx", 0, TextCommand::ConvertIndentation { to_spaces: true }, "    x"),
        ("a  ", 0, TextCommand::TrimTrailingWhitespace { only_modified: false, trim_on_caret: true }, "a"),
        ("b\na", 0, TextCommand::ReorderLines(LineOrder::Sort { case_sensitive: true }), "a\nb"),
        ("x", 0, TextCommand::ToggleComment { block: false }, "x"),
    ];
    for (text, caret, command, expected) in cases {
        let world = World::new();
        let mut editor = editor_with(&world, text, caret);
        editor.run_text_command(command);
        assert_eq!(active_text(&editor), expected, "starting from {text:?}");
    }
}

#[test]
fn selection_commands_only_move_the_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nfoo bar", 4);
    editor.run_text_command(TextCommand::SelectWord);
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(2, 5));
    editor.run_text_command(TextCommand::ExpandSelectionToLine);
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(2, 9));
    assert_eq!(active_text(&editor), "a\nfoo bar");
}

#[test]
fn toggle_comment_uses_the_document_syntax() {
    let world = World::new().with_file("/p/a.py", "x = 1");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    editor.run_text_command(TextCommand::ToggleComment { block: false });
    assert_eq!(active_text(&editor), "# x = 1");
}

#[test]
fn indentation_follows_the_syntax_specific_settings() {
    let world = World::new()
        .with_file("/p/a.py", "x")
        .with_syntax_settings("Python", json!({ "tab_size": 2, "translate_tabs_to_spaces": true }));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    editor.run_text_command(TextCommand::Indent);
    assert_eq!(active_text(&editor), "  x");
}

#[test]
fn read_only_documents_ignore_text_commands() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_scratch("Defaults", "{}", "JSON", true);
    editor.run_text_command(TextCommand::Insert("x".into()));
    assert_eq!(active_text(&editor), "{}");
}

#[test]
fn text_commands_without_documents_do_nothing() {
    let world = World::new();
    let mut editor = world.editor();
    editor.close_all_documents();
    editor.run_text_command(TextCommand::DuplicateLine);
    assert!(editor.state().documents.is_empty());
}

#[test]
fn go_to_line_moves_the_caret_to_the_line_start() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nbb\nccc", 0);
    let revision = editor.active_document().unwrap().selection_revision();
    editor.go_to_line(2);
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::caret(5));
    assert!(doc.selection_revision() > revision, "the view must scroll there");
}

#[test]
fn go_to_line_past_the_end_goes_to_the_end() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nb", 0);
    editor.go_to_line(99);
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(3));
}

#[test]
fn clipboard_text_comes_from_the_clipboard_gateway() {
    let world = World::new();
    *world.clipboard.borrow_mut() = Ok("copied".into());
    let mut editor = world.editor();
    assert_eq!(editor.clipboard_text(), Some("copied".into()));
}

#[test]
fn clipboard_errors_are_reported() {
    let world = World::new();
    *world.clipboard.borrow_mut() = Err("locked".into());
    let mut editor = world.editor();
    assert_eq!(editor.clipboard_text(), None);
    assert_eq!(status(&editor), "Clipboard unavailable: locked");
}

#[test]
fn select_lines_adds_a_caret_below_and_makes_it_the_primary_one() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc\nabc\nabc", 1);
    editor.add_carets_on_next_line(true);
    editor.add_carets_on_next_line(true);
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::caret(9));
    assert_eq!(doc.selections(), vec![Selection::caret(1), Selection::caret(5), Selection::caret(9)]);
    editor.insert_at_selections("x");
    assert_eq!(active_text(&editor), "axbc\naxbc\naxbc");
}

#[test]
fn select_lines_upwards_on_the_first_line_does_nothing() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc\nabc", 1);
    editor.add_carets_on_next_line(false);
    assert!(editor.active_document().unwrap().extra_selections().is_empty());
    assert_eq!(editor.active_document().unwrap().selection(), Selection::caret(1));
}

#[test]
fn split_selection_into_lines_keeps_the_caret_line_as_primary() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab\ncd\nef", 0);
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, Selection::new(0, 8));
    editor.split_selection_into_lines();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selection(), Selection::new(6, 8));
    assert_eq!(doc.selections(), vec![Selection::new(0, 2), Selection::new(3, 5), Selection::new(6, 8)]);

    editor.sync_selection_from_view(id, Selection::new(8, 0));
    editor.split_selection_into_lines();
    assert_eq!(editor.active_document().unwrap().selection(), Selection::new(0, 2), "a backwards selection keeps the first line");
}
