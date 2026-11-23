//! Tests for the editing use cases (`use_cases::editing`).

use entities::selection::Selection;
use entities::text_commands::{Case, LineOrder};
use serde_json::json;
use test_support::{active_text, open, status, World};
use use_cases::editing::{DragBy, DragCombine, DragPoint, DragSelect, TextCommand};
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
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(2)
    );
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
        (
            "abc",
            1,
            TextCommand::DeleteToLineEdge { to_end: true },
            "a",
        ),
        (
            "abc",
            2,
            TextCommand::DeleteToLineEdge { to_end: false },
            "c",
        ),
        ("a", 0, TextCommand::Indent, "\ta"),
        ("\ta", 0, TextCommand::Unindent, "a"),
        ("a", 1, TextCommand::InsertTab, "a\t"),
        ("{}", 1, TextCommand::NewlineAutoIndent, "{\n\t\n}"),
        ("a\nb", 0, TextCommand::JoinLines, "a b"),
        ("abc", 1, TextCommand::ChangeCase(Case::Upper), "ABC"),
        ("ab", 1, TextCommand::Transpose, "ba"),
        (
            "\tx",
            0,
            TextCommand::ConvertIndentation { to_spaces: true },
            "    x",
        ),
        (
            "a  ",
            0,
            TextCommand::TrimTrailingWhitespace {
                only_modified: false,
                trim_on_caret: true,
            },
            "a",
        ),
        (
            "b\na",
            0,
            TextCommand::ReorderLines(LineOrder::Sort {
                case_sensitive: true,
            }),
            "a\nb",
        ),
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
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(2, 5)
    );
    editor.run_text_command(TextCommand::ExpandSelectionToLine);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(2, 9)
    );
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
    let world = World::new().with_file("/p/a.py", "x").with_syntax_settings(
        "Python",
        json!({ "tab_size": 2, "translate_tabs_to_spaces": true }),
    );
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
    assert!(
        doc.selection_revision() > revision,
        "the view must scroll there"
    );
}

#[test]
fn go_to_line_past_the_end_goes_to_the_end() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nb", 0);
    editor.go_to_line(99);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(3)
    );
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
    assert_eq!(
        doc.selections(),
        vec![
            Selection::caret(1),
            Selection::caret(5),
            Selection::caret(9)
        ]
    );
    editor.insert_at_selections("x");
    assert_eq!(active_text(&editor), "axbc\naxbc\naxbc");
}

#[test]
fn select_lines_upwards_on_the_first_line_does_nothing() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc\nabc", 1);
    editor.add_carets_on_next_line(false);
    assert!(editor
        .active_document()
        .unwrap()
        .extra_selections()
        .is_empty());
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(1)
    );
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
    assert_eq!(
        doc.selections(),
        vec![
            Selection::new(0, 2),
            Selection::new(3, 5),
            Selection::new(6, 8)
        ]
    );

    editor.sync_selection_from_view(id, Selection::new(8, 0));
    editor.split_selection_into_lines();
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(0, 2),
        "a backwards selection keeps the first line"
    );
}

#[test]
fn insert_command_types_at_every_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc\nabc", 1);
    editor.add_carets_on_next_line(true);
    editor.run_text_command(TextCommand::Insert("x".into()));
    assert_eq!(active_text(&editor), "axbc\naxbc");
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(7)
    );
}

#[test]
fn delete_word_removes_to_the_word_boundary_at_every_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "one two  three", 7);
    editor.run_view_command_on_active(&entities::command::Command::new("delete_word"));
    assert_eq!(active_text(&editor), "one   three");
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(4)
    );

    let mut editor = editor_with(&world, "one two three", 4);
    let forward = entities::command::Command::with_args("delete_word", json!({ "forward": true }));
    editor.run_view_command_on_active(&forward);
    assert_eq!(active_text(&editor), "one three");

    // A selection goes whole, spaces and all.
    let mut editor = editor_with(&world, "one two three", 4);
    editor.sync_selection_from_view(editor.active_document().unwrap().id, Selection::new(4, 8));
    editor.run_view_command_on_active(&entities::command::Command::new("delete_word"));
    assert_eq!(active_text(&editor), "one three");
}

fn drag(anchor: usize, head: usize, by: DragBy, combine: DragCombine, pressed: bool) -> DragSelect {
    DragSelect {
        anchor: DragPoint::at(anchor),
        head: DragPoint::at(head),
        by,
        combine,
        pressed,
    }
}

