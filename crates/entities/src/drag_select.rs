//! The selections a mouse drag makes (Sublime's `drag_select`): by characters, words or lines
//! between where the button went down and where the pointer is, or a column of them over the
//! lines in between (`by: "columns"`). Whether they replace, add to or are taken out of the
//! current selections is the use case's (`additive` / `subtractive`, `selection::add_region` /
//! `subtract_region`).

use crate::selection::Selection;
use crate::text::{line_col, line_to_char, word_at};

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

/// A place in the view for a column drag: a line and a column on screen, which may be past the
/// line's end (the pointer is in the empty space after it). Tabs count up to the next multiple
/// of the tab size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub line: usize,
    pub column: usize,
}

/// Where `point` (a char index) is on screen, `past_end` columns further right (the pointer past
/// the end of its line). Tabs count up to the next multiple of the tab size.
pub fn screen_point(text: &str, point: usize, past_end: usize, tab_size: usize) -> ScreenPoint {
    let (line, column) = line_col(text, point);
    let first = line_to_char(text, line);
    let before: String = text.chars().skip(first).take(column).collect();
    ScreenPoint {
        line,
        column: visual_width(&before, tab_size.max(1)) + past_end,
    }
}

/// The char of line `line` at screen column `column`: the line's end past it, the nearer side
/// inside a tab.
pub fn point_at_column(text: &str, line: usize, column: usize, tab_size: usize) -> usize {
    let row = text.split('\n').nth(line).unwrap_or("");
    line_to_char(text, line) + char_at_column(row, column, tab_size.max(1))
}

/// A column drag (`drag_select {by: "columns"}`) from `anchor` to `head`: one selection per line
/// between them, from the anchor's column to the head's, ordered from the anchor's line to the
/// head's (the last is where the pointer is, the primary one). Lines that end before the
/// column's left side are left out, unless all of them do: then each gets a caret at its end.
/// Columns past a line's end stop at it; a column inside a tab goes to its nearer side.
pub fn column_selections(
    text: &str,
    anchor: ScreenPoint,
    head: ScreenPoint,
    tab_size: usize,
) -> Vec<Selection> {
    let tab_size = tab_size.max(1);
    let last = text.split('\n').count() - 1;
    let lines: Vec<usize> = if anchor.line <= head.line {
        (anchor.line.min(last)..=head.line.min(last)).collect()
    } else {
        (head.line.min(last)..=anchor.line.min(last))
            .rev()
            .collect()
    };
    let rows: Vec<(usize, &str)> = lines
        .iter()
        .map(|&line| {
            let first = line_to_char(text, line);
            (first, text.split('\n').nth(line).unwrap_or(""))
        })
        .collect();
    let left = anchor.column.min(head.column);
    let reaches = |row: &str| visual_width(row, tab_size) >= left;
    if !rows.iter().any(|(_, row)| reaches(row)) {
        return rows
            .iter()
            .map(|(first, row)| Selection::caret(first + row.chars().count()))
            .collect();
    }
    rows.iter()
        .filter(|(_, row)| reaches(row))
        .map(|(first, row)| {
            Selection::new(
                first + char_at_column(row, anchor.column, tab_size),
                first + char_at_column(row, head.column, tab_size),
            )
        })
        .collect()
}

/// How many columns `line` takes on screen.
fn visual_width(line: &str, tab_size: usize) -> usize {
    line.chars()
        .fold(0, |column, c| next_column(column, c, tab_size))
}

fn next_column(column: usize, c: char, tab_size: usize) -> usize {
    if c == '\t' {
        column + tab_size - column % tab_size
    } else {
        column + 1
    }
}

/// The char of `line` at screen column `column`: its end past it, the nearer side inside a tab.
fn char_at_column(line: &str, column: usize, tab_size: usize) -> usize {
    let mut at = 0;
    for (index, c) in line.chars().enumerate() {
        let next = next_column(at, c, tab_size);
        if column < next {
            return if column - at <= next - column {
                index
            } else {
                index + 1
            };
        }
        at = next;
    }
    line.chars().count()
}
