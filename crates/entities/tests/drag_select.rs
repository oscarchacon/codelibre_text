//! Tests for `entities::drag_select`.

use entities::drag_select::{
    column_selections, drag_selection, point_at_column, screen_point, screen_point_after_row,
    DragUnit, Rows, ScreenPoint,
};
use entities::selection::Selection;
use entities::view_geometry::ViewGeometry;

fn at(row: usize, column: usize) -> ScreenPoint {
    ScreenPoint { row, column }
}

fn pairs(selections: &[Selection]) -> Vec<(usize, usize)> {
    selections.iter().map(|s| (s.anchor, s.head)).collect()
}

#[test]
fn a_drag_by_characters_goes_from_the_press_to_the_pointer() {
    let text = "one two three";
    assert_eq!(
        drag_selection(text, 2, 9, DragUnit::Characters),
        Selection::new(2, 9)
    );
    assert_eq!(
        drag_selection(text, 9, 2, DragUnit::Characters),
        Selection::new(9, 2)
    );
}

#[test]
fn a_drag_by_words_takes_whole_words_and_keeps_the_press_s_side() {
    let text = "one two three";
    assert_eq!(
        drag_selection(text, 5, 5, DragUnit::Words),
        Selection::new(4, 7)
    );
    assert_eq!(
        drag_selection(text, 5, 10, DragUnit::Words),
        Selection::new(4, 13)
    );
    // Upwards: the anchor stays at the end of the word pressed.
    assert_eq!(
        drag_selection(text, 10, 1, DragUnit::Words),
        Selection::new(13, 0)
    );
    // Between words there is no word: the point itself.
    assert_eq!(
        drag_selection("a  b", 2, 2, DragUnit::Words),
        Selection::caret(2)
    );
}

#[test]
fn a_drag_by_lines_takes_whole_lines_with_their_newlines() {
    let text = "one\ntwo\nthree";
    assert_eq!(
        drag_selection(text, 5, 5, DragUnit::Lines),
        Selection::new(4, 8)
    );
    assert_eq!(
        drag_selection(text, 1, 10, DragUnit::Lines),
        Selection::new(0, 13)
    );
    assert_eq!(
        drag_selection(text, 10, 1, DragUnit::Lines),
        Selection::new(13, 0)
    );
}

#[test]
fn a_column_drag_selects_the_same_columns_on_every_line() {
    let text = "abcdef\nghijkl\nmnopqr";
    // Lines start at 0, 7 and 14.
    let down = column_selections(text, &Rows::lines(text), at(0, 1), at(2, 4), 4);
    assert_eq!(pairs(&down), [(1, 4), (8, 11), (15, 18)]);
    // Upwards and leftwards: from the anchor's line to the pointer's, each selection backwards.
    let up = column_selections(text, &Rows::lines(text), at(2, 4), at(0, 1), 4);
    assert_eq!(pairs(&up), [(18, 15), (11, 8), (4, 1)]);
}

#[test]
fn a_zero_width_column_drag_puts_a_caret_on_every_line() {
    let text = "abc\ndef\nghi";
    assert_eq!(
        pairs(&column_selections(
            text,
            &Rows::lines(text),
            at(0, 2),
            at(2, 2),
            4
        )),
        [(2, 2), (6, 6), (10, 10)]
    );
}

#[test]
fn lines_that_end_before_the_column_are_left_out_and_long_ones_stop_at_their_end() {
    let text = "abcdef\n\nab\nabcd";
    // Lines start at 0, 7, 8 and 11; the empty one and "ab" end before column 3.
    let selections = column_selections(text, &Rows::lines(text), at(0, 3), at(3, 6), 4);
    assert_eq!(pairs(&selections), [(3, 6), (14, 15)]);
}

#[test]
fn a_column_past_the_end_of_every_line_puts_carets_at_their_ends() {
    let text = "ab\nabc\n";
    assert_eq!(
        pairs(&column_selections(
            text,
            &Rows::lines(text),
            at(0, 8),
            at(2, 9),
            4
        )),
        [(2, 2), (6, 6), (7, 7)]
    );
}

