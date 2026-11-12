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
fn parentheses_group_and_ampersand_asks_for_both() {
    let comment = "source.python comment.line.number-sign.python";
    assert!(!matches_selector(STRING, "source - (string | comment)"));
    assert!(!matches_selector(comment, "source - (string | comment)"));
    assert!(matches_selector("source.python meta.function", "source - (string | comment)"));
    // Without the group the exclusion binds tighter: (source - string) | comment.
    assert!(matches_selector(comment, "source - string | comment"));
    assert!(matches_selector(STRING, "(source.js | source.python) & string"));
    assert!(!matches_selector("source.js", "(source.js | source.python) & string"));
    assert!(matches_selector(STRING, "source & -comment"));
    assert!(!matches_selector(comment, "source & -comment"));
    assert!(matches_selector(STRING, "((string))"));
    assert!(matches_selector(STRING, "(string"), "an unclosed group still matches");
    assert!(matches_selector(STRING, "comment) | string"), "a stray ) does not hide what follows");
    assert_eq!(score_selector(STRING, "(string)"), score_selector(STRING, "string"));
    assert_eq!(score_selector(STRING, "(comment | string) - meta.tag"), score_selector(STRING, "string"));
}

#[test]
fn a_dash_inside_a_name_is_part_of_it() {
    assert!(matches_selector("text.html.markdown-gfm", "text.html.markdown-gfm"));
    assert!(!matches_selector("text.html.markdown", "text.html.markdown-gfm"));
    assert!(matches_selector("source.python", "source -string"), "a dash after a space excludes");
}

#[test]
fn scores_are_sublimes_own_values() {
    // The values Sublime gives (sublimehq/sublime_text#2152 and the forum): 8 per first scope, 8³ per depth.
    assert_eq!(score_selector("a", "a"), 8);
    assert_eq!(score_selector("x a", "a"), 64);
    assert_eq!(score_selector("a b", "b"), 64);
    assert_eq!(score_selector("a b c", "c"), 512);
    assert_eq!(score_selector("a b c d", "d"), 4096);
    assert_eq!(score_selector("a b b a a", "a"), 8, "the first match ends the selector");
    assert_eq!(score_selector("s a b a b a", "a"), 64);
    assert_eq!(score_selector("s a b a b a", "b"), 512);
    assert_eq!(score_selector("source.python", "source.python"), 16, "two dotted parts");
    assert_eq!(score_selector("source.python", "source"), 8);
    assert_eq!(score_selector("a", "a.b"), 0, "more parts than the scope has");
}

#[test]
fn the_twentieth_scope_is_the_last_distinct_slot() {
    // 1-based positions 1..=20 each own a 3-bit slot (shift 3, 6, … 60). Position 21
    // adds into slot 20, the same way Sublime does (sublimehq/sublime_text#2152).
    let twenty = (0..20).map(|i| format!("s{i}")).collect::<Vec<_>>().join(" ");
    let twenty_one = format!("{twenty} s20");
    assert_eq!(score_selector(&twenty, "s19"), 1 << 60, "scope index 19 is slot 20");
    assert_eq!(score_selector(&twenty_one, "s20"), 1 << 60, "scope index 20 stays in slot 20");
    let full = (1..=20).fold(0u64, |score, slot| score + (7 << (3 * slot)));
    let parts = "1.2.3.4.5.6.7 ".repeat(20);
    let overflow = "1.2.3.4.5.6.7 ".repeat(21);
    assert_eq!(score_selector(parts.trim(), parts.trim()), full);
    assert_eq!(score_selector(overflow.trim(), overflow.trim()), full.wrapping_add(7 << 60));
}

#[test]
fn deeper_and_longer_matches_score_more() {
    assert_eq!(score_selector(STRING, "comment"), 0);
    assert!(score_selector(STRING, "string") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "source.python") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "string.quoted") > score_selector(STRING, "string"));
    assert_eq!(score_selector(STRING, ""), 1, "an empty selector matches everything");
}
