//! Text commands (Sublime's `duplicate_line`, `swap_line_up`, `toggle_comment`, …) as pure
//! functions: they take the text and the current selection, change the text, and return the
//! selection that should follow.

use std::collections::HashSet;

use crate::selection::Selection;
use crate::text::{byte_to_char, char_to_byte, is_word_char, line_end, line_start, selected_text, word_at};

/// Byte range of the whole lines touched by the selection (without the last newline).
/// A selection that ends at column 0 does not include that last line, as in Sublime.
fn selected_lines(text: &str, selection: Selection) -> (usize, usize) {
    let start = char_to_byte(text, selection.start());
    let mut end = char_to_byte(text, selection.end());
    if end > start && text[..end].ends_with('\n') {
        end -= 1;
    }
    (line_start(text, start), line_end(text, end))
}

/// Start byte of every line between `start` and `end` (as returned by `selected_lines`).
fn line_starts(text: &str, start: usize, end: usize) -> Vec<usize> {
    let mut starts = vec![start];
    let mut pos = start;
    loop {
        let le = line_end(text, pos);
        if le >= end {
            return starts;
        }
        pos = le + 1;
        starts.push(pos);
    }
}

fn leading_whitespace(line: &str) -> &str {
    let n = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..n]
}

fn indent_unit(tab_size: usize, use_spaces: bool) -> String {
    if use_spaces {
        " ".repeat(tab_size)
    } else {
        "\t".into()
    }
}

fn visual_column(line_prefix: &str, tab_size: usize) -> usize {
    line_prefix.chars().fold(0, |col, c| {
        if c == '\t' {
            col + tab_size - col % tab_size
        } else {
            col + 1
        }
    })
}

/// One replacement of the byte range `start..end` with `insert`.
struct Edit {
    start: usize,
    end: usize,
    insert: String,
}

/// Applies non-overlapping edits and moves the selection along with the text.
fn apply_edits(text: &mut String, mut edits: Vec<Edit>, selection: Selection) -> Selection {
    edits.sort_by_key(|e| e.start);
    let map = |text: &str, char_pos: usize, edits: &[Edit]| -> usize {
        let pos = char_to_byte(text, char_pos);
        let mut delta: isize = 0;
        for e in edits {
            if e.end <= pos {
                delta += e.insert.len() as isize - (e.end - e.start) as isize;
            } else if e.start < pos {
                return (e.start as isize + delta) as usize + e.insert.len();
            } else {
                break;
            }
        }
        (pos as isize + delta) as usize
    };
    let anchor = map(text, selection.anchor, &edits);
    let head = map(text, selection.head, &edits);
    for e in edits.iter().rev() {
        text.replace_range(e.start..e.end, &e.insert);
    }
    Selection::new(byte_to_char(text, anchor), byte_to_char(text, head))
}

/// Replaces the selection with `insert`; the caret ends after the inserted text.
pub fn replace_selection(text: &mut String, selection: Selection, insert: &str) -> Selection {
    let start = char_to_byte(text, selection.start());
    let end = char_to_byte(text, selection.end());
    text.replace_range(start..end, insert);
    Selection::caret(selection.start() + insert.chars().count())
}

/// Tab key: indents a multi-line selection, otherwise inserts a tab (or spaces up to the next tab stop).
pub fn insert_tab(text: &mut String, selection: Selection, tab_size: usize, use_spaces: bool) -> Selection {
    let (s, e) = selected_lines(text, selection);
    if text[s..e].contains('\n') {
        return indent(text, selection, tab_size, use_spaces);
    }
    let insert = if use_spaces {
        let start = char_to_byte(text, selection.start());
        let column = visual_column(&text[line_start(text, start)..start], tab_size);
        " ".repeat(tab_size - column % tab_size)
    } else {
        "\t".to_string()
    };
    replace_selection(text, selection, &insert)
}

/// Enter with `auto_indent`: keeps the indentation, adds a level after an opening bracket or a
/// trailing `:` (Python, YAML) and moves a matching closing bracket to its own line.
pub fn newline_auto_indent(text: &mut String, selection: Selection, tab_size: usize, use_spaces: bool) -> Selection {
    let start = char_to_byte(text, selection.start());
    let end = char_to_byte(text, selection.end());
    let ls = line_start(text, start);
    let indent = leading_whitespace(&text[ls..start]).to_string();
    let opener = text[ls..start].trim_end().chars().last();
    let closer = text[end..line_end(text, end)].trim_start().chars().next();
    let opens = matches!(opener, Some('{' | '[' | '(' | ':'));
    let pairs = matches!(
        (opener, closer),
        (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')'))
    );

    if opens {
        let inner = format!("{indent}{}", indent_unit(tab_size, use_spaces));
        let insert = if pairs {
            format!("\n{inner}\n{indent}")
        } else {
            format!("\n{inner}")
        };
        replace_selection(text, selection, &insert);
        Selection::caret(selection.start() + 1 + inner.chars().count())
    } else {
        replace_selection(text, selection, &format!("\n{indent}"))
    }
}

