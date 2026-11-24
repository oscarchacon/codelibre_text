//! Tests for `entities::text` (position helpers).

use entities::selection::Selection;
use entities::text::*;

#[test]
fn char_to_byte_handles_multibyte_characters() {
    let text = "año\nb";
    assert_eq!(char_to_byte(text, 0), 0);
    assert_eq!(char_to_byte(text, 2), 3);
    assert_eq!(char_to_byte(text, 4), 5);
}

#[test]
fn char_to_byte_past_the_end_is_the_text_length() {
    assert_eq!(char_to_byte("año", 99), 4);
}

#[test]
fn byte_to_char_is_the_inverse_and_clamps() {
    assert_eq!(byte_to_char("año", 3), 2);
    assert_eq!(byte_to_char("año", 99), 3);
}

#[test]
fn line_start_and_line_end() {
    let text = "ab\ncd";
    assert_eq!(line_start(text, 0), 0);
    assert_eq!(line_start(text, 4), 3);
    assert_eq!(line_end(text, 0), 2);
    assert_eq!(line_end(text, 3), 5);
}

#[test]
fn line_col_is_zero_based() {
    assert_eq!(line_col("ab\ncd", 4), (1, 1));
    assert_eq!(line_col("ab\ncd", 0), (0, 0));
    assert_eq!(line_col("", 0), (0, 0));
    assert_eq!(line_col("ñ\nñx", 4), (1, 2));
}

#[test]
fn line_to_char_finds_line_starts_and_clamps() {
    let text = "ab\ncd\nef";
    assert_eq!(line_to_char(text, 0), 0);
    assert_eq!(line_to_char(text, 1), 3);
    assert_eq!(line_to_char(text, 2), 6);
    assert_eq!(line_to_char(text, 9), 8);
}

#[test]
fn word_characters_include_letters_digits_and_underscore() {
    assert!(is_word_char('a'));
    assert!(is_word_char('é'));
    assert!(is_word_char('7'));
    assert!(is_word_char('_'));
    assert!(!is_word_char('-'));
    assert!(!is_word_char(' '));
}

#[test]
fn word_at_finds_the_word_around_the_caret() {
    let text = "let foo_bar = 1";
    assert_eq!(word_at(text, 6), Selection::new(4, 11));
    assert_eq!(word_at(text, 4), Selection::new(4, 11));
    assert_eq!(word_at(text, 11), Selection::new(4, 11));
}

/// `view.word(point)` at every point of each text, as Sublime 4215 answered (2026-11-24).
#[test]
fn view_word_at_is_sublime_s_view_word() {
    let cases: &[(&str, &[(usize, usize)])] = &[
        ("x = x", &[(0, 1), (0, 1), (1, 3), (2, 4), (4, 5), (4, 5)]),
        ("a..b", &[(0, 1), (0, 1), (1, 3), (3, 4), (3, 4)]),
        ("a...b", &[(0, 1), (0, 1), (1, 4), (1, 4), (4, 5), (4, 5)]),
        (
            "a    b",
            &[(0, 1), (0, 1), (1, 5), (1, 5), (1, 5), (5, 6), (5, 6)],
        ),
        ("a \t b", &[(0, 1), (0, 1), (1, 4), (1, 4), (4, 5), (4, 5)]),
        (
            "a += b",
            &[(0, 1), (0, 1), (1, 4), (2, 4), (2, 5), (5, 6), (5, 6)],
        ),
        (
            "ab \ncd",
            &[(0, 2), (0, 2), (0, 2), (2, 4), (4, 6), (4, 6), (4, 6)],
        ),
        ("a\n\nb", &[(0, 1), (0, 1), (2, 2), (3, 4), (3, 4)]),
        (
            "x =  y",
            &[(0, 1), (0, 1), (1, 3), (2, 5), (3, 5), (5, 6), (5, 6)],
        ),
        (
            "f(a, b)",
            &[
                (0, 1),
                (0, 1),
                (2, 3),
                (2, 3),
                (3, 5),
                (5, 6),
                (5, 6),
                (6, 7),
            ],
        ),
        ("  ab", &[(0, 2), (0, 2), (2, 4), (2, 4), (2, 4)]),
        ("ab  ", &[(0, 2), (0, 2), (0, 2), (2, 4), (2, 4)]),
        (
            "a-b c_d",
            &[
                (0, 1),
                (0, 1),
                (2, 3),
                (2, 3),
                (4, 7),
                (4, 7),
                (4, 7),
                (4, 7),
            ],
        ),
        ("$x @y", &[(0, 1), (1, 2), (1, 2), (2, 4), (4, 5), (4, 5)]),
        (
            "año ñu",
            &[(0, 3), (0, 3), (0, 3), (0, 3), (4, 6), (4, 6), (4, 6)],
        ),
        ("a \n b", &[(0, 1), (0, 1), (1, 3), (2, 4), (4, 5), (4, 5)]),
        ("a\n  b", &[(0, 1), (0, 1), (1, 4), (2, 4), (4, 5), (4, 5)]),
        (". \n.", &[(0, 1), (0, 2), (1, 3), (2, 4), (3, 4)]),
        (
            "a .\n\n. b",
            &[
                (0, 1),
                (0, 1),
                (1, 3),
                (2, 4),
                (4, 4),
                (4, 6),
                (5, 7),
                (7, 8),
                (7, 8),
            ],
        ),
        ("\n\n", &[(0, 0), (1, 1), (2, 2)]),
        ("a\n", &[(0, 1), (0, 1), (2, 2)]),
        (" \n ", &[(0, 1), (0, 2), (1, 3), (2, 3)]),
    ];
    for (text, words) in cases {
        let got: Vec<(usize, usize)> = (0..words.len())
            .map(|p| {
                let w = view_word_at(text, p);
                (w.start(), w.end())
            })
            .collect();
        assert_eq!(&got, words, "{text:?}");
    }
}

#[test]
fn word_at_is_empty_between_words() {
    assert_eq!(word_at("a = b", 2), Selection::caret(2));
}

#[test]
fn word_at_clamps_past_the_end() {
    assert_eq!(word_at("abc", 50), Selection::new(0, 3));
}

#[test]
fn selected_text_works_in_both_directions_with_unicode() {
    assert_eq!(selected_text("héllo", Selection::new(4, 1)), "éll");
    assert_eq!(selected_text("héllo", Selection::caret(2)), "");
}
