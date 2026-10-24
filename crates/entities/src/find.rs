//! Find and replace with Sublime's options: regex, case sensitive, whole word and wrap.

use std::ops::Range;

use regex::{NoExpand, Regex, RegexBuilder};

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
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
            wrap: true,
        }
    }
}

impl FindOptions {
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
            .map_err(|e| e.to_string().lines().last().unwrap_or("invalid regex").to_string())
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

/// The next (or previous) match after the selection, wrapping around if `wrap`.
pub fn find_from(pattern: &Regex, text: &str, selection: Selection, forward: bool, wrap: bool) -> Option<Selection> {
    let to_selection = |r: &Range<usize>| Selection::new(byte_to_char(text, r.start), byte_to_char(text, r.end));
    let matches = find_all(pattern, text);
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

/// Replaces the selection if it is a match, then selects the next match.
pub fn replace_next(
    pattern: &Regex,
    text: &mut String,
    selection: Selection,
    replacement: &str,
    options: &FindOptions,
) -> Selection {
    let start = char_to_byte(text, selection.start());
    let end = char_to_byte(text, selection.end());
    let is_match = !selection.is_empty()
        && pattern
            .find_at(text, start)
            .is_some_and(|m| m.start() == start && m.end() == end);
    let mut selection = selection;
    if is_match {
        let replaced = if options.regex {
            let mut out = String::new();
            if let Some(caps) = pattern.captures(&text[start..end]) {
                caps.expand(&replacement_template(replacement), &mut out);
            }
            out
        } else {
            replacement.to_string()
        };
        text.replace_range(start..end, &replaced);
        selection = Selection::caret(byte_to_char(text, start + replaced.len()));
    }
    find_from(pattern, text, selection, true, options.wrap).unwrap_or(selection)
}

/// Replaces every match and returns how many there were.
pub fn replace_all(pattern: &Regex, text: &mut String, replacement: &str, options: &FindOptions) -> usize {
    let count = find_all(pattern, text).len();
    if count > 0 {
        *text = if options.regex {
            pattern.replace_all(text, replacement_template(replacement).as_str()).into_owned()
        } else {
            pattern.replace_all(text, NoExpand(replacement)).into_owned()
        };
    }
    count
}
