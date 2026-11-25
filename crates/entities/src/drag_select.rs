//! The selections a mouse drag makes (Sublime's `drag_select`): by characters, words or lines
//! between where the button went down and where the pointer is, or a column of them over the
//! lines in between (`by: "columns"`). Whether they replace, add to or are taken out of the
//! current selections is the use case's (`additive` / `subtractive`, `selection::add_region` /
//! `subtract_region`).

use crate::selection::Selection;
use crate::text::{char_to_byte, line_col, line_to_char, word_at};
use crate::view_geometry::ViewGeometry;

/// What a drag selects: `by` of Sublime's `drag_select` (`columns` is [`column_selections`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragUnit {
    /// A click: from the press to the pointer.
    #[default]
    Characters,
    /// A double click: whole words, from the press's to the pointer's.
    Words,
    /// A triple click: whole lines, their newlines included.
    Lines,
}

/// The selection a drag from `anchor` to `head` (char indices) makes. Its anchor stays at the
/// side where the press was, so a drag upwards selects backwards.
pub fn drag_selection(text: &str, anchor: usize, head: usize, unit: DragUnit) -> Selection {
    let (from, to) = match unit {
        DragUnit::Characters => return Selection::new(anchor, head),
        DragUnit::Words => (word_or_point(text, anchor), word_or_point(text, head)),
        DragUnit::Lines => (line_of(text, anchor), line_of(text, head)),
    };
    let (start, end) = (from.0.min(to.0), from.1.max(to.1));
    if head < anchor {
        Selection::new(end, start)
    } else {
        Selection::new(start, end)
    }
}

/// The word at `point`, or the point itself where there is none.
fn word_or_point(text: &str, point: usize) -> (usize, usize) {
    let word = word_at(text, point);
    (word.start(), word.end())
}

/// The whole line of `point`, its newline included.
fn line_of(text: &str, point: usize) -> (usize, usize) {
    let (line, _) = line_col(text, point);
    (line_to_char(text, line), line_to_char(text, line + 1))
}

/// The rows a view draws, as char ranges without their line breaks: one per line, more when the
/// view wraps lines ([`ViewGeometry::rows`]). Columns and Up / Down go by them, as in Sublime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    rows: Vec<(usize, usize)>,
}

impl Rows {
    /// A row per line (a view that does not wrap them).
    pub fn lines(text: &str) -> Self {
        Self::of(text, &ViewGeometry::default())
    }

