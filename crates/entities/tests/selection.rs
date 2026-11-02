//! Tests for `entities::selection`.

use entities::selection::Selection;

#[test]
fn a_caret_is_an_empty_selection() {
    let caret = Selection::caret(3);
    assert!(caret.is_empty());
    assert_eq!((caret.anchor, caret.head), (3, 3));
    assert_eq!(caret.len(), 0);
}

#[test]
fn start_and_end_ignore_the_direction() {
    let backwards = Selection::new(7, 2);
    assert_eq!(backwards.start(), 2);
    assert_eq!(backwards.end(), 7);
    assert_eq!(backwards.len(), 5);
    assert!(!backwards.is_empty());
}

#[test]
fn clamped_limits_both_ends_to_the_text_length() {
    assert_eq!(Selection::new(2, 10).clamped(5), Selection::new(2, 5));
    assert_eq!(Selection::new(9, 8).clamped(5), Selection::caret(5));
    assert_eq!(Selection::new(1, 2).clamped(5), Selection::new(1, 2));
}

#[test]
fn default_is_a_caret_at_the_start() {
    assert_eq!(Selection::default(), Selection::caret(0));
}
