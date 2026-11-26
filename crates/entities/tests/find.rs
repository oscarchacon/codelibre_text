//! Tests for `entities::find`.

use entities::find::*;
use entities::selection::Selection;

fn options(query: &str) -> FindOptions {
    FindOptions {
        query: query.into(),
        ..Default::default()
    }
}

fn pattern(options: &FindOptions) -> regex::Regex {
    options
        .pattern()
        .expect("valid pattern")
        .expect("non-empty query")
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
    let sensitive = FindOptions {
        case_sensitive: true,
        ..options("cat")
    };
    assert_eq!(find_all(&pattern(&sensitive), "Cat cat").len(), 1);
}

#[test]
fn whole_word_skips_partial_matches() {
    let whole = FindOptions {
        whole_word: true,
        ..options("cat")
    };
    assert_eq!(
        find_all(&pattern(&whole), "cat concat cat"),
        vec![0..3, 11..14]
    );
}

#[test]
fn invalid_regex_is_an_error() {
    assert!(FindOptions {
        regex: true,
        ..options("(")
    }
    .pattern()
    .is_err());
}

#[test]
fn empty_matches_are_ignored() {
    let regex = FindOptions {
        regex: true,
        ..options("x*")
    };
    assert!(find_all(&pattern(&regex), "abc").is_empty());
}

#[test]
fn find_forward_wraps_around() {
    let p = pattern(&options("ab"));
    let text = "ab x ab";
    let first = find_from(&p, text, Selection::caret(1), true, true).unwrap();
    assert_eq!(first, Selection::new(5, 7));
    assert_eq!(
        find_from(&p, text, first, true, true),
        Some(Selection::new(0, 2))
    );
    assert_eq!(find_from(&p, text, first, true, false), None);
}

#[test]
fn find_backwards_wraps_around() {
    let p = pattern(&options("ab"));
    let text = "ab x ab";
    assert_eq!(
        find_from(&p, text, Selection::new(5, 7), false, true),
        Some(Selection::new(0, 2))
    );
    assert_eq!(
        find_from(&p, text, Selection::caret(0), false, true),
        Some(Selection::new(5, 7))
    );
    assert_eq!(find_from(&p, text, Selection::caret(0), false, false), None);
}

#[test]
fn find_reports_char_positions_with_unicode_text() {
    let p = pattern(&options("ñ"));
    let first = find_from(&p, "ñañ", Selection::caret(0), true, true).unwrap();
    assert_eq!(first, Selection::new(0, 1));
    assert_eq!(
        find_from(&p, "ñañ", first, true, true),
        Some(Selection::new(2, 3))
    );
}

#[test]
fn replace_next_replaces_a_selected_match_and_selects_the_next() {
    let opts = options("cat");
    let mut text = "cat cat".to_string();
    let next = replace_next(
        &pattern(&opts),
        &mut text,
        Selection::new(0, 3),
        "dog",
        &opts,
    );
    assert_eq!(text, "dog cat");
    assert_eq!(next, Selection::new(4, 7));
}

#[test]
fn replace_next_without_a_selected_match_only_finds() {
    let opts = options("cat");
    let mut text = "cat cat".to_string();
    let next = replace_next(
        &pattern(&opts),
        &mut text,
        Selection::caret(0),
        "dog",
        &opts,
    );
    assert_eq!(text, "cat cat");
    assert_eq!(next, Selection::new(0, 3));
}

#[test]
fn replace_next_expands_regex_groups() {
    let opts = FindOptions {
        regex: true,
        ..options(r"(\w)(\w)")
    };
    let mut text = "ab".to_string();
    replace_next(
        &pattern(&opts),
        &mut text,
        Selection::new(0, 2),
        r"\2\1",
        &opts,
    );
    assert_eq!(text, "ba");
}

#[test]
fn replace_all_counts_and_supports_sublime_groups() {
    let opts = FindOptions {
        regex: true,
        ..options(r"(\w+)@(\w+)")
    };
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
    let opts = FindOptions {
        regex: true,
        ..options(",")
    };
    let mut text = "a,b".to_string();
    replace_all(&pattern(&opts), &mut text, r"\n", &opts);
    assert_eq!(text, "a\nb");
}