fn all_selections(editor: &Editor) -> Vec<(usize, usize)> {
    let doc = editor.active_document().unwrap();
    doc.selections()
        .iter()
        .map(|s| (s.anchor, s.head))
        .collect()
}

#[test]
fn an_additive_click_adds_a_caret_and_makes_it_the_primary_one() {
    let world = World::new();
    let mut editor = editor_with(&world, "one two three", 1);
    let id = editor.active_document().unwrap().id;
    editor.drag_select(id, drag(5, 5, DragBy::Characters, DragCombine::Add, true));
    editor.end_drag_select();
    assert_eq!(all_selections(&editor), vec![(1, 1), (5, 5)]);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(5)
    );
}

#[test]
fn an_additive_drag_replaces_what_it_selected_so_far_on_every_move() {
    let world = World::new();
    let mut editor = editor_with(&world, "one two three", 0);
    let id = editor.active_document().unwrap().id;
    editor.drag_select(id, drag(4, 4, DragBy::Characters, DragCombine::Add, true));
    editor.drag_select(id, drag(4, 13, DragBy::Characters, DragCombine::Add, false));
    editor.drag_select(id, drag(4, 7, DragBy::Characters, DragCombine::Add, false));
    assert_eq!(all_selections(&editor), vec![(0, 0), (4, 7)]);
}

#[test]
fn an_additive_double_click_adds_the_word() {
    let world = World::new();
    let mut editor = editor_with(&world, "one two three", 0);
    let id = editor.active_document().unwrap().id;
    editor.drag_select(id, drag(9, 9, DragBy::Words, DragCombine::Add, true));
    assert_eq!(all_selections(&editor), vec![(0, 0), (8, 13)]);
}

#[test]
fn a_subtractive_click_takes_a_caret_away_but_never_the_last_one() {
    let world = World::new();
    let mut editor = editor_with(&world, "one two three", 1);
    let id = editor.active_document().unwrap().id;
    editor.drag_select(id, drag(5, 5, DragBy::Characters, DragCombine::Add, true));
    editor.end_drag_select();
    editor.drag_select(
        id,
        drag(5, 5, DragBy::Characters, DragCombine::Subtract, true),
    );
    editor.end_drag_select();
    assert_eq!(all_selections(&editor), vec![(1, 1)]);
    editor.drag_select(
        id,
        drag(1, 1, DragBy::Characters, DragCombine::Subtract, true),
    );
    assert_eq!(all_selections(&editor), vec![(1, 1)]);
}

#[test]
fn a_column_drag_selects_the_same_columns_on_every_line() {
    let world = World::new();
    let mut editor = editor_with(&world, "abcd\nab\nabcd", 0);
    let id = editor.active_document().unwrap().id;
    let column = DragSelect {
        anchor: DragPoint::at(1),
        head: DragPoint::at(11),
        by: DragBy::Columns,
        combine: DragCombine::Replace,
        pressed: true,
    };
    editor.drag_select(id, column);
    assert_eq!(all_selections(&editor), vec![(1, 3), (6, 7), (9, 11)]);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::new(9, 11)
    );
    // Typing goes to every line.
    editor.insert_at_selections("x");
    assert_eq!(active_text(&editor), "axd\nax\naxd");
}

#[test]
fn a_column_drag_past_the_ends_of_the_lines_puts_carets_at_their_ends() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab\nabc", 0);
    let id = editor.active_document().unwrap().id;
    let column = DragSelect {
        anchor: DragPoint {
            point: 2,
            past_end: 4,
        },
        head: DragPoint {
            point: 6,
            past_end: 3,
        },
        by: DragBy::Columns,
        combine: DragCombine::Replace,
        pressed: true,
    };
    editor.drag_select(id, column);
    assert_eq!(all_selections(&editor), vec![(2, 2), (6, 6)]);
}

fn move_lines(forward: bool, extend: bool) -> entities::command::Command {
    entities::command::Command::with_args(
        "move",
        json!({ "by": "lines", "forward": forward, "extend": extend }),
    )
}

/// An editor showing `text` with carets at `carets` (the last one primary).
fn editor_with_carets(world: &World, text: &str, carets: &[usize]) -> Editor {
    let mut editor = editor_with(world, text, carets[0]);
    let id = editor.active_document().unwrap().id;
    for &caret in &carets[1..] {
        editor.drag_select(
            id,
            drag(caret, caret, DragBy::Characters, DragCombine::Add, true),
        );
        editor.end_drag_select();
    }
    editor
}