pub fn indent(text: &mut String, selection: Selection, tab_size: usize, use_spaces: bool) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let unit = indent_unit(tab_size, use_spaces);
    let single_line = s == e;
    let edits = line_starts(text, s, e)
        .into_iter()
        .filter(|&pos| single_line || line_end(text, pos) > pos)
        .map(|pos| Edit { start: pos, end: pos, insert: unit.clone() })
        .collect();
    apply_edits(text, edits, selection)
}

pub fn unindent(text: &mut String, selection: Selection, tab_size: usize) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let edits = line_starts(text, s, e)
        .into_iter()
        .filter_map(|pos| {
            let line = &text[pos..line_end(text, pos)];
            let remove = if line.starts_with('\t') {
                1
            } else {
                line.chars().take(tab_size).take_while(|c| *c == ' ').count()
            };
            (remove > 0).then(|| Edit { start: pos, end: pos + remove, insert: String::new() })
        })
        .collect();
    apply_edits(text, edits, selection)
}

/// Duplicates the selected lines (or the selected text, if any) below.
pub fn duplicate_line(text: &mut String, selection: Selection) -> Selection {
    if !selection.is_empty() {
        let copy = selected_text(text, selection).to_string();
        let at = char_to_byte(text, selection.end());
        text.insert_str(at, &copy);
        return selection;
    }
    let (s, e) = selected_lines(text, selection);
    let block = format!("{}\n", &text[s..e]);
    let shift = block.chars().count();
    text.insert_str(s, &block);
    Selection::new(selection.anchor + shift, selection.head + shift)
}

/// Moves the selected lines one line up or down. Returns `None` at the top/bottom.
pub fn swap_line(text: &mut String, selection: Selection, up: bool) -> Option<Selection> {
    let (s, e) = selected_lines(text, selection);
    let block = text[s..e].to_string();
    if up {
        if s == 0 {
            return None;
        }
        let ps = line_start(text, s - 1);
        let previous = text[ps..s - 1].to_string();
        text.replace_range(ps..e, &format!("{block}\n{previous}"));
        let shift = previous.chars().count() + 1;
        Some(Selection::new(selection.anchor - shift, selection.head - shift))
    } else {
        if e >= text.len() {
            return None;
        }
        let ne = line_end(text, e + 1);
        let next = text[e + 1..ne].to_string();
        text.replace_range(s..ne, &format!("{next}\n{block}"));
        let shift = next.chars().count() + 1;
        Some(Selection::new(selection.anchor + shift, selection.head + shift))
    }
}

pub fn delete_line(text: &mut String, selection: Selection) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let (from, to) = if e < text.len() {
        (s, e + 1)
    } else if s > 0 {
        (s - 1, e)
    } else {
        (s, e)
    };
    text.replace_range(from..to, "");
    let caret = line_start(text, from.min(text.len()));
    Selection::caret(byte_to_char(text, caret))
}

/// Opens a new line after (or before) the current one with the same indentation.
pub fn add_line(text: &mut String, selection: Selection, before: bool) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let indent = leading_whitespace(&text[s..e]).to_string();
    if before {
        text.insert_str(s, &format!("{indent}\n"));
        Selection::caret(byte_to_char(text, s + indent.len()))
    } else {
        text.insert_str(e, &format!("\n{indent}"));
        Selection::caret(byte_to_char(text, e + 1 + indent.len()))
    }
}

/// Deletes from the caret to the end (or start) of the line.
pub fn delete_to_line_edge(text: &mut String, selection: Selection, to_end: bool) -> Selection {
    if !selection.is_empty() {
        return replace_selection(text, selection, "");
    }
    let caret = char_to_byte(text, selection.head);
    let (from, to) = if to_end {
        let le = line_end(text, caret);
        // At the end of a line, delete the newline (joins with the next line).
        (caret, if le == caret { (le + 1).min(text.len()) } else { le })
    } else {
        (line_start(text, caret), caret)
    };
    text.replace_range(from..to, "");
    Selection::caret(byte_to_char(text, from))
}