#[test]
fn next_unselected_skips_selected_matches_and_wraps() {
    let opts = options("ab");
    let text = "ab ab ab";
    let selected = [Selection::new(3, 5), Selection::new(6, 8)];
    assert_eq!(
        next_unselected(&pattern(&opts), text, Selection::new(3, 5), &selected),
        Some(Selection::new(0, 2))
    );
    let all = [
        Selection::new(0, 2),
        Selection::new(3, 5),
        Selection::new(6, 8),
    ];
    assert_eq!(
        next_unselected(&pattern(&opts), text, Selection::new(0, 2), &all),
        None
    );
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
    assert_eq!(
        find_all_selections(&pattern(&opts), "é aé"),
        vec![Selection::new(0, 1), Selection::new(3, 4)]
    );
}

#[test]
fn toggles_flip_their_option() {
    let mut opts = options("x");
    for toggle in [
        FindToggle::Regex,
        FindToggle::CaseSensitive,
        FindToggle::WholeWord,
        FindToggle::PreserveCase,
    ] {
        opts.toggle(toggle);
    }
    assert!(opts.regex && opts.case_sensitive && opts.whole_word && opts.preserve_case);
    opts.toggle(FindToggle::Regex);
    assert!(!opts.regex && opts.case_sensitive);
}

/// What Sublime 4215 replaced with "Preserve case" on (a probe plugin in a portable copy, the
/// replacement from `slurp_replace_string`, then `replace_all`).
#[test]
fn preserve_case_replaces_as_sublime_does() {
    let cases = [
        // No capital letter found: all lowercase, past the match's end too.
        ("foo", "barBaz", "barbaz"),
        ("hello world", "Goodbye MOON TAIL", "goodbye moon tail"),
        ("a1", "B2CD", "b2cd"),
        ("12", "AbC", "abc"),
        ("1a", "XYZ", "xyz"),
        // Only capitals: all uppercase, `ß` as one letter.
        ("FOO", "barBaz", "BARBAZ"),
        ("AB", "xyzw", "XYZW"),
        ("A", "xyz", "XYZ"),
        ("ÑA", "xyz", "XYZ"),
        ("AB", "éñü", "ÉÑÜ"),
        ("STRASSE", "straße", "STRAẞE"),
        // Otherwise letter by letter; the rest as written.
        ("Foo", "barBaz", "BarBaz"),
        ("fOO", "barBaz", "bARBaz"),
        ("fOo", "bar", "bAr"),
        ("Foo", "BAR", "Bar"),
        ("Ab", "XYZ", "XyZ"),
        ("aB", "xyzw", "xYzw"),
        ("aBcd", "xy", "xY"),
        ("abCD", "xy", "xy"),
        ("Ña", "xyz", "Xyz"),
        ("A1", "xyz", "Xyz"),
        ("1A", "xyz", "xYz"),
        ("日A", "xyz", "xYz"),
        ("A B", "xyzw", "XyZw"),
        ("A_B", "wxyz", "WxYz"),
        ("Hello World", "goodbye moon", "GoodbyE moon"),
        ("HELLO WORLD", "goodbye moon", "GOODByE MOOn"),
        ("Foo_Bar", "baz_qux", "Baz_Qux"),
        ("Foo Bar", "BAZ QUUX", "Baz QuuX"),
    ];
    for (found, replacement, expected) in cases {
        assert_eq!(
            preserve_case(found, replacement),
            expected,
            "{found} → {replacement}"
        );
    }
}

#[test]
fn replacing_with_preserve_case_keeps_each_matchs_case() {
    let opts = FindOptions {
        preserve_case: true,
        ..options("foo")
    };
    let mut text = "foo Foo FOO fOO fOo".to_string();
    assert_eq!(replace_all(&pattern(&opts), &mut text, "bar", &opts), 5);
    assert_eq!(text, "bar Bar BAR bAR bAr");

    let mut text = "Foo foo".to_string();
    replace_next(
        &pattern(&opts),
        &mut text,
        Selection::new(0, 3),
        "bar",
        &opts,
    );
    assert_eq!(text, "Bar foo");
}

