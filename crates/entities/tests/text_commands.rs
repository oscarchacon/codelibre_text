//! Tests for `entities::text_commands` (Sublime's text commands as pure functions).

use entities::selection::Selection;
use entities::text_commands::*;

/// Runs a command on `text` with `selection` and returns the new text and selection.
fn run(text: &str, selection: Selection, command: impl FnOnce(&mut String, Selection) -> Selection) -> (String, Selection) {
    let mut text = text.to_string();
    let selection = command(&mut text, selection);
    (text, selection)
}

fn caret(position: usize) -> Selection {
    Selection::caret(position)
}

fn sel(anchor: usize, head: usize) -> Selection {
    Selection::new(anchor, head)
}

// ----- replace_selection / insert_tab -------------------------------------------------------------

#[test]
fn replace_selection_puts_the_caret_after_the_insert() {
    let (text, selection) = run("hello world", sel(6, 11), |t, s| replace_selection(t, s, "there"));
    assert_eq!(text, "hello there");
    assert_eq!(selection, caret(11));
}

#[test]
fn insert_tab_inserts_a_tab_character() {
    let (text, selection) = run("ab", caret(1), |t, s| insert_tab(t, s, 4, false));
    assert_eq!(text, "a\tb");
    assert_eq!(selection, caret(2));
}

#[test]
fn insert_tab_with_spaces_goes_to_the_next_tab_stop() {
    let (text, selection) = run("ab", caret(1), |t, s| insert_tab(t, s, 4, true));
    assert_eq!(text, "a   b");
    assert_eq!(selection, caret(4));
}

#[test]
fn insert_tab_counts_existing_tabs_as_full_stops() {
    let (text, _) = run("\tx", caret(1), |t, s| insert_tab(t, s, 4, true));
    assert_eq!(text, "\t    x");
}

#[test]
fn insert_tab_on_a_multi_line_selection_indents_the_lines() {
    let (text, _) = run("a\nb", sel(0, 3), |t, s| insert_tab(t, s, 4, true));
    assert_eq!(text, "    a\n    b");
}

// ----- newline_auto_indent -------------------------------------------------------------------------

#[test]
fn enter_keeps_the_current_indentation() {
    let (text, selection) = run("    x", caret(5), |t, s| newline_auto_indent(t, s, 4, true));
    assert_eq!(text, "    x\n    ");
    assert_eq!(selection, caret(10));
}

#[test]
fn enter_between_braces_opens_an_indented_block() {
    let (text, selection) = run("fn x() {}", caret(8), |t, s| newline_auto_indent(t, s, 4, true));
    assert_eq!(text, "fn x() {\n    \n}");
    assert_eq!(selection, caret(13));
}

#[test]
fn enter_after_an_open_bracket_indents_one_level() {
    let (text, selection) = run("f(", caret(2), |t, s| newline_auto_indent(t, s, 4, true));
    assert_eq!(text, "f(\n    ");
    assert_eq!(selection, caret(7));
}

#[test]
fn enter_after_a_colon_indents_one_level() {
    let (text, _) = run("def f():", caret(8), |t, s| newline_auto_indent(t, s, 4, true));
    assert_eq!(text, "def f():\n    ");
}

#[test]
fn enter_uses_a_tab_when_not_translating_tabs() {
    let (text, _) = run("if x {", caret(6), |t, s| newline_auto_indent(t, s, 4, false));
    assert_eq!(text, "if x {\n\t");
}

#[test]
fn enter_replaces_the_selection() {
    let (text, selection) = run("ab", sel(0, 2), |t, s| newline_auto_indent(t, s, 4, true));
    assert_eq!(text, "\n");
    assert_eq!(selection, caret(1));
}

// ----- indent / unindent ---------------------------------------------------------------------------

#[test]
fn indent_moves_the_caret_with_the_text() {
    let (text, selection) = run("a", caret(1), |t, s| indent(t, s, 4, true));
    assert_eq!(text, "    a");
    assert_eq!(selection, caret(5));
}

#[test]
fn indent_skips_empty_lines_in_a_block() {
    let (text, _) = run("a\n\nb", sel(0, 4), |t, s| indent(t, s, 4, true));
    assert_eq!(text, "    a\n\n    b");
}

#[test]
fn indent_on_a_single_empty_line_still_indents() {
    let (text, _) = run("", caret(0), |t, s| indent(t, s, 4, false));
    assert_eq!(text, "\t");
}

