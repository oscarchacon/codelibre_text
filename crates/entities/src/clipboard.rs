//! Copy, cut and paste with several selections, as Sublime does them: what `copy` puts on the
//! clipboard (the selections' texts, one per line, or with nothing selected and
//! `copy_with_empty_selection` the carets' whole lines) and how `paste` shares the clipboard out
//! (one line per selection when it has as many lines as there are selections, else all of it in
//! each). Selections come sorted by position, as `Document::edit_all` gives them.

use crate::selection::Selection;
use crate::text::{line_col, line_to_char};
use crate::text_commands::replace_char_ranges_each;

/// What a copy takes from the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub text: String,
    /// The carets' whole lines (nothing was selected): a paste puts them above the line.
    pub whole_lines: bool,
}

/// What `copy` / `cut` take: the selected texts joined by newlines (empty selections left
/// out); with none selected, when `with_empty_selection`, each caret's line with its newline
/// (once per line). None when there is nothing to copy.
pub fn copied(text: &str, selections: &[Selection], with_empty_selection: bool) -> Option<Copied> {
    let selected: Vec<String> = selections
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| {
            text.chars()
                .skip(s.start())
                .take(s.end() - s.start())
                .collect()
        })
        .collect();
    if !selected.is_empty() {
        return Some(Copied {
            text: selected.join("\n"),
            whole_lines: false,
        });
    }
    if !with_empty_selection || selections.is_empty() {
        return None;
    }
    let mut lines: Vec<usize> = selections
        .iter()
        .map(|s| line_col(text, s.head).0)
        .collect();
    lines.dedup();
    let text_lines: Vec<&str> = text.split('\n').collect();
    let copied = lines
        .iter()
        .map(|&line| format!("{}\n", text_lines.get(line).copied().unwrap_or("")))
        .collect();
    Some(Copied {
        text: copied,
        whole_lines: true,
    })
}

/// The whole line of each selection's caret, its newline included (or the one before it on the
/// last line), as char ranges.
fn full_line_range(text: &str, selection: Selection, chars: usize) -> (usize, usize) {
    let line = line_col(text, selection.head).0;
    let start = line_to_char(text, line);
    let next = line_to_char(text, line + 1);
    let ends_with_newline = next > start && text.chars().nth(next - 1) == Some('\n');
    match (ends_with_newline, start) {
        (true, _) => (start, next),
        (false, 0) => (0, chars),
        (false, _) => (start - 1, chars),
    }
}

/// `cut` with nothing selected: deletes each caret's line (once); each caret goes to the start of
/// the line that took its place.
pub fn delete_full_lines(text: &mut String, selections: &[Selection]) -> Vec<Selection> {
    let chars = text.chars().count();
    let ranges: Vec<(usize, usize, &str)> = selections
        .iter()
        .map(|&s| {
            let (start, end) = full_line_range(text, s, chars);
            (start, end, "")
        })
        .collect();
    let carets = replace_char_ranges_each(text, &ranges);
    carets
        .into_iter()
        .map(|caret| Selection::caret(line_to_char(text, line_col(text, caret.head).0)))
        .collect()
}

/// A paste of whole lines: `pieces[i]` goes above the i-th selection's line; the selections stay
/// where they were in the text.
pub fn insert_above_lines(
    text: &mut String,
    selections: &[Selection],
    pieces: &[&str],
) -> Vec<Selection> {
    let starts: Vec<usize> = selections
        .iter()
        .map(|s| line_to_char(text, line_col(text, s.head).0))
        .collect();
    let lengths: Vec<usize> = pieces.iter().map(|p| p.chars().count()).collect();
    let ranges: Vec<(usize, usize, &str)> = starts
        .iter()
        .zip(pieces)
        .map(|(&start, &piece)| (start, start, piece))
        .collect();
    replace_char_ranges_each(text, &ranges);
    let shifted = |point: usize| {
        point
            + starts
                .iter()
                .zip(&lengths)
                .filter(|(&start, _)| start <= point)
                .map(|(_, &length)| length)
                .sum::<usize>()
    };
    selections
        .iter()
        .map(|s| Selection::new(shifted(s.anchor), shifted(s.head)))
        .collect()
}

/// What each of `count` selections gets from `clipboard`: its lines one each when it has as many
/// as there are selections (more than one), else all of it.
pub fn paste_pieces(clipboard: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = clipboard
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if count > 1 && lines.len() == count {
        lines
    } else {
        vec![clipboard; count]
    }
}

/// [`paste_pieces`] for whole lines (each piece ends with its newline).
pub fn line_pieces(clipboard: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = clipboard.split_inclusive('\n').collect();
    if count > 1 && lines.len() == count {
        lines
    } else {
        vec![clipboard; count]
    }
}