#[test]
fn preserve_case_applies_after_regex_groups_expand() {
    let opts = FindOptions {
        regex: true,
        preserve_case: true,
        ..options("f(o+)")
    };
    let mut text = "foo Foo FOO".to_string();
    replace_all(&pattern(&opts), &mut text, r"x\1y", &opts);
    assert_eq!(text, "xooy Xooy XOOY");
}

#[test]
fn escape_finds_the_text_literally() {
    let opts = FindOptions {
        regex: true,
        ..options(&escape("a.b(c)"))
    };
    let mut text = "axb(c) a.b(c)".to_string();
    replace_all(&pattern(&opts), &mut text, "x", &opts);
    assert_eq!(text, "axb(c) x");
}

#[test]
fn the_new_toggles_flip_wrap_in_selection_and_highlighting() {
    let mut opts = options("x");
    assert!(opts.wrap && !opts.in_selection && opts.highlight_matches);
    for toggle in [
        FindToggle::Wrap,
        FindToggle::InSelection,
        FindToggle::HighlightMatches,
    ] {
        opts.toggle(toggle);
    }
    assert!(!opts.wrap && opts.in_selection && !opts.highlight_matches);
}

#[test]
fn inside_keeps_the_matches_whole_in_the_area() {
    let matches = vec![0..3, 4..7, 8..11, 12..15];
    assert_eq!(
        inside(matches.clone(), std::slice::from_ref(&(4..11))),
        vec![4..7, 8..11]
    );
    assert_eq!(
        inside(matches, &[0..3, 9..15]),
        vec![0..3, 12..15],
        "8..11 is cut"
    );
}

#[test]
fn find_among_goes_round_the_matches_given() {
    let text = "foo foo foo foo";
    let matches = [4..7, 8..11];
    let next = |from: usize, forward: bool| {
        find_among(&matches, text, Selection::caret(from), forward, true)
    };
    assert_eq!(next(0, true), Some(Selection::new(4, 7)));
    assert_eq!(next(9, true), Some(Selection::new(4, 7)), "wraps inside");
    assert_eq!(next(4, false), Some(Selection::new(8, 11)));
    assert_eq!(
        find_among(&matches, text, Selection::caret(12), true, false),
        None
    );
}

#[test]
fn replace_all_in_replaces_only_inside_the_area() {
    let opts = options("foo");
    let mut text = "foo foo foo foo".to_string();
    assert_eq!(
        replace_all_in(&pattern(&opts), &mut text, "bar", &opts, 4..11),
        2
    );
    assert_eq!(text, "foo bar bar foo");
    let mut text = "foo foo".to_string();
    assert_eq!(
        replace_all_in(&pattern(&opts), &mut text, "bar", &opts, 1..7),
        1
    );
    assert_eq!(text, "foo bar", "a match cut by the area stays");
}

#[test]
fn replace_selected_changes_nothing_without_a_match_selected() {
    let opts = options("foo");
    let mut text = "foo x".to_string();
    let p = pattern(&opts);
    assert_eq!(
        replace_selected(&p, &mut text, Selection::new(4, 5), "bar", &opts),
        None
    );
    assert_eq!(
        replace_selected(&p, &mut text, Selection::new(0, 3), "bar", &opts),
        Some(Selection::caret(3))
    );
    assert_eq!(text, "bar x");
}

/// What Sublime 4215's Replace All made of empty regex matches.
#[test]
fn replace_all_replaces_empty_matches_as_sublime_does() {
    for (query, replacement, text, expected) in [
        ("^", "// ", "a\nb\nc", "// a\n// b\n// c"),
        ("$", ";", "a\nb", "a;\nb;"),
        ("x*", "-", "ab", "-a-b-"),
        ("b*", "-", "abba", "-a-a-"),
    ] {
        let opts = FindOptions {
            regex: true,
            ..options(query)
        };
        let mut text = text.to_string();
        replace_all(&pattern(&opts), &mut text, replacement, &opts);
        assert_eq!(text, expected, "{query}");
    }
}