#[test]
fn unindent_removes_one_level_of_tabs_or_spaces() {
    assert_eq!(run("\tx", caret(2), |t, s| unindent(t, s, 4)).0, "x");
    assert_eq!(run("  x", caret(0), |t, s| unindent(t, s, 4)).0, "x");
    assert_eq!(run("      x", caret(0), |t, s| unindent(t, s, 4)).0, "  x");
    assert_eq!(run("x", caret(0), |t, s| unindent(t, s, 4)).0, "x");
}

#[test]
fn indent_then_unindent_restores_the_text() {
    let (indented, selection) = run("a\nb", sel(0, 3), |t, s| indent(t, s, 4, true));
    assert_eq!(indented, "    a\n    b");
    let (text, _) = run(&indented, selection, |t, s| unindent(t, s, 4));
    assert_eq!(text, "a\nb");
}

// ----- line commands -------------------------------------------------------------------------------

#[test]
fn duplicate_line_copies_the_line_below_and_follows_it() {
    let (text, selection) = run("a\nb\n", caret(2), duplicate_line);
    assert_eq!(text, "a\nb\nb\n");
    assert_eq!(selection, caret(4));
}

#[test]
fn duplicate_line_with_a_selection_duplicates_only_the_selection() {
    let (text, selection) = run("abc", sel(0, 2), duplicate_line);
    assert_eq!(text, "ababc");
    assert_eq!(selection, sel(0, 2));
}

#[test]
fn swap_line_up_and_down() {
    let mut text = "one\ntwo\n".to_string();
    assert_eq!(swap_line(&mut text, caret(5), true), Some(caret(1)));
    assert_eq!(text, "two\none\n");

    let mut text = "one\ntwo".to_string();
    assert_eq!(swap_line(&mut text, caret(1), false), Some(caret(5)));
    assert_eq!(text, "two\none");
}

#[test]
fn swap_line_stops_at_the_edges() {
    let mut text = "one\ntwo".to_string();
    assert_eq!(swap_line(&mut text, caret(1), true), None);
    assert_eq!(swap_line(&mut text, caret(5), false), None);
    assert_eq!(text, "one\ntwo");
}

#[test]
fn delete_line_removes_the_whole_line() {
    assert_eq!(run("a\nb\nc", caret(2), delete_line), ("a\nc".to_string(), caret(2)));
    assert_eq!(run("a\nb", caret(3), delete_line), ("a".to_string(), caret(0)));
    assert_eq!(run("abc", caret(1), delete_line), (String::new(), caret(0)));
}

#[test]
fn add_line_keeps_the_indentation() {
    let (text, selection) = run("    x", caret(2), |t, s| add_line(t, s, false));
    assert_eq!(text, "    x\n    ");
    assert_eq!(selection, caret(10));

    let (text, selection) = run("    x", caret(2), |t, s| add_line(t, s, true));
    assert_eq!(text, "    \n    x");
    assert_eq!(selection, caret(4));
}

#[test]
fn delete_to_line_end_and_start() {
    assert_eq!(run("abc\ndef", caret(1), |t, s| delete_to_line_edge(t, s, true)), ("a\ndef".into(), caret(1)));
    assert_eq!(run("abc\ndef", caret(2), |t, s| delete_to_line_edge(t, s, false)), ("c\ndef".into(), caret(0)));
}

#[test]
fn delete_to_line_end_at_the_end_joins_the_next_line() {
    assert_eq!(run("abc\ndef", caret(3), |t, s| delete_to_line_edge(t, s, true)), ("abcdef".into(), caret(3)));
}

#[test]
fn delete_to_line_edge_with_a_selection_deletes_the_selection() {
    assert_eq!(run("abcdef", sel(1, 3), |t, s| delete_to_line_edge(t, s, true)), ("adef".into(), caret(1)));
}

#[test]
fn expand_selection_to_line_then_to_the_next_line() {
    let text = "a\nb\nc";
    let first = expand_selection_to_line(text, caret(2));
    assert_eq!(first, sel(2, 4));
    assert_eq!(expand_selection_to_line(text, first), sel(2, 5));
}

#[test]
fn expand_selection_on_the_last_line_has_no_newline() {
    assert_eq!(expand_selection_to_line("a\nb", caret(3)), sel(2, 3));
}

