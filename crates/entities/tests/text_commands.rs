//! Tests for `entities::text_commands` (Sublime's text commands as pure functions).

use entities::selection::Selection;
use entities::text::selected_text;
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

#[test]
fn split_into_lines_gives_one_selection_per_line() {
    assert_eq!(split_into_lines("ab\ncd\nef", sel(1, 7)), vec![sel(1, 2), sel(3, 5), sel(6, 7)]);
    assert_eq!(split_into_lines("ab\n\ncd", sel(0, 6)), vec![sel(0, 2), caret(3), sel(4, 6)], "empty lines get a caret");
    assert_eq!(split_into_lines("ab\ncd\n", sel(0, 6)), vec![sel(0, 2), sel(3, 5)], "ending at column 0 skips that line");
    assert_eq!(split_into_lines("ab\ncd", caret(4)), vec![caret(4)]);
    assert_eq!(split_into_lines("áb\nçd", sel(0, 5)), vec![sel(0, 2), sel(3, 5)], "char indices, not bytes");
}

#[test]
fn carets_on_next_line_add_one_line_in_each_direction() {
    assert_eq!(carets_on_next_line("abc\nabc\nabc", &[caret(1)], true), vec![caret(5)]);
    assert_eq!(carets_on_next_line("abc\nabc\nabc", &[caret(9)], false), vec![caret(5)]);
    assert!(carets_on_next_line("abc\nabc", &[caret(5)], true).is_empty(), "no line below");
    assert!(carets_on_next_line("abc\nabc", &[caret(1)], false).is_empty(), "no line above");
}

#[test]
fn carets_on_next_line_keep_the_column_across_short_lines() {
    let text = "abcdef\nab\nabcdef";
    let first = carets_on_next_line(text, &[caret(4)], true);
    assert_eq!(first, vec![caret(9)], "clamped to the end of the short line");
    let second = carets_on_next_line(text, &[caret(4), caret(9)], true);
    assert_eq!(second, vec![caret(14)], "back at column 4");
}

#[test]
fn carets_on_next_line_skip_lines_that_already_have_a_selection() {
    assert_eq!(carets_on_next_line("a\nb\nc\nd", &[caret(0), caret(4)], true), vec![caret(2), caret(6)]);
    assert_eq!(carets_on_next_line("a\nb\nc", &[caret(0), caret(2)], true), vec![caret(4)]);
}

#[test]
fn expand_selection_to_line_upward_puts_the_caret_on_top_and_grows_up() {
    let text = "a\nb\nc";
    let first = expand_selection_to_line_upward(text, caret(4));
    assert_eq!(first, sel(5, 4));
    assert_eq!(expand_selection_to_line_upward(text, first), sel(5, 2));
}

#[test]
fn expand_selection_to_line_content_leaves_the_newline_out() {
    assert_eq!(expand_selection_to_line_content("ab\ncd\n", sel(1, 4)), sel(0, 5));
}

#[test]
fn expand_selection_to_brackets_inside_then_with_the_brackets_then_out() {
    let text = "f(a, [b, c])";
    let inside = expand_selection_to_brackets(text, caret(7));
    assert_eq!(inside, sel(6, 10));
    let with_brackets = expand_selection_to_brackets(text, inside);
    assert_eq!(with_brackets, sel(5, 11));
    assert_eq!(expand_selection_to_brackets(text, with_brackets), sel(2, 11));
    assert_eq!(expand_selection_to_brackets("no brackets", caret(3)), caret(3));
    assert_eq!(expand_selection_to_brackets("(a) (b)", caret(5)), sel(5, 6), "not fooled by a closed pair before");
}

#[test]
fn expand_selection_to_indentation_takes_the_block_then_goes_out() {
    let text = "fn f() {\n    if x {\n        a\n\n        b\n    }\n}";
    let block = expand_selection_to_indentation(text, caret(30), 4);
    assert_eq!(selected_text(text, block), "        a\n\n        b");
    let outer = expand_selection_to_indentation(text, block, 4);
    assert_eq!(selected_text(text, outer), "    if x {\n        a\n\n        b\n    }");
}

