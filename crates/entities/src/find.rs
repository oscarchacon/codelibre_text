//! Find and replace with Sublime's options: regex, case sensitive, whole word, wrap and preserve case.

use std::ops::Range;

pub use regex::Regex;
use regex::{Captures, RegexBuilder};

use crate::selection::Selection;
use crate::text::{byte_to_char, char_to_byte};

const MAX_MATCHES: usize = 100_000;

#[derive(Debug, Clone, PartialEq)]
pub struct FindOptions {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub wrap: bool,
    /// Replacing keeps the case of what was found ([`preserve_case`]).
    pub preserve_case: bool,
    /// Search only the area the selections had when it was turned on (the use cases keep it).
    pub in_selection: bool,
    /// The matches are highlighted while the panel shows.
    pub highlight_matches: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
            wrap: true,
            preserve_case: false,
            in_selection: false,
            highlight_matches: true,
        }
    }
}

/// An option the find panel turns on and off (`toggle_regex`, `toggle_case_sensitive`,
/// `toggle_whole_word`, `toggle_preserve_case`, `toggle_wrap`, `toggle_in_selection`, `toggle_highlight`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindToggle {
    Regex,
    CaseSensitive,
    WholeWord,
    PreserveCase,
    Wrap,
    InSelection,
    HighlightMatches,
}

impl FindOptions {
    pub fn toggle(&mut self, option: FindToggle) {
        let value = match option {
            FindToggle::Regex => &mut self.regex,
            FindToggle::CaseSensitive => &mut self.case_sensitive,
            FindToggle::WholeWord => &mut self.whole_word,
            FindToggle::PreserveCase => &mut self.preserve_case,
            FindToggle::Wrap => &mut self.wrap,
            FindToggle::InSelection => &mut self.in_selection,
            FindToggle::HighlightMatches => &mut self.highlight_matches,
        };
        *value = !*value;
    }

    /// The search pattern, or `Ok(None)` when the query is empty.
    pub fn pattern(&self) -> Result<Option<Regex>, String> {
        if self.query.is_empty() {
            return Ok(None);
        }
        let mut pattern = if self.regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if self.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        RegexBuilder::new(&pattern)
            .case_insensitive(!self.case_sensitive)
            .multi_line(true)
            .build()
            .map(Some)
            .map_err(|e| {
                e.to_string()
                    .lines()
                    .last()
                    .unwrap_or("invalid regex")
                    .to_string()
            })
    }
}

/// Byte ranges of all non-empty matches.
pub fn find_all(pattern: &Regex, text: &str) -> Vec<Range<usize>> {
    pattern
        .find_iter(text)
        .filter(|m| !m.range().is_empty())
        .take(MAX_MATCHES)
        .map(|m| m.range())
        .collect()
}

/// The matches that lie whole inside one of the `area` byte ranges (Find "In selection").
pub fn inside(matches: Vec<Range<usize>>, area: &[Range<usize>]) -> Vec<Range<usize>> {
    matches
        .into_iter()
        .filter(|m| area.iter().any(|a| a.start <= m.start && m.end <= a.end))
        .collect()
}

/// The next (or previous) match after the selection, wrapping around if `wrap`.
pub fn find_from(
    pattern: &Regex,
    text: &str,
    selection: Selection,
    forward: bool,
    wrap: bool,
) -> Option<Selection> {
    find_among(&find_all(pattern, text), text, selection, forward, wrap)
}

/// [`find_from`] among `matches` (byte ranges in order).
pub fn find_among(
    matches: &[Range<usize>],
    text: &str,
    selection: Selection,
    forward: bool,
    wrap: bool,
) -> Option<Selection> {
    let to_selection =
        |r: &Range<usize>| Selection::new(byte_to_char(text, r.start), byte_to_char(text, r.end));
    let found = if forward {
        let from = char_to_byte(text, selection.end());
        matches
            .iter()
            .find(|r| r.start >= from)
            .or_else(|| if wrap { matches.first() } else { None })
    } else {
        let from = char_to_byte(text, selection.start());
        matches
            .iter()
            .rev()
            .find(|r| r.end <= from && r.start < from)
            .or_else(|| if wrap { matches.last() } else { None })
    };
    found.map(to_selection)
}

/// Every match as a selection (`find_all_under`).
pub fn find_all_selections(pattern: &Regex, text: &str) -> Vec<Selection> {
    to_selections(text, &find_all(pattern, text))
}

/// Byte ranges of `text` (in order) as selections.
pub fn to_selections(text: &str, ranges: &[Range<usize>]) -> Vec<Selection> {
    let (mut byte, mut char) = (0, 0);
    let mut to_char = |target: usize| {
        char += text[byte..target].chars().count();
        byte = target;
        char
    };
    ranges
        .iter()
        .map(|r| Selection::new(to_char(r.start), to_char(r.end)))
        .collect()
}