#[test]
fn join_lines_joins_with_one_space() {
    assert_eq!(run("a  \n   b", caret(0), join_lines).0, "a b");
    assert_eq!(run("a\nb\nc", sel(0, 5), join_lines).0, "a b c");
}

#[test]
fn join_lines_with_an_empty_next_line_adds_no_space() {
    assert_eq!(run("a\n\nb", caret(0), join_lines).0, "a\nb");
}

#[test]
fn join_lines_on_the_last_line_does_nothing() {
    assert_eq!(run("abc", caret(1), join_lines), ("abc".to_string(), caret(1)));
}

// ----- case and transpose --------------------------------------------------------------------------

#[test]
fn change_case_variants() {
    let all = |t: &str| sel(0, t.chars().count());
    let text = "hello wORLD foo_bar";
    assert_eq!(run(text, all(text), |t, s| change_case(t, s, Case::Upper)).0, "HELLO WORLD FOO_BAR");
    assert_eq!(run(text, all(text), |t, s| change_case(t, s, Case::Lower)).0, "hello world foo_bar");
    assert_eq!(run(text, all(text), |t, s| change_case(t, s, Case::Title)).0, "Hello World Foo_bar");
    assert_eq!(run("aBc", sel(0, 3), |t, s| change_case(t, s, Case::Swap)).0, "AbC");
}

#[test]
fn change_case_without_a_selection_changes_the_word_under_the_caret() {
    let (text, selection) = run("foo bar", caret(5), |t, s| change_case(t, s, Case::Upper));
    assert_eq!(text, "foo BAR");
    assert_eq!(selection, sel(4, 7));
}

#[test]
fn transpose_swaps_the_characters_around_the_caret() {
    let mut text = "abc".to_string();
    assert_eq!(transpose(&mut text, caret(1)), Some(caret(2)));
    assert_eq!(text, "bac");

    let mut text = "abc".to_string();
    assert_eq!(transpose(&mut text, caret(3)), Some(caret(3)));
    assert_eq!(text, "acb");
}

#[test]
fn transpose_needs_a_caret_after_the_first_character() {
    let mut text = "abc".to_string();
    assert_eq!(transpose(&mut text, caret(0)), None);
    assert_eq!(transpose(&mut text, sel(0, 2)), None);
    assert_eq!(text, "abc");
}

// ----- comments ------------------------------------------------------------------------------------

#[test]
fn comment_tokens_per_syntax() {
    assert_eq!(comment_tokens("Rust"), (Some("// "), Some(("/*", "*/"))));
    assert_eq!(comment_tokens("Python"), (Some("# "), None));
    assert_eq!(comment_tokens("SQL"), (Some("-- "), None));
    assert_eq!(comment_tokens("HTML"), (None, Some(("<!--", "-->"))));
    assert_eq!(comment_tokens("CSS"), (None, Some(("/*", "*/"))));
    assert_eq!(comment_tokens("Plain Text"), (None, None));
}

#[test]
fn toggle_comment_comments_and_uncomments_lines() {
    let mut text = "fn a() {\n    x();\n}".to_string();
    let all = sel(0, text.chars().count());
    toggle_comment(&mut text, all, "Rust", false);
    assert_eq!(text, "// fn a() {\n//     x();\n// }");
    let all = sel(0, text.chars().count());
    toggle_comment(&mut text, all, "Rust", false);
    assert_eq!(text, "fn a() {\n    x();\n}");
}

#[test]
fn toggle_comment_aligns_markers_and_skips_empty_lines() {
    let mut text = "  a\n\n  b".to_string();
    toggle_comment(&mut text, sel(0, 8), "Rust", false);
    assert_eq!(text, "  // a\n\n  // b");
}

#[test]
fn toggle_comment_uncomments_markers_without_a_space() {
    let mut text = "//x".to_string();
    toggle_comment(&mut text, caret(0), "Rust", false);
    assert_eq!(text, "x");
}

#[test]
fn toggle_comment_uses_the_syntax_marker() {
    let mut text = "x = 1".to_string();
    toggle_comment(&mut text, caret(0), "Python", false);
    assert_eq!(text, "# x = 1");
}

#[test]
fn syntaxes_without_line_comments_use_block_comments() {
    let mut text = "<p>".to_string();
    toggle_comment(&mut text, caret(0), "HTML", false);
    assert_eq!(text, "<!-- <p> -->");
    toggle_comment(&mut text, caret(0), "HTML", false);
    assert_eq!(text, "<p>");
}

