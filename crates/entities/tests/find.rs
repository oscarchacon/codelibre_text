//! Tests for `entities::find`.

use entities::find::*;
use entities::selection::Selection;

fn options(query: &str) -> FindOptions {
    FindOptions { query: query.into(), ..Default::default() }
}

fn pattern(options: &FindOptions) -> regex::Regex {
    options.pattern().expect("valid pattern").expect("non-empty query")
}

#[test]
fn defaults_wrap_and_ignore_case() {
    let defaults = FindOptions::default();
    assert!(defaults.wrap);
    assert!(!defaults.case_sensitive && !defaults.regex && !defaults.whole_word);
}

#[test]
fn an_empty_query_has_no_pattern() {
    assert!(options("").pattern().unwrap().is_none());
}

#[test]
fn plain_queries_are_literal() {
    assert_eq!(find_all(&pattern(&options("a.b")), "a.b axb").len(), 1);
}

#[test]
fn case_sensitivity() {
    assert_eq!(find_all(&pattern(&options("cat")), "Cat cat").len(), 2);
    let sensitive = FindOptions { case_sensitive: true, ..options("cat") };
    assert_eq!(find_all(&pattern(&sensitive), "Cat cat").len(), 1);
}

#[test]
fn whole_word_skips_partial_matches() {
    let whole = FindOptions { whole_word: true, ..options("cat") };
    assert_eq!(find_all(&pattern(&whole), "cat concat cat"), vec![0..3, 11..14]);
}

#[test]
fn invalid_regex_is_an_error() {
    assert!(FindOptions { regex: true, ..options("(") }.pattern().is_err());
}

#[test]
fn empty_matches_are_ignored() {
    let regex = FindOptions { regex: true, ..options("x*") };
    assert!(find_all(&pattern(&regex), "abc").is_empty());
}

#[test]
fn find_forward_wraps_around() {
    let p = pattern(&options("ab"));
    let text = "ab x ab";
    let first = find_from(&p, text, Selection::caret(1), true, true).unwrap();
    assert_eq!(first, Selection::new(5, 7));
    assert_eq!(find_from(&p, text, first, true, true), Some(Selection::new(0, 2)));
    assert_eq!(find_from(&p, text, first, true, false), None);
}

#[test]
fn find_backwards_wraps_around() {
    let p = pattern(&options("ab"));
    let text = "ab x ab";
    assert_eq!(find_from(&p, text, Selection::new(5, 7), false, true), Some(Selection::new(0, 2)));
    assert_eq!(find_from(&p, text, Selection::caret(0), false, true), Some(Selection::new(5, 7)));
    assert_eq!(find_from(&p, text, Selection::caret(0), false, false), None);
}

#[test]
fn find_reports_char_positions_with_unicode_text() {
    let p = pattern(&options("ñ"));
    let first = find_from(&p, "ñañ", Selection::caret(0), true, true).unwrap();
    assert_eq!(first, Selection::new(0, 1));
    assert_eq!(find_from(&p, "ñañ", first, true, true), Some(Selection::new(2, 3)));
}

#[test]
fn replace_next_replaces_a_selected_match_and_selects_the_next() {
    let opts = options("cat");
    let mut text = "cat cat".to_string();
    let next = replace_next(&pattern(&opts), &mut text, Selection::new(0, 3), "dog", &opts);
    assert_eq!(text, "dog cat");
    assert_eq!(next, Selection::new(4, 7));
}

#[test]
fn replace_next_without_a_selected_match_only_finds() {
    let opts = options("cat");
    let mut text = "cat cat".to_string();
    let next = replace_next(&pattern(&opts), &mut text, Selection::caret(0), "dog", &opts);
    assert_eq!(text, "cat cat");
    assert_eq!(next, Selection::new(0, 3));
}

#[test]
fn replace_next_expands_regex_groups() {
    let opts = FindOptions { regex: true, ..options(r"(\w)(\w)") };
    let mut text = "ab".to_string();
    replace_next(&pattern(&opts), &mut text, Selection::new(0, 2), r"\2\1", &opts);
    assert_eq!(text, "ba");
}

#[test]
fn replace_all_counts_and_supports_sublime_groups() {
    let opts = FindOptions { regex: true, ..options(r"(\w+)@(\w+)") };
    let mut text = "a@b c@d".to_string();
    assert_eq!(replace_all(&pattern(&opts), &mut text, r"\2@\1", &opts), 2);
    assert_eq!(text, "b@a d@c");
}

#[test]
fn replace_all_without_regex_is_literal() {
    let opts = options("a");
    let mut text = "a-a".to_string();
    assert_eq!(replace_all(&pattern(&opts), &mut text, "$1", &opts), 2);
    assert_eq!(text, "$1-$1");
}

#[test]
fn replace_all_understands_newline_escapes_in_regex_mode() {
    let opts = FindOptions { regex: true, ..options(",") };
    let mut text = "a,b".to_string();
    replace_all(&pattern(&opts), &mut text, r"\n", &opts);
    assert_eq!(text, "a\nb");
}

#[test]
fn next_unselected_skips_selected_matches_and_wraps() {
    let opts = options("ab");
    let text = "ab ab ab";
    let selected = [Selection::new(3, 5), Selection::new(6, 8)];
    assert_eq!(next_unselected(&pattern(&opts), text, Selection::new(3, 5), &selected), Some(Selection::new(0, 2)));
    let all = [Selection::new(0, 2), Selection::new(3, 5), Selection::new(6, 8)];
    assert_eq!(next_unselected(&pattern(&opts), text, Selection::new(0, 2), &all), None);
}

#[test]
fn replace_all_with_no_matches_changes_nothing() {
    let opts = options("zzz");
    let mut text = "abc".to_string();
    assert_eq!(replace_all(&pattern(&opts), &mut text, "x", &opts), 0);
    assert_eq!(text, "abc");
}

#[test]
fn find_all_selections_uses_char_indices() {
    let opts = options("é");
    assert_eq!(find_all_selections(&pattern(&opts), "é aé"), vec![Selection::new(0, 1), Selection::new(3, 4)]);
}
