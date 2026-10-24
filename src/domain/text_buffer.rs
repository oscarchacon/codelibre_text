//! TextBuffer - Core text editing buffer with multi-cursor support

use std::sync::Arc;
use std::borrow::Cow;
use uuid::Uuid;

/// A single cursor in the buffer
#[derive(Debug, Clone)]
pub struct Cursor {
    pub position: Position,
    pub anchor: Position,
}

impl Cursor {
    pub fn new(position: Position) -> Self {
        Self {
            position,
            anchor: position,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.position != self.anchor
    }

    /// Returns the range covered by this cursor (min..max)
    pub fn range(&self) -> std::ops::Range<usize> {
        let (start, end) = if self.position.0 < self.anchor.0 {
            (self.position.0, self.anchor.0)
        } else {
            (self.anchor.0, self.position.0)
        };
        start..end
    }
}

/// Position in the text buffer (byte offset)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(pub usize);

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Region - a span of text in the buffer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub start: Position,
    pub end: Position,
    pub id: Uuid,
}

impl Region {
    pub fn new(start: Position, end: Position) -> Self {
        Self {
            start,
            end,
            id: Uuid::new_v4(),
        }
    }

    pub fn len(&self) -> usize {
        (self.end.0 - self.start.0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn contains(&self, pos: Position) -> bool {
        pos >= self.start && pos <= self.end
    }
}

/// Text change operation for undo/redo
#[derive(Debug, Clone)]
pub struct TextChange {
    pub change_type: ChangeType,
    pub position: Position,
    pub old_text: String,
    pub new_text: String,
    pub regions_affected: Vec<Region>,
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Insert,
    Delete,
    Replace,
}

/// The text buffer - handles all text operations
#[derive(Debug, Clone)]
pub struct TextBuffer {
    pub id: Uuid,
    pub name: String,
    pub file_path: Option<Arc<str>>,
    content: String,
    cursors: Vec<Cursor>,
    regions: Vec<Region>,
    pub changes: Vec<TextChange>,
    pub modified: bool,
    syntax: String,
}

impl TextBuffer {
    pub fn new(name: String, file_path: Option<Arc<str>>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            file_path,
            content: String::new(),
            cursors: vec![Cursor::new(Position(0))],
            regions: Vec::new(),
            changes: Vec::new(),
            modified: false,
            syntax: "text".to_string(),
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn content_mut(&mut self) -> &mut String {
        &mut self.content
    }

    pub fn modified(&self) -> bool {
        self.modified
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn set_modified(&mut self, modified: bool) {
        self.modified = modified;
    }

    pub fn cursors(&self) -> &[Cursor] {
        &self.cursors
    }

    pub fn set_cursors(&mut self, cursors: Vec<Cursor>) {
        self.cursors = cursors;
    }

    pub fn syntax(&self) -> &str {
        &self.syntax
    }

    pub fn set_syntax(&mut self, syntax: String) {
        self.syntax = syntax;
    }

    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Insert text at position
    pub fn insert(&mut self, pos: Position, text: &str) {
        if text.is_empty() {
            return;
        }

        let char_pos = self.byte_to_char(pos.0);
        let bytes_pos = pos.0;

        let change = TextChange {
            change_type: ChangeType::Insert,
            position: pos,
            old_text: String::new(),
            new_text: text.to_string(),
            regions_affected: Vec::new(),
        };

        self.content.insert_str(char_pos, text);
        self.changes.push(change);
        self.modified = true;

        // Update cursor positions
        let len = text.chars().count();
        for cursor in &mut self.cursors {
            if cursor.position.0 >= bytes_pos {
                cursor.position.0 += len;
            }
            if cursor.anchor.0 >= bytes_pos {
                cursor.anchor.0 += len;
            }
        }
    }

    /// Delete text in range
    pub fn delete(&mut self, range: std::ops::Range<usize>) {
        let start = self.byte_to_char(range.start);
        let end = self.byte_to_char(range.end);

        if start >= self.content.len() {
            return;
        }

        let deleted: String = self.content.drain(start..end.min(self.content.len())).collect();

        let change = TextChange {
            change_type: ChangeType::Delete,
            position: Position(range.start),
            old_text: deleted.clone(),
            new_text: String::new(),
            regions_affected: Vec::new(),
        };

        self.changes.push(change);
        self.modified = true;

        // Update cursors
        let len = deleted.chars().count();
        for cursor in &mut self.cursors {
            if cursor.position.0 > range.start {
                cursor.position.0 = cursor.position.0.saturating_sub(len);
            }
            if cursor.anchor.0 > range.start {
                cursor.anchor.0 = cursor.anchor.0.saturating_sub(len);
            }
        }
    }

    /// Replace text in range
    pub fn replace(&mut self, range: std::ops::Range<usize>, new_text: &str) {
        let old_range = range.clone();
        self.delete(range);
        if !new_text.is_empty() {
            self.insert(Position(old_range.start), new_text);
        }
    }

    /// Get text at position range
    pub fn text_range(&self, start: Position, end: Position) -> String {
        let s = self.byte_to_char(start.0);
        let e = self.byte_to_char(end.0);
        if s <= self.content.len() && e <= self.content.len() {
            self.content[s..e].to_string()
        } else {
            String::new()
        }
    }

    /// Convert byte offset to char offset
    fn byte_to_char(&self, byte: usize) -> usize {
        if byte >= self.content.len() {
            return self.content.len();
        }
        self.content
            .char_indices()
            .take_while(|(i, _)| *i <= byte)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(self.content.len())
    }

    /// Get line number for position
    pub fn line_number_at(&self, pos: Position) -> usize {
        let s = self.byte_to_char(pos.0);
        self.content[..s.min(self.content.len())]
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1
    }

    /// Get column number for position
    pub fn column_at(&self, pos: Position) -> usize {
        let s = self.byte_to_char(pos.0);
        let line_start = self.content[..s.min(self.content.len())]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        self.content[line_start..s.min(self.content.len())]
            .chars()
            .count()
    }

    /// Get position at line/column (1-indexed)
    pub fn position_from_line_col(&self, line: usize, col: usize) -> Position {
        let mut current_line = 0;
        let mut byte_pos = 0;

        for (i, c) in self.content.char_indices() {
            if current_line == line - 1 {
                if col == 1 || i == byte_pos {
                    // Find the column
                    let line_content = &self.content[byte_pos..i];
                    let col_chars = line_content.chars().count();
                    if col_chars >= col - 1 {
                        return Position(i);
                    }
                }
            }
            if c == '\n' {
                current_line += 1;
                byte_pos = i + 1;
            }
        }
        Position(self.content.len())
    }

    /// Get all lines in the buffer
    pub fn lines(&self) -> Vec<String> {
        self.content.lines().map(String::from).collect()
    }

    /// Get line at index
    pub fn line(&self, line: usize) -> Option<&str> {
        self.content.lines().nth(line)
    }

    /// Line length (without newline)
    pub fn line_length(&self, line: usize) -> Option<usize> {
        self.content.lines().nth(line).map(|l| l.len())
    }

    /// Total number of lines
    pub fn line_count(&self) -> usize {
        if self.content.is_empty() {
            0
        } else {
            self.content.lines().count() + 1
        }
    }

    /// Add a region (for syntax highlighting, errors, etc.)
    pub fn add_region(&mut self, region: Region) {
        self.regions.push(region);
    }

    /// Get all regions
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Clear all regions
    pub fn clear_regions(&mut self) {
        self.regions.clear();
    }

    /// Find all occurrences of pattern starting from position
    pub fn find_all(&self, pattern: &str, start: Position) -> Vec<Position> {
        let mut text = &self.content[start.0.min(self.content.len())..];
        let mut positions = Vec::new();
        let mut search_start = start.0;

        while let Some(idx) = text.find(pattern) {
            positions.push(Position(search_start + idx));
            search_start += idx + 1;
            if search_start >= self.content.len() {
                break;
            }
            text = &self.content[search_start..];
        }

        positions
    }

    /// Find with regex
    pub fn find_all_regex(&self, regex: &regex::Regex, start: Position) -> Vec<(Position, String)> {
        let mut text = &self.content[start.0.min(self.content.len())..];
        let mut matches = Vec::new();
        let mut search_start = start.0;

        for m in regex.find_iter(text) {
            matches.push((
                Position(search_start + m.start()),
                m.as_str().to_string(),
            ));
        }

        matches
    }

    /// Replace all occurrences
    pub fn replace_all(&mut self, pattern: &str, replacement: &str) {
        let new_content = self.content.replace(pattern, replacement);
        if new_content != self.content {
            self.content = new_content;
            self.modified = true;
        }
    }

    /// Replace with regex
    pub fn replace_all_regex(&mut self, regex: &regex::Regex, replacement: &str) {
        self.content = regex.replace_all(&self.content, replacement).to_string();
        self.modified = true;
    }
}