#[test]
fn smart_expansion_grows_step_by_step() {
    let text = "call(\"hello world\", x)";
    let mut selection = caret(8);
    let mut steps = Vec::new();
    for _ in 0..5 {
        selection = expand_selection_smart(text, selection, 4);
        steps.push(selected_text(text, selection).to_string());
    }
    assert_eq!(steps, ["hello", "hello world", "\"hello world\"", "\"hello world\", x", "(\"hello world\", x)"]);
}

#[test]
fn trim_only_modified_lines_and_not_on_the_caret() {
    let saved = "a  \nb  ";
    let mut text = "a  \nb  \nc  ".to_string();
    let options = TrimOptions { only_modified_since: Some(saved), trim_on_caret: true };
    trim_trailing_whitespace_with(&mut text, caret(0), &options);
    assert_eq!(text, "a  \nb  \nc", "only the new line");

    let mut text = "a  \nb  ".to_string();
    let options = TrimOptions { only_modified_since: None, trim_on_caret: false };
    trim_trailing_whitespace_with(&mut text, caret(7), &options);
    assert_eq!(text, "a\nb  ", "the caret's line is kept");
}

/// Reindents the whole text with 4 spaces.
fn reindented(text: &str, syntax: &str) -> String {
    let mut text = text.to_string();
    reindent(&mut text, Selection::caret(0), syntax, (4, true), true, true);
    text
}

#[test]
fn reindent_follows_the_brackets() {
    assert_eq!(
        reindented("fn main() {\nif a {\nb(\nc,\n);\n} else {\nd();\n}\n}\n", "Rust"),
        "fn main() {\n    if a {\n        b(\n            c,\n        );\n    } else {\n        d();\n    }\n}\n"
    );
}

#[test]
fn reindent_uses_tabs_when_the_settings_say_so() {
    let mut text = "a {\nb\n}".to_string();
    reindent(&mut text, Selection::caret(0), "CSS", (4, false), true, true);
    assert_eq!(text, "a {\n\tb\n}");
}

#[test]
fn reindent_ignores_brackets_in_strings_and_comments() {
    let source = "a {\nb = \"{\"; // {\nc = '(';\n/* {\n   x { */\nd\n}\n";
    assert_eq!(reindented(source, "JavaScript"), "a {\n    b = \"{\"; // {\n    c = '(';\n    /* {\n   x { */\n    d\n}\n");
}

#[test]
fn reindent_knows_rust_lifetimes_from_chars() {
    let source = "fn f<'a>(x: &'a str) -> &'a str {\nlet c = '{';\nlet e = '\\'';\nx\n}\n";
    assert_eq!(reindented(source, "Rust"), "fn f<'a>(x: &'a str) -> &'a str {\n    let c = '{';\n    let e = '\\'';\n    x\n}\n");
}

#[test]
fn reindent_leaves_blank_lines_and_preprocessor_lines() {
    assert_eq!(reindented("int f() {\n\n#ifdef A\nb;\n#endif\n}\n", "C++"), "int f() {\n\n#ifdef A\n    b;\n#endif\n}\n");
    assert_eq!(reindented("fn f() {\n#[x]\ng();\n}", "Rust"), "fn f() {\n    #[x]\n    g();\n}");
}

#[test]
fn reindent_with_a_caret_is_one_line_unless_the_whole_file_is_asked_for() {
    let mut text = "a {\nb\nc\n}".to_string();
    let selection = reindent(&mut text, Selection::caret(4), "CSS", (2, true), false, true);
    assert_eq!(text, "a {\n  b\nc\n}");
    assert_eq!(selection, Some(Selection::caret(6)), "the caret stays on its char");
}

#[test]
fn reindent_changes_the_selected_lines_counting_depth_from_the_start() {
    let source = "a {\nb {\nc\nd\n}\n}";
    let mut text = source.to_string();
    let lines = Selection::new(source.find('c').unwrap(), source.find('d').unwrap() + 1);
    reindent(&mut text, lines, "CSS", (2, true), false, true);
    assert_eq!(text, "a {\nb {\n    c\n    d\n}\n}");
}

#[test]
fn reindent_without_force_keeps_lines_already_indented() {
    let mut text = "a {\n b\nc\n}".to_string();
    reindent(&mut text, Selection::caret(0), "CSS", (2, true), true, false);
    assert_eq!(text, "a {\n b\n  c\n}");
}

#[test]
fn reindent_never_goes_below_the_margin() {
    assert_eq!(reindented("}\n}\na\n", "Rust"), "}\n}\na\n");
}

