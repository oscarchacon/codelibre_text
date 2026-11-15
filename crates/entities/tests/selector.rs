//! Tests for `entities::selector` (scope selectors).

use entities::selector::{matches_selector, score_selector};

const STRING: &str = "source.python meta.string string.quoted.double.python";

#[test]
fn a_selector_matches_whole_parts_of_any_scope_name() {
    assert!(matches_selector(STRING, "source"));
    assert!(matches_selector(STRING, "string.quoted"));
    assert!(
        !matches_selector(STRING, "string.quo"),
        "only whole parts of the name"
    );
    assert!(!matches_selector(STRING, "comment"));
}

#[test]
fn spaces_mean_one_scope_inside_another() {
    assert!(matches_selector(STRING, "source.python string"));
    assert!(
        !matches_selector(STRING, "string source.python"),
        "in that order"
    );
}

#[test]
fn alternatives_and_exclusions() {
    assert!(matches_selector(STRING, "comment, string"));
    assert!(matches_selector(STRING, "comment | string"));
    assert!(!matches_selector(STRING, "source - string"));
    assert!(matches_selector("source.python", "source - string"));
    assert!(
        !matches_selector(STRING, "-string"),
        "an exclusion alone matches the rest"
    );
    assert!(matches_selector("source.python", "- string"));
    assert!(matches_selector(STRING, "(comment, string)"));
}

#[test]
fn parentheses_group_and_ampersand_asks_for_both() {
    let comment = "source.python comment.line.number-sign.python";
    assert!(!matches_selector(STRING, "source - (string | comment)"));
    assert!(!matches_selector(comment, "source - (string | comment)"));
    assert!(matches_selector(
        "source.python meta.function",
        "source - (string | comment)"
    ));
    // Without the group the exclusion binds tighter: (source - string) | comment.
    assert!(matches_selector(comment, "source - string | comment"));
    assert!(matches_selector(
        STRING,
        "(source.js | source.python) & string"
    ));
    assert!(!matches_selector(
        "source.js",
        "(source.js | source.python) & string"
    ));
    assert!(matches_selector(STRING, "source & -comment"));
    assert!(!matches_selector(comment, "source & -comment"));
    assert!(matches_selector(STRING, "((string))"));
    assert!(
        !matches_selector(STRING, "(string"),
        "an unclosed group matches nothing"
    );
    assert!(
        !matches_selector(STRING, "comment) | string"),
        "a stray ) ends the selector"
    );
    assert_eq!(
        score_selector(STRING, "(string)"),
        score_selector(STRING, "string")
    );
    assert_eq!(
        score_selector(STRING, "(comment | string) - meta.tag"),
        score_selector(STRING, "string")
    );
}

#[test]
fn a_dash_inside_a_name_is_part_of_it() {
    assert!(matches_selector(
        "text.html.markdown-gfm",
        "text.html.markdown-gfm"
    ));
    assert!(!matches_selector(
        "text.html.markdown",
        "text.html.markdown-gfm"
    ));
    assert!(
        matches_selector("source.python", "source -string"),
        "a dash after a space excludes"
    );
}

#[test]
fn scores_are_sublimes_own_values() {
    // As Sublime Text 4215 answers: the matched parts at bit 3 × position + 1, the last element
    // at the rightmost scope that matches it.
    assert_eq!(score_selector("a", "a"), 2);
    assert_eq!(score_selector("x a", "a"), 16);
    assert_eq!(score_selector("a b c", "c"), 128);
    assert_eq!(score_selector("s a b a b a", "a"), 65536, "the rightmost a");
    assert_eq!(score_selector("s a b a b a", "a b"), 9216);
    assert_eq!(score_selector("s a b a b a", "s a a a"), 66578);
    assert_eq!(
        score_selector("source.python", "source.python"),
        4,
        "two dotted parts"
    );
    assert_eq!(
        score_selector("a.b.c.d.e.f.g.h.i", "a.b.c.d.e.f.g.h"),
        14,
        "at most 7"
    );
    assert_eq!(
        score_selector("a", "a.b"),
        0,
        "more parts than the scope has"
    );
}

#[test]
fn only_the_innermost_21_scopes_have_slots_of_their_own() {
    let deep = (0..25)
        .map(|i| format!("s{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(score_selector(&deep, "s24"), 1 << 61);
    assert_eq!(score_selector(&deep, "s18"), 1 << 43);
    assert_eq!(
        score_selector(&deep, "s0 s24"),
        (1 << 61) + 2,
        "s0 to s4 share the first slot"
    );
}

/// Sublime reads no further at two terms with no operator between them, and an empty part
/// (but a separator ending the selector) makes it match nothing.
#[test]
fn what_sublime_reads_of_odd_selectors() {
    assert_eq!(score_selector("a b c", "(a) b"), 2);
    assert_eq!(score_selector("a b c", "(a) b, c"), 2, ", c is not read");
    assert_eq!(score_selector("a b c", "(a b) c"), 18);
    assert_eq!(score_selector("a b c", "((a) (b))"), 0, "left open");
    assert_eq!(score_selector("a b c", "a,"), 2);
    assert_eq!(score_selector("a b c", "a, , c"), 0);
    assert_eq!(score_selector("a b c", "(a,)"), 0);
    assert_eq!(score_selector("a b c", "a -"), 0);
    assert_eq!(score_selector("a b c", "a || c"), 128);
}

/// Every case asked of Sublime Text 4215 itself (`sublime.score_selector`, run with a throwaway
/// profile; see `docs/plugins.md`): the scope, the selector and what it answered.
#[test]
fn every_case_answers_as_sublime_4215_does() {
    let cases: Vec<(String, String, u64)> =
        serde_json::from_str(include_str!("data/score_selector_sublime_4215.json")).unwrap();
    let wrong: Vec<String> = cases
        .iter()
        .filter(|(scope, selector, want)| score_selector(scope, selector) != *want)
        .map(|(scope, selector, want)| {
            format!(
                "{scope:?} / {selector:?}: Sublime {want}, ours {}",
                score_selector(scope, selector)
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {}:\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

#[test]
fn deeper_and_longer_matches_score_more() {
    assert_eq!(score_selector(STRING, "comment"), 0);
    assert!(score_selector(STRING, "string") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "source.python") > score_selector(STRING, "source"));
    assert!(score_selector(STRING, "string.quoted") > score_selector(STRING, "string"));
    assert_eq!(
        score_selector(STRING, ""),
        1,
        "an empty selector matches everything"
    );
}
