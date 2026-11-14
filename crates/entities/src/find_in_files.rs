//! Find in Files (Sublime's `find_in_files` panel): what its "Where" field lists, which files a
//! search reads, the lines it finds in each, and how Find Results writes them. The format is
//! the one Sublime Text 4215 writes (checked by running it):
//!
//! ```text
//! Searching 3 files for "foo"
//!
//! ~/data/a.txt:
//!     1  one
//!     2: two foo
//!     3  three
//!     .
//!     8: eight foo
//!
//! 2 matches in 1 file
//! ```

use std::ops::Range;
use std::path::Path;

use regex::Regex;

use crate::find;
use crate::project::matches_pattern;

/// Find Results' `result_file_regex` and `result_line_regex` (Sublime's).
pub const RESULT_FILE_REGEX: &str = r"^([^ \t].*):$";
pub const RESULT_LINE_REGEX: &str = r"^ +([0-9]+):";

/// What the "Where" field lists, comma-separated: folders and files, `<open folders>`,
/// `<open files>`, `<current file>`, `<project filters>`, include patterns (`*.rs`) and exclude
/// patterns (`-*.min.js`, `-node_modules`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Where {
    pub paths: Vec<String>,
    pub open_folders: bool,
    pub open_files: bool,
    pub current_file: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Reads the "Where" field. With no folder, file or `<open …>` entry (empty, or only patterns),
/// the open folders are searched.
pub fn parse_where(text: &str) -> Where {
    let mut parsed = Where::default();
    for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match entry {
            "<open folders>" => parsed.open_folders = true,
            "<open files>" => parsed.open_files = true,
            "<current file>" => parsed.current_file = true,
            // The folder and file exclude patterns always apply.
            "<project filters>" => {}
            _ if entry.starts_with('-') => parsed.exclude.push(entry[1..].trim().to_string()),
            _ if entry.contains(['*', '?']) => parsed.include.push(entry.to_string()),
            _ => parsed.paths.push(entry.to_string()),
        }
    }
    if parsed.paths.is_empty() && !parsed.open_files && !parsed.current_file {
        parsed.open_folders = true;
    }
    parsed
}

/// `add_where_snippet {"snippet": "*.${0:txt}"}`: the snippet's text, its fields' placeholders
/// in place of the fields.
pub fn where_snippet_text(snippet: &str) -> String {
    let fields = Regex::new(r"\$\{[0-9]+:([^}]*)\}|\$[0-9]+").expect("valid regex");
    fields
        .replace_all(snippet, |c: &regex::Captures| {
            c.get(1).map_or(String::new(), |m| m.as_str().to_string())
        })
        .into_owned()
}

/// `text` with `entry` added at the end of the list.
pub fn add_where_entry(text: &str, entry: &str) -> String {
    let text = text.trim_end().trim_end_matches(',');
    if text.trim().is_empty() {
        entry.to_string()
    } else {
        format!("{text},{entry}")
    }
}

/// Which files a search reads: the "Where" patterns and the settings `folder_exclude_patterns`,
/// `file_exclude_patterns` and `binary_file_patterns`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub folder_excludes: Vec<String>,
    pub file_excludes: Vec<String>,
    pub binary: Vec<String>,
}

impl PathFilter {
    /// A folder whose files are not searched (nor its subfolders').
    pub fn skips_folder(&self, path: &Path) -> bool {
        let name = file_name(path);
        self.folder_excludes
            .iter()
            .any(|p| matches_pattern(p, &name))
            || self.exclude.iter().any(|p| pattern_matches(p, path))
    }

    pub fn accepts_file(&self, path: &Path) -> bool {
        let name = file_name(path);
        let excluded = self
            .file_excludes
            .iter()
            .chain(&self.binary)
            .any(|p| matches_pattern(p, &name))
            || self.exclude.iter().any(|p| pattern_matches(p, path));
        !excluded
            && (self.include.is_empty() || self.include.iter().any(|p| pattern_matches(p, path)))
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A pattern with a slash is matched against the whole path, any other against the name.
fn pattern_matches(pattern: &str, path: &Path) -> bool {
    if pattern.contains(['/', '\\']) {
        let path = path.to_string_lossy().replace('\\', "/");
        let pattern = pattern.replace('\\', "/");
        matches_pattern(&pattern, &path) || matches_pattern(&format!("*/{pattern}"), &path)
    } else {
        matches_pattern(pattern, &file_name(path))
    }
}

/// A line Find Results shows: a match, or one around a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultLine {
    /// From 1.
    pub row: usize,
    pub text: String,
    /// The matches on the line, in chars of `text` (empty for a line of context).
    pub matches: Vec<Range<usize>>,
}

/// What a search found in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHits {
    pub path: std::path::PathBuf,
    pub lines: Vec<ResultLine>,
    pub matches: usize,
}