/// `find_under_expand`: the first match after `after` (wrapping around) that is not already one
/// of the `selected` ranges.
pub fn next_unselected(
    pattern: &Regex,
    text: &str,
    after: Selection,
    selected: &[Selection],
) -> Option<Selection> {
    let from = char_to_byte(text, after.end());
    let matches = find_all(pattern, text);
    let split = matches.partition_point(|r| r.start < from);
    matches[split..]
        .iter()
        .chain(&matches[..split])
        .map(|r| Selection::new(byte_to_char(text, r.start), byte_to_char(text, r.end)))
        .find(|m| {
            !selected
                .iter()
                .any(|s| s.start() == m.start() && s.end() == m.end())
        })
}

/// Sublime writes groups as `\1` or `$1`; the regex crate expects `${1}`.
fn replacement_template(replacement: &str) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\\', Some(d)) if d.is_ascii_digit() => {
                out.push_str(&format!("${{{d}}}"));
                chars.next();
            }
            ('\\', Some('n')) => {
                out.push('\n');
                chars.next();
            }
            ('\\', Some('t')) => {
                out.push('\t');
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// What one match becomes: the replacement (its groups expanded with `regex`), in the match's
/// case with `preserve_case`.
fn replaced_text(captures: &Captures, replacement: &str, options: &FindOptions) -> String {
    let mut out = String::new();
    if options.regex {
        captures.expand(&replacement_template(replacement), &mut out);
    } else {
        out.push_str(replacement);
    }
    if options.preserve_case {
        out = preserve_case(&captures[0], &out);
    }
    out
}

/// Sublime's "Preserve case" (as 4215 replaces): `found` with no capital letter makes the whole
/// `replacement` lowercase, `found` made only of capitals makes it uppercase; otherwise each
/// character takes the case of `found`'s character at its position (one neither upper nor lower
/// case leaves it alone), and what goes past `found`'s end stays as written.
pub fn preserve_case(found: &str, replacement: &str) -> String {
    if !found.chars().any(char::is_uppercase) {
        return replacement.chars().map(lower_char).collect();
    }
    if found.chars().all(char::is_uppercase) {
        return replacement.chars().map(upper_char).collect();
    }
    let mut found = found.chars();
    replacement
        .chars()
        .map(|c| match found.next() {
            Some(f) if f.is_uppercase() => upper_char(c),
            Some(f) if f.is_lowercase() => lower_char(c),
            _ => c,
        })
        .collect()
}

/// A character's uppercase as one character, as Sublime maps them (`ß` → `ẞ`, not `SS`).
fn upper_char(c: char) -> char {
    if c == 'ß' {
        return 'ẞ';
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

fn lower_char(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// Replaces the selection if it is a match; returns the caret after the replacement, or `None`
/// when the selection is no match (nothing changes).
pub fn replace_selected(
    pattern: &Regex,
    text: &mut String,
    selection: Selection,
    replacement: &str,
    options: &FindOptions,
) -> Option<Selection> {
    let start = char_to_byte(text, selection.start());
    let end = char_to_byte(text, selection.end());
    let is_match = !selection.is_empty()
        && pattern
            .find_at(text, start)
            .is_some_and(|m| m.start() == start && m.end() == end);
    if !is_match {
        return None;
    }
    let replaced = pattern
        .captures(&text[start..end])
        .map(|captures| replaced_text(&captures, replacement, options))
        .unwrap_or_default();
    text.replace_range(start..end, &replaced);
    Some(Selection::caret(byte_to_char(text, start + replaced.len())))
}

/// Replaces the selection if it is a match, then selects the next match.
pub fn replace_next(
    pattern: &Regex,
    text: &mut String,
    selection: Selection,
    replacement: &str,
    options: &FindOptions,
) -> Selection {
    let selection =
        replace_selected(pattern, text, selection, replacement, options).unwrap_or(selection);
    find_from(pattern, text, selection, true, options.wrap).unwrap_or(selection)
}

/// Replaces every match and returns how many there were.
pub fn replace_all(
    pattern: &Regex,
    text: &mut String,
    replacement: &str,
    options: &FindOptions,
) -> usize {
    replace_all_in(pattern, text, replacement, options, 0..text.len())
}

/// [`replace_all`] for the matches that lie whole inside the bytes `area` (they are looked for in
/// the whole text, so `^` and `\b` read what is around them). Empty matches are replaced too, as
/// Sublime 4215 does (`^` puts the replacement at the start of every line).
pub fn replace_all_in(
    pattern: &Regex,
    text: &mut String,
    replacement: &str,
    options: &FindOptions,
    area: Range<usize>,
) -> usize {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut count = 0;
    for captures in pattern.captures_iter(text) {
        let found = captures.get(0).expect("group 0 is the match");
        if found.start() < area.start || found.end() > area.end {
            continue;
        }
        out.push_str(&text[last..found.start()]);
        if options.regex || options.preserve_case {
            out.push_str(&replaced_text(&captures, replacement, options));
        } else {
            out.push_str(replacement);
        }
        last = found.end();
        count += 1;
        if count == MAX_MATCHES {
            break;
        }
    }
    if count > 0 {
        out.push_str(&text[last..]);
        *text = out;
    }
    count
}

/// `text` as a regex that finds it literally.
pub fn escape(text: &str) -> String {
    regex::escape(text)
}
