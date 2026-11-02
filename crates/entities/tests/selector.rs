//! Tests for `entities::selector` (scope selectors).

use entities::selector::{matches_selector, score_selector};

const STRING: &str = "source.python meta.string string.quoted.double.python";

#[test]
fn a_selector_matches_whole_parts_of_any_scope_name() {
    assert!(matches_selector(STRING, "source"));
    assert!(matches_selector(STRING, "string.quoted"));
    assert!(!matches_selector(STRING, "string.quo"), "only whole parts of the name");
    assert!(!matches_selector(STRING, "comment"));
}

#[test]
fn spaces_mean_one_scope_inside_another() {
    assert!(matches_selector(STRING, "source.python string"));
    assert!(!matches_selector(STRING, "string source.python"), "in that order");
}

#[test]
fn alternatives_and_exclusions() {
    assert!(matches_selector(STRING, "comment, string"));
    assert!(matches_selector(STRING, "comment | string"));
    assert!(!matches_selector(STRING, "source - string"));
    assert!(matches_selector("source.python", "source - string"));
    assert!(!matches_selector(STRING, "-string"), "an exclusion alone matches the rest");
    assert!(matches_selector("source.python", "- string"));
    assert!(matches_selector(STRING, "(comment, string)"));
}

#[test]
fn deeper_and_longer_matches_score_more() {
    assert_eq!(score_selector(STRING, "comment"), 0);
    assert!(score_selector(STRING, "string") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "source.python") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "string.quoted") > score_selector(STRING, "string"));
    assert_eq!(score_selector(STRING, ""), 1, "an empty selector matches everything");
}
