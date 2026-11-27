//! The answers of Sublime 4215 (a probe plugin in a portable copy: six tabs, `select_sheets`,
//! `focus_view`, then the command).

use entities::tab_selection::{TabSelection, TabSelectionCommand as C};

fn after(selected: &[usize], focused: usize, command: C) -> (Vec<usize>, usize) {
    let result = TabSelection::new(selected.iter().copied(), focused).apply(command, 6);
    (result.selected().to_vec(), result.focused())
}

#[test]
fn select_to_right_and_left_add_the_tab_next_to_the_outermost_selected_one() {
    for focused in [1, 2, 4] {
        assert_eq!(
            after(&[1, 2, 4], focused, C::SelectToRight),
            (vec![1, 2, 4, 5], 5)
        );
        assert_eq!(
            after(&[1, 2, 4], focused, C::SelectToLeft),
            (vec![0, 1, 2, 4], 0)
        );
    }
    assert_eq!(after(&[2], 2, C::SelectToRight), (vec![2, 3], 3));
    assert_eq!(after(&[2], 2, C::SelectToLeft), (vec![1, 2], 1));
    // No tab beyond them: nothing changes.
    assert_eq!(after(&[0, 5], 0, C::SelectToRight), (vec![0, 5], 0));
    assert_eq!(after(&[0, 5], 0, C::SelectToLeft), (vec![0, 5], 0));
}

#[test]
fn unselect_to_a_side_takes_the_selected_tabs_beyond_the_focused_one() {
    assert_eq!(after(&[1, 2, 4], 2, C::UnselectToLeft), (vec![2, 4], 2));
    assert_eq!(after(&[1, 2, 4], 4, C::UnselectToLeft), (vec![4], 4));
    assert_eq!(after(&[1, 2, 4], 1, C::UnselectToLeft), (vec![1, 2, 4], 1));
    assert_eq!(after(&[1, 2, 4], 2, C::UnselectToRight), (vec![1, 2], 2));
    assert_eq!(after(&[1, 2, 4], 4, C::UnselectToRight), (vec![1, 2, 4], 4));
    assert_eq!(after(&[1, 2, 4], 1, C::UnselectToRight), (vec![1], 1));
    assert_eq!(after(&[0, 5], 0, C::UnselectToRight), (vec![0], 0));
}

#[test]
fn unselect_others_keeps_the_focused_tab() {
    for focused in [1, 2, 4] {
        assert_eq!(
            after(&[1, 2, 4], focused, C::UnselectOthers),
            (vec![focused], focused)
        );
    }
}

#[test]
fn unselect_current_gives_the_focus_to_the_next_selected_tab_right_then_left() {
    assert_eq!(after(&[1, 2, 4], 2, C::UnselectCurrent), (vec![1, 4], 4));
    assert_eq!(after(&[1, 2, 4], 4, C::UnselectCurrent), (vec![1, 2], 2));
    assert_eq!(after(&[1, 2, 4], 1, C::UnselectCurrent), (vec![2, 4], 2));
    assert_eq!(after(&[0, 5], 0, C::UnselectCurrent), (vec![5], 5));
    // The only one stays.
    assert_eq!(after(&[2], 2, C::UnselectCurrent), (vec![2], 2));
}

#[test]
fn focus_moves_among_the_selected_tabs_without_wrapping() {
    assert_eq!(after(&[1, 2, 4], 2, C::FocusToLeft), (vec![1, 2, 4], 1));
    assert_eq!(after(&[1, 2, 4], 4, C::FocusToLeft), (vec![1, 2, 4], 2));
    assert_eq!(after(&[1, 2, 4], 1, C::FocusToLeft), (vec![1, 2, 4], 1));
    assert_eq!(after(&[1, 2, 4], 2, C::FocusToRight), (vec![1, 2, 4], 4));
    assert_eq!(after(&[1, 2, 4], 4, C::FocusToRight), (vec![1, 2, 4], 4));
    assert_eq!(after(&[0, 5], 0, C::FocusToRight), (vec![0, 5], 5));
}

#[test]
fn focus_by_index_counts_the_selected_tabs() {
    assert_eq!(after(&[1, 3, 4], 3, C::FocusByIndex(0)), (vec![1, 3, 4], 1));
    assert_eq!(after(&[1, 3, 4], 3, C::FocusByIndex(1)), (vec![1, 3, 4], 3));
    assert_eq!(after(&[1, 3, 4], 3, C::FocusByIndex(2)), (vec![1, 3, 4], 4));
    assert_eq!(after(&[1, 3, 4], 3, C::FocusByIndex(5)), (vec![1, 3, 4], 3));
}

#[test]
fn closing_the_focused_tab_focuses_the_next_selected_one() {
    let closed = |selected: &[usize], focused| {
        TabSelection::new(selected.iter().copied(), focused)
            .without_focused()
            .map(|s| s.focused())
    };
    assert_eq!(closed(&[1, 3, 4], 3), Some(4));
    assert_eq!(closed(&[1, 3, 4], 1), Some(3));
    assert_eq!(closed(&[1, 3, 4], 4), Some(3));
    assert_eq!(closed(&[3], 3), None);
}

#[test]
fn next_and_previous_view_collapse_to_the_neighbouring_selected_tab() {
    let selection = TabSelection::new([1, 3], 3);
    assert_eq!(selection.collapse_towards(true), 3);
    assert_eq!(selection.collapse_towards(false), 1);
    assert_eq!(TabSelection::new([1, 3], 1).collapse_towards(true), 3);
}

#[test]
fn ctrl_click_adds_or_takes_away_a_tab() {
    let selection = TabSelection::new([1, 3], 3);
    let added = selection.toggle(4);
    assert_eq!((added.selected(), added.focused()), (&[1, 3, 4][..], 4));
    let removed = selection.toggle(1);
    assert_eq!((removed.selected(), removed.focused()), (&[3][..], 3));
    let focused_removed = selection.toggle(3);
    assert_eq!(
        (focused_removed.selected(), focused_removed.focused()),
        (&[1][..], 1)
    );
    let only = TabSelection::single(2).toggle(2);
    assert_eq!((only.selected(), only.focused()), (&[2][..], 2));
}

#[test]
fn shift_click_selects_the_tabs_from_the_focused_one() {
    let right = TabSelection::new([0], 1).extend_to(4);
    assert_eq!((right.selected(), right.focused()), (&[1, 2, 3, 4][..], 4));
    let left = TabSelection::single(3).extend_to(1);
    assert_eq!((left.selected(), left.focused()), (&[1, 2, 3][..], 1));
}

#[test]
fn commands_by_name() {
    assert_eq!(C::from_name("focus_by_index", 2), Some(C::FocusByIndex(2)));
    assert_eq!(
        C::from_name("unselect_current", 0),
        Some(C::UnselectCurrent)
    );
    assert_eq!(C::from_name("close_selected", 0), None);
}