    /// The rows `geometry` draws `text` in.
    pub fn of(text: &str, geometry: &ViewGeometry) -> Self {
        let mut rows = geometry.rows(text);
        if rows.is_empty() {
            rows.push((0, 0));
        }
        Self { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row of `point`: where a wrapped row ends and the next begins is the next one's.
    pub fn row_of(&self, point: usize) -> usize {
        self.rows
            .iter()
            .rposition(|&(start, _)| start <= point)
            .unwrap_or(0)
    }

    /// Whether `row` goes on in the next row (the view wrapped its line there).
    fn wraps(&self, row: usize) -> bool {
        self.rows
            .get(row + 1)
            .is_some_and(|&(next, _)| next == self.rows[row].1)
    }

    /// Where a caret at the end of `row` stays on it: the end of its line, or for a wrapped row
    /// its last char (its end is where the next row begins).
    fn caret_end(&self, row: usize) -> usize {
        let (start, end) = self.rows[row];
        if self.wraps(row) && end > start {
            end - 1
        } else {
            end
        }
    }
}

/// A place in the view for a column drag or Up / Down: a row and a column on screen counted from
/// the row's start, which may be past the row's end (the pointer is in the empty space after it).
/// Tabs count up to the next multiple of the tab size from the line's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub row: usize,
    pub column: usize,
}

/// Where `point` (a char index) is on screen, `past_end` columns further right (the pointer past
/// the end of its row).
pub fn screen_point(
    text: &str,
    rows: &Rows,
    point: usize,
    past_end: usize,
    tab_size: usize,
) -> ScreenPoint {
    let row = rows.row_of(point);
    ScreenPoint {
        row,
        column: columns_in_row(text, rows, row, point, tab_size.max(1)) + past_end,
    }
}

/// [`screen_point`] of the pointer after the end of a wrapped row, `past_end` columns beyond it:
/// `point` is the row's end, which [`Rows::row_of`] gives the next row.
pub fn screen_point_after_row(
    text: &str,
    rows: &Rows,
    point: usize,
    past_end: usize,
    tab_size: usize,
) -> ScreenPoint {
    let row = rows.row_of(point.saturating_sub(1));
    let end = rows.rows[row].1;
    ScreenPoint {
        row,
        column: columns_in_row(text, rows, row, end, tab_size.max(1)) + past_end,
    }
}

/// The char of row `row` at screen column `column`: where a caret stays at the row's end past
/// it, the nearer side inside a tab.
pub fn point_at_column(
    text: &str,
    rows: &Rows,
    row: usize,
    column: usize,
    tab_size: usize,
) -> usize {
    let row = row.min(rows.len() - 1);
    point_in_row(text, rows, row, column, tab_size.max(1))
}

/// A column drag (`drag_select {by: "columns"}`) from `anchor` to `head`: one selection per row
/// between them, from the anchor's column to the head's, ordered from the anchor's row to the
/// head's (the last is where the pointer is, the primary one). Rows that end before the column's
/// left side are left out, unless all of them do: then each gets a caret at its end. Columns
/// past a row's end stop at it; a column inside a tab goes to its nearer side.
pub fn column_selections(
    text: &str,
    rows: &Rows,
    anchor: ScreenPoint,
    head: ScreenPoint,
    tab_size: usize,
) -> Vec<Selection> {
    let tab_size = tab_size.max(1);
    let last = rows.len() - 1;
    let (from, to) = (anchor.row.min(last), head.row.min(last));
    let span: Vec<usize> = if from <= to {
        (from..=to).collect()
    } else {
        (to..=from).rev().collect()
    };
    let left = anchor.column.min(head.column);
    let reaches = |row: usize| columns_in_row(text, rows, row, rows.rows[row].1, tab_size) >= left;
    if !span.iter().any(|&row| reaches(row)) {
        return span
            .iter()
            .map(|&row| Selection::caret(rows.caret_end(row)))
            .collect();
    }
    span.iter()
        .filter(|&&row| reaches(row))
        .map(|&row| {
            Selection::new(
                point_in_row(text, rows, row, anchor.column, tab_size),
                point_in_row(text, rows, row, head.column, tab_size),
            )
        })
        .collect()
}

/// The chars of `text` from the start of the line of `row` to the end of `row`, with the screen
/// column each starts at (tabs to the next stop from the line's start), from the row's start.
fn row_chars(text: &str, rows: &Rows, row: usize, tab_size: usize) -> Vec<(usize, char, usize)> {
    let (start, end) = rows.rows[row];
    let line_start = line_to_char(text, line_col(text, start).0);
    let mut column = 0;
    let mut row_column = 0;
    let mut chars = Vec::new();
    let from = char_to_byte(text, line_start);
    for (index, c) in (line_start..).zip(text[from..].chars().take(end - line_start)) {
        if index == start {
            row_column = column;
        }
        if index >= start {
            chars.push((index, c, column));
        }
        column = next_column(column, c, tab_size);
    }
    if start == end {
        row_column = column;
    }
    chars.push((end, '\n', column));
    chars
        .into_iter()
        .map(|(index, c, column)| (index, c, column - row_column))
        .collect()
}

/// The screen column of `point` (in `row`, or its end) from the row's start.
fn columns_in_row(text: &str, rows: &Rows, row: usize, point: usize, tab_size: usize) -> usize {
    row_chars(text, rows, row, tab_size)
        .iter()
        .find(|&&(index, _, _)| index >= point)
        .map_or(0, |&(_, _, column)| column)
}

/// The char of `row` at screen column `column` from its start: where a caret stays at its end
/// past it, the nearer side inside a tab.
fn point_in_row(text: &str, rows: &Rows, row: usize, column: usize, tab_size: usize) -> usize {
    let chars = row_chars(text, rows, row, tab_size);
    for pair in chars.windows(2) {
        let ((index, _, at), (_, _, next)) = (pair[0], pair[1]);
        if column < next {
            let point = if column - at <= next - column {
                index
            } else {
                index + 1
            };
            return point.min(rows.caret_end(row));
        }
    }
    rows.caret_end(row)
}

fn next_column(column: usize, c: char, tab_size: usize) -> usize {
    if c == '\t' {
        column + tab_size - column % tab_size
    } else {
        column + 1
    }
}