/// Selects whole lines; repeating extends the selection by one more line.
pub fn expand_selection_to_line(text: &str, selection: Selection) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let full_end = if e < text.len() { e + 1 } else { e };
    let (start, end) = (byte_to_char(text, s), byte_to_char(text, full_end));
    if selection.start() == start && selection.end() == end && full_end < text.len() {
        let next_end = line_end(text, full_end);
        let next_end = if next_end < text.len() { next_end + 1 } else { next_end };
        return Selection::new(start, byte_to_char(text, next_end));
    }
    Selection::new(start, end)
}

/// Joins the selected lines (or the current line with the next one) with single spaces.
pub fn join_lines(text: &mut String, selection: Selection) -> Selection {
    let (s, mut e) = selected_lines(text, selection);
    if !text[s..e].contains('\n') {
        if e >= text.len() {
            return selection;
        }
        e = line_end(text, e + 1);
    }
    let mut edits = Vec::new();
    for (i, _) in text[s..e].match_indices('\n') {
        let newline = s + i;
        let next = &text[newline + 1..];
        let next_ws = next.len() - next.trim_start_matches([' ', '\t']).len();
        let trailing_ws = text[..newline].len() - text[..newline].trim_end_matches([' ', '\t']).len();
        let next_is_empty = next[next_ws..].starts_with('\n') || next_ws == next.len();
        edits.push(Edit {
            start: newline - trailing_ws,
            end: newline + 1 + next_ws,
            insert: if next_is_empty { String::new() } else { " ".into() },
        });
    }
    apply_edits(text, edits, selection)
}

pub enum Case {
    Upper,
    Lower,
    Title,
    Swap,
}

/// Changes the case of the selection (or of the word under the caret).
pub fn change_case(text: &mut String, selection: Selection, case: Case) -> Selection {
    let selection = if selection.is_empty() {
        word_at(text, selection.head)
    } else {
        selection
    };
    let source = selected_text(text, selection).to_string();
    let changed: String = match case {
        Case::Upper => source.to_uppercase(),
        Case::Lower => source.to_lowercase(),
        Case::Swap => source
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<String>()
                } else {
                    c.to_uppercase().collect()
                }
            })
            .collect(),
        Case::Title => {
            let mut inside_word = false;
            source
                .chars()
                .map(|c| {
                    let s: String = if inside_word {
                        c.to_lowercase().collect()
                    } else {
                        c.to_uppercase().collect()
                    };
                    inside_word = is_word_char(c);
                    s
                })
                .collect()
        }
    };
    let start = char_to_byte(text, selection.start());
    let end = char_to_byte(text, selection.end());
    text.replace_range(start..end, &changed);
    Selection::new(selection.start(), selection.start() + changed.chars().count())
}

/// Swaps the characters around the caret.
pub fn transpose(text: &mut String, selection: Selection) -> Option<Selection> {
    if !selection.is_empty() || selection.head == 0 {
        return None;
    }
    let mut chars: Vec<char> = text.chars().collect();
    let at = selection.head.min(chars.len() - 1);
    if at == 0 {
        return None;
    }
    chars.swap(at - 1, at);
    *text = chars.into_iter().collect();
    Some(Selection::caret(at + 1))
}

/// Line and block comment markers for a syntax name.
/// (Sublime reads them from `Comments.tmPreferences`; these are the common ones.)
pub fn comment_tokens(syntax: &str) -> (Option<&'static str>, Option<(&'static str, &'static str)>) {
    match syntax.to_lowercase().as_str() {
        "python" | "ruby" | "perl" | "yaml" | "toml" | "makefile" | "r" | "dockerfile" | "powershell"
        | "coffeescript" | "nim" | "elixir" | "julia" | "cmake" | "nginx" | "git ignore" | "git config"
        | "bourne again shell (bash)" | "shell-unix-generic" | "shell script" => (Some("# "), None),
        "sql" | "lua" | "haskell" | "ada" | "elm" => (Some("-- "), None),
        "lisp" | "clojure" | "scheme" | "ini" | "assembly" | "asm" => (Some("; "), None),
        "latex" | "tex" | "erlang" | "matlab" | "bibtex" => (Some("% "), None),
        "batch file" => (Some("REM "), None),
        "vimscript" | "viml" => (Some("\" "), None),
        "html" | "xml" | "markdown" | "multimarkdown" | "vue component" | "svg" => (None, Some(("<!--", "-->"))),
        "css" => (None, Some(("/*", "*/"))),
        "plain text" => (None, None),
        _ => (Some("// "), Some(("/*", "*/"))),
    }
}