#[test]
fn tabs_count_up_to_the_next_tab_stop() {
    // "\tx": the tab takes columns 0..4, x is at column 4.
    let text = "\tx\nabcdx";
    assert_eq!(
        pairs(&column_selections(
            text,
            &Rows::lines(text),
            at(0, 4),
            at(1, 5),
            4
        )),
        [(1, 2), (7, 8)]
    );
    // Column 1 is nearer the tab's start, column 3 its end.
    assert_eq!(
        pairs(&column_selections(
            text,
            &Rows::lines(text),
            at(0, 1),
            at(0, 3),
            4
        )),
        [(0, 1)]
    );
}

#[test]
fn lines_past_the_end_of_the_text_are_its_last_line() {
    let text = "abc\nde";
    assert_eq!(
        pairs(&column_selections(
            text,
            &Rows::lines(text),
            at(0, 1),
            at(9, 2),
            4
        )),
        [(1, 2), (5, 6)]
    );
}

#[test]
fn a_screen_point_counts_tabs_to_the_next_stop_and_the_columns_past_the_end() {
    let text = "ab\n\tcd";
    assert_eq!(screen_point(text, &Rows::lines(text), 1, 0, 4), at(0, 1));
    assert_eq!(screen_point(text, &Rows::lines(text), 2, 3, 4), at(0, 5));
    assert_eq!(screen_point(text, &Rows::lines(text), 4, 0, 4), at(1, 4));
    assert_eq!(screen_point(text, &Rows::lines(text), 5, 0, 4), at(1, 5));
}

/// Rows of `columns` columns, as the view wraps lines (after the last space that fits).
fn wrapped(text: &str, columns: usize) -> Rows {
    Rows::of(
        text,
        &ViewGeometry {
            wrap_columns: Some(columns),
            ..ViewGeometry::default()
        },
    )
}

#[test]
fn wrapped_lines_have_a_row_each_and_columns_count_from_the_rows_start() {
    // Rows: "aaaa bbbb " (0..10), "cccc dddd" (10..19), then "xy" (20..22).
    let text = "aaaa bbbb cccc dddd\nxy";
    let rows = wrapped(text, 10);
    assert_eq!(rows.len(), 3);
    assert_eq!(screen_point(text, &rows, 7, 0, 4), at(0, 7));
    // Where the first row ends the second begins: its column 0.
    assert_eq!(screen_point(text, &rows, 10, 0, 4), at(1, 0));
    assert_eq!(screen_point(text, &rows, 12, 0, 4), at(1, 2));
    assert_eq!(screen_point(text, &rows, 21, 0, 4), at(2, 1));
    // Past the end of the first row (the pointer after its space).
    assert_eq!(screen_point_after_row(text, &rows, 10, 3, 4), at(0, 13));
    // A caret stays on a wrapped row: before the space it broke after, past its end.
    assert_eq!(point_at_column(text, &rows, 0, 30, 4), 9);
    assert_eq!(point_at_column(text, &rows, 1, 30, 4), 19);
    assert_eq!(point_at_column(text, &rows, 1, 2, 4), 12);
}

#[test]
fn a_column_drag_over_wrapped_lines_selects_on_every_row() {
    let text = "aaaa bbbb cccc dddd\nxy";
    let rows = wrapped(text, 10);
    assert_eq!(
        pairs(&column_selections(text, &rows, at(0, 1), at(2, 3), 4)),
        [(1, 3), (11, 13), (21, 22)]
    );
    // Rows that end before the column are left out; a column past them stays on each row.
    assert_eq!(
        pairs(&column_selections(text, &rows, at(0, 5), at(2, 12), 4)),
        [(5, 9), (15, 19)]
    );
    assert_eq!(
        pairs(&column_selections(text, &rows, at(0, 12), at(1, 12), 4)),
        [(9, 9), (19, 19)]
    );
}

#[test]
fn tabs_in_a_wrapped_line_count_from_the_line_s_start() {
    // Rows: "\tab cd " (0..7: the tab takes 4 columns, 10 in all), "ef" (7..9).
    let text = "\tab cd ef";
    let rows = wrapped(text, 10);
    assert_eq!(rows.len(), 2);
    assert_eq!(screen_point(text, &rows, 2, 0, 4), at(0, 5));
    assert_eq!(screen_point(text, &rows, 8, 0, 4), at(1, 1));
    assert_eq!(point_at_column(text, &rows, 1, 1, 4), 8);
}
