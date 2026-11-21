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

// The cases below were checked against Sublime 4215 (`view.sel().add` / `subtract` on
// "abcdefghij\nklmnop\n" in a plugin): its answers are the expected values.

use entities::selection::{add_region, merged, subtract_region};

fn added(regions: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut set = Vec::new();
    for &(a, b) in regions {
        add_region(&mut set, Selection::new(a, b));
    }
    set.iter().map(|s| (s.anchor, s.head)).collect()
}

#[test]
fn overlapping_selections_become_one_in_the_direction_of_the_newer() {
    assert_eq!(added(&[(0, 3), (2, 5)]), [(0, 5)]);
    assert_eq!(added(&[(3, 0), (2, 5)]), [(0, 5)]);
    assert_eq!(added(&[(0, 3), (5, 2)]), [(5, 0)]);
    // One that spans two takes both in.
    assert_eq!(added(&[(0, 2), (4, 6), (1, 5)]), [(0, 6)]);
}

#[test]
fn selections_that_only_touch_stay_apart() {
    assert_eq!(added(&[(0, 2), (2, 4)]), [(0, 2), (2, 4)]);
    assert_eq!(added(&[(2, 0), (4, 2)]), [(2, 0), (4, 2)]);
}

#[test]
fn a_caret_at_or_inside_a_selection_goes_into_it() {
    assert_eq!(added(&[(0, 2), (2, 2)]), [(0, 2)]);
    assert_eq!(added(&[(2, 4), (2, 2)]), [(2, 4)]);
    assert_eq!(added(&[(0, 4), (2, 2)]), [(0, 4)]);
    assert_eq!(added(&[(2, 2), (0, 2)]), [(0, 2)]);
    assert_eq!(added(&[(2, 2), (2, 2)]), [(2, 2)]);
}

#[test]
fn added_selections_are_kept_sorted() {
    assert_eq!(added(&[(8, 8), (1, 1), (4, 4)]), [(1, 1), (4, 4), (8, 8)]);
}

#[test]
fn merged_gives_the_region_holding_the_primary_and_the_others() {
    let (primary, others) = merged(
        Selection::caret(3),
        [
            Selection::new(0, 4),
            Selection::caret(8),
            Selection::caret(8),
        ],
    );
    assert_eq!(primary, Selection::new(0, 4));
    assert_eq!(others, [Selection::caret(8)]);
}

fn subtracted(regions: &[(usize, usize)], removed: (usize, usize)) -> Vec<(usize, usize)> {
    let selections: Vec<Selection> = regions.iter().map(|&(a, b)| Selection::new(a, b)).collect();
    subtract_region(&selections, Selection::new(removed.0, removed.1))
        .iter()
        .map(|s| (s.anchor, s.head))
        .collect()
}

#[test]
fn subtracting_cuts_selections_and_two_pieces_go_forward() {
    assert_eq!(subtracted(&[(0, 6)], (2, 4)), [(0, 2), (4, 6)]);
    assert_eq!(subtracted(&[(6, 0)], (2, 4)), [(0, 2), (4, 6)]);
    assert_eq!(subtracted(&[(6, 0)], (4, 8)), [(4, 0)]);
    assert_eq!(subtracted(&[(6, 2)], (0, 4)), [(6, 4)]);
    assert_eq!(subtracted(&[(0, 6)], (0, 6)), []);
    assert_eq!(subtracted(&[(0, 2)], (2, 4)), [(0, 2)]);
}

#[test]
fn subtracting_takes_the_carets_it_holds_and_a_caret_splits_a_selection() {
    assert_eq!(subtracted(&[(2, 2), (5, 5)], (2, 2)), [(5, 5)]);
    assert_eq!(subtracted(&[(1, 1), (3, 3), (8, 8)], (0, 4)), [(8, 8)]);
    assert_eq!(subtracted(&[(0, 0), (2, 2), (4, 4)], (0, 4)), []);
    assert_eq!(
        subtracted(&[(0, 6), (8, 8)], (3, 3)),
        [(0, 3), (3, 6), (8, 8)]
    );
    assert_eq!(subtracted(&[(2, 6)], (2, 2)), [(2, 6)]);
    assert_eq!(subtracted(&[(2, 6)], (6, 6)), [(2, 6)]);
}