#[test]
fn up_and_down_move_every_caret_keeping_its_column() {
    let world = World::new();
    let mut editor = editor_with_carets(&world, "abcdef\nab\nabcdef\nabcdef", &[5, 15]);
    editor.run_view_command_on_active(&move_lines(true, false));
    assert_eq!(all_selections(&editor), vec![(9, 9), (22, 22)]);
    // The first caret gets its column back past the short line; the last stays on the last line.
    editor.run_view_command_on_active(&move_lines(true, false));
    assert_eq!(all_selections(&editor), vec![(15, 15), (22, 22)]);
    assert_eq!(
        editor.active_document().unwrap().selection(),
        Selection::caret(22)
    );
}

#[test]
fn shift_down_extends_every_selection_and_carets_that_meet_merge() {
    let world = World::new();
    let mut editor = editor_with_carets(&world, "abc\nabc\nabc\nabc", &[1, 9]);
    editor.run_view_command_on_active(&move_lines(true, true));
    assert_eq!(all_selections(&editor), vec![(1, 5), (9, 13)]);

    let mut editor = editor_with_carets(&world, "abc\nabc", &[1, 5]);
    editor.run_view_command_on_active(&move_lines(false, false));
    assert_eq!(all_selections(&editor), vec![(1, 1)]);
}

/// An editor showing `text` with `selections` (the last one primary), added as Ctrl + drags.
fn editor_with_selections(world: &World, text: &str, selections: &[(usize, usize)]) -> Editor {
    let mut editor = editor_with(world, text, 0);
    let id = editor.active_document().unwrap().id;
    let (anchor, head) = selections[0];
    editor.sync_selection_from_view(id, Selection::new(anchor, head));
    for &(anchor, head) in &selections[1..] {
        editor.drag_select(
            id,
            drag(anchor, anchor, DragBy::Characters, DragCombine::Add, true),
        );
        editor.drag_select(
            id,
            drag(anchor, head, DragBy::Characters, DragCombine::Add, false),
        );
        editor.end_drag_select();
    }
    editor
}

#[test]
fn copy_and_cut_take_every_selection_one_per_line() {
    let world = World::new();
    let mut editor = editor_with_selections(&world, "one two three", &[(0, 3), (8, 13)]);
    editor.copy_selections(false);
    assert_eq!(*world.clipboard.borrow(), Ok("one\nthree".to_string()));
    assert_eq!(editor.clipboard_history()[0], "one\nthree");
    editor.copy_selections(true);
    assert_eq!(active_text(&editor), " two ");
    assert_eq!(all_selections(&editor), vec![(0, 0), (5, 5)]);
    // Undone in one step.
    editor.undo();
    assert_eq!(active_text(&editor), "one two three");
}

#[test]
fn a_paste_gives_each_selection_its_line_when_the_counts_match() {
    let world = World::new();
    let mut editor = editor_with_selections(&world, "a-b-c", &[(0, 1), (2, 3), (4, 5)]);
    *world.clipboard.borrow_mut() = Ok("1\n2\n3".into());
    editor.paste_at_selections();
    assert_eq!(active_text(&editor), "1-2-3");

    *world.clipboard.borrow_mut() = Ok("x\ny".into());
    editor.paste_at_selections();
    // Two lines for three carets: each gets all of it.
    assert_eq!(active_text(&editor), "1x\ny-2x\ny-3x\ny");
}

#[test]
fn copying_and_pasting_with_carets_only_works_on_whole_lines() {
    let world = World::new();
    let mut editor = editor_with_carets(&world, "ab\ncd\nef", &[1, 7]);
    assert!(editor.copies_whole_line());
    editor.copy_whole_line(false);
    assert_eq!(*world.clipboard.borrow(), Ok("ab\nef\n".to_string()));
    assert!(editor.pastes_whole_line());
    editor.paste_whole_line();
    assert_eq!(active_text(&editor), "ab\nab\ncd\nef\nef");
    assert_eq!(all_selections(&editor), vec![(4, 4), (13, 13)]);

    let mut editor = editor_with_carets(&world, "ab\ncd\nef", &[1, 7]);
    editor.copy_whole_line(true);
    assert_eq!(active_text(&editor), "cd");
    editor.undo();
    assert_eq!(active_text(&editor), "ab\ncd\nef");
}