/// Comments or uncomments the selected lines (`block = false`) or the selection (`block = true`).
pub fn toggle_comment(text: &mut String, selection: Selection, syntax: &str, block: bool) -> Option<Selection> {
    match (block, comment_tokens(syntax)) {
        (false, (Some(token), _)) => Some(toggle_line_comment(text, selection, token)),
        (_, (_, Some((open, close)))) => Some(toggle_block_comment(text, selection, open, close)),
        (true, (Some(token), None)) => Some(toggle_line_comment(text, selection, token)),
        _ => None,
    }
}

fn toggle_line_comment(text: &mut String, selection: Selection, token: &str) -> Selection {
    let (s, e) = selected_lines(text, selection);
    let bare = token.trim_end();
    let lines: Vec<(usize, &str)> = line_starts(text, s, e)
        .into_iter()
        .map(|pos| (pos, &text[pos..line_end(text, pos)]))
        .collect();
    let non_empty: Vec<&(usize, &str)> = lines.iter().filter(|(_, l)| !l.trim().is_empty()).collect();
    let all_commented = !non_empty.is_empty() && non_empty.iter().all(|(_, l)| l.trim_start().starts_with(bare));

    let mut edits = Vec::new();
    if all_commented {
        for (pos, line) in &non_empty {
            let ws = leading_whitespace(line).len();
            let len = if line[ws..].starts_with(token) { token.len() } else { bare.len() };
            edits.push(Edit { start: pos + ws, end: pos + ws + len, insert: String::new() });
        }
    } else {
        // Comment markers go at the smallest indentation, so the block stays aligned.
        let min_indent = non_empty.iter().map(|(_, l)| leading_whitespace(l).len()).min().unwrap_or(0);
        for (pos, line) in &lines {
            if line.trim().is_empty() && !non_empty.is_empty() {
                continue;
            }
            let at = min_indent.min(line.len());
            edits.push(Edit { start: pos + at, end: pos + at, insert: token.to_string() });
        }
    }
    apply_edits(text, edits, selection)
}

fn toggle_block_comment(text: &mut String, selection: Selection, open: &str, close: &str) -> Selection {
    let (s, e) = if selection.is_empty() {
        let (s, e) = selected_lines(text, selection);
        (s + leading_whitespace(&text[s..e]).len(), e)
    } else {
        (char_to_byte(text, selection.start()), char_to_byte(text, selection.end()))
    };
    let region = &text[s..e];
    let trimmed = region.trim();
    let edits = if trimmed.starts_with(open) && trimmed.ends_with(close) && trimmed.len() >= open.len() + close.len() {
        let open_start = s + (region.len() - region.trim_start().len());
        let close_end = e - (region.len() - region.trim_end().len());
        let mut open_end = open_start + open.len();
        if text[open_end..].starts_with(' ') {
            open_end += 1;
        }
        let mut close_start = close_end - close.len();
        if close_start > open_end && text[..close_start].ends_with(' ') {
            close_start -= 1;
        }
        vec![
            Edit { start: open_start, end: open_end, insert: String::new() },
            Edit { start: close_start, end: close_end, insert: String::new() },
        ]
    } else {
        vec![
            Edit { start: s, end: s, insert: format!("{open} ") },
            Edit { start: e, end: e, insert: format!(" {close}") },
        ]
    };
    apply_edits(text, edits, selection)
}

pub enum LineOrder {
    Sort { case_sensitive: bool },
    Reverse,
    Unique,
}

/// Sorts / reverses / de-duplicates the selected lines, or the whole text if nothing is selected.
pub fn reorder_lines(text: &mut String, selection: Selection, order: LineOrder) -> Selection {
    let (s, e) = if selection.is_empty() {
        (0, text.len())
    } else {
        selected_lines(text, selection)
    };
    let region = &text[s..e];
    let trailing_newline = region.ends_with('\n');
    let mut lines: Vec<&str> = region.lines().collect();
    match order {
        LineOrder::Sort { case_sensitive: true } => lines.sort(),
        LineOrder::Sort { case_sensitive: false } => lines.sort_by_key(|l| l.to_lowercase()),
        LineOrder::Reverse => lines.reverse(),
        LineOrder::Unique => {
            let mut seen = HashSet::new();
            lines.retain(|l| seen.insert(*l));
        }
    }
    let mut reordered = lines.join("\n");
    if trailing_newline {
        reordered.push('\n');
    }
    text.replace_range(s..e, &reordered);
    if selection.is_empty() {
        Selection::caret(selection.head.min(text.chars().count()))
    } else {
        Selection::new(byte_to_char(text, s), byte_to_char(text, s + reordered.len()))
    }
}