#[test]
fn reindent_refuses_syntaxes_where_indentation_is_the_code() {
    assert!(!can_reindent("Python"));
    assert!(!can_reindent("Plain Text"));
    let mut text = "if a:\nb\n".to_string();
    assert_eq!(reindent(&mut text, Selection::caret(0), "Python", (4, true), true, true), None);
    assert_eq!(text, "if a:\nb\n");
}

#[test]
fn reindent_says_when_nothing_changed() {
    let mut text = "a {\n  b\n}".to_string();
    assert_eq!(reindent(&mut text, Selection::caret(0), "CSS", (2, true), true, true), None);
}

#[test]
fn move_to_goes_to_the_ends_of_lines_and_of_the_text() {
    let text = "  one\n  two\n";
    let caret = Selection::caret(9);
    assert_eq!(move_to(text, caret, "bof", false), Some(Selection::caret(0)));
    assert_eq!(move_to(text, caret, "eof", false), Some(Selection::caret(12)));
    assert_eq!(move_to(text, caret, "hardbol", false), Some(Selection::caret(6)));
    assert_eq!(move_to(text, caret, "eol", false), Some(Selection::caret(11)));
    assert_eq!(move_to(text, caret, "hardeol", false), Some(Selection::caret(11)));
    assert_eq!(move_to(text, caret, "bol", false), Some(Selection::caret(8)), "the first char");
    assert_eq!(move_to(text, Selection::caret(8), "bol", false), Some(Selection::caret(6)), "then the start");
    assert_eq!(move_to(text, caret, "nowhere", false), None);
}

#[test]
fn move_to_extends_from_the_anchor() {
    assert_eq!(move_to("abc", Selection::new(1, 2), "eof", true), Some(Selection::new(1, 3)));
    assert_eq!(move_to("abc", Selection::new(1, 2), "eof", false), Some(Selection::caret(3)));
}

#[test]
fn moving_by_characters_collapses_a_selection_first() {
    let selection = Selection::new(1, 3);
    assert_eq!(move_by("abcd", selection, "characters", true, false), Some(Selection::caret(3)));
    assert_eq!(move_by("abcd", selection, "characters", false, false), Some(Selection::caret(1)));
    assert_eq!(move_by("abcd", selection, "characters", true, true), Some(Selection::new(1, 4)));
    assert_eq!(move_by("ab", Selection::caret(2), "characters", true, false), Some(Selection::caret(2)));
    assert_eq!(move_by("ab", Selection::caret(0), "characters", false, false), Some(Selection::caret(0)));
}

#[test]
fn moving_by_lines_keeps_the_column_when_it_can() {
    let text = "long line\nab\nanother";
    assert_eq!(move_by(text, Selection::caret(5), "lines", true, false), Some(Selection::caret(12)), "the end of the short line");
    assert_eq!(move_by(text, Selection::caret(11), "lines", false, false), Some(Selection::caret(1)));
    assert_eq!(move_by(text, Selection::caret(14), "lines", true, false), Some(Selection::caret(20)), "from the last line, the end");
    assert_eq!(move_by(text, Selection::caret(3), "lines", false, false), Some(Selection::caret(0)), "from the first, the start");
}

#[test]
fn moving_by_words_stops_at_starts_and_ends() {
    let text = "one, two\nthree";
    let by = |at: usize, unit: &str, forward: bool| move_by(text, Selection::caret(at), unit, forward, false).unwrap().head;
    assert_eq!(by(0, "words", true), 3, "the comma");
    assert_eq!(by(3, "words", true), 5, "past the space");
    assert_eq!(by(5, "words", true), 8, "the end of the line");
    assert_eq!(by(8, "words", true), 9, "the line break is a stop");
    assert_eq!(by(8, "words", false), 5);
    assert_eq!(by(5, "words", false), 3);
    assert_eq!(by(0, "word_ends", true), 3);
    assert_eq!(by(4, "word_ends", true), 8, "over the space");
    assert_eq!(by(8, "word_ends", false), 4, "the end of the comma");
    assert_eq!(by(14, "words", true), 14, "nowhere to go");
    assert_eq!(by(0, "words", false), 0);
}

#[test]
fn moving_by_an_unknown_unit_is_refused() {
    assert_eq!(move_by("a", Selection::caret(0), "pages", true, false), None);
}
