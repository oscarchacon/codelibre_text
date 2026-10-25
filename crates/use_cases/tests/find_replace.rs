//! Tests for the find / replace use cases (`use_cases::find_replace`).

use entities::find::FindOptions;
use entities::selection::Selection;
use test_support::{active_text, status, World};
use use_cases::editing::TextCommand;
use use_cases::Editor;

fn editor_with(world: &World, text: &str, selection: Selection) -> Editor {
    let mut editor = world.editor();
    editor.run_text_command(TextCommand::Insert(text.into()));
    let id = editor.active_document().unwrap().id;
    editor.sync_selection_from_view(id, selection);
    editor
}

fn selection(editor: &Editor) -> Selection {
    editor.active_document().unwrap().selection()
}

fn query(text: &str) -> FindOptions {
    FindOptions { query: text.into(), ..Default::default() }
}

#[test]
fn a_single_line_selection_becomes_the_query_when_the_panel_opens() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat dog", Selection::new(4, 7));
    editor.use_selection_as_query_if_single_line();
    assert_eq!(editor.state().find.query, "dog");
}

#[test]
fn multi_line_selections_do_not_replace_the_query() {
    let world = World::new();
    let mut editor = editor_with(&world, "a\nb", Selection::new(0, 3));
    editor.set_find_options(query("keep"));
    editor.use_selection_as_query_if_single_line();
    assert_eq!(editor.state().find.query, "keep");
}

#[test]
fn find_under_caret_selects_the_word_and_searches_it_as_a_whole_word() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo bar", Selection::caret(5));
    editor.set_find_options(FindOptions { regex: true, ..query("x") });
    editor.use_selection_as_query(true);
    let find = &editor.state().find;
    assert_eq!(find.query, "bar");
    assert!(find.whole_word && !find.regex);
    assert_eq!(selection(&editor), Selection::new(4, 7));
}

#[test]
fn using_an_explicit_selection_is_not_whole_word() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo bar", Selection::new(0, 2));
    editor.use_selection_as_query(true);
    assert_eq!(editor.state().find.query, "fo");
    assert!(!editor.state().find.whole_word);
}

#[test]
fn find_next_and_previous_move_the_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat dog cat", Selection::caret(0));
    editor.set_find_options(query("cat"));
    assert!(editor.find_next(true));
    assert_eq!(selection(&editor), Selection::new(0, 3));
    editor.find_next(true);
    assert_eq!(selection(&editor), Selection::new(8, 11));
    editor.find_next(false);
    assert_eq!(selection(&editor), Selection::new(0, 3));
}

#[test]
fn find_next_with_an_empty_query_reports_nothing_to_search() {
    let world = World::new();
    let mut editor = world.editor();
    assert!(!editor.find_next(true));
}

#[test]
fn not_found_is_reported_in_the_status_bar() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(query("zzz"));
    editor.find_next(true);
    assert_eq!(status(&editor), "Unable to find zzz");
}

#[test]
fn an_invalid_regex_is_shown_as_a_find_error() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(FindOptions { regex: true, ..query("(") });
    assert!(editor.find_next(true));
    assert!(editor.state().find_error.is_some());
}

#[test]
fn ctrl_d_selects_the_word_then_adds_the_next_occurrences() {
    let world = World::new();
    let mut editor = editor_with(&world, "x = x + x", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(0, 1));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(4, 5));
    editor.select_word_or_next_occurrence();
    let doc = editor.active_document().unwrap();
    assert_eq!(doc.selections(), vec![Selection::new(0, 1), Selection::new(4, 5), Selection::new(8, 9)]);
    editor.select_word_or_next_occurrence();
    assert_eq!(editor.active_document().unwrap().selections().len(), 3, "every occurrence is already selected");
}

#[test]
fn ctrl_d_from_a_word_matches_whole_words_and_case() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo Foo foobar foo", Selection::caret(1));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(15, 18));
}

#[test]
fn ctrl_d_from_part_of_a_word_matches_inside_words() {
    let world = World::new();
    let mut editor = editor_with(&world, "foo foobar", Selection::new(0, 3));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(0, 3), "a whole-word selection skips foobar");
    let mut editor = editor_with(&world, "fo foobar", Selection::new(0, 1));
    editor.select_word_or_next_occurrence();
    assert_eq!(selection(&editor), Selection::new(3, 4), "part of a word also matches inside words");
}

#[test]
fn typing_deleting_and_moving_with_several_selections() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab ab", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    editor.insert_at_selections("xyz");
    assert_eq!(active_text(&editor), "xyz xyz");
    editor.delete_at_selections(false);
    assert_eq!(active_text(&editor), "xy xy");
    editor.move_carets(entities::text_commands::CaretMotion::LineStart);
    editor.delete_at_selections(true);
    assert_eq!(active_text(&editor), "y xy", "both carets went to the line start and merged");
    assert!(editor.active_document().unwrap().extra_selections().is_empty());
}

#[test]
fn escape_keeps_only_the_primary_selection() {
    let world = World::new();
    let mut editor = editor_with(&world, "a a", Selection::caret(0));
    editor.select_word_or_next_occurrence();
    editor.select_word_or_next_occurrence();
    editor.single_selection();
    let doc = editor.active_document().unwrap();
    assert!(doc.extra_selections().is_empty());
    assert_eq!(doc.selection(), Selection::new(2, 3));
}

#[test]
fn replace_next_replaces_the_selected_match() {
    let world = World::new();
    let mut editor = editor_with(&world, "cat cat", Selection::new(0, 3));
    editor.set_find_options(query("cat"));
    editor.set_replacement("dog".into());
    editor.replace_next();
    assert_eq!(active_text(&editor), "dog cat");
    assert_eq!(selection(&editor), Selection::new(4, 7));
}

#[test]
fn replace_all_reports_how_many_were_replaced() {
    let world = World::new();
    let mut editor = editor_with(&world, "a-a-a", Selection::caret(0));
    editor.set_find_options(query("a"));
    editor.set_replacement("b".into());
    editor.replace_all();
    assert_eq!(active_text(&editor), "b-b-b");
    assert_eq!(status(&editor), "Replaced 3 occurrences");
}

#[test]
fn replace_with_an_empty_query_does_nothing() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.replace_all();
    editor.replace_next();
    assert_eq!(active_text(&editor), "abc");
}

#[test]
fn find_matches_lists_every_match_and_follows_edits() {
    let world = World::new();
    let mut editor = editor_with(&world, "ab ab", Selection::caret(0));
    editor.set_find_options(query("ab"));
    assert_eq!(editor.find_matches(), vec![0..2, 3..5]);
    editor.run_text_command(TextCommand::Insert("ab ".into()));
    assert_eq!(editor.find_matches().len(), 3);
}

#[test]
fn find_matches_records_regex_errors() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    editor.set_find_options(FindOptions { regex: true, ..query("[") });
    assert!(editor.find_matches().is_empty());
    assert!(editor.state().find_error.is_some());
    editor.set_find_options(query("b"));
    assert_eq!(editor.find_matches(), vec![1..2]);
    assert!(editor.state().find_error.is_none());
}

#[test]
fn find_matches_is_empty_without_a_query() {
    let world = World::new();
    let mut editor = editor_with(&world, "abc", Selection::caret(0));
    assert!(editor.find_matches().is_empty());
}