pub fn trim_trailing_whitespace(text: &mut String, selection: Selection) -> Selection {
    let edits = line_starts(text, 0, text.len())
        .into_iter()
        .filter_map(|pos| {
            let le = line_end(text, pos);
            let kept = text[pos..le].trim_end_matches([' ', '\t']).len();
            (pos + kept < le).then(|| Edit { start: pos + kept, end: le, insert: String::new() })
        })
        .collect();
    apply_edits(text, edits, selection)
}

/// Converts leading indentation to spaces (`to_spaces`) or to tabs.
pub fn convert_indentation(text: &mut String, selection: Selection, tab_size: usize, to_spaces: bool) -> Selection {
    let edits = line_starts(text, 0, text.len())
        .into_iter()
        .filter_map(|pos| {
            let ws = leading_whitespace(&text[pos..line_end(text, pos)]);
            let width = visual_column(ws, tab_size);
            let replacement = if to_spaces {
                " ".repeat(width)
            } else {
                format!("{}{}", "\t".repeat(width / tab_size), " ".repeat(width % tab_size))
            };
            (replacement != ws).then(|| Edit { start: pos, end: pos + ws.len(), insert: replacement })
        })
        .collect();
    apply_edits(text, edits, selection)
}

pub fn ensure_trailing_newline(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, selection: Selection, f: impl FnOnce(&mut String, Selection) -> Selection) -> (String, Selection) {
        let mut text = text.to_string();
        let selection = f(&mut text, selection);
        (text, selection)
    }

    #[test]
    fn duplicates_the_current_line_below() {
        let (text, sel) = run("a\nb\n", Selection::caret(2), duplicate_line);
        assert_eq!(text, "a\nb\nb\n");
        assert_eq!(sel, Selection::caret(4));
    }

    #[test]
    fn swaps_line_up_and_keeps_caret_on_it() {
        let mut text = "one\ntwo\n".to_string();
        let sel = swap_line(&mut text, Selection::caret(5), true).unwrap();
        assert_eq!(text, "two\none\n");
        assert_eq!(sel, Selection::caret(1));
    }

    #[test]
    fn toggles_line_comments_at_the_smallest_indentation() {
        let mut text = "fn a() {\n    x();\n}".to_string();
        let all = Selection::new(0, text.chars().count());
        toggle_comment(&mut text, all, "Rust", false);
        assert_eq!(text, "// fn a() {\n//     x();\n// }");
        let all = Selection::new(0, text.chars().count());
        toggle_comment(&mut text, all, "Rust", false);
        assert_eq!(text, "fn a() {\n    x();\n}");
    }

    #[test]
    fn uses_hash_comments_for_python() {
        let (text, _) = run("x = 1", Selection::caret(0), |t, s| toggle_comment(t, s, "Python", false).unwrap());
        assert_eq!(text, "# x = 1");
    }

    #[test]
    fn indents_and_unindents_selected_lines() {
        let (text, sel) = run("a\nb", Selection::new(0, 3), |t, s| indent(t, s, 4, true));
        assert_eq!(text, "    a\n    b");
        let (text, _) = run(&text, sel, |t, s| unindent(t, s, 4));
        assert_eq!(text, "a\nb");
    }

    #[test]
    fn auto_indents_between_braces() {
        let (text, sel) = run("fn x() {}", Selection::caret(8), |t, s| newline_auto_indent(t, s, 4, true));
        assert_eq!(text, "fn x() {\n    \n}");
        assert_eq!(sel, Selection::caret(13));
    }

    #[test]
    fn auto_indents_after_a_colon() {
        let (text, _) = run("def f():", Selection::caret(8), |t, s| newline_auto_indent(t, s, 4, true));
        assert_eq!(text, "def f():\n    ");
    }

    #[test]
    fn joins_lines_with_a_single_space() {
        let (text, _) = run("a  \n   b", Selection::caret(0), join_lines);
        assert_eq!(text, "a b");
    }

    #[test]
    fn sorts_and_deduplicates_lines() {
        let (text, _) = run("b\na\nb\n", Selection::caret(0), |t, s| reorder_lines(t, s, LineOrder::Unique));
        assert_eq!(text, "b\na\n");
        let (text, _) = run("b\nA\n", Selection::caret(0), |t, s| {
            reorder_lines(t, s, LineOrder::Sort { case_sensitive: false })
        });
        assert_eq!(text, "A\nb\n");
    }
}