/// A file Sublime does not search: it has a NUL byte.
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

/// `search_text` over a file's bytes (`\r\n` read as `\n`); `None` when it is binary.
pub fn search_bytes(
    pattern: &Regex,
    bytes: &[u8],
    context: usize,
) -> Option<(Vec<ResultLine>, usize)> {
    if is_binary(bytes) {
        return None;
    }
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    Some(search_text(pattern, &text, context))
}

/// The lines of `text` with matches, and `context` lines around each, and how many matches
/// there are. A match over several lines marks each of them.
pub fn search_text(pattern: &Regex, text: &str, context: usize) -> (Vec<ResultLine>, usize) {
    let matches = find::find_all(pattern, text);
    if matches.is_empty() {
        return (Vec::new(), 0);
    }
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |byte: usize| starts.partition_point(|&s| s <= byte) - 1;
    let line_range = |line: usize| {
        let end = starts.get(line + 1).map_or(text.len(), |&next| next - 1);
        starts[line]..end
    };
    // The matches of each line, as byte ranges of the line.
    let mut matched: Vec<(usize, Vec<Range<usize>>)> = Vec::new();
    for m in &matches {
        let last = line_of(m.end - 1);
        for line in line_of(m.start)..=last {
            let range = line_range(line);
            let piece = m.start.max(range.start) - range.start..m.end.min(range.end) - range.start;
            match matched.last_mut() {
                Some((l, ranges)) if *l == line => ranges.push(piece),
                _ => matched.push((line, vec![piece])),
            }
        }
    }
    let mut shown: Vec<usize> = Vec::new();
    for (line, _) in &matched {
        let from = line.saturating_sub(context);
        let to = (line + context).min(starts.len() - 1);
        for l in from.max(shown.last().map_or(0, |&s| s + 1))..=to {
            shown.push(l);
        }
    }
    let lines = shown
        .into_iter()
        .map(|line| {
            let content = &text[line_range(line)];
            let ranges = matched
                .iter()
                .find(|(l, _)| *l == line)
                .map(|(_, ranges)| {
                    let to_char = |byte: usize| content[..byte].chars().count();
                    ranges
                        .iter()
                        .map(|r| to_char(r.start)..to_char(r.end))
                        .collect()
                })
                .unwrap_or_default();
            ResultLine {
                row: line + 1,
                text: content.to_string(),
                matches: ranges,
            }
        })
        .collect();
    (lines, matches.len())
}

/// The text of a result line within `find_in_files_context_characters` (0: no limit): a match
/// keeps that many chars before its first match and after its last, a line of context its first
/// and last chars. `skipped(n)` names what was left out (`<skipped n characters>`).
pub fn shorten_line(line: &ResultLine, limit: usize, skipped: &dyn Fn(usize) -> String) -> String {
    let chars: Vec<char> = line.text.chars().collect();
    let len = chars.len();
    let piece = |range: Range<usize>| chars[range].iter().collect::<String>();
    if limit == 0 {
        return line.text.clone();
    }
    match (line.matches.first(), line.matches.last()) {
        (Some(first), Some(last)) => {
            let from = first.start.saturating_sub(limit);
            let to = (last.end + limit).min(len);
            let mut out = String::new();
            if from > 0 {
                out.push_str(&format!("{} ...", skipped(from)));
            }
            out.push_str(&piece(from..to));
            if to < len {
                out.push_str(&format!("... {} ...", skipped(len - to)));
            }
            out
        }
        _ if len > 2 * limit => format!(
            "{}... {} ...{}",
            piece(0..limit),
            skipped(len - 2 * limit),
            piece(len - limit..len)
        ),
        _ => line.text.clone(),
    }
}

/// One file's part of Find Results: its path, then its lines (`  12: ` for a match, `  12  ` for
/// context, ` .` between lines that do not follow each other), then an empty line.
pub fn format_file(path: &str, lines: &[(usize, bool, String)]) -> String {
    let mut out = format!("{path}:\n");
    let mut previous: Option<usize> = None;
    for (row, is_match, text) in lines {
        if previous.is_some_and(|p| p + 1 != *row) {
            out.push_str("    .\n");
        }
        let number = format!("{row:>5}");
        // The line regex needs a space first.
        let pad = if number.starts_with(' ') { "" } else { " " };
        let mark = if *is_match { ':' } else { ' ' };
        out.push_str(&format!("{pad}{number}{mark} {text}\n"));
        previous = Some(*row);
    }
    out.push('\n');
    out
}