#[test]
fn block_comment_wraps_the_selection() {
    let mut text = "a x+y b".to_string();
    toggle_comment(&mut text, sel(2, 5), "Rust", true);
    assert_eq!(text, "a /* x+y */ b");
}

#[test]
fn plain_text_has_no_comments() {
    let mut text = "hello".to_string();
    assert_eq!(toggle_comment(&mut text, caret(0), "Plain Text", false), None);
    assert_eq!(text, "hello");
}

// ----- whole-text transformations ------------------------------------------------------------------

#[test]
fn reorder_lines_without_a_selection_uses_the_whole_text() {
    assert_eq!(run("b\nB\na", caret(0), |t, s| reorder_lines(t, s, LineOrder::Sort { case_sensitive: true })).0, "B\na\nb");
    assert_eq!(run("b\nA\n", caret(0), |t, s| reorder_lines(t, s, LineOrder::Sort { case_sensitive: false })).0, "A\nb\n");
    assert_eq!(run("1\n2\n3", caret(0), |t, s| reorder_lines(t, s, LineOrder::Reverse)).0, "3\n2\n1");
    assert_eq!(run("b\na\nb\n", caret(0), |t, s| reorder_lines(t, s, LineOrder::Unique)).0, "b\na\n");
}

#[test]
fn reorder_lines_with_a_selection_only_touches_those_lines() {
    let (text, selection) = run("1\n2\n3", sel(0, 3), |t, s| reorder_lines(t, s, LineOrder::Reverse));
    assert_eq!(text, "2\n1\n3");
    assert_eq!(selection, sel(0, 3));
}

#[test]
fn trim_trailing_whitespace_on_every_line() {
    assert_eq!(run("a  \nb\t\nc", caret(0), trim_trailing_whitespace).0, "a\nb\nc");
}

#[test]
fn convert_indentation_between_tabs_and_spaces() {
    assert_eq!(run("\tx\n  \ty", caret(0), |t, s| convert_indentation(t, s, 4, true)).0, "    x\n    y");
    assert_eq!(run("      x", caret(0), |t, s| convert_indentation(t, s, 4, false)).0, "\t  x");
}

// ----- several selections (Ctrl+D) ----------------------------------------------------------------

#[test]
fn replace_selections_types_at_every_selection() {
    let mut text = "ab ab\nab".to_string();
    let carets = replace_selections(&mut text, &[sel(0, 2), sel(3, 5), sel(6, 8)], "héllo");
    assert_eq!(text, "héllo héllo\nhéllo");
    assert_eq!(carets, vec![caret(5), caret(11), caret(17)]);
}

#[test]
fn delete_at_selections_removes_selected_text_or_one_char() {
    let mut text = "abc abc".to_string();
    let carets = delete_at_selections(&mut text, &[caret(1), sel(4, 6)], false);
    assert_eq!((text.as_str(), carets), ("bc c", vec![caret(0), caret(3)]));
    let mut text = "abc abc".to_string();
    let carets = delete_at_selections(&mut text, &[caret(1), caret(7)], true);
    assert_eq!((text.as_str(), carets), ("ac abc", vec![caret(1), caret(6)]));
}

#[test]
fn deleting_at_adjacent_carets_does_not_remove_a_char_twice() {
    let mut text = "abc".to_string();
    let carets = delete_at_selections(&mut text, &[caret(0), caret(1)], true);
    assert_eq!((text.as_str(), carets), ("c", vec![caret(0), caret(0)]));
}

#[test]
fn move_carets_collapses_and_moves_every_selection() {
    let text = "ab\ncd";
    let selections = [sel(0, 2), caret(4)];
    assert_eq!(move_carets(text, &selections, CaretMotion::Left), vec![caret(0), caret(3)]);
    assert_eq!(move_carets(text, &selections, CaretMotion::Right), vec![caret(2), caret(5)]);
    assert_eq!(move_carets(text, &selections, CaretMotion::LineStart), vec![caret(0), caret(3)]);
    assert_eq!(move_carets(text, &selections, CaretMotion::LineEnd), vec![caret(2), caret(5)]);
}

#[test]
fn ensure_trailing_newline_adds_one_only_when_missing() {
    for (input, expected) in [("a", "a\n"), ("a\n", "a\n"), ("", "")] {
        let mut text = input.to_string();
        ensure_trailing_newline(&mut text);
        assert_eq!(text, expected);
    }
}
